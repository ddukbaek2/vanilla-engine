//==============================================================================
//! Opus 해석기. (symphonia 코덱 등록표에 넣는 엔진 해석기, 속은 opus-pure)
//! - 크롬 media/filters/opus_audio_decoder.cc 와 같게 한다.
//!   48kHz 로 풀고(kOpusDefaultSampleRate), OpusHead 의 출력 게인을 디코더에 건다(OPUS_SET_GAIN).
//!   채널 매핑 패밀리 1 은 Vorbis 채널 순서를 크로미엄 순서로 바꿔 내보낸다. (GetVorbisToChromiumChannelLayoutOffsets, 5.1 은 L R C LFE SL SR)
//!   pre-skip 과 끝 버리기는 여기서 하지 않고 크롬처럼 바깥(trim.rs 의 AudioDiscardHelper 자리)이 한다.
//! - OpusHead 는 symphonia 의 ogg, mkv 풀기가 extra_data 로 준다. (ogg 는 첫 패킷, mkv 는 CodecPrivate)
//==============================================================================
// 옮긴 원문: 크로미엄 media/filters/opus_audio_decoder.cc, media/formats/common/opus_constants.h 에서 옮김, BSD-3-Clause, THIRD_PARTY_LICENSES.md
use opus_pure::{OpusDecoder, OpusMSDecoder, MAX_PACKET_SAMPLES};
use symphonia::core::audio::{AsGenericAudioBufferRef, AudioBuffer, AudioMut, AudioSpec, Channels, GenericAudioBufferRef, Position};
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult};
use symphonia::core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia::core::codecs::CodecInfo;
use symphonia::core::errors::{decode_error, unsupported_error, Result};
use symphonia::core::packet::PacketRef;

/// 크롬 Opus 해석 표본율. (kOpusDefaultSampleRate)
pub(crate) const OPUS_SAMPLE_RATE: u32 = 48000;

/// Vorbis 채널 순서 → 크로미엄 채널 순서. (media/formats/common/opus_constants.h, 크로미엄 채널 i 는 Vorbis 채널 표[i])
const VORBIS_TO_CHROMIUM_CHANNEL_OFFSET_TABLE: [&[usize]; 8] = [
	&[0],
	&[0, 1],
	&[0, 2, 1],
	&[0, 1, 2, 3],
	&[0, 2, 1, 3, 4],
	&[0, 2, 1, 5, 3, 4],
	&[0, 2, 1, 6, 5, 3, 4],
	&[0, 2, 1, 7, 5, 6, 3, 4],
];

//==============================================================================
/// OpusHead. (RFC 7845 5.1, 크롬 ParseOpusExtraData 가 읽는 칸)
//==============================================================================
#[derive(Clone, Debug)]
pub(crate) struct OpusHeader {
	pub channel_count: usize,
	pub pre_skip: u32,
	pub gain_q8: i16,
	pub mapping_family: u8,
	pub stream_count: usize,
	pub coupled_count: usize,
	pub stream_map: Vec<u8>,
}

impl OpusHeader {
	//==============================================================================
	/// 바이트에서 읽기. (OpusHead 로 시작하는 19 바이트 이상)
	//==============================================================================
	pub(crate) fn parse(bytes: &[u8]) -> Option<OpusHeader> {
		if bytes.len() < 19 || &bytes[0..8] != b"OpusHead" {
			return None;
		}
		let channel_count = bytes[9] as usize;
		if channel_count == 0 {
			return None;
		}
		let pre_skip = u16::from_le_bytes([bytes[10], bytes[11]]) as u32;
		let gain_q8 = i16::from_le_bytes([bytes[16], bytes[17]]);
		let mapping_family = bytes[18];
		if mapping_family == 0 {
			if channel_count > 2 {
				return None;
			}
			return Some(OpusHeader {
				channel_count,
				pre_skip,
				gain_q8,
				mapping_family,
				stream_count: 1,
				coupled_count: if channel_count > 1 { 1 } else { 0 },
				stream_map: (0..channel_count as u8).collect(),
			});
		}
		if bytes.len() < 21 + channel_count {
			return None;
		}
		Some(OpusHeader {
			channel_count,
			pre_skip,
			gain_q8,
			mapping_family,
			stream_count: bytes[19] as usize,
			coupled_count: bytes[20] as usize,
			stream_map: bytes[21..21 + channel_count].to_vec(),
		})
	}
}

//==============================================================================
/// 스트림을 푸는 쪽. (모노와 스테레오 한 스트림, 또는 여러 스트림)
//==============================================================================
enum OpusStreamDecoder {
	Single(OpusDecoder),
	Multi(OpusMSDecoder),
}

//==============================================================================
/// symphonia 에 넣는 Opus 해석기.
//==============================================================================
pub(crate) struct OpusPacketDecoder {
	codec_params: AudioCodecParameters,
	stream_decoder: OpusStreamDecoder,
	channel_count: usize,
	/// 내보낼 채널 i 가 디코더 출력의 몇째 채널인지. (None 이면 무음, 크롬 순서로 바꾼 것)
	output_channel_source_list: Vec<Option<usize>>,
	interleaved_samples: Vec<f32>,
	buffer: AudioBuffer<f32>,
}

impl OpusPacketDecoder {
	//==============================================================================
	/// 생성. (OpusHead 로)
	//==============================================================================
	pub(crate) fn create(codec_params: &AudioCodecParameters) -> Result<OpusPacketDecoder> {
		let header = match codec_params.extra_data.as_deref().and_then(OpusHeader::parse) {
			Some(header) => {
				header
			}
			None => {
				return decode_error("opus: OpusHead 가 없거나 틀렸습니다");
			}
		};
		let channel_count = header.channel_count;
		// 디코더 출력 채널 c 가 어느 스트림 채널인지. (opus-pure 는 표준 배치로 만든다)
		let (stream_decoder, decoder_mapping): (OpusStreamDecoder, Vec<u8>) = if header.mapping_family == 0 {
			let mut decoder = match OpusDecoder::new(OPUS_SAMPLE_RATE as i32, channel_count) {
				Ok(decoder) => {
					decoder
				}
				Err(_) => {
					return unsupported_error("opus: 디코더를 만들 수 없습니다");
				}
			};
			decoder.gain_q8 = header.gain_q8 as i32;
			(OpusStreamDecoder::Single(decoder), (0..channel_count as u8).collect())
		}
		else {
			let mut decoder = match OpusMSDecoder::new(OPUS_SAMPLE_RATE as i32, channel_count, if header.mapping_family == 1 { 1 } else { header.mapping_family }) {
				Ok(decoder) => {
					decoder
				}
				Err(_) => {
					return unsupported_error("opus: 이 채널 매핑은 풀 수 없습니다");
				}
			};
			let layout = decoder.layout().clone();
			if layout.nb_streams != header.stream_count || layout.nb_coupled_streams != header.coupled_count {
				return unsupported_error("opus: 표준이 아닌 스트림 수는 풀 수 없습니다");
			}
			for stream_decoder in decoder.streams_mut() {
				stream_decoder.gain_q8 = header.gain_q8 as i32;
			}
			(OpusStreamDecoder::Multi(decoder), layout.mapping.clone())
		};
		// 머리의 매핑으로 채널 c 가 가리키는 스트림 채널 → 디코더 출력 채널.
		let mut header_channel_source_list: Vec<Option<usize>> = Vec::with_capacity(channel_count);
		for channel_index in 0..channel_count {
			let stream_channel = header.stream_map[channel_index];
			if stream_channel == 255 {
				header_channel_source_list.push(None);
			}
			else {
				header_channel_source_list.push(decoder_mapping.iter().position(|mapped| *mapped == stream_channel));
			}
		}
		// 패밀리 1 은 크로미엄 순서로 바꾼다. (크롬은 매핑 표를 바꿔 libopus 가 그 순서로 내게 한다)
		let output_channel_source_list: Vec<Option<usize>> = if header.mapping_family == 1 && channel_count <= 8 {
			VORBIS_TO_CHROMIUM_CHANNEL_OFFSET_TABLE[channel_count - 1].iter().map(|vorbis_index| header_channel_source_list[*vorbis_index]).collect()
		}
		else {
			header_channel_source_list
		};
		let channels = create_chromium_channels(channel_count, header.mapping_family);
		let mut decoder_codec_params = codec_params.clone();
		decoder_codec_params.with_sample_rate(OPUS_SAMPLE_RATE).with_channels(channels.clone());
		Ok(OpusPacketDecoder {
			codec_params: decoder_codec_params,
			stream_decoder,
			channel_count,
			output_channel_source_list,
			interleaved_samples: vec![0.0; MAX_PACKET_SAMPLES * channel_count],
			buffer: AudioBuffer::new(AudioSpec::new(OPUS_SAMPLE_RATE, channels), MAX_PACKET_SAMPLES),
		})
	}

	//==============================================================================
	/// 머리에 적힌 pre-skip. (크롬 codec_delay_)
	//==============================================================================
	pub(crate) fn read_pre_skip(codec_params: &AudioCodecParameters) -> u32 {
		codec_params.extra_data.as_deref().and_then(OpusHeader::parse).map(|header| header.pre_skip).unwrap_or(0)
	}
}

//==============================================================================
/// 크로미엄 순서의 채널 자리. (1: 모노, 2: L R, 그 밖은 크로미엄 배치, 패밀리 255 는 자리 없는 채널)
//==============================================================================
fn create_chromium_channels(channel_count: usize, mapping_family: u8) -> Channels {
	if mapping_family > 1 {
		return Channels::Discrete(channel_count as u16);
	}
	let position = match channel_count {
		1 => {
			Position::FRONT_CENTER
		}
		2 => {
			Position::FRONT_LEFT | Position::FRONT_RIGHT
		}
		3 => {
			Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::FRONT_CENTER
		}
		4 => {
			Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::REAR_LEFT | Position::REAR_RIGHT
		}
		5 => {
			Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::FRONT_CENTER | Position::REAR_LEFT | Position::REAR_RIGHT
		}
		6 => {
			Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::FRONT_CENTER | Position::LFE1 | Position::REAR_LEFT | Position::REAR_RIGHT
		}
		_ => {
			return Channels::Discrete(channel_count as u16);
		}
	};
	Channels::Positioned(position)
}

impl AudioDecoder for OpusPacketDecoder {
	//==============================================================================
	/// 처음 상태로. (디코더 상태를 되돌린다, 오류는 넘긴다)
	//==============================================================================
	fn reset(&mut self) {
		match &mut self.stream_decoder {
			OpusStreamDecoder::Single(decoder) => {
				let _ = decoder.reset_state();
			}
			OpusStreamDecoder::Multi(decoder) => {
				let _ = decoder.reset_state();
			}
		}
	}

	//==============================================================================
	/// 코덱 정보. (등록표의 Opus 항목)
	//==============================================================================
	fn codec_info(&self) -> &CodecInfo {
		&OPUS_SUPPORTED_CODEC_LIST[0].info
	}

	//==============================================================================
	/// 코덱 인자. (48kHz 와 크로미엄 차례의 채널로 고친 것)
	//==============================================================================
	fn codec_params(&self) -> &AudioCodecParameters {
		&self.codec_params
	}

	//==============================================================================
	/// 패킷 하나 풀기. (48kHz, 크로미엄 차례로 옮기고 자리 없는 채널은 0, 실패하면 Err)
	//==============================================================================
	fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
		self.buffer.clear();
		let decode_result = match &mut self.stream_decoder {
			OpusStreamDecoder::Single(decoder) => {
				decoder.decode(packet.data, MAX_PACKET_SAMPLES, &mut self.interleaved_samples)
			}
			OpusStreamDecoder::Multi(decoder) => {
				decoder.decode(packet.data, MAX_PACKET_SAMPLES, &mut self.interleaved_samples)
			}
		};
		let frame_count = match decode_result {
			Ok(frame_count) => {
				frame_count
			}
			Err(_) => {
				return decode_error("opus: 패킷을 풀 수 없습니다");
			}
		};
		self.buffer.render_uninit(Some(frame_count));
		let channel_count = self.channel_count;
		for (output_channel, source) in self.output_channel_source_list.iter().enumerate() {
			let plane = match self.buffer.plane_mut(output_channel) {
				Some(plane) => {
					plane
				}
				None => {
					continue;
				}
			};
			match source {
				Some(source_channel) => {
					for (frame_index, sample) in plane.iter_mut().enumerate() {
						*sample = self.interleaved_samples[frame_index * channel_count + source_channel];
					}
				}
				None => {
					plane.fill(0.0);
				}
			}
		}
		Ok(self.buffer.as_generic_audio_buffer_ref())
	}

	//==============================================================================
	/// 끝내기. (따로 셈할 것이 없어 기본 결과)
	//==============================================================================
	fn finalize(&mut self) -> FinalizeResult {
		FinalizeResult::default()
	}

	//==============================================================================
	/// 마지막으로 푼 버퍼.
	//==============================================================================
	fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
		self.buffer.as_generic_audio_buffer_ref()
	}
}

impl RegisterableAudioDecoder for OpusPacketDecoder {
	//==============================================================================
	/// 등록표가 부르는 생성. (create 를 상자에 담는다)
	//==============================================================================
	fn try_registry_new(params: &AudioCodecParameters, _opts: &AudioDecoderOptions) -> Result<Box<dyn AudioDecoder>>
	where
		Self: Sized,
	{
		Ok(Box::new(OpusPacketDecoder::create(params)?))
	}

	//==============================================================================
	/// 받는 코덱. (Opus 하나)
	//==============================================================================
	fn supported_codecs() -> &'static [SupportedAudioCodec] {
		&OPUS_SUPPORTED_CODEC_LIST
	}
}

static OPUS_SUPPORTED_CODEC_LIST: [SupportedAudioCodec; 1] = [SupportedAudioCodec {
	id: CODEC_ID_OPUS,
	info: CodecInfo {
		short_name: "opus",
		long_name: "Opus (opus-pure)",
		profiles: &[],
	},
}];

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// OpusHead 읽기. (패밀리 1, 6 채널의 pre-skip, 출력 게인, 스트림 수, 매핑 표)
	//==============================================================================
	#[test]
	fn opus_header_reads_family_one_map() {
		let mut bytes = b"OpusHead".to_vec();
		bytes.extend_from_slice(&[1, 6, 0x38, 0x01, 0x80, 0xbb, 0, 0, 0x00, 0x01, 1, 4, 2, 0, 4, 1, 2, 3, 5]);
		let header = OpusHeader::parse(&bytes).expect("header");
		assert_eq!(header.channel_count, 6);
		assert_eq!(header.pre_skip, 312);
		assert_eq!(header.gain_q8, 256);
		assert_eq!(header.stream_count, 4);
		assert_eq!(header.coupled_count, 2);
		assert_eq!(header.stream_map, vec![0, 4, 1, 2, 3, 5]);
	}
}

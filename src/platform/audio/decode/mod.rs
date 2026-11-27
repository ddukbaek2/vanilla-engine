//==============================================================================
//! 소리 해석. (데스크탑, 원본이 쓰는 크롬 decodeAudioData 와 같게)
//! - 크롬(158)은 FFmpeg 로 디먹스하고 FLAC, MP3, PCM, Vorbis 를 symphonia 로, Opus 를 libopus 로, AAC 를 FFmpeg 로 푼다. (media/filters/audio_file_reader.cc)
//!   여기서는 디먹스도 symphonia 0.6 의 풀기(wav, ogg, mkv(webm), isomp4(m4a), flac, mp3, adts)로 하고,
//!   FLAC, MP3, PCM, Vorbis 는 크롬과 같은 symphonia 해석기, Opus 는 opus.rs(opus-pure),
//!   AAC 는 aac.rs 가 AAC-LC 2 채널까지 symphonia, HE-AAC 와 3 채널 이상은 oxideav-aac 로 푼다.
//! - 꼴은 내용으로 가린다. (decodeAudioData 처럼 형(MIME)이나 확장자를 보지 않는다. symphonia 의 탐지)
//! - 차례: 패킷마다 풀기 → 크롬처럼 앞뒤 다듬기(trim.rs) → 정수 표본을 크롬 식으로 실수로 → 채널마다 원본 DECODE_SAMPLE_RATE(44100)로 바꾸기(resample.rs)
//!   → WebAudio 스피커 규칙으로 1 채널이나 2 채널로 내리기. (크롬은 AudioBuffer 를 채널 그대로 두고 그릴 때 내린다. 뒤 노드가 모두 선형이라 미리 내려도 같다.
//!   모노는 StereoPanner 식이 달라 모노로 둔다)
//==============================================================================
mod aac;
mod opus;
pub(crate) mod resample;
pub(crate) mod stream;
mod trim;

use opus::OpusPacketDecoder;
use std::io::Cursor;
use std::sync::OnceLock;
use symphonia::core::audio::{Audio, GenericAudioBufferRef};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

//==============================================================================
/// 코덱 등록표. (symphonia 가 켠 해석기와 엔진 Opus 해석기)
//==============================================================================
fn get_codec_registry() -> &'static CodecRegistry {
	static CODEC_REGISTRY: OnceLock<CodecRegistry> = OnceLock::new();
	CODEC_REGISTRY.get_or_init(|| {
		let mut codec_registry = CodecRegistry::new();
		symphonia::default::register_enabled_codecs(&mut codec_registry);
		codec_registry.register_audio_decoder::<OpusPacketDecoder>();
		codec_registry.register_audio_decoder_at_tier::<aac::AacPacketDecoder>(symphonia::core::common::Tier::Preferred);
		codec_registry
	})
}

/// 푼 소리의 표본율. (원본 audioasset.js DECODE_SAMPLE_RATE, OfflineAudioContext(1, 1, 44100) 로 풀어 이 표본율이 된다)
pub(crate) const DECODE_SAMPLE_RATE: u32 = 44100;

//==============================================================================
/// 푼 소리. (크롬 AudioBuffer 를 스피커 규칙으로 1 채널이나 2 채널로 내린 것)
//==============================================================================
pub(crate) struct DecodedAudio {
	pub sample_rate: u32,
	pub channel_data: Vec<Vec<f32>>,
}

impl DecodedAudio {
	//==============================================================================
	/// 프레임 수. (첫 채널의 길이, 채널이 없으면 0, 시험)
	//==============================================================================
	#[cfg(test)]
	pub(crate) fn get_frame_count(&self) -> usize {
		self.channel_data.first().map(|channel| channel.len()).unwrap_or(0)
	}
}

//==============================================================================
/// 소리 파일 바이트 풀기. (크롬 decodeAudioData 와 같은 길이, 채널, 표본)
//==============================================================================
pub(crate) fn decode_audio_data(bytes: &[u8]) -> Result<DecodedAudio, String> {
	let channel_list = decode_audio_channels(bytes)?;
	Ok(DecodedAudio {
		sample_rate: DECODE_SAMPLE_RATE,
		channel_data: down_mix_channels(channel_list),
	})
}

//==============================================================================
/// 크롬 AudioBuffer 와 같은 채널들. (내리기 전, DECODE_SAMPLE_RATE, HRTF 자료 Composite.flac 도 이 길로 푼다)
//==============================================================================
pub(crate) fn decode_audio_channels(bytes: &[u8]) -> Result<Vec<Vec<f32>>, String> {
	let raw = decode_raw(bytes)?;
	let trimmed_channel_list = trim::trim_like_chrome(&raw, bytes);
	if trimmed_channel_list.first().map(|channel| channel.is_empty()).unwrap_or(true) {
		return Err(String::from("no audio frames"));
	}
	Ok(trimmed_channel_list.iter().map(|channel| resample::resample_channel(channel, raw.sample_rate as f64, DECODE_SAMPLE_RATE as f64)).collect())
}

//==============================================================================
/// WebAudio 스피커 규칙으로 1 채널이나 2 채널로 내리기. (Web Audio API 4.5 Channel Up-Mixing and Down-Mixing)
/// - 1: 모노 그대로, 2: 그대로, 4(쿼드): L = 0.5 * (L + SL), R = 0.5 * (R + SR),
///   6(5.1): L = L + sqrt(0.5) * (C + SL), R = R + sqrt(0.5) * (C + SR), 그 밖(3, 5, 7, 8 ...)은 discrete 로 앞 두 채널.
/// - 채널 차례는 크로미엄 배치다. (L R C LFE SL SR, 쿼드 L R SL SR)
//==============================================================================
pub(crate) fn down_mix_channels(mut channel_list: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
	let half_root = std::f32::consts::FRAC_1_SQRT_2;
	match channel_list.len() {
		0 | 1 | 2 => {
			channel_list
		}
		4 => {
			let frame_count = channel_list[0].len();
			let left: Vec<f32> = (0..frame_count).map(|index| 0.5 * (channel_list[0][index] + channel_list[2][index])).collect();
			let right: Vec<f32> = (0..frame_count).map(|index| 0.5 * (channel_list[1][index] + channel_list[3][index])).collect();
			vec![left, right]
		}
		6 => {
			let frame_count = channel_list[0].len();
			let left: Vec<f32> = (0..frame_count).map(|index| channel_list[0][index] + half_root * (channel_list[2][index] + channel_list[4][index])).collect();
			let right: Vec<f32> = (0..frame_count).map(|index| channel_list[1][index] + half_root * (channel_list[2][index] + channel_list[5][index])).collect();
			vec![left, right]
		}
		_ => {
			channel_list.truncate(2);
			channel_list
		}
	}
}

//==============================================================================
/// 패킷 하나를 다듬지 않고 푼 것. (symphonia 가 매긴 길이와 다듬기 값을 함께 든다)
//==============================================================================
pub(crate) struct RawPacket {
	/// 패킷 시각. (트랙 시간 단위, 스트리밍의 길이 어림에 쓴다)
	pub timestamp: u64,
	pub duration: u64,
	pub trim_start: u64,
	pub trim_end: u64,
	pub planes: Vec<Vec<f32>>,
}

//==============================================================================
/// 다듬지 않고 푼 소리 전체.
//==============================================================================
pub(crate) struct RawDecode {
	/// symphonia 의 풀기와 해석기 이름. (wave, ogg, matroska, isomp4, mp3, aac, flac / pcm_*, flac, mp3, vorbis, aac, opus)
	pub format_name: String,
	pub codec_name: String,
	pub sample_rate: u32,
	pub channel_count: usize,
	pub time_base: Option<(u32, u32)>,
	pub track_delay: Option<u32>,
	pub track_padding: Option<u32>,
	/// OpusHead 의 pre-skip. (Opus 가 아니면 0)
	pub opus_pre_skip: u32,
	pub packet_list: Vec<RawPacket>,
}

//==============================================================================
/// 표본을 실수로. (크롬 media/filters/symphonia_decoder_bridge.rs 가 넘기는 꼴과 media/base/audio_sample_types.h FixedSampleTypeTraits::To)
/// - 정수는 0 보다 작으면 (0 - 최소)로, 아니면 (최대 - 0)으로 나눈다. (s16 은 -32768 과 32767)
/// - S24 는 32 비트로 8 비트 올려 32 비트 식으로, S32 는 원본이 16 비트 이하면(FLAC) 16 비트로 내려(>> 16) 16 비트 식으로.
/// - MP3 의 실수는 -1 ~ 1 로 자르고 NaN 은 0 이다.
//==============================================================================
fn copy_planes(buffer: &GenericAudioBufferRef<'_>, bits_per_sample: Option<u32>, is_clamped: bool) -> Vec<Vec<f32>> {
	macro_rules! convert_planes {
		($buffer:expr, $convert:expr) => {{
			let mut plane_list: Vec<Vec<f32>> = Vec::with_capacity($buffer.num_planes());
			for plane_index in 0..$buffer.num_planes() {
				let plane = $buffer.plane(plane_index).unwrap_or(&[]);
				plane_list.push(plane.iter().map(|sample| $convert(*sample)).collect());
			}
			plane_list
		}};
	}
	match buffer {
		GenericAudioBufferRef::U8(buffer) => {
			convert_planes!(buffer, |sample: u8| {
				let offset_value = sample as f32 - 128.0;
				if offset_value < 0.0 { offset_value * (1.0 / 128.0) } else { offset_value * (1.0 / 127.0) }
			})
		}
		GenericAudioBufferRef::S16(buffer) => {
			convert_planes!(buffer, |sample: i16| {
				let offset_value = sample as f32;
				if offset_value < 0.0 { offset_value * (1.0 / 32768.0) } else { offset_value * (1.0 / 32767.0) }
			})
		}
		GenericAudioBufferRef::S24(buffer) => {
			convert_planes!(buffer, |sample: symphonia::core::audio::sample::i24| {
				let offset_value = (sample.inner() << 8) as f32;
				if offset_value < 0.0 { offset_value * (1.0 / 2147483648.0) } else { offset_value * (1.0 / 2147483647.0) }
			})
		}
		GenericAudioBufferRef::S32(buffer) => {
			if bits_per_sample.map(|bits| bits <= 16).unwrap_or(false) {
				convert_planes!(buffer, |sample: i32| {
					let offset_value = (sample >> 16) as i16 as f32;
					if offset_value < 0.0 { offset_value * (1.0 / 32768.0) } else { offset_value * (1.0 / 32767.0) }
				})
			}
			else {
				convert_planes!(buffer, |sample: i32| {
					let offset_value = sample as f32;
					if offset_value < 0.0 { offset_value * (1.0 / 2147483648.0) } else { offset_value * (1.0 / 2147483647.0) }
				})
			}
		}
		GenericAudioBufferRef::F32(buffer) => {
			if is_clamped {
				convert_planes!(buffer, |sample: f32| if sample.is_nan() { 0.0 } else { sample.clamp(-1.0, 1.0) })
			}
			else {
				convert_planes!(buffer, |sample: f32| sample)
			}
		}
		GenericAudioBufferRef::F64(buffer) => {
			convert_planes!(buffer, |sample: f64| sample as f32)
		}
		GenericAudioBufferRef::S8(buffer) => {
			convert_planes!(buffer, |sample: i8| {
				let offset_value = sample as f32;
				if offset_value < 0.0 { offset_value * (1.0 / 128.0) } else { offset_value * (1.0 / 127.0) }
			})
		}
		GenericAudioBufferRef::U16(buffer) => {
			convert_planes!(buffer, |sample: u16| {
				let offset_value = sample as f32 - 32768.0;
				if offset_value < 0.0 { offset_value * (1.0 / 32768.0) } else { offset_value * (1.0 / 32767.0) }
			})
		}
		GenericAudioBufferRef::U24(buffer) => {
			convert_planes!(buffer, |sample: symphonia::core::audio::sample::u24| {
				let offset_value = ((sample.inner() as i32 - 8388608) << 8) as f32;
				if offset_value < 0.0 { offset_value * (1.0 / 2147483648.0) } else { offset_value * (1.0 / 2147483647.0) }
			})
		}
		GenericAudioBufferRef::U32(buffer) => {
			convert_planes!(buffer, |sample: u32| {
				let offset_value = (sample as i64 - 2147483648) as f32;
				if offset_value < 0.0 { offset_value * (1.0 / 2147483648.0) } else { offset_value * (1.0 / 2147483647.0) }
			})
		}
	}
}

//==============================================================================
/// 패킷 읽개. (symphonia 로 디먹스해 패킷마다 푼다, 통째로 풀기(decode_raw)와 스트리밍(stream.rs)이 같이 쓴다)
/// - 풀기 오류가 난 패킷은 크롬 SymphoniaAudioDecoder 처럼 버리고 이어 가되, 두 번 잇따르면 실패로 친다.
//==============================================================================
pub(crate) struct PacketReader {
	format_reader: Box<dyn symphonia::core::formats::FormatReader>,
	decoder: Box<dyn symphonia::core::codecs::audio::AudioDecoder>,
	track_id: u32,
	bits_per_sample: Option<u32>,
	is_clamped: bool,
	consecutive_error_count: usize,
	/// 머리 정보. (packet_list 는 비어 있다, sample_rate 와 channel_count 는 푼 패킷마다 고친다)
	header: RawDecode,
}

impl PacketReader {
	//==============================================================================
	/// 열기. (바이트나 파일, 꼴은 내용으로 가린다)
	//==============================================================================
	pub(crate) fn open(media_source: Box<dyn symphonia::core::io::MediaSource>) -> Result<PacketReader, String> {
		let media_source_stream = MediaSourceStream::new(media_source, Default::default());
		let format_reader = symphonia::default::get_probe().probe(&Hint::new(), media_source_stream, FormatOptions::default(), MetadataOptions::default()).map_err(|error| format!("{}", error))?;
		let format_name = format_reader.format_info().short_name.to_string();
		let track = format_reader.default_track(TrackType::Audio).ok_or_else(|| String::from("no audio track"))?.clone();
		let codec_params = track.codec_params.as_ref().and_then(|codec_params| codec_params.audio()).ok_or_else(|| String::from("no audio codec parameters"))?.clone();
		let decoder = get_codec_registry().make_audio_decoder(&codec_params, &AudioDecoderOptions::default()).map_err(|error| format!("{}", error))?;
		let codec_name = decoder.codec_info().short_name.to_string();
		Ok(PacketReader {
			format_reader,
			decoder,
			track_id: track.id,
			bits_per_sample: codec_params.bits_per_sample,
			is_clamped: codec_params.codec == symphonia::core::codecs::audio::well_known::CODEC_ID_MP3,
			consecutive_error_count: 0,
			header: RawDecode {
				format_name,
				codec_name,
				sample_rate: codec_params.sample_rate.unwrap_or(0),
				channel_count: codec_params.channels.as_ref().map(|channels| channels.count()).unwrap_or(0),
				time_base: track.time_base.map(|time_base| (time_base.numer.get(), time_base.denom.get())),
				track_delay: track.delay,
				track_padding: track.padding,
				opus_pre_skip: OpusPacketDecoder::read_pre_skip(&codec_params),
				packet_list: Vec::new(),
			},
		})
	}

	//==============================================================================
	/// 머리 정보. (지금까지 푼 패킷의 표본율과 채널 수)
	//==============================================================================
	pub(crate) fn get_header(&self) -> &RawDecode {
		&self.header
	}

	//==============================================================================
	/// 다음 패킷. (is_decoded 가 거짓이면 풀지 않고 길이와 다듬기 값만, 끝이면 None)
	//==============================================================================
	pub(crate) fn read_packet(&mut self, is_decoded: bool) -> Result<Option<RawPacket>, String> {
		loop {
			let mut packet = match self.format_reader.next_packet() {
				Ok(Some(packet)) => {
					packet
				}
				Ok(None) => {
					return Ok(None);
				}
				Err(error) => {
					return Err(format!("{}", error));
				}
			};
			if packet.track_id != self.track_id {
				continue;
			}
			// 해석기가 다듬지 않게 다듬기 값을 덜어 둔다. (크롬도 다듬지 않은 패킷을 넘긴다)
			let trim_start = packet.trim_start.get();
			let trim_end = packet.trim_end.get();
			packet.trim_start = symphonia::core::units::Duration::from(0u64);
			packet.trim_end = symphonia::core::units::Duration::from(0u64);
			let planes = if !is_decoded {
				Vec::new()
			}
			else {
				match self.decoder.decode(&packet) {
					Ok(buffer) => {
						self.consecutive_error_count = 0;
						self.header.sample_rate = buffer.spec().rate();
						self.header.channel_count = buffer.spec().channels().count();
						copy_planes(&buffer, self.bits_per_sample, self.is_clamped)
					}
					Err(symphonia::core::errors::Error::DecodeError(error)) => {
						self.consecutive_error_count += 1;
						if self.consecutive_error_count > 1 {
							return Err(format!("decode: {}", error));
						}
						vec![Vec::new(); self.header.channel_count]
					}
					Err(error) => {
						return Err(format!("decode: {}", error));
					}
				}
			};
			return Ok(Some(RawPacket {
				timestamp: packet.pts.get().max(0) as u64,
				duration: packet.dur.get(),
				trim_start,
				trim_end,
				planes,
			}));
		}
	}
}

//==============================================================================
/// 다듬지 않고 풀기. (symphonia 로 디먹스해 패킷마다 풀고, 다듬기는 trim.rs 가 크롬처럼 한다)
//==============================================================================
pub(crate) fn decode_raw(bytes: &[u8]) -> Result<RawDecode, String> {
	let mut packet_reader = PacketReader::open(Box::new(Cursor::new(bytes.to_vec())))?;
	let mut packet_list: Vec<RawPacket> = Vec::new();
	while let Some(raw_packet) = packet_reader.read_packet(true)? {
		packet_list.push(raw_packet);
	}
	let mut raw_decode = packet_reader.header;
	raw_decode.packet_list = packet_list;
	Ok(raw_decode)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// 시험 소리. (tests/assets/audio, tools 없이 ffmpeg 6.0 과 맥 afconvert 로 만든 수 KB 짜리)
	const AUDIO_ASSET_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/audio");

	//==============================================================================
	/// 크롬(158) decodeAudioData 의 결과와 견준다. (tests/assets/audio/chromedecode.cjs 가 찍은 chrome_decode.json)
	/// - 길이와 채널 수는 같아야 하고, 정한 자리의 표본은 2e-6 안, 채널마다 제곱합은 1e-5 (상대) 안.
	///   FLAC, MP3, PCM, Vorbis 는 크롬과 같은 symphonia 라 비트까지 같고, Opus(libopus 와 opus-pure)와 AAC(FFmpeg 와 symphonia, oxideav-aac)는 1.5e-7 안팎으로 다르다.
	//==============================================================================
	#[test]
	fn decode_matches_chrome() {
		let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/chrome_decode.json", AUDIO_ASSET_DIRECTORY)).expect("chrome_decode.json")).expect("json");
		let file_table = json["files"].as_object().expect("files");
		assert_eq!(file_table.len(), 27);
		for (name, file) in file_table {
			let bytes = std::fs::read(format!("{}/{}", AUDIO_ASSET_DIRECTORY, name)).expect("asset");
			let channel_list = decode_audio_channels(&bytes).unwrap_or_else(|error| panic!("{}: {}", name, error));
			assert_eq!(channel_list.len() as u64, file["channels"].as_u64().unwrap(), "{} 채널", name);
			assert_eq!(channel_list[0].len() as u64, file["length"].as_u64().unwrap(), "{} 길이", name);
			let index_list: Vec<usize> = file["index"].as_array().unwrap().iter().map(|value| value.as_u64().unwrap() as usize).collect();
			for (channel_index, channel) in channel_list.iter().enumerate() {
				let value_list = file["values"][channel_index].as_array().unwrap();
				for (index, value) in index_list.iter().zip(value_list.iter()) {
					let expected = value.as_f64().unwrap();
					assert!((channel[*index] as f64 - expected).abs() < 2e-6, "{} 채널 {} 자리 {}: {} / {}", name, channel_index, index, channel[*index], expected);
				}
				let energy: f64 = channel.iter().map(|sample| *sample as f64 * *sample as f64).sum();
				let expected_energy = file["energy"][channel_index].as_f64().unwrap();
				assert!((energy - expected_energy).abs() <= expected_energy * 1e-5 + 1e-9, "{} 채널 {} 제곱합: {} / {}", name, channel_index, energy, expected_energy);
			}
		}
	}

	//==============================================================================
	/// 꼴마다 첫 표본과 길이, 내린 채널. (decode_audio_data 가 엔진에 넘기는 것)
	//==============================================================================
	#[test]
	fn decode_audio_data_down_mixes_like_webaudio() {
		let decode = |name: &str| decode_audio_data(&std::fs::read(format!("{}/{}", AUDIO_ASSET_DIRECTORY, name)).unwrap()).unwrap();
		// webm opus: webm.cjs 의 꼴. 0.25 초를 크롬처럼 11002 프레임으로. (CodecDelay 6.5ms 를 FFmpeg 가 7ms 로 반올림한 몫까지 버린다)
		let opus_webm = decode("opus_stereo.webm");
		assert_eq!(opus_webm.sample_rate, 44100);
		assert_eq!(opus_webm.channel_data.len(), 2);
		assert_eq!(opus_webm.get_frame_count(), 11002);
		// ogg opus 는 pre-skip 과 granule 로 12000 프레임(48kHz) → 11024. (Blink 의 길이 셈이 소수 아래를 버린다)
		assert_eq!(decode("opus_stereo.ogg").get_frame_count(), 11024);
		// 모노는 모노로 둔다.
		let mono = decode("opus_mono.webm");
		assert_eq!(mono.channel_data.len(), 1);
		// 5.1 은 L + sqrt(0.5) * (C + SL), R + sqrt(0.5) * (C + SR).
		let six_channel_list = decode_audio_channels(&std::fs::read(format!("{}/flac_51.flac", AUDIO_ASSET_DIRECTORY)).unwrap()).unwrap();
		let six = decode("flac_51.flac");
		assert_eq!(six_channel_list.len(), 6);
		assert_eq!(six.channel_data.len(), 2);
		let index = 500;
		let expected_left = six_channel_list[0][index] + std::f32::consts::FRAC_1_SQRT_2 * (six_channel_list[2][index] + six_channel_list[4][index]);
		let expected_right = six_channel_list[1][index] + std::f32::consts::FRAC_1_SQRT_2 * (six_channel_list[2][index] + six_channel_list[5][index]);
		assert_eq!(six.channel_data[0][index], expected_left);
		assert_eq!(six.channel_data[1][index], expected_right);
		// 쿼드는 0.5 * (L + SL), 0.5 * (R + SR).
		let quad_channel_list = decode_audio_channels(&std::fs::read(format!("{}/wav_s16_quad.wav", AUDIO_ASSET_DIRECTORY)).unwrap()).unwrap();
		let quad = decode("wav_s16_quad.wav");
		assert_eq!(quad.channel_data[0][100], 0.5 * (quad_channel_list[0][100] + quad_channel_list[2][100]));
		assert_eq!(quad.channel_data[1][100], 0.5 * (quad_channel_list[1][100] + quad_channel_list[3][100]));
		// 16 비트 PCM 은 양수를 32767 로, 음수를 32768 로 나눈다. (440Hz 0.5 의 첫 표본들)
		let pcm = decode("wav_s16_stereo.wav");
		assert_eq!(pcm.channel_data[0][0], 0.0);
		assert_eq!(pcm.channel_data[0][1], 1026.0 * (1.0 / 32767.0));
	}

	//==============================================================================
	/// 소리가 아닌 바이트는 실패.
	//==============================================================================
	#[test]
	fn decode_rejects_unknown_bytes() {
		assert!(decode_audio_data(b"not an audio file at all").is_err());
		assert!(decode_audio_data(&[]).is_err());
	}
}

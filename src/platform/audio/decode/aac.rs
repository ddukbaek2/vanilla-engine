//==============================================================================
//! AAC 해석기. (symphonia 코덱 등록표에 symphonia 의 AAC 보다 앞에 넣는 엔진 해석기)
//! - 크롬은 AAC 를 FFmpeg 로 푼다. AAC-LC 에 더해 HE-AAC v1(SBR), v2(PS)와 3 채널 이상(5.1 같은)을 받는다.
//! - AAC-LC 2 채널까지는 symphonia 의 AAC 로 푼다. (크롬과 1e-7 안팎으로 같고, PNS 잡음도 같다)
//!   symphonia 0.6 이 받지 않는 것(ASC 에 적힌 SBR, PS, 3 채널 이상, LC 가 아닌 꼴)과 몸통에만 SBR 이 실린 것(ADTS 의 HE-AAC 같은)은
//!   oxideav-aac 로 푼다. (ISO/IEC 14496-3, 13818-7 명세로 짠 순수 러스트 해석기, MIT, 저장소 OxideAV/oxideav-aac)
//!   ASC 로는 가릴 수 없는 몸통의 SBR 은 첫 패킷을 oxideav-aac 로 풀어 표본율이 두 배가 되는지로 가린다. (크롬 FFmpeg 도 몸통의 SBR 로 두 배 표본율이 된다)
//! - ASC(MP4 의 esds)가 있으면 그것으로, 없으면(ADTS) 코덱 인자의 표본율과 채널 수로 AAC-LC 로 연다.
//! - 채널은 크로미엄 차례로 낸다. (5.1 은 L R C LFE SL SR) oxideav-aac 의 표본은 정수로 반올림하기 전의 실수다.
//==============================================================================
use oxideav_aac::asc::AudioSpecificConfig;
use oxideav_aac::decode::StreamDecoder;
use symphonia::core::audio::{AsGenericAudioBufferRef, Audio, AudioBuffer, AudioMut, AudioSpec, Channels, GenericAudioBufferRef, Position};
use symphonia::core::codecs::audio::well_known::CODEC_ID_AAC;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult};
use symphonia::core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia::core::codecs::CodecInfo;
use symphonia::core::errors::{decode_error, unsupported_error, Result};
use symphonia::core::packet::PacketRef;

/// AAC 표본율 표. (ISO/IEC 14496-3 Table 1.18 samplingFrequencyIndex)
const SAMPLING_FREQUENCY_TABLE: [u32; 13] = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];

//==============================================================================
/// oxideav-aac 로 푸는 쪽. (스트림 해독기와 열 때의 값)
//==============================================================================
struct OxideavDecoder {
	stream_decoder: StreamDecoder,
	audio_object_type: u8,
	sampling_frequency_index: u8,
	sample_rate: u32,
	channel_configuration: u8,
}

impl OxideavDecoder {
	//==============================================================================
	/// 생성. (실수 표본을 받아 두게 켠다, last_pcm_f64 로 읽는다)
	//==============================================================================
	fn new(audio_object_type: u8, sampling_frequency_index: u8, sample_rate: u32, channel_configuration: u8) -> OxideavDecoder {
		let mut stream_decoder = StreamDecoder::new();
		stream_decoder.set_float_pcm_capture(true);
		OxideavDecoder {
			stream_decoder,
			audio_object_type,
			sampling_frequency_index,
			sample_rate,
			channel_configuration,
		}
	}

	//==============================================================================
	/// 패킷 하나 풀어 버퍼에 채우기. (표본율, 채널 수)
	//==============================================================================
	fn decode_into(&mut self, data: &[u8], buffer: &mut AudioBuffer<f32>) -> Option<(u32, usize)> {
		let decoded_frame = self.stream_decoder.decode_raw_data_block(self.audio_object_type, self.sampling_frequency_index, self.sample_rate, self.channel_configuration, 1, data).ok()?;
		let channel_count = decoded_frame.channels.max(1);
		let float_samples = self.stream_decoder.last_pcm_f64().unwrap_or(&[]);
		let frame_count = float_samples.len() / channel_count;
		prepare_buffer(buffer, decoded_frame.sample_rate, channel_count, frame_count);
		for channel_index in 0..channel_count {
			if let Some(plane) = buffer.plane_mut(channel_index) {
				for (frame_index, sample) in plane.iter_mut().enumerate() {
					*sample = float_samples[frame_index * channel_count + channel_index] as f32;
				}
			}
		}
		Some((decoded_frame.sample_rate, channel_count))
	}
}

//==============================================================================
/// 버퍼를 그 꼴과 길이로 비우고 채울 자리를 잡는다.
//==============================================================================
fn prepare_buffer(buffer: &mut AudioBuffer<f32>, sample_rate: u32, channel_count: usize, frame_count: usize) {
	let spec = AudioSpec::new(sample_rate, create_chromium_channels(channel_count));
	if buffer.spec() != &spec || buffer.capacity() < frame_count {
		*buffer = AudioBuffer::new(spec, frame_count.max(2048));
	}
	buffer.clear();
	buffer.render_uninit(Some(frame_count));
}

//==============================================================================
/// 푸는 길. (아직 안 가림: 첫 패킷에서 몸통의 SBR 을 본다)
//==============================================================================
enum AacRoute {
	Undecided(Box<dyn AudioDecoder>, OxideavDecoder),
	Symphonia(Box<dyn AudioDecoder>),
	Oxideav(OxideavDecoder),
}

//==============================================================================
/// symphonia 에 넣는 AAC 해석기.
//==============================================================================
pub(crate) struct AacPacketDecoder {
	codec_params: AudioCodecParameters,
	route: Option<AacRoute>,
	core_sample_rate: u32,
	buffer: AudioBuffer<f32>,
}

impl AacPacketDecoder {
	//==============================================================================
	/// 생성. (ASC 가 있으면 그것으로, 없으면 표본율과 채널 수로 AAC-LC. symphonia 가 받으면 첫 패킷까지 둘 다 든다)
	//==============================================================================
	pub(crate) fn create(codec_params: &AudioCodecParameters) -> Result<AacPacketDecoder> {
		let (audio_object_type, sampling_frequency_index, sample_rate, channel_configuration, has_explicit_extension) = match codec_params.extra_data.as_deref() {
			Some(extra_data) if !extra_data.is_empty() => {
				match AudioSpecificConfig::parse(extra_data) {
					Ok((audio_specific_config, _)) => {
						(audio_specific_config.aot, audio_specific_config.sampling_frequency_index, audio_specific_config.sample_rate, audio_specific_config.channel_configuration, audio_specific_config.sbr_present || audio_specific_config.ps_present)
					}
					Err(_) => {
						return unsupported_error("aac: AudioSpecificConfig 를 풀 수 없습니다");
					}
				}
			}
			_ => {
				let sample_rate = match codec_params.sample_rate {
					Some(sample_rate) => {
						sample_rate
					}
					None => {
						return unsupported_error("aac: 표본율이 없습니다");
					}
				};
				let Some(sampling_frequency_index) = SAMPLING_FREQUENCY_TABLE.iter().position(|rate| *rate == sample_rate) else {
					return unsupported_error("aac: 표본율 번호가 없습니다");
				};
				let channel_count = codec_params.channels.as_ref().map(|channels| channels.count()).unwrap_or(2);
				let channel_configuration = match channel_count {
					1..=6 => {
						channel_count as u8
					}
					8 => {
						7
					}
					_ => {
						return unsupported_error("aac: 채널 수를 채널 구성으로 바꿀 수 없습니다");
					}
				};
				(2, sampling_frequency_index as u8, sample_rate, channel_configuration, false)
			}
		};
		let oxideav_decoder = OxideavDecoder::new(audio_object_type, sampling_frequency_index, sample_rate, channel_configuration);
		let is_symphonia_candidate = audio_object_type == 2 && !has_explicit_extension && (channel_configuration == 1 || channel_configuration == 2);
		let route = match is_symphonia_candidate {
			true => {
				match symphonia::default::codecs::AacDecoder::try_registry_new(codec_params, &AudioDecoderOptions::default()) {
					Ok(symphonia_decoder) => {
						AacRoute::Undecided(symphonia_decoder, oxideav_decoder)
					}
					Err(_) => {
						AacRoute::Oxideav(oxideav_decoder)
					}
				}
			}
			false => {
				AacRoute::Oxideav(oxideav_decoder)
			}
		};
		let mut decoder_codec_params = codec_params.clone();
		decoder_codec_params.with_sample_rate(sample_rate);
		Ok(AacPacketDecoder {
			codec_params: decoder_codec_params,
			route: Some(route),
			core_sample_rate: sample_rate,
			buffer: AudioBuffer::new(AudioSpec::new(sample_rate, create_chromium_channels(2)), 2048),
		})
	}

	//==============================================================================
	/// symphonia 로 풀어 버퍼에 베끼기.
	//==============================================================================
	fn decode_with_symphonia(symphonia_decoder: &mut Box<dyn AudioDecoder>, packet: &PacketRef<'_>, buffer: &mut AudioBuffer<f32>) -> Result<()> {
		let decoded = symphonia_decoder.decode_ref(packet)?;
		let GenericAudioBufferRef::F32(decoded) = decoded else {
			return decode_error("aac: 실수가 아닌 버퍼");
		};
		let channel_count = decoded.num_planes();
		prepare_buffer(buffer, decoded.spec().rate(), channel_count, decoded.frames());
		for channel_index in 0..channel_count {
			if let (Some(source), Some(destination)) = (decoded.plane(channel_index), buffer.plane_mut(channel_index)) {
				destination.copy_from_slice(source);
			}
		}
		Ok(())
	}
}

//==============================================================================
/// 크로미엄 차례의 채널 자리. (1 모노, 2 L R, 3 L R C, 4 L R C 뒤가운데, 5 L R C SL SR, 6 5.1, 8 7.1)
//==============================================================================
fn create_chromium_channels(channel_count: usize) -> Channels {
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
		5 => {
			Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::FRONT_CENTER | Position::SIDE_LEFT | Position::SIDE_RIGHT
		}
		6 => {
			Position::FRONT_LEFT | Position::FRONT_RIGHT | Position::FRONT_CENTER | Position::LFE1 | Position::SIDE_LEFT | Position::SIDE_RIGHT
		}
		_ => {
			return Channels::Discrete(channel_count as u16);
		}
	};
	Channels::Positioned(position)
}

impl AudioDecoder for AacPacketDecoder {
	//==============================================================================
	/// 처음 상태로. (symphonia 쪽은 reset, oxideav-aac 쪽은 같은 값으로 새로 만든다)
	//==============================================================================
	fn reset(&mut self) {
		match self.route.as_mut() {
			Some(AacRoute::Undecided(symphonia_decoder, _)) | Some(AacRoute::Symphonia(symphonia_decoder)) => {
				symphonia_decoder.reset();
			}
			Some(AacRoute::Oxideav(oxideav_decoder)) => {
				*oxideav_decoder = OxideavDecoder::new(oxideav_decoder.audio_object_type, oxideav_decoder.sampling_frequency_index, oxideav_decoder.sample_rate, oxideav_decoder.channel_configuration);
			}
			None => {}
		}
	}

	//==============================================================================
	/// 코덱 정보. (등록표의 AAC 항목)
	//==============================================================================
	fn codec_info(&self) -> &CodecInfo {
		&AAC_SUPPORTED_CODEC_LIST[0].info
	}

	//==============================================================================
	/// 코덱 인자. (ASC 의 표본율로 고친 것)
	//==============================================================================
	fn codec_params(&self) -> &AudioCodecParameters {
		&self.codec_params
	}

	//==============================================================================
	/// 패킷 하나 풀기. (첫 패킷에서 푸는 길을 가린다, 실패하면 버퍼를 비우고 Err)
	//==============================================================================
	fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
		let route = self.route.take();
		let (next_route, result) = match route {
			Some(AacRoute::Undecided(mut symphonia_decoder, mut oxideav_decoder)) => {
				// 첫 패킷: 몸통에 SBR 이 있으면(표본율이 두 배) oxideav-aac 로, 아니면 symphonia 로 간다.
				match oxideav_decoder.decode_into(packet.data, &mut self.buffer) {
					Some((sample_rate, _)) if sample_rate != self.core_sample_rate => {
						(AacRoute::Oxideav(oxideav_decoder), Ok(()))
					}
					_ => {
						let result = AacPacketDecoder::decode_with_symphonia(&mut symphonia_decoder, packet, &mut self.buffer);
						(AacRoute::Symphonia(symphonia_decoder), result)
					}
				}
			}
			Some(AacRoute::Symphonia(mut symphonia_decoder)) => {
				let result = AacPacketDecoder::decode_with_symphonia(&mut symphonia_decoder, packet, &mut self.buffer);
				(AacRoute::Symphonia(symphonia_decoder), result)
			}
			Some(AacRoute::Oxideav(mut oxideav_decoder)) => {
				let result = match oxideav_decoder.decode_into(packet.data, &mut self.buffer) {
					Some(_) => {
						Ok(())
					}
					None => {
						decode_error("aac: 패킷을 풀 수 없습니다")
					}
				};
				(AacRoute::Oxideav(oxideav_decoder), result)
			}
			None => {
				return decode_error("aac: 해석기가 없습니다");
			}
		};
		self.route = Some(next_route);
		if let Err(error) = result {
			self.buffer.clear();
			return Err(error);
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

impl RegisterableAudioDecoder for AacPacketDecoder {
	//==============================================================================
	/// 등록표가 부르는 생성. (create 를 상자에 담는다)
	//==============================================================================
	fn try_registry_new(params: &AudioCodecParameters, _opts: &AudioDecoderOptions) -> Result<Box<dyn AudioDecoder>>
	where
		Self: Sized,
	{
		Ok(Box::new(AacPacketDecoder::create(params)?))
	}

	//==============================================================================
	/// 받는 코덱. (AAC 하나)
	//==============================================================================
	fn supported_codecs() -> &'static [SupportedAudioCodec] {
		&AAC_SUPPORTED_CODEC_LIST
	}
}

static AAC_SUPPORTED_CODEC_LIST: [SupportedAudioCodec; 1] = [SupportedAudioCodec {
	id: CODEC_ID_AAC,
	info: CodecInfo {
		short_name: "aac",
		long_name: "AAC (oxideav-aac)",
		profiles: &[],
	},
}];

//==============================================================================
//! 스트리밍 풀기. (데스크탑, 원본 AudioLoadType.streaming 자리, 유니티 AudioClip Load Type: Streaming 꼴)
//! - 통째로 풀지 않고 패킷을 조금씩 풀어 조각마다 내놓는다. 통째로 풀기(decode_audio_data)와 같은 길이다:
//!   같은 패킷 읽개(PacketReader) → 같은 다듬기(TrimPlan, TrimState) → 같은 SincResampler 를 조각마다 이어 부르기 → 같은 스피커 규칙으로 내리기.
//!   그래서 내놓는 표본이 통째로 푼 버퍼와 비트까지 같다. (시험 streaming_matches_decode_audio_data)
//! - 열 때(read_stream_header) 파일을 한 번 디먹스만 해서(첫 소리 패킷만 푼다) 다듬기 계획과 길이 어림을 얻는다. 표본은 들지 않는다.
//!   길이 어림은 컨테이너가 매긴 패킷 길이로 셈한다. (원본 미디어 요소의 메타데이터 길이 자리) 끝까지 한 번 풀면 푼 길이를 안다.
//==============================================================================
use super::resample::{SincResampler, REQUEST_FRAMES};
use super::trim::{TrimPlan, TrimState};
use super::{down_mix_channels, PacketReader, RawDecode, DECODE_SAMPLE_RATE};
use std::collections::VecDeque;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

/// 한 번에 내놓는 조각의 프레임 수. (SincResampler 를 이만큼씩 부른다)
const STREAM_CHUNK_FRAMES: usize = 4096;

//==============================================================================
/// 스트리밍 소리의 자리. (파일 경로면 틀 때마다 파일에서 조금씩 읽고, 바이트면 그 압축된 바이트를 든다)
//==============================================================================
#[derive(Clone, Debug)]
pub(crate) enum StreamMedia {
	File(PathBuf),
	Bytes(Arc<[u8]>),
}

impl StreamMedia {
	//==============================================================================
	/// symphonia 가 읽을 자리 열기.
	//==============================================================================
	fn open_media_source(&self) -> Result<Box<dyn symphonia::core::io::MediaSource>, String> {
		match self {
			StreamMedia::File(path) => {
				let file = std::fs::File::open(path).map_err(|error| format!("{}", error))?;
				Ok(Box::new(file))
			}
			StreamMedia::Bytes(bytes) => {
				Ok(Box::new(Cursor::new(bytes.clone())))
			}
		}
	}

	//==============================================================================
	/// 드는 메모리. (바이트면 그 크기, 파일이면 0, 시험)
	//==============================================================================
	#[cfg(test)]
	pub(crate) fn get_memory_size(&self) -> usize {
		match self {
			StreamMedia::File(_) => {
				0
			}
			StreamMedia::Bytes(bytes) => {
				bytes.len()
			}
		}
	}
}

//==============================================================================
/// 스트리밍 머리. (다듬기 계획, 채널 수, 길이 어림)
//==============================================================================
#[derive(Clone, Debug)]
pub(crate) struct StreamHeader {
	trim_plan: TrimPlan,
	source_sample_rate: u32,
	channel_count: usize,
	/// 내놓는 채널 수. (스피커 규칙으로 내린 1 이나 2)
	pub(crate) output_channel_count: usize,
	/// 내놓을 프레임 수 어림. (DECODE_SAMPLE_RATE, 컨테이너의 패킷 길이로 셈한다)
	pub(crate) estimated_frame_count: usize,
}

//==============================================================================
/// 스트리밍 머리 읽기. (파일을 한 번 디먹스한다, 첫 소리 패킷만 풀어 표본율과 채널 수를 안다)
//==============================================================================
pub(crate) fn read_stream_header(stream_media: &StreamMedia) -> Result<StreamHeader, String> {
	// 다듬기 계획은 컨테이너 바이트(WebM 블록, MP4 상자, Ogg 쪽)를 본다. 파일이면 여기서만 읽고 놓는다.
	let bytes: Arc<[u8]> = match stream_media {
		StreamMedia::File(path) => {
			Arc::from(std::fs::read(path).map_err(|error| format!("{}", error))?)
		}
		StreamMedia::Bytes(bytes) => {
			bytes.clone()
		}
	};
	let mut packet_reader = PacketReader::open(Box::new(Cursor::new(bytes.clone())))?;
	let mut packet_list = Vec::new();
	let mut is_decoded = true;
	while let Some(mut raw_packet) = packet_reader.read_packet(is_decoded)? {
		if raw_packet.planes.first().map(|plane| !plane.is_empty()).unwrap_or(false) {
			is_decoded = false;
		}
		raw_packet.planes = Vec::new();
		packet_list.push(raw_packet);
	}
	let header = packet_reader.get_header();
	if is_decoded || header.sample_rate == 0 || header.channel_count == 0 {
		return Err(String::from("no audio frames"));
	}
	let raw = RawDecode {
		format_name: header.format_name.clone(),
		codec_name: header.codec_name.clone(),
		sample_rate: header.sample_rate,
		channel_count: header.channel_count,
		time_base: header.time_base,
		track_delay: header.track_delay,
		track_padding: header.track_padding,
		opus_pre_skip: header.opus_pre_skip,
		packet_list,
	};
	let trim_plan = TrimPlan::new(&raw, &bytes);
	let estimated_source_frame_count = estimate_source_frame_count(&raw, &trim_plan);
	let sample_rate_ratio = raw.sample_rate as f64 / DECODE_SAMPLE_RATE as f64;
	let estimated_frame_count = if raw.sample_rate == DECODE_SAMPLE_RATE { estimated_source_frame_count } else { (estimated_source_frame_count as f64 / sample_rate_ratio) as usize };
	let output_channel_count = match raw.channel_count {
		1 => {
			1
		}
		_ => {
			2
		}
	};
	Ok(StreamHeader {
		trim_plan,
		source_sample_rate: raw.sample_rate,
		channel_count: raw.channel_count,
		output_channel_count,
		estimated_frame_count,
	})
}

//==============================================================================
/// 다듬은 프레임 수 어림. (패킷 길이의 합과 시각의 폭 가운데 큰 것에서 앞뒤 버리는 것을 뺀다, 표본율은 원래 것)
//==============================================================================
fn estimate_source_frame_count(raw: &RawDecode, trim_plan: &TrimPlan) -> usize {
	// Ogg 의 패킷 길이는 끝에서 버릴 것(trim_end)을 이미 뺐다. (뒤에서 tail_discard 로 다시 뺀다)
	let is_ogg = raw.format_name == "ogg";
	let duration_sum: u64 = raw.packet_list.iter().map(|packet| packet.duration + if is_ogg { packet.trim_end } else { 0 }).sum();
	let first_timestamp = raw.packet_list.iter().map(|packet| packet.timestamp).min().unwrap_or(0);
	let mut end_timestamp = raw.packet_list.iter().map(|packet| packet.timestamp + packet.duration).max().unwrap_or(0);
	// 패킷 길이가 없는 꼴(WebM 블록)은 마지막 패킷의 길이를 앞 간격으로 어림한다.
	if duration_sum == 0 && raw.packet_list.len() >= 2 {
		let last_index = raw.packet_list.len() - 1;
		end_timestamp += raw.packet_list[last_index].timestamp.saturating_sub(raw.packet_list[last_index - 1].timestamp);
	}
	let unit_count = duration_sum.max(end_timestamp.saturating_sub(first_timestamp)) as f64;
	let frame_count = match raw.time_base {
		Some((numerator, denominator)) if denominator > 0 => {
			unit_count * numerator as f64 / denominator as f64 * raw.sample_rate as f64
		}
		_ => {
			unit_count
		}
	};
	let trimmed_frame_count = (frame_count.round() as usize).saturating_sub(trim_plan.get_front_discard() + trim_plan.get_tail_discard());
	match trim_plan.get_total_limit() {
		Some(total_limit) => {
			trimmed_frame_count.min(total_limit)
		}
		None => {
			trimmed_frame_count
		}
	}
}

//==============================================================================
/// 스트리밍 풀개. (처음부터 조각마다 풀어 내놓는다, 되감기와 반복은 새로 연다)
//==============================================================================
pub(crate) struct StreamDecoder {
	packet_reader: PacketReader,
	trim_state: TrimState,
	packet_index: usize,
	/// 채널마다 SincResampler. (표본율이 같으면 비어 있다)
	resampler_list: Vec<SincResampler>,
	sample_rate_ratio: f64,
	/// 다듬었지만 아직 바꾸지 않은 원래 표본율의 프레임. (채널마다)
	pending_channel_list: Vec<VecDeque<f32>>,
	trimmed_channel_list: Vec<Vec<f32>>,
	is_source_ended: bool,
	trimmed_frame_count: usize,
	/// 내놓을 프레임 수. (원본을 끝까지 다듬어야 안다, Blink 처럼 (int)(다듬은 길이 / 비))
	destination_frame_count: Option<usize>,
	produced_frame_count: usize,
}

impl StreamDecoder {
	//==============================================================================
	/// 처음부터 열기.
	//==============================================================================
	pub(crate) fn open(stream_media: &StreamMedia, stream_header: &StreamHeader) -> Result<StreamDecoder, String> {
		let packet_reader = PacketReader::open(stream_media.open_media_source()?)?;
		let channel_count = stream_header.channel_count;
		let sample_rate_ratio = stream_header.source_sample_rate as f64 / DECODE_SAMPLE_RATE as f64;
		let resampler_list = if stream_header.source_sample_rate == DECODE_SAMPLE_RATE { Vec::new() } else { (0..channel_count).map(|_| SincResampler::new(sample_rate_ratio)).collect() };
		Ok(StreamDecoder {
			packet_reader,
			trim_state: TrimState::new(stream_header.trim_plan.clone(), channel_count),
			packet_index: 0,
			resampler_list,
			sample_rate_ratio,
			pending_channel_list: vec![VecDeque::new(); channel_count],
			trimmed_channel_list: vec![Vec::new(); channel_count],
			is_source_ended: false,
			trimmed_frame_count: 0,
			destination_frame_count: None,
			produced_frame_count: 0,
		})
	}

	//==============================================================================
	/// 내놓은 프레임 수. (처음부터)
	//==============================================================================
	pub(crate) fn get_produced_frame_count(&self) -> usize {
		self.produced_frame_count
	}

	//==============================================================================
	/// 끝까지 내놓았는지.
	//==============================================================================
	pub(crate) fn is_ended(&self) -> bool {
		matches!(self.destination_frame_count, Some(destination_frame_count) if self.produced_frame_count >= destination_frame_count)
	}

	//==============================================================================
	/// 패킷 하나 더 풀어 다듬기. (끝이면 is_source_ended)
	//==============================================================================
	fn decode_next_packet(&mut self) -> Result<(), String> {
		match self.packet_reader.read_packet(true)? {
			Some(raw_packet) => {
				for trimmed_channel in self.trimmed_channel_list.iter_mut() {
					trimmed_channel.clear();
				}
				self.trim_state.push_packet(self.packet_index, &raw_packet.planes, &mut self.trimmed_channel_list);
				self.packet_index += 1;
				self.trimmed_frame_count += self.trimmed_channel_list.first().map(|channel| channel.len()).unwrap_or(0);
				for (pending_channel, trimmed_channel) in self.pending_channel_list.iter_mut().zip(self.trimmed_channel_list.iter()) {
					pending_channel.extend(trimmed_channel.iter());
				}
			}
			None => {
				self.is_source_ended = true;
				let destination_frame_count = if self.resampler_list.is_empty() { self.trimmed_frame_count } else { (self.trimmed_frame_count as f64 / self.sample_rate_ratio) as usize };
				self.destination_frame_count = Some(destination_frame_count);
			}
		}
		Ok(())
	}

	//==============================================================================
	/// 다음 프레임들 내놓기. (내린 채널마다 output 에 덧붙인다, 끝이면 0)
	//==============================================================================
	pub(crate) fn read(&mut self, output: &mut [Vec<f32>], frame_count: usize) -> Result<usize, String> {
		let mut written_frame_count = 0;
		while written_frame_count < frame_count {
			let chunk_frame_count = (frame_count - written_frame_count).min(STREAM_CHUNK_FRAMES);
			// 바꾸기가 이 조각을 내는 데 읽을 원래 프레임보다 넉넉히 풀어 둔다. (SincResampler 는 64 프레임씩 앞서 읽는다)
			let needed_frame_count = if self.resampler_list.is_empty() { chunk_frame_count } else { (chunk_frame_count as f64 * self.sample_rate_ratio).ceil() as usize + 4 * REQUEST_FRAMES };
			while !self.is_source_ended && self.pending_channel_list.first().map(|channel| channel.len()).unwrap_or(0) < needed_frame_count {
				self.decode_next_packet()?;
			}
			let mut count = chunk_frame_count;
			if let Some(destination_frame_count) = self.destination_frame_count {
				count = count.min(destination_frame_count.saturating_sub(self.produced_frame_count));
			}
			if self.resampler_list.is_empty() {
				count = count.min(self.pending_channel_list.first().map(|channel| channel.len()).unwrap_or(0));
			}
			if count == 0 {
				break;
			}
			let channel_chunk_list: Vec<Vec<f32>> = if self.resampler_list.is_empty() {
				self.pending_channel_list.iter_mut().map(|pending_channel| pending_channel.drain(..count).collect()).collect()
			}
			else {
				let mut channel_chunk_list = Vec::with_capacity(self.resampler_list.len());
				for (resampler, pending_channel) in self.resampler_list.iter_mut().zip(self.pending_channel_list.iter_mut()) {
					let mut destination = vec![0.0f32; count];
					// 원본이 끝났으면 0 으로 채운다. (resample_channel 과 같다)
					resampler.resample(&mut destination, &mut |chunk: &mut [f32]| {
						for sample in chunk.iter_mut() {
							*sample = pending_channel.pop_front().unwrap_or(0.0);
						}
					});
					channel_chunk_list.push(destination);
				}
				channel_chunk_list
			};
			for (output_channel, mixed_channel) in output.iter_mut().zip(down_mix_channels(channel_chunk_list).iter()) {
				output_channel.extend_from_slice(mixed_channel);
			}
			self.produced_frame_count += count;
			written_frame_count += count;
		}
		Ok(written_frame_count)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const AUDIO_ASSET_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/audio");

	//==============================================================================
	/// 시험 소리 이름들. (크롬과 견주는 27 개)
	//==============================================================================
	fn read_asset_name_list() -> Vec<String> {
		let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/chrome_decode.json", AUDIO_ASSET_DIRECTORY)).expect("chrome_decode.json")).expect("json");
		json["files"].as_object().expect("files").keys().cloned().collect()
	}

	//==============================================================================
	/// 조각마다 푼 것이 통째로 푼 것과 비트까지 같다. (27 개 꼴, 조각 크기를 바꿔도, 파일과 바이트 둘 다)
	/// - 길이 어림은 컨테이너 패킷 길이로 셈해 다를 수 있어 따로 적는다. (끝까지 풀면 푼 길이를 쓴다)
	//==============================================================================
	#[test]
	fn streaming_matches_decode_audio_data() {
		let name_list = read_asset_name_list();
		assert_eq!(name_list.len(), 27);
		for name in name_list {
			let path = PathBuf::from(format!("{}/{}", AUDIO_ASSET_DIRECTORY, name));
			let bytes = std::fs::read(&path).unwrap();
			let decoded = super::super::decode_audio_data(&bytes).unwrap();
			for (stream_media, read_frame_count) in [(StreamMedia::File(path.clone()), 128usize), (StreamMedia::Bytes(Arc::from(bytes.clone())), 1000usize)] {
				let stream_header = read_stream_header(&stream_media).unwrap();
				assert_eq!(stream_header.output_channel_count, decoded.channel_data.len(), "{} 채널", name);
				let mut stream_decoder = StreamDecoder::open(&stream_media, &stream_header).unwrap();
				let mut output: Vec<Vec<f32>> = vec![Vec::new(); stream_header.output_channel_count];
				while stream_decoder.read(&mut output, read_frame_count).unwrap() > 0 {}
				assert!(stream_decoder.is_ended());
				assert_eq!(output[0].len(), decoded.channel_data[0].len(), "{} 길이", name);
				for (stream_channel, decoded_channel) in output.iter().zip(decoded.channel_data.iter()) {
					assert!(stream_channel.iter().zip(decoded_channel.iter()).all(|(left, right)| left.to_bits() == right.to_bits() || (*left == 0.0 && *right == 0.0)), "{} 표본", name);
				}
				let frame_difference = stream_header.estimated_frame_count as i64 - output[0].len() as i64;
				eprintln!("길이 어림 {}: {} / {} ({})", name, stream_header.estimated_frame_count, output[0].len(), frame_difference);
			}
		}
	}
}

//==============================================================================
//! 앞뒤 다듬기. (크롬 decodeAudioData 가 버리는 프레임과 같게)
//! - 크롬은 FFmpeg 로 디먹스해 패킷의 시각과 AV_PKT_DATA_SKIP_SAMPLES 를 받고, AudioDiscardHelper 가 버리며,
//!   AudioFileReader 가 0 보다 앞선 시각의 프레임과 AAC 의 끝을 자른다. (media/filters/audio_file_reader.cc, media/base/audio_discard_helper.cc)
//!   symphonia 의 풀기는 시각과 다듬기 값을 FFmpeg 와 조금 다르게 매기므로, 꼴마다 FFmpeg 가 매길 값을 셈해 같은 프레임을 남긴다.
//! - WAV(PCM), FLAC, ADTS(AAC): 버리지 않는다.
//! - MP3: LAME 머리의 지연 + 529 를 앞에서, 채움 - 529 를 뒤에서. (FFmpeg mp3dec start_skip_samples, first_discard_sample. symphonia 의 값과 같다)
//! - Ogg Opus: OpusHead 의 pre-skip 을 앞에서, 마지막 쪽 granule 로 남는 것을 뒤에서. (oggparseopus)
//! - Ogg Vorbis: 앞은 버리지 않고, 마지막 쪽 패킷 길이의 합 - (마지막 granule - 앞 쪽 granule)을 뒤에서. (oggparsevorbis)
//!   FFmpeg 는 첫 패킷 길이를 블록 크기의 반으로 세므로, 첫 패킷이 마지막 쪽에 있으면(쪽 하나짜리 짧은 소리) 그만큼 더 버린다.
//! - WebM(Matroska) Opus: pre-skip 을 앞에서 버린 뒤, 첫 블록 시각(트랙 시간 단위로 반올림한 CodecDelay 를 뺀 것)이 0 보다 앞서면 그만큼 더,
//!   블록의 DiscardPadding 을 그 패킷 뒤에서. (matroskadec, OpusAudioDecoder 의 discard_helper_, FinalizeDecodedBuffer)
//! - MP4(AAC): 편집 목록(elst)의 media_time 을 앞에서, 마지막 패킷 길이나 컨테이너 길이로 끝을. (mov.c, MaybeTrimAacFinalBuffer)
//==============================================================================
// 옮긴 원문: 크로미엄 media/filters/audio_file_reader.cc, media/base/audio_discard_helper.cc 의 버리기 셈을 옮김, BSD-3-Clause, THIRD_PARTY_LICENSES.md
use super::RawDecode;

//==============================================================================
/// 다듬기 계획. (꼴마다 버릴 프레임, 푼 표본 없이 패킷의 길이와 다듬기 값, 파일 바이트로 정한다)
/// - 통째로 풀기(trim_like_chrome)와 스트리밍(stream.rs)이 같은 계획과 같은 TrimState 로 다듬어 같은 프레임을 남긴다.
/// - 끝에서 버리는 것은 둘로 나눈다. 이어 붙인 소리의 마지막 tail_discard 프레임(MP3 채움, Ogg 의 granule),
///   패킷마다 그 패킷 뒤에서 버리는 것(WebM 의 DiscardPadding, MP4 AAC 의 마지막 패킷). 앞에서 버리는 것은 이어 붙인 소리의 처음 front_discard 프레임이다.
//==============================================================================
#[derive(Clone, Debug)]
pub(crate) struct TrimPlan {
	front_discard: usize,
	tail_discard: usize,
	/// (패킷 차례, 그 패킷 뒤에서 버릴 프레임) (WebM DiscardPadding, 차례대로)
	packet_end_discard_list: Vec<(usize, usize)>,
	/// MP4 AAC 의 마지막 패킷. (차례, 컨테이너가 매긴 그 패킷 길이(마이크로초), 방법 2 의 컨테이너 길이(프레임))
	mp4_aac_last_packet: Option<(usize, i64, Option<usize>)>,
	/// 다듬은 소리의 길이 한계. (MP4 iTunSMPB 의 remainder)
	total_limit: Option<usize>,
	sample_rate: f64,
}

impl TrimPlan {
	//==============================================================================
	/// 계획 세우기. (raw 의 패킷은 길이와 다듬기 값만 본다, 표본은 보지 않는다)
	//==============================================================================
	pub(crate) fn new(raw: &RawDecode, bytes: &[u8]) -> TrimPlan {
		let packet_count = raw.packet_list.len();
		let sample_rate = raw.sample_rate as f64;
		let mut trim_plan = TrimPlan {
			front_discard: 0,
			tail_discard: 0,
			packet_end_discard_list: Vec::new(),
			mp4_aac_last_packet: None,
			total_limit: None,
			sample_rate,
		};
		match (raw.format_name.as_str(), raw.codec_name.as_str()) {
			("mp3", _) => {
				trim_plan.front_discard = raw.track_delay.unwrap_or(0) as usize;
				trim_plan.tail_discard = raw.track_padding.unwrap_or(0) as usize;
			}
			("ogg", "opus") => {
				trim_plan.front_discard = raw.opus_pre_skip as usize;
				trim_plan.tail_discard = raw.packet_list.iter().map(|packet| packet.trim_end).sum::<u64>() as usize;
			}
			("ogg", "vorbis") => {
				let mut end_discard: u64 = raw.packet_list.iter().map(|packet| packet.trim_end).sum();
				if is_first_ogg_data_page_last(bytes) {
					end_discard += raw.packet_list.first().map(|packet| packet.trim_start).unwrap_or(0);
				}
				trim_plan.tail_discard = end_discard as usize;
			}
			("matroska", codec_name) => {
				let matroska_timing = read_matroska_timing(bytes);
				let pre_skip = if codec_name == "opus" { raw.opus_pre_skip as usize } else { 0 };
				// 첫 블록 시각. (마이크로초, FFmpeg 는 트랙 시간 단위로 반올림한 CodecDelay 를 뺀다)
				let time_base_nanoseconds = matroska_timing.timecode_scale as f64;
				let codec_delay_in_track_time_base = round_half_away_from_zero(matroska_timing.codec_delay as f64 / time_base_nanoseconds);
				let first_timestamp_microseconds = (matroska_timing.first_block_timecode.saturating_sub(codec_delay_in_track_time_base) as f64 * time_base_nanoseconds / 1000.0).round() as i64;
				// pre-skip 을 버린 뒤의 시각. (AudioTimestampHelper::ComputeTimestamp)
				let microseconds_per_frame = 1000000.0 / sample_rate;
				let timestamp_after_pre_skip = first_timestamp_microseconds.saturating_add((microseconds_per_frame * pre_skip as f64) as i64);
				let negative_discard = if timestamp_after_pre_skip < 0 { (-(timestamp_after_pre_skip as f64) / 1000000.0 * sample_rate).round() as usize } else { 0 };
				// 블록의 DiscardPadding. (나노초 → 표본(반올림) → 마이크로초(버림) → 프레임(+0.5 버림))
				for (block_index, discard_padding_nanoseconds) in &matroska_timing.discard_padding_list {
					if *block_index < packet_count && *discard_padding_nanoseconds > 0 {
						let discard_samples = round_half_away_from_zero(*discard_padding_nanoseconds as f64 * sample_rate / 1000000000.0);
						let discard_microseconds = (discard_samples as f64 * 1000000.0 / sample_rate) as i64;
						trim_plan.packet_end_discard_list.push((*block_index, (discard_microseconds as f64 / 1000000.0 * sample_rate + 0.5) as usize));
					}
				}
				trim_plan.packet_end_discard_list.sort_by_key(|(block_index, _)| *block_index);
				trim_plan.packet_end_discard_list.dedup_by_key(|(block_index, _)| *block_index);
				trim_plan.front_discard = pre_skip.saturating_add(negative_discard);
			}
			("isomp4", _) => {
				let mp4_timing = read_mp4_timing(bytes);
				let to_samples = |media_units: i64| -> usize {
					if mp4_timing.media_timescale > 0 { (media_units as f64 * sample_rate / mp4_timing.media_timescale as f64).round().max(0.0) as usize } else { 0 }
				};
				// 편집 목록이 있으면 그 media_time, 없으면 iTunSMPB 의 priming 을 앞에서. remainder 는 (길이 - remainder) 부터 버린다. (mov.c)
				let front_discard = if mp4_timing.has_edit_list {
					to_samples(mp4_timing.edit_media_time)
				}
				else if mp4_timing.itunes_priming > 0 && mp4_timing.itunes_priming < 16384 {
					to_samples(mp4_timing.itunes_priming)
				}
				else {
					0
				};
				if !mp4_timing.has_edit_list && mp4_timing.itunes_remainder > 0 {
					let duration = to_samples(mp4_timing.media_duration as i64);
					let remainder = to_samples(mp4_timing.itunes_remainder);
					if duration > remainder {
						trim_plan.total_limit = Some((duration - remainder).saturating_sub(front_discard));
					}
				}
				if raw.codec_name == "aac" && packet_count > 0 {
					// 방법 1 은 마지막 패킷을 푼 길이를 보고 정한다. (TrimState) 방법 2 의 컨테이너 길이는 여기서 셈해 둔다. (마이크로초로 반올림한 길이의 프레임 수)
					let last_index = packet_count - 1;
					let last_duration_microseconds = match (raw.packet_list.get(last_index), raw.time_base) {
						(Some(packet), Some((numerator, denominator))) => {
							(packet.duration as f64 * numerator as f64 * 1000000.0 / denominator as f64).round() as i64
						}
						_ => {
							0
						}
					};
					let container_limit = if mp4_timing.duration_seconds > 0.0 {
						let duration_microseconds = (mp4_timing.duration_seconds * 1000000.0).round();
						Some((duration_microseconds / 1000000.0 * sample_rate).round() as usize)
					}
					else {
						None
					};
					trim_plan.mp4_aac_last_packet = Some((last_index, last_duration_microseconds, container_limit));
				}
				trim_plan.front_discard = front_discard;
			}
			_ => {}
		}
		trim_plan
	}

	//==============================================================================
	/// 앞에서 버리는 프레임 수.
	//==============================================================================
	pub(crate) fn get_front_discard(&self) -> usize {
		self.front_discard
	}

	//==============================================================================
	/// 이어 붙인 소리의 끝에서 버리는 프레임 수.
	//==============================================================================
	pub(crate) fn get_tail_discard(&self) -> usize {
		self.tail_discard
	}

	//==============================================================================
	/// 다듬은 소리의 길이 한계. (MP4 iTunSMPB)
	//==============================================================================
	pub(crate) fn get_total_limit(&self) -> Option<usize> {
		self.total_limit
	}
}

//==============================================================================
/// 다듬기 진행. (패킷을 차례로 넣으면 남길 프레임을 내놓는다, 끝의 tail_discard 프레임은 쥐고 있다가 버린다)
//==============================================================================
pub(crate) struct TrimState {
	trim_plan: TrimPlan,
	remaining_front_discard: usize,
	/// 끝에서 버릴지 모르는 프레임. (채널마다, tail_discard 개까지 쥔다)
	held_channel_list: Vec<std::collections::VecDeque<f32>>,
	/// 내놓은 프레임 수와 그 한계.
	emitted_frame_count: usize,
	total_limit: Option<usize>,
	packet_end_discard_position: usize,
}

impl TrimState {
	//==============================================================================
	/// 생성. (channel_count 는 내놓을 채널 수)
	//==============================================================================
	pub(crate) fn new(trim_plan: TrimPlan, channel_count: usize) -> TrimState {
		let remaining_front_discard = trim_plan.front_discard;
		let total_limit = trim_plan.total_limit;
		TrimState {
			trim_plan,
			remaining_front_discard,
			held_channel_list: vec![std::collections::VecDeque::new(); channel_count],
			emitted_frame_count: 0,
			total_limit,
			packet_end_discard_position: 0,
		}
	}

	//==============================================================================
	/// 패킷 하나 넣기. (packet_index 차례의 푼 표본, 남길 프레임을 output 의 채널마다 덧붙인다)
	//==============================================================================
	pub(crate) fn push_packet(&mut self, packet_index: usize, planes: &[Vec<f32>], output: &mut [Vec<f32>]) {
		let frame_count = planes.first().map(|plane| plane.len()).unwrap_or(0);
		let mut end_discard = 0usize;
		while let Some((block_index, discard)) = self.trim_plan.packet_end_discard_list.get(self.packet_end_discard_position).copied() {
			if block_index > packet_index {
				break;
			}
			if block_index == packet_index {
				end_discard = discard;
			}
			self.packet_end_discard_position += 1;
		}
		if let Some((last_index, last_duration_microseconds, container_limit)) = self.trim_plan.mp4_aac_last_packet {
			if packet_index == last_index {
				// 방법 1: 마지막 패킷 길이가 디코드한 길이보다 짧으면 그 비로 줄인다. (TimeDelta 마이크로초끼리의 비) 아니면 방법 2: 컨테이너 길이.
				let frame_duration_microseconds = (frame_count as f64 * 1000000.0 / self.trim_plan.sample_rate) as i64;
				if last_duration_microseconds > 0 && last_duration_microseconds < frame_duration_microseconds {
					let kept = (frame_count as f64 * (last_duration_microseconds as f64 / frame_duration_microseconds as f64)).floor() as usize;
					end_discard = frame_count - kept;
				}
				else if let Some(container_limit) = container_limit {
					self.total_limit = Some(self.total_limit.map(|limit| limit.min(container_limit)).unwrap_or(container_limit));
				}
			}
		}
		let end_discard = end_discard.min(frame_count);
		let front = self.remaining_front_discard.min(frame_count - end_discard);
		self.remaining_front_discard -= self.remaining_front_discard.min(frame_count);
		let kept_count = frame_count - end_discard - front;
		if kept_count == 0 {
			return;
		}
		let tail_discard = self.trim_plan.tail_discard;
		for (channel_index, held_channel) in self.held_channel_list.iter_mut().enumerate() {
			match planes.get(channel_index) {
				Some(plane) => {
					held_channel.extend(&plane[front..frame_count - end_discard]);
				}
				None => {
					held_channel.extend(std::iter::repeat_n(0.0, kept_count));
				}
			}
		}
		// 쥔 것이 tail_discard 를 넘으면 앞의 것을 내놓는다. (한계를 넘는 것은 버린다)
		let held_count = self.held_channel_list.first().map(|held_channel| held_channel.len()).unwrap_or(0);
		if held_count <= tail_discard {
			return;
		}
		let release_count = held_count - tail_discard;
		let allowed_count = match self.total_limit {
			Some(total_limit) => {
				release_count.min(total_limit.saturating_sub(self.emitted_frame_count))
			}
			None => {
				release_count
			}
		};
		for (held_channel, output_channel) in self.held_channel_list.iter_mut().zip(output.iter_mut()) {
			output_channel.extend(held_channel.drain(..release_count).take(allowed_count));
		}
		self.emitted_frame_count += allowed_count;
	}
}

//==============================================================================
/// 크롬처럼 다듬어 채널마다 이어 붙이기. (TrimPlan 과 TrimState 로, 스트리밍과 같은 길)
//==============================================================================
pub(crate) fn trim_like_chrome(raw: &RawDecode, bytes: &[u8]) -> Vec<Vec<f32>> {
	let mut trim_state = TrimState::new(TrimPlan::new(raw, bytes), raw.channel_count);
	let mut channel_data: Vec<Vec<f32>> = vec![Vec::new(); raw.channel_count];
	for (packet_index, packet) in raw.packet_list.iter().enumerate() {
		trim_state.push_packet(packet_index, &packet.planes, &mut channel_data);
	}
	channel_data
}

//==============================================================================
/// 0.5 는 0 에서 먼 쪽으로 반올림. (FFmpeg av_rescale_q 의 AV_ROUND_NEAR_INF)
//==============================================================================
fn round_half_away_from_zero(value: f64) -> i64 {
	value.round() as i64
}

//==============================================================================
/// Ogg: 첫 소리 패킷이 들어 있는 쪽이 마지막 쪽인지. (머리 패킷 셋 뒤의 첫 패킷, 쪽 머리의 EOS 깃발)
//==============================================================================
fn is_first_ogg_data_page_last(bytes: &[u8]) -> bool {
	let mut position = 0;
	let mut packet_count = 0;
	let mut is_in_packet = false;
	while position + 27 <= bytes.len() && &bytes[position..position + 4] == b"OggS" {
		let header_type = bytes[position + 5];
		let segment_count = bytes[position + 26] as usize;
		if position + 27 + segment_count > bytes.len() {
			return false;
		}
		let segment_table = &bytes[position + 27..position + 27 + segment_count];
		let mut body_size = 0;
		for segment in segment_table {
			body_size += *segment as usize;
			if !is_in_packet && packet_count == 3 {
				return header_type & 0x04 != 0;
			}
			is_in_packet = true;
			if *segment < 255 {
				packet_count += 1;
				is_in_packet = false;
			}
		}
		position += 27 + segment_count + body_size;
	}
	false
}

//==============================================================================
/// Matroska 시간 정보. (TimecodeScale, 소리 트랙의 CodecDelay, 첫 블록 시각, 블록 차례별 DiscardPadding)
//==============================================================================
struct MatroskaTiming {
	timecode_scale: u64,
	codec_delay: u64,
	first_block_timecode: i64,
	discard_padding_list: Vec<(usize, i64)>,
}

//==============================================================================
/// EBML 가변 정수 읽기. (id 는 표시 비트를 남기고, 크기는 지운다. 모르는 크기(모두 1)는 None)
//==============================================================================
fn read_ebml_variable_integer(bytes: &[u8], position: usize, is_id: bool) -> Option<(u64, usize, bool)> {
	let first = *bytes.get(position)?;
	let length = first.leading_zeros() as usize + 1;
	if length > 8 || position + length > bytes.len() {
		return None;
	}
	let mut value = if is_id { first as u64 } else { (first as u64) & ((1u64 << (8 - length)) - 1) };
	for index in 1..length {
		value = (value << 8) | bytes[position + index] as u64;
	}
	let is_unknown_size = !is_id && value == (1u64 << (7 * length)) - 1;
	Some((value, length, is_unknown_size))
}

//==============================================================================
/// 큰 끝 부호 없는 정수 읽기. (바이트 수만큼, 8 바이트를 넘으면 위쪽은 버려진다)
//==============================================================================
fn read_unsigned(bytes: &[u8]) -> u64 {
	bytes.iter().fold(0u64, |value, byte| (value << 8) | *byte as u64)
}

//==============================================================================
/// 큰 끝 부호 있는 정수 읽기. (맨 위 비트로 부호를 넓힌다, 비면 0)
//==============================================================================
fn read_signed(bytes: &[u8]) -> i64 {
	if bytes.is_empty() || bytes.len() > 8 {
		return 0;
	}
	let unsigned = read_unsigned(bytes);
	let shift = 64 - 8 * bytes.len() as u32;
	((unsigned << shift) as i64) >> shift
}

//==============================================================================
/// Matroska(webm) 시간 정보 읽기. (TimecodeScale, 첫 소리 트랙의 CodecDelay, 첫 블록 시각, DiscardPadding)
//==============================================================================
fn read_matroska_timing(bytes: &[u8]) -> MatroskaTiming {
	let mut matroska_timing = MatroskaTiming { timecode_scale: 1000000, codec_delay: 0, first_block_timecode: 0, discard_padding_list: Vec::new() };
	let mut audio_track_number: Option<u64> = None;
	let mut block_index = 0usize;
	let mut cluster_timecode: i64 = 0;
	let mut is_first_block_found = false;
	/// 마스터 요소(EBML 머리, Segment, Info, Tracks, Cluster)는 몸통으로 들어가고, 나머지는 몸통을 넘긴다.
	/// 여기서 보는 요소의 번호는 수준마다 겹치지 않아 평평하게 걸어도 되고, 크기를 모르는 Segment, Cluster 도 그대로 걷는다.
	const MASTER_ID_LIST: [u64; 5] = [0x1A45DFA3, 0x18538067, 0x1549A966, 0x1654AE6B, 0x1F43B675];
	let mut position = 0;
	while let Some((element_id, body_start, body_end)) = read_ebml_element(bytes, position) {
		if MASTER_ID_LIST.contains(&element_id) {
			position = body_start;
			continue;
		}
		let body = &bytes[body_start..body_end];
		match element_id {
			0x2AD7B1 => {
				matroska_timing.timecode_scale = read_unsigned(body).max(1);
			}
			0xAE => {
				// TrackEntry: 소리 트랙(TrackType 2)의 번호와 CodecDelay.
				let mut track_number = 0;
				let mut track_type = 0;
				let mut codec_delay = 0;
				let mut child_position = 0;
				while let Some((child_id, child_start, child_end)) = read_ebml_element(body, child_position) {
					let child_body = &body[child_start..child_end];
					match child_id {
						0xD7 => {
							track_number = read_unsigned(child_body);
						}
						0x83 => {
							track_type = read_unsigned(child_body);
						}
						0x56AA => {
							codec_delay = read_unsigned(child_body);
						}
						_ => {}
					}
					child_position = child_end;
				}
				if track_type == 2 && audio_track_number.is_none() {
					audio_track_number = Some(track_number);
					matroska_timing.codec_delay = codec_delay;
				}
			}
			0xE7 => {
				cluster_timecode = read_unsigned(body) as i64;
			}
			0xA3 | 0xA0 => {
				// SimpleBlock, BlockGroup(Block 과 DiscardPadding).
				let mut block: &[u8] = body;
				let mut discard_padding = 0i64;
				if element_id == 0xA0 {
					block = &[];
					let mut child_position = 0;
					while let Some((child_id, child_start, child_end)) = read_ebml_element(body, child_position) {
						if child_id == 0xA1 {
							block = &body[child_start..child_end];
						}
						else if child_id == 0x75A2 {
							discard_padding = read_signed(&body[child_start..child_end]);
						}
						child_position = child_end;
					}
				}
				if let Some((track_number, track_length, _)) = read_ebml_variable_integer(block, 0, false) {
					if Some(track_number) == audio_track_number && block.len() >= track_length + 3 {
						let relative_timecode = i16::from_be_bytes([block[track_length], block[track_length + 1]]) as i64;
						if !is_first_block_found {
							matroska_timing.first_block_timecode = cluster_timecode.saturating_add(relative_timecode);
							is_first_block_found = true;
						}
						// 레이싱이 있으면 프레임 수만큼 패킷이 나온다. (DiscardPadding 은 블록의 마지막 프레임)
						let lacing = (block[track_length + 2] >> 1) & 3;
						let lace_count = if lacing != 0 { block.get(track_length + 3).map(|count| *count as usize + 1).unwrap_or(1) } else { 1 };
						if discard_padding != 0 {
							matroska_timing.discard_padding_list.push((block_index + lace_count - 1, discard_padding));
						}
						block_index += lace_count;
					}
				}
			}
			_ => {}
		}
		position = body_end;
	}
	matroska_timing
}

//==============================================================================
/// EBML 요소 하나 읽기. (번호, 몸통 시작, 몸통 끝. 크기를 모르면 끝까지)
//==============================================================================
fn read_ebml_element(bytes: &[u8], position: usize) -> Option<(u64, usize, usize)> {
	let (element_id, id_length, _) = read_ebml_variable_integer(bytes, position, true)?;
	let (size, size_length, is_unknown_size) = read_ebml_variable_integer(bytes, position + id_length, false)?;
	let body_start = position + id_length + size_length;
	let body_end = if is_unknown_size { bytes.len() } else { (body_start as u64).saturating_add(size).min(bytes.len() as u64) as usize };
	Some((element_id, body_start, body_end))
}

//==============================================================================
/// MP4 시간 정보. (소리 트랙의 편집 목록 media_time 과 mdhd 시간 단위, 컨테이너 길이(초))
//==============================================================================
struct Mp4Timing {
	edit_media_time: i64,
	has_edit_list: bool,
	media_timescale: u32,
	media_duration: u64,
	duration_seconds: f64,
	/// iTunSMPB 의 priming, remainder. (편집 목록이 없을 때 FFmpeg 가 쓴다)
	itunes_priming: i64,
	itunes_remainder: i64,
}

//==============================================================================
/// MP4 시간 정보 읽기. (moov 의 첫 소리 트랙의 mdhd 와 편집 목록, iTunSMPB)
//==============================================================================
fn read_mp4_timing(bytes: &[u8]) -> Mp4Timing {
	let mut mp4_timing = Mp4Timing { edit_media_time: 0, has_edit_list: false, media_timescale: 0, media_duration: 0, duration_seconds: 0.0, itunes_priming: 0, itunes_remainder: 0 };
	let Some(moov) = find_mp4_box(bytes, b"moov") else {
		return mp4_timing;
	};
	let movie_timescale = find_mp4_box(moov, b"mvhd").map(|mvhd| read_mp4_header_timescale_and_duration(mvhd).0).unwrap_or(0);
	let mut position = 0;
	while let Some((box_type, body, next)) = read_mp4_box(moov, position) {
		position = next;
		if &box_type != b"trak" {
			continue;
		}
		let Some(mdia) = find_mp4_box(body, b"mdia") else {
			continue;
		};
		let is_sound = find_mp4_box(mdia, b"hdlr").map(|hdlr| hdlr.len() >= 12 && &hdlr[8..12] == b"soun").unwrap_or(false);
		if !is_sound {
			continue;
		}
		let (media_timescale, media_duration) = find_mp4_box(mdia, b"mdhd").map(read_mp4_header_timescale_and_duration).unwrap_or((0, 0));
		mp4_timing.media_timescale = media_timescale;
		mp4_timing.media_duration = media_duration;
		if media_timescale > 0 {
			mp4_timing.duration_seconds = media_duration as f64 / media_timescale as f64;
		}
		if let Some(elst) = find_mp4_box(body, b"edts").and_then(|edts| find_mp4_box(edts, b"elst")) {
			// 첫 편집의 media_time 과 길이(무비 시간 단위). 판 1 은 64 비트.
			if elst.len() >= 8 {
				let version = elst[0];
				let entry_count = u32::from_be_bytes([elst[4], elst[5], elst[6], elst[7]]);
				let entry_size = if version == 1 { 20 } else { 12 };
				let mut segment_duration_total = 0u64;
				for entry_index in 0..entry_count as usize {
					let offset = 8 + entry_index * entry_size;
					if offset + entry_size > elst.len() {
						break;
					}
					let (segment_duration, media_time) = if version == 1 {
						(read_unsigned(&elst[offset..offset + 8]), read_signed(&elst[offset + 8..offset + 16]))
					}
					else {
						(read_unsigned(&elst[offset..offset + 4]), read_signed(&elst[offset + 4..offset + 8]))
					};
					if media_time >= 0 {
						mp4_timing.has_edit_list = true;
						if mp4_timing.edit_media_time == 0 {
							mp4_timing.edit_media_time = media_time;
						}
						segment_duration_total = segment_duration_total.saturating_add(segment_duration);
					}
				}
				if movie_timescale > 0 && segment_duration_total > 0 {
					mp4_timing.duration_seconds = segment_duration_total as f64 / movie_timescale as f64;
				}
			}
		}
		break;
	}
	// iTunSMPB. (" 00000000 priming remainder samples ..." 16 진수, FFmpeg ff_itunes_parse_smpb)
	if let Some(key_position) = moov.windows(8).position(|window| window == b"iTunSMPB") {
		if let Some(data_offset) = moov[key_position..].windows(4).position(|window| window == b"data") {
			let text_start = key_position + data_offset + 4 + 8;
			if text_start < moov.len() {
				let text_end = (text_start + 128).min(moov.len());
				let text: String = moov[text_start..text_end].iter().take_while(|byte| **byte != 0).map(|byte| *byte as char).collect();
				let field_list: Vec<i64> = text.split_whitespace().filter_map(|field| i64::from_str_radix(field, 16).ok()).collect();
				if field_list.len() >= 4 {
					mp4_timing.itunes_priming = field_list[1];
					mp4_timing.itunes_remainder = field_list[2];
				}
			}
		}
	}
	mp4_timing
}

//==============================================================================
/// MP4 상자 하나 읽기. (종류, 몸통, 다음 자리)
//==============================================================================
fn read_mp4_box(bytes: &[u8], position: usize) -> Option<([u8; 4], &[u8], usize)> {
	if position + 8 > bytes.len() {
		return None;
	}
	let size = u32::from_be_bytes([bytes[position], bytes[position + 1], bytes[position + 2], bytes[position + 3]]) as u64;
	let box_type = [bytes[position + 4], bytes[position + 5], bytes[position + 6], bytes[position + 7]];
	let (header_size, box_size) = match size {
		0 => {
			(8u64, (bytes.len() - position) as u64)
		}
		1 => {
			if position + 16 > bytes.len() {
				return None;
			}
			(16, read_unsigned(&bytes[position + 8..position + 16]))
		}
		_ => {
			(8, size)
		}
	};
	if box_size < header_size {
		return None;
	}
	let end = (position as u64).saturating_add(box_size).min(bytes.len() as u64) as usize;
	Some((box_type, &bytes[position + header_size as usize..end], end))
}

//==============================================================================
/// 그 종류의 첫 MP4 상자 몸통 찾기. (한 수준만 걷는다, 없으면 None)
//==============================================================================
fn find_mp4_box<'a>(bytes: &'a [u8], wanted_type: &[u8; 4]) -> Option<&'a [u8]> {
	let mut position = 0;
	while let Some((box_type, body, next)) = read_mp4_box(bytes, position) {
		if &box_type == wanted_type {
			return Some(body);
		}
		position = next;
	}
	None
}

//==============================================================================
/// mvhd, mdhd 의 시간 단위와 길이. (판 1 은 64 비트 시각)
//==============================================================================
fn read_mp4_header_timescale_and_duration(body: &[u8]) -> (u32, u64) {
	if body.is_empty() {
		return (0, 0);
	}
	if body[0] == 1 {
		if body.len() < 32 {
			return (0, 0);
		}
		(u32::from_be_bytes([body[20], body[21], body[22], body[23]]), read_unsigned(&body[24..32]))
	}
	else {
		if body.len() < 20 {
			return (0, 0);
		}
		(u32::from_be_bytes([body[12], body[13], body[14], body[15]]), read_unsigned(&body[16..20]))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::platform::audio::decode::RawPacket;

	//==============================================================================
	/// EBML 요소 하나 짓기. (번호 바이트, 몸통, 크기는 1 바이트 가변 정수)
	//==============================================================================
	fn create_ebml_element(id_bytes: &[u8], body: &[u8]) -> Vec<u8> {
		let mut element = id_bytes.to_vec();
		element.push(0x80 | body.len() as u8);
		element.extend_from_slice(body);
		element
	}

	//==============================================================================
	/// MP4 상자 하나 짓기.
	//==============================================================================
	fn create_mp4_box(box_type: &[u8; 4], body: &[u8]) -> Vec<u8> {
		let mut mp4_box = ((body.len() + 8) as u32).to_be_bytes().to_vec();
		mp4_box.extend_from_slice(box_type);
		mp4_box.extend_from_slice(body);
		mp4_box
	}

	//==============================================================================
	/// 소리 패킷 하나로 된 풀기 결과. (모노 48000)
	//==============================================================================
	fn create_raw_decode(format_name: &str, codec_name: &str) -> RawDecode {
		RawDecode {
			format_name: String::from(format_name),
			codec_name: String::from(codec_name),
			sample_rate: 48000,
			channel_count: 1,
			time_base: None,
			track_delay: None,
			track_padding: None,
			opus_pre_skip: 312,
			packet_list: vec![RawPacket { timestamp: 0, duration: 960, trim_start: 0, trim_end: 0, planes: vec![vec![0.0; 960]] }],
		}
	}

	//==============================================================================
	/// 부호 있는 EBML 정수: 8 바이트를 넘으면 틀린 값이라 0 이다. (FFmpeg ebml_read_sint 처럼)
	//==============================================================================
	#[test]
	fn read_signed_rejects_long_integers() {
		assert_eq!(read_signed(&[0xFF]), -1);
		assert_eq!(read_signed(&[0x01, 0x00]), 256);
		assert_eq!(read_signed(&[0x80, 0, 0, 0, 0, 0, 0, 0]), i64::MIN);
		assert_eq!(read_signed(&[0xFF; 9]), 0);
		assert_eq!(read_signed(&[]), 0);
	}

	//==============================================================================
	/// 망가진 WebM: 9 바이트 DiscardPadding, 끝 값 클러스터 시각, 아주 큰 CodecDelay 에도 패닉 없이 다듬는다.
	//==============================================================================
	#[test]
	fn malformed_matroska_does_not_panic() {
		let track_entry = [create_ebml_element(&[0xD7], &[1]), create_ebml_element(&[0x83], &[2]), create_ebml_element(&[0x56, 0xAA], &[0xFF; 8])].concat();
		let tracks = create_ebml_element(&[0x16, 0x54, 0xAE, 0x6B], &create_ebml_element(&[0xAE], &track_entry));
		let block = [0x81, 0x00, 0x00, 0x00];
		let block_group = [create_ebml_element(&[0xA1], &block), create_ebml_element(&[0x75, 0xA2], &[0x7F; 9])].concat();
		for cluster_timecode in [[0x80, 0, 0, 0, 0, 0, 0, 0], [0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]] {
			let cluster_body = [create_ebml_element(&[0xE7], &cluster_timecode), create_ebml_element(&[0xA0], &block_group), create_ebml_element(&[0xA3], &[0x81, 0x7F, 0xFF, 0x00])].concat();
			let bytes = [tracks.clone(), create_ebml_element(&[0x1F, 0x43, 0xB6, 0x75], &cluster_body)].concat();
			let matroska_timing = read_matroska_timing(&bytes);
			assert!(matroska_timing.discard_padding_list.is_empty());
			let channel_data = trim_like_chrome(&create_raw_decode("matroska", "opus"), &bytes);
			assert_eq!(channel_data.len(), 1);
			assert!(channel_data[0].len() <= 960);
		}
	}

	//==============================================================================
	/// 망가진 MP4: 아주 큰 64 비트 상자 크기, 넘치는 편집 길이, 소리 길이보다 큰 iTunSMPB priming 에도 패닉 없이 다듬는다.
	//==============================================================================
	#[test]
	fn malformed_mp4_does_not_panic() {
		let mut huge_box = vec![0, 0, 0, 1];
		huge_box.extend_from_slice(b"free");
		huge_box.extend_from_slice(&u64::MAX.to_be_bytes());
		assert!(read_mp4_box(&huge_box, 0).is_some());
		let mut mdhd_body = vec![0u8; 20];
		mdhd_body[12..16].copy_from_slice(&48000u32.to_be_bytes());
		mdhd_body[16..20].copy_from_slice(&100u32.to_be_bytes());
		let mut hdlr_body = vec![0u8; 12];
		hdlr_body[8..12].copy_from_slice(b"soun");
		let mdia = create_mp4_box(b"mdia", &[create_mp4_box(b"hdlr", &hdlr_body), create_mp4_box(b"mdhd", &mdhd_body)].concat());
		let mut smpb_body = b"iTunSMPBdata\0\0\0\x01\0\0\0\0".to_vec();
		smpb_body.extend_from_slice(b" 00000000 00003000 00000010 0000000000000000\0");
		let udta = create_mp4_box(b"udta", &smpb_body);
		let moov = create_mp4_box(b"moov", &[create_mp4_box(b"trak", &mdia), udta].concat());
		let channel_data = trim_like_chrome(&create_raw_decode("isomp4", "aac"), &moov);
		assert_eq!(channel_data[0].len(), 0);
		let mut elst_body = vec![1, 0, 0, 0, 0, 0, 0, 2];
		for _ in 0..2 {
			elst_body.extend_from_slice(&u64::MAX.to_be_bytes());
			elst_body.extend_from_slice(&0i64.to_be_bytes());
			elst_body.extend_from_slice(&[0, 1, 0, 0]);
		}
		let trak = create_mp4_box(b"trak", &[create_mp4_box(b"edts", &create_mp4_box(b"elst", &elst_body)), mdia].concat());
		let moov = create_mp4_box(b"moov", &[create_mp4_box(b"mvhd", &mdhd_body), trak].concat());
		let mp4_timing = read_mp4_timing(&moov);
		assert!(mp4_timing.has_edit_list);
		assert!(mp4_timing.duration_seconds > 0.0);
	}
}

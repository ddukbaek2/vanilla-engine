//==============================================================================
//! 스트리밍 재생. (데스크탑, 원본 AudioLoadType.streaming 의 미디어 요소 자리, 유니티 AudioClip Load Type: Streaming 꼴)
//! - 풀개 스레드가 소리를 조각마다 풀어(decode/stream.rs) 링 버퍼를 채우고, 소리 스레드의 믹서(StreamSource)가 그 링에서 읽는다.
//!   소리 스레드는 풀지 않는다. (실시간 스레드에서 풀기와 파일 읽기를 하지 않는 정석)
//! - 링은 플레이어마다 하나, STREAM_RING_FRAMES 프레임이다. (44100 에서 약 0.37 초, 스테레오 f32 128 KiB)
//! - 반복은 풀개가 끝에서 새로 열어 처음 프레임을 바로 뒤에 이어 쓴다. (틈 없이, 통째로 푼 버퍼의 반복과 같은 표본)
//! - 되감기(시작 자리)는 처음부터 풀어 그 자리까지 버린다. (통째로 푼 것과 같은 표본에서 시작한다)
//==============================================================================
use super::decode::stream::{read_stream_header, StreamDecoder, StreamHeader, StreamMedia};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// 플레이어마다 드는 링 버퍼의 프레임 수.
pub(crate) const STREAM_RING_FRAMES: usize = 16384;
/// 한 번에 풀어 쓰는 프레임 수.
const FEED_CHUNK_FRAMES: usize = 4096;
/// 링이 찼을 때 풀개 스레드가 쉬는 시간. (밀리초, 링 0.37 초에 견주어 짧다)
const FEED_SLEEP_MILLISECONDS: u64 = 5;

//==============================================================================
/// 스트리밍으로 읽은 소리. (원본 getStreamUrl 의 자리와 길이, 표본은 들지 않는다)
//==============================================================================
pub(crate) struct AudioStreamData {
	pub(crate) stream_media: StreamMedia,
	pub(crate) stream_header: StreamHeader,
	/// 프레임 수. (열 때는 컨테이너로 셈한 어림, 끝까지 한 번 풀면 푼 길이)
	frame_count: AtomicU64,
}

impl AudioStreamData {
	//==============================================================================
	/// 열기. (머리만 읽는다)
	//==============================================================================
	pub(crate) fn open(stream_media: StreamMedia) -> Result<AudioStreamData, String> {
		let stream_header = read_stream_header(&stream_media)?;
		let frame_count = AtomicU64::new(stream_header.estimated_frame_count as u64);
		Ok(AudioStreamData {
			stream_media,
			stream_header,
			frame_count,
		})
	}

	//==============================================================================
	/// 프레임 수. (DECODE_SAMPLE_RATE)
	//==============================================================================
	pub(crate) fn get_frame_count(&self) -> u64 {
		self.frame_count.load(Ordering::Acquire)
	}

	//==============================================================================
	/// 내놓는 채널 수. (1 이나 2)
	//==============================================================================
	pub(crate) fn get_channel_count(&self) -> usize {
		self.stream_header.output_channel_count
	}
}

//==============================================================================
/// 링 버퍼. (풀개 스레드 하나가 쓰고 소리 스레드 하나가 읽는다, 표본은 f32 비트를 AtomicU32 에)
//==============================================================================
pub(crate) struct StreamRing {
	sample_list: Box<[AtomicU32]>,
	channel_count: usize,
	written_frame_count: AtomicU64,
	read_frame_count: AtomicU64,
	/// 다 쓴 프레임 수. (끝나지 않았으면 u64::MAX)
	end_frame_count: AtomicU64,
	/// 소리 한 바퀴의 프레임 수. (풀개가 처음 끝에 닿으면 적는다, 그 전에는 u64::MAX, 소스가 버퍼의 루프처럼 자리를 되감는 데 쓴다)
	loop_frame_count: AtomicU64,
	/// 읽는 쪽이 놓았는지. (풀개 스레드가 멈춘다)
	is_closed: AtomicBool,
}

impl StreamRing {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub(crate) fn new(channel_count: usize) -> StreamRing {
		let channel_count = channel_count.clamp(1, 2);
		StreamRing {
			sample_list: (0..STREAM_RING_FRAMES * channel_count).map(|_| AtomicU32::new(0)).collect(),
			channel_count,
			written_frame_count: AtomicU64::new(0),
			read_frame_count: AtomicU64::new(0),
			end_frame_count: AtomicU64::new(u64::MAX),
			loop_frame_count: AtomicU64::new(u64::MAX),
			is_closed: AtomicBool::new(false),
		}
	}

	//==============================================================================
	/// 채널 수.
	//==============================================================================
	pub(crate) fn get_channel_count(&self) -> usize {
		self.channel_count
	}

	//==============================================================================
	/// 드는 메모리. (바이트, 시험)
	//==============================================================================
	#[cfg(test)]
	pub(crate) fn get_memory_size(&self) -> usize {
		self.sample_list.len() * std::mem::size_of::<AtomicU32>()
	}

	//==============================================================================
	/// 쓸 수 있는 프레임 수.
	//==============================================================================
	fn get_free_frame_count(&self) -> usize {
		let written = self.written_frame_count.load(Ordering::Relaxed);
		let read = self.read_frame_count.load(Ordering::Acquire);
		STREAM_RING_FRAMES - (written - read) as usize
	}

	//==============================================================================
	/// 쓰기. (채널마다 같은 길이, 빈 칸보다 많으면 안 된다)
	//==============================================================================
	fn write(&self, channel_list: &[Vec<f32>]) {
		let frame_count = channel_list.first().map(|channel| channel.len()).unwrap_or(0);
		let written = self.written_frame_count.load(Ordering::Relaxed);
		for frame_index in 0..frame_count {
			let slot = ((written as usize + frame_index) % STREAM_RING_FRAMES) * self.channel_count;
			for channel_index in 0..self.channel_count {
				let sample = channel_list.get(channel_index).or(channel_list.first()).map(|channel| channel[frame_index]).unwrap_or(0.0);
				self.sample_list[slot + channel_index].store(sample.to_bits(), Ordering::Relaxed);
			}
		}
		self.written_frame_count.store(written + frame_count as u64, Ordering::Release);
	}

	//==============================================================================
	/// 끝 알리기. (다 쓴 프레임 수를 적는다)
	//==============================================================================
	fn finish(&self) {
		self.end_frame_count.store(self.written_frame_count.load(Ordering::Relaxed), Ordering::Release);
	}

	//==============================================================================
	/// 읽을 수 있는 프레임 수.
	//==============================================================================
	pub(crate) fn get_available_frame_count(&self) -> u64 {
		self.written_frame_count.load(Ordering::Acquire) - self.read_frame_count.load(Ordering::Relaxed)
	}

	//==============================================================================
	/// 지금까지 읽은 프레임 수.
	//==============================================================================
	pub(crate) fn get_read_frame_count(&self) -> u64 {
		self.read_frame_count.load(Ordering::Relaxed)
	}

	//==============================================================================
	/// 다 쓴 프레임 수. (끝나지 않았으면 None)
	//==============================================================================
	pub(crate) fn get_end_frame_count(&self) -> Option<u64> {
		match self.end_frame_count.load(Ordering::Acquire) {
			u64::MAX => {
				None
			}
			end_frame_count => {
				Some(end_frame_count)
			}
		}
	}

	//==============================================================================
	/// 소리 한 바퀴의 프레임 수. (풀개가 끝에 닿기 전이면 None)
	//==============================================================================
	pub(crate) fn get_loop_frame_count(&self) -> Option<u64> {
		match self.loop_frame_count.load(Ordering::Acquire) {
			u64::MAX => {
				None
			}
			loop_frame_count => {
				Some(loop_frame_count)
			}
		}
	}

	//==============================================================================
	/// 한 프레임 읽기. (get_available_frame_count 가 0 보다 클 때만)
	//==============================================================================
	pub(crate) fn read_frame(&self) -> [f32; 2] {
		let read = self.read_frame_count.load(Ordering::Relaxed);
		let slot = (read as usize % STREAM_RING_FRAMES) * self.channel_count;
		let left = f32::from_bits(self.sample_list[slot].load(Ordering::Relaxed));
		let right = if self.channel_count > 1 { f32::from_bits(self.sample_list[slot + 1].load(Ordering::Relaxed)) } else { left };
		self.read_frame_count.store(read + 1, Ordering::Release);
		[left, right]
	}

	//==============================================================================
	/// 읽는 쪽이 놓았는지.
	//==============================================================================
	fn is_closed(&self) -> bool {
		self.is_closed.load(Ordering::Acquire)
	}
}

//==============================================================================
/// 링을 읽는 쪽 손잡이. (믹서의 StreamSource 가 든다, 버리면 풀개 스레드가 멈춘다)
//==============================================================================
pub(crate) struct StreamRingReader {
	ring: Arc<StreamRing>,
}

impl StreamRingReader {
	//==============================================================================
	/// 링.
	//==============================================================================
	pub(crate) fn get_ring(&self) -> &StreamRing {
		&self.ring
	}
}

impl Drop for StreamRingReader {
	//==============================================================================
	/// 놓았다고 알린다.
	//==============================================================================
	fn drop(&mut self) {
		self.ring.is_closed.store(true, Ordering::Release);
	}
}

//==============================================================================
/// 풀개. (링의 빈 칸만큼 풀어 쓴다, 반복이면 끝에서 새로 연다)
//==============================================================================
pub(crate) struct StreamFeeder {
	stream_data: Arc<AudioStreamData>,
	ring: Arc<StreamRing>,
	stream_decoder: Option<StreamDecoder>,
	is_loop: bool,
	/// 처음에 버릴 프레임 수. (시작 자리)
	remaining_skip_frame_count: u64,
	chunk_channel_list: Vec<Vec<f32>>,
	is_finished: bool,
}

impl StreamFeeder {
	//==============================================================================
	/// 생성. (링과 읽는 쪽 손잡이를 같이 돌려준다)
	//==============================================================================
	pub(crate) fn new(stream_data: Arc<AudioStreamData>, is_loop: bool, skip_frame_count: u64) -> (StreamFeeder, StreamRingReader) {
		let channel_count = stream_data.get_channel_count();
		let ring = Arc::new(StreamRing::new(channel_count));
		let stream_ring_reader = StreamRingReader {
			ring: ring.clone(),
		};
		let stream_feeder = StreamFeeder {
			stream_data,
			ring,
			stream_decoder: None,
			is_loop,
			remaining_skip_frame_count: skip_frame_count,
			chunk_channel_list: vec![Vec::new(); channel_count],
			is_finished: false,
		};
		(stream_feeder, stream_ring_reader)
	}

	//==============================================================================
	/// 끝났는지. (다 썼거나, 읽는 쪽이 놓았거나, 풀지 못했거나)
	//==============================================================================
	pub(crate) fn is_finished(&self) -> bool {
		self.is_finished || self.ring.is_closed()
	}

	//==============================================================================
	/// 끝내기. (링에 끝을 적는다)
	//==============================================================================
	fn finish(&mut self) {
		self.ring.finish();
		self.is_finished = true;
		self.stream_decoder = None;
	}

	//==============================================================================
	/// 풀개 하나 꺼내기. (없으면 처음부터 연다, 열지 못하면 끝낸다)
	//==============================================================================
	fn take_decoder(&mut self) -> Option<StreamDecoder> {
		if let Some(stream_decoder) = self.stream_decoder.take() {
			return Some(stream_decoder);
		}
		match StreamDecoder::open(&self.stream_data.stream_media, &self.stream_data.stream_header) {
			Ok(stream_decoder) => {
				Some(stream_decoder)
			}
			Err(_) => {
				self.finish();
				None
			}
		}
	}

	//==============================================================================
	/// 풀개가 끝에 닿음. (푼 길이를 적고, 반복이면 다음에 새로 열고 아니면 끝낸다, 빈 소리면 반복하지 않는다)
	//==============================================================================
	fn reach_end(&mut self, stream_decoder: StreamDecoder) {
		let produced_frame_count = stream_decoder.get_produced_frame_count() as u64;
		self.stream_data.frame_count.store(produced_frame_count, Ordering::Release);
		self.ring.loop_frame_count.store(produced_frame_count, Ordering::Release);
		if !self.is_loop || produced_frame_count == 0 {
			self.finish();
		}
	}

	//==============================================================================
	/// 채우기. (빈 칸만큼 풀어 쓴다, 쓴 것이 있으면 참)
	//==============================================================================
	pub(crate) fn pump(&mut self) -> bool {
		let mut is_written = false;
		while !self.is_finished() {
			let mut stream_decoder = match self.take_decoder() {
				Some(stream_decoder) => {
					stream_decoder
				}
				None => {
					return is_written;
				}
			};
			// 시작 자리까지 버린다. (반복이면 길이를 넘는 자리도 다음 바퀴로 이어 버린다)
			let read_limit = if self.remaining_skip_frame_count > 0 {
				self.remaining_skip_frame_count.min(FEED_CHUNK_FRAMES as u64 * 4) as usize
			}
			else {
				let free_frame_count = self.ring.get_free_frame_count();
				if free_frame_count == 0 {
					self.stream_decoder = Some(stream_decoder);
					return is_written;
				}
				free_frame_count.min(FEED_CHUNK_FRAMES)
			};
			for chunk_channel in self.chunk_channel_list.iter_mut() {
				chunk_channel.clear();
			}
			let read_frame_count = match stream_decoder.read(&mut self.chunk_channel_list, read_limit) {
				Ok(read_frame_count) => {
					read_frame_count
				}
				Err(_) => {
					self.finish();
					return is_written;
				}
			};
			if self.remaining_skip_frame_count > 0 {
				self.remaining_skip_frame_count -= read_frame_count as u64;
			}
			else if read_frame_count > 0 {
				self.ring.write(&self.chunk_channel_list);
				is_written = true;
			}
			if read_frame_count == 0 || stream_decoder.is_ended() {
				self.reach_end(stream_decoder);
			}
			else {
				self.stream_decoder = Some(stream_decoder);
			}
		}
		is_written
	}
}

//==============================================================================
/// 스트리밍 시작 준비. (원본 미디어 요소 currentTime = time, 시작 자리는 버퍼 소스와 같게 잡는다)
/// - 빠르기 1 이면 TimeToSampleFrame 으로 반올림한 정수 자리, 아니면 소수 자리다. 정수 부분은 풀개가 처음부터 풀어 버리고, 소수 부분은 소스가 보간한다.
///
/// # 돌려줌
/// (풀개, 링을 읽는 쪽 손잡이, 시작 자리(소리의 프레임), 풀개가 버리는 프레임 수(링의 0 번 프레임이 소리의 이 프레임이다))
//==============================================================================
pub(crate) fn create_stream_start(stream_data: Arc<AudioStreamData>, is_loop: bool, time: f64, playback_rate: f64) -> (StreamFeeder, StreamRingReader, f64, u64) {
	let sample_rate = super::decode::DECODE_SAMPLE_RATE as f64;
	let duration = stream_data.get_frame_count() as f64 / sample_rate;
	let grain_offset = time.clamp(0.0, duration);
	let playback_rate = playback_rate as f32 as f64;
	let virtual_read_index = if playback_rate == 1.0 { super::mixer::time_to_sample_frame(grain_offset, sample_rate) } else { grain_offset * sample_rate };
	let skip_frame_count = virtual_read_index.floor().max(0.0);
	let (stream_feeder, stream_ring_reader) = StreamFeeder::new(stream_data, is_loop, skip_frame_count as u64);
	(stream_feeder, stream_ring_reader, virtual_read_index, skip_frame_count as u64)
}

//==============================================================================
/// 풀개 스레드 띄우기. (끝나거나 읽는 쪽이 놓으면 스스로 끝난다)
//==============================================================================
pub(crate) fn spawn_stream_feeder(mut stream_feeder: StreamFeeder) {
	let spawn_result = std::thread::Builder::new().name(String::from("vanilla-audio-stream")).spawn(move || {
		while !stream_feeder.is_finished() {
			if !stream_feeder.pump() {
				std::thread::sleep(std::time::Duration::from_millis(FEED_SLEEP_MILLISECONDS));
			}
		}
	});
	if let Err(error) = spawn_result {
		eprintln!("[audio] 스트리밍 스레드를 띄우지 못했습니다: {}", error);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 메모리: 통째로 풀면 PCM(프레임 x 채널 x 4 바이트), 스트리밍은 플레이어마다 링(16384 x 채널 x 4 바이트)과 메모리에서 읽었으면 압축된 바이트.
	/// - 3 분 스테레오면 PCM 은 180 x 44100 x 2 x 4 = 63,504,000 바이트(약 61 MiB), 링은 131,072 바이트(128 KiB)다.
	//==============================================================================
	#[test]
	fn stream_memory_is_ring_and_compressed_bytes() {
		let path = std::path::PathBuf::from(format!("{}/tests/assets/audio/vorbis_stereo.ogg", env!("CARGO_MANIFEST_DIR")));
		let bytes = std::fs::read(&path).unwrap();
		let decoded = super::super::decode::decode_audio_data(&bytes).unwrap();
		let pcm_size = decoded.channel_data.iter().map(|channel| channel.len() * 4).sum::<usize>();
		assert_eq!(pcm_size, decoded.channel_data[0].len() * 2 * 4);
		let file_stream = AudioStreamData::open(StreamMedia::File(path)).unwrap();
		assert_eq!(file_stream.stream_media.get_memory_size(), 0);
		let byte_stream = AudioStreamData::open(StreamMedia::Bytes(Arc::from(bytes.clone()))).unwrap();
		assert_eq!(byte_stream.stream_media.get_memory_size(), bytes.len());
		let (_stream_feeder, stream_ring_reader) = StreamFeeder::new(Arc::new(byte_stream), true, 0);
		assert_eq!(stream_ring_reader.get_ring().get_memory_size(), STREAM_RING_FRAMES * 2 * 4);
		assert_eq!(STREAM_RING_FRAMES * 2 * 4, 131072);
	}
}

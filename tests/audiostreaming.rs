//==============================================================================
//! 소리 읽기 방식 시험. (원본 tests/core/audiostreamingchrome.test.mjs 의 짝, AudioLoadType 과 스트리밍 애셋, 데스크탑)
//! - 자동(Automatic)의 1 MiB 문턱, 스트리밍 애셋의 길이와 주소, 플레이어로 틀고 멈추기.
//! - 스트리밍이 통째로 푼 것과 같은 표본을 내는지는 엔진 안의 시험이 본다. (decode::stream 의 streaming_matches_decode_audio_data,
//!   mixer 의 stream_source_matches_buffer_source, starved_stream_keeps_context_timing, stream_feeder_thread_fills_ring)
//==============================================================================
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;
use std::rc::Rc;
use vanilla::{AudioAsset, AudioLoadType, Engine, EngineConfiguration, AUTOMATIC_STREAMING_BYTE_THRESHOLD};

//==============================================================================
/// 16 비트 모노 8000Hz 무음 WAV. (sample_count 표본)
//==============================================================================
fn create_silent_wav(sample_count: u32) -> Vec<u8> {
	let data_size = sample_count * 2;
	let mut bytes = Vec::new();
	bytes.extend_from_slice(b"RIFF");
	bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
	bytes.extend_from_slice(b"WAVEfmt ");
	bytes.extend_from_slice(&16u32.to_le_bytes());
	bytes.extend_from_slice(&1u16.to_le_bytes());
	bytes.extend_from_slice(&1u16.to_le_bytes());
	bytes.extend_from_slice(&8000u32.to_le_bytes());
	bytes.extend_from_slice(&16000u32.to_le_bytes());
	bytes.extend_from_slice(&2u16.to_le_bytes());
	bytes.extend_from_slice(&16u16.to_le_bytes());
	bytes.extend_from_slice(b"data");
	bytes.extend_from_slice(&data_size.to_le_bytes());
	bytes.extend(std::iter::repeat_n(0u8, data_size as usize));
	bytes
}

//==============================================================================
/// 자동은 1 MiB 보다 큰 파일만 스트리밍한다. (바이트와 파일 둘 다, 기본은 통째로 풀기, 문턱 바로 위아래)
//==============================================================================
#[test]
fn automatic_load_type_uses_byte_threshold() {
	assert_eq!(AUTOMATIC_STREAMING_BYTE_THRESHOLD, 1024 * 1024);
	assert_eq!(AudioAsset::new().get_load_type(), AudioLoadType::DecompressOnLoad);
	// 머리 44 바이트 + 표본, 문턱보다 2 바이트 크게와 문턱과 같게.
	let large_bytes = create_silent_wav(((AUTOMATIC_STREAMING_BYTE_THRESHOLD - 44) / 2 + 1) as u32);
	let small_bytes = create_silent_wav(((AUTOMATIC_STREAMING_BYTE_THRESHOLD - 44) / 2) as u32);
	assert!(large_bytes.len() as u64 > AUTOMATIC_STREAMING_BYTE_THRESHOLD);
	assert_eq!(small_bytes.len() as u64, AUTOMATIC_STREAMING_BYTE_THRESHOLD);
	let load = |bytes: &[u8], load_type: AudioLoadType| -> Rc<AudioAsset> {
		let audio_asset = Rc::new(AudioAsset::new());
		audio_asset.set_load_type(load_type);
		assert!(audio_asset.load_from_bytes(bytes.to_vec()).get_load_error().is_none());
		assert!(audio_asset.is_loaded());
		audio_asset
	};
	let large_automatic = load(&large_bytes, AudioLoadType::Automatic);
	assert!(large_automatic.is_streaming());
	assert!(large_automatic.get_audio_buffer().is_none());
	assert_eq!(large_automatic.get_stream_url(), None);
	let small_automatic = load(&small_bytes, AudioLoadType::Automatic);
	assert!(!small_automatic.is_streaming());
	assert!(small_automatic.get_audio_buffer().is_some());
	assert!(!load(&large_bytes, AudioLoadType::DecompressOnLoad).is_streaming());
	assert!(load(&small_bytes, AudioLoadType::Streaming).is_streaming());
	// 길이는 통째로 푼 것과 같다. (PCM 은 컨테이너 길이가 정확하다)
	assert!((large_automatic.get_duration() - load(&large_bytes, AudioLoadType::DecompressOnLoad).get_duration()).abs() < 1e-9);
	// 파일: 크기는 파일 정보로 가리고, 스트리밍 주소는 그 경로다.
	let directory = std::env::temp_dir().join(format!("vanilla-audiostreaming-{}", std::process::id()));
	std::fs::create_dir_all(&directory).unwrap();
	let large_path = directory.join("large.wav");
	let small_path = directory.join("small.wav");
	std::fs::write(&large_path, &large_bytes).unwrap();
	std::fs::write(&small_path, &small_bytes).unwrap();
	let large_file_asset = Rc::new(AudioAsset::new());
	large_file_asset.set_load_type(AudioLoadType::Automatic);
	assert!(large_file_asset.load(&large_path).get_load_error().is_none());
	assert!(large_file_asset.is_streaming());
	assert_eq!(large_file_asset.get_stream_url(), Some(large_path.to_string_lossy().to_string()));
	let small_file_asset = Rc::new(AudioAsset::new());
	small_file_asset.set_load_type(AudioLoadType::Automatic);
	small_file_asset.load(&small_path);
	assert!(!small_file_asset.is_streaming());
	let _ = std::fs::remove_dir_all(&directory);
}

//==============================================================================
/// 스트리밍 애셋의 길이는 통째로 푼 것과 같고, 읽기 방식은 다음 읽기부터다. (없는 파일은 까닭이 남는다)
//==============================================================================
#[test]
fn streaming_asset_reports_duration_and_errors() {
	for name in ["mp3_stereo.mp3", "wav_s16_stereo.wav", "aac_stereo.m4a", "flac_stereo48.flac"] {
		let path = format!("tests/assets/audio/{}", name);
		let streaming_asset = Rc::new(AudioAsset::new());
		streaming_asset.set_load_type(AudioLoadType::Streaming);
		assert!(streaming_asset.load(Path::new(&path)).get_load_error().is_none(), "{}", name);
		let decoded_asset = Rc::new(AudioAsset::new());
		decoded_asset.load(Path::new(&path));
		assert!(streaming_asset.is_streaming());
		assert!((streaming_asset.get_duration() - decoded_asset.get_duration()).abs() < 1e-9, "{}: {} / {}", name, streaming_asset.get_duration(), decoded_asset.get_duration());
	}
	let missing_asset = Rc::new(AudioAsset::new());
	missing_asset.set_load_type(AudioLoadType::Streaming);
	assert!(missing_asset.load(Path::new("tests/assets/audio/missing.mp3")).get_load_error().is_some());
	assert!(!missing_asset.is_streaming());
	let not_audio_asset = Rc::new(AudioAsset::new());
	not_audio_asset.set_load_type(AudioLoadType::Streaming);
	assert!(not_audio_asset.load_from_bytes(b"not an audio file".to_vec()).get_load_error().is_some());
	// 이미 읽힌 애셋은 읽기 방식을 바꿔도 그대로다.
	let loaded_asset = Rc::new(AudioAsset::new());
	loaded_asset.load(Path::new("tests/assets/audio/mp3_stereo.mp3"));
	loaded_asset.set_load_type(AudioLoadType::Streaming);
	loaded_asset.load(Path::new("tests/assets/audio/mp3_stereo.mp3"));
	assert!(!loaded_asset.is_streaming());
	assert_eq!(loaded_asset.get_load_type(), AudioLoadType::Streaming);
}

//==============================================================================
/// 플레이어로 스트리밍 애셋을 틀고, 위치가 흐르고, 멈추고, 반복 없이 끝나면 is_playing 이 거짓이 된다. (소리 장치가 없으면 건너뛴다)
//==============================================================================
#[test]
fn audio_player_plays_streaming_asset() {
	let engine = Engine::new(EngineConfiguration::new());
	let audio_manager = engine.get_audio_manager().clone();
	let audio_asset = Rc::new(AudioAsset::new());
	audio_asset.set_load_type(AudioLoadType::Streaming);
	audio_asset.load(Path::new("tests/assets/audio/mp3_stereo.mp3"));
	let mut audio_player = audio_manager.create_audio_player();
	audio_player.set_audio_asset(audio_asset.clone());
	audio_player.play(true);
	if !audio_manager.has_audio_context() {
		return;
	}
	assert!(audio_player.is_playing());
	std::thread::sleep(std::time::Duration::from_millis(400));
	// 0.25 초 소리를 반복하므로 위치는 한 바퀴 안이다.
	let time = audio_player.get_time();
	assert!(time >= 0.0 && time < audio_asset.get_duration(), "{}", time);
	audio_player.stop();
	assert!(!audio_player.is_playing());
	// 반복 없이 틀면 끝난 뒤 거짓.
	audio_player.set_time(0.0);
	audio_player.play(false);
	let start_instant = std::time::Instant::now();
	while audio_player.is_playing() && start_instant.elapsed() < std::time::Duration::from_secs(3) {
		std::thread::sleep(std::time::Duration::from_millis(20));
	}
	assert!(!audio_player.is_playing());
}

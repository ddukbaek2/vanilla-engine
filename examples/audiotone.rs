//==============================================================================
// 오디오 확인 예제. (데스크탑, 창 없이 1초 남짓 소리를 내고 스스로 끝난다)
// - 440Hz 사인파 wav 를 메모리에서 만들어 AudioAsset::load_from_bytes 로 디코딩하고(kira 의 wav 해석),
//   AudioPlayer, SoundEffectPool, BeepPlayer 로 차례로 튼다.
// - 실행: cargo run --example audiotone
//==============================================================================

use std::rc::Rc;
use std::time::Duration;
use vanilla::{AudioAsset, AudioManager, BeepPlayer, SoundEffectPool};

// 만들 소리의 샘플레이트.
const TONE_SAMPLE_RATE: u32 = 44100;

//==============================================================================
// 사인파 wav 파일 내용을 만든다. (16비트 PCM 모노, 앞뒤 5밀리초는 부드럽게)
//==============================================================================
fn create_tone_wave_bytes(frequency: f64, duration_seconds: f64, amplitude: f64) -> Vec<u8> {
	let sample_count = (duration_seconds * TONE_SAMPLE_RATE as f64) as u32;
	let fade_sample_count = (0.005 * TONE_SAMPLE_RATE as f64) as u32;
	let data_byte_count = sample_count * 2;
	let mut wave_bytes = Vec::with_capacity(44 + data_byte_count as usize);
	wave_bytes.extend_from_slice(b"RIFF");
	wave_bytes.extend_from_slice(&(36 + data_byte_count).to_le_bytes());
	wave_bytes.extend_from_slice(b"WAVE");
	wave_bytes.extend_from_slice(b"fmt ");
	wave_bytes.extend_from_slice(&16u32.to_le_bytes());
	wave_bytes.extend_from_slice(&1u16.to_le_bytes());
	wave_bytes.extend_from_slice(&1u16.to_le_bytes());
	wave_bytes.extend_from_slice(&TONE_SAMPLE_RATE.to_le_bytes());
	wave_bytes.extend_from_slice(&(TONE_SAMPLE_RATE * 2).to_le_bytes());
	wave_bytes.extend_from_slice(&2u16.to_le_bytes());
	wave_bytes.extend_from_slice(&16u16.to_le_bytes());
	wave_bytes.extend_from_slice(b"data");
	wave_bytes.extend_from_slice(&data_byte_count.to_le_bytes());
	for sample_index in 0..sample_count {
		let elapsed_seconds = sample_index as f64 / TONE_SAMPLE_RATE as f64;
		let fade_in_scale = (sample_index as f64 / fade_sample_count as f64).min(1.0);
		let fade_out_scale = ((sample_count - sample_index) as f64 / fade_sample_count as f64).min(1.0);
		let sample_value = (elapsed_seconds * frequency * std::f64::consts::TAU).sin() * amplitude * fade_in_scale * fade_out_scale;
		let sample_integer = (sample_value * i16::MAX as f64) as i16;
		wave_bytes.extend_from_slice(&sample_integer.to_le_bytes());
	}
	wave_bytes
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let audio_manager = AudioManager::new();

	// 메모리의 wav 를 디코딩한다.
	let tone_audio_asset = Rc::new(AudioAsset::new());
	let tone_wave_bytes = create_tone_wave_bytes(440.0, 0.4, 0.5);
	if let Some(error) = tone_audio_asset.load_from_bytes(tone_wave_bytes).get_load_error() {
		eprintln!("audiotone: {}", error);
		std::process::exit(1);
	}
	println!("audiotone: 디코딩 {} (길이 {:.3}초)", tone_audio_asset.is_loaded(), tone_audio_asset.get_duration());

	// 플레이어 하나로 튼다.
	let mut audio_player = audio_manager.create_audio_player();
	println!("audiotone: 소리 장치 {}", audio_manager.has_audio_context());
	audio_player.set_audio_asset(tone_audio_asset.clone());
	audio_player.set_volume(0.6);
	audio_player.play(false);
	std::thread::sleep(Duration::from_millis(100));
	println!("audiotone: 재생 중 {} (0.1초 뒤)", audio_player.is_playing());
	std::thread::sleep(Duration::from_millis(450));
	println!("audiotone: 재생 중 {} (0.55초 뒤, 끝났어야 한다)", audio_player.is_playing());

	// 루프로 틀다가 멈춘다.
	audio_player.set_time(0.1);
	audio_player.play(true);
	std::thread::sleep(Duration::from_millis(500));
	println!("audiotone: 루프 재생 중 {} (0.5초 뒤)", audio_player.is_playing());
	audio_player.stop();
	println!("audiotone: 정지 뒤 재생 중 {}", audio_player.is_playing());

	// 효과음 풀로 겹쳐 튼다.
	let mut sound_effect_pool = SoundEffectPool::new(&audio_manager, 3);
	sound_effect_pool.set_volume(0.5);
	sound_effect_pool.play(&tone_audio_asset, 1.0);
	std::thread::sleep(Duration::from_millis(80));
	sound_effect_pool.play(&tone_audio_asset, 0.5);
	std::thread::sleep(Duration::from_millis(250));

	// 비프 프리셋.
	let beep_player = BeepPlayer::new(&audio_manager, BeepPlayer::DEFAULT_VOLUME);
	beep_player.play_preset("success");
	std::thread::sleep(Duration::from_millis(400));
	println!("audiotone: 끝");
}

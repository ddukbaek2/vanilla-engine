//==============================================================================
// 오디오 확인 예제. (데스크탑, 창 없이 1초 남짓 소리를 내고 스스로 끝난다)
// - 440Hz 사인파 wav 를 메모리에서 만들어 AudioAsset::loadFromBytes 로 디코딩하고(kira 의 wav 해석),
//   AudioPlayer, SoundEffectPool, BeepPlayer 로 차례로 튼다.
// - 실행: cargo run --example audiotone
//==============================================================================
#![allow(non_snake_case)]

use std::rc::Rc;
use std::time::Duration;
use vanilla::{AudioAsset, AudioManager, BeepPlayer, SoundEffectPool};

// 만들 소리의 샘플레이트.
const TONE_SAMPLE_RATE: u32 = 44100;

//==============================================================================
// 사인파 wav 파일 내용을 만든다. (16비트 PCM 모노, 앞뒤 5밀리초는 부드럽게)
//==============================================================================
fn createToneWaveBytes(frequency: f64, durationSeconds: f64, amplitude: f64) -> Vec<u8> {
	let sampleCount = (durationSeconds * TONE_SAMPLE_RATE as f64) as u32;
	let fadeSampleCount = (0.005 * TONE_SAMPLE_RATE as f64) as u32;
	let dataByteCount = sampleCount * 2;
	let mut waveBytes = Vec::with_capacity(44 + dataByteCount as usize);
	waveBytes.extend_from_slice(b"RIFF");
	waveBytes.extend_from_slice(&(36 + dataByteCount).to_le_bytes());
	waveBytes.extend_from_slice(b"WAVE");
	waveBytes.extend_from_slice(b"fmt ");
	waveBytes.extend_from_slice(&16u32.to_le_bytes());
	waveBytes.extend_from_slice(&1u16.to_le_bytes());
	waveBytes.extend_from_slice(&1u16.to_le_bytes());
	waveBytes.extend_from_slice(&TONE_SAMPLE_RATE.to_le_bytes());
	waveBytes.extend_from_slice(&(TONE_SAMPLE_RATE * 2).to_le_bytes());
	waveBytes.extend_from_slice(&2u16.to_le_bytes());
	waveBytes.extend_from_slice(&16u16.to_le_bytes());
	waveBytes.extend_from_slice(b"data");
	waveBytes.extend_from_slice(&dataByteCount.to_le_bytes());
	for sampleIndex in 0..sampleCount {
		let elapsedSeconds = sampleIndex as f64 / TONE_SAMPLE_RATE as f64;
		let fadeInScale = (sampleIndex as f64 / fadeSampleCount as f64).min(1.0);
		let fadeOutScale = ((sampleCount - sampleIndex) as f64 / fadeSampleCount as f64).min(1.0);
		let sampleValue = (elapsedSeconds * frequency * std::f64::consts::TAU).sin() * amplitude * fadeInScale * fadeOutScale;
		let sampleInteger = (sampleValue * i16::MAX as f64) as i16;
		waveBytes.extend_from_slice(&sampleInteger.to_le_bytes());
	}
	waveBytes
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let audioManager = AudioManager::new();

	// 메모리의 wav 를 디코딩한다.
	let toneAudioAsset = Rc::new(AudioAsset::new());
	let toneWaveBytes = createToneWaveBytes(440.0, 0.4, 0.5);
	if let Err(error) = toneAudioAsset.loadFromBytes(toneWaveBytes) {
		eprintln!("audiotone: {}", error);
		std::process::exit(1);
	}
	println!("audiotone: 디코딩 {} (길이 {:.3}초)", toneAudioAsset.isLoaded(), toneAudioAsset.getDuration());

	// 플레이어 하나로 튼다.
	let mut audioPlayer = audioManager.createAudioPlayer();
	println!("audiotone: 소리 장치 {}", audioManager.hasAudioContext());
	audioPlayer.setAudioAsset(toneAudioAsset.clone());
	audioPlayer.setVolume(0.6);
	audioPlayer.play(false);
	std::thread::sleep(Duration::from_millis(100));
	println!("audiotone: 재생 중 {} (0.1초 뒤)", audioPlayer.isPlaying());
	std::thread::sleep(Duration::from_millis(450));
	println!("audiotone: 재생 중 {} (0.55초 뒤, 끝났어야 한다)", audioPlayer.isPlaying());

	// 루프로 틀다가 멈춘다.
	audioPlayer.setTime(0.1);
	audioPlayer.play(true);
	std::thread::sleep(Duration::from_millis(500));
	println!("audiotone: 루프 재생 중 {} (0.5초 뒤)", audioPlayer.isPlaying());
	audioPlayer.stop();
	println!("audiotone: 정지 뒤 재생 중 {}", audioPlayer.isPlaying());

	// 효과음 풀로 겹쳐 튼다.
	let mut soundEffectPool = SoundEffectPool::new(&audioManager, 3);
	soundEffectPool.setVolume(0.5);
	soundEffectPool.play(&toneAudioAsset, 1.0);
	std::thread::sleep(Duration::from_millis(80));
	soundEffectPool.play(&toneAudioAsset, 0.5);
	std::thread::sleep(Duration::from_millis(250));

	// 비프 프리셋.
	let beepPlayer = BeepPlayer::new(&audioManager, BeepPlayer::DEFAULT_VOLUME);
	beepPlayer.playPreset("success");
	std::thread::sleep(Duration::from_millis(400));
	println!("audiotone: 끝");
}

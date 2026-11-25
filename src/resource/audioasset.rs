//==============================================================================
// 오디오 애셋.
// - 원본 resource/audioasset.js. 소리 파일을 디코딩한 오디오 버퍼를 들고 있다.
// - 오디오 버퍼 자리는 백엔드가 채운다. (platform/audio 의 AudioBuffer)
//   데스크탑: kira 의 StaticSoundData. (platform/audio/decode 가 원본의 크롬 decodeAudioData 와 같은 길이, 표본율(44100), 표본으로 푼다.
//   webm, ogg 의 Opus 와 Vorbis, mp3, wav, flac, m4a 와 aac 의 AAC. 꼴은 확장자가 아니라 내용으로 가린다)
//   웹: 원본과 같은 WebAudio AudioBuffer. (OfflineAudioContext 로 디코딩한다, 없으면 잠시 쓰는 AudioContext)
// - 읽기:
//   데스크탑은 load / load_from_bytes 가 그 자리에서 파일을 읽고 디코딩한 뒤 돌아온다. 실패하면 Err.
//   웹은 막을 수 없어서 load 가 fetch 와 디코딩을 걸어 두고 바로 Ok 로 돌아온다. 끝나면 is_loaded() 가 참이 되고,
//   실패하면 get_load_error() 에 까닭이 남는다. 그 전에 플레이어에 실어 둬도 되고, 버퍼가 없는 동안 play 는 아무것도 하지 않는다.
//   엔진에서는 engine.load_audio_asset("sounds/hit.ogg") 로 애셋 폴더 기준 경로를 읽는다.
// - 원본과 다른 점:
//   Asset 기반 클래스(애셋 타입)가 없다. 읽기 실패는 예외 대신 Err 로 알린다. (까닭은 원본 getLoadError 처럼 get_load_error() 에 남는다)
//   플레이어들이 같이 들 수 있게 Rc<AudioAsset> 로 쓰고, 웹에서 디코딩이 끝난 뒤 채우도록 안쪽을 RefCell 로 둔다.
//==============================================================================
use crate::platform::audio::{self as audio_backend, AudioBuffer};
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

//==============================================================================
// 오디오 애셋.
//==============================================================================
pub struct AudioAsset {
	is_loaded: Cell<bool>,
	audio_buffer: RefCell<Option<AudioBuffer>>,
	load_error: RefCell<Option<String>>,
}

impl AudioAsset {
	//==============================================================================
	// 생성. (비어 있는 애셋, load 로 채운다)
	//==============================================================================
	pub fn new() -> AudioAsset {
		AudioAsset {
			is_loaded: Cell::new(false),
			audio_buffer: RefCell::new(None),
			load_error: RefCell::new(None),
		}
	}

	//==============================================================================
	// 파일에서 읽기. (데스크탑: 바로 읽고 디코딩한다. 웹: 경로를 주소로 fetch 하고 디코딩을 걸어 둔다)
	// - 이미 로드된 상태라면 아무것도 하지 않는다.
	//==============================================================================
	pub fn load(self: &Rc<AudioAsset>, asset_path: &Path) -> Result<(), String> {
		if self.is_loaded() {
			return Ok(());
		}
		self.load_error.replace(None);
		audio_backend::load_audio_file(self, asset_path)
	}

	//==============================================================================
	// 메모리에서 읽기. (파일 내용 그대로. 데스크탑: 바로 디코딩한다. 웹: 디코딩을 걸어 둔다)
	//==============================================================================
	pub fn load_from_bytes(self: &Rc<AudioAsset>, bytes: Vec<u8>) -> Result<(), String> {
		if self.is_loaded() {
			return Ok(());
		}
		self.load_error.replace(None);
		audio_backend::load_audio_bytes(self, bytes)
	}

	//==============================================================================
	// 로드 여부.
	//==============================================================================
	pub fn is_loaded(&self) -> bool {
		self.is_loaded.get()
	}

	//==============================================================================
	// 마지막 읽기 실패의 까닭. (웹에서 디코딩이 끝난 뒤에 알 수 있다)
	//==============================================================================
	pub fn get_load_error(&self) -> Option<String> {
		self.load_error.borrow().clone()
	}

	//==============================================================================
	// 오디오 버퍼 반환. (값싼 복제: 데스크탑은 Arc, 웹은 자바스크립트 객체 참조)
	//==============================================================================
	pub fn get_audio_buffer(&self) -> Option<AudioBuffer> {
		self.audio_buffer.borrow().clone()
	}

	//==============================================================================
	// 재생 시간 반환. (초, 버퍼가 없으면 0)
	//==============================================================================
	pub fn get_duration(&self) -> f64 {
		match self.audio_buffer.borrow().as_ref() {
			Some(audio_buffer) => {
				audio_backend::get_audio_buffer_duration(audio_buffer)
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	// 디코딩한 버퍼를 채운다. (백엔드가 부른다)
	//==============================================================================
	pub(crate) fn set_audio_buffer(&self, audio_buffer: AudioBuffer) {
		self.audio_buffer.replace(Some(audio_buffer));
		self.is_loaded.set(true);
	}

	//==============================================================================
	// 읽기 실패를 남긴다. (백엔드가 부른다)
	//==============================================================================
	pub(crate) fn set_load_error(&self, load_error: String) {
		self.load_error.replace(Some(load_error));
	}
}

impl Default for AudioAsset {
	fn default() -> AudioAsset {
		AudioAsset::new()
	}
}

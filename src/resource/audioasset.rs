//==============================================================================
// 오디오 애셋.
// - 원본 resource/audioasset.js. 소리 파일을 디코딩한 오디오 버퍼를 들고 있다.
// - 오디오 버퍼 자리는 백엔드가 채운다. (platform/audio 의 AudioBuffer)
//   데스크탑: kira 의 StaticSoundData. (ogg vorbis, mp3, wav 를 symphonia 로 디코딩한다)
//   웹: 원본과 같은 WebAudio AudioBuffer. (OfflineAudioContext 로 디코딩한다, 없으면 잠시 쓰는 AudioContext)
// - 읽기:
//   데스크탑은 load / loadFromBytes 가 그 자리에서 파일을 읽고 디코딩한 뒤 돌아온다. 실패하면 Err.
//   웹은 막을 수 없어서 load 가 fetch 와 디코딩을 걸어 두고 바로 Ok 로 돌아온다. 끝나면 isLoaded() 가 참이 되고,
//   실패하면 getLoadError() 에 까닭이 남는다. 그 전에 플레이어에 실어 둬도 되고, 버퍼가 없는 동안 play 는 아무것도 하지 않는다.
//   엔진에서는 engine.loadAudioAsset("sounds/hit.ogg") 로 애셋 폴더 기준 경로를 읽는다.
// - 원본과 다른 점:
//   Asset 기반 클래스(애셋 타입)가 없다. 읽기 실패는 예외 대신 Err 나 getLoadError() 로 알린다.
//   플레이어들이 같이 들 수 있게 Rc<AudioAsset> 로 쓰고, 웹에서 디코딩이 끝난 뒤 채우도록 안쪽을 RefCell 로 둔다.
//==============================================================================
use crate::platform::audio::{self as audioBackend, AudioBuffer};
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

//==============================================================================
// 오디오 애셋.
//==============================================================================
pub struct AudioAsset {
	isLoaded: Cell<bool>,
	audioBuffer: RefCell<Option<AudioBuffer>>,
	loadError: RefCell<Option<String>>,
}

impl AudioAsset {
	//==============================================================================
	// 생성. (비어 있는 애셋, load 로 채운다)
	//==============================================================================
	pub fn new() -> AudioAsset {
		AudioAsset {
			isLoaded: Cell::new(false),
			audioBuffer: RefCell::new(None),
			loadError: RefCell::new(None),
		}
	}

	//==============================================================================
	// 파일에서 읽기. (데스크탑: 바로 읽고 디코딩한다. 웹: 경로를 주소로 fetch 하고 디코딩을 걸어 둔다)
	// - 이미 로드된 상태라면 아무것도 하지 않는다.
	//==============================================================================
	pub fn load(self: &Rc<AudioAsset>, assetPath: &Path) -> Result<(), String> {
		if self.isLoaded() {
			return Ok(());
		}
		self.loadError.replace(None);
		audioBackend::loadAudioFile(self, assetPath)
	}

	//==============================================================================
	// 메모리에서 읽기. (파일 내용 그대로. 데스크탑: 바로 디코딩한다. 웹: 디코딩을 걸어 둔다)
	//==============================================================================
	pub fn loadFromBytes(self: &Rc<AudioAsset>, bytes: Vec<u8>) -> Result<(), String> {
		if self.isLoaded() {
			return Ok(());
		}
		self.loadError.replace(None);
		audioBackend::loadAudioBytes(self, bytes)
	}

	//==============================================================================
	// 로드 여부.
	//==============================================================================
	pub fn isLoaded(&self) -> bool {
		self.isLoaded.get()
	}

	//==============================================================================
	// 마지막 읽기 실패의 까닭. (웹에서 디코딩이 끝난 뒤에 알 수 있다)
	//==============================================================================
	pub fn getLoadError(&self) -> Option<String> {
		self.loadError.borrow().clone()
	}

	//==============================================================================
	// 오디오 버퍼 반환. (값싼 복제: 데스크탑은 Arc, 웹은 자바스크립트 객체 참조)
	//==============================================================================
	pub fn getAudioBuffer(&self) -> Option<AudioBuffer> {
		self.audioBuffer.borrow().clone()
	}

	//==============================================================================
	// 재생 시간 반환. (초, 버퍼가 없으면 0)
	//==============================================================================
	pub fn getDuration(&self) -> f64 {
		match self.audioBuffer.borrow().as_ref() {
			Some(audioBuffer) => {
				audioBackend::getAudioBufferDuration(audioBuffer)
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	// 디코딩한 버퍼를 채운다. (백엔드가 부른다)
	//==============================================================================
	pub(crate) fn setAudioBuffer(&self, audioBuffer: AudioBuffer) {
		self.audioBuffer.replace(Some(audioBuffer));
		self.isLoaded.set(true);
	}

	//==============================================================================
	// 읽기 실패를 남긴다. (백엔드가 부른다)
	//==============================================================================
	pub(crate) fn setLoadError(&self, loadError: String) {
		self.loadError.replace(Some(loadError));
	}
}

impl Default for AudioAsset {
	fn default() -> AudioAsset {
		AudioAsset::new()
	}
}

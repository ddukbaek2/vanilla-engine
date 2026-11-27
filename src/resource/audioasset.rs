//==============================================================================
//! 오디오 애셋.
//! - 원본 resource/audioasset.js. 소리 파일을 디코딩한 오디오 버퍼를 들고 있다.
//! - 오디오 버퍼 자리는 백엔드가 채운다. (platform/audio 의 AudioBuffer)
//!   데스크탑: 푼 소리를 Arc 로 든 AudioBuffer. (platform/audio/decode 가 원본의 크롬 decodeAudioData 와 같은 길이, 표본율(44100), 표본으로 푼다.
//!   webm, ogg 의 Opus 와 Vorbis, mp3, wav, flac, m4a 와 aac 의 AAC. 꼴은 확장자가 아니라 내용으로 가린다)
//!   웹: 원본과 같은 WebAudio AudioBuffer. (OfflineAudioContext 로 디코딩한다, 없으면 잠시 쓰는 AudioContext)
//! - 읽기:
//!   데스크탑은 load / load_from_bytes 가 그 자리에서 파일을 읽고 디코딩한 뒤 돌아온다. 실패하면 Err.
//!   웹은 막을 수 없어서 load 가 fetch 와 디코딩을 걸어 두고 바로 Ok 로 돌아온다. 끝나면 is_loaded() 가 참이 되고,
//!   실패하면 get_load_error() 에 까닭이 남는다. 그 전에 플레이어에 실어 둬도 되고, 버퍼가 없는 동안 play 는 아무것도 하지 않는다.
//!   load, load_from_bytes, load_from_asset 은 원본처럼 await 할 수 있는 AudioLoadHandle 을 돌려준다. 디코딩까지 끝나면 Ok, 실패하면 까닭(원본 reject)이다.
//!   (그 자리에서 실패를 알 수 있는 데스크탑은 손잡이의 get_load_error 로 바로 본다) 이미 읽기를 건 애셋은 wait_for_load().await 로도 기다린다.
//!   엔진에서는 engine.load_audio_asset("sounds/hit.ogg") 로 애셋 폴더 기준 경로를 읽는다.
//! - 원본과 다른 점:
//!   Asset 기반 클래스(애셋 타입)가 없다. 읽기 실패는 예외 대신 Err 로 알린다. (까닭은 원본 getLoadError 처럼 get_load_error() 에 남는다)
//!   플레이어들이 같이 들 수 있게 `Rc<AudioAsset>` 로 쓰고, 웹에서 디코딩이 끝난 뒤 채우도록 안쪽을 RefCell 로 둔다.
//==============================================================================
use crate::core::engine::Engine;
use crate::platform::audio::{self as audio_backend, AudioBuffer};
use std::cell::{Cell, RefCell};
use std::future::{Future, IntoFuture};
use std::path::Path;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

//==============================================================================
/// 오디오 읽기 손잡이. (AudioAsset 의 load 들이 돌려준다. 원본 load 의 Promise 자리, 애셋을 나눠 든다)
///
/// # 예제
/// 소리가 아닌 바이트는 데스크탑에서 그 자리에서 실패로 끝난다
/// ```rust
/// use std::rc::Rc;
/// use vanilla::AudioAsset;
///
/// let audio_asset = Rc::new(AudioAsset::new());
/// let audio_load_handle = audio_asset.load_from_bytes(vec![0, 1, 2, 3]);
/// assert!(audio_load_handle.is_finished());
/// assert!(audio_load_handle.get_load_error().is_some());
/// assert!(!audio_load_handle.get_audio_asset().is_loaded());
/// ```
//==============================================================================
#[derive(Clone)]
pub struct AudioLoadHandle {
	audio_asset: Rc<AudioAsset>,
}

impl AudioLoadHandle {
	//==============================================================================
	/// 읽는 애셋.
	///
	/// # 돌려줌
	/// 이 손잡이가 나눠 든 오디오 애셋.
	//==============================================================================
	pub fn get_audio_asset(&self) -> &Rc<AudioAsset> {
		&self.audio_asset
	}

	//==============================================================================
	/// 읽기 실패의 까닭. (데스크탑은 그 자리에서, 웹은 디코딩이 끝난 뒤 알 수 있다)
	///
	/// # 돌려줌
	/// 실패 까닭. 읽는 중이거나 읽혔으면 None.
	//==============================================================================
	pub fn get_load_error(&self) -> Option<String> {
		self.audio_asset.get_load_error()
	}

	//==============================================================================
	/// 끝났는지. (디코딩까지 끝났거나 실패했거나)
	///
	/// # 돌려줌
	/// 디코딩까지 끝났거나 실패했으면 true.
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.audio_asset.is_loaded() || self.audio_asset.get_load_error().is_some()
	}
}

impl std::fmt::Debug for AudioLoadHandle {
	//==============================================================================
	/// 디버그 출력. (읽혔는지와 실패 까닭)
	//==============================================================================
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.debug_struct("AudioLoadHandle").field("is_loaded", &self.audio_asset.is_loaded()).field("load_error", &self.audio_asset.get_load_error()).finish()
	}
}

//==============================================================================
/// 오디오 읽기를 기다리는 퓨처. (AudioLoadHandle 을 await 하거나 wait_for_load 가 돌려준다. 읽기를 걸지 않은 애셋이면 끝나지 않는다)
/// - 디코딩까지 끝나면 Ok(()), 실패하면 Err(까닭)으로 끝난다.
///
/// # 예제
/// 엔진 작업 안에서 소리 읽기를 기다리기
/// ```rust
/// use std::path::Path;
/// use std::rc::Rc;
/// use vanilla::{AudioAsset, Engine, EngineConfiguration};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let audio_asset = Rc::new(AudioAsset::new());
/// audio_asset.load(Path::new("tests/assets/audio/wav_s16_stereo.wav"));
/// let audio_load_future = audio_asset.wait_for_load();
/// let task_handle = engine.spawn_task(|_engine_context| async move {
/// 	assert!(audio_load_future.await.is_ok());
/// });
/// assert!(task_handle.is_finished());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct AudioLoadFuture {
	audio_load_handle: AudioLoadHandle,
}

impl Future for AudioLoadFuture {
	type Output = Result<(), String>;

	//==============================================================================
	/// 끝났는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut AudioLoadFuture>, context: &mut Context<'_>) -> Poll<Result<(), String>> {
		if self.audio_load_handle.audio_asset.is_loaded() {
			return Poll::Ready(Ok(()));
		}
		if let Some(load_error) = self.audio_load_handle.audio_asset.get_load_error() {
			return Poll::Ready(Err(load_error));
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

impl IntoFuture for AudioLoadHandle {
	type Output = Result<(), String>;
	type IntoFuture = AudioLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> AudioLoadFuture {
		AudioLoadFuture {
			audio_load_handle: self,
		}
	}
}

//==============================================================================
/// 오디오 애셋.
/// - 플레이어들이 같이 들 수 있게 `Rc<AudioAsset>` 로 쓴다. load 들이 Rc 에 대고 부르는 까닭이다.
///
/// # 예제
/// 애셋 경로로 소리를 읽어 길이 보기
/// ```rust
/// use std::rc::Rc;
/// use vanilla::{AudioAsset, Engine, EngineConfiguration};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let audio_asset = Rc::new(AudioAsset::new());
/// let audio_load_handle = audio_asset.load_from_asset(&engine, "tests/assets/audio/wav_s16_stereo.wav");
/// assert!(audio_load_handle.get_load_error().is_none());
/// assert!(audio_asset.is_loaded());
/// assert!(audio_asset.get_duration() > 0.0);
/// assert!(audio_asset.get_audio_buffer().is_some());
/// ```
//==============================================================================
pub struct AudioAsset {
	is_loaded: Cell<bool>,
	audio_buffer: RefCell<Option<AudioBuffer>>,
	load_error: RefCell<Option<String>>,
}

impl AudioAsset {
	//==============================================================================
	/// 생성. (비어 있는 애셋, load 로 채운다)
	///
	/// # 돌려줌
	/// 버퍼도 실패 까닭도 없는 오디오 애셋.
	//==============================================================================
	pub fn new() -> AudioAsset {
		AudioAsset {
			is_loaded: Cell::new(false),
			audio_buffer: RefCell::new(None),
			load_error: RefCell::new(None),
		}
	}

	//==============================================================================
	/// 파일에서 읽기. (데스크탑: 바로 읽고 디코딩한다. 웹: 경로를 주소로 fetch 하고 디코딩을 걸어 둔다, await 할 수 있는 손잡이를 돌려준다)
	/// - 이미 로드된 상태라면 아무것도 하지 않는다.
	/// - 읽기 전에 앞의 실패 까닭을 지운다. 실패하면 까닭이 get_load_error 에 남는다.
	///
	/// # 인자
	/// - `asset_path`: 소리 파일 경로. (데스크탑은 파일 경로 그대로, 웹은 fetch 할 주소, 애셋 폴더 기준으로 읽으려면 load_from_asset)
	///
	/// # 돌려줌
	/// 읽기 손잡이. (데스크탑은 그 자리에서 디코딩까지 끝나 있다)
	///
	/// # 예제
	/// 파일 경로로 읽고, 없는 파일은 까닭이 남는다
	/// ```rust
	/// use std::path::Path;
	/// use std::rc::Rc;
	/// use vanilla::AudioAsset;
	///
	/// let audio_asset = Rc::new(AudioAsset::new());
	/// audio_asset.load(Path::new("tests/assets/audio/wav_u8_mono8k.wav"));
	/// assert!(audio_asset.is_loaded());
	/// assert!((audio_asset.get_duration() - 0.25).abs() < 0.01);
	///
	/// let missing_asset = Rc::new(AudioAsset::new());
	/// let audio_load_handle = missing_asset.load(Path::new("tests/assets/audio/missing.wav"));
	/// assert!(audio_load_handle.get_load_error().is_some());
	/// assert_eq!(missing_asset.get_duration(), 0.0);
	/// ```
	//==============================================================================
	pub fn load(self: &Rc<AudioAsset>, asset_path: &Path) -> AudioLoadHandle {
		if !self.is_loaded() {
			self.load_error.replace(None);
			if let Err(load_error) = audio_backend::load_audio_file(self, asset_path) {
				self.set_load_error(load_error);
			}
		}
		self.create_load_handle()
	}

	//==============================================================================
	/// 애셋 경로로 읽기. (데스크탑은 애셋 폴더 후보, 웹은 asset_base_url 기준, "data:" 주소도 된다. 원본 load(경로) 자리, await 할 수 있는 손잡이를 돌려준다)
	/// - 데스크탑의 data: 주소는 형을 보지 않고 바이트를 내용으로 가려 푼다. (원본 decodeAudioData 와 같다, 웹은 브라우저 fetch 가 푼다)
	/// - 데스크탑에서 data: 주소를 풀지 못하면 실패 까닭을 남긴다.
	///
	/// # 인자
	/// - `engine`: 애셋 폴더 후보와 웹 기준 주소를 가진 엔진.
	/// - `relative_path`: 애셋 폴더 기준 상대 경로나 "data:" 주소.
	///
	/// # 돌려줌
	/// 읽기 손잡이. (데스크탑은 그 자리에서 디코딩까지 끝나 있다)
	//==============================================================================
	pub fn load_from_asset(self: &Rc<AudioAsset>, engine: &Engine, relative_path: &str) -> AudioLoadHandle {
		#[cfg(not(target_arch = "wasm32"))]
		{
			if crate::base::dataurl::is_data_url(relative_path) {
				return match crate::base::dataurl::parse_data_url(relative_path) {
					Some(data_url) => {
						self.load_from_bytes(data_url.bytes)
					}
					None => {
						self.set_load_error(String::from("Error loading sound (data: 주소를 풀 수 없습니다)"));
						self.create_load_handle()
					}
				};
			}
		}
		self.load(&engine.resolve_audio_asset_path(relative_path))
	}

	//==============================================================================
	/// 메모리에서 읽기. (파일 내용 그대로. 데스크탑: 바로 디코딩한다. 웹: 디코딩을 걸어 둔다, await 할 수 있는 손잡이를 돌려준다)
	/// - 이미 로드된 상태라면 아무것도 하지 않는다. 꼴은 확장자 없이 내용으로 가린다.
	///
	/// # 인자
	/// - `bytes`: 소리 파일 바이트. (wav, mp3, ogg, webm, flac, m4a, aac)
	///
	/// # 돌려줌
	/// 읽기 손잡이. (데스크탑은 그 자리에서 디코딩까지 끝나 있다)
	///
	/// # 예제
	/// 파일 바이트를 메모리에서 디코딩하기
	/// ```rust
	/// use std::rc::Rc;
	/// use vanilla::AudioAsset;
	///
	/// let wav_bytes = std::fs::read("tests/assets/audio/wav_s16_stereo.wav").expect("wav file");
	/// let audio_asset = Rc::new(AudioAsset::new());
	/// let audio_load_handle = audio_asset.load_from_bytes(wav_bytes);
	/// assert!(audio_load_handle.is_finished());
	/// assert!(audio_asset.is_loaded());
	/// ```
	//==============================================================================
	pub fn load_from_bytes(self: &Rc<AudioAsset>, bytes: Vec<u8>) -> AudioLoadHandle {
		if !self.is_loaded() {
			self.load_error.replace(None);
			if let Err(load_error) = audio_backend::load_audio_bytes(self, bytes) {
				self.set_load_error(load_error);
			}
		}
		self.create_load_handle()
	}

	//==============================================================================
	/// 읽기 손잡이 만들기. (애셋을 나눠 든다)
	//==============================================================================
	fn create_load_handle(self: &Rc<AudioAsset>) -> AudioLoadHandle {
		AudioLoadHandle {
			audio_asset: self.clone(),
		}
	}

	//==============================================================================
	/// 로드 여부.
	///
	/// # 돌려줌
	/// 디코딩한 버퍼가 들어 있으면 true.
	//==============================================================================
	pub fn is_loaded(&self) -> bool {
		self.is_loaded.get()
	}

	//==============================================================================
	/// 마지막 읽기 실패의 까닭. (웹에서 디코딩이 끝난 뒤에 알 수 있다)
	///
	/// # 돌려줌
	/// 실패 까닭. 실패한 적이 없거나 다시 읽기를 걸었으면 None.
	//==============================================================================
	pub fn get_load_error(&self) -> Option<String> {
		self.load_error.borrow().clone()
	}

	//==============================================================================
	/// 디코딩까지 끝나기를 기다리는 퓨처. (원본 await audioAsset.load() 의 await 자리, 엔진 작업 안에서 await 한다. 실패하면 까닭)
	/// - 애셋을 나눠 들어 빌리지 않는다. 읽기를 걸지 않은 애셋이면 끝나지 않는다.
	///
	/// # 돌려줌
	/// 디코딩까지 끝나면 Ok(()), 실패하면 Err(까닭)으로 끝나는 퓨처.
	//==============================================================================
	pub fn wait_for_load(self: &Rc<AudioAsset>) -> AudioLoadFuture {
		self.create_load_handle().into_future()
	}

	//==============================================================================
	/// 읽기 손잡이. (애셋을 나눠 든다, 읽기를 건 뒤의 것을 모아 기다릴 때)
	///
	/// # 돌려줌
	/// 이 애셋을 나눠 든 새 읽기 손잡이.
	//==============================================================================
	pub fn get_load_handle(self: &Rc<AudioAsset>) -> AudioLoadHandle {
		self.create_load_handle()
	}

	//==============================================================================
	/// 오디오 버퍼 반환. (값싼 복제: 데스크탑은 Arc, 웹은 자바스크립트 객체 참조)
	///
	/// # 돌려줌
	/// 디코딩한 버퍼. 읽히기 전이거나 실패했으면 None.
	//==============================================================================
	pub fn get_audio_buffer(&self) -> Option<AudioBuffer> {
		self.audio_buffer.borrow().clone()
	}

	//==============================================================================
	/// 재생 시간 반환. (초, 버퍼가 없으면 0)
	///
	/// # 돌려줌
	/// 버퍼의 재생 시간. (초) 버퍼가 없으면 0.
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
	/// 디코딩한 버퍼를 채운다. (백엔드가 부른다)
	//==============================================================================
	pub(crate) fn set_audio_buffer(&self, audio_buffer: AudioBuffer) {
		self.audio_buffer.replace(Some(audio_buffer));
		self.is_loaded.set(true);
	}

	//==============================================================================
	/// 읽기 실패를 남긴다. (백엔드가 부른다)
	//==============================================================================
	pub(crate) fn set_load_error(&self, load_error: String) {
		self.load_error.replace(Some(load_error));
	}
}

impl Default for AudioAsset {
	//==============================================================================
	/// 비어 있는 오디오 애셋. (new 와 같다)
	//==============================================================================
	fn default() -> AudioAsset {
		AudioAsset::new()
	}
}

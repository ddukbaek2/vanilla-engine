//==============================================================================
// 데스크탑 오디오 백엔드. (윈도우, 맥, 리눅스, cpal 출력 위의 엔진 믹서)
// - AudioContext 자리: cpal 로 기본 출력 장치를 열고 그 표본율로 도는 믹서(mixer.rs). 장치가 없으면 한 번 알리고 다시 열지 않는다.
//   브라우저 자동재생 정책이 없어서 user gesture 는 처음부터 있는 것으로 둔다. sample_rate(), current_time() 은 웹 AudioContext 와 같은 이름이다.
// - 노드 그래프는 원본 WebAudio 와 같고 셈도 크롬과 같다. (mixer.rs 머리 주석)
//   AudioPlayer 하나 = 소스 + GainNode (+ 0 이 아닌 좌우를 처음 걸면 StereoPannerNode), 내보낼 곳(AudioOutputNode)을 주면 그 버스로, 없으면 출력으로.
//   AudioBus 하나 = 출력에 이은 GainNode. (AudioGainNode, 원본 getGainNode) 버스로 만든 플레이어는 버스 손잡이를 들고 있어,
//   원본 WebAudio 처럼 이어진 플레이어가 있는 동안 버스가 살아 있다. 손잡이가 모두 없어지면 노드를 뺀다.
// - 컨텍스트 멈춤(SoundSystem 이 음소거 동안 부른다): 믹서가 그리기를 멈춰 소리와 시계가 그 자리에서 선다. (원본 AudioContext.suspend) resume 이 다시 돌린다.
// - 컨텍스트 시계(get_audio_context_time)는 그린 시간이다. (원본 currentTime, 128 프레임 퀀텀마다 128 / 표본율 초씩 오른다)
// - 스트림은 관리 스레드가 든다. 장치가 빠지면(맥이 아니면 기본 장치가 바뀌어도) 같은 믹서로 기본 장치를 다시 연다. (원본 크롬도 기본 장치를 따라간다)
// - AudioBuffer 자리: 푼 소리를 Arc 로 든다. 복제는 값싸다. 해석은 decode 가 원본의 크롬 decodeAudioData 와 같게 한다.
//   (webm, ogg 의 Opus 와 Vorbis, mp3, wav, flac, m4a 와 aac 의 AAC. 앞뒤 다듬기, 44100Hz, WebAudio 스피커 규칙으로 내린 1 채널이나 2 채널)
// - 음량, 좌우, 정지는 다음 렌더 퀀텀부터 바로 바뀐다. 재생 빠르기는 다음 재생부터다. (원본과 같다)
// - 음량은 GainNode 의 AudioParam 이다. 음량 설정은 원본 gain.value 대입, SoundSystem 은 원본처럼 자동화(setValueAtTime 들)와 stop(when)을 건다.
//   지금 gain.value(get_gain_value)는 소리 스레드가 퀀텀마다 셈한 내재 값이다.
// - BeepPlayer 의 톤은 엔벨로프를 곱한 파형을 컨텍스트 표본율로 그려 출력에 섞는다. (원본 OscillatorNode 가 컨텍스트 표본율로 도는 것과 같다)
//   파형은 크롬 WebAudio 오실레이터(PeriodicWave)와 같은 셈이다. 푸리에 계수를 표 크기의 반 아래까지 두고 주파수에 따라 1/3 옥타브마다 높은 배음을 덜어 낸
//   두 표 사이를 섞으며(대역 제한), 배음을 다 둔 첫 표의 가장 큰 값이 1 이 되게 크기를 맞춘다.
//==============================================================================
use super::audioparam::ParamAutomation;
use super::mixer::{AudioBufferData, MixerCommand, MixerRenderer, MixerShared};
use super::GainAutomation;
use crate::core::audiomanager::AudioManagerState;
use crate::misc::beepplayer::{BeepEnvelope, BeepWaveform, BEEP_FLOOR_VOLUME, BEEP_STOP_MARGIN_SECONDS};
use crate::resource::audioasset::AudioAsset;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::sync::mpsc::Sender;
use std::sync::Arc;

//==============================================================================
// 공유 오디오 컨텍스트. (원본 AudioContext 자리, 장치 스트림과 믹서로 가는 길)
//==============================================================================
pub struct AudioContext {
	_stream_manager: StreamManager,
	mixer_link: Rc<MixerLink>,
}

impl AudioContext {
	//==============================================================================
	// 표본율. (원본 audioContext.sampleRate, 장치의 표본율)
	//==============================================================================
	pub fn sample_rate(&self) -> f32 {
		self.mixer_link.shared.get_sample_rate() as f32
	}

	//==============================================================================
	// 시계. (초, 원본 audioContext.currentTime)
	//==============================================================================
	pub fn current_time(&self) -> f64 {
		self.mixer_link.get_current_time()
	}
}

//==============================================================================
// 푼 소리. (원본 AudioBuffer 자리, 복제는 Arc 하나라 값싸다)
//==============================================================================
#[derive(Clone)]
pub struct AudioBuffer {
	data: Arc<AudioBufferData>,
}

impl AudioBuffer {
	//==============================================================================
	// 재생 시간. (초, 원본 duration)
	//==============================================================================
	pub fn duration(&self) -> f64 {
		self.data.get_length() as f64 / self.data.sample_rate
	}

	//==============================================================================
	// 프레임 수. (원본 length)
	//==============================================================================
	pub fn length(&self) -> usize {
		self.data.get_length()
	}

	//==============================================================================
	// 채널 수. (원본 numberOfChannels, 데스크탑은 스피커 규칙으로 내린 1 이나 2)
	//==============================================================================
	pub fn number_of_channels(&self) -> usize {
		self.data.channel_list.len()
	}

	//==============================================================================
	// 표본율. (원본 sampleRate, 44100)
	//==============================================================================
	pub fn sample_rate(&self) -> f32 {
		self.data.sample_rate as f32
	}
}

//==============================================================================
// 버스의 GainNode. (원본 AudioBus.getGainNode, 마지막 손잡이가 없어지면 믹서에서 뺀다)
//==============================================================================
pub struct MixerGainNode {
	inner: Rc<MixerGainNodeInner>,
}

struct MixerGainNodeInner {
	mixer_link: Rc<MixerLink>,
	bus_id: u64,
}

impl Drop for MixerGainNodeInner {
	fn drop(&mut self) {
		self.mixer_link.send(MixerCommand::RemoveBus { bus_id: self.bus_id });
	}
}

// 버스의 GainNode 자리. (원본 AudioBus.getGainNode, 버스를 나눠 든다)
pub type AudioGainNode = Rc<MixerGainNode>;
// 플레이어가 소리를 내보낼 곳. (원본 AudioPlayer 생성자의 outputNode 자리, 그 버스로 낸다. AudioGainNode 의 as_deref)
pub type AudioOutputNode<'a> = &'a MixerGainNode;

// 데스크탑에는 자동재생 정책이 없다.
pub const HAS_USER_GESTURE_AT_START: bool = true;
// 대역 제한 표의 한 옥타브를 나누는 구간 수. (크롬 PeriodicWave 의 kNumberOfOctaveBands)
const PERIODIC_WAVE_OCTAVE_BAND_COUNT: f64 = 3.0;
// 한 옥타브의 센트.
const CENTS_PER_OCTAVE: f64 = 1200.0;

//==============================================================================
// 믹서로 가는 길. (명령을 보내고 시계를 읽는다, 노드 번호를 매긴다)
//==============================================================================
struct MixerLink {
	command_sender: Sender<MixerCommand>,
	shared: Arc<MixerShared>,
	next_node_id: Cell<u64>,
}

impl MixerLink {
	fn send(&self, command: MixerCommand) {
		let _ = self.command_sender.send(command);
	}

	fn create_node_id(&self) -> u64 {
		let node_id = self.next_node_id.get() + 1;
		self.next_node_id.set(node_id);
		node_id
	}

	fn get_current_time(&self) -> f64 {
		self.shared.get_rendered_seconds()
	}
}

//==============================================================================
// 매니저가 드는 백엔드 상태.
//==============================================================================
pub struct AudioPlatformState {
	// 소리 장치를 열지 못했는지. (한 번 실패하면 다시 열지 않는다)
	is_audio_context_failed: bool,
	// 컨텍스트를 멈춰 두었는지.
	is_audio_context_suspended: bool,
	// 믹서로 가는 길. (컨텍스트가 있을 때)
	mixer_link: Option<Rc<MixerLink>>,
}

//==============================================================================
// 백엔드 상태 생성.
//==============================================================================
pub fn create_audio_platform_state() -> AudioPlatformState {
	AudioPlatformState {
		is_audio_context_failed: false,
		is_audio_context_suspended: false,
		mixer_link: None,
	}
}

//==============================================================================
// 오디오 컨텍스트 생성. (cpal 기본 출력 장치를 그 기본 설정으로 열고 믹서를 소리 스레드에 넘긴다)
//==============================================================================
pub fn create_audio_context(audio_platform_state: &mut AudioPlatformState) -> Option<AudioContext> {
	if audio_platform_state.is_audio_context_failed {
		return None;
	}
	let (command_sender, command_receiver) = channel();
	let shared = Arc::new(MixerShared::new(0.0));
	match StreamManager::start(command_receiver, shared.clone()) {
		Ok(stream_manager) => {
			let mixer_link = Rc::new(MixerLink {
				command_sender,
				shared,
				next_node_id: Cell::new(0),
			});
			audio_platform_state.is_audio_context_suspended = false;
			audio_platform_state.mixer_link = Some(mixer_link.clone());
			Some(AudioContext {
				_stream_manager: stream_manager,
				mixer_link,
			})
		}
		Err(error) => {
			eprintln!("AudioManager: 소리 장치를 열지 못했습니다. 소리 없이 계속합니다. ({})", error);
			audio_platform_state.is_audio_context_failed = true;
			None
		}
	}
}

// 장치를 살피는 간격. (빠지거나 바뀐 장치로 스트림을 다시 연다)
const STREAM_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

//==============================================================================
// 출력 스트림 관리자. (스트림을 든 스레드, 장치가 빠지면(그리고 맥이 아니면 기본 장치가 바뀌면) 같은 믹서로 스트림을 다시 연다)
// - 원본 크롬도 기본 장치를 따라간다. 컨텍스트를 버리면 스레드가 스트림을 닫고 끝난다.
//==============================================================================
struct StreamManager {
	should_stop: Arc<AtomicBool>,
}

impl StreamManager {
	//==============================================================================
	// 시작. (스레드에서 첫 스트림을 열고 그 결과를 기다린다)
	//==============================================================================
	fn start(command_receiver: std::sync::mpsc::Receiver<MixerCommand>, shared: Arc<MixerShared>) -> Result<StreamManager, String> {
		let should_stop = Arc::new(AtomicBool::new(false));
		let thread_should_stop = should_stop.clone();
		let (result_sender, result_receiver) = channel::<Result<(), String>>();
		std::thread::Builder::new().name(String::from("vanilla-audio-stream")).spawn(move || {
			let renderer = Arc::new(std::sync::Mutex::new(MixerRenderer::new(command_receiver, shared, 0.0)));
			let needs_restart = Arc::new(AtomicBool::new(false));
			let mut running_stream = match open_output_stream(&renderer, &needs_restart) {
				Ok(running_stream) => {
					let _ = result_sender.send(Ok(()));
					running_stream
				}
				Err(error) => {
					let _ = result_sender.send(Err(error));
					return;
				}
			};
			loop {
				std::thread::sleep(STREAM_CHECK_INTERVAL);
				if thread_should_stop.load(Ordering::Acquire) {
					break;
				}
				let should_reopen = needs_restart.swap(false, Ordering::AcqRel) || has_default_device_changed(&running_stream);
				if should_reopen {
					drop(running_stream);
					match open_output_stream(&renderer, &needs_restart) {
						Ok(reopened_stream) => {
							running_stream = reopened_stream;
						}
						Err(error) => {
							eprintln!("AudioManager: 소리 장치를 다시 열지 못했습니다. ({})", error);
							// 다음 간격에 다시 해 본다.
							needs_restart.store(true, Ordering::Release);
							running_stream = RunningStream::empty();
						}
					}
				}
			}
		}).map_err(|error| format!("{}", error))?;
		result_receiver.recv().map_err(|error| format!("{}", error))??;
		Ok(StreamManager {
			should_stop,
		})
	}
}

impl Drop for StreamManager {
	fn drop(&mut self) {
		self.should_stop.store(true, Ordering::Release);
	}
}

//==============================================================================
// 열린 스트림. (장치 이름표와 표본율, 스트림을 버리면 닫힌다)
//==============================================================================
struct RunningStream {
	_stream: Option<cpal::Stream>,
	#[cfg_attr(target_os = "macos", allow(dead_code))]
	device_id: String,
	#[cfg_attr(target_os = "macos", allow(dead_code))]
	sample_rate: u32,
}

impl RunningStream {
	fn empty() -> RunningStream {
		RunningStream {
			_stream: None,
			device_id: String::new(),
			sample_rate: 0,
		}
	}
}

//==============================================================================
// 기본 출력 장치가 바뀌었는지. (이름표나 표본율, 맥은 재생 중에 장치를 물으면 소리가 튀어 묻지 않고 빠짐 오류로만 다시 연다)
//==============================================================================
#[cfg(not(target_os = "macos"))]
fn has_default_device_changed(running_stream: &RunningStream) -> bool {
	let Some(device) = cpal::default_host().default_output_device() else {
		return false;
	};
	let Ok(supported_config) = device.default_output_config() else {
		return false;
	};
	read_device_id(&device) != running_stream.device_id || supported_config.sample_rate() != running_stream.sample_rate
}

#[cfg(target_os = "macos")]
fn has_default_device_changed(_running_stream: &RunningStream) -> bool {
	false
}

fn read_device_id(device: &cpal::Device) -> String {
	device.id().map(|device_id| format!("{}", device_id)).unwrap_or_default()
}

//==============================================================================
// 기본 출력 장치를 열어 스트림 짓기. (장치의 표본 꼴마다 실수 믹서 출력을 그 꼴로 바꿔 낸다, 표본율이 바뀌었으면 믹서에 알린다)
//==============================================================================
fn open_output_stream(renderer: &Arc<std::sync::Mutex<MixerRenderer>>, needs_restart: &Arc<AtomicBool>) -> Result<RunningStream, String> {
	let host = cpal::default_host();
	let device = host.default_output_device().ok_or_else(|| String::from("기본 출력 장치가 없습니다"))?;
	let supported_config = device.default_output_config().map_err(|error| format!("{}", error))?;
	let sample_format = supported_config.sample_format();
	let stream_config = supported_config.config();
	let sample_rate = stream_config.sample_rate;
	if let Ok(mut renderer) = renderer.lock() {
		renderer.set_sample_rate(sample_rate as f64);
	}
	let stream = match sample_format {
		cpal::SampleFormat::F32 => build_output_stream::<f32>(&device, stream_config, renderer, needs_restart),
		cpal::SampleFormat::F64 => build_output_stream::<f64>(&device, stream_config, renderer, needs_restart),
		cpal::SampleFormat::I16 => build_output_stream::<i16>(&device, stream_config, renderer, needs_restart),
		cpal::SampleFormat::I32 => build_output_stream::<i32>(&device, stream_config, renderer, needs_restart),
		cpal::SampleFormat::U16 => build_output_stream::<u16>(&device, stream_config, renderer, needs_restart),
		cpal::SampleFormat::U8 => build_output_stream::<u8>(&device, stream_config, renderer, needs_restart),
		cpal::SampleFormat::I8 => build_output_stream::<i8>(&device, stream_config, renderer, needs_restart),
		other => {
			return Err(format!("표본 꼴 {} 은 받지 않습니다", other));
		}
	}?;
	stream.play().map_err(|error| format!("{}", error))?;
	Ok(RunningStream {
		_stream: Some(stream),
		device_id: read_device_id(&device),
		sample_rate,
	})
}

//==============================================================================
// 꼴 하나로 스트림 짓기. (믹서가 실수로 채운 것을 장치 꼴로 바꾼다. 장치가 빠지면 다시 열라고 알리고, 그 밖의 오류는 한 번만 알린다)
// - 믹서는 스트림을 다시 열 때만 잠깐 잠긴다. 소리 스레드는 기다리지 않고 그 동안 0 을 낸다.
//==============================================================================
fn build_output_stream<T>(device: &cpal::Device, stream_config: cpal::StreamConfig, renderer: &Arc<std::sync::Mutex<MixerRenderer>>, needs_restart: &Arc<AtomicBool>) -> Result<cpal::Stream, String>
where
	T: cpal::SizedSample + cpal::FromSample<f32>,
{
	let channel_count = stream_config.channels as usize;
	let mut float_output: Vec<f32> = Vec::new();
	let is_error_reported = Arc::new(AtomicBool::new(false));
	let callback_renderer = renderer.clone();
	let error_needs_restart = needs_restart.clone();
	device.build_output_stream::<T, _, _>(
		stream_config,
		move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
			if float_output.len() < output.len() {
				float_output.resize(output.len(), 0.0);
			}
			let float_slice = &mut float_output[..output.len()];
			match callback_renderer.try_lock() {
				Ok(mut renderer) => {
					renderer.render_interleaved(float_slice, channel_count);
				}
				Err(_) => {
					float_slice.fill(0.0);
				}
			}
			for (sample, value) in output.iter_mut().zip(float_slice.iter()) {
				*sample = T::from_sample(*value);
			}
		},
		move |error| {
			match error.kind() {
				cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::StreamInvalidated => {
					error_needs_restart.store(true, Ordering::Release);
				}
				_ => {
					if !is_error_reported.swap(true, Ordering::Relaxed) {
						eprintln!("AudioManager: 소리 장치 오류. ({})", error);
					}
				}
			}
		},
		None,
	).map_err(|error| format!("{}", error))
}

//==============================================================================
// 오디오 컨텍스트 재개. (suspend_audio_context 로 멈춰 둔 것만 다시 돌린다)
//==============================================================================
pub fn resume_audio_context(state_cell: &Rc<RefCell<AudioManagerState>>) {
	let state = &mut *state_cell.borrow_mut();
	let platform_state = &mut state.platform_state;
	if state.audio_context.is_none() || !platform_state.is_audio_context_suspended {
		return;
	}
	if let Some(mixer_link) = &platform_state.mixer_link {
		mixer_link.send(MixerCommand::Resume);
	}
	platform_state.is_audio_context_suspended = false;
}

//==============================================================================
// 오디오 컨텍스트 멈춤. (돌고 있을 때만 그리기와 시계를 세운다, 원본 audio_context.suspend() 자리)
//==============================================================================
pub fn suspend_audio_context(state_cell: &Rc<RefCell<AudioManagerState>>) {
	let state = &mut *state_cell.borrow_mut();
	let platform_state = &mut state.platform_state;
	if state.audio_context.is_none() || platform_state.is_audio_context_suspended {
		return;
	}
	if let Some(mixer_link) = &platform_state.mixer_link {
		mixer_link.send(MixerCommand::Suspend);
	}
	platform_state.is_audio_context_suspended = true;
}

//==============================================================================
// 재생하기 전에 멈춘 컨텍스트 깨우기. (원본 AudioPlayer.play 가 "suspended" 면 resume() 뒤에 트는 자리, 데스크탑은 그 자리에서 돈다)
//==============================================================================
pub fn resume_audio_context_before_playback(state_cell: &Rc<RefCell<AudioManagerState>>) {
	resume_audio_context(state_cell);
}

//==============================================================================
// 오디오 컨텍스트가 있고 돌고 있는지. (멈춰 둔 동안은 거짓)
//==============================================================================
pub fn is_audio_context_running(state: &AudioManagerState) -> bool {
	state.audio_context.is_some() && !state.platform_state.is_audio_context_suspended
}

//==============================================================================
// 오디오 컨텍스트 시계. (초, 원본 audio_context.currentTime 자리, 그린 프레임 수 / 표본율, 없으면 0)
//==============================================================================
pub fn get_audio_context_time(state: &AudioManagerState) -> f64 {
	state.platform_state.mixer_link.as_ref().map(|mixer_link| mixer_link.get_current_time()).unwrap_or(0.0)
}

//==============================================================================
// 플레이어 하나의 소리 내는 부분. (믹서의 플레이어 노드, 지금 소스의 세대)
//==============================================================================
pub struct AudioPlayerBackend {
	mixer_link: Option<Rc<MixerLink>>,
	player_id: u64,
	// 내보내는 버스. (원본처럼 이어진 동안 버스를 살려 둔다)
	_output_gain_node: Option<Rc<MixerGainNodeInner>>,
	// 시작한 소스의 세대와 믹서가 끝냈다고 알린 세대.
	started_generation: u64,
	ended_generation: Arc<AtomicU64>,
	// GainNode 의 gain 내재 값. (원본 gain.value 가 읽는 값, 소리 스레드가 퀀텀마다 쓴다)
	gain_value: Arc<AtomicU32>,
	is_source_active: bool,
	playback_rate: f64,
}

//==============================================================================
// 자동화를 믹서의 AudioParam 호출로.
//==============================================================================
fn to_param_automation(automation: GainAutomation) -> ParamAutomation {
	match automation {
		GainAutomation::SetValueAtTime { value, time } => {
			ParamAutomation::SetValueAtTime { value, time }
		}
		GainAutomation::LinearRampToValueAtTime { value, time } => {
			ParamAutomation::LinearRampToValueAtTime { value, time }
		}
		GainAutomation::ExponentialRampToValueAtTime { value, time } => {
			ParamAutomation::ExponentialRampToValueAtTime { value, time }
		}
		GainAutomation::SetTargetAtTime { target, time, time_constant } => {
			ParamAutomation::SetTargetAtTime { target, time, time_constant }
		}
		GainAutomation::CancelScheduledValues { time } => {
			ParamAutomation::CancelScheduledValues { time }
		}
	}
}

impl AudioPlayerBackend {
	//==============================================================================
	// 생성. (내보낼 버스를 주면 그 버스로, 없으면 출력으로. 컨텍스트가 없으면 소리 없는 플레이어)
	//==============================================================================
	pub fn create(state: &mut AudioManagerState, output_node: Option<AudioOutputNode<'_>>) -> AudioPlayerBackend {
		let output_gain_node = output_node.map(|output_node| output_node.inner.clone());
		let mixer_link = match &output_gain_node {
			Some(output_gain_node) => {
				Some(output_gain_node.mixer_link.clone())
			}
			None => {
				state.platform_state.mixer_link.clone()
			}
		};
		AudioPlayerBackend::create_with_output(mixer_link, output_gain_node)
	}

	//==============================================================================
	// 소리 없는 플레이어 생성. (시험)
	//==============================================================================
	#[cfg(test)]
	pub fn create_silent() -> AudioPlayerBackend {
		AudioPlayerBackend::create_with_output(None, None)
	}

	//==============================================================================
	// 믹서에 플레이어 노드를 더하고 생성. (믹서가 없으면 소리 없는 플레이어)
	//==============================================================================
	fn create_with_output(mixer_link: Option<Rc<MixerLink>>, output_gain_node: Option<Rc<MixerGainNodeInner>>) -> AudioPlayerBackend {
		let ended_generation = Arc::new(AtomicU64::new(0));
		let gain_value = Arc::new(AtomicU32::new(1.0f32.to_bits()));
		let player_id = match &mixer_link {
			Some(mixer_link) => {
				let player_id = mixer_link.create_node_id();
				mixer_link.send(MixerCommand::AddPlayer {
					player_id,
					output_bus_id: output_gain_node.as_ref().map(|output_gain_node| output_gain_node.bus_id),
					ended_generation: ended_generation.clone(),
					gain_value: gain_value.clone(),
				});
				player_id
			}
			None => {
				0
			}
		};
		AudioPlayerBackend {
			mixer_link,
			player_id,
			_output_gain_node: output_gain_node,
			started_generation: 0,
			ended_generation,
			gain_value,
			is_source_active: false,
			playback_rate: 1.0,
		}
	}

	fn send(&self, command: MixerCommand) {
		if let Some(mixer_link) = &self.mixer_link {
			mixer_link.send(command);
		}
	}

	//==============================================================================
	// 소리를 낼 수 있는지. (원본 audio_context && gain_node)
	//==============================================================================
	pub fn is_available(&self) -> bool {
		self.mixer_link.is_some()
	}

	//==============================================================================
	// 재생 중인 소스를 멈춘다. (원본 source.stop())
	//==============================================================================
	pub fn stop_source(&mut self) {
		if self.is_source_active {
			self.send(MixerCommand::StopSource { player_id: self.player_id, time: 0.0 });
			self.is_source_active = false;
		}
	}

	//==============================================================================
	// 그 시각에 소스를 멈춘다. (원본 source.stop(when), 끝나면 is_playing 이 거짓이 된다)
	//==============================================================================
	pub fn stop_source_at_time(&mut self, time: f64) {
		if self.is_source_active {
			self.send(MixerCommand::StopSource { player_id: self.player_id, time });
		}
	}

	//==============================================================================
	// 정지.
	//==============================================================================
	pub fn stop(&mut self) {
		self.stop_source();
	}

	//==============================================================================
	// 재생 시작. (버퍼를 time 초부터, 루프면 버퍼 전체를 되풀이. 원본 createBufferSource + start(0, time))
	//==============================================================================
	pub fn start_playback(&mut self, audio_buffer: &AudioBuffer, is_loop: bool, time: f64) {
		if self.mixer_link.is_none() {
			return;
		}
		self.started_generation += 1;
		self.send(MixerCommand::StartSource {
			player_id: self.player_id,
			audio_buffer: audio_buffer.data.clone(),
			is_loop,
			offset_seconds: time,
			playback_rate: self.playback_rate,
			generation: self.started_generation,
		});
		self.is_source_active = true;
	}

	//==============================================================================
	// 재생 빠르기 설정. (1 이면 그대로, 음 높이도 같이 바뀐다, 다음 재생부터)
	//==============================================================================
	pub fn set_playback_rate(&mut self, playback_rate: f64) {
		self.playback_rate = playback_rate;
	}

	//==============================================================================
	// 좌우 설정. (-1 왼쪽 ~ 1 오른쪽, 0 이 아니면 GainNode 와 내보낼 곳 사이에 StereoPannerNode 를 끼운다. 원본 SoundSystem.playEffect, 웹 백엔드와 같다)
	//==============================================================================
	pub fn set_pan(&mut self, pan: f32) {
		self.send(MixerCommand::SetPlayerPan { player_id: self.player_id, pan });
	}

	//==============================================================================
	// 음량 설정. (GainNode 의 값, 0 ~ 1)
	//==============================================================================
	pub fn set_gain(&mut self, gain: f32) {
		// 원본 gain.value 대입: 읽는 값은 바로 바뀌고 믹서에는 setValueAtTime(값, 지금)이 들어간다.
		self.gain_value.store(gain.to_bits(), Ordering::Release);
		self.send(MixerCommand::PlayerGainAutomation { player_id: self.player_id, automation: ParamAutomation::SetValue { value: gain } });
	}

	//==============================================================================
	// 음량 자동화. (원본 gain.setValueAtTime 들)
	//==============================================================================
	pub fn apply_gain_automation(&mut self, automation: GainAutomation) {
		self.send(MixerCommand::PlayerGainAutomation { player_id: self.player_id, automation: to_param_automation(automation) });
	}

	//==============================================================================
	// 지금 음량. (원본 gain.value 읽기)
	//==============================================================================
	pub fn get_gain_value(&self) -> f32 {
		f32::from_bits(self.gain_value.load(Ordering::Acquire))
	}

	//==============================================================================
	// 재생 중인지. (시작한 소스를 믹서가 아직 끝내지 않았으면 참, 원본 onended 전)
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.is_source_active && self.ended_generation.load(Ordering::Acquire) < self.started_generation
	}
}

impl Drop for AudioPlayerBackend {
	fn drop(&mut self) {
		self.send(MixerCommand::RemovePlayer { player_id: self.player_id });
	}
}

//==============================================================================
// 버스 하나의 소리 모으는 부분. (출력에 이은 GainNode)
//==============================================================================
pub struct AudioBusBackend {
	gain_node: Option<AudioGainNode>,
}

impl AudioBusBackend {
	//==============================================================================
	// 생성. (컨텍스트가 있으면 믹서에 버스 노드를 더한다. 없으면 소리 없는 버스)
	//==============================================================================
	pub fn create(state: &mut AudioManagerState) -> AudioBusBackend {
		let gain_node = state.platform_state.mixer_link.as_ref().map(|mixer_link| {
			let bus_id = mixer_link.create_node_id();
			mixer_link.send(MixerCommand::AddBus { bus_id });
			Rc::new(MixerGainNode {
				inner: Rc::new(MixerGainNodeInner {
					mixer_link: mixer_link.clone(),
					bus_id,
				}),
			})
		});
		AudioBusBackend {
			gain_node,
		}
	}

	//==============================================================================
	// 소리를 모을 수 있는지. (원본 audio_context && gain_node)
	//==============================================================================
	pub fn is_available(&self) -> bool {
		self.gain_node.is_some()
	}

	//==============================================================================
	// 음량 설정. (버스 GainNode 의 값, 0 ~ 1)
	//==============================================================================
	pub fn set_gain(&mut self, gain: f32) {
		if let Some(gain_node) = &self.gain_node {
			gain_node.inner.mixer_link.send(MixerCommand::BusGainAutomation { bus_id: gain_node.inner.bus_id, automation: ParamAutomation::SetValue { value: gain } });
		}
	}

	//==============================================================================
	// 음량 자동화. (원본 SoundSystem 이 버스 gain 에 거는 setTargetAtTime 들)
	//==============================================================================
	pub fn apply_gain_automation(&mut self, automation: GainAutomation) {
		if let Some(gain_node) = &self.gain_node {
			gain_node.inner.mixer_link.send(MixerCommand::BusGainAutomation { bus_id: gain_node.inner.bus_id, automation: to_param_automation(automation) });
		}
	}

	//==============================================================================
	// 이 버스로 트는 플레이어 백엔드 생성. (버스가 소리 없으면 소리 없는 플레이어)
	//==============================================================================
	pub fn create_player_backend(&mut self) -> AudioPlayerBackend {
		match &self.gain_node {
			Some(gain_node) => {
				AudioPlayerBackend::create_with_output(Some(gain_node.inner.mixer_link.clone()), Some(gain_node.inner.clone()))
			}
			None => {
				AudioPlayerBackend::create_with_output(None, None)
			}
		}
	}

	//==============================================================================
	// 버스 GainNode. (원본 getGainNode 자리, 소리 없는 버스는 None)
	//==============================================================================
	pub fn get_gain_node(&self) -> Option<&AudioGainNode> {
		self.gain_node.as_ref()
	}
}

//==============================================================================
// 파일 읽기. (그 자리에서 읽고 디코딩한다, 읽은 파일은 리소스 사용량에 적는다)
//==============================================================================
pub fn load_audio_file(audio_asset: &Rc<AudioAsset>, asset_path: &Path) -> Result<(), String> {
	let decode_result = std::fs::read(asset_path).map_err(|error| format!("{}", error)).and_then(|bytes| {
		crate::base::platform::record_loaded_file(&asset_path.to_string_lossy(), bytes.len() as u64);
		create_audio_buffer(&bytes)
	});
	match decode_result {
		Ok(audio_buffer) => {
			audio_asset.set_audio_buffer(audio_buffer);
			Ok(())
		}
		Err(error) => {
			let load_error = format!("Error loading sound: {} ({})", asset_path.display(), error);
			audio_asset.set_load_error(load_error.clone());
			Err(load_error)
		}
	}
}

//==============================================================================
// 메모리에서 읽기. (그 자리에서 디코딩한다)
//==============================================================================
pub fn load_audio_bytes(audio_asset: &Rc<AudioAsset>, bytes: Vec<u8>) -> Result<(), String> {
	match create_audio_buffer(&bytes) {
		Ok(audio_buffer) => {
			audio_asset.set_audio_buffer(audio_buffer);
			Ok(())
		}
		Err(error) => {
			let load_error = format!("Error loading sound from memory ({})", error);
			audio_asset.set_load_error(load_error.clone());
			Err(load_error)
		}
	}
}

//==============================================================================
// 소리 파일 바이트를 버퍼로. (decode 가 크롬 decodeAudioData 처럼 푼 1 채널이나 2 채널)
//==============================================================================
fn create_audio_buffer(bytes: &[u8]) -> Result<AudioBuffer, String> {
	let decoded_audio = super::decode::decode_audio_data(bytes)?;
	Ok(AudioBuffer {
		data: Arc::new(AudioBufferData {
			sample_rate: decoded_audio.sample_rate as f64,
			channel_list: decoded_audio.channel_data,
		}),
	})
}

//==============================================================================
// 버퍼의 재생 시간. (초)
//==============================================================================
pub fn get_audio_buffer_duration(audio_buffer: &AudioBuffer) -> f64 {
	audio_buffer.duration()
}

//==============================================================================
// 비프 톤 하나 재생. (컨텍스트 표본율로 그린 표본을 출력에, 지연은 프레임으로. 원본 osc.start(now + delay))
//==============================================================================
pub fn play_beep_tone(state: &mut AudioManagerState, beep_envelope: &BeepEnvelope) {
	let Some(mixer_link) = &state.platform_state.mixer_link else {
		return;
	};
	let sample_rate = mixer_link.shared.get_sample_rate();
	let sample_list = render_beep_samples(beep_envelope, sample_rate);
	let delay_frame_count = (beep_envelope.delay_seconds.max(0.0) * sample_rate).round() as u64;
	mixer_link.send(MixerCommand::PlayBeep { sample_list, delay_frame_count });
}

//==============================================================================
// 비프 톤을 표본으로 그린다. (원본 오실레이터가 도는 길이 = 엔벨로프 길이 + 0.01초, 모노)
//==============================================================================
fn render_beep_samples(beep_envelope: &BeepEnvelope, sample_rate: f64) -> Vec<f32> {
	let total_seconds = beep_envelope.duration_seconds + BEEP_STOP_MARGIN_SECONDS;
	let frame_count = (total_seconds * sample_rate).ceil() as usize;
	let phase_step = beep_envelope.frequency as f64 / sample_rate;
	let band_limited_waveform = BandLimitedWaveform::create(beep_envelope.waveform, beep_envelope.frequency as f64, sample_rate);
	let mut phase = 0.0f64;
	let mut sample_list = Vec::with_capacity(frame_count);
	for frame_index in 0..frame_count {
		let elapsed_seconds = frame_index as f64 / sample_rate;
		let gain = get_beep_envelope_gain(beep_envelope, elapsed_seconds);
		sample_list.push(band_limited_waveform.read_sample(phase) * gain);
		phase += phase_step;
		phase -= phase.floor();
	}
	sample_list
}

//==============================================================================
// 엔벨로프 음량. (원본 GainNode 자동화와 같은 값, elapsed_seconds 는 톤 시작 기준)
// - 0 ~ attack: 바닥에서 최고까지 선형, attack ~ release 시작: 최고 유지, release 시작 ~ 끝: 바닥까지 지수, 그 뒤 바닥.
//==============================================================================
fn get_beep_envelope_gain(beep_envelope: &BeepEnvelope, elapsed_seconds: f64) -> f32 {
	let floor_volume = BEEP_FLOOR_VOLUME as f64;
	let peak_volume = beep_envelope.peak_volume as f64;
	let release_start_seconds = beep_envelope.get_release_start_seconds();
	if elapsed_seconds < beep_envelope.attack_seconds {
		let progress = elapsed_seconds / beep_envelope.attack_seconds;
		return (floor_volume + (peak_volume - floor_volume) * progress) as f32;
	}
	if elapsed_seconds < release_start_seconds {
		return peak_volume as f32;
	}
	if elapsed_seconds < beep_envelope.duration_seconds {
		let progress = (elapsed_seconds - release_start_seconds) / (beep_envelope.duration_seconds - release_start_seconds);
		return (peak_volume * (floor_volume / peak_volume).powf(progress)) as f32;
	}
	floor_volume as f32
}

//==============================================================================
// 대역 제한 표 크기. (크롬 PeriodicWave: 샘플레이트 24000 이하 2048, 88200 이하 4096, 그 위 16384)
//==============================================================================
fn get_periodic_wave_size(sample_rate: f64) -> usize {
	if sample_rate <= 24000.0 {
		return 2048;
	}
	if sample_rate <= 88200.0 {
		return 4096;
	}
	16384
}

//==============================================================================
// 파형의 푸리에 계수. (사인 항 b[harmonic_index], 크롬 GenerateBasicWaveform 과 같은 식, 0 번은 0)
//==============================================================================
fn get_fourier_coefficient(beep_waveform: BeepWaveform, harmonic_index: usize) -> f64 {
	if harmonic_index == 0 {
		return 0.0;
	}
	let is_odd = harmonic_index % 2 == 1;
	let pi_factor = 2.0 / (harmonic_index as f64 * std::f64::consts::PI);
	match beep_waveform {
		BeepWaveform::Sine => {
			if harmonic_index == 1 {
				1.0
			}
			else {
				0.0
			}
		}
		BeepWaveform::Square => {
			if is_odd {
				2.0 * pi_factor
			}
			else {
				0.0
			}
		}
		BeepWaveform::Sawtooth => {
			if is_odd {
				pi_factor
			}
			else {
				-pi_factor
			}
		}
		BeepWaveform::Triangle => {
			if !is_odd {
				return 0.0;
			}
			let sign = if ((harmonic_index - 1) / 2) % 2 == 1 {
				-1.0
			}
			else {
				1.0
			};
			2.0 * pi_factor * pi_factor * sign
		}
	}
}

//==============================================================================
// 정규화 배율. (배음을 다 둔 첫 표의 표 칸 값 가운데 가장 큰 크기의 역수, 크롬 normalization_scale, 파형과 표 크기마다 한 번 셈해 둔다)
// - 네 파형은 모두 f(1 - x) = -f(x) 라서 표의 앞 반과 가운데만 본다.
//==============================================================================
fn get_normalization_scale(beep_waveform: BeepWaveform, periodic_wave_size: usize) -> f64 {
	thread_local! {
		static NORMALIZATION_SCALE_LIST: RefCell<Vec<(BeepWaveform, usize, f64)>> = const { RefCell::new(Vec::new()) };
	}
	let cached_scale = NORMALIZATION_SCALE_LIST.with(|normalization_scale_list| {
		normalization_scale_list.borrow().iter().find(|(cached_waveform, cached_size, _)| *cached_waveform == beep_waveform && *cached_size == periodic_wave_size).map(|(_, _, cached_scale)| *cached_scale)
	});
	if let Some(cached_scale) = cached_scale {
		return cached_scale;
	}
	let half_size = periodic_wave_size / 2;
	let index_mask = periodic_wave_size - 1;
	let sine_table: Vec<f64> = (0..periodic_wave_size).map(|table_index| (std::f64::consts::TAU * table_index as f64 / periodic_wave_size as f64).sin()).collect();
	let harmonic_list: Vec<(usize, f64)> = (1..half_size).map(|harmonic_index| (harmonic_index, get_fourier_coefficient(beep_waveform, harmonic_index))).filter(|(_, coefficient)| *coefficient != 0.0).collect();
	let mut maximum_value = 0.0f64;
	for table_index in 0..=half_size {
		let mut value = 0.0f64;
		for (harmonic_index, coefficient) in harmonic_list.iter() {
			value += coefficient * sine_table[(harmonic_index * table_index) & index_mask];
		}
		maximum_value = maximum_value.max(value.abs());
	}
	let normalization_scale = if maximum_value > 0.0 {
		1.0 / maximum_value
	}
	else {
		1.0
	};
	NORMALIZATION_SCALE_LIST.with(|normalization_scale_list| {
		normalization_scale_list.borrow_mut().push((beep_waveform, periodic_wave_size, normalization_scale));
	});
	normalization_scale
}

//==============================================================================
// 대역을 제한한 파형 하나. (주파수 하나에 맞춘 크롬 OscillatorNode 의 두 표 섞기)
//==============================================================================
struct BandLimitedWaveform {
	// 배음을 더 둔 표의 계수. (1 번부터, 배음을 덜 둔 표는 앞의 lower_partial_count 개)
	coefficient_list: Vec<f64>,
	lower_partial_count: usize,
	// 두 표를 섞는 비. (0 이면 배음을 더 둔 표, 1 이면 덜 둔 표)
	table_interpolation_factor: f64,
	normalization_scale: f64,
}

impl BandLimitedWaveform {
	//==============================================================================
	// 생성. (크롬 WaveDataForFundamentalFrequency 처럼 주파수로 표 둘과 섞는 비를 고른다)
	// - 표 r 의 배음 수는 표 크기의 반 x 2^(-r/3) 을 내린 것이다. (표 크기의 반 - 1 을 넘지 않는다)
	//==============================================================================
	fn create(beep_waveform: BeepWaveform, frequency: f64, sample_rate: f64) -> BandLimitedWaveform {
		let periodic_wave_size = get_periodic_wave_size(sample_rate);
		let half_size = periodic_wave_size / 2;
		let range_count = (PERIODIC_WAVE_OCTAVE_BAND_COUNT * (periodic_wave_size as f64).log2()).round() as usize;
		let cents_per_range = CENTS_PER_OCTAVE / PERIODIC_WAVE_OCTAVE_BAND_COUNT;
		let lowest_fundamental_frequency = sample_rate / periodic_wave_size as f64;
		let absolute_frequency = frequency.abs();
		let frequency_ratio = if absolute_frequency > 0.0 {
			absolute_frequency / lowest_fundamental_frequency
		}
		else {
			0.5
		};
		let cents_above_lowest_frequency = frequency_ratio.log2() * CENTS_PER_OCTAVE;
		let pitch_range = (1.0 + cents_above_lowest_frequency / cents_per_range).clamp(0.0, (range_count - 1) as f64);
		let higher_range_index = pitch_range as usize;
		let lower_range_index = if higher_range_index < range_count - 1 {
			higher_range_index + 1
		}
		else {
			higher_range_index
		};
		let read_partial_count = |range_index: usize| -> usize {
			let culling_scale = 2.0f64.powf(-(range_index as f64 * cents_per_range) / CENTS_PER_OCTAVE);
			let partial_count = (culling_scale * half_size as f64) as usize;
			partial_count.min(half_size - 1)
		};
		let higher_partial_count = read_partial_count(higher_range_index);
		let lower_partial_count = read_partial_count(lower_range_index);
		BandLimitedWaveform {
			coefficient_list: (1..=higher_partial_count).map(|harmonic_index| get_fourier_coefficient(beep_waveform, harmonic_index)).collect(),
			lower_partial_count,
			table_interpolation_factor: pitch_range - higher_range_index as f64,
			normalization_scale: get_normalization_scale(beep_waveform, periodic_wave_size),
		}
	}

	//==============================================================================
	// 파형 한 점. (phase: 0 ~ 1, 0 에서 시작해 위로 올라가는 꼴, 사인 배음은 점화식 sin(nθ) = 2cos(θ)sin((n-1)θ) - sin((n-2)θ) 로 잇는다)
	//==============================================================================
	fn read_sample(&self, phase: f64) -> f32 {
		let angle = phase * std::f64::consts::TAU;
		let doubled_cosine = 2.0 * angle.cos();
		let mut previous_sine = 0.0f64;
		let mut current_sine = angle.sin();
		let mut higher_sum = 0.0f64;
		let mut lower_sum = 0.0f64;
		for (coefficient_index, coefficient) in self.coefficient_list.iter().enumerate() {
			let term = coefficient * current_sine;
			higher_sum += term;
			if coefficient_index < self.lower_partial_count {
				lower_sum += term;
			}
			let next_sine = doubled_cosine * current_sine - previous_sine;
			previous_sine = current_sine;
			current_sine = next_sine;
		}
		let sample = ((1.0 - self.table_interpolation_factor) * higher_sum + self.table_interpolation_factor * lower_sum) * self.normalization_scale;
		sample as f32
	}

	//==============================================================================
	// 배음을 더 둔 표의 배음 수.
	//==============================================================================
	#[cfg(test)]
	fn get_higher_partial_count(&self) -> usize {
		self.coefficient_list.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 엔벨로프는 바닥에서 올라 최고를 지키다 바닥으로 내려간다.
	//==============================================================================
	#[test]
	fn beep_envelope_gain_follows_automation() {
		let beep_envelope = BeepEnvelope::create(880.0, 0.045, BeepWaveform::Sine, 0.0, 0.35, 1.0);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.0) - BEEP_FLOOR_VOLUME).abs() < 1e-7);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.0025) - (0.0001 + 0.3499 * 0.5)).abs() < 1e-5);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.01) - 0.35).abs() < 1e-6);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.024) - 0.35).abs() < 1e-6);
		let middle_gain = get_beep_envelope_gain(&beep_envelope, 0.035);
		assert!((middle_gain as f64 - (0.35f64 * 0.0001).sqrt()).abs() < 1e-5);
		assert!((get_beep_envelope_gain(&beep_envelope, 0.05) - BEEP_FLOOR_VOLUME).abs() < 1e-7);
	}

	//==============================================================================
	// 파형은 0 에서 시작해 위로 올라가고, 사인은 그대로 사인이다. 정규화 배율은 크롬처럼 배음을 다 둔 표의 가장 큰 값으로 맞춘다.
	// - 기대값은 같은 식(표 4096 칸, 배음 2047 개)을 파이썬으로 따로 셈한 것이다. (사각 1.17898, 톱니 1.17849, 삼각 0.99980)
	//==============================================================================
	#[test]
	fn band_limited_waveform_matches_periodic_wave() {
		let sample_rate = 44100.0;
		for beep_waveform in [BeepWaveform::Sine, BeepWaveform::Square, BeepWaveform::Sawtooth, BeepWaveform::Triangle] {
			let band_limited_waveform = BandLimitedWaveform::create(beep_waveform, 440.0, sample_rate);
			assert!(band_limited_waveform.read_sample(0.0).abs() < 1e-6);
			assert!(band_limited_waveform.read_sample(0.1) > 0.0);
		}
		let sine_waveform = BandLimitedWaveform::create(BeepWaveform::Sine, 880.0, sample_rate);
		for step_index in 0..16 {
			let phase = step_index as f64 / 16.0;
			assert!((sine_waveform.read_sample(phase) as f64 - (phase * std::f64::consts::TAU).sin()).abs() < 1e-6);
		}
		assert!((get_normalization_scale(BeepWaveform::Square, 4096) - 1.0 / 1.1789798).abs() < 1e-5);
		assert!((get_normalization_scale(BeepWaveform::Sawtooth, 4096) - 1.0 / 1.1784914).abs() < 1e-5);
		assert!((get_normalization_scale(BeepWaveform::Triangle, 4096) - 1.0 / 0.9998021).abs() < 1e-5);

		// 사각파의 가운데는 정규화 배율 근처, 삼각파의 꼭대기는 1 근처다.
		let square_waveform = BandLimitedWaveform::create(BeepWaveform::Square, 440.0, sample_rate);
		assert!((square_waveform.read_sample(0.25) - 0.848).abs() < 0.03);
		assert!((square_waveform.read_sample(0.75) + 0.848).abs() < 0.03);
		let triangle_waveform = BandLimitedWaveform::create(BeepWaveform::Triangle, 440.0, sample_rate);
		assert!((triangle_waveform.read_sample(0.25) - 1.0).abs() < 0.02);
	}

	//==============================================================================
	// 높은 배음은 나이퀴스트 아래에서 끊긴다. (크롬처럼 440 Hz 는 40 개와 32 개 사이, 880 Hz 는 20 개와 16 개 사이)
	//==============================================================================
	#[test]
	fn band_limited_waveform_culls_partials() {
		let sample_rate = 44100.0;
		let waveform_440 = BandLimitedWaveform::create(BeepWaveform::Sawtooth, 440.0, sample_rate);
		assert_eq!(waveform_440.get_higher_partial_count(), 40);
		assert_eq!(waveform_440.lower_partial_count, 32);
		let waveform_880 = BandLimitedWaveform::create(BeepWaveform::Square, 880.0, sample_rate);
		assert_eq!(waveform_880.get_higher_partial_count(), 20);
		assert_eq!(waveform_880.lower_partial_count, 16);
		for frequency in [60.0, 220.0, 1000.0, 3000.0, 8000.0] {
			let band_limited_waveform = BandLimitedWaveform::create(BeepWaveform::Sawtooth, frequency, sample_rate);
			assert!(band_limited_waveform.get_higher_partial_count() as f64 * frequency < sample_rate * 0.5);
		}
	}

	//==============================================================================
	// 그린 샘플 수는 (길이 + 0.01초) x 샘플레이트다.
	//==============================================================================
	#[test]
	fn render_beep_frames_length() {
		let beep_envelope = BeepEnvelope::create(440.0, 0.09, BeepWaveform::Square, 0.06, 0.35, 0.7);
		let frame_list = render_beep_samples(&beep_envelope, 44100.0);
		// (0.09 + 0.01 의 부동소수 오차로 마지막 한 샘플은 올림에 따라 달라진다)
		let expected_frame_count = (0.1f64 * 44100.0) as i64;
		assert!((frame_list.len() as i64 - expected_frame_count).abs() <= 1);
		// 대역을 제한한 사각파는 깁스 너울 꼭대기가 1 근처다.
		let peak_sample = frame_list.iter().map(|sample| sample.abs()).fold(0.0f32, f32::max);
		assert!((peak_sample - 0.35 * 0.7).abs() < 0.35 * 0.7 * 0.05);
	}
}

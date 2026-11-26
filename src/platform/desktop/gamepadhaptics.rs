//==============================================================================
//! 맥 게임패드 진동. (macOS 만)
//! - 원본에 없는 파일이다. 원본은 브라우저의 Gamepad.vibrationActuator 로 떨고, 러스트 판 데스크탑은 gilrs force feedback 으로 떤다.
//!   gilrs 의 macOS 구현은 force feedback 이 비어 있어(is_ff_supported 가 늘 false) 맥만 여기서 애플 GameController 와 CoreHaptics 로 떤다.
//! - GCController.controllers() → controller.haptics(GCDeviceHaptics) → createEngineWithLocality 로 CHHapticEngine 을 짓고,
//!   연속 이벤트(세기, 날카로움) 하나로 된 CHHapticPattern 의 플레이어를 바로 시작한다.
//!   - 무거운 모터(strong)는 왼손잡이(GCHapticsLocalityLeftHandle), 가벼운 모터(weak)는 오른손잡이(GCHapticsLocalityRightHandle) 엔진에 낸다.
//!     둘 다 대지 않는 패드는 GCHapticsLocalityHandles, 그것도 없으면 GCHapticsLocalityDefault 엔진 하나에 둘 중 큰 세기로 낸다.
//!   - 다시 부르면 앞 진동을 멈추고 새로 낸다. 세기나 길이가 0 이면 멈추기만 한다.
//!   - 연속 이벤트 하나는 30 초까지라 더 길면 30 초짜리를 이어 붙인다.
//!   - 엔진은 컨트롤러와 손잡이 자리마다 한 번 짓고 담아 둔다. 목록에서 사라진 컨트롤러의 엔진은 버린다.
//!     엔진이 바깥 까닭으로 멈추거나(stoppedHandler) 서버 오류에서 돌아오면(resetHandler) 표시만 해 두고 다음에 떨 때 다시 시작한다.
//!     시작이나 플레이어가 실패하면 엔진을 버리고 한 번 다시 지어 본다.
//! - gilrs 패드와 GCController 짝 맞추기: 이름(gilrs 의 OS 이름과 GCController.vendorName, 대소문자 무시)이 같은 것을 먼저 잇고,
//!   남은 것은 gilrs 의 연결 차례와 GCController.controllers() 의 차례로 잇는다. (그래서 컨트롤러가 하나면 이름이 달라도 그것을 쓴다)
//!   GameController 는 제조사, 제품 번호를 공개 API 로 주지 않아 번호로는 맞추지 않는다.
//! - objc2-game-controller 0.3.2 는 createEngineWithLocality 를 iOS, tvOS, visionOS 에서만 묶어 내서 맥은 msg_send 로 직접 부른다.
//!   (애플 문서는 GCDeviceHaptics 를 macOS 11 부터 쓸 수 있다고 적는다)
//! - GameController 객체는 메인 스레드에서 다룬다. 엔진 프레임(winit 이벤트 루프)이 메인 스레드라 그대로 부르고, 다른 스레드에서 부르면 건너뛴다.
//! - startWirelessControllerDiscovery 는 새 무선 패드를 짝지을 때 쓰는 것이라 부르지 않는다. 이미 붙은 패드는 controllers() 에 있다.
//!   GCController.shouldMonitorBackgroundEvents 는 입력을 넘기는 일에 대한 것이고 입력은 gilrs 가 읽으므로 건드리지 않는다.
//!
//! # 예제
//! 패드 하나를 0.2 초 떨고 멈추기 (패드가 있어야 해서 실행하지 않는다, 보통은 GamepadManager.vibrate 가 부른다)
//! ```rust,no_run
//! use vanilla::platform;
//!
//! let gamepad_name_list = vec![String::from("DualSense Wireless Controller")];
//! platform::vibrate_gamepads(&gamepad_name_list, 1.0, 0.5, 200);
//! platform::vibrate_gamepads(&gamepad_name_list, 0.0, 0.0, 0);
//! ```
//==============================================================================
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{msg_send, AllocAnyThread, MainThreadMarker};
use objc2_core_haptics::{CHHapticEngine, CHHapticEngineStoppedReason, CHHapticEvent, CHHapticEventParameter, CHHapticEventParameterIDHapticIntensity, CHHapticEventParameterIDHapticSharpness, CHHapticEventTypeHapticContinuous, CHHapticPattern, CHHapticPatternPlayer};
use objc2_foundation::NSArray;
use objc2_game_controller::{GCController, GCDevice, GCHapticsLocality, GCHapticsLocalityDefault, GCHapticsLocalityHandles, GCHapticsLocalityLeftHandle, GCHapticsLocalityRightHandle};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// 무거운 모터 자리의 날카로움. (낮을수록 묵직하게 떤다)
const STRONG_MOTOR_SHARPNESS: f32 = 0.3;
/// 가벼운 모터 자리의 날카로움. (높을수록 가볍고 또렷하게 떤다)
const WEAK_MOTOR_SHARPNESS: f32 = 0.7;
/// 연속 햅틱 이벤트 하나의 가장 긴 길이. (CoreHaptics 문서: 30 초)
const MAXIMUM_CONTINUOUS_EVENT_SECONDS: f64 = 30.0;

thread_local! {
	/// 컨트롤러마다 지은 진동 엔진. (메인 스레드에서만 쓴다)
	static CONTROLLER_HAPTICS_LIST: RefCell<Vec<ControllerHaptics>> = const { RefCell::new(Vec::new()) };
	/// 연결된 컨트롤러가 없다고 이미 알렸는지. (떨 때마다 같은 로그를 내지 않는다)
	static IS_EMPTY_CONTROLLER_LIST_LOGGED: RefCell<bool> = const { RefCell::new(false) };
}

//==============================================================================
/// 손잡이 자리 하나의 진동 엔진.
//==============================================================================
struct HapticMotor {
	/// 엔진의 손잡이 자리. (GCHapticsLocality 값)
	locality: &'static GCHapticsLocality,
	/// 지은 엔진. (아직 짓지 않았거나 버렸으면 None)
	engine: Option<Retained<CHHapticEngine>>,
	/// 엔진을 (다시) 시작해야 하는지. (멈춤, 리셋 처리기가 다른 스레드에서 켠다)
	is_start_needed: Arc<AtomicBool>,
	/// 지금 떨고 있는 플레이어. (다시 부르면 멈춘다)
	player: Option<Retained<ProtocolObject<dyn CHHapticPatternPlayer>>>,
}

//==============================================================================
/// 컨트롤러 하나의 진동 엔진들.
//==============================================================================
struct ControllerHaptics {
	controller: Retained<GCController>,
	/// 손잡이 자리마다 하나. (둘이면 0 이 무거운 모터인 왼쪽, 1 이 가벼운 모터인 오른쪽)
	motor_list: Vec<HapticMotor>,
}

//==============================================================================
/// gilrs 패드들과 GCController 들의 짝. (결과의 번호가 gilrs 패드, 값이 컨트롤러 번호)
/// - 이름이 같은 것을 먼저 잇고, 남은 것은 서로의 차례로 잇는다.
//==============================================================================
fn pair_gamepads_with_controllers(gamepad_name_list: &[String], controller_name_list: &[Option<String>]) -> Vec<Option<usize>> {
	let mut pair_list: Vec<Option<usize>> = vec![None; gamepad_name_list.len()];
	let mut is_controller_used_list = vec![false; controller_name_list.len()];
	for (gamepad_number, gamepad_name) in gamepad_name_list.iter().enumerate() {
		let trimmed_gamepad_name = gamepad_name.trim();
		if trimmed_gamepad_name.is_empty() {
			continue;
		}
		for (controller_number, controller_name) in controller_name_list.iter().enumerate() {
			if is_controller_used_list[controller_number] {
				continue;
			}
			let is_same_name = match controller_name {
				Some(controller_name) => {
					controller_name.trim().eq_ignore_ascii_case(trimmed_gamepad_name)
				}
				None => {
					false
				}
			};
			if is_same_name {
				pair_list[gamepad_number] = Some(controller_number);
				is_controller_used_list[controller_number] = true;
				break;
			}
		}
	}
	for pair in pair_list.iter_mut() {
		if pair.is_some() {
			continue;
		}
		match is_controller_used_list.iter().position(|is_used| !*is_used) {
			Some(controller_number) => {
				*pair = Some(controller_number);
				is_controller_used_list[controller_number] = true;
			}
			None => {
				break;
			}
		}
	}
	pair_list
}

//==============================================================================
/// gilrs 패드들과 짝인 컨트롤러를 떨기. (세기 0 ~ 1, 밀리초. 길이나 세기가 0 이면 멈춘다)
/// - gamepad_name_list 는 연결된 gilrs 패드의 OS 이름을 연결 차례로 둔 것이다.
///
/// # 인자
/// - `gamepad_name_list`: 떨 gilrs 패드의 OS 이름 목록. (연결 차례, GCController 와 짝을 맞추는 데 쓴다)
/// - `strong_magnitude`: 무거운 모터(왼쪽 손잡이) 세기. (0 ~ 1)
/// - `weak_magnitude`: 가벼운 모터(오른쪽 손잡이) 세기. (0 ~ 1)
/// - `duration_milliseconds`: 떠는 길이. (밀리초, 0 이면 멈추기만 한다)
//==============================================================================
pub fn vibrate_gamepads(gamepad_name_list: &[String], strong_magnitude: f32, weak_magnitude: f32, duration_milliseconds: u32) {
	if MainThreadMarker::new().is_none() {
		write_log("[GamepadManager] 맥 패드 진동은 메인 스레드에서만 낼 수 있어 건너뜁니다.");
		return;
	}
	let controller_list = read_controller_list();
	release_missing_controllers(&controller_list);
	if controller_list.is_empty() {
		if !gamepad_name_list.is_empty() {
			IS_EMPTY_CONTROLLER_LIST_LOGGED.with(|is_logged| {
				if !*is_logged.borrow() {
					*is_logged.borrow_mut() = true;
					write_log("[GamepadManager] GameController 에 연결된 컨트롤러가 없어 맥 패드 진동을 건너뜁니다.");
				}
			});
		}
		return;
	}
	IS_EMPTY_CONTROLLER_LIST_LOGGED.with(|is_logged| {
		*is_logged.borrow_mut() = false;
	});
	let controller_name_list: Vec<Option<String>> = controller_list.iter().map(|controller| unsafe { controller.vendorName() }.map(|vendor_name| vendor_name.to_string())).collect();
	let pair_list = pair_gamepads_with_controllers(gamepad_name_list, &controller_name_list);
	let seconds = duration_milliseconds as f64 / 1000.0;
	for controller_number in pair_list.into_iter().flatten() {
		let controller = &controller_list[controller_number];
		let controller_name = controller_name_list[controller_number].clone().unwrap_or_default();
		CONTROLLER_HAPTICS_LIST.with(|controller_haptics_list| {
			let mut controller_haptics_list = controller_haptics_list.borrow_mut();
			let list_index = match controller_haptics_list.iter().position(|controller_haptics| controller_haptics.controller == *controller) {
				Some(list_index) => {
					list_index
				}
				None => {
					match ControllerHaptics::create(controller) {
						Some(controller_haptics) => {
							controller_haptics_list.push(controller_haptics);
							controller_haptics_list.len() - 1
						}
						None => {
							return;
						}
					}
				}
			};
			controller_haptics_list[list_index].vibrate(&controller_name, strong_magnitude, weak_magnitude, seconds);
		});
	}
}

//==============================================================================
/// 연결이 끊긴 컨트롤러의 진동 엔진 버리기. (gilrs 가 패드 해제를 알릴 때 부른다)
//==============================================================================
pub fn release_disconnected_gamepad_haptics() {
	if MainThreadMarker::new().is_none() {
		return;
	}
	let controller_list = read_controller_list();
	release_missing_controllers(&controller_list);
}

//==============================================================================
/// 연결된 GCController 목록.
//==============================================================================
fn read_controller_list() -> Vec<Retained<GCController>> {
	let controller_array = unsafe { GCController::controllers() };
	controller_array.to_vec()
}

//==============================================================================
/// 목록에 없는 컨트롤러의 엔진 버리기.
//==============================================================================
fn release_missing_controllers(controller_list: &[Retained<GCController>]) {
	CONTROLLER_HAPTICS_LIST.with(|controller_haptics_list| {
		controller_haptics_list.borrow_mut().retain(|controller_haptics| controller_list.contains(&controller_haptics.controller));
	});
}

impl ControllerHaptics {
	//==============================================================================
	/// 생성. (진동을 대지 않는 컨트롤러면 None. 엔진은 처음 떨 때 짓는다)
	//==============================================================================
	fn create(controller: &Retained<GCController>) -> Option<ControllerHaptics> {
		let haptics = unsafe { controller.haptics() }?;
		let supported_locality_set = unsafe { haptics.supportedLocalities() };
		let left_handle_locality = unsafe { GCHapticsLocalityLeftHandle };
		let right_handle_locality = unsafe { GCHapticsLocalityRightHandle };
		let handles_locality = unsafe { GCHapticsLocalityHandles };
		let default_locality = unsafe { GCHapticsLocalityDefault };
		let locality_list: Vec<&'static GCHapticsLocality> = if supported_locality_set.containsObject(left_handle_locality) && supported_locality_set.containsObject(right_handle_locality) {
			vec![left_handle_locality, right_handle_locality]
		}
		else if supported_locality_set.containsObject(handles_locality) {
			vec![handles_locality]
		}
		else {
			vec![default_locality]
		};
		let motor_list = locality_list.into_iter().map(|locality| HapticMotor {
			locality,
			engine: None,
			is_start_needed: Arc::new(AtomicBool::new(true)),
			player: None,
		}).collect();
		Some(ControllerHaptics {
			controller: controller.clone(),
			motor_list,
		})
	}

	//==============================================================================
	/// 떨기. (손잡이 둘이면 나눠 내고, 하나면 큰 세기로 낸다)
	//==============================================================================
	fn vibrate(&mut self, controller_name: &str, strong_magnitude: f32, weak_magnitude: f32, seconds: f64) {
		let controller = self.controller.clone();
		if self.motor_list.len() >= 2 {
			self.motor_list[0].vibrate(&controller, controller_name, strong_magnitude, STRONG_MOTOR_SHARPNESS, seconds);
			self.motor_list[1].vibrate(&controller, controller_name, weak_magnitude, WEAK_MOTOR_SHARPNESS, seconds);
		}
		else if let Some(motor) = self.motor_list.first_mut() {
			let (intensity, sharpness) = if strong_magnitude >= weak_magnitude {
				(strong_magnitude, STRONG_MOTOR_SHARPNESS)
			}
			else {
				(weak_magnitude, WEAK_MOTOR_SHARPNESS)
			};
			motor.vibrate(&controller, controller_name, intensity, sharpness, seconds);
		}
	}
}

impl HapticMotor {
	//==============================================================================
	/// 떨기. (앞 진동을 멈추고, 세기와 길이가 있으면 새로 낸다. 실패하면 엔진을 다시 지어 한 번 더 해 본다)
	//==============================================================================
	fn vibrate(&mut self, controller: &GCController, controller_name: &str, intensity: f32, sharpness: f32, seconds: f64) {
		if let Some(player) = self.player.take() {
			let _ = unsafe { player.stopAtTime_error(0.0) };
		}
		if intensity <= 0.0 || seconds <= 0.0 {
			return;
		}
		for attempt_number in 0..2 {
			match self.start_player(controller, intensity, sharpness, seconds) {
				Ok(player) => {
					self.player = Some(player);
					return;
				}
				Err(error_message) => {
					write_log(&format!("[GamepadManager] 맥 패드 진동 실패 ({} {}, {}번째): {}", controller_name, self.locality, attempt_number + 1, error_message));
					self.engine = None;
					self.is_start_needed.store(true, Ordering::SeqCst);
				}
			}
		}
	}

	//==============================================================================
	/// 엔진을 (지어) 시작하고 연속 이벤트 플레이어를 시작한다.
	//==============================================================================
	fn start_player(&mut self, controller: &GCController, intensity: f32, sharpness: f32, seconds: f64) -> Result<Retained<ProtocolObject<dyn CHHapticPatternPlayer>>, String> {
		if self.engine.is_none() {
			self.engine = Some(self.create_engine(controller)?);
		}
		let engine = match self.engine.as_ref() {
			Some(engine) => {
				engine
			}
			None => {
				return Err(String::from("엔진이 없습니다."));
			}
		};
		if self.is_start_needed.swap(false, Ordering::SeqCst) {
			if let Err(error) = unsafe { engine.startAndReturnError() } {
				self.is_start_needed.store(true, Ordering::SeqCst);
				return Err(format!("엔진 시작 실패: {}", error.localizedDescription()));
			}
			write_log(&format!("[GamepadManager] 맥 패드 진동 엔진 시작: {}", self.locality));
		}
		let pattern = create_continuous_pattern(intensity, sharpness, seconds)?;
		let player = unsafe { engine.createPlayerWithPattern_error(&pattern) }.map_err(|error| format!("플레이어 생성 실패: {}", error.localizedDescription()))?;
		unsafe { player.startAtTime_error(0.0) }.map_err(|error| format!("플레이어 시작 실패: {}", error.localizedDescription()))?;
		Ok(player)
	}

	//==============================================================================
	/// 엔진 짓기. (멈춤과 리셋 처리기는 다시 시작할 표시만 켠다)
	//==============================================================================
	fn create_engine(&self, controller: &GCController) -> Result<Retained<CHHapticEngine>, String> {
		let haptics = match unsafe { controller.haptics() } {
			Some(haptics) => {
				haptics
			}
			None => {
				return Err(String::from("컨트롤러가 햅틱 정보를 주지 않습니다."));
			}
		};
		let engine: Option<Retained<CHHapticEngine>> = unsafe { msg_send![&*haptics, createEngineWithLocality: self.locality] };
		let engine = match engine {
			Some(engine) => {
				engine
			}
			None => {
				return Err(String::from("createEngineWithLocality 가 엔진을 주지 않았습니다."));
			}
		};
		let stopped_flag = self.is_start_needed.clone();
		let stopped_handler = RcBlock::new(move |_stopped_reason: CHHapticEngineStoppedReason| {
			stopped_flag.store(true, Ordering::SeqCst);
		});
		let reset_flag = self.is_start_needed.clone();
		let reset_handler = RcBlock::new(move || {
			reset_flag.store(true, Ordering::SeqCst);
		});
		unsafe {
			engine.setStoppedHandler(RcBlock::as_ptr(&stopped_handler));
			engine.setResetHandler(RcBlock::as_ptr(&reset_handler));
		}
		self.is_start_needed.store(true, Ordering::SeqCst);
		write_log(&format!("[GamepadManager] 맥 패드 진동 엔진 생성: {}", self.locality));
		Ok(engine)
	}
}

//==============================================================================
/// 세기와 날카로움이 고른 연속 진동 패턴. (30 초를 넘으면 이어 붙인다)
//==============================================================================
fn create_continuous_pattern(intensity: f32, sharpness: f32, seconds: f64) -> Result<Retained<CHHapticPattern>, String> {
	let mut event_list: Vec<Retained<CHHapticEvent>> = Vec::new();
	let mut relative_time = 0.0;
	while relative_time < seconds {
		let event_seconds = (seconds - relative_time).min(MAXIMUM_CONTINUOUS_EVENT_SECONDS);
		let intensity_parameter = unsafe { CHHapticEventParameter::initWithParameterID_value(CHHapticEventParameter::alloc(), CHHapticEventParameterIDHapticIntensity, intensity) };
		let sharpness_parameter = unsafe { CHHapticEventParameter::initWithParameterID_value(CHHapticEventParameter::alloc(), CHHapticEventParameterIDHapticSharpness, sharpness) };
		let parameter_array = NSArray::from_retained_slice(&[intensity_parameter, sharpness_parameter]);
		let event = unsafe { CHHapticEvent::initWithEventType_parameters_relativeTime_duration(CHHapticEvent::alloc(), CHHapticEventTypeHapticContinuous, &parameter_array, relative_time, event_seconds) };
		event_list.push(event);
		relative_time += event_seconds;
	}
	let event_array = NSArray::from_retained_slice(&event_list);
	let dynamic_parameter_array = NSArray::new();
	unsafe { CHHapticPattern::initWithEvents_parameters_error(CHHapticPattern::alloc(), &event_array, &dynamic_parameter_array) }.map_err(|error| format!("패턴 생성 실패: {}", error.localizedDescription()))
}

//==============================================================================
/// 로그. (게임패드 관리자의 로그와 같은 곳에 낸다)
//==============================================================================
fn write_log(message: &str) {
	eprintln!("{}", message);
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 시험용 이름 목록.
	//==============================================================================
	fn create_name_list(name_list: &[&str]) -> Vec<String> {
		name_list.iter().map(|name| name.to_string()).collect()
	}

	//==============================================================================
	/// 이름이 같은 것끼리 잇는다. (차례가 달라도, 대소문자를 무시하고)
	//==============================================================================
	#[test]
	fn pair_by_name() {
		let gamepad_name_list = create_name_list(&["Xbox Wireless Controller", "DualSense Wireless Controller"]);
		let controller_name_list = vec![Some(String::from("dualsense wireless controller")), Some(String::from("Xbox Wireless Controller"))];
		assert_eq!(pair_gamepads_with_controllers(&gamepad_name_list, &controller_name_list), vec![Some(1), Some(0)]);
	}

	//==============================================================================
	/// 이름이 안 맞으면 차례로 잇고, 컨트롤러가 모자라면 남은 패드는 짝이 없다.
	//==============================================================================
	#[test]
	fn pair_by_order() {
		let gamepad_name_list = create_name_list(&["Generic USB Pad", "DualSense Wireless Controller", "Other Pad"]);
		let controller_name_list = vec![Some(String::from("DualSense Wireless Controller")), None];
		assert_eq!(pair_gamepads_with_controllers(&gamepad_name_list, &controller_name_list), vec![Some(1), Some(0), None]);
	}

	//==============================================================================
	/// 컨트롤러가 하나면 이름이 달라도 그것과 잇는다. 없으면 모두 짝이 없다.
	//==============================================================================
	#[test]
	fn pair_single_controller() {
		let gamepad_name_list = create_name_list(&["Wireless Controller"]);
		assert_eq!(pair_gamepads_with_controllers(&gamepad_name_list, &[Some(String::from("DualSense Wireless Controller"))]), vec![Some(0)]);
		assert_eq!(pair_gamepads_with_controllers(&gamepad_name_list, &[]), vec![None]);
		assert_eq!(pair_gamepads_with_controllers(&[], &[None]), Vec::<Option<usize>>::new());
	}

	//==============================================================================
	/// 30 초가 넘는 진동은 연속 이벤트를 이어 붙여 그 길이의 패턴이 된다. (패드 없이 CoreHaptics 패턴만 짓는다)
	//==============================================================================
	#[test]
	fn continuous_pattern_duration() {
		let short_pattern = create_continuous_pattern(0.5, STRONG_MOTOR_SHARPNESS, 0.25).expect("short pattern");
		assert!((unsafe { short_pattern.duration() } - 0.25).abs() < 0.001);
		let long_pattern = create_continuous_pattern(1.0, WEAK_MOTOR_SHARPNESS, 45.0).expect("long pattern");
		assert!((unsafe { long_pattern.duration() } - 45.0).abs() < 0.001);
	}
}

//==============================================================================
// 알림이 나오고 사라지는 모션. (vanilla.js ui/toastmotion.js 의 ToastMotion, readToastSeconds)
// - 알림 노드에 붙여 show 로 띄운다. 바깥쪽(위 알림은 위, 아래 알림은 아래)에서 미끄러져 들어오며 보이고 살짝 커졌다 돌아오고,
//   머무는 시간이 지나면 떠오르며 사라진다.
// - 떠 있는 동안 다른 글이 오면 앞 것이 빨리 비키고 새 글이 들어오며, 같은 글이 또 오면 시간만 다시 채우고 통 튄다.
// - 노드의 자리는 놓인 자리를 기준으로 그 둘레에서만 움직인다. (그 사이 다시 놓으면 그 자리가 새 기준)
// - 원본과 다른 것: show 는 요청만 받아 두고 다음 tick 에서 노드를 움직인다. (장면 코드는 노드 문맥 없이 부르므로)
//   글은 set_apply_text_handler 로 건 함수가 노드에 넣는다. 숨은 동안 노드는 끄지 않고 보이지 않게만 한다. (tick 이 계속 돌게)
// - UIToast 는 줄 세운 알약 알림을 스스로 만들고, 이것은 게임이 만든 알림 노드를 움직인다.
//==============================================================================
use std::f32::consts::PI;

use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::ComponentContext;

// 나옴: 바깥쪽에서 미끄러져 들어오는 거리, 투명에서 보이기까지와 커졌다 돌아오는 초, 크기.
const ENTER_DISTANCE: f32 = 10.0;
const ENTER_SECONDS: f32 = 0.18;
const ENTER_SETTLE_SECONDS: f32 = 0.08;
const ENTER_START_SCALE: f32 = 0.94;
const ENTER_PEAK_SCALE: f32 = 1.02;
// 사라짐: 떠오르는 거리와 초, 새 알림에 비킬 때의 초.
const EXIT_DISTANCE: f32 = 8.0;
const EXIT_SECONDS: f32 = 0.25;
const QUICK_EXIT_SECONDS: f32 = 0.12;
// 같은 글이 또 오면 통 튀는 크기와 초.
const REPEAT_SCALE: f32 = 1.05;
const REPEAT_SECONDS: f32 = 0.15;
// 머무는 시간: 글이 이 글자 수를 넘으면 그만큼마다 더하고, 이 초를 넘지 않는다.
const LONG_TEXT_CHARACTERS: usize = 20;
const LONG_TEXT_EXTRA_SECONDS: f32 = 0.5;
const MAXIMUM_STAY_SECONDS: f32 = 4.0;

// 노드에 글을 넣는 함수. (같은 노드의 다른 컴포넌트, 예를 들어 UILabel 은 문맥의 노드에서 찾는다)
pub type ToastApplyTextHandler = Box<dyn FnMut(&mut ComponentContext, &str)>;

//==============================================================================
// 알림의 단계.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastPhase {
	Hidden,
	Enter,
	Stay,
	Exit,
}

//==============================================================================
// 알림 머무는 시간. (긴 글이면 20 자마다 0.5 초를 더하고 4 초까지)
//==============================================================================
pub fn read_toast_seconds(text: &str, base_seconds: f32) -> f32 {
	let character_count = text.chars().count();
	let extra_steps = if character_count > LONG_TEXT_CHARACTERS { (character_count - LONG_TEXT_CHARACTERS).div_ceil(LONG_TEXT_CHARACTERS) } else { 0 };
	(base_seconds + extra_steps as f32 * LONG_TEXT_EXTRA_SECONDS).max(base_seconds).min(MAXIMUM_STAY_SECONDS)
}

pub struct ToastMotion {
	component_base: ComponentBase,
	enter_side: f32,
	phase: ToastPhase,
	phase_seconds: f32,
	exit_length: f32,
	stay_seconds: f32,
	repeat_seconds: f32,
	text: String,
	requested: Option<(String, f32)>,
	pending: Option<(String, f32)>,
	is_hide_requested: bool,
	base_position: Option<Vector2>,
	applied_position: Option<Vector2>,
	apply_text_handler: Option<ToastApplyTextHandler>,
}

impl ToastMotion {
	//==============================================================================
	// 생성. (들어오는 쪽: -1 이면 위에서, 1 이면 아래에서)
	//==============================================================================
	pub fn new(enter_side: f32) -> ToastMotion {
		ToastMotion {
			component_base: ComponentBase::new(),
			enter_side,
			phase: ToastPhase::Hidden,
			phase_seconds: 0.0,
			exit_length: EXIT_SECONDS,
			stay_seconds: 0.0,
			repeat_seconds: 0.0,
			text: String::new(),
			requested: None,
			pending: None,
			is_hide_requested: false,
			base_position: None,
			applied_position: None,
			apply_text_handler: None,
		}
	}

	//==============================================================================
	// 노드에 글을 넣는 함수 설정.
	//==============================================================================
	pub fn set_apply_text_handler(&mut self, apply_text_handler: ToastApplyTextHandler) {
		self.apply_text_handler = Some(apply_text_handler);
	}

	//==============================================================================
	// 띄우기 요청. (다음 tick 에서 띄운다, 떠 있는 다른 글은 빨리 비킨 뒤 띄운다)
	//==============================================================================
	pub fn show(&mut self, text: &str, seconds: f32) {
		self.requested = Some((String::from(text), seconds));
	}

	//==============================================================================
	// 곧바로 숨기기 요청. (판이 바뀔 때)
	//==============================================================================
	pub fn hide_now(&mut self) {
		self.is_hide_requested = true;
		self.requested = None;
		self.pending = None;
	}

	//==============================================================================
	// 떠 있는지. (나오는 중, 머무는 중, 사라지는 중, 띄우기 요청이 있음)
	//==============================================================================
	pub fn is_showing(&self) -> bool {
		self.phase != ToastPhase::Hidden || self.requested.is_some()
	}

	//==============================================================================
	// 단계 반환.
	//==============================================================================
	pub fn get_phase(&self) -> ToastPhase {
		self.phase
	}

	//==============================================================================
	// 들어오는 쪽 설정.
	//==============================================================================
	pub fn set_enter_side(&mut self, enter_side: f32) {
		self.enter_side = enter_side;
	}

	//==============================================================================
	// 요청 처리.
	//==============================================================================
	fn handle_request(&mut self, context: &mut ComponentContext) {
		if self.is_hide_requested {
			self.is_hide_requested = false;
			self.phase = ToastPhase::Hidden;
			self.place_around(context, 1.0, 0.0, 1.0);
			context.get_transform_mut().set_visible(false);
			self.base_position = None;
			self.applied_position = None;
		}
		let (text, seconds) = match self.requested.take() {
			Some(requested) => {
				requested
			}
			None => {
				return;
			}
		};
		let is_shown = self.phase != ToastPhase::Hidden;
		if is_shown && text == self.text && self.phase != ToastPhase::Exit {
			self.stay_seconds = seconds;
			self.repeat_seconds = REPEAT_SECONDS;
			return;
		}
		if is_shown {
			self.pending = Some((text, seconds));
			self.start_exit(QUICK_EXIT_SECONDS);
			return;
		}
		self.start_enter(context, &text, seconds);
	}

	//==============================================================================
	// 나오기 시작.
	//==============================================================================
	fn start_enter(&mut self, context: &mut ComponentContext, text: &str, seconds: f32) {
		if let Some(apply_text_handler) = self.apply_text_handler.as_mut() {
			apply_text_handler(context, text);
		}
		self.text = String::from(text);
		self.stay_seconds = seconds;
		self.repeat_seconds = 0.0;
		self.phase = ToastPhase::Enter;
		self.phase_seconds = 0.0;
		context.get_transform_mut().set_visible(true);
		let enter_offset = self.enter_side * ENTER_DISTANCE;
		self.place_around(context, ENTER_START_SCALE, enter_offset, 0.0);
	}

	//==============================================================================
	// 사라지기 시작.
	//==============================================================================
	fn start_exit(&mut self, exit_length: f32) {
		self.phase = ToastPhase::Exit;
		self.phase_seconds = 0.0;
		self.exit_length = exit_length;
	}

	//==============================================================================
	// 놓인 자리를 기준으로 가운데를 두고 그 크기와 위아래 비킴, 불투명도로 놓기.
	//==============================================================================
	fn place_around(&mut self, context: &mut ComponentContext, scale: f32, offset_y: f32, opacity: f32) {
		let transform = context.get_transform_mut();
		let local_position = transform.get_local_position();
		let is_moved_by_layout = self.applied_position != Some(local_position);
		if is_moved_by_layout || self.base_position.is_none() {
			self.base_position = Some(local_position);
		}
		let base_position = self.base_position.unwrap_or(local_position);
		let content_size = transform.get_content_size();
		let position = Vector2::create(base_position.x - content_size.x * (scale - 1.0) * 0.5, base_position.y - content_size.y * (scale - 1.0) * 0.5 + offset_y);
		transform.set_local_position(position);
		transform.set_local_scale(Vector2::create(scale, scale));
		transform.set_local_opacity(opacity);
		self.applied_position = Some(position);
	}
}

impl Component for ToastMotion {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"ToastMotion"
	}

	//==============================================================================
	// 갱신. (요청을 처리하고, 단계마다 자리와 크기와 불투명도)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		self.handle_request(context);
		if self.phase == ToastPhase::Hidden {
			return;
		}
		self.phase_seconds += time_delta;
		let mut scale = 1.0;
		let mut offset_y = 0.0;
		let mut opacity = 1.0;
		match self.phase {
			ToastPhase::Enter => {
				let enter_ratio = (self.phase_seconds / ENTER_SECONDS).min(1.0);
				let eased_ratio = 1.0 - (1.0 - enter_ratio) * (1.0 - enter_ratio);
				offset_y = self.enter_side * ENTER_DISTANCE * (1.0 - eased_ratio);
				opacity = eased_ratio;
				if self.phase_seconds <= ENTER_SECONDS {
					scale = ENTER_START_SCALE + (ENTER_PEAK_SCALE - ENTER_START_SCALE) * eased_ratio;
				}
				else {
					let settle_ratio = ((self.phase_seconds - ENTER_SECONDS) / ENTER_SETTLE_SECONDS).min(1.0);
					scale = ENTER_PEAK_SCALE + (1.0 - ENTER_PEAK_SCALE) * settle_ratio;
				}
				if self.phase_seconds >= ENTER_SECONDS + ENTER_SETTLE_SECONDS {
					self.phase = ToastPhase::Stay;
					self.phase_seconds = 0.0;
				}
			}
			ToastPhase::Stay => {
				self.stay_seconds -= time_delta;
				if self.repeat_seconds > 0.0 {
					self.repeat_seconds = (self.repeat_seconds - time_delta).max(0.0);
					scale = 1.0 + (REPEAT_SCALE - 1.0) * ((1.0 - self.repeat_seconds / REPEAT_SECONDS) * PI).sin();
				}
				if self.stay_seconds <= 0.0 {
					self.start_exit(EXIT_SECONDS);
				}
			}
			ToastPhase::Exit => {
				let exit_ratio = (self.phase_seconds / self.exit_length).min(1.0);
				offset_y = -EXIT_DISTANCE * exit_ratio;
				opacity = 1.0 - exit_ratio;
				if exit_ratio >= 1.0 {
					self.place_around(context, 1.0, 0.0, 1.0);
					self.base_position = None;
					self.applied_position = None;
					match self.pending.take() {
						Some((text, seconds)) => {
							self.start_enter(context, &text, seconds);
						}
						None => {
							self.phase = ToastPhase::Hidden;
							context.get_transform_mut().set_visible(false);
						}
					}
					return;
				}
			}
			ToastPhase::Hidden => {
			}
		}
		self.place_around(context, scale, offset_y, opacity);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn long_text_stays_longer() {
		assert_eq!(read_toast_seconds("짧은 글", 2.0), 2.0);
		assert_eq!(read_toast_seconds(&"가".repeat(45), 2.0), 3.0);
		assert_eq!(read_toast_seconds(&"가".repeat(400), 2.0), 4.0);
	}
}

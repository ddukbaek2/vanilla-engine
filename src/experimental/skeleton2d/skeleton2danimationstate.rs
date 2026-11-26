//==============================================================================
//! 2D 뼈대 애니메이션 상태. (원본 experimental/skeleton2d/skeleton2danimationstate.js, 트랙마다 클립 하나, 트랙 안 크로스페이드, 트랙끼리 층 섞기, 사건)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 트랙은 번호 차례로 쌓인다. 0 번이 바탕이고, 위 트랙은 그 몫(alpha)만큼 아래 결과를 덮는다. (유니티 Animator 의 덮어쓰기 층, Spine 트랙과 같은 꼴)
//! - 트랙에 새 클립을 섞는 시간과 함께 걸면 앞 클립이 그 시간 동안 이어 돌며 새 클립으로 넘어간다. 빈 트랙에 걸면 그 시간 동안 나타난다.
//! - apply 는 지금 자세에서 시작해, 걸린 클립들이 움직이는 값만 처음 자세로 되돌린 뒤 섞는다.
//!   클립이 움직이지 않는 뼈(IK 목표처럼 코드가 옮기는 뼈)는 그대로 남는다. 클립이 놓아 준 값도 그대로 남는다. (처음으로 돌리려면 Skeleton2D::set_to_setup_pose)
//! - 사건은 트랙의 지금 클립(섞여 나가는 앞 클립은 빼고)이 지난 시각의 것을 모았다가 take_events 로 꺼낸다.
//! - 원본과 다른 점: 트랙 항목은 클립을 `Rc<Skeleton2DAnimation>` 으로 든다. 섞여 나가는 앞 항목은 Box 로 든다.
//!
//! # 예제
//! 걷기 위에 손 흔들기를 반만 얹고 0.5 초 뒤 사건 꺼내기
//! ```rust
//! use std::rc::Rc;
//! use vanilla::experimental::skeleton2d::{Skeleton2D, Skeleton2DAnimationState, Skeleton2DData};
//!
//! let json = serde_json::json!({
//! 	"bones": [{ "name": "root" }, { "name": "leg", "parent": "root" }, { "name": "arm", "parent": "root" }],
//! 	"animations": {
//! 		"walk": { "bones": { "leg": { "rotate": [{ "time": 0, "value": -20 }, { "time": 1, "value": 20 }] } }, "events": [{ "time": 0.5, "name": "step" }] },
//! 		"wave": { "bones": { "arm": { "rotate": [{ "time": 0, "value": 0 }, { "time": 1, "value": 80 }] } } }
//! 	}
//! });
//! let skeleton_data = Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton"));
//! let mut skeleton = Skeleton2D::new(skeleton_data.clone());
//! let mut animation_state = Skeleton2DAnimationState::new(skeleton_data.clone());
//! animation_state.set_animation(0, "walk", true, 0.0);
//! animation_state.set_animation(1, "wave", true, 0.0);
//! animation_state.set_track_alpha(1, 0.5);
//! animation_state.update(0.5);
//! animation_state.apply(&mut skeleton);
//! let fired_events = animation_state.take_events();
//! assert!((skeleton.find_bone("arm").expect("arm").get_rotation() - 20.0).abs() < 0.001);
//! assert_eq!(fired_events[0].get_event().get_name(), "step");
//! ```
//==============================================================================
use crate::experimental::jsmath::js_min;
use crate::experimental::skeleton2d::skeleton2d::Skeleton2D;
use crate::experimental::skeleton2d::skeleton2danimation::{Skeleton2DAnimation, Skeleton2DEvent, Skeleton2DPose};
use crate::experimental::skeleton2d::skeleton2ddata::Skeleton2DData;
use std::rc::Rc;

//==============================================================================
/// 트랙 항목. (트랙에 건 클립 하나의 재생 상태)
///
/// # 예제
/// 트랙 항목 읽기
/// ```rust
/// use std::rc::Rc;
/// use vanilla::experimental::skeleton2d::{Skeleton2DAnimationState, Skeleton2DData};
///
/// let json = serde_json::json!({
/// 	"bones": [{ "name": "root" }],
/// 	"animations": { "idle": { "duration": 2, "bones": { "root": { "translate": [{ "time": 0, "y": 0 }, { "time": 2, "y": 4 }] } } } }
/// });
/// let mut animation_state = Skeleton2DAnimationState::new(Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton")));
/// animation_state.set_animation(0, "idle", false, 0.25);
/// animation_state.update(2.5);
/// let track_entry = animation_state.get_track(0).expect("track");
/// assert_eq!(track_entry.get_animation_name(), "idle");
/// assert_eq!(track_entry.get_animation_time(), 2.0);
/// assert_eq!(track_entry.get_mix_ratio(), 1.0);
/// assert!(track_entry.is_complete());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DTrackEntry {
	animation: Rc<Skeleton2DAnimation>,
	is_loop: bool,
	track_time: f64,
	previous_track_time: f64,
	alpha: f64,
	mix_duration: f64,
	mix_time: f64,
	mixing_from: Option<Box<Skeleton2DTrackEntry>>,
	is_started: bool,
}

impl Skeleton2DTrackEntry {
	//==============================================================================
	/// 생성. (트랙 시각 0, 아직 돌지 않음)
	///
	/// # 인자
	/// - `animation`: 클립.
	/// - `is_loop`: 되풀이하면 true.
	/// - `alpha`: 층 몫. (0 ~ 1)
	/// - `mix_duration`: 섞는 시간. (초, 0 이면 바로)
	///
	/// # 돌려줌
	/// 트랙 항목.
	//==============================================================================
	pub fn new(animation: Rc<Skeleton2DAnimation>, is_loop: bool, alpha: f64, mix_duration: f64) -> Skeleton2DTrackEntry {
		Skeleton2DTrackEntry {
			animation,
			is_loop,
			track_time: 0.0,
			previous_track_time: -1.0,
			alpha,
			mix_duration,
			mix_time: 0.0,
			mixing_from: None,
			is_started: false,
		}
	}

	//==============================================================================
	/// 클립 반환.
	///
	/// # 돌려줌
	/// 클립.
	//==============================================================================
	pub fn get_animation(&self) -> &Rc<Skeleton2DAnimation> {
		&self.animation
	}

	//==============================================================================
	/// 클립 이름 반환.
	///
	/// # 돌려줌
	/// 클립 이름.
	//==============================================================================
	pub fn get_animation_name(&self) -> &str {
		self.animation.get_name()
	}

	//==============================================================================
	/// 되풀이 여부 반환.
	///
	/// # 돌려줌
	/// 되풀이하면 true.
	//==============================================================================
	pub fn is_loop(&self) -> bool {
		self.is_loop
	}

	//==============================================================================
	/// 트랙 시각 반환. (건 뒤 흐른 시간, 되풀이해도 접지 않는다)
	///
	/// # 돌려줌
	/// 트랙 시각. (초)
	//==============================================================================
	pub fn get_track_time(&self) -> f64 {
		self.track_time
	}

	//==============================================================================
	/// 트랙 시각 설정. (건너뛰기, 그 사이 사건은 알리지 않는다)
	///
	/// # 인자
	/// - `track_time`: 트랙 시각. (초)
	//==============================================================================
	pub fn set_track_time(&mut self, track_time: f64) {
		self.track_time = track_time;
		self.previous_track_time = track_time;
		self.is_started = true;
	}

	//==============================================================================
	/// 앞 update 의 트랙 시각 반환.
	///
	/// # 돌려줌
	/// 앞 트랙 시각. (초, 아직 돌지 않았으면 -1)
	//==============================================================================
	pub fn get_previous_track_time(&self) -> f64 {
		self.previous_track_time
	}

	//==============================================================================
	/// 트랙 시각 흘리기. (앞 시각을 적어 두고 더한다, 처음 흘릴 때의 앞 시각은 -1 이라 0 의 사건도 든다, 섞는 시간도 흐른다)
	///
	/// # 인자
	/// - `time_delta`: 흐른 시간. (초)
	//==============================================================================
	pub fn advance(&mut self, time_delta: f64) {
		self.previous_track_time = if self.is_started { self.track_time } else { -1.0 };
		self.is_started = true;
		self.track_time += time_delta;
		self.mix_time += time_delta;
	}

	//==============================================================================
	/// 클립 안의 시각 반환. (되풀이하면 길이로 접고, 아니면 길이에서 멈춘다)
	///
	/// # 돌려줌
	/// 클립 안의 시각. (초)
	//==============================================================================
	pub fn get_animation_time(&self) -> f64 {
		let duration = self.animation.get_duration();
		if duration <= 0.0 {
			return 0.0;
		}
		if self.is_loop {
			return self.track_time - (self.track_time / duration).floor() * duration;
		}
		js_min(self.track_time, duration)
	}

	//==============================================================================
	/// 층 몫 반환.
	///
	/// # 돌려줌
	/// 층 몫. (0 ~ 1)
	//==============================================================================
	pub fn get_alpha(&self) -> f64 {
		self.alpha
	}

	//==============================================================================
	/// 층 몫 설정.
	///
	/// # 인자
	/// - `alpha`: 층 몫. (0 이면 보이지 않고 1 이면 아래를 다 덮는다)
	//==============================================================================
	pub fn set_alpha(&mut self, alpha: f64) {
		self.alpha = alpha;
	}

	//==============================================================================
	/// 섞는 시간 반환.
	///
	/// # 돌려줌
	/// 섞는 시간. (초)
	//==============================================================================
	pub fn get_mix_duration(&self) -> f64 {
		self.mix_duration
	}

	//==============================================================================
	/// 섞기 시작 뒤 흐른 시간 반환.
	///
	/// # 돌려줌
	/// 흐른 시간. (초)
	//==============================================================================
	pub fn get_mix_time(&self) -> f64 {
		self.mix_time
	}

	//==============================================================================
	/// 섞인 몫 반환.
	///
	/// # 돌려줌
	/// 이 클립의 몫. (0 ~ 1, 섞는 시간이 0 이거나 다 섞였으면 1)
	//==============================================================================
	pub fn get_mix_ratio(&self) -> f64 {
		if self.mix_duration <= 0.0 {
			return 1.0;
		}
		js_min(self.mix_time / self.mix_duration, 1.0)
	}

	//==============================================================================
	/// 섞여 나가는 앞 항목 반환.
	///
	/// # 돌려줌
	/// 앞 항목. (없으면 None)
	//==============================================================================
	pub fn get_mixing_from(&self) -> Option<&Skeleton2DTrackEntry> {
		self.mixing_from.as_deref()
	}

	//==============================================================================
	/// 섞여 나가는 앞 항목 설정. (Skeleton2DAnimationState 가 쓴다)
	///
	/// # 인자
	/// - `mixing_from`: 앞 항목. (None 이면 끊는다)
	//==============================================================================
	pub fn set_mixing_from(&mut self, mixing_from: Option<Skeleton2DTrackEntry>) {
		self.mixing_from = mixing_from.map(Box::new);
	}

	//==============================================================================
	/// 끝났는지.
	///
	/// # 돌려줌
	/// 되풀이하지 않는 클립이 길이에 닿았으면 true. (되풀이하면 늘 false)
	//==============================================================================
	pub fn is_complete(&self) -> bool {
		!self.is_loop && self.track_time >= self.animation.get_duration()
	}
}

//==============================================================================
/// 알린 사건. (어느 트랙의 어느 클립에서 난 사건인지)
///
/// # 예제
/// 알린 사건 읽기
/// ```rust
/// use vanilla::experimental::skeleton2d::{Skeleton2DEvent, Skeleton2DFiredEvent};
///
/// let fired_event = Skeleton2DFiredEvent::new(1, "attack", Skeleton2DEvent::new("hit", 0.2, 0, 0.0, ""));
/// assert_eq!(fired_event.get_track_index(), 1);
/// assert_eq!(fired_event.get_animation_name(), "attack");
/// assert_eq!(fired_event.get_event().get_name(), "hit");
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton2DFiredEvent {
	track_index: usize,
	animation_name: String,
	event: Skeleton2DEvent,
}

impl Skeleton2DFiredEvent {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `track_index`: 트랙 번호.
	/// - `animation_name`: 클립 이름.
	/// - `event`: 사건.
	///
	/// # 돌려줌
	/// 알린 사건.
	//==============================================================================
	pub fn new(track_index: usize, animation_name: &str, event: Skeleton2DEvent) -> Skeleton2DFiredEvent {
		Skeleton2DFiredEvent {
			track_index,
			animation_name: animation_name.to_string(),
			event,
		}
	}

	//==============================================================================
	/// 트랙 번호 반환.
	///
	/// # 돌려줌
	/// 트랙 번호.
	//==============================================================================
	pub fn get_track_index(&self) -> usize {
		self.track_index
	}

	//==============================================================================
	/// 클립 이름 반환.
	///
	/// # 돌려줌
	/// 클립 이름.
	//==============================================================================
	pub fn get_animation_name(&self) -> &str {
		&self.animation_name
	}

	//==============================================================================
	/// 사건 반환.
	///
	/// # 돌려줌
	/// 사건.
	//==============================================================================
	pub fn get_event(&self) -> &Skeleton2DEvent {
		&self.event
	}
}

//==============================================================================
/// 2D 뼈대 애니메이션 상태.
///
/// # 예제
/// 대기에서 걷기로 0.2 초 동안 넘어가기
/// ```rust
/// use std::rc::Rc;
/// use vanilla::experimental::skeleton2d::{Skeleton2D, Skeleton2DAnimationState, Skeleton2DData};
///
/// let json = serde_json::json!({
/// 	"bones": [{ "name": "root" }, { "name": "body", "parent": "root" }],
/// 	"animations": {
/// 		"idle": { "duration": 1, "bones": { "body": { "translate": [{ "time": 0, "y": 0 }] } } },
/// 		"walk": { "duration": 1, "bones": { "body": { "translate": [{ "time": 0, "y": -10 }] } } }
/// 	}
/// });
/// let skeleton_data = Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton"));
/// let mut skeleton = Skeleton2D::new(skeleton_data.clone());
/// let mut animation_state = Skeleton2DAnimationState::new(skeleton_data.clone());
/// animation_state.set_animation(0, "idle", true, 0.0);
/// animation_state.update(0.5);
/// animation_state.set_animation(0, "walk", true, 0.2);
/// animation_state.update(0.1);
/// animation_state.apply(&mut skeleton);
/// assert!((skeleton.find_bone("body").expect("body").get_y() + 5.0).abs() < 0.001);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DAnimationState {
	data: Rc<Skeleton2DData>,
	tracks: Vec<Option<Skeleton2DTrackEntry>>,
	fired_events: Vec<Skeleton2DFiredEvent>,
	base_pose: Skeleton2DPose,
	from_pose: Skeleton2DPose,
	to_pose: Skeleton2DPose,
}

impl Skeleton2DAnimationState {
	//==============================================================================
	/// 생성. (트랙 없음)
	///
	/// # 인자
	/// - `skeleton_data`: 뼈대 자료.
	///
	/// # 돌려줌
	/// 애니메이션 상태.
	//==============================================================================
	pub fn new(skeleton_data: Rc<Skeleton2DData>) -> Skeleton2DAnimationState {
		let bone_count = skeleton_data.get_bones().len();
		let slot_count = skeleton_data.get_slots().len();
		Skeleton2DAnimationState {
			data: skeleton_data,
			tracks: Vec::new(),
			fired_events: Vec::new(),
			base_pose: Skeleton2DPose::new(bone_count, slot_count),
			from_pose: Skeleton2DPose::new(bone_count, slot_count),
			to_pose: Skeleton2DPose::new(bone_count, slot_count),
		}
	}

	//==============================================================================
	/// 자료 반환.
	///
	/// # 돌려줌
	/// 뼈대 자료.
	//==============================================================================
	pub fn get_data(&self) -> &Rc<Skeleton2DData> {
		&self.data
	}

	//==============================================================================
	/// 트랙에 클립 걸기.
	/// - 섞는 시간이 0 보다 크면 그 트랙의 앞 클립이 그 시간 동안 이어 돌며 섞여 나간다. 트랙이 비었으면 그 시간 동안 나타난다.
	/// - 층 몫은 그 트랙의 앞 항목 것을 이어받는다. (처음은 1)
	///
	/// # 인자
	/// - `track_index`: 트랙 번호. (0 이 바탕)
	/// - `animation_name`: 클립 이름.
	/// - `is_loop`: 되풀이하면 true.
	/// - `mix_duration`: 섞는 시간. (초)
	///
	/// # 돌려줌
	/// 걸었으면 true. (없는 클립이면 false)
	//==============================================================================
	pub fn set_animation(&mut self, track_index: usize, animation_name: &str, is_loop: bool, mix_duration: f64) -> bool {
		let animation = match self.data.find_animation(animation_name) {
			Some(animation) => {
				animation.clone()
			}
			None => {
				return false;
			}
		};
		while self.tracks.len() <= track_index {
			self.tracks.push(None);
		}
		let current_entry = self.tracks[track_index].take();
		let alpha = current_entry.as_ref().map(|current_entry| current_entry.get_alpha()).unwrap_or(1.0);
		let mut track_entry = Skeleton2DTrackEntry::new(animation, is_loop, alpha, mix_duration);
		if let Some(mut current_entry) = current_entry {
			if mix_duration > 0.0 {
				current_entry.set_mixing_from(None);
				track_entry.set_mixing_from(Some(current_entry));
			}
		}
		self.tracks[track_index] = Some(track_entry);
		true
	}

	//==============================================================================
	/// 트랙 항목 반환.
	///
	/// # 인자
	/// - `track_index`: 트랙 번호.
	///
	/// # 돌려줌
	/// 트랙 항목. (비었으면 None)
	//==============================================================================
	pub fn get_track(&self, track_index: usize) -> Option<&Skeleton2DTrackEntry> {
		self.tracks.get(track_index).and_then(|track_entry| track_entry.as_ref())
	}

	//==============================================================================
	/// 트랙 항목 반환. (변경용)
	///
	/// # 인자
	/// - `track_index`: 트랙 번호.
	///
	/// # 돌려줌
	/// 트랙 항목. (비었으면 None)
	//==============================================================================
	pub fn get_track_mut(&mut self, track_index: usize) -> Option<&mut Skeleton2DTrackEntry> {
		self.tracks.get_mut(track_index).and_then(|track_entry| track_entry.as_mut())
	}

	//==============================================================================
	/// 트랙 수 반환.
	///
	/// # 돌려줌
	/// 가장 큰 번호 + 1. (빈 트랙도 센다)
	//==============================================================================
	pub fn get_track_count(&self) -> usize {
		self.tracks.len()
	}

	//==============================================================================
	/// 트랙 비우기. (그 트랙이 움직이던 값은 그대로 남는다)
	///
	/// # 인자
	/// - `track_index`: 트랙 번호.
	//==============================================================================
	pub fn clear_track(&mut self, track_index: usize) {
		if let Some(track_entry) = self.tracks.get_mut(track_index) {
			*track_entry = None;
		}
	}

	//==============================================================================
	/// 모든 트랙 비우기.
	//==============================================================================
	pub fn clear_tracks(&mut self) {
		self.tracks.clear();
	}

	//==============================================================================
	/// 트랙의 층 몫 설정. (빈 트랙이면 아무것도 하지 않는다)
	///
	/// # 인자
	/// - `track_index`: 트랙 번호.
	/// - `alpha`: 층 몫. (0 ~ 1)
	//==============================================================================
	pub fn set_track_alpha(&mut self, track_index: usize, alpha: f64) {
		if let Some(track_entry) = self.get_track_mut(track_index) {
			track_entry.set_alpha(alpha);
		}
	}

	//==============================================================================
	/// 트랙의 층 몫 반환.
	///
	/// # 인자
	/// - `track_index`: 트랙 번호.
	///
	/// # 돌려줌
	/// 층 몫. (빈 트랙이면 0)
	//==============================================================================
	pub fn get_track_alpha(&self, track_index: usize) -> f64 {
		self.get_track(track_index).map(|track_entry| track_entry.get_alpha()).unwrap_or(0.0)
	}

	//==============================================================================
	/// 한 항목이 지난 시각의 사건 모으기. (되풀이하면 접힌 구간마다)
	//==============================================================================
	fn collect_track_events(track_index: usize, track_entry: &Skeleton2DTrackEntry, fired_events: &mut Vec<Skeleton2DFiredEvent>) {
		let animation = track_entry.get_animation();
		let duration = animation.get_duration();
		let previous_track_time = track_entry.get_previous_track_time();
		let track_time = track_entry.get_track_time();
		let mut events: Vec<Skeleton2DEvent> = Vec::new();
		if !track_entry.is_loop() || duration <= 0.0 {
			let to_time = if duration > 0.0 { js_min(track_time, duration) } else { track_time };
			if to_time > previous_track_time {
				animation.collect_events(previous_track_time, to_time, &mut events);
			}
		}
		else {
			let start_loop = if previous_track_time < 0.0 { 0.0 } else { (previous_track_time / duration).floor() };
			let previous_local_time = if previous_track_time < 0.0 { -1.0 } else { previous_track_time - start_loop * duration };
			let current_loop = (track_time / duration).floor();
			let current_local_time = track_time - current_loop * duration;
			if current_loop == start_loop {
				animation.collect_events(previous_local_time, current_local_time, &mut events);
			}
			else {
				animation.collect_events(previous_local_time, duration, &mut events);
				let mut loop_index = start_loop + 1.0;
				while loop_index < current_loop {
					animation.collect_events(-1.0, duration, &mut events);
					loop_index += 1.0;
				}
				animation.collect_events(-1.0, current_local_time, &mut events);
			}
		}
		for event in events {
			fired_events.push(Skeleton2DFiredEvent::new(track_index, animation.get_name(), event));
		}
	}

	//==============================================================================
	/// 시간 흘리기. (트랙과 섞여 나가는 항목의 시각, 다 섞인 앞 항목은 떼고, 사건을 모은다)
	///
	/// # 인자
	/// - `time_delta`: 흐른 시간. (초)
	//==============================================================================
	pub fn update(&mut self, time_delta: f64) {
		for (track_index, track_entry) in self.tracks.iter_mut().enumerate() {
			let Some(track_entry) = track_entry else {
				continue;
			};
			track_entry.advance(time_delta);
			if let Some(mixing_from) = track_entry.mixing_from.as_mut() {
				mixing_from.advance(time_delta);
				if track_entry.get_mix_ratio() >= 1.0 {
					track_entry.set_mixing_from(None);
				}
			}
			Self::collect_track_events(track_index, track_entry, &mut self.fired_events);
		}
	}

	//==============================================================================
	/// 뼈대에 걸기. (트랙 0 부터 차례로 섞어 뼈대의 지역 값과 슬롯에 쓴다)
	///
	/// # 인자
	/// - `skeleton`: 쓸 뼈대. (이 자료로 지은 것)
	//==============================================================================
	pub fn apply(&mut self, skeleton: &mut Skeleton2D) {
		let skeleton_data = self.data.as_ref();
		skeleton.read_pose(&mut self.base_pose);

		// 걸린 클립들이 움직이는 값은 처음 자세에서 시작한다. (프레임마다 쌓이지 않게)
		for track_entry in self.tracks.iter().flatten() {
			track_entry.get_animation().reset_keyed_to_setup(&mut self.base_pose, skeleton_data);
			if let Some(mixing_from) = track_entry.get_mixing_from() {
				mixing_from.get_animation().reset_keyed_to_setup(&mut self.base_pose, skeleton_data);
			}
		}

		for track_entry in self.tracks.iter().flatten() {
			let mix_ratio = track_entry.get_mix_ratio();
			let mut alpha = track_entry.get_alpha();
			self.to_pose.copy_from(&self.base_pose);
			track_entry.get_animation().apply(&mut self.to_pose, skeleton_data, track_entry.get_animation_time());
			match track_entry.get_mixing_from() {
				Some(mixing_from) => {
					self.from_pose.copy_from(&self.base_pose);
					mixing_from.get_animation().apply(&mut self.from_pose, skeleton_data, mixing_from.get_animation_time());
					self.from_pose.blend(&self.to_pose, mix_ratio);
					self.base_pose.blend(&self.from_pose, alpha);
				}
				None => {
					alpha *= mix_ratio;
					self.base_pose.blend(&self.to_pose, alpha);
				}
			}
		}
		skeleton.write_pose(&self.base_pose);
	}

	//==============================================================================
	/// 모은 사건 꺼내기. (꺼낸 뒤 비운다)
	///
	/// # 돌려줌
	/// 지난 update 들에서 모은 사건. (난 차례)
	//==============================================================================
	pub fn take_events(&mut self) -> Vec<Skeleton2DFiredEvent> {
		std::mem::take(&mut self.fired_events)
	}
}

//==============================================================================
/// 시험. (원본 확인 스크립트 examples/skeleton2d/skeleton2dcheck.mjs 와 같은 입력, 같은 기댓값)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	const TOLERANCE: f64 = 0.0005;

	//==============================================================================
	/// 기댓값과 견주기.
	//==============================================================================
	fn assert_near(label: &str, actual: f64, expected: f64) {
		assert!((actual - expected).abs() <= TOLERANCE, "{}: {} (기댓값 {})", label, actual, expected);
	}

	//==============================================================================
	/// 사건 이름 목록.
	//==============================================================================
	fn read_event_names(fired_events: &[Skeleton2DFiredEvent]) -> Vec<String> {
		fired_events.iter().map(|fired_event| fired_event.get_event().get_name().to_string()).collect()
	}

	//==============================================================================
	/// 3. 클립 걸기, 크로스페이드, 층.
	//==============================================================================
	#[test]
	fn crossfade_and_layer_blend() {
		let json = serde_json::json!({
			"bones": [{ "name": "root" }, { "name": "body", "parent": "root", "y": -20 }, { "name": "leg", "parent": "root", "rotation": 90 }, { "name": "arm", "parent": "root" }],
			"animations": {
				"idle": { "duration": 1, "bones": { "body": { "translate": [{ "time": 0, "y": 0 }] } } },
				"walk": {
					"duration": 1,
					"bones": {
						"body": { "translate": [{ "time": 0, "y": -10 }] },
						"leg": { "rotate": [{ "time": 0, "value": -20, "curve": "stepped" }, { "time": 0.5, "value": 20, "curve": [0.42, 0, 0.58, 1] }, { "time": 1, "value": -20 }] }
					}
				},
				"wave": { "bones": { "arm": { "rotate": [{ "time": 0, "value": 0 }, { "time": 1, "value": 80 }] } } }
			}
		});
		let skeleton_data = Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton"));
		let mut skeleton = Skeleton2D::new(skeleton_data.clone());
		let mut animation_state = Skeleton2DAnimationState::new(skeleton_data.clone());
		animation_state.set_animation(0, "idle", true, 0.0);
		animation_state.update(0.5);
		animation_state.set_animation(0, "walk", true, 0.2);
		animation_state.update(0.1);
		animation_state.apply(&mut skeleton);
		assert_near("크로스페이드 body y", skeleton.find_bone("body").expect("body").get_y(), -25.0);
		assert_near("크로스페이드 leg 계단", skeleton.find_bone("leg").expect("leg").get_rotation(), 80.0);
		animation_state.update(0.15);
		animation_state.apply(&mut skeleton);
		assert_near("크로스페이드 끝 body y", skeleton.find_bone("body").expect("body").get_y(), -30.0);
		assert!(animation_state.get_track(0).expect("track").get_mixing_from().is_none());
		animation_state.update(0.5);
		animation_state.apply(&mut skeleton);
		assert_near("베지어 키 leg", skeleton.find_bone("leg").expect("leg").get_rotation(), 90.0);
		animation_state.set_animation(1, "wave", true, 0.0);
		animation_state.set_track_alpha(1, 0.5);
		animation_state.update(0.5);
		animation_state.apply(&mut skeleton);
		assert_near("층 몫 arm", skeleton.find_bone("arm").expect("arm").get_rotation(), 20.0);
	}

	//==============================================================================
	/// 6. 사건과 그리기 차례, 붙임 시간줄.
	//==============================================================================
	#[test]
	fn events_draw_order_and_attachment() {
		let json = serde_json::json!({
			"bones": [{ "name": "root" }],
			"slots": [{ "name": "back", "bone": "root", "attachment": "a" }, { "name": "front", "bone": "root" }],
			"skins": [{ "name": "default", "attachments": { "back": { "a": { "width": 1, "height": 1 }, "b": { "width": 1, "height": 1 } } } }],
			"animations": {
				"loop": {
					"duration": 1,
					"slots": { "back": { "attachment": [{ "time": 0, "name": "a" }, { "time": 0.5, "name": "b" }] } },
					"drawOrder": [{ "time": 0 }, { "time": 0.5, "order": ["front", "back"] }],
					"events": [{ "time": 0, "name": "start" }, { "time": 0.5, "name": "middle", "int": 3, "float": 1.5, "string": "x" }]
				}
			}
		});
		let skeleton_data = Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton"));
		let mut skeleton = Skeleton2D::new(skeleton_data.clone());
		let mut animation_state = Skeleton2DAnimationState::new(skeleton_data.clone());
		animation_state.set_animation(0, "loop", true, 0.0);
		animation_state.update(0.25);
		assert_eq!(read_event_names(&animation_state.take_events()), vec!["start"]);
		animation_state.update(0.5);
		animation_state.apply(&mut skeleton);
		let second_events = animation_state.take_events();
		assert_eq!(read_event_names(&second_events), vec!["middle"]);
		let middle_event = second_events[0].get_event();
		assert_eq!((middle_event.get_int_value(), middle_event.get_float_value(), middle_event.get_string_value()), (3, 1.5, "x"));
		assert_eq!(skeleton.get_draw_order(), &[1, 0]);
		assert_eq!(skeleton.find_slot("back").expect("back").get_attachment_name(), Some("b"));
		animation_state.update(1.5);
		assert_eq!(read_event_names(&animation_state.take_events()), vec!["start", "middle", "start"]);
	}

	//==============================================================================
	/// 7. DragonBones 읽기. (예제의 같은 캐릭터를 두 꼴로 읽어 같은 꼭짓점이 나오는지, 회전 키 펴기, zOrder, 곡선)
	//==============================================================================
	#[test]
	fn dragon_bones_matches_engine_json() {
		let engine_json: serde_json::Value = serde_json::from_str(include_str!("../../../examples/assets/skeleton2d/hero.skeleton.json")).expect("engine json");
		let dragon_bones_json: serde_json::Value = serde_json::from_str(include_str!("../../../examples/assets/skeleton2d/hero_ske.json")).expect("dragon bones json");
		let engine_data = Rc::new(Skeleton2DData::create_from_json(&engine_json).expect("engine data"));
		let dragon_bones_data = Rc::new(Skeleton2DData::create_from_dragon_bones(&dragon_bones_json, Some("hero")).expect("dragon bones data"));
		assert_eq!(dragon_bones_data.get_bones().len(), engine_data.get_bones().len());
		assert_eq!(dragon_bones_data.get_slots().len(), engine_data.get_slots().len());
		let mut engine_skeleton = Skeleton2D::new(engine_data.clone());
		let mut dragon_bones_skeleton = Skeleton2D::new(dragon_bones_data.clone());
		let mut engine_state = Skeleton2DAnimationState::new(engine_data.clone());
		let mut dragon_bones_state = Skeleton2DAnimationState::new(dragon_bones_data.clone());
		engine_state.set_animation(0, "walk", true, 0.0);
		dragon_bones_state.set_animation(0, "walk", true, 0.0);
		let mut maximum_difference = 0.0f64;
		for _ in 0..40 {
			engine_state.update(1.0 / 30.0);
			dragon_bones_state.update(1.0 / 30.0);
			engine_state.apply(&mut engine_skeleton);
			dragon_bones_state.apply(&mut dragon_bones_skeleton);
			engine_skeleton.update_world_transform();
			dragon_bones_skeleton.update_world_transform();
			for slot_index in 0..engine_data.get_slots().len() {
				let mut engine_positions = [0.0f64; 256];
				let mut dragon_bones_positions = [0.0f64; 256];
				let engine_count = engine_skeleton.compute_attachment_vertices(slot_index, &mut engine_positions, 16.0, 16.0);
				let dragon_bones_count = dragon_bones_skeleton.compute_attachment_vertices(slot_index, &mut dragon_bones_positions, 16.0, 16.0);
				assert_eq!(engine_count, dragon_bones_count);
				for value_index in 0..engine_count * 2 {
					maximum_difference = maximum_difference.max((engine_positions[value_index] - dragon_bones_positions[value_index]).abs());
				}
			}
		}
		assert!(maximum_difference <= 0.002, "꼭짓점 차이 {}", maximum_difference);

		let unwrap_json = serde_json::json!({
			"frameRate": 10,
			"armature": [{
				"name": "spin",
				"bone": [{ "name": "root" }, { "name": "wheel", "parent": "root", "transform": { "skX": 30, "skY": 10 } }],
				"slot": [{ "name": "a", "parent": "root" }, { "name": "b", "parent": "root" }, { "name": "c", "parent": "root" }],
				"animation": [{
					"name": "turn", "duration": 30,
					"bone": [{ "name": "wheel", "rotateFrame": [
						{ "duration": 10, "tweenEasing": 0, "rotate": 170 },
						{ "duration": 10, "tweenEasing": 0, "rotate": -170, "clockwise": 1 },
						{ "duration": 10, "tweenEasing": 0.5, "rotate": 10 },
						{ "duration": 0, "rotate": 0 }
					] }],
					"zOrder": { "frame": [{ "duration": 10, "zOrder": [2, -2] }, { "duration": 0 }] }
				}]
			}]
		});
		let unwrap_data = Rc::new(Skeleton2DData::create_from_dragon_bones(&unwrap_json, None).expect("unwrap"));
		let wheel_data = unwrap_data.find_bone("wheel").expect("wheel");
		assert_near("DragonBones 회전 skY", wheel_data.get_rotation(), 10.0);
		assert_near("DragonBones 기울임 skX - skY", wheel_data.get_shear_y(), 20.0);
		let turn_animation = unwrap_data.find_animation("turn").expect("turn");
		assert_near("DragonBones 길이", turn_animation.get_duration(), 3.0);
		let rotate_timeline = &turn_animation.get_timelines()[0];
		let mut sampled = [0.0f64; 1];
		rotate_timeline.sample_values(1.0, &mut sampled);
		assert_near("DragonBones 가까운 쪽 펴기", sampled[0], 190.0);
		rotate_timeline.sample_values(2.0, &mut sampled);
		assert_near("DragonBones clockwise 한 바퀴", sampled[0], 370.0);
		rotate_timeline.sample_values(2.5, &mut sampled);
		assert_near("DragonBones QuadOut 반 세기", sampled[0], 370.0 + (360.0 - 370.0) * 0.625);
		let mut unwrap_skeleton = Skeleton2D::new(unwrap_data.clone());
		let mut unwrap_state = Skeleton2DAnimationState::new(unwrap_data.clone());
		unwrap_state.set_animation(0, "turn", false, 0.0);
		unwrap_state.update(0.5);
		unwrap_state.apply(&mut unwrap_skeleton);
		assert_eq!(unwrap_skeleton.get_draw_order(), &[2, 0, 1]);
		unwrap_state.update(1.0);
		unwrap_state.apply(&mut unwrap_skeleton);
		assert_eq!(unwrap_skeleton.get_draw_order(), &[0, 1, 2]);
	}

	//==============================================================================
	/// 크롬 158 의 원본과 같은 비트. (원본 tests/experimental/skeleton2dchrome.test.mjs 와 chrome_skeleton2d.json 이 같은 입력, 같은 비트)
	/// - 월드 변환, IK(뼈 둘, 꺾임 쪽, 닿지 않음, 세기, 뼈 하나), 크로스페이드와 층의 뼈 값을 f64 비트로 견준다.
	//==============================================================================
	#[test]
	fn same_bits_as_chrome() {
		const CHROME_VALUES: [(&str, u64); 152] = [("world child a", 0x3ff0000000000000), ("world child b", 0x3c91a62633145c07), ("world child c", 0x3c91a62633145c07), ("world child d", 0x4000000000000000), ("world child x", 0x4059000000000000), ("world child y", 0x4051800000000000), ("world child rotation", 0x3cef99b70cfba96e), ("world child scale x", 0x3ff0000000000000), ("world child scale y", 0x4000000000000000), ("world child applied", 0xc056800000000000), ("world sheared a", 0x3ff0000000000000), ("world sheared b", 0x3c91a62633145c07), ("world sheared c", 0xbfdffffffffffffe), ("world sheared d", 0x3ffbb67ae8584cab), ("world sheared x", 0x405a400000000000), ("world sheared y", 0x4051800000000000), ("world sheared rotation", 0x3cef99b70cfba96e), ("world sheared scale x", 0x3ff0000000000000), ("world sheared scale y", 0x3ffcd82b446159f4), ("world sheared applied", 0x0000000000000000), ("world odd a", 0x3fe26d0215cdd012), ("world odd b", 0x4000e3311c507398), ("world odd c", 0xbfe10e78b49e0dde), ("world odd d", 0x3fede08316b67cc0), ("world odd x", 0x405c566666666666), ("world odd y", 0x405097365df9e716), ("world odd rotation", 0x4052af83c587dbcb), ("world odd scale x", 0x400181240caf711d), ("world odd scale y", 0x3ff133929cb533df), ("world odd applied", 0x4040a66666666666), ("ik positive upper a", 0x3fefde7057d48a99), ("ik positive upper b", 0x3fb726284424fbbf), ("ik positive upper c", 0xbfb726284424fbbf), ("ik positive upper d", 0x3fefde7057d48a99), ("ik positive upper x", 0x0000000000000000), ("ik positive upper y", 0x0000000000000000), ("ik positive upper rotation", 0x4014c0a0fbf39570), ("ik positive upper scale x", 0x3feffffffffffffe), ("ik positive upper scale y", 0x3feffffffffffffe), ("ik positive upper applied", 0xc0027ebe0818d520), ("ik positive lower a", 0x3fba7de4331d170e), ("ik positive lower b", 0x3fefd404c25a6ddb), ("ik positive lower c", 0xbfefd404c25a6ddb), ("ik positive lower d", 0x3fba7de4331d170e), ("ik positive lower x", 0x4048e5c7c49e0c48), ("ik positive lower y", 0x401215cf753ce4ad), ("ik positive lower rotation", 0x405503da56a4c06c), ("ik positive lower scale x", 0x3ff0000000000000), ("ik positive lower scale y", 0x3ff0000000000000), ("ik positive lower applied", 0x4053b7d046e58715), ("ik negative upper a", 0x3fcc6026a7a268fa), ("ik negative upper b", 0x3fef342bb20546bf), ("ik negative upper c", 0xbfef342bb20546bf), ("ik negative upper d", 0x3fcc6026a7a268fa), ("ik negative upper x", 0x0000000000000000), ("ik negative upper y", 0x0000000000000000), ("ik negative upper rotation", 0x40534c4a1238d55d), ("ik negative upper scale x", 0x3ff0000000000000), ("ik negative upper scale y", 0x3ff0000000000000), ("ik negative upper applied", 0x40516c4a1238d55d), ("ik negative lower a", 0x3feffc7a7a5aa3b9), ("ik negative lower b", 0xbf9e05aa4d8818f8), ("ik negative lower c", 0x3f9e05aa4d8818f8), ("ik negative lower d", 0x3feffc7a7a5aa3b9), ("ik negative lower x", 0x40262b1e32f6e203), ("ik negative lower y", 0x404860c223141f45), ("ik negative lower rotation", 0xbffae18d2b2c6e25), ("ik negative lower scale x", 0x3ff0000000000001), ("ik negative lower scale y", 0x3ff0000000000001), ("ik negative lower applied", 0xc053b7d046e58716), ("ik far upper a", 0xbfc0b5150f6da2ce), ("ik far upper b", 0x3fefb9ea92ec689b), ("ik far upper c", 0xbfefb9ea92ec689b), ("ik far upper d", 0xbfc0b5150f6da2ce), ("ik far upper x", 0x0000000000000000), ("ik far upper y", 0x0000000000000000), ("ik far upper rotation", 0x4058600000000000), ("ik far upper scale x", 0x3fefffffffffffff), ("ik far upper scale y", 0x3fefffffffffffff), ("ik far upper applied", 0x4056800000000000), ("ik far lower a", 0xbfc0b5150f6da2d1), ("ik far lower b", 0x3fefb9ea92ec689b), ("ik far lower c", 0xbfefb9ea92ec689b), ("ik far lower d", 0xbfc0b5150f6da2d1), ("ik far lower x", 0xc01a1af0e81b4e62), ("ik far lower y", 0x4048c93f42c8b1b9), ("ik far lower rotation", 0x4058600000000000), ("ik far lower scale x", 0x3fefffffffffffff), ("ik far lower scale y", 0x3fefffffffffffff), ("ik far lower applied", 0x3cf57c6513cad17a), ("ik odd upper a", 0x3fe77e3e0e5e5180), ("ik odd upper b", 0xbfe5ba2bea3cafc3), ("ik odd upper c", 0x3fe5ba2bea3cafc3), ("ik odd upper d", 0x3fe77e3e0e5e5180), ("ik odd upper x", 0x0000000000000000), ("ik odd upper y", 0x0000000000000000), ("ik odd upper rotation", 0xc04561c02d00f360), ("ik odd upper scale x", 0x3ff0000000000000), ("ik odd upper scale y", 0x3ff0000000000000), ("ik odd upper applied", 0xc04921c02d00f360), ("ik odd lower a", 0x3fecdfd841eff3ac), ("ik odd lower b", 0x3fdb966f3852db76), ("ik odd lower c", 0xbfdb966f3852db76), ("ik odd lower d", 0x3fecdfd841eff3ac), ("ik odd lower x", 0x40425aa07b39afac), ("ik odd lower y", 0xc040f9724eff6950), ("ik odd lower rotation", 0x403988de85bc85d7), ("ik odd lower scale x", 0x3ff0000000000000), ("ik odd lower scale y", 0x3ff0000000000000), ("ik odd lower applied", 0x40511317b7ef9b26), ("ik aim arm a", 0x3fd0907dc1930693), ("ik aim arm b", 0x3feee8dd4748bf14), ("ik aim arm c", 0xbfeee8dd4748bf14), ("ik aim arm d", 0x3fd0907dc1930693), ("ik aim arm x", 0x0000000000000000), ("ik aim arm y", 0x0000000000000000), ("ik aim arm rotation", 0x4052bfffffffffff), ("ik aim arm scale x", 0x3ff0000000000000), ("ik aim arm scale y", 0x3ff0000000000000), ("ik aim arm applied", 0x4046800000000000), ("fade body y", 0xc039000000000000), ("fade leg rotation", 0x4054000000000000), ("fade arm rotation", 0x0000000000000000), ("fade end body y", 0xc03e000000000000), ("fade end leg rotation", 0x4051800000000000), ("fade end arm rotation", 0x0000000000000000), ("bezier body y", 0xc03e000000000000), ("bezier leg rotation", 0x405a4fe8d258d566), ("bezier arm rotation", 0x0000000000000000), ("layer body y", 0xc03e000000000000), ("layer leg rotation", 0x4051800000000000), ("layer arm rotation", 0x403148b439581062), ("layer leg a", 0x3fd5e3a8748a0bf7), ("layer leg b", 0x3fee11f642522d1b), ("layer leg c", 0xbfee11f642522d1b), ("layer leg d", 0x3fd5e3a8748a0bf7), ("layer leg x", 0x0000000000000000), ("layer leg y", 0x0000000000000000), ("layer leg rotation", 0x4051800000000000), ("layer leg scale x", 0x3fefffffffffffff), ("layer leg scale y", 0x3fefffffffffffff), ("layer leg applied", 0x4051800000000000), ("layer arm a", 0x3fee8e1498692520), ("layer arm b", 0x3fd303d24abc7ed0), ("layer arm c", 0xbfd303d24abc7ed0), ("layer arm d", 0x3fee8e1498692520), ("layer arm x", 0x0000000000000000), ("layer arm y", 0x0000000000000000), ("layer arm rotation", 0x403148b439581062), ("layer arm scale x", 0x3fefffffffffffff), ("layer arm scale y", 0x3fefffffffffffff), ("layer arm applied", 0x403148b439581062)];
		let mut value_list: Vec<(String, f64)> = Vec::new();
		fn push_bone(value_list: &mut Vec<(String, f64)>, label: &str, bone: &crate::experimental::skeleton2d::Skeleton2DBone) {
			let entry_list = [("a", bone.get_world_a()), ("b", bone.get_world_b()), ("c", bone.get_world_c()), ("d", bone.get_world_d()), ("x", bone.get_world_x()), ("y", bone.get_world_y()), ("rotation", bone.get_world_rotation()), ("scale x", bone.get_world_scale_x()), ("scale y", bone.get_world_scale_y()), ("applied", bone.get_applied_rotation())];
			for (name, value) in entry_list {
				value_list.push((format!("{} {}", label, name), value));
			}
		}
		let create = |json: serde_json::Value| Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton"));
		// 1. 월드 변환.
		let mut world = Skeleton2D::new(create(serde_json::json!({
			"bones": [
				{ "name": "root", "x": 100, "y": 50, "rotation": 90, "scaleX": 2 },
				{ "name": "child", "parent": "root", "x": 10, "rotation": -90 },
				{ "name": "sheared", "parent": "child", "x": 5, "shearY": 30 },
				{ "name": "odd", "parent": "sheared", "x": 7.3, "y": -2.1, "rotation": 33.3, "scaleX": 1.7, "scaleY": 0.6, "shearX": 12.5, "shearY": -7.25 }
			]
		})));
		world.update_world_transform();
		for name in ["child", "sheared", "odd"] {
			push_bone(&mut value_list, &format!("world {}", name), world.find_bone(name).expect("bone"));
		}
		// 2. IK.
		for (label, is_bend_positive, target_x, target_y, mix) in [("ik positive", true, 60.0, 40.0, 1.0), ("ik negative", false, 60.0, 40.0, 1.0), ("ik far", true, 0.0, 300.0, 1.0), ("ik odd", true, 37.7, -21.3, 0.65)] {
			let mut arm = Skeleton2D::new(create(serde_json::json!({
				"bones": [
					{ "name": "root", "rotation": 7.5 },
					{ "name": "upper", "parent": "root", "length": 50, "rotation": 12.5 },
					{ "name": "lower", "parent": "upper", "x": 50, "length": 43.2, "rotation": -31 },
					{ "name": "target", "parent": "root", "x": target_x, "y": target_y }
				],
				"ik": [{ "name": "armIk", "bones": ["upper", "lower"], "target": "target", "bendPositive": is_bend_positive, "mix": mix }]
			})));
			arm.update_world_transform();
			push_bone(&mut value_list, &format!("{} upper", label), arm.find_bone("upper").expect("upper"));
			push_bone(&mut value_list, &format!("{} lower", label), arm.find_bone("lower").expect("lower"));
		}
		let mut aim = Skeleton2D::new(create(serde_json::json!({
			"bones": [{ "name": "root", "rotation": 30 }, { "name": "arm", "parent": "root", "length": 10 }, { "name": "target", "parent": "root", "x": 0, "y": 10 }],
			"ik": [{ "name": "aim", "bones": ["arm"], "target": "target", "mix": 0.5 }]
		})));
		aim.update_world_transform();
		push_bone(&mut value_list, "ik aim arm", aim.find_bone("arm").expect("arm"));
		// 3. 크로스페이드와 층.
		let animation_data = create(serde_json::json!({
			"bones": [{ "name": "root" }, { "name": "body", "parent": "root", "y": -20 }, { "name": "leg", "parent": "root", "rotation": 90 }, { "name": "arm", "parent": "root" }],
			"animations": {
				"idle": { "duration": 1, "bones": { "body": { "translate": [{ "time": 0, "y": 0 }] } } },
				"walk": {
					"duration": 1,
					"bones": {
						"body": { "translate": [{ "time": 0, "y": -10 }] },
						"leg": { "rotate": [{ "time": 0, "value": -20, "curve": "stepped" }, { "time": 0.5, "value": 20, "curve": [0.42, 0, 0.58, 1] }, { "time": 1, "value": -20 }] }
					}
				},
				"wave": { "bones": { "arm": { "rotate": [{ "time": 0, "value": 0 }, { "time": 1, "value": 80 }] } } }
			}
		}));
		let mut animated = Skeleton2D::new(animation_data.clone());
		let mut animation_state = Skeleton2DAnimationState::new(animation_data);
		let mut push_animated = |value_list: &mut Vec<(String, f64)>, animation_state: &mut Skeleton2DAnimationState, label: &str| {
			animation_state.apply(&mut animated);
			value_list.push((format!("{} body y", label), animated.find_bone("body").expect("body").get_y()));
			value_list.push((format!("{} leg rotation", label), animated.find_bone("leg").expect("leg").get_rotation()));
			value_list.push((format!("{} arm rotation", label), animated.find_bone("arm").expect("arm").get_rotation()));
		};
		animation_state.set_animation(0, "idle", true, 0.0);
		animation_state.update(0.5);
		animation_state.set_animation(0, "walk", true, 0.2);
		animation_state.update(0.1);
		push_animated(&mut value_list, &mut animation_state, "fade");
		animation_state.update(0.15);
		push_animated(&mut value_list, &mut animation_state, "fade end");
		animation_state.update(0.37);
		push_animated(&mut value_list, &mut animation_state, "bezier");
		animation_state.set_animation(1, "wave", true, 0.0);
		animation_state.set_track_alpha(1, 0.5);
		animation_state.update(0.4321);
		push_animated(&mut value_list, &mut animation_state, "layer");
		animated.update_world_transform();
		push_bone(&mut value_list, "layer leg", animated.find_bone("leg").expect("leg"));
		push_bone(&mut value_list, "layer arm", animated.find_bone("arm").expect("arm"));
		assert_eq!(value_list.len(), CHROME_VALUES.len());
		let mismatch_list: Vec<String> = value_list.iter().zip(CHROME_VALUES.iter()).filter(|((label, value), (chrome_label, chrome_bits))| label != chrome_label || value.to_bits() != *chrome_bits).map(|((label, value), (_, chrome_bits))| format!("{}: {:016x} {} (크롬 {:016x} {})", label, value.to_bits(), value, chrome_bits, f64::from_bits(*chrome_bits))).collect();
		assert!(mismatch_list.is_empty(), "크롬과 다른 값 {} 개\n{}", mismatch_list.len(), mismatch_list.join("\n"));
	}
}

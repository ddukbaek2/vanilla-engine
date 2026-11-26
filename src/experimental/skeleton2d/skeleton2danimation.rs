//==============================================================================
//! 2D 뼈대 애니메이션의 곡선, 시간줄, 자세, 클립. (원본 experimental/skeleton2d/skeleton2danimation.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 곡선은 키마다 다음 키까지의 보간이다. 선형, 계단, 3 차 베지어(CSS cubic-bezier 와 같은 [x1, y1, x2, y2], 끝점 (0, 0), (1, 1)) 셋이다.
//! - 뼈 시간줄의 값은 처음 자세에 대한 몫이다. 회전, 이동, 기울임은 더하고 크기는 곱한다. (Spine, DragonBones 와 같은 꼴)
//! - 슬롯 색은 그 값 그대로, 붙임과 그리기 차례는 계단, 사건은 그 시각에 한 번이다.
//! - 자세(Skeleton2DPose)는 뼈 지역 값과 슬롯 상태만 든 배열이다. 클립은 자세에 값을 쓰고, 섞기는 자세끼리 섞는다.
//! - 원본과 다른 점: 시간줄 대상 번호는 `Option<usize>` 다. (원본의 -1 은 None) 베지어 제어점은 `Option<[f64; 4]>` 이다.
//!
//! # 예제
//! 베지어 곡선과 회전 시간줄 하나를 자세에 걸기
//! ```rust
//! use vanilla::experimental::skeleton2d::{evaluate_skeleton2d_curve, Skeleton2DCurveType, Skeleton2DData, Skeleton2DPose, SKELETON2D_BONE_VALUE_COUNT};
//!
//! let eased_value = evaluate_skeleton2d_curve(Skeleton2DCurveType::Bezier, Some([0.42, 0.0, 0.58, 1.0]), 0.5);
//! let json = serde_json::json!({
//! 	"bones": [{ "name": "root" }, { "name": "arm", "parent": "root", "length": 40 }],
//! 	"animations": { "swing": { "bones": { "arm": { "rotate": [{ "time": 0, "value": 0 }, { "time": 1, "value": 90 }] } } } }
//! });
//! let skeleton_data = Skeleton2DData::create_from_json(&json).expect("skeleton");
//! let animation = skeleton_data.find_animation("swing").expect("swing");
//! let mut pose = Skeleton2DPose::new(2, 0);
//! pose.set_to_setup_pose(&skeleton_data);
//! animation.apply(&mut pose, &skeleton_data, 0.5);
//! assert!((eased_value - 0.5).abs() < 0.001);
//! assert!((pose.get_bone_values()[SKELETON2D_BONE_VALUE_COUNT + 2] - 45.0).abs() < 0.001);
//! ```
//==============================================================================
use crate::experimental::skeleton2d::skeleton2ddata::{Skeleton2DBoneData, Skeleton2DData, Skeleton2DSlotData};

/// 자세에서 뼈 하나가 차지하는 값 수. (x, y, 회전, 크기 x, 크기 y, 기울임 x, 기울임 y)
pub const SKELETON2D_BONE_VALUE_COUNT: usize = 7;

/// 베지어 곡선에서 시간 x 로 매개변수 t 를 찾는 반 가르기 횟수. (2^-20 이면 t 의 오차가 백만분의 일쯤)
const BEZIER_BISECTION_COUNT: usize = 20;

//==============================================================================
/// 곡선 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skeleton2DCurveType {
	/// 곧게. (원본 "linear")
	Linear,
	/// 다음 키까지 그 값. (원본 "stepped")
	Stepped,
	/// 3 차 베지어. (원본 "bezier")
	Bezier,
}

//==============================================================================
/// 시간줄 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skeleton2DTimelineType {
	/// 뼈 회전. (값 하나, 도)
	Rotate,
	/// 뼈 이동. (값 둘)
	Translate,
	/// 뼈 크기. (값 둘, 곱한다)
	Scale,
	/// 뼈 기울임. (값 둘, 도)
	Shear,
	/// 슬롯 색. (값 넷, 0 ~ 1)
	Color,
	/// 슬롯 붙임. (계단)
	Attachment,
	/// 그리기 차례. (계단)
	DrawOrder,
	/// 사건.
	Event,
}

//==============================================================================
/// 3 차 베지어 한 축의 값. (끝점 0 과 1, 제어점 first_control, second_control)
//==============================================================================
fn evaluate_bezier_component(first_control: f64, second_control: f64, parameter: f64) -> f64 {
	let inverse = 1.0 - parameter;
	3.0 * inverse * inverse * parameter * first_control + 3.0 * inverse * parameter * parameter * second_control + parameter * parameter * parameter
}

//==============================================================================
/// 곡선 값 셈. (키 사이의 진행 몫 0 ~ 1 을 곡선에 따라 바꾼 몫)
/// - 선형은 그대로, 계단은 다음 키에 닿기 전까지 0 이다.
/// - 베지어는 시간 축 x(t) 가 percent 인 t 를 반 가르기로 찾아 y(t) 를 돌려준다. (x 제어점이 0 ~ 1 이어야 t 가 하나다)
///
/// # 인자
/// - `curve_type`: 곡선 종류.
/// - `curve_values`: 베지어 제어점 [x1, y1, x2, y2]. (베지어가 아니면 쓰지 않는다, None 이면 선형)
/// - `percent`: 키 사이의 진행 몫. (0 ~ 1)
///
/// # 돌려줌
/// 곡선을 지난 몫. (베지어는 0 ~ 1 밖으로 넘을 수 있다)
///
/// # 예제
/// 세 곡선의 값
/// ```rust
/// use vanilla::experimental::skeleton2d::{evaluate_skeleton2d_curve, Skeleton2DCurveType};
///
/// assert_eq!(evaluate_skeleton2d_curve(Skeleton2DCurveType::Linear, None, 0.25), 0.25);
/// assert_eq!(evaluate_skeleton2d_curve(Skeleton2DCurveType::Stepped, None, 0.75), 0.0);
/// let bezier_value = evaluate_skeleton2d_curve(Skeleton2DCurveType::Bezier, Some([0.25, 0.1, 0.25, 1.0]), 0.5);
/// assert!((bezier_value - 0.8024).abs() < 0.001);
/// ```
//==============================================================================
pub fn evaluate_skeleton2d_curve(curve_type: Skeleton2DCurveType, curve_values: Option<[f64; 4]>, percent: f64) -> f64 {
	if curve_type == Skeleton2DCurveType::Stepped {
		return if percent >= 1.0 { 1.0 } else { 0.0 };
	}
	let curve_values = match (curve_type, curve_values) {
		(Skeleton2DCurveType::Bezier, Some(curve_values)) => {
			curve_values
		}
		_ => {
			return percent;
		}
	};
	let mut low_parameter = 0.0f64;
	let mut high_parameter = 1.0f64;
	for _ in 0..BEZIER_BISECTION_COUNT {
		let middle_parameter = (low_parameter + high_parameter) * 0.5;
		let middle_x = evaluate_bezier_component(curve_values[0], curve_values[2], middle_parameter);
		if middle_x < percent {
			low_parameter = middle_parameter;
		}
		else {
			high_parameter = middle_parameter;
		}
	}
	let parameter = (low_parameter + high_parameter) * 0.5;
	evaluate_bezier_component(curve_values[1], curve_values[3], parameter)
}

//==============================================================================
/// 각도 차를 -180 ~ 180 도로 접기.
//==============================================================================
pub(crate) fn wrap_degree(degree: f64) -> f64 {
	let mut wrapped_degree = degree % 360.0;
	if wrapped_degree > 180.0 {
		wrapped_degree -= 360.0;
	}
	else if wrapped_degree < -180.0 {
		wrapped_degree += 360.0;
	}
	wrapped_degree
}

//==============================================================================
/// 2D 뼈대 사건. (클립의 그 시각에 한 번 알린다, 발소리나 타격 판정 자리)
///
/// # 예제
/// 사건 하나 만들기
/// ```rust
/// use vanilla::experimental::skeleton2d::Skeleton2DEvent;
///
/// let event = Skeleton2DEvent::new("step", 0.25, 1, 0.5, "left");
/// assert_eq!(event.get_name(), "step");
/// assert_eq!(event.get_int_value(), 1);
/// assert_eq!(event.get_string_value(), "left");
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton2DEvent {
	name: String,
	time: f64,
	int_value: i32,
	float_value: f64,
	string_value: String,
}

impl Skeleton2DEvent {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `name`: 사건 이름.
	/// - `time`: 클립 안의 시각. (초)
	/// - `int_value`: 정수 값.
	/// - `float_value`: 실수 값.
	/// - `string_value`: 글 값.
	///
	/// # 돌려줌
	/// 사건.
	//==============================================================================
	pub fn new(name: &str, time: f64, int_value: i32, float_value: f64, string_value: &str) -> Skeleton2DEvent {
		Skeleton2DEvent {
			name: name.to_string(),
			time,
			int_value,
			float_value,
			string_value: string_value.to_string(),
		}
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 사건 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 시각 반환.
	///
	/// # 돌려줌
	/// 클립 안의 시각. (초)
	//==============================================================================
	pub fn get_time(&self) -> f64 {
		self.time
	}

	//==============================================================================
	/// 정수 값 반환.
	///
	/// # 돌려줌
	/// 정수 값.
	//==============================================================================
	pub fn get_int_value(&self) -> i32 {
		self.int_value
	}

	//==============================================================================
	/// 실수 값 반환.
	///
	/// # 돌려줌
	/// 실수 값.
	//==============================================================================
	pub fn get_float_value(&self) -> f64 {
		self.float_value
	}

	//==============================================================================
	/// 글 값 반환.
	///
	/// # 돌려줌
	/// 글 값.
	//==============================================================================
	pub fn get_string_value(&self) -> &str {
		&self.string_value
	}
}

//==============================================================================
/// 2D 뼈대 자세. (뼈 지역 값, 슬롯 색, 슬롯 붙임 이름, 그리기 차례)
/// - 뼈 값은 뼈마다 SKELETON2D_BONE_VALUE_COUNT 개: x, y, 회전(도), 크기 x, 크기 y, 기울임 x(도), 기울임 y(도).
/// - 슬롯 색은 슬롯마다 빨강, 초록, 파랑, 투명도 넷이다. (0 ~ 1)
/// - 그리기 차례는 그리는 차례의 슬롯 번호 목록이다.
///
/// # 예제
/// 두 자세를 반씩 섞기 (회전은 짧은 쪽으로)
/// ```rust
/// use vanilla::experimental::skeleton2d::Skeleton2DPose;
///
/// let mut from_pose = Skeleton2DPose::new(1, 0);
/// let mut to_pose = Skeleton2DPose::new(1, 0);
/// from_pose.get_bone_values_mut()[2] = 170.0;
/// to_pose.get_bone_values_mut()[2] = -170.0;
/// from_pose.blend(&to_pose, 0.5);
/// assert!((from_pose.get_bone_values()[2] - 180.0).abs() < 0.001);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DPose {
	bone_values: Vec<f64>,
	slot_colors: Vec<f64>,
	slot_attachment_names: Vec<Option<String>>,
	draw_order: Vec<usize>,
}

impl Skeleton2DPose {
	//==============================================================================
	/// 생성. (뼈 값은 크기 1, 나머지 0, 슬롯 색은 흰색, 붙임은 없음, 그리기 차례는 슬롯 차례)
	///
	/// # 인자
	/// - `bone_count`: 뼈 수.
	/// - `slot_count`: 슬롯 수.
	///
	/// # 돌려줌
	/// 자세.
	//==============================================================================
	pub fn new(bone_count: usize, slot_count: usize) -> Skeleton2DPose {
		let mut bone_values = vec![0.0f64; bone_count * SKELETON2D_BONE_VALUE_COUNT];
		for bone_index in 0..bone_count {
			bone_values[bone_index * SKELETON2D_BONE_VALUE_COUNT + 3] = 1.0;
			bone_values[bone_index * SKELETON2D_BONE_VALUE_COUNT + 4] = 1.0;
		}
		Skeleton2DPose {
			bone_values,
			slot_colors: vec![1.0f64; slot_count * 4],
			slot_attachment_names: vec![None; slot_count],
			draw_order: (0..slot_count).collect(),
		}
	}

	//==============================================================================
	/// 뼈 값 배열 반환. (뼈마다 SKELETON2D_BONE_VALUE_COUNT 개)
	///
	/// # 돌려줌
	/// 뼈 값 배열.
	//==============================================================================
	pub fn get_bone_values(&self) -> &[f64] {
		&self.bone_values
	}

	//==============================================================================
	/// 뼈 값 배열 반환. (변경용)
	///
	/// # 돌려줌
	/// 뼈 값 배열.
	//==============================================================================
	pub fn get_bone_values_mut(&mut self) -> &mut [f64] {
		&mut self.bone_values
	}

	//==============================================================================
	/// 슬롯 색 배열 반환. (슬롯마다 빨강, 초록, 파랑, 투명도)
	///
	/// # 돌려줌
	/// 슬롯 색 배열.
	//==============================================================================
	pub fn get_slot_colors(&self) -> &[f64] {
		&self.slot_colors
	}

	//==============================================================================
	/// 슬롯 색 배열 반환. (변경용)
	///
	/// # 돌려줌
	/// 슬롯 색 배열.
	//==============================================================================
	pub fn get_slot_colors_mut(&mut self) -> &mut [f64] {
		&mut self.slot_colors
	}

	//==============================================================================
	/// 슬롯 붙임 이름 배열 반환. (없으면 None)
	///
	/// # 돌려줌
	/// 슬롯마다의 붙임 이름.
	//==============================================================================
	pub fn get_slot_attachment_names(&self) -> &[Option<String>] {
		&self.slot_attachment_names
	}

	//==============================================================================
	/// 슬롯 붙임 이름 배열 반환. (변경용)
	///
	/// # 돌려줌
	/// 슬롯마다의 붙임 이름.
	//==============================================================================
	pub fn get_slot_attachment_names_mut(&mut self) -> &mut [Option<String>] {
		&mut self.slot_attachment_names
	}

	//==============================================================================
	/// 그리기 차례 반환. (그리는 차례의 슬롯 번호)
	///
	/// # 돌려줌
	/// 그리기 차례.
	//==============================================================================
	pub fn get_draw_order(&self) -> &[usize] {
		&self.draw_order
	}

	//==============================================================================
	/// 그리기 차례 반환. (변경용)
	///
	/// # 돌려줌
	/// 그리기 차례.
	//==============================================================================
	pub fn get_draw_order_mut(&mut self) -> &mut [usize] {
		&mut self.draw_order
	}

	//==============================================================================
	/// 다른 자세를 그대로 옮겨 오기. (뼈와 슬롯 수가 같아야 한다)
	///
	/// # 인자
	/// - `pose`: 옮겨 올 자세.
	//==============================================================================
	pub fn copy_from(&mut self, pose: &Skeleton2DPose) {
		self.bone_values.copy_from_slice(&pose.bone_values);
		self.slot_colors.copy_from_slice(&pose.slot_colors);
		for (own_name, name) in self.slot_attachment_names.iter_mut().zip(pose.slot_attachment_names.iter()) {
			own_name.clone_from(name);
		}
		self.draw_order.copy_from_slice(&pose.draw_order);
	}

	//==============================================================================
	/// 다른 자세 쪽으로 섞기. (이 자세 = 이 자세 + (대상 - 이 자세) x alpha)
	/// - 회전과 기울임은 짧은 쪽으로 돈다. 크기와 이동과 색은 곧게 섞는다.
	/// - 붙임 이름과 그리기 차례는 섞을 수 없어 alpha 가 0.5 이상이면 대상 것을 쓴다.
	/// - alpha 가 1 이상이면 대상을 그대로 옮긴다. 0 이하면 아무것도 하지 않는다.
	///
	/// # 인자
	/// - `target_pose`: 섞을 대상 자세.
	/// - `alpha`: 대상의 몫. (0 ~ 1)
	//==============================================================================
	pub fn blend(&mut self, target_pose: &Skeleton2DPose, alpha: f64) {
		if alpha <= 0.0 {
			return;
		}
		if alpha >= 1.0 {
			self.copy_from(target_pose);
			return;
		}
		for (value_index, (value, target_value)) in self.bone_values.iter_mut().zip(target_pose.bone_values.iter()).enumerate() {
			let component_index = value_index % SKELETON2D_BONE_VALUE_COUNT;
			let is_angle = component_index == 2 || component_index == 5 || component_index == 6;
			let difference = if is_angle { wrap_degree(*target_value - *value) } else { *target_value - *value };
			*value += difference * alpha;
		}
		for (value, target_value) in self.slot_colors.iter_mut().zip(target_pose.slot_colors.iter()) {
			*value += (*target_value - *value) * alpha;
		}
		if alpha >= 0.5 {
			for (own_name, name) in self.slot_attachment_names.iter_mut().zip(target_pose.slot_attachment_names.iter()) {
				own_name.clone_from(name);
			}
			self.draw_order.copy_from_slice(&target_pose.draw_order);
		}
	}

	//==============================================================================
	/// 처음 자세로. (뼈대 자료의 뼈 값, 슬롯 색과 붙임, 슬롯 차례)
	///
	/// # 인자
	/// - `skeleton_data`: 뼈대 자료. (뼈와 슬롯 수가 이 자세와 같아야 한다)
	//==============================================================================
	pub fn set_to_setup_pose(&mut self, skeleton_data: &Skeleton2DData) {
		for (bone_index, bone_data) in skeleton_data.get_bones().iter().enumerate() {
			write_setup_bone_values(bone_data, &mut self.bone_values, bone_index * SKELETON2D_BONE_VALUE_COUNT);
		}
		for slot_data in skeleton_data.get_slots() {
			self.reset_slot_to_setup(slot_data);
		}
		for (order_index, slot_index) in self.draw_order.iter_mut().enumerate() {
			*slot_index = order_index;
		}
	}

	//==============================================================================
	/// 슬롯 하나의 색과 붙임을 처음 값으로.
	///
	/// # 인자
	/// - `slot_data`: 슬롯 자료.
	//==============================================================================
	pub fn reset_slot_to_setup(&mut self, slot_data: &Skeleton2DSlotData) {
		let slot_index = slot_data.get_index();
		let color = slot_data.get_color();
		self.slot_colors[slot_index * 4] = color.red;
		self.slot_colors[slot_index * 4 + 1] = color.green;
		self.slot_colors[slot_index * 4 + 2] = color.blue;
		self.slot_colors[slot_index * 4 + 3] = color.alpha;
		self.slot_attachment_names[slot_index] = slot_data.get_attachment_name().map(|name| name.to_string());
	}
}

//==============================================================================
/// 뼈 자료의 처음 값을 뼈 값 배열에 적기.
//==============================================================================
fn write_setup_bone_values(bone_data: &Skeleton2DBoneData, bone_values: &mut [f64], offset: usize) {
	bone_values[offset] = bone_data.get_x();
	bone_values[offset + 1] = bone_data.get_y();
	bone_values[offset + 2] = bone_data.get_rotation();
	bone_values[offset + 3] = bone_data.get_scale_x();
	bone_values[offset + 4] = bone_data.get_scale_y();
	bone_values[offset + 5] = bone_data.get_shear_x();
	bone_values[offset + 6] = bone_data.get_shear_y();
}

//==============================================================================
/// 2D 뼈대 시간줄. (뼈 하나나 슬롯 하나의 한 갈래 값, 또는 그리기 차례, 사건)
/// - 키는 시각 차례로 더한다. 처음 키 앞은 처음 키의 값, 마지막 키 뒤는 마지막 키의 값이다.
/// - 값 시간줄(회전 1, 이동, 크기, 기울임 2, 색 4 개)은 키마다 곡선을 둔다. 곡선은 그 키에서 다음 키까지다.
///
/// # 예제
/// 이동 시간줄을 만들어 가운데 시각의 값 읽기
/// ```rust
/// use vanilla::experimental::skeleton2d::{Skeleton2DCurveType, Skeleton2DTimeline, Skeleton2DTimelineType};
///
/// let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Translate, Some(0));
/// timeline.add_value_frame(0.0, &[0.0, 0.0], Skeleton2DCurveType::Linear, None);
/// timeline.add_value_frame(1.0, &[10.0, -20.0], Skeleton2DCurveType::Linear, None);
/// let mut values = [0.0f64; 2];
/// assert!(timeline.sample_values(0.5, &mut values));
/// assert_eq!(values, [5.0, -10.0]);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DTimeline {
	timeline_type: Skeleton2DTimelineType,
	target_index: Option<usize>,
	frame_times: Vec<f64>,
	frame_values: Vec<f64>,
	curve_types: Vec<Skeleton2DCurveType>,
	curve_values: Vec<[f64; 4]>,
	attachment_names: Vec<Option<String>>,
	draw_orders: Vec<Option<Vec<usize>>>,
	events: Vec<Skeleton2DEvent>,
}

impl Skeleton2DTimeline {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `timeline_type`: 시간줄 종류.
	/// - `target_index`: 뼈 번호나 슬롯 번호. (그리기 차례와 사건은 None)
	///
	/// # 돌려줌
	/// 키 없는 시간줄.
	//==============================================================================
	pub fn new(timeline_type: Skeleton2DTimelineType, target_index: Option<usize>) -> Skeleton2DTimeline {
		Skeleton2DTimeline {
			timeline_type,
			target_index,
			frame_times: Vec::new(),
			frame_values: Vec::new(),
			curve_types: Vec::new(),
			curve_values: Vec::new(),
			attachment_names: Vec::new(),
			draw_orders: Vec::new(),
			events: Vec::new(),
		}
	}

	//==============================================================================
	/// 종류 반환.
	///
	/// # 돌려줌
	/// 시간줄 종류.
	//==============================================================================
	pub fn get_timeline_type(&self) -> Skeleton2DTimelineType {
		self.timeline_type
	}

	//==============================================================================
	/// 대상 번호 반환.
	///
	/// # 돌려줌
	/// 뼈 번호나 슬롯 번호. (그리기 차례와 사건은 None)
	//==============================================================================
	pub fn get_target_index(&self) -> Option<usize> {
		self.target_index
	}

	//==============================================================================
	/// 키 하나의 값 수 반환.
	///
	/// # 돌려줌
	/// 회전 1, 이동, 크기, 기울임 2, 색 4, 그 밖 0.
	//==============================================================================
	pub fn get_value_stride(&self) -> usize {
		match self.timeline_type {
			Skeleton2DTimelineType::Rotate => {
				1
			}
			Skeleton2DTimelineType::Translate | Skeleton2DTimelineType::Scale | Skeleton2DTimelineType::Shear => {
				2
			}
			Skeleton2DTimelineType::Color => {
				4
			}
			_ => {
				0
			}
		}
	}

	//==============================================================================
	/// 키 수 반환.
	///
	/// # 돌려줌
	/// 키 수.
	//==============================================================================
	pub fn get_frame_count(&self) -> usize {
		self.frame_times.len()
	}

	//==============================================================================
	/// 키 시각 반환.
	///
	/// # 인자
	/// - `frame_index`: 키 번호. (키 수보다 작아야 한다)
	///
	/// # 돌려줌
	/// 그 키의 시각. (초)
	//==============================================================================
	pub fn get_frame_time(&self, frame_index: usize) -> f64 {
		self.frame_times[frame_index]
	}

	//==============================================================================
	/// 마지막 키의 시각 반환.
	///
	/// # 돌려줌
	/// 마지막 키의 시각. (초, 키가 없으면 0)
	//==============================================================================
	pub fn get_last_frame_time(&self) -> f64 {
		self.frame_times.last().copied().unwrap_or(0.0)
	}

	//==============================================================================
	/// 값 키 더하기. (회전, 이동, 크기, 기울임, 색 시간줄)
	///
	/// # 인자
	/// - `time`: 시각. (초, 앞 키보다 늦어야 한다)
	/// - `values`: 값. (get_value_stride 개, 뼈는 처음 자세에 대한 몫, 색은 0 ~ 1 의 빨강, 초록, 파랑, 투명도)
	/// - `curve_type`: 다음 키까지의 곡선.
	/// - `curve_values`: 베지어 제어점 [x1, y1, x2, y2]. (x 는 0 ~ 1 로 자른다, 베지어가 아니면 None)
	//==============================================================================
	pub fn add_value_frame(&mut self, time: f64, values: &[f64], curve_type: Skeleton2DCurveType, curve_values: Option<[f64; 4]>) {
		let value_stride = self.get_value_stride();
		self.frame_times.push(time);
		self.frame_values.extend_from_slice(&values[..value_stride]);
		self.curve_types.push(curve_type);
		let stored_curve_values = match (curve_type, curve_values) {
			(Skeleton2DCurveType::Bezier, Some(curve_values)) => {
				[curve_values[0].clamp(0.0, 1.0), curve_values[1], curve_values[2].clamp(0.0, 1.0), curve_values[3]]
			}
			_ => {
				[0.0, 0.0, 1.0, 1.0]
			}
		};
		self.curve_values.push(stored_curve_values);
	}

	//==============================================================================
	/// 붙임 키 더하기. (붙임 시간줄, 계단)
	///
	/// # 인자
	/// - `time`: 시각. (초, 앞 키보다 늦어야 한다)
	/// - `attachment_name`: 그 시각부터 보일 붙임 이름. (None 이면 숨긴다)
	//==============================================================================
	pub fn add_attachment_frame(&mut self, time: f64, attachment_name: Option<&str>) {
		self.frame_times.push(time);
		self.attachment_names.push(attachment_name.map(|name| name.to_string()));
	}

	//==============================================================================
	/// 그리기 차례 키 더하기. (그리기 차례 시간줄, 계단)
	///
	/// # 인자
	/// - `time`: 시각. (초, 앞 키보다 늦어야 한다)
	/// - `draw_order`: 그리는 차례의 슬롯 번호 모두. (None 이면 처음 차례)
	//==============================================================================
	pub fn add_draw_order_frame(&mut self, time: f64, draw_order: Option<Vec<usize>>) {
		self.frame_times.push(time);
		self.draw_orders.push(draw_order);
	}

	//==============================================================================
	/// 사건 키 더하기. (사건 시간줄)
	///
	/// # 인자
	/// - `event`: 사건. (시각은 사건이 든다, 앞 사건보다 늦거나 같아야 한다)
	//==============================================================================
	pub fn add_event_frame(&mut self, event: Skeleton2DEvent) {
		self.frame_times.push(event.get_time());
		self.events.push(event);
	}

	//==============================================================================
	/// 사건 목록 반환.
	///
	/// # 돌려줌
	/// 사건 목록. (시각 차례)
	//==============================================================================
	pub fn get_events(&self) -> &[Skeleton2DEvent] {
		&self.events
	}

	//==============================================================================
	/// 그 시각에 걸리는 키 번호.
	///
	/// # 인자
	/// - `time`: 시각. (초)
	///
	/// # 돌려줌
	/// 시각이 그 키 이상인 마지막 키. (처음 키 앞이면 0, 키가 없으면 None)
	//==============================================================================
	pub fn find_frame_index(&self, time: f64) -> Option<usize> {
		if self.frame_times.is_empty() {
			return None;
		}
		let mut low_index = 0usize;
		let mut high_index = self.frame_times.len() - 1;
		while low_index < high_index {
			let middle_index = (low_index + high_index + 1) / 2;
			if self.frame_times[middle_index] <= time {
				low_index = middle_index;
			}
			else {
				high_index = middle_index - 1;
			}
		}
		Some(low_index)
	}

	//==============================================================================
	/// 그 시각의 값 읽기. (값 시간줄, get_value_stride 개를 output 에 적는다)
	///
	/// # 인자
	/// - `time`: 시각. (초)
	/// - `output`: 값을 적을 곳. (get_value_stride 개 이상)
	///
	/// # 돌려줌
	/// 적었으면 true. (키가 없으면 적지 않고 false)
	//==============================================================================
	pub fn sample_values(&self, time: f64, output: &mut [f64]) -> bool {
		let frame_index = match self.find_frame_index(time) {
			Some(frame_index) => {
				frame_index
			}
			None => {
				return false;
			}
		};
		let value_stride = self.get_value_stride();
		let offset = frame_index * value_stride;
		let is_last_frame = frame_index == self.frame_times.len() - 1;
		if is_last_frame || time <= self.frame_times[frame_index] {
			output[..value_stride].copy_from_slice(&self.frame_values[offset..offset + value_stride]);
			return true;
		}
		let frame_time = self.frame_times[frame_index];
		let next_frame_time = self.frame_times[frame_index + 1];
		let span = next_frame_time - frame_time;
		let percent = if span > 0.0 { (time - frame_time) / span } else { 1.0 };
		let eased_percent = evaluate_skeleton2d_curve(self.curve_types[frame_index], Some(self.curve_values[frame_index]), percent);
		let next_offset = offset + value_stride;
		for value_index in 0..value_stride {
			let value = self.frame_values[offset + value_index];
			let next_value = self.frame_values[next_offset + value_index];
			output[value_index] = value + (next_value - value) * eased_percent;
		}
		true
	}

	//==============================================================================
	/// 자세에 걸기. (뼈는 처음 값에 몫을 더하거나 곱하고, 슬롯 색과 붙임과 그리기 차례는 그 값으로)
	///
	/// # 인자
	/// - `pose`: 값을 쓸 자세.
	/// - `skeleton_data`: 뼈대 자료. (처음 값을 읽는다)
	/// - `time`: 클립 안의 시각. (초)
	//==============================================================================
	pub fn apply(&self, pose: &mut Skeleton2DPose, skeleton_data: &Skeleton2DData, time: f64) {
		match self.timeline_type {
			Skeleton2DTimelineType::Rotate | Skeleton2DTimelineType::Translate | Skeleton2DTimelineType::Scale | Skeleton2DTimelineType::Shear => {
				let target_index = match self.target_index {
					Some(target_index) => {
						target_index
					}
					None => {
						return;
					}
				};
				let mut values = [0.0f64; 2];
				if !self.sample_values(time, &mut values) {
					return;
				}
				let bone_data = &skeleton_data.get_bones()[target_index];
				let offset = target_index * SKELETON2D_BONE_VALUE_COUNT;
				let bone_values = pose.get_bone_values_mut();
				match self.timeline_type {
					Skeleton2DTimelineType::Rotate => {
						bone_values[offset + 2] = bone_data.get_rotation() + values[0];
					}
					Skeleton2DTimelineType::Translate => {
						bone_values[offset] = bone_data.get_x() + values[0];
						bone_values[offset + 1] = bone_data.get_y() + values[1];
					}
					Skeleton2DTimelineType::Scale => {
						bone_values[offset + 3] = bone_data.get_scale_x() * values[0];
						bone_values[offset + 4] = bone_data.get_scale_y() * values[1];
					}
					_ => {
						bone_values[offset + 5] = bone_data.get_shear_x() + values[0];
						bone_values[offset + 6] = bone_data.get_shear_y() + values[1];
					}
				}
			}
			Skeleton2DTimelineType::Color => {
				let target_index = match self.target_index {
					Some(target_index) => {
						target_index
					}
					None => {
						return;
					}
				};
				let mut values = [1.0f64; 4];
				if !self.sample_values(time, &mut values) {
					return;
				}
				pose.get_slot_colors_mut()[target_index * 4..target_index * 4 + 4].copy_from_slice(&values);
			}
			Skeleton2DTimelineType::Attachment => {
				let (Some(target_index), Some(frame_index)) = (self.target_index, self.find_frame_index(time)) else {
					return;
				};
				pose.get_slot_attachment_names_mut()[target_index].clone_from(&self.attachment_names[frame_index]);
			}
			Skeleton2DTimelineType::DrawOrder => {
				let frame_index = match self.find_frame_index(time) {
					Some(frame_index) => {
						frame_index
					}
					None => {
						return;
					}
				};
				let frame_draw_order = &self.draw_orders[frame_index];
				for (order_index, slot_index) in pose.get_draw_order_mut().iter_mut().enumerate() {
					*slot_index = match frame_draw_order {
						Some(frame_draw_order) => {
							frame_draw_order[order_index]
						}
						None => {
							order_index
						}
					};
				}
			}
			Skeleton2DTimelineType::Event => {}
		}
	}

	//==============================================================================
	/// 이 시간줄이 움직이는 값을 처음 값으로. (사건은 아무것도 하지 않는다)
	///
	/// # 인자
	/// - `pose`: 값을 쓸 자세.
	/// - `skeleton_data`: 뼈대 자료.
	//==============================================================================
	pub fn reset_to_setup(&self, pose: &mut Skeleton2DPose, skeleton_data: &Skeleton2DData) {
		match self.timeline_type {
			Skeleton2DTimelineType::Rotate | Skeleton2DTimelineType::Translate | Skeleton2DTimelineType::Scale | Skeleton2DTimelineType::Shear => {
				let Some(target_index) = self.target_index else {
					return;
				};
				let bone_data = &skeleton_data.get_bones()[target_index];
				let offset = target_index * SKELETON2D_BONE_VALUE_COUNT;
				let bone_values = pose.get_bone_values_mut();
				match self.timeline_type {
					Skeleton2DTimelineType::Rotate => {
						bone_values[offset + 2] = bone_data.get_rotation();
					}
					Skeleton2DTimelineType::Translate => {
						bone_values[offset] = bone_data.get_x();
						bone_values[offset + 1] = bone_data.get_y();
					}
					Skeleton2DTimelineType::Scale => {
						bone_values[offset + 3] = bone_data.get_scale_x();
						bone_values[offset + 4] = bone_data.get_scale_y();
					}
					_ => {
						bone_values[offset + 5] = bone_data.get_shear_x();
						bone_values[offset + 6] = bone_data.get_shear_y();
					}
				}
			}
			Skeleton2DTimelineType::Color => {
				let Some(target_index) = self.target_index else {
					return;
				};
				let color = skeleton_data.get_slots()[target_index].get_color();
				pose.get_slot_colors_mut()[target_index * 4..target_index * 4 + 4].copy_from_slice(&[color.red, color.green, color.blue, color.alpha]);
			}
			Skeleton2DTimelineType::Attachment => {
				let Some(target_index) = self.target_index else {
					return;
				};
				let attachment_name = skeleton_data.get_slots()[target_index].get_attachment_name().map(|name| name.to_string());
				pose.get_slot_attachment_names_mut()[target_index] = attachment_name;
			}
			Skeleton2DTimelineType::DrawOrder => {
				for (order_index, slot_index) in pose.get_draw_order_mut().iter_mut().enumerate() {
					*slot_index = order_index;
				}
			}
			Skeleton2DTimelineType::Event => {}
		}
	}
}

//==============================================================================
/// 2D 뼈대 클립. (이름, 길이, 시간줄 목록)
/// - apply 는 시각을 그대로 쓴다. 되풀이와 끝에서 멈추기는 부르는 쪽(Skeleton2DAnimationState)이 시각을 접어 준다.
///
/// # 예제
/// 코드로 클립을 지어 자세에 걸기
/// ```rust
/// use vanilla::experimental::skeleton2d::{Skeleton2DAnimation, Skeleton2DCurveType, Skeleton2DData, Skeleton2DPose, Skeleton2DTimeline, Skeleton2DTimelineType};
///
/// let json = serde_json::json!({ "bones": [{ "name": "root" }, { "name": "body", "parent": "root" }] });
/// let skeleton_data = Skeleton2DData::create_from_json(&json).expect("skeleton");
/// let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Translate, Some(1));
/// timeline.add_value_frame(0.0, &[0.0, 0.0], Skeleton2DCurveType::Linear, None);
/// timeline.add_value_frame(1.0, &[0.0, -10.0], Skeleton2DCurveType::Linear, None);
/// let mut animation = Skeleton2DAnimation::new("jump", 1.0);
/// animation.add_timeline(timeline);
/// let mut pose = Skeleton2DPose::new(2, 0);
/// pose.set_to_setup_pose(&skeleton_data);
/// animation.apply(&mut pose, &skeleton_data, 0.5);
/// assert_eq!(pose.get_bone_values()[8], -5.0);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DAnimation {
	name: String,
	duration: f64,
	timelines: Vec<Skeleton2DTimeline>,
}

impl Skeleton2DAnimation {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `name`: 클립 이름.
	/// - `duration`: 길이. (초)
	///
	/// # 돌려줌
	/// 시간줄 없는 클립.
	//==============================================================================
	pub fn new(name: &str, duration: f64) -> Skeleton2DAnimation {
		Skeleton2DAnimation {
			name: name.to_string(),
			duration,
			timelines: Vec::new(),
		}
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 클립 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 길이 반환.
	///
	/// # 돌려줌
	/// 길이. (초)
	//==============================================================================
	pub fn get_duration(&self) -> f64 {
		self.duration
	}

	//==============================================================================
	/// 길이 설정.
	///
	/// # 인자
	/// - `duration`: 길이. (초)
	//==============================================================================
	pub fn set_duration(&mut self, duration: f64) {
		self.duration = duration;
	}

	//==============================================================================
	/// 시간줄 더하기.
	///
	/// # 인자
	/// - `timeline`: 더할 시간줄.
	//==============================================================================
	pub fn add_timeline(&mut self, timeline: Skeleton2DTimeline) {
		self.timelines.push(timeline);
	}

	//==============================================================================
	/// 시간줄 목록 반환.
	///
	/// # 돌려줌
	/// 시간줄 목록.
	//==============================================================================
	pub fn get_timelines(&self) -> &[Skeleton2DTimeline] {
		&self.timelines
	}

	//==============================================================================
	/// 자세에 걸기. (시간줄마다 apply)
	///
	/// # 인자
	/// - `pose`: 값을 쓸 자세.
	/// - `skeleton_data`: 뼈대 자료.
	/// - `time`: 클립 안의 시각. (초)
	//==============================================================================
	pub fn apply(&self, pose: &mut Skeleton2DPose, skeleton_data: &Skeleton2DData, time: f64) {
		for timeline in &self.timelines {
			timeline.apply(pose, skeleton_data, time);
		}
	}

	//==============================================================================
	/// 이 클립이 움직이는 값을 처음 값으로. (시간줄마다 reset_to_setup)
	///
	/// # 인자
	/// - `pose`: 값을 쓸 자세.
	/// - `skeleton_data`: 뼈대 자료.
	//==============================================================================
	pub fn reset_keyed_to_setup(&self, pose: &mut Skeleton2DPose, skeleton_data: &Skeleton2DData) {
		for timeline in &self.timelines {
			timeline.reset_to_setup(pose, skeleton_data);
		}
	}

	//==============================================================================
	/// 두 시각 사이의 사건 모으기. (from_time 초과 to_time 이하, from_time 이 음수면 0 의 사건도 든다)
	///
	/// # 인자
	/// - `from_time`: 앞 시각. (초, 이 시각의 사건은 빠진다)
	/// - `to_time`: 뒤 시각. (초, 이 시각의 사건은 든다)
	/// - `output`: 사건을 덧붙일 목록. (복사해 담는다)
	//==============================================================================
	pub fn collect_events(&self, from_time: f64, to_time: f64, output: &mut Vec<Skeleton2DEvent>) {
		for timeline in &self.timelines {
			if timeline.get_timeline_type() != Skeleton2DTimelineType::Event {
				continue;
			}
			for event in timeline.get_events() {
				let event_time = event.get_time();
				if event_time > from_time && event_time <= to_time {
					output.push(event.clone());
				}
			}
		}
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 곡선 보간. (원본 확인 스크립트 2 번과 같은 값)
	//==============================================================================
	#[test]
	fn curve_values_match_reference() {
		assert!((evaluate_skeleton2d_curve(Skeleton2DCurveType::Linear, None, 0.3) - 0.3).abs() < 0.0005);
		assert_eq!(evaluate_skeleton2d_curve(Skeleton2DCurveType::Stepped, None, 0.99), 0.0);
		assert!((evaluate_skeleton2d_curve(Skeleton2DCurveType::Bezier, Some([0.42, 0.0, 0.58, 1.0]), 0.5) - 0.5).abs() < 0.0005);
		assert!((evaluate_skeleton2d_curve(Skeleton2DCurveType::Bezier, Some([0.25, 0.1, 0.25, 1.0]), 0.5) - 0.8024034).abs() < 0.0005);
		assert!((evaluate_skeleton2d_curve(Skeleton2DCurveType::Bezier, Some([0.25, 0.1, 0.25, 1.0]), 1.0) - 1.0).abs() < 0.0005);
	}

	//==============================================================================
	/// 짧은 쪽 회전 섞기와 계단 값.
	//==============================================================================
	#[test]
	fn pose_blend_takes_short_rotation() {
		let mut pose = Skeleton2DPose::new(1, 1);
		let mut target_pose = Skeleton2DPose::new(1, 1);
		pose.get_bone_values_mut()[2] = 170.0;
		target_pose.get_bone_values_mut()[2] = -170.0;
		target_pose.get_slot_attachment_names_mut()[0] = Some(String::from("b"));
		pose.blend(&target_pose, 0.4);
		assert!((pose.get_bone_values()[2] - 178.0).abs() < 0.0005);
		assert_eq!(pose.get_slot_attachment_names()[0], None);
		pose.blend(&target_pose, 0.5);
		assert_eq!(pose.get_slot_attachment_names()[0].as_deref(), Some("b"));
	}
}

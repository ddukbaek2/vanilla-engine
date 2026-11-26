//==============================================================================
//! 2D 뼈대 인스턴스. (원본 experimental/skeleton2d/skeleton2d.js, 자료 하나로 지은 뼈, 슬롯, IK 의 지금 자세, 월드 행렬, 붙임 꼭짓점)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 뼈의 지역 값(이동, 회전, 크기, 기울임)은 클립이나 코드가 쓴다. update_world_transform 이 부모부터 월드 행렬을 셈하고 IK 를 건다.
//! - IK 는 지역 값을 바꾸지 않고 '걸린 회전'(get_applied_rotation)만 바꾼다. 다음 셈은 다시 지역 값에서 시작해 쌓이지 않는다.
//! - 뼈 하나 IK 는 뼈 x 축이 목표를 보게 돌린다. 뼈 둘 IK 는 코사인 법칙으로 부모를 돌려 팔꿈치를 놓고 자식이 목표를 보게 돌린다.
//!   닿지 않으면 곧게 편다. 세기(mix)로 원래 회전과 섞는다. 할아버지 뼈가 뒤집혀 있으면(행렬식 음수) 꺾임 쪽을 뒤집는다. (DragonBones 와 같다)
//! - 원본과 다른 점: 뼈, 슬롯, IK 인스턴스는 제 자료를 복사해 든다. 고른 스킨은 번호로 든다. IK 걸기는 제약 번호로 부른다.
//!
//! # 예제
//! 팔 두 뼈를 IK 로 목표에 닿게 하기
//! ```rust
//! use std::rc::Rc;
//! use vanilla::experimental::skeleton2d::{Skeleton2D, Skeleton2DData};
//!
//! let json = serde_json::json!({
//! 	"bones": [
//! 		{ "name": "root" },
//! 		{ "name": "upper", "parent": "root", "length": 50 },
//! 		{ "name": "lower", "parent": "upper", "x": 50, "length": 50 },
//! 		{ "name": "target", "parent": "root", "x": 60, "y": 40 }
//! 	],
//! 	"ik": [{ "name": "armIk", "bones": ["upper", "lower"], "target": "target", "bendPositive": true }]
//! });
//! let mut skeleton = Skeleton2D::new(Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton")));
//! skeleton.update_world_transform();
//! let tip = skeleton.find_bone("lower").expect("lower").local_to_world(50.0, 0.0);
//! assert!((tip.x - 60.0).abs() < 0.01 && (tip.y - 40.0).abs() < 0.01);
//! ```
//==============================================================================
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::experimental::skeleton2d::skeleton2danimation::{wrap_degree, Skeleton2DPose, SKELETON2D_BONE_VALUE_COUNT};
use crate::experimental::skeleton2d::skeleton2ddata::{Skeleton2DAttachment, Skeleton2DBoneData, Skeleton2DData, Skeleton2DIkConstraintData, Skeleton2DSlotData};
use std::rc::Rc;

/// IK 가 길이 0 으로 보는 값.
const IK_EPSILON: f32 = 0.00001;

//==============================================================================
/// 뼈. (지역 값, IK 가 건 회전, 월드 행렬)
/// - 월드 행렬: 월드 x = a x 지역 x + c x 지역 y + 월드 x 자리, 월드 y = b x 지역 x + d x 지역 y + 월드 y 자리.
///
/// # 예제
/// 뼈를 돌리고 뼈 끝의 월드 자리 읽기
/// ```rust
/// use std::rc::Rc;
/// use vanilla::experimental::skeleton2d::{Skeleton2D, Skeleton2DData};
///
/// let json = serde_json::json!({ "bones": [{ "name": "root" }, { "name": "arm", "parent": "root", "x": 10, "length": 20 }] });
/// let mut skeleton = Skeleton2D::new(Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton")));
/// skeleton.find_bone_mut("arm").expect("arm").set_rotation(90.0);
/// skeleton.update_world_transform();
/// let arm_bone = skeleton.find_bone("arm").expect("arm");
/// let tip = arm_bone.local_to_world(20.0, 0.0);
/// let local = arm_bone.world_to_local(tip.x, tip.y);
/// assert!((arm_bone.get_world_rotation() - 90.0).abs() < 0.001);
/// assert!((tip.x - 10.0).abs() < 0.001 && (tip.y - 20.0).abs() < 0.001);
/// assert!((local.x - 20.0).abs() < 0.001);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DBone {
	data: Skeleton2DBoneData,
	x: f32,
	y: f32,
	rotation: f32,
	scale_x: f32,
	scale_y: f32,
	shear_x: f32,
	shear_y: f32,
	applied_rotation: f32,
	world_a: f32,
	world_b: f32,
	world_c: f32,
	world_d: f32,
	world_x: f32,
	world_y: f32,
}

impl Skeleton2DBone {
	//==============================================================================
	/// 생성. (처음 자세, 월드 행렬은 단위 행렬)
	///
	/// # 인자
	/// - `bone_data`: 뼈 자료. (복사해 든다)
	///
	/// # 돌려줌
	/// 뼈.
	//==============================================================================
	pub fn new(bone_data: &Skeleton2DBoneData) -> Skeleton2DBone {
		let mut bone = Skeleton2DBone {
			data: bone_data.clone(),
			x: 0.0,
			y: 0.0,
			rotation: 0.0,
			scale_x: 1.0,
			scale_y: 1.0,
			shear_x: 0.0,
			shear_y: 0.0,
			applied_rotation: 0.0,
			world_a: 1.0,
			world_b: 0.0,
			world_c: 0.0,
			world_d: 1.0,
			world_x: 0.0,
			world_y: 0.0,
		};
		bone.set_to_setup_pose();
		bone
	}

	//==============================================================================
	/// 자료 반환.
	///
	/// # 돌려줌
	/// 뼈 자료.
	//==============================================================================
	pub fn get_data(&self) -> &Skeleton2DBoneData {
		&self.data
	}

	//==============================================================================
	/// 번호 반환.
	///
	/// # 돌려줌
	/// 뼈 번호.
	//==============================================================================
	pub fn get_index(&self) -> usize {
		self.data.get_index()
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 뼈 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		self.data.get_name()
	}

	//==============================================================================
	/// 부모 번호 반환.
	///
	/// # 돌려줌
	/// 부모 뼈 번호. (뿌리는 None)
	//==============================================================================
	pub fn get_parent_index(&self) -> Option<usize> {
		self.data.get_parent_index()
	}

	//==============================================================================
	/// 처음 자세로. (지역 값을 자료 값으로)
	//==============================================================================
	pub fn set_to_setup_pose(&mut self) {
		self.x = self.data.get_x();
		self.y = self.data.get_y();
		self.rotation = self.data.get_rotation();
		self.scale_x = self.data.get_scale_x();
		self.scale_y = self.data.get_scale_y();
		self.shear_x = self.data.get_shear_x();
		self.shear_y = self.data.get_shear_y();
		self.applied_rotation = self.rotation;
	}

	//==============================================================================
	/// 지역 x 반환.
	///
	/// # 돌려줌
	/// 부모 기준 x.
	//==============================================================================
	pub fn get_x(&self) -> f32 {
		self.x
	}

	//==============================================================================
	/// 지역 y 반환.
	///
	/// # 돌려줌
	/// 부모 기준 y.
	//==============================================================================
	pub fn get_y(&self) -> f32 {
		self.y
	}

	//==============================================================================
	/// 지역 자리 설정.
	///
	/// # 인자
	/// - `x`: 부모 기준 x.
	/// - `y`: 부모 기준 y.
	//==============================================================================
	pub fn set_position(&mut self, x: f32, y: f32) {
		self.x = x;
		self.y = y;
	}

	//==============================================================================
	/// 지역 회전 반환.
	///
	/// # 돌려줌
	/// 회전. (도)
	//==============================================================================
	pub fn get_rotation(&self) -> f32 {
		self.rotation
	}

	//==============================================================================
	/// 지역 회전 설정.
	///
	/// # 인자
	/// - `rotation`: 회전. (도)
	//==============================================================================
	pub fn set_rotation(&mut self, rotation: f32) {
		self.rotation = rotation;
	}

	//==============================================================================
	/// 크기 x 반환.
	///
	/// # 돌려줌
	/// 크기 x.
	//==============================================================================
	pub fn get_scale_x(&self) -> f32 {
		self.scale_x
	}

	//==============================================================================
	/// 크기 y 반환.
	///
	/// # 돌려줌
	/// 크기 y.
	//==============================================================================
	pub fn get_scale_y(&self) -> f32 {
		self.scale_y
	}

	//==============================================================================
	/// 크기 설정.
	///
	/// # 인자
	/// - `scale_x`: 크기 x.
	/// - `scale_y`: 크기 y.
	//==============================================================================
	pub fn set_scale(&mut self, scale_x: f32, scale_y: f32) {
		self.scale_x = scale_x;
		self.scale_y = scale_y;
	}

	//==============================================================================
	/// 기울임 x 반환.
	///
	/// # 돌려줌
	/// 기울임 x. (도)
	//==============================================================================
	pub fn get_shear_x(&self) -> f32 {
		self.shear_x
	}

	//==============================================================================
	/// 기울임 y 반환.
	///
	/// # 돌려줌
	/// 기울임 y. (도)
	//==============================================================================
	pub fn get_shear_y(&self) -> f32 {
		self.shear_y
	}

	//==============================================================================
	/// 기울임 설정.
	///
	/// # 인자
	/// - `shear_x`: 기울임 x. (도)
	/// - `shear_y`: 기울임 y. (도)
	//==============================================================================
	pub fn set_shear(&mut self, shear_x: f32, shear_y: f32) {
		self.shear_x = shear_x;
		self.shear_y = shear_y;
	}

	//==============================================================================
	/// 걸린 회전 반환. (IK 가 바꾼 회전, IK 가 없으면 지역 회전, update_world_transform 뒤의 값)
	///
	/// # 돌려줌
	/// 걸린 회전. (도)
	//==============================================================================
	pub fn get_applied_rotation(&self) -> f32 {
		self.applied_rotation
	}

	//==============================================================================
	/// 걸린 회전 설정. (Skeleton2D 가 월드 행렬 셈과 IK 에서 쓴다)
	///
	/// # 인자
	/// - `applied_rotation`: 걸린 회전. (도)
	//==============================================================================
	pub fn set_applied_rotation(&mut self, applied_rotation: f32) {
		self.applied_rotation = applied_rotation;
	}

	//==============================================================================
	/// 월드 행렬 설정. (Skeleton2D 가 셈해 넣는다)
	///
	/// # 인자
	/// - `world_a`: 월드 행렬 a.
	/// - `world_b`: 월드 행렬 b.
	/// - `world_c`: 월드 행렬 c.
	/// - `world_d`: 월드 행렬 d.
	/// - `world_x`: 월드 x 자리.
	/// - `world_y`: 월드 y 자리.
	//==============================================================================
	pub fn set_world_transform(&mut self, world_a: f32, world_b: f32, world_c: f32, world_d: f32, world_x: f32, world_y: f32) {
		self.world_a = world_a;
		self.world_b = world_b;
		self.world_c = world_c;
		self.world_d = world_d;
		self.world_x = world_x;
		self.world_y = world_y;
	}

	//==============================================================================
	/// 월드 행렬 a 반환. (지역 x 축의 월드 x)
	///
	/// # 돌려줌
	/// 월드 행렬 a.
	//==============================================================================
	pub fn get_world_a(&self) -> f32 {
		self.world_a
	}

	//==============================================================================
	/// 월드 행렬 b 반환. (지역 x 축의 월드 y)
	///
	/// # 돌려줌
	/// 월드 행렬 b.
	//==============================================================================
	pub fn get_world_b(&self) -> f32 {
		self.world_b
	}

	//==============================================================================
	/// 월드 행렬 c 반환. (지역 y 축의 월드 x)
	///
	/// # 돌려줌
	/// 월드 행렬 c.
	//==============================================================================
	pub fn get_world_c(&self) -> f32 {
		self.world_c
	}

	//==============================================================================
	/// 월드 행렬 d 반환. (지역 y 축의 월드 y)
	///
	/// # 돌려줌
	/// 월드 행렬 d.
	//==============================================================================
	pub fn get_world_d(&self) -> f32 {
		self.world_d
	}

	//==============================================================================
	/// 월드 x 자리 반환.
	///
	/// # 돌려줌
	/// 월드 x 자리. (뼈대 기준)
	//==============================================================================
	pub fn get_world_x(&self) -> f32 {
		self.world_x
	}

	//==============================================================================
	/// 월드 y 자리 반환.
	///
	/// # 돌려줌
	/// 월드 y 자리. (뼈대 기준)
	//==============================================================================
	pub fn get_world_y(&self) -> f32 {
		self.world_y
	}

	//==============================================================================
	/// 월드 회전 반환. (지역 x 축이 월드에서 가리키는 각)
	///
	/// # 돌려줌
	/// 월드 회전. (도, -180 ~ 180)
	//==============================================================================
	pub fn get_world_rotation(&self) -> f32 {
		self.world_b.atan2(self.world_a).to_degrees()
	}

	//==============================================================================
	/// 월드 크기 x 반환. (지역 x 축의 길이)
	///
	/// # 돌려줌
	/// 월드 크기 x.
	//==============================================================================
	pub fn get_world_scale_x(&self) -> f32 {
		self.world_a.hypot(self.world_b)
	}

	//==============================================================================
	/// 월드 크기 y 반환. (지역 y 축의 길이)
	///
	/// # 돌려줌
	/// 월드 크기 y.
	//==============================================================================
	pub fn get_world_scale_y(&self) -> f32 {
		self.world_c.hypot(self.world_d)
	}

	//==============================================================================
	/// 뼈 기준 자리를 월드 자리로.
	///
	/// # 인자
	/// - `local_x`: 뼈 기준 x.
	/// - `local_y`: 뼈 기준 y.
	///
	/// # 돌려줌
	/// 월드 자리. (뼈대 기준)
	//==============================================================================
	pub fn local_to_world(&self, local_x: f32, local_y: f32) -> Vector2 {
		Vector2::create(self.world_a * local_x + self.world_c * local_y + self.world_x, self.world_b * local_x + self.world_d * local_y + self.world_y)
	}

	//==============================================================================
	/// 월드 자리를 뼈 기준 자리로.
	///
	/// # 인자
	/// - `world_x`: 월드 x. (뼈대 기준)
	/// - `world_y`: 월드 y. (뼈대 기준)
	///
	/// # 돌려줌
	/// 뼈 기준 자리. (행렬을 되돌릴 수 없으면 0, 0)
	//==============================================================================
	pub fn world_to_local(&self, world_x: f32, world_y: f32) -> Vector2 {
		let determinant = self.world_a * self.world_d - self.world_b * self.world_c;
		if determinant == 0.0 {
			return Vector2::create(0.0, 0.0);
		}
		let difference_x = world_x - self.world_x;
		let difference_y = world_y - self.world_y;
		Vector2::create((self.world_d * difference_x - self.world_c * difference_y) / determinant, (self.world_a * difference_y - self.world_b * difference_x) / determinant)
	}
}

//==============================================================================
/// 슬롯. (지금 색과 붙임 이름)
///
/// # 예제
/// 슬롯 색 바꾸기
/// ```rust
/// use std::rc::Rc;
/// use vanilla::experimental::skeleton2d::{Skeleton2D, Skeleton2DData};
/// use vanilla::Color;
///
/// let json = serde_json::json!({ "bones": [{ "name": "root" }], "slots": [{ "name": "body", "bone": "root" }] });
/// let mut skeleton = Skeleton2D::new(Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton")));
/// let body_slot = skeleton.find_slot_mut("body").expect("body");
/// body_slot.set_color(Color::create(1.0, 0.0, 0.0, 0.5));
/// assert_eq!(body_slot.get_color().alpha, 0.5);
/// assert_eq!(body_slot.get_attachment_name(), None);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DSlot {
	data: Skeleton2DSlotData,
	color: Color,
	attachment_name: Option<String>,
}

impl Skeleton2DSlot {
	//==============================================================================
	/// 생성. (처음 색과 붙임)
	///
	/// # 인자
	/// - `slot_data`: 슬롯 자료. (복사해 든다)
	///
	/// # 돌려줌
	/// 슬롯.
	//==============================================================================
	pub fn new(slot_data: &Skeleton2DSlotData) -> Skeleton2DSlot {
		Skeleton2DSlot {
			data: slot_data.clone(),
			color: slot_data.get_color(),
			attachment_name: slot_data.get_attachment_name().map(|name| name.to_string()),
		}
	}

	//==============================================================================
	/// 자료 반환.
	///
	/// # 돌려줌
	/// 슬롯 자료.
	//==============================================================================
	pub fn get_data(&self) -> &Skeleton2DSlotData {
		&self.data
	}

	//==============================================================================
	/// 번호 반환.
	///
	/// # 돌려줌
	/// 슬롯 번호.
	//==============================================================================
	pub fn get_index(&self) -> usize {
		self.data.get_index()
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 슬롯 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		self.data.get_name()
	}

	//==============================================================================
	/// 뼈 번호 반환.
	///
	/// # 돌려줌
	/// 붙는 뼈 번호.
	//==============================================================================
	pub fn get_bone_index(&self) -> usize {
		self.data.get_bone_index()
	}

	//==============================================================================
	/// 처음 자세로. (처음 색과 붙임 이름)
	//==============================================================================
	pub fn set_to_setup_pose(&mut self) {
		self.color = self.data.get_color();
		self.attachment_name = self.data.get_attachment_name().map(|name| name.to_string());
	}

	//==============================================================================
	/// 색 반환. (그림에 곱하는 색)
	///
	/// # 돌려줌
	/// 지금 색.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.color
	}

	//==============================================================================
	/// 색 설정.
	///
	/// # 인자
	/// - `color`: 새 색.
	//==============================================================================
	pub fn set_color(&mut self, color: Color) {
		self.color = color;
	}

	//==============================================================================
	/// 붙임 이름 반환.
	///
	/// # 돌려줌
	/// 지금 붙임 이름. (없으면 None)
	//==============================================================================
	pub fn get_attachment_name(&self) -> Option<&str> {
		self.attachment_name.as_deref()
	}

	//==============================================================================
	/// 붙임 이름 설정. (스킨에 있는지는 보지 않는다, 확인하려면 Skeleton2D::set_attachment)
	///
	/// # 인자
	/// - `attachment_name`: 붙임 이름. (None 이면 숨긴다)
	//==============================================================================
	pub fn set_attachment_name(&mut self, attachment_name: Option<&str>) {
		if self.attachment_name.as_deref() == attachment_name {
			return;
		}
		self.attachment_name = attachment_name.map(|name| name.to_string());
	}
}

//==============================================================================
/// IK 제약. (지금 세기와 꺾임 쪽)
///
/// # 예제
/// IK 세기를 반으로
/// ```rust
/// use std::rc::Rc;
/// use vanilla::experimental::skeleton2d::{Skeleton2D, Skeleton2DData};
///
/// let json = serde_json::json!({
/// 	"bones": [{ "name": "root" }, { "name": "arm", "parent": "root", "length": 10 }, { "name": "target", "parent": "root", "y": 10 }],
/// 	"ik": [{ "name": "aim", "bones": ["arm"], "target": "target" }]
/// });
/// let mut skeleton = Skeleton2D::new(Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton")));
/// skeleton.find_ik_constraint_mut("aim").expect("aim").set_mix(0.5);
/// skeleton.update_world_transform();
/// assert!((skeleton.find_bone("arm").expect("arm").get_world_rotation() - 45.0).abs() < 0.001);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DIkConstraint {
	data: Skeleton2DIkConstraintData,
	mix: f32,
	is_bend_positive: bool,
}

impl Skeleton2DIkConstraint {
	//==============================================================================
	/// 생성. (자료의 세기와 꺾임 쪽)
	///
	/// # 인자
	/// - `ik_constraint_data`: 제약 자료. (복사해 든다)
	///
	/// # 돌려줌
	/// IK 제약.
	//==============================================================================
	pub fn new(ik_constraint_data: &Skeleton2DIkConstraintData) -> Skeleton2DIkConstraint {
		Skeleton2DIkConstraint {
			data: ik_constraint_data.clone(),
			mix: ik_constraint_data.get_mix(),
			is_bend_positive: ik_constraint_data.is_bend_positive(),
		}
	}

	//==============================================================================
	/// 자료 반환.
	///
	/// # 돌려줌
	/// 제약 자료.
	//==============================================================================
	pub fn get_data(&self) -> &Skeleton2DIkConstraintData {
		&self.data
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 제약 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		self.data.get_name()
	}

	//==============================================================================
	/// 세기 반환.
	///
	/// # 돌려줌
	/// 세기. (0 ~ 1)
	//==============================================================================
	pub fn get_mix(&self) -> f32 {
		self.mix
	}

	//==============================================================================
	/// 세기 설정.
	///
	/// # 인자
	/// - `mix`: 세기. (0 이면 걸지 않고 1 이면 끝까지)
	//==============================================================================
	pub fn set_mix(&mut self, mix: f32) {
		self.mix = mix;
	}

	//==============================================================================
	/// 꺾임 쪽 반환.
	///
	/// # 돌려줌
	/// 양의 쪽으로 꺾으면 true.
	//==============================================================================
	pub fn is_bend_positive(&self) -> bool {
		self.is_bend_positive
	}

	//==============================================================================
	/// 꺾임 쪽 설정.
	///
	/// # 인자
	/// - `is_bend_positive`: 양의 쪽으로 꺾으면 true.
	//==============================================================================
	pub fn set_bend_positive(&mut self, is_bend_positive: bool) {
		self.is_bend_positive = is_bend_positive;
	}

	//==============================================================================
	/// 처음 값으로. (자료의 세기와 꺾임 쪽)
	//==============================================================================
	pub fn set_to_setup_pose(&mut self) {
		self.mix = self.data.get_mix();
		self.is_bend_positive = self.data.is_bend_positive();
	}
}

//==============================================================================
/// 2D 뼈대 인스턴스.
///
/// # 예제
/// 붙임을 바꾼 뒤 슬롯 꼭짓점 셈하기
/// ```rust
/// use std::rc::Rc;
/// use vanilla::experimental::skeleton2d::{Skeleton2D, Skeleton2DData};
///
/// let json = serde_json::json!({
/// 	"bones": [{ "name": "root" }, { "name": "head", "parent": "root", "y": -50 }],
/// 	"slots": [{ "name": "head", "bone": "head", "attachment": "face" }],
/// 	"skins": [{ "name": "default", "attachments": { "head": {
/// 		"face": { "type": "region", "frame": "face", "width": 20, "height": 20 },
/// 		"mask": { "type": "region", "frame": "mask", "width": 24, "height": 20 }
/// 	} } }]
/// });
/// let mut skeleton = Skeleton2D::new(Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton")));
/// assert!(skeleton.set_attachment("head", Some("mask")));
/// skeleton.update_world_transform();
/// let mut positions = [0.0f32; 8];
/// assert_eq!(skeleton.compute_attachment_vertices(0, &mut positions, 0.0, 0.0), 4);
/// assert_eq!((positions[0], positions[1]), (-12.0, -60.0));
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2D {
	data: Rc<Skeleton2DData>,
	bones: Vec<Skeleton2DBone>,
	slots: Vec<Skeleton2DSlot>,
	ik_constraints: Vec<Skeleton2DIkConstraint>,
	draw_order: Vec<usize>,
	skin_index: Option<usize>,
	dirty_flags: Vec<bool>,
}

impl Skeleton2D {
	//==============================================================================
	/// 생성. (처음 자세, 처음 스킨)
	///
	/// # 인자
	/// - `skeleton_data`: 뼈대 자료.
	///
	/// # 돌려줌
	/// 뼈대 인스턴스. (월드 행렬은 update_world_transform 뒤에 맞다)
	//==============================================================================
	pub fn new(skeleton_data: Rc<Skeleton2DData>) -> Skeleton2D {
		let bones: Vec<Skeleton2DBone> = skeleton_data.get_bones().iter().map(Skeleton2DBone::new).collect();
		let slots: Vec<Skeleton2DSlot> = skeleton_data.get_slots().iter().map(Skeleton2DSlot::new).collect();
		let ik_constraints = skeleton_data.get_ik_constraints().iter().map(Skeleton2DIkConstraint::new).collect();
		let draw_order = (0..slots.len()).collect();
		let skin_index = skeleton_data.get_default_skin_index();
		let dirty_flags = vec![false; bones.len()];
		Skeleton2D {
			data: skeleton_data,
			bones,
			slots,
			ik_constraints,
			draw_order,
			skin_index,
			dirty_flags,
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
	/// 뼈 목록 반환.
	///
	/// # 돌려줌
	/// 뼈 목록. (부모가 앞)
	//==============================================================================
	pub fn get_bones(&self) -> &[Skeleton2DBone] {
		&self.bones
	}

	//==============================================================================
	/// 뼈 목록 반환. (변경용)
	///
	/// # 돌려줌
	/// 뼈 목록.
	//==============================================================================
	pub fn get_bones_mut(&mut self) -> &mut [Skeleton2DBone] {
		&mut self.bones
	}

	//==============================================================================
	/// 슬롯 목록 반환.
	///
	/// # 돌려줌
	/// 슬롯 목록. (번호 차례)
	//==============================================================================
	pub fn get_slots(&self) -> &[Skeleton2DSlot] {
		&self.slots
	}

	//==============================================================================
	/// 슬롯 목록 반환. (변경용)
	///
	/// # 돌려줌
	/// 슬롯 목록.
	//==============================================================================
	pub fn get_slots_mut(&mut self) -> &mut [Skeleton2DSlot] {
		&mut self.slots
	}

	//==============================================================================
	/// IK 제약 목록 반환.
	///
	/// # 돌려줌
	/// 제약 목록. (거는 차례)
	//==============================================================================
	pub fn get_ik_constraints(&self) -> &[Skeleton2DIkConstraint] {
		&self.ik_constraints
	}

	//==============================================================================
	/// IK 제약 목록 반환. (변경용)
	///
	/// # 돌려줌
	/// 제약 목록.
	//==============================================================================
	pub fn get_ik_constraints_mut(&mut self) -> &mut [Skeleton2DIkConstraint] {
		&mut self.ik_constraints
	}

	//==============================================================================
	/// 그리기 차례 반환.
	///
	/// # 돌려줌
	/// 그리는 차례의 슬롯 번호.
	//==============================================================================
	pub fn get_draw_order(&self) -> &[usize] {
		&self.draw_order
	}

	//==============================================================================
	/// 그리기 차례 반환. (변경용, 고치면 바로 걸린다)
	///
	/// # 돌려줌
	/// 그리는 차례의 슬롯 번호.
	//==============================================================================
	pub fn get_draw_order_mut(&mut self) -> &mut [usize] {
		&mut self.draw_order
	}

	//==============================================================================
	/// 뼈 찾기.
	///
	/// # 인자
	/// - `bone_name`: 뼈 이름.
	///
	/// # 돌려줌
	/// 뼈. (없으면 None)
	//==============================================================================
	pub fn find_bone(&self, bone_name: &str) -> Option<&Skeleton2DBone> {
		self.find_bone_index(bone_name).map(|bone_index| &self.bones[bone_index])
	}

	//==============================================================================
	/// 뼈 찾기. (변경용)
	///
	/// # 인자
	/// - `bone_name`: 뼈 이름.
	///
	/// # 돌려줌
	/// 뼈. (없으면 None)
	//==============================================================================
	pub fn find_bone_mut(&mut self, bone_name: &str) -> Option<&mut Skeleton2DBone> {
		self.find_bone_index(bone_name).map(|bone_index| &mut self.bones[bone_index])
	}

	//==============================================================================
	/// 뼈 번호 찾기.
	///
	/// # 인자
	/// - `bone_name`: 뼈 이름.
	///
	/// # 돌려줌
	/// 뼈 번호. (없으면 None)
	//==============================================================================
	pub fn find_bone_index(&self, bone_name: &str) -> Option<usize> {
		self.data.find_bone_index(bone_name)
	}

	//==============================================================================
	/// 슬롯 찾기.
	///
	/// # 인자
	/// - `slot_name`: 슬롯 이름.
	///
	/// # 돌려줌
	/// 슬롯. (없으면 None)
	//==============================================================================
	pub fn find_slot(&self, slot_name: &str) -> Option<&Skeleton2DSlot> {
		self.find_slot_index(slot_name).map(|slot_index| &self.slots[slot_index])
	}

	//==============================================================================
	/// 슬롯 찾기. (변경용)
	///
	/// # 인자
	/// - `slot_name`: 슬롯 이름.
	///
	/// # 돌려줌
	/// 슬롯. (없으면 None)
	//==============================================================================
	pub fn find_slot_mut(&mut self, slot_name: &str) -> Option<&mut Skeleton2DSlot> {
		self.find_slot_index(slot_name).map(|slot_index| &mut self.slots[slot_index])
	}

	//==============================================================================
	/// 슬롯 번호 찾기.
	///
	/// # 인자
	/// - `slot_name`: 슬롯 이름.
	///
	/// # 돌려줌
	/// 슬롯 번호. (없으면 None)
	//==============================================================================
	pub fn find_slot_index(&self, slot_name: &str) -> Option<usize> {
		self.data.find_slot_index(slot_name)
	}

	//==============================================================================
	/// IK 제약 찾기.
	///
	/// # 인자
	/// - `ik_constraint_name`: 제약 이름.
	///
	/// # 돌려줌
	/// 제약. (없으면 None)
	//==============================================================================
	pub fn find_ik_constraint(&self, ik_constraint_name: &str) -> Option<&Skeleton2DIkConstraint> {
		self.ik_constraints.iter().find(|ik_constraint| ik_constraint.get_name() == ik_constraint_name)
	}

	//==============================================================================
	/// IK 제약 찾기. (변경용)
	///
	/// # 인자
	/// - `ik_constraint_name`: 제약 이름.
	///
	/// # 돌려줌
	/// 제약. (없으면 None)
	//==============================================================================
	pub fn find_ik_constraint_mut(&mut self, ik_constraint_name: &str) -> Option<&mut Skeleton2DIkConstraint> {
		self.ik_constraints.iter_mut().find(|ik_constraint| ik_constraint.get_name() == ik_constraint_name)
	}

	//==============================================================================
	/// 스킨 고르기. (고른 스킨에 없는 붙임은 처음 스킨에서 찾는다)
	///
	/// # 인자
	/// - `skin_name`: 스킨 이름.
	///
	/// # 돌려줌
	/// 골랐으면 true. (없는 이름이면 false)
	//==============================================================================
	pub fn set_skin(&mut self, skin_name: &str) -> bool {
		match self.data.find_skin_index(skin_name) {
			Some(skin_index) => {
				self.skin_index = Some(skin_index);
				true
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 고른 스킨 이름 반환.
	///
	/// # 돌려줌
	/// 스킨 이름. (스킨이 없으면 None)
	//==============================================================================
	pub fn get_skin_name(&self) -> Option<&str> {
		self.skin_index.map(|skin_index| self.data.get_skins()[skin_index].get_name())
	}

	//==============================================================================
	/// 붙임 찾기. (고른 스킨, 없으면 처음 스킨에서)
	///
	/// # 인자
	/// - `slot_index`: 슬롯 번호.
	/// - `attachment_name`: 붙임 이름.
	///
	/// # 돌려줌
	/// 붙임. (없으면 None)
	//==============================================================================
	pub fn get_attachment(&self, slot_index: usize, attachment_name: Option<&str>) -> Option<&Skeleton2DAttachment> {
		let attachment_name = attachment_name?;
		let skins = self.data.get_skins();
		if let Some(skin_index) = self.skin_index {
			if let Some(attachment) = skins[skin_index].get_attachment(slot_index, attachment_name) {
				return Some(attachment);
			}
		}
		let default_skin_index = self.data.get_default_skin_index()?;
		if Some(default_skin_index) == self.skin_index {
			return None;
		}
		skins[default_skin_index].get_attachment(slot_index, attachment_name)
	}

	//==============================================================================
	/// 슬롯의 지금 붙임 반환.
	///
	/// # 인자
	/// - `slot_index`: 슬롯 번호.
	///
	/// # 돌려줌
	/// 붙임. (없으면 None)
	//==============================================================================
	pub fn get_slot_attachment(&self, slot_index: usize) -> Option<&Skeleton2DAttachment> {
		self.get_attachment(slot_index, self.slots[slot_index].get_attachment_name())
	}

	//==============================================================================
	/// 슬롯의 붙임 바꾸기.
	///
	/// # 인자
	/// - `slot_name`: 슬롯 이름.
	/// - `attachment_name`: 붙임 이름. (None 이면 숨긴다)
	///
	/// # 돌려줌
	/// 바꿨으면 true. (슬롯이나 붙임이 없으면 false)
	//==============================================================================
	pub fn set_attachment(&mut self, slot_name: &str, attachment_name: Option<&str>) -> bool {
		let Some(slot_index) = self.find_slot_index(slot_name) else {
			return false;
		};
		if attachment_name.is_some() && self.get_attachment(slot_index, attachment_name).is_none() {
			return false;
		}
		self.slots[slot_index].set_attachment_name(attachment_name);
		true
	}

	//==============================================================================
	/// 모두 처음 자세로. (뼈, 슬롯, 그리기 차례, IK 값)
	//==============================================================================
	pub fn set_to_setup_pose(&mut self) {
		self.set_bones_to_setup_pose();
		self.set_slots_to_setup_pose();
	}

	//==============================================================================
	/// 뼈와 IK 를 처음 자세로.
	//==============================================================================
	pub fn set_bones_to_setup_pose(&mut self) {
		for bone in self.bones.iter_mut() {
			bone.set_to_setup_pose();
		}
		for ik_constraint in self.ik_constraints.iter_mut() {
			ik_constraint.set_to_setup_pose();
		}
	}

	//==============================================================================
	/// 슬롯과 그리기 차례를 처음 자세로.
	//==============================================================================
	pub fn set_slots_to_setup_pose(&mut self) {
		for slot in self.slots.iter_mut() {
			slot.set_to_setup_pose();
		}
		for (order_index, slot_index) in self.draw_order.iter_mut().enumerate() {
			*slot_index = order_index;
		}
	}

	//==============================================================================
	/// 지금 자세를 자세 배열로 읽기.
	///
	/// # 인자
	/// - `pose`: 적을 자세. (뼈와 슬롯 수가 같아야 한다)
	//==============================================================================
	pub fn read_pose(&self, pose: &mut Skeleton2DPose) {
		let bone_values = pose.get_bone_values_mut();
		for (bone_index, bone) in self.bones.iter().enumerate() {
			let offset = bone_index * SKELETON2D_BONE_VALUE_COUNT;
			bone_values[offset..offset + SKELETON2D_BONE_VALUE_COUNT].copy_from_slice(&[bone.x, bone.y, bone.rotation, bone.scale_x, bone.scale_y, bone.shear_x, bone.shear_y]);
		}
		let slot_colors = pose.get_slot_colors_mut();
		for (slot_index, slot) in self.slots.iter().enumerate() {
			slot_colors[slot_index * 4..slot_index * 4 + 4].copy_from_slice(&[slot.color.red, slot.color.green, slot.color.blue, slot.color.alpha]);
		}
		let slot_attachment_names = pose.get_slot_attachment_names_mut();
		for (slot_index, slot) in self.slots.iter().enumerate() {
			slot_attachment_names[slot_index].clone_from(&slot.attachment_name);
		}
		pose.get_draw_order_mut().copy_from_slice(&self.draw_order);
	}

	//==============================================================================
	/// 자세 배열을 지금 자세로 쓰기.
	///
	/// # 인자
	/// - `pose`: 읽을 자세. (뼈와 슬롯 수가 같아야 한다)
	//==============================================================================
	pub fn write_pose(&mut self, pose: &Skeleton2DPose) {
		let bone_values = pose.get_bone_values();
		for (bone_index, bone) in self.bones.iter_mut().enumerate() {
			let offset = bone_index * SKELETON2D_BONE_VALUE_COUNT;
			bone.x = bone_values[offset];
			bone.y = bone_values[offset + 1];
			bone.rotation = bone_values[offset + 2];
			bone.scale_x = bone_values[offset + 3];
			bone.scale_y = bone_values[offset + 4];
			bone.shear_x = bone_values[offset + 5];
			bone.shear_y = bone_values[offset + 6];
		}
		let slot_colors = pose.get_slot_colors();
		let slot_attachment_names = pose.get_slot_attachment_names();
		for (slot_index, slot) in self.slots.iter_mut().enumerate() {
			slot.color = Color::create(slot_colors[slot_index * 4], slot_colors[slot_index * 4 + 1], slot_colors[slot_index * 4 + 2], slot_colors[slot_index * 4 + 3]);
			slot.attachment_name.clone_from(&slot_attachment_names[slot_index]);
		}
		self.draw_order.copy_from_slice(pose.get_draw_order());
	}

	//==============================================================================
	/// 뼈 하나의 월드 행렬 셈. (걸린 회전으로, 부모의 월드 행렬은 셈해 둔 것이어야 한다)
	///
	/// # 인자
	/// - `bone_index`: 뼈 번호.
	//==============================================================================
	pub fn update_bone_world_transform(&mut self, bone_index: usize) {
		let bone = &self.bones[bone_index];
		let x_axis_radian = (bone.applied_rotation + bone.shear_x).to_radians();
		let y_axis_radian = (bone.applied_rotation + bone.shear_y).to_radians();
		let local_a = x_axis_radian.cos() * bone.scale_x;
		let local_b = x_axis_radian.sin() * bone.scale_x;
		let local_c = -y_axis_radian.sin() * bone.scale_y;
		let local_d = y_axis_radian.cos() * bone.scale_y;
		let x = bone.x;
		let y = bone.y;
		let world_values = match bone.get_parent_index() {
			Some(parent_index) => {
				let parent = &self.bones[parent_index];
				[
					parent.world_a * local_a + parent.world_c * local_b,
					parent.world_b * local_a + parent.world_d * local_b,
					parent.world_a * local_c + parent.world_c * local_d,
					parent.world_b * local_c + parent.world_d * local_d,
					parent.world_a * x + parent.world_c * y + parent.world_x,
					parent.world_b * x + parent.world_d * y + parent.world_y,
				]
			}
			None => {
				[local_a, local_b, local_c, local_d, x, y]
			}
		};
		self.bones[bone_index].set_world_transform(world_values[0], world_values[1], world_values[2], world_values[3], world_values[4], world_values[5]);
	}

	//==============================================================================
	/// 뼈의 월드 회전을 그 값이 되게 걸린 회전 정하기. (부모 행렬을 되돌려 지역 각을 찾는다, 월드 행렬은 셈하지 않는다)
	///
	/// # 인자
	/// - `bone_index`: 뼈 번호.
	/// - `world_rotation`: 바라는 월드 회전. (도)
	//==============================================================================
	pub fn set_bone_world_rotation(&mut self, bone_index: usize, world_rotation: f32) {
		let mut local_rotation = world_rotation;
		if let Some(parent_index) = self.bones[bone_index].get_parent_index() {
			let parent = &self.bones[parent_index];
			let determinant = parent.world_a * parent.world_d - parent.world_b * parent.world_c;
			let radian = world_rotation.to_radians();
			let direction_x = radian.cos();
			let direction_y = radian.sin();
			if determinant != 0.0 {
				let local_direction_x = (parent.world_d * direction_x - parent.world_c * direction_y) / determinant;
				let local_direction_y = (parent.world_a * direction_y - parent.world_b * direction_x) / determinant;
				local_rotation = local_direction_y.atan2(local_direction_x).to_degrees();
			}
			else {
				local_rotation = world_rotation - parent.get_world_rotation();
			}
		}
		let bone = &mut self.bones[bone_index];
		if bone.scale_x < 0.0 {
			local_rotation += 180.0;
		}
		bone.applied_rotation = local_rotation - bone.shear_x;
	}

	//==============================================================================
	/// 뼈 하나와 그 자손의 월드 행렬 다시 셈. (그 뼈는 셈해 두었어야 한다)
	///
	/// # 인자
	/// - `root_bone_index`: 다시 셈한 뼈 번호.
	//==============================================================================
	pub fn update_descendant_world_transforms(&mut self, root_bone_index: usize) {
		for dirty_flag in self.dirty_flags.iter_mut() {
			*dirty_flag = false;
		}
		self.dirty_flags[root_bone_index] = true;
		for bone_index in root_bone_index + 1..self.bones.len() {
			if let Some(parent_index) = self.bones[bone_index].get_parent_index() {
				if self.dirty_flags[parent_index] {
					self.update_bone_world_transform(bone_index);
					self.dirty_flags[bone_index] = true;
				}
			}
		}
	}

	//==============================================================================
	/// IK 제약 하나 걸기.
	///
	/// # 인자
	/// - `ik_constraint_index`: 제약 번호.
	//==============================================================================
	pub fn apply_ik_constraint(&mut self, ik_constraint_index: usize) {
		let ik_constraint = &self.ik_constraints[ik_constraint_index];
		let mix = ik_constraint.get_mix();
		if mix <= 0.0 {
			return;
		}
		let is_bend_positive_setting = ik_constraint.is_bend_positive();
		let bone_indices = ik_constraint.get_data().get_bone_indices().to_vec();
		let target = &self.bones[ik_constraint.get_data().get_target_index()];
		let target_x = target.world_x;
		let target_y = target.world_y;

		// 뼈 하나: x 축이 목표를 보게 돌린다.
		if bone_indices.len() == 1 {
			let bone_index = bone_indices[0];
			let bone = &self.bones[bone_index];
			let current_rotation = bone.get_world_rotation();
			let mut desired_rotation = (target_y - bone.world_y).atan2(target_x - bone.world_x).to_degrees();
			if bone.scale_x < 0.0 {
				desired_rotation += 180.0;
			}
			let delta_rotation = wrap_degree(desired_rotation - current_rotation) * mix;
			self.set_bone_world_rotation(bone_index, current_rotation + delta_rotation);
			self.update_bone_world_transform(bone_index);
			self.update_descendant_world_transforms(bone_index);
			return;
		}

		// 뼈 둘: 부모를 돌려 팔꿈치(자식 뿌리)를 놓고, 자식이 목표를 보게 돌린다.
		let parent_index = bone_indices[0];
		let child_index = bone_indices[1];
		let parent = &self.bones[parent_index];
		let child = &self.bones[child_index];
		let parent_x = parent.world_x;
		let parent_y = parent.world_y;
		let child_length_world = child.get_data().get_length() * child.get_world_scale_x();
		let upper_length = (child.world_x - parent_x).hypot(child.world_y - parent_y);
		let target_distance = (target_x - parent_x).hypot(target_y - parent_y);
		let current_elbow_rotation = (child.world_y - parent_y).atan2(child.world_x - parent_x).to_degrees();
		let target_rotation = (target_y - parent_y).atan2(target_x - parent_x).to_degrees();
		let mut desired_elbow_rotation = target_rotation;
		if upper_length > IK_EPSILON && child_length_world > IK_EPSILON && target_distance > IK_EPSILON {
			let cosine = (upper_length * upper_length + target_distance * target_distance - child_length_world * child_length_world) / (2.0 * upper_length * target_distance);
			let elbow_angle = cosine.clamp(-1.0, 1.0).acos().to_degrees();
			let mut is_bend_positive = is_bend_positive_setting;
			if let Some(grandparent_index) = parent.get_parent_index() {
				let grandparent = &self.bones[grandparent_index];
				if grandparent.world_a * grandparent.world_d - grandparent.world_b * grandparent.world_c < 0.0 {
					is_bend_positive = !is_bend_positive;
				}
			}
			desired_elbow_rotation = if is_bend_positive { target_rotation - elbow_angle } else { target_rotation + elbow_angle };
		}
		let parent_delta_rotation = wrap_degree(desired_elbow_rotation - current_elbow_rotation) * mix;
		let parent_world_rotation = parent.get_world_rotation();
		self.set_bone_world_rotation(parent_index, parent_world_rotation + parent_delta_rotation);
		self.update_bone_world_transform(parent_index);
		self.update_bone_world_transform(child_index);

		let child = &self.bones[child_index];
		let child_world_rotation = child.get_world_rotation();
		let mut desired_child_rotation = (target_y - child.world_y).atan2(target_x - child.world_x).to_degrees();
		if child.scale_x < 0.0 {
			desired_child_rotation += 180.0;
		}
		let child_delta_rotation = wrap_degree(desired_child_rotation - child_world_rotation) * mix;
		self.set_bone_world_rotation(child_index, child_world_rotation + child_delta_rotation);
		self.update_bone_world_transform(child_index);
		self.update_descendant_world_transforms(parent_index);
	}

	//==============================================================================
	/// 월드 행렬 셈. (지역 값으로 부모부터 셈한 뒤 IK 를 차례로 건다)
	//==============================================================================
	pub fn update_world_transform(&mut self) {
		for bone_index in 0..self.bones.len() {
			let bone = &mut self.bones[bone_index];
			bone.applied_rotation = bone.rotation;
			self.update_bone_world_transform(bone_index);
		}
		for ik_constraint_index in 0..self.ik_constraints.len() {
			self.apply_ik_constraint(ik_constraint_index);
		}
	}

	//==============================================================================
	/// 슬롯 붙임의 꼭짓점 월드 자리 셈. (뼈대 기준, update_world_transform 뒤에 부른다)
	/// - 네모는 왼쪽 위부터 시계 방향 넷, 메시는 꼭짓점 차례로 x, y 를 적는다.
	///
	/// # 인자
	/// - `slot_index`: 슬롯 번호.
	/// - `output`: 꼭짓점마다 x, y 를 적을 곳. (붙임 꼭짓점 수 x 2 이상)
	/// - `frame_width`: 크기가 0 인 네모 붙임에 쓸 조각 너비.
	/// - `frame_height`: 크기가 0 인 네모 붙임에 쓸 조각 높이.
	///
	/// # 돌려줌
	/// 적은 꼭짓점 수. (붙임이 없으면 0)
	//==============================================================================
	pub fn compute_attachment_vertices(&self, slot_index: usize, output: &mut [f32], frame_width: f32, frame_height: f32) -> usize {
		let Some(attachment) = self.get_slot_attachment(slot_index) else {
			return 0;
		};
		let bone = &self.bones[self.slots[slot_index].get_bone_index()];
		match attachment {
			Skeleton2DAttachment::Region(region_attachment) => {
				let width = if region_attachment.get_width() > 0.0 { region_attachment.get_width() } else { frame_width };
				let height = if region_attachment.get_height() > 0.0 { region_attachment.get_height() } else { frame_height };
				let radian = region_attachment.get_rotation().to_radians();
				let cosine = radian.cos();
				let sine = radian.sin();
				let local_a = cosine * region_attachment.get_scale_x();
				let local_b = sine * region_attachment.get_scale_x();
				let local_c = -sine * region_attachment.get_scale_y();
				let local_d = cosine * region_attachment.get_scale_y();
				let left = -region_attachment.get_pivot_x() * width;
				let top = -region_attachment.get_pivot_y() * height;
				let corner_values = [left, top, left + width, top, left + width, top + height, left, top + height];
				for corner_index in 0..4 {
					let corner_x = corner_values[corner_index * 2];
					let corner_y = corner_values[corner_index * 2 + 1];
					let attached_x = local_a * corner_x + local_c * corner_y + region_attachment.get_x();
					let attached_y = local_b * corner_x + local_d * corner_y + region_attachment.get_y();
					output[corner_index * 2] = bone.world_a * attached_x + bone.world_c * attached_y + bone.world_x;
					output[corner_index * 2 + 1] = bone.world_b * attached_x + bone.world_d * attached_y + bone.world_y;
				}
				4
			}
			Skeleton2DAttachment::Mesh(mesh_attachment) => {
				let vertex_count = mesh_attachment.get_vertex_count();
				if !mesh_attachment.is_weighted() {
					let vertices = mesh_attachment.get_vertices();
					for vertex_index in 0..vertex_count {
						let local_x = vertices[vertex_index * 2];
						let local_y = vertices[vertex_index * 2 + 1];
						output[vertex_index * 2] = bone.world_a * local_x + bone.world_c * local_y + bone.world_x;
						output[vertex_index * 2 + 1] = bone.world_b * local_x + bone.world_d * local_y + bone.world_y;
					}
					return vertex_count;
				}
				let influence_counts = mesh_attachment.get_influence_counts();
				let influence_bone_indices = mesh_attachment.get_influence_bone_indices();
				let influence_values = mesh_attachment.get_influence_values();
				let mut influence_index = 0usize;
				for vertex_index in 0..vertex_count {
					let mut world_x = 0.0f32;
					let mut world_y = 0.0f32;
					for _ in 0..influence_counts[vertex_index] {
						let influence_bone = &self.bones[influence_bone_indices[influence_index]];
						let bind_x = influence_values[influence_index * 3];
						let bind_y = influence_values[influence_index * 3 + 1];
						let weight = influence_values[influence_index * 3 + 2];
						world_x += (influence_bone.world_a * bind_x + influence_bone.world_c * bind_y + influence_bone.world_x) * weight;
						world_y += (influence_bone.world_b * bind_x + influence_bone.world_d * bind_y + influence_bone.world_y) * weight;
						influence_index += 1;
					}
					output[vertex_index * 2] = world_x;
					output[vertex_index * 2 + 1] = world_y;
				}
				vertex_count
			}
		}
	}
}

//==============================================================================
/// 시험. (원본 확인 스크립트 examples/skeleton2d/skeleton2dcheck.mjs 와 같은 입력, 같은 기댓값)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	const TOLERANCE: f32 = 0.0005;

	//==============================================================================
	/// 기댓값과 견주기.
	//==============================================================================
	fn assert_near(label: &str, actual: f32, expected: f32) {
		assert!((actual - expected).abs() <= TOLERANCE, "{}: {} (기댓값 {})", label, actual, expected);
	}

	//==============================================================================
	/// JSON 으로 인스턴스 짓기.
	//==============================================================================
	fn create_skeleton(json: serde_json::Value) -> Skeleton2D {
		Skeleton2D::new(Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton")))
	}

	//==============================================================================
	/// 1. 월드 변환. (부모의 이동, 회전, 크기, 기울임을 물려받는다)
	//==============================================================================
	#[test]
	fn world_transform_inherits_parent() {
		let mut skeleton = create_skeleton(serde_json::json!({
			"bones": [
				{ "name": "root", "x": 100, "y": 50, "rotation": 90, "scaleX": 2 },
				{ "name": "child", "parent": "root", "x": 10, "rotation": -90 },
				{ "name": "sheared", "parent": "child", "x": 5, "shearY": 30 }
			]
		}));
		skeleton.update_world_transform();
		let child = skeleton.find_bone("child").expect("child");
		let sheared = skeleton.find_bone("sheared").expect("sheared");
		assert_near("월드 child x", child.get_world_x(), 100.0);
		assert_near("월드 child y", child.get_world_y(), 70.0);
		assert_near("월드 child a", child.get_world_a(), 1.0);
		assert_near("월드 child d", child.get_world_d(), 2.0);
		assert_near("월드 sheared x", sheared.get_world_x(), 105.0);
		assert_near("월드 sheared y", sheared.get_world_y(), 70.0);
		assert_near("월드 sheared c", sheared.get_world_c(), -0.5);
		assert_near("월드 sheared d", sheared.get_world_d(), 1.7320508);
		let local = sheared.world_to_local(110.0, 80.0);
		let back = sheared.local_to_world(local.x, local.y);
		assert_near("월드 되돌림 x", back.x, 110.0);
		assert_near("월드 되돌림 y", back.y, 80.0);
	}

	//==============================================================================
	/// 4. IK. (뼈 둘이 닿는 목표, 꺾임 쪽, 닿지 않는 목표, 뼈 하나, 세기)
	//==============================================================================
	#[test]
	fn ik_two_bone_and_one_bone() {
		let create_arm = |is_bend_positive: bool, target_x: f32, target_y: f32| -> Skeleton2D {
			create_skeleton(serde_json::json!({
				"bones": [
					{ "name": "root" },
					{ "name": "upper", "parent": "root", "length": 50 },
					{ "name": "lower", "parent": "upper", "x": 50, "length": 50 },
					{ "name": "target", "parent": "root", "x": target_x, "y": target_y }
				],
				"ik": [{ "name": "armIk", "bones": ["upper", "lower"], "target": "target", "bendPositive": is_bend_positive }]
			}))
		};
		let mut positive_skeleton = create_arm(true, 60.0, 40.0);
		positive_skeleton.update_world_transform();
		let lower = positive_skeleton.find_bone("lower").expect("lower");
		let tip = lower.local_to_world(50.0, 0.0);
		assert_near("IK 둘 끝 x", tip.x, 60.0);
		assert_near("IK 둘 끝 y", tip.y, 40.0);
		assert_near("IK 둘 팔꿈치 x", lower.get_world_x(), 49.2153785);
		assert_near("IK 둘 팔꿈치 y", lower.get_world_y(), -8.8230677);
		assert_near("IK 둘 걸린 회전", lower.get_applied_rotation(), 87.7075572);
		assert_near("IK 지역 회전 그대로", lower.get_rotation(), 0.0);
		let mut negative_skeleton = create_arm(false, 60.0, 40.0);
		negative_skeleton.update_world_transform();
		let lower = negative_skeleton.find_bone("lower").expect("lower");
		let tip = lower.local_to_world(50.0, 0.0);
		assert_near("IK 반대 꺾임 끝 x", tip.x, 60.0);
		assert_near("IK 반대 꺾임 끝 y", tip.y, 40.0);
		assert_near("IK 반대 꺾임 팔꿈치 x", lower.get_world_x(), 10.7846215);
		assert_near("IK 반대 꺾임 팔꿈치 y", lower.get_world_y(), 48.8230677);
		let mut far_skeleton = create_arm(true, 0.0, 300.0);
		far_skeleton.update_world_transform();
		let tip = far_skeleton.find_bone("lower").expect("lower").local_to_world(50.0, 0.0);
		assert_near("IK 닿지 않음 x", tip.x, 0.0);
		assert_near("IK 닿지 않음 y", tip.y, 100.0);
		let mut aim_skeleton = create_skeleton(serde_json::json!({
			"bones": [{ "name": "root", "rotation": 30 }, { "name": "arm", "parent": "root", "length": 10 }, { "name": "target", "parent": "root", "x": 0, "y": 10 }],
			"ik": [{ "name": "aim", "bones": ["arm"], "target": "target", "mix": 0.5 }]
		}));
		aim_skeleton.update_world_transform();
		assert_near("IK 하나 세기 반", aim_skeleton.find_bone("arm").expect("arm").get_world_rotation(), 75.0);
		assert_near("IK 하나 걸린 회전", aim_skeleton.find_bone("arm").expect("arm").get_applied_rotation(), 45.0);
	}

	//==============================================================================
	/// 5. 스킨 꼭짓점. (무게 반반, 한 뼈를 돌림) 과 네모 붙임 축점.
	//==============================================================================
	#[test]
	fn skinned_and_region_vertices() {
		let mut skeleton = create_skeleton(serde_json::json!({
			"bones": [{ "name": "root" }, { "name": "boneA", "parent": "root" }, { "name": "boneB", "parent": "root", "x": 100 }],
			"slots": [{ "name": "cloth", "bone": "root", "attachment": "cloth" }],
			"skins": [{ "name": "default", "attachments": { "cloth": { "cloth": {
				"type": "mesh", "frame": "cloth",
				"vertices": [0, 0, 100, 10, 100, -10],
				"uvs": [0, 0.5, 1, 1, 1, 0],
				"triangles": [0, 1, 2],
				"weights": [["boneA", 1], ["boneA", 1, "boneB", 1], ["boneB", 2]]
			} } } }]
		}));
		skeleton.find_bone_mut("boneB").expect("boneB").set_rotation(90.0);
		skeleton.update_world_transform();
		let mut positions = [0.0f32; 6];
		assert_eq!(skeleton.compute_attachment_vertices(0, &mut positions, 0.0, 0.0), 3);
		assert_near("스킨 0 x", positions[0], 0.0);
		assert_near("스킨 1 x", positions[2], 95.0);
		assert_near("스킨 1 y", positions[3], 5.0);
		assert_near("스킨 2 x", positions[4], 110.0);
		assert_near("스킨 2 y", positions[5], 0.0);
		let mut region_skeleton = create_skeleton(serde_json::json!({
			"bones": [{ "name": "root" }, { "name": "head", "parent": "root", "x": 10, "rotation": 90 }],
			"slots": [{ "name": "head", "bone": "head", "attachment": "face" }],
			"skins": [{ "name": "default", "attachments": { "head": { "face": { "type": "region", "x": 5, "width": 20, "height": 10, "pivotX": 0, "pivotY": 0.5 } } } }]
		}));
		region_skeleton.update_world_transform();
		let mut region_positions = [0.0f32; 8];
		region_skeleton.compute_attachment_vertices(0, &mut region_positions, 0.0, 0.0);
		assert_near("네모 왼쪽 위 x", region_positions[0], 15.0);
		assert_near("네모 왼쪽 위 y", region_positions[1], 5.0);
		assert_near("네모 오른쪽 아래 x", region_positions[4], 5.0);
		assert_near("네모 오른쪽 아래 y", region_positions[5], 25.0);
	}

	//==============================================================================
	/// 8. 읽기 오류. (부모가 뒤에 있는 뼈, 원본과 같은 글)
	//==============================================================================
	#[test]
	fn read_error_message_matches_original() {
		let error = Skeleton2DData::create_from_json(&serde_json::json!({ "bones": [{ "name": "child", "parent": "root" }, { "name": "root" }] })).expect_err("error");
		assert_eq!(error, "[Skeleton2D] 뼈 \"child\" 의 부모 \"root\" 가 앞에 없다. (부모가 앞에 와야 한다)");
	}
}

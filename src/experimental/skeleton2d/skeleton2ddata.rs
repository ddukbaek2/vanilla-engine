//==============================================================================
//! 2D 뼈대 자료. (원본 experimental/skeleton2d/skeleton2ddata.js, 뼈, 슬롯, IK, 스킨과 붙임, 클립의 처음 자세 자료, 여러 인스턴스가 나눠 쓴다)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 좌표는 엔진과 같이 y 가 아래로 커진다. 회전과 기울임은 도 단위이고 양수가 화면에서 시계 방향이다.
//! - 뼈의 지역 행렬: x 축 = (cos(회전 + 기울임 x), sin(회전 + 기울임 x)) x 크기 x, y 축 = (-sin(회전 + 기울임 y), cos(회전 + 기울임 y)) x 크기 y.
//!   월드 행렬은 부모 월드 행렬 x 지역 행렬이다. (이동, 회전, 크기, 기울임 모두 물려받는다)
//! - 뼈는 부모가 앞에 오는 차례여야 한다. (읽기가 확인한다)
//! - 읽는 꼴 둘: 엔진 자체 JSON(create_from_json)과 DragonBones 5.x JSON(create_from_dragon_bones).
//!   Spine 형식은 읽지 않는다. Spine 런타임 라이선스가 그 런타임을 넣은 제품의 사용자마다 Spine 편집기 라이선스를 요구해서다.
//! - 원본과 다른 점: 부모 번호는 `Option<usize>`(원본 -1 은 None), 슬롯 섞기는 BlendMode, 붙임은 두 갈래 열거 Skeleton2DAttachment,
//!   클립은 `Rc<Skeleton2DAnimation>` 으로 들어 트랙 항목이 나눠 든다.
//!
//! # 예제
//! 엔진 JSON 으로 자료를 지어 뼈와 슬롯 찾기
//! ```rust
//! use vanilla::experimental::skeleton2d::Skeleton2DData;
//!
//! let json = serde_json::json!({
//! 	"name": "hero",
//! 	"bones": [{ "name": "root" }, { "name": "body", "parent": "root", "y": -40, "length": 30 }],
//! 	"slots": [{ "name": "body", "bone": "body", "attachment": "body" }],
//! 	"skins": [{ "name": "default", "attachments": { "body": { "body": { "type": "region", "frame": "body", "width": 20, "height": 40 } } } }]
//! });
//! let skeleton_data = Skeleton2DData::create_from_json(&json).expect("skeleton");
//! let attachment = skeleton_data.get_default_skin().expect("skin").get_attachment(0, "body").expect("attachment");
//! assert_eq!(skeleton_data.get_name(), "hero");
//! assert_eq!(skeleton_data.find_bone("body").expect("body").get_y(), -40.0);
//! assert_eq!(attachment.get_frame_name(), "body");
//! ```
//==============================================================================
use crate::base::color::Color;
use crate::base::trigonometry;
use crate::core::graphic::BlendMode;
use crate::experimental::skeleton2d::dragonbonesreader::read_dragon_bones_json;
use crate::experimental::skeleton2d::skeleton2danimation::Skeleton2DAnimation;
use crate::experimental::skeleton2d::skeleton2djsonreader::read_skeleton2d_json;
use std::f64::consts::PI;
use std::rc::Rc;

/// 처음 스킨 이름.
const DEFAULT_SKIN_NAME: &str = "default";

/// 네모 붙임의 꼭짓점 UV. (조각 안 0 ~ 1, 왼쪽 위, 오른쪽 위, 오른쪽 아래, 왼쪽 아래)
const REGION_UVS: [f64; 8] = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];

/// 네모 붙임의 삼각형 둘.
const REGION_TRIANGLES: [usize; 6] = [0, 1, 2, 0, 2, 3];

//==============================================================================
/// 붙임 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skeleton2DAttachmentType {
	/// 네모. (원본 "region")
	Region,
	/// 메시. (원본 "mesh")
	Mesh,
}

//==============================================================================
/// 뼈 자료. (이름, 부모, 처음 지역 값, 길이)
///
/// # 예제
/// 뼈 자료 하나 만들기
/// ```rust
/// use vanilla::experimental::skeleton2d::Skeleton2DBoneData;
///
/// let mut bone_data = Skeleton2DBoneData::new(1, "arm", Some(0));
/// bone_data.set_transform(10.0, 0.0, 30.0, 1.0, 1.0, 0.0, 0.0);
/// bone_data.set_length(40.0);
/// assert_eq!(bone_data.get_parent_index(), Some(0));
/// assert_eq!(bone_data.get_rotation(), 30.0);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DBoneData {
	index: usize,
	name: String,
	parent_index: Option<usize>,
	x: f64,
	y: f64,
	rotation: f64,
	scale_x: f64,
	scale_y: f64,
	shear_x: f64,
	shear_y: f64,
	length: f64,
}

impl Skeleton2DBoneData {
	//==============================================================================
	/// 생성. (지역 값은 이동 0, 회전 0, 크기 1, 기울임 0, 길이 0)
	///
	/// # 인자
	/// - `index`: 뼈 번호.
	/// - `name`: 뼈 이름.
	/// - `parent_index`: 부모 뼈 번호. (뿌리는 None)
	///
	/// # 돌려줌
	/// 뼈 자료.
	//==============================================================================
	pub fn new(index: usize, name: &str, parent_index: Option<usize>) -> Skeleton2DBoneData {
		Skeleton2DBoneData {
			index,
			name: name.to_string(),
			parent_index,
			x: 0.0,
			y: 0.0,
			rotation: 0.0,
			scale_x: 1.0,
			scale_y: 1.0,
			shear_x: 0.0,
			shear_y: 0.0,
			length: 0.0,
		}
	}

	//==============================================================================
	/// 번호 반환.
	///
	/// # 돌려줌
	/// 뼈 번호.
	//==============================================================================
	pub fn get_index(&self) -> usize {
		self.index
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 뼈 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 부모 번호 반환.
	///
	/// # 돌려줌
	/// 부모 뼈 번호. (뿌리는 None)
	//==============================================================================
	pub fn get_parent_index(&self) -> Option<usize> {
		self.parent_index
	}

	//==============================================================================
	/// 처음 지역 값 설정.
	///
	/// # 인자
	/// - `x`: 부모 기준 x.
	/// - `y`: 부모 기준 y.
	/// - `rotation`: 회전. (도)
	/// - `scale_x`: 크기 x.
	/// - `scale_y`: 크기 y.
	/// - `shear_x`: 기울임 x. (도, x 축만 더 돈다)
	/// - `shear_y`: 기울임 y. (도, y 축만 더 돈다)
	//==============================================================================
	pub fn set_transform(&mut self, x: f64, y: f64, rotation: f64, scale_x: f64, scale_y: f64, shear_x: f64, shear_y: f64) {
		self.x = x;
		self.y = y;
		self.rotation = rotation;
		self.scale_x = scale_x;
		self.scale_y = scale_y;
		self.shear_x = shear_x;
		self.shear_y = shear_y;
	}

	//==============================================================================
	/// x 반환.
	///
	/// # 돌려줌
	/// 부모 기준 x.
	//==============================================================================
	pub fn get_x(&self) -> f64 {
		self.x
	}

	//==============================================================================
	/// y 반환.
	///
	/// # 돌려줌
	/// 부모 기준 y.
	//==============================================================================
	pub fn get_y(&self) -> f64 {
		self.y
	}

	//==============================================================================
	/// 회전 반환.
	///
	/// # 돌려줌
	/// 회전. (도)
	//==============================================================================
	pub fn get_rotation(&self) -> f64 {
		self.rotation
	}

	//==============================================================================
	/// 크기 x 반환.
	///
	/// # 돌려줌
	/// 크기 x.
	//==============================================================================
	pub fn get_scale_x(&self) -> f64 {
		self.scale_x
	}

	//==============================================================================
	/// 크기 y 반환.
	///
	/// # 돌려줌
	/// 크기 y.
	//==============================================================================
	pub fn get_scale_y(&self) -> f64 {
		self.scale_y
	}

	//==============================================================================
	/// 기울임 x 반환.
	///
	/// # 돌려줌
	/// 기울임 x. (도)
	//==============================================================================
	pub fn get_shear_x(&self) -> f64 {
		self.shear_x
	}

	//==============================================================================
	/// 기울임 y 반환.
	///
	/// # 돌려줌
	/// 기울임 y. (도)
	//==============================================================================
	pub fn get_shear_y(&self) -> f64 {
		self.shear_y
	}

	//==============================================================================
	/// 길이 설정. (IK 가 끝점을 셈하고, 기즈모가 뼈를 그린다)
	///
	/// # 인자
	/// - `length`: 뼈 길이. (지역 x 축 쪽)
	//==============================================================================
	pub fn set_length(&mut self, length: f64) {
		self.length = length;
	}

	//==============================================================================
	/// 길이 반환.
	///
	/// # 돌려줌
	/// 뼈 길이.
	//==============================================================================
	pub fn get_length(&self) -> f64 {
		self.length
	}
}

//==============================================================================
/// 슬롯 자료. (그림이 붙는 자리: 뼈, 처음 색, 처음 붙임, 섞기 방식)
///
/// # 예제
/// 슬롯 자료 하나 만들기
/// ```rust
/// use vanilla::experimental::skeleton2d::Skeleton2DSlotData;
/// use vanilla::{BlendMode, Color};
///
/// let mut slot_data = Skeleton2DSlotData::new(0, "body", 1);
/// slot_data.set_color(Color::create(1.0, 0.5, 0.5, 1.0));
/// slot_data.set_attachment_name(Some("body"));
/// slot_data.set_blend_mode(BlendMode::Lighter);
/// assert_eq!(slot_data.get_attachment_name(), Some("body"));
/// assert_eq!(slot_data.get_blend_mode(), BlendMode::Lighter);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DSlotData {
	index: usize,
	name: String,
	bone_index: usize,
	color: Color,
	attachment_name: Option<String>,
	blend_mode: BlendMode,
}

impl Skeleton2DSlotData {
	//==============================================================================
	/// 생성. (흰색, 붙임 없음, 섞기 Normal)
	///
	/// # 인자
	/// - `index`: 슬롯 번호. (처음 그리기 차례)
	/// - `name`: 슬롯 이름.
	/// - `bone_index`: 붙는 뼈 번호.
	///
	/// # 돌려줌
	/// 슬롯 자료.
	//==============================================================================
	pub fn new(index: usize, name: &str, bone_index: usize) -> Skeleton2DSlotData {
		Skeleton2DSlotData {
			index,
			name: name.to_string(),
			bone_index,
			color: Color::create(1.0, 1.0, 1.0, 1.0),
			attachment_name: None,
			blend_mode: BlendMode::Normal,
		}
	}

	//==============================================================================
	/// 번호 반환.
	///
	/// # 돌려줌
	/// 슬롯 번호.
	//==============================================================================
	pub fn get_index(&self) -> usize {
		self.index
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 슬롯 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 뼈 번호 반환.
	///
	/// # 돌려줌
	/// 붙는 뼈 번호.
	//==============================================================================
	pub fn get_bone_index(&self) -> usize {
		self.bone_index
	}

	//==============================================================================
	/// 처음 색 설정. (그림에 곱한다)
	///
	/// # 인자
	/// - `color`: 처음 색.
	//==============================================================================
	pub fn set_color(&mut self, color: Color) {
		self.color = color;
	}

	//==============================================================================
	/// 처음 색 반환.
	///
	/// # 돌려줌
	/// 처음 색.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.color
	}

	//==============================================================================
	/// 처음 붙임 이름 설정.
	///
	/// # 인자
	/// - `attachment_name`: 처음 붙임 이름. (None 이면 비운다)
	//==============================================================================
	pub fn set_attachment_name(&mut self, attachment_name: Option<&str>) {
		self.attachment_name = attachment_name.map(|name| name.to_string());
	}

	//==============================================================================
	/// 처음 붙임 이름 반환.
	///
	/// # 돌려줌
	/// 처음 붙임 이름. (없으면 None)
	//==============================================================================
	pub fn get_attachment_name(&self) -> Option<&str> {
		self.attachment_name.as_deref()
	}

	//==============================================================================
	/// 섞기 방식 설정.
	///
	/// # 인자
	/// - `blend_mode`: 섞기 방식.
	//==============================================================================
	pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
		self.blend_mode = blend_mode;
	}

	//==============================================================================
	/// 섞기 방식 반환.
	///
	/// # 돌려줌
	/// 섞기 방식.
	//==============================================================================
	pub fn get_blend_mode(&self) -> BlendMode {
		self.blend_mode
	}
}

//==============================================================================
/// IK 제약 자료. (뼈 하나는 끝을 목표로 돌리고, 뼈 둘(부모, 자식)은 두 뼈 IK 로 끝이 목표에 닿게 꺾는다)
///
/// # 예제
/// 두 뼈 IK 자료 하나 만들기
/// ```rust
/// use vanilla::experimental::skeleton2d::Skeleton2DIkConstraintData;
///
/// let mut ik_constraint_data = Skeleton2DIkConstraintData::new(0, "armIk", vec![1, 2], 3);
/// ik_constraint_data.set_bend_positive(false);
/// ik_constraint_data.set_mix(0.5);
/// assert_eq!(ik_constraint_data.get_bone_indices(), &[1, 2]);
/// assert!(!ik_constraint_data.is_bend_positive());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DIkConstraintData {
	index: usize,
	name: String,
	bone_indices: Vec<usize>,
	target_index: usize,
	is_bend_positive: bool,
	mix: f64,
}

impl Skeleton2DIkConstraintData {
	//==============================================================================
	/// 생성. (꺾임은 양의 쪽, 세기 1)
	///
	/// # 인자
	/// - `index`: 제약 번호. (거는 차례)
	/// - `name`: 제약 이름.
	/// - `bone_indices`: 움직일 뼈 번호. (하나, 또는 부모와 그 자식 둘)
	/// - `target_index`: 목표 뼈 번호.
	///
	/// # 돌려줌
	/// 제약 자료.
	//==============================================================================
	pub fn new(index: usize, name: &str, bone_indices: Vec<usize>, target_index: usize) -> Skeleton2DIkConstraintData {
		Skeleton2DIkConstraintData {
			index,
			name: name.to_string(),
			bone_indices,
			target_index,
			is_bend_positive: true,
			mix: 1.0,
		}
	}

	//==============================================================================
	/// 번호 반환.
	///
	/// # 돌려줌
	/// 제약 번호.
	//==============================================================================
	pub fn get_index(&self) -> usize {
		self.index
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 제약 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 움직일 뼈 번호 반환.
	///
	/// # 돌려줌
	/// 움직일 뼈 번호. (하나, 또는 부모와 자식 둘)
	//==============================================================================
	pub fn get_bone_indices(&self) -> &[usize] {
		&self.bone_indices
	}

	//==============================================================================
	/// 목표 뼈 번호 반환.
	///
	/// # 돌려줌
	/// 목표 뼈 번호.
	//==============================================================================
	pub fn get_target_index(&self) -> usize {
		self.target_index
	}

	//==============================================================================
	/// 꺾임 쪽 설정. (참이면 자식이 부모에 대해 양의 쪽, 화면에서 시계 방향으로 꺾인다)
	///
	/// # 인자
	/// - `is_bend_positive`: 양의 쪽으로 꺾으면 true.
	//==============================================================================
	pub fn set_bend_positive(&mut self, is_bend_positive: bool) {
		self.is_bend_positive = is_bend_positive;
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
	/// 세기 설정.
	///
	/// # 인자
	/// - `mix`: 세기. (0 이면 걸지 않고 1 이면 끝까지)
	//==============================================================================
	pub fn set_mix(&mut self, mix: f64) {
		self.mix = mix;
	}

	//==============================================================================
	/// 세기 반환.
	///
	/// # 돌려줌
	/// 세기. (0 ~ 1)
	//==============================================================================
	pub fn get_mix(&self) -> f64 {
		self.mix
	}
}

//==============================================================================
/// 네모 붙임. (아틀라스 조각 하나를 뼈에 붙인 네모, 축점을 뼈 기준 자리에 둔다)
///
/// # 예제
/// 네모 붙임 하나 만들기
/// ```rust
/// use vanilla::experimental::skeleton2d::Skeleton2DRegionAttachment;
///
/// let mut region_attachment = Skeleton2DRegionAttachment::new("head", "head.png");
/// region_attachment.set_transform(0.0, -10.0, 0.0, 1.0, 1.0);
/// region_attachment.set_size(32.0, 32.0);
/// region_attachment.set_pivot(0.5, 1.0);
/// assert_eq!(region_attachment.get_frame_name(), "head.png");
/// assert_eq!(region_attachment.get_pivot_y(), 1.0);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DRegionAttachment {
	name: String,
	frame_name: String,
	x: f64,
	y: f64,
	rotation: f64,
	scale_x: f64,
	scale_y: f64,
	width: f64,
	height: f64,
	pivot_x: f64,
	pivot_y: f64,
}

impl Skeleton2DRegionAttachment {
	//==============================================================================
	/// 생성. (뼈 자리에 가운데를 두고, 크기는 조각 크기)
	///
	/// # 인자
	/// - `name`: 붙임 이름.
	/// - `frame_name`: 아틀라스 조각 이름.
	///
	/// # 돌려줌
	/// 네모 붙임.
	//==============================================================================
	pub fn new(name: &str, frame_name: &str) -> Skeleton2DRegionAttachment {
		Skeleton2DRegionAttachment {
			name: name.to_string(),
			frame_name: frame_name.to_string(),
			x: 0.0,
			y: 0.0,
			rotation: 0.0,
			scale_x: 1.0,
			scale_y: 1.0,
			width: 0.0,
			height: 0.0,
			pivot_x: 0.5,
			pivot_y: 0.5,
		}
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 붙임 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 조각 이름 반환.
	///
	/// # 돌려줌
	/// 아틀라스 조각 이름.
	//==============================================================================
	pub fn get_frame_name(&self) -> &str {
		&self.frame_name
	}

	//==============================================================================
	/// 뼈 기준 변환 설정.
	///
	/// # 인자
	/// - `x`: 뼈 기준 x. (축점이 올 자리)
	/// - `y`: 뼈 기준 y.
	/// - `rotation`: 회전. (도)
	/// - `scale_x`: 크기 x.
	/// - `scale_y`: 크기 y.
	//==============================================================================
	pub fn set_transform(&mut self, x: f64, y: f64, rotation: f64, scale_x: f64, scale_y: f64) {
		self.x = x;
		self.y = y;
		self.rotation = rotation;
		self.scale_x = scale_x;
		self.scale_y = scale_y;
	}

	//==============================================================================
	/// x 반환.
	///
	/// # 돌려줌
	/// 뼈 기준 x.
	//==============================================================================
	pub fn get_x(&self) -> f64 {
		self.x
	}

	//==============================================================================
	/// y 반환.
	///
	/// # 돌려줌
	/// 뼈 기준 y.
	//==============================================================================
	pub fn get_y(&self) -> f64 {
		self.y
	}

	//==============================================================================
	/// 회전 반환.
	///
	/// # 돌려줌
	/// 회전. (도)
	//==============================================================================
	pub fn get_rotation(&self) -> f64 {
		self.rotation
	}

	//==============================================================================
	/// 크기 x 반환.
	///
	/// # 돌려줌
	/// 크기 x.
	//==============================================================================
	pub fn get_scale_x(&self) -> f64 {
		self.scale_x
	}

	//==============================================================================
	/// 크기 y 반환.
	///
	/// # 돌려줌
	/// 크기 y.
	//==============================================================================
	pub fn get_scale_y(&self) -> f64 {
		self.scale_y
	}

	//==============================================================================
	/// 크기 설정. (0 이면 조각 크기를 쓴다)
	///
	/// # 인자
	/// - `width`: 너비.
	/// - `height`: 높이.
	//==============================================================================
	pub fn set_size(&mut self, width: f64, height: f64) {
		self.width = width;
		self.height = height;
	}

	//==============================================================================
	/// 너비 반환.
	///
	/// # 돌려줌
	/// 너비. (0 이면 조각 너비)
	//==============================================================================
	pub fn get_width(&self) -> f64 {
		self.width
	}

	//==============================================================================
	/// 높이 반환.
	///
	/// # 돌려줌
	/// 높이. (0 이면 조각 높이)
	//==============================================================================
	pub fn get_height(&self) -> f64 {
		self.height
	}

	//==============================================================================
	/// 축점 설정. (네모 안의 몫, 왼쪽 위 0, 오른쪽 아래 1)
	///
	/// # 인자
	/// - `pivot_x`: 축점 x 몫.
	/// - `pivot_y`: 축점 y 몫.
	//==============================================================================
	pub fn set_pivot(&mut self, pivot_x: f64, pivot_y: f64) {
		self.pivot_x = pivot_x;
		self.pivot_y = pivot_y;
	}

	//==============================================================================
	/// 축점 x 반환.
	///
	/// # 돌려줌
	/// 축점 x 몫.
	//==============================================================================
	pub fn get_pivot_x(&self) -> f64 {
		self.pivot_x
	}

	//==============================================================================
	/// 축점 y 반환.
	///
	/// # 돌려줌
	/// 축점 y 몫.
	//==============================================================================
	pub fn get_pivot_y(&self) -> f64 {
		self.pivot_y
	}
}

//==============================================================================
/// 메시 붙임. (아틀라스 조각을 삼각형 메시로, 무게가 없으면 슬롯 뼈를 따르고 있으면 뼈 여럿의 무게로 꼭짓점을 옮긴다)
/// - 무게 없는 꼭짓점은 슬롯 뼈 기준 자리다.
/// - 무게 있는 꼭짓점은 꼭짓점마다 영향 수와, 영향마다 뼈 번호와 (그 뼈 기준 묶은 자리 x, y, 무게) 다.
///   월드 자리 = Σ 무게 x (뼈 월드 행렬 x 묶은 자리). (선형 블렌드 스키닝)
///
/// # 예제
/// 무게 있는 네모 메시 하나 만들기
/// ```rust
/// use vanilla::experimental::skeleton2d::Skeleton2DMeshAttachment;
///
/// let mut mesh_attachment = Skeleton2DMeshAttachment::new("cape", "cape.png");
/// mesh_attachment.set_geometry(vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0], vec![0, 1, 2, 0, 2, 3]);
/// mesh_attachment.set_weights(vec![1, 1, 1, 1], vec![0, 0, 1, 1], vec![-5.0, 0.0, 1.0, 5.0, 0.0, 1.0, 5.0, 10.0, 1.0, -5.0, 10.0, 1.0]);
/// assert_eq!(mesh_attachment.get_vertex_count(), 4);
/// assert!(mesh_attachment.is_weighted());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DMeshAttachment {
	name: String,
	frame_name: String,
	uvs: Vec<f64>,
	triangles: Vec<usize>,
	vertices: Vec<f64>,
	influence_counts: Vec<usize>,
	influence_bone_indices: Vec<usize>,
	influence_values: Vec<f64>,
}

impl Skeleton2DMeshAttachment {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `name`: 붙임 이름.
	/// - `frame_name`: 아틀라스 조각 이름.
	///
	/// # 돌려줌
	/// 꼭짓점 없는 메시 붙임.
	//==============================================================================
	pub fn new(name: &str, frame_name: &str) -> Skeleton2DMeshAttachment {
		Skeleton2DMeshAttachment {
			name: name.to_string(),
			frame_name: frame_name.to_string(),
			uvs: Vec::new(),
			triangles: Vec::new(),
			vertices: Vec::new(),
			influence_counts: Vec::new(),
			influence_bone_indices: Vec::new(),
			influence_values: Vec::new(),
		}
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 붙임 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 조각 이름 반환.
	///
	/// # 돌려줌
	/// 아틀라스 조각 이름.
	//==============================================================================
	pub fn get_frame_name(&self) -> &str {
		&self.frame_name
	}

	//==============================================================================
	/// UV 와 삼각형 설정.
	///
	/// # 인자
	/// - `uvs`: 꼭짓점마다 u, v. (조각 안 0 ~ 1)
	/// - `triangles`: 삼각형마다 꼭짓점 번호 셋.
	//==============================================================================
	pub fn set_geometry(&mut self, uvs: Vec<f64>, triangles: Vec<usize>) {
		self.uvs = uvs;
		self.triangles = triangles;
	}

	//==============================================================================
	/// UV 반환.
	///
	/// # 돌려줌
	/// 꼭짓점마다 u, v.
	//==============================================================================
	pub fn get_uvs(&self) -> &[f64] {
		&self.uvs
	}

	//==============================================================================
	/// 삼각형 반환.
	///
	/// # 돌려줌
	/// 삼각형마다 꼭짓점 번호 셋.
	//==============================================================================
	pub fn get_triangles(&self) -> &[usize] {
		&self.triangles
	}

	//==============================================================================
	/// 꼭짓점 수 반환. (UV 수로 센다)
	///
	/// # 돌려줌
	/// 꼭짓점 수.
	//==============================================================================
	pub fn get_vertex_count(&self) -> usize {
		self.uvs.len() / 2
	}

	//==============================================================================
	/// 무게 없는 꼭짓점 설정. (슬롯 뼈 기준 x, y, 무게는 지운다)
	///
	/// # 인자
	/// - `vertices`: 꼭짓점마다 x, y.
	//==============================================================================
	pub fn set_vertices(&mut self, vertices: Vec<f64>) {
		self.vertices = vertices;
		self.influence_counts.clear();
		self.influence_bone_indices.clear();
		self.influence_values.clear();
	}

	//==============================================================================
	/// 무게 없는 꼭짓점 반환.
	///
	/// # 돌려줌
	/// 꼭짓점마다 x, y. (무게 있는 메시는 빈 배열)
	//==============================================================================
	pub fn get_vertices(&self) -> &[f64] {
		&self.vertices
	}

	//==============================================================================
	/// 무게 설정. (무게 없는 꼭짓점은 지운다)
	///
	/// # 인자
	/// - `influence_counts`: 꼭짓점마다 영향 뼈 수.
	/// - `influence_bone_indices`: 영향마다 뼈 번호.
	/// - `influence_values`: 영향마다 그 뼈 기준 묶은 자리 x, y 와 무게.
	//==============================================================================
	pub fn set_weights(&mut self, influence_counts: Vec<usize>, influence_bone_indices: Vec<usize>, influence_values: Vec<f64>) {
		self.vertices.clear();
		self.influence_counts = influence_counts;
		self.influence_bone_indices = influence_bone_indices;
		self.influence_values = influence_values;
	}

	//==============================================================================
	/// 무게가 있는지.
	///
	/// # 돌려줌
	/// 무게로 옮기면 true.
	//==============================================================================
	pub fn is_weighted(&self) -> bool {
		!self.influence_counts.is_empty()
	}

	//==============================================================================
	/// 꼭짓점마다 영향 수 반환.
	///
	/// # 돌려줌
	/// 꼭짓점마다 영향 뼈 수.
	//==============================================================================
	pub fn get_influence_counts(&self) -> &[usize] {
		&self.influence_counts
	}

	//==============================================================================
	/// 영향마다 뼈 번호 반환.
	///
	/// # 돌려줌
	/// 영향마다 뼈 번호.
	//==============================================================================
	pub fn get_influence_bone_indices(&self) -> &[usize] {
		&self.influence_bone_indices
	}

	//==============================================================================
	/// 영향마다 묶은 자리와 무게 반환.
	///
	/// # 돌려줌
	/// 영향마다 x, y, 무게.
	//==============================================================================
	pub fn get_influence_values(&self) -> &[f64] {
		&self.influence_values
	}
}

//==============================================================================
/// 붙임. (원본은 네모와 메시 두 클래스가 같은 함수를 가진다. 여기서는 두 갈래 열거)
///
/// # 예제
/// 붙임 종류와 꼭짓점 수 읽기
/// ```rust
/// use vanilla::experimental::skeleton2d::{Skeleton2DAttachment, Skeleton2DAttachmentType, Skeleton2DRegionAttachment};
///
/// let attachment = Skeleton2DAttachment::Region(Skeleton2DRegionAttachment::new("face", "face.png"));
/// assert_eq!(attachment.get_attachment_type(), Skeleton2DAttachmentType::Region);
/// assert_eq!(attachment.get_vertex_count(), 4);
/// assert_eq!(attachment.get_triangles(), &[0, 1, 2, 0, 2, 3]);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub enum Skeleton2DAttachment {
	/// 네모 붙임.
	Region(Skeleton2DRegionAttachment),
	/// 메시 붙임.
	Mesh(Skeleton2DMeshAttachment),
}

impl Skeleton2DAttachment {
	//==============================================================================
	/// 종류 반환.
	///
	/// # 돌려줌
	/// 붙임 종류.
	//==============================================================================
	pub fn get_attachment_type(&self) -> Skeleton2DAttachmentType {
		match self {
			Skeleton2DAttachment::Region(_) => {
				Skeleton2DAttachmentType::Region
			}
			Skeleton2DAttachment::Mesh(_) => {
				Skeleton2DAttachmentType::Mesh
			}
		}
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 붙임 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		match self {
			Skeleton2DAttachment::Region(region_attachment) => {
				region_attachment.get_name()
			}
			Skeleton2DAttachment::Mesh(mesh_attachment) => {
				mesh_attachment.get_name()
			}
		}
	}

	//==============================================================================
	/// 조각 이름 반환.
	///
	/// # 돌려줌
	/// 아틀라스 조각 이름.
	//==============================================================================
	pub fn get_frame_name(&self) -> &str {
		match self {
			Skeleton2DAttachment::Region(region_attachment) => {
				region_attachment.get_frame_name()
			}
			Skeleton2DAttachment::Mesh(mesh_attachment) => {
				mesh_attachment.get_frame_name()
			}
		}
	}

	//==============================================================================
	/// 꼭짓점 UV 반환. (조각 안 0 ~ 1)
	///
	/// # 돌려줌
	/// 꼭짓점마다 u, v. (네모는 왼쪽 위부터 시계 방향 넷)
	//==============================================================================
	pub fn get_uvs(&self) -> &[f64] {
		match self {
			Skeleton2DAttachment::Region(_) => {
				&REGION_UVS
			}
			Skeleton2DAttachment::Mesh(mesh_attachment) => {
				mesh_attachment.get_uvs()
			}
		}
	}

	//==============================================================================
	/// 삼각형 꼭짓점 번호 반환.
	///
	/// # 돌려줌
	/// 삼각형마다 꼭짓점 번호 셋.
	//==============================================================================
	pub fn get_triangles(&self) -> &[usize] {
		match self {
			Skeleton2DAttachment::Region(_) => {
				&REGION_TRIANGLES
			}
			Skeleton2DAttachment::Mesh(mesh_attachment) => {
				mesh_attachment.get_triangles()
			}
		}
	}

	//==============================================================================
	/// 꼭짓점 수 반환.
	///
	/// # 돌려줌
	/// 꼭짓점 수. (네모는 4)
	//==============================================================================
	pub fn get_vertex_count(&self) -> usize {
		match self {
			Skeleton2DAttachment::Region(_) => {
				4
			}
			Skeleton2DAttachment::Mesh(mesh_attachment) => {
				mesh_attachment.get_vertex_count()
			}
		}
	}
}

//==============================================================================
/// 스킨. (슬롯마다 이름 붙은 붙임 묶음, 옷이나 무기 바꾸기 자리)
///
/// # 예제
/// 스킨에 붙임을 넣고 찾기
/// ```rust
/// use vanilla::experimental::skeleton2d::{Skeleton2DAttachment, Skeleton2DRegionAttachment, Skeleton2DSkin};
///
/// let mut skin = Skeleton2DSkin::new("armor");
/// skin.set_attachment(0, "head", Skeleton2DAttachment::Region(Skeleton2DRegionAttachment::new("helmet", "helmet.png")));
/// assert_eq!(skin.get_attachment(0, "head").expect("head").get_frame_name(), "helmet.png");
/// assert_eq!(skin.get_attachment_names(0), vec!["head"]);
/// assert!(skin.get_attachment(1, "head").is_none());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DSkin {
	name: String,
	attachment_list: Vec<(usize, String, Skeleton2DAttachment)>,
}

impl Skeleton2DSkin {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `name`: 스킨 이름.
	///
	/// # 돌려줌
	/// 빈 스킨.
	//==============================================================================
	pub fn new(name: &str) -> Skeleton2DSkin {
		Skeleton2DSkin {
			name: name.to_string(),
			attachment_list: Vec::new(),
		}
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 스킨 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 붙임 넣기. (같은 슬롯, 같은 이름이 있으면 바꾼다)
	///
	/// # 인자
	/// - `slot_index`: 슬롯 번호.
	/// - `attachment_name`: 슬롯 안의 붙임 이름.
	/// - `attachment`: 붙임.
	//==============================================================================
	pub fn set_attachment(&mut self, slot_index: usize, attachment_name: &str, attachment: Skeleton2DAttachment) {
		for (entry_slot_index, entry_name, entry_attachment) in self.attachment_list.iter_mut() {
			if *entry_slot_index == slot_index && entry_name == attachment_name {
				*entry_attachment = attachment;
				return;
			}
		}
		self.attachment_list.push((slot_index, attachment_name.to_string(), attachment));
	}

	//==============================================================================
	/// 붙임 찾기.
	///
	/// # 인자
	/// - `slot_index`: 슬롯 번호.
	/// - `attachment_name`: 슬롯 안의 붙임 이름.
	///
	/// # 돌려줌
	/// 붙임. (없으면 None)
	//==============================================================================
	pub fn get_attachment(&self, slot_index: usize, attachment_name: &str) -> Option<&Skeleton2DAttachment> {
		self.attachment_list.iter().find(|(entry_slot_index, entry_name, _)| *entry_slot_index == slot_index && entry_name == attachment_name).map(|(_, _, attachment)| attachment)
	}

	//==============================================================================
	/// 슬롯의 붙임 이름 목록 반환. (넣은 차례)
	///
	/// # 인자
	/// - `slot_index`: 슬롯 번호.
	///
	/// # 돌려줌
	/// 붙임 이름 목록.
	//==============================================================================
	pub fn get_attachment_names(&self, slot_index: usize) -> Vec<&str> {
		self.attachment_list.iter().filter(|(entry_slot_index, _, _)| *entry_slot_index == slot_index).map(|(_, entry_name, _)| entry_name.as_str()).collect()
	}

	//==============================================================================
	/// 모든 붙임 목록 반환. (조각 찾기 같은 데 쓴다)
	///
	/// # 돌려줌
	/// 붙임 목록. (넣은 차례)
	//==============================================================================
	pub fn get_attachment_list(&self) -> Vec<&Skeleton2DAttachment> {
		self.attachment_list.iter().map(|(_, _, attachment)| attachment).collect()
	}
}

//==============================================================================
/// 2D 뼈대 자료.
///
/// # 예제
/// 코드로 뼈 둘을 짓고 처음 자세의 월드 행렬 셈하기
/// ```rust
/// use vanilla::experimental::skeleton2d::{Skeleton2DBoneData, Skeleton2DData};
///
/// let mut skeleton_data = Skeleton2DData::new("arm");
/// let root_bone_data = Skeleton2DBoneData::new(0, "root", None);
/// let mut arm_bone_data = Skeleton2DBoneData::new(1, "arm", Some(0));
/// arm_bone_data.set_transform(10.0, 0.0, 90.0, 1.0, 1.0, 0.0, 0.0);
/// skeleton_data.add_bone(root_bone_data);
/// skeleton_data.add_bone(arm_bone_data);
/// let world_matrices = Skeleton2DData::compute_setup_world_matrices(skeleton_data.get_bones());
/// assert_eq!(skeleton_data.find_bone_index("arm"), Some(1));
/// assert_eq!(world_matrices[6 + 4], 10.0);
/// assert!((world_matrices[6 + 1] - 1.0).abs() < 0.0001);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct Skeleton2DData {
	name: String,
	frame_rate: f64,
	bones: Vec<Skeleton2DBoneData>,
	slots: Vec<Skeleton2DSlotData>,
	ik_constraints: Vec<Skeleton2DIkConstraintData>,
	skins: Vec<Skeleton2DSkin>,
	animations: Vec<Rc<Skeleton2DAnimation>>,
}

impl Skeleton2DData {
	//==============================================================================
	/// 생성. (빈 자료, 초당 프레임 30)
	///
	/// # 인자
	/// - `name`: 뼈대 이름.
	///
	/// # 돌려줌
	/// 빈 뼈대 자료.
	//==============================================================================
	pub fn new(name: &str) -> Skeleton2DData {
		Skeleton2DData {
			name: name.to_string(),
			frame_rate: 30.0,
			bones: Vec::new(),
			slots: Vec::new(),
			ik_constraints: Vec::new(),
			skins: Vec::new(),
			animations: Vec::new(),
		}
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 뼈대 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 초당 프레임 설정. (만든 도구의 값, 시각은 모두 초라 재생에는 쓰지 않는다)
	///
	/// # 인자
	/// - `frame_rate`: 초당 프레임.
	//==============================================================================
	pub fn set_frame_rate(&mut self, frame_rate: f64) {
		self.frame_rate = frame_rate;
	}

	//==============================================================================
	/// 초당 프레임 반환.
	///
	/// # 돌려줌
	/// 초당 프레임.
	//==============================================================================
	pub fn get_frame_rate(&self) -> f64 {
		self.frame_rate
	}

	//==============================================================================
	/// 뼈 더하기. (부모가 앞에 있어야 한다)
	///
	/// # 인자
	/// - `bone_data`: 더할 뼈. (번호는 지금 뼈 수와 같아야 한다)
	//==============================================================================
	pub fn add_bone(&mut self, bone_data: Skeleton2DBoneData) {
		self.bones.push(bone_data);
	}

	//==============================================================================
	/// 뼈 목록 반환.
	///
	/// # 돌려줌
	/// 뼈 목록. (부모가 앞)
	//==============================================================================
	pub fn get_bones(&self) -> &[Skeleton2DBoneData] {
		&self.bones
	}

	//==============================================================================
	/// 슬롯 더하기.
	///
	/// # 인자
	/// - `slot_data`: 더할 슬롯. (번호는 지금 슬롯 수와 같아야 한다)
	//==============================================================================
	pub fn add_slot(&mut self, slot_data: Skeleton2DSlotData) {
		self.slots.push(slot_data);
	}

	//==============================================================================
	/// 슬롯 목록 반환.
	///
	/// # 돌려줌
	/// 슬롯 목록. (처음 그리기 차례)
	//==============================================================================
	pub fn get_slots(&self) -> &[Skeleton2DSlotData] {
		&self.slots
	}

	//==============================================================================
	/// IK 제약 더하기.
	///
	/// # 인자
	/// - `ik_constraint_data`: 더할 제약.
	//==============================================================================
	pub fn add_ik_constraint(&mut self, ik_constraint_data: Skeleton2DIkConstraintData) {
		self.ik_constraints.push(ik_constraint_data);
	}

	//==============================================================================
	/// IK 제약 목록 반환.
	///
	/// # 돌려줌
	/// 제약 목록. (거는 차례)
	//==============================================================================
	pub fn get_ik_constraints(&self) -> &[Skeleton2DIkConstraintData] {
		&self.ik_constraints
	}

	//==============================================================================
	/// 스킨 더하기.
	///
	/// # 인자
	/// - `skin`: 더할 스킨.
	//==============================================================================
	pub fn add_skin(&mut self, skin: Skeleton2DSkin) {
		self.skins.push(skin);
	}

	//==============================================================================
	/// 스킨 목록 반환.
	///
	/// # 돌려줌
	/// 스킨 목록.
	//==============================================================================
	pub fn get_skins(&self) -> &[Skeleton2DSkin] {
		&self.skins
	}

	//==============================================================================
	/// 클립 더하기.
	///
	/// # 인자
	/// - `animation`: 더할 클립.
	//==============================================================================
	pub fn add_animation(&mut self, animation: Skeleton2DAnimation) {
		self.animations.push(Rc::new(animation));
	}

	//==============================================================================
	/// 클립 목록 반환.
	///
	/// # 돌려줌
	/// 클립 목록.
	//==============================================================================
	pub fn get_animations(&self) -> &[Rc<Skeleton2DAnimation>] {
		&self.animations
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
	pub fn find_bone(&self, bone_name: &str) -> Option<&Skeleton2DBoneData> {
		self.bones.iter().find(|bone_data| bone_data.get_name() == bone_name)
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
		self.bones.iter().position(|bone_data| bone_data.get_name() == bone_name)
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
	pub fn find_slot(&self, slot_name: &str) -> Option<&Skeleton2DSlotData> {
		self.slots.iter().find(|slot_data| slot_data.get_name() == slot_name)
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
		self.slots.iter().position(|slot_data| slot_data.get_name() == slot_name)
	}

	//==============================================================================
	/// 스킨 찾기.
	///
	/// # 인자
	/// - `skin_name`: 스킨 이름.
	///
	/// # 돌려줌
	/// 스킨. (없으면 None)
	//==============================================================================
	pub fn find_skin(&self, skin_name: &str) -> Option<&Skeleton2DSkin> {
		self.skins.iter().find(|skin| skin.get_name() == skin_name)
	}

	//==============================================================================
	/// 스킨 번호 찾기.
	///
	/// # 인자
	/// - `skin_name`: 스킨 이름.
	///
	/// # 돌려줌
	/// 스킨 번호. (없으면 None)
	//==============================================================================
	pub fn find_skin_index(&self, skin_name: &str) -> Option<usize> {
		self.skins.iter().position(|skin| skin.get_name() == skin_name)
	}

	//==============================================================================
	/// 처음 스킨 번호 반환.
	///
	/// # 돌려줌
	/// "default" 스킨, 없으면 첫 스킨의 번호. (스킨이 없으면 None)
	//==============================================================================
	pub fn get_default_skin_index(&self) -> Option<usize> {
		match self.find_skin_index(DEFAULT_SKIN_NAME) {
			Some(skin_index) => {
				Some(skin_index)
			}
			None => {
				if self.skins.is_empty() {
					None
				}
				else {
					Some(0)
				}
			}
		}
	}

	//==============================================================================
	/// 처음 스킨 반환.
	///
	/// # 돌려줌
	/// "default" 스킨, 없으면 첫 스킨. (스킨이 없으면 None)
	//==============================================================================
	pub fn get_default_skin(&self) -> Option<&Skeleton2DSkin> {
		self.get_default_skin_index().map(|skin_index| &self.skins[skin_index])
	}

	//==============================================================================
	/// 클립 찾기.
	///
	/// # 인자
	/// - `animation_name`: 클립 이름.
	///
	/// # 돌려줌
	/// 클립. (없으면 None)
	//==============================================================================
	pub fn find_animation(&self, animation_name: &str) -> Option<&Rc<Skeleton2DAnimation>> {
		self.animations.iter().find(|animation| animation.get_name() == animation_name)
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
	pub fn find_ik_constraint(&self, ik_constraint_name: &str) -> Option<&Skeleton2DIkConstraintData> {
		self.ik_constraints.iter().find(|ik_constraint_data| ik_constraint_data.get_name() == ik_constraint_name)
	}

	//==============================================================================
	/// 처음 자세의 월드 행렬 셈. (IK 는 걸지 않는다)
	///
	/// # 인자
	/// - `bone_data_list`: 뼈 목록. (부모가 앞)
	///
	/// # 돌려줌
	/// 뼈마다 월드 행렬 a, b, c, d, x, y 여섯 값.
	//==============================================================================
	pub fn compute_setup_world_matrices(bone_data_list: &[Skeleton2DBoneData]) -> Vec<f64> {
		let mut world_matrices = vec![0.0f64; bone_data_list.len() * 6];
		for (bone_index, bone_data) in bone_data_list.iter().enumerate() {
			let x_axis_radian = (bone_data.get_rotation() + bone_data.get_shear_x()) * PI / 180.0;
			let y_axis_radian = (bone_data.get_rotation() + bone_data.get_shear_y()) * PI / 180.0;
			let local_a = trigonometry::cos(x_axis_radian) * bone_data.get_scale_x();
			let local_b = trigonometry::sin(x_axis_radian) * bone_data.get_scale_x();
			let local_c = -trigonometry::sin(y_axis_radian) * bone_data.get_scale_y();
			let local_d = trigonometry::cos(y_axis_radian) * bone_data.get_scale_y();
			let x = bone_data.get_x();
			let y = bone_data.get_y();
			let offset = bone_index * 6;
			match bone_data.get_parent_index() {
				Some(parent_index) => {
					let parent_offset = parent_index * 6;
					let parent_a = world_matrices[parent_offset];
					let parent_b = world_matrices[parent_offset + 1];
					let parent_c = world_matrices[parent_offset + 2];
					let parent_d = world_matrices[parent_offset + 3];
					let parent_x = world_matrices[parent_offset + 4];
					let parent_y = world_matrices[parent_offset + 5];
					world_matrices[offset] = parent_a * local_a + parent_c * local_b;
					world_matrices[offset + 1] = parent_b * local_a + parent_d * local_b;
					world_matrices[offset + 2] = parent_a * local_c + parent_c * local_d;
					world_matrices[offset + 3] = parent_b * local_c + parent_d * local_d;
					world_matrices[offset + 4] = parent_a * x + parent_c * y + parent_x;
					world_matrices[offset + 5] = parent_b * x + parent_d * y + parent_y;
				}
				None => {
					world_matrices[offset..offset + 6].copy_from_slice(&[local_a, local_b, local_c, local_d, x, y]);
				}
			}
		}
		world_matrices
	}

	//==============================================================================
	/// 엔진 자체 JSON 으로 짓기.
	/// - 꼴: { name, frameRate, bones: [{ name, parent, x, y, rotation, scaleX, scaleY, shearX, shearY, length }],
	///   slots: [{ name, bone, attachment, color: "#rrggbb(aa)", blendMode }], ik: [{ name, bones: [부모, 자식], target, bendPositive, mix }],
	///   skins: [{ name, attachments: { 슬롯: { 붙임: { type: "region" | "mesh", frame, ... } } } }],
	///   animations: { 이름: { duration, bones: { 뼈: { rotate, translate, scale, shear } }, slots: { 슬롯: { color, attachment } }, drawOrder, events } } }
	/// - 네모 붙임: x, y, rotation, scaleX, scaleY, width, height, pivotX, pivotY. 메시 붙임: vertices(슬롯 뼈 기준), uvs, triangles, weights(꼭짓점마다 [뼈 이름, 무게, ...]).
	///   무게 있는 메시의 묶은 자리는 처음 자세에서 셈한다. (유니티 2D Animation, DragonBones 와 같은 꼴)
	/// - 키: { time(초), value 또는 x, y 또는 color 또는 name, curve: "linear" | "stepped" | [x1, y1, x2, y2] }. 그리기 차례 키의 order 는 슬롯 이름 모두다.
	///
	/// # 인자
	/// - `json`: JSON 값.
	///
	/// # 돌려줌
	/// 지은 자료. 꼴이 틀리면 `"[Skeleton2D]"` 로 시작하는 Err. (원본과 같은 글)
	///
	/// # 예제
	/// 걷기 클립 하나를 가진 자료
	/// ```rust
	/// use vanilla::experimental::skeleton2d::Skeleton2DData;
	///
	/// let json = serde_json::json!({
	/// 	"bones": [{ "name": "root" }, { "name": "leg", "parent": "root", "rotation": 90, "length": 30 }],
	/// 	"animations": { "walk": {
	/// 		"bones": { "leg": { "rotate": [{ "time": 0, "value": -20, "curve": [0.42, 0, 0.58, 1] }, { "time": 0.5, "value": 20 }, { "time": 1, "value": -20 }] } },
	/// 		"events": [{ "time": 0.5, "name": "step" }]
	/// 	} }
	/// });
	/// let skeleton_data = Skeleton2DData::create_from_json(&json).expect("skeleton");
	/// let walk_animation = skeleton_data.find_animation("walk").expect("walk");
	/// assert_eq!(walk_animation.get_duration(), 1.0);
	/// assert_eq!(walk_animation.get_timelines().len(), 2);
	/// ```
	//==============================================================================
	pub fn create_from_json(json: &serde_json::Value) -> Result<Skeleton2DData, String> {
		read_skeleton2d_json(json)
	}

	//==============================================================================
	/// DragonBones 5.x JSON 으로 짓기.
	/// - 뼈(transform 의 x, y, skX, skY, scX, scY, length), 슬롯(displayIndex, color 의 곱, blendMode), 스킨의 image 와 mesh(무게는 slotPose, bonePose),
	///   IK(bone, target, chain 0 이나 1, bendPositive, weight), 클립(translateFrame, rotateFrame 의 rotate 와 skew, scaleFrame, displayFrame, colorFrame, zOrder, 사건)을 읽는다.
	/// - 회전은 skY, 기울임 y 는 skX - skY 다. 프레임 수는 frameRate 로 초가 된다. 회전 키는 clockwise 를 따라 이어지는 값으로 펴서 담는다.
	/// - tweenEasing 이 없으면 계단, 0 이면 선형, 그 밖은 같은 꼴의 베지어로, curve 는 [x1, y1, x2, y2] 하나만 베지어로 읽는다. (여러 토막 곡선은 선형)
	/// - 읽지 않는 것: 뼈의 inheritRotation, inheritScale 끄기, 색 더하기(aO 등), armature, boundingBox, path 붙임, ffd, IK 시간줄, 소리와 행동.
	///
	/// # 인자
	/// - `json`: DragonBones JSON 값. (_ske.json)
	/// - `armature_name`: 읽을 뼈대 이름. (None 이면 첫 뼈대)
	///
	/// # 돌려줌
	/// 지은 자료. 꼴이 틀리면 `"[Skeleton2D] DragonBones"` 로 시작하는 Err.
	///
	/// # 예제
	/// DragonBones 뼈대 하나 읽기
	/// ```rust
	/// use vanilla::experimental::skeleton2d::Skeleton2DData;
	///
	/// let json = serde_json::json!({
	/// 	"frameRate": 24,
	/// 	"armature": [{
	/// 		"name": "hero",
	/// 		"bone": [{ "name": "root" }, { "name": "arm", "parent": "root", "length": 40, "transform": { "x": 10, "skX": 30, "skY": 30 } }],
	/// 		"slot": [{ "name": "arm", "parent": "arm" }],
	/// 		"skin": [{ "slot": [{ "name": "arm", "display": [{ "name": "arm_image" }] }] }],
	/// 		"animation": [{ "name": "wave", "duration": 24, "bone": [{ "name": "arm", "rotateFrame": [{ "duration": 24, "tweenEasing": 0, "rotate": 0 }, { "duration": 0, "rotate": 45 }] }] }]
	/// 	}]
	/// });
	/// let skeleton_data = Skeleton2DData::create_from_dragon_bones(&json, None).expect("skeleton");
	/// assert_eq!(skeleton_data.get_name(), "hero");
	/// assert_eq!(skeleton_data.find_bone("arm").expect("arm").get_rotation(), 30.0);
	/// assert_eq!(skeleton_data.find_animation("wave").expect("wave").get_duration(), 1.0);
	/// ```
	//==============================================================================
	pub fn create_from_dragon_bones(json: &serde_json::Value, armature_name: Option<&str>) -> Result<Skeleton2DData, String> {
		read_dragon_bones_json(json, armature_name)
	}
}

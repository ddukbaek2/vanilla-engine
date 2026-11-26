//==============================================================================
//! 툰 3D GPU 효과의 바탕. (vanilla.js graphics3d/toon/toonvfx.js 의 ToonVfx)
//! - 텍스처: 그림 파일 없이 처음 그릴 때 코드로 짓는 아틀라스 한 장. 왼쪽 반은 연기 플립북 16 칸, 오른쪽 반은 ToonVfxSlot 칸과 색 램프 여덟 줄.
//!   램프 색은 게임이 정한다. (set_ramps, 줄마다 흰 심에서 어둠으로 가는 색들, 효과의 ramp 는 줄 번호, -1 이면 램프 없이 color 만)
//! - 파티클: 인스턴스로 그리는 GPU 파티클. 낼 때 한 번 버퍼에 쓰고 자리, 크기, 색은 정점 셰이더가 나이로 셈한다. (끌림, 중력, 바닥에서 멈춤, 속도 쪽으로 늘인 줄기, 색 램프)
//!   상한(set_particle_capacity)을 넘으면 가장 오래 전에 낸 것부터 덮고, set_count_ratio 로 내는 수에 몫을 곱한다. (폰과 가벼운 길)
//! - 효과 메쉬: 불덩이(add_fireball), 충격파 띠와 바닥 빛(add_shockwave), 바닥 자국(add_decal, queue_decal), 연기 기둥(add_plume),
//!   회전체 빔(queue_beam), 카메라 쪽으로 편 리본(queue_ribbon)과 그것을 감아 내린 번개(queue_lightning), 사는 동안 다시 꺾이는 번개(add_lightning), 그 자리의 빛 점(queue_glint).
//! - 테: 빛 파티클의 rim, 리본의 glow 와 rim, 불덩이와 충격파의 rim 은 가장자리를 테 색(set_rim_color)으로 덮어 밝은 바탕에서도 보이게 한다. (기본은 더하는 빛만)
//! - 밝기 배: set_look_gain(꼴, 배) 로 파티클과 빛 점의 꼴마다 밝기를 곱한다. (기본 1, HDR 판에서 불꽃 줄기, 빛살, 불씨를 선형 몇 배로 할 때)
//! - 나이와 열쇠: emit_particles, add_fireball, add_shockwave, add_decal, add_plume, add_lightning, add_flash 는 options 의 age(이미 지난 초, 음수면 그만큼 늦게)와
//!   key(사건 열쇠, 이미 낸 열쇠면 다시 내지 않음, clear 가 잊음)를 받는다. 장면이 매 프레임 상태에서 사건을 다시 내도 한 번만, 늦게 보인 효과는 그 나이부터 튼다.
//!   흔들림(add_shake)은 Shaker 의 지수 감쇠(초당 9)라 늦게 보인 사건은 세기 x exp(-9 x 나이) 로 넘기면 같다.
//! - 발밑 그늘: queue_blob_shadow 는 그 프레임의 둥근 그늘 자국(ToonVfxDecalMode::Shade)이다.
//! - 화면: 섬광(add_flash)은 draw 끝에 지금 뷰포트(가위가 걸려 있으면 그 안)를 그 색으로 덮고 걷힌다.
//!   카메라 흔들림(add_shake)은 엔진 Shaker 로 셈하고, read_shake 가 준 오프셋을 게임이 카메라의 오른쪽과 위쪽으로 더한다.
//! - 어느 사건에 어느 효과를 어떤 값으로 낼지(처방)는 게임 몫이다. 엔진은 매개변수만 받는다.
//! - 바닥: set_floor_height_reader 로 게임이 바닥 높이를 알려 주면 자국, 충격파, 연기 기둥은 그 높이에, is_grounded 파티클은 그 높이에서 멈춘다. (없으면 0)
//! - 시계: update(초) 가 이 바탕의 시계를 나아가게 한다. queue 로 넘긴 것은 그 프레임에 한 번 그리고 비운다.
//! - 그리기: draw(gl, 보기, 투영) 를 툰 모형을 그린 뒤 부른다. 깊이 시험은 켜고 깊이는 쓰지 않으며 미리 곱한 알파로 섞는다. 앞의 상태(프로그램과 버텍스 어레이 포함)를 적어 두었다 돌린다.
//! - 원본과 다른 점: 효과마다의 options 객체 대신 기본값을 가진 구조체(ToonParticleOptions 등)를 받는다. 범위가 되는 값은 ToonVfxRange 다.
//!   원본에서 주지 않아도 되는 값(undefined)은 Option 이다. add_lightning 의 life, age 는 ToonLightningOptions 에 함께 든다.
//!   사건 열쇠는 원본이 수나 글을 받지만 러스트 판은 u64 다. (글 열쇠는 게임이 수로 바꿔 넘긴다)
//==============================================================================
use std::collections::HashSet;

use glow::HasContext;

use crate::base::matrix4::Matrix4;
use crate::base::seededrandom::SeededRandom;
use crate::base::vector2::Vector2;
use crate::misc::shaker::{Shaker, ShakerOptions};
use crate::core::graphic::{GlBuffer, GlTexture, GlVertexArray};
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::toon::meshbuilder::read_color_values;

/// 아틀라스. (픽셀, 왼쪽 반은 연기 플립북 4 x 4 칸, 오른쪽 반은 128 픽셀 칸 열여섯)
const ATLAS_WIDTH: usize = 1024;
const ATLAS_HEIGHT: usize = 512;
const ATLAS_SLOT_SIZE: usize = 128;
const SMOKE_FRAME_COUNT: usize = 16;

/// 색 램프 줄의 수 상한. (아틀라스의 램프 칸 하나에 여덟 줄)
pub const TOON_VFX_MAXIMUM_RAMPS: usize = 8;

/// 한꺼번에 떠 있는 파티클의 기본 상한.
pub const TOON_VFX_DEFAULT_PARTICLE_CAPACITY: usize = 3000;

/// 파티클 하나의 값. (vec4 일곱)
const PARTICLE_FLOAT_COUNT: usize = 28;
/// 바닥에 멈추지 않는 파티클의 바닥 높이.
const NO_FLOOR: f32 = -100000.0;
/// 불덩이 구와 충격파 띠, 회전체(빔, 연기 기둥)의 나눔. (둘레, 가로질러나 윤곽을 따라)
const FIREBALL_SEGMENTS: [usize; 2] = [32, 20];
const BAND_SEGMENTS: [usize; 2] = [96, 8];
const LATHE_SEGMENTS: [usize; 2] = [40, 28];
/// 리본 정점 하나의 값. (x, y, z, 길이 쪽 0 ~ 1, 가로 0 ~ 1)
const RIBBON_FLOAT_COUNT: usize = 5;
/// 번개가 다시 꺾이는 초와 번개 하나의 마디 수.
const LIGHTNING_KINK_SECONDS: f32 = 0.08;
const LIGHTNING_POINT_COUNT: usize = 22;
/// 프레임마다 넘기는 빛 점의 수 기본 상한. (set_glint_capacity)
const DEFAULT_GLINT_CAPACITY: usize = 96;
/// 파티클 꼴의 수. (꼴마다 밝기 배)
const LOOK_COUNT: usize = 7;
/// 발밑 그늘의 기본 짙기. (가운데)
const DEFAULT_BLOB_SHADOW_ALPHA: f32 = 0.45;
/// 바닥 자국과 충격파 바닥 빛을 바닥 위로 띄우는 높이.
const DECAL_LIFT: f32 = 0.014;
const GROUND_GLOW_LIFT: f32 = 0.02;
/// 아틀라스를 묶는 텍스처 유닛. (툰 렌더러의 그림자 유닛 3, 화면 공간 바깥선의 5, 6 과 겹치지 않게)
const VFX_TEXTURE_UNIT: u32 = 4;
/// 효과 테의 기본 색. (테를 두른 파티클과 번짐 리본의 가장자리, ToonRenderer 의 기본 바깥선 색과 같다)
const DEFAULT_RIM_COLOR: [f32; 3] = [0.05, 0.05, 0.08];
/// 카메라 흔들림을 Shaker 에 넣을 때 곱하는 몫. (Shaker 는 세기 0.05 아래에서 멈춘다. 그 문턱이 화면 픽셀 기준이라 장면 길이를 백 배로 넣는다)
const SHAKE_UNIT_SCALE: f32 = 100.0;
/// 번개가 끝에 엷어지는 초.
const LIGHTNING_FADE_SECONDS: f32 = 0.1;
/// 어트리뷰트 자리.
const PARTICLE_ATTRIBUTE_LOCATIONS: [(&str, u32); 8] = [("vertexCorner", 0), ("particleStart", 1), ("particleVelocity", 2), ("particleMotion", 3), ("particleColor", 4), ("particleLook", 5), ("particleExtra", 6), ("particleShape", 7)];
const SPHERE_ATTRIBUTE_LOCATIONS: [(&str, u32); 1] = [("vertexPosition", 0)];
const GRID_ATTRIBUTE_LOCATIONS: [(&str, u32); 1] = [("vertexGrid", 0)];
const CORNER_ATTRIBUTE_LOCATIONS: [(&str, u32); 1] = [("vertexCorner", 0)];
const RIBBON_ATTRIBUTE_LOCATIONS: [(&str, u32); 2] = [("vertexPosition", 0), ("vertexRibbon", 1)];

//==============================================================================
/// 아틀라스 오른쪽 반의 칸. (셰이더의 readSlot 번호, 6, 7, 11 ~ 15 는 비어 있다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonVfxSlot {
	/// 부드러운 알갱이.
	Soft,
	/// 불꽃 줄기. (v 1 쪽이 머리)
	Streak,
	/// 빛살.
	Star,
	/// 충격 띠.
	ShockBand,
	/// 빔 결.
	BeamGrain,
	/// 그을음. (알파는 어둡게 덮는 몫, 빨간 채널은 열기)
	Scorch,
	/// 색 램프 여덟 줄.
	Ramps,
	/// 패인 땅.
	Dent,
	/// 넓은 둘레 빛.
	Glow,
}

impl ToonVfxSlot {
	//==============================================================================
	/// 칸 번호.
	///
	/// # 돌려줌
	/// 칸 번호. (셰이더의 readSlot 번호)
	//==============================================================================
	pub fn get_index(&self) -> usize {
		match self {
			ToonVfxSlot::Soft => {
				0
			}
			ToonVfxSlot::Streak => {
				1
			}
			ToonVfxSlot::Star => {
				2
			}
			ToonVfxSlot::ShockBand => {
				3
			}
			ToonVfxSlot::BeamGrain => {
				4
			}
			ToonVfxSlot::Scorch => {
				5
			}
			ToonVfxSlot::Ramps => {
				8
			}
			ToonVfxSlot::Dent => {
				9
			}
			ToonVfxSlot::Glow => {
				10
			}
		}
	}
}

//==============================================================================
/// 파티클의 꼴. (셰이더의 look 번호와 같은 차례)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonVfxLook {
	/// 부드러운 알갱이. (더하는 빛)
	Soft,
	/// 속도 쪽으로 늘인 불꽃 줄기. (더하는 빛, stretch 가 속도에 곱하는 늘임)
	Streak,
	/// 연기 플립북. (덮기)
	Smoke,
	/// 빛살. (더하는 빛)
	Star,
	/// 깜빡이는 불씨. (더하는 빛)
	Ember,
	/// 넓은 둘레 빛. (더하는 빛)
	Glow,
	/// 빛을 내지 않는 돌과 흙 알갱이. (덮기)
	Grit,
}

impl ToonVfxLook {
	//==============================================================================
	/// 셰이더 번호.
	//==============================================================================
	fn get_index(&self) -> f32 {
		match self {
			ToonVfxLook::Soft => {
				0.0
			}
			ToonVfxLook::Streak => {
				1.0
			}
			ToonVfxLook::Smoke => {
				2.0
			}
			ToonVfxLook::Star => {
				3.0
			}
			ToonVfxLook::Ember => {
				4.0
			}
			ToonVfxLook::Glow => {
				5.0
			}
			ToonVfxLook::Grit => {
				6.0
			}
		}
	}
}

//==============================================================================
/// 파티클을 내는 꼴.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonVfxShape {
	/// 한 점에서 모든 쪽으로.
	Point,
	/// 반지름 radius 공 안에서 바깥쪽으로.
	Sphere,
	/// 바닥과 나란한 고리 위에서 바깥쪽으로.
	Ring,
	/// 바닥과 나란한 원판 안에서 바깥쪽으로.
	Disc,
	/// direction 을 가운데로 spread 각 안의 원뿔.
	Cone,
	/// direction 쪽으로 length 까지의 선 위(둘레 radius 안)에서 direction 쪽으로.
	Line,
}

//==============================================================================
/// 바닥 자국의 꼴.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonVfxDecalMode {
	/// 알파로 어둡게 덮고 빨간 채널의 금에서 열기가 식는다.
	Scorch,
	/// 칸의 알파만큼 더하는 빛.
	Glow,
	/// 둘레로 흐르는 잡음 테. (ring 이 (테 반지름, 테 너비, 잡음 잘기, 흐르는 빠르기) 의 반지름 몫)
	Ring,
	/// 칸의 알파만큼 tint 색으로 덮고, 칸의 가운데 짙기(테 둘레)는 테 색(set_rim_color) 쪽으로 물든다. (밝은 바닥의 얼룩, 서리)
	Stain,
	/// 칸의 알파 몸을 tint 로 덮고 몸 가장자리에 흰 윤을 두른다. (기름 윤)
	Sheen,
	/// 가운데에서 바깥으로 둥글게 엷어지는 그늘. (알파만큼 어둡게 덮음, 칸은 보지 않음, 발밑 그늘 queue_blob_shadow)
	Shade,
}

impl ToonVfxDecalMode {
	//==============================================================================
	/// 셰이더 번호.
	//==============================================================================
	fn get_index(&self) -> f32 {
		match self {
			ToonVfxDecalMode::Scorch => {
				0.0
			}
			ToonVfxDecalMode::Glow => {
				1.0
			}
			ToonVfxDecalMode::Ring => {
				2.0
			}
			ToonVfxDecalMode::Stain => {
				3.0
			}
			ToonVfxDecalMode::Sheen => {
				4.0
			}
			ToonVfxDecalMode::Shade => {
				5.0
			}
		}
	}
}

//==============================================================================
/// 값의 범위. (최소와 최대 사이 무작위, 같으면 그 값)
///
/// # 예제
/// 무작위 빠르기와 고른 크기로 파티클 값 채우기
/// ```rust
/// use vanilla::{ToonParticleOptions, ToonVfxRange};
///
/// let speed = ToonVfxRange::new(2.0, 5.0);
/// let size = ToonVfxRange::from(0.1);
/// assert_eq!(size.minimum, size.maximum);
/// let options = ToonParticleOptions { count: 40, speed, size, life: ToonVfxRange::new(0.4, 0.9), ..ToonParticleOptions::default() };
/// assert_eq!(options.speed.maximum, 5.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonVfxRange {
	/// 최소.
	pub minimum: f32,
	/// 최대.
	pub maximum: f32,
}

impl ToonVfxRange {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `minimum`: 최소.
	/// - `maximum`: 최대.
	///
	/// # 돌려줌
	/// 새 범위.
	//==============================================================================
	pub fn new(minimum: f32, maximum: f32) -> ToonVfxRange {
		ToonVfxRange { minimum, maximum }
	}

	//==============================================================================
	/// 값 하나 읽기.
	//==============================================================================
	fn read_value(&self, seeded_random: &mut SeededRandom) -> f32 {
		if self.minimum == self.maximum {
			return self.minimum;
		}
		self.minimum + (self.maximum - self.minimum) * seeded_random.next_value() as f32
	}
}

impl From<f32> for ToonVfxRange {
	//==============================================================================
	/// 값 하나로. (최소와 최대가 같은 범위, 원본이 수 하나를 받던 자리)
	//==============================================================================
	fn from(value: f32) -> Self {
		ToonVfxRange { minimum: value, maximum: value }
	}
}

//==============================================================================
/// 파티클 내기의 값.
///
/// # 예제
/// 원뿔로 튀는 불꽃 줄기, 같은 열쇠로 다시 내면 무시
/// ```rust,no_run
/// use vanilla::{ToonParticleOptions, ToonVfx, ToonVfxLook, ToonVfxRange, ToonVfxShape};
///
/// fn emit_sparks(vfx: &mut ToonVfx) {
/// 	let spark_options = ToonParticleOptions {
/// 		count: 30,
/// 		position: [0.0, 1.0, 0.0],
/// 		shape: ToonVfxShape::Cone,
/// 		direction: [0.0, 1.0, 0.0],
/// 		spread: 0.4,
/// 		speed: ToonVfxRange::new(3.0, 6.0),
/// 		gravity: 9.0,
/// 		drag: 1.5,
/// 		size: ToonVfxRange::new(0.04, 0.08),
/// 		size_end: ToonVfxRange::from(0.3),
/// 		life: ToonVfxRange::new(0.5, 1.0),
/// 		look: ToonVfxLook::Streak,
/// 		stretch: 0.04,
/// 		color: [1.0, 0.7, 0.3],
/// 		is_grounded: true,
/// 		age: 0.1,
/// 		key: Some(12),
/// 		..ToonParticleOptions::default()
/// 	};
/// 	vfx.emit_particles(&spark_options);
/// 	vfx.emit_particles(&spark_options);
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonParticleOptions {
	/// 낼 수. (set_count_ratio 의 몫을 곱한다)
	pub count: u32,
	/// 낼 자리.
	pub position: [f32; 3],
	/// 내는 꼴.
	pub shape: ToonVfxShape,
	/// 꼴의 반지름. (Point 는 보지 않음)
	pub radius: f32,
	/// 원뿔과 선의 쪽.
	pub direction: [f32; 3],
	/// 원뿔의 벌어짐. (라디안)
	pub spread: f32,
	/// 선의 길이.
	pub length: f32,
	/// 아래로 가는 쪽을 위로 뒤집을지.
	pub is_upward: bool,
	/// 속도를 무시하고 나이가 다할 때 낸 자리로 빨려 들지.
	pub is_inward: bool,
	/// 처음 빠르기. (내는 쪽으로)
	pub speed: ToonVfxRange,
	/// 위로 더하는 처음 빠르기.
	pub rise: ToonVfxRange,
	/// 아래로 당기는 가속.
	pub gravity: f32,
	/// 끌림. (빠르기가 exp(-drag x 나이) 로 준다)
	pub drag: f32,
	/// 처음 크기.
	pub size: ToonVfxRange,
	/// 끝 크기. (시작 크기에 곱함)
	pub size_end: ToonVfxRange,
	/// 크기가 처음에서 끝으로 가는 굽기. (나아감의 거듭제곱, 1 이면 곧게)
	pub size_curve: f32,
	/// 사는 초.
	pub life: ToonVfxRange,
	/// 늦게 나타나는 초.
	pub delay: ToonVfxRange,
	/// 색. (0 ~ 1 의 [r, g, b], 램프가 있으면 램프 색에 곱함)
	pub color: [f32; 3],
	/// 램프 줄. (-1 이면 없음)
	pub ramp: i32,
	/// 파티클의 꼴.
	pub look: ToonVfxLook,
	/// Streak 꼴에서 화면에 비친 속도에 곱하는 늘임.
	pub stretch: f32,
	/// 초당 도는 각. (라디안, Streak 꼴은 속도 쪽을 따르므로 돌지 않음)
	pub spin: ToonVfxRange,
	/// 멈출 바닥 높이. (None 이면 is_grounded 를 본다)
	pub floor: Option<f32>,
	/// 낸 자리의 바닥 높이에서 멈출지.
	pub is_grounded: bool,
	/// 밝기 배. (꼴마다의 set_look_gain 배도 곱함)
	pub brightness: f32,
	/// 짙기.
	pub alpha: f32,
	/// 나타나는 몫. (나이의 이만큼 동안 엷음에서 짙어짐)
	pub fade_in: f32,
	/// 빛의 꼴의 테. (0 이면 더하는 빛만, 1 이면 테 색으로 덮음, 음수면 엷게 덮는 빛무리)
	pub rim: f32,
	/// 이미 지난 초. (음수면 그만큼 늦게)
	pub age: f32,
	/// 사건 열쇠. (이미 낸 열쇠면 내지 않음)
	pub key: Option<u64>,
}

impl Default for ToonParticleOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonParticleOptions {
			count: 1,
			position: [0.0; 3],
			shape: ToonVfxShape::Point,
			radius: 0.0,
			direction: [0.0, 1.0, 0.0],
			spread: std::f32::consts::PI,
			length: 1.0,
			is_upward: false,
			is_inward: false,
			speed: ToonVfxRange::from(1.0),
			rise: ToonVfxRange::from(0.0),
			gravity: 0.0,
			drag: 0.0,
			size: ToonVfxRange::from(0.2),
			size_end: ToonVfxRange::from(1.0),
			size_curve: 1.0,
			life: ToonVfxRange::from(1.0),
			delay: ToonVfxRange::from(0.0),
			color: [1.0; 3],
			ramp: -1,
			look: ToonVfxLook::Soft,
			stretch: 0.0,
			spin: ToonVfxRange::from(0.0),
			floor: None,
			is_grounded: false,
			brightness: 1.0,
			alpha: 1.0,
			fade_in: 0.0,
			rim: 0.0,
			age: 0.0,
			key: None,
		}
	}
}

//==============================================================================
/// 불덩이의 값.
///
/// # 예제
/// 부풀어 식으며 떠오르는 불덩이
/// ```rust,no_run
/// use vanilla::{ToonFireballOptions, ToonVfx};
///
/// fn add_barrel_explosion(vfx: &mut ToonVfx) {
/// 	vfx.set_ramps(&[vec!["#ffffff", "#ffe080", "#ff8020", "#801808", "#200804"]]);
/// 	vfx.add_fireball(&ToonFireballOptions { position: [0.0, 1.0, 0.0], radius: 1.2, life: 1.5, ramp: 0, rise: 1.0, grow_seconds: 0.25, boil: 0.4, smoke_color: [0.2, 0.18, 0.16], key: Some(3), ..ToonFireballOptions::default() });
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonFireballOptions {
	/// 가운데.
	pub position: [f32; 3],
	/// 다 부푼 반지름. (끝까지 1.25 배로 더 커진다)
	pub radius: f32,
	/// 사는 초.
	pub life: f32,
	/// 불의 램프 줄.
	pub ramp: i32,
	/// 식어 덮는 연기 색.
	pub smoke_color: [f32; 3],
	/// 사는 동안 떠오르는 높이.
	pub rise: f32,
	/// 부푸는 초. (반지름의 0.25 배에서 1 배로)
	pub grow_seconds: f32,
	/// 겉이 끓는 정도.
	pub boil: f32,
	/// 덮는 꼴의 몫. (0 이면 더하는 빛만, 1 이면 몸을 덮고 가장자리를 테 색으로)
	pub rim: f32,
	/// 이미 지난 초. (음수면 그만큼 늦게)
	pub age: f32,
	/// 사건 열쇠. (이미 낸 열쇠면 내지 않음)
	pub key: Option<u64>,
}

impl Default for ToonFireballOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonFireballOptions { position: [0.0; 3], radius: 0.5, life: 1.0, ramp: 0, smoke_color: [0.16, 0.14, 0.13], rise: 0.6, grow_seconds: 0.3, boil: 0.32, rim: 0.0, age: 0.0, key: None }
	}
}

//==============================================================================
/// 충격파 띠의 값.
///
/// # 예제
/// 조금 늦게 퍼지며 말려 오르는 충격파 띠
/// ```rust,no_run
/// use vanilla::{ToonShockwaveOptions, ToonVfx};
///
/// fn add_shockwave(vfx: &mut ToonVfx) {
/// 	vfx.add_shockwave(&ToonShockwaveOptions { x: 0.0, z: 0.0, start_radius: 0.2, end_radius: 4.0, width: 0.8, life: 0.7, ramp: 0, color: [1.0, 0.9, 0.7], lift: 0.4, delay: 0.05, rim: 0.5, ..ToonShockwaveOptions::default() });
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonShockwaveOptions {
	/// 가운데 자리. (높이는 바닥 높이)
	pub x: f32,
	/// 가운데 자리의 z.
	pub z: f32,
	/// 퍼지는 처음과 끝 반지름. (빠르게 퍼지다 천천히)
	pub start_radius: f32,
	/// 퍼지는 끝 반지름.
	pub end_radius: f32,
	/// 띠 너비. (나이만큼 0.55 배까지 가늘어지고 반지름을 넘지 않음)
	pub width: f32,
	/// 사는 초. (delay 뒤부터)
	pub life: f32,
	/// 램프 줄.
	pub ramp: i32,
	/// 램프 색에 곱하는 색.
	pub color: [f32; 3],
	/// 바깥 끝이 말려 오르는 높이.
	pub lift: f32,
	/// 바닥 위로 띄우는 높이.
	pub height: f32,
	/// 늦게 퍼지기 시작하는 초.
	pub delay: f32,
	/// 덮는 꼴의 몫. (0 이면 더하는 빛만, 1 이면 띠의 몸을 테 색으로 덮고 심은 색)
	pub rim: f32,
	/// 이미 지난 초. (음수면 그만큼 늦게)
	pub age: f32,
	/// 사건 열쇠. (이미 낸 열쇠면 내지 않음)
	pub key: Option<u64>,
}

impl Default for ToonShockwaveOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonShockwaveOptions { x: 0.0, z: 0.0, start_radius: 0.3, end_radius: 2.0, width: 0.6, life: 1.0, ramp: 0, color: [1.0; 3], lift: 0.22, height: 0.03, delay: 0.0, rim: 0.0, age: 0.0, key: None }
	}
}

//==============================================================================
/// 바닥 자국의 값. (queue_decal 은 나이 대신 heat 를 쓴다)
///
/// # 예제
/// 퍼지며 식는 그을음과 깜빡이는 얼룩
/// ```rust,no_run
/// use vanilla::{ToonDecalOptions, ToonVfx, ToonVfxDecalMode, ToonVfxSlot};
///
/// fn add_decals(vfx: &mut ToonVfx) {
/// 	vfx.add_decal(&ToonDecalOptions { x: 0.0, z: 0.0, radius: 1.5, start_radius: Some(0.4), grow_seconds: 0.3, life: 10.0, heat_seconds: 2.0, slot: ToonVfxSlot::Scorch, mode: ToonVfxDecalMode::Scorch, ramp: 0, ..ToonDecalOptions::default() });
/// 	vfx.add_decal(&ToonDecalOptions { x: 3.0, z: 0.0, radius: 0.8, life: 6.0, mode: ToonVfxDecalMode::Stain, tint: [0.2, 0.45, 0.1], alpha: 0.7, flicker: 0.25, ..ToonDecalOptions::default() });
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonDecalOptions {
	/// 가운데 자리. (높이는 바닥 높이)
	pub x: f32,
	/// 가운데 자리의 z.
	pub z: f32,
	/// 반지름. (start_radius 에서 grow_seconds 동안 퍼져 닿는 반지름)
	pub radius: f32,
	/// 퍼지기 시작하는 반지름. (None 이면 radius)
	pub start_radius: Option<f32>,
	/// 퍼지는 초. (0 이면 처음부터 radius)
	pub grow_seconds: f32,
	/// 사는 초. (끝 1 초에 엷어짐)
	pub life: f32,
	/// 열기가 식는 초. (0 이면 열기 없음)
	pub heat_seconds: f32,
	/// 그릴 아틀라스 칸.
	pub slot: ToonVfxSlot,
	/// 자국의 꼴.
	pub mode: ToonVfxDecalMode,
	/// 열기와 빛, 테의 램프 줄. (Scorch, Glow, Ring 꼴이 읽는다)
	pub ramp: i32,
	/// 색. (빛과 테는 램프 색에 곱하고, Stain, Sheen 은 이 색으로 덮는다)
	pub tint: [f32; 3],
	/// 짙기.
	pub alpha: f32,
	/// 돎. (None 이면 add_decal 은 무작위, queue_decal 은 0)
	pub angle: Option<f32>,
	/// Ring 꼴의 (테 반지름, 테 너비, 잡음 잘기, 흐르는 빠르기). (반지름 몫)
	pub ring: [f32; 4],
	/// queue_decal 의 열기. (0 ~ 1)
	pub heat: f32,
	/// 잡음 씨앗. (None 이면 add_decal 은 무작위, queue_decal 은 0)
	pub seed: Option<f32>,
	/// 깜빡이는 반주기 초. (0 이면 없음, 반주기마다 걸러 엷게)
	pub flicker: f32,
	/// 이미 지난 초. (add_decal 만 읽음, 음수면 그만큼 늦게)
	pub age: f32,
	/// 사건 열쇠. (add_decal 만 읽음, 이미 낸 열쇠면 내지 않음)
	pub key: Option<u64>,
}

impl Default for ToonDecalOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonDecalOptions { x: 0.0, z: 0.0, radius: 1.0, start_radius: None, grow_seconds: 0.0, life: 1.0, heat_seconds: 0.0, slot: ToonVfxSlot::Scorch, mode: ToonVfxDecalMode::Scorch, ramp: 0, tint: [1.0; 3], alpha: 0.85, angle: None, ring: [0.9, 0.05, 3.0, 0.3], heat: 0.0, seed: None, flicker: 0.0, age: 0.0, key: None }
	}
}

//==============================================================================
/// 발밑 그늘의 값. (가운데 alpha 에서 바깥으로 둥글게 엷어지는 그늘)
///
/// # 예제
/// 캐릭터마다 발밑 그늘을 매 프레임 넘기기
/// ```rust,no_run
/// use vanilla::{Matrix4, ToonBlobShadowOptions, ToonVfx, Vector3};
///
/// fn draw_blob_shadows(gl: &glow::Context, vfx: &mut ToonVfx) {
/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 4.0, 6.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// 	let projection_matrix = Matrix4::create_perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
/// 	let character_positions = [[0.0, 0.0], [1.5, -0.5], [-1.0, 1.0]];
/// 	for position in character_positions {
/// 		vfx.queue_blob_shadow(&ToonBlobShadowOptions { x: position[0], z: position[1], radius: 0.5, ..ToonBlobShadowOptions::default() });
/// 	}
/// 	vfx.draw(gl, &view_matrix, &projection_matrix);
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonBlobShadowOptions {
	/// 가운데 자리. (높이는 바닥 높이)
	pub x: f32,
	/// 가운데 자리의 z.
	pub z: f32,
	/// 반지름.
	pub radius: f32,
	/// 가운데 짙기.
	pub alpha: f32,
}

impl Default for ToonBlobShadowOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonBlobShadowOptions { x: 0.0, z: 0.0, radius: 0.5, alpha: DEFAULT_BLOB_SHADOW_ALPHA }
	}
}

//==============================================================================
/// 연기 기둥의 값.
///
/// # 예제
/// 큰 폭발 뒤에 솟는 연기 기둥
/// ```rust,no_run
/// use vanilla::{ToonPlumeOptions, ToonVfx};
///
/// fn add_blast_plume(vfx: &mut ToonVfx) {
/// 	vfx.add_plume(&ToonPlumeOptions { x: 0.0, z: 0.0, height: 4.0, cap_radius: Some(1.6), stem_radius: Some(0.35), life: 5.0, ramp: 0, smoke_color: [0.3, 0.28, 0.26], key: Some(9), ..ToonPlumeOptions::default() });
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonPlumeOptions {
	/// 밑 자리. (높이는 바닥 높이)
	pub x: f32,
	/// 밑 자리의 z.
	pub z: f32,
	/// 기둥 높이.
	pub height: f32,
	/// 갓 반지름. (None 이면 높이의 0.4)
	pub cap_radius: Option<f32>,
	/// 기둥 반지름. (None 이면 높이의 0.1)
	pub stem_radius: Option<f32>,
	/// 사는 초.
	pub life: f32,
	/// 갓 밑이 달아오르는 램프 줄.
	pub ramp: i32,
	/// 연기 색.
	pub smoke_color: [f32; 3],
	/// 이미 지난 초. (음수면 그만큼 늦게)
	pub age: f32,
	/// 사건 열쇠. (이미 낸 열쇠면 내지 않음)
	pub key: Option<u64>,
}

impl Default for ToonPlumeOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonPlumeOptions { x: 0.0, z: 0.0, height: 2.0, cap_radius: None, stem_radius: None, life: 3.0, ramp: 0, smoke_color: [0.34, 0.31, 0.29], age: 0.0, key: None }
	}
}

//==============================================================================
/// 회전체 빔의 값. (결은 start 에서 end 쪽으로 흐름)
///
/// # 예제
/// 흰 심과 램프 겉집 두 겹의 빔
/// ```rust,no_run
/// use vanilla::{Matrix4, ToonBeamOptions, ToonVfx, Vector3};
///
/// fn draw_beam(gl: &glow::Context, vfx: &mut ToonVfx) {
/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(4.0, 3.0, 4.0), Vector3::create(0.0, 1.0, -4.0), Vector3::create(0.0, 1.0, 0.0));
/// 	let projection_matrix = Matrix4::create_perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
/// 	let start = [0.0, 1.0, 0.0];
/// 	let end = [0.0, 1.0, -8.0];
/// 	vfx.queue_beam(&ToonBeamOptions { start, end, radius: 0.12, is_core: true, alpha: 1.0, brightness: 1.5, ..ToonBeamOptions::default() });
/// 	vfx.queue_beam(&ToonBeamOptions { start, end, radius: 0.3, alpha: 0.8, ramp: 0, wobble: 0.15, flow_speed: 6.0, grain_repeat: 0.6, seed: 2.0, ..ToonBeamOptions::default() });
/// 	vfx.draw(gl, &view_matrix, &projection_matrix);
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonBeamOptions {
	/// 축의 시작과 끝.
	pub start: [f32; 3],
	/// 축의 끝.
	pub end: [f32; 3],
	/// 반지름.
	pub radius: f32,
	/// 심이면 흰빛, 아니면 램프의 겉집.
	pub is_core: bool,
	/// 짙기.
	pub alpha: f32,
	/// 밝기 배.
	pub brightness: f32,
	/// 겉집의 램프 줄.
	pub ramp: i32,
	/// 겉이 일렁이는 몫. (반지름에 곱함)
	pub wobble: f32,
	/// 결이 흐르는 빠르기.
	pub flow_speed: f32,
	/// 길이 한 칸마다의 결.
	pub grain_repeat: f32,
	/// 결과 일렁임의 시각. (None 이면 이 바탕의 시계)
	pub time: Option<f32>,
	/// 잡음 씨앗.
	pub seed: f32,
}

impl Default for ToonBeamOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonBeamOptions { start: [0.0; 3], end: [0.0, 1.0, 0.0], radius: 0.1, is_core: false, alpha: 1.0, brightness: 1.0, ramp: 0, wobble: 0.0, flow_speed: 4.0, grain_repeat: 0.5, time: None, seed: 0.0 }
	}
}

//==============================================================================
/// 리본의 값. (점열을 카메라 쪽으로 편 띠)
///
/// # 예제
/// 휘두른 자취를 번짐 리본과 더하는 빛 리본으로
/// ```rust,no_run
/// use vanilla::{ToonRibbonOptions, ToonVfx};
///
/// fn queue_swing_trail(vfx: &mut ToonVfx) {
/// 	let trail_points = vec![[-1.0, 1.0, 0.0], [-0.5, 1.3, -0.4], [0.0, 1.4, -0.6], [0.5, 1.3, -0.4], [1.0, 1.0, 0.0]];
/// 	vfx.queue_ribbon(ToonRibbonOptions { points: trail_points.clone(), width: 0.25, taper: 1.0, alpha: 1.0, brightness: 1.2, glow: Some([0.4, 0.8, 1.0]), ..ToonRibbonOptions::default() });
/// 	let flow = vfx.get_time() * 4.0;
/// 	vfx.queue_ribbon(ToonRibbonOptions { points: trail_points, width: 0.1, color: [1.0, 0.9, 0.6], alpha: 0.8, grain_repeat: 2.0, flow: Some(flow), ..ToonRibbonOptions::default() });
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct ToonRibbonOptions {
	/// 점열. (둘 이상)
	pub points: Vec<[f32; 3]>,
	/// 너비.
	pub width: f32,
	/// 0 이면 고른 너비, 1 이면 두 끝이 0.
	pub taper: f32,
	/// 색. (0 ~ 1 의 [r, g, b], 램프가 있으면 램프 색에 곱함)
	pub color: [f32; 3],
	/// 램프 줄. (-1 이면 color 만)
	pub ramp: i32,
	/// 짙기.
	pub alpha: f32,
	/// 밝기 배.
	pub brightness: f32,
	/// 길이 한 칸마다의 결.
	pub grain_repeat: f32,
	/// 결의 자리. (None 이면 이 바탕의 시계)
	pub flow: Option<f32>,
	/// 번짐 색. (주면 흰 심과 그 색 둘레와 테 색 가장자리로 덮는다, None 이면 더하는 빛)
	pub glow: Option<[f32; 3]>,
	/// 덮는 꼴의 몫. (번짐 색이 없을 때 몸을 테 색으로 덮고 심은 색, 0 이면 더하는 빛만)
	pub rim: f32,
}

impl Default for ToonRibbonOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonRibbonOptions { points: Vec::new(), width: 0.1, taper: 0.0, color: [1.0; 3], ramp: -1, alpha: 1.0, brightness: 1.0, grain_repeat: 1.0, flow: None, glow: None, rim: 0.0 }
	}
}

//==============================================================================
/// 번개의 값. (축을 감아 내려가는 리본 여럿)
///
/// # 예제
/// 이번 프레임에 내리치는 번개 셋과 0.4 초 동안 다시 꺾이며 사는 번개
/// ```rust,no_run
/// use vanilla::{ToonLightningOptions, ToonVfx};
///
/// fn strike(vfx: &mut ToonVfx) {
/// 	vfx.queue_lightning(&ToonLightningOptions { start: [0.0, 4.0, 0.0], end: [0.0, 0.0, 0.0], radius: 0.15, count: 3, width: 0.06, turns: 1.5, color: [0.8, 0.9, 1.0], alpha: 1.0, brightness: 1.5, seed: Some(7.0), ..ToonLightningOptions::default() });
/// 	vfx.add_lightning(&ToonLightningOptions { start: [0.0, 4.0, 0.0], end: [0.5, 0.0, 0.2], life: 0.4, count: 2, width: 0.08, radius: 0.2, glow: Some([0.5, 0.7, 1.0]), key: Some(5), ..ToonLightningOptions::default() });
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonLightningOptions {
	/// 축의 시작과 끝.
	pub start: [f32; 3],
	/// 축의 끝.
	pub end: [f32; 3],
	/// 감는 반지름.
	pub radius: f32,
	/// 감아 내리는 리본 수.
	pub count: u32,
	/// 리본 너비. (리본마다 0.7 ~ 1.3 배)
	pub width: f32,
	/// 감는 바퀴.
	pub turns: f32,
	/// 리본 색. (0 ~ 1 의 [r, g, b], 램프가 있으면 램프 색에 곱함)
	pub color: [f32; 3],
	/// 리본의 램프 줄. (-1 이면 color 만)
	pub ramp: i32,
	/// 리본의 짙기.
	pub alpha: f32,
	/// 리본의 밝기 배.
	pub brightness: f32,
	/// 꺾임의 시각. (None 이면 이 바탕의 시계, add_lightning 은 늘 이 바탕의 시계)
	pub time: Option<f32>,
	/// 씨앗. (None 이면 queue_lightning 은 0, add_lightning 은 무작위)
	pub seed: Option<f32>,
	/// 리본의 번짐 색.
	pub glow: Option<[f32; 3]>,
	/// 사는 초와 나이. (add_lightning 만 읽는다, 나이가 음수면 그만큼 늦게 나타남)
	pub life: f32,
	/// 나이. (add_lightning 만 읽는다, 음수면 그만큼 늦게 나타남)
	pub age: f32,
	/// 리본의 덮는 꼴의 몫.
	pub rim: f32,
	/// 사건 열쇠. (add_lightning 만 읽는다, 이미 낸 열쇠면 내지 않음)
	pub key: Option<u64>,
}

impl Default for ToonLightningOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonLightningOptions { start: [0.0; 3], end: [0.0, 1.0, 0.0], radius: 0.05, count: 1, width: 0.05, turns: 1.0, color: [1.0; 3], ramp: -1, alpha: 1.0, brightness: 1.0, time: None, seed: None, glow: None, life: 0.5, age: 0.0, rim: 0.0, key: None }
	}
}

//==============================================================================
/// 빛 점의 값. (파티클과 같은 꼴, 나이 없이 그 자리에)
///
/// # 예제
/// 보석 위에 반짝이는 빛살을 매 프레임
/// ```rust,no_run
/// use vanilla::{ToonGlintOptions, ToonVfx, ToonVfxLook};
///
/// fn queue_gem_glint(vfx: &mut ToonVfx) {
/// 	vfx.set_glint_capacity(32);
/// 	let twinkle = 0.5 + 0.5 * (vfx.get_time() * 6.0).sin();
/// 	vfx.queue_glint(&ToonGlintOptions { position: [0.5, 1.2, 0.0], size: 0.4 * twinkle, look: ToonVfxLook::Star, color: [1.0, 0.95, 0.7], angle: 0.3, brightness: 1.5, ..ToonGlintOptions::default() });
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonGlintOptions {
	/// 그 자리.
	pub position: [f32; 3],
	/// 크기.
	pub size: f32,
	/// 색. (0 ~ 1 의 [r, g, b], 램프가 있으면 램프 색에 곱함)
	pub color: [f32; 3],
	/// 램프 줄. (-1 이면 없음)
	pub ramp: i32,
	/// 꼴.
	pub look: ToonVfxLook,
	/// 밝기 배. (꼴마다의 set_look_gain 배도 곱함)
	pub brightness: f32,
	/// 짙기.
	pub alpha: f32,
	/// 돎. (라디안)
	pub angle: f32,
	/// 파티클의 rim 과 같다.
	pub rim: f32,
}

impl Default for ToonGlintOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonGlintOptions { position: [0.0; 3], size: 0.2, color: [1.0; 3], ramp: -1, look: ToonVfxLook::Soft, brightness: 1.0, alpha: 1.0, angle: 0.0, rim: 0.0 }
	}
}

//==============================================================================
/// 화면 섬광의 값. (뷰포트를 그 색으로 잠깐 덮고 빠르게 걷힘)
///
/// # 예제
/// 터질 때 화면을 잠깐 밝게 덮기
/// ```rust,no_run
/// use vanilla::{ToonFlashOptions, ToonVfx};
///
/// fn flash_on_blast(vfx: &mut ToonVfx) {
/// 	vfx.add_flash(&ToonFlashOptions { color: [1.0, 0.9, 0.7], strength: 0.6, life: 0.12, key: Some(9), ..ToonFlashOptions::default() });
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonFlashOptions {
	/// 덮는 색.
	pub color: [f32; 3],
	/// 처음 덮는 몫.
	pub strength: f32,
	/// 걷히는 초.
	pub life: f32,
	/// 나이. (음수면 그만큼 늦게 나타남)
	pub age: f32,
	/// 사건 열쇠. (이미 낸 열쇠면 내지 않음)
	pub key: Option<u64>,
}

impl Default for ToonFlashOptions {
	//==============================================================================
	/// 기본값.
	//==============================================================================
	fn default() -> Self {
		ToonFlashOptions { color: [1.0; 3], strength: 0.8, life: 0.08, age: 0.0, key: None }
	}
}

//==============================================================================
/// 램프 한 칸의 색. ("#rrggbb" 글이나 0 ~ 1 의 [r, g, b], 원본 setRamps 가 둘 다 받는다)
///
/// # 예제
/// 색 글자와 0 ~ 1 의 수를 같은 색으로 읽기
/// ```rust
/// use vanilla::ToonRampColor;
///
/// let from_text = "#ff8000".read_ramp_color();
/// let values: [f32; 3] = [1.0, 0.5, 0.0];
/// let from_values = values.read_ramp_color();
/// assert_eq!(from_text[0], 1.0);
/// assert!((from_text[1] - from_values[1]).abs() < 0.01);
/// assert_eq!(from_text[2], from_values[2]);
/// ```
//==============================================================================
pub trait ToonRampColor {
	//==============================================================================
	/// 0 ~ 1 의 [r, g, b] 읽기.
	///
	/// # 돌려줌
	/// 0 ~ 1 의 [r, g, b].
	//==============================================================================
	fn read_ramp_color(&self) -> [f32; 3];
}

impl ToonRampColor for [f32; 3] {
	//==============================================================================
	/// 그대로.
	//==============================================================================
	fn read_ramp_color(&self) -> [f32; 3] {
		*self
	}
}

impl ToonRampColor for &str {
	//==============================================================================
	/// 색 글자 읽기. ("#rrggbb")
	//==============================================================================
	fn read_ramp_color(&self) -> [f32; 3] {
		read_color_values(self)
	}
}

impl ToonRampColor for String {
	//==============================================================================
	/// 색 글자 읽기. ("#rrggbb")
	//==============================================================================
	fn read_ramp_color(&self) -> [f32; 3] {
		read_color_values(self)
	}
}

//==============================================================================
/// 떠 있는 효과의 수.
///
/// # 예제
/// 효과가 다 사라졌는지 보기
/// ```rust,no_run
/// use vanilla::{ToonFireballOptions, ToonParticleOptions, ToonVfx, ToonVfxRange};
///
/// fn is_quiet_after_one_second(vfx: &mut ToonVfx) -> bool {
/// 	vfx.add_fireball(&ToonFireballOptions { position: [0.0, 1.0, 0.0], radius: 1.0, life: 0.5, ..ToonFireballOptions::default() });
/// 	vfx.emit_particles(&ToonParticleOptions { count: 10, position: [0.0, 1.0, 0.0], life: ToonVfxRange::from(0.3), ..ToonParticleOptions::default() });
/// 	vfx.update(1.0);
/// 	let counts = vfx.get_active_counts();
/// 	counts.particle_count == 0 && counts.fireball_count == 0
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToonVfxCounts {
	/// 아직 살아 있을 수 있는 파티클 칸.
	pub particle_count: usize,
	/// 불덩이. (add_fireball)
	pub fireball_count: usize,
	/// 충격파 띠. (add_shockwave)
	pub shockwave_count: usize,
	/// 바닥 자국. (add_decal, queue_decal 로 넘긴 것은 세지 않음)
	pub decal_count: usize,
	/// 연기 기둥. (add_plume)
	pub plume_count: usize,
	/// 사는 동안 다시 꺾이는 번개. (add_lightning)
	pub lightning_count: usize,
	/// 화면 섬광. (add_flash)
	pub flash_count: usize,
}

//==============================================================================
/// 떠 있는 효과 하나. (불덩이, 띠, 자국, 기둥이 함께 쓴다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct LiveFireball {
	options: ToonFireballOptions,
	/// 잡음 씨앗.
	seed: f32,
	/// 나이. (음수면 아직 나타나지 않음)
	age: f32,
}

//==============================================================================
/// 떠 있는 충격파 띠 하나. (바닥 높이는 낼 때 읽어 둔다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct LiveShockwave {
	options: ToonShockwaveOptions,
	/// 낼 때 읽은 바닥 높이.
	floor_height: f32,
	seed: f32,
	age: f32,
}

//==============================================================================
/// 떠 있는 바닥 자국 하나. (queue_decal 로 넘긴 것도 같은 꼴)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct LiveDecal {
	options: ToonDecalOptions,
	/// 낼 때 읽은 바닥 높이.
	floor_height: f32,
	/// 정한 돎. (options.angle 이 None 이면 무작위나 0)
	angle: f32,
	seed: f32,
	age: f32,
}

//==============================================================================
/// 떠 있는 연기 기둥 하나. (갓과 기둥 반지름은 낼 때 정한다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct LivePlume {
	options: ToonPlumeOptions,
	/// 낼 때 읽은 바닥 높이.
	floor_height: f32,
	cap_radius: f32,
	stem_radius: f32,
	seed: f32,
	age: f32,
}

//==============================================================================
/// 번호로 그리는 메쉬.
//==============================================================================
struct IndexedMesh {
	vertex_array: GlVertexArray,
	index_count: i32,
}

//==============================================================================
/// 그리는 프레임의 카메라.
//==============================================================================
struct VfxFrame {
	view_projection_matrix: Matrix4,
	/// 장면에서 카메라 쪽.
	view_direction: [f32; 3],
	camera_right: [f32; 3],
	camera_up: [f32; 3],
	/// 원근이면 [자리, 1], 직교면 [카메라 쪽, 0].
	camera_point: [f32; 4],
}

//==============================================================================
/// 툰 3D GPU 효과의 바탕. (머리말 참고)
///
/// # 예제
/// 툰 장면 위에 폭발 효과를 내고 한 프레임 그리기
/// ```rust,no_run
/// use vanilla::{create_box_geometry, Matrix4, MeshBuilder, ToonDecalOptions, ToonDrawOptions, ToonFireballOptions, ToonParticleOptions, ToonPartOptions, ToonRenderer, ToonShockwaveOptions, ToonVfx, ToonVfxDecalMode, ToonVfxLook, ToonVfxRange, ToonVfxShape, Vector3, TOON_VFX_DEFAULT_PARTICLE_CAPACITY};
///
/// fn draw_explosion(gl: &glow::Context) -> Result<(), String> {
/// 	let mut renderer = ToonRenderer::new(gl)?;
/// 	let mut builder = MeshBuilder::new();
/// 	builder.add_part(&create_box_geometry(8.0, 0.2, 8.0), &Matrix4::create_translation(0.0, -0.1, 0.0), "#708090", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
/// 	let floor_mesh = renderer.create_mesh(gl, &builder.read_arrays()).ok_or("floor mesh")?;
/// 	let mut vfx = ToonVfx::new(gl, TOON_VFX_DEFAULT_PARTICLE_CAPACITY)?;
/// 	vfx.set_ramps(&[vec!["#ffffff", "#ffe080", "#ff8020", "#a02010", "#301008"]]);
/// 	vfx.prepare(gl);
/// 	vfx.add_fireball(&ToonFireballOptions { position: [0.0, 0.8, 0.0], radius: 0.9, life: 1.2, ramp: 0, ..ToonFireballOptions::default() });
/// 	vfx.add_shockwave(&ToonShockwaveOptions { end_radius: 3.0, life: 0.6, ramp: 0, ..ToonShockwaveOptions::default() });
/// 	vfx.add_decal(&ToonDecalOptions { radius: 1.4, life: 8.0, heat_seconds: 1.5, mode: ToonVfxDecalMode::Scorch, ramp: 0, ..ToonDecalOptions::default() });
/// 	vfx.emit_particles(&ToonParticleOptions { count: 40, position: [0.0, 0.6, 0.0], shape: ToonVfxShape::Sphere, radius: 0.3, speed: ToonVfxRange::new(2.0, 5.0), gravity: 6.0, size: ToonVfxRange::new(0.05, 0.12), life: ToonVfxRange::new(0.4, 0.9), look: ToonVfxLook::Streak, stretch: 0.05, ramp: 0, is_grounded: true, ..ToonParticleOptions::default() });
/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(4.0, 3.0, 5.0), Vector3::create(0.0, 0.5, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// 	let projection_matrix = Matrix4::create_perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
/// 	vfx.update(1.0 / 60.0);
/// 	renderer.begin_frame_with_matrices(gl, &view_matrix, &projection_matrix);
/// 	renderer.draw_mesh(gl, &floor_mesh, &Matrix4::create_identity(), ToonDrawOptions::default());
/// 	renderer.end_frame(gl);
/// 	vfx.draw(gl, &view_matrix, &projection_matrix);
/// 	Ok(())
/// }
/// ```
//==============================================================================
pub struct ToonVfx {
	/// 효과 아틀라스. (처음 그릴 때 지음)
	atlas_texture: Option<GlTexture>,
	/// 아틀라스 픽셀. (램프를 바꿀 때 다시 칠함)
	atlas_pixels: Option<Vec<u8>>,
	/// 색 램프 줄들. (줄마다 흰 심에서 어둠으로 가는 색)
	ramp_list: Vec<Vec<[f32; 3]>>,
	/// 램프 칸을 다시 올릴지.
	is_ramp_dirty: bool,
	particle_program: ShaderProgram,
	fireball_program: ShaderProgram,
	band_program: ShaderProgram,
	decal_program: ShaderProgram,
	beam_program: ShaderProgram,
	plume_program: ShaderProgram,
	ribbon_program: ShaderProgram,
	flash_program: ShaderProgram,
	/// 효과 테의 색.
	rim_color: [f32; 3],
	/// 지울 버퍼.
	buffer_list: Vec<GlBuffer>,
	particle_vertex_array: GlVertexArray,
	particle_buffer: GlBuffer,
	/// 파티클 값. (PARTICLE_FLOAT_COUNT 씩, 고리처럼 돌려 씀)
	particle_data: Vec<f32>,
	particle_capacity: usize,
	/// 다음에 쓸 칸.
	particle_write_index: usize,
	/// 한 번이라도 쓴 칸의 수. (그리는 인스턴스 수)
	particle_used_count: usize,
	/// 버퍼에 올릴 것이 있는지.
	is_particle_dirty: bool,
	/// 낸 파티클이 다 사라지는 시각.
	particle_live_until: f32,
	/// 내는 파티클 수에 곱하는 몫.
	count_ratio: f32,
	/// 불덩이 구.
	sphere_mesh: IndexedMesh,
	/// 충격파 띠.
	band_mesh: IndexedMesh,
	/// 회전체 격자. (빔과 연기 기둥)
	lathe_mesh: IndexedMesh,
	/// 바닥 자국과 섬광의 네 귀퉁이.
	decal_vertex_array: GlVertexArray,
	ribbon_vertex_array: GlVertexArray,
	ribbon_buffer: GlBuffer,
	/// 리본 정점을 쌓는 자리. (RIBBON_FLOAT_COUNT 씩, 프레임마다 다시 씀)
	ribbon_data: Vec<f32>,
	/// 네 귀퉁이와 빛 점 인스턴스 버퍼. (파티클과 같은 꼴)
	glint_vertex_array: GlVertexArray,
	glint_buffer: GlBuffer,
	glint_data: Vec<f32>,
	/// 프레임마다 넘기는 빛 점의 상한.
	glint_capacity: usize,
	/// 파티클 꼴마다의 밝기 배.
	look_gain_list: [f32; LOOK_COUNT],
	/// 이미 낸 사건 열쇠.
	used_event_key_set: HashSet<u64>,
	fireball_list: Vec<LiveFireball>,
	shockwave_list: Vec<LiveShockwave>,
	decal_list: Vec<LiveDecal>,
	plume_list: Vec<LivePlume>,
	/// 사는 동안 다시 꺾이는 번개. (씨앗을 채운 값, 나이는 age)
	lightning_list: Vec<ToonLightningOptions>,
	/// 화면 섬광. (나이는 age)
	flash_list: Vec<ToonFlashOptions>,
	/// 카메라 흔들림. (장면 길이에 SHAKE_UNIT_SCALE 을 곱해 넣음)
	shaker: Shaker,
	/// 이번 프레임에만 그리는 바닥 자국, 빔, 리본. (draw 가 그리고 비운다)
	queued_decal_list: Vec<LiveDecal>,
	queued_beam_list: Vec<ToonBeamOptions>,
	queued_ribbon_list: Vec<ToonRibbonOptions>,
	/// 이번 프레임에 glint_data 에 쓴 빛 점 수.
	queued_glint_count: usize,
	/// 이 바탕의 시계. (초)
	time: f32,
	seeded_random: SeededRandom,
	/// (x, z) → 바닥 높이. (None 이면 0)
	floor_height_reader: Option<Box<dyn Fn(f32, f32) -> f32>>,
}

//==============================================================================
/// 배열을 바이트로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn float_slice_to_bytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

//==============================================================================
/// u16 배열을 바이트로 본다. (GL 번호 버퍼 업로드용)
//==============================================================================
fn index_slice_to_bytes(values: &[u16]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

//==============================================================================
/// 셰이더의 공통 조각 넣기. ("//#include noise", "//#include atlas" 자리)
//==============================================================================
fn expand_shader(source: &str) -> String {
	source.replace("//#include noise", NOISE_SHADER_CHUNK).replace("//#include atlas", ATLAS_SHADER_CHUNK)
}

//==============================================================================
/// 0 ~ 1 로 가두기와 부드러운 계단. (셰이더의 clamp, smoothstep)
//==============================================================================
fn clamp_unit(value: f32) -> f32 {
	value.clamp(0.0, 1.0)
}

//==============================================================================
/// 부드러운 계단. (셰이더의 smoothstep)
//==============================================================================
fn smooth_step(edge_start: f32, edge_end: f32, value: f32) -> f32 {
	let ratio = clamp_unit((value - edge_start) / (edge_end - edge_start));
	ratio * ratio * (3.0 - 2.0 * ratio)
}

//==============================================================================
/// 값 잡음의 격자. (256 x 256 의 0 ~ 1, 아틀라스를 지을 때만)
//==============================================================================
fn create_noise_grid(seed: u32) -> Vec<f32> {
	let mut seeded_random = SeededRandom::new(seed);
	(0..256 * 256).map(|_| seeded_random.next_value() as f32).collect()
}

//==============================================================================
/// 잇는 값 잡음 하나. (주기마다 이어 붙어 타일이 된다)
//==============================================================================
fn read_grid_noise(grid: &[f32], x: f32, y: f32, period_x: i64, period_y: i64) -> f32 {
	let column = x.floor();
	let row = y.floor();
	let offset_x = x - column;
	let offset_y = y - row;
	let weight_x = offset_x * offset_x * (3.0 - 2.0 * offset_x);
	let weight_y = offset_y * offset_y * (3.0 - 2.0 * offset_y);
	let left = (column as i64).rem_euclid(period_x);
	let right = (left + 1) % period_x;
	let top = (row as i64).rem_euclid(period_y);
	let bottom = (top + 1) % period_y;
	let read = |grid_row: i64, grid_column: i64| grid[((grid_row & 255) * 256 + (grid_column & 255)) as usize];
	let upper = read(top, left) + (read(top, right) - read(top, left)) * weight_x;
	let lower = read(bottom, left) + (read(bottom, right) - read(bottom, left)) * weight_x;
	upper + (lower - upper) * weight_y
}

//==============================================================================
/// 여러 겹 값 잡음. (겹마다 두 배로 잘게, 주기도 두 배, 0 ~ 1)
//==============================================================================
fn read_grid_fbm(grid: &[f32], x: f32, y: f32, period_x: i64, period_y: i64, octave_count: u32) -> f32 {
	let mut sum = 0.0;
	let mut total = 0.0;
	let mut amplitude = 0.5;
	let mut frequency = 1.0;
	for octave_index in 0..octave_count {
		let scale = 1i64 << octave_index;
		sum += amplitude * read_grid_noise(grid, x * frequency, y * frequency, period_x * scale, period_y * scale);
		total += amplitude;
		amplitude *= 0.5;
		frequency *= 2.0;
	}
	sum / total
}

//==============================================================================
/// 아틀라스 칸 하나 칠하기. (painter 가 (u, v, texel) 로 0 ~ 1 의 RGBA 를 채움, 첫 줄이 텍스처의 v 0)
//==============================================================================
fn paint_cell(pixels: &mut [u8], origin_x: usize, origin_y: usize, painter: impl Fn(f32, f32, &mut [f32; 4])) {
	for pixel_y in 0..ATLAS_SLOT_SIZE {
		let v = (pixel_y as f32 + 0.5) / ATLAS_SLOT_SIZE as f32;
		for pixel_x in 0..ATLAS_SLOT_SIZE {
			let u = (pixel_x as f32 + 0.5) / ATLAS_SLOT_SIZE as f32;
			let mut texel = [1.0, 1.0, 1.0, 0.0];
			painter(u, v, &mut texel);
			let offset = ((origin_y + pixel_y) * ATLAS_WIDTH + origin_x + pixel_x) * 4;
			for channel in 0..4 {
				pixels[offset + channel] = (clamp_unit(texel[channel]) * 255.0).round() as u8;
			}
		}
	}
}

//==============================================================================
/// 아틀라스 오른쪽 반의 칸 하나 칠하기.
//==============================================================================
fn paint_slot(pixels: &mut [u8], slot: ToonVfxSlot, painter: impl Fn(f32, f32, &mut [f32; 4])) {
	let index = slot.get_index();
	paint_cell(pixels, ATLAS_WIDTH / 2 + (index % 4) * ATLAS_SLOT_SIZE, (index / 4) * ATLAS_SLOT_SIZE, painter);
}

//==============================================================================
/// 색 램프 칸 칠하기. (줄마다 색들을 고르게 나눠 잇는다, 여덟 줄보다 적으면 마지막 줄을 되풀이)
//==============================================================================
fn paint_ramps(pixels: &mut [u8], ramp_list: &[Vec<[f32; 3]>]) {
	paint_slot(pixels, ToonVfxSlot::Ramps, |u, v, color| {
		let row_index = ((v * TOON_VFX_MAXIMUM_RAMPS as f32).floor() as usize).min(ramp_list.len() - 1);
		let stops = &ramp_list[row_index];
		if stops.len() == 1 {
			color[0] = stops[0][0];
			color[1] = stops[0][1];
			color[2] = stops[0][2];
			color[3] = 1.0;
			return;
		}
		let position = u * (stops.len() - 1) as f32;
		let stop_index = (position.floor() as usize).min(stops.len() - 2);
		let blend = position - stop_index as f32;
		for channel in 0..3 {
			color[channel] = stops[stop_index][channel] + (stops[stop_index + 1][channel] - stops[stop_index][channel]) * blend;
		}
		color[3] = 1.0;
	});
}

//==============================================================================
/// 아틀라스의 픽셀 짓기. (RGBA8, 램프 칸은 비워 둔다, 원본과 같은 그림)
//==============================================================================
fn create_atlas_pixels() -> Vec<u8> {
	let mut pixels = vec![0u8; ATLAS_WIDTH * ATLAS_HEIGHT * 4];
	let grid = create_noise_grid(4164);
	let grid = grid.as_slice();
	// 연기 플립북. (잡음 덩어리가 칸마다 말리며 퍼지고 엷어짐, 위쪽이 밝음)
	for frame in 0..SMOKE_FRAME_COUNT {
		let progress = frame as f32 / (SMOKE_FRAME_COUNT - 1) as f32;
		let swirl = progress * 1.1;
		let swirl_cosine = swirl.cos();
		let swirl_sine = swirl.sin();
		paint_cell(&mut pixels, (frame % 4) * ATLAS_SLOT_SIZE, (frame / 4) * ATLAS_SLOT_SIZE, |u, v, color| {
			let center_x = u * 2.0 - 1.0;
			let center_y = v * 2.0 - 1.0;
			let turned_x = center_x * swirl_cosine - center_y * swirl_sine;
			let turned_y = center_x * swirl_sine + center_y * swirl_cosine;
			let warp = read_grid_fbm(grid, turned_x * 1.7 + 40.0, turned_y * 1.7 + 40.0, 256, 256, 3);
			let density = read_grid_fbm(grid, turned_x * 2.2 + warp * 1.4 + 90.0, turned_y * 2.2 - progress * 1.3 + 90.0, 256, 256, 4);
			let distance = (center_x * center_x + center_y * center_y).sqrt();
			let radius = 0.55 + 0.33 * progress;
			let shape = 1.0 - smooth_step(radius * 0.45, radius, distance + (density - 0.5) * 0.55);
			let shade = 0.55 + 0.4 * v + (density - 0.5) * 0.35;
			color[0] = shade;
			color[1] = shade;
			color[2] = shade;
			color[3] = shape * clamp_unit(0.45 + density * 0.75) * (1.0 - progress * 0.3);
		});
	}
	// 부드러운 알갱이.
	paint_slot(&mut pixels, ToonVfxSlot::Soft, |u, v, color| {
		let distance = ((u * 2.0 - 1.0).powi(2) + (v * 2.0 - 1.0).powi(2)).sqrt();
		color[3] = (1.0 - distance).max(0.0).powf(2.2) + (-distance * distance * 30.0).exp() * 0.4;
	});
	// 불꽃 줄기. (v 1 쪽이 밝은 머리, 0 쪽이 엷은 꼬리)
	paint_slot(&mut pixels, ToonVfxSlot::Streak, |u, v, color| {
		let across = u * 2.0 - 1.0;
		let width = 0.16 * (0.3 + 0.7 * v);
		let body = (-(across / width) * (across / width)).exp() * v.powf(1.6) * (1.0 - smooth_step(0.92, 1.0, v));
		let head = (-(across * across * 40.0 + (v - 0.84) * (v - 0.84) * 60.0)).exp();
		color[3] = body + head * 0.8;
	});
	// 빛살. (긴 빛줄기 넷, 짧은 넷, 가운데 빛)
	paint_slot(&mut pixels, ToonVfxSlot::Star, |u, v, color| {
		let center_x = u * 2.0 - 1.0;
		let center_y = v * 2.0 - 1.0;
		let distance = (center_x * center_x + center_y * center_y).sqrt();
		let angle = center_y.atan2(center_x);
		let long_rays = (angle * 2.0).cos().abs().powf(60.0) * (1.0 - distance).max(0.0).powf(1.4);
		let short_rays = (angle * 2.0).sin().abs().powf(90.0) * (1.0 - distance * 1.5).max(0.0).powf(1.4) * 0.6;
		let glow = (-distance * distance * 16.0).exp() * 0.7 + (-distance * distance * 140.0).exp() * 0.6;
		color[3] = long_rays + short_rays + glow;
	});
	// 충격 띠. (바깥 앞머리가 밝은 테이고 잡음으로 찢긴 가장자리)
	paint_slot(&mut pixels, ToonVfxSlot::ShockBand, |u, v, color| {
		let torn = read_grid_fbm(grid, u * 12.0, v * 3.0 + 7.0, 12, 256, 4);
		let lead = (-((v - 0.8) / 0.07) * ((v - 0.8) / 0.07)).exp();
		let trail = if v < 0.8 { (v / 0.8) * (v / 0.8) * 0.5 } else { 0.0 };
		let outer_cut = 1.0 - smooth_step(0.84 + (torn - 0.5) * 0.12, 0.96, v);
		color[3] = (lead + trail) * (0.45 + 0.75 * torn) * outer_cut * smooth_step(0.0, 0.15, v);
	});
	// 빔 결. (위아래로 이어지는 잡음 결)
	paint_slot(&mut pixels, ToonVfxSlot::BeamGrain, |u, v, color| {
		let coarse = read_grid_fbm(grid, u * 6.0 + 3.0, v * 2.0 + 3.0, 6, 2, 4);
		let fine = read_grid_fbm(grid, u * 14.0 + 9.0, v * 5.0 + 9.0, 14, 5, 3);
		let value = clamp_unit(0.5 + (coarse * 0.65 + fine * 0.35 - 0.5) * 1.8);
		*color = [value; 4];
	});
	// 그을음. (알파는 어둡게 덮는 몫, 빨간 채널은 열기가 남는 금과 가운데)
	paint_slot(&mut pixels, ToonVfxSlot::Scorch, |u, v, color| {
		let center_x = u * 2.0 - 1.0;
		let center_y = v * 2.0 - 1.0;
		let distance = (center_x * center_x + center_y * center_y).sqrt();
		let edge_noise = read_grid_fbm(grid, center_x * 3.0 + 20.0, center_y * 3.0 + 20.0, 256, 256, 4);
		let threshold = 0.78 + (edge_noise - 0.5) * 0.45;
		let grain = read_grid_fbm(grid, center_x * 9.0 + 50.0, center_y * 9.0 + 50.0, 256, 256, 3);
		let darkness = (1.0 - smooth_step(threshold - 0.32, threshold, distance)) * (0.7 + 0.3 * grain);
		let ridge = 1.0 - (2.0 * read_grid_fbm(grid, center_x * 4.5 + 11.0, center_y * 4.5 + 3.0, 256, 256, 4) - 1.0).abs();
		let crack = smooth_step(0.82, 0.95, ridge) * (1.0 - smooth_step(0.2, threshold, distance));
		color[0] = crack + (-distance * distance * 7.0).exp() * 0.8;
		color[1] = 0.0;
		color[2] = 0.0;
		color[3] = darkness * 0.9;
	});
	// 패인 땅. (가운데가 어둡고 가장자리 테가 밝음)
	paint_slot(&mut pixels, ToonVfxSlot::Dent, |u, v, color| {
		let center_x = u * 2.0 - 1.0;
		let center_y = v * 2.0 - 1.0;
		let distance = (center_x * center_x + center_y * center_y).sqrt() + (read_grid_fbm(grid, center_x * 4.0 + 70.0, center_y * 4.0 + 70.0, 256, 256, 3) - 0.5) * 0.12;
		let rim = (-((distance - 0.72) / 0.07) * ((distance - 0.72) / 0.07)).exp();
		let inner = (1.0 - smooth_step(0.3, 0.76, distance)) * 0.55;
		color[0] = rim * 0.8 + (-distance * distance * 10.0).exp() * 0.5;
		color[1] = 0.0;
		color[2] = 0.0;
		color[3] = inner + rim * 0.3;
	});
	// 넓은 둘레 빛.
	paint_slot(&mut pixels, ToonVfxSlot::Glow, |u, v, color| {
		let distance = ((u * 2.0 - 1.0).powi(2) + (v * 2.0 - 1.0).powi(2)).sqrt();
		color[3] = (-distance * distance * 4.5).exp() * (1.0 - smooth_step(0.85, 1.0, distance));
	});
	pixels
}

//==============================================================================
/// 불덩이의 단위 구. ((자리, 번호), 노멀은 자리와 같음)
//==============================================================================
fn create_sphere_arrays() -> (Vec<f32>, Vec<u16>) {
	let [around_count, down_count] = FIREBALL_SEGMENTS;
	let mut positions = Vec::new();
	let mut indices = Vec::new();
	for down_index in 0..=down_count {
		let polar = down_index as f32 / down_count as f32 * std::f32::consts::PI;
		for around_index in 0..=around_count {
			let azimuth = around_index as f32 / around_count as f32 * std::f32::consts::PI * 2.0;
			positions.extend_from_slice(&[polar.sin() * azimuth.cos(), polar.cos(), polar.sin() * azimuth.sin()]);
		}
	}
	push_grid_indices(&mut indices, around_count, down_count);
	(positions, indices)
}

//==============================================================================
/// 둘레로 도는 격자. ((자리, 번호), 자리는 (둘레 0 ~ 1, 가로질러 0 ~ 1))
//==============================================================================
fn create_grid_arrays(segments: [usize; 2]) -> (Vec<f32>, Vec<u16>) {
	let [around_count, across_count] = segments;
	let mut positions = Vec::new();
	let mut indices = Vec::new();
	for across_index in 0..=across_count {
		for around_index in 0..=around_count {
			positions.extend_from_slice(&[around_index as f32 / around_count as f32, across_index as f32 / across_count as f32]);
		}
	}
	push_grid_indices(&mut indices, around_count, across_count);
	(positions, indices)
}

//==============================================================================
/// 격자의 세모 번호. (줄마다 around_count + 1 점)
//==============================================================================
fn push_grid_indices(indices: &mut Vec<u16>, around_count: usize, row_count: usize) {
	for row_index in 0..row_count {
		for around_index in 0..around_count {
			let first = (row_index * (around_count + 1) + around_index) as u16;
			let second = first + around_count as u16 + 1;
			indices.extend_from_slice(&[first, second, first + 1, second, second + 1, first + 1]);
		}
	}
}

//==============================================================================
/// 축에 수직인 두 쪽. ([옆 A, 옆 B], 둘 다 길이 1, 축이 0 이면 x 와 z)
//==============================================================================
fn read_axis_sides(axis: [f32; 3]) -> [[f32; 3]; 2] {
	let axis_length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
	if axis_length < 0.000001 {
		return [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
	}
	let [axis_x, axis_y, axis_z] = [axis[0] / axis_length, axis[1] / axis_length, axis[2] / axis_length];
	let helper = if axis_y.abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
	let mut side = [axis_y * helper[2] - axis_z * helper[1], axis_z * helper[0] - axis_x * helper[2], axis_x * helper[1] - axis_y * helper[0]];
	let side_length = (side[0] * side[0] + side[1] * side[1] + side[2] * side[2]).sqrt();
	for value in side.iter_mut() {
		*value /= side_length;
	}
	[side, [axis_y * side[2] - axis_z * side[1], axis_z * side[0] - axis_x * side[2], axis_x * side[1] - axis_y * side[0]]]
}

//==============================================================================
/// 유니폼 올리기. (이름으로 찾아)
//==============================================================================
fn set_uniform_float(gl: &glow::Context, shader_program: &ShaderProgram, name: &str, value: f32) {
	unsafe {
		gl.uniform_1_f32(shader_program.get_uniform_location(gl, name).as_ref(), value);
	}
}

//==============================================================================
/// vec2 유니폼 올리기. (이름으로 찾아)
//==============================================================================
fn set_uniform_vector2(gl: &glow::Context, shader_program: &ShaderProgram, name: &str, x: f32, y: f32) {
	unsafe {
		gl.uniform_2_f32(shader_program.get_uniform_location(gl, name).as_ref(), x, y);
	}
}

//==============================================================================
/// vec3 유니폼 올리기. (이름으로 찾아)
//==============================================================================
fn set_uniform_vector3(gl: &glow::Context, shader_program: &ShaderProgram, name: &str, value: [f32; 3]) {
	unsafe {
		gl.uniform_3_f32(shader_program.get_uniform_location(gl, name).as_ref(), value[0], value[1], value[2]);
	}
}

//==============================================================================
/// vec4 유니폼 올리기. (이름으로 찾아)
//==============================================================================
fn set_uniform_vector4(gl: &glow::Context, shader_program: &ShaderProgram, name: &str, value: [f32; 4]) {
	unsafe {
		gl.uniform_4_f32(shader_program.get_uniform_location(gl, name).as_ref(), value[0], value[1], value[2], value[3]);
	}
}

impl ToonVfx {
	//==============================================================================
	/// 셰이더 소스. ((정점, 조각) 여덟 벌: 파티클, 불덩이, 띠, 자국, 빔, 기둥, 리본, 섬광, 공통 조각을 넣은 것)
	///
	/// # 돌려줌
	/// (정점, 조각) 셰이더 소스의 목록. (파티클, 불덩이, 띠, 자국, 빔, 기둥, 리본, 섬광 차례, 공통 조각을 넣은 것)
	//==============================================================================
	pub fn read_program_sources() -> Vec<(String, String)> {
		[(PARTICLE_VERTEX_SHADER, PARTICLE_FRAGMENT_SHADER), (FIREBALL_VERTEX_SHADER, FIREBALL_FRAGMENT_SHADER), (BAND_VERTEX_SHADER, BAND_FRAGMENT_SHADER), (DECAL_VERTEX_SHADER, DECAL_FRAGMENT_SHADER), (BEAM_VERTEX_SHADER, BEAM_FRAGMENT_SHADER), (PLUME_VERTEX_SHADER, PLUME_FRAGMENT_SHADER), (RIBBON_VERTEX_SHADER, RIBBON_FRAGMENT_SHADER), (FLASH_VERTEX_SHADER, FLASH_FRAGMENT_SHADER)]
			.iter()
			.map(|(vertex_source, fragment_source)| (expand_shader(vertex_source), expand_shader(fragment_source)))
			.collect()
	}

	//==============================================================================
	/// 생성. (셰이더와 메쉬, 파티클 버퍼를 지음, 아틀라스는 처음 그릴 때, 상한은 한꺼번에 떠 있는 파티클 수)
	///
	/// # 인자
	/// - `gl`: 효과를 그릴 GL 컨텍스트.
	/// - `particle_capacity`: 한꺼번에 떠 있는 파티클의 상한. (1 이상으로 올림, 원본 기본값은 TOON_VFX_DEFAULT_PARTICLE_CAPACITY)
	///
	/// # 돌려줌
	/// 효과 바탕. 셰이더, 버퍼, 버텍스 어레이를 못 지으면 Err(오류 글).
	//==============================================================================
	pub fn new(gl: &glow::Context, particle_capacity: usize) -> Result<ToonVfx, String> {
		let source_list = ToonVfx::read_program_sources();
		let location_list: [&[(&str, u32)]; 8] = [&PARTICLE_ATTRIBUTE_LOCATIONS, &SPHERE_ATTRIBUTE_LOCATIONS, &GRID_ATTRIBUTE_LOCATIONS, &CORNER_ATTRIBUTE_LOCATIONS, &GRID_ATTRIBUTE_LOCATIONS, &GRID_ATTRIBUTE_LOCATIONS, &RIBBON_ATTRIBUTE_LOCATIONS, &CORNER_ATTRIBUTE_LOCATIONS];
		let mut program_list = Vec::new();
		for ((vertex_source, fragment_source), attribute_locations) in source_list.iter().zip(location_list.iter()) {
			program_list.push(ShaderProgram::create_with_attribute_locations(gl, vertex_source, fragment_source, attribute_locations)?);
		}
		let mut program_iterator = program_list.into_iter();
		let mut take_program = || program_iterator.next().ok_or_else(|| String::from("ToonVfx: 셰이더가 모자란다"));
		let particle_program = take_program()?;
		let fireball_program = take_program()?;
		let band_program = take_program()?;
		let decal_program = take_program()?;
		let beam_program = take_program()?;
		let plume_program = take_program()?;
		let ribbon_program = take_program()?;
		let flash_program = take_program()?;
		let mut buffer_list = Vec::new();
		let corner_data = [-1.0f32, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
		let particle_capacity = particle_capacity.max(1);
		unsafe {
			let particle_buffer = gl.create_buffer()?;
			buffer_list.push(particle_buffer);
			let particle_vertex_array = ToonVfx::create_particle_vertex_array(gl, &mut buffer_list, &corner_data, particle_buffer)?;
			let glint_buffer = gl.create_buffer()?;
			buffer_list.push(glint_buffer);
			let glint_vertex_array = ToonVfx::create_particle_vertex_array(gl, &mut buffer_list, &corner_data, glint_buffer)?;
			// 리본: 자리와 (길이 쪽, 가로).
			let ribbon_vertex_array = gl.create_vertex_array()?;
			gl.bind_vertex_array(Some(ribbon_vertex_array));
			let ribbon_buffer = gl.create_buffer()?;
			buffer_list.push(ribbon_buffer);
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(ribbon_buffer));
			gl.enable_vertex_attrib_array(0);
			gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, (RIBBON_FLOAT_COUNT * 4) as i32, 0);
			gl.enable_vertex_attrib_array(1);
			gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, (RIBBON_FLOAT_COUNT * 4) as i32, 12);
			gl.bind_vertex_array(None);
			// 바닥 자국: 네 귀퉁이만.
			let decal_vertex_array = gl.create_vertex_array()?;
			gl.bind_vertex_array(Some(decal_vertex_array));
			ToonVfx::bind_corner_buffer(gl, &mut buffer_list, &corner_data, 0)?;
			gl.bind_vertex_array(None);
			let (sphere_positions, sphere_indices) = create_sphere_arrays();
			let sphere_mesh = ToonVfx::create_indexed_mesh(gl, &mut buffer_list, &sphere_positions, &sphere_indices, 3)?;
			let (band_positions, band_indices) = create_grid_arrays(BAND_SEGMENTS);
			let band_mesh = ToonVfx::create_indexed_mesh(gl, &mut buffer_list, &band_positions, &band_indices, 2)?;
			let (lathe_positions, lathe_indices) = create_grid_arrays(LATHE_SEGMENTS);
			let lathe_mesh = ToonVfx::create_indexed_mesh(gl, &mut buffer_list, &lathe_positions, &lathe_indices, 2)?;
			Ok(ToonVfx {
				atlas_texture: None,
				atlas_pixels: None,
				ramp_list: ToonVfx::read_default_ramps(),
				is_ramp_dirty: true,
				particle_program,
				fireball_program,
				band_program,
				decal_program,
				beam_program,
				plume_program,
				ribbon_program,
				flash_program,
				rim_color: DEFAULT_RIM_COLOR,
				buffer_list,
				particle_vertex_array,
				particle_buffer,
				particle_data: vec![0.0; particle_capacity * PARTICLE_FLOAT_COUNT],
				particle_capacity,
				particle_write_index: 0,
				particle_used_count: 0,
				is_particle_dirty: false,
				particle_live_until: 0.0,
				count_ratio: 1.0,
				sphere_mesh,
				band_mesh,
				lathe_mesh,
				decal_vertex_array,
				ribbon_vertex_array,
				ribbon_buffer,
				ribbon_data: Vec::new(),
				glint_vertex_array,
				glint_buffer,
				glint_data: vec![0.0; DEFAULT_GLINT_CAPACITY * PARTICLE_FLOAT_COUNT],
				glint_capacity: DEFAULT_GLINT_CAPACITY,
				look_gain_list: [1.0; LOOK_COUNT],
				used_event_key_set: HashSet::new(),
				fireball_list: Vec::new(),
				shockwave_list: Vec::new(),
				decal_list: Vec::new(),
				plume_list: Vec::new(),
				lightning_list: Vec::new(),
				flash_list: Vec::new(),
				shaker: Shaker::new(ShakerOptions::default()),
				queued_decal_list: Vec::new(),
				queued_beam_list: Vec::new(),
				queued_ribbon_list: Vec::new(),
				queued_glint_count: 0,
				time: 0.0,
				seeded_random: SeededRandom::new(164),
				floor_height_reader: None,
			})
		}
	}

	//==============================================================================
	/// 기본 색 램프. (흰빛에서 어둠으로 식는 한 줄)
	//==============================================================================
	fn read_default_ramps() -> Vec<Vec<[f32; 3]>> {
		vec![vec![[1.0, 1.0, 1.0], [0.85, 0.85, 0.85], [0.63, 0.63, 0.63], [0.41, 0.41, 0.41], [0.2, 0.2, 0.2], [0.06, 0.06, 0.06]]]
	}

	//==============================================================================
	/// 파티클 꼴의 버텍스 어레이. (네 귀퉁이와 인스턴스 값 일곱)
	//==============================================================================
	unsafe fn create_particle_vertex_array(gl: &glow::Context, buffer_list: &mut Vec<GlBuffer>, corner_data: &[f32], instance_buffer: GlBuffer) -> Result<GlVertexArray, String> {
		let vertex_array = gl.create_vertex_array()?;
		gl.bind_vertex_array(Some(vertex_array));
		ToonVfx::bind_corner_buffer(gl, buffer_list, corner_data, 0)?;
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_buffer));
		for attribute_index in 0..7u32 {
			let location = 1 + attribute_index;
			gl.enable_vertex_attrib_array(location);
			gl.vertex_attrib_pointer_f32(location, 4, glow::FLOAT, false, (PARTICLE_FLOAT_COUNT * 4) as i32, (attribute_index * 16) as i32);
			gl.vertex_attrib_divisor(location, 1);
		}
		gl.bind_vertex_array(None);
		Ok(vertex_array)
	}

	//==============================================================================
	/// 네 귀퉁이 버퍼를 묶인 버텍스 어레이에 붙이기.
	//==============================================================================
	unsafe fn bind_corner_buffer(gl: &glow::Context, buffer_list: &mut Vec<GlBuffer>, corner_data: &[f32], location: u32) -> Result<(), String> {
		let corner_buffer = gl.create_buffer()?;
		buffer_list.push(corner_buffer);
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(corner_buffer));
		gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(corner_data), glow::STATIC_DRAW);
		gl.enable_vertex_attrib_array(location);
		gl.vertex_attrib_pointer_f32(location, 2, glow::FLOAT, false, 8, 0);
		Ok(())
	}

	//==============================================================================
	/// 번호로 그리는 메쉬 짓기. (자리는 어트리뷰트 0)
	//==============================================================================
	unsafe fn create_indexed_mesh(gl: &glow::Context, buffer_list: &mut Vec<GlBuffer>, positions: &[f32], indices: &[u16], component_count: i32) -> Result<IndexedMesh, String> {
		let vertex_array = gl.create_vertex_array()?;
		gl.bind_vertex_array(Some(vertex_array));
		let vertex_buffer = gl.create_buffer()?;
		buffer_list.push(vertex_buffer);
		gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
		gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(positions), glow::STATIC_DRAW);
		gl.enable_vertex_attrib_array(0);
		gl.vertex_attrib_pointer_f32(0, component_count, glow::FLOAT, false, component_count * 4, 0);
		let index_buffer = gl.create_buffer()?;
		buffer_list.push(index_buffer);
		gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(index_buffer));
		gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_slice_to_bytes(indices), glow::STATIC_DRAW);
		gl.bind_vertex_array(None);
		Ok(IndexedMesh { vertex_array, index_count: indices.len() as i32 })
	}

	//==============================================================================
	/// 색 램프 설정. (줄마다 흰 심에서 어둠으로 가는 "#rrggbb" 나 0 ~ 1 의 [r, g, b], 여덟 줄까지, 효과의 ramp 는 이 차례, 비면 기본 한 줄)
	///
	/// # 인자
	/// - `ramp_list`: 줄마다 흰 심에서 어둠으로 가는 색들. (여덟 줄까지, 빈 줄은 버림)
	//==============================================================================
	pub fn set_ramps<C: ToonRampColor>(&mut self, ramp_list: &[Vec<C>]) {
		let valid_list: Vec<Vec<[f32; 3]>> = ramp_list.iter().filter(|stops| !stops.is_empty()).take(TOON_VFX_MAXIMUM_RAMPS).map(|stops| stops.iter().map(|stop| stop.read_ramp_color()).collect()).collect();
		self.ramp_list = if valid_list.is_empty() { ToonVfx::read_default_ramps() } else { valid_list };
		self.is_ramp_dirty = true;
	}

	//==============================================================================
	/// 아틀라스 텍스처 읽기. (처음 부를 때 픽셀을 지어 올리고, 램프가 바뀌었으면 다시 올림)
	//==============================================================================
	fn read_atlas_texture(&mut self, gl: &glow::Context) -> Option<GlTexture> {
		if let (Some(atlas_texture), false) = (self.atlas_texture, self.is_ramp_dirty) {
			return Some(atlas_texture);
		}
		let mut pixels = self.atlas_pixels.take().unwrap_or_else(create_atlas_pixels);
		paint_ramps(&mut pixels, &self.ramp_list);
		self.is_ramp_dirty = false;
		unsafe {
			let is_new = self.atlas_texture.is_none();
			let atlas_texture = match self.atlas_texture {
				Some(atlas_texture) => {
					atlas_texture
				}
				None => {
					gl.create_texture().ok()?
				}
			};
			gl.active_texture(glow::TEXTURE0 + VFX_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(atlas_texture));
			gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, ATLAS_WIDTH as i32, ATLAS_HEIGHT as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&pixels)));
			if is_new {
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
				gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			}
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
			self.atlas_texture = Some(atlas_texture);
		}
		self.atlas_pixels = Some(pixels);
		self.atlas_texture
	}

	//==============================================================================
	/// 아틀라스를 미리 지어 두기. (처음 터질 때 멈칫하지 않게)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn prepare(&mut self, gl: &glow::Context) {
		self.read_atlas_texture(gl);
	}

	//==============================================================================
	/// 파티클 상한 설정. (떠 있던 파티클은 지운다)
	///
	/// # 인자
	/// - `particle_capacity`: 한꺼번에 떠 있는 파티클의 상한. (1 이상)
	//==============================================================================
	pub fn set_particle_capacity(&mut self, particle_capacity: usize) {
		self.particle_capacity = particle_capacity.max(1);
		self.particle_data = vec![0.0; self.particle_capacity * PARTICLE_FLOAT_COUNT];
		self.particle_write_index = 0;
		self.particle_used_count = 0;
		self.particle_live_until = self.time;
		self.is_particle_dirty = false;
	}

	//==============================================================================
	/// 파티클 상한 반환.
	///
	/// # 돌려줌
	/// 한꺼번에 떠 있는 파티클의 상한.
	//==============================================================================
	pub fn get_particle_capacity(&self) -> usize {
		self.particle_capacity
	}

	//==============================================================================
	/// 내는 파티클 수의 몫 설정. (폰이나 가벼운 길에서 수만 줄임, 0 ~ 1, 내면 적어도 하나)
	///
	/// # 인자
	/// - `count_ratio`: 내는 파티클 수에 곱하는 몫. (0 ~ 1)
	//==============================================================================
	pub fn set_count_ratio(&mut self, count_ratio: f32) {
		self.count_ratio = count_ratio.clamp(0.0, 1.0);
	}

	//==============================================================================
	/// 내는 파티클 수의 몫 반환.
	///
	/// # 돌려줌
	/// 내는 파티클 수에 곱하는 몫. (0 ~ 1)
	//==============================================================================
	pub fn get_count_ratio(&self) -> f32 {
		self.count_ratio
	}

	//==============================================================================
	/// 빛 점 상한 설정. (프레임마다 queue_glint 로 넘기는 수, 기본 96, 이번 프레임에 넘긴 빛 점은 지운다)
	///
	/// # 인자
	/// - `glint_capacity`: 프레임마다 넘기는 빛 점의 상한. (1 이상)
	//==============================================================================
	pub fn set_glint_capacity(&mut self, glint_capacity: usize) {
		self.glint_capacity = glint_capacity.max(1);
		self.glint_data = vec![0.0; self.glint_capacity * PARTICLE_FLOAT_COUNT];
		self.queued_glint_count = 0;
	}

	//==============================================================================
	/// 빛 점 상한 반환.
	///
	/// # 돌려줌
	/// 프레임마다 넘기는 빛 점의 상한.
	//==============================================================================
	pub fn get_glint_capacity(&self) -> usize {
		self.glint_capacity
	}

	//==============================================================================
	/// 꼴마다 밝기 배 설정. (파티클과 빛 점의 밝기에 곱함, 기본 1)
	///
	/// # 인자
	/// - `look`: 꼴. (ToonVfxLook)
	/// - `gain`: 밝기 배. (0 이상, 기본 1)
	//==============================================================================
	pub fn set_look_gain(&mut self, look: ToonVfxLook, gain: f32) {
		self.look_gain_list[look.get_index() as usize] = gain.max(0.0);
	}

	//==============================================================================
	/// 꼴마다 밝기 배 반환.
	///
	/// # 인자
	/// - `look`: 꼴. (ToonVfxLook)
	///
	/// # 돌려줌
	/// 그 꼴의 밝기 배.
	//==============================================================================
	pub fn get_look_gain(&self, look: ToonVfxLook) -> f32 {
		self.look_gain_list[look.get_index() as usize]
	}

	//==============================================================================
	/// 사건 열쇠 적기. (열쇠가 없으면 늘 true, 처음 보는 열쇠면 적고 true, 이미 낸 열쇠면 false, clear 가 잊는다)
	///
	/// # 인자
	/// - `key`: 사건 열쇠. (None 이면 늘 낸다)
	///
	/// # 돌려줌
	/// 내도 되면 true, 이미 낸 열쇠면 false.
	//==============================================================================
	pub fn register_event_key(&mut self, key: Option<u64>) -> bool {
		match key {
			Some(event_key) => {
				self.used_event_key_set.insert(event_key)
			}
			None => {
				true
			}
		}
	}

	//==============================================================================
	/// 효과 테의 색 설정. ([r, g, b] 0 ~ 1, 테를 두른 파티클과 번짐 리본의 가장자리, 기본은 ToonRenderer 의 기본 바깥선 색)
	///
	/// # 인자
	/// - `rim_color`: 테의 색. ([r, g, b] 0 ~ 1)
	//==============================================================================
	pub fn set_rim_color(&mut self, rim_color: [f32; 3]) {
		self.rim_color = rim_color;
	}

	//==============================================================================
	/// 효과 테의 색 반환.
	///
	/// # 돌려줌
	/// 효과 테의 색. ([r, g, b] 0 ~ 1)
	//==============================================================================
	pub fn get_rim_color(&self) -> [f32; 3] {
		self.rim_color
	}

	//==============================================================================
	/// 바닥 높이를 읽는 함수 설정. ((x, z) → 높이, None 이면 0)
	///
	/// # 인자
	/// - `floor_height_reader`: (x, z) 로 바닥 높이를 돌려주는 함수. (None 이면 늘 0)
	//==============================================================================
	pub fn set_floor_height_reader(&mut self, floor_height_reader: Option<Box<dyn Fn(f32, f32) -> f32>>) {
		self.floor_height_reader = floor_height_reader;
	}

	//==============================================================================
	/// 그 자리의 바닥 높이.
	///
	/// # 인자
	/// - `x`: 장면의 x.
	/// - `z`: 장면의 z.
	///
	/// # 돌려줌
	/// 그 자리의 바닥 높이.
	//==============================================================================
	pub fn read_floor_height(&self, x: f32, z: f32) -> f32 {
		match self.floor_height_reader.as_ref() {
			Some(floor_height_reader) => {
				floor_height_reader(x, z)
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 이 바탕의 시계. (초)
	///
	/// # 돌려줌
	/// 이 바탕의 시계. (초, update 로 나아감)
	//==============================================================================
	pub fn get_time(&self) -> f32 {
		self.time
	}

	//==============================================================================
	/// 떠 있는 효과의 수.
	///
	/// # 돌려줌
	/// 떠 있는 효과의 수. (ToonVfxCounts)
	//==============================================================================
	pub fn get_active_counts(&self) -> ToonVfxCounts {
		ToonVfxCounts {
			particle_count: if self.time < self.particle_live_until { self.particle_used_count } else { 0 },
			fireball_count: self.fireball_list.len(),
			shockwave_count: self.shockwave_list.len(),
			decal_count: self.decal_list.len(),
			plume_count: self.plume_list.len(),
			lightning_count: self.lightning_list.len(),
			flash_count: self.flash_list.len(),
		}
	}

	//==============================================================================
	/// 다 지우기. (판이 바뀔 때, 사건 열쇠도 잊는다)
	//==============================================================================
	pub fn clear(&mut self) {
		self.fireball_list.clear();
		self.shockwave_list.clear();
		self.decal_list.clear();
		self.plume_list.clear();
		self.lightning_list.clear();
		self.flash_list.clear();
		self.shaker.stop();
		self.particle_live_until = self.time;
		self.used_event_key_set.clear();
		self.clear_queue();
	}

	//==============================================================================
	/// 이번 프레임에 넘긴 것 비우기. (그린 뒤)
	//==============================================================================
	pub fn clear_queue(&mut self) {
		self.queued_decal_list.clear();
		self.queued_beam_list.clear();
		self.queued_ribbon_list.clear();
		self.queued_glint_count = 0;
	}

	//==============================================================================
	/// 한 프레임 나이 먹이기. (시계를 나아가게 하고 다 된 불덩이, 띠, 자국, 기둥, 번개, 섬광을 지움, 흔들림을 잦아들게 함)
	///
	/// # 인자
	/// - `delta_time`: 초.
	//==============================================================================
	pub fn update(&mut self, delta_time: f32) {
		self.time += delta_time;
		self.fireball_list.retain_mut(|effect| {
			effect.age += delta_time;
			effect.age < effect.options.life
		});
		self.shockwave_list.retain_mut(|effect| {
			effect.age += delta_time;
			effect.age < effect.options.life + effect.options.delay
		});
		self.decal_list.retain_mut(|effect| {
			effect.age += delta_time;
			effect.age < effect.options.life
		});
		self.plume_list.retain_mut(|effect| {
			effect.age += delta_time;
			effect.age < effect.options.life
		});
		self.lightning_list.retain_mut(|effect| {
			effect.age += delta_time;
			effect.age < effect.life
		});
		self.flash_list.retain_mut(|effect| {
			effect.age += delta_time;
			effect.age < effect.life
		});
		self.shaker.tick(delta_time);
	}

	//==============================================================================
	/// 낼 파티클 수. (몫을 곱하고, 내면 하나 이상)
	//==============================================================================
	fn read_count(&self, count: u32) -> u32 {
		if count == 0 {
			return 0;
		}
		if self.count_ratio >= 1.0 {
			return count;
		}
		((count as f32 * self.count_ratio).round() as u32).max(1)
	}

	//==============================================================================
	/// 파티클 내기. (floor 를 주면 그 높이에서, is_grounded 면 낸 자리의 바닥 높이에서 멈춘다)
	///
	/// # 인자
	/// - `options`: 내는 값. (ToonParticleOptions)
	//==============================================================================
	pub fn emit_particles(&mut self, options: &ToonParticleOptions) {
		if !self.register_event_key(options.key) {
			return;
		}
		let count = self.read_count(options.count);
		let [x, y, z] = options.position;
		let floor = match options.floor {
			Some(floor) => {
				floor
			}
			None => {
				if options.is_grounded { self.read_floor_height(x, z) } else { NO_FLOOR }
			}
		};
		let direction = options.direction;
		let direction_length = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt().max(0.0001);
		let axis = [direction[0] / direction_length, direction[1] / direction_length, direction[2] / direction_length];
		let sides = read_axis_sides(direction);
		let radius = options.radius;
		for particle_index in 0..count {
			let mut random = || self.seeded_random.next_value() as f32;
			let mut offset = [0.0f32; 3];
			let mut heading = [0.0f32, 1.0, 0.0];
			match options.shape {
				ToonVfxShape::Ring => {
					let angle = (particle_index as f32 + random() * 0.8) / count as f32 * std::f32::consts::PI * 2.0;
					offset = [angle.cos() * radius, 0.0, angle.sin() * radius];
					heading = [angle.cos(), 0.0, angle.sin()];
				}
				ToonVfxShape::Disc => {
					let angle = random() * std::f32::consts::PI * 2.0;
					let reach = random().sqrt() * radius;
					offset = [angle.cos() * reach, 0.0, angle.sin() * reach];
					heading = [angle.cos(), 0.0, angle.sin()];
				}
				ToonVfxShape::Cone => {
					// 고른 쪽을 가운데로 spread 안의 원뿔. (쪽에 수직인 두 축으로 기울임)
					let tilt = random() * options.spread;
					let turn = random() * std::f32::consts::PI * 2.0;
					let reach = random().sqrt() * radius;
					for component in 0..3 {
						let around = sides[0][component] * turn.cos() + sides[1][component] * turn.sin();
						heading[component] = axis[component] * tilt.cos() + around * tilt.sin();
						offset[component] = around * reach;
					}
				}
				ToonVfxShape::Line => {
					let along = random() * options.length;
					let turn = random() * std::f32::consts::PI * 2.0;
					let reach = random().sqrt() * radius;
					heading = axis;
					for component in 0..3 {
						offset[component] = axis[component] * along + (sides[0][component] * turn.cos() + sides[1][component] * turn.sin()) * reach;
					}
				}
				ToonVfxShape::Point | ToonVfxShape::Sphere => {
					// 공 안의 고른 쪽. (point 는 반지름을 보지 않는다)
					let height_ratio = random() * 2.0 - 1.0;
					let angle = random() * std::f32::consts::PI * 2.0;
					let ring_ratio = (1.0 - height_ratio * height_ratio).sqrt();
					heading = [angle.cos() * ring_ratio, height_ratio, angle.sin() * ring_ratio];
					let reach = if options.shape == ToonVfxShape::Point { 0.0 } else { random().cbrt() * radius };
					offset = [heading[0] * reach, heading[1] * reach, heading[2] * reach];
				}
			}
			if options.is_upward && heading[1] < 0.0 {
				heading[1] = -heading[1];
				offset[1] = offset[1].abs();
			}
			let seeded_random = &mut self.seeded_random;
			let speed = options.speed.read_value(seeded_random);
			let rise = options.rise.read_value(seeded_random);
			let size = options.size.read_value(seeded_random);
			let size_end = size * options.size_end.read_value(seeded_random);
			let life = options.life.read_value(seeded_random).max(0.001);
			let delay = options.delay.read_value(seeded_random);
			let spin = options.spin.read_value(seeded_random);
			let seed = seeded_random.next_value() as f32;
			let mut velocity = [heading[0] * speed, heading[1] * speed + rise, heading[2] * speed];
			if options.is_inward {
				velocity = [-offset[0] / life, -offset[1] / life, -offset[2] / life];
			}
			let start_time = self.time + delay - options.age;
			self.write_particle([
				[x + offset[0], y + offset[1], z + offset[2], start_time],
				[velocity[0], velocity[1], velocity[2], life],
				[options.gravity, options.drag, size, size_end],
				[options.color[0], options.color[1], options.color[2], options.ramp as f32],
				[options.look.get_index(), options.stretch, spin, floor],
				[options.brightness, options.alpha, seed, options.fade_in],
				[options.size_curve, options.rim, 0.0, 0.0],
			]);
		}
	}

	//==============================================================================
	/// 파티클 하나를 다음 칸에 쓰기. (가득 차면 가장 오래 전에 쓴 칸을 덮음, 값 일곱은 원본 writeParticle 의 차례)
	//==============================================================================
	fn write_particle(&mut self, value_list: [[f32; 4]; 7]) {
		let offset = self.particle_write_index * PARTICLE_FLOAT_COUNT;
		for (value_index, values) in value_list.iter().enumerate() {
			self.particle_data[offset + value_index * 4..offset + value_index * 4 + 4].copy_from_slice(values);
		}
		self.particle_write_index = (self.particle_write_index + 1) % self.particle_capacity;
		self.particle_used_count = (self.particle_used_count + 1).min(self.particle_capacity);
		self.particle_live_until = self.particle_live_until.max(value_list[0][3] + value_list[1][3]);
		self.is_particle_dirty = true;
	}

	//==============================================================================
	/// 불덩이 더하기. (잡음으로 끓는 구가 부풀며 식어 연기가 되고 위로 떠오름)
	///
	/// # 인자
	/// - `options`: 불덩이 값. (ToonFireballOptions)
	//==============================================================================
	pub fn add_fireball(&mut self, options: &ToonFireballOptions) {
		if !self.register_event_key(options.key) || (options.age > 0.0 && options.age >= options.life) {
			return;
		}
		let seed = self.seeded_random.next_value() as f32 * 40.0;
		let mut options = *options;
		options.grow_seconds = options.grow_seconds.max(0.001);
		self.fireball_list.push(LiveFireball { options, seed, age: options.age });
	}

	//==============================================================================
	/// 충격파 띠 더하기. (빠르게 퍼지다 천천히, 바깥 끝이 말려 오르고 찢기며 엷어짐, 밑의 바닥이 잠깐 밝아짐, 바닥 높이에 놓임)
	///
	/// # 인자
	/// - `options`: 충격파 값. (ToonShockwaveOptions)
	//==============================================================================
	pub fn add_shockwave(&mut self, options: &ToonShockwaveOptions) {
		if !self.register_event_key(options.key) || (options.age > 0.0 && options.age >= options.life + options.delay) {
			return;
		}
		let floor_height = self.read_floor_height(options.x, options.z);
		let seed = self.seeded_random.next_value() as f32 * 40.0;
		self.shockwave_list.push(LiveShockwave { options: *options, floor_height, seed, age: options.age });
	}

	//==============================================================================
	/// 바닥 자국 더하기. (그을음은 열기에서 식고 끝 1 초에 엷어짐, 바닥 높이에 놓임)
	///
	/// # 인자
	/// - `options`: 자국 값. (ToonDecalOptions)
	//==============================================================================
	pub fn add_decal(&mut self, options: &ToonDecalOptions) {
		if !self.register_event_key(options.key) || (options.age > 0.0 && options.age >= options.life) {
			return;
		}
		let floor_height = self.read_floor_height(options.x, options.z);
		let random_angle = self.seeded_random.next_value() as f32 * std::f32::consts::PI * 2.0;
		let random_seed = self.seeded_random.next_value() as f32 * 40.0;
		self.decal_list.push(LiveDecal { options: *options, floor_height, angle: options.angle.unwrap_or(random_angle), seed: options.seed.unwrap_or(random_seed), age: options.age });
	}

	//==============================================================================
	/// 이번 프레임에만 그리는 바닥 자국 넘기기. (나이 대신 options.heat, 돎과 씨앗은 주지 않으면 0)
	///
	/// # 인자
	/// - `options`: 자국 값. (ToonDecalOptions, 나이 대신 heat(0 ~ 1)를 본다)
	//==============================================================================
	pub fn queue_decal(&mut self, options: &ToonDecalOptions) {
		let floor_height = self.read_floor_height(options.x, options.z);
		self.queued_decal_list.push(LiveDecal { options: *options, floor_height, angle: options.angle.unwrap_or(0.0), seed: options.seed.unwrap_or(0.0), age: 0.0 });
	}

	//==============================================================================
	/// 이번 프레임에만 그리는 발밑 그늘 넘기기. (가운데 alpha 에서 바깥으로 둥글게 엷어지는 그늘, 바닥 높이에 놓임)
	///
	/// # 인자
	/// - `options`: 그늘 값. (ToonBlobShadowOptions)
	//==============================================================================
	pub fn queue_blob_shadow(&mut self, options: &ToonBlobShadowOptions) {
		self.queue_decal(&ToonDecalOptions { x: options.x, z: options.z, radius: options.radius, mode: ToonVfxDecalMode::Shade, alpha: options.alpha, ..ToonDecalOptions::default() });
	}

	//==============================================================================
	/// 연기 기둥 더하기. (기둥이 솟고 갓이 말려 오르며 커지고, 갓 밑이 달아올랐다 식으며 엷어짐, 바닥 높이에 섬)
	///
	/// # 인자
	/// - `options`: 연기 기둥 값. (ToonPlumeOptions)
	//==============================================================================
	pub fn add_plume(&mut self, options: &ToonPlumeOptions) {
		if !self.register_event_key(options.key) || (options.age > 0.0 && options.age >= options.life) {
			return;
		}
		let floor_height = self.read_floor_height(options.x, options.z);
		let seed = self.seeded_random.next_value() as f32 * 40.0;
		self.plume_list.push(LivePlume { options: *options, floor_height, cap_radius: options.cap_radius.unwrap_or(options.height * 0.4), stem_radius: options.stem_radius.unwrap_or(options.height * 0.1), seed, age: options.age });
	}

	//==============================================================================
	/// 이번 프레임에만 그리는 회전체 빔 넘기기.
	///
	/// # 인자
	/// - `options`: 빔 값. (ToonBeamOptions, radius 와 alpha 가 0 보다 커야 넘어감)
	//==============================================================================
	pub fn queue_beam(&mut self, options: &ToonBeamOptions) {
		if options.radius <= 0.0 || options.alpha <= 0.0 {
			return;
		}
		self.queued_beam_list.push(*options);
	}

	//==============================================================================
	/// 이번 프레임에만 그리는 리본 넘기기. (점 둘 이상)
	///
	/// # 인자
	/// - `options`: 리본 값. (ToonRibbonOptions, 점이 둘 이상이고 width 와 alpha 가 0 보다 커야 넘어감)
	//==============================================================================
	pub fn queue_ribbon(&mut self, options: ToonRibbonOptions) {
		if options.points.len() < 2 || options.width <= 0.0 || options.alpha <= 0.0 {
			return;
		}
		self.queued_ribbon_list.push(options);
	}

	//==============================================================================
	/// 이번 프레임의 번개 넘기기. (축을 감아 내려가는 리본 여럿, 0.08 초마다 다시 꺾임, 같은 시각과 씨앗이면 같은 꼴)
	///
	/// # 인자
	/// - `options`: 번개 값. (ToonLightningOptions, life, age, key 는 보지 않음)
	//==============================================================================
	pub fn queue_lightning(&mut self, options: &ToonLightningOptions) {
		let start = options.start;
		let time = options.time.unwrap_or(self.time);
		let axis = [options.end[0] - start[0], options.end[1] - start[1], options.end[2] - start[2]];
		let sides = read_axis_sides(axis);
		let kink_step = (time / LIGHTNING_KINK_SECONDS).floor() as i64;
		let count = options.count;
		let base_seed = options.seed.unwrap_or(0.0);
		for bolt_index in 0..count {
			let seed = ((base_seed * 7919.0).floor() as i64 + kink_step * 131 + bolt_index as i64 * 977) as u32;
			let mut seeded_random = SeededRandom::new(seed);
			let mut random = || seeded_random.next_value() as f32;
			let phase = bolt_index as f32 / count as f32 * std::f32::consts::PI * 2.0 + random() * 0.8;
			let turn_scale = options.turns * (0.8 + random() * 0.4);
			let bolt_turns = if random() < 0.5 { -turn_scale } else { turn_scale };
			let mut points = Vec::with_capacity(LIGHTNING_POINT_COUNT);
			for point_index in 0..LIGHTNING_POINT_COUNT {
				let along = point_index as f32 / (LIGHTNING_POINT_COUNT - 1) as f32;
				let angle = phase + along * bolt_turns * std::f32::consts::PI * 2.0 + (random() - 0.5) * 0.5;
				let reach = options.radius * (0.75 + random() * 0.5);
				let cosine = angle.cos() * reach;
				let sine = angle.sin() * reach;
				points.push([
					start[0] + axis[0] * along + sides[0][0] * cosine + sides[1][0] * sine,
					start[1] + axis[1] * along + sides[0][1] * cosine + sides[1][1] * sine,
					start[2] + axis[2] * along + sides[0][2] * cosine + sides[1][2] * sine,
				]);
			}
			let width = options.width * (0.7 + random() * 0.6);
			self.queue_ribbon(ToonRibbonOptions { points, width, taper: 0.6, color: options.color, ramp: options.ramp, alpha: options.alpha, brightness: options.brightness, grain_repeat: 1.5, flow: Some(time * 6.0 + bolt_index as f32), glow: options.glow, rim: options.rim });
		}
	}

	//==============================================================================
	/// 이번 프레임에만 그리는 빛 점 넘기기. (set_glint_capacity 까지, 기본 96)
	///
	/// # 인자
	/// - `options`: 빛 점 값. (ToonGlintOptions)
	//==============================================================================
	pub fn queue_glint(&mut self, options: &ToonGlintOptions) {
		if self.queued_glint_count >= self.glint_capacity {
			return;
		}
		let [x, y, z] = options.position;
		let value_list = [
			[x, y, z, self.time],
			[0.0, 0.0, 0.0, 1000000.0],
			[0.0, 0.0, options.size, options.size],
			[options.color[0], options.color[1], options.color[2], options.ramp as f32],
			[options.look.get_index(), 0.0, 0.0, NO_FLOOR],
			[options.brightness, options.alpha, options.angle / (std::f32::consts::PI * 2.0), 0.0],
			[1.0, options.rim, 0.0, 0.0],
		];
		let offset = self.queued_glint_count * PARTICLE_FLOAT_COUNT;
		for (value_index, values) in value_list.iter().enumerate() {
			self.glint_data[offset + value_index * 4..offset + value_index * 4 + 4].copy_from_slice(values);
		}
		self.queued_glint_count += 1;
	}

	//==============================================================================
	/// 사는 동안 다시 꺾이는 번개 더하기. (그릴 때마다 queue_lightning 으로 넘긴다, 끝 0.1 초에 엷어짐, 나이가 음수면 그만큼 늦게 나타남)
	///
	/// # 인자
	/// - `options`: 번개 값. (ToonLightningOptions, life, age, key 도 읽는다)
	//==============================================================================
	pub fn add_lightning(&mut self, options: &ToonLightningOptions) {
		if !self.register_event_key(options.key) {
			return;
		}
		let mut lightning = *options;
		if lightning.seed.is_none() {
			lightning.seed = Some(self.seeded_random.next_value() as f32 * 1000.0);
		}
		self.lightning_list.push(lightning);
	}

	//==============================================================================
	/// 화면 섬광 더하기. (뷰포트를 그 색으로 잠깐 덮고 빠르게 걷힘)
	///
	/// # 인자
	/// - `options`: 섬광 값. (ToonFlashOptions)
	//==============================================================================
	pub fn add_flash(&mut self, options: &ToonFlashOptions) {
		if !self.register_event_key(options.key) {
			return;
		}
		self.flash_list.push(*options);
	}

	//==============================================================================
	/// 카메라 흔들림 더하기. (장면 길이의 최대 오프셋, 엔진 Shaker 가 위상이 다른 두 사인파와 지수 감쇠로 셈, 흔들리는 중이면 센 쪽)
	///
	/// # 인자
	/// - `strength`: 흔들림의 최대 오프셋. (장면 길이)
	//==============================================================================
	pub fn add_shake(&mut self, strength: f32) {
		self.shaker.add_shake(strength * SHAKE_UNIT_SCALE);
	}

	//==============================================================================
	/// 지금의 카메라 흔들림. (장면 길이, x 는 카메라 오른쪽, y 는 카메라 위쪽으로 카메라 자리와 보는 곳에 같이 더한다)
	///
	/// # 돌려줌
	/// 흔들림 오프셋. (장면 길이, x 는 카메라 오른쪽, y 는 카메라 위쪽)
	//==============================================================================
	pub fn read_shake(&self) -> Vector2 {
		let offset = self.shaker.get_offset();
		Vector2::create(offset.x / SHAKE_UNIT_SCALE, offset.y / SHAKE_UNIT_SCALE)
	}

	//==============================================================================
	/// 카메라가 흔들리는 중인지.
	///
	/// # 돌려줌
	/// 흔들리는 중이면 true.
	//==============================================================================
	pub fn is_shaking(&self) -> bool {
		self.shaker.is_shaking()
	}

	//==============================================================================
	/// 그리기. (바닥 자국, 띠, 연기 기둥, 불덩이, 빔, 리본, 파티클, 섬광 차례, 사는 번개는 이번 프레임의 리본으로 넘김, 넘긴 것은 그린 뒤 비움)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `view_matrix`: 카메라의 보기 행렬.
	/// - `projection_matrix`: 투영 행렬. (원근이나 직교)
	//==============================================================================
	pub fn draw(&mut self, gl: &glow::Context, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
		let is_particle_live = self.particle_used_count > 0 && self.time < self.particle_live_until;
		let time = self.time;
		let live_lightning_list: Vec<ToonLightningOptions> = self.lightning_list.iter().filter(|lightning| lightning.age >= 0.0).copied().collect();
		for lightning in live_lightning_list {
			let fade = ((lightning.life - lightning.age) / LIGHTNING_FADE_SECONDS).min(1.0);
			self.queue_lightning(&ToonLightningOptions { alpha: lightning.alpha * fade, time: Some(time), ..lightning });
		}
		let is_queued = !self.queued_decal_list.is_empty() || !self.queued_beam_list.is_empty() || !self.queued_ribbon_list.is_empty() || self.queued_glint_count > 0;
		if self.decal_list.is_empty() && self.shockwave_list.is_empty() && self.fireball_list.is_empty() && self.plume_list.is_empty() && self.flash_list.is_empty() && !is_particle_live && !is_queued {
			return;
		}
		let frame = ToonVfx::read_frame(view_matrix, projection_matrix);
		let atlas_texture = match self.read_atlas_texture(gl) {
			Some(atlas_texture) => {
				atlas_texture
			}
			None => {
				return;
			}
		};
		unsafe {
			let is_blend_enabled = gl.is_enabled(glow::BLEND);
			let is_depth_test_enabled = gl.is_enabled(glow::DEPTH_TEST);
			let is_cull_face_enabled = gl.is_enabled(glow::CULL_FACE);
			let depth_mask = gl.get_parameter_bool(glow::DEPTH_WRITEMASK);
			let depth_function = gl.get_parameter_i32(glow::DEPTH_FUNC);
			let blend_functions = [gl.get_parameter_i32(glow::BLEND_SRC_RGB), gl.get_parameter_i32(glow::BLEND_DST_RGB), gl.get_parameter_i32(glow::BLEND_SRC_ALPHA), gl.get_parameter_i32(glow::BLEND_DST_ALPHA)];
			let previous_program = crate::graphics3d::read_program_binding(gl);
			let previous_vertex_array = crate::graphics3d::read_vertex_array_binding(gl);
			gl.enable(glow::DEPTH_TEST);
			gl.depth_func(glow::LEQUAL);
			gl.depth_mask(false);
			gl.disable(glow::CULL_FACE);
			gl.enable(glow::BLEND);
			gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
			gl.active_texture(glow::TEXTURE0 + VFX_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, Some(atlas_texture));
			gl.active_texture(glow::TEXTURE0);
			self.draw_decals(gl, &frame);
			self.draw_shockwaves(gl, &frame);
			self.draw_plumes(gl, &frame);
			self.draw_fireballs(gl, &frame);
			self.draw_beams(gl, &frame);
			self.draw_ribbons(gl, &frame);
			self.draw_particles(gl, &frame, is_particle_live);
			self.draw_flashes(gl);
			gl.active_texture(glow::TEXTURE0 + VFX_TEXTURE_UNIT);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_vertex_array(previous_vertex_array);
			gl.use_program(previous_program);
			gl.depth_mask(depth_mask);
			gl.depth_func(depth_function as u32);
			gl.blend_func_separate(blend_functions[0] as u32, blend_functions[1] as u32, blend_functions[2] as u32, blend_functions[3] as u32);
			for (capability, is_enabled) in [(glow::BLEND, is_blend_enabled), (glow::DEPTH_TEST, is_depth_test_enabled), (glow::CULL_FACE, is_cull_face_enabled)] {
				if is_enabled {
					gl.enable(capability);
				}
				else {
					gl.disable(capability);
				}
			}
		}
		self.clear_queue();
	}

	//==============================================================================
	/// 그리는 프레임의 카메라. (보기 행렬에서 카메라 쪽, 오른쪽, 위쪽, 자리를 읽는다, 원근이면 빔과 리본이 카메라 자리를 향한다)
	//==============================================================================
	fn read_frame(view_matrix: &Matrix4, projection_matrix: &Matrix4) -> VfxFrame {
		let view_elements = &view_matrix.elements;
		let mut view_projection_matrix = *projection_matrix;
		view_projection_matrix.multiply(view_matrix);
		let view_direction = [view_elements[2], view_elements[6], view_elements[10]];
		let is_perspective = projection_matrix.elements[11].abs() > 0.5;
		let camera_point = if is_perspective {
			let [translation_x, translation_y, translation_z] = [view_elements[12], view_elements[13], view_elements[14]];
			[
				-(view_elements[0] * translation_x + view_elements[1] * translation_y + view_elements[2] * translation_z),
				-(view_elements[4] * translation_x + view_elements[5] * translation_y + view_elements[6] * translation_z),
				-(view_elements[8] * translation_x + view_elements[9] * translation_y + view_elements[10] * translation_z),
				1.0,
			]
		}
		else {
			[view_direction[0], view_direction[1], view_direction[2], 0.0]
		};
		VfxFrame { view_projection_matrix, view_direction, camera_right: [view_elements[0], view_elements[4], view_elements[8]], camera_up: [view_elements[1], view_elements[5], view_elements[9]], camera_point }
	}

	//==============================================================================
	/// 프로그램을 쓰고 보기와 투영과 아틀라스 유니폼 올리기.
	//==============================================================================
	fn use_program(gl: &glow::Context, shader_program: &ShaderProgram, frame: &VfxFrame) {
		shader_program.use_program(gl);
		unsafe {
			gl.uniform_matrix_4_f32_slice(shader_program.get_uniform_location(gl, "viewProjectionMatrix").as_ref(), false, &frame.view_projection_matrix.elements);
			gl.uniform_1_i32(shader_program.get_uniform_location(gl, "vfxAtlas").as_ref(), VFX_TEXTURE_UNIT as i32);
		}
	}

	//==============================================================================
	/// 번호로 그리는 메쉬 그리기.
	//==============================================================================
	fn draw_indexed_mesh(gl: &glow::Context, indexed_mesh: &IndexedMesh) {
		unsafe {
			gl.draw_elements(glow::TRIANGLES, indexed_mesh.index_count, glow::UNSIGNED_SHORT, 0);
		}
	}

	//==============================================================================
	/// 바닥 자국 그리기. (나이 먹는 자국, 이번 프레임에 넘긴 자국 차례, 아직 나타나지 않은 것은 건너뜀, 깜빡이면 걸러 엷게)
	//==============================================================================
	fn draw_decals(&self, gl: &glow::Context, frame: &VfxFrame) {
		if self.decal_list.is_empty() && self.queued_decal_list.is_empty() {
			return;
		}
		let decal_program = &self.decal_program;
		ToonVfx::use_program(gl, decal_program, frame);
		set_uniform_float(gl, decal_program, "decalTime", self.time);
		set_uniform_vector3(gl, decal_program, "rimColor", self.rim_color);
		unsafe {
			gl.bind_vertex_array(Some(self.decal_vertex_array));
		}
		for decal in self.decal_list.iter() {
			if decal.age < 0.0 {
				continue;
			}
			let options = &decal.options;
			let heat = if options.heat_seconds > 0.0 { (1.0 - decal.age / options.heat_seconds).max(0.0) } else { 0.0 };
			let mut fade = (options.life - decal.age).min(1.0) * options.alpha;
			if options.flicker > 0.0 && (decal.age / options.flicker).floor() as i64 % 2 == 1 {
				fade *= 0.3;
			}
			self.draw_decal(gl, decal, fade, heat * heat);
		}
		for decal in self.queued_decal_list.iter() {
			self.draw_decal(gl, decal, decal.options.alpha, decal.options.heat);
		}
	}

	//==============================================================================
	/// 바닥 자국 하나 그리기. (자국 셰이더와 네 귀퉁이가 묶여 있음)
	//==============================================================================
	fn draw_decal(&self, gl: &glow::Context, decal: &LiveDecal, alpha: f32, heat: f32) {
		let decal_program = &self.decal_program;
		let options = &decal.options;
		let start_radius = options.start_radius.unwrap_or(options.radius);
		let grow = if options.grow_seconds > 0.0 { 1.0 - (1.0 - (decal.age.max(0.0) / options.grow_seconds).min(1.0)).powi(2) } else { 1.0 };
		let radius = start_radius + (options.radius - start_radius) * grow;
		set_uniform_vector4(gl, decal_program, "decalShape", [options.x, options.z, radius, decal.angle]);
		set_uniform_float(gl, decal_program, "decalHeight", decal.floor_height + DECAL_LIFT);
		set_uniform_float(gl, decal_program, "decalSlot", options.slot.get_index() as f32);
		set_uniform_float(gl, decal_program, "decalAlpha", alpha);
		set_uniform_float(gl, decal_program, "decalHeat", heat);
		set_uniform_float(gl, decal_program, "decalRamp", options.ramp as f32);
		set_uniform_vector3(gl, decal_program, "decalTint", options.tint);
		set_uniform_float(gl, decal_program, "decalMode", options.mode.get_index());
		set_uniform_vector4(gl, decal_program, "decalRing", options.ring);
		set_uniform_float(gl, decal_program, "decalSeed", decal.seed);
		unsafe {
			gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
		}
	}

	//==============================================================================
	/// 충격파 띠 그리기. (바닥 빛을 먼저 납작하게, 그 위에 말려 오르는 띠)
	//==============================================================================
	fn draw_shockwaves(&self, gl: &glow::Context, frame: &VfxFrame) {
		if self.shockwave_list.is_empty() {
			return;
		}
		let band_program = &self.band_program;
		ToonVfx::use_program(gl, band_program, frame);
		set_uniform_vector3(gl, band_program, "rimColor", self.rim_color);
		unsafe {
			gl.bind_vertex_array(Some(self.band_mesh.vertex_array));
		}
		for shockwave in self.shockwave_list.iter() {
			let options = &shockwave.options;
			let active_age = shockwave.age - options.delay;
			if active_age < 0.0 {
				continue;
			}
			let progress = (active_age / options.life.max(0.001)).min(1.0);
			let spread = 1.0 - (1.0 - progress).powi(3);
			let radius = options.start_radius + (options.end_radius - options.start_radius) * spread;
			let width = radius.min(options.width * (1.0 - 0.45 * progress));
			let alpha = (1.0 - progress).powf(1.3);
			set_uniform_float(gl, band_program, "bandProgress", progress);
			set_uniform_float(gl, band_program, "bandRepeat", (options.end_radius * 4.0).round().max(6.0));
			set_uniform_float(gl, band_program, "bandRamp", options.ramp as f32);
			set_uniform_vector3(gl, band_program, "bandColor", options.color);
			set_uniform_float(gl, band_program, "bandSeed", shockwave.seed);
			set_uniform_float(gl, band_program, "bandAlpha", alpha);
			set_uniform_float(gl, band_program, "bandRim", options.rim);
			// 바닥 빛. (띠보다 넓게, 납작하게)
			set_uniform_float(gl, band_program, "bandFlat", 1.0);
			set_uniform_vector4(gl, band_program, "bandShape", [options.x, options.z, radius, radius.min(width * 2.6)]);
			set_uniform_vector2(gl, band_program, "bandLift", 0.0, shockwave.floor_height + GROUND_GLOW_LIFT);
			ToonVfx::draw_indexed_mesh(gl, &self.band_mesh);
			// 말려 오르는 띠.
			set_uniform_float(gl, band_program, "bandFlat", 0.0);
			set_uniform_vector4(gl, band_program, "bandShape", [options.x, options.z, radius, width]);
			set_uniform_vector2(gl, band_program, "bandLift", options.lift * (1.0 - progress * 0.5), shockwave.floor_height + options.height);
			ToonVfx::draw_indexed_mesh(gl, &self.band_mesh);
		}
	}

	//==============================================================================
	/// 연기 기둥 그리기. (빠르게 솟아 퍼지고, 열기는 앞 칠 할에 식고, 끝 사 할에 엷어짐)
	//==============================================================================
	fn draw_plumes(&self, gl: &glow::Context, frame: &VfxFrame) {
		if self.plume_list.is_empty() {
			return;
		}
		let plume_program = &self.plume_program;
		ToonVfx::use_program(gl, plume_program, frame);
		let view_direction = frame.view_direction;
		set_uniform_vector4(gl, plume_program, "cameraPoint", [view_direction[0], view_direction[1], view_direction[2], 0.0]);
		unsafe {
			gl.bind_vertex_array(Some(self.lathe_mesh.vertex_array));
		}
		for plume in self.plume_list.iter() {
			if plume.age < 0.0 {
				continue;
			}
			let options = &plume.options;
			let progress = (plume.age / options.life).min(1.0);
			let alpha = smooth_step(0.0, 0.08, progress) * (1.0 - smooth_step(0.6, 1.0, progress));
			let heat = (1.0 - progress * 1.4).max(0.0);
			set_uniform_vector4(gl, plume_program, "plumeShape", [options.x, options.z, options.height, plume.cap_radius]);
			set_uniform_vector4(gl, plume_program, "plumeMotion", [progress, plume.age, plume.seed, plume.stem_radius]);
			set_uniform_float(gl, plume_program, "plumeBase", plume.floor_height);
			set_uniform_vector4(gl, plume_program, "plumeLook", [alpha, heat, options.ramp as f32, plume.age]);
			set_uniform_vector3(gl, plume_program, "smokeColor", options.smoke_color);
			ToonVfx::draw_indexed_mesh(gl, &self.lathe_mesh);
		}
	}

	//==============================================================================
	/// 불덩이 그리기. (부풀기는 빠르게 시작해 느려지고, 열기는 식고, 끝에 연기로 걷힘)
	//==============================================================================
	fn draw_fireballs(&self, gl: &glow::Context, frame: &VfxFrame) {
		if self.fireball_list.is_empty() {
			return;
		}
		let fireball_program = &self.fireball_program;
		ToonVfx::use_program(gl, fireball_program, frame);
		set_uniform_vector3(gl, fireball_program, "viewDirection", frame.view_direction);
		set_uniform_vector3(gl, fireball_program, "rimColor", self.rim_color);
		unsafe {
			gl.bind_vertex_array(Some(self.sphere_mesh.vertex_array));
		}
		for fireball in self.fireball_list.iter() {
			if fireball.age < 0.0 {
				continue;
			}
			let options = &fireball.options;
			let progress = (fireball.age / options.life).min(1.0);
			let grow = 1.0 - (1.0 - (fireball.age / options.grow_seconds).min(1.0)).powi(3);
			let radius = options.radius * (0.25 + 0.75 * grow) * (1.0 + 0.25 * progress);
			let heat = (1.0 - progress.powf(1.4) * 1.25).max(0.0);
			let alpha = 1.0 - smooth_step(0.55, 1.0, progress);
			set_uniform_vector4(gl, fireball_program, "ballCenter", [options.position[0], options.position[1] + options.rise * progress, options.position[2], radius]);
			set_uniform_float(gl, fireball_program, "ballTime", fireball.age);
			set_uniform_float(gl, fireball_program, "ballSeed", fireball.seed);
			set_uniform_float(gl, fireball_program, "ballBoil", options.boil);
			set_uniform_float(gl, fireball_program, "ballHeat", heat);
			set_uniform_float(gl, fireball_program, "ballAlpha", alpha);
			set_uniform_float(gl, fireball_program, "ballRamp", options.ramp as f32);
			set_uniform_vector3(gl, fireball_program, "smokeColor", options.smoke_color);
			set_uniform_float(gl, fireball_program, "ballRim", options.rim);
			ToonVfx::draw_indexed_mesh(gl, &self.sphere_mesh);
		}
	}

	//==============================================================================
	/// 회전체 빔 그리기. (넘긴 차례대로, 축과 두 옆 쪽은 여기서 셈)
	//==============================================================================
	fn draw_beams(&self, gl: &glow::Context, frame: &VfxFrame) {
		if self.queued_beam_list.is_empty() {
			return;
		}
		let beam_program = &self.beam_program;
		ToonVfx::use_program(gl, beam_program, frame);
		set_uniform_vector4(gl, beam_program, "cameraPoint", frame.camera_point);
		unsafe {
			gl.bind_vertex_array(Some(self.lathe_mesh.vertex_array));
		}
		for beam in self.queued_beam_list.iter() {
			let start = beam.start;
			let axis = [beam.end[0] - start[0], beam.end[1] - start[1], beam.end[2] - start[2]];
			let beam_length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
			let sides = read_axis_sides(axis);
			set_uniform_vector3(gl, beam_program, "beamStart", start);
			set_uniform_vector3(gl, beam_program, "beamAxis", axis);
			set_uniform_vector3(gl, beam_program, "beamSideA", sides[0]);
			set_uniform_vector3(gl, beam_program, "beamSideB", sides[1]);
			set_uniform_vector4(gl, beam_program, "beamShape", [beam.radius, beam.wobble, beam.time.unwrap_or(self.time), beam.seed]);
			set_uniform_vector4(gl, beam_program, "beamFlow", [beam_length, beam.flow_speed, beam.grain_repeat, 0.0]);
			set_uniform_vector4(gl, beam_program, "beamLook", [if beam.is_core { 0.0 } else { 1.0 }, beam.alpha, beam.ramp as f32, beam.brightness]);
			ToonVfx::draw_indexed_mesh(gl, &self.lathe_mesh);
		}
	}

	//==============================================================================
	/// 리본 그리기. (점마다 앞뒤 점으로 잡은 쪽과 카메라 쪽에 수직으로 펴서 한 버퍼에 쓰고, 리본마다 띠로 그림)
	//==============================================================================
	fn draw_ribbons(&mut self, gl: &glow::Context, frame: &VfxFrame) {
		if self.queued_ribbon_list.is_empty() {
			return;
		}
		let camera_point = frame.camera_point;
		let mut ribbon_data = std::mem::take(&mut self.ribbon_data);
		ribbon_data.clear();
		let mut ribbon_length_list = Vec::with_capacity(self.queued_ribbon_list.len());
		for ribbon in self.queued_ribbon_list.iter() {
			let points = &ribbon.points;
			let last_index = points.len() - 1;
			let mut ribbon_length = 0.0;
			for point_index in 0..=last_index {
				let point = points[point_index];
				let previous = points[point_index.saturating_sub(1)];
				let next = points[(point_index + 1).min(last_index)];
				if point_index > 0 {
					ribbon_length += ((point[0] - previous[0]).powi(2) + (point[1] - previous[1]).powi(2) + (point[2] - previous[2]).powi(2)).sqrt();
				}
				let tangent = [next[0] - previous[0], next[1] - previous[1], next[2] - previous[2]];
				let mut toward = [camera_point[0], camera_point[1], camera_point[2]];
				if camera_point[3] > 0.5 {
					toward = [toward[0] - point[0], toward[1] - point[1], toward[2] - point[2]];
				}
				let mut side = [tangent[1] * toward[2] - tangent[2] * toward[1], tangent[2] * toward[0] - tangent[0] * toward[2], tangent[0] * toward[1] - tangent[1] * toward[0]];
				let side_length = (side[0] * side[0] + side[1] * side[1] + side[2] * side[2]).sqrt().max(0.000001);
				let along = point_index as f32 / last_index as f32;
				let edge = 2.0 * along - 1.0;
				let half_width = ribbon.width * 0.5 * (1.0 - ribbon.taper * edge * edge);
				for value in side.iter_mut() {
					*value *= half_width / side_length;
				}
				for (sign, across) in [(1.0f32, 1.0f32), (-1.0, 0.0)] {
					ribbon_data.extend_from_slice(&[point[0] + side[0] * sign, point[1] + side[1] * sign, point[2] + side[2] * sign, along, across]);
				}
			}
			ribbon_length_list.push(ribbon_length);
		}
		let ribbon_program = &self.ribbon_program;
		ToonVfx::use_program(gl, ribbon_program, frame);
		set_uniform_vector3(gl, ribbon_program, "rimColor", self.rim_color);
		unsafe {
			gl.bind_vertex_array(Some(self.ribbon_vertex_array));
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.ribbon_buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&ribbon_data), glow::DYNAMIC_DRAW);
		}
		let mut first_vertex = 0;
		for (ribbon, ribbon_length) in self.queued_ribbon_list.iter().zip(ribbon_length_list.iter()) {
			let vertex_count = (ribbon.points.len() * 2) as i32;
			set_uniform_vector3(gl, ribbon_program, "ribbonColor", ribbon.color);
			set_uniform_vector4(gl, ribbon_program, "ribbonLook", [ribbon.ramp as f32, ribbon.alpha, ribbon.brightness, ribbon.flow.unwrap_or(self.time)]);
			set_uniform_vector2(gl, ribbon_program, "ribbonGrain", ribbon_length * ribbon.grain_repeat, ribbon.rim);
			let glow_value = match ribbon.glow {
				Some(glow) => {
					[glow[0], glow[1], glow[2], 1.0]
				}
				None => {
					[0.0; 4]
				}
			};
			set_uniform_vector4(gl, ribbon_program, "ribbonGlow", glow_value);
			unsafe {
				gl.draw_arrays(glow::TRIANGLE_STRIP, first_vertex, vertex_count);
			}
			first_vertex += vertex_count;
		}
		self.ribbon_data = ribbon_data;
	}

	//==============================================================================
	/// 파티클 그리기. (새로 낸 것이 있으면 쓴 칸까지 버퍼를 다시 올림, 사라진 것은 정점 셰이더가 접음, 이번 프레임의 빛 점도 같은 셰이더로)
	//==============================================================================
	fn draw_particles(&mut self, gl: &glow::Context, frame: &VfxFrame, is_particle_live: bool) {
		let glint_count = self.queued_glint_count;
		if !is_particle_live && glint_count == 0 {
			return;
		}
		let particle_program = &self.particle_program;
		ToonVfx::use_program(gl, particle_program, frame);
		set_uniform_vector3(gl, particle_program, "viewDirection", frame.view_direction);
		set_uniform_vector3(gl, particle_program, "cameraRight", frame.camera_right);
		set_uniform_vector3(gl, particle_program, "cameraUp", frame.camera_up);
		set_uniform_float(gl, particle_program, "time", self.time);
		set_uniform_vector3(gl, particle_program, "rimColor", self.rim_color);
		unsafe {
			gl.uniform_1_f32_slice(particle_program.get_uniform_location(gl, "lookGains").as_ref(), &self.look_gain_list);
			if is_particle_live {
				gl.bind_vertex_array(Some(self.particle_vertex_array));
				let used_count = self.particle_used_count;
				if self.is_particle_dirty {
					gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.particle_buffer));
					gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&self.particle_data[..used_count * PARTICLE_FLOAT_COUNT]), glow::DYNAMIC_DRAW);
					self.is_particle_dirty = false;
				}
				gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, used_count as i32);
			}
			if glint_count > 0 {
				gl.bind_vertex_array(Some(self.glint_vertex_array));
				gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.glint_buffer));
				gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, float_slice_to_bytes(&self.glint_data[..glint_count * PARTICLE_FLOAT_COUNT]), glow::DYNAMIC_DRAW);
				gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, glint_count as i32);
			}
		}
	}

	//==============================================================================
	/// 섬광 그리기. (뷰포트 전체의 네 귀퉁이를 깊이 시험 없이, 섬광마다 처음이 세고 나이로 빠르게 걷힘)
	//==============================================================================
	fn draw_flashes(&self, gl: &glow::Context) {
		if self.flash_list.is_empty() {
			return;
		}
		let flash_program = &self.flash_program;
		flash_program.use_program(gl);
		unsafe {
			gl.disable(glow::DEPTH_TEST);
			gl.bind_vertex_array(Some(self.decal_vertex_array));
		}
		for flash in self.flash_list.iter() {
			if flash.age < 0.0 {
				continue;
			}
			let fade = 1.0 - flash.age / flash.life;
			set_uniform_vector4(gl, flash_program, "flashColor", [flash.color[0], flash.color[1], flash.color[2], flash.strength * fade * fade]);
			unsafe {
				gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
			}
		}
		unsafe {
			gl.enable(glow::DEPTH_TEST);
		}
	}

	//==============================================================================
	/// 다 쓴 뒤 지우기.
	///
	/// # 인자
	/// - `gl`: 만들 때 쓴 GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		unsafe {
			for buffer in self.buffer_list.drain(..) {
				gl.delete_buffer(buffer);
			}
			for vertex_array in [self.particle_vertex_array, self.glint_vertex_array, self.ribbon_vertex_array, self.decal_vertex_array, self.sphere_mesh.vertex_array, self.band_mesh.vertex_array, self.lathe_mesh.vertex_array] {
				gl.delete_vertex_array(vertex_array);
			}
			for shader_program in [&self.particle_program, &self.fireball_program, &self.band_program, &self.decal_program, &self.beam_program, &self.plume_program, &self.ribbon_program, &self.flash_program] {
				gl.delete_program(shader_program.get_program());
			}
			if let Some(atlas_texture) = self.atlas_texture.take() {
				gl.delete_texture(atlas_texture);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 효과 값의 기본이 예전 화면과 같은지. (나이 0, 열쇠 없음, 테 0, 깜빡임 0, 발밑 그늘 기본 짙기, 그늘 꼴 번호 5)
	//==============================================================================
	#[test]
	fn options_default_keep_previous_behavior() {
		let particle_options = ToonParticleOptions::default();
		assert_eq!(particle_options.age, 0.0);
		assert!(particle_options.key.is_none());
		assert_eq!(ToonFireballOptions::default().rim, 0.0);
		assert_eq!(ToonShockwaveOptions::default().rim, 0.0);
		assert_eq!(ToonRibbonOptions::default().rim, 0.0);
		assert_eq!(ToonDecalOptions::default().flicker, 0.0);
		assert_eq!(ToonBlobShadowOptions::default().alpha, DEFAULT_BLOB_SHADOW_ALPHA);
		assert_eq!(ToonVfxDecalMode::Shade.get_index(), 5.0);
	}

	//==============================================================================
	/// 잇는 값 잡음이 주기마다 이어 붙는지. (한 주기 옮긴 자리의 값이 같고 0 ~ 1 안)
	//==============================================================================
	#[test]
	fn grid_noise_tiles_at_period() {
		let grid = create_noise_grid(7);
		let first = read_grid_fbm(&grid, 0.3, 0.6, 12, 256, 4);
		let wrapped = read_grid_fbm(&grid, 12.3, 0.6, 12, 256, 4);
		assert!((first - wrapped).abs() < 0.0001);
		assert!((0.0..=1.0).contains(&first));
	}

	//==============================================================================
	/// 축에 수직인 두 쪽이 길이 1 이고 서로와 축에 수직인지.
	//==============================================================================
	#[test]
	fn axis_sides_are_unit_and_perpendicular() {
		let sides = read_axis_sides([0.3, 2.0, -0.5]);
		let dot = |first: [f32; 3], second: [f32; 3]| first[0] * second[0] + first[1] * second[1] + first[2] * second[2];
		assert!((dot(sides[0], sides[0]) - 1.0).abs() < 0.0001);
		assert!((dot(sides[1], sides[1]) - 1.0).abs() < 0.0001);
		assert!(dot(sides[0], sides[1]).abs() < 0.0001);
		assert!(dot(sides[0], [0.3, 2.0, -0.5]).abs() < 0.0001);
	}

	//==============================================================================
	/// 색 램프 칸이 줄마다 색을 잇고 모자란 줄은 마지막 줄을 되풀이하는지.
	//==============================================================================
	#[test]
	fn ramps_fill_rows_and_repeat_last() {
		let mut pixels = vec![0u8; ATLAS_WIDTH * ATLAS_HEIGHT * 4];
		paint_ramps(&mut pixels, &[vec![[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], vec![[0.0, 1.0, 0.0]]]);
		let read = |row: usize, column: usize| {
			let origin_x = ATLAS_WIDTH / 2 + (ToonVfxSlot::Ramps.get_index() % 4) * ATLAS_SLOT_SIZE;
			let origin_y = (ToonVfxSlot::Ramps.get_index() / 4) * ATLAS_SLOT_SIZE;
			let offset = ((origin_y + row) * ATLAS_WIDTH + origin_x + column) * 4;
			[pixels[offset], pixels[offset + 1], pixels[offset + 2]]
		};
		assert!(read(2, 0)[0] > 250);
		assert!(read(2, 127)[2] > 250);
		assert_eq!(read(40, 64), [0, 255, 0]);
		assert_eq!(read(120, 3), [0, 255, 0]);
	}
}

/// 셰이더 공통: 3 차원 잇는 값 잡음과 네 겹 잡음.
const NOISE_SHADER_CHUNK: &str = "
float hashPoint(vec3 point) {
	return fract(sin(dot(point, vec3(127.1, 311.7, 74.7))) * 43758.5453);
}
float readValueNoise(vec3 point) {
	vec3 cell = floor(point);
	vec3 offset = fract(point);
	vec3 weight = offset * offset * (3.0 - 2.0 * offset);
	float bottomFront = mix(hashPoint(cell), hashPoint(cell + vec3(1.0, 0.0, 0.0)), weight.x);
	float bottomBack = mix(hashPoint(cell + vec3(0.0, 0.0, 1.0)), hashPoint(cell + vec3(1.0, 0.0, 1.0)), weight.x);
	float topFront = mix(hashPoint(cell + vec3(0.0, 1.0, 0.0)), hashPoint(cell + vec3(1.0, 1.0, 0.0)), weight.x);
	float topBack = mix(hashPoint(cell + vec3(0.0, 1.0, 1.0)), hashPoint(cell + vec3(1.0, 1.0, 1.0)), weight.x);
	return mix(mix(bottomFront, bottomBack, weight.z), mix(topFront, topBack, weight.z), weight.y);
}
float readFbm(vec3 point) {
	float sum = 0.0;
	float amplitude = 0.5;
	for (int octave = 0; octave < 4; ++octave) {
		sum += amplitude * readValueNoise(point);
		point = point * 2.07 + vec3(13.1, 7.7, 3.3);
		amplitude *= 0.5;
	}
	return sum / 0.9375;
}
";

/// 셰이더 공통: 아틀라스의 칸, 연기 플립북의 칸, 색 램프 읽기.
const ATLAS_SHADER_CHUNK: &str = "
uniform sampler2D vfxAtlas;
vec4 readSlot(float slot, vec2 coordinate) {
	vec2 inset = clamp(coordinate, vec2(0.004), vec2(0.996));
	float column = mod(slot, 4.0);
	float row = floor(slot / 4.0);
	return texture(vfxAtlas, vec2(0.5 + (column + inset.x) * 0.125, (row + inset.y) * 0.25));
}
vec4 readSmokeFrame(float frame, vec2 coordinate) {
	vec2 inset = clamp(coordinate, vec2(0.004), vec2(0.996));
	return texture(vfxAtlas, vec2((mod(frame, 4.0) + inset.x) * 0.125, (floor(frame / 4.0) + inset.y) * 0.25));
}
vec3 readRamp(float ramp, float position) {
	float row = (ramp + 0.5) / 8.0;
	return texture(vfxAtlas, vec2(0.5 + clamp(position, 0.01, 0.99) * 0.125, (2.0 + row) * 0.25)).rgb;
}
";

/// 파티클. (네 귀퉁이를 인스턴스마다 펴고, 나이로 자리와 크기를 셈, 줄기는 화면에 비친 속도 쪽으로 늘임)
const PARTICLE_VERTEX_SHADER: &str = "#version 300 es
precision highp float;
in vec2 vertexCorner;
in vec4 particleStart;
in vec4 particleVelocity;
in vec4 particleMotion;
in vec4 particleColor;
in vec4 particleLook;
in vec4 particleExtra;
in vec4 particleShape;
uniform mat4 viewProjectionMatrix;
uniform vec3 viewDirection;
uniform vec3 cameraRight;
uniform vec3 cameraUp;
uniform float time;
out vec2 cornerCoordinate;
out float particleProgress;
out vec4 tintValue;
out vec4 lookValue;
out vec4 extraValue;
out float rimValue;
void main() {
	float age = time - particleStart.w;
	float life = particleVelocity.w;
	if (age < 0.0 || age >= life) {
		gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
		return;
	}
	float progress = age / life;
	float drag = particleMotion.y;
	float travel = drag > 0.001 ? (1.0 - exp(-drag * age)) / drag : age;
	vec3 position = particleStart.xyz + particleVelocity.xyz * travel;
	position.y -= 0.5 * particleMotion.x * age * age;
	vec3 velocity = particleVelocity.xyz * exp(-drag * age);
	velocity.y -= particleMotion.x * age;
	if (position.y < particleLook.w) {
		position.y = particleLook.w;
		velocity = vec3(0.0);
	}
	float size = mix(particleMotion.z, particleMotion.w, pow(progress, particleShape.x));
	vec3 right;
	vec3 up;
	if (abs(particleLook.x - 1.0) < 0.5) {
		vec3 along = velocity - dot(velocity, viewDirection) * viewDirection;
		float speed = length(along);
		along = speed > 0.0001 ? along / speed : cameraUp;
		right = normalize(cross(viewDirection, along)) * size * 0.5;
		up = along * (size * 0.5 + speed * particleLook.y);
	}
	else {
		float angle = particleLook.z * age + particleExtra.z * 6.2832;
		float cosine = cos(angle);
		float sine = sin(angle);
		right = (cameraRight * cosine + cameraUp * sine) * size * 0.5;
		up = (cameraUp * cosine - cameraRight * sine) * size * 0.5;
	}
	vec3 worldPosition = position + right * vertexCorner.x + up * vertexCorner.y;
	gl_Position = viewProjectionMatrix * vec4(worldPosition, 1.0);
	cornerCoordinate = vertexCorner * 0.5 + 0.5;
	particleProgress = progress;
	tintValue = particleColor;
	lookValue = particleLook;
	extraValue = particleExtra;
	rimValue = particleShape.y;
}
";

/// 파티클의 칠. (더하는 빛은 알파 0, 연기와 알갱이는 미리 곱한 알파, 색 램프가 있으면 나이로 흰 심에서 램프 색으로 식음)
///   빛의 꼴(Soft, Streak, Star, Ember, Glow)은 테(rim)를 받는다. rim 이 0 이면 더하는 빛만, 0 보다 크면 꼴의 몸을 테 색(rimColor)으로 덮고 심은 색,
///   흰 심만 더하기로 겹친다(밝은 바탕에서도 보이게, 덮는 몫은 rim). 0 보다 작으면 테 없이 그 색으로 엷게 덮는다. (빛무리, 덮는 몫은 -rim)
///   밝기에는 꼴마다의 배(lookGains, set_look_gain)를 곱한다. (HDR 판에서 불꽃 줄기, 빛살, 불씨만 더 밝게 할 때)
const PARTICLE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec2 cornerCoordinate;
in float particleProgress;
in vec4 tintValue;
in vec4 lookValue;
in vec4 extraValue;
in float rimValue;
uniform vec3 rimColor;
uniform float lookGains[7];
out vec4 fragmentColor;
//#include atlas
vec4 shadeRimmed(vec3 color, float shapeAlpha, float fade, float brightness) {
	float glow = shapeAlpha * fade;
	if (rimValue < 0.0) {
		return vec4(min(color * brightness, vec3(1.0)) * glow * -rimValue + color * glow * brightness * 0.2, glow * -rimValue);
	}
	if (rimValue <= 0.0) {
		return vec4(color * glow * brightness, 0.0);
	}
	float body = smoothstep(0.03, 0.18, shapeAlpha) * fade;
	float core = smoothstep(0.3, 0.72, shapeAlpha);
	vec3 covered = mix(rimColor, min(color * brightness, vec3(1.0)), core);
	float cover = clamp(body * rimValue, 0.0, 1.0);
	return vec4(covered * cover + color * core * glow * brightness * 0.35, cover);
}
void main() {
	float look = lookValue.x;
	float progress = particleProgress;
	vec3 color = tintValue.rgb;
	if (tintValue.a >= 0.0) {
		color *= readRamp(tintValue.a, progress);
	}
	float fadeIn = extraValue.w > 0.0 ? clamp(progress / extraValue.w, 0.0, 1.0) : 1.0;
	float alpha = extraValue.y * fadeIn;
	float brightness = extraValue.x * lookGains[int(clamp(look, 0.0, 6.0) + 0.5)];
	if (look < 0.5) {
		fragmentColor = shadeRimmed(color, readSlot(0.0, cornerCoordinate).a, alpha * (1.0 - progress), brightness);
	}
	else if (look < 1.5) {
		fragmentColor = shadeRimmed(color, readSlot(1.0, cornerCoordinate).a, alpha * (1.0 - progress * progress), brightness);
	}
	else if (look < 2.5) {
		vec4 smoke = readSmokeFrame(floor(progress * 15.99), cornerCoordinate);
		float density = smoke.a * alpha * (1.0 - progress);
		fragmentColor = vec4(color * smoke.rgb * density * brightness, density);
	}
	else if (look < 3.5) {
		fragmentColor = shadeRimmed(color, readSlot(2.0, cornerCoordinate).a, alpha * (1.0 - progress), brightness);
	}
	else if (look < 4.5) {
		float flicker = 0.6 + 0.4 * sin(progress * 46.0 + extraValue.z * 60.0);
		fragmentColor = shadeRimmed(color, readSlot(0.0, cornerCoordinate).a, alpha * flicker * (1.0 - progress), brightness);
	}
	else if (look < 5.5) {
		fragmentColor = shadeRimmed(color, readSlot(10.0, cornerCoordinate).a, alpha * (1.0 - progress) * (1.0 - progress), brightness);
	}
	else {
		float body = smoothstep(0.05, 0.45, readSlot(0.0, cornerCoordinate).a);
		float density = body * alpha * (1.0 - progress * progress * progress * progress);
		fragmentColor = vec4(color * brightness * density, density);
	}
}
";

/// 불덩이 구. (단위 구의 점을 잡음으로 밀어 겉이 끓고 위로 흐름)
const FIREBALL_VERTEX_SHADER: &str = "#version 300 es
precision highp float;
in vec3 vertexPosition;
uniform mat4 viewProjectionMatrix;
uniform vec4 ballCenter;
uniform float ballTime;
uniform float ballSeed;
uniform float ballBoil;
out vec3 surfaceNormal;
out float surfaceNoise;
out vec3 noisePoint;
//#include noise
void main() {
	vec3 point = vertexPosition * 2.1 + vec3(ballSeed, ballSeed * 0.7, -ballSeed * 1.3) + vec3(0.0, -ballTime * 1.6, 0.0);
	float noiseValue = readFbm(point);
	vec3 displaced = vertexPosition * (1.0 + ballBoil * (noiseValue - 0.5) * 2.0);
	vec3 worldPosition = ballCenter.xyz + displaced * ballCenter.w;
	gl_Position = viewProjectionMatrix * vec4(worldPosition, 1.0);
	surfaceNormal = vertexPosition;
	surfaceNoise = noiseValue;
	noisePoint = vertexPosition * 4.3 + vec3(0.0, -ballTime * 2.4, ballSeed);
}
";

/// 불덩이 구의 칠. (뜨거운 곳은 색 램프로 더하는 빛, 식은 곳은 연기로 덮음, 가장자리는 0, 뒤쪽 반은 버림)
///   ballRim 이 0 보다 크면 불의 몸을 덮고 가장자리를 테 색으로 두르는 꼴과 그 몫만큼 섞는다. (밝은 바탕에서도 보이게, 1 이면 덮는 꼴만)
const FIREBALL_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec3 surfaceNormal;
in float surfaceNoise;
in vec3 noisePoint;
uniform vec3 viewDirection;
uniform float ballHeat;
uniform float ballAlpha;
uniform float ballRamp;
uniform vec3 smokeColor;
uniform float ballRim;
uniform vec3 rimColor;
out vec4 fragmentColor;
//#include noise
//#include atlas
void main() {
	vec3 normal = normalize(surfaceNormal);
	float facing = dot(normal, viewDirection);
	if (facing <= 0.0) {
		discard;
	}
	float detail = readFbm(noisePoint);
	float edge = smoothstep(0.0, 0.6, facing);
	float temperature = clamp(ballHeat * 1.4 - (1.0 - surfaceNoise) * 0.55 - detail * 0.3 + facing * 0.3, 0.0, 1.0);
	vec3 fireColor = readRamp(ballRamp, 1.0 - temperature);
	float cover = edge * smoothstep(0.0, 0.2, surfaceNoise + detail * 0.5 - (1.0 - ballAlpha) * 1.1);
	float hot = smoothstep(0.12, 0.55, temperature);
	vec3 emitted = fireColor * hot * (0.8 + temperature * 0.9);
	float smoke = (1.0 - hot) * cover * 0.8;
	fragmentColor = vec4(emitted * cover + smokeColor * smoke * (0.6 + detail * 0.4), smoke);
	if (ballRim > 0.0) {
		float fireCover = hot * cover * 0.9;
		float rimDark = 1.0 - smoothstep(0.06, 0.38, facing);
		vec3 fireBody = mix(min(fireColor * (0.85 + temperature * 0.4), vec3(1.0)), rimColor, rimDark * 0.85);
		vec4 covered = vec4(fireBody * fireCover + emitted * cover * 0.3 + smokeColor * smoke * (0.6 + detail * 0.4), max(fireCover, smoke));
		fragmentColor = mix(fragmentColor, covered, min(ballRim, 1.0));
	}
}
";

/// 충격파 띠. (둘레와 가로지름으로 지은 띠를 반지름과 너비로 펴고, 바깥 끝을 위로 말아 올림, 바닥 빛은 납작하게)
const BAND_VERTEX_SHADER: &str = "#version 300 es
precision highp float;
in vec2 vertexGrid;
uniform mat4 viewProjectionMatrix;
uniform vec4 bandShape;
uniform vec2 bandLift;
uniform float bandFlat;
out vec2 bandCoordinate;
void main() {
	float angle = vertexGrid.x * 6.2832;
	float radius = max(0.0, bandShape.z - bandShape.w * (1.0 - vertexGrid.y));
	float height = bandLift.y + (1.0 - bandFlat) * bandLift.x * sin(vertexGrid.y * 2.4);
	vec3 worldPosition = vec3(bandShape.x + cos(angle) * radius, height, bandShape.y + sin(angle) * radius);
	gl_Position = viewProjectionMatrix * vec4(worldPosition, 1.0);
	bandCoordinate = vertexGrid;
}
";

/// 충격파 띠의 칠. (충격 띠 칸, 잡음이 바깥으로 흐르며 나이만큼 찢어 먹음, 흰 앞머리에서 램프 색으로 식음)
///   bandRim 이 0 보다 크면 띠의 몸을 테 색으로 덮고 심은 색인 꼴과 그 몫만큼 섞는다. (바닥 빛은 그대로)
const BAND_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec2 bandCoordinate;
uniform float bandProgress;
uniform float bandRepeat;
uniform float bandRamp;
uniform vec3 bandColor;
uniform float bandAlpha;
uniform float bandFlat;
uniform float bandSeed;
uniform float bandRim;
uniform vec3 rimColor;
out vec4 fragmentColor;
//#include noise
//#include atlas
void main() {
	if (bandFlat > 0.5) {
		float glow = smoothstep(0.0, 0.8, bandCoordinate.y) * (1.0 - smoothstep(0.8, 1.0, bandCoordinate.y));
		vec3 glowColor = bandColor * readRamp(bandRamp, 0.2 + bandProgress * 0.5);
		fragmentColor = vec4(glowColor * glow * bandAlpha * 0.5, 0.0);
		return;
	}
	vec2 coordinate = vec2(fract(bandCoordinate.x * bandRepeat), bandCoordinate.y);
	float strength = readSlot(3.0, coordinate).a;
	float tear = readFbm(vec3(bandCoordinate.x * bandRepeat * 2.0, bandCoordinate.y * 2.5 - bandProgress * 4.0, bandSeed));
	float cut = smoothstep(bandProgress * 0.7, bandProgress * 0.7 + 0.22, tear);
	float alpha = strength * cut * bandAlpha;
	float rampPosition = clamp(0.12 + bandProgress * 0.65 + (1.0 - strength) * 0.3, 0.0, 0.95);
	vec3 color = bandColor * readRamp(bandRamp, rampPosition);
	fragmentColor = vec4(color * alpha * 1.5, alpha * 0.12);
	if (bandRim > 0.0) {
		float core = smoothstep(0.35, 0.8, strength);
		vec3 covered = mix(rimColor, min(color * 1.2, vec3(1.0)), core);
		fragmentColor = mix(fragmentColor, vec4(covered * alpha * 0.85 + color * core * alpha * 0.4, alpha * 0.85), min(bandRim, 1.0));
	}
}
";

/// 바닥 자국. (가운데, 반지름, 돎으로 편 바닥 네모)
const DECAL_VERTEX_SHADER: &str = "#version 300 es
precision highp float;
in vec2 vertexCorner;
uniform mat4 viewProjectionMatrix;
uniform vec4 decalShape;
uniform float decalHeight;
out vec2 decalCoordinate;
void main() {
	float cosine = cos(decalShape.w);
	float sine = sin(decalShape.w);
	vec2 offset = vec2(vertexCorner.x * cosine - vertexCorner.y * sine, vertexCorner.x * sine + vertexCorner.y * cosine) * decalShape.z;
	gl_Position = viewProjectionMatrix * vec4(decalShape.x + offset.x, decalHeight, decalShape.y + offset.y, 1.0);
	decalCoordinate = vertexCorner * 0.5 + 0.5;
}
";

/// 바닥 자국의 칠. (그을음은 알파로 어둡게 덮고 빨간 채널에서 열기가 식음, 빛은 더함, 테는 둘레로 흐르는 잡음 테, 물든 자국과 기름 윤은 tint 로 덮음, 그늘은 둥글게 어둡게)
const DECAL_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec2 decalCoordinate;
uniform float decalSlot;
uniform float decalAlpha;
uniform float decalHeat;
uniform float decalRamp;
uniform vec3 decalTint;
uniform float decalMode;
uniform vec4 decalRing;
uniform float decalTime;
uniform float decalSeed;
uniform vec3 rimColor;
out vec4 fragmentColor;
//#include noise
//#include atlas
void main() {
	if (decalMode < 0.5) {
		vec4 texel = readSlot(decalSlot, decalCoordinate);
		float darkness = texel.a * decalAlpha;
		vec3 glowColor = readRamp(decalRamp, 1.0 - decalHeat) * decalHeat;
		fragmentColor = vec4(glowColor * texel.r * decalAlpha * 1.8, darkness);
	}
	else if (decalMode < 1.5) {
		vec4 texel = readSlot(decalSlot, decalCoordinate);
		float alpha = texel.a * decalAlpha;
		fragmentColor = vec4(decalTint * readRamp(decalRamp, 1.0 - decalHeat) * alpha, 0.0);
	}
	else if (decalMode < 2.5) {
		vec2 centered = decalCoordinate * 2.0 - 1.0;
		float distance = length(centered);
		float offset = (distance - decalRing.x) / decalRing.y;
		float ring = exp(-offset * offset);
		vec2 around = centered / max(distance, 0.0001);
		float turn = decalTime * decalRing.w;
		vec2 forward = vec2(around.x * cos(turn) - around.y * sin(turn), around.x * sin(turn) + around.y * cos(turn));
		vec2 backward = vec2(around.x * cos(turn * 0.6) + around.y * sin(turn * 0.6), -around.x * sin(turn * 0.6) + around.y * cos(turn * 0.6));
		float flow = readFbm(vec3(forward * decalRing.z, decalSeed + decalTime * 0.4));
		float drift = readFbm(vec3(backward * decalRing.z * 2.1, decalSeed * 1.7 - decalTime * 0.3));
		float strength = clamp(0.2 + (flow * 0.7 + drift * 0.5 - 0.35) * 1.6, 0.0, 1.4);
		float alpha = ring * strength * decalAlpha;
		fragmentColor = vec4(decalTint * readRamp(decalRamp, 1.0 - decalHeat) * alpha, 0.0);
	}
	else if (decalMode < 3.5) {
		vec4 texel = readSlot(decalSlot, decalCoordinate);
		float alpha = texel.a * decalAlpha;
		float edge = smoothstep(0.15, 0.55, texel.a) * (1.0 - smoothstep(0.55, 0.95, texel.a));
		vec3 color = mix(decalTint, rimColor, edge * 0.35);
		fragmentColor = vec4(color * alpha, alpha);
	}
	else if (decalMode < 4.5) {
		vec4 texel = readSlot(decalSlot, decalCoordinate);
		float body = smoothstep(0.1, 0.28, texel.a) * decalAlpha;
		float sheen = (smoothstep(0.1, 0.15, texel.a) - smoothstep(0.17, 0.24, texel.a)) * 0.85;
		vec3 color = mix(decalTint, vec3(1.0), sheen);
		float alpha = max(body, sheen * decalAlpha);
		fragmentColor = vec4(color * alpha, alpha);
	}
	else {
		float alpha = decalAlpha * (1.0 - smoothstep(0.25, 1.0, length(decalCoordinate * 2.0 - 1.0)));
		fragmentColor = vec4(0.0, 0.0, 0.0, alpha);
	}
}
";

/// 회전체 빔. (단위 원기둥을 축과 두 옆 쪽으로 펴고, 겉을 잡음으로 일렁임)
///   beamShape 는 (반지름, 일렁임, 시각, 씨앗), beamFlow 는 (길이, 흐르는 빠르기, 결의 잘기, 비움) 이다.
const BEAM_VERTEX_SHADER: &str = "#version 300 es
precision highp float;
in vec2 vertexGrid;
uniform mat4 viewProjectionMatrix;
uniform vec3 beamStart;
uniform vec3 beamAxis;
uniform vec3 beamSideA;
uniform vec3 beamSideB;
uniform vec4 beamShape;
uniform vec4 beamFlow;
out vec2 beamCoordinate;
out vec3 beamNormal;
out vec3 worldPosition;
//#include noise
void main() {
	float angle = vertexGrid.x * 6.2832;
	vec3 around = beamSideA * cos(angle) + beamSideB * sin(angle);
	float along = vertexGrid.y;
	float wobble = beamShape.y * (readValueNoise(vec3(cos(angle) * 1.8 + beamShape.w, sin(angle) * 1.8, along * beamFlow.x * 1.3 + beamShape.z * beamFlow.y * 0.5)) - 0.5) * 2.0;
	float radius = beamShape.x * (1.0 + wobble);
	worldPosition = beamStart + beamAxis * along + around * radius;
	gl_Position = viewProjectionMatrix * vec4(worldPosition, 1.0);
	beamCoordinate = vertexGrid;
	beamNormal = around;
}
";

/// 회전체 빔의 칠. (심은 흰빛, 겉집은 색 램프로 가장자리가 0, 빔 결이 시작에서 끝 쪽으로 흐름, 둘 다 더하는 빛)
///   cameraPoint 의 w 가 1 이면 카메라 자리(원근), 0 이면 카메라 쪽(직교)이다. beamLook 은 (꼴 0 심 1 겉집, 알파, 램프, 밝기) 이다.
const BEAM_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec2 beamCoordinate;
in vec3 beamNormal;
in vec3 worldPosition;
uniform vec3 beamAxis;
uniform vec4 beamShape;
uniform vec4 beamFlow;
uniform vec4 beamLook;
uniform vec4 cameraPoint;
out vec4 fragmentColor;
//#include atlas
void main() {
	vec3 toward = cameraPoint.w > 0.5 ? normalize(cameraPoint.xyz - worldPosition) : cameraPoint.xyz;
	vec3 axisUnit = normalize(beamAxis);
	vec3 across = toward - axisUnit * dot(toward, axisUnit);
	float facing = dot(normalize(beamNormal), across / max(length(across), 0.0001));
	if (facing <= 0.0) {
		discard;
	}
	float along = beamCoordinate.y;
	float endFade = smoothstep(0.0, 0.04, along) * (1.0 - smoothstep(0.985, 1.0, along));
	vec2 grainCoordinate = vec2(fract(beamCoordinate.x * 3.0), fract(along * beamFlow.x * beamFlow.z - beamShape.z * beamFlow.y));
	float grain = readSlot(4.0, grainCoordinate).a;
	vec3 color;
	if (beamLook.x < 0.5) {
		float body = smoothstep(0.0, 0.45, facing);
		color = mix(readRamp(beamLook.z, 0.12), vec3(1.0), body) * (0.85 + 0.4 * grain) * body;
	}
	else {
		float body = pow(facing, 1.7);
		color = readRamp(beamLook.z, 0.22 + (1.0 - facing) * 0.45 + (1.0 - grain) * 0.25) * (0.3 + 0.95 * grain) * body;
	}
	fragmentColor = vec4(color * endFade * beamLook.y * beamLook.w, 0.0);
}
";

/// 연기 기둥. (윤곽 0 ~ .4 는 밑이 퍼진 기둥, .4 ~ 1 은 갓의 둥근 단면을 안쪽 아래에서 바깥을 돌아 위로, 잡음으로 흔듦)
///   plumeShape 는 (x, z, 높이, 갓 반지름), plumeMotion 은 (나아감 0 ~ 1, 시각, 씨앗, 기둥 반지름), plumeBase 는 밑의 높이다.
const PLUME_VERTEX_SHADER: &str = "#version 300 es
precision highp float;
in vec2 vertexGrid;
uniform mat4 viewProjectionMatrix;
uniform vec4 plumeShape;
uniform vec4 plumeMotion;
uniform float plumeBase;
out vec2 plumeCoordinate;
out vec3 plumeNormal;
out float plumeNoise;
//#include noise
void main() {
	float angle = vertexGrid.x * 6.2832;
	vec3 around = vec3(cos(angle), 0.0, sin(angle));
	float profile = vertexGrid.y;
	float progress = plumeMotion.x;
	float grow = 1.0 - pow(1.0 - min(1.0, progress * 1.8), 2.0);
	float height = plumeShape.z * (0.3 + 0.7 * grow);
	float capRadius = plumeShape.w * (0.4 + 0.6 * grow) * (1.0 + 0.2 * progress);
	float stemRadius = plumeMotion.w * (0.8 + 0.5 * progress);
	float capCenter = height - capRadius * 0.45;
	float radius;
	float rise;
	vec2 profileNormal;
	if (profile < 0.4) {
		float ratio = profile / 0.4;
		rise = ratio * capCenter;
		radius = mix(stemRadius * 2.2, stemRadius, smoothstep(0.0, 0.3, ratio));
		profileNormal = vec2(1.0, 0.0);
	}
	else {
		float ratio = (profile - 0.4) / 0.6;
		float turn = mix(-1.9, 2.6, ratio);
		float tube = capRadius * 0.5;
		radius = max(0.0, capRadius * 0.55 + cos(turn) * tube);
		rise = capCenter + sin(turn) * tube * 0.85;
		profileNormal = vec2(cos(turn), sin(turn));
	}
	float noiseValue = readFbm(vec3(around.xz * 1.7 + plumeMotion.z, profile * 4.0 - plumeMotion.y * 1.1));
	radius *= 1.0 + (noiseValue - 0.5) * 0.55;
	rise += (noiseValue - 0.5) * 0.25 * capRadius;
	vec3 worldPosition = vec3(plumeShape.x, plumeBase, plumeShape.y) + around * radius + vec3(0.0, rise, 0.0);
	gl_Position = viewProjectionMatrix * vec4(worldPosition, 1.0);
	plumeCoordinate = vertexGrid;
	plumeNormal = normalize(around * profileNormal.x + vec3(0.0, profileNormal.y, 0.0));
	plumeNoise = noiseValue;
}
";

/// 연기 기둥의 칠. (연기 플립북이 윤곽을 따라 말려 흐르고, 처음에는 갓 밑이 색 램프로 달아올랐다 식음, 미리 곱한 알파)
///   plumeLook 은 (알파, 열기, 램프, 시각) 이다.
const PLUME_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec2 plumeCoordinate;
in vec3 plumeNormal;
in float plumeNoise;
uniform vec4 cameraPoint;
uniform vec4 plumeLook;
uniform vec4 plumeMotion;
uniform vec3 smokeColor;
out vec4 fragmentColor;
//#include atlas
void main() {
	vec3 normal = normalize(plumeNormal);
	float facing = dot(normal, cameraPoint.xyz);
	if (facing <= 0.0) {
		discard;
	}
	float frame = floor(clamp(plumeMotion.x, 0.0, 1.0) * 10.0);
	vec2 smokeCoordinate = vec2(fract(plumeCoordinate.x * 5.0), fract(plumeCoordinate.y * 3.0 - plumeLook.w * 0.45));
	vec4 smoke = readSmokeFrame(frame, smokeCoordinate);
	float density = smoothstep(0.0, 0.55, facing) * clamp(0.35 + smoke.a * 0.9 + (plumeNoise - 0.5) * 0.6, 0.0, 1.0) * plumeLook.x;
	float shade = 0.55 + 0.45 * clamp(normal.y * 0.6 + 0.5, 0.0, 1.0);
	float underside = 1.0 - smoothstep(-0.2, 0.5, normal.y);
	float heat = plumeLook.y * clamp(smoke.a * 1.2 - 0.25 + underside * 0.5 + (plumeNoise - 0.5), 0.0, 1.0);
	vec3 glowColor = readRamp(plumeLook.z, 1.0 - heat) * heat * 1.6;
	fragmentColor = vec4((smokeColor * shade * smoke.rgb + glowColor) * density, density);
}
";

/// 리본. (정점은 미리 카메라 쪽으로 편 자리, 길이 쪽과 가로 값만 넘김)
const RIBBON_VERTEX_SHADER: &str = "#version 300 es
precision highp float;
in vec3 vertexPosition;
in vec2 vertexRibbon;
uniform mat4 viewProjectionMatrix;
out vec2 ribbonCoordinate;
void main() {
	gl_Position = viewProjectionMatrix * vec4(vertexPosition, 1.0);
	ribbonCoordinate = vertexRibbon;
}
";

/// 리본의 칠. (가로로 밝은 심과 엷은 둘레, 빔 결이 길이 쪽으로 흐름, 램프가 있으면 가장자리로 갈수록 램프 색, 더하는 빛)
///   ribbonLook 은 (램프, 알파, 밝기, 결의 자리), ribbonGrain 은 (결의 잘기, 덮는 꼴의 몫) 이다. 덮는 꼴은 몸을 테 색으로 덮고 심은 색인 꼴이다. (번짐 색이 없을 때)
///   ribbonGlow 의 a 가 1 이면 번짐 색: 가운데 흰 심, 그 둘레를 ribbonGlow.rgb 로, 가장자리를 테 색(rimColor)으로 겹쳐 덮는다. (밝은 바탕에서도 보이게)
const RIBBON_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec2 ribbonCoordinate;
uniform vec3 ribbonColor;
uniform vec4 ribbonLook;
uniform vec2 ribbonGrain;
uniform vec4 ribbonGlow;
uniform vec3 rimColor;
out vec4 fragmentColor;
//#include atlas
void main() {
	float across = abs(ribbonCoordinate.y * 2.0 - 1.0);
	float body = exp(-across * across * 7.0) * 0.75 + exp(-across * across * 50.0) * 0.9;
	float grain = readSlot(4.0, vec2(0.3 + ribbonCoordinate.y * 0.4, fract(ribbonCoordinate.x * ribbonGrain.x - ribbonLook.w))).a;
	if (ribbonGlow.a > 0.5) {
		float white = 1.0 - smoothstep(0.24, 0.36, across);
		float glowZone = 1.0 - smoothstep(0.58, 0.78, across);
		float edge = 1.0 - smoothstep(0.82, 1.0, across);
		float layered = clamp(edge * (0.7 + 0.3 * grain) * ribbonLook.y, 0.0, 1.0);
		vec3 layeredColor = mix(rimColor, ribbonGlow.rgb, glowZone);
		layeredColor = mix(layeredColor, vec3(1.0), white);
		fragmentColor = vec4(layeredColor * layered + vec3(white * layered * 0.35 * ribbonLook.z), layered);
		return;
	}
	vec3 color = ribbonColor;
	if (ribbonLook.x >= 0.0) {
		color *= readRamp(ribbonLook.x, across * 0.55 + (1.0 - grain) * 0.3);
	}
	float alpha = body * (0.45 + 0.75 * grain) * ribbonLook.y;
	fragmentColor = vec4(color * alpha * ribbonLook.z, 0.0);
	if (ribbonGrain.y > 0.0) {
		float coverBody = exp(-across * across * 7.0);
		float core = exp(-across * across * 50.0);
		float cover = clamp(coverBody * (0.55 + 0.6 * grain) * ribbonLook.y, 0.0, 1.0);
		vec3 covered = mix(rimColor, min(color * ribbonLook.z, vec3(1.0)), smoothstep(0.15, 0.6, core));
		fragmentColor = mix(fragmentColor, vec4(covered * cover + color * core * cover * 0.35 * ribbonLook.z, cover), min(ribbonGrain.y, 1.0));
	}
}
";

/// 화면 섬광. (네 귀퉁이가 곧 뷰포트, 그 색으로 덮음, 미리 곱한 알파)
const FLASH_VERTEX_SHADER: &str = "#version 300 es
precision highp float;
in vec2 vertexCorner;
void main() {
	gl_Position = vec4(vertexCorner, 0.0, 1.0);
}
";

/// 화면 섬광의 칠. (flashColor 의 알파를 미리 곱해 덮음)
const FLASH_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
uniform vec4 flashColor;
out vec4 fragmentColor;
void main() {
	fragmentColor = vec4(flashColor.rgb * flashColor.a, flashColor.a);
}
";

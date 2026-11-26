//==============================================================================
//! 셰이더 스프라이트 컴포넌트.
//! - 원본 effect/shadersprite.js 를 옮긴 것이다. Sprite 와 같지만 프래그먼트 셰이더 효과(디졸브 / 섬광 / 외곽선 / 홀로그램 / 물결 / 모자이크 / 글리치 / 광택 / 실루엣 / 무채색 / 색상 회전 / 와이프 / 아이리스 / 색수차)를 건다.
//! - 효과의 시간은 스스로 흐르고(tick), 진행도(progress)는 트윈 / 타임라인이 밀어 넣는다.
//! - 효과 이름, 기본 파라미터, 기본 색, GLSL 본문은 원본 표를 그대로 옮겼다. 셰이더는 GLSL ES 3.00 이고, 데스크탑은 ShaderProgram 이 머리만 #version 330 core 로 바꾼다.
//! - 그리기: 효과가 있으면 Graphic.set_shader_program_override 로 효과 프로그램을 바꿔 끼우고 효과 유니폼을 올린 뒤 Sprite 출력 경로를 그대로 탄다.
//!   바꿔 끼운 동안은 묶어 그리지 않는다. (효과 없는 ShaderSprite 는 Sprite 와 같이 묶인다)
//! - 사용:
//!   ```text
//!     let shader_sprite = node.add_component(ShaderSprite::create_with_image(image));
//!     shader_sprite.set_effect(ShaderSpriteEffect::Dissolve);
//!     shader_sprite.set_effect_progress(0.5);
//!   ```
//! - 원본과 다른 점:
//!   - 원본은 Sprite 를 상속한다. 러스트 판은 Sprite 를 품고 Deref / DerefMut 로 Sprite 함수(set_image, set_image_rect, set_color ...)를 그대로 부르게 한다.
//!     Component::get_base_component 로 품은 Sprite 를 알려 `get_component::<Sprite>()` 로도 찾힌다. (원본 instanceof 와 같다)
//!   - 효과는 문자열 대신 ShaderSpriteEffect 열거형이다. (원본 문자열 이름은 get_name / from_name)
//!   - 효과 프로그램 캐시는 원본의 컨텍스트별 WeakMap 대신 Graphic.resolve_shader_program 이 든다.
//! - 프로그램 만들기에 실패하면 두 엔진 모두 한 번 알리고 효과 없이 Sprite 로 그린다. (한 효과 실패로 프레임이 멈추지 않는다)
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::rect::Rect;
use crate::core::component::sprite::Sprite;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::image::Image;
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;
use crate::core::shaderprogram::ShaderProgram;
use glow::HasContext;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

//==============================================================================
/// 셰이더 스프라이트 효과 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderSpriteEffect {
	/// 효과 없음. (Sprite 와 같이 그리고 효과 없는 것끼리 묶어 그린다)
	None,
	// --- 색 ---
	/// 실루엣을 단색으로 덮음. (피격 섬광: progress = 세기, color = 덮을 색)
	Flash,         // 실루엣을 단색으로 덮음. (피격 섬광: progress = 세기, color = 덮을 색)
	/// 실루엣 단색. (progress = 세기, color = 색: 그림자 / 잔상)
	Silhouette,    // 실루엣 단색. (progress = 세기, color = 색: 그림자 / 잔상)
	/// 곱셈 틴트. (progress = 세기, color = 곱할 색)
	Colorize,      // 곱셈 틴트. (progress = 세기, color = 곱할 색)
	/// 무채색. (progress = 세기, color = 틴트)
	Grayscale,     // 무채색. (progress = 세기, color = 틴트)
	/// 세피아.
	Sepia,         // 세피아.
	/// 색 반전.
	Invert,        // 색 반전.
	/// 색상 회전. (progress 0 → 1 = 0 → 360도)
	HueShift,      // 색상 회전. (progress 0 → 1 = 0 → 360도)
	/// 대비 / 채도 / 밝기. (parameters = `[대비, 채도, 밝기]`, progress = 적용 비율)
	ColorAdjust,   // 대비 / 채도 / 밝기. (parameters = [대비, 채도, 밝기], progress = 적용 비율)
	/// 포스터화. (parameters = `[단계 수]`)
	Posterize,     // 포스터화. (parameters = [단계 수])
	/// 흑백 문턱. (parameters = `[문턱]`)
	Threshold,     // 흑백 문턱. (parameters = [문턱])
	/// 밝기를 두 색 사이로 매핑. (parameters.xyz = 어두운 색, color = 밝은 색)
	GradientMap,   // 밝기를 두 색 사이로 매핑. (parameters.xyz = 어두운 색, color = 밝은 색)
	// --- 외곽 ---
	/// 실루엣 바깥 외곽선. (color = 선 색, parameters = `[두께 픽셀]`)
	Outline,       // 실루엣 바깥 외곽선. (color = 선 색, parameters = [두께 픽셀])
	/// 실루엣 안쪽 외곽선. (color = 선 색, parameters = `[두께 픽셀]`)
	InnerOutline,  // 실루엣 안쪽 외곽선. (color = 선 색, parameters = [두께 픽셀])
	/// 바깥 발광. (color = 빛 색, parameters = `[반지름 픽셀]`)
	Glow,          // 바깥 발광. (color = 빛 색, parameters = [반지름 픽셀])
	/// 안쪽 발광. (color = 빛 색, parameters = `[반지름 픽셀]`)
	InnerGlow,     // 안쪽 발광. (color = 빛 색, parameters = [반지름 픽셀])
	/// 드롭 섀도. (color = 그림자 색, parameters = `[오프셋 x 픽셀, 오프셋 y 픽셀]`)
	Shadow,        // 드롭 섀도. (color = 그림자 색, parameters = [오프셋 x 픽셀, 오프셋 y 픽셀])
	/// 윤곽 검출. (color = 선 색)
	EdgeDetect,    // 윤곽 검출. (color = 선 색)
	/// 엠보스.
	Emboss,        // 엠보스.
	/// 샤픈.
	Sharpen,       // 샤픈.
	// --- 흐림 ---
	/// 가우시안 블러. (parameters = `[반지름 픽셀]`)
	Blur,          // 가우시안 블러. (parameters = [반지름 픽셀])
	/// 방향 블러. (parameters = `[방향 x, 방향 y, 길이 픽셀]`)
	MotionBlur,    // 방향 블러. (parameters = [방향 x, 방향 y, 길이 픽셀])
	/// 중심 방사 블러. (parameters = `[길이 0 ~ 1]`)
	RadialBlur,    // 중심 방사 블러. (parameters = [길이 0 ~ 1])
	// --- 왜곡 ---
	/// 가로 물결 왜곡. (parameters = `[빈도, 속도, 진폭]`)
	Wave,          // 가로 물결 왜곡. (parameters = [빈도, 속도, 진폭])
	/// 깃발 펄럭임. (parameters = `[빈도, 속도, 진폭]`)
	Flag,          // 깃발 펄럭임. (parameters = [빈도, 속도, 진폭])
	/// 중심 파문. (parameters = `[빈도, 속도, 진폭]`)
	Ripple,        // 중심 파문. (parameters = [빈도, 속도, 진폭])
	/// 아지랑이. (parameters = `[노이즈 배율, 속도, 진폭]`)
	HeatHaze,      // 아지랑이. (parameters = [노이즈 배율, 속도, 진폭])
	/// 볼록 / 오목. (parameters = `[세기: 음수면 오목]`)
	Bulge,         // 볼록 / 오목. (parameters = [세기: 음수면 오목])
	/// 소용돌이. (parameters = `[회전 수]`)
	Swirl,         // 소용돌이. (parameters = [회전 수])
	/// 좌우 대칭. (parameters = `[0 가로 / 1 세로]`)
	Mirror,        // 좌우 대칭. (parameters = [0 가로 / 1 세로])
	/// 만화경. (parameters = `[조각 수]`)
	Kaleidoscope,  // 만화경. (parameters = [조각 수])
	/// 흔들림. (parameters = `[진폭 픽셀, 초당 변화]`)
	Jitter,        // 흔들림. (parameters = [진폭 픽셀, 초당 변화])
	/// 모자이크. (parameters = `[가장 거친 블록 수]`)
	Pixelate,      // 모자이크. (parameters = [가장 거친 블록 수])
	/// 가로 줄 어긋남 + 색 분리. (parameters = `[줄 수, 초당 변화, 어긋남 폭]`)
	Glitch,        // 가로 줄 어긋남 + 색 분리. (parameters = [줄 수, 초당 변화, 어긋남 폭])
	/// 색수차. (parameters = `[분리 픽셀]`)
	Chromatic,     // 색수차. (parameters = [분리 픽셀])
	// --- 화면 무늬 ---
	/// 주사선 + 틴트 + 깜빡임. (color = 틴트, parameters = `[주사선 수, 흐르는 속도]`)
	Hologram,      // 주사선 + 틴트 + 깜빡임. (color = 틴트, parameters = [주사선 수, 흐르는 속도])
	/// 주사선. (parameters = `[주사선 수]`)
	Scanlines,     // 주사선. (parameters = [주사선 수])
	/// 낡은 필름. (세피아 + 그레인 + 스크래치 + 깜빡임)
	OldFilm,       // 낡은 필름. (세피아 + 그레인 + 스크래치 + 깜빡임)
	/// 비네트. (color = 가장자리 색, parameters = `[시작, 끝]`)
	Vignette,      // 비네트. (color = 가장자리 색, parameters = [시작, 끝])
	/// 대각선 하이라이트 띠가 지나감. (color = 띠 색, parameters = `[띠 폭]`)
	Shine,         // 대각선 하이라이트 띠가 지나감. (color = 띠 색, parameters = [띠 폭])
	// --- 사라짐 / 드러남 (progress 0 → 1) ---
	/// 노이즈로 타 들어가며 사라짐. (color = 가장자리 색, parameters = `[노이즈 배율, 가장자리 폭]`)
	Dissolve,      // 노이즈로 타 들어가며 사라짐. (color = 가장자리 색, parameters = [노이즈 배율, 가장자리 폭])
	/// 불에 타듯 사라짐. (안쪽 밝은 띠 + 바깥 그을음, parameters = `[노이즈 배율, 띠 폭]`)
	Burn,          // 불에 타듯 사라짐. (안쪽 밝은 띠 + 바깥 그을음, parameters = [노이즈 배율, 띠 폭])
	/// 노이즈 알파 페이드. (parameters = `[노이즈 배율, 부드러움]`)
	NoiseFade,     // 노이즈 알파 페이드. (parameters = [노이즈 배율, 부드러움])
	/// 블록 단위로 사라짐. (parameters = `[블록 수]`)
	PixelDissolve, // 블록 단위로 사라짐. (parameters = [블록 수])
	/// 방향 와이프로 드러남. (parameters = `[방향 x, 방향 y, 부드러움]`)
	Wipe,          // 방향 와이프로 드러남. (parameters = [방향 x, 방향 y, 부드러움])
	/// 원형 아이리스로 드러남. (parameters = `[중심 x, 중심 y, 부드러움]`)
	Iris,          // 원형 아이리스로 드러남. (parameters = [중심 x, 중심 y, 부드러움])
	/// 마름모 와이프. (parameters = `[부드러움]`)
	DiamondWipe,   // 마름모 와이프. (parameters = [부드러움])
	/// 시계 방향 와이프. (parameters = `[부드러움]`)
	ClockWipe,     // 시계 방향 와이프. (parameters = [부드러움])
	/// 블라인드. (parameters = `[줄 수, 부드러움]`)
	Blinds,        // 블라인드. (parameters = [줄 수, 부드러움])
	/// 체커 보드. (parameters = `[칸 수]`)
	CheckerWipe,   // 체커 보드. (parameters = [칸 수])
}

/// 효과 목록. (선언 차례, 아래 표들의 차례와 같다)
const EFFECT_LIST: [ShaderSpriteEffect; 50] = [
	ShaderSpriteEffect::None, ShaderSpriteEffect::Flash, ShaderSpriteEffect::Silhouette, ShaderSpriteEffect::Colorize, ShaderSpriteEffect::Grayscale, ShaderSpriteEffect::Sepia,
	ShaderSpriteEffect::Invert, ShaderSpriteEffect::HueShift, ShaderSpriteEffect::ColorAdjust, ShaderSpriteEffect::Posterize, ShaderSpriteEffect::Threshold, ShaderSpriteEffect::GradientMap,
	ShaderSpriteEffect::Outline, ShaderSpriteEffect::InnerOutline, ShaderSpriteEffect::Glow, ShaderSpriteEffect::InnerGlow, ShaderSpriteEffect::Shadow, ShaderSpriteEffect::EdgeDetect,
	ShaderSpriteEffect::Emboss, ShaderSpriteEffect::Sharpen, ShaderSpriteEffect::Blur, ShaderSpriteEffect::MotionBlur, ShaderSpriteEffect::RadialBlur, ShaderSpriteEffect::Wave,
	ShaderSpriteEffect::Flag, ShaderSpriteEffect::Ripple, ShaderSpriteEffect::HeatHaze, ShaderSpriteEffect::Bulge, ShaderSpriteEffect::Swirl, ShaderSpriteEffect::Mirror,
	ShaderSpriteEffect::Kaleidoscope, ShaderSpriteEffect::Jitter, ShaderSpriteEffect::Pixelate, ShaderSpriteEffect::Glitch, ShaderSpriteEffect::Chromatic, ShaderSpriteEffect::Hologram,
	ShaderSpriteEffect::Scanlines, ShaderSpriteEffect::OldFilm, ShaderSpriteEffect::Vignette, ShaderSpriteEffect::Shine, ShaderSpriteEffect::Dissolve, ShaderSpriteEffect::Burn,
	ShaderSpriteEffect::NoiseFade, ShaderSpriteEffect::PixelDissolve, ShaderSpriteEffect::Wipe, ShaderSpriteEffect::Iris, ShaderSpriteEffect::DiamondWipe, ShaderSpriteEffect::ClockWipe,
	ShaderSpriteEffect::Blinds, ShaderSpriteEffect::CheckerWipe,
];

/// 효과별 원본 이름.
const EFFECT_NAME_TABLE: [(ShaderSpriteEffect, &str); 50] = [
	(ShaderSpriteEffect::None, "none"),
	(ShaderSpriteEffect::Flash, "flash"),
	(ShaderSpriteEffect::Silhouette, "silhouette"),
	(ShaderSpriteEffect::Colorize, "colorize"),
	(ShaderSpriteEffect::Grayscale, "grayscale"),
	(ShaderSpriteEffect::Sepia, "sepia"),
	(ShaderSpriteEffect::Invert, "invert"),
	(ShaderSpriteEffect::HueShift, "hueShift"),
	(ShaderSpriteEffect::ColorAdjust, "colorAdjust"),
	(ShaderSpriteEffect::Posterize, "posterize"),
	(ShaderSpriteEffect::Threshold, "threshold"),
	(ShaderSpriteEffect::GradientMap, "gradientMap"),
	(ShaderSpriteEffect::Outline, "outline"),
	(ShaderSpriteEffect::InnerOutline, "innerOutline"),
	(ShaderSpriteEffect::Glow, "glow"),
	(ShaderSpriteEffect::InnerGlow, "innerGlow"),
	(ShaderSpriteEffect::Shadow, "shadow"),
	(ShaderSpriteEffect::EdgeDetect, "edgeDetect"),
	(ShaderSpriteEffect::Emboss, "emboss"),
	(ShaderSpriteEffect::Sharpen, "sharpen"),
	(ShaderSpriteEffect::Blur, "blur"),
	(ShaderSpriteEffect::MotionBlur, "motionBlur"),
	(ShaderSpriteEffect::RadialBlur, "radialBlur"),
	(ShaderSpriteEffect::Wave, "wave"),
	(ShaderSpriteEffect::Flag, "flag"),
	(ShaderSpriteEffect::Ripple, "ripple"),
	(ShaderSpriteEffect::HeatHaze, "heatHaze"),
	(ShaderSpriteEffect::Bulge, "bulge"),
	(ShaderSpriteEffect::Swirl, "swirl"),
	(ShaderSpriteEffect::Mirror, "mirror"),
	(ShaderSpriteEffect::Kaleidoscope, "kaleidoscope"),
	(ShaderSpriteEffect::Jitter, "jitter"),
	(ShaderSpriteEffect::Pixelate, "pixelate"),
	(ShaderSpriteEffect::Glitch, "glitch"),
	(ShaderSpriteEffect::Chromatic, "chromatic"),
	(ShaderSpriteEffect::Hologram, "hologram"),
	(ShaderSpriteEffect::Scanlines, "scanlines"),
	(ShaderSpriteEffect::OldFilm, "oldFilm"),
	(ShaderSpriteEffect::Vignette, "vignette"),
	(ShaderSpriteEffect::Shine, "shine"),
	(ShaderSpriteEffect::Dissolve, "dissolve"),
	(ShaderSpriteEffect::Burn, "burn"),
	(ShaderSpriteEffect::NoiseFade, "noiseFade"),
	(ShaderSpriteEffect::PixelDissolve, "pixelDissolve"),
	(ShaderSpriteEffect::Wipe, "wipe"),
	(ShaderSpriteEffect::Iris, "iris"),
	(ShaderSpriteEffect::DiamondWipe, "diamondWipe"),
	(ShaderSpriteEffect::ClockWipe, "clockWipe"),
	(ShaderSpriteEffect::Blinds, "blinds"),
	(ShaderSpriteEffect::CheckerWipe, "checkerWipe"),
];

// 효과의 반사 값. (원본 ShaderSpriteEffect 글)
crate::core::reflection::impl_reflect_names!(ShaderSpriteEffect, EFFECT_NAME_TABLE);

/// 효과별 기본 파라미터.
const DEFAULT_PARAMETER_TABLE: [(ShaderSpriteEffect, [f32; 4]); 50] = [
	(ShaderSpriteEffect::None, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Flash, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Silhouette, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Colorize, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Grayscale, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Sepia, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Invert, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::HueShift, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::ColorAdjust, [1.4, 1.3, 1.1, 0.0]),
	(ShaderSpriteEffect::Posterize, [4.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Threshold, [0.5, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::GradientMap, [0.1, 0.05, 0.25, 0.0]),
	(ShaderSpriteEffect::Outline, [2.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::InnerOutline, [2.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Glow, [6.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::InnerGlow, [5.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Shadow, [6.0, 6.0, 0.0, 0.0]),
	(ShaderSpriteEffect::EdgeDetect, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Emboss, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Sharpen, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Blur, [3.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::MotionBlur, [1.0, 0.0, 12.0, 0.0]),
	(ShaderSpriteEffect::RadialBlur, [0.12, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Wave, [12.0, 4.0, 0.03, 0.0]),
	(ShaderSpriteEffect::Flag, [8.0, 5.0, 0.06, 0.0]),
	(ShaderSpriteEffect::Ripple, [40.0, 6.0, 0.02, 0.0]),
	(ShaderSpriteEffect::HeatHaze, [6.0, 1.5, 0.02, 0.0]),
	(ShaderSpriteEffect::Bulge, [0.6, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Swirl, [1.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Mirror, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Kaleidoscope, [6.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Jitter, [4.0, 24.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Pixelate, [24.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Glitch, [18.0, 12.0, 0.15, 0.0]),
	(ShaderSpriteEffect::Chromatic, [3.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Hologram, [120.0, 6.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Scanlines, [90.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::OldFilm, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Vignette, [0.4, 1.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Shine, [0.15, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Dissolve, [8.0, 0.08, 0.0, 0.0]),
	(ShaderSpriteEffect::Burn, [8.0, 0.06, 0.0, 0.0]),
	(ShaderSpriteEffect::NoiseFade, [6.0, 0.2, 0.0, 0.0]),
	(ShaderSpriteEffect::PixelDissolve, [16.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Wipe, [1.0, 0.0, 0.05, 0.0]),
	(ShaderSpriteEffect::Iris, [0.5, 0.5, 0.05, 0.0]),
	(ShaderSpriteEffect::DiamondWipe, [0.05, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::ClockWipe, [0.02, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::Blinds, [8.0, 0.05, 0.0, 0.0]),
	(ShaderSpriteEffect::CheckerWipe, [8.0, 0.0, 0.0, 0.0]),
];

/// 효과별 기본 색.
const DEFAULT_COLOR_TABLE: [(ShaderSpriteEffect, [f32; 4]); 50] = [
	(ShaderSpriteEffect::None, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Flash, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Silhouette, [0.0, 0.0, 0.0, 1.0]),
	(ShaderSpriteEffect::Colorize, [1.0, 0.5, 0.5, 1.0]),
	(ShaderSpriteEffect::Grayscale, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Sepia, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Invert, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::HueShift, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::ColorAdjust, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Posterize, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Threshold, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::GradientMap, [1.0, 0.85, 0.5, 1.0]),
	(ShaderSpriteEffect::Outline, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::InnerOutline, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Glow, [1.0, 0.85, 0.4, 1.0]),
	(ShaderSpriteEffect::InnerGlow, [1.0, 0.85, 0.4, 1.0]),
	(ShaderSpriteEffect::Shadow, [0.0, 0.0, 0.0, 0.6]),
	(ShaderSpriteEffect::EdgeDetect, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Emboss, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Sharpen, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Blur, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::MotionBlur, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::RadialBlur, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Wave, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Flag, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Ripple, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::HeatHaze, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Bulge, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Swirl, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Mirror, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Kaleidoscope, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Jitter, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Pixelate, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Glitch, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Chromatic, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Hologram, [0.4, 0.9, 1.0, 1.0]),
	(ShaderSpriteEffect::Scanlines, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::OldFilm, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Vignette, [0.0, 0.0, 0.0, 1.0]),
	(ShaderSpriteEffect::Shine, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Dissolve, [1.0, 0.6, 0.2, 1.0]),
	(ShaderSpriteEffect::Burn, [1.0, 0.7, 0.25, 1.0]),
	(ShaderSpriteEffect::NoiseFade, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::PixelDissolve, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Wipe, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Iris, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::DiamondWipe, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::ClockWipe, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::Blinds, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::CheckerWipe, [1.0, 1.0, 1.0, 1.0]),
];

/// 효과별 프래그먼트 본문. (uv / local / textureColor 를 바꾸거나 다시 샘플한다. 텍스처는 프리멀티플라이드 알파)
/// - 공용: sampleSprite(uv) / sampleLocal(local) / unpremultiply(color) / luminanceAt(uv) / hash21 / valueNoise.
const EFFECT_BODY_TABLE: [(ShaderSpriteEffect, &str); 49] = [
	(ShaderSpriteEffect::Flash, "
	textureColor.rgb = mix(textureColor.rgb, effectColor.rgb * textureColor.a, effectProgress * effectColor.a);
	"),
	(ShaderSpriteEffect::Silhouette, "
	textureColor.rgb = mix(textureColor.rgb, effectColor.rgb * textureColor.a, effectProgress);
	textureColor *= mix(1.0, effectColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::Colorize, "
	textureColor.rgb *= mix(vec3(1.0), effectColor.rgb, effectProgress * effectColor.a);
	"),
	(ShaderSpriteEffect::Grayscale, "
	float gray = dot(textureColor.rgb, vec3(0.299, 0.587, 0.114));
	textureColor.rgb = mix(textureColor.rgb, vec3(gray) * effectColor.rgb, effectProgress);
	"),
	(ShaderSpriteEffect::Sepia, "
	float gray = dot(textureColor.rgb, vec3(0.299, 0.587, 0.114));
	textureColor.rgb = mix(textureColor.rgb, gray * vec3(1.2, 1.0, 0.78), effectProgress);
	"),
	(ShaderSpriteEffect::Invert, "
	textureColor.rgb = mix(textureColor.rgb, vec3(textureColor.a) - textureColor.rgb, effectProgress);
	"),
	(ShaderSpriteEffect::HueShift, "
	float hueAngle = effectProgress * 6.28318530718;
	vec3 yiq = mat3(0.299, 0.596, 0.211, 0.587, -0.274, -0.523, 0.114, -0.322, 0.312) * textureColor.rgb;
	float chroma = length(yiq.yz);
	float hue = atan(yiq.z, yiq.y) + hueAngle;
	yiq.yz = vec2(cos(hue), sin(hue)) * chroma;
	textureColor.rgb = mat3(1.0, 1.0, 1.0, 0.956, -0.272, -1.106, 0.621, -0.647, 1.703) * yiq;
	"),
	(ShaderSpriteEffect::ColorAdjust, "
	vec3 straight = unpremultiply(textureColor);
	float gray = dot(straight, vec3(0.299, 0.587, 0.114));
	vec3 adjusted = mix(vec3(gray), straight, effectParameters.y);
	adjusted = (adjusted - 0.5) * effectParameters.x + 0.5;
	adjusted *= effectParameters.z;
	textureColor.rgb = mix(textureColor.rgb, clamp(adjusted, 0.0, 1.0) * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::Posterize, "
	vec3 straight = unpremultiply(textureColor);
	float levels = max(effectParameters.x, 1.0);
	vec3 posterized = floor(straight * levels + 0.5) / levels;
	textureColor.rgb = mix(textureColor.rgb, posterized * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::Threshold, "
	vec3 straight = unpremultiply(textureColor);
	float gray = dot(straight, vec3(0.299, 0.587, 0.114));
	vec3 binary = vec3(step(effectParameters.x, gray)) * effectColor.rgb;
	textureColor.rgb = mix(textureColor.rgb, binary * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::GradientMap, "
	vec3 straight = unpremultiply(textureColor);
	float gray = dot(straight, vec3(0.299, 0.587, 0.114));
	vec3 mapped = mix(effectParameters.xyz, effectColor.rgb, gray);
	textureColor.rgb = mix(textureColor.rgb, mapped * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::Outline, "
	vec2 outlineStep = texelSize * effectParameters.x;
	float neighborAlpha = 0.0;
	for (int sampleIndex = 0; sampleIndex < 8; ++sampleIndex) {
		float angle = float(sampleIndex) * 0.78539816;
		neighborAlpha = max(neighborAlpha, sampleSprite(uv + vec2(cos(angle), sin(angle)) * outlineStep).a);
	}
	float outlineMask = clamp(neighborAlpha - textureColor.a, 0.0, 1.0) * effectProgress * effectColor.a;
	textureColor.rgb += effectColor.rgb * outlineMask;
	textureColor.a += outlineMask;
	"),
	(ShaderSpriteEffect::InnerOutline, "
	vec2 outlineStep = texelSize * effectParameters.x;
	float minimumAlpha = 1.0;
	for (int sampleIndex = 0; sampleIndex < 8; ++sampleIndex) {
		float angle = float(sampleIndex) * 0.78539816;
		minimumAlpha = min(minimumAlpha, sampleSprite(uv + vec2(cos(angle), sin(angle)) * outlineStep).a);
	}
	float edgeMask = clamp(textureColor.a - minimumAlpha, 0.0, 1.0) * effectProgress * effectColor.a;
	textureColor.rgb = mix(textureColor.rgb, effectColor.rgb * textureColor.a, edgeMask);
	"),
	(ShaderSpriteEffect::Glow, "
	float alphaSum = 0.0;
	for (int sampleIndex = 0; sampleIndex < 12; ++sampleIndex) {
		float angle = float(sampleIndex) * 0.52359878;
		vec2 offset = vec2(cos(angle), sin(angle)) * texelSize * effectParameters.x;
		alphaSum += sampleSprite(uv + offset).a;
		alphaSum += sampleSprite(uv + offset * 0.5).a;
	}
	float glowAlpha = clamp(alphaSum / 24.0 * 1.5, 0.0, 1.0) * (1.0 - textureColor.a) * effectProgress * effectColor.a;
	textureColor.rgb += effectColor.rgb * glowAlpha;
	textureColor.a += glowAlpha;
	"),
	(ShaderSpriteEffect::InnerGlow, "
	float alphaSum = 0.0;
	for (int sampleIndex = 0; sampleIndex < 12; ++sampleIndex) {
		float angle = float(sampleIndex) * 0.52359878;
		vec2 offset = vec2(cos(angle), sin(angle)) * texelSize * effectParameters.x;
		alphaSum += sampleSprite(uv + offset).a;
		alphaSum += sampleSprite(uv + offset * 0.5).a;
	}
	float glowMask = clamp(1.0 - alphaSum / 24.0, 0.0, 1.0) * textureColor.a * effectProgress * effectColor.a;
	textureColor.rgb = mix(textureColor.rgb, effectColor.rgb * textureColor.a, glowMask);
	"),
	(ShaderSpriteEffect::Shadow, "
	vec2 shadowOffset = effectParameters.xy * texelSize;
	float shadowAlpha = sampleSprite(uv - shadowOffset).a * effectColor.a * effectProgress * (1.0 - textureColor.a);
	textureColor.rgb += effectColor.rgb * shadowAlpha;
	textureColor.a += shadowAlpha;
	"),
	(ShaderSpriteEffect::EdgeDetect, "
	float gradientX = 0.0;
	float gradientY = 0.0;
	gradientX += luminanceAt(uv + vec2(-texelSize.x, -texelSize.y)) * -1.0 + luminanceAt(uv + vec2(texelSize.x, -texelSize.y));
	gradientX += luminanceAt(uv + vec2(-texelSize.x, 0.0)) * -2.0 + luminanceAt(uv + vec2(texelSize.x, 0.0)) * 2.0;
	gradientX += luminanceAt(uv + vec2(-texelSize.x, texelSize.y)) * -1.0 + luminanceAt(uv + vec2(texelSize.x, texelSize.y));
	gradientY += luminanceAt(uv + vec2(-texelSize.x, -texelSize.y)) * -1.0 + luminanceAt(uv + vec2(-texelSize.x, texelSize.y));
	gradientY += luminanceAt(uv + vec2(0.0, -texelSize.y)) * -2.0 + luminanceAt(uv + vec2(0.0, texelSize.y)) * 2.0;
	gradientY += luminanceAt(uv + vec2(texelSize.x, -texelSize.y)) * -1.0 + luminanceAt(uv + vec2(texelSize.x, texelSize.y));
	float edge = clamp(length(vec2(gradientX, gradientY)) * 2.0, 0.0, 1.0);
	textureColor = mix(textureColor, vec4(effectColor.rgb * edge, edge) * effectColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::Emboss, "
	vec3 lower = sampleSprite(uv - texelSize).rgb;
	vec3 upper = sampleSprite(uv + texelSize).rgb;
	float relief = dot(upper - lower, vec3(0.299, 0.587, 0.114)) * 2.0 + 0.5;
	textureColor.rgb = mix(textureColor.rgb, vec3(clamp(relief, 0.0, 1.0)) * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::Sharpen, "
	vec4 neighbors = sampleSprite(uv + vec2(texelSize.x, 0.0)) + sampleSprite(uv - vec2(texelSize.x, 0.0)) + sampleSprite(uv + vec2(0.0, texelSize.y)) + sampleSprite(uv - vec2(0.0, texelSize.y));
	vec4 sharpened = clamp(textureColor * 5.0 - neighbors, 0.0, 1.0);
	textureColor = mix(textureColor, sharpened, effectProgress);
	"),
	(ShaderSpriteEffect::Blur, "
	vec2 blurStep = texelSize * effectParameters.x * effectProgress;
	vec4 sum = vec4(0.0);
	for (int y = -1; y <= 1; ++y) {
		for (int x = -1; x <= 1; ++x) {
			float weight = (x == 0 && y == 0) ? 4.0 : ((x == 0 || y == 0) ? 2.0 : 1.0);
			sum += sampleSprite(uv + vec2(float(x), float(y)) * blurStep) * weight;
		}
	}
	textureColor = sum / 16.0;
	"),
	(ShaderSpriteEffect::MotionBlur, "
	vec2 blurDirection = normalize(effectParameters.xy + vec2(0.0001, 0.0)) * texelSize * effectParameters.z * effectProgress;
	vec4 sum = vec4(0.0);
	for (int sampleIndex = 0; sampleIndex < 8; ++sampleIndex) {
		float offset = (float(sampleIndex) - 3.5) / 3.5;
		sum += sampleSprite(uv + blurDirection * offset);
	}
	textureColor = sum / 8.0;
	"),
	(ShaderSpriteEffect::RadialBlur, "
	vec2 toCenter = (vec2(0.5) - local) * effectParameters.x * effectProgress;
	vec4 sum = vec4(0.0);
	for (int sampleIndex = 0; sampleIndex < 8; ++sampleIndex) {
		sum += sampleLocal(local + toCenter * (float(sampleIndex) / 8.0));
	}
	textureColor = sum / 8.0;
	"),
	(ShaderSpriteEffect::Wave, "
	float waveOffset = sin(local.y * effectParameters.x + effectTime * effectParameters.y) * effectParameters.z * effectProgress;
	textureColor = sampleLocal(local + vec2(waveOffset, 0.0));
	"),
	(ShaderSpriteEffect::Flag, "
	float flagOffset = sin(local.x * effectParameters.x - effectTime * effectParameters.y) * effectParameters.z * effectProgress * local.x;
	textureColor = sampleLocal(local + vec2(0.0, flagOffset));
	"),
	(ShaderSpriteEffect::Ripple, "
	vec2 fromCenter = local - 0.5;
	float rippleDistance = length(fromCenter);
	float rippleWave = sin(rippleDistance * effectParameters.x - effectTime * effectParameters.y) * effectParameters.z * effectProgress;
	vec2 rippleDirection = rippleDistance > 0.0001 ? fromCenter / rippleDistance : vec2(0.0);
	textureColor = sampleLocal(local + rippleDirection * rippleWave);
	"),
	(ShaderSpriteEffect::HeatHaze, "
	vec2 hazeOffset = vec2(valueNoise(local * effectParameters.x + vec2(0.0, effectTime * effectParameters.y)), valueNoise(local * effectParameters.x + vec2(effectTime * effectParameters.y, 7.3))) - 0.5;
	textureColor = sampleLocal(local + hazeOffset * effectParameters.z * effectProgress);
	"),
	(ShaderSpriteEffect::Bulge, "
	vec2 fromCenter = local - 0.5;
	float bulgeRadius = length(fromCenter) * 2.0;
	float bulgeFactor = 1.0 - effectParameters.x * effectProgress * (1.0 - clamp(bulgeRadius, 0.0, 1.0));
	textureColor = sampleLocal(0.5 + fromCenter * bulgeFactor);
	"),
	(ShaderSpriteEffect::Swirl, "
	vec2 fromCenter = local - 0.5;
	float swirlRadius = length(fromCenter) * 2.0;
	float swirlAngle = effectParameters.x * effectProgress * 6.28318530718 * pow(1.0 - clamp(swirlRadius, 0.0, 1.0), 2.0);
	float cosine = cos(swirlAngle);
	float sine = sin(swirlAngle);
	textureColor = sampleLocal(0.5 + vec2(fromCenter.x * cosine - fromCenter.y * sine, fromCenter.x * sine + fromCenter.y * cosine));
	"),
	(ShaderSpriteEffect::Mirror, "
	vec2 mirrored = local;
	if (effectParameters.x < 0.5) {
		mirrored.x = local.x < 0.5 ? local.x : 1.0 - local.x;
	}
	else {
		mirrored.y = local.y < 0.5 ? local.y : 1.0 - local.y;
	}
	textureColor = mix(textureColor, sampleLocal(mirrored), step(0.5, effectProgress));
	"),
	(ShaderSpriteEffect::Kaleidoscope, "
	vec2 fromCenter = local - 0.5;
	float kaleidoRadius = length(fromCenter);
	float segmentAngle = 6.28318530718 / max(effectParameters.x, 1.0);
	float kaleidoAngle = atan(fromCenter.y, fromCenter.x);
	kaleidoAngle = abs(mod(kaleidoAngle, segmentAngle) - segmentAngle * 0.5);
	vec4 kaleidoColor = sampleLocal(0.5 + vec2(cos(kaleidoAngle), sin(kaleidoAngle)) * kaleidoRadius);
	textureColor = mix(textureColor, kaleidoColor, effectProgress);
	"),
	(ShaderSpriteEffect::Jitter, "
	float jitterSeed = floor(effectTime * effectParameters.y) + effectSeed;
	vec2 jitterOffset = (vec2(hash21(vec2(jitterSeed, 1.0)), hash21(vec2(jitterSeed, 2.0))) - 0.5) * effectParameters.x * effectProgress;
	textureColor = sampleSprite(uv + jitterOffset * texelSize);
	"),
	(ShaderSpriteEffect::Pixelate, "
	float blockCount = mix(512.0, max(1.0, effectParameters.x), effectProgress);
	vec2 blockLocal = (floor(local * blockCount) + 0.5) / blockCount;
	textureColor = sampleLocal(blockLocal);
	"),
	(ShaderSpriteEffect::Glitch, "
	float glitchSeed = floor(effectTime * effectParameters.y) + effectSeed;
	float glitchRow = floor(local.y * effectParameters.x);
	float rowRandom = hash21(vec2(glitchRow, glitchSeed));
	float shift = step(0.75, rowRandom) * (hash21(vec2(glitchSeed, glitchRow)) - 0.5) * effectParameters.z * effectProgress;
	vec2 shiftedLocal = local + vec2(shift, 0.0);
	textureColor = sampleLocal(shiftedLocal);
	float split = effectProgress * 0.02 * step(0.5, rowRandom);
	textureColor.r = sampleLocal(shiftedLocal + vec2(split, 0.0)).r;
	textureColor.b = sampleLocal(shiftedLocal - vec2(split, 0.0)).b;
	"),
	(ShaderSpriteEffect::Chromatic, "
	vec2 chromaticStep = vec2(texelSize.x * effectParameters.x * effectProgress, 0.0);
	textureColor.r = sampleSprite(uv + chromaticStep).r;
	textureColor.b = sampleSprite(uv - chromaticStep).b;
	"),
	(ShaderSpriteEffect::Hologram, "
	float scan = 0.7 + 0.3 * sin(local.y * effectParameters.x - effectTime * effectParameters.y);
	float flicker = 0.92 + 0.08 * sin(effectTime * 23.0 + effectSeed);
	float jitter = step(0.97, hash21(vec2(floor(effectTime * 14.0), effectSeed))) * 0.03;
	vec4 jitterColor = sampleLocal(local + vec2(jitter, 0.0));
	textureColor = mix(textureColor, jitterColor, effectProgress);
	float luminance = dot(textureColor.rgb, vec3(0.299, 0.587, 0.114));
	vec3 holoColor = effectColor.rgb * (luminance * 0.6 + textureColor.a * 0.4) * scan * flicker;
	textureColor.rgb = mix(textureColor.rgb, holoColor, effectProgress * effectColor.a);
	textureColor.a *= mix(1.0, 0.8 * scan, effectProgress);
	"),
	(ShaderSpriteEffect::Scanlines, "
	float lines = 0.5 + 0.5 * sin(local.y * effectParameters.x);
	textureColor.rgb *= 1.0 - effectProgress * 0.5 * lines;
	"),
	(ShaderSpriteEffect::OldFilm, "
	float gray = dot(textureColor.rgb, vec3(0.299, 0.587, 0.114));
	vec3 filmColor = gray * vec3(1.15, 1.0, 0.8);
	float grain = (hash21(local * 300.0 + fract(effectTime * 9.1) * 100.0) - 0.5) * 0.25 * textureColor.a;
	float scratchSeed = floor(effectTime * 8.0);
	float scratchX = hash21(vec2(scratchSeed, effectSeed));
	float scratch = (1.0 - smoothstep(0.0, 0.006, abs(local.x - scratchX))) * step(0.6, hash21(vec2(scratchSeed, 3.0))) * textureColor.a;
	float flicker = 0.9 + 0.1 * hash21(vec2(floor(effectTime * 12.0), 5.0));
	float vignette = 1.0 - smoothstep(0.5, 1.0, distance(local, vec2(0.5)) * 1.4142) * 0.6;
	vec3 aged = (filmColor + grain + scratch * 0.8) * flicker * vignette;
	textureColor.rgb = mix(textureColor.rgb, clamp(aged, 0.0, 1.0), effectProgress);
	"),
	(ShaderSpriteEffect::Vignette, "
	float vignetteDistance = distance(local, vec2(0.5)) * 1.4142;
	float vignetteMask = smoothstep(effectParameters.x, effectParameters.y, vignetteDistance) * effectProgress * effectColor.a;
	textureColor.rgb = mix(textureColor.rgb, effectColor.rgb * textureColor.a, vignetteMask);
	"),
	(ShaderSpriteEffect::Shine, "
	float band = local.x * 0.8 + local.y * 0.5;
	float bandCenter = mix(-0.4, 1.7, effectProgress);
	float glow = 1.0 - smoothstep(0.0, max(effectParameters.x, 0.001), abs(band - bandCenter));
	textureColor.rgb += effectColor.rgb * glow * glow * textureColor.a * effectColor.a;
	"),
	(ShaderSpriteEffect::Dissolve, "
	float dissolveNoise = valueNoise(local * effectParameters.x + vec2(effectSeed * 17.0, effectSeed * 31.0));
	float threshold = effectProgress * (1.0 + effectParameters.y * 2.0) - effectParameters.y;
	float edge = smoothstep(threshold, threshold + effectParameters.y, dissolveNoise);
	float keep = step(threshold, dissolveNoise);
	vec3 edgeColor = effectColor.rgb * textureColor.a * effectColor.a;
	textureColor.rgb = mix(edgeColor, textureColor.rgb, edge) * keep;
	textureColor.a *= keep;
	"),
	(ShaderSpriteEffect::Burn, "
	float burnNoise = valueNoise(local * effectParameters.x + vec2(effectSeed * 13.0, effectSeed * 29.0));
	float threshold = effectProgress * (1.0 + effectParameters.y * 4.0) - effectParameters.y * 2.0;
	float keep = step(threshold, burnNoise);
	float innerBand = smoothstep(threshold, threshold + effectParameters.y, burnNoise);
	float outerBand = smoothstep(threshold + effectParameters.y, threshold + effectParameters.y * 2.0, burnNoise);
	vec3 emberColor = effectColor.rgb * textureColor.a * effectColor.a;
	vec3 sootColor = vec3(0.05, 0.02, 0.01) * textureColor.a;
	textureColor.rgb = mix(emberColor, mix(sootColor, textureColor.rgb, outerBand), innerBand) * keep;
	textureColor.a *= keep;
	"),
	(ShaderSpriteEffect::NoiseFade, "
	float fadeNoise = valueNoise(local * effectParameters.x + vec2(effectSeed * 11.0, effectSeed * 23.0));
	float softness = max(effectParameters.y, 0.001);
	float fadeMask = smoothstep(effectProgress * (1.0 + softness) - softness, effectProgress * (1.0 + softness), fadeNoise);
	textureColor *= fadeMask;
	"),
	(ShaderSpriteEffect::PixelDissolve, "
	vec2 blockCell = floor(local * effectParameters.x);
	float blockRandom = hash21(blockCell + effectSeed);
	textureColor *= step(effectProgress, blockRandom);
	"),
	(ShaderSpriteEffect::Wipe, "
	vec2 wipeDirection = normalize(effectParameters.xy + vec2(0.0001, 0.0));
	float wipeDistance = dot(local - 0.5, wipeDirection) + 0.5;
	float softness = max(effectParameters.z, 0.0001);
	float reveal = 1.0 - smoothstep(effectProgress * (1.0 + softness) - softness, effectProgress * (1.0 + softness), wipeDistance);
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::Iris, "
	float irisRadius = effectProgress * 0.7072;
	float softness = max(effectParameters.z, 0.0001);
	float reveal = 1.0 - smoothstep(irisRadius, irisRadius + softness, distance(local, effectParameters.xy));
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::DiamondWipe, "
	float diamondDistance = abs(local.x - 0.5) + abs(local.y - 0.5);
	float softness = max(effectParameters.x, 0.0001);
	float reveal = 1.0 - smoothstep(effectProgress * (1.0 + softness), effectProgress * (1.0 + softness) + softness, diamondDistance);
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::ClockWipe, "
	vec2 fromCenter = local - 0.5;
	float clockAngle = (atan(fromCenter.x, -fromCenter.y) + 3.14159265) / 6.28318530718;
	float softness = max(effectParameters.x, 0.0001);
	float reveal = 1.0 - smoothstep(effectProgress, effectProgress + softness, clockAngle);
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::Blinds, "
	float stripe = fract(local.y * effectParameters.x);
	float softness = max(effectParameters.y, 0.0001);
	float reveal = 1.0 - smoothstep(effectProgress, effectProgress + softness, stripe);
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::CheckerWipe, "
	vec2 checkerCell = floor(local * effectParameters.x);
	float parity = mod(checkerCell.x + checkerCell.y, 2.0);
	float cellThreshold = parity * 0.5 + hash21(checkerCell + effectSeed) * 0.5;
	textureColor *= step(cellThreshold, effectProgress * 1.0001);
	"),
];

/// 버텍스 셰이더. (Graphic 기본 프로그램과 동일)
const VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexPosition;
in vec2 vertexTextureCoordinate;
uniform mat3 projectionMatrix;
uniform mat3 modelMatrix;
out vec2 fragmentTextureCoordinate;
void main() {
	vec3 transformedPosition = projectionMatrix * (modelMatrix * vec3(vertexPosition, 1.0));
	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	fragmentTextureCoordinate = vertexTextureCoordinate;
}
";

/// 프래그먼트 셰이더 틀. (효과 본문을 EFFECT_BODY 자리에 끼운다)
const FRAGMENTSHADER_TEMPLATE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D mainTexture;
uniform vec4 mainColor;
uniform vec4 tintColor;
uniform float globalAlpha;
uniform float effectTime;
uniform float effectProgress;
uniform float effectSeed;
uniform vec4 effectColor;
uniform vec4 effectParameters;
uniform vec4 textureRect;
uniform vec2 texelSize;
out vec4 outputColor;

// 점 해시. 두 float 의 비트를 MurmurHash3 의 fmix32 마무리(Austin Appleby, 공개 도메인)로 섞어 [0, 1) 로 낸다.
highp uint hashMix32(highp uint value) {
	value ^= value >> 16u;
	value *= 0x85ebca6bu;
	value ^= value >> 13u;
	value *= 0xc2b2ae35u;
	value ^= value >> 16u;
	return value;
}

float hash21(vec2 point) {
	highp uvec2 bits = floatBitsToUint(point);
	highp uint value = hashMix32(bits.x ^ hashMix32(bits.y + 0x9e3779b9u));
	return float(value >> 8u) * (1.0 / 16777216.0);
}

float valueNoise(vec2 point) {
	vec2 cell = floor(point);
	vec2 fraction = fract(point);
	vec2 blend = fraction * fraction * (3.0 - 2.0 * fraction);
	float bottom = mix(hash21(cell), hash21(cell + vec2(1.0, 0.0)), blend.x);
	float top = mix(hash21(cell + vec2(0.0, 1.0)), hash21(cell + vec2(1.0, 1.0)), blend.x);
	return mix(bottom, top, blend.y);
}

vec4 sampleSprite(vec2 uv) {
	if (uv.x < textureRect.x || uv.x > textureRect.z || uv.y < textureRect.y || uv.y > textureRect.w) {
		return vec4(0.0);
	}
	return texture(mainTexture, uv);
}

vec4 sampleLocal(vec2 local) {
	if (local.x < 0.0 || local.x > 1.0 || local.y < 0.0 || local.y > 1.0) {
		return vec4(0.0);
	}
	return texture(mainTexture, textureRect.xy + local * (textureRect.zw - textureRect.xy));
}

vec3 unpremultiply(vec4 color) {
	return color.rgb / max(color.a, 0.0001);
}

float luminanceAt(vec2 uv) {
	vec4 sampled = sampleSprite(uv);
	return dot(sampled.rgb, vec3(0.299, 0.587, 0.114));
}

void main() {
	vec2 uv = fragmentTextureCoordinate;
	vec2 local = (uv - textureRect.xy) / max(textureRect.zw - textureRect.xy, vec2(0.000001));
	vec4 textureColor = sampleSprite(uv);
	EFFECT_BODY
	vec3 tintedColor = mix(textureColor.rgb, tintColor.rgb * textureColor.a, tintColor.a);
	float finalAlpha = textureColor.a * mainColor.a * globalAlpha;
	vec3 finalColor = tintedColor * mainColor.rgb * mainColor.a * globalAlpha;
	outputColor = vec4(finalColor, finalAlpha);
}
";

/// Graphic 프로그램 캐시에서 이 효과들이 쓰는 묶음 이름.
const PROGRAM_GROUP_NAME: &str = "ShaderSprite";

impl ShaderSpriteEffect {
	//==============================================================================
	/// 원본 이름 반환. ("dissolve" 같은 문자열)
	///
	/// # 돌려줌
	/// 원본 효과 이름. (camelCase, 없음은 "none")
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		EFFECT_NAME_TABLE[*self as usize].1
	}

	//==============================================================================
	/// 원본 이름으로 찾기. (모르는 이름이면 None)
	///
	/// # 인자
	/// - `name`: 원본 효과 이름. (대소문자를 가린다, "heatHaze" 처럼 camelCase)
	///
	/// # 돌려줌
	/// 그 이름의 효과. 모르는 이름이면 None.
	///
	/// # 예제
	/// 이름과 효과 오가기
	/// ```rust
	/// use vanilla::ShaderSpriteEffect;
	///
	/// assert_eq!(ShaderSpriteEffect::from_name("dissolve"), Some(ShaderSpriteEffect::Dissolve));
	/// assert_eq!(ShaderSpriteEffect::Dissolve.get_name(), "dissolve");
	/// assert_eq!(ShaderSpriteEffect::from_name("Dissolve"), None);
	/// assert_eq!(ShaderSpriteEffect::get_effect_list().len(), 50);
	/// ```
	//==============================================================================
	pub fn from_name(name: &str) -> Option<ShaderSpriteEffect> {
		EFFECT_NAME_TABLE.iter().find(|(_, effect_name)| *effect_name == name).map(|(effect, _)| *effect)
	}

	//==============================================================================
	/// 효과 목록 반환. (선언 차례, 원본 Object.keys(ShaderSpriteEffect) 자리)
	///
	/// # 돌려줌
	/// 모든 효과 50 개. (None 이 처음)
	//==============================================================================
	pub fn get_effect_list() -> &'static [ShaderSpriteEffect] {
		&EFFECT_LIST
	}
}

//==============================================================================
/// 효과 본문 반환. (none 이면 None)
//==============================================================================
fn find_effect_body(effect: ShaderSpriteEffect) -> Option<&'static str> {
	EFFECT_BODY_TABLE.iter().find(|(body_effect, _)| *body_effect == effect).map(|(_, effect_body)| *effect_body)
}

//==============================================================================
/// 효과 프로그램 반환. (없으면 컴파일해 Graphic 캐시에 담는다. 실패했거나 본문이 없으면 None)
//==============================================================================
fn resolve_effect_program(graphic: &mut Graphic, effect: ShaderSpriteEffect) -> Option<Rc<ShaderProgram>> {
	let effect_body = find_effect_body(effect)?;
	graphic.resolve_shader_program(PROGRAM_GROUP_NAME, effect.get_name(), || {
		let fragment_shader_source = FRAGMENTSHADER_TEMPLATE.replace("EFFECT_BODY", effect_body);
		(VERTEXSHADER_SOURCE.to_string(), fragment_shader_source)
	})
}

//==============================================================================
/// 셰이더 스프라이트 컴포넌트.
/// - Sprite 를 품고 Deref 로 Sprite 함수를 그대로 부른다. 효과가 있으면 효과 프로그램으로 바꿔 끼워 그린다.
///
/// # 예제
/// 이미지 스프라이트에 디졸브 효과를 반쯤 걸기
/// ```rust
/// use vanilla::{Image, NodeTree, ShaderSprite, ShaderSpriteEffect};
///
/// let image = Image::create_from_pixels(32, 32, vec![255u8; 32 * 32 * 4]);
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let hero_id = node_tree.create_child(root_id, "Hero");
/// let shader_sprite = node_tree[hero_id].add_component(ShaderSprite::create_with_image(image));
/// shader_sprite.set_effect(ShaderSpriteEffect::Dissolve);
/// shader_sprite.set_effect_progress(0.5);
/// assert_eq!(shader_sprite.get_effect_parameters(), [8.0, 0.08, 0.0, 0.0]);
/// assert_eq!(shader_sprite.get_effect_progress(), 0.5);
/// assert!(shader_sprite.get_image().is_some());
/// ```
/// <!-- image: shader-sprite.webp -->
//==============================================================================
pub struct ShaderSprite {
	sprite: Sprite,
	effect: ShaderSpriteEffect,
	effect_progress: f32,
	effect_time: f32,
	effect_seed: f32,
	effect_color: Color,
	effect_parameters: [f32; 4],
}

#[crate::reflect(base = sprite)]
impl ShaderSprite {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 이미지가 없는 Sprite 를 품은 셰이더 스프라이트. (효과 없음, 진행도 0, 효과 시간 0, 시드 0 ~ 100 무작위, 효과 색 흰색, 파라미터 0)
	//==============================================================================
	pub fn new() -> ShaderSprite {
		ShaderSprite {
			sprite: Sprite::new(),
			effect: ShaderSpriteEffect::None,
			effect_progress: 0.0,
			effect_time: 0.0,
			effect_seed: math::random() * 100.0,
			effect_color: Color::white(),
			effect_parameters: [0.0, 0.0, 0.0, 0.0],
		}
	}

	//==============================================================================
	/// 이미지와 함께 생성.
	///
	/// # 인자
	/// - `image`: 품은 Sprite 에 걸 이미지.
	///
	/// # 돌려줌
	/// 그 이미지를 건 셰이더 스프라이트. (나머지는 new 와 같다)
	//==============================================================================
	pub fn create_with_image(image: Rc<Image>) -> ShaderSprite {
		let mut shader_sprite = ShaderSprite::new();
		shader_sprite.sprite.set_image(Some(image));
		shader_sprite
	}

	//==============================================================================
	/// 품은 Sprite 반환.
	///
	/// # 돌려줌
	/// 품은 Sprite 의 참조.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_sprite(&self) -> &Sprite {
		&self.sprite
	}

	//==============================================================================
	/// 품은 Sprite 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 Sprite 의 변경 참조.
	//==============================================================================
	pub fn get_sprite_mut(&mut self) -> &mut Sprite {
		&mut self.sprite
	}

	//==============================================================================
	/// 효과 설정. (효과별 기본 파라미터 / 색으로 되돌린다)
	/// - 진행도와 효과 시간은 그대로 둔다. 파라미터나 색을 바꾸려면 이것 다음에 부른다.
	///
	/// # 인자
	/// - `effect`: 걸 효과. (None 이면 효과 없이 Sprite 로 그린다)
	///
	/// # 예제
	/// 효과를 고른 뒤 두께와 색 바꾸기
	/// ```rust
	/// use vanilla::{Color, ShaderSprite, ShaderSpriteEffect};
	///
	/// let mut shader_sprite = ShaderSprite::new();
	/// shader_sprite.set_effect(ShaderSpriteEffect::Glow);
	/// assert_eq!(shader_sprite.get_effect_parameters(), [6.0, 0.0, 0.0, 0.0]);
	/// assert_eq!(shader_sprite.get_effect_color(), Color::create(1.0, 0.85, 0.4, 1.0));
	/// shader_sprite.set_effect(ShaderSpriteEffect::Outline);
	/// shader_sprite.set_effect_parameters(4.0, 0.0, 0.0, 0.0);
	/// shader_sprite.set_effect_color(Color::create(1.0, 0.0, 0.0, 1.0));
	/// assert_eq!(shader_sprite.get_effect_parameters(), [4.0, 0.0, 0.0, 0.0]);
	/// assert_eq!(shader_sprite.get_effect(), ShaderSpriteEffect::Outline);
	/// ```
	//==============================================================================
	pub fn set_effect(&mut self, effect: ShaderSpriteEffect) {
		self.effect = effect;
		self.effect_parameters = ShaderSprite::get_default_parameters(effect);
		self.effect_color = ShaderSprite::get_default_color(effect);
	}

	//==============================================================================
	/// 효과 반환.
	///
	/// # 돌려줌
	/// 지금 효과. (기본 None)
	//==============================================================================
	pub fn get_effect(&self) -> ShaderSpriteEffect {
		self.effect
	}

	//==============================================================================
	/// 진행도 설정. (0 ~ 1)
	///
	/// # 인자
	/// - `effect_progress`: 효과 진행도. (0 ~ 1 로 자른다, 뜻은 효과마다 다르다. 세기나 사라진 정도)
	//==============================================================================
	pub fn set_effect_progress(&mut self, effect_progress: f32) {
		self.effect_progress = effect_progress.clamp(0.0, 1.0);
	}

	//==============================================================================
	/// 진행도 반환.
	///
	/// # 돌려줌
	/// 효과 진행도. (0 ~ 1)
	//==============================================================================
	pub fn get_effect_progress(&self) -> f32 {
		self.effect_progress
	}

	//==============================================================================
	/// 효과 시간 설정.
	///
	/// # 인자
	/// - `effect_time`: 효과 시간. (초, 물결이나 깜빡임처럼 움직이는 효과가 쓴다. tick 마다 time_delta 만큼 는다)
	//==============================================================================
	pub fn set_effect_time(&mut self, effect_time: f32) {
		self.effect_time = effect_time;
	}

	//==============================================================================
	/// 효과 시간 반환.
	///
	/// # 돌려줌
	/// 효과 시간. (초)
	//==============================================================================
	pub fn get_effect_time(&self) -> f32 {
		self.effect_time
	}

	//==============================================================================
	/// 시드 설정. (같은 노이즈 무늬를 원할 때 고정)
	///
	/// # 인자
	/// - `effect_seed`: 노이즈 씨앗 값. (기본은 생성 때 0 ~ 100 무작위)
	//==============================================================================
	pub fn set_effect_seed(&mut self, effect_seed: f32) {
		self.effect_seed = effect_seed;
	}

	//==============================================================================
	/// 시드 반환.
	///
	/// # 돌려줌
	/// 노이즈 씨앗 값.
	//==============================================================================
	pub fn get_effect_seed(&self) -> f32 {
		self.effect_seed
	}

	//==============================================================================
	/// 효과 색 설정. (Color 또는 "#rrggbb" 같은 문자열)
	///
	/// # 인자
	/// - `color`: 효과 색. (뜻은 효과마다 다르다. 선 색, 빛 색, 틴트 같은 것)
	//==============================================================================
	pub fn set_effect_color(&mut self, color: impl Into<Color>) {
		self.effect_color = color.into();
	}

	//==============================================================================
	/// 효과 색 반환.
	///
	/// # 돌려줌
	/// 효과 색. (set_effect 가 효과별 기본 색으로 되돌린다)
	//==============================================================================
	pub fn get_effect_color(&self) -> Color {
		self.effect_color
	}

	//==============================================================================
	/// 효과 파라미터 설정. (효과마다 뜻이 다르다. ShaderSpriteEffect 주석 참고, 원본의 y / z / w 기본값 0 은 직접 넘긴다)
	///
	/// # 인자
	/// - `x`: 첫째 파라미터. (Outline 의 두께 픽셀 같은 것)
	/// - `y`: 둘째 파라미터. (쓰지 않는 효과는 0)
	/// - `z`: 셋째 파라미터. (쓰지 않는 효과는 0)
	/// - `w`: 넷째 파라미터. (쓰지 않는 효과는 0)
	//==============================================================================
	pub fn set_effect_parameters(&mut self, x: f32, y: f32, z: f32, w: f32) {
		self.effect_parameters = [x, y, z, w];
	}

	//==============================================================================
	/// 효과 파라미터 반환.
	///
	/// # 돌려줌
	/// 파라미터 [x, y, z, w].
	//==============================================================================
	pub fn get_effect_parameters(&self) -> [f32; 4] {
		self.effect_parameters
	}

	//==============================================================================
	/// 효과 기본 파라미터 반환. (정적)
	///
	/// # 인자
	/// - `effect`: 효과.
	///
	/// # 돌려줌
	/// 그 효과의 원본 기본 파라미터 [x, y, z, w]. (Glow 는 [6, 0, 0, 0])
	//==============================================================================
	pub fn get_default_parameters(effect: ShaderSpriteEffect) -> [f32; 4] {
		DEFAULT_PARAMETER_TABLE[effect as usize].1
	}

	//==============================================================================
	/// 효과 기본 색 반환. (정적)
	///
	/// # 인자
	/// - `effect`: 효과.
	///
	/// # 돌려줌
	/// 그 효과의 원본 기본 색. (색을 쓰지 않는 효과는 흰색)
	//==============================================================================
	pub fn get_default_color(effect: ShaderSpriteEffect) -> Color {
		let default_color = DEFAULT_COLOR_TABLE[effect as usize].1;
		Color::create(default_color[0], default_color[1], default_color[2], default_color[3])
	}
}

impl Default for ShaderSprite {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> ShaderSprite {
		ShaderSprite::new()
	}
}

impl Deref for ShaderSprite {
	type Target = Sprite;

	//==============================================================================
	/// 품은 Sprite 로 풀기. (Sprite 함수를 그대로 부르게 한다)
	//==============================================================================
	fn deref(&self) -> &Sprite {
		&self.sprite
	}
}

impl DerefMut for ShaderSprite {
	//==============================================================================
	/// 품은 Sprite 로 풀기. (변경용)
	//==============================================================================
	fn deref_mut(&mut self) -> &mut Sprite {
		&mut self.sprite
	}
}

#[crate::reflect]
impl Component for ShaderSprite {
	//==============================================================================
	/// 품은 기반 컴포넌트. (원본 상위 클래스 Sprite 자리, get_component::<Sprite>() 로도 찾힌다)
	//==============================================================================
	fn get_base_component(&self) -> Option<&dyn Component> {
		Some(&self.sprite)
	}

	//==============================================================================
	/// 품은 기반 컴포넌트. (변경용)
	//==============================================================================
	fn get_base_component_mut(&mut self) -> Option<&mut dyn Component> {
		Some(&mut self.sprite)
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (품은 Sprite 의 것)
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		self.sprite.get_component_base()
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용, 품은 Sprite 의 것)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.sprite.get_component_base_mut()
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("ShaderSprite")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"ShaderSprite"
	}

	/// 효과 시간을 흘려야 하므로 Sprite 와 달리 tick 을 받는다.
	#[reflect(after = "getEffectParameters")]
	fn is_tick_needed(&self) -> bool {
		true
	}

	/// 효과는 이미지 쿼드 안에서만 그려지므로 Sprite 와 같다.
	fn is_drawn_inside_content(&self) -> bool {
		self.sprite.is_drawn_inside_content()
	}

	//==============================================================================
	/// 갱신. (효과 시간 누적)
	//==============================================================================
	fn tick(&mut self, _context: &mut ComponentContext, time_delta: f32) {
		self.effect_time += time_delta;
	}

	//==============================================================================
	/// 출력. (효과가 있으면 프로그램을 바꿔 끼우고 Sprite 출력 경로를 그대로 탄다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let image = match self.sprite.get_image() {
			Some(image) => {
				image.clone()
			}
			None => {
				self.sprite.draw(transform, graphic);
				return;
			}
		};
		let effect = self.get_effect();
		if effect == ShaderSpriteEffect::None {
			self.sprite.draw(transform, graphic);
			return;
		}
		let shader_program = match resolve_effect_program(graphic, effect) {
			Some(shader_program) => {
				shader_program
			}
			None => {
				self.sprite.draw(transform, graphic);
				return;
			}
		};
		graphic.set_shader_program_override(Some(shader_program.clone()));

		// 효과 유니폼. (바꿔 끼운 프로그램이 사용 중이다)
		let mut image_rect = self.sprite.get_image_rect();
		if image_rect.is_zero() {
			image_rect = Rect::create(0.0, 0.0, image.get_width() as f32, image.get_height() as f32);
		}
		let image_width = image.get_width().max(1) as f32;
		let image_height = image.get_height().max(1) as f32;
		let effect_color = self.get_effect_color();
		let effect_parameters = self.get_effect_parameters();
		let gl = graphic.get_gl();
		unsafe {
			gl.uniform_1_f32(shader_program.get_uniform_location(gl, "effectTime").as_ref(), self.get_effect_time());
			gl.uniform_1_f32(shader_program.get_uniform_location(gl, "effectProgress").as_ref(), self.get_effect_progress());
			gl.uniform_1_f32(shader_program.get_uniform_location(gl, "effectSeed").as_ref(), self.get_effect_seed());
			gl.uniform_4_f32(shader_program.get_uniform_location(gl, "effectColor").as_ref(), effect_color.red, effect_color.green, effect_color.blue, effect_color.alpha);
			gl.uniform_4_f32(shader_program.get_uniform_location(gl, "effectParameters").as_ref(), effect_parameters[0], effect_parameters[1], effect_parameters[2], effect_parameters[3]);
			gl.uniform_4_f32(
				shader_program.get_uniform_location(gl, "textureRect").as_ref(),
				image_rect.position.x / image_width,
				image_rect.position.y / image_height,
				(image_rect.position.x + image_rect.size.x) / image_width,
				(image_rect.position.y + image_rect.size.y) / image_height,
			);
			gl.uniform_2_f32(shader_program.get_uniform_location(gl, "texelSize").as_ref(), 1.0 / image_width, 1.0 / image_height);
		}

		self.sprite.draw(transform, graphic);
		graphic.set_shader_program_override(None);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 표들이 열거형 선언 차례와 맞는지. (get_name, 기본값 표는 효과 번호로 바로 찾는다)
	//==============================================================================
	#[test]
	fn tables_follow_enum_order() {
		for (effect_index, effect) in EFFECT_LIST.iter().enumerate() {
			assert_eq!(*effect as usize, effect_index);
			assert_eq!(EFFECT_NAME_TABLE[effect_index].0, *effect);
			assert_eq!(DEFAULT_PARAMETER_TABLE[effect_index].0, *effect);
			assert_eq!(DEFAULT_COLOR_TABLE[effect_index].0, *effect);
			assert_eq!(ShaderSpriteEffect::from_name(effect.get_name()), Some(*effect));
		}
	}

	//==============================================================================
	/// none 을 뺀 효과는 모두 본문이 있다.
	//==============================================================================
	#[test]
	fn every_effect_has_body() {
		for effect in EFFECT_LIST.iter() {
			let has_body = find_effect_body(*effect).is_some();
			assert_eq!(has_body, *effect != ShaderSpriteEffect::None, "{:?}", effect);
		}
	}

	//==============================================================================
	/// set_effect 는 기본 파라미터 / 색으로 되돌리고, 진행도는 0 ~ 1 로 자른다.
	//==============================================================================
	#[test]
	fn set_effect_resets_defaults() {
		let mut shader_sprite = ShaderSprite::new();
		shader_sprite.set_effect_parameters(9.0, 9.0, 9.0, 9.0);
		shader_sprite.set_effect(ShaderSpriteEffect::Glow);
		assert_eq!(shader_sprite.get_effect_parameters(), [6.0, 0.0, 0.0, 0.0]);
		assert_eq!(shader_sprite.get_effect_color(), Color::create(1.0, 0.85, 0.4, 1.0));
		shader_sprite.set_effect_progress(1.5);
		assert_eq!(shader_sprite.get_effect_progress(), 1.0);
		shader_sprite.set_effect_progress(-1.0);
		assert_eq!(shader_sprite.get_effect_progress(), 0.0);
	}
}

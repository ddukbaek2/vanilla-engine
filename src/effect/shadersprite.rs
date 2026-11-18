//==============================================================================
// 셰이더 스프라이트 컴포넌트.
// - 원본 effect/shadersprite.js 를 옮긴 것이다. Sprite 와 같지만 프래그먼트 셰이더 효과(디졸브 / 섬광 / 외곽선 / 홀로그램 / 물결 / 모자이크 / 글리치 / 광택 / 실루엣 / 무채색 / 색상 회전 / 와이프 / 아이리스 / 색수차)를 건다.
// - 효과의 시간은 스스로 흐르고(tick), 진행도(progress)는 트윈 / 타임라인이 밀어 넣는다.
// - 효과 이름, 기본 파라미터, 기본 색, GLSL 본문은 원본 표를 그대로 옮겼다. 셰이더는 GLSL ES 3.00 이고, 데스크탑은 ShaderProgram 이 머리만 #version 330 core 로 바꾼다.
// - 그리기: 효과가 있으면 Graphic.setShaderProgramOverride 로 효과 프로그램을 바꿔 끼우고 효과 유니폼을 올린 뒤 Sprite 출력 경로를 그대로 탄다.
//   바꿔 끼운 동안은 묶어 그리지 않는다. (효과 없는 ShaderSprite 는 Sprite 와 같이 묶인다)
// - 사용:
//     let shaderSprite = node.addComponent(ShaderSprite::createWithImage(image));
//     shaderSprite.setEffect(ShaderSpriteEffect::dissolve);
//     shaderSprite.setEffectProgress(0.5);
// - 원본과 다른 점:
//   - 원본은 Sprite 를 상속한다. 러스트 판은 Sprite 를 품고 Deref / DerefMut 로 Sprite 함수(setImage, setImageRect, setColor ...)를 그대로 부르게 한다.
//     그래서 getComponent::<Sprite>() 로는 찾지 못하고 getComponent::<ShaderSprite>() 로 찾는다.
//   - 효과는 문자열 대신 ShaderSpriteEffect 열거형이다. (원본 문자열 이름은 getName / fromName)
//   - 효과 프로그램 캐시는 원본의 컨텍스트별 WeakMap 대신 Graphic.resolveShaderProgram 이 든다.
//   - 프로그램 만들기에 실패하면 원본은 예외를 던지고, 러스트 판은 한 번 알리고 효과 없이 Sprite 로 그린다.
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
// 셰이더 스프라이트 효과 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderSpriteEffect {
	none,
	// --- 색 ---
	flash,         // 실루엣을 단색으로 덮음. (피격 섬광: progress = 세기, color = 덮을 색)
	silhouette,    // 실루엣 단색. (progress = 세기, color = 색: 그림자 / 잔상)
	colorize,      // 곱셈 틴트. (progress = 세기, color = 곱할 색)
	grayscale,     // 무채색. (progress = 세기, color = 틴트)
	sepia,         // 세피아.
	invert,        // 색 반전.
	hueShift,      // 색상 회전. (progress 0 → 1 = 0 → 360도)
	colorAdjust,   // 대비 / 채도 / 밝기. (parameters = [대비, 채도, 밝기], progress = 적용 비율)
	posterize,     // 포스터화. (parameters = [단계 수])
	threshold,     // 흑백 문턱. (parameters = [문턱])
	gradientMap,   // 밝기를 두 색 사이로 매핑. (parameters.xyz = 어두운 색, color = 밝은 색)
	// --- 외곽 ---
	outline,       // 실루엣 바깥 외곽선. (color = 선 색, parameters = [두께 픽셀])
	innerOutline,  // 실루엣 안쪽 외곽선. (color = 선 색, parameters = [두께 픽셀])
	glow,          // 바깥 발광. (color = 빛 색, parameters = [반지름 픽셀])
	innerGlow,     // 안쪽 발광. (color = 빛 색, parameters = [반지름 픽셀])
	shadow,        // 드롭 섀도. (color = 그림자 색, parameters = [오프셋 x 픽셀, 오프셋 y 픽셀])
	edgeDetect,    // 윤곽 검출. (color = 선 색)
	emboss,        // 엠보스.
	sharpen,       // 샤픈.
	// --- 흐림 ---
	blur,          // 가우시안 블러. (parameters = [반지름 픽셀])
	motionBlur,    // 방향 블러. (parameters = [방향 x, 방향 y, 길이 픽셀])
	radialBlur,    // 중심 방사 블러. (parameters = [길이 0 ~ 1])
	// --- 왜곡 ---
	wave,          // 가로 물결 왜곡. (parameters = [빈도, 속도, 진폭])
	flag,          // 깃발 펄럭임. (parameters = [빈도, 속도, 진폭])
	ripple,        // 중심 파문. (parameters = [빈도, 속도, 진폭])
	heatHaze,      // 아지랑이. (parameters = [노이즈 배율, 속도, 진폭])
	bulge,         // 볼록 / 오목. (parameters = [세기: 음수면 오목])
	swirl,         // 소용돌이. (parameters = [회전 수])
	mirror,        // 좌우 대칭. (parameters = [0 가로 / 1 세로])
	kaleidoscope,  // 만화경. (parameters = [조각 수])
	jitter,        // 흔들림. (parameters = [진폭 픽셀, 초당 변화])
	pixelate,      // 모자이크. (parameters = [가장 거친 블록 수])
	glitch,        // 가로 줄 어긋남 + 색 분리. (parameters = [줄 수, 초당 변화, 어긋남 폭])
	chromatic,     // 색수차. (parameters = [분리 픽셀])
	// --- 화면 무늬 ---
	hologram,      // 주사선 + 틴트 + 깜빡임. (color = 틴트, parameters = [주사선 수, 흐르는 속도])
	scanlines,     // 주사선. (parameters = [주사선 수])
	oldFilm,       // 낡은 필름. (세피아 + 그레인 + 스크래치 + 깜빡임)
	vignette,      // 비네트. (color = 가장자리 색, parameters = [시작, 끝])
	shine,         // 대각선 하이라이트 띠가 지나감. (color = 띠 색, parameters = [띠 폭])
	// --- 사라짐 / 드러남 (progress 0 → 1) ---
	dissolve,      // 노이즈로 타 들어가며 사라짐. (color = 가장자리 색, parameters = [노이즈 배율, 가장자리 폭])
	burn,          // 불에 타듯 사라짐. (안쪽 밝은 띠 + 바깥 그을음, parameters = [노이즈 배율, 띠 폭])
	noiseFade,     // 노이즈 알파 페이드. (parameters = [노이즈 배율, 부드러움])
	pixelDissolve, // 블록 단위로 사라짐. (parameters = [블록 수])
	wipe,          // 방향 와이프로 드러남. (parameters = [방향 x, 방향 y, 부드러움])
	iris,          // 원형 아이리스로 드러남. (parameters = [중심 x, 중심 y, 부드러움])
	diamondWipe,   // 마름모 와이프. (parameters = [부드러움])
	clockWipe,     // 시계 방향 와이프. (parameters = [부드러움])
	blinds,        // 블라인드. (parameters = [줄 수, 부드러움])
	checkerWipe,   // 체커 보드. (parameters = [칸 수])
}

// 효과 목록. (선언 차례, 아래 표들의 차례와 같다)
const EFFECT_LIST: [ShaderSpriteEffect; 50] = [
	ShaderSpriteEffect::none, ShaderSpriteEffect::flash, ShaderSpriteEffect::silhouette, ShaderSpriteEffect::colorize, ShaderSpriteEffect::grayscale, ShaderSpriteEffect::sepia,
	ShaderSpriteEffect::invert, ShaderSpriteEffect::hueShift, ShaderSpriteEffect::colorAdjust, ShaderSpriteEffect::posterize, ShaderSpriteEffect::threshold, ShaderSpriteEffect::gradientMap,
	ShaderSpriteEffect::outline, ShaderSpriteEffect::innerOutline, ShaderSpriteEffect::glow, ShaderSpriteEffect::innerGlow, ShaderSpriteEffect::shadow, ShaderSpriteEffect::edgeDetect,
	ShaderSpriteEffect::emboss, ShaderSpriteEffect::sharpen, ShaderSpriteEffect::blur, ShaderSpriteEffect::motionBlur, ShaderSpriteEffect::radialBlur, ShaderSpriteEffect::wave,
	ShaderSpriteEffect::flag, ShaderSpriteEffect::ripple, ShaderSpriteEffect::heatHaze, ShaderSpriteEffect::bulge, ShaderSpriteEffect::swirl, ShaderSpriteEffect::mirror,
	ShaderSpriteEffect::kaleidoscope, ShaderSpriteEffect::jitter, ShaderSpriteEffect::pixelate, ShaderSpriteEffect::glitch, ShaderSpriteEffect::chromatic, ShaderSpriteEffect::hologram,
	ShaderSpriteEffect::scanlines, ShaderSpriteEffect::oldFilm, ShaderSpriteEffect::vignette, ShaderSpriteEffect::shine, ShaderSpriteEffect::dissolve, ShaderSpriteEffect::burn,
	ShaderSpriteEffect::noiseFade, ShaderSpriteEffect::pixelDissolve, ShaderSpriteEffect::wipe, ShaderSpriteEffect::iris, ShaderSpriteEffect::diamondWipe, ShaderSpriteEffect::clockWipe,
	ShaderSpriteEffect::blinds, ShaderSpriteEffect::checkerWipe,
];

// 효과별 원본 이름.
const EFFECT_NAME_TABLE: [(ShaderSpriteEffect, &str); 50] = [
	(ShaderSpriteEffect::none, "none"),
	(ShaderSpriteEffect::flash, "flash"),
	(ShaderSpriteEffect::silhouette, "silhouette"),
	(ShaderSpriteEffect::colorize, "colorize"),
	(ShaderSpriteEffect::grayscale, "grayscale"),
	(ShaderSpriteEffect::sepia, "sepia"),
	(ShaderSpriteEffect::invert, "invert"),
	(ShaderSpriteEffect::hueShift, "hueShift"),
	(ShaderSpriteEffect::colorAdjust, "colorAdjust"),
	(ShaderSpriteEffect::posterize, "posterize"),
	(ShaderSpriteEffect::threshold, "threshold"),
	(ShaderSpriteEffect::gradientMap, "gradientMap"),
	(ShaderSpriteEffect::outline, "outline"),
	(ShaderSpriteEffect::innerOutline, "innerOutline"),
	(ShaderSpriteEffect::glow, "glow"),
	(ShaderSpriteEffect::innerGlow, "innerGlow"),
	(ShaderSpriteEffect::shadow, "shadow"),
	(ShaderSpriteEffect::edgeDetect, "edgeDetect"),
	(ShaderSpriteEffect::emboss, "emboss"),
	(ShaderSpriteEffect::sharpen, "sharpen"),
	(ShaderSpriteEffect::blur, "blur"),
	(ShaderSpriteEffect::motionBlur, "motionBlur"),
	(ShaderSpriteEffect::radialBlur, "radialBlur"),
	(ShaderSpriteEffect::wave, "wave"),
	(ShaderSpriteEffect::flag, "flag"),
	(ShaderSpriteEffect::ripple, "ripple"),
	(ShaderSpriteEffect::heatHaze, "heatHaze"),
	(ShaderSpriteEffect::bulge, "bulge"),
	(ShaderSpriteEffect::swirl, "swirl"),
	(ShaderSpriteEffect::mirror, "mirror"),
	(ShaderSpriteEffect::kaleidoscope, "kaleidoscope"),
	(ShaderSpriteEffect::jitter, "jitter"),
	(ShaderSpriteEffect::pixelate, "pixelate"),
	(ShaderSpriteEffect::glitch, "glitch"),
	(ShaderSpriteEffect::chromatic, "chromatic"),
	(ShaderSpriteEffect::hologram, "hologram"),
	(ShaderSpriteEffect::scanlines, "scanlines"),
	(ShaderSpriteEffect::oldFilm, "oldFilm"),
	(ShaderSpriteEffect::vignette, "vignette"),
	(ShaderSpriteEffect::shine, "shine"),
	(ShaderSpriteEffect::dissolve, "dissolve"),
	(ShaderSpriteEffect::burn, "burn"),
	(ShaderSpriteEffect::noiseFade, "noiseFade"),
	(ShaderSpriteEffect::pixelDissolve, "pixelDissolve"),
	(ShaderSpriteEffect::wipe, "wipe"),
	(ShaderSpriteEffect::iris, "iris"),
	(ShaderSpriteEffect::diamondWipe, "diamondWipe"),
	(ShaderSpriteEffect::clockWipe, "clockWipe"),
	(ShaderSpriteEffect::blinds, "blinds"),
	(ShaderSpriteEffect::checkerWipe, "checkerWipe"),
];

// 효과별 기본 파라미터.
const DEFAULT_PARAMETER_TABLE: [(ShaderSpriteEffect, [f32; 4]); 50] = [
	(ShaderSpriteEffect::none, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::flash, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::silhouette, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::colorize, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::grayscale, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::sepia, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::invert, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::hueShift, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::colorAdjust, [1.4, 1.3, 1.1, 0.0]),
	(ShaderSpriteEffect::posterize, [4.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::threshold, [0.5, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::gradientMap, [0.1, 0.05, 0.25, 0.0]),
	(ShaderSpriteEffect::outline, [2.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::innerOutline, [2.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::glow, [6.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::innerGlow, [5.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::shadow, [6.0, 6.0, 0.0, 0.0]),
	(ShaderSpriteEffect::edgeDetect, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::emboss, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::sharpen, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::blur, [3.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::motionBlur, [1.0, 0.0, 12.0, 0.0]),
	(ShaderSpriteEffect::radialBlur, [0.12, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::wave, [12.0, 4.0, 0.03, 0.0]),
	(ShaderSpriteEffect::flag, [8.0, 5.0, 0.06, 0.0]),
	(ShaderSpriteEffect::ripple, [40.0, 6.0, 0.02, 0.0]),
	(ShaderSpriteEffect::heatHaze, [6.0, 1.5, 0.02, 0.0]),
	(ShaderSpriteEffect::bulge, [0.6, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::swirl, [1.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::mirror, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::kaleidoscope, [6.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::jitter, [4.0, 24.0, 0.0, 0.0]),
	(ShaderSpriteEffect::pixelate, [24.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::glitch, [18.0, 12.0, 0.15, 0.0]),
	(ShaderSpriteEffect::chromatic, [3.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::hologram, [120.0, 6.0, 0.0, 0.0]),
	(ShaderSpriteEffect::scanlines, [90.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::oldFilm, [0.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::vignette, [0.4, 1.0, 0.0, 0.0]),
	(ShaderSpriteEffect::shine, [0.15, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::dissolve, [8.0, 0.08, 0.0, 0.0]),
	(ShaderSpriteEffect::burn, [8.0, 0.06, 0.0, 0.0]),
	(ShaderSpriteEffect::noiseFade, [6.0, 0.2, 0.0, 0.0]),
	(ShaderSpriteEffect::pixelDissolve, [16.0, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::wipe, [1.0, 0.0, 0.05, 0.0]),
	(ShaderSpriteEffect::iris, [0.5, 0.5, 0.05, 0.0]),
	(ShaderSpriteEffect::diamondWipe, [0.05, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::clockWipe, [0.02, 0.0, 0.0, 0.0]),
	(ShaderSpriteEffect::blinds, [8.0, 0.05, 0.0, 0.0]),
	(ShaderSpriteEffect::checkerWipe, [8.0, 0.0, 0.0, 0.0]),
];

// 효과별 기본 색.
const DEFAULT_COLOR_TABLE: [(ShaderSpriteEffect, [f32; 4]); 50] = [
	(ShaderSpriteEffect::none, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::flash, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::silhouette, [0.0, 0.0, 0.0, 1.0]),
	(ShaderSpriteEffect::colorize, [1.0, 0.5, 0.5, 1.0]),
	(ShaderSpriteEffect::grayscale, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::sepia, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::invert, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::hueShift, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::colorAdjust, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::posterize, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::threshold, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::gradientMap, [1.0, 0.85, 0.5, 1.0]),
	(ShaderSpriteEffect::outline, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::innerOutline, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::glow, [1.0, 0.85, 0.4, 1.0]),
	(ShaderSpriteEffect::innerGlow, [1.0, 0.85, 0.4, 1.0]),
	(ShaderSpriteEffect::shadow, [0.0, 0.0, 0.0, 0.6]),
	(ShaderSpriteEffect::edgeDetect, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::emboss, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::sharpen, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::blur, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::motionBlur, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::radialBlur, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::wave, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::flag, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::ripple, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::heatHaze, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::bulge, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::swirl, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::mirror, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::kaleidoscope, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::jitter, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::pixelate, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::glitch, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::chromatic, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::hologram, [0.4, 0.9, 1.0, 1.0]),
	(ShaderSpriteEffect::scanlines, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::oldFilm, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::vignette, [0.0, 0.0, 0.0, 1.0]),
	(ShaderSpriteEffect::shine, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::dissolve, [1.0, 0.6, 0.2, 1.0]),
	(ShaderSpriteEffect::burn, [1.0, 0.7, 0.25, 1.0]),
	(ShaderSpriteEffect::noiseFade, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::pixelDissolve, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::wipe, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::iris, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::diamondWipe, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::clockWipe, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::blinds, [1.0, 1.0, 1.0, 1.0]),
	(ShaderSpriteEffect::checkerWipe, [1.0, 1.0, 1.0, 1.0]),
];

// 효과별 프래그먼트 본문. (uv / local / textureColor 를 바꾸거나 다시 샘플한다. 텍스처는 프리멀티플라이드 알파)
// - 공용: sampleSprite(uv) / sampleLocal(local) / unpremultiply(color) / luminanceAt(uv) / hash21 / valueNoise.
const EFFECT_BODY_TABLE: [(ShaderSpriteEffect, &str); 49] = [
	(ShaderSpriteEffect::flash, "
	textureColor.rgb = mix(textureColor.rgb, effectColor.rgb * textureColor.a, effectProgress * effectColor.a);
	"),
	(ShaderSpriteEffect::silhouette, "
	textureColor.rgb = mix(textureColor.rgb, effectColor.rgb * textureColor.a, effectProgress);
	textureColor *= mix(1.0, effectColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::colorize, "
	textureColor.rgb *= mix(vec3(1.0), effectColor.rgb, effectProgress * effectColor.a);
	"),
	(ShaderSpriteEffect::grayscale, "
	float gray = dot(textureColor.rgb, vec3(0.299, 0.587, 0.114));
	textureColor.rgb = mix(textureColor.rgb, vec3(gray) * effectColor.rgb, effectProgress);
	"),
	(ShaderSpriteEffect::sepia, "
	float gray = dot(textureColor.rgb, vec3(0.299, 0.587, 0.114));
	textureColor.rgb = mix(textureColor.rgb, gray * vec3(1.2, 1.0, 0.78), effectProgress);
	"),
	(ShaderSpriteEffect::invert, "
	textureColor.rgb = mix(textureColor.rgb, vec3(textureColor.a) - textureColor.rgb, effectProgress);
	"),
	(ShaderSpriteEffect::hueShift, "
	float hueAngle = effectProgress * 6.28318530718;
	vec3 yiq = mat3(0.299, 0.596, 0.211, 0.587, -0.274, -0.523, 0.114, -0.322, 0.312) * textureColor.rgb;
	float chroma = length(yiq.yz);
	float hue = atan(yiq.z, yiq.y) + hueAngle;
	yiq.yz = vec2(cos(hue), sin(hue)) * chroma;
	textureColor.rgb = mat3(1.0, 1.0, 1.0, 0.956, -0.272, -1.106, 0.621, -0.647, 1.703) * yiq;
	"),
	(ShaderSpriteEffect::colorAdjust, "
	vec3 straight = unpremultiply(textureColor);
	float gray = dot(straight, vec3(0.299, 0.587, 0.114));
	vec3 adjusted = mix(vec3(gray), straight, effectParameters.y);
	adjusted = (adjusted - 0.5) * effectParameters.x + 0.5;
	adjusted *= effectParameters.z;
	textureColor.rgb = mix(textureColor.rgb, clamp(adjusted, 0.0, 1.0) * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::posterize, "
	vec3 straight = unpremultiply(textureColor);
	float levels = max(effectParameters.x, 1.0);
	vec3 posterized = floor(straight * levels + 0.5) / levels;
	textureColor.rgb = mix(textureColor.rgb, posterized * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::threshold, "
	vec3 straight = unpremultiply(textureColor);
	float gray = dot(straight, vec3(0.299, 0.587, 0.114));
	vec3 binary = vec3(step(effectParameters.x, gray)) * effectColor.rgb;
	textureColor.rgb = mix(textureColor.rgb, binary * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::gradientMap, "
	vec3 straight = unpremultiply(textureColor);
	float gray = dot(straight, vec3(0.299, 0.587, 0.114));
	vec3 mapped = mix(effectParameters.xyz, effectColor.rgb, gray);
	textureColor.rgb = mix(textureColor.rgb, mapped * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::outline, "
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
	(ShaderSpriteEffect::innerOutline, "
	vec2 outlineStep = texelSize * effectParameters.x;
	float minimumAlpha = 1.0;
	for (int sampleIndex = 0; sampleIndex < 8; ++sampleIndex) {
		float angle = float(sampleIndex) * 0.78539816;
		minimumAlpha = min(minimumAlpha, sampleSprite(uv + vec2(cos(angle), sin(angle)) * outlineStep).a);
	}
	float edgeMask = clamp(textureColor.a - minimumAlpha, 0.0, 1.0) * effectProgress * effectColor.a;
	textureColor.rgb = mix(textureColor.rgb, effectColor.rgb * textureColor.a, edgeMask);
	"),
	(ShaderSpriteEffect::glow, "
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
	(ShaderSpriteEffect::innerGlow, "
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
	(ShaderSpriteEffect::shadow, "
	vec2 shadowOffset = effectParameters.xy * texelSize;
	float shadowAlpha = sampleSprite(uv - shadowOffset).a * effectColor.a * effectProgress * (1.0 - textureColor.a);
	textureColor.rgb += effectColor.rgb * shadowAlpha;
	textureColor.a += shadowAlpha;
	"),
	(ShaderSpriteEffect::edgeDetect, "
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
	(ShaderSpriteEffect::emboss, "
	vec3 lower = sampleSprite(uv - texelSize).rgb;
	vec3 upper = sampleSprite(uv + texelSize).rgb;
	float relief = dot(upper - lower, vec3(0.299, 0.587, 0.114)) * 2.0 + 0.5;
	textureColor.rgb = mix(textureColor.rgb, vec3(clamp(relief, 0.0, 1.0)) * textureColor.a, effectProgress);
	"),
	(ShaderSpriteEffect::sharpen, "
	vec4 neighbors = sampleSprite(uv + vec2(texelSize.x, 0.0)) + sampleSprite(uv - vec2(texelSize.x, 0.0)) + sampleSprite(uv + vec2(0.0, texelSize.y)) + sampleSprite(uv - vec2(0.0, texelSize.y));
	vec4 sharpened = clamp(textureColor * 5.0 - neighbors, 0.0, 1.0);
	textureColor = mix(textureColor, sharpened, effectProgress);
	"),
	(ShaderSpriteEffect::blur, "
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
	(ShaderSpriteEffect::motionBlur, "
	vec2 blurDirection = normalize(effectParameters.xy + vec2(0.0001, 0.0)) * texelSize * effectParameters.z * effectProgress;
	vec4 sum = vec4(0.0);
	for (int sampleIndex = 0; sampleIndex < 8; ++sampleIndex) {
		float offset = (float(sampleIndex) - 3.5) / 3.5;
		sum += sampleSprite(uv + blurDirection * offset);
	}
	textureColor = sum / 8.0;
	"),
	(ShaderSpriteEffect::radialBlur, "
	vec2 toCenter = (vec2(0.5) - local) * effectParameters.x * effectProgress;
	vec4 sum = vec4(0.0);
	for (int sampleIndex = 0; sampleIndex < 8; ++sampleIndex) {
		sum += sampleLocal(local + toCenter * (float(sampleIndex) / 8.0));
	}
	textureColor = sum / 8.0;
	"),
	(ShaderSpriteEffect::wave, "
	float waveOffset = sin(local.y * effectParameters.x + effectTime * effectParameters.y) * effectParameters.z * effectProgress;
	textureColor = sampleLocal(local + vec2(waveOffset, 0.0));
	"),
	(ShaderSpriteEffect::flag, "
	float flagOffset = sin(local.x * effectParameters.x - effectTime * effectParameters.y) * effectParameters.z * effectProgress * local.x;
	textureColor = sampleLocal(local + vec2(0.0, flagOffset));
	"),
	(ShaderSpriteEffect::ripple, "
	vec2 fromCenter = local - 0.5;
	float rippleDistance = length(fromCenter);
	float rippleWave = sin(rippleDistance * effectParameters.x - effectTime * effectParameters.y) * effectParameters.z * effectProgress;
	vec2 rippleDirection = rippleDistance > 0.0001 ? fromCenter / rippleDistance : vec2(0.0);
	textureColor = sampleLocal(local + rippleDirection * rippleWave);
	"),
	(ShaderSpriteEffect::heatHaze, "
	vec2 hazeOffset = vec2(valueNoise(local * effectParameters.x + vec2(0.0, effectTime * effectParameters.y)), valueNoise(local * effectParameters.x + vec2(effectTime * effectParameters.y, 7.3))) - 0.5;
	textureColor = sampleLocal(local + hazeOffset * effectParameters.z * effectProgress);
	"),
	(ShaderSpriteEffect::bulge, "
	vec2 fromCenter = local - 0.5;
	float bulgeRadius = length(fromCenter) * 2.0;
	float bulgeFactor = 1.0 - effectParameters.x * effectProgress * (1.0 - clamp(bulgeRadius, 0.0, 1.0));
	textureColor = sampleLocal(0.5 + fromCenter * bulgeFactor);
	"),
	(ShaderSpriteEffect::swirl, "
	vec2 fromCenter = local - 0.5;
	float swirlRadius = length(fromCenter) * 2.0;
	float swirlAngle = effectParameters.x * effectProgress * 6.28318530718 * pow(1.0 - clamp(swirlRadius, 0.0, 1.0), 2.0);
	float cosine = cos(swirlAngle);
	float sine = sin(swirlAngle);
	textureColor = sampleLocal(0.5 + vec2(fromCenter.x * cosine - fromCenter.y * sine, fromCenter.x * sine + fromCenter.y * cosine));
	"),
	(ShaderSpriteEffect::mirror, "
	vec2 mirrored = local;
	if (effectParameters.x < 0.5) {
		mirrored.x = local.x < 0.5 ? local.x : 1.0 - local.x;
	}
	else {
		mirrored.y = local.y < 0.5 ? local.y : 1.0 - local.y;
	}
	textureColor = mix(textureColor, sampleLocal(mirrored), step(0.5, effectProgress));
	"),
	(ShaderSpriteEffect::kaleidoscope, "
	vec2 fromCenter = local - 0.5;
	float kaleidoRadius = length(fromCenter);
	float segmentAngle = 6.28318530718 / max(effectParameters.x, 1.0);
	float kaleidoAngle = atan(fromCenter.y, fromCenter.x);
	kaleidoAngle = abs(mod(kaleidoAngle, segmentAngle) - segmentAngle * 0.5);
	vec4 kaleidoColor = sampleLocal(0.5 + vec2(cos(kaleidoAngle), sin(kaleidoAngle)) * kaleidoRadius);
	textureColor = mix(textureColor, kaleidoColor, effectProgress);
	"),
	(ShaderSpriteEffect::jitter, "
	float jitterSeed = floor(effectTime * effectParameters.y) + effectSeed;
	vec2 jitterOffset = (vec2(hash21(vec2(jitterSeed, 1.0)), hash21(vec2(jitterSeed, 2.0))) - 0.5) * effectParameters.x * effectProgress;
	textureColor = sampleSprite(uv + jitterOffset * texelSize);
	"),
	(ShaderSpriteEffect::pixelate, "
	float blockCount = mix(512.0, max(1.0, effectParameters.x), effectProgress);
	vec2 blockLocal = (floor(local * blockCount) + 0.5) / blockCount;
	textureColor = sampleLocal(blockLocal);
	"),
	(ShaderSpriteEffect::glitch, "
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
	(ShaderSpriteEffect::chromatic, "
	vec2 chromaticStep = vec2(texelSize.x * effectParameters.x * effectProgress, 0.0);
	textureColor.r = sampleSprite(uv + chromaticStep).r;
	textureColor.b = sampleSprite(uv - chromaticStep).b;
	"),
	(ShaderSpriteEffect::hologram, "
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
	(ShaderSpriteEffect::scanlines, "
	float lines = 0.5 + 0.5 * sin(local.y * effectParameters.x);
	textureColor.rgb *= 1.0 - effectProgress * 0.5 * lines;
	"),
	(ShaderSpriteEffect::oldFilm, "
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
	(ShaderSpriteEffect::vignette, "
	float vignetteDistance = distance(local, vec2(0.5)) * 1.4142;
	float vignetteMask = smoothstep(effectParameters.x, effectParameters.y, vignetteDistance) * effectProgress * effectColor.a;
	textureColor.rgb = mix(textureColor.rgb, effectColor.rgb * textureColor.a, vignetteMask);
	"),
	(ShaderSpriteEffect::shine, "
	float band = local.x * 0.8 + local.y * 0.5;
	float bandCenter = mix(-0.4, 1.7, effectProgress);
	float glow = 1.0 - smoothstep(0.0, max(effectParameters.x, 0.001), abs(band - bandCenter));
	textureColor.rgb += effectColor.rgb * glow * glow * textureColor.a * effectColor.a;
	"),
	(ShaderSpriteEffect::dissolve, "
	float dissolveNoise = valueNoise(local * effectParameters.x + vec2(effectSeed * 17.0, effectSeed * 31.0));
	float threshold = effectProgress * (1.0 + effectParameters.y * 2.0) - effectParameters.y;
	float edge = smoothstep(threshold, threshold + effectParameters.y, dissolveNoise);
	float keep = step(threshold, dissolveNoise);
	vec3 edgeColor = effectColor.rgb * textureColor.a * effectColor.a;
	textureColor.rgb = mix(edgeColor, textureColor.rgb, edge) * keep;
	textureColor.a *= keep;
	"),
	(ShaderSpriteEffect::burn, "
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
	(ShaderSpriteEffect::noiseFade, "
	float fadeNoise = valueNoise(local * effectParameters.x + vec2(effectSeed * 11.0, effectSeed * 23.0));
	float softness = max(effectParameters.y, 0.001);
	float fadeMask = smoothstep(effectProgress * (1.0 + softness) - softness, effectProgress * (1.0 + softness), fadeNoise);
	textureColor *= fadeMask;
	"),
	(ShaderSpriteEffect::pixelDissolve, "
	vec2 blockCell = floor(local * effectParameters.x);
	float blockRandom = hash21(blockCell + effectSeed);
	textureColor *= step(effectProgress, blockRandom);
	"),
	(ShaderSpriteEffect::wipe, "
	vec2 wipeDirection = normalize(effectParameters.xy + vec2(0.0001, 0.0));
	float wipeDistance = dot(local - 0.5, wipeDirection) + 0.5;
	float softness = max(effectParameters.z, 0.0001);
	float reveal = 1.0 - smoothstep(effectProgress * (1.0 + softness) - softness, effectProgress * (1.0 + softness), wipeDistance);
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::iris, "
	float irisRadius = effectProgress * 0.7072;
	float softness = max(effectParameters.z, 0.0001);
	float reveal = 1.0 - smoothstep(irisRadius, irisRadius + softness, distance(local, effectParameters.xy));
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::diamondWipe, "
	float diamondDistance = abs(local.x - 0.5) + abs(local.y - 0.5);
	float softness = max(effectParameters.x, 0.0001);
	float reveal = 1.0 - smoothstep(effectProgress * (1.0 + softness), effectProgress * (1.0 + softness) + softness, diamondDistance);
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::clockWipe, "
	vec2 fromCenter = local - 0.5;
	float clockAngle = (atan(fromCenter.x, -fromCenter.y) + 3.14159265) / 6.28318530718;
	float softness = max(effectParameters.x, 0.0001);
	float reveal = 1.0 - smoothstep(effectProgress, effectProgress + softness, clockAngle);
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::blinds, "
	float stripe = fract(local.y * effectParameters.x);
	float softness = max(effectParameters.y, 0.0001);
	float reveal = 1.0 - smoothstep(effectProgress, effectProgress + softness, stripe);
	textureColor *= reveal;
	"),
	(ShaderSpriteEffect::checkerWipe, "
	vec2 checkerCell = floor(local * effectParameters.x);
	float parity = mod(checkerCell.x + checkerCell.y, 2.0);
	float cellThreshold = parity * 0.5 + hash21(checkerCell + effectSeed) * 0.5;
	textureColor *= step(cellThreshold, effectProgress * 1.0001);
	"),
];

// 버텍스 셰이더. (Graphic 기본 프로그램과 동일)
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

// 프래그먼트 셰이더 틀. (효과 본문을 EFFECT_BODY 자리에 끼운다)
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

float hash21(vec2 point) {
	vec3 scrambled = fract(vec3(point.xyx) * 0.1031);
	scrambled += dot(scrambled, scrambled.yzx + 33.33);
	return fract((scrambled.x + scrambled.y) * scrambled.z);
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

// Graphic 프로그램 캐시에서 이 효과들이 쓰는 묶음 이름.
const PROGRAM_GROUP_NAME: &str = "ShaderSprite";

impl ShaderSpriteEffect {
	//==============================================================================
	// 원본 이름 반환. ("dissolve" 같은 문자열)
	//==============================================================================
	pub fn getName(&self) -> &'static str {
		EFFECT_NAME_TABLE[*self as usize].1
	}

	//==============================================================================
	// 원본 이름으로 찾기. (모르는 이름이면 None)
	//==============================================================================
	pub fn fromName(name: &str) -> Option<ShaderSpriteEffect> {
		EFFECT_NAME_TABLE.iter().find(|(_, effectName)| *effectName == name).map(|(effect, _)| *effect)
	}

	//==============================================================================
	// 효과 목록 반환. (선언 차례, 원본 Object.keys(ShaderSpriteEffect) 자리)
	//==============================================================================
	pub fn getEffectList() -> &'static [ShaderSpriteEffect] {
		&EFFECT_LIST
	}
}

//==============================================================================
// 효과 본문 반환. (none 이면 None)
//==============================================================================
fn findEffectBody(effect: ShaderSpriteEffect) -> Option<&'static str> {
	EFFECT_BODY_TABLE.iter().find(|(bodyEffect, _)| *bodyEffect == effect).map(|(_, effectBody)| *effectBody)
}

//==============================================================================
// 효과 프로그램 반환. (없으면 컴파일해 Graphic 캐시에 담는다. 실패했거나 본문이 없으면 None)
//==============================================================================
fn resolveEffectProgram(graphic: &mut Graphic, effect: ShaderSpriteEffect) -> Option<Rc<ShaderProgram>> {
	let effectBody = findEffectBody(effect)?;
	graphic.resolveShaderProgram(PROGRAM_GROUP_NAME, effect.getName(), || {
		let fragmentShaderSource = FRAGMENTSHADER_TEMPLATE.replace("EFFECT_BODY", effectBody);
		(VERTEXSHADER_SOURCE.to_string(), fragmentShaderSource)
	})
}

pub struct ShaderSprite {
	sprite: Sprite,
	effect: ShaderSpriteEffect,
	effectProgress: f32,
	effectTime: f32,
	effectSeed: f32,
	effectColor: Color,
	effectParameters: [f32; 4],
}

impl ShaderSprite {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> ShaderSprite {
		ShaderSprite {
			sprite: Sprite::new(),
			effect: ShaderSpriteEffect::none,
			effectProgress: 0.0,
			effectTime: 0.0,
			effectSeed: math::random() * 100.0,
			effectColor: Color::white(),
			effectParameters: [0.0, 0.0, 0.0, 0.0],
		}
	}

	//==============================================================================
	// 이미지와 함께 생성.
	//==============================================================================
	pub fn createWithImage(image: Rc<Image>) -> ShaderSprite {
		let mut shaderSprite = ShaderSprite::new();
		shaderSprite.sprite.setImage(Some(image));
		shaderSprite
	}

	//==============================================================================
	// 품은 Sprite 반환.
	//==============================================================================
	pub fn getSprite(&self) -> &Sprite {
		&self.sprite
	}

	//==============================================================================
	// 품은 Sprite 반환. (변경용)
	//==============================================================================
	pub fn getSpriteMut(&mut self) -> &mut Sprite {
		&mut self.sprite
	}

	//==============================================================================
	// 효과 설정. (효과별 기본 파라미터 / 색으로 되돌린다)
	//==============================================================================
	pub fn setEffect(&mut self, effect: ShaderSpriteEffect) {
		self.effect = effect;
		self.effectParameters = ShaderSprite::getDefaultParameters(effect);
		self.effectColor = ShaderSprite::getDefaultColor(effect);
	}

	//==============================================================================
	// 효과 반환.
	//==============================================================================
	pub fn getEffect(&self) -> ShaderSpriteEffect {
		self.effect
	}

	//==============================================================================
	// 진행도 설정. (0 ~ 1)
	//==============================================================================
	pub fn setEffectProgress(&mut self, effectProgress: f32) {
		self.effectProgress = effectProgress.clamp(0.0, 1.0);
	}

	//==============================================================================
	// 진행도 반환.
	//==============================================================================
	pub fn getEffectProgress(&self) -> f32 {
		self.effectProgress
	}

	//==============================================================================
	// 효과 시간 설정.
	//==============================================================================
	pub fn setEffectTime(&mut self, effectTime: f32) {
		self.effectTime = effectTime;
	}

	//==============================================================================
	// 효과 시간 반환.
	//==============================================================================
	pub fn getEffectTime(&self) -> f32 {
		self.effectTime
	}

	//==============================================================================
	// 시드 설정. (같은 노이즈 무늬를 원할 때 고정)
	//==============================================================================
	pub fn setEffectSeed(&mut self, effectSeed: f32) {
		self.effectSeed = effectSeed;
	}

	//==============================================================================
	// 시드 반환.
	//==============================================================================
	pub fn getEffectSeed(&self) -> f32 {
		self.effectSeed
	}

	//==============================================================================
	// 효과 색 설정. (Color 또는 "#rrggbb" 같은 문자열)
	//==============================================================================
	pub fn setEffectColor(&mut self, color: impl Into<Color>) {
		self.effectColor = color.into();
	}

	//==============================================================================
	// 효과 색 반환.
	//==============================================================================
	pub fn getEffectColor(&self) -> Color {
		self.effectColor
	}

	//==============================================================================
	// 효과 파라미터 설정. (효과마다 뜻이 다르다. ShaderSpriteEffect 주석 참고, 원본의 y / z / w 기본값 0 은 직접 넘긴다)
	//==============================================================================
	pub fn setEffectParameters(&mut self, x: f32, y: f32, z: f32, w: f32) {
		self.effectParameters = [x, y, z, w];
	}

	//==============================================================================
	// 효과 파라미터 반환.
	//==============================================================================
	pub fn getEffectParameters(&self) -> [f32; 4] {
		self.effectParameters
	}

	//==============================================================================
	// 효과 기본 파라미터 반환. (정적)
	//==============================================================================
	pub fn getDefaultParameters(effect: ShaderSpriteEffect) -> [f32; 4] {
		DEFAULT_PARAMETER_TABLE[effect as usize].1
	}

	//==============================================================================
	// 효과 기본 색 반환. (정적)
	//==============================================================================
	pub fn getDefaultColor(effect: ShaderSpriteEffect) -> Color {
		let defaultColor = DEFAULT_COLOR_TABLE[effect as usize].1;
		Color::create(defaultColor[0], defaultColor[1], defaultColor[2], defaultColor[3])
	}
}

impl Default for ShaderSprite {
	fn default() -> ShaderSprite {
		ShaderSprite::new()
	}
}

impl Deref for ShaderSprite {
	type Target = Sprite;

	fn deref(&self) -> &Sprite {
		&self.sprite
	}
}

impl DerefMut for ShaderSprite {
	fn deref_mut(&mut self) -> &mut Sprite {
		&mut self.sprite
	}
}

impl Component for ShaderSprite {
	fn getComponentBase(&self) -> &ComponentBase {
		self.sprite.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.sprite.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"ShaderSprite"
	}

	// 효과 시간을 흘려야 하므로 Sprite 와 달리 tick 을 받는다.
	fn isTickNeeded(&self) -> bool {
		true
	}

	// 효과는 이미지 쿼드 안에서만 그려지므로 Sprite 와 같다.
	fn isDrawnInsideContent(&self) -> bool {
		self.sprite.isDrawnInsideContent()
	}

	//==============================================================================
	// 갱신. (효과 시간 누적)
	//==============================================================================
	fn tick(&mut self, _context: &mut ComponentContext, timeDelta: f32) {
		self.effectTime += timeDelta;
	}

	//==============================================================================
	// 출력. (효과가 있으면 프로그램을 바꿔 끼우고 Sprite 출력 경로를 그대로 탄다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let image = match self.sprite.getImage() {
			Some(image) => {
				image.clone()
			}
			None => {
				self.sprite.draw(transform, graphic);
				return;
			}
		};
		let effect = self.getEffect();
		if effect == ShaderSpriteEffect::none {
			self.sprite.draw(transform, graphic);
			return;
		}
		let shaderProgram = match resolveEffectProgram(graphic, effect) {
			Some(shaderProgram) => {
				shaderProgram
			}
			None => {
				self.sprite.draw(transform, graphic);
				return;
			}
		};
		graphic.setShaderProgramOverride(Some(shaderProgram.clone()));

		// 효과 유니폼. (바꿔 끼운 프로그램이 사용 중이다)
		let mut imageRect = self.sprite.getImageRect();
		if imageRect.isZero() {
			imageRect = Rect::create(0.0, 0.0, image.getWidth() as f32, image.getHeight() as f32);
		}
		let imageWidth = image.getWidth().max(1) as f32;
		let imageHeight = image.getHeight().max(1) as f32;
		let effectColor = self.getEffectColor();
		let effectParameters = self.getEffectParameters();
		let gl = graphic.getGL();
		unsafe {
			gl.uniform_1_f32(shaderProgram.getUniformLocation(gl, "effectTime").as_ref(), self.getEffectTime());
			gl.uniform_1_f32(shaderProgram.getUniformLocation(gl, "effectProgress").as_ref(), self.getEffectProgress());
			gl.uniform_1_f32(shaderProgram.getUniformLocation(gl, "effectSeed").as_ref(), self.getEffectSeed());
			gl.uniform_4_f32(shaderProgram.getUniformLocation(gl, "effectColor").as_ref(), effectColor.red, effectColor.green, effectColor.blue, effectColor.alpha);
			gl.uniform_4_f32(shaderProgram.getUniformLocation(gl, "effectParameters").as_ref(), effectParameters[0], effectParameters[1], effectParameters[2], effectParameters[3]);
			gl.uniform_4_f32(
				shaderProgram.getUniformLocation(gl, "textureRect").as_ref(),
				imageRect.position.x / imageWidth,
				imageRect.position.y / imageHeight,
				(imageRect.position.x + imageRect.size.x) / imageWidth,
				(imageRect.position.y + imageRect.size.y) / imageHeight,
			);
			gl.uniform_2_f32(shaderProgram.getUniformLocation(gl, "texelSize").as_ref(), 1.0 / imageWidth, 1.0 / imageHeight);
		}

		self.sprite.draw(transform, graphic);
		graphic.setShaderProgramOverride(None);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 표들이 열거형 선언 차례와 맞는지. (getName, 기본값 표는 효과 번호로 바로 찾는다)
	//==============================================================================
	#[test]
	fn tablesFollowEnumOrder() {
		for (effectIndex, effect) in EFFECT_LIST.iter().enumerate() {
			assert_eq!(*effect as usize, effectIndex);
			assert_eq!(EFFECT_NAME_TABLE[effectIndex].0, *effect);
			assert_eq!(DEFAULT_PARAMETER_TABLE[effectIndex].0, *effect);
			assert_eq!(DEFAULT_COLOR_TABLE[effectIndex].0, *effect);
			assert_eq!(ShaderSpriteEffect::fromName(effect.getName()), Some(*effect));
		}
	}

	//==============================================================================
	// none 을 뺀 효과는 모두 본문이 있다.
	//==============================================================================
	#[test]
	fn everyEffectHasBody() {
		for effect in EFFECT_LIST.iter() {
			let hasBody = findEffectBody(*effect).is_some();
			assert_eq!(hasBody, *effect != ShaderSpriteEffect::none, "{:?}", effect);
		}
	}

	//==============================================================================
	// setEffect 는 기본 파라미터 / 색으로 되돌리고, 진행도는 0 ~ 1 로 자른다.
	//==============================================================================
	#[test]
	fn setEffectResetsDefaults() {
		let mut shaderSprite = ShaderSprite::new();
		shaderSprite.setEffectParameters(9.0, 9.0, 9.0, 9.0);
		shaderSprite.setEffect(ShaderSpriteEffect::glow);
		assert_eq!(shaderSprite.getEffectParameters(), [6.0, 0.0, 0.0, 0.0]);
		assert_eq!(shaderSprite.getEffectColor(), Color::create(1.0, 0.85, 0.4, 1.0));
		shaderSprite.setEffectProgress(1.5);
		assert_eq!(shaderSprite.getEffectProgress(), 1.0);
		shaderSprite.setEffectProgress(-1.0);
		assert_eq!(shaderSprite.getEffectProgress(), 0.0);
	}
}

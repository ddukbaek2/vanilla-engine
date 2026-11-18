//==============================================================================
// 화면 효과. (씬 후처리 사슬)
// - 원본 effect/screeneffect.js 를 옮긴 것이다. 효과 이름, 적용 차례, 기본 파라미터, 파라미터 이름, 기본 색, GLSL 본문은 원본 표를 그대로 옮겼다.
// - 씬은 화면에 그대로 그리고, end() 에서 화면(또는 setRegion 으로 정한 영역)을 떠 와 켜진 효과를 차례로 건 뒤 같은 자리에 되돌려 놓는다.
//   화면은 copyTexSubImage2D 로 RGB8 텍스처에 떠 오고, 효과는 핑퐁 / 절반 크기 프레임버퍼를 오가며 FullscreenPass 로 건다. 마지막 단계는 기본 프레임버퍼에 그린다.
// - 영역은 캔버스 픽셀 좌표(왼쪽 위 원점)이며 둥근 모서리 반지름을 주면 그 바깥은 원본을 유지한다.
// - 셰이더는 GLSL ES 3.00 이다. 데스크탑(GL 3.3 코어)은 ShaderProgram 이 머리만 #version 330 core 로 바꾸고, 웹(WebGL2)은 그대로 쓴다.
// - 엔진은 고치지 않는다. 원본처럼 씬이 preDraw 첫머리에서 begin, postDraw 끝에서 end, tick 에서 tick 을 부른다:
//     // 씬 create 에서
//     self.screenEffect = ScreenEffect::new();
//     self.screenEffect.setEnabled(ScreenEffectType::vignette, true);
//     // preDraw 처음 / postDraw 끝에서
//     self.screenEffect.begin(graphic);  ...  self.screenEffect.end(graphic);
//     // tick 에서
//     self.screenEffect.tick(timeDelta);
// - 원본과 다른 점:
//   - 원본은 생성자에서 Graphic 을 받아 GL 컨텍스트를 든다. 러스트 판은 아무것도 받지 않고 만들며, GL 자원은 처음 end 에서 만들고 함수마다 Graphic 을 받는다.
//     그래서 destroy 도 Graphic 을 받는다. 씬을 내리기 전에 destroy 를 부르지 않으면 GL 자원은 컨텍스트가 없어질 때까지 남는다.
//   - 효과는 문자열 대신 ScreenEffectType 열거형이다. (원본 문자열 이름은 getName / fromName)
//   - setTimelineProperty 의 값은 문자열로 받는다. (원본은 숫자, 문자열, 참거짓을 받는다. "0.5", "#ff0000", "true")
//   - 패스 셰이더 만들기에 실패하면 원본은 예외를 던지고, 러스트 판은 한 번 알리고 그 단계를 건너뛴다.
//   - destroy 는 렌더 대상과 함께 패스 프로그램도 지운다. (원본은 렌더 대상만 지운다)
//==============================================================================
use crate::base::color::Color;
use crate::core::fullscreenpass::FullscreenPass;
use crate::core::graphic::{GlTexture, Graphic};
use glow::HasContext;
use std::collections::HashMap;

// GL 객체 타입. (데스크탑은 glow::Native*, 웹은 WebGL 객체)
type GlFramebuffer = <glow::Context as glow::HasContext>::Framebuffer;
type GlRenderbuffer = <glow::Context as glow::HasContext>::Renderbuffer;

//==============================================================================
// 화면 효과 종류. (선언 차례가 적용 차례다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScreenEffectType {
	// --- 형태 왜곡 ---
	lensDistortion, // 배럴 / 핀쿠션 렌즈 왜곡. (parameters = [세기: 음수면 핀쿠션])
	bulge,          // 볼록 / 오목. (parameters = [중심 x, 중심 y, 세기, 반지름])
	swirl,          // 소용돌이. (parameters = [중심 x, 중심 y, 회전 수, 반지름])
	kaleidoscope,   // 만화경. (parameters = [조각 수])
	mirror,         // 대칭. (parameters = [0 가로 / 1 세로])
	ripple,         // 물 파문. (parameters = [중심 x, 중심 y, 빈도, 속도])
	heatHaze,       // 아지랑이. (parameters = [노이즈 배율, 속도])
	jitter,         // 화면 흔들림. (parameters = [진폭 픽셀, 초당 변화])
	doubleVision,   // 겹쳐 보임. (parameters = [간격, 흔들리는 속도])
	pixelate,       // 모자이크. (strength 0 → 1, parameters = [가장 큰 블록 픽셀])
	shockwave,      // 충격파 링 왜곡. (parameters = [중심 x, 중심 y, 반지름 0 ~ 1.5, 폭], color = 링 빛)
	wave,           // 화면 물결. (parameters = [빈도, 속도])
	// --- 흐림 ---
	motionBlur,     // 방향 블러. (parameters = [각도 라디안, 길이 픽셀])
	zoomBlur,       // 중심으로 뻗는 방사 블러. (parameters = [중심 x, 중심 y, 길이])
	blur,           // 가우시안 블러. (parameters = [반지름 픽셀])
	tiltShift,      // 틸트 시프트. (parameters = [초점 y, 초점 폭, 반지름 픽셀])
	godRays,        // 빛줄기. (parameters = [중심 x, 중심 y, 문턱, 감쇠], color = 빛 색)
	bloom,          // 밝은 곳 번짐. (parameters = [문턱, 세기 배율, 반지름 픽셀])
	anamorphic,     // 가로 렌즈 플레어. (parameters = [문턱, 세기 배율, 폭 픽셀], color = 플레어 색)
	// --- 글리치 ---
	glitch,         // 가로 줄 어긋남 + 색 분리 + 블록 반전. (parameters = [줄 수, 초당 변화, 어긋남 폭])
	vhs,            // VHS 테이프. (흔들림 + 색 번짐 + 노이즈 줄, parameters = [흔들림, 노이즈])
	chromatic,      // 색수차. (parameters = [배율])
	// --- 색 ---
	whiteBalance,   // 색온도 / 틴트. (parameters = [온도 -1 ~ 1, 틴트 -1 ~ 1])
	liftGammaGain,  // 리프트 / 감마 / 게인. (parameters = [리프트, 감마, 게인])
	colorGrade,     // 대비 / 채도 / 밝기 + 틴트. (parameters = [대비, 채도, 밝기], color = 틴트)
	hueShift,       // 색상 회전. (parameters = [회전 수: strength 와 곱])
	splitToning,    // 스플릿 토닝. (parameters.xyz = 그림자 색, color = 하이라이트 색, w = 균형)
	tonemap,        // ACES 톤매핑. (parameters = [노출])
	gradientMap,    // 밝기를 두 색 사이로 매핑. (parameters.xyz = 어두운 색, color = 밝은 색)
	sepia,          // 세피아.
	grayscale,      // 무채색.
	posterize,      // 포스터화. (parameters = [단계 수])
	dither,         // 오더드 디더링. (parameters = [단계 수, 픽셀 크기])
	halftone,       // 하프톤 점. (parameters = [격자 픽셀])
	sharpen,        // 샤픈.
	edgeDetect,     // 윤곽 검출. (color = 선 색)
	oldFilm,        // 낡은 필름. (세피아 + 그레인 + 스크래치 + 깜빡임)
	nightVision,    // 야시경. (color = 틴트)
	invert,         // 색 반전.
	// --- 무늬 / 마스크 ---
	scanline,       // CRT 곡면 + 주사선 + RGB 마스크. (parameters = [곡률, 주사선 밀도])
	grain,          // 필름 그레인.
	rain,           // 빗줄기. (parameters = [줄 수, 밀도, 속도], color = 빗줄기 색)
	fog,            // 세로 안개. (parameters = [시작 y, 끝 y], color = 안개 색)
	vignette,       // 비네트. (parameters = [시작 거리, 끝 거리], color = 가장자리 색)
	spotlight,      // 스포트라이트 바깥 어둡힘. (parameters = [중심 x, 중심 y, 반지름, 부드러움], color = 바깥 색)
	speedLines,     // 집중선. (parameters = [중심 x, 중심 y, 밀도, 속도], color = 선 색)
	letterbox,      // 위아래 / 좌우 검은 띠. (parameters = [세로 띠 비율, 가로 띠 비율], color = 띠 색)
	// --- 전환 (strength 0 → 1 로 color 에 덮인다) ---
	pixelDissolve,  // 블록 디졸브. (parameters = [블록 수])
	wipe,           // 방향 와이프. (parameters = [방향 x, 방향 y, 부드러움])
	irisWipe,       // 원형 아이리스. (parameters = [중심 x, 중심 y, 부드러움])
	diamondWipe,    // 마름모 와이프. (parameters = [부드러움])
	clockWipe,      // 시계 와이프. (parameters = [부드러움])
	blinds,         // 블라인드. (parameters = [줄 수, 부드러움])
	checkerWipe,    // 체커 보드. (parameters = [칸 수])
	noiseFade,      // 노이즈 페이드. (parameters = [노이즈 배율, 부드러움])
	burn,           // 불에 타듯. (parameters = [노이즈 배율, 띠 폭], color = 덮을 색, 띠는 주황)
	tvOff,          // 브라운관 끄기. (세로로 눌리며 밝은 선으로)
	fade,           // 단색으로 페이드. (color = 목표 색)
}

// 적용 차례. (형태 왜곡 → 흐림 → 글리치 → 색 → 무늬 → 전환)
const EFFECT_ORDER: [ScreenEffectType; 58] = [
	ScreenEffectType::lensDistortion, ScreenEffectType::bulge, ScreenEffectType::swirl, ScreenEffectType::kaleidoscope, ScreenEffectType::mirror, ScreenEffectType::ripple, ScreenEffectType::heatHaze, ScreenEffectType::jitter, ScreenEffectType::doubleVision, ScreenEffectType::pixelate, ScreenEffectType::shockwave, ScreenEffectType::wave,
	ScreenEffectType::motionBlur, ScreenEffectType::zoomBlur, ScreenEffectType::blur, ScreenEffectType::tiltShift, ScreenEffectType::godRays, ScreenEffectType::bloom, ScreenEffectType::anamorphic,
	ScreenEffectType::glitch, ScreenEffectType::vhs, ScreenEffectType::chromatic,
	ScreenEffectType::whiteBalance, ScreenEffectType::liftGammaGain, ScreenEffectType::colorGrade, ScreenEffectType::hueShift, ScreenEffectType::splitToning, ScreenEffectType::tonemap, ScreenEffectType::gradientMap, ScreenEffectType::sepia, ScreenEffectType::grayscale, ScreenEffectType::posterize, ScreenEffectType::dither, ScreenEffectType::halftone, ScreenEffectType::sharpen, ScreenEffectType::edgeDetect, ScreenEffectType::oldFilm, ScreenEffectType::nightVision, ScreenEffectType::invert,
	ScreenEffectType::scanline, ScreenEffectType::grain, ScreenEffectType::rain, ScreenEffectType::fog, ScreenEffectType::vignette, ScreenEffectType::spotlight, ScreenEffectType::speedLines, ScreenEffectType::letterbox,
	ScreenEffectType::pixelDissolve, ScreenEffectType::wipe, ScreenEffectType::irisWipe, ScreenEffectType::diamondWipe, ScreenEffectType::clockWipe, ScreenEffectType::blinds, ScreenEffectType::checkerWipe, ScreenEffectType::noiseFade, ScreenEffectType::burn, ScreenEffectType::tvOff, ScreenEffectType::fade,
];

// 효과별 원본 이름. (적용 차례)
const EFFECT_NAME_TABLE: [(ScreenEffectType, &str); 58] = [
	(ScreenEffectType::lensDistortion, "lensDistortion"),
	(ScreenEffectType::bulge, "bulge"),
	(ScreenEffectType::swirl, "swirl"),
	(ScreenEffectType::kaleidoscope, "kaleidoscope"),
	(ScreenEffectType::mirror, "mirror"),
	(ScreenEffectType::ripple, "ripple"),
	(ScreenEffectType::heatHaze, "heatHaze"),
	(ScreenEffectType::jitter, "jitter"),
	(ScreenEffectType::doubleVision, "doubleVision"),
	(ScreenEffectType::pixelate, "pixelate"),
	(ScreenEffectType::shockwave, "shockwave"),
	(ScreenEffectType::wave, "wave"),
	(ScreenEffectType::motionBlur, "motionBlur"),
	(ScreenEffectType::zoomBlur, "zoomBlur"),
	(ScreenEffectType::blur, "blur"),
	(ScreenEffectType::tiltShift, "tiltShift"),
	(ScreenEffectType::godRays, "godRays"),
	(ScreenEffectType::bloom, "bloom"),
	(ScreenEffectType::anamorphic, "anamorphic"),
	(ScreenEffectType::glitch, "glitch"),
	(ScreenEffectType::vhs, "vhs"),
	(ScreenEffectType::chromatic, "chromatic"),
	(ScreenEffectType::whiteBalance, "whiteBalance"),
	(ScreenEffectType::liftGammaGain, "liftGammaGain"),
	(ScreenEffectType::colorGrade, "colorGrade"),
	(ScreenEffectType::hueShift, "hueShift"),
	(ScreenEffectType::splitToning, "splitToning"),
	(ScreenEffectType::tonemap, "tonemap"),
	(ScreenEffectType::gradientMap, "gradientMap"),
	(ScreenEffectType::sepia, "sepia"),
	(ScreenEffectType::grayscale, "grayscale"),
	(ScreenEffectType::posterize, "posterize"),
	(ScreenEffectType::dither, "dither"),
	(ScreenEffectType::halftone, "halftone"),
	(ScreenEffectType::sharpen, "sharpen"),
	(ScreenEffectType::edgeDetect, "edgeDetect"),
	(ScreenEffectType::oldFilm, "oldFilm"),
	(ScreenEffectType::nightVision, "nightVision"),
	(ScreenEffectType::invert, "invert"),
	(ScreenEffectType::scanline, "scanline"),
	(ScreenEffectType::grain, "grain"),
	(ScreenEffectType::rain, "rain"),
	(ScreenEffectType::fog, "fog"),
	(ScreenEffectType::vignette, "vignette"),
	(ScreenEffectType::spotlight, "spotlight"),
	(ScreenEffectType::speedLines, "speedLines"),
	(ScreenEffectType::letterbox, "letterbox"),
	(ScreenEffectType::pixelDissolve, "pixelDissolve"),
	(ScreenEffectType::wipe, "wipe"),
	(ScreenEffectType::irisWipe, "irisWipe"),
	(ScreenEffectType::diamondWipe, "diamondWipe"),
	(ScreenEffectType::clockWipe, "clockWipe"),
	(ScreenEffectType::blinds, "blinds"),
	(ScreenEffectType::checkerWipe, "checkerWipe"),
	(ScreenEffectType::noiseFade, "noiseFade"),
	(ScreenEffectType::burn, "burn"),
	(ScreenEffectType::tvOff, "tvOff"),
	(ScreenEffectType::fade, "fade"),
];

// 효과별 기본 파라미터. (적용 차례)
const DEFAULT_PARAMETER_TABLE: [(ScreenEffectType, [f32; 4]); 58] = [
	(ScreenEffectType::lensDistortion, [0.35, 0.0, 0.0, 0.0]),
	(ScreenEffectType::bulge, [0.5, 0.5, 0.5, 0.5]),
	(ScreenEffectType::swirl, [0.5, 0.5, 0.6, 0.5]),
	(ScreenEffectType::kaleidoscope, [6.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::mirror, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::ripple, [0.5, 0.5, 40.0, 6.0]),
	(ScreenEffectType::heatHaze, [6.0, 1.5, 0.0, 0.0]),
	(ScreenEffectType::jitter, [8.0, 24.0, 0.0, 0.0]),
	(ScreenEffectType::doubleVision, [0.02, 3.0, 0.0, 0.0]),
	(ScreenEffectType::pixelate, [12.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::shockwave, [0.5, 0.5, 0.0, 0.1]),
	(ScreenEffectType::wave, [18.0, 4.0, 0.0, 0.0]),
	(ScreenEffectType::motionBlur, [0.0, 24.0, 0.0, 0.0]),
	(ScreenEffectType::zoomBlur, [0.5, 0.5, 0.35, 0.0]),
	(ScreenEffectType::blur, [4.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::tiltShift, [0.5, 0.18, 6.0, 0.0]),
	(ScreenEffectType::godRays, [0.5, 0.35, 0.6, 0.94]),
	(ScreenEffectType::bloom, [0.65, 1.2, 4.0, 0.0]),
	(ScreenEffectType::anamorphic, [0.75, 1.5, 24.0, 0.0]),
	(ScreenEffectType::glitch, [20.0, 10.0, 0.08, 0.0]),
	(ScreenEffectType::vhs, [0.01, 0.35, 0.0, 0.0]),
	(ScreenEffectType::chromatic, [1.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::whiteBalance, [0.4, 0.0, 0.0, 0.0]),
	(ScreenEffectType::liftGammaGain, [0.0, 1.0, 1.0, 0.0]),
	(ScreenEffectType::colorGrade, [1.0, 1.0, 1.0, 0.0]),
	(ScreenEffectType::hueShift, [1.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::splitToning, [0.1, 0.15, 0.45, 0.5]),
	(ScreenEffectType::tonemap, [1.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::gradientMap, [0.05, 0.03, 0.2, 0.0]),
	(ScreenEffectType::sepia, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::grayscale, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::posterize, [5.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::dither, [4.0, 2.0, 0.0, 0.0]),
	(ScreenEffectType::halftone, [6.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::sharpen, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::edgeDetect, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::oldFilm, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::nightVision, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::invert, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::scanline, [0.15, 0.5, 0.0, 0.0]),
	(ScreenEffectType::grain, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::rain, [90.0, 8.0, 1.6, 0.0]),
	(ScreenEffectType::fog, [0.35, 1.0, 0.0, 0.0]),
	(ScreenEffectType::vignette, [0.45, 1.1, 0.0, 0.0]),
	(ScreenEffectType::spotlight, [0.5, 0.5, 0.25, 0.2]),
	(ScreenEffectType::speedLines, [0.5, 0.5, 14.0, 6.0]),
	(ScreenEffectType::letterbox, [0.12, 0.0, 0.0, 0.0]),
	(ScreenEffectType::pixelDissolve, [24.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::wipe, [1.0, 0.0, 0.05, 0.0]),
	(ScreenEffectType::irisWipe, [0.5, 0.5, 0.05, 0.0]),
	(ScreenEffectType::diamondWipe, [0.05, 0.0, 0.0, 0.0]),
	(ScreenEffectType::clockWipe, [0.02, 0.0, 0.0, 0.0]),
	(ScreenEffectType::blinds, [10.0, 0.05, 0.0, 0.0]),
	(ScreenEffectType::checkerWipe, [10.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::noiseFade, [5.0, 0.2, 0.0, 0.0]),
	(ScreenEffectType::burn, [5.0, 0.06, 0.0, 0.0]),
	(ScreenEffectType::tvOff, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::fade, [0.0, 0.0, 0.0, 0.0]),
];

// 효과별 파라미터 이름. (타임라인 속성 "효과.이름" 으로 접근, 적용 차례)
pub const SCREEN_EFFECT_PARAMETER_NAMES: [(ScreenEffectType, &[&str]); 58] = [
	(ScreenEffectType::lensDistortion, &["intensity"]),
	(ScreenEffectType::bulge, &["centerX", "centerY", "intensity", "radius"]),
	(ScreenEffectType::swirl, &["centerX", "centerY", "turns", "radius"]),
	(ScreenEffectType::kaleidoscope, &["segments"]),
	(ScreenEffectType::mirror, &["axis"]),
	(ScreenEffectType::ripple, &["centerX", "centerY", "frequency", "speed"]),
	(ScreenEffectType::heatHaze, &["scale", "speed"]),
	(ScreenEffectType::jitter, &["amplitude", "rate"]),
	(ScreenEffectType::doubleVision, &["offset", "speed"]),
	(ScreenEffectType::pixelate, &["blockSize"]),
	(ScreenEffectType::shockwave, &["centerX", "centerY", "radius", "width"]),
	(ScreenEffectType::wave, &["frequency", "speed"]),
	(ScreenEffectType::motionBlur, &["angle", "length"]),
	(ScreenEffectType::zoomBlur, &["centerX", "centerY", "length"]),
	(ScreenEffectType::blur, &["radius"]),
	(ScreenEffectType::tiltShift, &["focusY", "focusWidth", "radius"]),
	(ScreenEffectType::godRays, &["centerX", "centerY", "threshold", "decay"]),
	(ScreenEffectType::bloom, &["threshold", "intensity", "radius"]),
	(ScreenEffectType::anamorphic, &["threshold", "intensity", "width"]),
	(ScreenEffectType::glitch, &["rows", "rate", "shift"]),
	(ScreenEffectType::vhs, &["wobble", "noise"]),
	(ScreenEffectType::chromatic, &["scale"]),
	(ScreenEffectType::whiteBalance, &["temperature", "tint"]),
	(ScreenEffectType::liftGammaGain, &["lift", "gamma", "gain"]),
	(ScreenEffectType::colorGrade, &["contrast", "saturation", "brightness"]),
	(ScreenEffectType::hueShift, &["turns"]),
	(ScreenEffectType::splitToning, &["shadowRed", "shadowGreen", "shadowBlue", "balance"]),
	(ScreenEffectType::tonemap, &["exposure"]),
	(ScreenEffectType::gradientMap, &["darkRed", "darkGreen", "darkBlue"]),
	(ScreenEffectType::sepia, &[]),
	(ScreenEffectType::grayscale, &[]),
	(ScreenEffectType::posterize, &["levels"]),
	(ScreenEffectType::dither, &["levels", "pixelSize"]),
	(ScreenEffectType::halftone, &["cellSize"]),
	(ScreenEffectType::sharpen, &[]),
	(ScreenEffectType::edgeDetect, &[]),
	(ScreenEffectType::oldFilm, &[]),
	(ScreenEffectType::nightVision, &[]),
	(ScreenEffectType::invert, &[]),
	(ScreenEffectType::scanline, &["curvature", "density"]),
	(ScreenEffectType::grain, &[]),
	(ScreenEffectType::rain, &["columns", "density", "speed"]),
	(ScreenEffectType::fog, &["start", "end"]),
	(ScreenEffectType::vignette, &["inner", "outer"]),
	(ScreenEffectType::spotlight, &["centerX", "centerY", "radius", "softness"]),
	(ScreenEffectType::speedLines, &["centerX", "centerY", "density", "speed"]),
	(ScreenEffectType::letterbox, &["vertical", "horizontal"]),
	(ScreenEffectType::pixelDissolve, &["blocks"]),
	(ScreenEffectType::wipe, &["directionX", "directionY", "softness"]),
	(ScreenEffectType::irisWipe, &["centerX", "centerY", "softness"]),
	(ScreenEffectType::diamondWipe, &["softness"]),
	(ScreenEffectType::clockWipe, &["softness"]),
	(ScreenEffectType::blinds, &["count", "softness"]),
	(ScreenEffectType::checkerWipe, &["cells"]),
	(ScreenEffectType::noiseFade, &["scale", "softness"]),
	(ScreenEffectType::burn, &["scale", "width"]),
	(ScreenEffectType::tvOff, &[]),
	(ScreenEffectType::fade, &[]),
];

// 효과별 기본 색. (없는 효과는 흰색)
const DEFAULT_COLOR_TABLE: [(ScreenEffectType, [f32; 4]); 25] = [
	(ScreenEffectType::shockwave, [1.0, 1.0, 1.0, 0.6]),
	(ScreenEffectType::godRays, [1.0, 0.95, 0.8, 1.0]),
	(ScreenEffectType::anamorphic, [0.5, 0.7, 1.0, 1.0]),
	(ScreenEffectType::colorGrade, [1.0, 1.0, 1.0, 0.0]),
	(ScreenEffectType::splitToning, [1.0, 0.85, 0.55, 1.0]),
	(ScreenEffectType::gradientMap, [1.0, 0.9, 0.6, 1.0]),
	(ScreenEffectType::edgeDetect, [1.0, 1.0, 1.0, 1.0]),
	(ScreenEffectType::nightVision, [0.35, 1.0, 0.45, 1.0]),
	(ScreenEffectType::rain, [0.8, 0.9, 1.0, 0.45]),
	(ScreenEffectType::fog, [0.75, 0.8, 0.9, 1.0]),
	(ScreenEffectType::vignette, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::spotlight, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::speedLines, [1.0, 1.0, 1.0, 0.8]),
	(ScreenEffectType::letterbox, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::pixelDissolve, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::wipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::irisWipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::diamondWipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::clockWipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::blinds, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::checkerWipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::noiseFade, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::burn, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::tvOff, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::fade, [0.0, 0.0, 0.0, 1.0]),
];

// 프래그먼트 셰이더 공통 머리. (효과 본문은 uv / color 를 다룬다)
const FRAGMENTSHADER_HEADER: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform sampler2D auxiliaryTexture;
uniform vec2 resolution;
uniform float time;
uniform float strength;
uniform vec4 parameters;
uniform vec4 effectColor;
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

vec4 sampleScreen(vec2 screenUv) {
	vec2 clamped = clamp(screenUv, 0.0, 1.0);
	return texture(sourceTexture, vec2(clamped.x, 1.0 - clamped.y));
}

float luminanceAt(vec2 uv) {
	return dot(texture(sourceTexture, uv).rgb, vec3(0.299, 0.587, 0.114));
}

void main() {
	vec2 uv = fragmentTextureCoordinate;
	vec2 screenUv = vec2(uv.x, 1.0 - uv.y);
	vec4 color = texture(sourceTexture, uv);
";

const FRAGMENTSHADER_FOOTER: &str = "
	outputColor = vec4(color.rgb, 1.0);
}
";

// 패스별 프래그먼트 본문. (효과 이름 + 내부 패스 copy / bloomBright / bloomComposite / anamorphicComposite / regionComposite)
// - uv: 텍스처 좌표(아래가 0), screenUv: 화면 좌표(위가 0), color: 현재 색. sampleScreen(screenUv) 는 화면 좌표로 샘플.
const EFFECT_BODY_TABLE: [(&str, &str); 61] = [
	("copy", ""),
	("lensDistortion", "
	vec2 centered = uv - 0.5;
	float radiusSquared = dot(centered, centered);
	float distortion = parameters.x * strength;
	vec2 warped = uv + centered * radiusSquared * distortion;
	warped = 0.5 + (warped - 0.5) / (1.0 + 0.25 * max(distortion, 0.0));
	vec2 insideMask = step(vec2(0.0), warped) * step(warped, vec2(1.0));
	color = texture(sourceTexture, warped) * insideMask.x * insideMask.y;
	"),
	("bulge", "
	float aspect = resolution.x / resolution.y;
	vec2 fromCenter = (screenUv - parameters.xy) * vec2(aspect, 1.0);
	float bulgeDistance = length(fromCenter) / max(parameters.w, 0.001);
	float bulgeFactor = 1.0 - parameters.z * strength * (1.0 - clamp(bulgeDistance, 0.0, 1.0));
	vec2 warpedScreen = parameters.xy + fromCenter * bulgeFactor / vec2(aspect, 1.0);
	color = sampleScreen(warpedScreen);
	"),
	("swirl", "
	float aspect = resolution.x / resolution.y;
	vec2 fromCenter = (screenUv - parameters.xy) * vec2(aspect, 1.0);
	float swirlDistance = length(fromCenter) / max(parameters.w, 0.001);
	float swirlAngle = parameters.z * strength * 6.28318530718 * pow(1.0 - clamp(swirlDistance, 0.0, 1.0), 2.0);
	float cosine = cos(swirlAngle);
	float sine = sin(swirlAngle);
	vec2 rotated = vec2(fromCenter.x * cosine - fromCenter.y * sine, fromCenter.x * sine + fromCenter.y * cosine);
	color = sampleScreen(parameters.xy + rotated / vec2(aspect, 1.0));
	"),
	("kaleidoscope", "
	float aspect = resolution.x / resolution.y;
	vec2 fromCenter = (screenUv - 0.5) * vec2(aspect, 1.0);
	float kaleidoRadius = length(fromCenter);
	float segmentAngle = 6.28318530718 / max(parameters.x, 1.0);
	float kaleidoAngle = atan(fromCenter.y, fromCenter.x);
	kaleidoAngle = abs(mod(kaleidoAngle, segmentAngle) - segmentAngle * 0.5);
	vec2 mirroredScreen = 0.5 + vec2(cos(kaleidoAngle), sin(kaleidoAngle)) * kaleidoRadius / vec2(aspect, 1.0);
	color = mix(color, sampleScreen(mirroredScreen), strength);
	"),
	("mirror", "
	vec2 mirroredScreen = screenUv;
	if (parameters.x < 0.5) {
		mirroredScreen.x = screenUv.x < 0.5 ? screenUv.x : 1.0 - screenUv.x;
	}
	else {
		mirroredScreen.y = screenUv.y < 0.5 ? screenUv.y : 1.0 - screenUv.y;
	}
	color = mix(color, sampleScreen(mirroredScreen), step(0.5, strength));
	"),
	("ripple", "
	float aspect = resolution.x / resolution.y;
	vec2 fromCenter = (screenUv - parameters.xy) * vec2(aspect, 1.0);
	float rippleDistance = length(fromCenter);
	float rippleWave = sin(rippleDistance * parameters.z - time * parameters.w) * strength * 0.012 / (1.0 + rippleDistance * 4.0);
	vec2 rippleDirection = rippleDistance > 0.0001 ? fromCenter / rippleDistance : vec2(0.0);
	color = sampleScreen(screenUv + rippleDirection * rippleWave / vec2(aspect, 1.0));
	"),
	("heatHaze", "
	vec2 hazeOffset = vec2(valueNoise(uv * parameters.x + vec2(0.0, time * parameters.y)), valueNoise(uv * parameters.x + vec2(time * parameters.y, 7.3))) - 0.5;
	color = texture(sourceTexture, uv + hazeOffset * strength * 0.03);
	"),
	("jitter", "
	float jitterSeed = floor(time * parameters.y);
	vec2 jitterOffset = (vec2(hash21(vec2(jitterSeed, 1.0)), hash21(vec2(jitterSeed, 2.0))) - 0.5) * parameters.x * strength / resolution;
	color = texture(sourceTexture, clamp(uv + jitterOffset, 0.0, 1.0));
	"),
	("doubleVision", "
	vec2 visionOffset = vec2(sin(time * parameters.y) * parameters.x, cos(time * parameters.y * 0.7) * parameters.x * 0.5) * strength;
	color = (texture(sourceTexture, uv + visionOffset) + texture(sourceTexture, uv - visionOffset)) * 0.5;
	"),
	("pixelate", "
	float block = max(1.0, parameters.x * strength);
	vec2 pixelUv = (floor(uv * resolution / block) + 0.5) * block / resolution;
	color = texture(sourceTexture, pixelUv);
	"),
	("shockwave", "
	float aspect = resolution.x / resolution.y;
	vec2 delta = (screenUv - parameters.xy) * vec2(aspect, 1.0);
	float ringDistance = length(delta);
	float ringWidth = max(parameters.w, 0.001);
	float ring = 1.0 - smoothstep(0.0, ringWidth, abs(ringDistance - parameters.z));
	float wave = sin((ringDistance - parameters.z) / ringWidth * 3.14159265) * ring;
	vec2 direction = ringDistance > 0.0001 ? delta / ringDistance : vec2(0.0);
	vec2 uvDirection = vec2(direction.x, -direction.y);
	vec2 displaced = uv - uvDirection / vec2(aspect, 1.0) * wave * strength * 0.06;
	color = texture(sourceTexture, displaced);
	color.rgb += effectColor.rgb * ring * ring * strength * effectColor.a;
	"),
	("wave", "
	vec2 displaced = uv + vec2(sin(uv.y * parameters.x + time * parameters.y), cos(uv.x * parameters.x * 0.7 + time * parameters.y * 0.8)) * strength * 0.012;
	color = texture(sourceTexture, displaced);
	"),
	("motionBlur", "
	vec2 blurDirection = vec2(cos(parameters.x), -sin(parameters.x)) * parameters.y * strength / resolution;
	vec4 sum = vec4(0.0);
	for (int sampleIndex = 0; sampleIndex < 12; ++sampleIndex) {
		float offset = (float(sampleIndex) - 5.5) / 5.5;
		sum += texture(sourceTexture, uv + blurDirection * offset);
	}
	color = sum / 12.0;
	"),
	("zoomBlur", "
	vec2 toCenter = (parameters.xy - screenUv) * strength * parameters.z;
	toCenter.y = -toCenter.y;
	vec4 sum = vec4(0.0);
	for (int sampleIndex = 0; sampleIndex < 12; ++sampleIndex) {
		sum += texture(sourceTexture, uv + toCenter * (float(sampleIndex) / 12.0));
	}
	color = sum / 12.0;
	"),
	("blur", "
	vec2 blurStep = parameters.zw / resolution * parameters.x * strength;
	vec4 sum = color * 0.227027;
	sum += (texture(sourceTexture, uv + blurStep * 1.3846) + texture(sourceTexture, uv - blurStep * 1.3846)) * 0.3162162;
	sum += (texture(sourceTexture, uv + blurStep * 3.2307) + texture(sourceTexture, uv - blurStep * 3.2307)) * 0.0702703;
	color = sum;
	"),
	("tiltShift", "
	float focusDistance = smoothstep(parameters.y * 0.5, parameters.y * 1.5, abs(screenUv.y - parameters.x));
	vec2 blurStep = parameters.zw / resolution * focusDistance * strength;
	vec4 sum = color * 0.227027;
	sum += (texture(sourceTexture, uv + blurStep * 1.3846) + texture(sourceTexture, uv - blurStep * 1.3846)) * 0.3162162;
	sum += (texture(sourceTexture, uv + blurStep * 3.2307) + texture(sourceTexture, uv - blurStep * 3.2307)) * 0.0702703;
	color = sum;
	"),
	("godRays", "
	vec2 rayCenter = vec2(parameters.x, 1.0 - parameters.y);
	vec2 rayStep = (rayCenter - uv) / 16.0;
	vec3 rays = vec3(0.0);
	float weight = 1.0;
	vec2 sampleUv = uv;
	for (int sampleIndex = 0; sampleIndex < 16; ++sampleIndex) {
		sampleUv += rayStep;
		vec3 sampled = texture(sourceTexture, sampleUv).rgb;
		float luminance = dot(sampled, vec3(0.299, 0.587, 0.114));
		rays += sampled * smoothstep(parameters.z - 0.1, parameters.z + 0.1, luminance) * weight;
		weight *= parameters.w;
	}
	color.rgb += rays / 16.0 * effectColor.rgb * strength * 2.0;
	"),
	("bloomBright", "
	float luminance = dot(color.rgb, vec3(0.299, 0.587, 0.114));
	float knee = smoothstep(parameters.x - 0.1, parameters.x + 0.1, luminance);
	color.rgb *= knee;
	"),
	("bloomComposite", "
	vec3 bloom = texture(auxiliaryTexture, uv).rgb;
	color.rgb += bloom * strength * parameters.y;
	"),
	("anamorphicComposite", "
	vec3 flare = texture(auxiliaryTexture, uv).rgb;
	color.rgb += flare * effectColor.rgb * strength * parameters.y;
	"),
	("glitch", "
	float seed = floor(time * parameters.y);
	float row = floor(uv.y * parameters.x);
	float rowRandom = hash21(vec2(row, seed));
	float shift = step(0.8, rowRandom) * (hash21(vec2(seed, row)) - 0.5) * parameters.z * strength;
	vec2 shifted = uv + vec2(shift, 0.0);
	color = texture(sourceTexture, shifted);
	float split = strength * 0.012 * step(0.5, rowRandom);
	color.r = texture(sourceTexture, shifted + vec2(split, 0.0)).r;
	color.b = texture(sourceTexture, shifted - vec2(split, 0.0)).b;
	float blockNoise = step(0.93, hash21(floor(uv * vec2(8.0, 6.0)) + seed)) * strength;
	color.rgb = mix(color.rgb, 1.0 - color.rgb, blockNoise * 0.6);
	"),
	("vhs", "
	float band = fract(time * 0.35);
	float bandMask = smoothstep(0.0, 0.05, abs(screenUv.y - band)) ;
	float wobble = (sin(time * 9.0 + uv.y * 40.0) * 0.5 + (1.0 - bandMask) * 3.0) * parameters.x * strength;
	vec2 shifted = uv + vec2(wobble, 0.0);
	color = texture(sourceTexture, shifted);
	color.r = texture(sourceTexture, shifted + vec2(0.006 * strength, 0.0)).r;
	color.b = texture(sourceTexture, shifted - vec2(0.006 * strength, 0.0)).b;
	float lineNoise = hash21(vec2(floor(uv.y * resolution.y * 0.5), floor(time * 30.0)));
	float noiseLine = step(1.0 - parameters.y * 0.08, lineNoise) * strength;
	color.rgb = mix(color.rgb, vec3(0.9), noiseLine * 0.6);
	float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
	color.rgb = mix(color.rgb, vec3(gray), 0.2 * strength);
	color.rgb += (hash21(uv * resolution + fract(time * 5.0) * 100.0) - 0.5) * parameters.y * 0.3 * strength;
	"),
	("chromatic", "
	vec2 offset = (uv - 0.5) * strength * 0.03 * parameters.x;
	color.r = texture(sourceTexture, uv + offset).r;
	color.b = texture(sourceTexture, uv - offset).b;
	"),
	("whiteBalance", "
	vec3 balanced = color.rgb * vec3(1.0 + parameters.x * 0.25, 1.0 + parameters.y * 0.15, 1.0 - parameters.x * 0.25);
	color.rgb = mix(color.rgb, clamp(balanced, 0.0, 1.0), strength);
	"),
	("liftGammaGain", "
	vec3 graded = color.rgb * parameters.z + parameters.x * (1.0 - color.rgb);
	graded = pow(max(graded, vec3(0.0)), vec3(1.0 / max(parameters.y, 0.01)));
	color.rgb = mix(color.rgb, clamp(graded, 0.0, 1.0), strength);
	"),
	("colorGrade", "
	float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
	vec3 graded = mix(vec3(gray), color.rgb, parameters.y);
	graded = (graded - 0.5) * parameters.x + 0.5;
	graded *= parameters.z;
	graded *= mix(vec3(1.0), effectColor.rgb, effectColor.a);
	color.rgb = mix(color.rgb, clamp(graded, 0.0, 1.0), strength);
	"),
	("hueShift", "
	float hueAngle = strength * parameters.x * 6.28318530718;
	vec3 yiq = mat3(0.299, 0.596, 0.211, 0.587, -0.274, -0.523, 0.114, -0.322, 0.312) * color.rgb;
	float chroma = length(yiq.yz);
	float hue = atan(yiq.z, yiq.y) + hueAngle;
	yiq.yz = vec2(cos(hue), sin(hue)) * chroma;
	color.rgb = clamp(mat3(1.0, 1.0, 1.0, 0.956, -0.272, -1.106, 0.621, -0.647, 1.703) * yiq, 0.0, 1.0);
	"),
	("splitToning", "
	float luminance = dot(color.rgb, vec3(0.299, 0.587, 0.114));
	float highlightWeight = smoothstep(parameters.w - 0.3, parameters.w + 0.3, luminance);
	vec3 toned = color.rgb * mix(parameters.xyz * 2.0, effectColor.rgb * 2.0, highlightWeight);
	color.rgb = mix(color.rgb, clamp(mix(color.rgb, toned, 0.5), 0.0, 1.0), strength);
	"),
	("tonemap", "
	vec3 exposed = color.rgb * parameters.x;
	vec3 mapped = (exposed * (2.51 * exposed + 0.03)) / (exposed * (2.43 * exposed + 0.59) + 0.14);
	color.rgb = mix(color.rgb, clamp(mapped, 0.0, 1.0), strength);
	"),
	("gradientMap", "
	float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
	color.rgb = mix(color.rgb, mix(parameters.xyz, effectColor.rgb, gray), strength);
	"),
	("sepia", "
	float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
	color.rgb = mix(color.rgb, gray * vec3(1.2, 1.0, 0.78), strength);
	"),
	("grayscale", "
	float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
	color.rgb = mix(color.rgb, vec3(gray), strength);
	"),
	("posterize", "
	float levels = max(parameters.x, 1.0);
	color.rgb = mix(color.rgb, floor(color.rgb * levels + 0.5) / levels, strength);
	"),
	("dither", "
	float pixelSize = max(parameters.y, 1.0);
	vec2 pixelCell = floor(uv * resolution / pixelSize);
	vec3 sampled = texture(sourceTexture, (pixelCell + 0.5) * pixelSize / resolution).rgb;
	int bayerX = int(mod(pixelCell.x, 4.0));
	int bayerY = int(mod(pixelCell.y, 4.0));
	int bayerIndex = bayerX + bayerY * 4;
	float bayerValues[16] = float[16](0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
	float bayerThreshold = (bayerValues[bayerIndex] + 0.5) / 16.0 - 0.5;
	float levels = max(parameters.x, 2.0) - 1.0;
	vec3 dithered = floor(sampled * levels + bayerThreshold + 0.5) / levels;
	color.rgb = mix(color.rgb, clamp(dithered, 0.0, 1.0), strength);
	"),
	("halftone", "
	float cellSize = max(parameters.x, 2.0);
	vec2 cell = floor(uv * resolution / cellSize);
	vec2 cellCenter = (cell + 0.5) * cellSize / resolution;
	vec3 sampled = texture(sourceTexture, cellCenter).rgb;
	float luminance = dot(sampled, vec3(0.299, 0.587, 0.114));
	float dotRadius = (1.0 - luminance) * 0.7;
	float cellDistance = length((uv * resolution / cellSize) - (cell + 0.5));
	float dotMask = 1.0 - smoothstep(dotRadius - 0.1, dotRadius + 0.1, cellDistance);
	vec3 halftoned = mix(vec3(1.0), vec3(0.05), dotMask);
	color.rgb = mix(color.rgb, halftoned * mix(vec3(1.0), sampled + 0.3, 0.4), strength);
	"),
	("sharpen", "
	vec2 texel = 1.0 / resolution;
	vec3 neighbors = texture(sourceTexture, uv + vec2(texel.x, 0.0)).rgb + texture(sourceTexture, uv - vec2(texel.x, 0.0)).rgb + texture(sourceTexture, uv + vec2(0.0, texel.y)).rgb + texture(sourceTexture, uv - vec2(0.0, texel.y)).rgb;
	vec3 sharpened = clamp(color.rgb * 5.0 - neighbors, 0.0, 1.0);
	color.rgb = mix(color.rgb, sharpened, strength);
	"),
	("edgeDetect", "
	vec2 texel = 1.0 / resolution;
	float gradientX = 0.0;
	float gradientY = 0.0;
	gradientX += luminanceAt(uv + vec2(-texel.x, -texel.y)) * -1.0 + luminanceAt(uv + vec2(texel.x, -texel.y));
	gradientX += luminanceAt(uv + vec2(-texel.x, 0.0)) * -2.0 + luminanceAt(uv + vec2(texel.x, 0.0)) * 2.0;
	gradientX += luminanceAt(uv + vec2(-texel.x, texel.y)) * -1.0 + luminanceAt(uv + vec2(texel.x, texel.y));
	gradientY += luminanceAt(uv + vec2(-texel.x, -texel.y)) * -1.0 + luminanceAt(uv + vec2(-texel.x, texel.y));
	gradientY += luminanceAt(uv + vec2(0.0, -texel.y)) * -2.0 + luminanceAt(uv + vec2(0.0, texel.y)) * 2.0;
	gradientY += luminanceAt(uv + vec2(texel.x, -texel.y)) * -1.0 + luminanceAt(uv + vec2(texel.x, texel.y));
	float edge = clamp(length(vec2(gradientX, gradientY)) * 2.0, 0.0, 1.0);
	color.rgb = mix(color.rgb, effectColor.rgb * edge, strength);
	"),
	("oldFilm", "
	float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
	vec3 filmColor = gray * vec3(1.15, 1.0, 0.8);
	float grain = (hash21(uv * resolution + fract(time * 9.1) * 100.0) - 0.5) * 0.25;
	float scratchSeed = floor(time * 8.0);
	float scratchX = hash21(vec2(scratchSeed, 1.0));
	float scratch = (1.0 - smoothstep(0.0, 0.003, abs(uv.x - scratchX))) * step(0.6, hash21(vec2(scratchSeed, 3.0)));
	float flicker = 0.9 + 0.1 * hash21(vec2(floor(time * 12.0), 5.0));
	float vignette = 1.0 - smoothstep(0.5, 1.0, distance(uv, vec2(0.5)) * 1.4142) * 0.6;
	vec3 aged = (filmColor + grain + scratch * 0.8) * flicker * vignette;
	color.rgb = mix(color.rgb, clamp(aged, 0.0, 1.0), strength);
	"),
	("nightVision", "
	float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
	float amplified = clamp(gray * 1.8 + 0.08, 0.0, 1.0);
	float noise = (hash21(uv * resolution + fract(time * 7.0) * 100.0) - 0.5) * 0.25;
	float lines = 0.9 + 0.1 * sin(uv.y * resolution.y * 1.5);
	float vignette = 1.0 - smoothstep(0.55, 1.0, distance(uv, vec2(0.5)) * 1.4142);
	vec3 visionColor = effectColor.rgb * (amplified + noise) * lines * vignette;
	color.rgb = mix(color.rgb, clamp(visionColor, 0.0, 1.0), strength);
	"),
	("invert", "
	color.rgb = mix(color.rgb, vec3(1.0) - color.rgb, strength);
	"),
	("scanline", "
	vec2 centered = uv - 0.5;
	float radiusSquared = dot(centered, centered);
	vec2 warped = uv + centered * radiusSquared * parameters.x * strength;
	vec2 insideMask = step(vec2(0.0), warped) * step(warped, vec2(1.0));
	color = texture(sourceTexture, warped) * insideMask.x * insideMask.y;
	float lines = 0.5 + 0.5 * sin(warped.y * resolution.y * 3.14159265 * parameters.y);
	color.rgb *= 1.0 - strength * 0.35 * lines;
	float maskPhase = warped.x * resolution.x * 2.0943951;
	color.rgb *= 1.0 + strength * 0.08 * vec3(sin(maskPhase), sin(maskPhase + 2.0943951), sin(maskPhase + 4.1887902));
	"),
	("grain", "
	float grainValue = hash21(uv * resolution + fract(time * 7.13) * 100.0) - 0.5;
	color.rgb += grainValue * strength * 0.25;
	"),
	("rain", "
	float column = floor(screenUv.x * parameters.x);
	float columnRandom = hash21(vec2(column, 1.0));
	float streakY = fract(screenUv.y * parameters.y * (0.6 + columnRandom * 0.8) - time * parameters.z * (0.8 + columnRandom * 0.6) + columnRandom * 7.0);
	float columnX = fract(screenUv.x * parameters.x);
	float streak = smoothstep(0.7, 1.0, streakY) * (1.0 - smoothstep(0.0, 0.25, abs(columnX - 0.5))) * step(0.35, columnRandom);
	color.rgb = mix(color.rgb, effectColor.rgb, streak * strength * effectColor.a);
	"),
	("fog", "
	float fogMask = smoothstep(parameters.x, parameters.y, screenUv.y) * strength;
	color.rgb = mix(color.rgb, effectColor.rgb, fogMask * effectColor.a);
	"),
	("vignette", "
	float vignetteDistance = distance(uv, vec2(0.5)) * 1.4142;
	float vignetteMask = smoothstep(parameters.x, parameters.y, vignetteDistance) * strength;
	color.rgb = mix(color.rgb, effectColor.rgb, vignetteMask * effectColor.a);
	"),
	("spotlight", "
	float aspect = resolution.x / resolution.y;
	float spotDistance = length((screenUv - parameters.xy) * vec2(aspect, 1.0));
	float spotMask = smoothstep(parameters.z, parameters.z + max(parameters.w, 0.001), spotDistance) * strength;
	color.rgb = mix(color.rgb, effectColor.rgb, spotMask * effectColor.a);
	"),
	("speedLines", "
	float aspect = resolution.x / resolution.y;
	vec2 delta = (screenUv - parameters.xy) * vec2(aspect, 1.0);
	float lineDistance = length(delta);
	float angle = atan(delta.y, delta.x);
	float streak = valueNoise(vec2(cos(angle), sin(angle)) * parameters.z + vec2(time * parameters.w, 0.0));
	float lineMask = smoothstep(0.55, 0.75, streak) * smoothstep(0.15, 0.6, lineDistance);
	color.rgb = mix(color.rgb, effectColor.rgb, lineMask * strength * effectColor.a);
	"),
	("letterbox", "
	float bar = parameters.x * strength;
	float pillar = parameters.y * strength;
	float inside = step(bar, uv.y) * step(uv.y, 1.0 - bar) * step(pillar, uv.x) * step(uv.x, 1.0 - pillar);
	color.rgb = mix(effectColor.rgb, color.rgb, inside);
	"),
	("pixelDissolve", "
	vec2 blockCell = floor(screenUv * vec2(parameters.x, parameters.x * resolution.y / resolution.x));
	float blockRandom = hash21(blockCell + 3.7);
	color.rgb = mix(color.rgb, effectColor.rgb, step(blockRandom, strength) * effectColor.a);
	"),
	("wipe", "
	vec2 wipeDirection = normalize(parameters.xy + vec2(0.0001, 0.0));
	float wipeDistance = dot(screenUv - 0.5, wipeDirection) + 0.5;
	float softness = max(parameters.z, 0.0001);
	float cover = 1.0 - smoothstep(strength * (1.0 + softness) - softness, strength * (1.0 + softness), wipeDistance);
	color.rgb = mix(color.rgb, effectColor.rgb, cover * effectColor.a);
	"),
	("irisWipe", "
	float aspect = resolution.x / resolution.y;
	float irisDistance = length((screenUv - parameters.xy) * vec2(aspect, 1.0));
	float softness = max(parameters.z, 0.0001);
	float visibleRadius = (1.0 - strength) * 0.9 * max(aspect, 1.0);
	float cover = smoothstep(visibleRadius, visibleRadius + softness, irisDistance);
	color.rgb = mix(color.rgb, effectColor.rgb, cover * effectColor.a);
	"),
	("diamondWipe", "
	float diamondDistance = abs(screenUv.x - 0.5) + abs(screenUv.y - 0.5);
	float softness = max(parameters.x, 0.0001);
	float cover = 1.0 - smoothstep(strength * (1.0 + softness), strength * (1.0 + softness) + softness, diamondDistance);
	color.rgb = mix(color.rgb, effectColor.rgb, cover * effectColor.a);
	"),
	("clockWipe", "
	vec2 fromCenter = screenUv - 0.5;
	float clockAngle = (atan(fromCenter.x, -fromCenter.y) + 3.14159265) / 6.28318530718;
	float softness = max(parameters.x, 0.0001);
	float cover = 1.0 - smoothstep(strength, strength + softness, clockAngle);
	color.rgb = mix(color.rgb, effectColor.rgb, cover * effectColor.a);
	"),
	("blinds", "
	float stripe = fract(screenUv.y * parameters.x);
	float softness = max(parameters.y, 0.0001);
	float cover = 1.0 - smoothstep(strength, strength + softness, stripe);
	color.rgb = mix(color.rgb, effectColor.rgb, cover * effectColor.a);
	"),
	("checkerWipe", "
	vec2 checkerCell = floor(screenUv * vec2(parameters.x, parameters.x * resolution.y / resolution.x));
	float parity = mod(checkerCell.x + checkerCell.y, 2.0);
	float cellThreshold = parity * 0.5 + hash21(checkerCell + 1.3) * 0.5;
	color.rgb = mix(color.rgb, effectColor.rgb, step(cellThreshold, strength * 1.0001) * effectColor.a);
	"),
	("noiseFade", "
	float fadeNoise = valueNoise(screenUv * vec2(parameters.x * resolution.x / resolution.y, parameters.x) + 2.1);
	float softness = max(parameters.y, 0.001);
	float cover = 1.0 - smoothstep(strength * (1.0 + softness) - softness, strength * (1.0 + softness), fadeNoise);
	color.rgb = mix(color.rgb, effectColor.rgb, cover * effectColor.a);
	"),
	("burn", "
	float burnNoise = valueNoise(screenUv * vec2(parameters.x * resolution.x / resolution.y, parameters.x) + 5.3);
	float threshold = strength * (1.0 + parameters.y * 4.0) - parameters.y * 2.0;
	float burned = 1.0 - step(threshold, burnNoise);
	float innerBand = smoothstep(threshold, threshold + parameters.y, burnNoise);
	float outerBand = smoothstep(threshold + parameters.y, threshold + parameters.y * 2.0, burnNoise);
	vec3 emberColor = vec3(1.0, 0.55, 0.15);
	vec3 sootColor = vec3(0.05, 0.02, 0.01);
	vec3 burning = mix(emberColor, mix(sootColor, color.rgb, outerBand), innerBand);
	color.rgb = mix(burning, effectColor.rgb, burned * effectColor.a);
	"),
	("tvOff", "
	float collapse = max(1.0 - strength, 0.0);
	float squeeze = max(collapse * collapse, 0.002);
	float centeredY = (screenUv.y - 0.5) / squeeze + 0.5;
	float widthSqueeze = smoothstep(0.0, 0.15, collapse);
	float centeredX = (screenUv.x - 0.5) / max(widthSqueeze, 0.002) + 0.5;
	float inside = step(0.0, centeredY) * step(centeredY, 1.0) * step(0.0, centeredX) * step(centeredX, 1.0);
	vec3 collapsed = sampleScreen(vec2(centeredX, centeredY)).rgb * (1.0 + (1.0 - collapse) * 2.0);
	float glowLine = (1.0 - smoothstep(0.0, 0.01 + squeeze * 0.5, abs(screenUv.y - 0.5))) * (1.0 - collapse) * step(0.02, widthSqueeze);
	color.rgb = mix(effectColor.rgb, collapsed, inside) + vec3(glowLine);
	color.rgb = mix(color.rgb, effectColor.rgb, step(0.999, strength));
	"),
	("fade", "
	color.rgb = mix(color.rgb, effectColor.rgb, strength * effectColor.a);
	"),
	("regionComposite", "
	vec4 original = texture(auxiliaryTexture, uv);
	vec2 pixel = uv * resolution;
	vec2 halfSize = resolution * 0.5;
	float cornerRadius = min(parameters.x, min(halfSize.x, halfSize.y));
	vec2 cornerOffset = abs(pixel - halfSize) - (halfSize - cornerRadius);
	float cornerDistance = length(max(cornerOffset, 0.0)) + min(max(cornerOffset.x, cornerOffset.y), 0.0) - cornerRadius;
	float inside = 1.0 - smoothstep(-0.75, 0.75, cornerDistance);
	color = mix(original, color, inside);
	"),
];

impl ScreenEffectType {
	//==============================================================================
	// 원본 이름 반환. ("vignette" 같은 문자열)
	//==============================================================================
	pub fn getName(&self) -> &'static str {
		EFFECT_NAME_TABLE[*self as usize].1
	}

	//==============================================================================
	// 원본 이름으로 찾기. (모르는 이름이면 None)
	//==============================================================================
	pub fn fromName(name: &str) -> Option<ScreenEffectType> {
		EFFECT_NAME_TABLE.iter().find(|(_, effectName)| *effectName == name).map(|(effectType, _)| *effectType)
	}

	//==============================================================================
	// 파라미터 이름 목록 반환. (SCREEN_EFFECT_PARAMETER_NAMES)
	//==============================================================================
	pub fn getParameterNames(&self) -> &'static [&'static str] {
		SCREEN_EFFECT_PARAMETER_NAMES[*self as usize].1
	}
}

//==============================================================================
// 효과 하나의 상태.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ScreenEffectState {
	isEnabled: bool,
	strength: f32,
	parameters: [f32; 4],
	color: Color,
}

//==============================================================================
// 적용 영역. (캔버스 픽셀, 왼쪽 위 원점)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenEffectRegion {
	pub x: f32,
	pub y: f32,
	pub width: f32,
	pub height: f32,
	// 둥근 모서리 반지름. (픽셀, 0 이면 사각형)
	pub cornerRadius: f32,
}

//==============================================================================
// 렌더 대상. (색 텍스처 + 필요하면 스텐실 렌더버퍼)
//==============================================================================
struct RenderTarget {
	framebuffer: GlFramebuffer,
	texture: GlTexture,
	stencilRenderbuffer: Option<GlRenderbuffer>,
	width: i32,
	height: i32,
}

//==============================================================================
// 렌더 대상 모음. (씬 사본, 핑퐁, 절반 크기 둘)
//==============================================================================
struct RenderTargetSet {
	sceneTarget: RenderTarget,
	pingTarget: RenderTarget,
	pongTarget: RenderTarget,
	halfTargetA: RenderTarget,
	halfTargetB: RenderTarget,
}

//==============================================================================
// 패스 단계가 그리는 곳.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassStepTarget {
	pingPong,
	halfA,
	halfB,
}

//==============================================================================
// 패스 단계가 읽는 텍스처.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassStepTexture {
	previous,
	halfA,
	halfB,
	scene,
}

//==============================================================================
// 패스 단계. (효과 하나가 여러 단계로 펴지기도 한다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct PassStep {
	passName: &'static str,
	strength: f32,
	color: Color,
	parameters: [f32; 4],
	target: PassStepTarget,
	source: PassStepTexture,
	auxiliary: Option<PassStepTexture>,
}

//==============================================================================
// 패스 단계 만들기. (상태가 없으면 세기 1, 흰색)
//==============================================================================
fn createPassStep(passName: &'static str, effectState: Option<&ScreenEffectState>, parameters: [f32; 4], target: PassStepTarget, source: PassStepTexture, auxiliary: Option<PassStepTexture>) -> PassStep {
	let (strength, color) = match effectState {
		Some(effectState) => {
			(effectState.strength, effectState.color)
		}
		None => {
			(1.0, Color::white())
		}
	};
	PassStep {
		passName,
		strength,
		color,
		parameters,
		target,
		source,
		auxiliary,
	}
}

//==============================================================================
// 색 텍스처 생성. (RGBA8, 화면 사본은 RGB8: 기본 프레임버퍼(알파 없음)에서 copyTexSubImage2D 로 떠 오려면 성분이 같아야 한다)
//==============================================================================
fn createColorTexture(gl: &glow::Context, width: i32, height: i32, isOpaque: bool) -> Result<GlTexture, String> {
	unsafe {
		let texture = gl.create_texture()?;
		gl.bind_texture(glow::TEXTURE_2D, Some(texture));
		if isOpaque {
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGB8 as i32, width, height, 0, glow::RGB, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
		}
		else {
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, width, height, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
		}
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
		Ok(texture)
	}
}

//==============================================================================
// 렌더 대상 생성. (색 텍스처 + 필요하면 스텐실 렌더버퍼)
//==============================================================================
fn createRenderTarget(gl: &glow::Context, width: i32, height: i32, useStencil: bool, isOpaque: bool) -> Result<RenderTarget, String> {
	let texture = createColorTexture(gl, width, height, isOpaque)?;
	unsafe {
		let framebuffer = gl.create_framebuffer()?;
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
		let mut stencilRenderbuffer = None;
		if useStencil {
			let renderbuffer = gl.create_renderbuffer()?;
			gl.bind_renderbuffer(glow::RENDERBUFFER, Some(renderbuffer));
			gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, width, height);
			gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(renderbuffer));
			stencilRenderbuffer = Some(renderbuffer);
		}
		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
		Ok(RenderTarget {
			framebuffer,
			texture,
			stencilRenderbuffer,
			width,
			height,
		})
	}
}

//==============================================================================
// 렌더 대상 파괴.
//==============================================================================
fn destroyRenderTarget(gl: &glow::Context, renderTarget: &RenderTarget) {
	unsafe {
		gl.delete_framebuffer(renderTarget.framebuffer);
		gl.delete_texture(renderTarget.texture);
		if let Some(stencilRenderbuffer) = renderTarget.stencilRenderbuffer {
			gl.delete_renderbuffer(stencilRenderbuffer);
		}
	}
}

//==============================================================================
// 렌더 대상 모음 파괴.
//==============================================================================
fn destroyRenderTargetSet(gl: &glow::Context, renderTargetSet: &RenderTargetSet) {
	destroyRenderTarget(gl, &renderTargetSet.sceneTarget);
	destroyRenderTarget(gl, &renderTargetSet.pingTarget);
	destroyRenderTarget(gl, &renderTargetSet.pongTarget);
	destroyRenderTarget(gl, &renderTargetSet.halfTargetA);
	destroyRenderTarget(gl, &renderTargetSet.halfTargetB);
}

//==============================================================================
// 패스 본문 반환. (모르는 이름이면 None)
//==============================================================================
fn findEffectBody(passName: &str) -> Option<&'static str> {
	EFFECT_BODY_TABLE.iter().find(|(bodyName, _)| *bodyName == passName).map(|(_, effectBody)| *effectBody)
}

pub struct ScreenEffect {
	passTable: HashMap<&'static str, Option<FullscreenPass>>,
	effectStateList: Vec<ScreenEffectState>,
	renderTargetSet: Option<RenderTargetSet>,
	stepList: Vec<PassStep>,
	time: f32,
	isBound: bool,
	region: Option<ScreenEffectRegion>,
}

impl ScreenEffect {
	//==============================================================================
	// 생성. (GL 자원은 처음 end 에서 만든다)
	//==============================================================================
	pub fn new() -> ScreenEffect {
		let mut screenEffect = ScreenEffect {
			passTable: HashMap::new(),
			effectStateList: Vec::with_capacity(EFFECT_ORDER.len()),
			renderTargetSet: None,
			stepList: Vec::new(),
			time: 0.0,
			isBound: false,
			region: None,
		};
		screenEffect.resetAll();
		screenEffect
	}

	//==============================================================================
	// 전부 기본값으로. (끄고, 세기 1, 파라미터 / 색 기본. 모드 전환 때 이전 설정이 남지 않게)
	//==============================================================================
	pub fn resetAll(&mut self) {
		self.effectStateList.clear();
		for effectType in EFFECT_ORDER {
			let defaultParameters = DEFAULT_PARAMETER_TABLE[effectType as usize].1;
			let defaultColor = match DEFAULT_COLOR_TABLE.iter().find(|(colorEffectType, _)| *colorEffectType == effectType) {
				Some((_, defaultColor)) => {
					*defaultColor
				}
				None => {
					[1.0, 1.0, 1.0, 1.0]
				}
			};
			self.effectStateList.push(ScreenEffectState {
				isEnabled: false,
				strength: 1.0,
				parameters: defaultParameters,
				color: Color::create(defaultColor[0], defaultColor[1], defaultColor[2], defaultColor[3]),
			});
		}
	}

	//==============================================================================
	// 적용 영역 설정. (캔버스 픽셀, 왼쪽 위 원점. cornerRadius 는 둥근 모서리 반지름(픽셀), 0 이면 사각형)
	//==============================================================================
	pub fn setRegion(&mut self, x: f32, y: f32, width: f32, height: f32, cornerRadius: f32) {
		self.region = Some(ScreenEffectRegion {
			x,
			y,
			width,
			height,
			cornerRadius: cornerRadius.max(0.0),
		});
	}

	//==============================================================================
	// 적용 영역 해제. (화면 전체)
	//==============================================================================
	pub fn clearRegion(&mut self) {
		self.region = None;
	}

	//==============================================================================
	// 적용 영역 반환. (None 이면 화면 전체)
	//==============================================================================
	pub fn getRegion(&self) -> Option<ScreenEffectRegion> {
		self.region
	}

	//==============================================================================
	// 갱신. (애니메이션 효과의 시간)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		self.time += timeDelta;
	}

	//==============================================================================
	// 시작. (씬 preDraw 첫머리. 씬은 화면에 그대로 그린다. 켜진 효과가 있을 때만 end 가 일한다)
	//==============================================================================
	pub fn begin(&mut self, _graphic: &mut Graphic) {
		self.isBound = self.hasActiveEffect();
	}

	//==============================================================================
	// 효과 적용. (씬 postDraw 끝. 화면의 영역을 떠 와 효과를 걸고 같은 자리에 되돌려 놓는다)
	//==============================================================================
	pub fn end(&mut self, graphic: &mut Graphic) {
		if !self.isBound {
			return;
		}
		self.isBound = false;

		// 묶어 그리기로 쌓인 것을 먼저 화면에 그린다. (그 화면을 떠 온다)
		graphic.flushBatch();
		let bufferWidth = graphic.getDrawingBufferWidth();
		let bufferHeight = graphic.getDrawingBufferHeight();
		if bufferWidth <= 0 || bufferHeight <= 0 {
			return;
		}

		// 영역 결정. (버퍼 안으로 자른다)
		let mut regionX = 0;
		let mut regionY = 0;
		let mut regionWidth = bufferWidth;
		let mut regionHeight = bufferHeight;
		let mut cornerRadius = 0.0;
		if let Some(region) = self.region {
			regionX = (region.x.round() as i32).max(0);
			regionY = (region.y.round() as i32).max(0);
			regionWidth = (bufferWidth - regionX).min(region.width.round() as i32);
			regionHeight = (bufferHeight - regionY).min(region.height.round() as i32);
			cornerRadius = region.cornerRadius;
		}
		if regionWidth <= 0 || regionHeight <= 0 {
			return;
		}
		let width = regionWidth;
		let height = regionHeight;
		let gl = graphic.getGL();
		if let Err(error) = self.ensureTargets(gl, width, height) {
			eprintln!("ScreenEffect: render target failed: {}", error);
			return;
		}

		// 켜진 효과를 패스 단계 목록으로 편다. (블러는 가로 / 세로 2단계, 블룸은 밝기 추출 → 하프 블러 2단계 → 합성)
		self.stepList.clear();
		for effectType in EFFECT_ORDER {
			let effectState = self.effectStateList[effectType as usize];
			if !effectState.isEnabled || effectState.strength <= 0.0 {
				continue;
			}
			let parameters = effectState.parameters;
			let state = Some(&effectState);
			match effectType {
				ScreenEffectType::blur => {
					self.stepList.push(createPassStep("blur", state, [parameters[0], 0.0, 1.0, 0.0], PassStepTarget::pingPong, PassStepTexture::previous, None));
					self.stepList.push(createPassStep("blur", state, [parameters[0], 0.0, 0.0, 1.0], PassStepTarget::pingPong, PassStepTexture::previous, None));
				}
				ScreenEffectType::bloom => {
					self.stepList.push(createPassStep("bloomBright", state, parameters, PassStepTarget::halfA, PassStepTexture::previous, None));
					self.stepList.push(createPassStep("blur", state, [parameters[2], 0.0, 1.0, 0.0], PassStepTarget::halfB, PassStepTexture::halfA, None));
					self.stepList.push(createPassStep("blur", state, [parameters[2], 0.0, 0.0, 1.0], PassStepTarget::halfA, PassStepTexture::halfB, None));
					self.stepList.push(createPassStep("bloomComposite", state, parameters, PassStepTarget::pingPong, PassStepTexture::previous, Some(PassStepTexture::halfA)));
				}
				ScreenEffectType::anamorphic => {
					self.stepList.push(createPassStep("bloomBright", state, parameters, PassStepTarget::halfA, PassStepTexture::previous, None));
					self.stepList.push(createPassStep("blur", state, [parameters[2], 0.0, 1.0, 0.0], PassStepTarget::halfB, PassStepTexture::halfA, None));
					self.stepList.push(createPassStep("blur", state, [parameters[2] * 0.5, 0.0, 1.0, 0.0], PassStepTarget::halfA, PassStepTexture::halfB, None));
					self.stepList.push(createPassStep("anamorphicComposite", state, parameters, PassStepTarget::pingPong, PassStepTexture::previous, Some(PassStepTexture::halfA)));
				}
				ScreenEffectType::tiltShift => {
					self.stepList.push(createPassStep("tiltShift", state, [parameters[0], parameters[1], parameters[2], 0.0], PassStepTarget::pingPong, PassStepTexture::previous, None));
					self.stepList.push(createPassStep("tiltShift", state, [parameters[0], parameters[1], 0.0, parameters[2]], PassStepTarget::pingPong, PassStepTexture::previous, None));
				}
				_ => {
					self.stepList.push(createPassStep(effectType.getName(), state, parameters, PassStepTarget::pingPong, PassStepTexture::previous, None));
				}
			}
		}
		if self.stepList.is_empty() {
			self.stepList.push(createPassStep("copy", None, [0.0, 0.0, 0.0, 0.0], PassStepTarget::pingPong, PassStepTexture::previous, None));
		}
		// 둥근 모서리 영역이면 마지막에 원본과 합성한다. (모서리 바깥은 원본 그대로)
		if cornerRadius > 0.0 {
			self.stepList.push(createPassStep("regionComposite", None, [cornerRadius, 0.0, 0.0, 0.0], PassStepTarget::pingPong, PassStepTexture::previous, Some(PassStepTexture::scene)));
		}

		// 마지막 화면 출력 단계를 찾는다. (하프 대상에 그리는 단계는 화면이 될 수 없다)
		let lastScreenIndex = self.stepList.iter().rposition(|step| step.target == PassStepTarget::pingPong);

		// 쓰는 패스를 미리 만든다.
		for stepIndex in 0..self.stepList.len() {
			let passName = self.stepList[stepIndex].passName;
			self.ensurePass(gl, passName);
		}

		let renderTargetSet = match &self.renderTargetSet {
			Some(renderTargetSet) => {
				renderTargetSet
			}
			None => {
				return;
			}
		};

		// 화면 영역 → 씬 대상 복사. (GL 은 아래가 0. 기본 프레임버퍼는 멀티샘플 / 무알파일 수 있어 blitFramebuffer 대신 copyTexSubImage2D 로 RGB8 텍스처에 떠 온다)
		let glRegionY = bufferHeight - (regionY + regionHeight);
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, Some(renderTargetSet.sceneTarget.texture));
			gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, regionX, glRegionY, width, height);
			gl.disable(glow::BLEND);
			gl.disable(glow::STENCIL_TEST);
		}

		let mut previousTexture = renderTargetSet.sceneTarget.texture;
		let mut pingPongTurn = 0;
		for (stepIndex, step) in self.stepList.iter().enumerate() {
			let pass = match self.passTable.get(step.passName) {
				Some(Some(pass)) => {
					pass
				}
				_ => {
					continue;
				}
			};
			let isHalfTarget = step.target != PassStepTarget::pingPong;

			// 대상 바인드.
			let mut targetTexture = None;
			unsafe {
				if Some(stepIndex) == lastScreenIndex {
					gl.bind_framebuffer(glow::FRAMEBUFFER, None);
					gl.viewport(regionX, glRegionY, width, height);
					gl.enable(glow::SCISSOR_TEST);
					gl.scissor(regionX, glRegionY, width, height);
				}
				else if isHalfTarget {
					let halfTarget = if step.target == PassStepTarget::halfA {
						&renderTargetSet.halfTargetA
					}
					else {
						&renderTargetSet.halfTargetB
					};
					gl.bind_framebuffer(glow::FRAMEBUFFER, Some(halfTarget.framebuffer));
					gl.viewport(0, 0, halfTarget.width, halfTarget.height);
					targetTexture = Some(halfTarget.texture);
				}
				else {
					let pingPongTarget = if pingPongTurn % 2 == 0 {
						&renderTargetSet.pingTarget
					}
					else {
						&renderTargetSet.pongTarget
					};
					pingPongTurn += 1;
					gl.bind_framebuffer(glow::FRAMEBUFFER, Some(pingPongTarget.framebuffer));
					gl.viewport(0, 0, width, height);
					targetTexture = Some(pingPongTarget.texture);
				}
			}

			// 소스 결정. (하프 단계는 지정한 하프 텍스처, 블룸 합성은 마지막 전체 해상도 결과)
			let sourceTexture = match step.source {
				PassStepTexture::halfA => {
					renderTargetSet.halfTargetA.texture
				}
				PassStepTexture::halfB => {
					renderTargetSet.halfTargetB.texture
				}
				_ => {
					previousTexture
				}
			};
			let (passWidth, passHeight) = if isHalfTarget {
				(renderTargetSet.halfTargetA.width, renderTargetSet.halfTargetA.height)
			}
			else {
				(width, height)
			};

			pass.useProgram(gl);
			unsafe {
				gl.active_texture(glow::TEXTURE0);
				gl.bind_texture(glow::TEXTURE_2D, Some(sourceTexture));
				gl.uniform_1_i32(pass.getUniformLocation(gl, "sourceTexture").as_ref(), 0);
				if let Some(auxiliary) = step.auxiliary {
					let auxiliaryTexture = match auxiliary {
						PassStepTexture::halfA => {
							renderTargetSet.halfTargetA.texture
						}
						PassStepTexture::scene => {
							renderTargetSet.sceneTarget.texture
						}
						_ => {
							renderTargetSet.halfTargetB.texture
						}
					};
					gl.active_texture(glow::TEXTURE1);
					gl.bind_texture(glow::TEXTURE_2D, Some(auxiliaryTexture));
					gl.uniform_1_i32(pass.getUniformLocation(gl, "auxiliaryTexture").as_ref(), 1);
					gl.active_texture(glow::TEXTURE0);
				}
				gl.uniform_2_f32(pass.getUniformLocation(gl, "resolution").as_ref(), passWidth as f32, passHeight as f32);
				gl.uniform_1_f32(pass.getUniformLocation(gl, "time").as_ref(), self.time);
				gl.uniform_1_f32(pass.getUniformLocation(gl, "strength").as_ref(), step.strength);
				gl.uniform_4_f32(pass.getUniformLocation(gl, "parameters").as_ref(), step.parameters[0], step.parameters[1], step.parameters[2], step.parameters[3]);
				gl.uniform_4_f32(pass.getUniformLocation(gl, "effectColor").as_ref(), step.color.red, step.color.green, step.color.blue, step.color.alpha);
			}
			pass.draw(gl);

			// 하프 단계는 전체 해상도 사슬의 "이전 결과" 를 바꾸지 않는다.
			if !isHalfTarget {
				if let Some(targetTexture) = targetTexture {
					previousTexture = targetTexture;
				}
			}
		}

		unsafe {
			gl.disable(glow::SCISSOR_TEST);
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.bind_vertex_array(None);
		}
		graphic.restoreRenderState();
	}

	//==============================================================================
	// 켜진 효과 여부. (세기 0 은 꺼진 것으로 본다)
	//==============================================================================
	pub fn hasActiveEffect(&self) -> bool {
		self.effectStateList.iter().any(|effectState| effectState.isEnabled && effectState.strength > 0.0)
	}

	//==============================================================================
	// 렌더 대상 준비. (크기가 바뀌면 다시 만든다)
	//==============================================================================
	fn ensureTargets(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
		if let Some(renderTargetSet) = &self.renderTargetSet {
			if renderTargetSet.sceneTarget.width == width && renderTargetSet.sceneTarget.height == height {
				return Ok(());
			}
			destroyRenderTargetSet(gl, renderTargetSet);
			self.renderTargetSet = None;
		}
		let halfWidth = ((width as f32 / 2.0).round() as i32).max(1);
		let halfHeight = ((height as f32 / 2.0).round() as i32).max(1);
		let renderTargetSet = RenderTargetSet {
			sceneTarget: createRenderTarget(gl, width, height, false, true)?,
			pingTarget: createRenderTarget(gl, width, height, false, false)?,
			pongTarget: createRenderTarget(gl, width, height, false, false)?,
			halfTargetA: createRenderTarget(gl, halfWidth, halfHeight, false, false)?,
			halfTargetB: createRenderTarget(gl, halfWidth, halfHeight, false, false)?,
		};
		self.renderTargetSet = Some(renderTargetSet);
		Ok(())
	}

	//==============================================================================
	// 패스 준비. (없으면 컴파일해 담는다. 실패도 담아 두고 한 번만 알린다. 원본 resolvePass)
	//==============================================================================
	fn ensurePass(&mut self, gl: &glow::Context, passName: &'static str) {
		if self.passTable.contains_key(passName) {
			return;
		}
		let effectBody = findEffectBody(passName).unwrap_or("");
		let fragmentShaderSource = format!("{}{}{}", FRAGMENTSHADER_HEADER, effectBody, FRAGMENTSHADER_FOOTER);
		let pass = match FullscreenPass::new(gl, &fragmentShaderSource) {
			Ok(pass) => {
				Some(pass)
			}
			Err(error) => {
				eprintln!("ScreenEffect: pass \"{}\" failed: {}", passName, error);
				None
			}
		};
		self.passTable.insert(passName, pass);
	}

	//==============================================================================
	// 효과 켜기 / 끄기.
	//==============================================================================
	pub fn setEnabled(&mut self, effectType: ScreenEffectType, isEnabled: bool) {
		self.effectStateList[effectType as usize].isEnabled = isEnabled;
	}

	//==============================================================================
	// 효과 켜짐 여부 반환.
	//==============================================================================
	pub fn isEnabled(&self, effectType: ScreenEffectType) -> bool {
		self.effectStateList[effectType as usize].isEnabled
	}

	//==============================================================================
	// 세기 설정. (0 이면 꺼진 것과 같다)
	//==============================================================================
	pub fn setStrength(&mut self, effectType: ScreenEffectType, strength: f32) {
		self.effectStateList[effectType as usize].strength = strength;
	}

	//==============================================================================
	// 세기 반환.
	//==============================================================================
	pub fn getStrength(&self, effectType: ScreenEffectType) -> f32 {
		self.effectStateList[effectType as usize].strength
	}

	//==============================================================================
	// 파라미터 설정. (효과마다 뜻이 다르다. ScreenEffectType 주석 참고, 원본의 y / z / w 기본값 0 은 직접 넘긴다)
	//==============================================================================
	pub fn setParameters(&mut self, effectType: ScreenEffectType, x: f32, y: f32, z: f32, w: f32) {
		self.effectStateList[effectType as usize].parameters = [x, y, z, w];
	}

	//==============================================================================
	// 파라미터 하나 설정. (이름은 SCREEN_EFFECT_PARAMETER_NAMES)
	//==============================================================================
	pub fn setParameter(&mut self, effectType: ScreenEffectType, parameterName: &str, value: f32) {
		let parameterNames = effectType.getParameterNames();
		if let Some(parameterIndex) = parameterNames.iter().position(|name| *name == parameterName) {
			self.effectStateList[effectType as usize].parameters[parameterIndex] = value;
		}
	}

	//==============================================================================
	// 파라미터 반환.
	//==============================================================================
	pub fn getParameters(&self, effectType: ScreenEffectType) -> [f32; 4] {
		self.effectStateList[effectType as usize].parameters
	}

	//==============================================================================
	// 색 설정. (Color 또는 "#rrggbb" 같은 문자열)
	//==============================================================================
	pub fn setColor(&mut self, effectType: ScreenEffectType, color: impl Into<Color>) {
		self.effectStateList[effectType as usize].color = color.into();
	}

	//==============================================================================
	// 색 반환.
	//==============================================================================
	pub fn getColor(&self, effectType: ScreenEffectType) -> Color {
		self.effectStateList[effectType as usize].color
	}

	//==============================================================================
	// 전부 끄기.
	//==============================================================================
	pub fn disableAll(&mut self) {
		for effectState in self.effectStateList.iter_mut() {
			effectState.isEnabled = false;
		}
	}

	//==============================================================================
	// 타임라인 속성 적용. ("vignette" = 세기(0 이면 끔), "shockwave.radius" = 파라미터, "fade.color" = 색, "fade.enabled" = 켜기)
	// - 값은 문자열이다. 숫자는 원본 Number() 처럼 읽고(빈 문자열은 0, 못 읽으면 NaN), 색은 "#rrggbb" 다.
	//==============================================================================
	pub fn setTimelineProperty(&mut self, propertyName: &str, value: &str) {
		let (effectName, memberName) = match propertyName.find('.') {
			Some(separatorIndex) => {
				(&propertyName[..separatorIndex], &propertyName[separatorIndex + 1..])
			}
			None => {
				(propertyName, "")
			}
		};
		let effectType = match ScreenEffectType::fromName(effectName) {
			Some(effectType) => {
				effectType
			}
			None => {
				return;
			}
		};
		if memberName.is_empty() {
			let strength = parseTimelineNumber(value);
			let effectState = &mut self.effectStateList[effectType as usize];
			effectState.strength = strength;
			effectState.isEnabled = strength > 0.0;
			return;
		}
		if memberName == "color" {
			self.effectStateList[effectType as usize].color = Color::createFromHEX(value);
			return;
		}
		if memberName == "enabled" {
			self.effectStateList[effectType as usize].isEnabled = value == "true";
			return;
		}
		self.setParameter(effectType, memberName, parseTimelineNumber(value));
	}

	//==============================================================================
	// 시간 반환.
	//==============================================================================
	pub fn getTime(&self) -> f32 {
		self.time
	}

	//==============================================================================
	// 씬 텍스처 반환. (end 이후, 마지막 프레임에 떠 온 영역 원본)
	//==============================================================================
	pub fn getSceneTexture(&self) -> Option<GlTexture> {
		self.renderTargetSet.as_ref().map(|renderTargetSet| renderTargetSet.sceneTarget.texture)
	}

	//==============================================================================
	// 적용 차례 반환. (정적)
	//==============================================================================
	pub fn getEffectOrder() -> Vec<ScreenEffectType> {
		EFFECT_ORDER.to_vec()
	}

	//==============================================================================
	// 파괴. (GL 자원 해제: 렌더 대상과 패스 프로그램)
	//==============================================================================
	pub fn destroy(&mut self, graphic: &mut Graphic) {
		let gl = graphic.getGL();
		if let Some(renderTargetSet) = self.renderTargetSet.take() {
			destroyRenderTargetSet(gl, &renderTargetSet);
		}
		for pass in self.passTable.values().flatten() {
			pass.destroy(gl);
		}
		self.passTable.clear();
	}
}

impl Default for ScreenEffect {
	fn default() -> ScreenEffect {
		ScreenEffect::new()
	}
}

//==============================================================================
// 타임라인 값을 숫자로 읽기. (원본 Number(): 빈 문자열은 0, 못 읽으면 NaN)
//==============================================================================
fn parseTimelineNumber(value: &str) -> f32 {
	let trimmedValue = value.trim();
	if trimmedValue.is_empty() {
		return 0.0;
	}
	trimmedValue.parse::<f32>().unwrap_or(f32::NAN)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 표들이 적용 차례(열거형 선언 차례)와 맞는지. (getName, 기본값 표는 효과 번호로 바로 찾는다)
	//==============================================================================
	#[test]
	fn tablesFollowEffectOrder() {
		for (effectIndex, effectType) in EFFECT_ORDER.iter().enumerate() {
			assert_eq!(*effectType as usize, effectIndex);
			assert_eq!(EFFECT_NAME_TABLE[effectIndex].0, *effectType);
			assert_eq!(DEFAULT_PARAMETER_TABLE[effectIndex].0, *effectType);
			assert_eq!(SCREEN_EFFECT_PARAMETER_NAMES[effectIndex].0, *effectType);
			assert_eq!(ScreenEffectType::fromName(effectType.getName()), Some(*effectType));
		}
	}

	//==============================================================================
	// 단계로 쓰이는 패스는 모두 본문이 있다. (bloom, anamorphic 은 내부 패스로 펴진다)
	//==============================================================================
	#[test]
	fn everyPassHasBody() {
		for effectType in EFFECT_ORDER.iter() {
			if *effectType == ScreenEffectType::bloom || *effectType == ScreenEffectType::anamorphic {
				continue;
			}
			assert!(findEffectBody(effectType.getName()).is_some(), "{:?}", effectType);
		}
		for passName in ["copy", "bloomBright", "bloomComposite", "anamorphicComposite", "regionComposite"] {
			assert!(findEffectBody(passName).is_some(), "{}", passName);
		}
	}

	//==============================================================================
	// 타임라인 속성: 세기, 파라미터 이름, 색, 켜기.
	//==============================================================================
	#[test]
	fn timelinePropertyUpdatesState() {
		let mut screenEffect = ScreenEffect::new();
		assert!(!screenEffect.hasActiveEffect());
		screenEffect.setTimelineProperty("vignette", "0.5");
		assert!(screenEffect.isEnabled(ScreenEffectType::vignette));
		assert_eq!(screenEffect.getStrength(ScreenEffectType::vignette), 0.5);
		assert!(screenEffect.hasActiveEffect());
		screenEffect.setTimelineProperty("vignette", "0");
		assert!(!screenEffect.isEnabled(ScreenEffectType::vignette));
		screenEffect.setTimelineProperty("shockwave.radius", "0.75");
		assert_eq!(screenEffect.getParameters(ScreenEffectType::shockwave), [0.5, 0.5, 0.75, 0.1]);
		screenEffect.setTimelineProperty("fade.color", "#ff0000");
		assert_eq!(screenEffect.getColor(ScreenEffectType::fade), Color::create(1.0, 0.0, 0.0, 1.0));
		screenEffect.setTimelineProperty("fade.enabled", "true");
		assert!(screenEffect.isEnabled(ScreenEffectType::fade));
		screenEffect.resetAll();
		assert!(!screenEffect.isEnabled(ScreenEffectType::fade));
		assert_eq!(screenEffect.getColor(ScreenEffectType::fade), Color::create(0.0, 0.0, 0.0, 1.0));
	}
}

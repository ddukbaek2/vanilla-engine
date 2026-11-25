//==============================================================================
//! 화면 효과. (씬 후처리 사슬)
//! - 원본 effect/screeneffect.js 를 옮긴 것이다. 효과 이름, 적용 차례, 기본 파라미터, 파라미터 이름, 기본 색, GLSL 본문은 원본 표를 그대로 옮겼다.
//! - 씬은 화면에 그대로 그리고, end() 에서 화면(또는 set_region 으로 정한 영역)을 떠 와 켜진 효과를 차례로 건 뒤 같은 자리에 되돌려 놓는다.
//!   화면은 copyTexSubImage2D 로 RGB8 텍스처에 떠 오고, 효과는 핑퐁 / 절반 크기 프레임버퍼를 오가며 FullscreenPass 로 건다. 마지막 단계는 기본 프레임버퍼에 그린다.
//! - 영역은 캔버스 픽셀 좌표(왼쪽 위 원점)이며 둥근 모서리 반지름을 주면 그 바깥은 원본을 유지한다.
//! - 셰이더는 GLSL ES 3.00 이다. 데스크탑(GL 3.3 코어)은 ShaderProgram 이 머리만 #version 330 core 로 바꾸고, 웹(WebGL2)은 그대로 쓴다.
//! - 엔진은 고치지 않는다. 원본처럼 씬이 pre_draw 첫머리에서 begin, post_draw 끝에서 end, tick 에서 tick 을 부른다:
//!   ```text
//!     // 씬 create 에서
//!     self.screen_effect = ScreenEffect::new();
//!     self.screen_effect.set_enabled(ScreenEffectType::Vignette, true);
//!     // pre_draw 처음 / post_draw 끝에서
//!     self.screen_effect.begin(graphic);  ...  self.screen_effect.end(graphic);
//!     // tick 에서
//!     self.screen_effect.tick(time_delta);
//!   ```
//! - 원본과 다른 점:
//!   - 원본은 생성자에서 Graphic 을 받아 GL 컨텍스트를 든다. 러스트 판은 아무것도 받지 않고 만들며, GL 자원은 처음 end 에서 만들고 함수마다 Graphic 을 받는다.
//!     그래서 destroy 도 Graphic 을 받는다. 씬을 내리기 전에 destroy 를 부르지 않으면 GL 자원은 컨텍스트가 없어질 때까지 남는다.
//!   - 효과는 문자열 대신 ScreenEffectType 열거형이다. (원본 문자열 이름은 get_name / from_name)
//!   - 패스 셰이더 만들기에 실패하면 원본은 예외를 던지고, 러스트 판은 한 번 알리고 그 단계를 건너뛴다.
//==============================================================================
use crate::animation::timeline::TimelineValue;
use crate::base::color::Color;
use crate::core::fullscreenpass::FullscreenPass;
use crate::core::graphic::{GlTexture, Graphic};
use glow::HasContext;
use std::collections::HashMap;

/// GL 객체 타입. (데스크탑은 glow::Native*, 웹은 WebGL 객체)
type GlFramebuffer = <glow::Context as glow::HasContext>::Framebuffer;
type GlRenderbuffer = <glow::Context as glow::HasContext>::Renderbuffer;

//==============================================================================
/// 화면 효과 종류. (선언 차례가 적용 차례다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScreenEffectType {
	/// --- 형태 왜곡 ---
	/// 배럴 / 핀쿠션 렌즈 왜곡. (parameters = `[세기: 음수면 핀쿠션]`)
	LensDistortion, // 배럴 / 핀쿠션 렌즈 왜곡. (parameters = [세기: 음수면 핀쿠션])
	/// 볼록 / 오목. (parameters = `[중심 x, 중심 y, 세기, 반지름]`)
	Bulge,          // 볼록 / 오목. (parameters = [중심 x, 중심 y, 세기, 반지름])
	/// 소용돌이. (parameters = `[중심 x, 중심 y, 회전 수, 반지름]`)
	Swirl,          // 소용돌이. (parameters = [중심 x, 중심 y, 회전 수, 반지름])
	/// 만화경. (parameters = `[조각 수]`)
	Kaleidoscope,   // 만화경. (parameters = [조각 수])
	/// 대칭. (parameters = `[0 가로 / 1 세로]`)
	Mirror,         // 대칭. (parameters = [0 가로 / 1 세로])
	/// 물 파문. (parameters = `[중심 x, 중심 y, 빈도, 속도]`)
	Ripple,         // 물 파문. (parameters = [중심 x, 중심 y, 빈도, 속도])
	/// 아지랑이. (parameters = `[노이즈 배율, 속도]`)
	HeatHaze,       // 아지랑이. (parameters = [노이즈 배율, 속도])
	/// 화면 흔들림. (parameters = `[진폭 픽셀, 초당 변화]`)
	Jitter,         // 화면 흔들림. (parameters = [진폭 픽셀, 초당 변화])
	/// 겹쳐 보임. (parameters = `[간격, 흔들리는 속도]`)
	DoubleVision,   // 겹쳐 보임. (parameters = [간격, 흔들리는 속도])
	/// 모자이크. (strength 0 → 1, parameters = `[가장 큰 블록 픽셀]`)
	Pixelate,       // 모자이크. (strength 0 → 1, parameters = [가장 큰 블록 픽셀])
	/// 충격파 링 왜곡. (parameters = `[중심 x, 중심 y, 반지름 0 ~ 1.5, 폭]`, color = 링 빛)
	Shockwave,      // 충격파 링 왜곡. (parameters = [중심 x, 중심 y, 반지름 0 ~ 1.5, 폭], color = 링 빛)
	/// 화면 물결. (parameters = `[빈도, 속도]`)
	Wave,           // 화면 물결. (parameters = [빈도, 속도])
	/// --- 흐림 ---
	/// 방향 블러. (parameters = `[각도 라디안, 길이 픽셀]`)
	MotionBlur,     // 방향 블러. (parameters = [각도 라디안, 길이 픽셀])
	/// 중심으로 뻗는 방사 블러. (parameters = `[중심 x, 중심 y, 길이]`)
	ZoomBlur,       // 중심으로 뻗는 방사 블러. (parameters = [중심 x, 중심 y, 길이])
	/// 가우시안 블러. (parameters = `[반지름 픽셀]`)
	Blur,           // 가우시안 블러. (parameters = [반지름 픽셀])
	/// 틸트 시프트. (parameters = `[초점 y, 초점 폭, 반지름 픽셀]`)
	TiltShift,      // 틸트 시프트. (parameters = [초점 y, 초점 폭, 반지름 픽셀])
	/// 빛줄기. (parameters = `[중심 x, 중심 y, 문턱, 감쇠]`, color = 빛 색)
	GodRays,        // 빛줄기. (parameters = [중심 x, 중심 y, 문턱, 감쇠], color = 빛 색)
	/// 밝은 곳 번짐. (parameters = `[문턱, 세기 배율, 반지름 픽셀]`)
	Bloom,          // 밝은 곳 번짐. (parameters = [문턱, 세기 배율, 반지름 픽셀])
	/// 가로 렌즈 플레어. (parameters = `[문턱, 세기 배율, 폭 픽셀]`, color = 플레어 색)
	Anamorphic,     // 가로 렌즈 플레어. (parameters = [문턱, 세기 배율, 폭 픽셀], color = 플레어 색)
	/// --- 글리치 ---
	/// 가로 줄 어긋남 + 색 분리 + 블록 반전. (parameters = `[줄 수, 초당 변화, 어긋남 폭]`)
	Glitch,         // 가로 줄 어긋남 + 색 분리 + 블록 반전. (parameters = [줄 수, 초당 변화, 어긋남 폭])
	/// VHS 테이프. (흔들림 + 색 번짐 + 노이즈 줄, parameters = `[흔들림, 노이즈]`)
	Vhs,            // VHS 테이프. (흔들림 + 색 번짐 + 노이즈 줄, parameters = [흔들림, 노이즈])
	/// 색수차. (parameters = `[배율]`)
	Chromatic,      // 색수차. (parameters = [배율])
	/// --- 색 ---
	/// 색온도 / 틴트. (parameters = `[온도 -1 ~ 1, 틴트 -1 ~ 1]`)
	WhiteBalance,   // 색온도 / 틴트. (parameters = [온도 -1 ~ 1, 틴트 -1 ~ 1])
	/// 리프트 / 감마 / 게인. (parameters = `[리프트, 감마, 게인]`)
	LiftGammaGain,  // 리프트 / 감마 / 게인. (parameters = [리프트, 감마, 게인])
	/// 대비 / 채도 / 밝기 + 틴트. (parameters = `[대비, 채도, 밝기]`, color = 틴트)
	ColorGrade,     // 대비 / 채도 / 밝기 + 틴트. (parameters = [대비, 채도, 밝기], color = 틴트)
	/// 색상 회전. (parameters = `[회전 수: strength 와 곱]`)
	HueShift,       // 색상 회전. (parameters = [회전 수: strength 와 곱])
	/// 스플릿 토닝. (parameters.xyz = 그림자 색, color = 하이라이트 색, w = 균형)
	SplitToning,    // 스플릿 토닝. (parameters.xyz = 그림자 색, color = 하이라이트 색, w = 균형)
	/// ACES 톤매핑. (parameters = `[노출]`)
	Tonemap,        // ACES 톤매핑. (parameters = [노출])
	/// 밝기를 두 색 사이로 매핑. (parameters.xyz = 어두운 색, color = 밝은 색)
	GradientMap,    // 밝기를 두 색 사이로 매핑. (parameters.xyz = 어두운 색, color = 밝은 색)
	/// 세피아.
	Sepia,          // 세피아.
	/// 무채색.
	Grayscale,      // 무채색.
	/// 포스터화. (parameters = `[단계 수]`)
	Posterize,      // 포스터화. (parameters = [단계 수])
	/// 오더드 디더링. (parameters = `[단계 수, 픽셀 크기]`)
	Dither,         // 오더드 디더링. (parameters = [단계 수, 픽셀 크기])
	/// 하프톤 점. (parameters = `[격자 픽셀]`)
	Halftone,       // 하프톤 점. (parameters = [격자 픽셀])
	/// 샤픈.
	Sharpen,        // 샤픈.
	/// 윤곽 검출. (color = 선 색)
	EdgeDetect,     // 윤곽 검출. (color = 선 색)
	/// 낡은 필름. (세피아 + 그레인 + 스크래치 + 깜빡임)
	OldFilm,        // 낡은 필름. (세피아 + 그레인 + 스크래치 + 깜빡임)
	/// 야시경. (color = 틴트)
	NightVision,    // 야시경. (color = 틴트)
	/// 색 반전.
	Invert,         // 색 반전.
	/// --- 무늬 / 마스크 ---
	/// CRT 곡면 + 주사선 + RGB 마스크. (parameters = `[곡률, 주사선 밀도]`)
	Scanline,       // CRT 곡면 + 주사선 + RGB 마스크. (parameters = [곡률, 주사선 밀도])
	/// 필름 그레인.
	Grain,          // 필름 그레인.
	/// 빗줄기. (parameters = `[줄 수, 밀도, 속도]`, color = 빗줄기 색)
	Rain,           // 빗줄기. (parameters = [줄 수, 밀도, 속도], color = 빗줄기 색)
	/// 세로 안개. (parameters = `[시작 y, 끝 y]`, color = 안개 색)
	Fog,            // 세로 안개. (parameters = [시작 y, 끝 y], color = 안개 색)
	/// 비네트. (parameters = `[시작 거리, 끝 거리]`, color = 가장자리 색)
	Vignette,       // 비네트. (parameters = [시작 거리, 끝 거리], color = 가장자리 색)
	/// 스포트라이트 바깥 어둡힘. (parameters = `[중심 x, 중심 y, 반지름, 부드러움]`, color = 바깥 색)
	Spotlight,      // 스포트라이트 바깥 어둡힘. (parameters = [중심 x, 중심 y, 반지름, 부드러움], color = 바깥 색)
	/// 집중선. (parameters = `[중심 x, 중심 y, 밀도, 속도]`, color = 선 색)
	SpeedLines,     // 집중선. (parameters = [중심 x, 중심 y, 밀도, 속도], color = 선 색)
	/// 위아래 / 좌우 검은 띠. (parameters = `[세로 띠 비율, 가로 띠 비율]`, color = 띠 색)
	Letterbox,      // 위아래 / 좌우 검은 띠. (parameters = [세로 띠 비율, 가로 띠 비율], color = 띠 색)
	/// --- 전환 (strength 0 → 1 로 color 에 덮인다) ---
	/// 블록 디졸브. (parameters = `[블록 수]`)
	PixelDissolve,  // 블록 디졸브. (parameters = [블록 수])
	/// 방향 와이프. (parameters = `[방향 x, 방향 y, 부드러움]`)
	Wipe,           // 방향 와이프. (parameters = [방향 x, 방향 y, 부드러움])
	/// 원형 아이리스. (parameters = `[중심 x, 중심 y, 부드러움]`)
	IrisWipe,       // 원형 아이리스. (parameters = [중심 x, 중심 y, 부드러움])
	/// 마름모 와이프. (parameters = `[부드러움]`)
	DiamondWipe,    // 마름모 와이프. (parameters = [부드러움])
	/// 시계 와이프. (parameters = `[부드러움]`)
	ClockWipe,      // 시계 와이프. (parameters = [부드러움])
	/// 블라인드. (parameters = `[줄 수, 부드러움]`)
	Blinds,         // 블라인드. (parameters = [줄 수, 부드러움])
	/// 체커 보드. (parameters = `[칸 수]`)
	CheckerWipe,    // 체커 보드. (parameters = [칸 수])
	/// 노이즈 페이드. (parameters = `[노이즈 배율, 부드러움]`)
	NoiseFade,      // 노이즈 페이드. (parameters = [노이즈 배율, 부드러움])
	/// 불에 타듯. (parameters = `[노이즈 배율, 띠 폭]`, color = 덮을 색, 띠는 주황)
	Burn,           // 불에 타듯. (parameters = [노이즈 배율, 띠 폭], color = 덮을 색, 띠는 주황)
	/// 브라운관 끄기. (세로로 눌리며 밝은 선으로)
	TvOff,          // 브라운관 끄기. (세로로 눌리며 밝은 선으로)
	/// 단색으로 페이드. (color = 목표 색)
	Fade,           // 단색으로 페이드. (color = 목표 색)
}

/// 적용 차례. (형태 왜곡 → 흐림 → 글리치 → 색 → 무늬 → 전환)
const EFFECT_ORDER: [ScreenEffectType; 58] = [
	ScreenEffectType::LensDistortion, ScreenEffectType::Bulge, ScreenEffectType::Swirl, ScreenEffectType::Kaleidoscope, ScreenEffectType::Mirror, ScreenEffectType::Ripple, ScreenEffectType::HeatHaze, ScreenEffectType::Jitter, ScreenEffectType::DoubleVision, ScreenEffectType::Pixelate, ScreenEffectType::Shockwave, ScreenEffectType::Wave,
	ScreenEffectType::MotionBlur, ScreenEffectType::ZoomBlur, ScreenEffectType::Blur, ScreenEffectType::TiltShift, ScreenEffectType::GodRays, ScreenEffectType::Bloom, ScreenEffectType::Anamorphic,
	ScreenEffectType::Glitch, ScreenEffectType::Vhs, ScreenEffectType::Chromatic,
	ScreenEffectType::WhiteBalance, ScreenEffectType::LiftGammaGain, ScreenEffectType::ColorGrade, ScreenEffectType::HueShift, ScreenEffectType::SplitToning, ScreenEffectType::Tonemap, ScreenEffectType::GradientMap, ScreenEffectType::Sepia, ScreenEffectType::Grayscale, ScreenEffectType::Posterize, ScreenEffectType::Dither, ScreenEffectType::Halftone, ScreenEffectType::Sharpen, ScreenEffectType::EdgeDetect, ScreenEffectType::OldFilm, ScreenEffectType::NightVision, ScreenEffectType::Invert,
	ScreenEffectType::Scanline, ScreenEffectType::Grain, ScreenEffectType::Rain, ScreenEffectType::Fog, ScreenEffectType::Vignette, ScreenEffectType::Spotlight, ScreenEffectType::SpeedLines, ScreenEffectType::Letterbox,
	ScreenEffectType::PixelDissolve, ScreenEffectType::Wipe, ScreenEffectType::IrisWipe, ScreenEffectType::DiamondWipe, ScreenEffectType::ClockWipe, ScreenEffectType::Blinds, ScreenEffectType::CheckerWipe, ScreenEffectType::NoiseFade, ScreenEffectType::Burn, ScreenEffectType::TvOff, ScreenEffectType::Fade,
];

/// 효과별 원본 이름. (적용 차례)
const EFFECT_NAME_TABLE: [(ScreenEffectType, &str); 58] = [
	(ScreenEffectType::LensDistortion, "lensDistortion"),
	(ScreenEffectType::Bulge, "bulge"),
	(ScreenEffectType::Swirl, "swirl"),
	(ScreenEffectType::Kaleidoscope, "kaleidoscope"),
	(ScreenEffectType::Mirror, "mirror"),
	(ScreenEffectType::Ripple, "ripple"),
	(ScreenEffectType::HeatHaze, "heatHaze"),
	(ScreenEffectType::Jitter, "jitter"),
	(ScreenEffectType::DoubleVision, "doubleVision"),
	(ScreenEffectType::Pixelate, "pixelate"),
	(ScreenEffectType::Shockwave, "shockwave"),
	(ScreenEffectType::Wave, "wave"),
	(ScreenEffectType::MotionBlur, "motionBlur"),
	(ScreenEffectType::ZoomBlur, "zoomBlur"),
	(ScreenEffectType::Blur, "blur"),
	(ScreenEffectType::TiltShift, "tiltShift"),
	(ScreenEffectType::GodRays, "godRays"),
	(ScreenEffectType::Bloom, "bloom"),
	(ScreenEffectType::Anamorphic, "anamorphic"),
	(ScreenEffectType::Glitch, "glitch"),
	(ScreenEffectType::Vhs, "vhs"),
	(ScreenEffectType::Chromatic, "chromatic"),
	(ScreenEffectType::WhiteBalance, "whiteBalance"),
	(ScreenEffectType::LiftGammaGain, "liftGammaGain"),
	(ScreenEffectType::ColorGrade, "colorGrade"),
	(ScreenEffectType::HueShift, "hueShift"),
	(ScreenEffectType::SplitToning, "splitToning"),
	(ScreenEffectType::Tonemap, "tonemap"),
	(ScreenEffectType::GradientMap, "gradientMap"),
	(ScreenEffectType::Sepia, "sepia"),
	(ScreenEffectType::Grayscale, "grayscale"),
	(ScreenEffectType::Posterize, "posterize"),
	(ScreenEffectType::Dither, "dither"),
	(ScreenEffectType::Halftone, "halftone"),
	(ScreenEffectType::Sharpen, "sharpen"),
	(ScreenEffectType::EdgeDetect, "edgeDetect"),
	(ScreenEffectType::OldFilm, "oldFilm"),
	(ScreenEffectType::NightVision, "nightVision"),
	(ScreenEffectType::Invert, "invert"),
	(ScreenEffectType::Scanline, "scanline"),
	(ScreenEffectType::Grain, "grain"),
	(ScreenEffectType::Rain, "rain"),
	(ScreenEffectType::Fog, "fog"),
	(ScreenEffectType::Vignette, "vignette"),
	(ScreenEffectType::Spotlight, "spotlight"),
	(ScreenEffectType::SpeedLines, "speedLines"),
	(ScreenEffectType::Letterbox, "letterbox"),
	(ScreenEffectType::PixelDissolve, "pixelDissolve"),
	(ScreenEffectType::Wipe, "wipe"),
	(ScreenEffectType::IrisWipe, "irisWipe"),
	(ScreenEffectType::DiamondWipe, "diamondWipe"),
	(ScreenEffectType::ClockWipe, "clockWipe"),
	(ScreenEffectType::Blinds, "blinds"),
	(ScreenEffectType::CheckerWipe, "checkerWipe"),
	(ScreenEffectType::NoiseFade, "noiseFade"),
	(ScreenEffectType::Burn, "burn"),
	(ScreenEffectType::TvOff, "tvOff"),
	(ScreenEffectType::Fade, "fade"),
];

/// 효과별 기본 파라미터. (적용 차례)
const DEFAULT_PARAMETER_TABLE: [(ScreenEffectType, [f32; 4]); 58] = [
	(ScreenEffectType::LensDistortion, [0.35, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Bulge, [0.5, 0.5, 0.5, 0.5]),
	(ScreenEffectType::Swirl, [0.5, 0.5, 0.6, 0.5]),
	(ScreenEffectType::Kaleidoscope, [6.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Mirror, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Ripple, [0.5, 0.5, 40.0, 6.0]),
	(ScreenEffectType::HeatHaze, [6.0, 1.5, 0.0, 0.0]),
	(ScreenEffectType::Jitter, [8.0, 24.0, 0.0, 0.0]),
	(ScreenEffectType::DoubleVision, [0.02, 3.0, 0.0, 0.0]),
	(ScreenEffectType::Pixelate, [12.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Shockwave, [0.5, 0.5, 0.0, 0.1]),
	(ScreenEffectType::Wave, [18.0, 4.0, 0.0, 0.0]),
	(ScreenEffectType::MotionBlur, [0.0, 24.0, 0.0, 0.0]),
	(ScreenEffectType::ZoomBlur, [0.5, 0.5, 0.35, 0.0]),
	(ScreenEffectType::Blur, [4.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::TiltShift, [0.5, 0.18, 6.0, 0.0]),
	(ScreenEffectType::GodRays, [0.5, 0.35, 0.6, 0.94]),
	(ScreenEffectType::Bloom, [0.65, 1.2, 4.0, 0.0]),
	(ScreenEffectType::Anamorphic, [0.75, 1.5, 24.0, 0.0]),
	(ScreenEffectType::Glitch, [20.0, 10.0, 0.08, 0.0]),
	(ScreenEffectType::Vhs, [0.01, 0.35, 0.0, 0.0]),
	(ScreenEffectType::Chromatic, [1.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::WhiteBalance, [0.4, 0.0, 0.0, 0.0]),
	(ScreenEffectType::LiftGammaGain, [0.0, 1.0, 1.0, 0.0]),
	(ScreenEffectType::ColorGrade, [1.0, 1.0, 1.0, 0.0]),
	(ScreenEffectType::HueShift, [1.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::SplitToning, [0.1, 0.15, 0.45, 0.5]),
	(ScreenEffectType::Tonemap, [1.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::GradientMap, [0.05, 0.03, 0.2, 0.0]),
	(ScreenEffectType::Sepia, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Grayscale, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Posterize, [5.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Dither, [4.0, 2.0, 0.0, 0.0]),
	(ScreenEffectType::Halftone, [6.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Sharpen, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::EdgeDetect, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::OldFilm, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::NightVision, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Invert, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Scanline, [0.15, 0.5, 0.0, 0.0]),
	(ScreenEffectType::Grain, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Rain, [90.0, 8.0, 1.6, 0.0]),
	(ScreenEffectType::Fog, [0.35, 1.0, 0.0, 0.0]),
	(ScreenEffectType::Vignette, [0.45, 1.1, 0.0, 0.0]),
	(ScreenEffectType::Spotlight, [0.5, 0.5, 0.25, 0.2]),
	(ScreenEffectType::SpeedLines, [0.5, 0.5, 14.0, 6.0]),
	(ScreenEffectType::Letterbox, [0.12, 0.0, 0.0, 0.0]),
	(ScreenEffectType::PixelDissolve, [24.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Wipe, [1.0, 0.0, 0.05, 0.0]),
	(ScreenEffectType::IrisWipe, [0.5, 0.5, 0.05, 0.0]),
	(ScreenEffectType::DiamondWipe, [0.05, 0.0, 0.0, 0.0]),
	(ScreenEffectType::ClockWipe, [0.02, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Blinds, [10.0, 0.05, 0.0, 0.0]),
	(ScreenEffectType::CheckerWipe, [10.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::NoiseFade, [5.0, 0.2, 0.0, 0.0]),
	(ScreenEffectType::Burn, [5.0, 0.06, 0.0, 0.0]),
	(ScreenEffectType::TvOff, [0.0, 0.0, 0.0, 0.0]),
	(ScreenEffectType::Fade, [0.0, 0.0, 0.0, 0.0]),
];

/// 효과별 파라미터 이름. (타임라인 속성 "효과.이름" 으로 접근, 적용 차례)
pub const SCREEN_EFFECT_PARAMETER_NAMES: [(ScreenEffectType, &[&str]); 58] = [
	(ScreenEffectType::LensDistortion, &["intensity"]),
	(ScreenEffectType::Bulge, &["centerX", "centerY", "intensity", "radius"]),
	(ScreenEffectType::Swirl, &["centerX", "centerY", "turns", "radius"]),
	(ScreenEffectType::Kaleidoscope, &["segments"]),
	(ScreenEffectType::Mirror, &["axis"]),
	(ScreenEffectType::Ripple, &["centerX", "centerY", "frequency", "speed"]),
	(ScreenEffectType::HeatHaze, &["scale", "speed"]),
	(ScreenEffectType::Jitter, &["amplitude", "rate"]),
	(ScreenEffectType::DoubleVision, &["offset", "speed"]),
	(ScreenEffectType::Pixelate, &["blockSize"]),
	(ScreenEffectType::Shockwave, &["centerX", "centerY", "radius", "width"]),
	(ScreenEffectType::Wave, &["frequency", "speed"]),
	(ScreenEffectType::MotionBlur, &["angle", "length"]),
	(ScreenEffectType::ZoomBlur, &["centerX", "centerY", "length"]),
	(ScreenEffectType::Blur, &["radius"]),
	(ScreenEffectType::TiltShift, &["focusY", "focusWidth", "radius"]),
	(ScreenEffectType::GodRays, &["centerX", "centerY", "threshold", "decay"]),
	(ScreenEffectType::Bloom, &["threshold", "intensity", "radius"]),
	(ScreenEffectType::Anamorphic, &["threshold", "intensity", "width"]),
	(ScreenEffectType::Glitch, &["rows", "rate", "shift"]),
	(ScreenEffectType::Vhs, &["wobble", "noise"]),
	(ScreenEffectType::Chromatic, &["scale"]),
	(ScreenEffectType::WhiteBalance, &["temperature", "tint"]),
	(ScreenEffectType::LiftGammaGain, &["lift", "gamma", "gain"]),
	(ScreenEffectType::ColorGrade, &["contrast", "saturation", "brightness"]),
	(ScreenEffectType::HueShift, &["turns"]),
	(ScreenEffectType::SplitToning, &["shadowRed", "shadowGreen", "shadowBlue", "balance"]),
	(ScreenEffectType::Tonemap, &["exposure"]),
	(ScreenEffectType::GradientMap, &["darkRed", "darkGreen", "darkBlue"]),
	(ScreenEffectType::Sepia, &[]),
	(ScreenEffectType::Grayscale, &[]),
	(ScreenEffectType::Posterize, &["levels"]),
	(ScreenEffectType::Dither, &["levels", "pixelSize"]),
	(ScreenEffectType::Halftone, &["cellSize"]),
	(ScreenEffectType::Sharpen, &[]),
	(ScreenEffectType::EdgeDetect, &[]),
	(ScreenEffectType::OldFilm, &[]),
	(ScreenEffectType::NightVision, &[]),
	(ScreenEffectType::Invert, &[]),
	(ScreenEffectType::Scanline, &["curvature", "density"]),
	(ScreenEffectType::Grain, &[]),
	(ScreenEffectType::Rain, &["columns", "density", "speed"]),
	(ScreenEffectType::Fog, &["start", "end"]),
	(ScreenEffectType::Vignette, &["inner", "outer"]),
	(ScreenEffectType::Spotlight, &["centerX", "centerY", "radius", "softness"]),
	(ScreenEffectType::SpeedLines, &["centerX", "centerY", "density", "speed"]),
	(ScreenEffectType::Letterbox, &["vertical", "horizontal"]),
	(ScreenEffectType::PixelDissolve, &["blocks"]),
	(ScreenEffectType::Wipe, &["directionX", "directionY", "softness"]),
	(ScreenEffectType::IrisWipe, &["centerX", "centerY", "softness"]),
	(ScreenEffectType::DiamondWipe, &["softness"]),
	(ScreenEffectType::ClockWipe, &["softness"]),
	(ScreenEffectType::Blinds, &["count", "softness"]),
	(ScreenEffectType::CheckerWipe, &["cells"]),
	(ScreenEffectType::NoiseFade, &["scale", "softness"]),
	(ScreenEffectType::Burn, &["scale", "width"]),
	(ScreenEffectType::TvOff, &[]),
	(ScreenEffectType::Fade, &[]),
];

/// 효과별 기본 색. (없는 효과는 흰색)
const DEFAULT_COLOR_TABLE: [(ScreenEffectType, [f32; 4]); 25] = [
	(ScreenEffectType::Shockwave, [1.0, 1.0, 1.0, 0.6]),
	(ScreenEffectType::GodRays, [1.0, 0.95, 0.8, 1.0]),
	(ScreenEffectType::Anamorphic, [0.5, 0.7, 1.0, 1.0]),
	(ScreenEffectType::ColorGrade, [1.0, 1.0, 1.0, 0.0]),
	(ScreenEffectType::SplitToning, [1.0, 0.85, 0.55, 1.0]),
	(ScreenEffectType::GradientMap, [1.0, 0.9, 0.6, 1.0]),
	(ScreenEffectType::EdgeDetect, [1.0, 1.0, 1.0, 1.0]),
	(ScreenEffectType::NightVision, [0.35, 1.0, 0.45, 1.0]),
	(ScreenEffectType::Rain, [0.8, 0.9, 1.0, 0.45]),
	(ScreenEffectType::Fog, [0.75, 0.8, 0.9, 1.0]),
	(ScreenEffectType::Vignette, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::Spotlight, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::SpeedLines, [1.0, 1.0, 1.0, 0.8]),
	(ScreenEffectType::Letterbox, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::PixelDissolve, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::Wipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::IrisWipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::DiamondWipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::ClockWipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::Blinds, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::CheckerWipe, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::NoiseFade, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::Burn, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::TvOff, [0.0, 0.0, 0.0, 1.0]),
	(ScreenEffectType::Fade, [0.0, 0.0, 0.0, 1.0]),
];

/// 프래그먼트 셰이더 공통 머리. (효과 본문은 uv / color 를 다룬다)
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

/// 패스별 프래그먼트 본문. (효과 이름 + 내부 패스 copy / bloomBright / bloomComposite / anamorphicComposite / regionComposite)
/// - uv: 텍스처 좌표(아래가 0), screenUv: 화면 좌표(위가 0), color: 현재 색. sampleScreen(screenUv) 는 화면 좌표로 샘플.
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
	/// 원본 이름 반환. ("vignette" 같은 문자열)
	///
	/// # 돌려줌
	/// 원본 효과 이름. (camelCase, "lensDistortion" 같은)
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		EFFECT_NAME_TABLE[*self as usize].1
	}

	//==============================================================================
	/// 원본 이름으로 찾기. (모르는 이름이면 None)
	///
	/// # 인자
	/// - `name`: 원본 효과 이름. (대소문자를 가린다)
	///
	/// # 돌려줌
	/// 그 이름의 효과. 모르는 이름이면 None.
	///
	/// # 예제
	/// 이름과 효과, 파라미터 이름
	/// ```rust
	/// use vanilla::ScreenEffectType;
	///
	/// assert_eq!(ScreenEffectType::from_name("shockwave"), Some(ScreenEffectType::Shockwave));
	/// assert_eq!(ScreenEffectType::Shockwave.get_name(), "shockwave");
	/// assert_eq!(ScreenEffectType::Shockwave.get_parameter_names(), &["centerX", "centerY", "radius", "width"]);
	/// assert!(ScreenEffectType::Sepia.get_parameter_names().is_empty());
	/// assert_eq!(ScreenEffectType::from_name("unknown"), None);
	/// ```
	//==============================================================================
	pub fn from_name(name: &str) -> Option<ScreenEffectType> {
		EFFECT_NAME_TABLE.iter().find(|(_, effect_name)| *effect_name == name).map(|(effect_type, _)| *effect_type)
	}

	//==============================================================================
	/// 파라미터 이름 목록 반환. (SCREEN_EFFECT_PARAMETER_NAMES)
	///
	/// # 돌려줌
	/// 파라미터 x, y, z, w 차례의 이름 목록. (0 ~ 4 개, 파라미터가 없는 효과는 빈 목록)
	//==============================================================================
	pub fn get_parameter_names(&self) -> &'static [&'static str] {
		SCREEN_EFFECT_PARAMETER_NAMES[*self as usize].1
	}
}

//==============================================================================
/// 효과 하나의 상태.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ScreenEffectState {
	is_enabled: bool,
	strength: f32,
	parameters: [f32; 4],
	color: Color,
}

//==============================================================================
/// 적용 영역. (캔버스 픽셀, 왼쪽 위 원점)
///
/// # 예제
/// 영역을 정하고 읽고 풀기
/// ```rust
/// use vanilla::{ScreenEffect, ScreenEffectRegion};
///
/// let mut screen_effect = ScreenEffect::new();
/// assert_eq!(screen_effect.get_region(), None);
/// screen_effect.set_region(40.0, 30.0, 320.0, 180.0, -5.0);
/// let expected_region = ScreenEffectRegion { x: 40.0, y: 30.0, width: 320.0, height: 180.0, corner_radius: 0.0 };
/// assert_eq!(screen_effect.get_region(), Some(expected_region));
/// screen_effect.clear_region();
/// assert!(screen_effect.get_region().is_none());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenEffectRegion {
	/// 영역 왼쪽. (캔버스 픽셀)
	pub x: f32,
	/// 영역 위. (캔버스 픽셀)
	pub y: f32,
	/// 영역 너비. (픽셀, 캔버스 밖으로 나가는 만큼은 잘린다)
	pub width: f32,
	/// 영역 높이. (픽셀, 캔버스 밖으로 나가는 만큼은 잘린다)
	pub height: f32,
	/// 둥근 모서리 반지름. (픽셀, 0 이면 사각형)
	pub corner_radius: f32,
}

//==============================================================================
/// 렌더 대상. (색 텍스처 + 필요하면 스텐실 렌더버퍼)
//==============================================================================
struct RenderTarget {
	framebuffer: GlFramebuffer,
	texture: GlTexture,
	stencil_renderbuffer: Option<GlRenderbuffer>,
	width: i32,
	height: i32,
}

//==============================================================================
/// 렌더 대상 모음. (씬 사본, 핑퐁, 절반 크기 둘)
//==============================================================================
struct RenderTargetSet {
	scene_target: RenderTarget,
	ping_target: RenderTarget,
	pong_target: RenderTarget,
	half_target_a: RenderTarget,
	half_target_b: RenderTarget,
}

//==============================================================================
/// 패스 단계가 그리는 곳.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassStepTarget {
	PingPong,
	HalfA,
	HalfB,
}

//==============================================================================
/// 패스 단계가 읽는 텍스처.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassStepTexture {
	Previous,
	HalfA,
	HalfB,
	Scene,
}

//==============================================================================
/// 패스 단계. (효과 하나가 여러 단계로 펴지기도 한다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct PassStep {
	pass_name: &'static str,
	strength: f32,
	color: Color,
	parameters: [f32; 4],
	target: PassStepTarget,
	source: PassStepTexture,
	auxiliary: Option<PassStepTexture>,
}

//==============================================================================
/// 패스 단계 만들기. (상태가 없으면 세기 1, 흰색)
//==============================================================================
fn create_pass_step(pass_name: &'static str, effect_state: Option<&ScreenEffectState>, parameters: [f32; 4], target: PassStepTarget, source: PassStepTexture, auxiliary: Option<PassStepTexture>) -> PassStep {
	let (strength, color) = match effect_state {
		Some(effect_state) => {
			(effect_state.strength, effect_state.color)
		}
		None => {
			(1.0, Color::white())
		}
	};
	PassStep {
		pass_name,
		strength,
		color,
		parameters,
		target,
		source,
		auxiliary,
	}
}

//==============================================================================
/// 색 텍스처 생성. (RGBA8, 화면 사본은 RGB8: 기본 프레임버퍼(알파 없음)에서 copyTexSubImage2D 로 떠 오려면 성분이 같아야 한다)
//==============================================================================
fn create_color_texture(gl: &glow::Context, width: i32, height: i32, is_opaque: bool) -> Result<GlTexture, String> {
	unsafe {
		let texture = gl.create_texture()?;
		gl.bind_texture(glow::TEXTURE_2D, Some(texture));
		if is_opaque {
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
/// 렌더 대상 생성. (색 텍스처 + 필요하면 스텐실 렌더버퍼)
//==============================================================================
fn create_render_target(gl: &glow::Context, width: i32, height: i32, use_stencil: bool, is_opaque: bool) -> Result<RenderTarget, String> {
	let texture = create_color_texture(gl, width, height, is_opaque)?;
	unsafe {
		let framebuffer = gl.create_framebuffer()?;
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
		let mut stencil_renderbuffer = None;
		if use_stencil {
			let renderbuffer = gl.create_renderbuffer()?;
			gl.bind_renderbuffer(glow::RENDERBUFFER, Some(renderbuffer));
			gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, width, height);
			gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(renderbuffer));
			stencil_renderbuffer = Some(renderbuffer);
		}
		gl.bind_framebuffer(glow::FRAMEBUFFER, None);
		Ok(RenderTarget {
			framebuffer,
			texture,
			stencil_renderbuffer,
			width,
			height,
		})
	}
}

//==============================================================================
/// 렌더 대상 파괴.
//==============================================================================
fn destroy_render_target(gl: &glow::Context, render_target: &RenderTarget) {
	unsafe {
		gl.delete_framebuffer(render_target.framebuffer);
		gl.delete_texture(render_target.texture);
		if let Some(stencil_renderbuffer) = render_target.stencil_renderbuffer {
			gl.delete_renderbuffer(stencil_renderbuffer);
		}
	}
}

//==============================================================================
/// 렌더 대상 모음 파괴.
//==============================================================================
fn destroy_render_target_set(gl: &glow::Context, render_target_set: &RenderTargetSet) {
	destroy_render_target(gl, &render_target_set.scene_target);
	destroy_render_target(gl, &render_target_set.ping_target);
	destroy_render_target(gl, &render_target_set.pong_target);
	destroy_render_target(gl, &render_target_set.half_target_a);
	destroy_render_target(gl, &render_target_set.half_target_b);
}

//==============================================================================
/// 패스 본문 반환. (모르는 이름이면 None)
//==============================================================================
fn find_effect_body(pass_name: &str) -> Option<&'static str> {
	EFFECT_BODY_TABLE.iter().find(|(body_name, _)| *body_name == pass_name).map(|(_, effect_body)| *effect_body)
}

//==============================================================================
/// 화면 효과.
/// - 효과마다 켜기, 세기, 파라미터 넷, 색을 든다. 씬이 pre_draw 첫머리에서 begin, post_draw 끝에서 end, tick 에서 tick 을 부르면 켜진 효과를 적용 차례로 건다.
///
/// # 예제
/// 비네트를 켜고 세기와 파라미터 정하기
/// ```rust
/// use vanilla::{Color, ScreenEffect, ScreenEffectType};
///
/// let mut screen_effect = ScreenEffect::new();
/// assert!(!screen_effect.has_active_effect());
/// screen_effect.set_enabled(ScreenEffectType::Vignette, true);
/// screen_effect.set_strength(ScreenEffectType::Vignette, 0.8);
/// screen_effect.set_parameter(ScreenEffectType::Vignette, "outer", 0.9);
/// screen_effect.set_color(ScreenEffectType::Vignette, Color::create(0.0, 0.0, 0.2, 1.0));
/// assert!(screen_effect.has_active_effect());
/// assert_eq!(screen_effect.get_parameters(ScreenEffectType::Vignette), [0.45, 0.9, 0.0, 0.0]);
/// screen_effect.set_strength(ScreenEffectType::Vignette, 0.0);
/// assert!(!screen_effect.has_active_effect());
/// ```
///
/// 씬에서 begin, end, tick 부르기
/// ```rust,no_run
/// use vanilla::{Engine, EngineConfiguration, Graphic, Scene, SceneBase, ScreenEffect, ScreenEffectType};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// 	screen_effect: ScreenEffect,
/// }
///
/// impl Scene for MainScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn create(&mut self, _engine: &mut Engine) {
/// 		self.screen_effect.set_enabled(ScreenEffectType::Vignette, true);
/// 	}
///
/// 	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
/// 		self.scene_base.tick(engine, time_delta);
/// 		self.screen_effect.tick(time_delta);
/// 	}
///
/// 	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
/// 		self.screen_effect.begin(graphic);
/// 		self.scene_base.draw_scene_background(engine, graphic);
/// 	}
///
/// 	fn post_draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
/// 		self.screen_effect.end(graphic);
/// 	}
/// }
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let main_scene = MainScene {
/// 	scene_base: SceneBase::new(),
/// 	screen_effect: ScreenEffect::new(),
/// };
/// engine.run(Box::new(main_scene)).unwrap();
/// ```
//==============================================================================
pub struct ScreenEffect {
	pass_table: HashMap<&'static str, Option<FullscreenPass>>,
	effect_state_list: Vec<ScreenEffectState>,
	render_target_set: Option<RenderTargetSet>,
	step_list: Vec<PassStep>,
	time: f32,
	is_bound: bool,
	region: Option<ScreenEffectRegion>,
}

impl ScreenEffect {
	//==============================================================================
	/// 생성. (GL 자원은 처음 end 에서 만든다)
	///
	/// # 돌려줌
	/// 모든 효과가 꺼지고 세기 1, 효과별 기본 파라미터와 색인 화면 효과. (영역 없음, 시간 0)
	//==============================================================================
	pub fn new() -> ScreenEffect {
		let mut screen_effect = ScreenEffect {
			pass_table: HashMap::new(),
			effect_state_list: Vec::with_capacity(EFFECT_ORDER.len()),
			render_target_set: None,
			step_list: Vec::new(),
			time: 0.0,
			is_bound: false,
			region: None,
		};
		screen_effect.reset_all();
		screen_effect
	}

	//==============================================================================
	/// 전부 기본값으로. (끄고, 세기 1, 파라미터 / 색 기본. 모드 전환 때 이전 설정이 남지 않게)
	//==============================================================================
	pub fn reset_all(&mut self) {
		self.effect_state_list.clear();
		for effect_type in EFFECT_ORDER {
			let default_parameters = DEFAULT_PARAMETER_TABLE[effect_type as usize].1;
			let default_color = match DEFAULT_COLOR_TABLE.iter().find(|(color_effect_type, _)| *color_effect_type == effect_type) {
				Some((_, default_color)) => {
					*default_color
				}
				None => {
					[1.0, 1.0, 1.0, 1.0]
				}
			};
			self.effect_state_list.push(ScreenEffectState {
				is_enabled: false,
				strength: 1.0,
				parameters: default_parameters,
				color: Color::create(default_color[0], default_color[1], default_color[2], default_color[3]),
			});
		}
	}

	//==============================================================================
	/// 적용 영역 설정. (캔버스 픽셀, 왼쪽 위 원점. corner_radius 는 둥근 모서리 반지름(픽셀), 0 이면 사각형)
	///
	/// # 인자
	/// - `x`: 영역 왼쪽. (캔버스 픽셀, end 에서 반올림하고 0 아래는 0)
	/// - `y`: 영역 위. (캔버스 픽셀)
	/// - `width`: 영역 너비. (픽셀, 캔버스 밖은 잘린다)
	/// - `height`: 영역 높이. (픽셀, 캔버스 밖은 잘린다)
	/// - `corner_radius`: 둥근 모서리 반지름. (픽셀, 음수는 0, 그 바깥은 원래 화면을 둔다)
	//==============================================================================
	pub fn set_region(&mut self, x: f32, y: f32, width: f32, height: f32, corner_radius: f32) {
		self.region = Some(ScreenEffectRegion {
			x,
			y,
			width,
			height,
			corner_radius: corner_radius.max(0.0),
		});
	}

	//==============================================================================
	/// 적용 영역 해제. (화면 전체)
	//==============================================================================
	pub fn clear_region(&mut self) {
		self.region = None;
	}

	//==============================================================================
	/// 적용 영역 반환. (None 이면 화면 전체)
	///
	/// # 돌려줌
	/// 정한 영역. 화면 전체면 None.
	//==============================================================================
	pub fn get_region(&self) -> Option<ScreenEffectRegion> {
		self.region
	}

	//==============================================================================
	/// 갱신. (애니메이션 효과의 시간)
	///
	/// # 인자
	/// - `time_delta`: 지난 프레임에서 흐른 시간. (초, 효과 시간에 더한다)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		self.time += time_delta;
	}

	//==============================================================================
	/// 시작. (씬 pre_draw 첫머리. 씬은 화면에 그대로 그린다. 켜진 효과가 있을 때만 end 가 일한다)
	///
	/// # 인자
	/// - `_graphic`: 그래픽. (지금은 쓰지 않는다, end 와 짝을 맞춘 인자)
	//==============================================================================
	pub fn begin(&mut self, _graphic: &mut Graphic) {
		self.is_bound = self.has_active_effect();
	}

	//==============================================================================
	/// 효과 적용. (씬 post_draw 끝. 화면의 영역을 떠 와 효과를 걸고 같은 자리에 되돌려 놓는다)
	/// - begin 에서 켜진 효과가 없었으면 아무것도 하지 않는다. 렌더 대상과 패스는 처음 필요할 때 만든다.
	///
	/// # 인자
	/// - `graphic`: 씬을 다 그린 그래픽. (묶음을 내보낸 뒤 그 화면을 떠 온다)
	//==============================================================================
	pub fn end(&mut self, graphic: &mut Graphic) {
		if !self.is_bound {
			return;
		}
		self.is_bound = false;

		// 묶어 그리기로 쌓인 것을 먼저 화면에 그린다. (그 화면을 떠 온다)
		graphic.flush_batch();
		let buffer_width = graphic.get_drawing_buffer_width();
		let buffer_height = graphic.get_drawing_buffer_height();
		if buffer_width <= 0 || buffer_height <= 0 {
			return;
		}

		// 영역 결정. (버퍼 안으로 자른다)
		let mut region_x = 0;
		let mut region_y = 0;
		let mut region_width = buffer_width;
		let mut region_height = buffer_height;
		let mut corner_radius = 0.0;
		if let Some(region) = self.region {
			region_x = (region.x.round() as i32).max(0);
			region_y = (region.y.round() as i32).max(0);
			region_width = (buffer_width - region_x).min(region.width.round() as i32);
			region_height = (buffer_height - region_y).min(region.height.round() as i32);
			corner_radius = region.corner_radius;
		}
		if region_width <= 0 || region_height <= 0 {
			return;
		}
		let width = region_width;
		let height = region_height;
		let gl = graphic.get_gl();
		if let Err(error) = self.ensure_targets(gl, width, height) {
			eprintln!("ScreenEffect: render target failed: {}", error);
			return;
		}

		// 켜진 효과를 패스 단계 목록으로 편다. (블러는 가로 / 세로 2단계, 블룸은 밝기 추출 → 하프 블러 2단계 → 합성)
		self.step_list.clear();
		for effect_type in EFFECT_ORDER {
			let effect_state = self.effect_state_list[effect_type as usize];
			if !effect_state.is_enabled || effect_state.strength <= 0.0 {
				continue;
			}
			let parameters = effect_state.parameters;
			let state = Some(&effect_state);
			match effect_type {
				ScreenEffectType::Blur => {
					self.step_list.push(create_pass_step("blur", state, [parameters[0], 0.0, 1.0, 0.0], PassStepTarget::PingPong, PassStepTexture::Previous, None));
					self.step_list.push(create_pass_step("blur", state, [parameters[0], 0.0, 0.0, 1.0], PassStepTarget::PingPong, PassStepTexture::Previous, None));
				}
				ScreenEffectType::Bloom => {
					self.step_list.push(create_pass_step("bloomBright", state, parameters, PassStepTarget::HalfA, PassStepTexture::Previous, None));
					self.step_list.push(create_pass_step("blur", state, [parameters[2], 0.0, 1.0, 0.0], PassStepTarget::HalfB, PassStepTexture::HalfA, None));
					self.step_list.push(create_pass_step("blur", state, [parameters[2], 0.0, 0.0, 1.0], PassStepTarget::HalfA, PassStepTexture::HalfB, None));
					self.step_list.push(create_pass_step("bloomComposite", state, parameters, PassStepTarget::PingPong, PassStepTexture::Previous, Some(PassStepTexture::HalfA)));
				}
				ScreenEffectType::Anamorphic => {
					self.step_list.push(create_pass_step("bloomBright", state, parameters, PassStepTarget::HalfA, PassStepTexture::Previous, None));
					self.step_list.push(create_pass_step("blur", state, [parameters[2], 0.0, 1.0, 0.0], PassStepTarget::HalfB, PassStepTexture::HalfA, None));
					self.step_list.push(create_pass_step("blur", state, [parameters[2] * 0.5, 0.0, 1.0, 0.0], PassStepTarget::HalfA, PassStepTexture::HalfB, None));
					self.step_list.push(create_pass_step("anamorphicComposite", state, parameters, PassStepTarget::PingPong, PassStepTexture::Previous, Some(PassStepTexture::HalfA)));
				}
				ScreenEffectType::TiltShift => {
					self.step_list.push(create_pass_step("tiltShift", state, [parameters[0], parameters[1], parameters[2], 0.0], PassStepTarget::PingPong, PassStepTexture::Previous, None));
					self.step_list.push(create_pass_step("tiltShift", state, [parameters[0], parameters[1], 0.0, parameters[2]], PassStepTarget::PingPong, PassStepTexture::Previous, None));
				}
				_ => {
					self.step_list.push(create_pass_step(effect_type.get_name(), state, parameters, PassStepTarget::PingPong, PassStepTexture::Previous, None));
				}
			}
		}
		if self.step_list.is_empty() {
			self.step_list.push(create_pass_step("copy", None, [0.0, 0.0, 0.0, 0.0], PassStepTarget::PingPong, PassStepTexture::Previous, None));
		}
		// 둥근 모서리 영역이면 마지막에 원본과 합성한다. (모서리 바깥은 원본 그대로)
		if corner_radius > 0.0 {
			self.step_list.push(create_pass_step("regionComposite", None, [corner_radius, 0.0, 0.0, 0.0], PassStepTarget::PingPong, PassStepTexture::Previous, Some(PassStepTexture::Scene)));
		}

		// 마지막 화면 출력 단계를 찾는다. (하프 대상에 그리는 단계는 화면이 될 수 없다)
		let last_screen_index = self.step_list.iter().rposition(|step| step.target == PassStepTarget::PingPong);

		// 쓰는 패스를 미리 만든다.
		for step_index in 0..self.step_list.len() {
			let pass_name = self.step_list[step_index].pass_name;
			self.ensure_pass(gl, pass_name);
		}

		let render_target_set = match &self.render_target_set {
			Some(render_target_set) => {
				render_target_set
			}
			None => {
				return;
			}
		};

		// 화면 영역 → 씬 대상 복사. (GL 은 아래가 0. 기본 프레임버퍼는 멀티샘플 / 무알파일 수 있어 blitFramebuffer 대신 copyTexSubImage2D 로 RGB8 텍스처에 떠 온다)
		let gl_region_y = buffer_height - (region_y + region_height);
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, Some(render_target_set.scene_target.texture));
			gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, region_x, gl_region_y, width, height);
			gl.disable(glow::BLEND);
			gl.disable(glow::STENCIL_TEST);
		}

		let mut previous_texture = render_target_set.scene_target.texture;
		let mut ping_pong_turn = 0;
		for (step_index, step) in self.step_list.iter().enumerate() {
			let pass = match self.pass_table.get(step.pass_name) {
				Some(Some(pass)) => {
					pass
				}
				_ => {
					continue;
				}
			};
			let is_half_target = step.target != PassStepTarget::PingPong;

			// 대상 바인드.
			let mut target_texture = None;
			unsafe {
				if Some(step_index) == last_screen_index {
					gl.bind_framebuffer(glow::FRAMEBUFFER, None);
					gl.viewport(region_x, gl_region_y, width, height);
					gl.enable(glow::SCISSOR_TEST);
					gl.scissor(region_x, gl_region_y, width, height);
				}
				else if is_half_target {
					let half_target = if step.target == PassStepTarget::HalfA {
						&render_target_set.half_target_a
					}
					else {
						&render_target_set.half_target_b
					};
					gl.bind_framebuffer(glow::FRAMEBUFFER, Some(half_target.framebuffer));
					gl.viewport(0, 0, half_target.width, half_target.height);
					target_texture = Some(half_target.texture);
				}
				else {
					let ping_pong_target = if ping_pong_turn % 2 == 0 {
						&render_target_set.ping_target
					}
					else {
						&render_target_set.pong_target
					};
					ping_pong_turn += 1;
					gl.bind_framebuffer(glow::FRAMEBUFFER, Some(ping_pong_target.framebuffer));
					gl.viewport(0, 0, width, height);
					target_texture = Some(ping_pong_target.texture);
				}
			}

			// 소스 결정. (하프 단계는 지정한 하프 텍스처, 블룸 합성은 마지막 전체 해상도 결과)
			let source_texture = match step.source {
				PassStepTexture::HalfA => {
					render_target_set.half_target_a.texture
				}
				PassStepTexture::HalfB => {
					render_target_set.half_target_b.texture
				}
				_ => {
					previous_texture
				}
			};
			let (pass_width, pass_height) = if is_half_target {
				(render_target_set.half_target_a.width, render_target_set.half_target_a.height)
			}
			else {
				(width, height)
			};

			pass.use_program(gl);
			unsafe {
				gl.active_texture(glow::TEXTURE0);
				gl.bind_texture(glow::TEXTURE_2D, Some(source_texture));
				gl.uniform_1_i32(pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
				if let Some(auxiliary) = step.auxiliary {
					let auxiliary_texture = match auxiliary {
						PassStepTexture::HalfA => {
							render_target_set.half_target_a.texture
						}
						PassStepTexture::Scene => {
							render_target_set.scene_target.texture
						}
						_ => {
							render_target_set.half_target_b.texture
						}
					};
					gl.active_texture(glow::TEXTURE1);
					gl.bind_texture(glow::TEXTURE_2D, Some(auxiliary_texture));
					gl.uniform_1_i32(pass.get_uniform_location(gl, "auxiliaryTexture").as_ref(), 1);
					gl.active_texture(glow::TEXTURE0);
				}
				gl.uniform_2_f32(pass.get_uniform_location(gl, "resolution").as_ref(), pass_width as f32, pass_height as f32);
				gl.uniform_1_f32(pass.get_uniform_location(gl, "time").as_ref(), self.time);
				gl.uniform_1_f32(pass.get_uniform_location(gl, "strength").as_ref(), step.strength);
				gl.uniform_4_f32(pass.get_uniform_location(gl, "parameters").as_ref(), step.parameters[0], step.parameters[1], step.parameters[2], step.parameters[3]);
				gl.uniform_4_f32(pass.get_uniform_location(gl, "effectColor").as_ref(), step.color.red, step.color.green, step.color.blue, step.color.alpha);
			}
			pass.draw(gl);

			// 하프 단계는 전체 해상도 사슬의 "이전 결과" 를 바꾸지 않는다.
			if !is_half_target {
				if let Some(target_texture) = target_texture {
					previous_texture = target_texture;
				}
			}
		}

		unsafe {
			gl.disable(glow::SCISSOR_TEST);
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.bind_vertex_array(None);
		}
		graphic.restore_render_state();
	}

	//==============================================================================
	/// 켜진 효과 여부. (세기 0 은 꺼진 것으로 본다)
	///
	/// # 돌려줌
	/// 켜져 있고 세기가 0 보다 큰 효과가 하나라도 있으면 true.
	//==============================================================================
	pub fn has_active_effect(&self) -> bool {
		self.effect_state_list.iter().any(|effect_state| effect_state.is_enabled && effect_state.strength > 0.0)
	}

	//==============================================================================
	/// 렌더 대상 준비. (크기가 바뀌면 다시 만든다)
	//==============================================================================
	fn ensure_targets(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
		if let Some(render_target_set) = &self.render_target_set {
			if render_target_set.scene_target.width == width && render_target_set.scene_target.height == height {
				return Ok(());
			}
			destroy_render_target_set(gl, render_target_set);
			self.render_target_set = None;
		}
		let half_width = ((width as f32 / 2.0).round() as i32).max(1);
		let half_height = ((height as f32 / 2.0).round() as i32).max(1);
		let render_target_set = RenderTargetSet {
			scene_target: create_render_target(gl, width, height, false, true)?,
			ping_target: create_render_target(gl, width, height, false, false)?,
			pong_target: create_render_target(gl, width, height, false, false)?,
			half_target_a: create_render_target(gl, half_width, half_height, false, false)?,
			half_target_b: create_render_target(gl, half_width, half_height, false, false)?,
		};
		self.render_target_set = Some(render_target_set);
		Ok(())
	}

	//==============================================================================
	/// 패스 준비. (없으면 컴파일해 담는다. 실패도 담아 두고 한 번만 알린다. 원본 resolvePass)
	//==============================================================================
	fn ensure_pass(&mut self, gl: &glow::Context, pass_name: &'static str) {
		if self.pass_table.contains_key(pass_name) {
			return;
		}
		let effect_body = find_effect_body(pass_name).unwrap_or("");
		let fragment_shader_source = format!("{}{}{}", FRAGMENTSHADER_HEADER, effect_body, FRAGMENTSHADER_FOOTER);
		let pass = match FullscreenPass::new(gl, &fragment_shader_source) {
			Ok(pass) => {
				Some(pass)
			}
			Err(error) => {
				eprintln!("ScreenEffect: pass \"{}\" failed: {}", pass_name, error);
				None
			}
		};
		self.pass_table.insert(pass_name, pass);
	}

	//==============================================================================
	/// 효과 켜기 / 끄기.
	///
	/// # 인자
	/// - `effect_type`: 효과.
	/// - `is_enabled`: 참이면 켠다.
	//==============================================================================
	pub fn set_enabled(&mut self, effect_type: ScreenEffectType, is_enabled: bool) {
		self.effect_state_list[effect_type as usize].is_enabled = is_enabled;
	}

	//==============================================================================
	/// 효과 켜짐 여부 반환.
	///
	/// # 인자
	/// - `effect_type`: 효과.
	///
	/// # 돌려줌
	/// 켜져 있으면 true. (세기는 보지 않는다)
	//==============================================================================
	pub fn is_enabled(&self, effect_type: ScreenEffectType) -> bool {
		self.effect_state_list[effect_type as usize].is_enabled
	}

	//==============================================================================
	/// 세기 설정. (0 이면 꺼진 것과 같다)
	///
	/// # 인자
	/// - `effect_type`: 효과.
	/// - `strength`: 세기. (보통 0 ~ 1, 0 이하면 걸지 않는다. 전환 효과는 덮인 정도, 기본 1)
	//==============================================================================
	pub fn set_strength(&mut self, effect_type: ScreenEffectType, strength: f32) {
		self.effect_state_list[effect_type as usize].strength = strength;
	}

	//==============================================================================
	/// 세기 반환.
	///
	/// # 인자
	/// - `effect_type`: 효과.
	///
	/// # 돌려줌
	/// 세기. (기본 1)
	//==============================================================================
	pub fn get_strength(&self, effect_type: ScreenEffectType) -> f32 {
		self.effect_state_list[effect_type as usize].strength
	}

	//==============================================================================
	/// 파라미터 설정. (효과마다 뜻이 다르다. ScreenEffectType 주석 참고, 원본의 y / z / w 기본값 0 은 직접 넘긴다)
	///
	/// # 인자
	/// - `effect_type`: 효과.
	/// - `x`: 첫째 파라미터.
	/// - `y`: 둘째 파라미터. (쓰지 않는 효과는 0)
	/// - `z`: 셋째 파라미터. (쓰지 않는 효과는 0)
	/// - `w`: 넷째 파라미터. (쓰지 않는 효과는 0)
	//==============================================================================
	pub fn set_parameters(&mut self, effect_type: ScreenEffectType, x: f32, y: f32, z: f32, w: f32) {
		self.effect_state_list[effect_type as usize].parameters = [x, y, z, w];
	}

	//==============================================================================
	/// 파라미터 하나 설정. (이름은 SCREEN_EFFECT_PARAMETER_NAMES)
	///
	/// # 인자
	/// - `effect_type`: 효과.
	/// - `parameter_name`: 파라미터 이름. (get_parameter_names 의 것, 모르는 이름이면 아무것도 하지 않는다)
	/// - `value`: 값.
	//==============================================================================
	pub fn set_parameter(&mut self, effect_type: ScreenEffectType, parameter_name: &str, value: f32) {
		let parameter_names = effect_type.get_parameter_names();
		if let Some(parameter_index) = parameter_names.iter().position(|name| *name == parameter_name) {
			self.effect_state_list[effect_type as usize].parameters[parameter_index] = value;
		}
	}

	//==============================================================================
	/// 파라미터 반환.
	///
	/// # 인자
	/// - `effect_type`: 효과.
	///
	/// # 돌려줌
	/// 파라미터 [x, y, z, w]. (reset_all 뒤에는 효과별 기본값)
	//==============================================================================
	pub fn get_parameters(&self, effect_type: ScreenEffectType) -> [f32; 4] {
		self.effect_state_list[effect_type as usize].parameters
	}

	//==============================================================================
	/// 색 설정. (Color 또는 "#rrggbb" 같은 문자열)
	///
	/// # 인자
	/// - `effect_type`: 효과.
	/// - `color`: 효과 색. (뜻은 효과마다 다르다. 가장자리 색, 빛 색, 전환이 덮을 색 같은 것)
	//==============================================================================
	pub fn set_color(&mut self, effect_type: ScreenEffectType, color: impl Into<Color>) {
		self.effect_state_list[effect_type as usize].color = color.into();
	}

	//==============================================================================
	/// 색 반환.
	///
	/// # 인자
	/// - `effect_type`: 효과.
	///
	/// # 돌려줌
	/// 효과 색. (reset_all 뒤에는 효과별 기본 색, 기본 색이 없는 효과는 흰색)
	//==============================================================================
	pub fn get_color(&self, effect_type: ScreenEffectType) -> Color {
		self.effect_state_list[effect_type as usize].color
	}

	//==============================================================================
	/// 전부 끄기.
	//==============================================================================
	pub fn disable_all(&mut self) {
		for effect_state in self.effect_state_list.iter_mut() {
			effect_state.is_enabled = false;
		}
	}

	//==============================================================================
	/// 타임라인 속성 적용. ("vignette" = 세기(0 이면 끔), "shockwave.radius" = 파라미터, "fade.color" = 색, "fade.enabled" = 켜기)
	/// - 값은 원본처럼 숫자, 문자열, 참거짓(TimelineValue)이다. 숫자는 원본 Number() 처럼 읽고(참거짓은 1 / 0, 빈 문자열은 0, 못 읽으면 NaN),
	///   색은 "#rrggbb" 문자열일 때만 바꾸고, 켜기는 true 또는 "true" 다.
	///
	/// # 인자
	/// - `property_name`: "효과" 또는 "효과.멤버". (효과는 원본 이름, 멤버는 color, enabled, 파라미터 이름. 모르는 효과나 파라미터 이름은 무시한다)
	/// - `value`: 걸 값.
	///
	/// # 예제
	/// 타임라인 속성 이름으로 세기, 파라미터, 색, 켜기 걸기
	/// ```rust
	/// use vanilla::{Color, ScreenEffect, ScreenEffectType, TimelineValue};
	///
	/// let mut screen_effect = ScreenEffect::new();
	/// screen_effect.set_timeline_property("vignette", &TimelineValue::Number(0.5));
	/// assert!(screen_effect.is_enabled(ScreenEffectType::Vignette));
	/// assert_eq!(screen_effect.get_strength(ScreenEffectType::Vignette), 0.5);
	/// screen_effect.set_timeline_property("shockwave.radius", &TimelineValue::Number(0.75));
	/// assert_eq!(screen_effect.get_parameters(ScreenEffectType::Shockwave)[2], 0.75);
	/// screen_effect.set_timeline_property("fade.color", &TimelineValue::String(String::from("#ff0000")));
	/// screen_effect.set_timeline_property("fade.enabled", &TimelineValue::Boolean(true));
	/// assert_eq!(screen_effect.get_color(ScreenEffectType::Fade), Color::create(1.0, 0.0, 0.0, 1.0));
	/// assert!(screen_effect.is_enabled(ScreenEffectType::Fade));
	/// ```
	//==============================================================================
	pub fn set_timeline_property(&mut self, property_name: &str, value: &TimelineValue) {
		let (effect_name, member_name) = match property_name.find('.') {
			Some(separator_index) => {
				(&property_name[..separator_index], &property_name[separator_index + 1..])
			}
			None => {
				(property_name, "")
			}
		};
		let effect_type = match ScreenEffectType::from_name(effect_name) {
			Some(effect_type) => {
				effect_type
			}
			None => {
				return;
			}
		};
		if member_name.is_empty() {
			let strength = value.as_number();
			let effect_state = &mut self.effect_state_list[effect_type as usize];
			effect_state.strength = strength;
			effect_state.is_enabled = strength > 0.0;
			return;
		}
		if member_name == "color" {
			if let TimelineValue::String(color_text) = value {
				self.effect_state_list[effect_type as usize].color = Color::create_from_hex(color_text);
			}
			return;
		}
		if member_name == "enabled" {
			self.effect_state_list[effect_type as usize].is_enabled = value.is_true();
			return;
		}
		self.set_parameter(effect_type, member_name, value.as_number());
	}

	//==============================================================================
	/// 시간 반환.
	///
	/// # 돌려줌
	/// 효과 시간. (초, tick 으로 쌓인다)
	//==============================================================================
	pub fn get_time(&self) -> f32 {
		self.time
	}

	//==============================================================================
	/// 씬 텍스처 반환. (end 이후, 마지막 프레임에 떠 온 영역 원본)
	///
	/// # 돌려줌
	/// 떠 온 영역의 원본 텍스처. end 가 렌더 대상을 만들기 전이거나 destroy 뒤면 None.
	//==============================================================================
	pub fn get_scene_texture(&self) -> Option<GlTexture> {
		self.render_target_set.as_ref().map(|render_target_set| render_target_set.scene_target.texture)
	}

	//==============================================================================
	/// 적용 차례 반환. (정적)
	///
	/// # 돌려줌
	/// 적용 차례대로의 효과 58 개. (열거 선언 차례와 같다)
	//==============================================================================
	pub fn get_effect_order() -> Vec<ScreenEffectType> {
		EFFECT_ORDER.to_vec()
	}

	//==============================================================================
	/// 파괴. (GL 자원 해제: 렌더 대상과 패스 프로그램)
	/// - 효과 설정은 그대로 두고, 다시 end 하면 자원을 새로 만든다.
	///
	/// # 인자
	/// - `graphic`: GL 자원을 만든 그래픽.
	//==============================================================================
	pub fn destroy(&mut self, graphic: &mut Graphic) {
		let gl = graphic.get_gl();
		if let Some(render_target_set) = self.render_target_set.take() {
			destroy_render_target_set(gl, &render_target_set);
		}
		for pass in self.pass_table.values().flatten() {
			pass.destroy(gl);
		}
		self.pass_table.clear();
	}
}

impl Default for ScreenEffect {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> ScreenEffect {
		ScreenEffect::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 표들이 적용 차례(열거형 선언 차례)와 맞는지. (get_name, 기본값 표는 효과 번호로 바로 찾는다)
	//==============================================================================
	#[test]
	fn tables_follow_effect_order() {
		for (effect_index, effect_type) in EFFECT_ORDER.iter().enumerate() {
			assert_eq!(*effect_type as usize, effect_index);
			assert_eq!(EFFECT_NAME_TABLE[effect_index].0, *effect_type);
			assert_eq!(DEFAULT_PARAMETER_TABLE[effect_index].0, *effect_type);
			assert_eq!(SCREEN_EFFECT_PARAMETER_NAMES[effect_index].0, *effect_type);
			assert_eq!(ScreenEffectType::from_name(effect_type.get_name()), Some(*effect_type));
		}
	}

	//==============================================================================
	/// 단계로 쓰이는 패스는 모두 본문이 있다. (bloom, anamorphic 은 내부 패스로 펴진다)
	//==============================================================================
	#[test]
	fn every_pass_has_body() {
		for effect_type in EFFECT_ORDER.iter() {
			if *effect_type == ScreenEffectType::Bloom || *effect_type == ScreenEffectType::Anamorphic {
				continue;
			}
			assert!(find_effect_body(effect_type.get_name()).is_some(), "{:?}", effect_type);
		}
		for pass_name in ["copy", "bloomBright", "bloomComposite", "anamorphicComposite", "regionComposite"] {
			assert!(find_effect_body(pass_name).is_some(), "{}", pass_name);
		}
	}

	//==============================================================================
	/// 타임라인 속성: 세기(참거짓은 1 과 0), 파라미터 이름, 색, 켜기.
	//==============================================================================
	#[test]
	fn timeline_property_updates_state() {
		let mut screen_effect = ScreenEffect::new();
		assert!(!screen_effect.has_active_effect());
		screen_effect.set_timeline_property("vignette", &TimelineValue::Number(0.5));
		assert!(screen_effect.is_enabled(ScreenEffectType::Vignette));
		assert_eq!(screen_effect.get_strength(ScreenEffectType::Vignette), 0.5);
		assert!(screen_effect.has_active_effect());
		screen_effect.set_timeline_property("vignette", &TimelineValue::String(String::from("0")));
		assert!(!screen_effect.is_enabled(ScreenEffectType::Vignette));
		screen_effect.set_timeline_property("vignette", &TimelineValue::Boolean(true));
		assert_eq!(screen_effect.get_strength(ScreenEffectType::Vignette), 1.0);
		screen_effect.set_timeline_property("vignette", &TimelineValue::Boolean(false));
		assert!(!screen_effect.is_enabled(ScreenEffectType::Vignette));
		screen_effect.set_timeline_property("shockwave.radius", &TimelineValue::Number(0.75));
		assert_eq!(screen_effect.get_parameters(ScreenEffectType::Shockwave), [0.5, 0.5, 0.75, 0.1]);
		screen_effect.set_timeline_property("fade.color", &TimelineValue::String(String::from("#ff0000")));
		screen_effect.set_timeline_property("fade.color", &TimelineValue::Number(1.0));
		assert_eq!(screen_effect.get_color(ScreenEffectType::Fade), Color::create(1.0, 0.0, 0.0, 1.0));
		screen_effect.set_timeline_property("fade.enabled", &TimelineValue::Boolean(true));
		assert!(screen_effect.is_enabled(ScreenEffectType::Fade));
		screen_effect.set_timeline_property("fade.enabled", &TimelineValue::Boolean(false));
		assert!(!screen_effect.is_enabled(ScreenEffectType::Fade));
		screen_effect.set_timeline_property("fade.enabled", &TimelineValue::String(String::from("true")));
		assert!(screen_effect.is_enabled(ScreenEffectType::Fade));
		screen_effect.set_timeline_property("vignette", &TimelineValue::Boolean(true));
		assert_eq!(screen_effect.get_strength(ScreenEffectType::Vignette), 1.0);
		screen_effect.reset_all();
		assert!(!screen_effect.is_enabled(ScreenEffectType::Fade));
		assert_eq!(screen_effect.get_color(ScreenEffectType::Fade), Color::create(0.0, 0.0, 0.0, 1.0));
	}
}

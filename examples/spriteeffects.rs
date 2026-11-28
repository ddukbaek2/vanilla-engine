//==============================================================================
// 스프라이트 이펙트 예제. (원본 examples/spriteeffects 와 같은 장면)
// - 왼쪽 목록에서 고르면 오른쪽 둥근 무대에서 무한 반복된다. 분류는 셋.
//   SPRITE SHADER: ShaderSpriteEffect 마다 하나씩(공용 피사체 여섯에 같은 효과를 건다), SCREEN EFFECT: ScreenEffect 적용 차례 표대로 하나씩(무대 사각형에만 건다),
//   STAGING: 엔진 기능 조합 연출 스물다섯(히트 스톱, 화면 흔들림, 슬로 모션, 잔상, 플립북, 떠오르는 숫자, 타자기 대사, 칼 궤적, 시차 배경, 카메라 펀치, 스쿼시 앤 스트레치, 등장과 퇴장,
//   체력 막대, 번개, 레벨 업, 순간 이동, 정지 화면, 포털, 물속, 상태 이상, 승리, 게임 오버, 낮과 밤, 피격 섬광, 폭풍우).
// - 공용 세계: 바탕, 바닥, 주인공 슬라임, 피사체(문장, 별, 금화, 보석, 박쥐), 연출 뿌리, 떠오르는 글자. 오른쪽 위에 fps.
// - 그림은 examples/assets/spriteeffects 의 PNG 일곱 장이다. (원본 저장소 examples/spriteeffects/assets 와 같은 파일)
// - 주인공 슬라임의 시트 애니메이션(대기 열 때, 점프 일곱 때)은 예제가 직접 돌린다. (SpriteAnimator 는 Sprite 컴포넌트만 찾고 ShaderSprite 안의 Sprite 는 못 찾는다)
// - 실행: cargo run --release --example spriteeffects
//   처음 모드 고르기: VANILLA_EXAMPLE_MODE=staging.hitStop cargo run --release --example spriteeffects
//   화면 저장: VANILLA_CAPTURE_PATH=shots/spriteeffects.png VANILLA_CAPTURE_FRAMES=90 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use serde_json::{json, Value};
use vanilla::core::graphic::{BlendMode, TextAlign, TextBaseline};
use vanilla::{
	Color, ComponentContext, Engine, EngineConfiguration, FloatingText, FloatingTextOptions, Graphic, Image, ImageAsset, Mask, NodeId, NodeTree, Paint, ParticleSystem, Pivot, Rect, Scene, SceneBase, ScreenEffect, ScreenEffectType, SeededRandom, Shaker, ShakerOptions,
	ShaderSprite, ShaderSpriteEffect, Text, TouchRecognizer, TrailRenderer, Typewriter, TypewriterOptions, UIButton, UILabel, UIScrollBar, UIScrollView, Vector2, ViewScaleMode,
};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const REFERENCE_WIDTH: f64 = 1280.0;
const REFERENCE_HEIGHT: f64 = 760.0;
const LIST_X: f64 = 24.0;
const LIST_Y: f64 = 78.0;
const LIST_WIDTH: f64 = 248.0;
const LIST_HEIGHT: f64 = 656.0;
const LIST_ROW_HEIGHT: f64 = 36.0;
const LIST_HEADER_HEIGHT: f64 = 34.0;
const STAGE_X: f64 = 288.0;
const STAGE_Y: f64 = 78.0;
const STAGE_WIDTH: f64 = 968.0;
const STAGE_HEIGHT: f64 = 656.0;
const FLOOR_Y: f64 = 440.0;
const ASSET_DIRECTORY: &str = "examples/assets/spriteeffects";
const IMAGE_NAME_LIST: [&str; 7] = ["emblem.png", "slime.png", "star.png", "coin.png", "bat.png", "gem.png", "cover.png"];
const EFFECT_RANDOM_SEED: u32 = 20260907;
const SUBJECT_EMBLEM: usize = 0;
const SUBJECT_STAR: usize = 1;
const SUBJECT_COIN: usize = 2;
const SUBJECT_GEM: usize = 3;
const SUBJECT_BAT: usize = 4;
const SPRITE_TRANSITION_NAMES: [&str; 11] = ["dissolve", "burn", "noiseFade", "pixelDissolve", "wipe", "iris", "diamondWipe", "clockWipe", "blinds", "checkerWipe", "pixelate"];
const SPRITE_OSCILLATE_NAMES: [&str; 27] = [
	"blur", "motionBlur", "radialBlur", "chromatic", "glitch", "jitter", "wave", "flag", "ripple", "heatHaze", "bulge", "swirl", "colorAdjust", "posterize", "threshold", "invert", "sepia", "grayscale", "silhouette", "colorize", "gradientMap", "vignette", "oldFilm", "scanlines", "emboss", "sharpen", "edgeDetect",
];
const SCREEN_TRANSITION_NAMES: [&str; 11] = ["pixelDissolve", "wipe", "irisWipe", "diamondWipe", "clockWipe", "blinds", "checkerWipe", "noiseFade", "burn", "tvOff", "fade"];
const SCREEN_OSCILLATE_NAMES: [&str; 18] = ["zoomBlur", "motionBlur", "blur", "tiltShift", "chromatic", "pixelate", "doubleVision", "jitter", "glitch", "vhs", "wave", "ripple", "heatHaze", "lensDistortion", "bulge", "swirl", "invert", "hueShift"];

fn text_color() -> Color {
	Color::create(0.91, 0.922, 0.949, 1.0)
}

fn text_faint_color() -> Color {
	Color::create(0.42, 0.451, 0.529, 1.0)
}

fn on_accent_color() -> Color {
	Color::create(0.165, 0.126, 0.086, 1.0)
}

fn accent_color() -> Color {
	Color::create(0.831, 0.69, 0.416, 1.0)
}

fn transparent_color() -> Color {
	Color::create(0.0, 0.0, 0.0, 0.0)
}

fn color(red: f64, green: f64, blue: f64, alpha: f64) -> Color {
	Color::create(red, green, blue, alpha)
}

//==============================================================================
// 이름 꾸미기. ("pixelDissolve" 는 "PIXEL DISSOLVE")
//==============================================================================
fn humanize(name: &str) -> String {
	let mut humanized = String::new();
	for character in name.chars() {
		if character.is_uppercase() {
			humanized.push(' ');
		}
		humanized.push(character.to_ascii_uppercase());
	}
	humanized
}

//==============================================================================
// 왕복 진행도. (0 에서 1 로 갔다 0 으로, 끝에서 잠깐 머문다)
//==============================================================================
fn ping_pong(elapsed: f64, period: f64) -> f64 {
	let phase = (elapsed % period) / period;
	let raw = if phase < 0.5 { phase * 2.0 } else { 2.0 - phase * 2.0 };
	(raw * 1.3 - 0.15).clamp(0.0, 1.0)
}

fn sprite_effect_tag(effect_name: &str) -> &'static str {
	match effect_name {
		"dissolve" => "노이즈 문턱 + 불꽃 가장자리",
		"burn" => "속 불씨 + 겉 그을음",
		"flash" => "피격 섬광",
		"outline" => "실루엣 바깥 선",
		"innerOutline" => "실루엣 안쪽 선",
		"glow" => "바깥 발광",
		"innerGlow" => "안쪽 발광",
		"shadow" => "드롭 섀도",
		"hologram" => "주사선 + 틴트 + 지터",
		"shine" => "광택 띠 지나감",
		"silhouette" => "그림자 / 잔상",
		"wipe" => "방향 와이프",
		"iris" => "원형 아이리스",
		"chromatic" => "RGB 분리",
		"hueShift" => "색상 360도 회전",
		"pixelate" => "모자이크",
		"glitch" => "줄 어긋남 + 색 분리",
		"wave" => "가로 물결",
		"flag" => "깃발 펄럭임",
		"ripple" => "중심 파문",
		"heatHaze" => "아지랑이",
		"bulge" => "볼록 / 오목",
		"swirl" => "소용돌이",
		"mirror" => "대칭",
		"kaleidoscope" => "만화경",
		"jitter" => "떨림",
		"blur" => "가우시안",
		"motionBlur" => "방향 블러",
		"radialBlur" => "방사 블러",
		"colorAdjust" => "대비 / 채도 / 밝기",
		"posterize" => "포스터화",
		"threshold" => "흑백 문턱",
		"gradientMap" => "2색 매핑",
		"edgeDetect" => "소벨 윤곽",
		"emboss" => "엠보스",
		"sharpen" => "샤픈",
		"scanlines" => "주사선",
		"oldFilm" => "낡은 필름",
		"vignette" => "비네트",
		"noiseFade" => "노이즈 알파",
		"pixelDissolve" => "블록 디졸브",
		"diamondWipe" => "마름모",
		"clockWipe" => "시계 방향",
		"blinds" => "블라인드",
		"checkerWipe" => "체커 보드",
		"sepia" => "세피아",
		"grayscale" => "무채색",
		"invert" => "반전",
		"colorize" => "곱셈 틴트",
		_ => "ShaderSprite",
	}
}

fn screen_effect_tag(effect_name: &str) -> &'static str {
	match effect_name {
		"lensDistortion" => "배럴 / 핀쿠션",
		"bulge" => "볼록 렌즈",
		"swirl" => "소용돌이",
		"kaleidoscope" => "만화경",
		"mirror" => "대칭",
		"ripple" => "물 파문",
		"heatHaze" => "아지랑이",
		"jitter" => "화면 떨림",
		"doubleVision" => "겹쳐 보임",
		"pixelate" => "모자이크",
		"shockwave" => "충격파 링",
		"wave" => "물결",
		"motionBlur" => "방향 블러",
		"zoomBlur" => "방사 블러",
		"blur" => "가우시안",
		"tiltShift" => "틸트 시프트",
		"godRays" => "빛줄기",
		"bloom" => "밝은 곳 번짐",
		"anamorphic" => "가로 플레어",
		"glitch" => "글리치",
		"vhs" => "VHS 테이프",
		"chromatic" => "색수차",
		"whiteBalance" => "색온도",
		"liftGammaGain" => "리프트 / 감마 / 게인",
		"colorGrade" => "대비 / 채도 / 틴트",
		"hueShift" => "색상 회전",
		"splitToning" => "스플릿 토닝",
		"tonemap" => "ACES",
		"gradientMap" => "2색 매핑",
		"sepia" => "세피아",
		"grayscale" => "무채색",
		"posterize" => "포스터화",
		"dither" => "베이어 디더",
		"halftone" => "하프톤 점",
		"sharpen" => "샤픈",
		"edgeDetect" => "윤곽",
		"oldFilm" => "낡은 필름",
		"nightVision" => "야시경",
		"invert" => "반전",
		"scanline" => "CRT",
		"grain" => "그레인",
		"rain" => "빗줄기",
		"fog" => "안개",
		"vignette" => "비네트",
		"spotlight" => "스포트라이트 — 주인공 추적",
		"speedLines" => "집중선",
		"letterbox" => "레터박스",
		"pixelDissolve" => "블록 전환",
		"wipe" => "방향 와이프",
		"irisWipe" => "아이리스",
		"diamondWipe" => "마름모",
		"clockWipe" => "시계",
		"blinds" => "블라인드",
		"checkerWipe" => "체커",
		"noiseFade" => "노이즈 페이드",
		"burn" => "불에 타듯",
		"tvOff" => "브라운관 끄기",
		"fade" => "페이드",
		_ => "ScreenEffect",
	}
}

//==============================================================================
// 연출 모드 종류. (엔진 기능 조합)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum Staging {
	HitStop,
	ScreenShake,
	SlowMotion,
	Afterimage,
	Flipbook,
	FloatingDamage,
	TypewriterDialogue,
	SlashTrail,
	Parallax,
	CameraPunch,
	SquashStretch,
	SpawnDespawn,
	HealthBar,
	Lightning,
	LevelUp,
	Teleport,
	FreezeFrame,
	Portal,
	Underwater,
	StatusEffects,
	Victory,
	GameOver,
	DayNight,
	DamageFlash,
	RainStorm,
}

const STAGING_TABLE: [(Staging, &str, &str, &str); 25] = [
	(Staging::HitStop, "staging.hitStop", "HIT STOP", "타임 스케일 0 + 섬광 + 흔들림 + 데미지 숫자"),
	(Staging::ScreenShake, "staging.screenShake", "SCREEN SHAKE", "Shaker, 사인 합성 + 지수 감쇠"),
	(Staging::SlowMotion, "staging.slowMotion", "SLOW MOTION", "타임 스케일 0.2 + 채도 / 집중선 (불릿 타임)"),
	(Staging::Afterimage, "staging.afterimage", "AFTERIMAGE", "실루엣 잔상, silhouette 효과 사본이 남는다"),
	(Staging::Flipbook, "staging.flipbook", "FLIPBOOK", "SpriteAnimator 클립 전환 + 완료 콜백"),
	(Staging::FloatingDamage, "staging.floatingDamage", "FLOATING DAMAGE", "FloatingText, 재사용 풀, 치명타는 크고 금색"),
	(Staging::TypewriterDialogue, "staging.typewriter", "TYPEWRITER DIALOGUE", "Typewriter, 초당 글자 수 + 스킵 + 완료 이벤트"),
	(Staging::SlashTrail, "staging.slashTrail", "SLASH TRAIL", "TrailRenderer 호 궤적 + 스파크 + 피격 섬광"),
	(Staging::Parallax, "staging.parallax", "PARALLAX", "층별 다른 속도 + 카메라 살짝 줌"),
	(Staging::CameraPunch, "staging.cameraPunch", "CAMERA PUNCH", "타격 순간 줌 인 후 되돌림 + 흔들림 + 색수차"),
	(Staging::SquashStretch, "staging.squashStretch", "SQUASH & STRETCH", "점프, 도약 늘림 / 착지 눌림"),
	(Staging::SpawnDespawn, "staging.spawnDespawn", "SPAWN / DESPAWN", "pixelDissolve 등장, back.out 팝, dissolve 퇴장"),
	(Staging::HealthBar, "staging.healthBar", "HEALTH BAR", "즉시 줄어드는 막대 + 뒤따르는 잔상 막대"),
	(Staging::Lightning, "staging.lightning", "LIGHTNING", "흰 섬광 + 반전 깜빡임 + 흔들림 + 천둥 흔들림"),
	(Staging::LevelUp, "staging.levelUp", "LEVEL UP", "확장 링 + 파티클 분수 + 색상 회전 발광 + 글자 팝"),
	(Staging::Teleport, "staging.teleport", "TELEPORT", "pixelDissolve 아웃, 순간 이동, 디졸브 인 + 색수차 팝"),
	(Staging::FreezeFrame, "staging.freezeFrame", "FREEZE FRAME", "레터박스 + 타임 스케일 0 + 세피아 + 자막"),
	(Staging::Portal, "staging.portal", "PORTAL", "화면 소용돌이 + 원형 빛 + 주인공 빨려 들어감"),
	(Staging::Underwater, "staging.underwater", "UNDERWATER", "물결 + 푸른 컬러 그레이드 + 거품 + 빛줄기"),
	(Staging::StatusEffects, "staging.statusEffects", "STATUS EFFECTS", "독 / 화염 / 빙결 / 유령 / 무적, 주인공 셰이더 순환"),
	(Staging::Victory, "staging.victory", "VICTORY", "컨페티 + 집중선 + 문장 광택 + 줌 + 글자"),
	(Staging::GameOver, "staging.gameOver", "GAME OVER", "무채색 페이드 + 레터박스 + 브라운관 끄기"),
	(Staging::DayNight, "staging.dayNight", "DAY / NIGHT", "색온도 + 리프트 / 게인 + 별 파티클 + 안개"),
	(Staging::DamageFlash, "staging.damageFlash", "DAMAGE FLASH + KNOCKBACK", "붉은 섬광 + 밀려남 + 무적 깜빡임"),
	(Staging::RainStorm, "staging.rainStorm", "RAIN STORM", "빗줄기 화면 효과 + 빗방울 파티클 + 물웅덩이 파문"),
];

//==============================================================================
// 모드 종류와 정의.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum ModeKind {
	SpriteShader(ShaderSpriteEffect),
	Screen(ScreenEffectType),
	Staging(Staging),
}

struct ModeDefinition {
	key: String,
	category: &'static str,
	title: String,
	#[allow(dead_code)]
	tag: String,
	kind: ModeKind,
}

//==============================================================================
// 모드 정의 목록 짓기. (스프라이트 셰이더, 화면 효과, 연출 차례)
//==============================================================================
fn compose_mode_list() -> Vec<ModeDefinition> {
	let mut mode_list: Vec<ModeDefinition> = Vec::new();
	for effect in ShaderSpriteEffect::get_effect_list().iter() {
		if *effect == ShaderSpriteEffect::None {
			continue;
		}
		let effect_name = effect.get_name();
		mode_list.push(ModeDefinition { key: format!("sprite.{}", effect_name), category: "SPRITE SHADER", title: humanize(effect_name), tag: String::from(sprite_effect_tag(effect_name)), kind: ModeKind::SpriteShader(*effect) });
	}
	for effect_type in ScreenEffect::get_effect_order() {
		let effect_name = effect_type.get_name();
		mode_list.push(ModeDefinition { key: format!("screen.{}", effect_name), category: "SCREEN EFFECT", title: humanize(effect_name), tag: String::from(screen_effect_tag(effect_name)), kind: ModeKind::Screen(effect_type) });
	}
	for (staging, key, title, tag) in STAGING_TABLE.iter() {
		mode_list.push(ModeDefinition { key: String::from(*key), category: "STAGING", title: String::from(*title), tag: String::from(*tag), kind: ModeKind::Staging(*staging) });
	}
	mode_list
}

//==============================================================================
// 연출 모드가 쓰는 상태. (모드를 고를 때마다 새로 만든다)
//==============================================================================
#[derive(Default)]
struct ModeState {
	next_event: f64,
	freeze_remaining: f64,
	flash: f64,
	phase: f64,
	timer: f64,
	accumulator: f64,
	blend: f64,
	punch: f64,
	side: i32,
	index: i32,
	ghost_list: Vec<(NodeId, f64)>,
	ring_list: Vec<(NodeId, f64)>,
	enemy_list: Vec<(NodeId, f64)>,
	layer_list: Vec<(NodeId, f64, f64)>,
	jump_time: f64,
	did_hit: bool,
	swing_start: f64,
	velocity: f64,
	y: f64,
	scale_x: f64,
	scale_y: f64,
	health: f64,
	ghost_health: f64,
	ghost_delay: f64,
	invincible: f64,
	node_a: Option<NodeId>,
	node_b: Option<NodeId>,
	node_c: Option<NodeId>,
	particle_node: Option<NodeId>,
	line_index: usize,
	typewriter: Option<Typewriter>,
}

//==============================================================================
// 목록 한 줄이 쥐는 노드.
//==============================================================================
struct ListRow {
	row_node_id: NodeId,
	paint_node_id: NodeId,
	title_node_id: NodeId,
	y: f64,
}

//==============================================================================
// 노드 만들기 도우미.
//==============================================================================
fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, x: f64, y: f64, width: f64, height: f64) -> NodeId {
	let node_id = node_tree.create_child(parent_id, name);
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_content_size(Vector2::create(width, height));
	node.set_local_position(Vector2::create(x, y));
	node_id
}

fn set_label_text(node_tree: &mut NodeTree, label_id: NodeId, text: &str) {
	if let Some(label) = node_tree[label_id].get_component_mut::<UILabel>() {
		label.set_text(text);
	}
}

fn set_label_color(node_tree: &mut NodeTree, label_id: NodeId, label_color: Color) {
	if let Some(label) = node_tree[label_id].get_component_mut::<UILabel>() {
		label.set_text_color(label_color);
	}
}

//==============================================================================
// 글자 노드. (원본 makeLabel: 가운데 정렬이면 가운데 기준, 아니면 왼쪽 가운데 기준, 세로는 가운데)
//==============================================================================
fn make_label(node_tree: &mut NodeTree, parent_id: NodeId, x: f64, y: f64, width: f64, height: f64, font_size: f64, label_color: Color, is_bold: bool, text_align: TextAlign) -> NodeId {
	let label_id = node_tree.create_child(parent_id, "label");
	let node = &mut node_tree[label_id];
	node.set_pivot(if text_align == TextAlign::Center { Pivot::MIDDLE_CENTER } else { Pivot::MIDDLE_LEFT });
	node.set_content_size(Vector2::create(width, height));
	node.set_local_position(Vector2::create(x, y));
	let label = node_tree.add_component(label_id, UILabel::new()).expect("label");
	label.set_font_size(font_size);
	label.set_text_color(label_color);
	label.set_text_align(text_align);
	label.set_text_baseline(TextBaseline::Middle);
	if is_bold {
		label.set_bold(true);
	}
	label_id
}

//==============================================================================
// 둥근 색 칠 노드. (가운데 기준)
//==============================================================================
fn make_paint(node_tree: &mut NodeTree, parent_id: NodeId, x: f64, y: f64, width: f64, height: f64, paint_color: Color, round_size: f64) -> NodeId {
	let node_id = node_tree.create_child(parent_id, "paint");
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::MIDDLE_CENTER);
	node.set_content_size(Vector2::create(width, height));
	node.set_local_position(Vector2::create(x, y));
	let paint = node.add_component(Paint::create_with_color(paint_color));
	paint.set_round_size(round_size);
	node_id
}

//==============================================================================
// ShaderSprite 꺼내기.
//==============================================================================
fn shader_sprite(node_tree: &mut NodeTree, node_id: NodeId) -> &mut ShaderSprite {
	node_tree[node_id].get_component_mut::<ShaderSprite>().expect("shader sprite")
}

//==============================================================================
// 스프라이트 노드. (가운데 기준, frame_columns 가 1 보다 크면 첫 칸만 보인다)
//==============================================================================
fn make_sprite(node_tree: &mut NodeTree, image_table: &HashMap<String, Rc<Image>>, parent_id: NodeId, image_name: &str, x: f64, y: f64, width: f64, height: f64, frame_columns: u32) -> NodeId {
	let node_id = node_tree.create_child(parent_id, image_name);
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::MIDDLE_CENTER);
	node.set_content_size(Vector2::create(width, height));
	node.set_local_position(Vector2::create(x, y));
	if let Some(image) = image_table.get(image_name) {
		let image_width = image.get_width();
		let image_height = image.get_height();
		let sprite = node.add_component(ShaderSprite::create_with_image(image.clone()));
		if frame_columns > 1 {
			sprite.get_sprite_mut().set_image_rect(Rect::create(0.0, 0.0, image_width as f64 / frame_columns as f64, image_height as f64));
		}
	}
	else {
		node.add_component(ShaderSprite::new());
	}
	node_id
}

const TYPEWRITER_LINE_LIST: [&str; 3] = [
	"That gem... it's been calling me since the ruins.",
	"Every hero needs a proper entrance. Roll the effects!",
	"Typewriter + FloatingText + Shaker, all engine built-ins.",
];
const STATUS_NAME_LIST: [&str; 6] = ["POISON", "BURNING", "FROZEN", "GHOST", "INVINCIBLE", "STEALTH"];

//==============================================================================
// 스프라이트 연출 씬.
//==============================================================================
struct SpriteFxScene {
	scene_base: SceneBase,
	touch_recognizer: TouchRecognizer,
	screen_effect: ScreenEffect,
	shaker: Shaker,
	random: SeededRandom,
	image_asset_list: Vec<(String, ImageAsset)>,
	image_table: HashMap<String, Rc<Image>>,
	mode_list: Vec<ModeDefinition>,
	mode_index: usize,
	state: ModeState,
	elapsed: f64,
	list_row_list: Vec<ListRow>,
	list_scroll_id: Option<NodeId>,
	stage_id: Option<NodeId>,
	world_id: Option<NodeId>,
	mode_root_id: Option<NodeId>,
	hero_id: Option<NodeId>,
	subject_ids: Vec<NodeId>,
	floating_text_id: Option<NodeId>,
	fps_label_id: Option<NodeId>,
	average_delta: f64,
	world_zoom: f64,
	hero_knockback: f64,
	hero_clip_is_jump: bool,
	hero_clip_time: f64,
	spawned_particle_list: Vec<(NodeId, f64)>,
	requested_mode: Rc<Cell<Option<usize>>>,
}

impl SpriteFxScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> SpriteFxScene {
		SpriteFxScene {
			scene_base: SceneBase::new(),
			touch_recognizer: TouchRecognizer::new(),
			screen_effect: ScreenEffect::new(),
			shaker: Shaker::new(ShakerOptions { decay_rate: 9.0, frequency: 42.0 }),
			random: SeededRandom::new(EFFECT_RANDOM_SEED),
			image_asset_list: Vec::new(),
			image_table: HashMap::new(),
			mode_list: compose_mode_list(),
			mode_index: 0,
			state: ModeState::default(),
			elapsed: 0.0,
			list_row_list: Vec::new(),
			list_scroll_id: None,
			stage_id: None,
			world_id: None,
			mode_root_id: None,
			hero_id: None,
			subject_ids: Vec::new(),
			floating_text_id: None,
			fps_label_id: None,
			average_delta: 0.0,
			world_zoom: 1.0,
			hero_knockback: 0.0,
			hero_clip_is_jump: false,
			hero_clip_time: 0.0,
			spawned_particle_list: Vec::new(),
			requested_mode: Rc::new(Cell::new(None)),
		}
	}

	//==============================================================================
	// 짧은 접근. (노드 트리, 주요 노드)
	//==============================================================================
	fn tree(&mut self) -> &mut NodeTree {
		self.scene_base.get_node_tree_mut()
	}

	fn hero(&self) -> NodeId {
		self.hero_id.expect("hero")
	}

	fn mode_root(&self) -> NodeId {
		self.mode_root_id.expect("mode root")
	}

	fn rand_value(&mut self) -> f64 {
		self.random.next_value()
	}

	//==============================================================================
	// 상단 제목과 fps.
	//==============================================================================
	fn build_header(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let title_id = make_label(node_tree, root_id, 24.0, 37.0, 400.0, 26.0, 18.0, text_color(), true, TextAlign::Left);
		set_label_text(node_tree, title_id, "SPRITE EFFECTS");
		let fps_label_id = make_label(node_tree, root_id, REFERENCE_WIDTH - 24.0, 37.0, 200.0, 20.0, 11.0, text_faint_color(), false, TextAlign::Right);
		node_tree[fps_label_id].set_pivot(Pivot::MIDDLE_RIGHT);
		set_label_text(node_tree, fps_label_id, "fps  --");
		self.fps_label_id = Some(fps_label_id);
	}

	//==============================================================================
	// 왼쪽 목록. (분류 머리말 + 스크롤)
	//==============================================================================
	fn build_list(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let list_id = create_box_node(node_tree, root_id, "EffectList", LIST_X, LIST_Y, LIST_WIDTH, LIST_HEIGHT);
		node_tree[list_id].add_component(Paint::create_with_color(color(0.106, 0.125, 0.165, 1.0))).set_round_size(12.0);
		let content_id = {
			let list_scroll = node_tree.add_component(list_id, UIScrollView::new()).expect("scroll view");
			list_scroll.set_horizontal(false);
			list_scroll.set_vertical(true);
			list_scroll.get_content().expect("content")
		};
		let scroll_bar_id = create_box_node(node_tree, root_id, "EffectListScrollBar", LIST_X + LIST_WIDTH - 10.0, LIST_Y + 6.0, 6.0, LIST_HEIGHT - 12.0);
		let scroll_bar = node_tree.add_component(scroll_bar_id, UIScrollBar::new()).expect("scroll bar");
		scroll_bar.set_scroll_view(Some(list_id));
		scroll_bar.set_track_color(color(1.0, 1.0, 1.0, 0.05));
		scroll_bar.set_thumb_color(color(1.0, 1.0, 1.0, 0.22));

		let mut y = 8.0;
		let mut last_category = "";
		for (mode_index, definition) in self.mode_list.iter().enumerate() {
			if definition.category != last_category {
				last_category = definition.category;
				let header_id = make_label(node_tree, content_id, 22.0, y + LIST_HEADER_HEIGHT / 2.0 + 4.0, LIST_WIDTH - 44.0, 20.0, 10.0, text_faint_color(), true, TextAlign::Left);
				set_label_text(node_tree, header_id, definition.category);
				y += LIST_HEADER_HEIGHT;
			}
			let row_id = create_box_node(node_tree, content_id, "EffectRow", 8.0, y, LIST_WIDTH - 16.0, LIST_ROW_HEIGHT - 4.0);
			node_tree[row_id].add_component(Paint::create_with_color(transparent_color())).set_round_size(9.0);
			let title_id = make_label(node_tree, row_id, 14.0, (LIST_ROW_HEIGHT - 4.0) / 2.0, LIST_WIDTH - 44.0, 20.0, 11.5, text_color(), true, TextAlign::Left);
			set_label_text(node_tree, title_id, &definition.title);
			let requested_mode = self.requested_mode.clone();
			let row_button = node_tree.add_component(row_id, UIButton::new()).expect("button");
			row_button.set_pressed_tint_color(color(1.0, 1.0, 1.0, 0.05));
			row_button.set_clicked_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
				requested_mode.set(Some(mode_index));
			}));
			row_button.exclude_component_from_tint::<Paint>(row_id);
			row_button.exclude_node_from_tint(title_id);
			self.list_row_list.push(ListRow { row_node_id: row_id, paint_node_id: row_id, title_node_id: title_id, y });
			y += LIST_ROW_HEIGHT;
		}
		if let Some(list_scroll) = node_tree[list_id].get_component_mut::<UIScrollView>() {
			list_scroll.set_scroll_content_size(Vector2::create(LIST_WIDTH, y + 8.0));
		}
		self.list_scroll_id = Some(list_id);
	}

	//==============================================================================
	// 목록 행 컬링. (보이는 범위 밖 행은 비활성, 많은 행의 그리기와 호버 판정을 아낀다)
	//==============================================================================
	fn update_list_culling(&mut self) {
		let list_id = match self.list_scroll_id {
			Some(list_id) => list_id,
			None => return,
		};
		let node_tree = self.scene_base.get_node_tree_mut();
		let scroll_offset = match node_tree[list_id].get_component::<UIScrollView>() {
			Some(list_scroll) => list_scroll.get_scroll_offset(),
			None => return,
		};
		// 스크롤 오프셋은 내릴수록 음수. (콘텐츠가 위로 이동)
		let top_y = -scroll_offset.y - LIST_ROW_HEIGHT;
		let bottom_y = -scroll_offset.y + LIST_HEIGHT + LIST_ROW_HEIGHT;
		for list_row in self.list_row_list.iter() {
			let is_inside = list_row.y + LIST_ROW_HEIGHT >= top_y && list_row.y <= bottom_y;
			if node_tree[list_row.row_node_id].is_active() != is_inside {
				node_tree[list_row.row_node_id].set_active(is_inside);
			}
		}
	}

	//==============================================================================
	// 무대. (테두리 + 둥근 클리핑)
	//==============================================================================
	fn build_stage(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let frame_id = create_box_node(node_tree, root_id, "StageFrame", STAGE_X - 1.0, STAGE_Y - 1.0, STAGE_WIDTH + 2.0, STAGE_HEIGHT + 2.0);
		node_tree[frame_id].add_component(Paint::create_with_color(color(0.224, 0.255, 0.329, 1.0))).set_round_size(15.0);
		let stage_id = create_box_node(node_tree, root_id, "Stage", STAGE_X, STAGE_Y, STAGE_WIDTH, STAGE_HEIGHT);
		node_tree[stage_id].add_component(Paint::create_with_color(color(0.051, 0.063, 0.09, 1.0))).set_round_size(14.0);
		node_tree[stage_id].add_component(Mask::new()).set_round_size(14.0);
		self.stage_id = Some(stage_id);
	}

	//==============================================================================
	// 공용 세계. (바탕, 바닥, 주인공, 피사체들, 연출 뿌리, 떠오르는 글자)
	//==============================================================================
	fn build_world(&mut self) {
		let stage_id = self.stage_id.expect("stage");
		let image_table = self.image_table.clone();
		let node_tree = self.scene_base.get_node_tree_mut();
		let world_id = node_tree.create_child(stage_id, "World");
		node_tree[world_id].set_pivot(Pivot::MIDDLE_CENTER);
		node_tree[world_id].set_content_size(Vector2::create(STAGE_WIDTH, STAGE_HEIGHT));
		node_tree[world_id].set_local_position(Vector2::create(STAGE_WIDTH / 2.0, STAGE_HEIGHT / 2.0));
		make_paint(node_tree, world_id, STAGE_WIDTH / 2.0, STAGE_HEIGHT / 2.0, STAGE_WIDTH + 80.0, STAGE_HEIGHT + 80.0, color(0.07, 0.09, 0.14, 1.0), 0.0);
		make_paint(node_tree, world_id, STAGE_WIDTH / 2.0, 60.0, 2200.0, 1200.0, color(0.11, 0.14, 0.22, 1.0), 600.0);
		make_paint(node_tree, world_id, STAGE_WIDTH / 2.0, FLOOR_Y + 140.0, STAGE_WIDTH + 80.0, 280.0, color(0.13, 0.17, 0.25, 1.0), 0.0);
		make_paint(node_tree, world_id, STAGE_WIDTH / 2.0, FLOOR_Y + 2.0, STAGE_WIDTH + 80.0, 4.0, color(0.25, 0.32, 0.45, 1.0), 0.0);
		let mode_root_id = node_tree.create_child(world_id, "ModeRoot");
		node_tree[mode_root_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[mode_root_id].set_content_size(Vector2::create(STAGE_WIDTH, STAGE_HEIGHT));
		// 원본은 피사체와 주인공을 먼저 붙이고 연출 뿌리를 그 뒤에 붙인다. (연출이 피사체 위에 그려진다)
		let emblem_id = make_sprite(node_tree, &image_table, world_id, "emblem.png", 484.0, 160.0, 150.0, 150.0, 1);
		let star_id = make_sprite(node_tree, &image_table, world_id, "star.png", 720.0, 150.0, 72.0, 72.0, 1);
		let coin_id = make_sprite(node_tree, &image_table, world_id, "coin.png", 620.0, FLOOR_Y - 32.0, 56.0, 56.0, 8);
		let gem_id = make_sprite(node_tree, &image_table, world_id, "gem.png", 800.0, FLOOR_Y - 34.0, 60.0, 60.0, 1);
		let bat_id = make_sprite(node_tree, &image_table, world_id, "bat.png", 800.0, 220.0, 96.0, 72.0, 4);
		let hero_id = make_sprite(node_tree, &image_table, world_id, "slime.png", 300.0, FLOOR_Y - 60.0, 128.0, 128.0, 8);
		node_tree.remove_from_parent(mode_root_id);
		node_tree.add_child(world_id, mode_root_id);
		let floating_text_id = FloatingText::create(node_tree, world_id);
		self.subject_ids = vec![emblem_id, star_id, coin_id, gem_id, bat_id];
		self.hero_id = Some(hero_id);
		self.world_id = Some(world_id);
		self.mode_root_id = Some(mode_root_id);
		self.floating_text_id = Some(floating_text_id);
	}

	//==============================================================================
	// 파티클 노드 하나 뿌리기. (원본 spawnParticle, 설명서 JSON 을 얹어 쓴다)
	//==============================================================================
	fn spawn_particle(&mut self, description_overrides: Value, x: f64, y: f64) -> NodeId {
		let mut description = json!({
			"looping": false, "duration": 1.2, "maxParticleCount": 800, "emissionRate": 0, "bursts": [{ "time": 0, "count": 40 }],
			"shape": "circle", "shapeRadius": 6, "coneAngle": 0.35, "boxSize": [200, 100], "edgeWidth": 400,
			"lifetime": [0.4, 0.9], "speed": [160, 420], "size": [3, 6], "rotation": [0, 0], "angularVelocity": [0, 0],
			"startColorA": [1, 0.85, 0.4, 1], "startColorB": [1, 0.55, 0.2, 1],
			"colorOverLifetime": [{ "time": 0, "color": [1, 1, 1, 1] }, { "time": 1, "color": [1, 0.4, 0.1, 0] }],
			"sizeOverLifetime": [1, 0.5], "gravity": [0, 300], "damping": 1.6, "wobble": [0, 4],
			"renderShape": "streak", "blendMode": "lighter", "streakScale": 0.04, "worldSpace": false,
		});
		if let (Some(base_map), Some(override_map)) = (description.as_object_mut(), description_overrides.as_object()) {
			for (key, value) in override_map.iter() {
				base_map.insert(key.clone(), value.clone());
			}
		}
		let is_looping = description["looping"].as_bool().unwrap_or(false);
		let expire_time = self.elapsed + description["duration"].as_f64().unwrap_or(1.2) + description["lifetime"][1].as_f64().unwrap_or(0.9) + 0.5;
		let mode_root_id = self.mode_root();
		let node_tree = self.scene_base.get_node_tree_mut();
		let particle_id = node_tree.create_child(mode_root_id, "particle");
		node_tree[particle_id].set_pivot(Pivot::MIDDLE_CENTER);
		node_tree[particle_id].set_content_size(Vector2::create(10.0, 10.0));
		node_tree[particle_id].set_local_position(Vector2::create(x, y));
		let particle_system = node_tree[particle_id].add_component(ParticleSystem::new());
		particle_system.apply_description(&description);
		particle_system.play();
		if !is_looping {
			// 비반복 시스템은 방출 창 + 최장 수명이 지나면 지운다.
			self.spawned_particle_list.push((particle_id, expire_time));
		}
		particle_id
	}

	//==============================================================================
	// 다 탄 파티클 노드 정리. (연출이 계속 뿌리는 시스템이 쌓여 프레임을 갉지 않게)
	//==============================================================================
	fn prune_particles(&mut self) {
		let elapsed = self.elapsed;
		let mut expired_ids: Vec<NodeId> = Vec::new();
		let node_tree = self.scene_base.get_node_tree();
		self.spawned_particle_list.retain(|(particle_id, expire_time)| {
			let particle_count = match node_tree.get_node(*particle_id).and_then(|node| node.get_component::<ParticleSystem>()) {
				Some(particle_system) => particle_system.get_particle_count(),
				None => return false,
			};
			if elapsed >= *expire_time && particle_count == 0 {
				expired_ids.push(*particle_id);
				return false;
			}
			true
		});
		let node_tree = self.scene_base.get_node_tree_mut();
		for particle_id in expired_ids {
			node_tree.destroy_node(particle_id);
		}
	}

	fn spawn_hit_sparks(&mut self, position: Vector2) {
		self.spawn_particle(json!({}), position.x, position.y);
	}

	fn spawn_dust(&mut self, position: Vector2) {
		self.spawn_particle(json!({ "bursts": [{ "time": 0, "count": 18 }], "speed": [60, 160], "gravity": [0, -40], "lifetime": [0.3, 0.6], "size": [6, 14], "renderShape": "circle", "blendMode": "source-over", "startColorA": [0.6, 0.6, 0.7, 0.5], "startColorB": [0.8, 0.8, 0.85, 0.35], "colorOverLifetime": [{ "time": 0, "color": [1, 1, 1, 1] }, { "time": 1, "color": [1, 1, 1, 0] }], "sizeOverLifetime": [0.6, 1.6] }), position.x, position.y);
	}

	fn spawn_rain_particles(&mut self) {
		self.spawn_particle(json!({ "looping": true, "emissionRate": 260, "bursts": [], "shape": "edge", "edgeWidth": 1100, "lifetime": [0.7, 0.9], "speed": [700, 900], "size": [1.4, 2.2], "gravity": [0, 0], "damping": 0, "renderShape": "streak", "streakScale": 0.03, "blendMode": "source-over", "startColorA": [0.62, 0.74, 0.95, 0.5], "startColorB": [0.75, 0.85, 1, 0.3], "sizeOverLifetime": [1, 1], "colorOverLifetime": [{ "time": 0, "color": [1, 1, 1, 1] }, { "time": 1, "color": [1, 1, 1, 1] }] }), 484.0, -40.0);
	}

	//==============================================================================
	// 떠오르는 글자.
	//==============================================================================
	fn spawn_floating_text(&mut self, text: &str, position: Vector2, floating_options: FloatingTextOptions) {
		let floating_text_id = self.floating_text_id.expect("floating text");
		FloatingText::spawn(self.scene_base.get_node_tree_mut(), floating_text_id, text, position, floating_options);
	}

	fn floating_options(font_size: f64, text_color: Color) -> FloatingTextOptions {
		FloatingTextOptions { color: text_color, font_size, ..FloatingTextOptions::default() }
	}

	//==============================================================================
	// 주인공 도우미.
	//==============================================================================
	fn get_hero_position(&mut self) -> Vector2 {
		let hero_id = self.hero();
		self.tree()[hero_id].get_local_position()
	}

	fn get_hero_stage_position(&mut self) -> Vector2 {
		let hero_position = self.get_hero_position();
		Vector2::create((STAGE_X + hero_position.x) / REFERENCE_WIDTH, (STAGE_Y + hero_position.y) / REFERENCE_HEIGHT)
	}

	fn set_time_scale(&self, time_scale: f64) {
		let engine = Engine::get_engine().unwrap();
		engine.get_time_manager_mut().set_time_scale(time_scale);
	}

	fn apply_hero_knockback(&mut self, time_delta: f64, distance: f64) {
		if self.hero_knockback <= 0.0 {
			return;
		}
		self.hero_knockback = (self.hero_knockback - time_delta * 2.5).max(0.0);
		let eased = self.hero_knockback * self.hero_knockback;
		let hero_id = self.hero();
		let hero_node = &mut self.tree()[hero_id];
		hero_node.set_local_position(Vector2::create(300.0 - eased * distance * 0.25, FLOOR_Y - 60.0));
		hero_node.set_local_rotation(-eased * 18.0);
	}

	fn set_subjects_visible(&mut self, is_visible: bool) {
		let subject_ids = self.subject_ids.clone();
		let hero_id = self.hero();
		let node_tree = self.tree();
		for subject_id in subject_ids {
			node_tree[subject_id].set_active(is_visible);
		}
		node_tree[hero_id].set_active(is_visible);
	}

	fn apply_subject_effect(&mut self, effect: ShaderSpriteEffect) {
		let mut sprite_ids = self.subject_ids.clone();
		sprite_ids.push(self.hero());
		let node_tree = self.tree();
		for sprite_id in sprite_ids {
			shader_sprite(node_tree, sprite_id).set_effect(effect);
		}
	}

	fn set_subject_progress(&mut self, progress: f64) {
		let mut sprite_ids = self.subject_ids.clone();
		sprite_ids.push(self.hero());
		let node_tree = self.tree();
		for sprite_id in sprite_ids {
			shader_sprite(node_tree, sprite_id).set_effect_progress(progress);
		}
	}

	fn hero_sprite(&mut self) -> &mut ShaderSprite {
		let hero_id = self.hero();
		shader_sprite(self.tree(), hero_id)
	}

	//==============================================================================
	// 모드 선택. (이전 모드 정리, 공용 세계 초기화, 새 모드 준비)
	//==============================================================================
	fn select_mode(&mut self, mode_index: usize) {
		// 이전 모드 정리. (시차 배경은 주인공을 뿌리 아래로 옮겨 두었으므로 돌려놓는다)
		if self.mode_list[self.mode_index].kind == ModeKind::Staging(Staging::Parallax) {
			let (hero_id, world_id) = (self.hero(), self.world_id.expect("world"));
			let node_tree = self.tree();
			node_tree.remove_from_parent(hero_id);
			node_tree.add_child(world_id, hero_id);
		}
		if self.mode_list[self.mode_index].kind == ModeKind::Staging(Staging::FreezeFrame) {
			self.set_time_scale(1.0);
		}
		self.mode_index = mode_index;
		self.elapsed = 0.0;
		self.state = ModeState::default();
		self.world_zoom = 1.0;
		self.hero_knockback = 0.0;
		self.set_time_scale(1.0);
		self.shaker.stop();
		self.screen_effect.reset_all();
		let (mode_root_id, floating_text_id, hero_id) = (self.mode_root(), self.floating_text_id.expect("floating text"), self.hero());
		let subject_ids = self.subject_ids.clone();
		let node_tree = self.scene_base.get_node_tree_mut();
		node_tree.destroy_children(mode_root_id);
		FloatingText::clear(node_tree, floating_text_id);
		self.spawned_particle_list.clear();
		self.set_subjects_visible(true);
		self.hero_clip_is_jump = false;
		self.hero_clip_time = 0.0;
		let node_tree = self.scene_base.get_node_tree_mut();
		let hero_node = &mut node_tree[hero_id];
		hero_node.set_local_position(Vector2::create(300.0, FLOOR_Y - 60.0));
		hero_node.set_local_scale(Vector2::create(1.0, 1.0));
		hero_node.set_local_rotation(0.0);
		hero_node.set_local_opacity(1.0);
		let hero_sprite = shader_sprite(node_tree, hero_id);
		hero_sprite.get_sprite_mut().set_horizontal_flip(false);
		hero_sprite.set_effect(ShaderSpriteEffect::None);
		node_tree[subject_ids[SUBJECT_BAT]].set_local_position(Vector2::create(800.0, 220.0));
		for subject_id in subject_ids.iter() {
			shader_sprite(node_tree, *subject_id).set_effect(ShaderSpriteEffect::None);
		}
		let mode_kind = self.mode_list[mode_index].kind;
		match mode_kind {
			ModeKind::SpriteShader(effect) => {
				self.apply_subject_effect(effect);
			}
			ModeKind::Screen(effect_type) => {
				self.screen_effect.set_enabled(effect_type, true);
				self.screen_effect.set_strength(effect_type, 1.0);
			}
			ModeKind::Staging(staging) => {
				self.setup_staging(staging);
			}
		}
		let node_tree = self.scene_base.get_node_tree_mut();
		for (row_index, list_row) in self.list_row_list.iter().enumerate() {
			let is_active = row_index == mode_index;
			if let Some(row_paint) = node_tree[list_row.paint_node_id].get_component_mut::<Paint>() {
				row_paint.set_color(if is_active { accent_color() } else { transparent_color() });
			}
			set_label_color(node_tree, list_row.title_node_id, if is_active { on_accent_color() } else { text_color() });
		}
	}

	//==============================================================================
	// 연출 모드 준비. (원본 STAGING_DEFINITIONS 의 setup)
	//==============================================================================
	fn setup_staging(&mut self, staging: Staging) {
		let mode_root_id = self.mode_root();
		let hero_id = self.hero();
		let subject_ids = self.subject_ids.clone();
		let image_table = self.image_table.clone();
		match staging {
			Staging::HitStop => {
				self.state.next_event = 0.6;
				self.screen_effect.set_enabled(ScreenEffectType::Chromatic, true);
				self.screen_effect.set_strength(ScreenEffectType::Chromatic, 0.0);
				self.hero_sprite().set_effect(ShaderSpriteEffect::Flash);
			}
			Staging::ScreenShake => {
				self.state.next_event = 0.4;
			}
			Staging::SlowMotion => {
				self.screen_effect.set_enabled(ScreenEffectType::ColorGrade, true);
				self.screen_effect.set_parameters(ScreenEffectType::ColorGrade, 1.2, 0.3, 1.05, 0.0);
				self.screen_effect.set_enabled(ScreenEffectType::SpeedLines, true);
				self.spawn_rain_particles();
			}
			Staging::Afterimage => {}
			Staging::Flipbook => {
				self.state.next_event = 1.0;
				self.state.jump_time = -1.0;
				self.set_subjects_visible(true);
			}
			Staging::FloatingDamage => {}
			Staging::TypewriterDialogue => {
				let node_tree = self.scene_base.get_node_tree_mut();
				make_paint(node_tree, mode_root_id, 484.0, 560.0, 764.0, 104.0, color(0.83, 0.69, 0.42, 0.25), 13.0);
				make_paint(node_tree, mode_root_id, 484.0, 560.0, 760.0, 100.0, color(0.03, 0.04, 0.07, 0.88), 12.0);
				let speaker_id = make_label(node_tree, mode_root_id, 132.0, 522.0, 300.0, 20.0, 12.0, accent_color(), true, TextAlign::Left);
				set_label_text(node_tree, speaker_id, "SLIME");
				let line_id = create_box_node(node_tree, mode_root_id, "line", 132.0, 540.0, 700.0, 60.0);
				let line_text = node_tree.add_component(line_id, Text::create_with_text("", 20.0)).expect("text");
				line_text.set_text_color(text_color());
				line_text.set_text_align(TextAlign::Left);
				line_text.set_text_baseline(TextBaseline::Top);
				let caret_id = make_label(node_tree, mode_root_id, 830.0, 590.0, 20.0, 20.0, 16.0, accent_color(), true, TextAlign::Center);
				set_label_text(node_tree, caret_id, "\u{25BC}");
				node_tree[hero_id].set_local_position(Vector2::create(300.0, FLOOR_Y - 60.0));
				let mut typewriter = Typewriter::new(TypewriterOptions { characters_per_second: 28.0, ..TypewriterOptions::default() });
				if let Some(line_text) = node_tree[line_id].get_component_mut::<Text>() {
					typewriter.start(line_text, Some(TYPEWRITER_LINE_LIST[0]));
				}
				self.state.typewriter = Some(typewriter);
				self.state.node_a = Some(line_id);
				self.state.node_b = Some(caret_id);
			}
			Staging::SlashTrail => {
				let node_tree = self.scene_base.get_node_tree_mut();
				let blade_id = node_tree.create_child(mode_root_id, "blade");
				node_tree[blade_id].set_pivot(Pivot::MIDDLE_CENTER);
				node_tree[blade_id].set_content_size(Vector2::create(8.0, 8.0));
				let trail = node_tree[blade_id].add_component(TrailRenderer::new());
				trail.set_time(0.28);
				trail.set_width(30.0, 0.0);
				trail.set_additive(true);
				trail.set_colors(color(1.0, 0.92, 0.6, 1.0), color(1.0, 0.55, 0.2, 0.0));
				self.state.node_a = Some(blade_id);
				self.state.swing_start = 0.4;
				node_tree[subject_ids[SUBJECT_BAT]].set_local_position(Vector2::create(700.0, 300.0));
			}
			Staging::Parallax => {
				self.set_subjects_visible(false);
				let layer_definition_list: [(f64, f64, f64, Color, usize, f64); 3] = [
					(18.0, 380.0, 300.0, color(0.11, 0.15, 0.25, 1.0), 5, 420.0),
					(60.0, 440.0, 260.0, color(0.16, 0.22, 0.34, 1.0), 6, 340.0),
					(150.0, 520.0, 220.0, color(0.2, 0.28, 0.42, 1.0), 8, 260.0),
				];
				let node_tree = self.scene_base.get_node_tree_mut();
				for (speed, y, height, layer_color, layer_count, width) in layer_definition_list {
					let layer_id = create_box_node(node_tree, mode_root_id, "layer", 0.0, 0.0, STAGE_WIDTH * 2.0, STAGE_HEIGHT);
					for index in 0..layer_count * 2 {
						make_paint(node_tree, layer_id, index as f64 * width, y, width * 1.1, height, layer_color, width * 0.5);
					}
					self.state.layer_list.push((layer_id, speed, layer_count as f64 * width));
				}
				node_tree[hero_id].set_active(true);
				node_tree.remove_from_parent(hero_id);
				node_tree.add_child(mode_root_id, hero_id);
				node_tree[hero_id].set_local_position(Vector2::create(300.0, FLOOR_Y - 60.0));
			}
			Staging::CameraPunch => {
				self.state.next_event = 0.5;
				self.screen_effect.set_enabled(ScreenEffectType::Chromatic, true);
				self.screen_effect.set_strength(ScreenEffectType::Chromatic, 0.0);
			}
			Staging::SquashStretch => {
				self.state.y = FLOOR_Y - 60.0;
				self.state.scale_x = 1.0;
				self.state.scale_y = 1.0;
				self.state.next_event = 0.5;
			}
			Staging::SpawnDespawn => {
				self.state.next_event = 0.3;
			}
			Staging::HealthBar => {
				let node_tree = self.scene_base.get_node_tree_mut();
				make_paint(node_tree, mode_root_id, 484.0, 590.0, 620.0, 30.0, color(0.03, 0.04, 0.07, 0.9), 15.0);
				let ghost_id = make_paint(node_tree, mode_root_id, 176.0, 590.0, 616.0, 22.0, color(1.0, 1.0, 1.0, 0.85), 11.0);
				node_tree[ghost_id].set_pivot(Vector2::create(0.0, 0.5));
				let fill_id = make_paint(node_tree, mode_root_id, 176.0, 590.0, 616.0, 22.0, color(0.27, 0.71, 0.52, 1.0), 11.0);
				node_tree[fill_id].set_pivot(Vector2::create(0.0, 0.5));
				let label_id = make_label(node_tree, mode_root_id, 484.0, 590.0, 200.0, 24.0, 13.0, text_color(), true, TextAlign::Center);
				self.state.health = 1.0;
				self.state.ghost_health = 1.0;
				self.state.next_event = 1.0;
				self.state.node_a = Some(fill_id);
				self.state.node_b = Some(ghost_id);
				self.state.node_c = Some(label_id);
				self.screen_effect.set_enabled(ScreenEffectType::Vignette, true);
				self.screen_effect.set_color(ScreenEffectType::Vignette, color(0.6, 0.05, 0.05, 1.0));
				self.screen_effect.set_strength(ScreenEffectType::Vignette, 0.0);
			}
			Staging::Lightning => {
				self.state.next_event = 0.8;
				self.screen_effect.set_enabled(ScreenEffectType::Fade, true);
				self.screen_effect.set_color(ScreenEffectType::Fade, color(1.0, 1.0, 1.0, 1.0));
				self.screen_effect.set_strength(ScreenEffectType::Fade, 0.0);
				self.screen_effect.set_enabled(ScreenEffectType::Invert, true);
				self.screen_effect.set_strength(ScreenEffectType::Invert, 0.0);
				self.spawn_rain_particles();
			}
			Staging::LevelUp => {
				self.state.next_event = 0.6;
				let hero_sprite = self.hero_sprite();
				hero_sprite.set_effect(ShaderSpriteEffect::Glow);
				hero_sprite.set_effect_color(accent_color());
				hero_sprite.set_effect_parameters(10.0, 0.0, 0.0, 0.0);
			}
			Staging::Teleport => {
				let hero_sprite = self.hero_sprite();
				hero_sprite.set_effect(ShaderSpriteEffect::PixelDissolve);
				hero_sprite.set_effect_parameters(12.0, 0.0, 0.0, 0.0);
				self.screen_effect.set_enabled(ScreenEffectType::Chromatic, true);
				self.screen_effect.set_strength(ScreenEffectType::Chromatic, 0.0);
			}
			Staging::FreezeFrame => {
				self.screen_effect.set_enabled(ScreenEffectType::Letterbox, true);
				self.screen_effect.set_enabled(ScreenEffectType::Sepia, true);
				self.screen_effect.set_enabled(ScreenEffectType::Vignette, true);
				let node_tree = self.scene_base.get_node_tree_mut();
				let label_id = make_label(node_tree, mode_root_id, 484.0, 96.0, 700.0, 40.0, 26.0, text_color(), true, TextAlign::Center);
				set_label_text(node_tree, label_id, "\"...and that was the moment everything changed.\"");
				self.state.node_a = Some(label_id);
			}
			Staging::Portal => {
				let node_tree = self.scene_base.get_node_tree_mut();
				let glow_id = make_paint(node_tree, mode_root_id, 700.0, 300.0, 180.0, 180.0, color(0.5, 0.35, 1.0, 0.55), 90.0);
				self.state.node_a = Some(glow_id);
				self.screen_effect.set_enabled(ScreenEffectType::Swirl, true);
				self.screen_effect.set_parameters(ScreenEffectType::Swirl, (STAGE_X + 700.0) / REFERENCE_WIDTH, (STAGE_Y + 300.0) / REFERENCE_HEIGHT, 0.9, 0.35);
				self.screen_effect.set_enabled(ScreenEffectType::Bloom, true);
				self.screen_effect.set_strength(ScreenEffectType::Bloom, 0.8);
				self.spawn_particle(json!({ "looping": true, "emissionRate": 90, "bursts": [], "shape": "circle", "shapeRadius": 90, "lifetime": [0.8, 1.4], "speed": [30, 90], "size": [2, 5], "gravity": [0, 0], "damping": 0.5, "renderShape": "circle", "attractor": { "x": 0, "y": 0, "strength": 240, "swirl": 320 }, "startColorA": [0.6, 0.4, 1, 1], "startColorB": [0.3, 0.8, 1, 1], "colorOverLifetime": [{ "time": 0, "color": [1, 1, 1, 0] }, { "time": 0.3, "color": [1, 1, 1, 1] }, { "time": 1, "color": [1, 1, 1, 0] }] }), 700.0, 300.0);
			}
			Staging::Underwater => {
				self.screen_effect.set_enabled(ScreenEffectType::Wave, true);
				self.screen_effect.set_strength(ScreenEffectType::Wave, 0.5);
				self.screen_effect.set_enabled(ScreenEffectType::ColorGrade, true);
				self.screen_effect.set_parameters(ScreenEffectType::ColorGrade, 1.05, 0.8, 0.95, 0.0);
				self.screen_effect.set_color(ScreenEffectType::ColorGrade, color(0.5, 0.8, 1.0, 0.6));
				self.screen_effect.set_enabled(ScreenEffectType::GodRays, true);
				self.screen_effect.set_parameters(ScreenEffectType::GodRays, 0.5, 0.05, 0.5, 0.93);
				self.screen_effect.set_strength(ScreenEffectType::GodRays, 0.6);
				self.spawn_particle(json!({ "looping": true, "emissionRate": 30, "bursts": [], "shape": "edge", "edgeWidth": 900, "lifetime": [2.6, 4.4], "speed": [50, 110], "size": [4, 14], "gravity": [0, -30], "damping": 0.2, "wobble": [20, 2.4], "renderShape": "circle", "blendMode": "source-over", "startColorA": [0.6, 0.85, 1, 0.5], "startColorB": [0.8, 0.95, 1, 0.3], "sizeOverLifetime": [0.7, 1.15], "colorOverLifetime": [{ "time": 0, "color": [1, 1, 1, 0] }, { "time": 0.1, "color": [1, 1, 1, 1] }, { "time": 0.85, "color": [1, 1, 1, 1] }, { "time": 1, "color": [1, 1, 1, 0] }] }), 484.0, STAGE_HEIGHT + 20.0);
			}
			Staging::StatusEffects => {
				self.state.index = -1;
				self.state.timer = 99.0;
				let node_tree = self.scene_base.get_node_tree_mut();
				let label_id = make_label(node_tree, mode_root_id, 300.0, FLOOR_Y - 150.0, 300.0, 24.0, 16.0, accent_color(), true, TextAlign::Center);
				self.state.node_a = Some(label_id);
				self.set_subjects_visible(false);
				self.scene_base.get_node_tree_mut()[hero_id].set_active(true);
			}
			Staging::Victory => {
				self.state.next_event = 0.2;
				self.screen_effect.set_enabled(ScreenEffectType::SpeedLines, true);
				self.screen_effect.set_strength(ScreenEffectType::SpeedLines, 0.6);
				self.screen_effect.set_enabled(ScreenEffectType::Bloom, true);
				self.screen_effect.set_strength(ScreenEffectType::Bloom, 0.5);
				let node_tree = self.scene_base.get_node_tree_mut();
				shader_sprite(node_tree, subject_ids[SUBJECT_EMBLEM]).set_effect(ShaderSpriteEffect::Shine);
				let label_id = make_label(node_tree, mode_root_id, 484.0, 330.0, 700.0, 80.0, 64.0, accent_color(), true, TextAlign::Center);
				set_label_text(node_tree, label_id, "VICTORY");
				self.state.node_a = Some(label_id);
			}
			Staging::GameOver => {
				self.screen_effect.set_enabled(ScreenEffectType::Grayscale, true);
				self.screen_effect.set_enabled(ScreenEffectType::Vignette, true);
				self.screen_effect.set_enabled(ScreenEffectType::TvOff, true);
				let node_tree = self.scene_base.get_node_tree_mut();
				let label_id = make_label(node_tree, mode_root_id, 484.0, 300.0, 700.0, 80.0, 56.0, color(0.9, 0.3, 0.3, 1.0), true, TextAlign::Center);
				set_label_text(node_tree, label_id, "GAME OVER");
				self.state.node_a = Some(label_id);
			}
			Staging::DayNight => {
				self.screen_effect.set_enabled(ScreenEffectType::WhiteBalance, true);
				self.screen_effect.set_enabled(ScreenEffectType::LiftGammaGain, true);
				self.screen_effect.set_enabled(ScreenEffectType::Fog, true);
				self.screen_effect.set_color(ScreenEffectType::Fog, color(0.9, 0.7, 0.5, 1.0));
				self.screen_effect.set_parameters(ScreenEffectType::Fog, 0.55, 1.1, 0.0, 0.0);
				let stars_id = self.spawn_particle(json!({ "looping": true, "emissionRate": 0, "bursts": [{ "time": 0, "count": 90 }], "shape": "box", "boxSize": [960, 300], "lifetime": [60, 60], "speed": [0, 0], "size": [1.5, 3], "gravity": [0, 0], "damping": 0, "renderShape": "circle", "startColorA": [1, 1, 1, 1], "startColorB": [0.8, 0.9, 1, 0.8], "colorOverLifetime": [{ "time": 0, "color": [1, 1, 1, 1] }, { "time": 1, "color": [1, 1, 1, 1] }], "sizeOverLifetime": [1, 1] }), 484.0, 180.0);
				self.state.node_a = Some(stars_id);
			}
			Staging::DamageFlash => {
				self.state.next_event = 0.7;
				let hero_sprite = self.hero_sprite();
				hero_sprite.set_effect(ShaderSpriteEffect::Flash);
				hero_sprite.set_effect_color(color(1.0, 0.3, 0.3, 1.0));
			}
			Staging::RainStorm => {
				self.screen_effect.set_enabled(ScreenEffectType::Rain, true);
				self.screen_effect.set_enabled(ScreenEffectType::ColorGrade, true);
				self.screen_effect.set_parameters(ScreenEffectType::ColorGrade, 1.1, 0.7, 0.85, 0.0);
				self.screen_effect.set_color(ScreenEffectType::ColorGrade, color(0.7, 0.8, 1.0, 0.5));
				self.spawn_rain_particles();
			}
		}
		let _ = image_table;
	}

	//==============================================================================
	// 링 목록 키우기. (확장하며 사라지면 지운다)
	//==============================================================================
	fn grow_rings(&mut self, time_delta: f64, duration: f64, grow: f64) {
		let mut ring_list = std::mem::take(&mut self.state.ring_list);
		let node_tree = self.scene_base.get_node_tree_mut();
		ring_list.retain_mut(|(ring_id, age)| {
			*age += time_delta;
			let phase = (*age / duration).min(1.0);
			node_tree[*ring_id].set_local_scale(Vector2::create(1.0 + phase * grow, 1.0 + phase * grow));
			node_tree[*ring_id].set_local_opacity(1.0 - phase);
			if phase >= 1.0 {
				node_tree.destroy_node(*ring_id);
				return false;
			}
			true
		});
		self.state.ring_list = ring_list;
	}

	//==============================================================================
	// 상태 이상 하나 걸기. (원본 statusList 의 apply)
	//==============================================================================
	fn apply_status(&mut self, status_index: i32) {
		let hero_id = self.hero();
		match status_index {
			0 => {
				let hero_sprite = self.hero_sprite();
				hero_sprite.set_effect(ShaderSpriteEffect::Colorize);
				hero_sprite.set_effect_color(color(0.45, 1.0, 0.35, 1.0));
			}
			1 => {
				self.hero_sprite().set_effect(ShaderSpriteEffect::Burn);
				self.screen_effect.set_enabled(ScreenEffectType::HeatHaze, true);
				self.screen_effect.set_strength(ScreenEffectType::HeatHaze, 0.6);
				let particle_id = self.spawn_particle(json!({ "looping": true, "emissionRate": 70, "bursts": [], "shape": "cone", "shapeRadius": 24, "coneAngle": 0.3, "lifetime": [0.5, 1], "speed": [80, 160], "size": [6, 14], "gravity": [0, -140], "damping": 1, "renderShape": "circle", "startColorA": [1, 0.75, 0.3, 1], "startColorB": [1, 0.5, 0.12, 1], "colorOverLifetime": [{ "time": 0, "color": [1, 1, 0.85, 1] }, { "time": 0.5, "color": [1, 0.6, 0.2, 0.9] }, { "time": 1, "color": [0.65, 0.1, 0.02, 0] }], "sizeOverLifetime": [1, 0.3] }), 300.0, FLOOR_Y - 60.0);
				self.state.particle_node = Some(particle_id);
			}
			2 => {
				let hero_sprite = self.hero_sprite();
				hero_sprite.set_effect(ShaderSpriteEffect::GradientMap);
				hero_sprite.set_effect_color(color(0.85, 0.95, 1.0, 1.0));
				hero_sprite.set_effect_parameters(0.1, 0.25, 0.55, 0.0);
				self.screen_effect.set_enabled(ScreenEffectType::WhiteBalance, true);
				self.screen_effect.set_parameters(ScreenEffectType::WhiteBalance, -0.8, 0.0, 0.0, 0.0);
			}
			3 => {
				self.hero_sprite().set_effect(ShaderSpriteEffect::Hologram);
			}
			4 => {
				let hero_sprite = self.hero_sprite();
				hero_sprite.set_effect(ShaderSpriteEffect::Outline);
				hero_sprite.set_effect_color(accent_color());
				hero_sprite.set_effect_parameters(4.0, 0.0, 0.0, 0.0);
				self.screen_effect.set_enabled(ScreenEffectType::SpeedLines, true);
				self.screen_effect.set_strength(ScreenEffectType::SpeedLines, 0.5);
			}
			_ => {
				let hero_sprite = self.hero_sprite();
				hero_sprite.set_effect(ShaderSpriteEffect::Silhouette);
				hero_sprite.set_effect_color(color(0.1, 0.1, 0.2, 0.55));
			}
		}
		let _ = hero_id;
	}

	//==============================================================================
	// 연출 모드 갱신. (원본 STAGING_DEFINITIONS 의 update)
	//==============================================================================
	fn update_staging(&mut self, staging: Staging, time_delta: f64, unscaled_delta: f64) {
		let elapsed = self.elapsed;
		let hero_id = self.hero();
		let mode_root_id = self.mode_root();
		let subject_ids = self.subject_ids.clone();
		let image_table = self.image_table.clone();
		match staging {
			Staging::HitStop => {
				if self.state.freeze_remaining > 0.0 {
					self.state.freeze_remaining -= unscaled_delta;
					if self.state.freeze_remaining <= 0.0 {
						self.set_time_scale(1.0);
					}
				}
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 1.4;
					self.state.freeze_remaining = 0.09;
					self.state.flash = 1.0;
					self.set_time_scale(0.0);
					self.shaker.add_shake(16.0);
					let hero_position = self.get_hero_position();
					self.spawn_hit_sparks(hero_position);
					let damage = (80.0 + self.rand_value() * 120.0).round();
					self.spawn_floating_text(&format!("-{}", damage), Vector2::create(hero_position.x + 20.0, hero_position.y - 70.0), SpriteFxScene::floating_options(30.0, color(1.0, 0.4, 0.4, 1.0)));
					self.hero_knockback = 1.0;
				}
				self.state.flash = (self.state.flash - unscaled_delta * 4.0).max(0.0);
				let flash = self.state.flash;
				self.hero_sprite().set_effect_progress(flash);
				self.screen_effect.set_strength(ScreenEffectType::Chromatic, flash * 1.5);
				self.apply_hero_knockback(time_delta, 260.0);
			}
			Staging::ScreenShake => {
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 1.6;
					self.shaker.add_shake(26.0);
					self.spawn_hit_sparks(Vector2::create(484.0, FLOOR_Y));
					self.spawn_floating_text("BOOM", Vector2::create(484.0, FLOOR_Y - 60.0), SpriteFxScene::floating_options(34.0, accent_color()));
				}
			}
			Staging::SlowMotion => {
				let cycle = (elapsed % 4.0) / 4.0;
				let is_slow = cycle > 0.35 && cycle < 0.85;
				self.set_time_scale(if is_slow { 0.2 } else { 1.0 });
				let blend = if is_slow { 1.0 } else { 0.0 };
				self.state.phase += (blend - self.state.phase) * (unscaled_delta * 6.0).min(1.0);
				self.screen_effect.set_strength(ScreenEffectType::ColorGrade, self.state.phase);
				self.screen_effect.set_strength(ScreenEffectType::SpeedLines, self.state.phase * 0.8);
			}
			Staging::Afterimage => {
				let hero_position = Vector2::create(484.0 + (elapsed * 2.2).sin() * 300.0, FLOOR_Y - 60.0 - (elapsed * 4.4).sin().abs() * 120.0);
				let is_flipped = (elapsed * 2.2).cos() < 0.0;
				let (hero_rect, hero_flip) = {
					let node_tree = self.scene_base.get_node_tree_mut();
					node_tree[hero_id].set_local_position(hero_position);
					let hero_sprite = shader_sprite(node_tree, hero_id).get_sprite_mut();
					hero_sprite.set_horizontal_flip(is_flipped);
					(hero_sprite.get_image_rect(), hero_sprite.is_horizontal_flip())
				};
				self.state.accumulator += time_delta;
				if self.state.accumulator >= 0.05 {
					self.state.accumulator = 0.0;
					let node_tree = self.scene_base.get_node_tree_mut();
					let ghost_id = make_sprite(node_tree, &image_table, mode_root_id, "slime.png", hero_position.x, hero_position.y, 128.0, 128.0, 8);
					let ghost_sprite = shader_sprite(node_tree, ghost_id);
					ghost_sprite.get_sprite_mut().set_image_rect(hero_rect);
					ghost_sprite.get_sprite_mut().set_horizontal_flip(hero_flip);
					ghost_sprite.set_effect(ShaderSpriteEffect::Silhouette);
					ghost_sprite.set_effect_color(color(0.4, 0.8, 1.0, 1.0));
					ghost_sprite.set_effect_progress(1.0);
					ghost_sprite.get_sprite_mut().set_sprite_blend_mode(BlendMode::Lighter);
					self.state.ghost_list.push((ghost_id, 0.45));
				}
				let mut ghost_list = std::mem::take(&mut self.state.ghost_list);
				let node_tree = self.scene_base.get_node_tree_mut();
				ghost_list.retain_mut(|(ghost_id, life)| {
					*life -= time_delta;
					node_tree[*ghost_id].set_local_opacity((*life / 0.45).max(0.0) * 0.6);
					if *life <= 0.0 {
						node_tree.destroy_node(*ghost_id);
						return false;
					}
					true
				});
				self.state.ghost_list = ghost_list;
			}
			Staging::Flipbook => {
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 2.2;
					self.state.jump_time = elapsed;
					self.hero_clip_is_jump = true;
					self.hero_clip_time = 0.0;
				}
				if self.state.jump_time >= 0.0 {
					let jump_phase = ((elapsed - self.state.jump_time) / 0.7).min(1.0);
					self.scene_base.get_node_tree_mut()[hero_id].set_local_position(Vector2::create(300.0, FLOOR_Y - 60.0 - (jump_phase * std::f64::consts::PI).sin() * 140.0));
					if jump_phase >= 1.0 {
						self.state.jump_time = -1.0;
					}
				}
			}
			Staging::FloatingDamage => {
				self.state.accumulator += time_delta;
				if self.state.accumulator >= 0.32 {
					self.state.accumulator = 0.0;
					let is_critical = self.rand_value() < 0.2;
					let target_x = 560.0 + (self.rand_value() - 0.5) * 360.0;
					let target_y = FLOOR_Y - 120.0 + (self.rand_value() - 0.5) * 120.0;
					let target_position = Vector2::create(target_x, target_y);
					let text = if is_critical { format!("CRIT {}", (500.0 + self.rand_value() * 500.0).round()) } else { format!("-{}", (20.0 + self.rand_value() * 180.0).round()) };
					let floating_options = FloatingTextOptions {
						color: if is_critical { accent_color() } else { color(1.0, 1.0, 1.0, 1.0) },
						font_size: if is_critical { 40.0 } else { 24.0 },
						rise: if is_critical { 90.0 } else { 50.0 },
						duration: if is_critical { 1.1 } else { 0.8 },
						..FloatingTextOptions::default()
					};
					self.spawn_floating_text(&text, target_position, floating_options);
					if is_critical {
						self.shaker.add_shake(8.0);
						self.spawn_hit_sparks(target_position);
					}
				}
			}
			Staging::TypewriterDialogue => {
				let (line_id, caret_id) = (self.state.node_a.expect("line"), self.state.node_b.expect("caret"));
				let mut typewriter = self.state.typewriter.take().expect("typewriter");
				let node_tree = self.scene_base.get_node_tree_mut();
				if let Some(line_text) = node_tree[line_id].get_component_mut::<Text>() {
					typewriter.tick(line_text, time_delta);
				}
				let is_finished = typewriter.is_finished();
				node_tree[caret_id].set_local_opacity(if is_finished { 0.5 + 0.5 * (elapsed * 8.0).sin() } else { 0.0 });
				if is_finished {
					self.state.timer += time_delta;
					if self.state.timer > 1.6 {
						self.state.timer = 0.0;
						self.state.line_index = (self.state.line_index + 1) % TYPEWRITER_LINE_LIST.len();
						if let Some(line_text) = node_tree[line_id].get_component_mut::<Text>() {
							typewriter.start(line_text, Some(TYPEWRITER_LINE_LIST[self.state.line_index]));
						}
					}
				}
				self.state.typewriter = Some(typewriter);
			}
			Staging::SlashTrail => {
				let blade_id = self.state.node_a.expect("blade");
				let swing_phase = (elapsed - self.state.swing_start) / 0.42;
				let mut did_hit_now = false;
				{
					let node_tree = self.scene_base.get_node_tree_mut();
					if (0.0..=1.0).contains(&swing_phase) {
						let angle = -2.4 + swing_phase * 3.2;
						let radius = 130.0 + (swing_phase * std::f64::consts::PI).sin() * 60.0;
						node_tree[blade_id].set_local_position(Vector2::create(560.0 + angle.cos() * radius, 320.0 + angle.sin() * radius));
						if let Some(trail) = node_tree[blade_id].get_component_mut::<TrailRenderer>() {
							trail.set_emitting(true);
						}
						if swing_phase > 0.55 && !self.state.did_hit {
							self.state.did_hit = true;
							did_hit_now = true;
						}
					}
					else {
						if let Some(trail) = node_tree[blade_id].get_component_mut::<TrailRenderer>() {
							trail.set_emitting(false);
						}
						if swing_phase > 1.0 {
							self.state.swing_start = elapsed + 0.9;
							self.state.did_hit = false;
						}
					}
				}
				if did_hit_now {
					self.spawn_hit_sparks(Vector2::create(700.0, 300.0));
					self.shaker.add_shake(9.0);
					let bat_sprite = shader_sprite(self.scene_base.get_node_tree_mut(), subject_ids[SUBJECT_BAT]);
					bat_sprite.set_effect(ShaderSpriteEffect::Flash);
					bat_sprite.set_effect_progress(1.0);
					let damage = (60.0 + self.rand_value() * 80.0).round();
					self.spawn_floating_text(&format!("-{}", damage), Vector2::create(700.0, 240.0), SpriteFxScene::floating_options(26.0, color(1.0, 0.5, 0.4, 1.0)));
				}
				let bat_sprite = shader_sprite(self.scene_base.get_node_tree_mut(), subject_ids[SUBJECT_BAT]);
				let faded_progress = (bat_sprite.get_effect_progress() - time_delta * 5.0).max(0.0);
				bat_sprite.set_effect_progress(faded_progress);
			}
			Staging::Parallax => {
				let layer_list = self.state.layer_list.clone();
				let node_tree = self.scene_base.get_node_tree_mut();
				for (layer_id, speed, wrap) in layer_list {
					let mut x = node_tree[layer_id].get_local_position().x - speed * time_delta;
					if x < -wrap {
						x += wrap;
					}
					node_tree[layer_id].set_local_position(Vector2::create(x, 0.0));
				}
				shader_sprite(node_tree, hero_id).get_sprite_mut().set_horizontal_flip(false);
				node_tree[hero_id].set_local_position(Vector2::create(300.0, FLOOR_Y - 60.0 - (elapsed * 6.0).sin().abs() * 14.0));
				self.world_zoom = 1.04 + (elapsed * 0.6).sin() * 0.03;
			}
			Staging::CameraPunch => {
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 1.5;
					self.state.punch = 1.0;
					self.shaker.add_shake(12.0);
					let hero_position = self.get_hero_position();
					self.spawn_hit_sparks(hero_position);
				}
				self.state.punch = (self.state.punch - time_delta * 3.0).max(0.0);
				self.world_zoom = 1.0 + self.state.punch * 0.14;
				self.screen_effect.set_strength(ScreenEffectType::Chromatic, self.state.punch * 1.5);
			}
			Staging::SquashStretch => {
				if elapsed >= self.state.next_event && self.state.y >= FLOOR_Y - 60.0 {
					self.state.next_event = elapsed + 1.6;
					self.state.velocity = -680.0;
					self.state.scale_x = 0.8;
					self.state.scale_y = 1.3;
				}
				self.state.velocity += 1600.0 * time_delta;
				self.state.y += self.state.velocity * time_delta;
				if self.state.y >= FLOOR_Y - 60.0 {
					if self.state.velocity > 200.0 {
						self.state.scale_x = 1.35;
						self.state.scale_y = 0.65;
						let hero_x = self.get_hero_position().x;
						self.spawn_dust(Vector2::create(hero_x, FLOOR_Y));
					}
					self.state.y = FLOOR_Y - 60.0;
					self.state.velocity = 0.0;
				}
				self.state.scale_x += (1.0 - self.state.scale_x) * (time_delta * 9.0).min(1.0);
				self.state.scale_y += (1.0 - self.state.scale_y) * (time_delta * 9.0).min(1.0);
				let (hero_y, scale_x, scale_y) = (self.state.y, self.state.scale_x, self.state.scale_y);
				let hero_node = &mut self.scene_base.get_node_tree_mut()[hero_id];
				hero_node.set_local_position(Vector2::create(300.0, hero_y));
				hero_node.set_local_scale(Vector2::create(scale_x, scale_y));
			}
			Staging::SpawnDespawn => {
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 0.9;
					let enemy_x = 420.0 + self.rand_value() * 460.0;
					let enemy_y = 180.0 + self.rand_value() * 180.0;
					let node_tree = self.scene_base.get_node_tree_mut();
					let enemy_id = make_sprite(node_tree, &image_table, mode_root_id, "bat.png", enemy_x, enemy_y, 96.0, 72.0, 4);
					shader_sprite(node_tree, enemy_id).set_effect(ShaderSpriteEffect::PixelDissolve);
					self.state.enemy_list.push((enemy_id, 0.0));
				}
				let mut enemy_list = std::mem::take(&mut self.state.enemy_list);
				let node_tree = self.scene_base.get_node_tree_mut();
				enemy_list.retain_mut(|(enemy_id, age)| {
					*age += time_delta;
					let frame_index = (*age * 12.0).floor() as i64 % 4;
					let enemy_sprite = shader_sprite(node_tree, *enemy_id);
					enemy_sprite.get_sprite_mut().set_image_rect(Rect::create(frame_index as f64 * 64.0, 0.0, 64.0, 48.0));
					if *age < 0.6 {
						enemy_sprite.set_effect_progress(1.0 - *age / 0.6);
						let pop = 1.0 + ((*age / 0.6).min(1.0) * std::f64::consts::PI).sin() * 0.25;
						node_tree[*enemy_id].set_local_scale(Vector2::create(pop, pop));
					}
					else if *age < 2.2 {
						enemy_sprite.set_effect_progress(0.0);
						let enemy_node = &mut node_tree[*enemy_id];
						enemy_node.set_local_scale(Vector2::create(1.0, 1.0));
						let position = enemy_node.get_local_position();
						enemy_node.set_local_position(Vector2::create(position.x, position.y + (*age * 6.0).sin() * 0.8));
					}
					else if *age < 3.0 {
						if enemy_sprite.get_effect() != ShaderSpriteEffect::Dissolve {
							enemy_sprite.set_effect(ShaderSpriteEffect::Dissolve);
							enemy_sprite.set_effect_color(color(0.6, 0.4, 1.0, 1.0));
						}
						enemy_sprite.set_effect_progress((*age - 2.2) / 0.8);
					}
					else {
						node_tree.destroy_node(*enemy_id);
						return false;
					}
					true
				});
				self.state.enemy_list = enemy_list;
			}
			Staging::HealthBar => {
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 1.1;
					if self.state.health <= 0.05 {
						self.state.health = 1.0;
						self.state.ghost_health = 1.0;
					}
					else {
						self.state.health = (self.state.health - 0.12 - self.rand_value() * 0.16).max(0.0);
						self.state.ghost_delay = 0.4;
						self.shaker.add_shake(6.0);
						self.hero_knockback = 1.0;
						let damage = if self.state.health * 100.0 > 0.0 { 40.0 + self.rand_value() * 60.0 } else { 99.0 };
						let hero_position = self.get_hero_position();
						self.spawn_floating_text(&format!("-{}", damage.round()), Vector2::create(hero_position.x, hero_position.y - 70.0), SpriteFxScene::floating_options(22.0, color(1.0, 0.45, 0.45, 1.0)));
					}
				}
				self.state.ghost_delay = (self.state.ghost_delay - time_delta).max(0.0);
				if self.state.ghost_delay <= 0.0 {
					self.state.ghost_health += (self.state.health - self.state.ghost_health) * (time_delta * 4.0).min(1.0);
				}
				let (health, ghost_health) = (self.state.health, self.state.ghost_health);
				let (fill_id, ghost_id, label_id) = (self.state.node_a.expect("fill"), self.state.node_b.expect("ghost"), self.state.node_c.expect("label"));
				let is_danger = health < 0.3;
				let pulse = 0.5 + 0.5 * (elapsed * 9.0).sin();
				let node_tree = self.scene_base.get_node_tree_mut();
				node_tree[fill_id].set_content_size(Vector2::create(616.0 * health, 22.0));
				node_tree[ghost_id].set_content_size(Vector2::create(616.0 * ghost_health, 22.0));
				if let Some(fill_paint) = node_tree[fill_id].get_component_mut::<Paint>() {
					fill_paint.set_color(if is_danger { color(0.9, 0.25 + pulse * 0.2, 0.25, 1.0) } else { color(0.27, 0.71, 0.52, 1.0) });
				}
				set_label_text(node_tree, label_id, &format!("HP {} / 100", (health * 100.0).round()));
				self.screen_effect.set_strength(ScreenEffectType::Vignette, if is_danger { 0.5 + pulse * 0.4 } else { 0.0 });
				self.apply_hero_knockback(time_delta, 180.0);
			}
			Staging::Lightning => {
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 1.8 + self.rand_value() * 1.6;
					self.state.flash = 1.0;
					self.shaker.add_shake(18.0);
				}
				self.state.flash = (self.state.flash - time_delta * 3.5).max(0.0);
				self.screen_effect.set_strength(ScreenEffectType::Fade, self.state.flash * 0.9);
				self.screen_effect.set_strength(ScreenEffectType::Invert, if self.state.flash > 0.85 { 1.0 } else { 0.0 });
			}
			Staging::LevelUp => {
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 2.6;
					let hero_position = self.get_hero_position();
					let node_tree = self.scene_base.get_node_tree_mut();
					let ring_id = make_paint(node_tree, mode_root_id, hero_position.x, hero_position.y, 120.0, 120.0, color(0.83, 0.69, 0.42, 0.9), 60.0);
					self.state.ring_list.push((ring_id, 0.0));
					self.spawn_particle(json!({ "bursts": [{ "time": 0, "count": 140 }], "speed": [200, 520], "gravity": [0, 520], "lifetime": [0.8, 1.6], "renderShape": "circle", "size": [3, 7], "startColorA": [1, 0.9, 0.5, 1], "startColorB": [1, 0.7, 0.3, 1], "colorOverLifetime": [{ "time": 0, "color": [1, 1, 1, 1] }, { "time": 1, "color": [1, 0.8, 0.3, 0] }] }), hero_position.x, hero_position.y);
					self.spawn_floating_text("LEVEL UP!", Vector2::create(hero_position.x, hero_position.y - 100.0), FloatingTextOptions { color: accent_color(), font_size: 40.0, rise: 60.0, duration: 1.4, ..FloatingTextOptions::default() });
					self.shaker.add_shake(6.0);
				}
				self.grow_rings(time_delta, 0.9, 4.0);
				self.hero_sprite().set_effect_progress(0.6 + 0.4 * (elapsed * 6.0).sin());
			}
			Staging::Teleport => {
				self.state.timer += time_delta;
				if self.state.phase == 0.0 && self.state.timer > 1.2 {
					self.state.phase = 1.0;
					self.state.timer = 0.0;
				}
				if self.state.phase == 1.0 {
					let progress = (self.state.timer / 0.5).min(1.0);
					self.hero_sprite().set_effect_progress(progress);
					if self.state.timer >= 0.5 {
						self.state.phase = 2.0;
						self.state.timer = 0.0;
						self.state.side = 1 - self.state.side;
						let side_x = if self.state.side == 0 { 300.0 } else { 680.0 };
						self.scene_base.get_node_tree_mut()[hero_id].set_local_position(Vector2::create(side_x, FLOOR_Y - 60.0));
						let hero_position = self.get_hero_position();
						self.spawn_hit_sparks(hero_position);
						self.screen_effect.set_strength(ScreenEffectType::Chromatic, 2.0);
					}
				}
				else if self.state.phase == 2.0 {
					let progress = 1.0 - (self.state.timer / 0.5).min(1.0);
					self.hero_sprite().set_effect_progress(progress);
					if self.state.timer >= 0.5 {
						self.state.phase = 0.0;
						self.state.timer = 0.0;
					}
				}
				let chromatic_strength = (self.screen_effect.get_strength(ScreenEffectType::Chromatic) - time_delta * 5.0).max(0.0);
				self.screen_effect.set_strength(ScreenEffectType::Chromatic, chromatic_strength);
			}
			Staging::FreezeFrame => {
				let cycle = (elapsed % 5.0) / 5.0;
				let is_frozen = cycle > 0.3 && cycle < 0.8;
				self.set_time_scale(if is_frozen { 0.0 } else { 1.0 });
				self.state.blend += ((if is_frozen { 1.0 } else { 0.0 }) - self.state.blend) * (unscaled_delta * 5.0).min(1.0);
				let blend = self.state.blend;
				self.screen_effect.set_strength(ScreenEffectType::Letterbox, blend);
				self.screen_effect.set_strength(ScreenEffectType::Sepia, blend * 0.8);
				self.screen_effect.set_strength(ScreenEffectType::Vignette, blend * 0.8);
				let label_id = self.state.node_a.expect("label");
				self.scene_base.get_node_tree_mut()[label_id].set_local_opacity(blend);
			}
			Staging::Portal => {
				let cycle = (elapsed % 3.2) / 3.2;
				let pull = ((cycle - 0.3) / 0.6).clamp(0.0, 1.0);
				let start_position = Vector2::create(300.0, FLOOR_Y - 60.0);
				let eased = pull * pull;
				let glow_id = self.state.node_a.expect("glow");
				let node_tree = self.scene_base.get_node_tree_mut();
				let hero_node = &mut node_tree[hero_id];
				hero_node.set_local_position(Vector2::create(start_position.x + (700.0 - start_position.x) * eased, start_position.y + (300.0 - start_position.y) * eased));
				hero_node.set_local_scale(Vector2::create(1.0 - eased, 1.0 - eased));
				hero_node.set_local_rotation(eased * 720.0);
				node_tree[glow_id].set_local_scale(Vector2::create(1.0 + (elapsed * 4.0).sin() * 0.08, 1.0 + (elapsed * 4.0).cos() * 0.08));
				self.screen_effect.set_strength(ScreenEffectType::Swirl, 0.6 + pull * 0.4);
			}
			Staging::Underwater => {
				self.scene_base.get_node_tree_mut()[hero_id].set_local_position(Vector2::create(300.0, FLOOR_Y - 90.0 + (elapsed * 1.5).sin() * 24.0));
			}
			Staging::StatusEffects => {
				self.state.timer += time_delta;
				if self.state.timer > 2.4 {
					self.state.timer = 0.0;
					self.state.index = (self.state.index + 1) % STATUS_NAME_LIST.len() as i32;
					self.screen_effect.reset_all();
					if let Some(particle_id) = self.state.particle_node.take() {
						self.scene_base.get_node_tree_mut().destroy_node(particle_id);
					}
					self.scene_base.get_node_tree_mut()[hero_id].set_local_opacity(1.0);
					self.apply_status(self.state.index);
					let label_id = self.state.node_a.expect("label");
					set_label_text(self.scene_base.get_node_tree_mut(), label_id, STATUS_NAME_LIST[self.state.index as usize]);
				}
				let status_name = if self.state.index >= 0 { STATUS_NAME_LIST[self.state.index as usize] } else { "" };
				match status_name {
					"POISON" => {
						self.hero_sprite().set_effect_progress(0.5 + 0.5 * (elapsed * 6.0).sin());
					}
					"BURNING" => {
						self.hero_sprite().set_effect_progress(0.2 + 0.1 * (elapsed * 9.0).sin());
					}
					"GHOST" => {
						self.hero_sprite().set_effect_progress(1.0);
						self.scene_base.get_node_tree_mut()[hero_id].set_local_opacity(0.7 + 0.2 * (elapsed * 5.0).sin());
					}
					"INVINCIBLE" => {
						self.hero_sprite().set_effect_progress(0.6 + 0.4 * (elapsed * 12.0).sin());
					}
					"STEALTH" => {
						self.hero_sprite().set_effect_progress(0.85 + 0.15 * (elapsed * 3.0).sin());
					}
					_ => {
						self.hero_sprite().set_effect_progress(1.0);
					}
				}
			}
			Staging::Victory => {
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 1.3;
					let burst_x = 200.0 + self.rand_value() * 560.0;
					self.spawn_particle(json!({ "bursts": [{ "time": 0, "count": 120 }], "shape": "circle", "shapeRadius": 10, "lifetime": [1.6, 2.6], "speed": [200, 520], "size": [5, 9], "rotation": [0, 6.28], "angularVelocity": [2.2, 3.6], "gravity": [0, 520], "damping": 1.5, "wobble": [8, 3], "renderShape": "rect", "blendMode": "source-over", "startColorA": [1, 0.45, 0.55, 1], "startColorB": [0.4, 0.75, 1, 1], "sizeOverLifetime": [1, 1], "colorOverLifetime": [{ "time": 0, "color": [1, 1, 1, 1] }, { "time": 0.85, "color": [1, 1, 1, 1] }, { "time": 1, "color": [1, 1, 1, 0] }] }), burst_x, 120.0);
				}
				let label_id = self.state.node_a.expect("label");
				let pop = 1.0 + (elapsed * 2.4).sin().max(0.0) * 0.08;
				let node_tree = self.scene_base.get_node_tree_mut();
				shader_sprite(node_tree, subject_ids[SUBJECT_EMBLEM]).set_effect_progress((elapsed % 1.6) / 1.6);
				node_tree[label_id].set_local_scale(Vector2::create(pop, pop));
				self.world_zoom = 1.03 + (elapsed * 1.2).sin() * 0.02;
			}
			Staging::GameOver => {
				let cycle = (elapsed % 6.0) / 6.0;
				let gray = (cycle / 0.3).min(1.0);
				self.screen_effect.set_strength(ScreenEffectType::Grayscale, gray);
				self.screen_effect.set_strength(ScreenEffectType::Vignette, gray);
				self.screen_effect.set_strength(ScreenEffectType::TvOff, ((cycle - 0.7) / 0.25).clamp(0.0, 1.0));
				let label_id = self.state.node_a.expect("label");
				let node_tree = self.scene_base.get_node_tree_mut();
				node_tree[label_id].set_local_opacity(((cycle - 0.2) / 0.2).clamp(0.0, 1.0));
				node_tree[hero_id].set_local_rotation((cycle * 400.0).min(90.0));
			}
			Staging::DayNight => {
				let day_phase = 0.5 + 0.5 * (elapsed * 0.7).sin();
				self.screen_effect.set_parameters(ScreenEffectType::WhiteBalance, 0.9 - day_phase * 1.6, 0.0, 0.0, 0.0);
				self.screen_effect.set_parameters(ScreenEffectType::LiftGammaGain, -0.08 * (1.0 - day_phase), 1.0, 0.55 + day_phase * 0.6, 0.0);
				self.screen_effect.set_strength(ScreenEffectType::Fog, day_phase * 0.5);
				let stars_id = self.state.node_a.expect("stars");
				self.scene_base.get_node_tree_mut()[stars_id].set_local_opacity(1.0 - day_phase);
			}
			Staging::DamageFlash => {
				if elapsed >= self.state.next_event {
					self.state.next_event = elapsed + 1.8;
					self.state.invincible = 0.9;
					self.hero_knockback = 1.0;
					self.shaker.add_shake(8.0);
					let hero_position = self.get_hero_position();
					self.spawn_hit_sparks(hero_position);
				}
				self.state.invincible = (self.state.invincible - time_delta).max(0.0);
				let invincible = self.state.invincible;
				self.hero_sprite().set_effect_progress(((invincible - 0.7) / 0.2).max(0.0));
				self.scene_base.get_node_tree_mut()[hero_id].set_local_opacity(if invincible > 0.0 { if (elapsed * 40.0).sin() > 0.0 { 1.0 } else { 0.35 } } else { 1.0 });
				self.apply_hero_knockback(time_delta, 220.0);
			}
			Staging::RainStorm => {
				self.state.accumulator += time_delta;
				if self.state.accumulator > 0.12 {
					self.state.accumulator = 0.0;
					let ring_x = 40.0 + self.rand_value() * 880.0;
					let ring_y = FLOOR_Y + 10.0 + self.rand_value() * 80.0;
					let node_tree = self.scene_base.get_node_tree_mut();
					let ring_id = make_paint(node_tree, mode_root_id, ring_x, ring_y, 20.0, 8.0, color(0.7, 0.85, 1.0, 0.5), 4.0);
					self.state.ring_list.push((ring_id, 0.0));
				}
				self.grow_rings(time_delta, 0.6, 2.5);
			}
		}
		let _ = image_table;
	}

	//==============================================================================
	// 스프라이트 셰이더 모드와 화면 효과 모드의 갱신.
	//==============================================================================
	fn update_effect_mode(&mut self, mode_kind: ModeKind) {
		let elapsed = self.elapsed;
		match mode_kind {
			ModeKind::SpriteShader(effect) => {
				let effect_name = effect.get_name();
				let progress = if SPRITE_TRANSITION_NAMES.contains(&effect_name) {
					ping_pong(elapsed, 3.2)
				}
				else if effect_name == "shine" {
					(elapsed % 1.8) / 1.8
				}
				else if effect_name == "flash" {
					(1.0 - ((elapsed % 1.2) / 1.2) * 3.0).max(0.0)
				}
				else if effect_name == "hueShift" {
					(elapsed % 3.0) / 3.0
				}
				else if SPRITE_OSCILLATE_NAMES.contains(&effect_name) {
					0.5 + 0.5 * (elapsed * 1.8).sin()
				}
				else {
					0.85 + 0.15 * (elapsed * 3.0).sin()
				};
				self.set_subject_progress(progress);
			}
			ModeKind::Screen(effect_type) => {
				let effect_name = effect_type.get_name();
				if SCREEN_TRANSITION_NAMES.contains(&effect_name) {
					self.screen_effect.set_strength(effect_type, ping_pong(elapsed, 3.2));
				}
				else if effect_name == "shockwave" {
					self.screen_effect.set_parameters(effect_type, 0.5, 0.55, (elapsed % 1.6) / 1.6 * 1.4, 0.12);
					self.screen_effect.set_strength(effect_type, 1.0);
				}
				else if effect_name == "spotlight" {
					let hero_position = self.get_hero_stage_position();
					self.screen_effect.set_parameters(effect_type, hero_position.x, hero_position.y, 0.16, 0.14);
				}
				else if effect_name == "godRays" {
					self.screen_effect.set_parameters(effect_type, 0.5, 0.2, 0.55, 0.94);
					self.screen_effect.set_strength(effect_type, 0.7 + 0.3 * (elapsed * 1.5).sin());
				}
				else if effect_name == "letterbox" {
					self.screen_effect.set_strength(effect_type, ping_pong(elapsed, 4.0));
				}
				else if SCREEN_OSCILLATE_NAMES.contains(&effect_name) {
					self.screen_effect.set_strength(effect_type, 0.5 + 0.5 * (elapsed * 1.6).sin());
				}
				else {
					self.screen_effect.set_strength(effect_type, 0.85 + 0.15 * (elapsed * 2.2).sin());
				}
			}
			ModeKind::Staging(_) => {}
		}
	}

	//==============================================================================
	// 주인공 시트 애니메이션. (대기 8칸 초당 10, 점프 7칸 초당 14 한 번 후 대기로)
	//==============================================================================
	fn animate_hero(&mut self, time_delta: f64) {
		self.hero_clip_time += time_delta;
		let frame_index = if self.hero_clip_is_jump {
			let jump_frame_list = [2, 3, 4, 5, 6, 7, 0];
			let jump_index = (self.hero_clip_time * 14.0).floor() as usize;
			if jump_index >= jump_frame_list.len() {
				self.hero_clip_is_jump = false;
				self.hero_clip_time = 0.0;
				0
			}
			else {
				jump_frame_list[jump_index]
			}
		}
		else {
			(self.hero_clip_time * 10.0).floor() as usize % 8
		};
		let hero_id = self.hero();
		shader_sprite(self.scene_base.get_node_tree_mut(), hero_id).get_sprite_mut().set_image_rect(Rect::create(frame_index as f64 * 64.0, 0.0, 64.0, 64.0));
	}

	//==============================================================================
	// 화면 효과 적용 영역 = 무대 사각형. (뷰 좌표에서 캔버스 픽셀로, 둥근 모서리 포함)
	//==============================================================================
	fn apply_stage_region(&mut self, engine: &Engine) {
		let view_manager = engine.get_view_manager();
		let view_native_rect = view_manager.get_view_native_rect();
		let target_resolution_scale = view_manager.get_target_resolution_scale();
		let device_pixel_ratio = view_manager.get_device_pixel_ratio();
		let pixel_scale = target_resolution_scale * device_pixel_ratio;
		self.screen_effect.set_region(
			view_native_rect.position.x * device_pixel_ratio + STAGE_X * pixel_scale,
			view_native_rect.position.y * device_pixel_ratio + STAGE_Y * pixel_scale,
			STAGE_WIDTH * pixel_scale,
			STAGE_HEIGHT * pixel_scale,
			14.0 * pixel_scale,
		);
	}
}

impl Scene for SpriteFxScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (그림 일곱 장을 요청해 맡긴다)
	//==============================================================================
	fn load(&mut self, engine: &Engine) {
		for image_name in IMAGE_NAME_LIST.iter() {
			let mut image_asset = ImageAsset::new();
			image_asset.load(engine, image_name);
			self.scene_base.track_asset(&image_asset);
			self.image_asset_list.push((image_name.to_string(), image_asset));
		}
	}

	//==============================================================================
	// 초기화. (제목, 목록, 무대, 공용 세계, 처음 모드)
	//==============================================================================
	fn initialize(&mut self, engine: &Engine) {
		let _ = engine;
		self.scene_base.set_scene_background_color("rgb(20, 23, 30)");
		for (image_name, image_asset) in self.image_asset_list.iter_mut() {
			if let Some(image) = image_asset.get_image() {
				self.image_table.insert(image_name.clone(), image);
			}
		}
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(REFERENCE_WIDTH, REFERENCE_HEIGHT));
		self.build_header(root_id);
		self.build_list(root_id);
		self.build_stage(root_id);
		self.build_world();
		let requested_mode = std::env::var("VANILLA_EXAMPLE_MODE").ok().and_then(|requested_key| self.mode_list.iter().position(|definition| definition.key == requested_key)).unwrap_or(0);
		self.select_mode(requested_mode);
	}

	//==============================================================================
	// 갱신. (모드 갱신, 공용 피사체 애니메이션, 흔들림, 화면 효과)
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		let engine = Engine::get_engine().unwrap();
		if let Some(requested_index) = self.requested_mode.replace(None) {
			self.select_mode(requested_index);
		}
		let unscaled_delta = engine.get_time_manager().get_unscale_delta_time();
		self.elapsed += unscaled_delta;
		let mode_kind = self.mode_list[self.mode_index].kind;
		match mode_kind {
			ModeKind::Staging(staging) => {
				self.update_staging(staging, time_delta, unscaled_delta);
			}
			_ => {
				self.update_effect_mode(mode_kind);
			}
		}
		self.prune_particles();
		self.update_list_culling();
		self.animate_hero(time_delta);
		// 공용 피사체 애니메이션.
		let elapsed = self.elapsed;
		let subject_ids = self.subject_ids.clone();
		let is_slash_trail = mode_kind == ModeKind::Staging(Staging::SlashTrail);
		let node_tree = self.scene_base.get_node_tree_mut();
		let coin_frame = (elapsed * 12.0).floor() as i64 % 8;
		shader_sprite(node_tree, subject_ids[SUBJECT_COIN]).get_sprite_mut().set_image_rect(Rect::create(coin_frame as f64 * 48.0, 0.0, 48.0, 48.0));
		let bat_frame = (elapsed * 10.0).floor() as i64 % 4;
		shader_sprite(node_tree, subject_ids[SUBJECT_BAT]).get_sprite_mut().set_image_rect(Rect::create(bat_frame as f64 * 64.0, 0.0, 64.0, 48.0));
		if !is_slash_trail {
			node_tree[subject_ids[SUBJECT_BAT]].set_local_position(Vector2::create(800.0 + (elapsed * 1.3).sin() * 60.0, 220.0 + (elapsed * 2.6).sin() * 30.0));
		}
		node_tree[subject_ids[SUBJECT_STAR]].set_local_rotation((elapsed * 1.5).sin() * 12.0);
		node_tree[subject_ids[SUBJECT_GEM]].set_local_position(Vector2::create(800.0, FLOOR_Y - 34.0 - (elapsed * 2.0).sin().abs() * 10.0));
		self.shaker.tick(unscaled_delta);
		let shake_offset = self.shaker.get_offset();
		let world_id = self.world_id.expect("world");
		let world_zoom = self.world_zoom;
		let node_tree = self.scene_base.get_node_tree_mut();
		node_tree[world_id].set_local_position(Vector2::create(STAGE_WIDTH / 2.0 + shake_offset.x, STAGE_HEIGHT / 2.0 + shake_offset.y));
		node_tree[world_id].set_local_scale(Vector2::create(world_zoom, world_zoom));
		self.screen_effect.tick(unscaled_delta);
		self.average_delta = if self.average_delta == 0.0 { unscaled_delta } else { self.average_delta * 0.95 + unscaled_delta * 0.05 };
		if self.average_delta > 0.0 {
			let fps_text = format!("fps  {}", (1.0 / self.average_delta).round() as i64);
			let fps_label_id = self.fps_label_id.expect("fps");
			set_label_text(self.scene_base.get_node_tree_mut(), fps_label_id, &fps_text);
		}
		self.scene_base.get_node_tree_mut().tick(engine, time_delta);
		self.touch_recognizer.tick_touch(self.scene_base.get_node_tree_mut(), engine);
	}

	//==============================================================================
	// 이전 출력. (영역을 정하고 효과 시작, 배경)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		self.apply_stage_region(engine);
		self.screen_effect.begin(graphic);
		self.scene_base.draw_scene_background(graphic);
	}

	//==============================================================================
	// 이후 출력. (화면을 떠 와 효과를 건다)
	//==============================================================================
	fn post_draw(&mut self, graphic: &mut Graphic) {
		self.screen_effect.end(graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("스프라이트 이펙트 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(REFERENCE_WIDTH, REFERENCE_HEIGHT);
	engine_configuration.window_size = Vector2::create(REFERENCE_WIDTH, REFERENCE_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	engine_configuration.maximum_frame_per_second = 60.0;
	engine_configuration.asset_directory_paths = vec![PathBuf::from(ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(SpriteFxScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}

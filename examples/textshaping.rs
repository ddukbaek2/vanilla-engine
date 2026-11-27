//==============================================================================
// 글자 모양 짓기 예제. (오른쪽에서 왼쪽 글, 아랍어 글자 잇기, 가짜 기울임과 굵게, 낱말 줄바꿈, 글자 아틀라스)
// - 아랍어는 오른쪽에서 왼쪽 문단(set_text_direction(Rtl))으로, 섞인 글은 Auto 로 그린다. 정렬 Start 는 오른쪽에서 왼쪽 문단에서 오른쪽이다.
// - 아랍어 글꼴은 시스템 글꼴 가운데 있는 것을 FontAsset 으로 올린다. (맥 SFArabic, GeezaPro, 윈도우 arial, 리눅스 DejaVuSans)
//   웹은 페이지 옆 fonts/arabic.ttf 를 받는다. 없으면 기본 글꼴에 없는 글자는 빈 칸으로 보인다.
// - 기울임과 굵게는 기본 글꼴에 기울인 얼굴, 굵은 얼굴이 없으면 브라우저와 같은 가짜 기울임(1/4), 가짜 굵게로 그린다.
// - 줄바꿈은 공백이 없는 한중일 글도 낱말 사이에서 나눈다. 표시 글자 수는 글자 묶음 단위다.
// - DynamicFont 는 숫자를 아틀라스 한 장에 모아 그린다. (글이 바뀌어도 다시 굽지 않는다)
// - 색 이모지: 데스크탑은 대체 글꼴의 색 글리프(맥 Apple Color Emoji 의 sbix 등)를, 웹은 브라우저가 그린 것을 제 색으로 굽는다.
//   ❤️ (FE0F) 는 색, 맨 ❤ 는 글 글꼴로, 국기, 피부색, 이음 문자 가족, 키캡, 그림자 글, 가짜 기울임, 글자 아틀라스의 ⭐ 를 보인다.
// - 실행: cargo run --example textshaping
//==============================================================================
use std::path::PathBuf;
use vanilla::{Component, ComponentBase, ComponentContext, DynamicFont, Engine, EngineConfiguration, FontAsset, Graphic, NodeId, NodeTransform, NodeTree, Pivot, RichText, Scene, SceneBase, ShadowText, Text, TextAlign, TextBaseline, TextDirection, Vector2};

const EXAMPLE_ASSET_DIRECTORY: &str = "examples/assets";
const VIEW_WIDTH: f64 = 1280.0;
const VIEW_HEIGHT: f64 = 720.0;
const BACKGROUND_COLOR: &str = "#151821";
const TEXT_COLOR: &str = "#e8ebf2";
const CAPTION_COLOR: &str = "#9aa2b2";
const ARABIC_FONT_FAMILY: &str = "ArabicExample";

// 아랍어 글꼴 후보. (앞에서부터 있는 첫 파일, 웹은 페이지 옆 파일)
#[cfg(not(target_arch = "wasm32"))]
const ARABIC_FONT_PATHS: [&str; 4] = ["/System/Library/Fonts/SFArabic.ttf", "/System/Library/Fonts/GeezaPro.ttc", "C:/Windows/Fonts/arial.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"];
#[cfg(target_arch = "wasm32")]
const ARABIC_FONT_PATHS: [&str; 1] = ["fonts/arabic.ttf"];

//==============================================================================
// 상자 노드 만들기. (왼쪽 위 피봇, set_rect)
//==============================================================================
fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, x: f64, y: f64, width: f64, height: f64) -> NodeId {
	let node_id = node_tree.create_child(parent_id, name);
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_rect(x, y, width, height);
	node_id
}

//==============================================================================
// 설명 글 노드 만들기.
//==============================================================================
fn create_caption_node(node_tree: &mut NodeTree, parent_id: NodeId, x: f64, y: f64, text: &str) {
	let caption_id = create_box_node(node_tree, parent_id, "caption", x, y, 560.0, 24.0);
	let caption_text = node_tree[caption_id].add_component(Text::create_with_text(text, 16.0));
	caption_text.set_text_color(CAPTION_COLOR);
	caption_text.set_text_align(TextAlign::Left);
}

//==============================================================================
// 글자 아틀라스로 숫자를 그리는 컴포넌트. (처음 그릴 때 아틀라스를 굽는다)
//==============================================================================
struct DynamicFontCounter {
	component_base: ComponentBase,
	dynamic_font: DynamicFont,
	elapsed_seconds: f64,
}

#[vanilla::reflect]
impl DynamicFontCounter {}

#[vanilla::reflect]
impl Component for DynamicFontCounter {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"DynamicFontCounter"
	}

	fn tick(&mut self, _context: &mut ComponentContext, time_delta: f64) {
		self.elapsed_seconds += time_delta;
	}

	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		self.dynamic_font.build_atlas(graphic, "0123456789:.\u{2B50}");
		let counter_text = format!("\u{2B50}{:08.2}", 1234.5 + self.elapsed_seconds);
		graphic.set_image_tint_color(None);
		self.dynamic_font.draw_text(&counter_text, Vector2::create(0.0, 32.0), graphic);
	}
}

//==============================================================================
// 첫 씬.
//==============================================================================
struct MainScene {
	scene_base: SceneBase,
	font_asset: FontAsset,
}

impl Scene for MainScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성.
	//==============================================================================
	fn create(&mut self, engine: &Engine) {
		self.scene_base.set_scene_background_color(BACKGROUND_COLOR);
		#[cfg(not(target_arch = "wasm32"))]
		let arabic_font_path = ARABIC_FONT_PATHS.iter().find(|font_path| std::path::Path::new(font_path).exists()).copied();
		#[cfg(target_arch = "wasm32")]
		let arabic_font_path = ARABIC_FONT_PATHS.first().copied();
		if let Some(arabic_font_path) = arabic_font_path {
			self.font_asset.load_font(engine, ARABIC_FONT_FAMILY, arabic_font_path);
		}
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		let title_id = create_box_node(node_tree, root_id, "title", 40.0, 24.0, 900.0, 50.0);
		let title_text = node_tree[title_id].add_component(Text::create_with_text("글자 모양 짓기", 34.0));
		title_text.set_text_color(TEXT_COLOR);
		title_text.set_text_align(TextAlign::Left);

		// 오른쪽에서 왼쪽 글. (상자 오른쪽에 붙는다)
		create_caption_node(node_tree, root_id, 40.0, 84.0, "Text Rtl + Start, Auto, RichText Rtl");
		let arabic_lines: [(&str, TextDirection); 3] = [("مرحبا بالعالم 123", TextDirection::Rtl), ("Hello مرحبا world", TextDirection::Auto), ("سلام عليكم 2024/11", TextDirection::Rtl)];
		for (line_index, (text, text_direction)) in arabic_lines.iter().enumerate() {
			let line_id = create_box_node(node_tree, root_id, "arabic", 40.0, 114.0 + line_index as f64 * 46.0, 520.0, 40.0);
			let line_text = node_tree[line_id].add_component(Text::create_with_text(text, 30.0));
			line_text.set_font_family(ARABIC_FONT_FAMILY);
			line_text.set_text_color(TEXT_COLOR);
			line_text.set_text_align(TextAlign::Start);
			line_text.set_text_direction(*text_direction);
		}
		let rich_id = create_box_node(node_tree, root_id, "richArabic", 40.0, 254.0, 520.0, 40.0);
		let rich_text = node_tree[rich_id].add_component(RichText::create_with_text("<b>مرحبا</b> <color=#ffd166>بالعالم</color> <u>نص</u>", 30.0));
		rich_text.set_font_family(ARABIC_FONT_FAMILY);
		rich_text.set_text_color(TEXT_COLOR);
		rich_text.set_text_align(TextAlign::Start);
		rich_text.set_text_direction(TextDirection::Rtl);

		// 기울임과 굵게. (기본 글꼴에 없는 얼굴은 가짜로)
		create_caption_node(node_tree, root_id, 40.0, 310.0, "italic, bold, ShadowText italic + underline + strikethrough");
		let style_list: [(&str, bool, bool); 3] = [("보통 Regular AVA", false, false), ("기울임 Italic AVA", false, true), ("굵게 Bold AVA", true, false)];
		for (style_index, (text, is_bold, is_italic)) in style_list.iter().enumerate() {
			let style_id = create_box_node(node_tree, root_id, "style", 40.0, 340.0 + style_index as f64 * 40.0, 520.0, 36.0);
			let style_text = node_tree[style_id].add_component(Text::create_with_text(text, 28.0));
			style_text.set_text_color(TEXT_COLOR);
			style_text.set_text_align(TextAlign::Left);
			style_text.set_bold(*is_bold);
			style_text.set_italic(*is_italic);
		}
		let shadow_id = create_box_node(node_tree, root_id, "shadow", 40.0, 466.0, 520.0, 40.0);
		let shadow_text = node_tree[shadow_id].add_component(ShadowText::create_with_text("그림자 글 Shadow", 28.0));
		shadow_text.set_text_style(28.0, false, TEXT_COLOR);
		shadow_text.set_text_align(TextAlign::Left);
		shadow_text.set_italic(true);
		shadow_text.set_underline(true);
		shadow_text.set_strikethrough(true);
		shadow_text.set_shadow("#000000", 1.0, 3.0, 2.0);

		// 낱말 줄바꿈과 표시 글자 수.
		create_caption_node(node_tree, root_id, 640.0, 84.0, "word wrap 260 (한중일 낱말 사이), visible 12 (글자 묶음)");
		let wrap_list = ["Vanilla엔진은 JavaScript와 Rust로 만든 게임engine입니다.", "我们今天去公园玩吧，天气非常好，大家都很开心。", "今日はいい天気ですね。明日も晴れるといいですね。"];
		for (wrap_index, text) in wrap_list.iter().enumerate() {
			let wrap_id = create_box_node(node_tree, root_id, "wrap", 640.0, 114.0 + wrap_index as f64 * 110.0, 260.0, 100.0);
			let wrap_text = node_tree[wrap_id].add_component(Text::create_with_text(text, 22.0));
			wrap_text.set_text_color(TEXT_COLOR);
			wrap_text.set_text_align(TextAlign::Left);
			wrap_text.set_text_baseline(TextBaseline::Top);
			wrap_text.set_word_wrap_width(260.0);
		}
		let visible_id = create_box_node(node_tree, root_id, "visible", 940.0, 114.0, 300.0, 100.0);
		let visible_text = node_tree[visible_id].add_component(Text::create_with_text("e\u{301}가나다라마바사아자차카타파하", 22.0));
		visible_text.set_text_color("#ffd166");
		visible_text.set_text_align(TextAlign::Left);
		visible_text.set_text_baseline(TextBaseline::Top);
		visible_text.set_word_wrap_width(150.0);
		visible_text.set_visible_character_count(12);

		// 색 이모지. (글 색은 흰색, 구운 색에 글 색이 곱해진다)
		create_caption_node(node_tree, root_id, 40.0, 520.0, "color emoji: Text, ShadowText, italic");
		let emoji_id = create_box_node(node_tree, root_id, "emoji", 40.0, 550.0, 560.0, 40.0);
		let emoji_text = node_tree[emoji_id].add_component(Text::create_with_text("이모지 \u{1F600}\u{1F44D}\u{1F3FD} \u{2764}\u{FE0F}\u{2764} \u{1F1F0}\u{1F1F7} \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} 1\u{FE0F}\u{20E3}", 30.0));
		emoji_text.set_text_color("#ffffff");
		emoji_text.set_text_align(TextAlign::Left);
		let emoji_shadow_id = create_box_node(node_tree, root_id, "emojiShadow", 40.0, 600.0, 560.0, 40.0);
		let emoji_shadow_text = node_tree[emoji_shadow_id].add_component(ShadowText::create_with_text("그림자 \u{1F389}\u{1F680}\u{1F308}", 30.0));
		emoji_shadow_text.set_text_style(30.0, false, "#ffffff");
		emoji_shadow_text.set_text_align(TextAlign::Left);
		emoji_shadow_text.set_shadow("#000000", 1.0, 3.0, 2.0);
		let emoji_italic_id = create_box_node(node_tree, root_id, "emojiItalic", 40.0, 650.0, 560.0, 40.0);
		let emoji_italic_text = node_tree[emoji_italic_id].add_component(Text::create_with_text("기울임 \u{1F600}\u{1F431}", 30.0));
		emoji_italic_text.set_text_color("#ffffff");
		emoji_italic_text.set_text_align(TextAlign::Left);
		emoji_italic_text.set_italic(true);

		// 글자 아틀라스.
		create_caption_node(node_tree, root_id, 640.0, 466.0, "DynamicFont (숫자와 \u{2B50} 아틀라스 한 장)");
		let counter_id = create_box_node(node_tree, root_id, "counter", 640.0, 496.0, 400.0, 48.0);
		let mut dynamic_font = DynamicFont::new();
		dynamic_font.set_font_size(36.0);
		node_tree[counter_id].add_component(DynamicFontCounter {
			component_base: ComponentBase::new(),
			dynamic_font,
			elapsed_seconds: 0.0,
		});
	}

	//==============================================================================
	// 이전 출력. (글을 1:1 로 굽고 배경을 칠한다)
	//==============================================================================
	fn pre_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		graphic.set_text_bake_scale_step(0.0);
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("글자 모양 짓기 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	let main_scene = MainScene {
		scene_base: SceneBase::new(),
		font_asset: FontAsset::new(),
	};
	if let Err(error) = engine.run(Box::new(main_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}

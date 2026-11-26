# 그리기

그리는 길은 둘입니다. 노드에 그리기 컴포넌트(`Paint`, `Sprite`, `Text`, `RichText` 와 꾸밈 컴포넌트)를 붙이면 노드 트리를 그릴 때 엔진이 함께 그립니다. 씬의 `pre_draw`, `draw`, `post_draw` 나 컴포넌트의 `draw` 안에서는 `Graphic` 으로 바로 그립니다. 이 장은 두 길과 블렌드 모드, 화면을 비추는 `Camera2D` 를 봅니다.

## Graphic 으로 바로 그리기

`Graphic` 은 OpenGL 3.3(데스크탑)과 WebGL2(웹) 위에 Canvas2D 와 같은 꼴(왼쪽 위 원점, 픽셀 단위, 상태 스택)을 만든 렌더러입니다. 씬의 그리기 함수가 `&mut Graphic` 으로 받습니다. 변환, 전역 알파, 블렌드 모드는 매 프레임 처음에 되돌아가므로 뷰 좌표로 그리려면 `pre_draw` 에서 `scene_base.draw_scene_background(engine, graphic)` 를 부릅니다. (배경을 칠하고 뷰 좌표계를 겁니다, "엔진과 씬 생명 주기" 장)

| 갈래 | 함수 |
| --- | --- |
| 색 | `set_fill_color(color)`, `set_stroke_color(color)`. `Color` 또는 `"#rgb"`, `"#rgba"`, `"#rrggbb"`, `"#rrggbbaa"`, `"rgb(...)"`, `"rgba(...)"` 글 |
| 상태 | `push_state()`, `pop_state()` (변환, 전역 알파, 블렌드, 색, 틴트, 글꼴, 글 정렬을 쌓고 되돌림), `translate(x, y)`, `rotate(radian)`, `scale(x, y)`, `set_global_alpha(value)`, `multiply_global_alpha(value)` |
| 채운 도형 | `draw_rect(rect)`, `draw_round_rect(rect, round_size)`, `draw_circle(center, radius)`, `draw_ring(center, inner_radius, outer_radius, start_radian, end_radian)`, `draw_convex_polygon(positions, color)` |
| 선 | `draw_line(positions, size)`, `draw_line_loop(positions, size)`, `draw_stroke_rect(rect, line_width)`, `draw_stroke_round_rect(rect, round_size, line_width)`, `draw_stroke_circle(center, radius, line_width)`, `draw_arc(center, radius, start_radian, end_radian, line_width)`, `draw_line_segments(position_values, size)` |
| 그림 | `draw_image(image, position, size)`, `draw_image_with_image_rect(image, position, size, image_rect)`, `draw_image_with_nine_slice(image, position, size, nine_slice)` |
| 글 | `set_font_string(font_string)`, `set_text_align(text_align)`, `set_text_baseline(text_baseline)`, `draw_fill_text(text, x, y)`, `draw_stroke_text(text, x, y, line_width)`, `measure_text(text)` |

채운 도형과 `draw_fill_text` 는 채움 색을, 선과 `draw_stroke_text` 는 선 색을 씁니다. `draw_convex_polygon` 만 색을 인자로 받습니다. 각도는 라디안이고 0 이 오른쪽, 늘수록 화면에서 시계 방향입니다. 그림 인자는 `&Rc<Image>` 입니다.

```rust,no_run
use vanilla::{math, Color, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, TextAlign, TextBaseline, Vector2};

struct ShapeScene {
	scene_base: SceneBase,
}

impl Scene for ShapeScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		graphic.set_fill_color("#2a3550");
		graphic.draw_round_rect(Rect::create(40.0, 40.0, 360.0, 220.0), 24.0);

		graphic.set_fill_color(Color::create(1.0, 0.8, 0.2, 1.0));
		graphic.draw_circle(Vector2::create(560.0, 150.0), 80.0);
		graphic.draw_ring(Vector2::create(800.0, 150.0), 50.0, 80.0, -math::PI / 2.0, math::PI);

		graphic.set_stroke_color("#66ddff");
		graphic.draw_line(&[Vector2::create(40.0, 320.0), Vector2::create(240.0, 420.0), Vector2::create(440.0, 320.0)], 6.0);
		graphic.draw_stroke_rect(Rect::create(520.0, 300.0, 160.0, 120.0), 3.0);

		let triangle = [Vector2::create(800.0, 300.0), Vector2::create(900.0, 460.0), Vector2::create(700.0, 460.0)];
		graphic.draw_convex_polygon(&triangle, Color::create(0.6, 0.4, 1.0, 1.0));

		graphic.push_state();
		graphic.translate(1040.0, 380.0);
		graphic.rotate(math::PI / 8.0);
		graphic.set_global_alpha(0.6);
		graphic.set_fill_color("#ff6688");
		graphic.draw_rect(Rect::create(-80.0, -50.0, 160.0, 100.0));
		graphic.pop_state();

		graphic.set_fill_color("#ffffff");
		graphic.set_font_string("32px sans-serif");
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Top);
		graphic.draw_fill_text("Hello, Graphic", 40.0, 520.0);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let shape_scene = ShapeScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(shape_scene)) {
		eprintln!("{}", error);
	}
}
```

씬의 기본 `draw` 가 노드 트리를 그립니다. `draw` 를 덮어쓰면 위처럼 `self.scene_base.draw(graphic)` 를 불러야 노드도 그려집니다.

컴포넌트의 `draw(transform, graphic)` 안에서도 같은 함수를 씁니다. 그때 원점은 노드 내용 영역의 왼쪽 위이고 노드의 자리, 회전, 크기, 투명도가 이미 걸려 있습니다. ("노드와 컴포넌트" 장)

## 그리기 컴포넌트

| 컴포넌트 | 그리는 것 |
| --- | --- |
| `Paint` | 내용 크기만큼 한 색 사각형 (`set_color`, `set_round_size`, 처음 색 흰색) |
| `Sprite` | 그림. 그림이 없으면 `Paint` 처럼 칠합니다 (처음 색은 투명) |
| `Text` | 한 글꼴, 한 크기, 한 색의 글 (여러 줄 가능) |
| `RichText` | 마크업으로 조각마다 꼴을 바꾼 글 |

컴포넌트는 노드의 내용 크기 안에 그립니다. 노드에 내용 크기를 주지 않으면(0, 0) 아무것도 보이지 않습니다. 원본의 `Sprite` 는 `Paint` 를, `RichText` 는 `Text` 를 이어받지만, 러스트 판은 품고 있는 꼴이라 서로 다른 타입입니다. (`RichText::get_text_component_mut()` 로 품은 `Text` 를 꺼낼 수 있습니다)

## Sprite

```rust,no_run
use std::path::PathBuf;
use vanilla::{Color, Engine, EngineConfiguration, Graphic, ImageAsset, Rect, Scene, SceneBase, Sprite, SpriteDrawMode, Vector2};

struct SpriteScene {
	scene_base: SceneBase,
	image_asset: ImageAsset,
}

impl Scene for SpriteScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.image_asset.load(engine, "atlas/icons.png");
		self.scene_base.track_asset(&self.image_asset);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let image = match self.image_asset.get_image() {
			Some(image) => {
				image
			}
			None => {
				return;
			}
		};
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();

		let whole_id = node_tree.create_child(root_id, "whole");
		node_tree[whole_id].set_content_size(Vector2::create(256.0, 256.0));
		node_tree[whole_id].set_local_position(Vector2::create(220.0, 360.0));
		node_tree[whole_id].add_component(Sprite::create_with_image(image.clone()));

		let frame_id = node_tree.create_child(root_id, "frame");
		node_tree[frame_id].set_content_size(Vector2::create(192.0, 192.0));
		node_tree[frame_id].set_local_position(Vector2::create(560.0, 360.0));
		let frame_sprite = node_tree[frame_id].add_component(Sprite::create_with_image(image.clone()));
		frame_sprite.set_image_rect(Rect::create(63.0, 2.0, 48.0, 48.0));

		let tint_id = node_tree.create_child(root_id, "tint");
		node_tree[tint_id].set_content_size(Vector2::create(160.0, 144.0));
		node_tree[tint_id].set_local_position(Vector2::create(800.0, 360.0));
		let tint_sprite = node_tree[tint_id].add_component(Sprite::create_with_image(image.clone()));
		tint_sprite.set_image_rect(Rect::create(39.0, 55.0, 40.0, 36.0));
		tint_sprite.set_color(Color::create(1.0, 0.2, 0.2, 0.5));

		let tiled_id = node_tree.create_child(root_id, "tiled");
		node_tree[tiled_id].set_content_size(Vector2::create(180.0, 400.0));
		node_tree[tiled_id].set_local_position(Vector2::create(1080.0, 360.0));
		let tiled_sprite = node_tree[tiled_id].add_component(Sprite::create_with_image(image));
		tiled_sprite.set_image_rect(Rect::create(84.0, 55.0, 30.0, 30.0));
		tiled_sprite.set_sprite_draw_mode(SpriteDrawMode::Tiled);
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.asset_directory_paths = vec![PathBuf::from("examples/assets")];
	let engine = Engine::new(engine_configuration);
	let sprite_scene = SpriteScene {
		scene_base: SceneBase::new(),
		image_asset: ImageAsset::new(),
	};
	if let Err(error) = engine.run(Box::new(sprite_scene)) {
		eprintln!("{}", error);
	}
}
```

`examples/assets/atlas/icons.png` 는 저장소에 든 128 x 128 아이콘 시트입니다. 그림은 `create` 에서 받기를 청하고 `track_asset` 으로 맡기면, 다 받은 뒤에 불리는 `initialize` 에서 `get_image()` 로 꺼냅니다. ("애셋" 장)

- `Sprite::create_with_image(image)` 와 `set_image(Some(image))` 는 `Rc<Image>` 를 받습니다. 그림은 노드의 내용 크기로 늘여 그립니다. `set_image(None)` 이면 색 사각형을 칠합니다.
- `set_image_rect(rect)` 는 그림 안의 한 조각만 그립니다. (픽셀, 그림 왼쪽 위 기준) `Rect::zero()` 면 그림 전체입니다. `set_frame(Some(&frame))` 은 `Frame`(그림과 영역)을 한 번에 겁니다. (`SpriteAtlas` 는 "애셋" 장)
- `set_sprite_draw_mode(mode)`: `SpriteDrawMode::Simple`(처음 값, 늘이기), `SpriteDrawMode::Sliced`(나인 슬라이스), `SpriteDrawMode::Tiled`(조각을 제 크기대로 되풀이해 채움, 넘치는 곳은 자름).
- `Sliced` 는 `set_nine_slice(rect)` 로 테두리 두께를 줍니다. 사각형의 `x` 가 왼쪽, `y` 가 위, `width` 가 오른쪽, `height` 가 아래 두께입니다. `Sliced` 는 그림 전체를 쓰고 그림 영역은 보지 않습니다. 내용 크기가 테두리 합보다 작으면 그리지 않습니다.
- `set_color(color)` 의 알파가 0 보다 크면 그림의 모양(불투명한 곳) 안에만 그 색을 알파 비율로 덮습니다.
- `set_horizontal_flip`, `set_vertical_flip` 은 내용 영역의 왼쪽 끝(위쪽 끝)을 축으로 뒤집어, 그림이 내용 영역의 왼쪽(위쪽) 바깥에 그려집니다. 제자리에서 뒤집으려면 노드의 로컬 크기 배율을 음수로 줍니다. (`set_local_scale_xy(-1.0, 1.0)`, 기준점을 축으로 뒤집힙니다)

```rust
use vanilla::{BlendMode, Image, Rect, Sprite, SpriteDrawMode, Vector2};

let image = Image::create_from_pixels(48, 48, vec![255u8; 48 * 48 * 4]);
let mut sprite = Sprite::create_with_image(image);
assert!(sprite.get_image_rect().is_zero());
assert_eq!(sprite.get_sprite_draw_mode(), SpriteDrawMode::Simple);
assert_eq!(sprite.get_sprite_blend_mode(), BlendMode::Normal);

sprite.set_sprite_draw_mode(SpriteDrawMode::Sliced);
sprite.set_nine_slice(Rect::create(16.0, 16.0, 16.0, 16.0));
assert_eq!(sprite.get_nine_slice().size.x, 16.0);

sprite.set_horizontal_flip(true);
assert_eq!(sprite.get_flip(), Vector2::create(-1.0, 1.0));

let mut plain = Sprite::new();
plain.set_color("#44aa66");
plain.set_round_size(8.0);
assert!(plain.get_image().is_none());
```

## Text

```rust,no_run
use vanilla::{Color, Engine, EngineConfiguration, Graphic, Paint, RichText, Scene, SceneBase, Text, TextAlign, TextBaseline, Vector2};

struct TextScene {
	scene_base: SceneBase,
}

impl Scene for TextScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();

		let title_id = node_tree.create_child(root_id, "title");
		node_tree[title_id].set_content_size(Vector2::create(800.0, 80.0));
		node_tree[title_id].set_local_position(Vector2::create(640.0, 100.0));
		let title = node_tree[title_id].add_component(Text::create_with_text("vanilla-engine", 64.0));
		title.set_bold(true);
		title.set_text_color("#ffe08a");
		title.set_stroke_color("#402000");
		title.set_stroke_width(4.0);

		let body_id = node_tree.create_child(root_id, "body");
		node_tree[body_id].set_content_size(Vector2::create(520.0, 200.0));
		node_tree[body_id].set_local_position(Vector2::create(400.0, 330.0));
		node_tree[body_id].add_component(Paint::create_with_color(Color::create(0.0, 0.0, 0.0, 0.4)));
		let body = node_tree[body_id].add_component(Text::create_with_text("Text wraps inside the node width when a word wrap width is set or the text has a line break.", 26.0));
		body.set_text_align(TextAlign::Left);
		body.set_text_baseline(TextBaseline::Top);
		body.set_word_wrap_width(500.0);
		body.set_line_spacing(1.4);

		let rich_id = node_tree.create_child(root_id, "rich");
		node_tree[rich_id].set_content_size(Vector2::create(600.0, 60.0));
		node_tree[rich_id].set_local_position(Vector2::create(960.0, 330.0));
		let rich = node_tree[rich_id].add_component(RichText::new());
		rich.set_font_size(30.0);
		rich.set_text("<b>Bold</b>, <color=#ff6666>red</color>, <size=40>big</size> and <u>underline</u>");

		let button_id = node_tree.create_child(root_id, "button");
		node_tree[button_id].set_content_size(Vector2::create(200.0, 60.0));
		node_tree[button_id].set_local_position(Vector2::create(960.0, 480.0));
		let button_paint = node_tree[button_id].add_component(Paint::create_with_color("#3366cc"));
		button_paint.set_round_size(12.0);
		let button_text = node_tree[button_id].add_component(Text::create_with_text("A very long button caption", 28.0));
		button_text.set_fit_width(180.0, 0.55);
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let text_scene = TextScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(text_scene)) {
		eprintln!("{}", error);
	}
}
```

`Text` 의 처음 값은 크기 32, 흰색, 가운데 정렬(`TextAlign::Center`), 가운데 기준선(`TextBaseline::Middle`), 외곽선 두께 0, 줄 간격 1.25, 낱말 단위 줄바꿈, 글꼴 이름 `"default"` 입니다. 글꼴 이름 `"default"` 는 엔진의 기본 글꼴(따로 주지 않으면 시스템 글꼴)입니다.

```rust
use vanilla::{Color, Text, TextAlign, TextBaseline, TextWrapMode};

let mut text = Text::new();
assert_eq!(text.get_font_size(), 32.0);
assert_eq!(text.get_text_color(), Color::white());
assert_eq!(text.get_text_align(), TextAlign::Center);
assert_eq!(text.get_text_baseline(), TextBaseline::Middle);
assert_eq!(text.get_stroke_width(), 0.0);
assert_eq!(text.get_line_spacing(), 1.25);
assert_eq!(text.get_wrap_mode(), TextWrapMode::Word);
assert_eq!(text.get_font_family(), "default");
assert_eq!(text.get_visible_character_count(), -1);

text.set_visible_character_count(-5);
assert_eq!(text.get_visible_character_count(), -1);
```

- 글은 노드의 내용 영역 안에 놓입니다. 가로는 정렬이 `Left` 면 왼쪽 끝, `Right` 면 오른쪽 끝, 그 밖은 가운데이고(`Start`, `End` 는 글 방향에 따라 왼쪽이나 오른쪽으로 풀립니다), 세로는 기준선이 `Top` 이면 위, `Bottom`, `Alphabetic`, `Ideographic` 이면 아래, 그 밖은 가운데입니다.
- `set_word_wrap_width(width)` 가 0 보다 크거나 글에 `"\n"` 이 있으면 여러 줄로 그립니다. 폭을 주지 않은 여러 줄 글은 노드 너비로 나눕니다. `set_wrap_mode(TextWrapMode::Character)` 는 낱말과 상관없이 글자 단위로 나눕니다. (처음 값 `TextWrapMode::Word`)
- `set_fit_width(width, minimum_ratio)` 는 한 줄 글이 그 너비를 넘치면 정한 크기에서 최소 비율까지 줄여 그립니다. (처음 최소 비율 0.55) 언어마다 길이가 다른 단추 글에 씁니다. 러스트는 기본 인자가 없어서 최소 비율도 함께 줍니다.
- `set_visible_character_count(count)` 는 앞에서 그만큼의 글자 묶음만 보입니다. (-1 이면 전체, 타자기 연출)
- `set_font_family(name)` 은 등록한 글꼴 이름을 받습니다. 원본의 `setFont(font)` 자리입니다. 글꼴 받기와 등록은 "애셋" 장에서 봅니다.

## RichText

`RichText::set_text` 는 마크업을 읽어 조각으로 나눕니다. 조각에 적지 않은 꼴은 품은 `Text` 의 설정을 따릅니다.

| 태그 | 뜻 |
| --- | --- |
| `<b>...</b>`, `<i>...</i>` | 굵게, 기울임 |
| `<u>...</u>`, `<s>...</s>` | 밑줄, 취소선 |
| `<color=#RRGGBB>...</color>`, `<color=#RRGGBBAA>...</color>` | 색 |
| `<color=red>...</color>` | 이름 있는 색 (red, green, blue, white, black, yellow, cyan, magenta, gray, orange, purple 같은 22 가지) |
| `<size=N>...</size>` | 글자 크기 |

모르는 태그는 글 그대로 보입니다. 알려진 태그가 하나도 없는 글은 조각 없이 품은 `Text` 로 그립니다. 마크업 없이 코드로 조각을 쌓을 때는 `append_segment(text, attributes)` 를 씁니다. 속성 `RichTextAttributes` 는 `text_color`, `font_size`, `font_family`, `stroke_color`, `stroke_width` (모두 `Option`, `None` 이면 `Text` 의 값)와 `is_bold`, `is_italic`, `is_underline`, `is_strikethrough` 입니다.

```rust
use vanilla::{Color, RichText, RichTextAttributes, Vector2, WorldNode};

let mut node = WorldNode::new("hp");
node.set_content_size(Vector2::create(600.0, 60.0));
let rich_text = node.add_component(RichText::new());
rich_text.append_segment("HP ", RichTextAttributes {
	is_bold: true,
	..RichTextAttributes::new()
});
rich_text.append_segment("120", RichTextAttributes {
	text_color: Some(Color::create(0.4, 1.0, 0.4, 1.0)),
	font_size: Some(40.0),
	..RichTextAttributes::new()
});
rich_text.append_segment(" / 150", RichTextAttributes {
	text_color: Some(Color::create(0.7, 0.7, 0.7, 1.0)),
	..RichTextAttributes::new()
});
assert_eq!(rich_text.get_segments().len(), 3);
assert_eq!(rich_text.get_segments()[1].attributes.font_size, Some(40.0));

let mut markup = RichText::new();
markup.set_text("<b>Bold</b>, <color=#ff6666>red</color>, <size=40>big</size> and <u>underline</u>");
assert_eq!(markup.get_segments().len(), 7);
assert_eq!(markup.get_segments()[0].text, "Bold");
assert!(markup.get_segments()[0].attributes.is_bold);

markup.set_text("plain text");
assert!(!markup.has_segments());
```

조각이 있는 `RichText` 는 조각을 왼쪽부터 한 줄로 이어 그립니다. 정렬과 기준선은 조각 폭의 합과 가장 큰 글자 크기로 셉니다. 글 방향이 오른쪽에서 왼쪽이면 조각을 오른쪽부터 놓습니다.

## 블렌드 모드

블렌드 모드는 Canvas2D 의 합성 26 가지를 모두 받는 `BlendMode` 열거입니다. 컴포넌트는 `Sprite::set_sprite_blend_mode(mode)` (`SpriteBlendMode` 는 `BlendMode` 의 다른 이름입니다), 바로 그릴 때는 `graphic.set_blend_mode(mode)` 를 씁니다. 처음 값은 `BlendMode::Normal`("source-over") 입니다.

- 고정 블렌딩: `Normal`, `SourceIn`, `SourceOut`, `SourceAtop`, `DestinationOver`, `DestinationIn`, `DestinationOut`, `DestinationAtop`, `Lighter`, `Copy`, `Xor`, `Multiply`, `Screen`
- 셰이더로 섞기: `Overlay`, `Darken`, `Lighten`, `ColorDodge`, `ColorBurn`, `HardLight`, `SoftLight`, `Difference`, `Exclusion`, `Hue`, `Saturation`, `Color`, `Luminosity`

셰이더로 섞는 13 가지는 그릴 때마다 그릴 영역 아래를 지금 화면에서 떠 와 W3C Compositing and Blending 공식대로 섞습니다. 이 모드에서는 묶어 그리지 않고 그리기마다 영역 복사가 한 번 더 듭니다. `Multiply` 는 렌더 타겟에 그릴 때만 셰이더로 섞습니다.

원본 이름 글(`"lighter"`, `"color-dodge"` 같은 것)은 `BlendMode::from_name(name)` 이 열거로 바꿉니다. 모르는 이름이면 `None` 입니다. 거꾸로는 `get_name()` 입니다.

```rust
use vanilla::BlendMode;

assert_eq!(BlendMode::from_name("lighter"), Some(BlendMode::Lighter));
assert_eq!(BlendMode::from_name("source-over"), Some(BlendMode::Normal));
assert_eq!(BlendMode::from_name("unknown"), None);
assert_eq!(BlendMode::Difference.get_name(), "difference");
```

```rust,no_run
use vanilla::{BlendMode, Color, Engine, EngineConfiguration, Graphic, Scene, SceneBase, Vector2};

struct BlendScene {
	scene_base: SceneBase,
}

impl Scene for BlendScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw(graphic);
		let blend_modes = [BlendMode::Normal, BlendMode::Lighter, BlendMode::Multiply, BlendMode::Screen, BlendMode::Difference];
		for (index, blend_mode) in blend_modes.iter().enumerate() {
			let center_x = 160.0 + index as f32 * 240.0;
			graphic.set_fill_color(Color::create(0.9, 0.3, 0.2, 1.0));
			graphic.draw_circle(Vector2::create(center_x - 40.0, 360.0), 90.0);
			graphic.push_state();
			graphic.set_blend_mode(*blend_mode);
			graphic.set_fill_color(Color::create(0.2, 0.5, 0.9, 1.0));
			graphic.draw_circle(Vector2::create(center_x + 40.0, 360.0), 90.0);
			graphic.pop_state();
		}
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let blend_scene = BlendScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(blend_scene)) {
		eprintln!("{}", error);
	}
}
```

`push_state` 와 `pop_state` 로 감싸면 블렌드 모드도 함께 되돌아갑니다. Canvas2D 와 달리 `SourceIn`, `SourceOut`, `DestinationIn`, `DestinationAtop`, `Copy` 는 그린 도형 안만 바꿉니다. `Sprite` 는 그림을 그릴 때만 블렌드 모드를 걸고, 그린 뒤 `Normal` 로 되돌립니다. (그림이 없을 때는 쓰지 않습니다)

## 꾸밈 컴포넌트

| 컴포넌트 | 하는 일 | 주요 함수 (처음 값) |
| --- | --- | --- |
| `Mask` | 노드의 내용 영역으로 자식 그리기를 자릅니다. 돌아간 노드도 맞게 자르고, 잘린 곳의 자식은 터치도 받지 않습니다 | `set_round_size` (0) |
| `Outline` | 테만 그립니다. 둥근 네모 테나 동그라미 테 (선은 내용 영역 안쪽에 들어옵니다) | `set_color` (흰색), `set_line_width` (1), `set_round_size` (0), `set_circle` (false) |
| `ShadowText` | 그림자를 깐 한 줄 글을 그림 한 장에 구워 그립니다. 노드를 흐리게 해도 글과 그림자가 한 장으로 흐려집니다. 넘치면 줄여 맞춥니다 | `set_text`, `set_text_style(font_size, is_bold, text_color)` (16, false, 흰색), `set_shadow(color, offset, blur)` (검정, 0, 0), `set_fit_minimum_ratio` (0.6) |
| `SoftShade` | 글 뒤를 부드럽게 어둡게 누르는 그늘 | `set_shade(shape, color, opacity, fade_width)` (`SoftShadeShape::Band`, 검정, 0.5, 0) |
| `ShadowImage` | 그림자를 깐 그림. 노드 안에 비를 지켜 가장 크게, 가운데로 | `set_image(image, image_rect)`, `set_image_color`, `set_shadow(color, offset, blur)` |

`SoftShadeShape::Band` 는 가운데 가로줄이 가장 짙고 위아래로 엷어지는 띠, `SoftShadeShape::Bottom` 은 아래 끝이 가장 짙은 세로 그라데이션입니다. 원본의 `setShadow(color, opacity, offset, blur)` 는 러스트 판에서 불투명도를 색의 알파로 받아 `set_shadow(color, offset, blur)` 입니다.

```rust,no_run
use vanilla::{Color, Engine, EngineConfiguration, Graphic, Mask, Outline, Paint, Rect, Scene, SceneBase, ShadowText, SoftShade, SoftShadeShape, Vector2};

struct DecorationScene {
	scene_base: SceneBase,
}

impl Scene for DecorationScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();

		let panel_id = node_tree.create_child(root_id, "panel");
		node_tree[panel_id].set_content_size(Vector2::create(480.0, 300.0));
		node_tree[panel_id].set_local_position(Vector2::create(400.0, 360.0));
		node_tree[panel_id].add_component(Paint::create_with_color("#1d2333")).set_round_size(20.0);
		node_tree[panel_id].add_component(Mask::new()).set_round_size(20.0);

		let circle_id = node_tree.create_child(panel_id, "circle");
		node_tree[circle_id].set_content_size(Vector2::create(320.0, 320.0));
		node_tree[circle_id].set_local_position(Vector2::create(460.0, 280.0));
		node_tree[circle_id].add_component(Paint::create_with_color(Color::create(1.0, 0.6, 0.2, 1.0))).set_round_size(160.0);

		let frame_id = node_tree.create_child(panel_id, "frame");
		node_tree[frame_id].set_content_size(Vector2::create(480.0, 300.0));
		node_tree[frame_id].set_local_position(Vector2::create(240.0, 150.0));
		let outline = node_tree[frame_id].add_component(Outline::new());
		outline.set_color("#8899cc");
		outline.set_line_width(3.0);
		outline.set_round_size(20.0);

		let shade_id = node_tree.create_child(root_id, "shade");
		node_tree[shade_id].set_content_size(Vector2::create(560.0, 120.0));
		node_tree[shade_id].set_local_position(Vector2::create(960.0, 360.0));
		node_tree[shade_id].add_component(SoftShade::new()).set_shade(SoftShadeShape::Band, Color::black(), 0.7, 80.0);

		let caption_id = node_tree.create_child(root_id, "caption");
		node_tree[caption_id].set_content_size(Vector2::create(520.0, 80.0));
		node_tree[caption_id].set_local_position(Vector2::create(960.0, 360.0));
		let shadow_text = node_tree[caption_id].add_component(ShadowText::create_with_text("Shadow Text", 48.0));
		shadow_text.set_text_style(48.0, true, Color::white());
		shadow_text.set_shadow(Color::create(0.0, 0.0, 0.0, 0.8), 4.0, 6.0);
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
		graphic.set_fill_color("#5a6b7a");
		graphic.draw_rect(Rect::create(700.0, 0.0, 580.0, 720.0));
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let decoration_scene = DecorationScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(decoration_scene)) {
		eprintln!("{}", error);
	}
}
```

`Mask` 는 컴포넌트를 그리는 단계에서는 아무것도 그리지 않고, 노드가 자기 컴포넌트를 다 그린 뒤 자식을 그리기 직전에 자르기를 시작해 직후에 풉니다. 그래서 같은 노드의 `Paint` 는 잘리지 않고, 자식만 잘립니다. 노드는 자기 컴포넌트를 모두 그린 뒤 자식을 그리므로, 테(`Outline`)를 자식 위에 보이려면 위 예제처럼 마지막 자식 노드에 붙입니다.

## Camera2D

`Camera2D` 는 컴포넌트가 아니라 씬이 들고 쓰는 값입니다. 바라보는 월드 좌표(자리, 화면 가운데에 옵니다), 확대 배율(처음 1, `set_zoom_range` 의 범위 안으로 자르며 처음 범위는 0.1 ~ 8), 회전(도, 화면 가운데가 축)을 갖고, `begin(graphic)` 과 `end(graphic)` 사이에서 그린 것에 그 변환을 겁니다.

| 함수 | 내용 |
| --- | --- |
| `set_view_size(size)` | 화면 크기. 매 프레임이나 리사이즈 때 뷰 크기를 줍니다 (처음 960 x 640) |
| `set_position`, `set_zoom`, `set_rotation` | 바로 정하기 |
| `set_world_bounds(Some(rect))` | 보이는 영역이 이 사각형 밖으로 나가지 않게 막습니다 (`None` 이면 막지 않음) |
| `set_zoom_range(minimum_zoom, maximum_zoom)` | 확대 배율 범위 |
| `follow(&target_positions, rate, time_delta)` | 대상(여럿이면 가운데)을 프레임과 상관없이 고르게 따라갑니다 |
| `pan_by_screen(delta_x, delta_y)`, `fling(velocity_x, velocity_y)` | 화면 픽셀 단위로 끌기, 놓을 때 관성 |
| `zoom_at(anchor_screen, zoom_factor)` | 화면의 한 점 아래 월드 지점을 고정한 채 확대 |
| `tween_to(position, zoom, duration)` | 자리와 배율로 부드럽게 옮기기 (원본의 기본 길이 0.4 초는 `Camera2D::DEFAULT_TWEEN_DURATION`) |
| `tick(time_delta)` | 관성, 옮기기, 경계 막기 갱신 |
| `screen_to_world(position)`, `world_to_screen(position)` | 좌표 바꾸기 |

```rust
use vanilla::{Camera2D, Rect, Vector2};

let mut camera = Camera2D::new();
camera.set_view_size(Vector2::create(800.0, 600.0));
camera.set_position(Vector2::create(400.0, 300.0));
assert_eq!(camera.world_to_screen(Vector2::create(400.0, 300.0)), Vector2::create(400.0, 300.0));

camera.set_zoom(2.0);
assert_eq!(camera.screen_to_world(Vector2::create(600.0, 300.0)), Vector2::create(500.0, 300.0));

camera.set_zoom(100.0);
assert_eq!(camera.get_zoom(), 8.0);

camera.set_zoom(1.0);
camera.set_world_bounds(Some(Rect::create(0.0, 0.0, 1000.0, 1000.0)));
camera.set_position(Vector2::create(0.0, 0.0));
assert_eq!(camera.get_position(), Vector2::create(400.0, 300.0));

camera.tween_to(Vector2::create(600.0, 500.0), 1.0, Camera2D::DEFAULT_TWEEN_DURATION);
for _ in 0..30 {
	camera.tick(1.0 / 60.0);
}
assert!(!camera.is_tweening());
```

```rust,no_run
use vanilla::{Camera2D, Color, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, Vector2};

struct CameraScene {
	scene_base: SceneBase,
	camera: Camera2D,
	elapsed_seconds: f32,
}

impl CameraScene {
	fn read_target_position(&self) -> Vector2 {
		Vector2::create(400.0 + self.elapsed_seconds * 200.0, 400.0 + self.elapsed_seconds * 100.0)
	}
}

impl Scene for CameraScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		self.camera.set_world_bounds(Some(Rect::create(0.0, 0.0, 2400.0, 1600.0)));
		self.camera.set_position(Vector2::create(400.0, 400.0));
		self.camera.set_zoom(1.5);
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		self.elapsed_seconds += time_delta;
		let view_size = engine.get_view_manager().get_view_size();
		self.camera.set_view_size(view_size);
		let target_position = self.read_target_position();
		self.camera.follow(&[target_position], 4.0, time_delta);
		self.camera.set_rotation(self.elapsed_seconds.sin() * 10.0);
		self.camera.tick(time_delta);
	}

	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.camera.begin(graphic);
		for row in 0..16 {
			for column in 0..24 {
				let is_dark = (row + column) % 2 == 0;
				if is_dark {
					graphic.set_fill_color("#283040");
				}
				else {
					graphic.set_fill_color("#3a4660");
				}
				graphic.draw_rect(Rect::create(column as f32 * 100.0, row as f32 * 100.0, 100.0, 100.0));
			}
		}
		graphic.set_fill_color(Color::create(1.0, 0.8, 0.2, 1.0));
		graphic.draw_circle(self.read_target_position(), 30.0);
		self.camera.end(graphic);
		self.scene_base.draw(graphic);
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	engine_configuration.auto_resize_on_window_resize = true;
	let engine = Engine::new(engine_configuration);
	let camera_scene = CameraScene {
		scene_base: SceneBase::new(),
		camera: Camera2D::new(),
		elapsed_seconds: 0.0,
	};
	if let Err(error) = engine.run(Box::new(camera_scene)) {
		eprintln!("{}", error);
	}
}
```

위 예제는 카메라를 끝낸 뒤 `self.scene_base.draw(graphic)` 로 노드 트리를 그리므로 노드는 카메라와 상관없이 화면에 고정됩니다. (HUD) 노드 트리까지 카메라로 비추려면 `self.scene_base.draw(graphic)` 를 `begin` 과 `end` 사이에서 부릅니다. 씬의 `tick` 을 덮어쓰면 위처럼 `self.scene_base.tick(engine, time_delta)` 를 불러야 노드, 터치, 트윈이 갱신됩니다.

## 그리기 설정

원본처럼 `engine.get_graphic_mut()` 으로 `Graphic` 을 얻어 씬의 `create`, `initialize`, `tick` 에서 정합니다. 씬의 그리기 함수(`pre_draw`, `draw`, `post_draw`, `draw_on_load`) 안에서는 인자로 받은 `graphic` 에 정합니다. (그 안에서는 엔진이 그래픽을 인자로 내어 주어 `engine.get_graphic_mut()` 이 `None` 입니다) 아래 설정은 매 프레임 되돌아가지 않으므로 한 번 정하면 됩니다.

- `set_image_smoothing_enabled(false)`: 그림을 가장 가까운 픽셀로 늘입니다. 도트 그림 게임이 씁니다. (처음 켬)
- `set_image_mipmap_enabled(true)`: 큰 그림을 크게 줄여 그릴 때 자글거림을 줄입니다. (처음 끔)
- `set_batching_enabled`, `set_culling_enabled`: 묶어 그리기와 화면 밖 거르기. 둘 다 처음 켜져 있습니다. 거르기는 켜진 컴포넌트가 모두 그리는 사각형을 알려 준 노드만 화면 밖일 때 컴포넌트 그리기를 건너뜁니다. 자식은 따로 판정합니다.

## 주의할 점

- 그림은 다 받은 뒤에 그립니다. 씬의 `create` 에서 받기를 청하고 `track_asset` 으로 맡긴 뒤, `initialize` 에서 `get_image()` 로 꺼내 노드를 짓습니다. 웹은 받기가 비동기라 `create` 바로 뒤에는 그림이 아직 없습니다.
- `Sprite` 는 `Rc<Image>` 만 받습니다. 원본의 `HTMLImageElement` 나 캔버스 자리는 없습니다. 픽셀로 그림을 지을 때는 `Image::create_from_pixels(width, height, pixels)` 를 씁니다.
- 색을 글로 줄 때는 `"#..."` 이나 `"rgb..."` 로 시작해야 합니다. 모르는 꼴의 글은 흰색이 됩니다. (`Paint`, `Text`, `Graphic` 모두 `impl Into<Color>` 를 받고 같은 규칙입니다)
- 그리기 모드와 블렌드 모드는 원본처럼 글이 아니라 `SpriteDrawMode`, `BlendMode` 열거로 줍니다. JSON 처럼 글로 온 이름은 `BlendMode::from_name` 으로 바꿉니다.
- 셰이더로 섞는 블렌드 모드는 그리기마다 영역 복사가 들고 묶어 그리지 않으므로 많이 쓰면 느려집니다.
- 글은 문자열과 꼴마다 텍스처로 구워 캐시에 둡니다. (상한 512 개, 오래 안 쓴 것부터 버립니다) 글자는 흰색으로 굽고 그릴 때 색을 곱하므로 색만 바뀌면 다시 굽지 않지만, 매 프레임 바뀌는 숫자처럼 글이 자주 바뀌면 바뀔 때마다 새로 굽습니다.
- 데스크탑은 엔진이 글을 모양 짓고 구우며, 웹은 원본처럼 브라우저 Canvas2D 로 굽습니다. 같은 글꼴이라도 글자 모양이 화소 단위로 조금 다를 수 있습니다.

## 관련 타입

Graphic, BlendMode, TextAlign, TextBaseline, Color, Colors, Rect, Image, Paint, Sprite, SpriteDrawMode, SpriteBlendMode, Text, TextWrapMode, RichText, RichTextAttributes, RichTextSegment, Mask, Outline, ShadowText, SoftShade, SoftShadeShape, ShadowImage, Camera2D, ImageAsset, Frame

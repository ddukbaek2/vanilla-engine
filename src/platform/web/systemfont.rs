//==============================================================================
// 웹 시스템 글꼴로 글자 굽기. (wasm32)
// - 원본 textstringtexturecache.js 의 bake_entry, measure_text 를 옮긴 것이다. 원본은 늘 오프스크린 2D 캔버스에
//   fillText / strokeText 로 굽는다. 그래서 글꼴을 따로 주지 않아도 브라우저가 고른 시스템 글꼴로 글자가 나온다.
// - 러스트 판은 글꼴 바이트(default_font_bytes, default_font_url, FontAsset)가 하나도 없을 때만 여기로 온다.
//   글꼴 바이트가 있으면 데스크탑과 같은 fontdue 길로 굽는다.
// - 글꼴 목록은 폰트 문자열의 이름을 앞에 두고 원본 SYSTEM_FONT_STRING 을 뒤에 붙인다.
//   (CSS @font-face 로 문서에 올린 글꼴 이름이면 그 글꼴이, 아니면 시스템 글꼴이 나온다)
// - 캔버스는 처음 쓸 때 하나 만들어 굽기와 재기에 같이 쓴다. 문서에는 붙이지 않는다.
// - 원본처럼 캔버스의 RGBA 를 그대로 올린다. 글자는 흰색이고 색 이모지는 브라우저가 그린 제 색이다. (프리멀티플라이드로 바꿔 둔다)
//==============================================================================
use crate::base::platform::SYSTEM_FONT_STRING;
use crate::core::textshaping::ShapingDirection;
use crate::core::textstringtexturecache::{FontDescription, TextDrawKind, TextMetrics};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, JsValue};

//==============================================================================
// 구운 비트맵. (길이 단위는 배율을 곱한 픽셀)
//==============================================================================
pub struct SystemFontBitmap {
	pub bitmap_width: i32,
	pub bitmap_height: i32,
	// 프리멀티플라이드 RGBA. (글자는 흰색이라 fontdue 길과 같은 꼴, 색 이모지는 제 색)
	pub pixels: Vec<u8>,
	pub pen_x: f32,
	pub baseline_y: f32,
	pub advance_width: f32,
	pub font_ascent: f32,
	pub font_descent: f32,
}

struct SystemFontCanvas {
	canvas: web_sys::HtmlCanvasElement,
	context: web_sys::CanvasRenderingContext2d,
}

thread_local! {
	// 굽기와 재기에 쓰는 캔버스. (처음 쓸 때 만든다. 못 만들면 비어 있다)
	static SYSTEM_FONT_CANVAS: RefCell<Option<SystemFontCanvas>> = const { RefCell::new(None) };
}

//==============================================================================
// 문자열 재기. (원본 measure_text)
//==============================================================================
pub fn measure_text(font_description: &FontDescription, text: &str) -> TextMetrics {
	let font_string = build_css_font_string(font_description, 1.0);
	let measured = with_canvas(|system_font_canvas| {
		system_font_canvas.context.set_font(&font_string);
		let text_metrics = system_font_canvas.context.measure_text(text).ok()?;
		let (font_ascent, font_descent) = read_font_bounding_box(&text_metrics);
		Some(TextMetrics {
			width: text_metrics.width() as f32,
			font_ascent,
			font_descent,
			actual_bounding_box_left: read_finite_value(text_metrics.actual_bounding_box_left(), 0.0),
			actual_bounding_box_right: read_finite_value(text_metrics.actual_bounding_box_right(), text_metrics.width() as f32),
			actual_bounding_box_ascent: read_finite_value(text_metrics.actual_bounding_box_ascent(), font_ascent),
			actual_bounding_box_descent: read_finite_value(text_metrics.actual_bounding_box_descent(), font_descent),
		})
	});
	measured.unwrap_or_default()
}

//==============================================================================
// 문자열 굽기. (원본 bake_entry. 크기와 펜 자리 계산이 원본과 같다, direction 은 캔버스의 direction)
//==============================================================================
pub fn bake_text(text_draw_kind: TextDrawKind, text: &str, font_description: &FontDescription, line_width: f32, bake_scale: f32, direction: ShapingDirection) -> Option<SystemFontBitmap> {
	let font_string = build_css_font_string(font_description, bake_scale);
	let direction_name = match direction {
		ShapingDirection::LeftToRight => {
			"ltr"
		}
		ShapingDirection::RightToLeft => {
			"rtl"
		}
	};
	with_canvas(|system_font_canvas| {
		let canvas = &system_font_canvas.canvas;
		let context = &system_font_canvas.context;

		// 배율을 곱한 폰트로 잰다. (경계는 왼쪽 기준)
		context.set_font(&font_string);
		set_canvas_direction(context, direction_name);
		context.set_text_align("left");
		let text_metrics = context.measure_text(text).ok()?;

		// 글리프 실측 경계. (측정 미지원 브라우저는 전진폭으로 어림)
		let advance_width = text_metrics.width() as f32;
		let actual_left = read_finite_value(text_metrics.actual_bounding_box_left(), 0.0).max(0.0).ceil();
		let actual_right = read_finite_value(text_metrics.actual_bounding_box_right(), advance_width).max(0.0).ceil();
		let (font_ascent, font_descent) = read_font_bounding_box(&text_metrics);

		// 외곽선 두께와 안티앨리어싱 여유.
		let stroke_width = match text_draw_kind {
			TextDrawKind::Stroke => {
				line_width * bake_scale
			}
			TextDrawKind::Fill => {
				0.0
			}
		};
		let padding = (stroke_width / 2.0).ceil() + 2.0;

		// 굽기 자리와 캔버스 크기.
		let pen_x = padding + actual_left;
		let baseline_y = padding + font_ascent.ceil();
		let bitmap_width = (pen_x + actual_right + padding) as i32;
		let bitmap_height = (baseline_y + font_descent.ceil() + padding) as i32;
		if bitmap_width <= 0 || bitmap_height <= 0 {
			return None;
		}

		// 캔버스 크기를 바꾸면 그리기 상태가 처음으로 돌아가서 폰트를 다시 넣는다.
		canvas.set_width(bitmap_width as u32);
		canvas.set_height(bitmap_height as u32);
		context.clear_rect(0.0, 0.0, bitmap_width as f64, bitmap_height as f64);
		context.set_font(&font_string);
		set_canvas_direction(context, direction_name);
		context.set_text_align("left");
		context.set_text_baseline("alphabetic");
		match text_draw_kind {
			TextDrawKind::Stroke => {
				context.set_stroke_style_str("#ffffff");
				context.set_line_width(stroke_width as f64);
				context.stroke_text(text, pen_x as f64, baseline_y as f64).ok()?;
			}
			TextDrawKind::Fill => {
				context.set_fill_style_str("#ffffff");
				context.fill_text(text, pen_x as f64, baseline_y as f64).ok()?;
			}
		}

		// 곧은 알파 RGBA 를 프리멀티플라이드로. (원본은 UNPACK_PREMULTIPLY_ALPHA_WEBGL 로 올린다. 흰 글자는 네 칸 모두 덮인 정도가 된다)
		let image_data = context.get_image_data(0.0, 0.0, bitmap_width as f64, bitmap_height as f64).ok()?;
		let mut pixels = image_data.data().0;
		for pixel in pixels.chunks_exact_mut(4) {
			let alpha = pixel[3] as u32;
			for channel_index in 0..3 {
				pixel[channel_index] = ((pixel[channel_index] as u32 * alpha + 127) / 255) as u8;
			}
		}

		Some(SystemFontBitmap {
			bitmap_width,
			bitmap_height,
			pixels,
			pen_x,
			baseline_y,
			advance_width,
			font_ascent,
			font_descent,
		})
	})
}

//==============================================================================
// 시스템 글꼴 조각 재기. (올린 글꼴에 없는 글자 묶음, 크기는 픽셀. 원본 브라우저의 글꼴 대체 자리)
//==============================================================================
pub fn measure_piece(font_description: &FontDescription, pixel_size: f32, text: &str, is_right_to_left: bool) -> f32 {
	let mut piece_description = font_description.clone();
	piece_description.font_size = pixel_size;
	let font_string = build_css_font_string(&piece_description, 1.0);
	let measured = with_canvas(|system_font_canvas| {
		system_font_canvas.context.set_font(&font_string);
		set_canvas_direction(&system_font_canvas.context, if is_right_to_left {
			"rtl"
		}
		else {
			"ltr"
		});
		let text_metrics = system_font_canvas.context.measure_text(text).ok()?;
		Some(text_metrics.width() as f32)
	});
	measured.unwrap_or(0.0)
}

//==============================================================================
// 시스템 글꼴 조각 래스터. (덮인 정도, 왼쪽은 펜 자리에서, 위는 기준선에서. 크기는 픽셀)
// - 흰 픽셀은 글자의 덮인 정도로, 흰색이 아닌 픽셀(브라우저가 그린 색 이모지)은 색 몫(color_pixels)으로 나눈다.
//==============================================================================
pub fn rasterize_piece(font_description: &FontDescription, pixel_size: f32, text: &str, is_right_to_left: bool) -> Option<crate::core::textshaping::GlyphRaster> {
	let mut piece_description = font_description.clone();
	piece_description.font_size = pixel_size;
	let direction = if is_right_to_left {
		ShapingDirection::RightToLeft
	}
	else {
		ShapingDirection::LeftToRight
	};
	let bitmap = bake_text(TextDrawKind::Fill, text, &piece_description, 0.0, 1.0, direction)?;
	let mut coverage: Vec<u8> = Vec::with_capacity(bitmap.pixels.len() / 4);
	let mut color_pixels = vec![0u8; bitmap.pixels.len()];
	let mut has_color = false;
	for (pixel_index, pixel) in bitmap.pixels.chunks_exact(4).enumerate() {
		let is_white = pixel[0] == pixel[3] && pixel[1] == pixel[3] && pixel[2] == pixel[3];
		if is_white {
			coverage.push(pixel[3]);
		}
		else {
			coverage.push(0);
			color_pixels[pixel_index * 4..pixel_index * 4 + 4].copy_from_slice(pixel);
			has_color = true;
		}
	}
	Some(crate::core::textshaping::GlyphRaster {
		coverage,
		width: bitmap.bitmap_width,
		height: bitmap.bitmap_height,
		left_x: -(bitmap.pen_x.round() as i32),
		top_y: bitmap.baseline_y.round() as i32,
		color_pixels: if has_color {
			Some(color_pixels)
		}
		else {
			None
		},
	})
}

//==============================================================================
// CSS 폰트 문자열 만들기. ("italic bold 32px "이름", 시스템 글꼴 목록")
// - 이름이 비었거나 "default" 면 시스템 글꼴 목록만 쓴다. 이름에 쉼표가 있으면 이미 목록이라 그대로 앞에 둔다.
//==============================================================================
fn build_css_font_string(font_description: &FontDescription, bake_scale: f32) -> String {
	let style_text = if font_description.is_italic {
		"italic "
	}
	else {
		""
	};
	let weight_text = if font_description.font_weight != 400 {
		format!("{} ", font_description.font_weight)
	}
	else {
		String::new()
	};
	let pixel_size = font_description.font_size * bake_scale;
	let family_name = font_description.family_name.trim();
	let family_list = if family_name.is_empty() || family_name.eq_ignore_ascii_case("default") {
		SYSTEM_FONT_STRING.to_string()
	}
	else if family_name.contains(',') {
		format!("{}, {}", family_name, SYSTEM_FONT_STRING)
	}
	else {
		format!("\"{}\", {}", family_name.replace('"', ""), SYSTEM_FONT_STRING)
	};
	format!("{}{}{}px {}", style_text, weight_text, pixel_size, family_list)
}

//==============================================================================
// 캔버스의 글 방향 넣기. (web-sys 에 direction 세터가 없는 판도 있어 속성으로 넣는다)
//==============================================================================
fn set_canvas_direction(context: &web_sys::CanvasRenderingContext2d, direction_name: &str) {
	let _ = js_sys::Reflect::set(context, &JsValue::from_str("direction"), &JsValue::from_str(direction_name));
}

//==============================================================================
// 폰트 위아래 경계. (fontBoundingBox 를 모르는 브라우저는 실측 경계로, 그것도 없으면 크기로 어림)
//==============================================================================
fn read_font_bounding_box(text_metrics: &web_sys::TextMetrics) -> (f32, f32) {
	let actual_ascent = read_finite_value(text_metrics.actual_bounding_box_ascent(), 0.0);
	let actual_descent = read_finite_value(text_metrics.actual_bounding_box_descent(), 0.0);
	let font_ascent = read_finite_value(text_metrics.font_bounding_box_ascent(), actual_ascent);
	let font_descent = read_finite_value(text_metrics.font_bounding_box_descent(), actual_descent);
	(font_ascent, font_descent)
}

//==============================================================================
// 브라우저가 모르는 값(undefined 는 NaN 으로 온다)이면 대신 값.
//==============================================================================
fn read_finite_value(value: f64, fallback_value: f32) -> f32 {
	if value.is_finite() {
		value as f32
	}
	else {
		fallback_value
	}
}

//==============================================================================
// 캔버스를 빌려 일하기. (없으면 만든다)
//==============================================================================
fn with_canvas<Output>(work: impl FnOnce(&SystemFontCanvas) -> Option<Output>) -> Option<Output> {
	SYSTEM_FONT_CANVAS.with(|system_font_canvas_cell| {
		let mut system_font_canvas_slot = system_font_canvas_cell.borrow_mut();
		if system_font_canvas_slot.is_none() {
			*system_font_canvas_slot = create_canvas();
		}
		let system_font_canvas = system_font_canvas_slot.as_ref()?;
		work(system_font_canvas)
	})
}

//==============================================================================
// 캔버스 만들기. (굽고 나서 픽셀을 바로 읽으니 willReadFrequently 를 켠다)
//==============================================================================
fn create_canvas() -> Option<SystemFontCanvas> {
	let document = web_sys::window()?.document()?;
	let canvas: web_sys::HtmlCanvasElement = document.create_element("canvas").ok()?.dyn_into().ok()?;
	let context_options = js_sys::Object::new();
	js_sys::Reflect::set(&context_options, &JsValue::from_str("willReadFrequently"), &JsValue::TRUE).ok()?;
	let context: web_sys::CanvasRenderingContext2d = canvas.get_context_with_context_options("2d", &context_options).ok()??.dyn_into().ok()?;
	Some(SystemFontCanvas {
		canvas,
		context,
	})
}

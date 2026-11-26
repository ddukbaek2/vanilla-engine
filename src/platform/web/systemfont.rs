//==============================================================================
//! 웹 시스템 글꼴로 글자 굽기. (wasm32)
//! - 원본 textstringtexturecache.js 의 bake_entry, measure_text 를 옮긴 것이다. 원본은 늘 오프스크린 2D 캔버스에
//!   fillText / strokeText 로 굽는다. 그래서 글꼴을 따로 주지 않아도 브라우저가 고른 시스템 글꼴로 글자가 나온다.
//! - 러스트 판 웹도 원본처럼 글자를 늘 여기서 굽고 잰다. 글꼴 바이트(default_font_bytes, default_font_url, FontAsset, UIFont 조각)는
//!   원본 new FontFace(이름, 바이트, 서술자) 와 같이 document.fonts 에 올려 브라우저가 고르고 그리게 한다. (sync_font_faces)
//!   기본 폰트는 VanillaEngineDefault, 대체 폰트는 VanillaEngineFallback1, 2 … 이름으로 올린다.
//! - 글꼴 목록은 폰트 문자열의 이름들, 기본 폰트와 대체 폰트의 이름, (default 를 청했으면) 원본 SYSTEM_FONT_STRING 차례다.
//!   (CSS @font-face 나 FontFace 로 문서에 올린 글꼴 이름이면 그 글꼴이, 아니면 시스템 글꼴이 나온다)
//! - 올린 글꼴이 다 받아지면(loadingdone) 바뀐 횟수를 늘린다. 글자 캐시가 프레임 시작에 보고 구운 글을 버린다. (원본 textstringtexturecache.js 와 같다)
//! - 그림자 글(bake_shadow_text)과 그림자 그림(bake_shadow_image)도 원본 ShadowText, ShadowImage 처럼 캔버스의 shadowBlur 와 drawImage(imageSmoothingQuality high)로 굽는다.
//! - 캔버스는 처음 쓸 때 하나 만들어 굽기와 재기에 같이 쓴다. 문서에는 붙이지 않는다.
//! - 원본처럼 캔버스의 RGBA 를 그대로 올린다. 글자는 흰색이고 색 이모지는 브라우저가 그린 제 색이다. (프리멀티플라이드로 바꿔 둔다)
//==============================================================================
use crate::base::color::Color;
use crate::base::platform::SYSTEM_FONT_STRING;
use crate::core::textshaping::{FontFaceDescriptors, LoadedFont, ShapingDirection};
use crate::core::textstringtexturecache::{FontDescription, TextDrawKind, TextMetrics};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

/// 구운 캔버스의 가장자리 여유. (원본 ShadowText, ShadowImage 의 BAKE_PADDING, 픽셀)
const SHADOW_BAKE_PADDING: f32 = 2.0;

//==============================================================================
/// 구운 비트맵. (길이 단위는 배율을 곱한 픽셀)
//==============================================================================
pub struct SystemFontBitmap {
	pub bitmap_width: i32,
	pub bitmap_height: i32,
	/// 프리멀티플라이드 RGBA. (글자는 흰색이라 fontdue 길과 같은 꼴, 색 이모지는 제 색)
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

//==============================================================================
/// 문서에 올린 FontFace 하나. (이름, 글꼴(같은 글꼴인지 Rc 자리로 본다), 서술자, FontFace)
//==============================================================================
struct BrowserFontFace {
	family: String,
	font: Rc<LoadedFont>,
	descriptors: FontFaceDescriptors,
	font_face: JsValue,
}

//==============================================================================
/// 캔버스로 구운 그림. (곧은 알파 RGBA, 캔버스 getImageData 그대로)
//==============================================================================
pub struct BrowserBakeBitmap {
	pub pixels: Vec<u8>,
	pub width: u32,
	pub height: u32,
}

//==============================================================================
/// 캔버스로 구운 그림자 글. (길이 단위는 배율을 곱한 픽셀, 원본 ShadowText 의 bakeMetrics)
//==============================================================================
pub struct BrowserShadowTextBake {
	pub bitmap: BrowserBakeBitmap,
	pub pen_x: f32,
	pub baseline_y: f32,
	pub advance_width: f32,
	pub font_ascent: f32,
	pub font_descent: f32,
}

thread_local! {
	/// 굽기와 재기에 쓰는 캔버스. (처음 쓸 때 만든다. 못 만들면 비어 있다)
	static SYSTEM_FONT_CANVAS: RefCell<Option<SystemFontCanvas>> = const { RefCell::new(None) };
	/// 문서에 올린 FontFace 들. (올린 차례)
	static BROWSER_FONT_FACE_LIST: RefCell<Vec<BrowserFontFace>> = const { RefCell::new(Vec::new()) };
	/// 폰트 문자열 이름 뒤, 시스템 글꼴 앞에 붙이는 이름. (기본 폰트, 대체 폰트)
	static EXTRA_FAMILY_LIST: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
	/// 올린 글꼴이 바뀌거나 받아진 횟수.
	static FONT_CHANGE_COUNT: Cell<u64> = const { Cell::new(0) };
	/// document.fonts 의 loadingdone 을 듣는지.
	static IS_LOADING_DONE_LISTENED: Cell<bool> = const { Cell::new(false) };
}

//==============================================================================
/// 올린 글꼴이 바뀌거나 받아진 횟수. (글자 캐시가 늘었으면 구운 글을 버린다)
//==============================================================================
pub fn read_font_change_count() -> u64 {
	FONT_CHANGE_COUNT.with(|font_change_count| font_change_count.get())
}

//==============================================================================
/// 글꼴 얼굴들을 문서에 올리기. ((이름, 서술자, 글꼴) 목록과 덧붙일 이름 목록, 원본 document.fonts.add)
/// - 지금 올린 것과 차례까지 같으면 아무것도 하지 않는다. 다르면 모두 내리고 차례대로 다시 올린다. (같은 글꼴의 FontFace 는 다시 쓴다)
///   같은 이름 안에서는 나중에 올린 얼굴이 먼저 그린다. (CSS 와 같다)
//==============================================================================
pub fn sync_font_faces(request_list: &[(String, FontFaceDescriptors, Rc<LoadedFont>)], extra_family_list: Vec<String>) {
	EXTRA_FAMILY_LIST.with(|extra_family_list_cell| {
		*extra_family_list_cell.borrow_mut() = extra_family_list;
	});
	let font_face_set = match web_sys::window().and_then(|window| window.document()).and_then(|document| js_sys::Reflect::get(&document, &JsValue::from_str("fonts")).ok()) {
		Some(font_face_set) => {
			font_face_set
		}
		None => {
			return;
		}
	};
	listen_loading_done(&font_face_set);
	BROWSER_FONT_FACE_LIST.with(|browser_font_face_list_cell| {
		let mut browser_font_face_list = browser_font_face_list_cell.borrow_mut();
		let is_same = browser_font_face_list.len() == request_list.len() && browser_font_face_list.iter().zip(request_list.iter()).all(|(browser_font_face, (family, descriptors, font))| browser_font_face.family == *family && Rc::ptr_eq(&browser_font_face.font, font) && browser_font_face.descriptors == *descriptors);
		if is_same {
			return;
		}
		let delete_function = js_sys::Reflect::get(&font_face_set, &JsValue::from_str("delete")).ok().and_then(|value| value.dyn_into::<js_sys::Function>().ok());
		let add_function = js_sys::Reflect::get(&font_face_set, &JsValue::from_str("add")).ok().and_then(|value| value.dyn_into::<js_sys::Function>().ok());
		let (delete_function, add_function) = match (delete_function, add_function) {
			(Some(delete_function), Some(add_function)) => {
				(delete_function, add_function)
			}
			_ => {
				return;
			}
		};
		let previous_list = std::mem::take(&mut *browser_font_face_list);
		for browser_font_face in previous_list.iter() {
			let _ = delete_function.call1(&font_face_set, &browser_font_face.font_face);
		}
		for (family, descriptors, font) in request_list {
			let reused_face = previous_list.iter().find(|browser_font_face| browser_font_face.family == *family && Rc::ptr_eq(&browser_font_face.font, font) && browser_font_face.descriptors == *descriptors).map(|browser_font_face| browser_font_face.font_face.clone());
			let font_face = match reused_face.or_else(|| create_font_face(family, descriptors, font)) {
				Some(font_face) => {
					font_face
				}
				None => {
					continue;
				}
			};
			let _ = add_function.call1(&font_face_set, &font_face);
			browser_font_face_list.push(BrowserFontFace {
				family: family.clone(),
				font: font.clone(),
				descriptors: descriptors.clone(),
				font_face,
			});
		}
	});
	FONT_CHANGE_COUNT.with(|font_change_count| font_change_count.set(font_change_count.get() + 1));
}

//==============================================================================
/// FontFace 만들기. (원본 new FontFace(family, source, descriptors), 바이트로 만든다)
//==============================================================================
fn create_font_face(family: &str, descriptors: &FontFaceDescriptors, font: &LoadedFont) -> Option<JsValue> {
	let font_face_class: js_sys::Function = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("FontFace")).ok()?.dyn_into().ok()?;
	let byte_array = js_sys::Uint8Array::from(&font.get_font_bytes()[..]);
	let descriptor_object = js_sys::Object::new();
	let weight_text = if descriptors.minimum_weight == descriptors.maximum_weight {
		descriptors.minimum_weight.to_string()
	}
	else {
		format!("{} {}", descriptors.minimum_weight, descriptors.maximum_weight)
	};
	js_sys::Reflect::set(&descriptor_object, &JsValue::from_str("weight"), &JsValue::from_str(&weight_text)).ok()?;
	js_sys::Reflect::set(&descriptor_object, &JsValue::from_str("style"), &JsValue::from_str(if descriptors.is_italic { "italic" } else { "normal" })).ok()?;
	if !descriptors.unicode_range_list.is_empty() {
		let unicode_range_text = descriptors.unicode_range_list.iter().map(|(start, end)| format!("U+{:X}-{:X}", start, end)).collect::<Vec<String>>().join(", ");
		js_sys::Reflect::set(&descriptor_object, &JsValue::from_str("unicodeRange"), &JsValue::from_str(&unicode_range_text)).ok()?;
	}
	let argument_list = js_sys::Array::of3(&JsValue::from_str(family), &byte_array.buffer(), &descriptor_object);
	js_sys::Reflect::construct(&font_face_class, &argument_list).ok()
}

//==============================================================================
/// document.fonts 의 loadingdone 듣기. (한 번만, 받아질 때마다 바뀐 횟수를 늘린다)
//==============================================================================
fn listen_loading_done(font_face_set: &JsValue) {
	if IS_LOADING_DONE_LISTENED.with(|is_listened| is_listened.replace(true)) {
		return;
	}
	let loading_done_closure = Closure::<dyn FnMut()>::new(|| {
		FONT_CHANGE_COUNT.with(|font_change_count| font_change_count.set(font_change_count.get() + 1));
	});
	let add_event_listener = js_sys::Reflect::get(font_face_set, &JsValue::from_str("addEventListener")).ok().and_then(|value| value.dyn_into::<js_sys::Function>().ok());
	if let Some(add_event_listener) = add_event_listener {
		let _ = add_event_listener.call2(font_face_set, &JsValue::from_str("loadingdone"), loading_done_closure.as_ref());
	}
	loading_done_closure.forget();
}

//==============================================================================
/// 색을 캔버스의 rgba 글로. (원본 buildRGBAString)
//==============================================================================
fn build_rgba_string(color: Color) -> String {
	format!("rgba({}, {}, {}, {})", (color.red * 255.0).round().clamp(0.0, 255.0), (color.green * 255.0).round().clamp(0.0, 255.0), (color.blue * 255.0).round().clamp(0.0, 255.0), color.alpha.clamp(0.0, 1.0))
}

//==============================================================================
/// 굽기용 새 캔버스. (원본처럼 구울 때마다 새로 만든다, 픽셀을 바로 읽으니 willReadFrequently)
//==============================================================================
fn create_bake_canvas(width: u32, height: u32) -> Option<(web_sys::HtmlCanvasElement, web_sys::CanvasRenderingContext2d)> {
	let document = web_sys::window()?.document()?;
	let canvas: web_sys::HtmlCanvasElement = document.create_element("canvas").ok()?.dyn_into().ok()?;
	canvas.set_width(width);
	canvas.set_height(height);
	let context_options = js_sys::Object::new();
	js_sys::Reflect::set(&context_options, &JsValue::from_str("willReadFrequently"), &JsValue::TRUE).ok()?;
	let context: web_sys::CanvasRenderingContext2d = canvas.get_context_with_context_options("2d", &context_options).ok()??.dyn_into().ok()?;
	Some((canvas, context))
}

//==============================================================================
/// 그림자 글 굽기. (원본 ShadowText.bakeText 와 같은 셈, font_description 의 크기는 맞춰 줄인 크기. 그림자 내림과 번짐은 내용 크기 단위)
//==============================================================================
#[allow(clippy::too_many_arguments)]
pub fn bake_shadow_text(text: &str, font_description: &FontDescription, bake_scale: f32, direction: ShapingDirection, text_color: Color, shadow_color: Color, shadow_offset: f32, shadow_blur: f32, is_underline: bool, is_strikethrough: bool) -> Option<BrowserShadowTextBake> {
	let scaled_font_string = build_css_font_string(font_description, bake_scale);
	let direction_name = match direction {
		ShapingDirection::LeftToRight => {
			"ltr"
		}
		ShapingDirection::RightToLeft => {
			"rtl"
		}
	};
	let (measure_canvas, measure_context) = create_bake_canvas(1, 1)?;
	measure_context.set_font(&scaled_font_string);
	set_canvas_direction(&measure_context, direction_name);
	measure_context.set_text_align("left");
	let text_metrics = measure_context.measure_text(text).ok()?;
	drop(measure_canvas);

	// 글자의 실제 경계와 글꼴의 위아래 경계.
	let advance_width = text_metrics.width() as f32;
	let actual_left = read_finite_value(text_metrics.actual_bounding_box_left(), 0.0).max(0.0).ceil();
	let actual_right = read_finite_value(text_metrics.actual_bounding_box_right(), advance_width).max(0.0).ceil();
	let (scaled_font_ascent, scaled_font_descent) = read_font_bounding_box(&text_metrics);
	let scaled_shadow_offset = shadow_offset * bake_scale;
	let scaled_shadow_blur = shadow_blur * bake_scale;

	// 번지는 그림자가 잘리지 않게 사방을 번지는 만큼 넓힌다.
	let bake_padding = SHADOW_BAKE_PADDING + scaled_shadow_blur.ceil();
	let scaled_pen_x = bake_padding + actual_left;
	let scaled_baseline_y = bake_padding + scaled_font_ascent.ceil();
	let bake_width = scaled_pen_x + actual_right + bake_padding;
	// 밑줄은 기준선 아래로 내려가므로 그만큼 여유를 더 둔다.
	let font_size = font_description.font_size;
	let scaled_decoration_line_width = (font_size / 16.0).max(1.0) * bake_scale;
	let scaled_middle_y = scaled_baseline_y - (scaled_font_ascent - scaled_font_descent) * 0.5;
	let scaled_underline_y = scaled_middle_y + (font_size * 0.5 + 2.0) * bake_scale;
	let mut bake_height = scaled_baseline_y + (scaled_font_descent + scaled_shadow_offset).ceil() + bake_padding;
	if is_underline {
		bake_height = bake_height.max((scaled_underline_y + scaled_decoration_line_width * 0.5 + scaled_shadow_offset).ceil() + bake_padding);
	}
	if bake_width <= 0.0 || bake_height <= 0.0 {
		return None;
	}
	let width = bake_width as u32;
	let height = bake_height as u32;
	let (_bake_canvas, context) = create_bake_canvas(width, height)?;
	context.set_font(&scaled_font_string);
	set_canvas_direction(&context, direction_name);
	context.set_text_align("left");
	context.set_text_baseline("alphabetic");
	context.set_shadow_color(&build_rgba_string(shadow_color));
	context.set_shadow_blur(scaled_shadow_blur as f64);
	context.set_shadow_offset_x(0.0);
	context.set_shadow_offset_y(scaled_shadow_offset as f64);
	context.set_fill_style_str(&build_rgba_string(text_color));
	context.fill_text(text, scaled_pen_x as f64, scaled_baseline_y as f64).ok()?;
	// 밑줄과 취소선도 같은 그림자로 칠한다. (길이는 전진폭, Text 의 가운데 기준선과 같은 자리)
	if is_underline {
		context.fill_rect(scaled_pen_x as f64, (scaled_underline_y - scaled_decoration_line_width * 0.5) as f64, advance_width as f64, scaled_decoration_line_width as f64);
	}
	if is_strikethrough {
		context.fill_rect(scaled_pen_x as f64, (scaled_middle_y - scaled_decoration_line_width * 0.5) as f64, advance_width as f64, scaled_decoration_line_width as f64);
	}
	let image_data = context.get_image_data(0.0, 0.0, width as f64, height as f64).ok()?;
	Some(BrowserShadowTextBake {
		bitmap: BrowserBakeBitmap {
			pixels: image_data.data().0,
			width,
			height,
		},
		pen_x: scaled_pen_x,
		baseline_y: scaled_baseline_y,
		advance_width,
		font_ascent: scaled_font_ascent,
		font_descent: scaled_font_descent,
	})
}

//==============================================================================
/// 그림자 그림 굽기. (원본 ShadowImage.bakeImage, readScaledImage 와 같다. 그림은 프리멀티플라이드 RGBA, 길이는 배율을 곱한 픽셀)
/// - 그림의 조각을 imageSmoothingQuality high 로 그릴 크기에 그리고, 색이 있으면 source-in 으로 칠한 뒤, 그 밑에 캔버스 그림자를 깔아 한 장으로.
//==============================================================================
#[allow(clippy::too_many_arguments)]
pub fn bake_shadow_image(source_pixels: &[u8], source_width: u32, source_height: u32, source_rect: [f32; 4], scaled_width: u32, scaled_height: u32, image_color: Option<Color>, shadow_color: Color, scaled_shadow_offset: f32, scaled_shadow_blur: f32) -> Option<BrowserBakeBitmap> {
	// 그림 픽셀을 캔버스에. (putImageData 는 곧은 알파)
	let mut straight_pixels = source_pixels.to_vec();
	for pixel in straight_pixels.chunks_exact_mut(4) {
		let alpha = pixel[3] as u32;
		if alpha > 0 && alpha < 255 {
			for channel_index in 0..3 {
				pixel[channel_index] = ((pixel[channel_index] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
			}
		}
	}
	let (source_canvas, source_context) = create_bake_canvas(source_width, source_height)?;
	let source_image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(wasm_bindgen::Clamped(&straight_pixels), source_width, source_height).ok()?;
	source_context.put_image_data(&source_image_data, 0.0, 0.0).ok()?;

	// 그릴 크기로 줄인 그림. (원본 readScaledImage)
	let (scaled_canvas, scaled_context) = create_bake_canvas(scaled_width, scaled_height)?;
	scaled_context.set_image_smoothing_enabled(true);
	let _ = js_sys::Reflect::set(&scaled_context, &JsValue::from_str("imageSmoothingQuality"), &JsValue::from_str("high"));
	scaled_context.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(&source_canvas, source_rect[0] as f64, source_rect[1] as f64, source_rect[2] as f64, source_rect[3] as f64, 0.0, 0.0, scaled_width as f64, scaled_height as f64).ok()?;
	if let Some(image_color) = image_color {
		scaled_context.set_global_composite_operation("source-in").ok()?;
		scaled_context.set_fill_style_str(&build_rgba_string(image_color));
		scaled_context.fill_rect(0.0, 0.0, scaled_width as f64, scaled_height as f64);
	}

	// 번지는 그림자가 잘리지 않게 사방을 번지는 만큼 넓힌다. (원본 bakeImage)
	let bake_padding = SHADOW_BAKE_PADDING + scaled_shadow_blur.ceil();
	let width = scaled_width + (bake_padding * 2.0) as u32;
	let height = scaled_height + (bake_padding * 2.0) as u32 + scaled_shadow_offset.max(0.0).ceil() as u32;
	let (_bake_canvas, context) = create_bake_canvas(width, height)?;
	context.set_shadow_color(&build_rgba_string(shadow_color));
	context.set_shadow_blur(scaled_shadow_blur as f64);
	context.set_shadow_offset_x(0.0);
	context.set_shadow_offset_y(scaled_shadow_offset as f64);
	context.draw_image_with_html_canvas_element(&scaled_canvas, bake_padding as f64, bake_padding as f64).ok()?;
	let image_data = context.get_image_data(0.0, 0.0, width as f64, height as f64).ok()?;
	Some(BrowserBakeBitmap {
		pixels: image_data.data().0,
		width,
		height,
	})
}

//==============================================================================
/// 문자열 재기. (원본 measureText)
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

thread_local! {
	/// 브라우저가 놓은 클래식 스크롤 막대 두께. ((얇은지) → px, 0 이면 얹는 막대)
	static CLASSIC_SCROLLBAR_THICKNESS_TABLE: RefCell<std::collections::HashMap<bool, f32>> = RefCell::new(std::collections::HashMap::new());
}

//==============================================================================
/// 브라우저가 놓는 클래식 스크롤 막대 두께 재기. (px, 얹는 막대거나 문서가 없으면 0)
/// - 문서 밖에 숨긴 overflow: scroll 요소의 offsetWidth - clientWidth 다. (scrollbar-width: thin 이면 그 값으로)
//==============================================================================
pub fn measure_classic_scrollbar_thickness(is_thin: bool) -> f32 {
	if let Some(thickness) = CLASSIC_SCROLLBAR_THICKNESS_TABLE.with(|table_cell| table_cell.borrow().get(&is_thin).copied()) {
		return thickness;
	}
	let measure = || -> Option<f32> {
		let document = web_sys::window()?.document()?;
		let body = document.body()?;
		let probe_element = document.create_element("div").ok()?.dyn_into::<web_sys::HtmlElement>().ok()?;
		let probe_style = probe_element.style();
		let scrollbar_width = if is_thin {
			"thin"
		}
		else {
			"auto"
		};
		for (property_name, property_value) in [("position", "absolute"), ("visibility", "hidden"), ("left", "-10000px"), ("top", "-10000px"), ("width", "100px"), ("height", "100px"), ("overflow", "scroll"), ("scrollbar-width", scrollbar_width)] {
			let _ = probe_style.set_property(property_name, property_value);
		}
		let _ = body.append_child(&probe_element);
		let thickness = (probe_element.offset_width() - probe_element.client_width()) as f32;
		probe_element.remove();
		Some(thickness.max(0.0))
	};
	let thickness = measure().unwrap_or(0.0);
	CLASSIC_SCROLLBAR_THICKNESS_TABLE.with(|table_cell| {
		table_cell.borrow_mut().insert(is_thin, thickness);
	});
	thickness
}

thread_local! {
	/// 브라우저가 놓은 line-height normal 의 줄 높이. ((CSS 폰트 문자열, 글꼴 바뀐 횟수) → px)
	static NORMAL_LINE_HEIGHT_TABLE: RefCell<std::collections::HashMap<(String, u64), f32>> = RefCell::new(std::collections::HashMap::new());
}

//==============================================================================
/// CSS line-height normal 의 줄 높이를 브라우저로 재기. (px, 문서가 없으면 None)
/// - 문서 밖에 숨긴 블록 요소에 그 폰트와 line-height: normal 로 글 한 줄을 놓고 높이를 읽는다. 블록의 한 줄 높이는 크롬이 첫째로 쓸 수 있는 글꼴의 LineSpacing 이다.
/// - 글꼴이 새로 받아지면(글꼴 바뀐 횟수) 다시 잰다.
//==============================================================================
pub fn measure_normal_line_height(font_description: &FontDescription) -> Option<f32> {
	let font_string = build_css_font_string(font_description, 1.0);
	let table_key = (font_string.clone(), read_font_change_count());
	if let Some(line_height) = NORMAL_LINE_HEIGHT_TABLE.with(|table_cell| table_cell.borrow().get(&table_key).copied()) {
		return Some(line_height);
	}
	let document = web_sys::window()?.document()?;
	let body = document.body()?;
	let probe_element = document.create_element("div").ok()?.dyn_into::<web_sys::HtmlElement>().ok()?;
	let probe_style = probe_element.style();
	for (property_name, property_value) in [("position", "absolute"), ("visibility", "hidden"), ("left", "-10000px"), ("top", "-10000px"), ("margin", "0"), ("padding", "0"), ("border", "0"), ("white-space", "pre"), ("line-height", "normal")] {
		let _ = probe_style.set_property(property_name, property_value);
	}
	let _ = probe_style.set_property("font", &font_string);
	probe_element.set_text_content(Some("x"));
	let _ = body.append_child(&probe_element);
	let line_height = probe_element.get_bounding_client_rect().height() as f32;
	probe_element.remove();
	if line_height <= 0.0 {
		return None;
	}
	NORMAL_LINE_HEIGHT_TABLE.with(|table_cell| {
		table_cell.borrow_mut().insert(table_key, line_height);
	});
	Some(line_height)
}

thread_local! {
	/// 브라우저로 푼 글자 크기. (폰트 문자열과 창 크기 → px, 틀린 문자열이면 None)
	static RESOLVED_FONT_SIZE_TABLE: RefCell<std::collections::HashMap<String, Option<f32>>> = RefCell::new(std::collections::HashMap::new());
}

/// 틀린 문자열을 가리려고 먼저 넣어 두는 폰트. (브라우저는 틀린 font 를 받지 않고 앞의 값을 둔다)
const FONT_SIZE_SENTINEL: &str = "7.25px vanilla-font-size-sentinel";

//==============================================================================
/// 폰트 문자열의 글자 크기를 브라우저로 풀기. (px, 틀린 문자열이면 None)
/// - 원본은 글꼴에 대는 단위(ex, ch, cap, ic, lh, rem 같은), 뷰포트 단위, calc() 를 브라우저 캔버스의 font 가 셈한다.
///   원본처럼 문서에 붙이지 않은 캔버스의 font 에 넣고 다시 읽어 같은 값을 받는다. 한 번 푼 문자열은 창 크기와 함께 기억한다.
//==============================================================================
pub fn resolve_font_size(font_string: &str) -> Option<f32> {
	let window = web_sys::window()?;
	let viewport_width = window.inner_width().ok().and_then(|value| value.as_f64()).unwrap_or(0.0);
	let viewport_height = window.inner_height().ok().and_then(|value| value.as_f64()).unwrap_or(0.0);
	let cache_key = format!("{}|{}x{}", font_string, viewport_width, viewport_height);
	if let Some(font_size) = RESOLVED_FONT_SIZE_TABLE.with(|table_cell| table_cell.borrow().get(&cache_key).copied()) {
		return font_size;
	}
	let font_size = with_canvas(|system_font_canvas| {
		system_font_canvas.context.set_font(FONT_SIZE_SENTINEL);
		system_font_canvas.context.set_font(font_string);
		let serialized_font = system_font_canvas.context.font();
		if serialized_font == FONT_SIZE_SENTINEL {
			return Some(None);
		}
		let font_size = serialized_font.split_whitespace().find_map(|token| token.strip_suffix("px").and_then(|number_text| number_text.parse::<f32>().ok()));
		Some(font_size)
	}).flatten();
	RESOLVED_FONT_SIZE_TABLE.with(|table_cell| {
		table_cell.borrow_mut().insert(cache_key, font_size);
	});
	font_size
}

//==============================================================================
/// 문자열 굽기. (원본 bakeEntry. 크기와 펜 자리 계산이 원본과 같다, direction 은 캔버스의 direction)
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
/// 시스템 글꼴 조각 재기. (올린 글꼴에 없는 글자 묶음, 크기는 픽셀. 원본 브라우저의 글꼴 대체 자리)
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
/// 시스템 글꼴 조각 래스터. (덮인 정도, 왼쪽은 펜 자리에서, 위는 기준선에서. 크기는 픽셀)
/// - 흰 픽셀은 글자의 덮인 정도로, 흰색이 아닌 픽셀(브라우저가 그린 색 이모지)은 색 몫(color_pixels)으로 나눈다.
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
/// CSS 폰트 문자열 만들기. ("italic bold 32px "이름", 시스템 글꼴 목록")
/// - 이름 목록을 차례로 앞에 두고 기본 폰트와 대체 폰트의 이름(sync_font_faces)을 뒤에 붙인다. 시스템 글꼴 목록(SYSTEM_FONT_STRING)은 default 를 청했을 때(목록이 비면 그때도)만 맨 뒤에 붙인다.
///   비었거나 "default" 인 이름은 빼고, 일반 글꼴 이름(sans-serif 등)은 따옴표 없이 둔다.
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
	let mut family_text_list: Vec<String> = Vec::new();
	for family_name in &font_description.family_name_list {
		let family_name = family_name.trim();
		if family_name.is_empty() || family_name.eq_ignore_ascii_case("default") {
			continue;
		}
		if is_generic_family_name(family_name) {
			family_text_list.push(family_name.to_string());
		}
		else {
			family_text_list.push(format!("\"{}\"", family_name.replace('"', "")));
		}
	}
	EXTRA_FAMILY_LIST.with(|extra_family_list_cell| {
		for extra_family_name in extra_family_list_cell.borrow().iter() {
			family_text_list.push(format!("\"{}\"", extra_family_name));
		}
	});
	// 원본 SYSTEM_FONT_STRING 은 기본 글꼴 이름 default 를 청했을 때만. (원본도 기본 글꼴일 때만 폰트 문자열이 그 목록이고, 다른 이름은 브라우저가 그 목록 없이 시스템 대체로 간다. 데스크탑 사슬과 같다)
	let is_default_requested = font_description.family_name_list.iter().any(|family_name| family_name.trim().eq_ignore_ascii_case("default"));
	if is_default_requested || family_text_list.is_empty() {
		family_text_list.push(SYSTEM_FONT_STRING.to_string());
	}
	format!("{}{}{}px {}", style_text, weight_text, pixel_size, family_text_list.join(", "))
}

//==============================================================================
/// CSS 일반 글꼴 이름인지. (따옴표로 싸면 그 이름의 글꼴을 찾으므로 그대로 둔다)
//==============================================================================
fn is_generic_family_name(family_name: &str) -> bool {
	let lowercase_family_name = family_name.to_lowercase();
	matches!(lowercase_family_name.as_str(), "serif" | "sans-serif" | "monospace" | "cursive" | "fantasy" | "system-ui" | "ui-serif" | "ui-sans-serif" | "ui-monospace" | "ui-rounded" | "math" | "emoji" | "fangsong" | "-apple-system" | "blinkmacsystemfont")
}

//==============================================================================
/// 캔버스의 글 방향 넣기. (web-sys 에 direction 세터가 없는 판도 있어 속성으로 넣는다)
//==============================================================================
fn set_canvas_direction(context: &web_sys::CanvasRenderingContext2d, direction_name: &str) {
	let _ = js_sys::Reflect::set(context, &JsValue::from_str("direction"), &JsValue::from_str(direction_name));
}

//==============================================================================
/// 폰트 위아래 경계. (fontBoundingBox 를 모르는 브라우저는 실측 경계로, 그것도 없으면 크기로 어림)
//==============================================================================
fn read_font_bounding_box(text_metrics: &web_sys::TextMetrics) -> (f32, f32) {
	let actual_ascent = read_finite_value(text_metrics.actual_bounding_box_ascent(), 0.0);
	let actual_descent = read_finite_value(text_metrics.actual_bounding_box_descent(), 0.0);
	let font_ascent = read_finite_value(text_metrics.font_bounding_box_ascent(), actual_ascent);
	let font_descent = read_finite_value(text_metrics.font_bounding_box_descent(), actual_descent);
	(font_ascent, font_descent)
}

//==============================================================================
/// 브라우저가 모르는 값(undefined 는 NaN 으로 온다)이면 대신 값.
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
/// 캔버스를 빌려 일하기. (없으면 만든다)
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
/// 캔버스 만들기. (굽고 나서 픽셀을 바로 읽으니 willReadFrequently 를 켠다)
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

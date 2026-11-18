//==============================================================================
// 웹 시스템 글꼴로 글자 굽기. (wasm32)
// - 원본 textstringtexturecache.js 의 bakeEntry, measureText 를 옮긴 것이다. 원본은 늘 오프스크린 2D 캔버스에
//   fillText / strokeText 로 굽는다. 그래서 글꼴을 따로 주지 않아도 브라우저가 고른 시스템 글꼴로 글자가 나온다.
// - 러스트 판은 글꼴 바이트(defaultFontBytes, defaultFontUrl, FontAsset)가 하나도 없을 때만 여기로 온다.
//   글꼴 바이트가 있으면 데스크탑과 같은 fontdue 길로 굽는다.
// - 글꼴 목록은 폰트 문자열의 이름을 앞에 두고 원본 SYSTEM_FONT_STRING 을 뒤에 붙인다.
//   (CSS @font-face 로 문서에 올린 글꼴 이름이면 그 글꼴이, 아니면 시스템 글꼴이 나온다)
// - 캔버스는 처음 쓸 때 하나 만들어 굽기와 재기에 같이 쓴다. 문서에는 붙이지 않는다.
//==============================================================================
use crate::base::platform::SYSTEM_FONT_STRING;
use crate::core::textstringtexturecache::{FontDescription, TextDrawKind, TextMetrics};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, JsValue};

//==============================================================================
// 구운 비트맵. (길이 단위는 배율을 곱한 픽셀)
//==============================================================================
pub struct SystemFontBitmap {
	pub bitmapWidth: i32,
	pub bitmapHeight: i32,
	// 흰색 프리멀티플라이드 RGBA. (fontdue 길과 같은 꼴)
	pub pixels: Vec<u8>,
	pub penX: f32,
	pub baselineY: f32,
	pub advanceWidth: f32,
	pub fontAscent: f32,
	pub fontDescent: f32,
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
// 문자열 재기. (원본 measureText)
//==============================================================================
pub fn measureText(fontDescription: &FontDescription, text: &str) -> TextMetrics {
	let fontString = buildCssFontString(fontDescription, 1.0);
	let measured = withCanvas(|systemFontCanvas| {
		systemFontCanvas.context.set_font(&fontString);
		let textMetrics = systemFontCanvas.context.measure_text(text).ok()?;
		let (fontAscent, fontDescent) = readFontBoundingBox(&textMetrics);
		Some(TextMetrics {
			width: textMetrics.width() as f32,
			fontAscent,
			fontDescent,
		})
	});
	measured.unwrap_or_default()
}

//==============================================================================
// 문자열 굽기. (원본 bakeEntry. 크기와 펜 자리 계산이 원본과 같다)
//==============================================================================
pub fn bakeText(textDrawKind: TextDrawKind, text: &str, fontDescription: &FontDescription, lineWidth: f32, bakeScale: f32) -> Option<SystemFontBitmap> {
	let fontString = buildCssFontString(fontDescription, bakeScale);
	withCanvas(|systemFontCanvas| {
		let canvas = &systemFontCanvas.canvas;
		let context = &systemFontCanvas.context;

		// 배율을 곱한 폰트로 잰다. (경계는 왼쪽 기준)
		context.set_font(&fontString);
		context.set_text_align("left");
		let textMetrics = context.measure_text(text).ok()?;

		// 글리프 실측 경계. (측정 미지원 브라우저는 전진폭으로 어림)
		let advanceWidth = textMetrics.width() as f32;
		let actualLeft = readFiniteValue(textMetrics.actual_bounding_box_left(), 0.0).max(0.0).ceil();
		let actualRight = readFiniteValue(textMetrics.actual_bounding_box_right(), advanceWidth).max(0.0).ceil();
		let (fontAscent, fontDescent) = readFontBoundingBox(&textMetrics);

		// 외곽선 두께와 안티앨리어싱 여유.
		let strokeWidth = match textDrawKind {
			TextDrawKind::stroke => {
				lineWidth * bakeScale
			}
			TextDrawKind::fill => {
				0.0
			}
		};
		let padding = (strokeWidth / 2.0).ceil() + 2.0;

		// 굽기 자리와 캔버스 크기.
		let penX = padding + actualLeft;
		let baselineY = padding + fontAscent.ceil();
		let bitmapWidth = (penX + actualRight + padding) as i32;
		let bitmapHeight = (baselineY + fontDescent.ceil() + padding) as i32;
		if bitmapWidth <= 0 || bitmapHeight <= 0 {
			return None;
		}

		// 캔버스 크기를 바꾸면 그리기 상태가 처음으로 돌아가서 폰트를 다시 넣는다.
		canvas.set_width(bitmapWidth as u32);
		canvas.set_height(bitmapHeight as u32);
		context.clear_rect(0.0, 0.0, bitmapWidth as f64, bitmapHeight as f64);
		context.set_font(&fontString);
		context.set_text_align("left");
		context.set_text_baseline("alphabetic");
		match textDrawKind {
			TextDrawKind::stroke => {
				context.set_stroke_style_str("#ffffff");
				context.set_line_width(strokeWidth as f64);
				context.stroke_text(text, penX as f64, baselineY as f64).ok()?;
			}
			TextDrawKind::fill => {
				context.set_fill_style_str("#ffffff");
				context.fill_text(text, penX as f64, baselineY as f64).ok()?;
			}
		}

		// 알파만 꺼내 흰색 프리멀티플라이드로. (흰색으로 그렸으니 알파가 곧 덮인 정도다)
		let imageData = context.get_image_data(0.0, 0.0, bitmapWidth as f64, bitmapHeight as f64).ok()?;
		let mut pixels = imageData.data().0;
		for pixel in pixels.chunks_exact_mut(4) {
			let alpha = pixel[3];
			pixel[0] = alpha;
			pixel[1] = alpha;
			pixel[2] = alpha;
		}

		Some(SystemFontBitmap {
			bitmapWidth,
			bitmapHeight,
			pixels,
			penX,
			baselineY,
			advanceWidth,
			fontAscent,
			fontDescent,
		})
	})
}

//==============================================================================
// CSS 폰트 문자열 만들기. ("bold 32px "이름", 시스템 글꼴 목록")
// - 이름이 비었거나 "default" 면 시스템 글꼴 목록만 쓴다. 이름에 쉼표가 있으면 이미 목록이라 그대로 앞에 둔다.
//==============================================================================
fn buildCssFontString(fontDescription: &FontDescription, bakeScale: f32) -> String {
	let weightText = if fontDescription.isBold {
		"bold "
	}
	else {
		""
	};
	let pixelSize = fontDescription.fontSize * bakeScale;
	let familyName = fontDescription.familyName.trim();
	let familyList = if familyName.is_empty() || familyName.eq_ignore_ascii_case("default") {
		SYSTEM_FONT_STRING.to_string()
	}
	else if familyName.contains(',') {
		format!("{}, {}", familyName, SYSTEM_FONT_STRING)
	}
	else {
		format!("\"{}\", {}", familyName.replace('"', ""), SYSTEM_FONT_STRING)
	};
	format!("{}{}px {}", weightText, pixelSize, familyList)
}

//==============================================================================
// 폰트 위아래 경계. (fontBoundingBox 를 모르는 브라우저는 실측 경계로, 그것도 없으면 크기로 어림)
//==============================================================================
fn readFontBoundingBox(textMetrics: &web_sys::TextMetrics) -> (f32, f32) {
	let actualAscent = readFiniteValue(textMetrics.actual_bounding_box_ascent(), 0.0);
	let actualDescent = readFiniteValue(textMetrics.actual_bounding_box_descent(), 0.0);
	let fontAscent = readFiniteValue(textMetrics.font_bounding_box_ascent(), actualAscent);
	let fontDescent = readFiniteValue(textMetrics.font_bounding_box_descent(), actualDescent);
	(fontAscent, fontDescent)
}

//==============================================================================
// 브라우저가 모르는 값(undefined 는 NaN 으로 온다)이면 대신 값.
//==============================================================================
fn readFiniteValue(value: f64, fallbackValue: f32) -> f32 {
	if value.is_finite() {
		value as f32
	}
	else {
		fallbackValue
	}
}

//==============================================================================
// 캔버스를 빌려 일하기. (없으면 만든다)
//==============================================================================
fn withCanvas<Output>(work: impl FnOnce(&SystemFontCanvas) -> Option<Output>) -> Option<Output> {
	SYSTEM_FONT_CANVAS.with(|systemFontCanvasCell| {
		let mut systemFontCanvasSlot = systemFontCanvasCell.borrow_mut();
		if systemFontCanvasSlot.is_none() {
			*systemFontCanvasSlot = createCanvas();
		}
		let systemFontCanvas = systemFontCanvasSlot.as_ref()?;
		work(systemFontCanvas)
	})
}

//==============================================================================
// 캔버스 만들기. (굽고 나서 픽셀을 바로 읽으니 willReadFrequently 를 켠다)
//==============================================================================
fn createCanvas() -> Option<SystemFontCanvas> {
	let document = web_sys::window()?.document()?;
	let canvas: web_sys::HtmlCanvasElement = document.create_element("canvas").ok()?.dyn_into().ok()?;
	let contextOptions = js_sys::Object::new();
	js_sys::Reflect::set(&contextOptions, &JsValue::from_str("willReadFrequently"), &JsValue::TRUE).ok()?;
	let context: web_sys::CanvasRenderingContext2d = canvas.get_context_with_context_options("2d", &contextOptions).ok()??.dyn_into().ok()?;
	Some(SystemFontCanvas {
		canvas,
		context,
	})
}

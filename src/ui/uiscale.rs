//==============================================================================
// UI 배율. (vanilla.js 의 ui/uiscale.js, UIScale)
// - 뷰의 고정된 쪽(세로형은 세로, 가로형은 가로)은 늘 같은 길이라, 작은 화면(포털의 PC iframe 등)에서는
//   뷰 한 단위가 화면에서 1 CSS 픽셀보다 한참 작아져 글자가 읽히지 않는다. 그때 화면 위의 UI 층만 키울 배율을 셈한다. (게임의 그림은 그대로)
// - 한 단위가 minimum_css_scale CSS 픽셀보다 작으면 그만큼 키운다. (1 ~ maximum_scale)
//   - 낮은 화면은 키운 UI 층의 높이가 minimum_layer_height 보다 낮아지게는 키우지 않는다. (0 이면 보지 않음)
//   - 세로 화면(가로가 세로의 portrait_ratio 배 이하)은 상한 없이 한 단위가 minimum_css_scale CSS 픽셀이 되게 키운다.
// - 원본과 다른 것:
//   - 전역 엔진이 없어 한 단위의 CSS 픽셀(ViewManager::get_css_pixel_scale)을 인자로 받는다. (원본은 options.cssPixelScale 이 없으면 엔진에서 읽는다)
//   - options 객체는 UIScaleOptions 구조체다.
// - 사용: let ui_scale = UIScale::read(view_width, view_height, view_manager.get_css_pixel_scale(), &UIScaleOptions::default());
//==============================================================================

//==============================================================================
// UI 배율 설정. (원본 options = { minimumCssScale = 0.8, maximumScale = 1.8, minimumLayerHeight = 0, portraitRatio = 1.1 })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIScaleOptions {
	// 한 단위가 이 CSS 픽셀보다 작으면 키운다.
	pub minimum_css_scale: f32,
	// 키우는 상한. (세로 화면은 받지 않는다)
	pub maximum_scale: f32,
	// 키운 UI 층이 지키는 가장 낮은 높이. (0 이면 보지 않는다)
	pub minimum_layer_height: f32,
	// 가로가 세로의 이 배 이하면 세로 화면.
	pub portrait_ratio: f32,
}

impl Default for UIScaleOptions {
	//==============================================================================
	// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UIScaleOptions {
		UIScaleOptions {
			minimum_css_scale: 0.8,
			maximum_scale: 1.8,
			minimum_layer_height: 0.0,
			portrait_ratio: 1.1,
		}
	}
}

//==============================================================================
// UI 배율.
//==============================================================================
pub struct UIScale;

impl UIScale {
	//==============================================================================
	// UI 배율 읽기. (view_width, view_height 는 UI 층이 덮는 영역, css_pixel_scale 은 뷰 한 단위의 CSS 픽셀)
	//==============================================================================
	pub fn read(view_width: f32, view_height: f32, css_pixel_scale: f32, options: &UIScaleOptions) -> f32 {
		if !(css_pixel_scale > 0.0) {
			return 1.0;
		}
		let desired_scale = options.minimum_css_scale / css_pixel_scale;
		if UIScale::is_portrait(view_width, view_height, options.portrait_ratio) {
			return desired_scale.max(1.0);
		}
		let mut ui_scale = desired_scale.min(options.maximum_scale);
		if options.minimum_layer_height > 0.0 && view_height > 0.0 {
			ui_scale = ui_scale.min(view_height / options.minimum_layer_height);
		}
		ui_scale.max(1.0)
	}

	//==============================================================================
	// 세로 화면인지. (가로가 세로의 portrait_ratio 배 이하)
	//==============================================================================
	pub fn is_portrait(view_width: f32, view_height: f32, portrait_ratio: f32) -> bool {
		view_width <= view_height * portrait_ratio
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 넉넉하면 1, 작으면 키우되 상한, 낮은 화면은 층 높이까지만, 세로 화면은 상한 없음, 못 재면 1.
	//==============================================================================
	#[test]
	fn read_follows_css_pixel_scale() {
		let options = UIScaleOptions::default();
		assert_eq!(UIScale::read(1920.0, 1080.0, 1.0, &options), 1.0);
		assert!((UIScale::read(1920.0, 1080.0, 0.5, &options) - 1.6).abs() < 0.0001);
		assert!((UIScale::read(1920.0, 1080.0, 0.2, &options) - 1.8).abs() < 0.0001);
		let low_options = UIScaleOptions { minimum_layer_height: 900.0, ..UIScaleOptions::default() };
		assert!((UIScale::read(1920.0, 1080.0, 0.4, &low_options) - 1.2).abs() < 0.0001);
		assert!((UIScale::read(1080.0, 1920.0, 0.2, &options) - 4.0).abs() < 0.0001);
		assert!(UIScale::is_portrait(1080.0, 1920.0, 1.1));
		assert!(!UIScale::is_portrait(1920.0, 1080.0, 1.1));
		assert_eq!(UIScale::read(1920.0, 1080.0, 0.0, &options), 1.0);
	}
}

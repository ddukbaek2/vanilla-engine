//==============================================================================
//! UI 배율. (vanilla.js 의 ui/uiscale.js, UIScale)
//! - 뷰의 고정된 쪽(세로형은 세로, 가로형은 가로)은 늘 같은 길이라, 작은 화면(포털의 PC iframe 등)에서는
//!   뷰 한 단위가 화면에서 1 CSS 픽셀보다 한참 작아져 글자가 읽히지 않는다. 그때 화면 위의 UI 층만 키울 배율을 셈한다. (게임의 그림은 그대로)
//! - 한 단위가 minimum_css_scale CSS 픽셀보다 작으면 그만큼 키운다. (1 ~ maximum_scale)
//!   - 낮은 화면은 키운 UI 층의 높이가 minimum_layer_height 보다 낮아지게는 키우지 않는다. (0 이면 보지 않음)
//!   - 세로 화면(가로가 세로의 portrait_ratio 배 이하)은 상한 없이 한 단위가 minimum_css_scale CSS 픽셀이 되게 키운다.
//! - 원본과 다른 것:
//!   - 한 단위의 CSS 픽셀(ViewManager::get_css_pixel_scale)을 인자로 받는다. (원본은 options.cssPixelScale 이 없으면 엔진에서 읽는다)
//!   - options 객체는 UIScaleOptions 구조체다.
//! - 사용: let ui_scale = UIScale::read(view_width, view_height, view_manager.get_css_pixel_scale(), &UIScaleOptions::default());
//==============================================================================

//==============================================================================
/// UI 배율 설정. (원본 options = { minimumCssScale = 0.8, maximumScale = 1.8, minimumLayerHeight = 0, portraitRatio = 1.1 })
///
/// # 예제
/// 낮은 화면에서 UI 층 높이 지키기
/// ```rust
/// use vanilla::{UIScale, UIScaleOptions};
///
/// let options = UIScaleOptions {
/// 	minimum_layer_height: 900.0,
/// 	..UIScaleOptions::default()
/// };
/// assert_eq!(options.maximum_scale, 1.8);
/// let ui_scale = UIScale::read(1920.0, 1080.0, 0.4, &options);
/// assert!((ui_scale - 1.2).abs() < 0.0001);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIScaleOptions {
	/// 한 단위가 이 CSS 픽셀보다 작으면 키운다.
	pub minimum_css_scale: f64,
	/// 키우는 상한. (세로 화면은 받지 않는다)
	pub maximum_scale: f64,
	/// 키운 UI 층이 지키는 가장 낮은 높이. (0 이면 보지 않는다)
	pub minimum_layer_height: f64,
	/// 가로가 세로의 이 배 이하면 세로 화면.
	pub portrait_ratio: f64,
}

impl Default for UIScaleOptions {
	//==============================================================================
	/// 기본값. (원본과 같다)
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
/// UI 배율.
///
/// # 예제
/// UI 층 노드를 화면에 맞춰 키우기
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, NodeTree, UIScale, UIScaleOptions, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let layer_id = node_tree.create_child(root_id, "UILayer");
/// let view_manager = engine.get_view_manager();
/// let view_size = view_manager.get_view_size();
/// let ui_scale = UIScale::read(view_size.x, view_size.y, view_manager.get_css_pixel_scale(), &UIScaleOptions::default());
/// node_tree[layer_id].set_local_scale(Vector2::create(ui_scale, ui_scale));
/// assert!(ui_scale >= 1.0);
/// ```
//==============================================================================
pub struct UIScale;

impl UIScale {
	//==============================================================================
	/// UI 배율 읽기. (view_width, view_height 는 UI 층이 덮는 영역, css_pixel_scale 은 뷰 한 단위의 CSS 픽셀)
	///
	/// # 인자
	/// - `view_width`: UI 층이 덮는 영역의 너비. (뷰 단위)
	/// - `view_height`: UI 층이 덮는 영역의 높이. (뷰 단위)
	/// - `css_pixel_scale`: 뷰 한 단위가 화면에서 차지하는 CSS 픽셀. (ViewManager::get_css_pixel_scale, 0 보다 크지 않으면 셈하지 않는다)
	/// - `options`: 배율 설정.
	///
	/// # 돌려줌
	/// UI 층에 곱할 배율. (1 이상, css_pixel_scale 이 0 보다 크지 않으면 1, 세로 화면은 maximum_scale 을 넘을 수 있다)
	///
	/// # 예제
	/// 한 단위의 CSS 픽셀에 따른 배율
	/// ```rust
	/// use vanilla::{UIScale, UIScaleOptions};
	///
	/// let options = UIScaleOptions::default();
	/// assert_eq!(UIScale::read(1920.0, 1080.0, 1.0, &options), 1.0);
	/// assert!((UIScale::read(1920.0, 1080.0, 0.5, &options) - 1.6).abs() < 0.0001);
	/// assert!((UIScale::read(1920.0, 1080.0, 0.2, &options) - 1.8).abs() < 0.0001);
	/// assert!((UIScale::read(1080.0, 1920.0, 0.2, &options) - 4.0).abs() < 0.0001);
	/// assert_eq!(UIScale::read(1920.0, 1080.0, 0.0, &options), 1.0);
	/// ```
	//==============================================================================
	pub fn read(view_width: f64, view_height: f64, css_pixel_scale: f64, options: &UIScaleOptions) -> f64 {
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
	/// 세로 화면인지. (가로가 세로의 portrait_ratio 배 이하)
	///
	/// # 인자
	/// - `view_width`: 영역의 너비. (뷰 단위)
	/// - `view_height`: 영역의 높이. (뷰 단위)
	/// - `portrait_ratio`: 세로 화면으로 보는 가로 / 세로 비의 상한. (기본 설정은 1.1)
	///
	/// # 돌려줌
	/// 가로가 세로의 portrait_ratio 배 이하면 true.
	//==============================================================================
	pub fn is_portrait(view_width: f64, view_height: f64, portrait_ratio: f64) -> bool {
		view_width <= view_height * portrait_ratio
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 넉넉하면 1, 작으면 키우되 상한, 낮은 화면은 층 높이까지만, 세로 화면은 상한 없음, 못 재면 1.
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

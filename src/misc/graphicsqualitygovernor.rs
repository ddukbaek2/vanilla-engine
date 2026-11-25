//==============================================================================
// 그래픽 품질 정하기. (vanilla.js misc/graphicsqualitygovernor.js 의 GraphicsQualityGovernor)
// - 설정과 프레임 재기로 가벼운 길을 쓸지 정하고, 렌더 배율 상한을 고른다.
// - 가벼운 길(is_lite)을 정하는 차례: 덮어쓰기(주소의 ?light 같은 것) → 설정 낮음(늘 가벼움), 높음(늘 아님) → 자동.
//   자동이면 처음부터 가벼운 길(set_auto_lite_default, 손가락 기기 같은 것)이거나 FrameProbe 가 처지는 기기로 잰 때 가벼운 길이다.
// - 매 프레임 update(프레임 초, 최대 프레임) 를 부르면 자동일 때만 FrameProbe 로 잰다. 가벼운 길이 바뀐 프레임에 true 를 돌려준다.
// - 렌더 배율 상한: set_render_pixel_ratio_limits(보통, 가벼운 길) 로 주고 apply_render_pixel_ratio(view_manager) 가 바뀔 때만 건다. (0 이면 제한 없음)
// - 가벼운 길에서 무엇을 줄일지(그림자, 바깥선, 효과, 그리는 수)는 게임이 is_lite 를 읽어 정한다.
// - 주소: apply_search_text 는 쿼리 글("?light", "?a=1&light=0")을 받는다. 원본은 URLSearchParams 로 읽고, 러스트 판은 & 와 = 로 나눠 읽는다. (퍼센트 풀기는 하지 않는다)
//   apply_page_query 는 웹에서 페이지 주소의 쿼리를 읽는다. 데스크탑은 페이지 주소가 없어 아무것도 하지 않는다. (브라우저 전용)
//==============================================================================
use crate::core::viewmanager::ViewManager;
use crate::misc::frameprobe::FrameProbe;
use crate::platform;

//==============================================================================
// 그래픽 품질 설정 값. (원본 GraphicsQuality.auto, high, low)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphicsQuality {
	#[default]
	Auto,
	High,
	Low,
}

pub struct GraphicsQualityGovernor {
	quality: GraphicsQuality,
	lite_override: Option<bool>,
	is_auto_lite_default: bool,
	frame_probe: FrameProbe,
	normal_max_render_pixel_ratio: f32,
	lite_max_render_pixel_ratio: f32,
	was_lite: bool,
}

impl Default for GraphicsQualityGovernor {
	fn default() -> GraphicsQualityGovernor {
		GraphicsQualityGovernor::new()
	}
}

impl GraphicsQualityGovernor {
	//==============================================================================
	// 생성. (자동, 덮어쓰기 없음, 렌더 배율 제한 없음)
	//==============================================================================
	pub fn new() -> GraphicsQualityGovernor {
		GraphicsQualityGovernor {
			quality: GraphicsQuality::Auto,
			lite_override: None,
			is_auto_lite_default: false,
			frame_probe: FrameProbe::new(),
			normal_max_render_pixel_ratio: 0.0,
			lite_max_render_pixel_ratio: 0.0,
			was_lite: false,
		}
	}

	//==============================================================================
	// 품질 설정.
	//==============================================================================
	pub fn set_quality(&mut self, quality: GraphicsQuality) {
		self.quality = quality;
	}

	//==============================================================================
	// 품질 설정 반환.
	//==============================================================================
	pub fn get_quality(&self) -> GraphicsQuality {
		self.quality
	}

	//==============================================================================
	// 덮어쓰기 설정. (Some(true) 면 늘 가벼운 길, Some(false) 면 늘 아님, None 이면 풀기)
	//==============================================================================
	pub fn set_lite_override(&mut self, lite_override: Option<bool>) {
		self.lite_override = lite_override;
	}

	//==============================================================================
	// 덮어쓰기 반환.
	//==============================================================================
	pub fn get_lite_override(&self) -> Option<bool> {
		self.lite_override
	}

	//==============================================================================
	// 쿼리 글로 덮어쓰기. (lite_key 가 있으면 가벼운 길, 값이 "0" 이나 "false" 면 늘 아님, 없으면 그대로)
	//==============================================================================
	pub fn apply_search_text(&mut self, search_text: &str, lite_key: &str) {
		let query_text = search_text.strip_prefix('?').unwrap_or(search_text);
		for pair_text in query_text.split('&') {
			let (key, value) = match pair_text.split_once('=') {
				Some((key, value)) => {
					(key, value)
				}
				None => {
					(pair_text, "")
				}
			};
			if key == lite_key {
				self.lite_override = Some(value != "0" && value != "false");
				return;
			}
		}
	}

	//==============================================================================
	// 페이지 주소의 쿼리로 덮어쓰기. (웹 전용, 데스크탑은 그대로)
	//==============================================================================
	pub fn apply_page_query(&mut self, lite_key: &str) {
		if let Some(value) = platform::read_page_query_value(lite_key) {
			self.lite_override = Some(value != "0" && value != "false");
		}
	}

	//==============================================================================
	// 자동일 때 처음부터 가벼운 길인지 설정. (손가락 기기처럼 재기 전부터 가볍게 갈 때)
	//==============================================================================
	pub fn set_auto_lite_default(&mut self, is_auto_lite_default: bool) {
		self.is_auto_lite_default = is_auto_lite_default;
	}

	//==============================================================================
	// 자동일 때 처음부터 가벼운 길인지 반환.
	//==============================================================================
	pub fn is_auto_lite_default(&self) -> bool {
		self.is_auto_lite_default
	}

	//==============================================================================
	// 프레임 재기 반환.
	//==============================================================================
	pub fn get_frame_probe(&self) -> &FrameProbe {
		&self.frame_probe
	}

	//==============================================================================
	// 프레임 재기 반환. (기다리는 초, 판정 문턱을 바꾸거나 무거운 장면에서 measure_again 할 때)
	//==============================================================================
	pub fn get_frame_probe_mut(&mut self) -> &mut FrameProbe {
		&mut self.frame_probe
	}

	//==============================================================================
	// 가벼운 길로 그리는지.
	//==============================================================================
	pub fn is_lite(&self) -> bool {
		if let Some(lite_override) = self.lite_override {
			return lite_override;
		}
		match self.quality {
			GraphicsQuality::Low => {
				true
			}
			GraphicsQuality::High => {
				false
			}
			GraphicsQuality::Auto => {
				self.is_auto_lite_default || self.frame_probe.is_slow()
			}
		}
	}

	//==============================================================================
	// 한 프레임. (자동이고 아직 가벼운 길이 아니면 프레임을 잰다, 가벼운 길이 바뀐 프레임에 true)
	//==============================================================================
	pub fn update(&mut self, frame_seconds: f32, maximum_frame_per_second: f32) -> bool {
		let is_measured = self.lite_override.is_none() && self.quality == GraphicsQuality::Auto && !self.is_auto_lite_default;
		if is_measured {
			self.frame_probe.measure_frame(frame_seconds, maximum_frame_per_second);
		}
		let is_lite = self.is_lite();
		let is_changed = is_lite != self.was_lite;
		self.was_lite = is_lite;
		is_changed
	}

	//==============================================================================
	// 렌더 배율 상한 설정. (보통 길과 가벼운 길, 0 이면 제한 없음)
	//==============================================================================
	pub fn set_render_pixel_ratio_limits(&mut self, normal_max_render_pixel_ratio: f32, lite_max_render_pixel_ratio: f32) {
		self.normal_max_render_pixel_ratio = normal_max_render_pixel_ratio.max(0.0);
		self.lite_max_render_pixel_ratio = lite_max_render_pixel_ratio.max(0.0);
	}

	//==============================================================================
	// 지금 길의 렌더 배율 상한.
	//==============================================================================
	pub fn read_max_render_pixel_ratio(&self) -> f32 {
		if self.is_lite() {
			self.lite_max_render_pixel_ratio
		}
		else {
			self.normal_max_render_pixel_ratio
		}
	}

	//==============================================================================
	// 렌더 배율 상한 걸기. (ViewManager 의 값과 다를 때만, 걸었으면 true)
	//==============================================================================
	pub fn apply_render_pixel_ratio(&self, view_manager: &mut ViewManager) -> bool {
		let max_render_pixel_ratio = self.read_max_render_pixel_ratio();
		if view_manager.get_max_render_pixel_ratio() == max_render_pixel_ratio {
			return false;
		}
		view_manager.set_max_render_pixel_ratio(max_render_pixel_ratio);
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn slow_frames_turn_auto_to_lite_once() {
		let mut governor = GraphicsQualityGovernor::new();
		governor.set_render_pixel_ratio_limits(0.0, 1.5);
		assert!(!governor.is_lite());
		governor.get_frame_probe_mut().set_seconds(0.0, 1.0);
		let mut changed_count = 0;
		for _ in 0..40 {
			if governor.update(0.05, 60.0) {
				changed_count += 1;
			}
		}
		assert!(governor.is_lite());
		assert_eq!(changed_count, 1);
		assert_eq!(governor.read_max_render_pixel_ratio(), 1.5);
	}

	#[test]
	fn override_beats_setting_and_setting_beats_auto() {
		let mut governor = GraphicsQualityGovernor::new();
		governor.set_quality(GraphicsQuality::High);
		governor.set_auto_lite_default(true);
		assert!(!governor.is_lite());
		governor.apply_search_text("?a=1&light", "light");
		assert_eq!(governor.get_lite_override(), Some(true));
		assert!(governor.is_lite());
		governor.apply_search_text("?light=0", "light");
		assert!(!governor.is_lite());
		governor.apply_search_text("?other=1", "light");
		assert_eq!(governor.get_lite_override(), Some(false));
		governor.set_lite_override(None);
		governor.set_quality(GraphicsQuality::Low);
		assert!(governor.is_lite());
		governor.set_quality(GraphicsQuality::Auto);
		assert!(governor.is_lite());
	}

	#[test]
	fn render_pixel_ratio_applies_only_on_change() {
		let mut governor = GraphicsQualityGovernor::new();
		governor.set_render_pixel_ratio_limits(2.0, 1.0);
		let mut view_manager = ViewManager::new(crate::base::vector2::Vector2::create(1280.0, 720.0), crate::core::viewmanager::ViewScaleMode::StretchShort);
		assert!(governor.apply_render_pixel_ratio(&mut view_manager));
		assert_eq!(view_manager.get_max_render_pixel_ratio(), 2.0);
		assert!(!governor.apply_render_pixel_ratio(&mut view_manager));
	}
}

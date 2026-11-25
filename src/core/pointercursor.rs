//==============================================================================
//! 마우스 커서. (vanilla.js core/pointercursor.js 의 PointerCursor)
//! - 엔진이 하나 든다. (Engine::get_pointer_cursor_mut) 켜면(set_enabled(true)) 엔진이 프레임 끝마다 창이나 캔버스의 커서를 정한다.
//!   꺼져 있으면(기본) 아무것도 하지 않아 지금까지와 같다.
//! - UIButton 위(누르는 중 포함)면 단추의 커서(처음은 손가락 pointer), 아니면 화면이 set_screen_cursor 로 정한 커서다.
//! - 이름은 CSS cursor 이름이다. (pointer, crosshair, not-allowed, grab, grabbing, text, move, help, wait, progress, none, default)
//!   데스크탑은 winit 의 커서 아이콘으로 바꿔 쓰고, 웹은 캔버스의 cursor 스타일에 쓴다.
//! - register_image_cursor 로 그림 커서를 더하면 웹은 그 그림(1 배와 2 배)을 CSS 값으로, 데스크탑은 그 그림 파일(애셋 폴더 기준)로 만든 창의 커서를 쓴다.
//!   (데스크탑은 만드는 동안과 그림을 못 읽으면 함께 준 CSS 커서)
//! - 손가락 기기(pointer: coarse)에서는 건드리지 않는다. 바뀔 때만 플랫폼에 넘긴다.
//! - 원본 apply 는 전역 엔진의 캔버스(readEngineCanvas)에 바로 쓴다. 러스트 판은 전역 엔진이 없어 엔진이 take_cursor_change 로 바뀐 값을 받아 플랫폼에 넘긴다.
//==============================================================================
use std::cell::Cell;
use std::collections::HashMap;

/// 쓸 수 있는 CSS 커서 이름. (그 밖의 이름은 기본 화살표)
const SHOWN_CURSOR_NAMES: [&str; 11] = ["pointer", "crosshair", "not-allowed", "grab", "grabbing", "text", "move", "help", "wait", "progress", "none"];

//==============================================================================
/// 플랫폼에 넘길 커서. (웹의 CSS 값, 데스크탑이 쓸 CSS 이름, 그림 커서면 그림 주소와 핫스팟)
//==============================================================================
#[derive(Clone, PartialEq, Debug)]
pub struct CursorValue {
	pub css_value: String,
	pub fallback_name: String,
	/// 그림 커서의 1 배 그림 주소. (그림 커서가 아니면 빈 글)
	pub image_url: String,
	/// 그림 커서의 2 배 그림 주소. (없으면 빈 글)
	pub image_url_2x: String,
	/// 1 배 그림 기준 핫스팟. (원본처럼 반올림한 값)
	pub hotspot_x: f32,
	pub hotspot_y: f32,
}

pub struct PointerCursor {
	is_enabled: bool,
	image_cursor_values: HashMap<String, CursorValue>,
	button_cursor_name: String,
	default_cursor_name: String,
	screen_cursor_name: String,
	is_over_button: Cell<bool>,
	applied_cursor_value: Option<CursorValue>,
	/// 이번 프레임에 문서 요소가 바란 CSS 커서. (Pane 리사이저의 col-resize 같은 것, 원본은 브라우저가 요소의 cursor 를 보인다)
	element_cursor_name: std::cell::RefCell<Option<String>>,
	is_element_cursor_applied: bool,
}

impl PointerCursor {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new() -> PointerCursor {
		PointerCursor {
			is_enabled: false,
			image_cursor_values: HashMap::new(),
			button_cursor_name: String::from("pointer"),
			default_cursor_name: String::from("default"),
			screen_cursor_name: String::from("default"),
			is_over_button: Cell::new(false),
			applied_cursor_value: None,
			element_cursor_name: std::cell::RefCell::new(None),
			is_element_cursor_applied: false,
		}
	}

	//==============================================================================
	/// 문서 요소의 커서 바라기. (이번 프레임만. 원본은 브라우저가 DOM 요소의 CSS cursor 를 보인다. 켜기 설정과 상관없이 보인다)
	//==============================================================================
	pub fn request_element_cursor(&self, cursor_name: &str) {
		*self.element_cursor_name.borrow_mut() = Some(String::from(cursor_name));
	}

	//==============================================================================
	/// 켜기 설정. (끄면 다음 프레임에 기본 커서로 돌린다)
	//==============================================================================
	pub fn set_enabled(&mut self, is_enabled: bool) {
		self.is_enabled = is_enabled;
		self.is_over_button.set(false);
		self.applied_cursor_value = None;
	}

	//==============================================================================
	/// 켜져 있는지.
	//==============================================================================
	pub fn is_enabled(&self) -> bool {
		self.is_enabled
	}

	//==============================================================================
	/// 마우스가 단추 위에 있다고 알리기. (올려 둔 단추가 매 프레임 부른다, 컴포넌트는 엔진을 빌려 읽기만 하므로 &self)
	//==============================================================================
	pub fn mark_over_button(&self) {
		self.is_over_button.set(true);
	}

	//==============================================================================
	/// 화면의 커서 설정. (단추 위가 아닐 때 보일 커서)
	//==============================================================================
	pub fn set_screen_cursor(&mut self, cursor_name: &str) {
		self.screen_cursor_name = String::from(cursor_name);
	}

	//==============================================================================
	/// 화면의 커서 반환.
	//==============================================================================
	pub fn get_screen_cursor(&self) -> &str {
		&self.screen_cursor_name
	}

	//==============================================================================
	/// 단추 위와 기본의 커서 설정.
	//==============================================================================
	pub fn set_cursor_names(&mut self, button_cursor_name: &str, default_cursor_name: &str) {
		self.button_cursor_name = String::from(button_cursor_name);
		self.default_cursor_name = String::from(default_cursor_name);
	}

	//==============================================================================
	/// 단추 위의 커서 반환.
	//==============================================================================
	pub fn get_button_cursor_name(&self) -> &str {
		&self.button_cursor_name
	}

	//==============================================================================
	/// 기본의 커서 반환.
	//==============================================================================
	pub fn get_default_cursor_name(&self) -> &str {
		&self.default_cursor_name
	}

	//==============================================================================
	/// 주된 손이 손가락인지. (원본 static isCoarsePointer, CSS 의 pointer: coarse, 데스크탑은 거짓)
	//==============================================================================
	pub fn is_coarse_pointer() -> bool {
		crate::platform::is_coarse_pointer()
	}

	//==============================================================================
	/// 그림 커서 더하기. (1 배와 2 배 그림 주소, 1 배 그림 기준 핫스팟, 그림을 못 쓸 때와 데스크탑의 CSS 커서)
	//==============================================================================
	pub fn register_image_cursor(&mut self, name: &str, url: &str, url_2x: &str, hotspot_x: f32, hotspot_y: f32, fallback_cursor_name: &str) {
		let hotspot_text = format!(" {} {}, {}", hotspot_x.round() as i32, hotspot_y.round() as i32, fallback_cursor_name);
		let css_value = if url_2x.is_empty() {
			format!("url(\"{}\"){}", url, hotspot_text)
		}
		else {
			format!("-webkit-image-set(url(\"{}\") 1x, url(\"{}\") 2x){}", url, url_2x, hotspot_text)
		};
		let cursor_value = CursorValue {
			css_value,
			fallback_name: String::from(fallback_cursor_name),
			image_url: String::from(url),
			image_url_2x: String::from(url_2x),
			hotspot_x: hotspot_x.round(),
			hotspot_y: hotspot_y.round(),
		};
		self.image_cursor_values.insert(String::from(name), cursor_value);
	}

	//==============================================================================
	/// 커서 이름을 플랫폼에 넘길 값으로. (그림 커서면 그 값, 쓸 수 있는 CSS 이름이면 그대로, 아니면 기본 화살표)
	//==============================================================================
	pub fn read_cursor_value(&self, cursor_name: &str) -> CursorValue {
		if let Some(image_cursor_value) = self.image_cursor_values.get(cursor_name) {
			return image_cursor_value.clone();
		}
		let shown_name = if SHOWN_CURSOR_NAMES.contains(&cursor_name) { cursor_name } else { "" };
		CursorValue {
			css_value: String::from(shown_name),
			fallback_name: String::from(shown_name),
			image_url: String::new(),
			image_url_2x: String::new(),
			hotspot_x: 0.0,
			hotspot_y: 0.0,
		}
	}

	//==============================================================================
	/// 이번 프레임의 커서가 바뀌었으면 꺼내기. (엔진이 프레임 끝마다 부른다, 단추 표시를 비운다)
	//==============================================================================
	pub fn take_cursor_change(&mut self, is_coarse_pointer: bool) -> Option<CursorValue> {
		let is_over_button = self.is_over_button.replace(false);
		let element_cursor_name = self.element_cursor_name.borrow_mut().take();
		if is_coarse_pointer {
			return None;
		}
		if let Some(element_cursor_name) = element_cursor_name {
			self.is_element_cursor_applied = true;
			let cursor_value = CursorValue {
				css_value: element_cursor_name.clone(),
				fallback_name: element_cursor_name,
				image_url: String::new(),
				image_url_2x: String::new(),
				hotspot_x: 0.0,
				hotspot_y: 0.0,
			};
			if self.applied_cursor_value.as_ref() == Some(&cursor_value) {
				return None;
			}
			self.applied_cursor_value = Some(cursor_value.clone());
			return Some(cursor_value);
		}
		if !self.is_enabled {
			// 켜지 않았으면 문서 요소가 바꾼 커서만 기본으로 돌려놓는다.
			if !self.is_element_cursor_applied {
				return None;
			}
			self.is_element_cursor_applied = false;
			let cursor_value = self.read_cursor_value("default");
			self.applied_cursor_value = Some(cursor_value.clone());
			return Some(cursor_value);
		}
		self.is_element_cursor_applied = false;
		let mut wanted_cursor_name = if is_over_button { self.button_cursor_name.clone() } else { self.screen_cursor_name.clone() };
		if wanted_cursor_name == "default" {
			wanted_cursor_name = self.default_cursor_name.clone();
		}
		let cursor_value = self.read_cursor_value(&wanted_cursor_name);
		if self.applied_cursor_value.as_ref() == Some(&cursor_value) {
			return None;
		}
		self.applied_cursor_value = Some(cursor_value.clone());
		Some(cursor_value)
	}
}

impl Default for PointerCursor {
	fn default() -> PointerCursor {
		PointerCursor::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn button_cursor_wins_and_changes_once() {
		let mut pointer_cursor = PointerCursor::new();
		assert_eq!(pointer_cursor.take_cursor_change(false), None);
		pointer_cursor.set_enabled(true);
		pointer_cursor.set_screen_cursor("crosshair");
		assert_eq!(pointer_cursor.take_cursor_change(false).map(|value| value.css_value), Some(String::from("crosshair")));
		assert_eq!(pointer_cursor.take_cursor_change(false), None);
		pointer_cursor.mark_over_button();
		assert_eq!(pointer_cursor.take_cursor_change(false).map(|value| value.css_value), Some(String::from("pointer")));
		pointer_cursor.mark_over_button();
		assert_eq!(pointer_cursor.take_cursor_change(true), None);
	}
}

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
//!
//! # 예제
//! 엔진의 커서를 켜고 화면 커서를 십자로
//! ```rust
//! use vanilla::{Engine, EngineConfiguration};
//!
//! let mut engine = Engine::new(EngineConfiguration::new());
//! let pointer_cursor = engine.get_pointer_cursor_mut();
//! pointer_cursor.set_enabled(true);
//! pointer_cursor.set_screen_cursor("crosshair");
//! assert_eq!(engine.get_pointer_cursor().get_screen_cursor(), "crosshair");
//! ```
//==============================================================================
use std::cell::Cell;
use std::collections::HashMap;

/// 쓸 수 있는 CSS 커서 이름. (그 밖의 이름은 기본 화살표)
const SHOWN_CURSOR_NAMES: [&str; 11] = ["pointer", "crosshair", "not-allowed", "grab", "grabbing", "text", "move", "help", "wait", "progress", "none"];

//==============================================================================
/// 플랫폼에 넘길 커서. (웹의 CSS 값, 데스크탑이 쓸 CSS 이름, 그림 커서면 그림 주소와 핫스팟)
///
/// # 예제
/// 그림 커서와 CSS 이름 커서의 값
/// ```rust
/// use vanilla::PointerCursor;
///
/// let mut pointer_cursor = PointerCursor::new();
/// pointer_cursor.register_image_cursor("icon", "examples/assets/atlas/icons.png", "", 3.4, 2.6, "pointer");
/// let image_cursor_value = pointer_cursor.read_cursor_value("icon");
/// assert_eq!(image_cursor_value.css_value, "url(\"examples/assets/atlas/icons.png\") 3 3, pointer");
/// assert_eq!(image_cursor_value.fallback_name, "pointer");
/// assert_eq!((image_cursor_value.hotspot_x, image_cursor_value.hotspot_y), (3.0, 3.0));
///
/// let css_cursor_value = pointer_cursor.read_cursor_value("grab");
/// assert_eq!(css_cursor_value.css_value, "grab");
/// assert!(css_cursor_value.image_url.is_empty());
/// assert_eq!(pointer_cursor.read_cursor_value("unknown").css_value, "");
/// ```
//==============================================================================
#[derive(Clone, PartialEq, Debug)]
pub struct CursorValue {
	/// 웹 캔버스의 cursor 스타일에 쓸 CSS 값. (그림 커서면 url 이나 -webkit-image-set 과 핫스팟과 CSS 커서, 기본 화살표면 빈 글)
	pub css_value: String,
	/// 그림을 못 쓸 때와 데스크탑이 쓸 CSS 커서 이름. (기본 화살표면 빈 글)
	pub fallback_name: String,
	/// 그림 커서의 1 배 그림 주소. (그림 커서가 아니면 빈 글)
	pub image_url: String,
	/// 그림 커서의 2 배 그림 주소. (없으면 빈 글)
	pub image_url_2x: String,
	/// 1 배 그림 기준 핫스팟. (원본처럼 반올림한 값)
	pub hotspot_x: f64,
	/// 1 배 그림 기준 핫스팟의 y. (픽셀, 원본처럼 반올림한 값, 그림 커서가 아니면 0)
	pub hotspot_y: f64,
}

//==============================================================================
/// 마우스 커서. (켜면 단추 위와 화면의 커서를 골라 바뀔 때만 플랫폼에 넘긴다, 엔진이 하나 든다)
///
/// # 예제
/// 화면 커서와 단추 위 커서가 바뀔 때만 꺼내기
/// ```rust
/// use vanilla::PointerCursor;
///
/// let mut pointer_cursor = PointerCursor::new();
/// assert_eq!(pointer_cursor.take_cursor_change(false), None);
/// pointer_cursor.set_enabled(true);
/// pointer_cursor.set_screen_cursor("crosshair");
/// assert_eq!(pointer_cursor.take_cursor_change(false).unwrap().css_value, "crosshair");
/// assert_eq!(pointer_cursor.take_cursor_change(false), None);
/// pointer_cursor.mark_over_button();
/// assert_eq!(pointer_cursor.take_cursor_change(false).unwrap().css_value, "pointer");
/// ```
//==============================================================================
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
	///
	/// # 돌려줌
	/// 꺼진 새 커서. (단추 위 pointer, 기본 default, 화면 default)
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
	///
	/// # 인자
	/// - `cursor_name`: 보일 CSS 커서 이름. (col-resize 같은 것, 그대로 플랫폼에 넘긴다)
	//==============================================================================
	pub fn request_element_cursor(&self, cursor_name: &str) {
		*self.element_cursor_name.borrow_mut() = Some(String::from(cursor_name));
	}

	//==============================================================================
	/// 켜기 설정. (끄면 다음 프레임에 기본 커서로 돌린다)
	///
	/// # 인자
	/// - `is_enabled`: true 면 엔진이 프레임 끝마다 커서를 정한다. (기본 false)
	//==============================================================================
	pub fn set_enabled(&mut self, is_enabled: bool) {
		self.is_enabled = is_enabled;
		self.is_over_button.set(false);
		self.applied_cursor_value = None;
	}

	//==============================================================================
	/// 켜져 있는지.
	///
	/// # 돌려줌
	/// set_enabled(true) 뒤면 true.
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
	///
	/// # 인자
	/// - `cursor_name`: CSS 커서 이름이나 register_image_cursor 로 더한 이름. (default 면 기본의 커서)
	//==============================================================================
	pub fn set_screen_cursor(&mut self, cursor_name: &str) {
		self.screen_cursor_name = String::from(cursor_name);
	}

	//==============================================================================
	/// 화면의 커서 반환.
	///
	/// # 돌려줌
	/// 화면의 커서 이름. (기본 default)
	//==============================================================================
	pub fn get_screen_cursor(&self) -> &str {
		&self.screen_cursor_name
	}

	//==============================================================================
	/// 단추 위와 기본의 커서 설정.
	///
	/// # 인자
	/// - `button_cursor_name`: 단추 위(누르는 중 포함)에서 보일 커서 이름. (기본 pointer)
	/// - `default_cursor_name`: 화면의 커서가 default 일 때 대신 보일 커서 이름. (기본 default)
	//==============================================================================
	pub fn set_cursor_names(&mut self, button_cursor_name: &str, default_cursor_name: &str) {
		self.button_cursor_name = String::from(button_cursor_name);
		self.default_cursor_name = String::from(default_cursor_name);
	}

	//==============================================================================
	/// 단추 위의 커서 반환.
	///
	/// # 돌려줌
	/// 단추 위의 커서 이름. (기본 pointer)
	//==============================================================================
	pub fn get_button_cursor_name(&self) -> &str {
		&self.button_cursor_name
	}

	//==============================================================================
	/// 기본의 커서 반환.
	///
	/// # 돌려줌
	/// 기본의 커서 이름. (기본 default)
	//==============================================================================
	pub fn get_default_cursor_name(&self) -> &str {
		&self.default_cursor_name
	}

	//==============================================================================
	/// 주된 손이 손가락인지. (원본 static isCoarsePointer, CSS 의 pointer: coarse. 데스크탑은 윈도우 태블릿 모드나 마우스 없는 터치 기기, 리눅스 터치스크린만 있는 기기면 참, 맥은 거짓)
	///
	/// # 돌려줌
	/// 주된 손이 손가락이면 true. (플랫폼의 is_coarse_pointer 를 그대로 돌려준다)
	//==============================================================================
	pub fn is_coarse_pointer() -> bool {
		crate::platform::is_coarse_pointer()
	}

	//==============================================================================
	/// 그림 커서 더하기. (1 배와 2 배 그림 주소, 1 배 그림 기준 핫스팟, 그림을 못 쓸 때와 데스크탑의 CSS 커서)
	/// - 같은 이름으로 다시 더하면 바꾼다.
	///
	/// # 인자
	/// - `name`: 커서 이름. (set_screen_cursor, set_cursor_names 에 쓴다)
	/// - `url`: 1 배 그림 주소. (웹은 CSS url, 데스크탑은 애셋 폴더 기준 그림 파일)
	/// - `url_2x`: 2 배 그림 주소. 빈 글이면 1 배 그림만 쓴다.
	/// - `hotspot_x`: 1 배 그림 기준 핫스팟 x. (픽셀, 반올림한다)
	/// - `hotspot_y`: 1 배 그림 기준 핫스팟 y. (픽셀, 반올림한다)
	/// - `fallback_cursor_name`: 그림을 못 쓸 때와 데스크탑이 그림을 만드는 동안 보일 CSS 커서 이름.
	///
	/// # 예제
	/// 그림 커서를 더해 화면 커서로
	/// ```rust
	/// use vanilla::PointerCursor;
	///
	/// let mut pointer_cursor = PointerCursor::new();
	/// pointer_cursor.register_image_cursor("icon", "examples/assets/atlas/icons.png", "", 0.0, 0.0, "default");
	/// pointer_cursor.set_enabled(true);
	/// pointer_cursor.set_screen_cursor("icon");
	/// let cursor_value = pointer_cursor.take_cursor_change(false).unwrap();
	/// assert_eq!(cursor_value.image_url, "examples/assets/atlas/icons.png");
	/// ```
	//==============================================================================
	pub fn register_image_cursor(&mut self, name: &str, url: &str, url_2x: &str, hotspot_x: f64, hotspot_y: f64, fallback_cursor_name: &str) {
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
	///
	/// # 인자
	/// - `cursor_name`: 커서 이름. (그림 커서 이름이나 CSS 커서 이름)
	///
	/// # 돌려줌
	/// 플랫폼에 넘길 값. (기본 화살표면 css_value 와 fallback_name 이 빈 글)
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
	/// - 문서 요소가 바란 커서가 먼저이고, 켜져 있으면 단추 위의 커서, 아니면 화면의 커서다.
	///
	/// # 인자
	/// - `is_coarse_pointer`: 주된 손이 손가락인지. (true 면 아무것도 바꾸지 않는다)
	///
	/// # 돌려줌
	/// 지난번에 넘긴 값과 다르면 새 값. 같거나, 손가락 기기이거나, 꺼져 있고 문서 요소 커서도 없었으면 None.
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
	//==============================================================================
	/// 기본값. (PointerCursor::new 와 같다)
	//==============================================================================
	fn default() -> PointerCursor {
		PointerCursor::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 단추 위 커서가 화면 커서를 이기고, 바뀔 때만 한 번 꺼낸다. (손가락 기기는 꺼내지 않는다)
	//==============================================================================
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

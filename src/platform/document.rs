//==============================================================================
//! 문서. (원본이 브라우저 document 에서 읽고 쓰는 것: html 의 lang, head 의 meta)
//! - 원본은 전역 document 를 그대로 쓴다. LanguagePack 이 언어를 불러오면 document.documentElement.lang 을 바꾸고,
//!   BuildInfo 는 `meta[name="build-version"]` 의 content 에서 배포 빌드가 적어 둔 판을 읽는다.
//! - 웹은 원본처럼 브라우저 문서를 읽고 쓴다.
//! - 데스크탑은 HTML 문서가 없어, 엔진이 스레드마다 문서 값(언어, meta 목록)을 들고 같은 일을 한다.
//!   처음 값은 EngineConfiguration 의 document_language, document_meta_list 다. (웹 판의 index.html 이 html lang 과 head 의 meta 로 적어 두는 자리)
//!   배포 빌드가 웹 판의 index.html 에 판을 적듯, 데스크탑 게임은 빌드할 때 넣은 값(build.rs 의 rustc-env 와 env! 같은 것)을 document_meta_list 에 넣는다.
//! - 엔진 설정의 값은 웹에서도 비어 있지 않으면 엔진을 만들 때 문서에 쓴다. (같은 게임 코드가 두 플랫폼에서 같은 문서를 갖는다)
//!   비어 있으면 웹은 페이지가 적어 둔 그대로다.
//==============================================================================

//==============================================================================
/// 문서. (정적 함수 묶음, 원본 전역 document 의 lang 과 meta)
//==============================================================================
pub struct Document;

impl Document {
	//==============================================================================
	/// 문서 언어. (원본 document.documentElement.lang, 적지 않았으면 빈 글)
	//==============================================================================
	pub fn get_language() -> String {
		platform_document::read_language()
	}

	//==============================================================================
	/// 문서 언어 설정. (원본 document.documentElement.lang = 언어 태그)
	//==============================================================================
	pub fn set_language(language: &str) {
		platform_document::write_language(language);
	}

	//==============================================================================
	/// meta 의 content. (원본 document.querySelector(`meta[name="이름"]`).getAttribute("content"), 그런 meta 가 없으면 None)
	//==============================================================================
	pub fn get_meta_content(meta_name: &str) -> Option<String> {
		platform_document::read_meta_content(meta_name)
	}

	//==============================================================================
	/// meta 의 content 설정. (그 이름의 meta 가 없으면 head 끝에 만든다)
	//==============================================================================
	pub fn set_meta_content(meta_name: &str, content: &str) {
		platform_document::write_meta_content(meta_name, content);
	}

	//==============================================================================
	/// meta 지우기. (그 이름의 첫 meta)
	//==============================================================================
	pub fn remove_meta(meta_name: &str) {
		platform_document::remove_meta(meta_name);
	}
}

//==============================================================================
/// 엔진 설정의 문서 값 쓰기. (Engine::new 가 부른다. 데스크탑은 문서를 새로 차리고, 웹은 비어 있지 않은 값만 쓴다)
//==============================================================================
pub(crate) fn apply_engine_configuration(language: &str, meta_list: &[(String, String)]) {
	platform_document::reset();
	if !language.is_empty() {
		Document::set_language(language);
	}
	for (meta_name, content) in meta_list {
		Document::set_meta_content(meta_name, content);
	}
}

//==============================================================================
/// 데스크탑. (스레드마다 엔진이 든 문서 값)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
mod platform_document {
	use std::cell::RefCell;

	//==============================================================================
	/// 데스크탑 문서. (html lang 과 head 의 meta 차례)
	//==============================================================================
	struct DesktopDocument {
		language: String,
		meta_list: Vec<(String, String)>,
	}

	thread_local! {
		static DESKTOP_DOCUMENT: RefCell<DesktopDocument> = const {
			RefCell::new(DesktopDocument {
				language: String::new(),
				meta_list: Vec::new(),
			})
		};
	}

	//==============================================================================
	/// 새 문서. (lang 도 meta 도 없는 페이지)
	//==============================================================================
	pub fn reset() {
		DESKTOP_DOCUMENT.with(|document_cell| {
			let mut document = document_cell.borrow_mut();
			document.language.clear();
			document.meta_list.clear();
		});
	}

	//==============================================================================
	/// 언어.
	//==============================================================================
	pub fn read_language() -> String {
		DESKTOP_DOCUMENT.with(|document_cell| document_cell.borrow().language.clone())
	}

	//==============================================================================
	/// 언어 쓰기.
	//==============================================================================
	pub fn write_language(language: &str) {
		DESKTOP_DOCUMENT.with(|document_cell| {
			document_cell.borrow_mut().language = language.to_string();
		});
	}

	//==============================================================================
	/// meta 의 content. (같은 이름이 여럿이면 원본 querySelector 처럼 앞의 것)
	//==============================================================================
	pub fn read_meta_content(meta_name: &str) -> Option<String> {
		DESKTOP_DOCUMENT.with(|document_cell| document_cell.borrow().meta_list.iter().find(|(name, _)| name == meta_name).map(|(_, content)| content.clone()))
	}

	//==============================================================================
	/// meta 의 content 쓰기. (없으면 끝에 더한다)
	//==============================================================================
	pub fn write_meta_content(meta_name: &str, content: &str) {
		DESKTOP_DOCUMENT.with(|document_cell| {
			let mut document = document_cell.borrow_mut();
			match document.meta_list.iter_mut().find(|(name, _)| name == meta_name) {
				Some((_, meta_content)) => {
					*meta_content = content.to_string();
				}
				None => {
					document.meta_list.push((meta_name.to_string(), content.to_string()));
				}
			}
		});
	}

	//==============================================================================
	/// meta 지우기.
	//==============================================================================
	pub fn remove_meta(meta_name: &str) {
		DESKTOP_DOCUMENT.with(|document_cell| {
			let mut document = document_cell.borrow_mut();
			if let Some(meta_index) = document.meta_list.iter().position(|(name, _)| name == meta_name) {
				document.meta_list.remove(meta_index);
			}
		});
	}
}

//==============================================================================
/// 웹. (브라우저 문서)
//==============================================================================
#[cfg(target_arch = "wasm32")]
mod platform_document {
	//==============================================================================
	/// 브라우저 문서. (없으면 None)
	//==============================================================================
	fn read_document() -> Option<web_sys::Document> {
		web_sys::window().and_then(|window| window.document())
	}

	//==============================================================================
	/// 그 이름의 첫 meta. (원본처럼 이름을 그대로 선택자에 넣는다)
	//==============================================================================
	fn find_meta_element(meta_name: &str) -> Option<web_sys::Element> {
		let document = read_document()?;
		let selector = format!("meta[name=\"{}\"]", meta_name);
		document.query_selector(&selector).ok().flatten()
	}

	//==============================================================================
	/// 페이지 문서는 페이지가 적어 둔 그대로 둔다.
	//==============================================================================
	pub fn reset() {
	}

	//==============================================================================
	/// 언어. (html 의 lang 속성)
	//==============================================================================
	pub fn read_language() -> String {
		read_document().and_then(|document| document.document_element()).and_then(|document_element| document_element.get_attribute("lang")).unwrap_or_default()
	}

	//==============================================================================
	/// 언어 쓰기.
	//==============================================================================
	pub fn write_language(language: &str) {
		if let Some(document_element) = read_document().and_then(|document| document.document_element()) {
			let _ = document_element.set_attribute("lang", language);
		}
	}

	//==============================================================================
	/// meta 의 content.
	//==============================================================================
	pub fn read_meta_content(meta_name: &str) -> Option<String> {
		find_meta_element(meta_name)?.get_attribute("content")
	}

	//==============================================================================
	/// meta 의 content 쓰기. (없으면 head 끝에 만든다)
	//==============================================================================
	pub fn write_meta_content(meta_name: &str, content: &str) {
		if let Some(meta_element) = find_meta_element(meta_name) {
			let _ = meta_element.set_attribute("content", content);
			return;
		}
		let document = match read_document() {
			Some(document) => {
				document
			}
			None => {
				return;
			}
		};
		let meta_element = match document.create_element("meta") {
			Ok(meta_element) => {
				meta_element
			}
			Err(_) => {
				return;
			}
		};
		let _ = meta_element.set_attribute("name", meta_name);
		let _ = meta_element.set_attribute("content", content);
		if let Some(head_element) = document.head() {
			let _ = head_element.append_child(&meta_element);
		}
	}

	//==============================================================================
	/// meta 지우기.
	//==============================================================================
	pub fn remove_meta(meta_name: &str) {
		if let Some(meta_element) = find_meta_element(meta_name) {
			meta_element.remove();
		}
	}
}

//==============================================================================
/// 시험. (데스크탑 문서)
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;

	//==============================================================================
	/// 엔진 설정의 값으로 차리고, 읽고 쓰고 지운다.
	//==============================================================================
	#[test]
	fn desktop_document_values() {
		apply_engine_configuration("ko", &[(String::from("build-version"), String::from("2026-11-26 abc1234")), (String::from("build-version"), String::from("second"))]);
		assert_eq!(Document::get_language(), "ko");
		assert_eq!(Document::get_meta_content("build-version").as_deref(), Some("second"));
		Document::set_language("en");
		Document::set_meta_content("author", "vanilla");
		assert_eq!(Document::get_language(), "en");
		assert_eq!(Document::get_meta_content("author").as_deref(), Some("vanilla"));
		Document::remove_meta("author");
		assert_eq!(Document::get_meta_content("author"), None);
		apply_engine_configuration("", &[]);
		assert_eq!(Document::get_language(), "");
		assert_eq!(Document::get_meta_content("build-version"), None);
	}
}

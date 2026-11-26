//==============================================================================
//! 타자기 연출기.
//! - 원본 misc/typewriter.js 를 옮긴 것이다. Text 처럼 보일 글자 수를 정할 수 있는 대상의 글자를 초당 정한 속도로 한 자씩 드러낸다.
//!   (비주얼노벨 / RPG 대사 연출)
//! - 사용:
//!   ```text
//!     let mut typewriter = Typewriter::new(TypewriterOptions { characters_per_second: 30.0, ..TypewriterOptions::default() });
//!     let label = node_tree[label_id].get_component_mut::<Text>().unwrap();
//!     typewriter.start(label, Some("안녕하세요. 모험을 시작합니다."));
//!     // 매 프레임: typewriter.tick(label, time_delta);
//!     // 입력에서: 끝났으면(typewriter.is_finished()) 다음 대사로, 아니면 typewriter.skip_to_end(label)
//!   ```
//! - 원본과 다른 점:
//!   원본은 대상을 생성자에서 받아 들고 있는다. 러스트 판은 노드 안의 컴포넌트를 들고 있을 수 없어서
//!   start / tick / skip_to_end 때마다 대상(&mut dyn TypewriterTarget)을 넘긴다. Text 는 TypewriterTarget 을 구현한다.
//!   글자 수는 원본처럼 글자 묶음(받침, 결합 부호, 이모지 묶음을 한 자로) 단위로 센다. 글자 공개 알림은 (글자 묶음, 번호) 를 받는다.
//!   options 객체는 TypewriterOptions 구조체다.
//==============================================================================
use crate::core::component::text::Text;
use unicode_segmentation::UnicodeSegmentation;

//==============================================================================
/// 타자기 대상. (글 설정, 글 반환, 보일 글자 수 설정. -1 이면 전체)
/// - Text 가 구현한다. 다른 글 그리기도 이것을 구현하면 Typewriter 로 드러낼 수 있다.
///
/// # 예제
/// 보일 글자 수만 기억하는 대상
/// ```rust
/// use vanilla::{Typewriter, TypewriterOptions, TypewriterTarget};
///
/// struct CountingLabel {
/// 	text: String,
/// 	visible_character_count: i32,
/// }
///
/// impl TypewriterTarget for CountingLabel {
/// 	fn set_text(&mut self, text: &str) {
/// 		self.text = text.to_string();
/// 	}
///
/// 	fn get_text(&self) -> &str {
/// 		&self.text
/// 	}
///
/// 	fn set_visible_character_count(&mut self, visible_character_count: i32) {
/// 		self.visible_character_count = visible_character_count;
/// 	}
/// }
///
/// let mut label = CountingLabel {
/// 	text: String::new(),
/// 	visible_character_count: -1,
/// };
/// let mut typewriter = Typewriter::new(TypewriterOptions {
/// 	characters_per_second: 10.0,
/// 	..TypewriterOptions::default()
/// });
/// typewriter.start(&mut label, Some("hello"));
/// typewriter.tick(&mut label, 0.25);
/// assert_eq!(label.visible_character_count, 2);
/// ```
//==============================================================================
pub trait TypewriterTarget {
	//==============================================================================
	/// 글 설정.
	///
	/// # 인자
	/// - `text`: 새 글.
	//==============================================================================
	fn set_text(&mut self, text: &str);

	//==============================================================================
	/// 글 반환.
	///
	/// # 돌려줌
	/// 지금 글 전체. (보일 글자 수와 상관없다)
	//==============================================================================
	fn get_text(&self) -> &str;

	//==============================================================================
	/// 보일 글자 수 설정. (-1 이면 전체)
	///
	/// # 인자
	/// - `visible_character_count`: 앞에서부터 보일 글자 묶음 수. (-1 이면 전체)
	//==============================================================================
	fn set_visible_character_count(&mut self, visible_character_count: i32);
}

impl TypewriterTarget for Text {
	//==============================================================================
	/// 글 설정. (Text::set_text)
	//==============================================================================
	fn set_text(&mut self, text: &str) {
		Text::set_text(self, text);
	}

	//==============================================================================
	/// 글 반환. (Text::get_text)
	//==============================================================================
	fn get_text(&self) -> &str {
		Text::get_text(self)
	}

	//==============================================================================
	/// 보일 글자 수 설정. (Text::set_visible_character_count)
	//==============================================================================
	fn set_visible_character_count(&mut self, visible_character_count: i32) {
		Text::set_visible_character_count(self, visible_character_count);
	}
}

//==============================================================================
/// 타자기 설정. (원본 options = { characters_per_second = 30, fast_forward_multiplier = 8 })
///
/// # 예제
/// 속도만 바꾸기
/// ```rust
/// use vanilla::{Typewriter, TypewriterOptions};
///
/// let typewriter_options = TypewriterOptions {
/// 	characters_per_second: 20.0,
/// 	..TypewriterOptions::default()
/// };
/// assert_eq!(typewriter_options.fast_forward_multiplier, 8.0);
/// let typewriter = Typewriter::new(typewriter_options);
/// assert!(typewriter.is_finished());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypewriterOptions {
	/// 초당 글자 수.
	pub characters_per_second: f64,
	/// 빨리 감기일 때 곱하는 배율.
	pub fast_forward_multiplier: f64,
}

impl Default for TypewriterOptions {
	//==============================================================================
	/// 기본값. (초당 30 자, 빨리 감기 8 배)
	//==============================================================================
	fn default() -> TypewriterOptions {
		TypewriterOptions {
			characters_per_second: 30.0,
			fast_forward_multiplier: 8.0,
		}
	}
}

/// 글자 공개 알림. (글자, 번호)
type CharacterRevealedEvent = Box<dyn FnMut(&str, usize)>;
/// 완료 알림.
type CompleteEvent = Box<dyn FnMut()>;

//==============================================================================
/// 타자기 연출기.
/// - start 로 대상과 글을 넘기고, 매 프레임 tick 에 같은 대상을 넘긴다. 입력이 오면 끝났으면 다음 대사로, 아니면 skip_to_end.
///
/// # 예제
/// 노드의 Text 에 대사 드러내기
/// ```rust
/// use vanilla::{NodeTree, Text, Typewriter, TypewriterOptions};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let label_id = node_tree.create_child(root_id, "label");
/// node_tree[label_id].add_component(Text::new());
///
/// let mut typewriter = Typewriter::new(TypewriterOptions {
/// 	characters_per_second: 30.0,
/// 	..TypewriterOptions::default()
/// });
/// let label = node_tree[label_id].get_component_mut::<Text>().unwrap();
/// typewriter.start(label, Some("Hello. The adventure begins."));
/// typewriter.tick(label, 0.1);
/// assert_eq!(label.get_visible_character_count(), 3);
///
/// if typewriter.is_finished() {
/// 	typewriter.start(label, Some("Next line."));
/// }
/// else {
/// 	typewriter.skip_to_end(label);
/// }
/// assert!(typewriter.is_finished());
/// assert_eq!(label.get_visible_character_count(), -1);
/// ```
//==============================================================================
pub struct Typewriter {
	full_characters: Vec<String>,
	revealed_count: f64,
	characters_per_second: f64,
	fast_forward_multiplier: f64,
	is_fast_forward: bool,
	is_finished: bool,
	character_revealed_event: Option<CharacterRevealedEvent>,
	complete_event: Option<CompleteEvent>,
}

impl Typewriter {
	//==============================================================================
	/// 생성. (처음에는 끝난 상태)
	///
	/// # 인자
	/// - `options`: 초당 글자 수와 빨리 감기 배율.
	///
	/// # 돌려줌
	/// 글이 없고 끝난 상태인 새 타자기.
	//==============================================================================
	pub fn new(options: TypewriterOptions) -> Typewriter {
		Typewriter {
			full_characters: Vec::new(),
			revealed_count: 0.0,
			characters_per_second: options.characters_per_second,
			fast_forward_multiplier: options.fast_forward_multiplier,
			is_fast_forward: false,
			is_finished: true,
			character_revealed_event: None,
			complete_event: None,
		}
	}

	//==============================================================================
	/// 시작. (text 를 넘기면 대상의 글도 바꾼다. 없으면 대상의 지금 글을 쓴다. 빈 글이면 바로 끝난 상태)
	/// - 대상의 보일 글자 수를 0 으로 하고 빨리 감기를 끈다. 빈 글이면 -1(전체)로 둔다. 완료 알림은 부르지 않는다.
	///
	/// # 인자
	/// - `target`: 글을 드러낼 대상.
	/// - `text`: 새로 넣을 글. None 이면 대상의 지금 글.
	//==============================================================================
	pub fn start(&mut self, target: &mut dyn TypewriterTarget, text: Option<&str>) {
		if let Some(text) = text {
			target.set_text(text);
		}
		self.full_characters = target.get_text().graphemes(true).map(|grapheme| grapheme.to_string()).collect();
		self.revealed_count = 0.0;
		self.is_finished = self.full_characters.is_empty();
		self.is_fast_forward = false;
		target.set_visible_character_count(0);
		if self.is_finished {
			target.set_visible_character_count(-1);
		}
	}

	//==============================================================================
	/// 갱신. (쌓인 만큼 글자를 드러내고, 새로 드러난 글자마다 알린다. 다 드러나면 끝낸다)
	/// - 끝난 뒤에는 아무것도 하지 않는다. 다 드러나면 보일 글자 수를 -1 로 하고 완료를 알린다.
	///
	/// # 인자
	/// - `target`: start 에 넘긴 대상.
	/// - `time_delta`: 지난 시간. (초, 초당 글자 수 * 배율 * time_delta 만큼 쌓는다)
	///
	/// # 예제
	/// 드러난 글자마다 알림 받기
	/// ```rust
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use vanilla::{Text, Typewriter, TypewriterOptions};
	///
	/// let revealed_list = Rc::new(RefCell::new(Vec::new()));
	/// let revealed_list_for_event = revealed_list.clone();
	/// let mut label = Text::new();
	/// let mut typewriter = Typewriter::new(TypewriterOptions {
	/// 	characters_per_second: 10.0,
	/// 	..TypewriterOptions::default()
	/// });
	/// typewriter.set_character_revealed_event(move |character, character_index| {
	/// 	revealed_list_for_event.borrow_mut().push((character.to_string(), character_index));
	/// });
	/// typewriter.start(&mut label, Some("abc"));
	/// typewriter.tick(&mut label, 0.25);
	/// assert_eq!(*revealed_list.borrow(), vec![(String::from("a"), 0), (String::from("b"), 1)]);
	/// ```
	//==============================================================================
	pub fn tick(&mut self, target: &mut dyn TypewriterTarget, time_delta: f64) {
		if self.is_finished {
			return;
		}
		let speed_multiplier = if self.is_fast_forward {
			self.fast_forward_multiplier
		}
		else {
			1.0
		};
		let speed = self.characters_per_second * speed_multiplier;
		let character_count = self.full_characters.len() as f64;
		let previous_whole_count = self.revealed_count.floor() as usize;
		self.revealed_count = character_count.min(self.revealed_count + speed * time_delta);
		let current_whole_count = self.revealed_count.floor() as usize;
		if current_whole_count != previous_whole_count {
			target.set_visible_character_count(current_whole_count as i32);
			if let Some(character_revealed_event) = self.character_revealed_event.as_mut() {
				for character_index in previous_whole_count..current_whole_count {
					character_revealed_event(&self.full_characters[character_index], character_index);
				}
			}
		}
		if self.revealed_count >= character_count {
			self.finish(target);
		}
	}

	//==============================================================================
	/// 곧바로 끝까지 드러내기. (드러나지 않은 글자의 공개 알림은 부르지 않는다)
	/// - 끝난 상태면 아무것도 하지 않는다. 아니면 보일 글자 수를 -1 로 하고 완료를 알린다.
	///
	/// # 인자
	/// - `target`: start 에 넘긴 대상.
	//==============================================================================
	pub fn skip_to_end(&mut self, target: &mut dyn TypewriterTarget) {
		if self.is_finished {
			return;
		}
		self.revealed_count = self.full_characters.len() as f64;
		self.finish(target);
	}

	//==============================================================================
	/// 끝내기. (대상의 글을 다 보이게 하고 완료를 알린다)
	//==============================================================================
	fn finish(&mut self, target: &mut dyn TypewriterTarget) {
		self.is_finished = true;
		target.set_visible_character_count(-1);
		if let Some(complete_event) = self.complete_event.as_mut() {
			complete_event();
		}
	}

	//==============================================================================
	/// 끝났는지.
	///
	/// # 돌려줌
	/// 글을 다 드러냈거나 아직 start 하지 않았으면 true.
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.is_finished
	}

	//==============================================================================
	/// 빨리 감기 설정. (누르고 있는 동안 등)
	///
	/// # 인자
	/// - `is_fast_forward`: true 면 속도에 빨리 감기 배율을 곱한다. (start 하면 false 로 돌아간다)
	//==============================================================================
	pub fn set_fast_forward(&mut self, is_fast_forward: bool) {
		self.is_fast_forward = is_fast_forward;
	}

	//==============================================================================
	/// 속도 설정. (초당 글자 수)
	///
	/// # 인자
	/// - `characters_per_second`: 초당 드러낼 글자 묶음 수.
	//==============================================================================
	pub fn set_characters_per_second(&mut self, characters_per_second: f64) {
		self.characters_per_second = characters_per_second;
	}

	//==============================================================================
	/// 글자 공개 알림 설정. (글자, 번호)
	///
	/// # 인자
	/// - `character_revealed_event`: 글자가 드러날 때마다 부를 함수. 그 글자 묶음과 0 부터 센 번호를 받는다.
	//==============================================================================
	pub fn set_character_revealed_event(&mut self, character_revealed_event: impl FnMut(&str, usize) + 'static) {
		self.character_revealed_event = Some(Box::new(character_revealed_event));
	}

	//==============================================================================
	/// 완료 알림 설정.
	///
	/// # 인자
	/// - `complete_event`: 다 드러나거나 skip_to_end 할 때 부를 함수.
	//==============================================================================
	pub fn set_complete_event(&mut self, complete_event: impl FnMut() + 'static) {
		self.complete_event = Some(Box::new(complete_event));
	}
}

impl Default for Typewriter {
	//==============================================================================
	/// 기본값. (기본 설정으로 만든다)
	//==============================================================================
	fn default() -> Typewriter {
		Typewriter::new(TypewriterOptions::default())
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	//==============================================================================
	/// 초당 10 자로 드러나고, 드러난 글자마다 알리고, 끝나면 전체(-1)로 돌린다.
	//==============================================================================
	#[test]
	fn reveals_characters_over_time() {
		let revealed_list: Rc<RefCell<Vec<(String, usize)>>> = Rc::new(RefCell::new(Vec::new()));
		let revealed_list_for_event = revealed_list.clone();
		let complete_count = Rc::new(RefCell::new(0));
		let complete_count_for_event = complete_count.clone();
		let mut label = Text::new();
		let mut typewriter = Typewriter::new(TypewriterOptions {
			characters_per_second: 10.0,
			..TypewriterOptions::default()
		});
		typewriter.set_character_revealed_event(move |character, character_index| {
			revealed_list_for_event.borrow_mut().push((character.to_string(), character_index));
		});
		typewriter.set_complete_event(move || {
			*complete_count_for_event.borrow_mut() += 1;
		});

		typewriter.start(&mut label, Some("안녕 세상"));
		assert_eq!(label.get_text(), "안녕 세상");
		assert_eq!(label.get_visible_character_count(), 0);
		assert!(!typewriter.is_finished());

		typewriter.tick(&mut label, 0.25);
		assert_eq!(label.get_visible_character_count(), 2);
		assert_eq!(*revealed_list.borrow(), vec![(String::from("안"), 0), (String::from("녕"), 1)]);

		typewriter.set_fast_forward(true);
		typewriter.tick(&mut label, 0.1);
		assert!(typewriter.is_finished());
		assert_eq!(label.get_visible_character_count(), -1);
		assert_eq!(revealed_list.borrow().len(), 5);
		assert_eq!(*complete_count.borrow(), 1);

		// 끝난 뒤에는 아무것도 하지 않는다.
		typewriter.tick(&mut label, 1.0);
		typewriter.skip_to_end(&mut label);
		assert_eq!(*complete_count.borrow(), 1);
	}

	//==============================================================================
	/// 건너뛰기, 대상의 지금 글로 시작, 빈 글은 바로 끝.
	//==============================================================================
	#[test]
	fn skip_and_empty_text() {
		let mut label = Text::create_with_text("abcdef", 20.0);
		let mut typewriter = Typewriter::default();
		typewriter.start(&mut label, None);
		typewriter.tick(&mut label, 0.1);
		assert_eq!(label.get_visible_character_count(), 3);
		typewriter.skip_to_end(&mut label);
		assert!(typewriter.is_finished());
		assert_eq!(label.get_visible_character_count(), -1);

		typewriter.start(&mut label, Some(""));
		assert!(typewriter.is_finished());
		assert_eq!(label.get_visible_character_count(), -1);

		// 이모지 묶음과 결합 부호는 한 자다.
		typewriter.start(&mut label, Some("👨‍👩‍👧e\u{301}x"));
		typewriter.tick(&mut label, 0.05);
		assert_eq!(label.get_visible_character_count(), 1);
		typewriter.skip_to_end(&mut label);
		typewriter.start(&mut label, Some("👨‍👩‍👧e\u{301}xy"));
		typewriter.tick(&mut label, 0.1);
		assert_eq!(label.get_visible_character_count(), 3);
	}
}

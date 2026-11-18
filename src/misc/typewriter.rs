//==============================================================================
// 타자기 연출기.
// - 원본 misc/typewriter.js 를 옮긴 것이다. Text 처럼 보일 글자 수를 정할 수 있는 대상의 글자를 초당 정한 속도로 한 자씩 드러낸다.
//   (비주얼노벨 / RPG 대사 연출)
// - 사용:
//     let mut typewriter = Typewriter::new(TypewriterOptions { charactersPerSecond: 30.0, ..TypewriterOptions::default() });
//     let label = nodeTree[labelId].getComponentMut::<Text>().unwrap();
//     typewriter.start(label, Some("안녕하세요. 모험을 시작합니다."));
//     // 매 프레임: typewriter.tick(label, timeDelta);
//     // 입력에서: 끝났으면(typewriter.isFinished()) 다음 대사로, 아니면 typewriter.skipToEnd(label)
// - 원본과 다른 점:
//   원본은 대상을 생성자에서 받아 들고 있는다. 러스트 판은 노드 안의 컴포넌트를 들고 있을 수 없어서
//   start / tick / skipToEnd 때마다 대상(&mut dyn TypewriterTarget)을 넘긴다. Text 는 TypewriterTarget 을 구현한다.
//   글자 수는 원본(UTF-16 단위)과 달리 글자(char) 단위로 센다. 글자 공개 알림은 (char, 번호) 를 받는다.
//   options 객체는 TypewriterOptions 구조체다.
//==============================================================================
use crate::core::component::text::Text;

//==============================================================================
// 타자기 대상. (글 설정, 글 반환, 보일 글자 수 설정. -1 이면 전체)
//==============================================================================
pub trait TypewriterTarget {
	//==============================================================================
	// 글 설정.
	//==============================================================================
	fn setText(&mut self, text: &str);

	//==============================================================================
	// 글 반환.
	//==============================================================================
	fn getText(&self) -> &str;

	//==============================================================================
	// 보일 글자 수 설정. (-1 이면 전체)
	//==============================================================================
	fn setVisibleCharacterCount(&mut self, visibleCharacterCount: i32);
}

impl TypewriterTarget for Text {
	//==============================================================================
	// 글 설정. (Text::setText)
	//==============================================================================
	fn setText(&mut self, text: &str) {
		Text::setText(self, text);
	}

	//==============================================================================
	// 글 반환. (Text::getText)
	//==============================================================================
	fn getText(&self) -> &str {
		Text::getText(self)
	}

	//==============================================================================
	// 보일 글자 수 설정. (Text::setVisibleCharacterCount)
	//==============================================================================
	fn setVisibleCharacterCount(&mut self, visibleCharacterCount: i32) {
		Text::setVisibleCharacterCount(self, visibleCharacterCount);
	}
}

//==============================================================================
// 타자기 설정. (원본 options = { charactersPerSecond = 30, fastForwardMultiplier = 8 })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypewriterOptions {
	// 초당 글자 수.
	pub charactersPerSecond: f32,
	// 빨리 감기일 때 곱하는 배율.
	pub fastForwardMultiplier: f32,
}

impl Default for TypewriterOptions {
	//==============================================================================
	// 기본값. (초당 30 자, 빨리 감기 8 배)
	//==============================================================================
	fn default() -> TypewriterOptions {
		TypewriterOptions {
			charactersPerSecond: 30.0,
			fastForwardMultiplier: 8.0,
		}
	}
}

// 글자 공개 알림. (글자, 번호)
type CharacterRevealedEvent = Box<dyn FnMut(char, usize)>;
// 완료 알림.
type CompleteEvent = Box<dyn FnMut()>;

//==============================================================================
// 타자기 연출기.
//==============================================================================
pub struct Typewriter {
	fullCharacters: Vec<char>,
	revealedCount: f64,
	charactersPerSecond: f32,
	fastForwardMultiplier: f32,
	isFastForward: bool,
	isFinished: bool,
	characterRevealedEvent: Option<CharacterRevealedEvent>,
	completeEvent: Option<CompleteEvent>,
}

impl Typewriter {
	//==============================================================================
	// 생성. (처음에는 끝난 상태)
	//==============================================================================
	pub fn new(options: TypewriterOptions) -> Typewriter {
		Typewriter {
			fullCharacters: Vec::new(),
			revealedCount: 0.0,
			charactersPerSecond: options.charactersPerSecond,
			fastForwardMultiplier: options.fastForwardMultiplier,
			isFastForward: false,
			isFinished: true,
			characterRevealedEvent: None,
			completeEvent: None,
		}
	}

	//==============================================================================
	// 시작. (text 를 넘기면 대상의 글도 바꾼다. 없으면 대상의 지금 글을 쓴다. 빈 글이면 바로 끝난 상태)
	//==============================================================================
	pub fn start(&mut self, target: &mut dyn TypewriterTarget, text: Option<&str>) {
		if let Some(text) = text {
			target.setText(text);
		}
		self.fullCharacters = target.getText().chars().collect();
		self.revealedCount = 0.0;
		self.isFinished = self.fullCharacters.is_empty();
		self.isFastForward = false;
		target.setVisibleCharacterCount(0);
		if self.isFinished {
			target.setVisibleCharacterCount(-1);
		}
	}

	//==============================================================================
	// 갱신. (쌓인 만큼 글자를 드러내고, 새로 드러난 글자마다 알린다. 다 드러나면 끝낸다)
	//==============================================================================
	pub fn tick(&mut self, target: &mut dyn TypewriterTarget, timeDelta: f32) {
		if self.isFinished {
			return;
		}
		let speedMultiplier = if self.isFastForward {
			self.fastForwardMultiplier
		}
		else {
			1.0
		};
		let speed = (self.charactersPerSecond * speedMultiplier) as f64;
		let characterCount = self.fullCharacters.len() as f64;
		let previousWholeCount = self.revealedCount.floor() as usize;
		self.revealedCount = characterCount.min(self.revealedCount + speed * timeDelta as f64);
		let currentWholeCount = self.revealedCount.floor() as usize;
		if currentWholeCount != previousWholeCount {
			target.setVisibleCharacterCount(currentWholeCount as i32);
			if let Some(characterRevealedEvent) = self.characterRevealedEvent.as_mut() {
				for characterIndex in previousWholeCount..currentWholeCount {
					characterRevealedEvent(self.fullCharacters[characterIndex], characterIndex);
				}
			}
		}
		if self.revealedCount >= characterCount {
			self.finish(target);
		}
	}

	//==============================================================================
	// 곧바로 끝까지 드러내기. (드러나지 않은 글자의 공개 알림은 부르지 않는다)
	//==============================================================================
	pub fn skipToEnd(&mut self, target: &mut dyn TypewriterTarget) {
		if self.isFinished {
			return;
		}
		self.revealedCount = self.fullCharacters.len() as f64;
		self.finish(target);
	}

	//==============================================================================
	// 끝내기. (대상의 글을 다 보이게 하고 완료를 알린다)
	//==============================================================================
	fn finish(&mut self, target: &mut dyn TypewriterTarget) {
		self.isFinished = true;
		target.setVisibleCharacterCount(-1);
		if let Some(completeEvent) = self.completeEvent.as_mut() {
			completeEvent();
		}
	}

	//==============================================================================
	// 끝났는지.
	//==============================================================================
	pub fn isFinished(&self) -> bool {
		self.isFinished
	}

	//==============================================================================
	// 빨리 감기 설정. (누르고 있는 동안 등)
	//==============================================================================
	pub fn setFastForward(&mut self, isFastForward: bool) {
		self.isFastForward = isFastForward;
	}

	//==============================================================================
	// 속도 설정. (초당 글자 수)
	//==============================================================================
	pub fn setCharactersPerSecond(&mut self, charactersPerSecond: f32) {
		self.charactersPerSecond = charactersPerSecond;
	}

	//==============================================================================
	// 글자 공개 알림 설정. (글자, 번호)
	//==============================================================================
	pub fn setCharacterRevealedEvent(&mut self, characterRevealedEvent: impl FnMut(char, usize) + 'static) {
		self.characterRevealedEvent = Some(Box::new(characterRevealedEvent));
	}

	//==============================================================================
	// 완료 알림 설정.
	//==============================================================================
	pub fn setCompleteEvent(&mut self, completeEvent: impl FnMut() + 'static) {
		self.completeEvent = Some(Box::new(completeEvent));
	}
}

impl Default for Typewriter {
	//==============================================================================
	// 기본값. (기본 설정으로 만든다)
	//==============================================================================
	fn default() -> Typewriter {
		Typewriter::new(TypewriterOptions::default())
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	//==============================================================================
	// 초당 10 자로 드러나고, 드러난 글자마다 알리고, 끝나면 전체(-1)로 돌린다.
	//==============================================================================
	#[test]
	fn revealsCharactersOverTime() {
		let revealedList: Rc<RefCell<Vec<(char, usize)>>> = Rc::new(RefCell::new(Vec::new()));
		let revealedListForEvent = revealedList.clone();
		let completeCount = Rc::new(RefCell::new(0));
		let completeCountForEvent = completeCount.clone();
		let mut label = Text::new();
		let mut typewriter = Typewriter::new(TypewriterOptions {
			charactersPerSecond: 10.0,
			..TypewriterOptions::default()
		});
		typewriter.setCharacterRevealedEvent(move |character, characterIndex| {
			revealedListForEvent.borrow_mut().push((character, characterIndex));
		});
		typewriter.setCompleteEvent(move || {
			*completeCountForEvent.borrow_mut() += 1;
		});

		typewriter.start(&mut label, Some("안녕 세상"));
		assert_eq!(label.getText(), "안녕 세상");
		assert_eq!(label.getVisibleCharacterCount(), 0);
		assert!(!typewriter.isFinished());

		typewriter.tick(&mut label, 0.25);
		assert_eq!(label.getVisibleCharacterCount(), 2);
		assert_eq!(*revealedList.borrow(), vec![('안', 0), ('녕', 1)]);

		typewriter.setFastForward(true);
		typewriter.tick(&mut label, 0.1);
		assert!(typewriter.isFinished());
		assert_eq!(label.getVisibleCharacterCount(), -1);
		assert_eq!(revealedList.borrow().len(), 5);
		assert_eq!(*completeCount.borrow(), 1);

		// 끝난 뒤에는 아무것도 하지 않는다.
		typewriter.tick(&mut label, 1.0);
		typewriter.skipToEnd(&mut label);
		assert_eq!(*completeCount.borrow(), 1);
	}

	//==============================================================================
	// 건너뛰기, 대상의 지금 글로 시작, 빈 글은 바로 끝.
	//==============================================================================
	#[test]
	fn skipAndEmptyText() {
		let mut label = Text::createWithText("abcdef", 20.0);
		let mut typewriter = Typewriter::default();
		typewriter.start(&mut label, None);
		typewriter.tick(&mut label, 0.1);
		assert_eq!(label.getVisibleCharacterCount(), 3);
		typewriter.skipToEnd(&mut label);
		assert!(typewriter.isFinished());
		assert_eq!(label.getVisibleCharacterCount(), -1);

		typewriter.start(&mut label, Some(""));
		assert!(typewriter.isFinished());
		assert_eq!(label.getVisibleCharacterCount(), -1);
	}
}

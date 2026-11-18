//==============================================================================
// 입력 필드. (원본 ui/uiinputfield.js 의 UIInputField, BackgroundLayerRenderer, OverlayLayerRenderer)
// - 글, 커서, 선택 영역, IME 조합 글과 밑줄, 포커스 테두리를 캔버스에 직접 그리고, 누르기와 끌기로 커서와 선택을 정한다.
// - 원본은 WorldNode 를 상속한 노드이고, 글은 UILabel 자식 노드 두 개(값, 안내 글), 커서와 테두리는 오버레이 자식 노드가 그린다.
//   러스트 판은 다른 위젯처럼 컴포넌트다. nodeTree.addComponent(nodeId, UIInputField::new()) 로 붙인다.
//   UIControl 을 품어 attachToTree 에서 content 노드를 만들고 호스트를 interactable 로 켠다. 피봇과 앵커는 원본처럼 왼쪽 위로 맞춘다.
//   글은 Text 두 개(값, 안내 글)를 품어 그리고, 바탕(원본 Paint 컴포넌트)도 품은 Paint 로 그린다.
//   층 순서는 원본과 같다: 바탕 → 선택 영역 → 글 → 조합 밑줄 → 커서 → 포커스 테두리. 한 컴포넌트가 한 번에 그려서
//   씬 코드가 tick 밖에서 setText 를 불러도 그 프레임에 바로 보인다. (자식 노드로 나누면 상태를 옮기느라 한 프레임 늦는다)
// - 입력은 엔진의 글 입력 줄(core/textinput.rs)에서 받는다. 포커스를 가진, attach() 한 필드가 tick 에서 꺼내 쓴다.
//   - 데스크탑: winit 의 KeyboardInput 과 Ime 이벤트. useDOMInput(원본 setUseDOMInput)이 켜진 필드가 포커스를 가질 때만 IME 가 켜진다.
//   - 웹: 원본처럼 필드마다 숨긴 DOM <input> 을 필드 위에 둔다. (platform/web/textinput.rs) 값과 선택은 DOM 이 주인이고 필드가 따라간다.
//     useDOMInput 이 꺼진 필드는 원본 캔버스 모드처럼 window keydown 으로 받는다.
// - 원본과 다른 것:
//   - 이벤트는 Box<dyn FnMut(&mut UIInputField, &mut ComponentContext)> 다. 원본 콜백은 값만 받는다. (값은 field.getText())
//   - setFont(FontFace) 는 setFontFamily(등록한 폰트 이름) 로 바꿨다. setEngine 은 옮기지 않았다. (엔진은 문맥으로 받는다)
//   - 글자 자리는 문자(char) 단위다. (원본은 UTF-16 단위. 한글은 같다) maxLength 도 문자 수다.
//   - 누른 자리 → 글자 자리는 draw 때 재 둔 글자 경계로 찾는다. (원본은 오프스크린 2D 캔버스로 그때그때 잰다)
//   - 데스크탑에서는 useDOMInput 이 켜진 필드도 원본 DOM 모드처럼 필드 밖을 누르면 포커스를 잃는다. 포커스는 엔진에 하나뿐이다.
//     (원본 캔버스 모드는 다른 필드를 눌러도 앞 필드가 포커스를 쥐고 있다. 러스트 판은 나중에 포커스를 얻은 필드만 남긴다)
//   - 포커스를 잃을 때 조합 중이던 글은 확정해 값에 넣는다. (브라우저가 DOM 입력에 하는 일과 같다)
//   - 데스크탑 문맥 메뉴(오른쪽 단추)는 DOM 이 없어 캔버스에 그린다. 루트의 마지막 자식 노드로 만들어 위에 그린다. 웹은 원본처럼 DOM 메뉴다.
//   - maxLength 로 잘린 붙여넣기에서 원본은 커서가 글 끝을 넘을 수 있다. 러스트 판은 글 끝으로 자른다.
//   - 노드를 끄거나(setActive(false)) 지우면 tick 이 돌지 않아 포커스와 IME, 웹의 숨긴 <input> 이 함께 사라진다. (원본은 detach 해야 사라진다)
//   - getRenderState 는 옮기지 않았다. (그리는 컴포넌트가 하나라 넘길 일이 없다)
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::engine::Engine;
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::core::textinput::{convertCharacterIndexToByteIndex, countCharacters, TextInputEventKind, TextInputFieldState, TextInputKey};
use crate::ui::uicontrol::UIControl;
use crate::ui::uiview::isViewPositionInsideNode;
use std::sync::atomic::{AtomicU64, Ordering};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 커서 깜빡임 주기. (초)
const CURSOR_BLINK_SECONDS: f32 = 0.5;
// 폰트 이름을 정하지 않았을 때 쓰는 이름. (엔진 기본 폰트, 원본 SYSTEM_FONT_STRING 자리)
const DEFAULT_FONT_FAMILY: &str = "default";
// 문맥 메뉴 크기. (원본 DOM 메뉴: 최소 폭 140, 위아래 여백 4, 항목 위아래 여백 8, 글자 14px)
const CONTEXT_MENU_WIDTH: f32 = 140.0;
const CONTEXT_MENU_PADDING_Y: f32 = 4.0;
const CONTEXT_MENU_ITEM_HEIGHT: f32 = 34.0;
const CONTEXT_MENU_ITEM_PADDING_X: f32 = 16.0;
const CONTEXT_MENU_FONT_STRING: &str = "14px default";

// 필드 id. (엔진 글 입력 줄에서 필드를 가리킨다. 0 은 "포커스를 가진 필드"라 1 부터)
static NEXT_FIELD_ID: AtomicU64 = AtomicU64::new(1);
// 포커스 차례. (나중에 포커스를 얻은 필드가 이긴다)
static NEXT_FOCUS_SERIAL: AtomicU64 = AtomicU64::new(1);

//==============================================================================
// 입력 필드 이벤트. (필드와 문맥을 받는다. 값은 field.getText())
//==============================================================================
pub type UIInputFieldEvent = Box<dyn FnMut(&mut UIInputField, &mut ComponentContext)>;

//==============================================================================
// 문맥 메뉴 항목. (원본 잘라내기 / 복사 / 붙여넣기)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UIInputFieldMenuAction {
	cut,
	copy,
	paste,
}

//==============================================================================
// 편집 버퍼. (값, 조합 글, 커서, 선택 시작. 원본 UIInputField 의 #value / #composition / #cursorIndex / #selectionStart 와 편집 함수)
// - 자리는 문자 단위다. 커서와 선택 시작이 다르면 그 사이가 선택 영역이다. (커서가 앞일 수도 있다)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct TextEditBuffer {
	value: String,
	composition: String,
	cursorIndex: usize,
	selectionStart: usize,
	maxLength: usize,
}

impl TextEditBuffer {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TextEditBuffer {
		TextEditBuffer::default()
	}

	//==============================================================================
	// 값 설정. (maxLength 로 자르고 커서는 끝에. 원본 setText)
	//==============================================================================
	pub fn setText(&mut self, text: &str) {
		self.value = text.to_string();
		self.truncateToMaxLength();
		self.cursorIndex = countCharacters(&self.value);
		self.selectionStart = self.cursorIndex;
	}

	//==============================================================================
	// 값 반환.
	//==============================================================================
	pub fn getText(&self) -> &str {
		&self.value
	}

	//==============================================================================
	// 플랫폼이 준 값으로 바꾸기. (자르지 않는다. 커서를 정하고 선택은 접는다. 원본 handleDOMInputEvent)
	//==============================================================================
	pub fn setValueFromPlatform(&mut self, value: &str, cursorIndex: usize) {
		self.value = value.to_string();
		self.cursorIndex = cursorIndex.min(countCharacters(&self.value));
		self.selectionStart = self.cursorIndex;
	}

	//==============================================================================
	// 최대 길이 설정. (0 이면 제한 없음. 원본처럼 지금 값은 자르지 않는다)
	//==============================================================================
	pub fn setMaxLength(&mut self, maxLength: usize) {
		self.maxLength = maxLength;
	}

	//==============================================================================
	// 최대 길이 반환.
	//==============================================================================
	pub fn getMaxLength(&self) -> usize {
		self.maxLength
	}

	//==============================================================================
	// 값을 최대 길이로 자르기.
	//==============================================================================
	fn truncateToMaxLength(&mut self) {
		if self.maxLength > 0 && countCharacters(&self.value) > self.maxLength {
			let byteIndex = convertCharacterIndexToByteIndex(&self.value, self.maxLength);
			self.value.truncate(byteIndex);
		}
	}

	//==============================================================================
	// 조합 글 설정.
	//==============================================================================
	pub fn setComposition(&mut self, composition: &str) {
		self.composition = composition.to_string();
	}

	//==============================================================================
	// 조합 글 반환.
	//==============================================================================
	pub fn getComposition(&self) -> &str {
		&self.composition
	}

	//==============================================================================
	// 커서 자리.
	//==============================================================================
	pub fn getCursorIndex(&self) -> usize {
		self.cursorIndex
	}

	//==============================================================================
	// 선택 시작 자리. (선택을 늘릴 때 고정되는 쪽)
	//==============================================================================
	pub fn getSelectionStart(&self) -> usize {
		self.selectionStart
	}

	//==============================================================================
	// 선택 정하기. (끝을 넘으면 자른다)
	//==============================================================================
	pub fn setSelection(&mut self, selectionStart: usize, cursorIndex: usize) {
		let characterCount = countCharacters(&self.value);
		self.selectionStart = selectionStart.min(characterCount);
		self.cursorIndex = cursorIndex.min(characterCount);
	}

	//==============================================================================
	// 선택이 있는지.
	//==============================================================================
	pub fn hasSelection(&self) -> bool {
		self.cursorIndex != self.selectionStart
	}

	//==============================================================================
	// 선택 범위. (앞, 뒤. 없으면 None)
	//==============================================================================
	pub fn getSelectionRange(&self) -> Option<(usize, usize)> {
		if !self.hasSelection() {
			return None;
		}
		Some((self.cursorIndex.min(self.selectionStart), self.cursorIndex.max(self.selectionStart)))
	}

	//==============================================================================
	// 선택된 글.
	//==============================================================================
	pub fn getSelectionText(&self) -> String {
		match self.getSelectionRange() {
			Some((startIndex, endIndex)) => {
				let startByteIndex = convertCharacterIndexToByteIndex(&self.value, startIndex);
				let endByteIndex = convertCharacterIndexToByteIndex(&self.value, endIndex);
				self.value[startByteIndex..endByteIndex].to_string()
			}
			None => {
				String::new()
			}
		}
	}

	//==============================================================================
	// 선택을 글로 바꾸기. (선택이 없으면 커서 자리에 끼운다. maxLength 로 자른다. 원본 replaceSelection)
	//==============================================================================
	pub fn replaceSelection(&mut self, text: &str) {
		let (startIndex, endIndex) = self.getSelectionRange().unwrap_or((self.cursorIndex, self.cursorIndex));
		let startByteIndex = convertCharacterIndexToByteIndex(&self.value, startIndex);
		let endByteIndex = convertCharacterIndexToByteIndex(&self.value, endIndex);
		let mut nextValue = String::with_capacity(self.value.len() + text.len());
		nextValue.push_str(&self.value[..startByteIndex]);
		nextValue.push_str(text);
		nextValue.push_str(&self.value[endByteIndex..]);
		self.value = nextValue;
		self.truncateToMaxLength();
		self.cursorIndex = (startIndex + countCharacters(text)).min(countCharacters(&self.value));
		self.selectionStart = self.cursorIndex;
	}

	//==============================================================================
	// 글자 하나 지우기. (원본 deleteCharAt)
	//==============================================================================
	pub fn deleteCharacterAt(&mut self, characterIndex: usize) {
		if characterIndex >= countCharacters(&self.value) {
			return;
		}
		let startByteIndex = convertCharacterIndexToByteIndex(&self.value, characterIndex);
		let endByteIndex = convertCharacterIndexToByteIndex(&self.value, characterIndex + 1);
		self.value.replace_range(startByteIndex..endByteIndex, "");
		if self.cursorIndex > characterIndex {
			self.cursorIndex -= 1;
		}
		self.selectionStart = self.cursorIndex;
	}

	//==============================================================================
	// 뒤로 지우기. (선택이 있으면 선택을, 없으면 커서 앞 글자. 바뀌었으면 true)
	//==============================================================================
	pub fn deleteBackward(&mut self) -> bool {
		if self.hasSelection() {
			self.replaceSelection("");
			return true;
		}
		if self.cursorIndex > 0 {
			self.deleteCharacterAt(self.cursorIndex - 1);
			return true;
		}
		false
	}

	//==============================================================================
	// 앞으로 지우기. (선택이 있으면 선택을, 없으면 커서 뒤 글자. 바뀌었으면 true)
	//==============================================================================
	pub fn deleteForward(&mut self) -> bool {
		if self.hasSelection() {
			self.replaceSelection("");
			return true;
		}
		if self.cursorIndex < countCharacters(&self.value) {
			self.deleteCharacterAt(self.cursorIndex);
			return true;
		}
		false
	}

	//==============================================================================
	// 커서 옮기기. (글 안으로 자른다. 선택을 늘리지 않으면 선택을 접는다. 원본 moveCursor)
	//==============================================================================
	pub fn moveCursor(&mut self, newIndex: usize, isSelectionExtended: bool) {
		self.cursorIndex = newIndex.min(countCharacters(&self.value));
		if !isSelectionExtended {
			self.selectionStart = self.cursorIndex;
		}
	}

	//==============================================================================
	// 모두 선택. (원본 selectAll)
	//==============================================================================
	pub fn selectAll(&mut self) {
		self.selectionStart = 0;
		self.cursorIndex = countCharacters(&self.value);
	}

	//==============================================================================
	// 커서 앞 글.
	//==============================================================================
	pub fn getTextBeforeCursor(&self) -> &str {
		let byteIndex = convertCharacterIndexToByteIndex(&self.value, self.cursorIndex);
		&self.value[..byteIndex]
	}

	//==============================================================================
	// 보이는 글. (커서 자리에 조합 글을 끼운 것. 원본 refreshTexts)
	//==============================================================================
	pub fn buildDisplayText(&self) -> String {
		let byteIndex = convertCharacterIndexToByteIndex(&self.value, self.cursorIndex);
		let mut displayText = String::with_capacity(self.value.len() + self.composition.len());
		displayText.push_str(&self.value[..byteIndex]);
		displayText.push_str(&self.composition);
		displayText.push_str(&self.value[byteIndex..]);
		displayText
	}
}

//==============================================================================
// 글자 경계 x 목록에서 누른 x 에 가까운 글자 자리 찾기. (원본 measureIndexAtLocalX 의 비교)
// - characterOffsets[i] 는 앞 i 글자의 폭이다. (0 번은 0)
//==============================================================================
pub fn findCharacterIndexAtOffset(characterOffsets: &[f32], targetX: f32) -> usize {
	let characterCount = characterOffsets.len().saturating_sub(1);
	if characterCount == 0 || targetX <= 0.0 {
		return 0;
	}
	for characterIndex in 1..=characterCount {
		let width = characterOffsets[characterIndex];
		if width >= targetX {
			let previousWidth = characterOffsets[characterIndex - 1];
			if targetX - previousWidth < width - targetX {
				return characterIndex - 1;
			}
			return characterIndex;
		}
	}
	characterCount
}

//==============================================================================
// 입력 필드.
//==============================================================================
pub struct UIInputField {
	uiControl: UIControl,
	fieldId: u64,
	editBuffer: TextEditBuffer,
	isComposing: bool,
	dragStartIndex: Option<usize>,
	isFocused: bool,
	focusSerial: u64,
	blinkTimer: f32,
	isCursorVisible: bool,
	useDOMInput: bool,
	useTouchEndFocus: bool,
	useDirectTouch: bool,
	isAttached: bool,
	fontSize: f32,
	fontFamily: String,
	padding: f32,
	placeholder: String,
	backgroundPaint: Paint,
	cursorColor: Color,
	compositionUnderlineColor: Color,
	focusBorderColor: Color,
	focusBorderWidth: f32,
	selectionColor: Color,
	valueText: Text,
	placeholderText: Text,
	changeEvent: Option<UIInputFieldEvent>,
	submitEvent: Option<UIInputFieldEvent>,
	characterOffsets: Vec<f32>,
	measuredValue: String,
	measuredFontString: String,
	caretLocalX: f32,
	valueRevision: u64,
	contextMenuNodeId: Option<NodeId>,
	pendingMenuAction: Option<UIInputFieldMenuAction>,
}

impl UIInputField {
	//==============================================================================
	// 생성. (기본값은 원본과 같다: 28px, 여백 16, 흰 바탕 둥글기 8, 글 #222222, 안내 글 #999999, 커서 #5b8def, 테두리 #f97316 3px)
	//==============================================================================
	pub fn new() -> UIInputField {
		let fontSize = 28.0;
		let mut backgroundPaint = Paint::createWithColor("#ffffff");
		backgroundPaint.setRoundSize(8.0);
		let mut valueText = Text::createWithText("", fontSize);
		valueText.setTextAlign(TextAlign::left);
		valueText.setTextBaseline(TextBaseline::middle);
		valueText.setTextColor("#222222");
		let mut placeholderText = Text::createWithText("", fontSize);
		placeholderText.setTextAlign(TextAlign::left);
		placeholderText.setTextBaseline(TextBaseline::middle);
		placeholderText.setTextColor("#999999");
		UIInputField {
			uiControl: UIControl::new(),
			fieldId: NEXT_FIELD_ID.fetch_add(1, Ordering::Relaxed),
			editBuffer: TextEditBuffer::new(),
			isComposing: false,
			dragStartIndex: None,
			isFocused: false,
			focusSerial: 0,
			blinkTimer: 0.0,
			isCursorVisible: true,
			useDOMInput: false,
			useTouchEndFocus: false,
			useDirectTouch: false,
			isAttached: false,
			fontSize,
			fontFamily: String::from(DEFAULT_FONT_FAMILY),
			padding: 16.0,
			placeholder: String::new(),
			backgroundPaint,
			cursorColor: Color::createFromHEX("#5b8def"),
			compositionUnderlineColor: Color::createFromHEX("#5b8def"),
			focusBorderColor: Color::createFromHEX("#f97316"),
			focusBorderWidth: 3.0,
			selectionColor: Color::create(91.0 / 255.0, 141.0 / 255.0, 239.0 / 255.0, 0.35),
			valueText,
			placeholderText,
			changeEvent: None,
			submitEvent: None,
			characterOffsets: Vec::new(),
			measuredValue: String::new(),
			measuredFontString: String::new(),
			caretLocalX: 16.0,
			valueRevision: 0,
			contextMenuNodeId: None,
			pendingMenuAction: None,
		}
	}

	//==============================================================================
	// 품은 컨트롤 반환.
	//==============================================================================
	pub fn getUIControl(&self) -> &UIControl {
		&self.uiControl
	}

	//==============================================================================
	// 품은 컨트롤 반환. (변경용)
	//==============================================================================
	pub fn getUIControlMut(&mut self) -> &mut UIControl {
		&mut self.uiControl
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	pub fn getContent(&self) -> Option<NodeId> {
		self.uiControl.getContent()
	}

	//==============================================================================
	// 필드 id. (엔진 글 입력 줄에서 이 필드를 가리키는 값)
	//==============================================================================
	pub fn getFieldId(&self) -> u64 {
		self.fieldId
	}

	//==============================================================================
	// 플랫폼 글 입력 사용 설정. (원본 setUseDOMInput. 켜야 한글 IME, 모바일 키보드, 웹 시스템 붙여넣기가 된다. 기본 꺼짐)
	//==============================================================================
	pub fn setUseDOMInput(&mut self, useDOMInput: bool) {
		self.useDOMInput = useDOMInput;
	}

	//==============================================================================
	// 플랫폼 글 입력 사용 여부.
	//==============================================================================
	pub fn isUsingDOMInput(&self) -> bool {
		self.useDOMInput
	}

	//==============================================================================
	// 손을 뗄 때도 포커스를 줄지 설정. (웹 DOM 입력 전용. 첫 터치에 키보드를 띄우지 않는 브라우저용, 기본 꺼짐)
	//==============================================================================
	pub fn setUseTouchEndFocus(&mut self, useTouchEndFocus: bool) {
		self.useTouchEndFocus = useTouchEndFocus;
	}

	//==============================================================================
	// 손을 뗄 때도 포커스를 주는지.
	//==============================================================================
	pub fn isUsingTouchEndFocus(&self) -> bool {
		self.useTouchEndFocus
	}

	//==============================================================================
	// DOM 입력을 필드 위에서 직접 누르게 할지 설정. (웹 DOM 입력 전용. 코드로 준 포커스를 받지 않는 웹뷰용, 기본 꺼짐)
	//==============================================================================
	pub fn setUseDirectTouch(&mut self, useDirectTouch: bool) {
		self.useDirectTouch = useDirectTouch;
	}

	//==============================================================================
	// DOM 입력을 직접 누르게 하는지.
	//==============================================================================
	pub fn isUsingDirectTouch(&self) -> bool {
		self.useDirectTouch
	}

	//==============================================================================
	// 값 설정. (maxLength 로 자르고 커서는 끝에. 바뀜 이벤트는 부르지 않는다. 원본 setText)
	//==============================================================================
	pub fn setText(&mut self, text: &str) {
		self.editBuffer.setText(text);
		self.bumpValueRevision();
	}

	//==============================================================================
	// 값 반환.
	//==============================================================================
	pub fn getText(&self) -> &str {
		self.editBuffer.getText()
	}

	//==============================================================================
	// 보이는 글 반환. (값의 커서 자리에 조합 글을 끼운 것)
	//==============================================================================
	pub fn getDisplayText(&self) -> String {
		self.editBuffer.buildDisplayText()
	}

	//==============================================================================
	// 조합 중인 글 반환.
	//==============================================================================
	pub fn getComposition(&self) -> &str {
		self.editBuffer.getComposition()
	}

	//==============================================================================
	// IME 조합 중인지.
	//==============================================================================
	pub fn isComposing(&self) -> bool {
		self.isComposing
	}

	//==============================================================================
	// 안내 글 설정. (값이 비었을 때 보인다)
	//==============================================================================
	pub fn setPlaceholder(&mut self, placeholder: &str) {
		self.placeholder = placeholder.to_string();
	}

	//==============================================================================
	// 안내 글 반환.
	//==============================================================================
	pub fn getPlaceholder(&self) -> &str {
		&self.placeholder
	}

	//==============================================================================
	// 최대 글자 수 설정. (0 이면 제한 없음)
	//==============================================================================
	pub fn setMaxLength(&mut self, maxLength: usize) {
		self.editBuffer.setMaxLength(maxLength);
		self.bumpValueRevision();
	}

	//==============================================================================
	// 최대 글자 수 반환.
	//==============================================================================
	pub fn getMaxLength(&self) -> usize {
		self.editBuffer.getMaxLength()
	}

	//==============================================================================
	// 글자 크기 설정.
	//==============================================================================
	pub fn setFontSize(&mut self, fontSize: f32) {
		self.fontSize = fontSize;
		self.valueText.setFontSize(fontSize);
		self.placeholderText.setFontSize(fontSize);
	}

	//==============================================================================
	// 글자 크기 반환.
	//==============================================================================
	pub fn getFontSize(&self) -> f32 {
		self.fontSize
	}

	//==============================================================================
	// 폰트 이름 설정. (원본 setFont 자리. Graphic 에 등록한 이름, 없으면 기본 폰트)
	//==============================================================================
	pub fn setFontFamily(&mut self, fontFamily: &str) {
		self.fontFamily = fontFamily.to_string();
		self.valueText.setFontFamily(fontFamily);
		self.placeholderText.setFontFamily(fontFamily);
	}

	//==============================================================================
	// 폰트 이름 반환.
	//==============================================================================
	pub fn getFontFamily(&self) -> &str {
		&self.fontFamily
	}

	//==============================================================================
	// 안쪽 여백 설정. (글 왼쪽 여백)
	//==============================================================================
	pub fn setPadding(&mut self, padding: f32) {
		self.padding = padding;
	}

	//==============================================================================
	// 안쪽 여백 반환.
	//==============================================================================
	pub fn getPadding(&self) -> f32 {
		self.padding
	}

	//==============================================================================
	// 바탕색 설정.
	//==============================================================================
	pub fn setBackgroundColor(&mut self, color: impl Into<Color>) {
		self.backgroundPaint.setColor(color);
	}

	//==============================================================================
	// 바탕색 반환.
	//==============================================================================
	pub fn getBackgroundColor(&self) -> Color {
		self.backgroundPaint.getColor()
	}

	//==============================================================================
	// 바탕 둥글기 설정. (포커스 테두리도 따른다)
	//==============================================================================
	pub fn setRoundSize(&mut self, roundSize: f32) {
		self.backgroundPaint.setRoundSize(roundSize);
	}

	//==============================================================================
	// 바탕 둥글기 반환.
	//==============================================================================
	pub fn getRoundSize(&self) -> f32 {
		self.backgroundPaint.getRoundSize()
	}

	//==============================================================================
	// 글자 색 설정.
	//==============================================================================
	pub fn setTextColor(&mut self, color: impl Into<Color>) {
		self.valueText.setTextColor(color);
	}

	//==============================================================================
	// 글자 색 반환.
	//==============================================================================
	pub fn getTextColor(&self) -> Color {
		self.valueText.getTextColor()
	}

	//==============================================================================
	// 안내 글 색 설정.
	//==============================================================================
	pub fn setPlaceholderColor(&mut self, color: impl Into<Color>) {
		self.placeholderText.setTextColor(color);
	}

	//==============================================================================
	// 안내 글 색 반환.
	//==============================================================================
	pub fn getPlaceholderColor(&self) -> Color {
		self.placeholderText.getTextColor()
	}

	//==============================================================================
	// 커서 색 설정. (조합 밑줄도 같은 색. 원본과 같음)
	//==============================================================================
	pub fn setCursorColor(&mut self, color: impl Into<Color>) {
		let color = color.into();
		self.cursorColor = color;
		self.compositionUnderlineColor = color;
	}

	//==============================================================================
	// 커서 색 반환.
	//==============================================================================
	pub fn getCursorColor(&self) -> Color {
		self.cursorColor
	}

	//==============================================================================
	// 포커스 테두리 색 설정.
	//==============================================================================
	pub fn setFocusBorderColor(&mut self, color: impl Into<Color>) {
		self.focusBorderColor = color.into();
	}

	//==============================================================================
	// 포커스 테두리 두께 설정. (0 이면 그리지 않는다)
	//==============================================================================
	pub fn setFocusBorderWidth(&mut self, focusBorderWidth: f32) {
		self.focusBorderWidth = focusBorderWidth;
	}

	//==============================================================================
	// 선택 영역 색 설정.
	//==============================================================================
	pub fn setSelectionColor(&mut self, color: impl Into<Color>) {
		self.selectionColor = color.into();
	}

	//==============================================================================
	// 바뀜 이벤트 설정. (사용자 입력으로 값이 바뀔 때. 원본 setOnChange)
	//==============================================================================
	pub fn setOnChange(&mut self, changeEvent: UIInputFieldEvent) {
		self.changeEvent = Some(changeEvent);
	}

	//==============================================================================
	// 제출 이벤트 설정. (Enter. 원본 setOnSubmit)
	//==============================================================================
	pub fn setOnSubmit(&mut self, submitEvent: UIInputFieldEvent) {
		self.submitEvent = Some(submitEvent);
	}

	//==============================================================================
	// 입력 받기 시작. (원본 attach: DOM 입력 또는 캔버스 키보드, 문맥 메뉴)
	//==============================================================================
	pub fn attach(&mut self) {
		self.isAttached = true;
		self.bumpValueRevision();
	}

	//==============================================================================
	// 입력 받기 끝. (원본 detach. DOM 입력을 쓰던 필드는 포커스와 조합 글도 잃는다)
	//==============================================================================
	pub fn detach(&mut self) {
		self.isAttached = false;
		if self.useDOMInput {
			self.isFocused = false;
			self.focusSerial = 0;
			self.isComposing = false;
			self.editBuffer.setComposition("");
		}
	}

	//==============================================================================
	// 입력을 받기로 했는지.
	//==============================================================================
	pub fn isAttached(&self) -> bool {
		self.isAttached
	}

	//==============================================================================
	// 포커스 주기. (원본 focus. 플랫폼 입력은 다음 프레임에 이 필드로 온다)
	//==============================================================================
	pub fn focus(&mut self) {
		self.handleFocus();
	}

	//==============================================================================
	// 포커스 빼기. (원본 blur. 조합 중이던 글은 확정한다. 씬 코드에서 부르면 바뀜 이벤트는 부르지 않는다)
	//==============================================================================
	pub fn blur(&mut self) {
		self.applyBlur();
	}

	//==============================================================================
	// 포커스를 가졌는지.
	//==============================================================================
	pub fn isFocused(&self) -> bool {
		self.isFocused
	}

	//==============================================================================
	// 모두 선택.
	//==============================================================================
	pub fn selectAll(&mut self) {
		self.editBuffer.selectAll();
		self.bumpValueRevision();
	}

	//==============================================================================
	// 선택이 있는지.
	//==============================================================================
	pub fn hasSelection(&self) -> bool {
		self.editBuffer.hasSelection()
	}

	//==============================================================================
	// 선택 범위. (앞, 뒤. 없으면 None)
	//==============================================================================
	pub fn getSelectionRange(&self) -> Option<(usize, usize)> {
		self.editBuffer.getSelectionRange()
	}

	//==============================================================================
	// 선택된 글.
	//==============================================================================
	pub fn getSelectionText(&self) -> String {
		self.editBuffer.getSelectionText()
	}

	//==============================================================================
	// 커서 자리. (문자 단위)
	//==============================================================================
	pub fn getCursorIndex(&self) -> usize {
		self.editBuffer.getCursorIndex()
	}

	//==============================================================================
	// 선택 시작 자리. (문자 단위)
	//==============================================================================
	pub fn getSelectionStart(&self) -> usize {
		self.editBuffer.getSelectionStart()
	}

	//==============================================================================
	// 커서 옮기기. (원본 moveCursor)
	//==============================================================================
	pub fn moveCursor(&mut self, newIndex: usize, isSelectionExtended: bool) {
		self.editBuffer.moveCursor(newIndex, isSelectionExtended);
		self.bumpValueRevision();
	}

	//==============================================================================
	// 선택을 글로 바꾸고 바뀜 이벤트 부르기. (원본 replaceSelection)
	//==============================================================================
	pub fn replaceSelection(&mut self, context: &mut ComponentContext, text: &str) {
		self.editBuffer.replaceSelection(text);
		self.bumpValueRevision();
		self.fireChangeEvent(context);
	}

	//==============================================================================
	// 잘라내기. (선택을 클립보드에 쓰고 지운다)
	//==============================================================================
	pub fn cut(&mut self, context: &mut ComponentContext) {
		if !self.hasSelection() {
			return;
		}
		let selectionText = self.getSelectionText();
		context.getEngine().writeClipboardText(&selectionText);
		self.replaceSelection(context, "");
	}

	//==============================================================================
	// 복사. (선택을 클립보드에 쓴다)
	//==============================================================================
	pub fn copy(&self, engine: &Engine) {
		if !self.hasSelection() {
			return;
		}
		engine.writeClipboardText(&self.getSelectionText());
	}

	//==============================================================================
	// 붙여넣기. (데스크탑은 클립보드를 그 자리에서 읽는다. 웹은 플랫폼이 비동기로 읽어 글 입력 줄로 넣는다)
	//==============================================================================
	pub fn paste(&mut self, context: &mut ComponentContext) {
		let clipboardText = context.getEngine().readClipboardText();
		if let Some(clipboardText) = clipboardText {
			if !clipboardText.is_empty() {
				self.replaceSelection(context, &clipboardText);
			}
		}
	}

	//==============================================================================
	// 문맥 메뉴 항목 부탁. (문맥 메뉴가 부른다. 다음 tick 에 이 필드의 문맥으로 한다)
	//==============================================================================
	pub fn requestMenuAction(&mut self, menuAction: UIInputFieldMenuAction) {
		self.pendingMenuAction = Some(menuAction);
	}

	//==============================================================================
	// 필드 쪽에서 값이나 커서를 바꿨다고 표시. (웹이 숨긴 <input> 에 다시 쓴다)
	//==============================================================================
	fn bumpValueRevision(&mut self) {
		self.valueRevision = self.valueRevision.wrapping_add(1);
	}

	//==============================================================================
	// 커서 깜빡임 처음으로.
	//==============================================================================
	fn resetCursorBlink(&mut self) {
		self.blinkTimer = 0.0;
		self.isCursorVisible = true;
	}

	//==============================================================================
	// 값과 선택이 웹 DOM 을 따라가는지. (웹에서 attach 한 DOM 입력 필드)
	//==============================================================================
	fn isDomSynchronized(&self) -> bool {
		cfg!(target_arch = "wasm32") && self.useDOMInput && self.isAttached
	}

	//==============================================================================
	// 포커스 얻음. (원본 handleFocus)
	//==============================================================================
	fn handleFocus(&mut self) {
		self.isFocused = true;
		self.focusSerial = NEXT_FOCUS_SERIAL.fetch_add(1, Ordering::Relaxed);
		self.resetCursorBlink();
	}

	//==============================================================================
	// 포커스 잃음. (조합 글을 확정했으면 그 글을 돌려준다. 원본 handleBlur)
	// - 웹 DOM 입력은 브라우저가 확정해 값으로 보내므로 조합 글만 비운다.
	//==============================================================================
	fn applyBlur(&mut self) -> Option<String> {
		self.isFocused = false;
		self.focusSerial = 0;
		self.dragStartIndex = None;
		let composition = self.editBuffer.getComposition().to_string();
		self.editBuffer.setComposition("");
		self.isComposing = false;
		if composition.is_empty() || self.isDomSynchronized() {
			return None;
		}
		self.editBuffer.replaceSelection(&composition);
		self.bumpValueRevision();
		Some(composition)
	}

	//==============================================================================
	// 포커스 잃음. (문맥이 있을 때: 확정한 조합 글은 OS 가 다시 보내도 버리게 하고, 바뀜 이벤트를 부르고, 문맥 메뉴를 닫는다)
	//==============================================================================
	fn handleBlur(&mut self, context: &mut ComponentContext) {
		let committedText = self.applyBlur();
		self.hideContextMenu(context);
		if let Some(committedText) = committedText {
			context.getEngine().getInputManager().getTextInput().setImeCommitGuard(&committedText);
			self.fireChangeEvent(context);
		}
	}

	//==============================================================================
	// 바뀜 이벤트 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fireChangeEvent(&mut self, context: &mut ComponentContext) {
		let mut changeEvent = match self.changeEvent.take() {
			Some(changeEvent) => {
				changeEvent
			}
			None => {
				return;
			}
		};
		changeEvent(self, context);
		if self.changeEvent.is_none() {
			self.changeEvent = Some(changeEvent);
		}
	}

	//==============================================================================
	// 제출 이벤트 부르기.
	//==============================================================================
	fn fireSubmitEvent(&mut self, context: &mut ComponentContext) {
		let mut submitEvent = match self.submitEvent.take() {
			Some(submitEvent) => {
				submitEvent
			}
			None => {
				return;
			}
		};
		submitEvent(self, context);
		if self.submitEvent.is_none() {
			self.submitEvent = Some(submitEvent);
		}
	}

	//==============================================================================
	// 노드 x(content 영역 기준) → 글자 자리. (draw 때 재 둔 글자 경계로 찾는다. 아직 재지 않았으면 글 끝)
	//==============================================================================
	pub fn measureIndexAtLocalX(&self, localX: f32) -> usize {
		let value = self.editBuffer.getText();
		if value.is_empty() {
			return 0;
		}
		if self.measuredValue != value || self.characterOffsets.is_empty() {
			return countCharacters(value);
		}
		findCharacterIndexAtOffset(&self.characterOffsets, localX - self.padding)
	}

	//==============================================================================
	// 뷰 좌표 → 이 노드의 content 영역 x.
	//==============================================================================
	fn convertViewPositionToLocalX(&self, context: &ComponentContext, viewInputPosition: Vector2) -> f32 {
		let nodeId = context.getNodeId();
		match context.getNodeTree().convertViewPositionToNodePosition(nodeId, viewInputPosition) {
			Some(localPosition) => {
				localPosition.x
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	// 글 입력 이벤트 하나 처리.
	//==============================================================================
	fn handleTextInputEvent(&mut self, context: &mut ComponentContext, eventKind: TextInputEventKind) {
		match eventKind {
			TextInputEventKind::commit(text) => {
				if !text.is_empty() {
					self.replaceSelection(context, &text);
				}
				self.resetCursorBlink();
			}
			TextInputEventKind::compositionStart => {
				self.isComposing = true;
			}
			TextInputEventKind::preedit { text, .. } => {
				self.editBuffer.setComposition(&text);
				self.resetCursorBlink();
			}
			TextInputEventKind::compositionEnd => {
				self.isComposing = false;
				self.editBuffer.setComposition("");
			}
			TextInputEventKind::key { key, isShift } => {
				self.handleEditKey(context, key, isShift);
			}
			TextInputEventKind::paste(text) => {
				if !text.is_empty() {
					self.replaceSelection(context, &text);
				}
				self.resetCursorBlink();
			}
			TextInputEventKind::replaceValue { value, selectionEnd, .. } => {
				// 원본 handleDOMInputEvent: 조합 중에는 무시하고, 같은 값이면 무시한다.
				if self.isComposing || value == self.editBuffer.getText() {
					return;
				}
				self.editBuffer.setValueFromPlatform(&value, selectionEnd);
				self.resetCursorBlink();
				self.fireChangeEvent(context);
			}
			TextInputEventKind::selection { start, end } => {
				// 원본 tick 의 selection 폴링: 포커스, 조합 아님, 끌기 아님일 때 범위가 다르면 따른다.
				if !self.isFocused || self.isComposing || self.dragStartIndex.is_some() {
					return;
				}
				let currentRange = self.editBuffer.getSelectionRange().unwrap_or((self.editBuffer.getCursorIndex(), self.editBuffer.getCursorIndex()));
				if currentRange != (start, end) {
					self.editBuffer.setSelection(start, end);
					self.resetCursorBlink();
				}
			}
			TextInputEventKind::focusGained => {
				self.handleFocus();
			}
			TextInputEventKind::focusLost => {
				self.handleBlur(context);
			}
		}
	}

	//==============================================================================
	// 편집 키 처리. (원본 handleCanvasKeydown)
	//==============================================================================
	fn handleEditKey(&mut self, context: &mut ComponentContext, key: TextInputKey, isShift: bool) {
		let cursorIndex = self.editBuffer.getCursorIndex();
		match key {
			TextInputKey::selectAll => {
				self.selectAll();
			}
			TextInputKey::copy => {
				self.copy(context.getEngine());
			}
			TextInputKey::cut => {
				self.cut(context);
			}
			TextInputKey::paste => {
				self.paste(context);
			}
			TextInputKey::backspace => {
				if self.editBuffer.deleteBackward() {
					self.bumpValueRevision();
					self.fireChangeEvent(context);
				}
			}
			TextInputKey::delete => {
				if self.editBuffer.deleteForward() {
					self.bumpValueRevision();
					self.fireChangeEvent(context);
				}
			}
			TextInputKey::left => {
				self.moveCursor(cursorIndex.saturating_sub(1), isShift);
			}
			TextInputKey::right => {
				self.moveCursor(cursorIndex + 1, isShift);
			}
			TextInputKey::home => {
				self.moveCursor(0, isShift);
			}
			TextInputKey::end => {
				let characterCount = countCharacters(self.editBuffer.getText());
				self.moveCursor(characterCount, isShift);
			}
			TextInputKey::enter => {
				self.fireSubmitEvent(context);
			}
			TextInputKey::tab => {
			}
		}
		self.resetCursorBlink();
	}

	//==============================================================================
	// 문맥 메뉴 항목 하기.
	//==============================================================================
	fn performMenuAction(&mut self, context: &mut ComponentContext, menuAction: UIInputFieldMenuAction) {
		match menuAction {
			UIInputFieldMenuAction::cut => {
				self.cut(context);
			}
			UIInputFieldMenuAction::copy => {
				self.copy(context.getEngine());
			}
			UIInputFieldMenuAction::paste => {
				self.paste(context);
			}
		}
	}

	//==============================================================================
	// 문맥 메뉴 열기. (루트의 마지막 자식 노드로 만들어 위에 그린다. 원본 showContextMenu)
	//==============================================================================
	fn showContextMenu(&mut self, context: &mut ComponentContext, viewPosition: Vector2) {
		self.hideContextMenu(context);
		let ownerNodeId = context.getNodeId();
		let isSelectionAvailable = self.hasSelection();
		let nodeTree = context.getNodeTreeMut();
		let rootId = nodeTree.getRootId();
		let localPosition = nodeTree.convertViewPositionToNodePosition(rootId, viewPosition).unwrap_or(viewPosition);
		let menuId = nodeTree.createChild(rootId, "uiInputFieldContextMenu");
		let menuNode = &mut nodeTree[menuId];
		menuNode.setPivot(Pivot::topLeft);
		menuNode.setAnchor(Vector2::zero());
		menuNode.setLocalPosition(localPosition);
		menuNode.setContentSize(Vector2::create(CONTEXT_MENU_WIDTH, CONTEXT_MENU_PADDING_Y * 2.0 + CONTEXT_MENU_ITEM_HEIGHT * CONTEXT_MENU_ITEM_LIST.len() as f32));
		nodeTree.addComponent(menuId, UIInputFieldContextMenu::new(ownerNodeId, isSelectionAvailable));
		self.contextMenuNodeId = Some(menuId);
	}

	//==============================================================================
	// 문맥 메뉴 닫기. (원본 hideContextMenu)
	//==============================================================================
	fn hideContextMenu(&mut self, context: &mut ComponentContext) {
		if let Some(contextMenuNodeId) = self.contextMenuNodeId.take() {
			context.getNodeTreeMut().destroyNode(contextMenuNodeId);
		}
	}

	//==============================================================================
	// 필드 밖을 눌렀으면 포커스를 뺀다. (원본 DOM 모드의 캔버스 누름 처리. 문맥 메뉴 위는 밖으로 치지 않는다)
	//==============================================================================
	fn blurOnOutsidePress(&mut self, context: &mut ComponentContext) {
		if !self.isFocused || !self.useDOMInput {
			return;
		}
		let inputManager = context.getEngine().getInputManager();
		if !inputManager.isTouchPressed() {
			return;
		}
		let viewInputPosition = inputManager.getViewInputPosition();
		let nodeId = context.getNodeId();
		let nodeTree = context.getNodeTree();
		let isInside = isNodeVisibleInHierarchy(nodeTree, nodeId) && isViewPositionInsideNode(nodeTree, nodeId, viewInputPosition);
		let isInsideMenu = match self.contextMenuNodeId {
			Some(contextMenuNodeId) => {
				isViewPositionInsideNode(nodeTree, contextMenuNodeId, viewInputPosition)
			}
			None => {
				false
			}
		};
		if !isInside && !isInsideMenu {
			self.handleBlur(context);
		}
	}

	//==============================================================================
	// 플랫폼에 올릴 상태 만들기. (자리는 창 안쪽 논리 좌표)
	//==============================================================================
	fn buildFieldState(&self, context: &ComponentContext) -> TextInputFieldState {
		let nodeId = context.getNodeId();
		let nodeTree = context.getNodeTree();
		let viewManager = context.getEngine().getViewManager();
		let contentSize = context.getTransform().getContentSize();
		let worldMatrix = nodeTree.calculateWorldMatrix(nodeId);
		let viewNativeRect = viewManager.getViewNativeRect();
		let targetResolutionScale = viewManager.getTargetResolutionScale();
		let convertLocalRect = |localRect: Rect| -> Rect {
			let cornerList = [
				Vector2::create(localRect.position.x, localRect.position.y),
				Vector2::create(localRect.position.x + localRect.size.x, localRect.position.y),
				Vector2::create(localRect.position.x, localRect.position.y + localRect.size.y),
				Vector2::create(localRect.position.x + localRect.size.x, localRect.position.y + localRect.size.y),
			];
			let mut minimumPosition = Vector2::create(f32::MAX, f32::MAX);
			let mut maximumPosition = Vector2::create(f32::MIN, f32::MIN);
			for corner in cornerList.iter() {
				let viewPosition = worldMatrix.transformPoint(corner.x, corner.y);
				let canvasX = viewNativeRect.position.x + viewPosition.x * targetResolutionScale;
				let canvasY = viewNativeRect.position.y + viewPosition.y * targetResolutionScale;
				minimumPosition = Vector2::create(minimumPosition.x.min(canvasX), minimumPosition.y.min(canvasY));
				maximumPosition = Vector2::create(maximumPosition.x.max(canvasX), maximumPosition.y.max(canvasY));
			}
			Rect::create(minimumPosition.x, minimumPosition.y, maximumPosition.x - minimumPosition.x, maximumPosition.y - minimumPosition.y)
		};
		let fieldRect = convertLocalRect(Rect::create(0.0, 0.0, contentSize.x, contentSize.y));
		let caretRect = convertLocalRect(Rect::create(self.caretLocalX, 0.0, 1.0, contentSize.y));
		TextInputFieldState {
			fieldId: self.fieldId,
			focusSerial: self.focusSerial,
			isFocused: self.isFocused,
			isAttached: self.isAttached,
			useDOMInput: self.useDOMInput,
			useTouchEndFocus: self.useTouchEndFocus,
			useDirectTouch: self.useDirectTouch,
			isVisible: isNodeVisibleInHierarchy(nodeTree, nodeId),
			fieldRect,
			caretRect,
			value: self.editBuffer.getText().to_string(),
			selectionStart: self.editBuffer.getSelectionStart(),
			cursorIndex: self.editBuffer.getCursorIndex(),
			maxLength: self.editBuffer.getMaxLength(),
			valueRevision: self.valueRevision,
		}
	}

	//==============================================================================
	// Canvas2D font 꼴 문자열.
	//==============================================================================
	fn buildFontString(&self) -> String {
		format!("{}px {}", self.fontSize, self.fontFamily)
	}

	//==============================================================================
	// 값의 글자 경계 다시 재기. (값이나 폰트가 바뀌었을 때만. graphic 의 폰트 문자열이 걸린 상태에서 부른다)
	//==============================================================================
	fn updateCharacterOffsets(&mut self, graphic: &Graphic, fontString: &str) {
		let value = self.editBuffer.getText();
		if self.measuredValue == value && self.measuredFontString == fontString && !self.characterOffsets.is_empty() {
			return;
		}
		let mut characterOffsets: Vec<f32> = Vec::with_capacity(value.len() + 1);
		characterOffsets.push(0.0);
		for (byteIndex, character) in value.char_indices() {
			let prefixEnd = byteIndex + character.len_utf8();
			characterOffsets.push(graphic.measureText(&value[..prefixEnd]).width);
		}
		self.characterOffsets = characterOffsets;
		self.measuredValue = value.to_string();
		self.measuredFontString = fontString.to_string();
	}

	//==============================================================================
	// 글자 경계 x. (재 둔 것이 없으면 0)
	//==============================================================================
	fn getCharacterOffset(&self, characterIndex: usize) -> f32 {
		match self.characterOffsets.get(characterIndex) {
			Some(characterOffset) => {
				*characterOffset
			}
			None => {
				self.characterOffsets.last().copied().unwrap_or(0.0)
			}
		}
	}
}

impl Default for UIInputField {
	fn default() -> UIInputField {
		UIInputField::new()
	}
}

impl Component for UIInputField {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiControl.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiControl.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"UIInputField"
	}

	fn attach(&mut self, transform: &mut NodeTransform) {
		transform.setPivot(Pivot::topLeft);
		transform.setAnchor(Pivot::topLeft);
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiControl.getUIViewMut().createContentNode(nodeTree, nodeId);
		nodeTree[nodeId].setInteractable(true);
	}

	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		// 문맥 메뉴에서 고른 항목.
		if let Some(menuAction) = self.pendingMenuAction.take() {
			self.performMenuAction(context, menuAction);
		}
		if let Some(contextMenuNodeId) = self.contextMenuNodeId {
			if !context.getNodeTree().contains(contextMenuNodeId) {
				self.contextMenuNodeId = None;
			}
		}

		// 다른 필드가 나중에 포커스를 얻었으면 포커스를 내준다. (엔진에 포커스는 하나)
		let latestFocusSerial = context.getEngine().getInputManager().getTextInput().getLatestFocusSerial();
		if self.isFocused && latestFocusSerial > self.focusSerial {
			self.handleBlur(context);
		}
		self.blurOnOutsidePress(context);

		// 문맥 메뉴 요청. (데스크탑 오른쪽 단추. 그 자리 맨 위가 이 필드일 때)
		let contextMenuRequestPosition = context.getEngine().getInputManager().getTextInput().getContextMenuRequestPosition();
		if let Some(contextMenuRequestPosition) = contextMenuRequestPosition {
			let nodeId = context.getNodeId();
			let topmostNodeId = context.getNodeTree().findTopmostInteractableNode(contextMenuRequestPosition);
			if self.isAttached && topmostNodeId == Some(nodeId) {
				self.handleFocus();
				self.showContextMenu(context, contextMenuRequestPosition);
			}
		}

		// 글 입력 줄에서 이 필드 몫 꺼내기.
		let acceptsFocusedFieldEvents = self.isFocused && self.isAttached;
		let eventKindList = context.getEngine().getInputManager().getTextInput().takeEventsForField(self.fieldId, acceptsFocusedFieldEvents);
		for eventKind in eventKindList {
			self.handleTextInputEvent(context, eventKind);
		}

		// 커서 깜빡임.
		if self.isFocused {
			self.blinkTimer += timeDelta;
			if self.blinkTimer >= CURSOR_BLINK_SECONDS {
				self.blinkTimer = 0.0;
				self.isCursorVisible = !self.isCursorVisible;
			}
		}
		else {
			self.isCursorVisible = true;
		}

		// 플랫폼에 올릴 상태.
		if self.isAttached || self.isFocused {
			let fieldState = self.buildFieldState(context);
			context.getEngine().getInputManager().getTextInput().registerFieldState(fieldState);
		}
	}

	fn touchPress(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		// 조합 중에 다른 자리를 누르면 조합을 확정하고 IME 를 끊는다. (웹 DOM 입력은 브라우저가 한다)
		if !self.editBuffer.getComposition().is_empty() && !self.isDomSynchronized() {
			let composition = self.editBuffer.getComposition().to_string();
			self.editBuffer.setComposition("");
			self.isComposing = false;
			let textInput = context.getEngine().getInputManager().getTextInput();
			textInput.setImeCommitGuard(&composition);
			textInput.requestImeReset();
			self.replaceSelection(context, &composition);
		}
		self.focus();
		self.hideContextMenu(context);
		let localX = self.convertViewPositionToLocalX(context, viewInputPosition);
		let characterIndex = self.measureIndexAtLocalX(localX);
		self.dragStartIndex = Some(characterIndex);
		self.editBuffer.setSelection(characterIndex, characterIndex);
		self.bumpValueRevision();
		self.resetCursorBlink();
	}

	fn touchMove(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		let dragStartIndex = match self.dragStartIndex {
			Some(dragStartIndex) => {
				dragStartIndex
			}
			None => {
				return;
			}
		};
		let localX = self.convertViewPositionToLocalX(context, viewInputPosition);
		let characterIndex = self.measureIndexAtLocalX(localX);
		self.editBuffer.setSelection(dragStartIndex, characterIndex);
		self.bumpValueRevision();
		self.resetCursorBlink();
	}

	fn touchRelease(&mut self, _context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.dragStartIndex = None;
	}

	fn touchCancel(&mut self, _context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.dragStartIndex = None;
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		// 1) 바탕.
		self.backgroundPaint.drawPaint(transform, graphic);
		let contentSize = transform.getContentSize();
		if contentSize.x <= 0.0 || contentSize.y <= 0.0 {
			return;
		}
		let fontString = self.buildFontString();
		let drawY = contentSize.y * 0.5;
		let padding = self.padding;
		let fontSize = self.fontSize;

		// 2) 선택 영역. (조합 중엔 그리지 않는다)
		graphic.pushState();
		graphic.setFontString(&fontString);
		self.updateCharacterOffsets(graphic, &fontString);
		if self.editBuffer.getComposition().is_empty() {
			if let Some((startIndex, endIndex)) = self.editBuffer.getSelectionRange() {
				let startX = padding + self.getCharacterOffset(startIndex);
				let endX = padding + self.getCharacterOffset(endIndex);
				let halfHeight = fontSize * 0.65;
				graphic.setFillColor(self.selectionColor);
				graphic.drawRect(Rect::create(startX, drawY - halfHeight, endX - startX, halfHeight * 2.0));
			}
		}
		graphic.popState();

		// 3) 글. (값이나 조합 글이 있으면 값, 없으면 안내 글. 원본 라벨 노드처럼 여백만큼 안쪽 영역에 왼쪽 정렬)
		let innerSize = Vector2::create((contentSize.x - padding * 2.0).max(0.0), contentSize.y);
		let mut innerTransform = NodeTransform::new();
		innerTransform.setContentSize(innerSize);
		let hasValue = !self.editBuffer.getText().is_empty() || !self.editBuffer.getComposition().is_empty();
		graphic.pushState();
		graphic.translate(padding, 0.0);
		if hasValue {
			let displayText = self.editBuffer.buildDisplayText();
			self.valueText.setText(&displayText);
			self.valueText.draw(&innerTransform, graphic);
		}
		else {
			self.placeholderText.setText(&self.placeholder);
			self.placeholderText.draw(&innerTransform, graphic);
		}
		graphic.popState();

		// 4) 조합 밑줄, 커서, 포커스 테두리.
		graphic.pushState();
		graphic.setFontString(&fontString);
		let cursorIndex = self.editBuffer.getCursorIndex();
		let beforeWidth = self.getCharacterOffset(cursorIndex);
		let composition = self.editBuffer.getComposition().to_string();
		let cursorX = if composition.is_empty() {
			padding + beforeWidth
		}
		else {
			let compositionWidth = graphic.measureText(&composition).width;
			let underlineY = drawY + fontSize * 0.55;
			graphic.setStrokeColor(self.compositionUnderlineColor);
			graphic.drawLine(&[Vector2::create(padding + beforeWidth, underlineY), Vector2::create(padding + beforeWidth + compositionWidth, underlineY)], 2.0);
			let cursorBeforeText = format!("{}{}", self.editBuffer.getTextBeforeCursor(), composition);
			padding + graphic.measureText(&cursorBeforeText).width
		};
		self.caretLocalX = cursorX;
		if self.isFocused && self.isCursorVisible {
			let halfHeight = fontSize * 0.6;
			graphic.setStrokeColor(self.cursorColor);
			graphic.drawLine(&[Vector2::create(cursorX, drawY - halfHeight), Vector2::create(cursorX, drawY + halfHeight)], 2.0);
		}
		if self.isFocused && self.focusBorderWidth > 0.0 {
			let borderWidth = self.focusBorderWidth;
			let inset = borderWidth * 0.5;
			let roundSize = (self.backgroundPaint.getRoundSize() - inset).max(0.0);
			graphic.setStrokeColor(self.focusBorderColor);
			graphic.drawStrokeRoundRect(Rect::create(inset, inset, contentSize.x - borderWidth, contentSize.y - borderWidth), roundSize, borderWidth);
		}
		graphic.popState();
	}
}

//==============================================================================
// 노드와 조상이 모두 보이는지. (원본 isVisibleInHierarchy)
//==============================================================================
fn isNodeVisibleInHierarchy(nodeTree: &NodeTree, nodeId: NodeId) -> bool {
	let mut currentId = Some(nodeId);
	while let Some(candidateId) = currentId {
		match nodeTree.getNode(candidateId) {
			Some(node) => {
				if !node.isVisible() || !node.isActive() {
					return false;
				}
			}
			None => {
				return false;
			}
		}
		currentId = nodeTree.getParentId(candidateId);
	}
	true
}

// 문맥 메뉴 항목. (글, 항목)
const CONTEXT_MENU_ITEM_LIST: [(&str, UIInputFieldMenuAction); 3] = [
	("잘라내기", UIInputFieldMenuAction::cut),
	("복사", UIInputFieldMenuAction::copy),
	("붙여넣기", UIInputFieldMenuAction::paste),
];

//==============================================================================
// 캔버스 문맥 메뉴. (데스크탑. 원본 showContextMenu 의 DOM 메뉴를 같은 모양으로 그린다)
// - 루트의 자식 노드에 붙는다. 항목을 누르면 주인 필드에 항목을 부탁하고(다음 tick 에 한다) 스스로 지운다.
// - 메뉴 밖을 누르거나, 다른 자리에 문맥 메뉴를 부르거나, 주인 필드가 포커스를 잃거나 없어지면 스스로 지운다.
//==============================================================================
struct UIInputFieldContextMenu {
	componentBase: ComponentBase,
	ownerNodeId: NodeId,
	isSelectionAvailable: bool,
	hoveredItemIndex: Option<usize>,
	isFirstTick: bool,
}

impl UIInputFieldContextMenu {
	//==============================================================================
	// 생성. (선택이 없으면 잘라내기와 복사는 흐리게)
	//==============================================================================
	fn new(ownerNodeId: NodeId, isSelectionAvailable: bool) -> UIInputFieldContextMenu {
		UIInputFieldContextMenu {
			componentBase: ComponentBase::new(),
			ownerNodeId,
			isSelectionAvailable,
			hoveredItemIndex: None,
			isFirstTick: true,
		}
	}

	//==============================================================================
	// 항목을 쓸 수 있는지.
	//==============================================================================
	fn isItemEnabled(&self, menuAction: UIInputFieldMenuAction) -> bool {
		match menuAction {
			UIInputFieldMenuAction::cut | UIInputFieldMenuAction::copy => {
				self.isSelectionAvailable
			}
			UIInputFieldMenuAction::paste => {
				true
			}
		}
	}

	//==============================================================================
	// 뷰 좌표 아래 항목 자리. (없으면 None)
	//==============================================================================
	fn findItemIndex(&self, context: &ComponentContext, viewPosition: Vector2) -> Option<usize> {
		let nodeId = context.getNodeId();
		let localPosition = context.getNodeTree().convertViewPositionToNodePosition(nodeId, viewPosition)?;
		if localPosition.x < 0.0 || localPosition.x > CONTEXT_MENU_WIDTH {
			return None;
		}
		let itemY = localPosition.y - CONTEXT_MENU_PADDING_Y;
		if itemY < 0.0 {
			return None;
		}
		let itemIndex = (itemY / CONTEXT_MENU_ITEM_HEIGHT) as usize;
		if itemIndex >= CONTEXT_MENU_ITEM_LIST.len() {
			return None;
		}
		Some(itemIndex)
	}

	//==============================================================================
	// 메뉴 닫기. (자기 노드를 지운다)
	//==============================================================================
	fn close(&self, context: &mut ComponentContext) {
		let nodeId = context.getNodeId();
		context.getNodeTreeMut().destroyNode(nodeId);
	}
}

impl Component for UIInputFieldContextMenu {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UIInputFieldContextMenu"
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		nodeTree[nodeId].setInteractable(true);
	}

	fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
		let isOwnerReady = match context.getNodeTree().getNode(self.ownerNodeId).and_then(|ownerNode| ownerNode.getComponent::<UIInputField>()) {
			Some(ownerField) => {
				ownerField.isFocused() && ownerField.isAttached()
			}
			None => {
				false
			}
		};
		if !isOwnerReady {
			self.close(context);
			return;
		}
		let inputManager = context.getEngine().getInputManager();
		let viewInputPosition = inputManager.getViewInputPosition();
		let isTouchPressed = inputManager.isTouchPressed();
		let contextMenuRequestPosition = inputManager.getTextInput().getContextMenuRequestPosition();
		let nodeId = context.getNodeId();
		self.hoveredItemIndex = self.findItemIndex(context, viewInputPosition);
		if !self.isFirstTick {
			let isPressedOutside = isTouchPressed && !isViewPositionInsideNode(context.getNodeTree(), nodeId, viewInputPosition);
			let isRequestedOutside = match contextMenuRequestPosition {
				Some(requestPosition) => {
					!isViewPositionInsideNode(context.getNodeTree(), nodeId, requestPosition)
				}
				None => {
					false
				}
			};
			if isPressedOutside || isRequestedOutside {
				self.close(context);
				return;
			}
		}
		self.isFirstTick = false;
	}

	fn touchPress(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		let itemIndex = match self.findItemIndex(context, viewInputPosition) {
			Some(itemIndex) => {
				itemIndex
			}
			None => {
				return;
			}
		};
		let menuAction = CONTEXT_MENU_ITEM_LIST[itemIndex].1;
		if !self.isItemEnabled(menuAction) {
			return;
		}
		let ownerNodeId = self.ownerNodeId;
		if let Some(ownerField) = context.getNodeTreeMut().getNodeMut(ownerNodeId).and_then(|ownerNode| ownerNode.getComponentMut::<UIInputField>()) {
			ownerField.requestMenuAction(menuAction);
		}
		self.close(context);
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let contentSize = transform.getContentSize();
		let menuRect = Rect::create(0.0, 0.0, contentSize.x, contentSize.y);
		graphic.pushState();
		graphic.setFillColor(Color::create(0.0, 0.0, 0.0, 0.12));
		graphic.drawRoundRect(Rect::create(0.0, 4.0, contentSize.x, contentSize.y), 8.0);
		graphic.setFillColor("#ffffff");
		graphic.drawRoundRect(menuRect, 8.0);
		graphic.setStrokeColor("#cccccc");
		graphic.drawStrokeRoundRect(Rect::create(0.5, 0.5, contentSize.x - 1.0, contentSize.y - 1.0), 7.5, 1.0);
		graphic.setFontString(CONTEXT_MENU_FONT_STRING);
		graphic.setTextAlign(TextAlign::left);
		graphic.setTextBaseline(TextBaseline::middle);
		for (itemIndex, (itemText, menuAction)) in CONTEXT_MENU_ITEM_LIST.iter().enumerate() {
			let itemTop = CONTEXT_MENU_PADDING_Y + itemIndex as f32 * CONTEXT_MENU_ITEM_HEIGHT;
			let isEnabled = self.isItemEnabled(*menuAction);
			if isEnabled && self.hoveredItemIndex == Some(itemIndex) {
				graphic.setFillColor("#eef2ff");
				graphic.drawRect(Rect::create(1.0, itemTop, contentSize.x - 2.0, CONTEXT_MENU_ITEM_HEIGHT));
			}
			let textColor = if isEnabled {
				"#222222"
			}
			else {
				"#aaaaaa"
			};
			graphic.setFillColor(textColor);
			graphic.drawFillText(itemText, CONTEXT_MENU_ITEM_PADDING_X, itemTop + CONTEXT_MENU_ITEM_HEIGHT * 0.5);
		}
		graphic.popState();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use crate::core::textinput::TextInputEvent;
	use std::cell::{Cell, RefCell};
	use std::rc::Rc;

	//==============================================================================
	// 시험 장면. (왼쪽 위 (100, 100), 크기 300 x 60 인 필드, DOM 입력 + attach)
	//==============================================================================
	fn createFieldTree() -> (NodeTree, NodeId) {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let fieldId = nodeTree.createChild(rootId, "field");
		nodeTree[fieldId].setLocalPosition(Vector2::create(100.0, 100.0));
		nodeTree[fieldId].setContentSize(Vector2::create(300.0, 60.0));
		let field = nodeTree.addComponent(fieldId, UIInputField::new()).expect("field");
		field.setUseDOMInput(true);
		field.attach();
		(nodeTree, fieldId)
	}

	//==============================================================================
	// 필드 반환.
	//==============================================================================
	fn getField(nodeTree: &mut NodeTree, fieldId: NodeId) -> &mut UIInputField {
		nodeTree[fieldId].getComponentMut::<UIInputField>().expect("field")
	}

	//==============================================================================
	// 포커스를 가진 필드에 갈 이벤트 넣기.
	//==============================================================================
	fn pushEvent(engine: &mut Engine, eventKind: TextInputEventKind) {
		engine.getInputManagerMut().getTextInputMut().pushFocusedFieldEvent(eventKind);
	}

	//==============================================================================
	// 키 이벤트.
	//==============================================================================
	fn keyEvent(key: TextInputKey, isShift: bool) -> TextInputEventKind {
		TextInputEventKind::key {
			key,
			isShift,
		}
	}

	//==============================================================================
	// 한 프레임. (트리 tick 다음 엔진 프레임 끝처럼 글 입력 줄 마무리)
	//==============================================================================
	fn runFrame(nodeTree: &mut NodeTree, engine: &mut Engine) {
		nodeTree.tick(engine, 1.0 / 60.0);
		engine.getInputManagerMut().getTextInputMut().finishFrame();
	}

	//==============================================================================
	// 편집 버퍼: 끼우기, 지우기, 선택, 이동, 최대 길이, 한글 자리.
	//==============================================================================
	#[test]
	fn editBufferOperations() {
		let mut editBuffer = TextEditBuffer::new();
		editBuffer.replaceSelection("안녕");
		editBuffer.replaceSelection("하세요");
		assert_eq!(editBuffer.getText(), "안녕하세요");
		assert_eq!(editBuffer.getCursorIndex(), 5);
		assert!(editBuffer.deleteBackward());
		assert_eq!(editBuffer.getText(), "안녕하세");
		editBuffer.moveCursor(1, false);
		assert!(editBuffer.deleteForward());
		assert_eq!(editBuffer.getText(), "안하세");
		assert_eq!(editBuffer.getCursorIndex(), 1);
		editBuffer.moveCursor(3, true);
		assert_eq!(editBuffer.getSelectionRange(), Some((1, 3)));
		assert_eq!(editBuffer.getSelectionText(), "하세");
		editBuffer.replaceSelection("x");
		assert_eq!(editBuffer.getText(), "안x");
		assert_eq!(editBuffer.getCursorIndex(), 2);
		editBuffer.moveCursor(0, false);
		assert!(!editBuffer.deleteBackward());
		editBuffer.moveCursor(99, false);
		assert_eq!(editBuffer.getCursorIndex(), 2);
		assert!(!editBuffer.deleteForward());
		editBuffer.selectAll();
		assert_eq!(editBuffer.getSelectionText(), "안x");
		assert!(editBuffer.deleteBackward());
		assert_eq!(editBuffer.getText(), "");

		// 최대 길이: setText 와 끼우기 모두 자르고, 커서는 글 끝을 넘지 않는다.
		editBuffer.setMaxLength(4);
		editBuffer.setText("가나다라마바");
		assert_eq!(editBuffer.getText(), "가나다라");
		editBuffer.moveCursor(2, false);
		editBuffer.replaceSelection("XYZ");
		assert_eq!(editBuffer.getText(), "가나XY");
		assert_eq!(editBuffer.getCursorIndex(), 4);

		// 보이는 글: 커서 자리에 조합 글.
		editBuffer.moveCursor(1, false);
		editBuffer.setComposition("한");
		assert_eq!(editBuffer.buildDisplayText(), "가한나XY");
		assert_eq!(editBuffer.getTextBeforeCursor(), "가");
	}

	//==============================================================================
	// 누른 x → 글자 자리. (가까운 경계)
	//==============================================================================
	#[test]
	fn characterIndexAtOffset() {
		let characterOffsets = [0.0, 10.0, 20.0, 30.0];
		assert_eq!(findCharacterIndexAtOffset(&characterOffsets, -5.0), 0);
		assert_eq!(findCharacterIndexAtOffset(&characterOffsets, 4.0), 0);
		assert_eq!(findCharacterIndexAtOffset(&characterOffsets, 6.0), 1);
		assert_eq!(findCharacterIndexAtOffset(&characterOffsets, 24.0), 2);
		assert_eq!(findCharacterIndexAtOffset(&characterOffsets, 99.0), 3);
		assert_eq!(findCharacterIndexAtOffset(&[0.0], 50.0), 0);
	}

	//==============================================================================
	// 붙이면 피봇이 왼쪽 위, 노드가 interactable 이 되고 content 가 생긴다.
	//==============================================================================
	#[test]
	fn attachToTreeSetsUpNode() {
		let (mut nodeTree, fieldId) = createFieldTree();
		assert!(nodeTree[fieldId].isInteractable());
		assert_eq!(nodeTree[fieldId].getPivot(), Pivot::topLeft);
		let contentId = getField(&mut nodeTree, fieldId).getContent().expect("content");
		assert_eq!(nodeTree.getParentId(contentId), Some(fieldId));
	}

	//==============================================================================
	// 누르면 포커스, 확정 글과 편집 키가 들어가고 바뀜 / 제출 이벤트가 불린다.
	//==============================================================================
	#[test]
	fn typingAndEditKeys() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, fieldId) = createFieldTree();
		let changeCount = Rc::new(Cell::new(0));
		let submittedText = Rc::new(RefCell::new(String::new()));
		{
			let field = getField(&mut nodeTree, fieldId);
			let changeCountForEvent = changeCount.clone();
			field.setOnChange(Box::new(move |_field: &mut UIInputField, _context: &mut ComponentContext| {
				changeCountForEvent.set(changeCountForEvent.get() + 1);
			}));
			let submittedTextForEvent = submittedText.clone();
			field.setOnSubmit(Box::new(move |field: &mut UIInputField, _context: &mut ComponentContext| {
				*submittedTextForEvent.borrow_mut() = field.getText().to_string();
			}));
		}
		nodeTree.dispatchTouch(fieldId, &engine, TouchKind::press, Vector2::create(150.0, 120.0));
		nodeTree.dispatchTouch(fieldId, &engine, TouchKind::release, Vector2::create(150.0, 120.0));
		assert!(getField(&mut nodeTree, fieldId).isFocused());

		pushEvent(&mut engine, TextInputEventKind::commit(String::from("abc")));
		pushEvent(&mut engine, keyEvent(TextInputKey::left, false));
		pushEvent(&mut engine, keyEvent(TextInputKey::backspace, false));
		pushEvent(&mut engine, TextInputEventKind::commit(String::from("X")));
		pushEvent(&mut engine, keyEvent(TextInputKey::home, true));
		runFrame(&mut nodeTree, &mut engine);
		{
			let field = getField(&mut nodeTree, fieldId);
			assert_eq!(field.getText(), "aXc");
			assert_eq!(field.getSelectionRange(), Some((0, 2)));
			assert_eq!(field.getSelectionText(), "aX");
		}
		assert_eq!(changeCount.get(), 3);

		pushEvent(&mut engine, keyEvent(TextInputKey::delete, false));
		pushEvent(&mut engine, keyEvent(TextInputKey::end, false));
		pushEvent(&mut engine, keyEvent(TextInputKey::enter, false));
		runFrame(&mut nodeTree, &mut engine);
		assert_eq!(getField(&mut nodeTree, fieldId).getText(), "c");
		assert_eq!(getField(&mut nodeTree, fieldId).getCursorIndex(), 1);
		assert_eq!(*submittedText.borrow(), "c");
		assert_eq!(changeCount.get(), 4);
	}

	//==============================================================================
	// IME: 조합 중에는 커서 자리에 조합 글이 보이고 값은 그대로, 확정하면 값에 들어간다.
	//==============================================================================
	#[test]
	fn imePreeditThenCommit() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, fieldId) = createFieldTree();
		{
			let field = getField(&mut nodeTree, fieldId);
			field.setText("가다");
			field.moveCursor(1, false);
			field.focus();
		}
		pushEvent(&mut engine, TextInputEventKind::compositionStart);
		pushEvent(&mut engine, TextInputEventKind::preedit {
			text: String::from("ㄴ"),
			cursorIndex: Some(1),
		});
		pushEvent(&mut engine, TextInputEventKind::preedit {
			text: String::from("나"),
			cursorIndex: Some(1),
		});
		runFrame(&mut nodeTree, &mut engine);
		{
			let field = getField(&mut nodeTree, fieldId);
			assert!(field.isComposing());
			assert_eq!(field.getText(), "가다");
			assert_eq!(field.getComposition(), "나");
			assert_eq!(field.getDisplayText(), "가나다");
		}
		pushEvent(&mut engine, TextInputEventKind::preedit {
			text: String::new(),
			cursorIndex: None,
		});
		pushEvent(&mut engine, TextInputEventKind::compositionEnd);
		pushEvent(&mut engine, TextInputEventKind::commit(String::from("나")));
		runFrame(&mut nodeTree, &mut engine);
		let field = getField(&mut nodeTree, fieldId);
		assert!(!field.isComposing());
		assert_eq!(field.getText(), "가나다");
		assert_eq!(field.getDisplayText(), "가나다");
		assert_eq!(field.getCursorIndex(), 2);
	}

	//==============================================================================
	// 조합 중에 필드 안을 누르면 조합 글을 확정하고, OS 의 같은 확정은 버리게 하고, IME 를 끊어 달라고 한다.
	//==============================================================================
	#[test]
	fn pressWhileComposingCommits() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, fieldId) = createFieldTree();
		getField(&mut nodeTree, fieldId).focus();
		pushEvent(&mut engine, TextInputEventKind::compositionStart);
		pushEvent(&mut engine, TextInputEventKind::preedit {
			text: String::from("글"),
			cursorIndex: Some(1),
		});
		runFrame(&mut nodeTree, &mut engine);
		nodeTree.dispatchTouch(fieldId, &engine, TouchKind::press, Vector2::create(110.0, 130.0));
		{
			let field = getField(&mut nodeTree, fieldId);
			assert_eq!(field.getText(), "글");
			assert_eq!(field.getComposition(), "");
			assert!(!field.isComposing());
			assert!(field.isFocused());
		}
		let textInput = engine.getInputManagerMut().getTextInputMut();
		assert!(textInput.takeImeResetRequest());
		assert!(!textInput.takeImeResetRequest());
		assert!(textInput.consumeImeCommitGuard("글"));
	}

	//==============================================================================
	// 최대 길이: 확정 글이 넘치면 자른다.
	//==============================================================================
	#[test]
	fn maxLengthLimitsCommit() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, fieldId) = createFieldTree();
		{
			let field = getField(&mut nodeTree, fieldId);
			field.setMaxLength(5);
			field.focus();
		}
		pushEvent(&mut engine, TextInputEventKind::commit(String::from("닉네임")));
		pushEvent(&mut engine, TextInputEventKind::commit(String::from("길게씀")));
		pushEvent(&mut engine, TextInputEventKind::paste(String::from("더")));
		runFrame(&mut nodeTree, &mut engine);
		let field = getField(&mut nodeTree, fieldId);
		assert_eq!(field.getText(), "닉네임길게");
		assert_eq!(field.getCursorIndex(), 5);
	}

	//==============================================================================
	// 포커스 바뀜: 나중에 포커스를 얻은 필드만 남는다. 필드 밖을 누르면 포커스를 잃고 조합 글은 확정된다.
	// attach 하지 않은 필드는 글 입력을 받지 않는다.
	//==============================================================================
	#[test]
	fn focusChanges() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, firstFieldId) = createFieldTree();
		let rootId = nodeTree.getRootId();
		let secondFieldId = nodeTree.createChild(rootId, "second");
		nodeTree[secondFieldId].setLocalPosition(Vector2::create(100.0, 300.0));
		nodeTree[secondFieldId].setContentSize(Vector2::create(300.0, 60.0));
		nodeTree.addComponent(secondFieldId, UIInputField::new()).expect("field").setUseDOMInput(true);

		getField(&mut nodeTree, firstFieldId).focus();
		runFrame(&mut nodeTree, &mut engine);
		assert_eq!(engine.getInputManager().getTextInput().getActiveFieldState().map(|fieldState| fieldState.fieldId), Some(getField(&mut nodeTree, firstFieldId).getFieldId()));

		// attach 하지 않은 두 번째 필드에 포커스: 첫 필드는 다음 tick 에 포커스를 내준다. 글은 아무도 받지 않는다.
		getField(&mut nodeTree, secondFieldId).focus();
		runFrame(&mut nodeTree, &mut engine);
		pushEvent(&mut engine, TextInputEventKind::commit(String::from("z")));
		runFrame(&mut nodeTree, &mut engine);
		assert!(!getField(&mut nodeTree, firstFieldId).isFocused());
		assert!(getField(&mut nodeTree, secondFieldId).isFocused());
		assert_eq!(getField(&mut nodeTree, firstFieldId).getText(), "");
		assert_eq!(getField(&mut nodeTree, secondFieldId).getText(), "");
		assert!(engine.getInputManager().getTextInput().getActiveFieldState().is_none());

		// 첫 필드를 다시 누르고 조합하다가 밖을 누른다.
		nodeTree.dispatchTouch(firstFieldId, &engine, TouchKind::press, Vector2::create(150.0, 120.0));
		pushEvent(&mut engine, TextInputEventKind::compositionStart);
		pushEvent(&mut engine, TextInputEventKind::preedit {
			text: String::from("한"),
			cursorIndex: Some(1),
		});
		runFrame(&mut nodeTree, &mut engine);
		assert!(getField(&mut nodeTree, firstFieldId).isFocused());
		assert_eq!(getField(&mut nodeTree, firstFieldId).getComposition(), "한");
		{
			let inputManager = engine.getInputManagerMut();
			inputManager.setViewInputPosition(Vector2::create(700.0, 600.0));
			inputManager.setTouchPressed(true);
		}
		runFrame(&mut nodeTree, &mut engine);
		engine.getInputManagerMut().setTouchPressed(false);
		assert!(!getField(&mut nodeTree, secondFieldId).isFocused());
		let firstField = getField(&mut nodeTree, firstFieldId);
		assert!(!firstField.isFocused());
		assert_eq!(firstField.getText(), "한");
		assert_eq!(firstField.getComposition(), "");
		// OS 가 같은 조합 글을 다시 보내면 버린다.
		assert!(engine.getInputManagerMut().getTextInputMut().consumeImeCommitGuard("한"));
	}

	//==============================================================================
	// 웹 DOM 입력 동기: 값 바뀜(조합 중엔 무시), 선택 바뀜, DOM 포커스.
	//==============================================================================
	#[test]
	fn domSynchronizationEvents() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, fieldId) = createFieldTree();
		let fieldIdValue = getField(&mut nodeTree, fieldId).getFieldId();
		let pushTargetedEvent = |engine: &mut Engine, eventKind: TextInputEventKind| {
			engine.getInputManagerMut().getTextInputMut().pushEvent(TextInputEvent {
				targetFieldId: fieldIdValue,
				kind: eventKind,
			});
		};
		pushTargetedEvent(&mut engine, TextInputEventKind::focusGained);
		pushTargetedEvent(&mut engine, TextInputEventKind::replaceValue {
			value: String::from("hello"),
			selectionStart: 5,
			selectionEnd: 5,
		});
		pushTargetedEvent(&mut engine, TextInputEventKind::selection {
			start: 1,
			end: 3,
		});
		pushTargetedEvent(&mut engine, TextInputEventKind::compositionStart);
		pushTargetedEvent(&mut engine, TextInputEventKind::replaceValue {
			value: String::from("hello?"),
			selectionStart: 6,
			selectionEnd: 6,
		});
		runFrame(&mut nodeTree, &mut engine);
		{
			let field = getField(&mut nodeTree, fieldId);
			assert!(field.isFocused());
			assert_eq!(field.getText(), "hello");
			assert_eq!(field.getSelectionRange(), Some((1, 3)));
			assert!(field.isComposing());
		}
		pushTargetedEvent(&mut engine, TextInputEventKind::compositionEnd);
		pushTargetedEvent(&mut engine, TextInputEventKind::focusLost);
		runFrame(&mut nodeTree, &mut engine);
		assert!(!getField(&mut nodeTree, fieldId).isFocused());
	}

	//==============================================================================
	// 문맥 메뉴: 오른쪽 단추 요청이 필드 위면 포커스를 주고 메뉴 노드를 만든다. 메뉴 밖을 누르면 닫힌다.
	// 붙여넣기 항목은 다음 tick 에 한다. (클립보드를 건드리지 않게 항목 부탁만 본다)
	//==============================================================================
	#[test]
	fn contextMenuOpensAndCloses() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut nodeTree, fieldId) = createFieldTree();
		let rootId = nodeTree.getRootId();
		engine.getInputManagerMut().getTextInputMut().setContextMenuRequestPosition(Some(Vector2::create(200.0, 120.0)));
		runFrame(&mut nodeTree, &mut engine);
		assert!(getField(&mut nodeTree, fieldId).isFocused());
		let menuId = nodeTree.findChildByName(rootId, "uiInputFieldContextMenu").expect("menu");
		assert_eq!(nodeTree[menuId].getLocalPosition(), Vector2::create(200.0, 120.0));
		assert!(nodeTree[menuId].isInteractable());
		runFrame(&mut nodeTree, &mut engine);
		assert!(nodeTree.contains(menuId));
		{
			let inputManager = engine.getInputManagerMut();
			inputManager.setViewInputPosition(Vector2::create(900.0, 600.0));
			inputManager.setTouchPressed(true);
		}
		runFrame(&mut nodeTree, &mut engine);
		engine.getInputManagerMut().setTouchPressed(false);
		assert!(!nodeTree.contains(menuId));
		assert!(!getField(&mut nodeTree, fieldId).isFocused());
	}
}

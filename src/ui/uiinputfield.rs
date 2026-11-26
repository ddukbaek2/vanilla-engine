//==============================================================================
//! 입력 필드. (원본 ui/uiinputfield.js 의 UIInputField, BackgroundLayerRenderer, OverlayLayerRenderer)
//! - 글, 커서, 선택 영역, IME 조합 글과 밑줄, 포커스 테두리를 캔버스에 직접 그리고, 누르기와 끌기로 커서와 선택을 정한다.
//! - 원본은 WorldNode 를 상속한 노드이고, 글은 UILabel 자식 노드 두 개(값, 안내 글), 커서와 테두리는 오버레이 자식 노드가 그린다.
//!   러스트 판은 다른 위젯처럼 컴포넌트다. node_tree.add_component(node_id, UIInputField::new()) 로 붙인다.
//!   UIControl 을 품어 attach_to_tree 에서 content 노드를 만들고 호스트를 interactable 로 켠다. 피봇과 앵커는 원본처럼 왼쪽 위로 맞춘다.
//!   글은 Text 두 개(값, 안내 글)를 품어 그리고, 바탕(원본 Paint 컴포넌트)도 품은 Paint 로 그린다.
//!   층 순서는 원본과 같다: 바탕 → 선택 영역 → 글 → 조합 밑줄 → 커서 → 포커스 테두리. 한 컴포넌트가 한 번에 그려서
//!   씬 코드가 tick 밖에서 set_text 를 불러도 그 프레임에 바로 보인다. (자식 노드로 나누면 상태를 옮기느라 한 프레임 늦는다)
//! - 입력은 엔진의 글 입력 줄(core/textinput.rs)에서 받는다. 포커스를 가진, attach() 한 필드가 tick 에서 꺼내 쓴다.
//!   - 데스크탑: winit 의 KeyboardInput 과 Ime 이벤트. use_dom_input(원본 setUseDOMInput)이 켜진 필드가 포커스를 가질 때만 IME 가 켜진다.
//!   - 웹: 원본처럼 필드마다 숨긴 DOM <input> 을 필드 위에 둔다. (platform/web/textinput.rs) 값과 선택은 DOM 이 주인이고 필드가 따라간다.
//!     use_dom_input 이 꺼진 필드는 원본 캔버스 모드처럼 window keydown 으로 받는다.
//! - 원본과 다른 것:
//!   - 이벤트는 Box<dyn FnMut(&mut UIInputField, &mut ComponentContext)> 다. 원본 콜백은 값만 받는다. (값은 field.get_text())
//!   - setFont(FontFace) 는 set_font_family(등록한 폰트 이름) 로 바꿨다. setEngine 은 옮기지 않았다. (엔진은 문맥으로 받는다)
//!   - 공개 함수의 글자 자리(get_cursor_index, get_selection_start, get_selection_range, move_cursor, measure_index_at_local_x)와 max_length 는
//!     원본처럼 UTF-16 단위로 센다. 안에서는 문자(char) 단위로 들어 서로게이트 쌍(이모지 같은 글자)을 쪼개지 않는다.
//!     그래서 UTF-16 자리가 서로게이트 쌍 가운데면 그 글자 뒤로 본다. (원본도 같다) max_length 가 있으면 블링크 입력 칸처럼 넣는 글을 남은 길이까지만 넣는다.
//!   - 누른 자리 → 글자 자리는 draw 때 재 둔 글자 경계로 찾는다. (원본은 오프스크린 2D 캔버스로 그때그때 잰다)
//!   - 데스크탑에서는 use_dom_input 이 켜진 필드도 원본 DOM 모드처럼 필드 밖을 누르면 포커스를 잃는다. 포커스는 엔진에 하나뿐이다.
//!     (원본 캔버스 모드는 다른 필드를 눌러도 앞 필드가 포커스를 쥐고 있다. 러스트 판은 나중에 포커스를 얻은 필드만 남긴다)
//!   - 포커스를 잃을 때 조합 중이던 글은 확정해 값에 넣는다. (브라우저가 DOM 입력에 하는 일과 같다)
//!   - 데스크탑 문맥 메뉴(오른쪽 단추)는 DOM 이 없어 캔버스에 그린다. 루트의 마지막 자식 노드로 만들어 위에 그린다. 웹은 원본처럼 DOM 메뉴다.
//!     항목은 원본처럼 잘라내기, 복사, 붙여넣기, 모두 선택이다. DOM 입력을 직접 누르게 한 필드(use_direct_touch)도 원본처럼 브라우저 편집 칸 메뉴 대신 같은 메뉴다.
//!     그 필드는 크롬 데스크탑 터치처럼 손가락 길게 누르기(650ms, 15 DIP 안)로 낱말을 고르고 터치 손잡이와 빠른 메뉴를 띄운다. (윈도우는 뗄 때 메뉴를 정한다)
//!   - 데스크탑 DOM 입력 필드는 원본 DOM 입력 칸처럼 되돌리기와 다시 하기를 한다. (Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y, 맥은 Cmd, 블링크의 묶음 규칙, TextEditBuffer)
//!   - 데스크탑 한 줄 DOM 입력 필드는 블링크 입력 칸처럼 넣는 글의 끝 줄바꿈을 지우고 가운데 줄바꿈을 공백으로 바꾸고, 위와 page 위는 글 앞, 아래와 page 아래는 글 끝으로 옮긴다.
//!   - set_multiline(true) 면 크롬 textarea 같은 여러 줄 입력이다. (원본 엔진에는 없고, 원본은 Pane 의 textarea 프리셋으로 DOM textarea 를 쓴다)
//!     글은 white-space: pre-wrap, overflow-wrap: break-word 처럼 줄바꿈 문자와 폭으로 나눈다. (ui/textarealayout.rs, 줄 나눔 자리는 크롬처럼 유니코드 줄 나누기)
//!     Enter 는 줄을 넣고(글자 넣기 되돌리기 묶음), 위아래는 첫 위아래 이동 때의 가로 자리를 지키며(첫 줄 위는 글 앞, 마지막 줄 아래는 글 끝),
//!     Home 과 End 는 보이는 줄의 앞과 끝(맥은 Cmd + 왼쪽, 오른쪽도), Ctrl + Home, End(맥은 Cmd + 위, 아래)는 글 앞과 끝,
//!     PageUp, PageDown(맥은 Option 과 함께)은 보이는 높이의 87.5%(맥은 높이 - 40 과 견줘 큰 쪽) 안의 가장 먼 줄로 옮겨 그 줄을 맨 위로 올린다.
//!     넣는 글의 CR, CRLF 는 LF 로 바꾼다. 커서는 편집과 이동 뒤 보이게 스크롤하고(맥이 아니면 가장자리, 맥은 가운데), 휠과 썸 끌기로 넘긴다.
//!     줄 높이의 normal 은 크롬처럼 첫째 글꼴의 위아래 길이와 줄 사이로 셈한다. 스크롤 막대는 크롬처럼 클래식 막대(윈도우, 리눅스, 맥 클래식 설정)면 넘칠 때 글 폭을 던다.
//!     맥 Home, End 는 크롬 맥처럼 커서를 두고 처음과 끝으로 넘기고(넘길 것이 없으면 줄 앞과 끝), PageUp, PageDown 은 커서를 두고 한 화면만큼 넘긴다.
//!   - DOM 입력을 흉내 내는 필드(use_dom_input, 여러 줄)는 크롬처럼 마우스로 두 번 누르면 낱말(윈도우는 뒤 공백까지), 세 번 누르면 문단을 고르고 끌면 그 단위로 넓힌다.
//!     이어 누르기는 크롬 aura 처럼 500ms(맥은 시스템의 두 번 클릭 시간) 안, 가로 세로 2 DIP 안이다. 웹은 브라우저가 센 mousedown 의 detail 을 쓴다.
//!   - 커서 이동과 Delete 는 크롬처럼 글자 묶음(확장 자소 묶음) 단위, 뒤로 지우기는 블링크 BackspaceStateMachine(코드 포인트 하나, 이모지 묶음은 통째로)이다.
//!     웹 DOM 입력은 숨긴 `<textarea>` 가 글, 조합, 되돌리기를 맡고, 보이는 줄로 옮기는 키(위아래, page, Home, End)는 필드가 엔진 배치로 옮긴다.
//!   - 낱말 이동과 지우기(윈도우와 리눅스 Ctrl, 맥 Option 과 왼쪽, 오른쪽, Backspace, Delete)는 크롬 textarea 처럼 블링크 NextWordPosition, PreviousWordPosition 대로다.
//!     (오른쪽은 윈도우만 다음 낱말 앞, 맥과 리눅스는 지금 낱말 끝. 맥은 넓히던 고르기가 고정 끝을 넘지 않는다) 웹 DOM 입력은 브라우저가 한다.
//!   - 노드를 끄거나(set_active(false)) 지우면 tick 이 돌지 않아 포커스와 IME, 웹의 숨긴 <input> 이 함께 사라진다. (원본은 detach 해야 사라진다)
//!   - getRenderState 는 옮기지 않았다. (그리는 컴포넌트가 하나라 넘길 일이 없다)
//!   - UI 문서(UIDocument)는 register_node_type 으로 등록하면 원본 노드 꼴로 쓰고 읽는다. (노드 type "UIInputField", 바탕 Paint, 그리기 층, 안내 글과 값 UILabel 자식,
//!     오버레이 자식. uidocument.rs 의 UIDocumentNode) 러스트 판이 만드는 content 노드는 문서에 적지 않는다.
//!     글 라벨과 오버레이의 자리와 크기는 원본 refreshTextPositions 처럼 tick 이 마지막으로 놓은 노드 크기(text_layout_size)로 적는다.
//!
//! # 예제
//! 이어 친 글자는 한 번에 되돌리고 다시 하기
//! ```rust
//! use vanilla::{TextEditBuffer, TextEditKind};
//!
//! let mut buffer = TextEditBuffer::new();
//! buffer.set_text("abc");
//! buffer.begin_edit(TextEditKind::InsertText);
//! buffer.replace_selection("d");
//! buffer.begin_edit(TextEditKind::InsertText);
//! buffer.replace_selection("e");
//! assert_eq!(buffer.get_text(), "abcde");
//!
//! assert!(buffer.undo());
//! assert_eq!(buffer.get_text(), "abc");
//! assert!(buffer.redo());
//! assert_eq!(buffer.get_text(), "abcde");
//! assert!(!buffer.can_redo());
//! ```
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::blurshadow::{css_blur_radius_to_sigma, BlurredRoundRectShadow};
use crate::core::component::paint::Paint;
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::engine::Engine;
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::core::textinput::{collect_grapheme_boundaries, convert_character_index_to_byte_index, convert_character_index_to_utf16_index, convert_utf16_index_to_character_index, count_characters, collect_word_boundaries, find_backward_deletion_boundary, find_next_grapheme_boundary, find_next_word_position, find_previous_grapheme_boundary, find_previous_word_position, TextInputEventKind, TextInputFieldState, TextInputKey};
use crate::ui::textarealayout::{build_text_area_layout, TextAreaLayout};
use crate::ui::uicontrol::UIControl;
use crate::ui::uiview::is_view_position_inside_node;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicU64, Ordering};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// 커서 깜빡임 주기. (초)
const CURSOR_BLINK_SECONDS: f32 = 0.5;
/// 손가락 길게 누르기. (크롬 데스크탑 aura: long_press_time 500 에 show_press_delay 150 을 더한 650ms, 허용 거리 max_touch_move_in_pixels_for_click 15 + 0.05)
#[cfg(not(target_arch = "wasm32"))]
const LONG_PRESS_SECONDS: f32 = 0.65;
const LONG_PRESS_TOUCH_SLOP: f32 = 15.05;
/// 빠른 메뉴는 손가락을 뗀 뒤 100ms 에 뜬다. (크롬 kQuickMenuDelayInMs)
#[cfg(not(target_arch = "wasm32"))]
const QUICK_MENU_DELAY_SECONDS: f32 = 0.1;
/// 터치 선택 손잡이. (크롬 touch_handle_drawable_aura: 왼쪽과 오른쪽 32, 가운데 39 DIP, 누름 판정은 좌우와 아래 6 더, 색 kColorSysPrimary #0B57D0, 불투명도 0.8)
const TOUCH_HANDLE_SIZE: f32 = 32.0;
const TOUCH_CENTER_HANDLE_SIZE: f32 = 39.0;
const TOUCH_HANDLE_PADDING: f32 = 6.0;
const TOUCH_HANDLE_COLOR: Color = Color::create(11.0 / 255.0, 87.0 / 255.0, 208.0 / 255.0, 0.8);
/// 이어 누르기 거리. (크롬 aura ui/events/event.cc 의 MouseEvent::IsRepeatedClickEvent: kDoubleClickWidth, kDoubleClickHeight 4 의 반, DIP)
#[cfg(not(target_arch = "wasm32"))]
const REPEATED_CLICK_SLOP: f32 = 2.0;
/// 여러 줄 필드의 탭 크기. (공백 몇 개 폭, CSS tab-size 기본 8)
const DEFAULT_TAB_SIZE: f32 = 8.0;
/// 여러 줄 필드의 얹는 스크롤 막대 썸. (맥 얹는 막대 설정, PaneStyle 과 같다. 클래식 막대는 web/pane.rs 의 크롬 치수)
const TEXT_AREA_SCROLLBAR_THICKNESS: f32 = 8.0;
const TEXT_AREA_SCROLLBAR_MINIMUM_THUMB_LENGTH: f32 = 20.0;
/// 폰트 이름을 정하지 않았을 때 쓰는 이름. (엔진 기본 폰트, 원본 SYSTEM_FONT_STRING 자리)
const DEFAULT_FONT_FAMILY: &str = "default";
/// 문맥 메뉴 크기. (원본 DOM 메뉴: 최소 폭 140, 위아래 여백 4, 항목 위아래 여백 8, 글자 14px)
const CONTEXT_MENU_WIDTH: f32 = 140.0;
const CONTEXT_MENU_PADDING_Y: f32 = 4.0;
const CONTEXT_MENU_ITEM_HEIGHT: f32 = 34.0;
const CONTEXT_MENU_ITEM_PADDING_X: f32 = 16.0;
const CONTEXT_MENU_FONT_STRING: &str = "14px default";

/// 필드 id. (엔진 글 입력 줄에서 필드를 가리킨다. 0 은 "포커스를 가진 필드"라 1 부터)
static NEXT_FIELD_ID: AtomicU64 = AtomicU64::new(1);
/// 포커스 차례. (나중에 포커스를 얻은 필드가 이긴다)
static NEXT_FOCUS_SERIAL: AtomicU64 = AtomicU64::new(1);

//==============================================================================
/// 입력 필드 이벤트. (필드와 문맥을 받는다. 값은 field.get_text())
//==============================================================================
pub type UIInputFieldEvent = Box<dyn FnMut(&mut UIInputField, &mut ComponentContext)>;

//==============================================================================
/// 문맥 메뉴 항목. (원본 UIInputFieldMenuAction 의 잘라내기 / 복사 / 붙여넣기 / 모두 선택, request_menu_action 이 받는다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UIInputFieldMenuAction {
	/// 잘라내기.
	Cut,
	/// 복사.
	Copy,
	/// 붙여넣기.
	Paste,
	/// 모두 선택.
	SelectAll,
}

//==============================================================================
/// 되돌리기 묶음 종류. (블링크 TypingCommand 와 붙여넣기)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEditKind {
	/// 글자 넣기. (열린 묶음이 있으면 종류를 가리지 않고 그 묶음에 붙고, 묶음은 글자 넣기가 된다)
	InsertText,
	/// 뒤로 지우기. (열린 묶음이 뒤로 지우기일 때만 붙고, 아니면 새 묶음)
	DeleteBackward,
	/// 앞으로 지우기. (열린 묶음이 앞으로 지우기일 때만 붙고, 아니면 새 묶음)
	DeleteForward,
	/// 붙여넣기와 잘라내기, 낱말 지우기 키로 고른 것 지우기. (블링크 ReplaceSelectionCommand, DeleteSelectionCommand 라 늘 따로 한 묶음이고, 그 뒤 글자 넣기도 새 묶음)
	Paste,
	/// 낱말 뒤로 지우기. (블링크는 늘 새 TypingCommand 를 열어 새 묶음이고, 그 뒤 뒤로 지우기와 글자 넣기는 이 묶음에 붙는다)
	DeleteWordBackward,
	/// 낱말 앞으로 지우기. (늘 새 묶음이고, 그 뒤 앞으로 지우기와 글자 넣기는 이 묶음에 붙는다)
	DeleteWordForward,
}

//==============================================================================
/// 되돌릴 때 돌아갈 값과 선택.
//==============================================================================
#[derive(Clone, Debug, Default)]
struct TextEditSnapshot {
	value: String,
	selection_start: usize,
	cursor_index: usize,
}

/// 되돌리기 깊이. (블링크 undo_stack.cc 의 kMaximumUndoStackDepth)
const MAXIMUM_UNDO_DEPTH: usize = 1000;

//==============================================================================
/// 편집 버퍼. (값, 조합 글, 커서, 선택 시작. 원본 UIInputField 의 #value / #composition / #cursor_index / #selection_start 와 편집 함수)
/// - 자리는 문자 단위다. 커서와 선택 시작이 다르면 그 사이가 선택 영역이다. (커서가 앞일 수도 있다)
/// - max_length 는 원본 maxLength 처럼 UTF-16 단위다. (웹 DOM <input> 의 maxlength 와 같은 셈)
/// - 되돌리기와 다시 하기: 원본 DOM 입력 칸은 브라우저가 해 준다. 묶음은 블링크와 같다. (이어 친 글자는 한 번에, 커서를 옮기거나 선택을 바꾸면 끊긴다,
///   새 편집은 다시 하기 목록을 지운다, 값을 코드로 넣으면 목록을 비운다) 편집하는 쪽이 고치기 전에 begin_edit 를 부른다.
///
/// # 예제
/// 최대 길이로 자르고 고른 글을 바꾼 뒤 뒤로 지우기
/// ```rust
/// use vanilla::TextEditBuffer;
///
/// let mut buffer = TextEditBuffer::new();
/// buffer.set_max_length(5);
/// buffer.set_text("hello world");
/// assert_eq!(buffer.get_text(), "hello");
///
/// buffer.set_selection(0, 5);
/// assert_eq!(buffer.get_selection_text(), "hello");
/// buffer.replace_selection("hi");
/// assert_eq!(buffer.get_text(), "hi");
/// assert_eq!(buffer.get_cursor_index(), 2);
/// assert!(buffer.delete_backward());
/// assert_eq!(buffer.get_text(), "h");
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct TextEditBuffer {
	value: String,
	composition: String,
	cursor_index: usize,
	selection_start: usize,
	max_length: usize,
	undo_list: Vec<TextEditSnapshot>,
	redo_list: Vec<TextEditSnapshot>,
	open_edit_kind: Option<TextEditKind>,
}

impl TextEditBuffer {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 빈 버퍼. (값과 조합 글이 비었고 최대 길이는 0)
	//==============================================================================
	pub fn new() -> TextEditBuffer {
		TextEditBuffer::default()
	}

	//==============================================================================
	/// 값 설정. (max_length 로 자르고 커서는 끝에. 원본 setText)
	///
	/// # 인자
	/// - `text`: 넣을 글. (되돌리기와 다시 하기 목록도 비운다)
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		self.value = text.to_string();
		self.truncate_to_max_length();
		self.cursor_index = count_characters(&self.value);
		self.selection_start = self.cursor_index;
		self.undo_list.clear();
		self.redo_list.clear();
		self.open_edit_kind = None;
	}

	//==============================================================================
	/// 편집 시작 알리기. (고치기 전에 부른다. 새 묶음이면 지금 값을 되돌리기 목록에 쌓고 다시 하기 목록을 지운다)
	///
	/// # 인자
	/// - `edit_kind`: 할 편집의 되돌리기 묶음 종류.
	//==============================================================================
	pub fn begin_edit(&mut self, edit_kind: TextEditKind) {
		match (self.open_edit_kind, edit_kind) {
			(Some(_), TextEditKind::InsertText) => {
				self.open_edit_kind = Some(TextEditKind::InsertText);
				return;
			}
			(Some(TextEditKind::DeleteBackward), TextEditKind::DeleteBackward) | (Some(TextEditKind::DeleteForward), TextEditKind::DeleteForward) => {
				return;
			}
			_ => {
			}
		}
		self.undo_list.push(self.create_snapshot());
		if self.undo_list.len() > MAXIMUM_UNDO_DEPTH {
			self.undo_list.remove(0);
		}
		self.redo_list.clear();
		self.open_edit_kind = match edit_kind {
			TextEditKind::Paste => {
				None
			}
			TextEditKind::DeleteWordBackward => {
				Some(TextEditKind::DeleteBackward)
			}
			TextEditKind::DeleteWordForward => {
				Some(TextEditKind::DeleteForward)
			}
			_ => {
				Some(edit_kind)
			}
		};
	}

	//==============================================================================
	/// 열린 묶음 닫기. (다음 편집은 새 묶음)
	//==============================================================================
	pub fn close_edit_group(&mut self) {
		self.open_edit_kind = None;
	}

	//==============================================================================
	/// 되돌릴 것이 있는지.
	///
	/// # 돌려줌
	/// 되돌릴 묶음이 있으면 true.
	//==============================================================================
	pub fn can_undo(&self) -> bool {
		!self.undo_list.is_empty()
	}

	//==============================================================================
	/// 다시 할 것이 있는지.
	///
	/// # 돌려줌
	/// 다시 할 묶음이 있으면 true.
	//==============================================================================
	pub fn can_redo(&self) -> bool {
		!self.redo_list.is_empty()
	}

	//==============================================================================
	/// 되돌리기. (했으면 true)
	///
	/// # 돌려줌
	/// 되돌렸으면 true. 되돌릴 것이 없으면 false.
	//==============================================================================
	pub fn undo(&mut self) -> bool {
		self.open_edit_kind = None;
		let snapshot = match self.undo_list.pop() {
			Some(snapshot) => {
				snapshot
			}
			None => {
				return false;
			}
		};
		self.redo_list.push(self.create_snapshot());
		self.apply_snapshot(snapshot);
		true
	}

	//==============================================================================
	/// 다시 하기. (했으면 true)
	///
	/// # 돌려줌
	/// 다시 했으면 true. 다시 할 것이 없으면 false.
	//==============================================================================
	pub fn redo(&mut self) -> bool {
		self.open_edit_kind = None;
		let snapshot = match self.redo_list.pop() {
			Some(snapshot) => {
				snapshot
			}
			None => {
				return false;
			}
		};
		self.undo_list.push(self.create_snapshot());
		self.apply_snapshot(snapshot);
		true
	}

	//==============================================================================
	/// 지금 값과 선택.
	//==============================================================================
	fn create_snapshot(&self) -> TextEditSnapshot {
		TextEditSnapshot {
			value: self.value.clone(),
			selection_start: self.selection_start,
			cursor_index: self.cursor_index,
		}
	}

	//==============================================================================
	/// 값과 선택 되돌려 놓기.
	//==============================================================================
	fn apply_snapshot(&mut self, snapshot: TextEditSnapshot) {
		self.value = snapshot.value;
		self.selection_start = snapshot.selection_start;
		self.cursor_index = snapshot.cursor_index;
	}

	//==============================================================================
	/// 값 반환.
	///
	/// # 돌려줌
	/// 입력한 글. (조합 중인 글은 빠진다)
	//==============================================================================
	pub fn get_text(&self) -> &str {
		&self.value
	}

	//==============================================================================
	/// 플랫폼이 준 값으로 바꾸기. (자르지 않는다. 커서를 정하고 선택은 접는다. 원본 handleDOMInputEvent)
	///
	/// # 인자
	/// - `value`: 플랫폼 입력 칸의 값.
	/// - `cursor_index`: 커서 자리. (문자 단위, 값 끝을 넘으면 끝으로)
	//==============================================================================
	pub fn set_value_from_platform(&mut self, value: &str, cursor_index: usize) {
		self.value = value.to_string();
		self.cursor_index = cursor_index.min(count_characters(&self.value));
		self.selection_start = self.cursor_index;
	}

	//==============================================================================
	/// 최대 길이 설정. (UTF-16 단위, 0 이면 제한 없음. 원본처럼 지금 값은 자르지 않는다)
	///
	/// # 인자
	/// - `max_length`: 최대 글자 수. (UTF-16 단위, 0 이면 제한하지 않는다, 지금 값은 자르지 않는다)
	//==============================================================================
	pub fn set_max_length(&mut self, max_length: usize) {
		self.max_length = max_length;
	}

	//==============================================================================
	/// 최대 길이 반환.
	///
	/// # 돌려줌
	/// 최대 글자 수. (0 이면 제한하지 않는다)
	//==============================================================================
	pub fn get_max_length(&self) -> usize {
		self.max_length
	}

	//==============================================================================
	/// 값을 최대 길이로 자르기. (UTF-16 길이가 max_length 를 넘지 않는 마지막 글자 경계까지)
	//==============================================================================
	fn truncate_to_max_length(&mut self) {
		if self.max_length == 0 {
			return;
		}
		let mut utf16_count = 0;
		for (byte_index, character) in self.value.char_indices() {
			utf16_count += character.len_utf16();
			if utf16_count > self.max_length {
				self.value.truncate(byte_index);
				return;
			}
		}
	}

	//==============================================================================
	/// 조합 글 설정.
	///
	/// # 인자
	/// - `composition`: IME 가 조합 중인 글. (빈 글이면 없음)
	//==============================================================================
	pub fn set_composition(&mut self, composition: &str) {
		self.composition = composition.to_string();
	}

	//==============================================================================
	/// 조합 글 반환.
	///
	/// # 돌려줌
	/// IME 가 조합 중인 글. (없으면 빈 글)
	//==============================================================================
	pub fn get_composition(&self) -> &str {
		&self.composition
	}

	//==============================================================================
	/// 커서 자리.
	///
	/// # 돌려줌
	/// 커서 자리. (문자 단위, 0 ~ 값의 문자 수)
	//==============================================================================
	pub fn get_cursor_index(&self) -> usize {
		self.cursor_index
	}

	//==============================================================================
	/// 선택 시작 자리. (선택을 늘릴 때 고정되는 쪽)
	///
	/// # 돌려줌
	/// 선택의 고정 끝. (문자 단위, 커서 자리와 같으면 선택이 없다)
	//==============================================================================
	pub fn get_selection_start(&self) -> usize {
		self.selection_start
	}

	//==============================================================================
	/// 선택 정하기. (끝을 넘으면 자른다)
	///
	/// # 인자
	/// - `selection_start`: 선택의 고정 끝. (문자 단위, 값 끝을 넘으면 끝으로)
	/// - `cursor_index`: 커서 자리. (문자 단위, 값 끝을 넘으면 끝으로)
	//==============================================================================
	pub fn set_selection(&mut self, selection_start: usize, cursor_index: usize) {
		let character_count = count_characters(&self.value);
		self.selection_start = selection_start.min(character_count);
		self.cursor_index = cursor_index.min(character_count);
		self.open_edit_kind = None;
	}

	//==============================================================================
	/// 선택이 있는지.
	///
	/// # 돌려줌
	/// 선택한 글이 있으면 참.
	//==============================================================================
	pub fn has_selection(&self) -> bool {
		self.cursor_index != self.selection_start
	}

	//==============================================================================
	/// 선택 범위. (앞, 뒤. 없으면 None)
	///
	/// # 돌려줌
	/// 선택 범위 (앞, 뒤). (문자 단위, 앞이 작은 쪽, 선택이 없으면 None)
	//==============================================================================
	pub fn get_selection_range(&self) -> Option<(usize, usize)> {
		if !self.has_selection() {
			return None;
		}
		Some((self.cursor_index.min(self.selection_start), self.cursor_index.max(self.selection_start)))
	}

	//==============================================================================
	/// 선택된 글.
	///
	/// # 돌려줌
	/// 선택한 글. (없으면 빈 글)
	//==============================================================================
	pub fn get_selection_text(&self) -> String {
		match self.get_selection_range() {
			Some((start_index, end_index)) => {
				let start_byte_index = convert_character_index_to_byte_index(&self.value, start_index);
				let end_byte_index = convert_character_index_to_byte_index(&self.value, end_index);
				self.value[start_byte_index..end_byte_index].to_string()
			}
			None => {
				String::new()
			}
		}
	}

	//==============================================================================
	/// 선택을 글로 바꾸기. (선택이 없으면 커서 자리에 끼운다. 원본 replaceSelection)
	/// - max_length 가 있으면 블링크 입력 칸(text_field_input_type.cc 의 FilterBeforeTextInserted)처럼 넣는 글을 남은 UTF-16 길이까지만 넣는다. (글자는 쪼개지 않는다)
	///
	/// # 인자
	/// - `text`: 넣을 글. (빈 글이면 선택을 지운다)
	//==============================================================================
	pub fn replace_selection(&mut self, text: &str) {
		let (start_index, end_index) = self.get_selection_range().unwrap_or((self.cursor_index, self.cursor_index));
		let start_byte_index = convert_character_index_to_byte_index(&self.value, start_index);
		let end_byte_index = convert_character_index_to_byte_index(&self.value, end_index);
		let mut inserted_text = text;
		if self.max_length > 0 {
			let value_length = self.value.encode_utf16().count();
			let selection_length = self.value[start_byte_index..end_byte_index].encode_utf16().count();
			let appendable_length = self.max_length.saturating_sub(value_length - selection_length);
			let mut utf16_count = 0;
			for (byte_index, character) in text.char_indices() {
				utf16_count += character.len_utf16();
				if utf16_count > appendable_length {
					inserted_text = &text[..byte_index];
					break;
				}
			}
		}
		let mut next_value = String::with_capacity(self.value.len() + inserted_text.len());
		next_value.push_str(&self.value[..start_byte_index]);
		next_value.push_str(inserted_text);
		next_value.push_str(&self.value[end_byte_index..]);
		self.value = next_value;
		self.cursor_index = (start_index + count_characters(inserted_text)).min(count_characters(&self.value));
		self.selection_start = self.cursor_index;
	}

	//==============================================================================
	/// 글자 하나 지우기. (그 자리부터 글자 묶음 하나, 크롬 Delete 키처럼 이모지나 결합 부호를 쪼개지 않는다. 원본 deleteCharAt)
	///
	/// # 인자
	/// - `character_index`: 지울 글자의 자리. (문자 단위)
	//==============================================================================
	pub fn delete_character_at(&mut self, character_index: usize) {
		let end_index = find_next_grapheme_boundary(&self.value, character_index);
		self.delete_range(character_index, end_index);
	}

	//==============================================================================
	/// 글 범위 지우기. (문자 자리, 커서는 지운 자리로 당기고 선택은 접는다. 원본 deleteRange)
	///
	/// # 인자
	/// - `start_index`: 지울 범위의 앞. (문자 단위)
	/// - `end_index`: 지울 범위의 뒤. (문자 단위, 앞보다 크지 않으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn delete_range(&mut self, start_index: usize, end_index: usize) {
		let end_index = end_index.min(count_characters(&self.value));
		if end_index <= start_index {
			return;
		}
		let start_byte_index = convert_character_index_to_byte_index(&self.value, start_index);
		let end_byte_index = convert_character_index_to_byte_index(&self.value, end_index);
		self.value.replace_range(start_byte_index..end_byte_index, "");
		if self.cursor_index >= end_index {
			self.cursor_index -= end_index - start_index;
		}
		else if self.cursor_index > start_index {
			self.cursor_index = start_index;
		}
		self.selection_start = self.cursor_index;
	}

	//==============================================================================
	/// 뒤로 지우기. (선택이 있으면 선택을, 없으면 블링크 BackspaceStateMachine 이 정한 커서 앞 범위. 바뀌었으면 true)
	///
	/// # 돌려줌
	/// 지웠으면 true. 선택이 없고 커서가 글 앞이면 false.
	//==============================================================================
	pub fn delete_backward(&mut self) -> bool {
		if self.has_selection() {
			self.replace_selection("");
			return true;
		}
		if self.cursor_index > 0 {
			let start_index = find_backward_deletion_boundary(&self.value, self.cursor_index);
			self.delete_range(start_index, self.cursor_index);
			return true;
		}
		false
	}

	//==============================================================================
	/// 앞으로 지우기. (선택이 있으면 선택을, 없으면 커서 뒤 글자 묶음. 바뀌었으면 true)
	///
	/// # 돌려줌
	/// 지웠으면 true. 선택이 없고 커서가 글 끝이면 false.
	//==============================================================================
	pub fn delete_forward(&mut self) -> bool {
		if self.has_selection() {
			self.replace_selection("");
			return true;
		}
		if self.cursor_index < count_characters(&self.value) {
			self.delete_character_at(self.cursor_index);
			return true;
		}
		false
	}

	//==============================================================================
	/// 커서 옮기기. (글 안으로 자른다. 선택을 늘리지 않으면 선택을 접는다. 원본 moveCursor)
	///
	/// # 인자
	/// - `new_index`: 옮길 자리. (문자 단위, 0 ~ 값의 문자 수로 자른다)
	/// - `is_selection_extended`: 참이면 선택의 고정 끝을 두고 선택을 늘린다.
	//==============================================================================
	pub fn move_cursor(&mut self, new_index: usize, is_selection_extended: bool) {
		self.cursor_index = new_index.min(count_characters(&self.value));
		if !is_selection_extended {
			self.selection_start = self.cursor_index;
		}
		self.open_edit_kind = None;
	}

	//==============================================================================
	/// 모두 선택. (원본 selectAll)
	//==============================================================================
	pub fn select_all(&mut self) {
		self.selection_start = 0;
		self.cursor_index = count_characters(&self.value);
		self.open_edit_kind = None;
	}

	//==============================================================================
	/// 커서 앞 글.
	///
	/// # 돌려줌
	/// 값에서 커서 앞까지의 글.
	//==============================================================================
	pub fn get_text_before_cursor(&self) -> &str {
		let byte_index = convert_character_index_to_byte_index(&self.value, self.cursor_index);
		&self.value[..byte_index]
	}

	//==============================================================================
	/// 보이는 글. (커서 자리에 조합 글을 끼운 것. 원본 refreshTexts)
	///
	/// # 돌려줌
	/// 보이는 글.
	//==============================================================================
	pub fn build_display_text(&self) -> String {
		let byte_index = convert_character_index_to_byte_index(&self.value, self.cursor_index);
		let mut display_text = String::with_capacity(self.value.len() + self.composition.len());
		display_text.push_str(&self.value[..byte_index]);
		display_text.push_str(&self.composition);
		display_text.push_str(&self.value[byte_index..]);
		display_text
	}
}

//==============================================================================
/// 글자 경계 x 목록에서 누른 x 에 가까운 글자 자리 찾기. (원본 measureIndexAtLocalX 의 비교)
/// - `character_offsets[i]` 는 앞 i 글자의 폭이다. (0 번은 0)
///
/// # 인자
/// - `character_offsets`: 앞 i 글자의 폭 목록. (0 번은 0, 글자 수 + 1 개)
/// - `target_x`: 누른 x. (첫 글자 왼쪽이 0)
///
/// # 돌려줌
/// 가장 가까운 글자 경계의 자리. (0 ~ 글자 수)
//==============================================================================
pub fn find_character_index_at_offset(character_offsets: &[f32], target_x: f32) -> usize {
	let character_count = character_offsets.len().saturating_sub(1);
	if character_count == 0 || target_x <= 0.0 {
		return 0;
	}
	for character_index in 1..=character_count {
		let width = character_offsets[character_index];
		if width >= target_x {
			let previous_width = character_offsets[character_index - 1];
			if target_x - previous_width < width - target_x {
				return character_index - 1;
			}
			return character_index;
		}
	}
	character_count
}

//==============================================================================
/// 글자 묶음 경계 가운데 누른 x 에 가까운 자리 찾기. (크롬처럼 커서는 글자 묶음 경계에만 선다)
/// - `character_offsets[i]` 는 앞 i 글자의 폭이다. `boundary_list` 는 0 과 글자 수를 넣은 경계 자리 목록이다. (오름차순)
//==============================================================================
pub(crate) fn find_boundary_index_at_offset(character_offsets: &[f32], boundary_list: &[usize], target_x: f32) -> usize {
	let last_boundary = boundary_list.last().copied().unwrap_or(0);
	if boundary_list.len() < 2 || target_x <= 0.0 {
		return 0;
	}
	let read_offset = |boundary: usize| -> f32 {
		match character_offsets.get(boundary) {
			Some(character_offset) => {
				*character_offset
			}
			None => {
				character_offsets.last().copied().unwrap_or(0.0)
			}
		}
	};
	for boundary_pair in boundary_list.windows(2) {
		let width = read_offset(boundary_pair[1]);
		if width >= target_x {
			let previous_width = read_offset(boundary_pair[0]);
			if target_x - previous_width < width - target_x {
				return boundary_pair[0];
			}
			return boundary_pair[1];
		}
	}
	last_boundary
}

//==============================================================================
/// 이어 누른 수 세기. (크롬 aura ui/events/event.cc 의 MouseEvent::GetRepeatCount 와 IsRepeatedClickEvent)
/// - 앞 누름에서 시간이 interval_seconds 를 넘지 않고, 가로와 세로가 각각 slop 을 넘지 않으면 앞 수에 하나 더한다. 셋을 넘지 않는다.
///
/// # 인자
/// - `previous_click`: (앞 누름에서 지난 초, 앞 누름 자리, 앞 누름의 수). 없으면 None.
/// - `position`: 이번 누름 자리. (DIP)
/// - `interval_seconds`: 이어 누르기로 볼 시간. (초)
/// - `slop`: 이어 누르기로 볼 가로, 세로 거리. (DIP)
///
/// # 돌려줌
/// 이번 누름의 수. (1 ~ 3)
//==============================================================================
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn count_repeated_clicks(previous_click: Option<(f32, Vector2, u32)>, position: Vector2, interval_seconds: f32, slop: f32) -> u32 {
	match previous_click {
		Some((elapsed_seconds, previous_position, previous_count)) => {
			let is_repeated = elapsed_seconds <= interval_seconds && (position.x - previous_position.x).abs() <= slop && (position.y - previous_position.y).abs() <= slop;
			if is_repeated {
				(previous_count + 1).min(3)
			}
			else {
				1
			}
		}
		None => {
			1
		}
	}
}

//==============================================================================
/// 자리의 낱말 범위. (문자 자리, 블링크 selection_adjuster.cc 의 ExpandWithGranularity kWord)
/// - 낱말 경계는 크롬 낱말 나누기(collect_word_boundaries)다. 공백 덩어리와 줄바꿈도 한 조각이라, 공백을 누르면 공백을 고른다.
/// - 자리가 경계면 ChooseWordSide 대로 뒤 조각을 고르고, 글 끝이거나 폭으로 나뉜 줄의 끝(is_upstream)이면 앞 조각을 고른다.
///   그래서 문단 끝(줄바꿈 앞) 자리는 줄바꿈 하나를 고른다.
//==============================================================================
fn find_word_range_at(text: &str, character_index: usize, is_upstream: bool) -> (usize, usize) {
	let character_count = count_characters(text);
	let character_index = character_index.min(character_count);
	let is_previous_side = character_index == character_count || is_upstream;
	for boundary_pair in collect_word_boundaries(text).windows(2) {
		let (segment_start, segment_end) = (boundary_pair[0], boundary_pair[1]);
		let is_hit = if is_previous_side {
			segment_start < character_index && character_index <= segment_end
		}
		else {
			segment_start <= character_index && character_index < segment_end
		};
		if is_hit {
			return (segment_start, segment_end);
		}
	}
	(character_index, character_index)
}

//==============================================================================
/// 자리의 문단 범위. (문자 자리, 블링크 ExpandWithGranularity kParagraph: 문단 끝의 줄바꿈까지 넣는다. 글 끝의 빈 줄이면 앞 문단)
//==============================================================================
fn find_paragraph_range_at(text: &str, character_index: usize) -> (usize, usize) {
	let character_list: Vec<char> = text.chars().collect();
	let character_count = character_list.len();
	let character_index = character_index.min(character_count);
	let mut start_probe = character_index;
	if character_index == character_count && character_index > 0 && character_list[character_index - 1] == '\n' {
		start_probe = character_index - 1;
	}
	let start_index = match character_list[..start_probe].iter().rposition(|character| *character == '\n') {
		Some(line_break_index) => {
			line_break_index + 1
		}
		None => {
			0
		}
	};
	let end_index = match character_list[character_index..].iter().position(|character| *character == '\n') {
		Some(line_break_offset) => {
			character_index + line_break_offset + 1
		}
		None => {
			character_count
		}
	};
	(start_index, end_index)
}

//==============================================================================
/// 윈도우인지. (데스크탑은 빌드 대상, 웹은 원본처럼 navigator.platform 이 Win 으로 시작하는지)
//==============================================================================
fn is_windows_platform() -> bool {
	#[cfg(target_arch = "wasm32")]
	{
		web_sys::window().and_then(|window| window.navigator().platform().ok()).map(|platform| platform.starts_with("Win")).unwrap_or(false)
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		cfg!(target_os = "windows")
	}
}

//==============================================================================
/// 맥인지. (데스크탑은 빌드 대상, 웹은 navigator.platform 이 Mac 으로 시작하는지)
//==============================================================================
fn is_mac_platform() -> bool {
	#[cfg(target_arch = "wasm32")]
	{
		web_sys::window().and_then(|window| window.navigator().platform().ok()).map(|platform| platform.starts_with("Mac")).unwrap_or(false)
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		cfg!(target_os = "macos")
	}
}

//==============================================================================
/// 뒤 공백 건너뛰기. (블링크 visible_units.cc 의 SkipWhitespace: 공백과 NBSP 는 넘고 줄바꿈에서 멈춘다)
//==============================================================================
fn skip_trailing_whitespace(text: &str, character_index: usize) -> usize {
	let skipped_count = text.chars().skip(character_index).take_while(|character| *character != '\n' && (character.is_whitespace() || *character == '\u{A0}')).count();
	character_index + skipped_count
}

//==============================================================================
/// 입력 필드.
///
/// # 예제
/// 이름 입력 필드를 붙이고 눌러 포커스 주기
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, NodeTree, TouchKind, UIInputField, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let field_id = node_tree.create_child(root_id, "NameField");
/// node_tree[field_id].set_local_position(Vector2::create(100.0, 50.0));
/// node_tree[field_id].set_content_size(Vector2::create(300.0, 56.0));
/// let input_field = node_tree.add_component(field_id, UIInputField::new()).expect("input field");
/// input_field.set_use_dom_input(true);
/// input_field.set_placeholder("Name");
/// input_field.set_max_length(8);
/// input_field.set_text("Vanilla Engine");
/// assert_eq!(input_field.get_text(), "Vanilla ");
/// input_field.move_cursor(0, true);
/// assert_eq!(input_field.get_selection_range(), Some((0, 8)));
/// input_field.attach();
///
/// node_tree.dispatch_touch(field_id, &engine, TouchKind::Press, Vector2::create(150.0, 70.0));
/// assert!(node_tree[field_id].get_component::<UIInputField>().expect("input field").is_focused());
/// ```
//==============================================================================
pub struct UIInputField {
	ui_control: UIControl,
	field_id: u64,
	edit_buffer: TextEditBuffer,
	is_composing: bool,
	drag_start_index: Option<usize>,
	is_focused: bool,
	focus_serial: u64,
	blink_timer: f32,
	is_cursor_visible: bool,
	use_dom_input: bool,
	use_touch_end_focus: bool,
	use_direct_touch: bool,
	is_attached: bool,
	font_size: f32,
	font_family: String,
	padding: f32,
	placeholder: String,
	background_paint: Paint,
	cursor_color: Color,
	composition_underline_color: Color,
	focus_border_color: Color,
	focus_border_width: f32,
	selection_color: Color,
	value_text: Text,
	placeholder_text: Text,
	change_event: Option<UIInputFieldEvent>,
	submit_event: Option<UIInputFieldEvent>,
	character_offsets: Vec<f32>,
	measured_value: String,
	measured_font_string: String,
	caret_local_x: f32,
	value_revision: u64,
	context_menu_node_id: Option<NodeId>,
	pending_menu_action: Option<UIInputFieldMenuAction>,
	/// 글 라벨을 마지막으로 놓은 노드 크기. (원본 refreshTextPositions 가 tick 에서 크기가 있을 때만 놓는다. 처음은 0, UI 문서가 원본 자식 꼴로 적는다)
	text_layout_size: Vector2,
	/// 커서의 줄 위와 높이. (IME 후보 창 자리, 한 줄 필드는 0 과 노드 높이)
	caret_local_top: f32,
	caret_local_height: f32,
	/// 여러 줄인지. (크롬 textarea: Enter 가 줄을 넣고, 폭에 맞춰 나누고, 위아래로 옮기고, 넘치면 스크롤한다)
	is_multiline: bool,
	/// 여러 줄 배치와 그 열쇠. (보인 글, 폰트, 폭, 줄 높이, 탭 크기)
	text_area_layout: TextAreaLayout,
	text_area_layout_key: Option<(String, String, f32, f32, f32)>,
	/// 줄 높이. (None 이면 CSS normal, 크롬처럼 첫째 글꼴의 위아래 길이와 줄 사이로 셈한다)
	line_height: Option<f32>,
	tab_size: f32,
	/// 위로 굴려 넘긴 만큼. (원본 textarea scrollTop)
	scroll_top: f32,
	/// 클래식 스크롤 막대가 차지한 폭. (여러 줄 필드, 얹는 막대거나 넘치지 않으면 0)
	text_area_scrollbar_gutter: f32,
	/// 위아래 이동이 지키는 가로 자리. (블링크 x_pos_for_vertical_arrow_navigation, 다른 선택 바뀜에 지운다)
	preferred_caret_x: Option<f32>,
	/// 폭으로 나뉜 자리의 커서를 앞 줄 끝에 보일지. (블링크의 위쪽 친화)
	is_caret_upstream: bool,
	/// 다음 배치에서 커서가 보이게 스크롤할지. (블링크 RevealSelection)
	caret_reveal_request: Option<CaretRevealKind>,
	/// 썸을 끄는 중이면 (누른 y, 그때 scroll_top).
	scrollbar_drag_start: Option<(f32, f32)>,
	/// 손가락 길게 누르기. (데스크탑, DOM 입력을 직접 누르게 한 필드)
	long_press_state: Option<LongPressState>,
	/// 터치 선택 손잡이를 보이는지와 손잡이 끝점들. ((노드 좌표 끝점, 앞인지, 커서 손잡이인지), 그릴 때 둔다)
	is_touch_handle_visible: bool,
	touch_handle_list: Vec<(Vector2, bool, bool)>,
	dragging_touch_handle: Option<TouchHandleDrag>,
	/// 빠른 메뉴. (크롬 quick_menu_requested_, 뗀 뒤 남은 시간, 띄운 노드)
	is_quick_menu_requested: bool,
	quick_menu_delay: Option<f32>,
	quick_menu_node_id: Option<NodeId>,
	#[cfg(not(target_arch = "wasm32"))]
	pending_quick_menu_command: Option<crate::ui::uiinputfieldmenu::QuickMenuCommand>,
	/// 노드 한 단위의 창 픽셀. (손잡이와 빠른 메뉴 크기)
	window_pixel_per_unit: f32,
	/// 지난 마우스 누름. ((시각, 창 DIP 자리, 이어 누른 수), 데스크탑이 이어 누른 수를 센다)
	#[cfg(not(target_arch = "wasm32"))]
	last_click: Option<(web_time::Instant, Vector2, u32)>,
	/// 두 번, 세 번 누른 뒤 끌기의 단위와 처음 고른 범위. (블링크 SelectionController 의 낱말, 문단 단위 끌기)
	drag_granularity: Option<(SelectionGranularity, usize, usize)>,
}

//==============================================================================
/// 끌어 넓히는 단위. (두 번 누르면 낱말, 세 번 누르면 문단)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionGranularity {
	Word,
	Paragraph,
}

//==============================================================================
/// 손가락 길게 누르기 상태. (누른 뷰 자리, 지난 시간, 길게 누르기를 했는지)
//==============================================================================
#[derive(Clone, Copy, Debug)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct LongPressState {
	start_view_position: Vector2,
	elapsed: f32,
	is_fired: bool,
}

//==============================================================================
/// 끄는 터치 손잡이. (앞 손잡이인지, 누른 자리에서 손잡이 끝점까지의 노드 좌표 차이)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct TouchHandleDrag {
	is_start_handle: bool,
	is_caret_handle: bool,
	focus_offset: Vector2,
}

//==============================================================================
/// 커서가 보이게 스크롤하는 꼴. (블링크 ScrollAlignment: 맥이 아니면 가장자리에 붙이고 page 이동은 맨 위로, 맥은 가운데로)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaretRevealKind {
	IfNeeded,
	Always,
}

#[crate::reflect]
impl UIInputField {
	//==============================================================================
	/// 생성. (기본값은 원본과 같다: 28px, 여백 16, 흰 바탕 둥글기 8, 글 #222222, 안내 글 #999999, 커서 #5b8def, 테두리 #f97316 3px)
	///
	/// # 돌려줌
	/// 새 입력 필드. (값과 안내 글이 비었고 포커스가 없다)
	//==============================================================================
	pub fn new() -> UIInputField {
		let font_size = 28.0;
		let mut background_paint = Paint::create_with_color("#ffffff");
		background_paint.set_round_size(8.0);
		let mut value_text = Text::create_with_text("", font_size);
		value_text.set_text_align(TextAlign::Left);
		value_text.set_text_baseline(TextBaseline::Middle);
		value_text.set_text_color("#222222");
		let mut placeholder_text = Text::create_with_text("", font_size);
		placeholder_text.set_text_align(TextAlign::Left);
		placeholder_text.set_text_baseline(TextBaseline::Middle);
		placeholder_text.set_text_color("#999999");
		UIInputField {
			ui_control: UIControl::new(),
			field_id: NEXT_FIELD_ID.fetch_add(1, Ordering::Relaxed),
			edit_buffer: TextEditBuffer::new(),
			is_composing: false,
			drag_start_index: None,
			is_focused: false,
			focus_serial: 0,
			blink_timer: 0.0,
			is_cursor_visible: true,
			use_dom_input: false,
			use_touch_end_focus: false,
			use_direct_touch: false,
			is_attached: false,
			font_size,
			font_family: String::from(DEFAULT_FONT_FAMILY),
			padding: 16.0,
			placeholder: String::new(),
			background_paint,
			cursor_color: Color::create_from_hex("#5b8def"),
			composition_underline_color: Color::create_from_hex("#5b8def"),
			focus_border_color: Color::create_from_hex("#f97316"),
			focus_border_width: 3.0,
			selection_color: Color::create(91.0 / 255.0, 141.0 / 255.0, 239.0 / 255.0, 0.35),
			value_text,
			placeholder_text,
			change_event: None,
			submit_event: None,
			character_offsets: Vec::new(),
			measured_value: String::new(),
			measured_font_string: String::new(),
			caret_local_x: 16.0,
			value_revision: 0,
			context_menu_node_id: None,
			pending_menu_action: None,
			text_layout_size: Vector2::zero(),
			caret_local_top: 0.0,
			caret_local_height: 0.0,
			is_multiline: false,
			text_area_layout: TextAreaLayout::default(),
			text_area_layout_key: None,
			line_height: None,
			tab_size: DEFAULT_TAB_SIZE,
			scroll_top: 0.0,
			text_area_scrollbar_gutter: 0.0,
			preferred_caret_x: None,
			is_caret_upstream: false,
			caret_reveal_request: None,
			scrollbar_drag_start: None,
			long_press_state: None,
			is_touch_handle_visible: false,
			touch_handle_list: Vec::new(),
			dragging_touch_handle: None,
			is_quick_menu_requested: false,
			quick_menu_delay: None,
			quick_menu_node_id: None,
			#[cfg(not(target_arch = "wasm32"))]
			pending_quick_menu_command: None,
			window_pixel_per_unit: 1.0,
			#[cfg(not(target_arch = "wasm32"))]
			last_click: None,
			drag_granularity: None,
		}
	}

	//==============================================================================
	/// 글 라벨을 마지막으로 놓은 노드 크기. (UI 문서가 원본 글 라벨과 오버레이 자식의 자리와 크기로 적는다, 아직 놓지 않았으면 0)
	//==============================================================================
	pub(crate) fn get_text_layout_size(&self) -> Vector2 {
		self.text_layout_size
	}

	//==============================================================================
	/// 글 라벨을 마지막으로 놓은 노드 크기 정하기. (UI 문서를 읽을 때)
	//==============================================================================
	pub(crate) fn set_text_layout_size(&mut self, text_layout_size: Vector2) {
		self.text_layout_size = text_layout_size;
	}

	//==============================================================================
	/// 품은 컨트롤 반환.
	///
	/// # 돌려줌
	/// 품은 UIControl.
	//==============================================================================
	pub fn get_uicontrol(&self) -> &UIControl {
		&self.ui_control
	}

	//==============================================================================
	/// 품은 컨트롤 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 UIControl.
	//==============================================================================
	pub fn get_uicontrol_mut(&mut self) -> &mut UIControl {
		&mut self.ui_control
	}

	//==============================================================================
	/// 컨텐트 노드 핸들.
	///
	/// # 돌려줌
	/// 호스트 노드 밑에 만든 컨텐트 노드. 트리에 붙기 전이면 None.
	//==============================================================================
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_control.get_content()
	}

	//==============================================================================
	/// 필드 id. (엔진 글 입력 줄에서 이 필드를 가리키는 값)
	///
	/// # 돌려줌
	/// 필드 id. (1 부터)
	//==============================================================================
	pub fn get_field_id(&self) -> u64 {
		self.field_id
	}

	//==============================================================================
	/// 플랫폼 글 입력 사용 설정. (원본 setUseDOMInput. 켜야 한글 IME, 모바일 키보드, 웹 시스템 붙여넣기가 된다. 기본 꺼짐)
	///
	/// # 인자
	/// - `use_dom_input`: true 면 플랫폼 글 입력으로 받는다. (웹은 숨은 DOM 입력, 데스크탑은 IME)
	//==============================================================================
	pub fn set_use_dom_input(&mut self, use_dom_input: bool) {
		self.use_dom_input = use_dom_input;
	}

	//==============================================================================
	/// 플랫폼 글 입력 사용 여부.
	///
	/// # 돌려줌
	/// DOM 입력을 쓰면 참.
	//==============================================================================
	pub fn is_using_dom_input(&self) -> bool {
		self.use_dom_input
	}

	//==============================================================================
	/// 손을 뗄 때도 포커스를 줄지 설정. (웹 DOM 입력 전용. 첫 터치에 키보드를 띄우지 않는 브라우저용, 기본 꺼짐)
	///
	/// # 인자
	/// - `use_touch_end_focus`: 참이면 touchend 에서도 포커스를 준다. (기본 거짓)
	//==============================================================================
	pub fn set_use_touch_end_focus(&mut self, use_touch_end_focus: bool) {
		self.use_touch_end_focus = use_touch_end_focus;
	}

	//==============================================================================
	/// 손을 뗄 때도 포커스를 주는지.
	///
	/// # 돌려줌
	/// touchend 에서도 포커스를 주면 참.
	//==============================================================================
	pub fn is_using_touch_end_focus(&self) -> bool {
		self.use_touch_end_focus
	}

	//==============================================================================
	/// DOM 입력을 필드 위에서 직접 누르게 할지 설정. (use_dom_input 을 켠 필드에서 뜻이 있다, 기본 꺼짐)
	/// - 웹: 원본처럼 투명한 DOM 입력이 필드 자리에서 누름을 받아, 브라우저 기본 동작으로 포커스와 키보드를 연다. (코드로 준 포커스를 받지 않는 웹뷰용)
	///   그 필드의 오른쪽 단추는 원본처럼 브라우저 편집 칸 메뉴 대신 다른 필드와 같은 엔진 문맥 메뉴(잘라내기, 복사, 붙여넣기, 모두 선택)다.
	///   손가락 길게 누르기는 크롬이 그 DOM 입력에 띄우는 것이다.
	/// - 데스크탑: 원본에서 크롬이 그 DOM 입력에 하는 일을 엔진이 한다. 오른쪽 단추는 같은 엔진 문맥 메뉴이고,
	///   손가락으로 650ms 길게 누르면(15 DIP 안) 낱말을 고르고 터치 손잡이와 빠른 메뉴를 띄운다. (크롬 데스크탑 터치, 윈도우는 뗄 때 메뉴를 정한다)
	///
	/// # 인자
	/// - `use_direct_touch`: 참이면 필드 자리의 누름을 DOM 입력이 직접 받는다. (데스크탑은 그 DOM 입력처럼 길게 누르기를 한다, 기본 거짓)
	//==============================================================================
	pub fn set_use_direct_touch(&mut self, use_direct_touch: bool) {
		self.use_direct_touch = use_direct_touch;
	}

	//==============================================================================
	/// DOM 입력을 직접 누르게 하는지.
	///
	/// # 돌려줌
	/// DOM 입력이 누름을 직접 받으면 참.
	//==============================================================================
	pub fn is_using_direct_touch(&self) -> bool {
		self.use_direct_touch
	}

	//==============================================================================
	/// 여러 줄 설정. (크롬 textarea 처럼. 웹 DOM 입력은 숨긴 `<textarea>` 를 쓴다. 원본 필드의 속성이 아니라 반사에서 뺀다)
	///
	/// # 인자
	/// - `is_multiline`: true 면 여러 줄, false 면 한 줄. (처음 false, 바꾸면 스크롤을 처음으로)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_multiline(&mut self, is_multiline: bool) {
		self.is_multiline = is_multiline;
		self.text_area_layout_key = None;
		self.scroll_top = 0.0;
		self.preferred_caret_x = None;
		self.is_caret_upstream = false;
	}

	//==============================================================================
	/// 여러 줄인지.
	///
	/// # 돌려줌
	/// 여러 줄이면 true.
	//==============================================================================
	#[reflect(skip)]
	pub fn is_multiline(&self) -> bool {
		self.is_multiline
	}

	//==============================================================================
	/// 줄 높이 설정. (여러 줄 필드, None 이면 CSS normal 이다. 크롬 SimpleFontData 처럼 첫째 글꼴의 위아래 길이와 줄 사이를 반올림해 더한다)
	///
	/// # 인자
	/// - `line_height`: 줄 높이. (내용 크기 단위, None 이면 CSS normal)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_line_height(&mut self, line_height: Option<f32>) {
		self.line_height = line_height;
		self.text_area_layout_key = None;
	}

	//==============================================================================
	/// 줄 높이. (여러 줄 필드)
	///
	/// # 돌려줌
	/// 줄 높이. (정하지 않았으면 CSS normal 로 잰 값, 글꼴을 하나도 읽지 못했으면 글꼴 크기의 1.2 배를 반올림한 값)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_line_height(&self) -> f32 {
		match self.line_height {
			Some(line_height) => {
				line_height
			}
			None => {
				// 글꼴을 하나도 읽지 못했으면(글자 캐시가 없는 시험 같은 때) 글꼴 크기의 1.2 배로 갈음한다.
				crate::core::textstringtexturecache::measure_normal_line_height_without_graphic(&self.build_font_string()).unwrap_or_else(|| (self.font_size * 1.2).round())
			}
		}
	}

	//==============================================================================
	/// 탭 크기 설정. (여러 줄 필드, 공백 몇 개 폭. CSS tab-size, 기본 8)
	///
	/// # 인자
	/// - `tab_size`: 탭 크기. (공백 몇 개 폭, 음수는 0, 기본 8)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_tab_size(&mut self, tab_size: f32) {
		self.tab_size = tab_size.max(0.0);
		self.text_area_layout_key = None;
	}

	//==============================================================================
	/// 탭 크기.
	///
	/// # 돌려줌
	/// 탭 크기. (공백 몇 개 폭)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_tab_size(&self) -> f32 {
		self.tab_size
	}

	//==============================================================================
	/// 위로 굴려 넘긴 만큼. (여러 줄 필드, 원본 textarea scrollTop)
	///
	/// # 돌려줌
	/// 위로 넘긴 만큼. (내용 크기 단위)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_scroll_top(&self) -> f32 {
		self.scroll_top
	}

	//==============================================================================
	/// 위로 굴려 넘기기. (여러 줄 필드, 다음 그리기에서 글 끝 안으로 줄인다)
	///
	/// # 인자
	/// - `scroll_top`: 위로 넘길 만큼. (내용 크기 단위, 음수는 0)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_scroll_top(&mut self, scroll_top: f32) {
		self.scroll_top = scroll_top.max(0.0);
	}

	//==============================================================================
	/// 되돌릴 것이 있는지.
	///
	/// # 돌려줌
	/// 되돌릴 묶음이 있으면 true.
	//==============================================================================
	pub fn can_undo(&self) -> bool {
		self.edit_buffer.can_undo()
	}

	//==============================================================================
	/// 다시 할 것이 있는지.
	///
	/// # 돌려줌
	/// 다시 할 묶음이 있으면 true.
	//==============================================================================
	pub fn can_redo(&self) -> bool {
		self.edit_buffer.can_redo()
	}

	//==============================================================================
	/// 값 설정. (max_length 로 자르고 커서는 끝에. 바뀜 이벤트는 부르지 않는다. 원본 setText)
	///
	/// # 인자
	/// - `text`: 넣을 글. (되돌리기와 다시 하기 목록도 비운다)
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		self.edit_buffer.set_text(text);
		self.bump_value_revision();
	}

	//==============================================================================
	/// 값 반환.
	///
	/// # 돌려줌
	/// 입력한 글. (조합 중인 글은 빠진다)
	//==============================================================================
	pub fn get_text(&self) -> &str {
		self.edit_buffer.get_text()
	}

	//==============================================================================
	/// 보이는 글 반환. (값의 커서 자리에 조합 글을 끼운 것)
	///
	/// # 돌려줌
	/// 보이는 글.
	//==============================================================================
	pub fn get_display_text(&self) -> String {
		self.edit_buffer.build_display_text()
	}

	//==============================================================================
	/// 조합 중인 글 반환.
	///
	/// # 돌려줌
	/// IME 가 조합 중인 글. (없으면 빈 글)
	//==============================================================================
	pub fn get_composition(&self) -> &str {
		self.edit_buffer.get_composition()
	}

	//==============================================================================
	/// IME 조합 중인지.
	///
	/// # 돌려줌
	/// 조합 중이면 참.
	//==============================================================================
	pub fn is_composing(&self) -> bool {
		self.is_composing
	}

	//==============================================================================
	/// 안내 글 설정. (값이 비었을 때 보인다)
	///
	/// # 인자
	/// - `placeholder`: 값이 비었을 때 보일 글.
	//==============================================================================
	pub fn set_placeholder(&mut self, placeholder: &str) {
		self.placeholder = placeholder.to_string();
	}

	//==============================================================================
	/// 안내 글 반환.
	///
	/// # 돌려줌
	/// 플레이스홀더 글.
	//==============================================================================
	pub fn get_placeholder(&self) -> &str {
		&self.placeholder
	}

	//==============================================================================
	/// 최대 글자 수 설정. (원본처럼 UTF-16 단위, 0 이면 제한 없음)
	///
	/// # 인자
	/// - `max_length`: 최대 글자 수. (UTF-16 단위, 0 이면 제한하지 않는다, 지금 값은 자르지 않는다)
	//==============================================================================
	pub fn set_max_length(&mut self, max_length: usize) {
		self.edit_buffer.set_max_length(max_length);
		self.bump_value_revision();
	}

	//==============================================================================
	/// 최대 글자 수 반환.
	///
	/// # 돌려줌
	/// 최대 글자 수. (0 이면 제한하지 않는다)
	//==============================================================================
	pub fn get_max_length(&self) -> usize {
		self.edit_buffer.get_max_length()
	}

	//==============================================================================
	/// 글자 크기 설정.
	///
	/// # 인자
	/// - `font_size`: 글자 크기. (내용 크기 단위, 기본 28)
	//==============================================================================
	pub fn set_font_size(&mut self, font_size: f32) {
		self.font_size = font_size;
		self.value_text.set_font_size(font_size);
		self.placeholder_text.set_font_size(font_size);
	}

	//==============================================================================
	/// 글자 크기 반환.
	///
	/// # 돌려줌
	/// 글자 크기.
	//==============================================================================
	pub fn get_font_size(&self) -> f32 {
		self.font_size
	}

	//==============================================================================
	/// 폰트 이름 설정. (원본 setFont 자리. Graphic 에 등록한 이름, 없으면 기본 폰트)
	///
	/// # 인자
	/// - `font_family`: Graphic 에 등록한 폰트 이름. (처음 "default", 엔진 기본 폰트)
	//==============================================================================
	pub fn set_font_family(&mut self, font_family: &str) {
		self.font_family = font_family.to_string();
		self.value_text.set_font_family(font_family);
		self.placeholder_text.set_font_family(font_family);
	}

	//==============================================================================
	/// 폰트 이름 반환.
	///
	/// # 돌려줌
	/// 폰트 이름. (처음 "default")
	//==============================================================================
	pub fn get_font_family(&self) -> &str {
		&self.font_family
	}

	//==============================================================================
	/// 안쪽 여백 설정. (글 왼쪽 여백)
	///
	/// # 인자
	/// - `padding`: 왼쪽과 오른쪽 안쪽 여백. (내용 크기 단위, 기본 16, 여러 줄 필드는 위아래에도)
	//==============================================================================
	pub fn set_padding(&mut self, padding: f32) {
		self.padding = padding;
	}

	//==============================================================================
	/// 안쪽 여백 반환.
	///
	/// # 돌려줌
	/// 왼쪽과 오른쪽 안쪽 여백.
	//==============================================================================
	pub fn get_padding(&self) -> f32 {
		self.padding
	}

	//==============================================================================
	/// 바탕색 설정.
	///
	/// # 인자
	/// - `color`: 배경색. (기본 흰색)
	//==============================================================================
	pub fn set_background_color(&mut self, color: impl Into<Color>) {
		self.background_paint.set_color(color);
	}

	//==============================================================================
	/// 바탕색 반환.
	///
	/// # 돌려줌
	/// 배경색.
	//==============================================================================
	pub fn get_background_color(&self) -> Color {
		self.background_paint.get_color()
	}

	//==============================================================================
	/// 바탕 둥글기 설정. (포커스 테두리도 따른다)
	///
	/// # 인자
	/// - `round_size`: 배경과 포커스 테두리의 모서리 반지름. (기본 8)
	//==============================================================================
	pub fn set_round_size(&mut self, round_size: f32) {
		self.background_paint.set_round_size(round_size);
	}

	//==============================================================================
	/// 바탕 둥글기 반환.
	///
	/// # 돌려줌
	/// 배경의 모서리 반지름.
	//==============================================================================
	pub fn get_round_size(&self) -> f32 {
		self.background_paint.get_round_size()
	}

	//==============================================================================
	/// 글자 색 설정.
	///
	/// # 인자
	/// - `color`: 값 글자의 색. (기본 #222222)
	//==============================================================================
	pub fn set_text_color(&mut self, color: impl Into<Color>) {
		self.value_text.set_text_color(color);
	}

	//==============================================================================
	/// 글자 색 반환.
	///
	/// # 돌려줌
	/// 값 글자의 색.
	//==============================================================================
	pub fn get_text_color(&self) -> Color {
		self.value_text.get_text_color()
	}

	//==============================================================================
	/// 안내 글 색 설정.
	///
	/// # 인자
	/// - `color`: 플레이스홀더 글자의 색. (기본 #999999)
	//==============================================================================
	pub fn set_placeholder_color(&mut self, color: impl Into<Color>) {
		self.placeholder_text.set_text_color(color);
	}

	//==============================================================================
	/// 안내 글 색 반환.
	///
	/// # 돌려줌
	/// 플레이스홀더 글자의 색.
	//==============================================================================
	pub fn get_placeholder_color(&self) -> Color {
		self.placeholder_text.get_text_color()
	}

	//==============================================================================
	/// 커서 색 설정. (조합 밑줄도 같은 색. 원본과 같음)
	///
	/// # 인자
	/// - `color`: 커서와 조합 밑줄의 색. (기본 #5b8def)
	//==============================================================================
	pub fn set_cursor_color(&mut self, color: impl Into<Color>) {
		let color = color.into();
		self.cursor_color = color;
		self.composition_underline_color = color;
	}

	//==============================================================================
	/// 커서 색 반환.
	///
	/// # 돌려줌
	/// 커서 색.
	//==============================================================================
	pub fn get_cursor_color(&self) -> Color {
		self.cursor_color
	}

	//==============================================================================
	/// 포커스 테두리 색 설정.
	///
	/// # 인자
	/// - `color`: 포커스 테두리 색. (기본 #f97316)
	//==============================================================================
	pub fn set_focus_border_color(&mut self, color: impl Into<Color>) {
		self.focus_border_color = color.into();
	}

	//==============================================================================
	/// 포커스 테두리 두께 설정. (0 이면 그리지 않는다)
	///
	/// # 인자
	/// - `focus_border_width`: 포커스 테두리 두께. (0 이하면 그리지 않는다, 기본 3)
	//==============================================================================
	pub fn set_focus_border_width(&mut self, focus_border_width: f32) {
		self.focus_border_width = focus_border_width;
	}

	//==============================================================================
	/// 선택 영역 색 설정.
	///
	/// # 인자
	/// - `color`: 선택 영역을 칠할 색. (기본 알파 0.35 의 파랑)
	//==============================================================================
	pub fn set_selection_color(&mut self, color: impl Into<Color>) {
		self.selection_color = color.into();
	}

	//==============================================================================
	/// 바뀜 이벤트 설정. (사용자 입력으로 값이 바뀔 때. 원본 setOnChange)
	///
	/// # 인자
	/// - `change_event`: 사용자 입력으로 값이 바뀔 때마다 부를 함수. (필드와 문맥을 받는다, 값은 `field.get_text()`, set_text 로 바꿀 때는 부르지 않는다)
	//==============================================================================
	pub fn set_on_change(&mut self, change_event: UIInputFieldEvent) {
		self.change_event = Some(change_event);
	}

	//==============================================================================
	/// 제출 이벤트 설정. (Enter. 원본 setOnSubmit)
	///
	/// # 인자
	/// - `submit_event`: 포커스된 채 Enter 를 누르면 부를 함수. (한 줄 필드만, 필드와 문맥을 받는다, 값은 `field.get_text()`)
	//==============================================================================
	pub fn set_on_submit(&mut self, submit_event: UIInputFieldEvent) {
		self.submit_event = Some(submit_event);
	}

	//==============================================================================
	/// 입력 받기 시작. (원본 attach: DOM 입력 또는 캔버스 키보드, 문맥 메뉴)
	//==============================================================================
	pub fn attach(&mut self) {
		self.is_attached = true;
		self.bump_value_revision();
	}

	//==============================================================================
	/// 입력 받기 끝. (원본 detach. DOM 입력을 쓰던 필드는 포커스와 조합 글도 잃는다)
	//==============================================================================
	pub fn detach(&mut self) {
		self.is_attached = false;
		if self.use_dom_input {
			self.is_focused = false;
			self.focus_serial = 0;
			self.is_composing = false;
			self.edit_buffer.set_composition("");
		}
	}

	//==============================================================================
	/// 입력을 받기로 했는지.
	///
	/// # 돌려줌
	/// attach 뒤 detach 전이면 true.
	//==============================================================================
	pub fn is_attached(&self) -> bool {
		self.is_attached
	}

	//==============================================================================
	/// 포커스 주기. (원본 focus. 플랫폼 입력은 다음 프레임에 이 필드로 온다)
	//==============================================================================
	pub fn focus(&mut self) {
		self.handle_focus();
	}

	//==============================================================================
	/// 포커스 빼기. (원본 blur. 조합 중이던 글은 확정한다. 씬 코드에서 부르면 바뀜 이벤트는 부르지 않는다)
	//==============================================================================
	pub fn blur(&mut self) {
		self.apply_blur();
	}

	//==============================================================================
	/// 포커스를 가졌는지.
	///
	/// # 돌려줌
	/// 포커스를 가졌으면 참.
	//==============================================================================
	pub fn is_focused(&self) -> bool {
		self.is_focused
	}

	//==============================================================================
	/// 모두 선택.
	//==============================================================================
	pub fn select_all(&mut self) {
		self.edit_buffer.select_all();
		self.bump_value_revision();
	}

	//==============================================================================
	/// 선택이 있는지.
	///
	/// # 돌려줌
	/// 선택한 글이 있으면 참.
	//==============================================================================
	pub fn has_selection(&self) -> bool {
		self.edit_buffer.has_selection()
	}

	//==============================================================================
	/// 선택 범위. (앞, 뒤. 원본처럼 UTF-16 단위, 없으면 None)
	///
	/// # 돌려줌
	/// 선택 범위 (앞, 뒤). (UTF-16 단위, 앞이 작은 쪽, 선택이 없으면 None)
	//==============================================================================
	pub fn get_selection_range(&self) -> Option<(usize, usize)> {
		let (start_index, end_index) = self.edit_buffer.get_selection_range()?;
		Some((self.convert_to_utf16_index(start_index), self.convert_to_utf16_index(end_index)))
	}

	//==============================================================================
	/// 선택된 글.
	///
	/// # 돌려줌
	/// 선택한 글. (없으면 빈 글)
	//==============================================================================
	pub fn get_selection_text(&self) -> String {
		self.edit_buffer.get_selection_text()
	}

	//==============================================================================
	/// 커서 자리. (원본처럼 UTF-16 단위)
	///
	/// # 돌려줌
	/// 커서 자리. (값 안의 글자 위치, 0 ~ 값 길이)
	//==============================================================================
	pub fn get_cursor_index(&self) -> usize {
		self.convert_to_utf16_index(self.edit_buffer.get_cursor_index())
	}

	//==============================================================================
	/// 선택 시작 자리. (원본처럼 UTF-16 단위)
	///
	/// # 돌려줌
	/// 선택의 고정 끝. (커서 자리와 같으면 선택이 없다)
	//==============================================================================
	pub fn get_selection_start(&self) -> usize {
		self.convert_to_utf16_index(self.edit_buffer.get_selection_start())
	}

	//==============================================================================
	/// 커서 옮기기. (원본 moveCursor, 자리는 UTF-16 단위)
	///
	/// # 인자
	/// - `new_index`: 옮길 자리. (UTF-16 단위, 0 ~ 값 길이로 자른다, 서로게이트 쌍 가운데면 그 글자 뒤로 본다)
	/// - `is_selection_extended`: 참이면 선택의 고정 끝을 두고 선택을 늘린다.
	//==============================================================================
	pub fn move_cursor(&mut self, new_index: usize, is_selection_extended: bool) {
		let character_index = convert_utf16_index_to_character_index(self.edit_buffer.get_text(), new_index);
		self.move_cursor_to_character(character_index, is_selection_extended);
	}

	//==============================================================================
	/// 커서를 문자 자리로 옮기기. (편집 키와 누르기가 쓴다)
	//==============================================================================
	fn move_cursor_to_character(&mut self, character_index: usize, is_selection_extended: bool) {
		self.edit_buffer.move_cursor(character_index, is_selection_extended);
		self.bump_value_revision();
	}

	//==============================================================================
	/// 값의 문자 자리 → UTF-16 자리. (공개 함수가 원본처럼 UTF-16 단위로 돌려줄 때)
	//==============================================================================
	fn convert_to_utf16_index(&self, character_index: usize) -> usize {
		convert_character_index_to_utf16_index(self.edit_buffer.get_text(), character_index)
	}

	//==============================================================================
	/// 선택을 글로 바꾸고 바뀜 이벤트 부르기. (원본 replaceSelection)
	///
	/// # 인자
	/// - `context`: 바뀜 이벤트에 넘길 컴포넌트 문맥.
	/// - `text`: 넣을 글. (빈 글이면 선택을 지운다)
	//==============================================================================
	pub fn replace_selection(&mut self, context: &mut ComponentContext, text: &str) {
		self.replace_selection_with_edit(context, text, TextEditKind::InsertText);
	}

	//==============================================================================
	/// 선택을 글로 바꾸기. (되돌리기 묶음 종류와 함께)
	//==============================================================================
	fn replace_selection_with_edit(&mut self, context: &mut ComponentContext, text: &str, edit_kind: TextEditKind) {
		self.edit_buffer.begin_edit(edit_kind);
		self.edit_buffer.replace_selection(text);
		self.bump_value_revision();
		self.fire_change_event(context);
	}

	//==============================================================================
	/// 되돌리기나 다시 하기. (했으면 바뀜 이벤트)
	//==============================================================================
	fn apply_edit_history(&mut self, context: &mut ComponentContext, is_redo: bool) {
		let is_changed = if is_redo {
			self.edit_buffer.redo()
		}
		else {
			self.edit_buffer.undo()
		};
		if is_changed {
			self.bump_value_revision();
			self.fire_change_event(context);
		}
	}

	//==============================================================================
	/// 잘라내기. (선택을 클립보드에 쓰고 지운다)
	///
	/// # 인자
	/// - `context`: 클립보드를 쓸 엔진과 바뀜 이벤트에 넘길 컴포넌트 문맥.
	//==============================================================================
	pub fn cut(&mut self, context: &mut ComponentContext) {
		if !self.has_selection() {
			return;
		}
		let selection_text = self.get_selection_text();
		context.get_engine().write_clipboard_text(&selection_text);
		self.replace_selection_with_edit(context, "", TextEditKind::Paste);
	}

	//==============================================================================
	/// 복사. (선택을 클립보드에 쓴다)
	///
	/// # 인자
	/// - `engine`: 클립보드를 쓸 엔진.
	//==============================================================================
	pub fn copy(&self, engine: &Engine) {
		if !self.has_selection() {
			return;
		}
		engine.write_clipboard_text(&self.get_selection_text());
	}

	//==============================================================================
	/// 붙여넣기. (데스크탑은 클립보드를 그 자리에서 읽는다. 웹은 원본처럼 비동기로 읽고, 다 읽으면 이 필드의 글 입력 줄로 넣어 다음 tick 에 붙인다)
	///
	/// # 인자
	/// - `context`: 클립보드를 읽을 엔진과 바뀜 이벤트에 넘길 컴포넌트 문맥. (웹은 쓰지 않는다)
	//==============================================================================
	pub fn paste(&mut self, context: &mut ComponentContext) {
		#[cfg(target_arch = "wasm32")]
		{
			let _ = context;
			crate::platform::request_clipboard_paste(self.field_id);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			self.paste_now(context);
		}
	}

	//==============================================================================
	/// 클립보드를 그 자리에서 읽어 붙여넣기. (데스크탑)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn paste_now(&mut self, context: &mut ComponentContext) {
		let clipboard_text = context.get_engine().read_clipboard_text();
		if let Some(clipboard_text) = clipboard_text {
			let clipboard_text = self.sanitize_inserted_text(&clipboard_text);
			if !clipboard_text.is_empty() {
				self.replace_selection_with_edit(context, &clipboard_text, TextEditKind::Paste);
				self.request_caret_reveal(CaretRevealKind::IfNeeded);
			}
		}
	}

	//==============================================================================
	/// 문맥 메뉴 항목 부탁. (문맥 메뉴가 부른다. 다음 tick 에 이 필드의 문맥으로 한다)
	///
	/// # 인자
	/// - `menu_action`: 할 항목. (UIInputFieldMenuAction 의 값)
	//==============================================================================
	pub fn request_menu_action(&mut self, menu_action: UIInputFieldMenuAction) {
		self.pending_menu_action = Some(menu_action);
	}

	//==============================================================================
	/// 필드 쪽에서 값이나 커서를 바꿨다고 표시. (웹이 숨긴 <input> 에 다시 쓴다)
	//==============================================================================
	fn bump_value_revision(&mut self) {
		self.value_revision = self.value_revision.wrapping_add(1);
	}

	//==============================================================================
	/// 커서 깜빡임 처음으로.
	//==============================================================================
	fn reset_cursor_blink(&mut self) {
		self.blink_timer = 0.0;
		self.is_cursor_visible = true;
	}

	//==============================================================================
	/// 값과 선택이 웹 DOM 을 따라가는지. (웹에서 attach 한 DOM 입력 필드)
	//==============================================================================
	fn is_dom_synchronized(&self) -> bool {
		cfg!(target_arch = "wasm32") && self.use_dom_input && self.is_attached
	}

	//==============================================================================
	/// 포커스 얻음. (원본 handleFocus)
	//==============================================================================
	fn handle_focus(&mut self) {
		self.is_focused = true;
		self.focus_serial = NEXT_FOCUS_SERIAL.fetch_add(1, Ordering::Relaxed);
		self.reset_cursor_blink();
	}

	//==============================================================================
	/// 포커스 잃음. (조합 글을 확정했으면 그 글을 돌려준다. 원본 handleBlur)
	/// - 웹 DOM 입력은 브라우저가 확정해 값으로 보내므로 조합 글만 비운다.
	//==============================================================================
	fn apply_blur(&mut self) -> Option<String> {
		self.is_focused = false;
		self.focus_serial = 0;
		self.drag_start_index = None;
		let composition = self.edit_buffer.get_composition().to_string();
		self.edit_buffer.set_composition("");
		self.is_composing = false;
		if composition.is_empty() || self.is_dom_synchronized() {
			return None;
		}
		self.edit_buffer.begin_edit(TextEditKind::InsertText);
		self.edit_buffer.replace_selection(&composition);
		self.bump_value_revision();
		Some(composition)
	}

	//==============================================================================
	/// 포커스 잃음. (문맥이 있을 때: 확정한 조합 글은 OS 가 다시 보내도 버리게 하고, 바뀜 이벤트를 부르고, 문맥 메뉴를 닫는다)
	//==============================================================================
	fn handle_blur(&mut self, context: &mut ComponentContext) {
		let committed_text = self.apply_blur();
		self.hide_context_menu(context);
		self.hide_touch_selection(context);
		self.long_press_state = None;
		if let Some(committed_text) = committed_text {
			context.get_engine().get_input_manager().get_text_input().set_ime_commit_guard(&committed_text);
			self.fire_change_event(context);
		}
	}

	//==============================================================================
	/// 바뀜 이벤트 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fire_change_event(&mut self, context: &mut ComponentContext) {
		let mut change_event = match self.change_event.take() {
			Some(change_event) => {
				change_event
			}
			None => {
				return;
			}
		};
		change_event(self, context);
		if self.change_event.is_none() {
			self.change_event = Some(change_event);
		}
	}

	//==============================================================================
	/// 제출 이벤트 부르기.
	//==============================================================================
	fn fire_submit_event(&mut self, context: &mut ComponentContext) {
		let mut submit_event = match self.submit_event.take() {
			Some(submit_event) => {
				submit_event
			}
			None => {
				return;
			}
		};
		submit_event(self, context);
		if self.submit_event.is_none() {
			self.submit_event = Some(submit_event);
		}
	}

	//==============================================================================
	/// 노드 x(content 영역 기준) → 글자 자리. (원본처럼 UTF-16 단위)
	///
	/// # 인자
	/// - `local_x`: 노드 안의 X. (내용 영역 왼쪽이 0)
	///
	/// # 돌려줌
	/// 그 자리에 가장 가까운 글자 묶음 경계. (UTF-16 단위, 0 ~ 값 길이, 아직 그려 재지 않았으면 값 길이)
	//==============================================================================
	pub fn measure_index_at_local_x(&self, local_x: f32) -> usize {
		let character_index = self.measure_character_index_at_local_x(local_x);
		self.convert_to_utf16_index(character_index)
	}

	//==============================================================================
	/// 노드 x(content 영역 기준) → 문자 자리. (draw 때 재 둔 글자 경계로 찾는다. 아직 재지 않았으면 글 끝)
	//==============================================================================
	fn measure_character_index_at_local_x(&self, local_x: f32) -> usize {
		let value = self.edit_buffer.get_text();
		if value.is_empty() {
			return 0;
		}
		if self.measured_value != value || self.character_offsets.is_empty() {
			return count_characters(value);
		}
		let boundary_list = collect_grapheme_boundaries(value);
		find_boundary_index_at_offset(&self.character_offsets, &boundary_list, local_x - self.padding)
	}

	//==============================================================================
	/// 뷰 좌표 → 이 노드의 content 영역 x.
	//==============================================================================
	fn convert_view_position_to_local_x(&self, context: &ComponentContext, view_input_position: Vector2) -> f32 {
		let node_id = context.get_node_id();
		match context.get_node_tree().convert_view_position_to_node_position(node_id, view_input_position) {
			Some(local_position) => {
				local_position.x
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 글 입력 이벤트 하나 처리.
	//==============================================================================
	fn handle_text_input_event(&mut self, context: &mut ComponentContext, event_kind: TextInputEventKind) {
		match event_kind {
			TextInputEventKind::Commit(text) => {
				let text = self.sanitize_inserted_text(&text);
				if !text.is_empty() {
					self.replace_selection(context, &text);
				}
				self.preferred_caret_x = None;
				self.is_caret_upstream = false;
				self.request_caret_reveal(CaretRevealKind::IfNeeded);
				self.reset_cursor_blink();
			}
			TextInputEventKind::CompositionStart => {
				self.is_composing = true;
			}
			TextInputEventKind::Preedit { text, .. } => {
				self.edit_buffer.set_composition(&text);
				self.reset_cursor_blink();
			}
			TextInputEventKind::CompositionEnd => {
				self.is_composing = false;
				self.edit_buffer.set_composition("");
			}
			TextInputEventKind::Key { key, is_shift } => {
				self.handle_edit_key(context, key, is_shift);
			}
			TextInputEventKind::Paste(text) => {
				let text = self.sanitize_inserted_text(&text);
				if !text.is_empty() {
					self.replace_selection_with_edit(context, &text, TextEditKind::Paste);
				}
				self.preferred_caret_x = None;
				self.is_caret_upstream = false;
				self.request_caret_reveal(CaretRevealKind::IfNeeded);
				self.reset_cursor_blink();
			}
			TextInputEventKind::ReplaceValue { value, selection_end, .. } => {
				// 원본 handleDOMInputEvent: 조합 중에는 무시하고, 같은 값이면 무시한다.
				if self.is_composing || value == self.edit_buffer.get_text() {
					return;
				}
				self.edit_buffer.set_value_from_platform(&value, selection_end);
				self.preferred_caret_x = None;
				self.is_caret_upstream = false;
				self.request_caret_reveal(CaretRevealKind::IfNeeded);
				self.reset_cursor_blink();
				self.fire_change_event(context);
			}
			TextInputEventKind::Selection { start, end } => {
				// 원본 tick 의 selection 폴링: 포커스, 조합 아님, 끌기 아님일 때 범위가 다르면 따른다.
				if !self.is_focused || self.is_composing || self.drag_start_index.is_some() {
					return;
				}
				let current_range = self.edit_buffer.get_selection_range().unwrap_or((self.edit_buffer.get_cursor_index(), self.edit_buffer.get_cursor_index()));
				if current_range != (start, end) {
					self.edit_buffer.set_selection(start, end);
					self.preferred_caret_x = None;
					self.is_caret_upstream = false;
					self.request_caret_reveal(CaretRevealKind::IfNeeded);
					self.reset_cursor_blink();
				}
			}
			TextInputEventKind::FocusGained => {
				self.handle_focus();
			}
			TextInputEventKind::FocusLost => {
				self.handle_blur(context);
			}
		}
	}

	//==============================================================================
	/// 낱말 지우기. (블링크 DeleteWithDirection 의 kWord, 바뀌었으면 true)
	/// - 고른 것이 있으면 고른 것만 지운다. (DeleteSelectionCommand, 따로 한 되돌리기 묶음)
	/// - 없으면 커서에서 낱말 자리까지 지운다. 낱말 자리가 커서와 같으면(글자가 있는 줄의 앞) 글자 묶음 하나를 지운다. (TypingCommand 의 킬 링 지우기)
	//==============================================================================
	fn delete_word(&mut self, is_forward: bool) -> bool {
		if self.edit_buffer.has_selection() {
			self.edit_buffer.begin_edit(TextEditKind::Paste);
			self.edit_buffer.replace_selection("");
			return true;
		}
		let text = self.edit_buffer.get_text();
		let cursor_index = self.edit_buffer.get_cursor_index();
		let (start_index, end_index, edit_kind) = if is_forward {
			let mut end_index = find_next_word_position(text, cursor_index, is_windows_platform());
			if end_index == cursor_index {
				end_index = find_next_grapheme_boundary(text, cursor_index);
			}
			(cursor_index, end_index, TextEditKind::DeleteWordForward)
		}
		else {
			let mut start_index = find_previous_word_position(text, cursor_index);
			if start_index == cursor_index {
				start_index = find_previous_grapheme_boundary(text, cursor_index);
			}
			(start_index, cursor_index, TextEditKind::DeleteWordBackward)
		};
		if end_index <= start_index {
			return false;
		}
		self.edit_buffer.begin_edit(edit_kind);
		self.edit_buffer.delete_range(start_index, end_index);
		true
	}

	//==============================================================================
	/// 편집 키 처리. (원본 handleCanvasKeydown)
	//==============================================================================
	fn handle_edit_key(&mut self, context: &mut ComponentContext, key: TextInputKey, is_shift: bool) {
		let cursor_index = self.edit_buffer.get_cursor_index();
		// 위아래 이동이 아니면 지키던 가로 자리를 지우고, 커서가 보이게 스크롤한다. (블링크 FrameSelection)
		// 맥 스크롤 키는 넘기기만 하면 선택이 그대로라 가로 자리를 지키고, 옮기게 되면 그 갈래에서 지운다.
		let is_vertical_key = matches!(key, TextInputKey::Up | TextInputKey::Down | TextInputKey::PageUp | TextInputKey::PageDown);
		let is_scroll_key = matches!(key, TextInputKey::ScrollToDocumentStart | TextInputKey::ScrollToDocumentEnd | TextInputKey::ScrollPageUp | TextInputKey::ScrollPageDown);
		let was_caret_upstream = self.is_caret_upstream;
		if !is_vertical_key && !is_scroll_key {
			self.preferred_caret_x = None;
			self.is_caret_upstream = false;
			if !matches!(key, TextInputKey::Copy | TextInputKey::Tab) {
				self.request_caret_reveal(CaretRevealKind::IfNeeded);
			}
		}
		match key {
			TextInputKey::SelectAll => {
				self.select_all();
			}
			TextInputKey::Copy => {
				self.copy(context.get_engine());
			}
			TextInputKey::Cut => {
				self.cut(context);
			}
			TextInputKey::Paste => {
				self.paste(context);
			}
			TextInputKey::Backspace => {
				if self.edit_buffer.has_selection() || cursor_index > 0 {
					self.edit_buffer.begin_edit(TextEditKind::DeleteBackward);
				}
				if self.edit_buffer.delete_backward() {
					self.bump_value_revision();
					self.fire_change_event(context);
				}
			}
			TextInputKey::Delete => {
				if self.edit_buffer.has_selection() || cursor_index < count_characters(self.edit_buffer.get_text()) {
					self.edit_buffer.begin_edit(TextEditKind::DeleteForward);
				}
				if self.edit_buffer.delete_forward() {
					self.bump_value_revision();
					self.fire_change_event(context);
				}
			}
			TextInputKey::Undo | TextInputKey::Redo => {
				// 원본 캔버스 모드에는 되돌리기가 없고, DOM 입력 칸은 브라우저가 한다. (웹은 숨긴 <input> 이 해서 값으로 온다)
				if self.use_dom_input && !self.is_dom_synchronized() {
					self.apply_edit_history(context, key == TextInputKey::Redo);
				}
			}
			TextInputKey::Left | TextInputKey::Right => {
				// 글자 묶음 단위로 옮긴다. 선택이 있으면 선택을 늘리지 않을 때 그 앞이나 뒤로 접는다. (블링크 SelectionModifier 의 kCharacter)
				let is_right = key == TextInputKey::Right;
				let target_index = match (self.edit_buffer.get_selection_range(), is_shift) {
					(Some((start_index, end_index)), false) => {
						if is_right {
							end_index
						}
						else {
							start_index
						}
					}
					_ => {
						if is_right {
							find_next_grapheme_boundary(self.edit_buffer.get_text(), cursor_index)
						}
						else {
							find_previous_grapheme_boundary(self.edit_buffer.get_text(), cursor_index)
						}
					}
				};
				self.move_cursor_to_character(target_index, is_shift);
			}
			TextInputKey::WordLeft | TextInputKey::WordRight => {
				// 낱말 단위로 옮긴다. 선택이 있어도 커서(focus) 자리에서 찾는다. (블링크 SelectionModifier 의 kWord)
				// 맥은 넓히던 고르기가 고정 끝을 넘으면 고정 끝에서 멈춘다. (editing_behavior.h 의 ShouldExtendSelectionByWordOrLineAcrossCaret)
				let text = self.edit_buffer.get_text();
				let mut target_index = if key == TextInputKey::WordRight {
					find_next_word_position(text, cursor_index, is_windows_platform())
				}
				else {
					find_previous_word_position(text, cursor_index)
				};
				let anchor_index = self.edit_buffer.get_selection_start();
				if is_shift && is_mac_platform() && anchor_index != cursor_index && (anchor_index <= cursor_index) != (anchor_index <= target_index) {
					target_index = anchor_index;
				}
				self.move_cursor_to_character(target_index, is_shift);
			}
			TextInputKey::DeleteWordBackward | TextInputKey::DeleteWordForward => {
				if self.delete_word(key == TextInputKey::DeleteWordForward) {
					self.bump_value_revision();
					self.fire_change_event(context);
				}
			}
			TextInputKey::Home | TextInputKey::End => {
				let is_end = key == TextInputKey::End;
				if self.is_multiline {
					// 보이는 줄의 앞과 끝. (블링크 MoveToBeginningOfLine, MoveToEndOfLine 의 kLineBoundary)
					let content_size = context.get_transform().get_content_size();
					self.ensure_text_area_layout_without_graphic(content_size);
					let line_index = self.text_area_layout.find_line_index(cursor_index, was_caret_upstream);
					let (target_index, is_upstream) = self.text_area_layout.get_line_boundary(line_index, is_end);
					self.move_cursor_to_character(target_index, is_shift);
					self.is_caret_upstream = is_upstream;
				}
				else if is_end {
					let character_count = count_characters(self.edit_buffer.get_text());
					self.move_cursor_to_character(character_count, is_shift);
				}
				else {
					self.move_cursor_to_character(0, is_shift);
				}
			}
			TextInputKey::Up | TextInputKey::Down | TextInputKey::PageUp | TextInputKey::PageDown => {
				if self.is_multiline {
					self.move_caret_vertically(context, key, is_shift);
				}
				else if self.use_dom_input {
					// 한 줄 DOM 입력 칸: 앞 줄이 없어 위는 글 앞, 아래는 글 끝. (블링크 selection_modifier_line.cc)
					let target_index = if matches!(key, TextInputKey::Up | TextInputKey::PageUp) {
						0
					}
					else {
						count_characters(self.edit_buffer.get_text())
					};
					self.move_cursor_to_character(target_index, is_shift);
				}
			}
			TextInputKey::DocumentStart | TextInputKey::DocumentEnd => {
				// 글 앞과 끝. (블링크 MoveToBeginningOfDocument, MoveToEndOfDocument)
				if self.is_multiline || self.use_dom_input {
					let target_index = if key == TextInputKey::DocumentStart {
						0
					}
					else {
						count_characters(self.edit_buffer.get_text())
					};
					self.move_cursor_to_character(target_index, is_shift);
				}
			}
			TextInputKey::ScrollToDocumentStart | TextInputKey::ScrollToDocumentEnd => {
				// 맥 Home, End. (크롬 맥: scrollToBeginningOfDocument 를 ScrollToBeginningOfDocument 편집 명령으로 넘겨 커서를 두고 넘기고,
				// 넘길 것이 없어 명령이 거짓이면 블링크 키 표의 MoveToBeginningOfLine, MoveToEndOfLine 으로 옮긴다. Shift 와 함께면 글 앞과 끝까지 고른다)
				// 원본 캔버스 모드는 keydown 의 Home, End 를 그대로 받는다.
				let is_end = key == TextInputKey::ScrollToDocumentEnd;
				let is_canvas_mode = !self.is_multiline && !self.use_dom_input;
				let mut is_scrolled = false;
				if self.is_multiline && !is_shift {
					let content_size = context.get_transform().get_content_size();
					self.ensure_text_area_layout_without_graphic(content_size);
					let maximum_scroll_top = self.get_maximum_scroll_top(content_size.y);
					let target_scroll_top = if is_end {
						maximum_scroll_top
					}
					else {
						0.0
					};
					if (target_scroll_top - self.scroll_top).abs() > 0.0 {
						self.scroll_top = target_scroll_top;
						is_scrolled = true;
					}
				}
				if !is_scrolled {
					self.preferred_caret_x = None;
					self.is_caret_upstream = false;
					self.request_caret_reveal(CaretRevealKind::IfNeeded);
					if is_shift && !is_canvas_mode {
						let target_index = if is_end {
							count_characters(self.edit_buffer.get_text())
						}
						else {
							0
						};
						self.move_cursor_to_character(target_index, true);
					}
					else if self.is_multiline {
						let content_size = context.get_transform().get_content_size();
						self.ensure_text_area_layout_without_graphic(content_size);
						let line_index = self.text_area_layout.find_line_index(cursor_index, was_caret_upstream);
						let (target_index, is_upstream) = self.text_area_layout.get_line_boundary(line_index, is_end);
						self.move_cursor_to_character(target_index, is_shift);
						self.is_caret_upstream = is_upstream;
					}
					else {
						let target_index = if is_end {
							count_characters(self.edit_buffer.get_text())
						}
						else {
							0
						};
						self.move_cursor_to_character(target_index, is_shift);
					}
				}
			}
			TextInputKey::ScrollPageUp | TextInputKey::ScrollPageDown => {
				// 맥 PageUp, PageDown. (크롬 맥: scrollPageUp 은 블링크 명령이 아니라 키 기본 동작이 한 화면만큼 넘긴다, cc ScrollUtils::CalculatePageStep. 커서는 그대로)
				if self.is_multiline {
					let content_size = context.get_transform().get_content_size();
					self.ensure_text_area_layout_without_graphic(content_size);
					let page_step = calculate_page_step(content_size.y);
					let direction = if key == TextInputKey::ScrollPageUp {
						-1.0
					}
					else {
						1.0
					};
					self.scroll_top = (self.scroll_top + page_step * direction).clamp(0.0, self.get_maximum_scroll_top(content_size.y));
				}
			}
			TextInputKey::Enter => {
				if self.is_multiline {
					// 줄 넣기. (블링크 InsertNewline, 글자 넣기 묶음에 붙는다)
					self.replace_selection_with_edit(context, "\n", TextEditKind::InsertText);
				}
				else {
					self.fire_submit_event(context);
				}
			}
			TextInputKey::Tab => {
			}
		}
		self.reset_cursor_blink();
	}

	//==============================================================================
	/// 문맥 메뉴 항목 하기.
	//==============================================================================
	fn perform_menu_action(&mut self, context: &mut ComponentContext, menu_action: UIInputFieldMenuAction) {
		match menu_action {
			UIInputFieldMenuAction::Cut => {
				self.cut(context);
			}
			UIInputFieldMenuAction::Copy => {
				self.copy(context.get_engine());
			}
			UIInputFieldMenuAction::Paste => {
				self.paste(context);
			}
			UIInputFieldMenuAction::SelectAll => {
				self.select_all();
			}
		}
	}

	//==============================================================================
	/// 문맥 메뉴 열기. (루트의 마지막 자식 노드로 만들어 위에 그린다. 원본 showContextMenu, 직접 누름 필드도 같다)
	//==============================================================================
	fn show_context_menu(&mut self, context: &mut ComponentContext, view_position: Vector2) {
		self.hide_context_menu(context);
		let owner_node_id = context.get_node_id();
		let is_selection_available = self.has_selection();
		let is_text_available = !self.get_text().is_empty();
		let node_tree = context.get_node_tree_mut();
		let root_id = node_tree.get_root_id();
		let local_position = node_tree.convert_view_position_to_node_position(root_id, view_position).unwrap_or(view_position);
		let menu_id = node_tree.create_child(root_id, "uiInputFieldContextMenu");
		let menu_node = &mut node_tree[menu_id];
		menu_node.set_pivot(Pivot::TOP_LEFT);
		menu_node.set_anchor(Vector2::zero());
		menu_node.set_local_position(local_position);
		menu_node.set_content_size(Vector2::create(CONTEXT_MENU_WIDTH, CONTEXT_MENU_PADDING_Y * 2.0 + CONTEXT_MENU_ITEM_HEIGHT * CONTEXT_MENU_ITEM_LIST.len() as f32));
		node_tree.add_component(menu_id, UIInputFieldContextMenu::new(owner_node_id, is_selection_available, is_text_available));
		self.context_menu_node_id = Some(menu_id);
	}

	//==============================================================================
	/// 문맥 메뉴 닫기. (원본 hideContextMenu)
	//==============================================================================
	fn hide_context_menu(&mut self, context: &mut ComponentContext) {
		if let Some(context_menu_node_id) = self.context_menu_node_id.take() {
			context.get_node_tree_mut().destroy_node(context_menu_node_id);
		}
	}

	//==============================================================================
	/// 필드 밖을 눌렀으면 포커스를 뺀다. (원본 DOM 모드의 캔버스 누름 처리. 문맥 메뉴 위는 밖으로 치지 않는다)
	//==============================================================================
	fn blur_on_outside_press(&mut self, context: &mut ComponentContext) {
		if !self.is_focused || !self.use_dom_input {
			return;
		}
		let input_manager = context.get_engine().get_input_manager();
		if !input_manager.is_touch_pressed() {
			return;
		}
		let view_input_position = input_manager.get_view_input_position();
		let node_id = context.get_node_id();
		let node_tree = context.get_node_tree();
		let is_inside = is_node_visible_in_hierarchy(node_tree, node_id) && is_view_position_inside_node(node_tree, node_id, view_input_position);
		let is_inside_menu = match self.context_menu_node_id {
			Some(context_menu_node_id) => {
				is_view_position_inside_node(node_tree, context_menu_node_id, view_input_position)
			}
			None => {
				false
			}
		};
		let is_inside_quick_menu = self.quick_menu_node_id.map(|quick_menu_node_id| node_tree.contains(quick_menu_node_id) && is_view_position_inside_node(node_tree, quick_menu_node_id, view_input_position)).unwrap_or(false);
		if !is_inside && !is_inside_menu && !is_inside_quick_menu {
			self.handle_blur(context);
		}
	}

	//==============================================================================
	/// 플랫폼에 올릴 상태 만들기. (자리는 창 안쪽 논리 좌표)
	//==============================================================================
	fn build_field_state(&self, context: &ComponentContext) -> TextInputFieldState {
		let node_id = context.get_node_id();
		let node_tree = context.get_node_tree();
		let view_manager = context.get_engine().get_view_manager();
		let content_size = context.get_transform().get_content_size();
		let world_matrix = node_tree.calculate_world_matrix(node_id);
		let view_native_rect = view_manager.get_view_native_rect();
		let target_resolution_scale = view_manager.get_target_resolution_scale();
		let convert_local_rect = |local_rect: Rect| -> Rect {
			let corner_list = [
				Vector2::create(local_rect.position.x, local_rect.position.y),
				Vector2::create(local_rect.position.x + local_rect.size.x, local_rect.position.y),
				Vector2::create(local_rect.position.x, local_rect.position.y + local_rect.size.y),
				Vector2::create(local_rect.position.x + local_rect.size.x, local_rect.position.y + local_rect.size.y),
			];
			let mut minimum_position = Vector2::create(f32::MAX, f32::MAX);
			let mut maximum_position = Vector2::create(f32::MIN, f32::MIN);
			for corner in corner_list.iter() {
				let view_position = world_matrix.transform_point(corner.x, corner.y);
				let canvas_x = view_native_rect.position.x + view_position.x * target_resolution_scale;
				let canvas_y = view_native_rect.position.y + view_position.y * target_resolution_scale;
				minimum_position = Vector2::create(minimum_position.x.min(canvas_x), minimum_position.y.min(canvas_y));
				maximum_position = Vector2::create(maximum_position.x.max(canvas_x), maximum_position.y.max(canvas_y));
			}
			Rect::create(minimum_position.x, minimum_position.y, maximum_position.x - minimum_position.x, maximum_position.y - minimum_position.y)
		};
		let field_rect = convert_local_rect(Rect::create(0.0, 0.0, content_size.x, content_size.y));
		let caret_height = if self.caret_local_height > 0.0 {
			self.caret_local_height
		}
		else {
			content_size.y
		};
		let caret_rect = convert_local_rect(Rect::create(self.caret_local_x, self.caret_local_top, 1.0, caret_height));
		TextInputFieldState {
			field_id: self.field_id,
			focus_serial: self.focus_serial,
			is_focused: self.is_focused,
			is_attached: self.is_attached,
			use_dominput: self.use_dom_input,
			use_touch_end_focus: self.use_touch_end_focus,
			use_direct_touch: self.use_direct_touch,
			is_visible: is_node_visible_in_hierarchy(node_tree, node_id),
			field_rect,
			caret_rect,
			value: self.edit_buffer.get_text().to_string(),
			selection_start: self.edit_buffer.get_selection_start(),
			cursor_index: self.edit_buffer.get_cursor_index(),
			max_length: self.edit_buffer.get_max_length(),
			value_revision: self.value_revision,
			is_multiline: self.is_multiline,
		}
	}

	//==============================================================================
	/// Canvas2D font 꼴 문자열.
	//==============================================================================
	fn build_font_string(&self) -> String {
		format!("{}px {}", self.font_size, self.font_family)
	}

	//==============================================================================
	/// 값의 글자 경계 다시 재기. (값이나 폰트가 바뀌었을 때만. graphic 의 폰트 문자열이 걸린 상태에서 부른다)
	//==============================================================================
	fn update_character_offsets(&mut self, graphic: &Graphic, font_string: &str) {
		let value = self.edit_buffer.get_text();
		if self.measured_value == value && self.measured_font_string == font_string && !self.character_offsets.is_empty() {
			return;
		}
		let mut character_offsets: Vec<f32> = Vec::with_capacity(value.len() + 1);
		character_offsets.push(0.0);
		for (byte_index, character) in value.char_indices() {
			let prefix_end = byte_index + character.len_utf8();
			character_offsets.push(graphic.measure_text(&value[..prefix_end]).width);
		}
		self.character_offsets = character_offsets;
		self.measured_value = value.to_string();
		self.measured_font_string = font_string.to_string();
	}

	//==============================================================================
	/// 글자 경계 x. (재 둔 것이 없으면 0)
	//==============================================================================
	fn get_character_offset(&self, character_index: usize) -> f32 {
		match self.character_offsets.get(character_index) {
			Some(character_offset) => {
				*character_offset
			}
			None => {
				self.character_offsets.last().copied().unwrap_or(0.0)
			}
		}
	}
}

impl Default for UIInputField {
	//==============================================================================
	/// 기본값. (UIInputField::new 와 같다)
	//==============================================================================
	fn default() -> UIInputField {
		UIInputField::new()
	}
}

//==============================================================================
/// 여러 줄 필드와 넣는 글 다듬기. (반사에 넣지 않는 안쪽 함수)
//==============================================================================
impl UIInputField {
	//==============================================================================
	/// 넣는 글 다듬기. (블링크: textarea 는 CR 과 CRLF 를 LF 로, 한 줄 DOM 입력 칸은 끝 줄바꿈을 지우고 가운데 줄바꿈을 공백으로. 원본 캔버스 모드는 그대로)
	//==============================================================================
	fn sanitize_inserted_text(&self, text: &str) -> String {
		if self.is_multiline {
			return text.replace("\r\n", "\n").replace('\r', "\n");
		}
		if self.use_dom_input && !self.is_dom_synchronized() && text.contains(['\r', '\n']) {
			let trimmed_text = text.trim_end_matches(['\r', '\n']);
			return trimmed_text.replace("\r\n", " ").replace(['\r', '\n'], " ");
		}
		text.to_string()
	}

	//==============================================================================
	/// 커서가 보이게 스크롤하기를 부탁. (여러 줄 필드, 다음 배치에서 한다. Always 가 IfNeeded 보다 앞선다)
	//==============================================================================
	fn request_caret_reveal(&mut self, reveal_kind: CaretRevealKind) {
		if !self.is_multiline {
			return;
		}
		if self.caret_reveal_request != Some(CaretRevealKind::Always) {
			self.caret_reveal_request = Some(reveal_kind);
		}
	}

	//==============================================================================
	/// 보이는 글의 커서 자리. (조합 중이면 조합 글 뒤)
	//==============================================================================
	fn get_display_cursor_index(&self) -> usize {
		self.edit_buffer.get_cursor_index() + count_characters(self.edit_buffer.get_composition())
	}

	//==============================================================================
	/// 여러 줄 배치 맞추기. (보인 글, 폰트, 폭, 줄 높이, 탭 크기가 바뀌었을 때만 다시 짓는다)
	/// - 크롬 textarea 는 overflow: auto 라, 클래식 스크롤 막대(윈도우, 리눅스, 맥 클래식 설정)는 넘칠 때 테두리 안쪽의 막대 두께만큼 글 폭을 줄인다.
	///   막대 없이 지어 보고 넘치면 막대 자리를 덜고 다시 짓는다. (crate::web::pane::read_classic_scrollbar_metrics)
	//==============================================================================
	fn ensure_text_area_layout(&mut self, content_size: Vector2, measure_width: &dyn Fn(&str) -> f32) {
		let display_text = self.edit_buffer.build_display_text();
		let font_string = self.build_font_string();
		let scrollbar_thickness = crate::web::pane::read_classic_scrollbar_metrics(false).map(|metrics| metrics.thickness).unwrap_or(0.0);
		let line_height = self.get_line_height();
		// 폭이 좁으면 글이 길어질 뿐이라, 막대를 둬도 넘치지 않으면 막대 없이도 넘치지 않고, 막대 없이 넘치면 막대를 둬도 넘친다. 지난 칸부터 본다.
		let mut scrollbar_gutter = if self.text_area_scrollbar_gutter > 0.0 {
			scrollbar_thickness
		}
		else {
			0.0
		};
		for _ in 0..2 {
			let wrap_width = (content_size.x - self.padding * 2.0 - scrollbar_gutter).max(0.0);
			let layout_key = (display_text.clone(), font_string.clone(), wrap_width, line_height, self.tab_size);
			if self.text_area_layout_key.as_ref() != Some(&layout_key) {
				let tab_width = measure_width(" ") * self.tab_size;
				self.text_area_layout = build_text_area_layout(&layout_key.0, wrap_width, line_height, tab_width, measure_width);
				self.text_area_layout_key = Some(layout_key);
			}
			self.text_area_scrollbar_gutter = scrollbar_gutter;
			let is_overflowing = self.text_area_layout.get_content_height() + self.padding * 2.0 > content_size.y + 0.5;
			let wanted_gutter = if is_overflowing {
				scrollbar_thickness
			}
			else {
				0.0
			};
			if wanted_gutter == scrollbar_gutter {
				return;
			}
			scrollbar_gutter = wanted_gutter;
		}
	}

	//==============================================================================
	/// Graphic 없이 여러 줄 배치 맞추기. (tick 의 위아래 이동과 누른 자리, 그릴 때와 같은 글자 캐시로 잰다)
	//==============================================================================
	fn ensure_text_area_layout_without_graphic(&mut self, content_size: Vector2) {
		let font_string = self.build_font_string();
		let font_size = self.font_size;
		let measure_width = move |measured_text: &str| -> f32 {
			match crate::core::textstringtexturecache::measure_text_without_graphic(&font_string, measured_text) {
				Some(text_metrics) => {
					text_metrics.width
				}
				None => {
					measured_text.chars().count() as f32 * font_size * 0.55
				}
			}
		};
		self.ensure_text_area_layout(content_size, &measure_width);
	}

	//==============================================================================
	/// 스크롤할 수 있는 끝. (글 높이 + 위아래 안쪽 여백 - 노드 높이)
	//==============================================================================
	fn get_maximum_scroll_top(&self, content_height: f32) -> f32 {
		(self.text_area_layout.get_content_height() + self.padding * 2.0 - content_height).max(0.0)
	}

	//==============================================================================
	/// 커서가 보이게 스크롤. (블링크 RevealSelection: 맥이 아니면 IfNeeded 는 가장자리, Always 는 맨 위. 맥은 가운데)
	//==============================================================================
	fn apply_caret_reveal(&mut self, content_height: f32) {
		let reveal_kind = match self.caret_reveal_request.take() {
			Some(reveal_kind) => {
				reveal_kind
			}
			None => {
				return;
			}
		};
		let (_, line_top) = self.text_area_layout.get_caret_position(self.get_display_cursor_index(), self.is_caret_upstream);
		let caret_top = self.padding + line_top;
		let caret_bottom = caret_top + self.text_area_layout.line_height;
		let is_visible = caret_top >= self.scroll_top && caret_bottom <= self.scroll_top + content_height;
		let is_center_align = cfg!(target_os = "macos");
		match (reveal_kind, is_center_align) {
			(CaretRevealKind::Always, false) => {
				self.scroll_top = caret_top;
			}
			(CaretRevealKind::Always, true) => {
				self.scroll_top = (caret_top + caret_bottom - content_height) * 0.5;
			}
			(CaretRevealKind::IfNeeded, true) => {
				if !is_visible {
					self.scroll_top = (caret_top + caret_bottom - content_height) * 0.5;
				}
			}
			(CaretRevealKind::IfNeeded, false) => {
				if caret_top < self.scroll_top {
					self.scroll_top = caret_top;
				}
				else if caret_bottom > self.scroll_top + content_height {
					self.scroll_top = caret_bottom - content_height;
				}
			}
		}
		self.scroll_top = self.scroll_top.clamp(0.0, self.get_maximum_scroll_top(content_height));
	}

	//==============================================================================
	/// 위아래로 커서 옮기기. (블링크 MoveUp, MoveDown, MovePageUp, MovePageDown)
	/// - 가로 자리는 첫 위아래 이동 때의 커서 x 를 지킨다. 첫 줄 위는 글 앞, 마지막 줄 아래는 글 끝.
	/// - page: 거리는 보이는 높이의 87.5% (맥은 높이 - 40 과 견줘 큰 쪽, cc ScrollUtils::CalculatePageStep), 그 안의 가장 먼 줄로 옮기고 그 줄을 맨 위로 스크롤한다.
	//==============================================================================
	fn move_caret_vertically(&mut self, context: &mut ComponentContext, key: TextInputKey, is_shift: bool) {
		let content_size = context.get_transform().get_content_size();
		self.ensure_text_area_layout_without_graphic(content_size);
		let cursor_index = self.edit_buffer.get_cursor_index();
		let (caret_x, _) = self.text_area_layout.get_caret_position(cursor_index, self.is_caret_upstream);
		let preferred_x = self.preferred_caret_x.unwrap_or(caret_x);
		let (target_index, is_upstream, reveal_kind) = match key {
			TextInputKey::Up | TextInputKey::Down => {
				let line_delta = if key == TextInputKey::Up {
					-1
				}
				else {
					1
				};
				let (target_index, is_upstream) = self.text_area_layout.move_vertically(cursor_index, self.is_caret_upstream, preferred_x, line_delta);
				(target_index, is_upstream, CaretRevealKind::IfNeeded)
			}
			_ => {
				let page_step = calculate_page_step(content_size.y);
				let line_height = self.text_area_layout.line_height.max(1.0);
				let line_count = ((page_step / line_height).floor() as isize).max(1);
				let line_index = self.text_area_layout.find_line_index(cursor_index, self.is_caret_upstream) as isize;
				let last_line_index = self.text_area_layout.line_list.len().saturating_sub(1) as isize;
				let target_line = if key == TextInputKey::PageUp {
					(line_index - line_count).max(0)
				}
				else {
					(line_index + line_count).min(last_line_index)
				};
				let (target_index, is_upstream) = self.text_area_layout.find_character_index_in_line(target_line as usize, preferred_x);
				(target_index, is_upstream, CaretRevealKind::Always)
			}
		};
		self.move_cursor_to_character(target_index, is_shift);
		self.is_caret_upstream = is_upstream;
		self.preferred_caret_x = Some(preferred_x);
		self.request_caret_reveal(reveal_kind);
	}

	//==============================================================================
	/// 여러 줄 필드의 누른 자리 → (문자 자리, 위쪽 친화). (노드 좌표)
	//==============================================================================
	fn find_text_area_index_at(&mut self, context: &ComponentContext, view_input_position: Vector2) -> (usize, bool) {
		let content_size = context.get_transform().get_content_size();
		self.ensure_text_area_layout_without_graphic(content_size);
		let local_position = context.get_node_tree().convert_view_position_to_node_position(context.get_node_id(), view_input_position).unwrap_or(Vector2::zero());
		self.text_area_layout.find_character_index_at(local_position.x - self.padding, local_position.y - self.padding + self.scroll_top)
	}

	//==============================================================================
	/// 여러 줄 필드의 스크롤 막대 썸 자리. (노드 좌표, 넘치지 않으면 None)
	//==============================================================================
	fn calculate_text_area_thumb_rect(&self, content_size: Vector2) -> Option<Rect> {
		let scroll_height = self.text_area_layout.get_content_height() + self.padding * 2.0;
		if scroll_height <= content_size.y + 0.5 || content_size.y <= 0.0 {
			return None;
		}
		// 클래식 막대: 막대 칸 안, 양끝 단추 사이의 크롬 치수 썸. (그린 썸 자리)
		if self.text_area_scrollbar_gutter > 0.0 {
			let metrics = crate::web::pane::read_classic_scrollbar_metrics(false)?;
			let (thumb_start, thumb_length) = crate::web::pane::calculate_classic_thumb_span(&metrics, content_size.y, content_size.y, scroll_height, self.scroll_top)?;
			let track_left = content_size.x - self.text_area_scrollbar_gutter;
			return Some(Rect::create(track_left + metrics.thumb_cross_offset, thumb_start + metrics.thumb_length_inset, metrics.thumb_thickness, (thumb_length - metrics.thumb_length_inset * 2.0).max(0.0)));
		}
		let thumb_length = (content_size.y * content_size.y / scroll_height).max(TEXT_AREA_SCROLLBAR_MINIMUM_THUMB_LENGTH).min(content_size.y);
		let maximum_scroll_top = scroll_height - content_size.y;
		let thumb_top = (content_size.y - thumb_length) * (self.scroll_top / maximum_scroll_top).clamp(0.0, 1.0);
		Some(Rect::create(content_size.x - TEXT_AREA_SCROLLBAR_THICKNESS, thumb_top, TEXT_AREA_SCROLLBAR_THICKNESS, thumb_length))
	}

	//==============================================================================
	/// 여러 줄 그리기. (선택 → 글 → 조합 밑줄 → 커서, 안쪽만 그리게 자른다. 넘치면 썸)
	//==============================================================================
	fn draw_multiline(&mut self, content_size: Vector2, graphic: &mut Graphic) {
		let font_string = self.build_font_string();
		graphic.push_state();
		graphic.set_font_string(&font_string);
		{
			let graphic_reference: &Graphic = graphic;
			self.ensure_text_area_layout(content_size, &|measured_text: &str| graphic_reference.measure_text(measured_text).width);
		}
		self.apply_caret_reveal(content_size.y);
		self.scroll_top = self.scroll_top.clamp(0.0, self.get_maximum_scroll_top(content_size.y));
		let padding = self.padding;
		let origin_y = padding - self.scroll_top;
		let line_height = self.text_area_layout.line_height;
		graphic.begin_clip_rect(Rect::create(0.0, 0.0, content_size.x, content_size.y), self.background_paint.get_round_size());

		// 선택. (조합 중엔 그리지 않는다)
		if self.edit_buffer.get_composition().is_empty() {
			if let Some((start_index, end_index)) = self.edit_buffer.get_selection_range() {
				graphic.set_fill_color(self.selection_color);
				for selection_rect in self.text_area_layout.build_selection_rect_list(start_index, end_index) {
					graphic.draw_rect(Rect::create(padding + selection_rect.position.x, origin_y + selection_rect.position.y, selection_rect.size.x, selection_rect.size.y));
				}
			}
		}

		// 글. (값이나 조합 글이 있으면 줄마다, 없으면 첫 줄에 안내 글. 탭은 탭 자리에서 이어 그린다)
		let has_value = !self.edit_buffer.get_text().is_empty() || !self.edit_buffer.get_composition().is_empty();
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		if has_value {
			let display_text = self.text_area_layout_key.as_ref().map(|layout_key| layout_key.0.clone()).unwrap_or_default();
			graphic.set_fill_color(self.value_text.get_text_color());
			let character_byte_list: Vec<usize> = display_text.char_indices().map(|(byte_index, _)| byte_index).chain(std::iter::once(display_text.len())).collect();
			for line in self.text_area_layout.line_list.iter() {
				let line_y = origin_y + line.top + line_height * 0.5;
				if line_y + line_height < 0.0 || line_y - line_height > content_size.y {
					continue;
				}
				let line_text = &display_text[character_byte_list[line.start]..character_byte_list[line.end]];
				let mut piece_character_offset = 0;
				for piece in line_text.split('\t') {
					if !piece.is_empty() {
						let piece_x = line.offsets.get(piece_character_offset).copied().unwrap_or(0.0);
						graphic.draw_fill_text(piece, padding + piece_x, line_y);
					}
					piece_character_offset += piece.chars().count() + 1;
				}
			}
		}
		else if !self.placeholder.is_empty() {
			graphic.set_fill_color(self.placeholder_text.get_text_color());
			graphic.draw_fill_text(&self.placeholder, padding, origin_y + line_height * 0.5);
		}

		// 조합 밑줄. (조합 글이 걸친 줄마다)
		let composition_count = count_characters(self.edit_buffer.get_composition());
		if composition_count > 0 {
			let composition_start = self.edit_buffer.get_cursor_index();
			let composition_end = composition_start + composition_count;
			graphic.set_stroke_color(self.composition_underline_color);
			for (line_index, line) in self.text_area_layout.line_list.iter().enumerate() {
				let segment_start = composition_start.max(line.start);
				let segment_end = composition_end.min(line.end);
				if segment_start >= segment_end {
					continue;
				}
				let underline_y = origin_y + line.top + line_height * 0.5 + self.font_size * 0.55;
				let start_x = padding + self.text_area_layout.get_x_in_line(line_index, segment_start);
				let end_x = padding + self.text_area_layout.get_x_in_line(line_index, segment_end);
				graphic.draw_line(&[Vector2::create(start_x, underline_y), Vector2::create(end_x, underline_y)], 2.0);
			}
		}

		// 커서.
		let (caret_x, caret_line_top) = self.text_area_layout.get_caret_position(self.get_display_cursor_index(), self.is_caret_upstream && composition_count == 0);
		self.caret_local_x = padding + caret_x;
		self.caret_local_top = origin_y + caret_line_top;
		self.caret_local_height = line_height;
		if self.is_focused && self.is_cursor_visible {
			let caret_center_y = self.caret_local_top + line_height * 0.5;
			let half_height = self.font_size * 0.6;
			graphic.set_stroke_color(self.cursor_color);
			graphic.draw_line(&[Vector2::create(self.caret_local_x, caret_center_y - half_height), Vector2::create(self.caret_local_x, caret_center_y + half_height)], 2.0);
		}
		graphic.end_clip_rect();

		// 터치 선택 손잡이. (경계 줄의 아래 끝)
		{
			let selection_range = self.edit_buffer.get_selection_range().filter(|_| composition_count == 0);
			let (start_point, end_point) = match selection_range {
				Some((start_index, end_index)) => {
					let (start_x, start_top) = self.text_area_layout.get_caret_position(start_index, false);
					let (end_x, end_top) = self.text_area_layout.get_caret_position(end_index, true);
					(Vector2::create(padding + start_x, origin_y + start_top + line_height), Vector2::create(padding + end_x, origin_y + end_top + line_height))
				}
				None => {
					let caret_point = Vector2::create(self.caret_local_x, self.caret_local_top + line_height);
					(caret_point, caret_point)
				}
			};
			self.place_and_draw_touch_handles(graphic, start_point, end_point, selection_range.is_some());
		}

		// 썸.
		if let Some(thumb_rect) = self.calculate_text_area_thumb_rect(content_size) {
			graphic.set_fill_color(Color::create(121.0 / 255.0, 121.0 / 255.0, 121.0 / 255.0, 0.4));
			graphic.draw_rect(thumb_rect);
		}
		graphic.pop_state();
	}
}

//==============================================================================
/// 손가락 길게 누르기, 터치 선택 손잡이, 빠른 메뉴. (데스크탑에서 DOM 입력을 직접 누르게 한 필드, 원본은 크롬 데스크탑 터치가 한다)
/// - 손가락으로 누른 채 650ms(크롬 aura 의 길게 누르기 500 + 누름 표시 150) 동안 15 DIP 안에 있으면 길게 누르기다.
/// - 길게 누르면 그 자리의 낱말을 고르고(블링크 터치: 줄바꿈으로 시작하거나 공백뿐이거나 글 끝이면 커서만) 손잡이를 보인다.
/// - 메뉴는 윈도우는 뗄 때(블링크 context_menu_on_mouse_up), 그 밖은 길게 누르는 순간 정한다. (크롬 TouchSelectionControllerClientAura::HandleContextMenu)
///   고른 것이 있거나 클립보드에 글이 있으면 빠른 메뉴를 부탁하고(뗀 뒤 100ms), 둘 다 없으면 손잡이를 숨기고 보통 문맥 메뉴다.
/// - 손잡이를 끌면 그 끝을 옮긴다. 키, 마우스 누름, 휠, 다른 자리 누름은 손잡이와 빠른 메뉴를 지운다.
//==============================================================================
impl UIInputField {
	//==============================================================================
	/// 손가락 길게 누르기를 받는 필드인지. (데스크탑, 크롬 편집 칸 메뉴를 그리는 필드와 같다)
	//==============================================================================
	fn is_long_press_target(&self) -> bool {
		cfg!(not(target_arch = "wasm32")) && self.use_direct_touch && self.use_dom_input
	}

	//==============================================================================
	/// 뷰 한 단위의 창 픽셀.
	//==============================================================================
	fn read_view_scale(context: &ComponentContext) -> Vector2 {
		let view_manager = context.get_engine().get_view_manager();
		let view_size = view_manager.get_view_size();
		let canvas_native_size = view_manager.get_canvas_native_size();
		let scale_x = canvas_native_size.x / view_size.x.max(1.0);
		let scale_y = canvas_native_size.y / view_size.y.max(1.0);
		// 창이 없으면(시험) 1.
		if scale_x <= 0.0 || scale_y <= 0.0 {
			return Vector2::create(1.0, 1.0);
		}
		Vector2::create(scale_x, scale_y)
	}

	//==============================================================================
	/// 빠른 메뉴 닫기. (부탁은 남긴다)
	//==============================================================================
	fn hide_quick_menu(&mut self, context: &mut ComponentContext) {
		if let Some(quick_menu_node_id) = self.quick_menu_node_id.take() {
			if context.get_node_tree().contains(quick_menu_node_id) {
				context.get_node_tree_mut().destroy_node(quick_menu_node_id);
			}
		}
	}

	//==============================================================================
	/// 손잡이와 빠른 메뉴 지우기. (크롬 HideAndDisallowShowingAutomatically)
	//==============================================================================
	fn hide_touch_selection(&mut self, context: &mut ComponentContext) {
		self.is_touch_handle_visible = false;
		self.is_quick_menu_requested = false;
		self.quick_menu_delay = None;
		self.dragging_touch_handle = None;
		self.hide_quick_menu(context);
	}

	//==============================================================================
	/// 노드 좌표의 문자 자리. ((자리, 위쪽 친화), 한 줄은 x, 여러 줄은 배치)
	//==============================================================================
	fn find_character_index_at_local(&mut self, context: &ComponentContext, local_position: Vector2) -> (usize, bool) {
		if self.is_multiline {
			let content_size = context.get_transform().get_content_size();
			self.ensure_text_area_layout_without_graphic(content_size);
			return self.text_area_layout.find_character_index_at(local_position.x - self.padding, local_position.y - self.padding + self.scroll_top);
		}
		(self.measure_character_index_at_local_x(local_position.x), false)
	}

	//==============================================================================
	/// 마우스로 이어 누른 수. (원본 DOM 입력 칸과 textarea 를 흉내 내는 필드만, 손가락과 캔버스 모드 필드는 1)
	/// - 데스크탑: 크롬 aura(윈도우, 리눅스)의 MouseEvent::GetRepeatCount 처럼 앞 누름에서 시간(read_double_click_seconds) 안,
	///   가로와 세로 2 DIP 안이면 하나 더 세고 셋을 넘지 않는다. 맥 크롬은 NSEvent clickCount 를 쓰고 시간은 같은 운영체제 설정이다.
	/// - 웹: 브라우저가 센 mousedown 의 detail. (플랫폼이 글 입력 줄에 넣는다)
	//==============================================================================
	fn count_mouse_clicks(&mut self, context: &ComponentContext, view_input_position: Vector2) -> u32 {
		let input_manager = context.get_engine().get_input_manager();
		if input_manager.is_touch_input() || (!self.use_dom_input && !self.is_multiline) {
			#[cfg(not(target_arch = "wasm32"))]
			{
				self.last_click = None;
			}
			return 1;
		}
		#[cfg(target_arch = "wasm32")]
		{
			let _ = view_input_position;
			input_manager.get_text_input().get_pointer_click_count().max(1)
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			let view_scale = UIInputField::read_view_scale(context);
			let window_position = Vector2::create(view_input_position.x * view_scale.x, view_input_position.y * view_scale.y);
			let now = web_time::Instant::now();
			let interval_seconds = crate::platform::desktop::systemfont::read_double_click_seconds();
			let previous_click = self.last_click.map(|(time, position, click_count)| (now.duration_since(time).as_secs_f32(), position, click_count));
			let click_count = count_repeated_clicks(previous_click, window_position, interval_seconds, REPEATED_CLICK_SLOP);
			self.last_click = Some((now, window_position, click_count));
			click_count
		}
	}

	//==============================================================================
	/// 이어 누른 수대로 고르기. (블링크 SelectionController: 두 번은 낱말(윈도우는 뒤 공백까지), 세 번 넘게는 문단, 끌면 그 단위로 넓힌다)
	//==============================================================================
	fn select_by_click_count(&mut self, click_count: u32, character_index: usize, is_upstream: bool) {
		if click_count < 2 {
			return;
		}
		let text = self.edit_buffer.get_text().to_string();
		let (granularity, (start_index, mut end_index)) = if click_count == 2 {
			(SelectionGranularity::Word, find_word_range_at(&text, character_index, is_upstream))
		}
		else {
			(SelectionGranularity::Paragraph, find_paragraph_range_at(&text, character_index))
		};
		// 윈도우 크롬은 두 번 누른 낱말에 뒤 공백을 붙인다. (settings.json5 selectTrailingWhitespaceEnabled 의 Win true, visible_units.cc 의 SkipWhitespace)
		if granularity == SelectionGranularity::Word && is_windows_platform() && end_index > start_index {
			end_index = skip_trailing_whitespace(&text, end_index);
		}
		self.edit_buffer.set_selection(start_index, end_index);
		self.drag_granularity = Some((granularity, start_index, end_index));
		self.is_caret_upstream = false;
	}

	//==============================================================================
	/// 끌어 고르기. (단위가 있으면 블링크 ExtendSelectionAsDirectional 처럼 처음 범위를 지키고 끄는 자리의 낱말이나 문단 끝까지 넓힌다)
	//==============================================================================
	fn extend_drag_selection(&mut self, drag_start_index: usize, character_index: usize, is_upstream: bool) {
		let (granularity, start_index, end_index) = match self.drag_granularity {
			Some(drag_granularity) => {
				drag_granularity
			}
			None => {
				self.edit_buffer.set_selection(drag_start_index, character_index);
				return;
			}
		};
		let text = self.edit_buffer.get_text().to_string();
		let (unit_start, unit_end) = match granularity {
			SelectionGranularity::Word => {
				find_word_range_at(&text, character_index, is_upstream)
			}
			SelectionGranularity::Paragraph => {
				find_paragraph_range_at(&text, character_index)
			}
		};
		if character_index < start_index {
			self.edit_buffer.set_selection(end_index, unit_start);
		}
		else {
			self.edit_buffer.set_selection(start_index, unit_end.max(end_index));
		}
	}

	//==============================================================================
	/// 길게 누른 자리의 낱말 고르기. (블링크 SelectClosestWordFromHitTestResult 의 터치, 뒤 공백은 넣지 않는다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn select_word_at(&mut self, character_index: usize) {
		let text = self.edit_buffer.get_text().to_string();
		let character_count = count_characters(&text);
		let mut word_range: Option<(usize, usize)> = None;
		if character_index < character_count {
			let mut segment_start = 0;
			for (segment, _) in crate::core::component::text::segment_words(&text) {
				let segment_length = segment.chars().count();
				if character_index < segment_start + segment_length {
					let is_no_word = segment.starts_with('\n') || segment.chars().all(char::is_whitespace);
					if !is_no_word {
						word_range = Some((segment_start, segment_start + segment_length));
					}
					break;
				}
				segment_start += segment_length;
			}
		}
		match word_range {
			Some((start_index, end_index)) => {
				self.edit_buffer.set_selection(start_index, end_index);
			}
			None => {
				self.edit_buffer.set_selection(character_index, character_index);
			}
		}
		self.bump_value_revision();
	}

	//==============================================================================
	/// 길게 누르기 셈. (누른 채 650ms 이면 낱말을 고르고 손잡이를 보인다. 윈도우가 아니면 메뉴도 이때 정한다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn update_long_press(&mut self, context: &mut ComponentContext, time_delta: f32) {
		let mut long_press_state = match self.long_press_state {
			Some(long_press_state) if !long_press_state.is_fired => {
				long_press_state
			}
			_ => {
				return;
			}
		};
		if !context.get_engine().get_input_manager().is_touch_down() {
			self.long_press_state = None;
			return;
		}
		long_press_state.elapsed += time_delta;
		if long_press_state.elapsed < LONG_PRESS_SECONDS {
			self.long_press_state = Some(long_press_state);
			return;
		}
		long_press_state.is_fired = true;
		self.long_press_state = Some(long_press_state);
		self.drag_start_index = None;
		let local_position = context.get_node_tree().convert_view_position_to_node_position(context.get_node_id(), long_press_state.start_view_position).unwrap_or(Vector2::zero());
		let (character_index, _) = self.find_character_index_at_local(context, local_position);
		self.select_word_at(character_index);
		self.is_touch_handle_visible = true;
		self.reset_cursor_blink();
		if !cfg!(target_os = "windows") {
			self.handle_touch_context_menu(context, long_press_state.start_view_position);
		}
	}

	//==============================================================================
	/// 터치 문맥 메뉴 정하기. (고른 것이 있거나 붙여 넣을 수 있으면 빠른 메뉴 부탁, 아니면 손잡이를 숨기고 보통 문맥 메뉴)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn handle_touch_context_menu(&mut self, context: &mut ComponentContext, view_position: Vector2) {
		let has_selection = self.edit_buffer.has_selection();
		let can_paste = context.get_engine().read_clipboard_text().map(|clipboard_text| !clipboard_text.is_empty()).unwrap_or(false);
		if crate::ui::uiinputfieldmenu::is_quick_menu_available(has_selection, can_paste) {
			self.is_quick_menu_requested = true;
			return;
		}
		self.hide_touch_selection(context);
		self.show_context_menu(context, view_position);
	}

	//==============================================================================
	/// 빠른 메뉴 띄우고 닫기. (부탁이 있고, 손가락이 떨어져 있고, 손잡이를 끌지 않을 때 100ms 뒤 띄운다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn update_quick_menu(&mut self, context: &mut ComponentContext, time_delta: f32) {
		if let Some(quick_menu_node_id) = self.quick_menu_node_id {
			if !context.get_node_tree().contains(quick_menu_node_id) {
				self.quick_menu_node_id = None;
			}
		}
		let is_touch_down = context.get_engine().get_input_manager().is_touch_down();
		let should_show = self.is_quick_menu_requested && self.is_focused && !is_touch_down && self.dragging_touch_handle.is_none();
		if !should_show {
			self.hide_quick_menu(context);
			self.quick_menu_delay = None;
			return;
		}
		if self.quick_menu_node_id.is_some() {
			return;
		}
		let remaining_seconds = self.quick_menu_delay.unwrap_or(QUICK_MENU_DELAY_SECONDS) - time_delta;
		if remaining_seconds > 0.0 {
			self.quick_menu_delay = Some(remaining_seconds);
			return;
		}
		self.quick_menu_delay = None;
		let has_selection = self.edit_buffer.has_selection();
		let can_paste = context.get_engine().read_clipboard_text().map(|clipboard_text| !clipboard_text.is_empty()).unwrap_or(false);
		if !crate::ui::uiinputfieldmenu::is_quick_menu_available(has_selection, can_paste) {
			return;
		}
		let command_list = crate::ui::uiinputfieldmenu::build_quick_menu_command_list(has_selection, can_paste);
		let anchor_rect = self.read_selection_bound_view_rect(context);
		let handle_size = if has_selection {
			Vector2::create(TOUCH_HANDLE_SIZE + TOUCH_HANDLE_PADDING * 2.0, TOUCH_HANDLE_SIZE)
		}
		else {
			Vector2::create(TOUCH_CENTER_HANDLE_SIZE + TOUCH_HANDLE_PADDING * 2.0, TOUCH_CENTER_HANDLE_SIZE)
		};
		let view_scale = UIInputField::read_view_scale(context);
		let view_size = context.get_engine().get_view_manager().get_view_size();
		let owner_node_id = context.get_node_id();
		let quick_menu_node_id = crate::ui::uiinputfieldmenu::create_quick_menu_node(context.get_node_tree_mut(), owner_node_id, anchor_rect, handle_size, view_scale, view_size, &command_list);
		self.quick_menu_node_id = Some(quick_menu_node_id);
	}

	//==============================================================================
	/// 선택 경계 사이의 뷰 사각형. (크롬 RectFBetweenVisibleSelectionBounds: 두 경계 선분의 끝점을 감싼다. 커서 하나면 커서 선분)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn read_selection_bound_view_rect(&self, context: &ComponentContext) -> Rect {
		let world_matrix = context.get_node_tree().calculate_world_matrix(context.get_node_id());
		let mut minimum_position = Vector2::create(f32::MAX, f32::MAX);
		let mut maximum_position = Vector2::create(f32::MIN, f32::MIN);
		for (focus_point, _, _) in self.touch_handle_list.iter() {
			for point in [Vector2::create(focus_point.x, focus_point.y - self.read_touch_line_height()), *focus_point] {
				let view_point = world_matrix.transform_point(point.x, point.y);
				minimum_position = Vector2::create(minimum_position.x.min(view_point.x), minimum_position.y.min(view_point.y));
				maximum_position = Vector2::create(maximum_position.x.max(view_point.x), maximum_position.y.max(view_point.y));
			}
		}
		if minimum_position.x > maximum_position.x {
			return Rect::zero();
		}
		Rect::create(minimum_position.x, minimum_position.y, maximum_position.x - minimum_position.x, maximum_position.y - minimum_position.y)
	}

	//==============================================================================
	/// 선택 경계 선분의 높이. (한 줄은 선택 사각형 높이, 여러 줄은 줄 높이)
	//==============================================================================
	fn read_touch_line_height(&self) -> f32 {
		if self.is_multiline {
			self.text_area_layout.line_height
		}
		else {
			self.font_size * 1.3
		}
	}

	//==============================================================================
	/// 빠른 메뉴 명령 하기. (잘라내기, 복사, 붙여넣기는 손잡이를 숨기고 한다. "..." 는 선택 위 가운데에서 보통 문맥 메뉴)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn perform_quick_menu_command(&mut self, context: &mut ComponentContext, command: crate::ui::uiinputfieldmenu::QuickMenuCommand) {
		use crate::ui::uiinputfieldmenu::QuickMenuCommand;
		let anchor_rect = self.read_selection_bound_view_rect(context);
		self.hide_touch_selection(context);
		match command {
			QuickMenuCommand::Cut => {
				self.perform_menu_action(context, UIInputFieldMenuAction::Cut);
			}
			QuickMenuCommand::Copy => {
				self.perform_menu_action(context, UIInputFieldMenuAction::Copy);
			}
			QuickMenuCommand::Paste => {
				self.perform_menu_action(context, UIInputFieldMenuAction::Paste);
			}
			QuickMenuCommand::Ellipsis => {
				let anchor_point = Vector2::create(anchor_rect.position.x + anchor_rect.size.x * 0.5, anchor_rect.position.y);
				self.show_context_menu(context, anchor_point);
			}
		}
	}

	//==============================================================================
	/// 빠른 메뉴 명령 부탁. (빠른 메뉴가 부른다, 다음 tick 에 한다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn request_quick_menu_command(&mut self, command: crate::ui::uiinputfieldmenu::QuickMenuCommand) {
		self.pending_quick_menu_command = Some(command);
	}

	//==============================================================================
	/// 손잡이 그림 사각형. (노드 좌표, 왼쪽 손잡이는 오른쪽 위가, 오른쪽 손잡이는 왼쪽 위가, 커서 손잡이는 위 꼭짓점이 끝점)
	//==============================================================================
	fn calculate_touch_handle_rect(&self, focus_point: Vector2, is_start_handle: bool, is_caret_handle: bool) -> Rect {
		let unit = 1.0 / self.window_pixel_per_unit.max(1e-4);
		if is_caret_handle {
			let size = TOUCH_CENTER_HANDLE_SIZE * unit;
			Rect::create(focus_point.x - 19.5 * unit, focus_point.y, size, size)
		}
		else if is_start_handle {
			let size = TOUCH_HANDLE_SIZE * unit;
			Rect::create(focus_point.x - size, focus_point.y, size, size)
		}
		else {
			let size = TOUCH_HANDLE_SIZE * unit;
			Rect::create(focus_point.x, focus_point.y, size, size)
		}
	}

	//==============================================================================
	/// 그 자리의 손잡이. (누름 판정은 그림 사각형을 좌우와 아래로 6 DIP 넓힌 것)
	//==============================================================================
	fn find_touch_handle_at(&self, context: &ComponentContext, view_position: Vector2) -> Option<TouchHandleDrag> {
		let local_position = context.get_node_tree().convert_view_position_to_node_position(context.get_node_id(), view_position)?;
		let padding = TOUCH_HANDLE_PADDING / self.window_pixel_per_unit.max(1e-4);
		for (focus_point, is_start_handle, is_caret_handle) in self.touch_handle_list.iter() {
			let handle_rect = self.calculate_touch_handle_rect(*focus_point, *is_start_handle, *is_caret_handle);
			let is_inside = local_position.x >= handle_rect.position.x - padding && local_position.x <= handle_rect.position.x + handle_rect.size.x + padding && local_position.y >= handle_rect.position.y && local_position.y <= handle_rect.position.y + handle_rect.size.y + padding;
			if is_inside {
				return Some(TouchHandleDrag {
					is_start_handle: *is_start_handle,
					is_caret_handle: *is_caret_handle,
					focus_offset: Vector2::create(focus_point.x - local_position.x, focus_point.y - local_position.y),
				});
			}
		}
		None
	}

	//==============================================================================
	/// 손잡이 끌기. (손가락 자리에 누를 때의 끝점 차이를 더하고 줄 안쪽으로 반 줄 올린 자리로 그 끝을 옮긴다)
	//==============================================================================
	fn drag_touch_handle(&mut self, context: &ComponentContext, view_position: Vector2, touch_handle_drag: TouchHandleDrag) {
		let local_position = match context.get_node_tree().convert_view_position_to_node_position(context.get_node_id(), view_position) {
			Some(local_position) => {
				local_position
			}
			None => {
				return;
			}
		};
		let target_position = Vector2::create(local_position.x + touch_handle_drag.focus_offset.x, local_position.y + touch_handle_drag.focus_offset.y - self.read_touch_line_height() * 0.5);
		let (target_index, is_upstream) = self.find_character_index_at_local(context, target_position);
		if touch_handle_drag.is_caret_handle {
			self.edit_buffer.set_selection(target_index, target_index);
		}
		else {
			let cursor_index = self.edit_buffer.get_cursor_index();
			let (start_index, end_index) = self.edit_buffer.get_selection_range().unwrap_or((cursor_index, cursor_index));
			let fixed_index = if touch_handle_drag.is_start_handle {
				end_index
			}
			else {
				start_index
			};
			self.edit_buffer.set_selection(fixed_index, target_index);
		}
		self.is_caret_upstream = is_upstream;
		self.request_caret_reveal(CaretRevealKind::IfNeeded);
		self.bump_value_revision();
		self.reset_cursor_blink();
	}

	//==============================================================================
	/// 손잡이 끝점 두기와 그리기. (선택이 있으면 앞과 뒤 경계 아래 끝, 없으면 커서 아래 끝. 보일 때만 그린다)
	/// - 그림은 크롬 벡터 아이콘 꼴: 반지름 16 원의 한 귀퉁이가 뾰족한 물방울. 볼록한 꼴이라 한 번에 칠한다.
	//==============================================================================
	fn place_and_draw_touch_handles(&mut self, graphic: &mut Graphic, start_bottom: Vector2, end_bottom: Vector2, has_selection: bool) {
		self.touch_handle_list.clear();
		if has_selection {
			self.touch_handle_list.push((start_bottom, true, false));
			self.touch_handle_list.push((end_bottom, false, false));
		}
		else {
			self.touch_handle_list.push((end_bottom, false, true));
		}
		if !self.is_touch_handle_visible || !self.is_focused {
			return;
		}
		let unit = 1.0 / self.window_pixel_per_unit.max(1e-4);
		for (focus_point, is_start_handle, is_caret_handle) in self.touch_handle_list.clone() {
			let handle_rect = self.calculate_touch_handle_rect(focus_point, is_start_handle, is_caret_handle);
			let origin = handle_rect.position;
			let mut point_list: Vec<Vector2> = Vec::new();
			let step_count = 24;
			if is_caret_handle {
				// 위 꼭짓점 (19.5, 0), 원 가운데 (19.5, 22.63), 반지름 16, 접점 (8.19, 11.31) 과 (30.81, 11.31).
				let center = Vector2::create(origin.x + 19.5 * unit, origin.y + 22.63 * unit);
				point_list.push(Vector2::create(origin.x + 19.5 * unit, origin.y));
				let start_angle = -PI * 0.25;
				for step_index in 0..=step_count {
					let angle = start_angle + PI * 1.5 * step_index as f32 / step_count as f32;
					point_list.push(Vector2::create(center.x + 16.0 * unit * angle.cos(), center.y + 16.0 * unit * angle.sin()));
				}
			}
			else {
				let radius = handle_rect.size.x * 0.5;
				let center = Vector2::create(origin.x + radius, origin.y + radius);
				// 뾰족한 귀퉁이: 왼쪽 손잡이는 오른쪽 위, 오른쪽 손잡이는 왼쪽 위. 나머지는 원호 270 도.
				let (corner, start_angle) = if is_start_handle {
					(Vector2::create(origin.x + handle_rect.size.x, origin.y), 0.0)
				}
				else {
					(origin, PI * 1.5)
				};
				point_list.push(corner);
				for step_index in 0..=step_count {
					let angle = start_angle + PI * 1.5 * step_index as f32 / step_count as f32;
					point_list.push(Vector2::create(center.x + radius * angle.cos(), center.y + radius * angle.sin()));
				}
			}
			graphic.draw_convex_polygon(&point_list, TOUCH_HANDLE_COLOR);
		}
	}
}

//==============================================================================
/// page 이동 거리. (블링크 move_commands.cc 와 cc ScrollUtils::CalculatePageStep: 높이의 87.5% 와 높이 - 겹침 상한 가운데 큰 쪽, 겹침 상한은 맥 40, 그 밖은 없음)
//==============================================================================
fn calculate_page_step(visible_height: f32) -> f32 {
	let page_step = visible_height * 0.875;
	if cfg!(target_os = "macos") {
		page_step.max(visible_height - 40.0)
	}
	else {
		page_step
	}
}

#[crate::reflect]
impl Component for UIInputField {
	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (품은 컨트롤의 것)
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_control.get_component_base()
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_control.get_component_base_mut()
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("UIInputField")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIInputField"
	}

	//==============================================================================
	/// 붙을 때. (피봇과 앵커를 원본처럼 왼쪽 위로)
	//==============================================================================
	fn attach(&mut self, transform: &mut NodeTransform) {
		transform.set_pivot(Pivot::TOP_LEFT);
		transform.set_anchor(Pivot::TOP_LEFT);
	}

	//==============================================================================
	/// 트리에 붙을 때. (컨텐트 노드를 만들고 호스트 노드를 interactable 로 켠다)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_control.get_uiview_mut().create_content_node(node_tree, node_id);
		node_tree[node_id].set_interactable(true);
	}

	//==============================================================================
	/// 갱신. (문맥 메뉴 항목, 포커스 넘기기, 글 입력 줄에서 이 필드 몫 꺼내기, 길게 누르기와 빠른 메뉴, 커서 깜빡임, 플랫폼에 올릴 상태)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		// 문맥 메뉴에서 고른 항목.
		if let Some(menu_action) = self.pending_menu_action.take() {
			self.perform_menu_action(context, menu_action);
		}

		// 글 라벨을 놓은 크기. (원본 refreshTextPositions, 크기가 있을 때만)
		let content_size = context.get_transform().get_content_size();
		if content_size.x > 0.0 && content_size.y > 0.0 {
			self.text_layout_size = content_size;
		}
		if let Some(context_menu_node_id) = self.context_menu_node_id {
			if !context.get_node_tree().contains(context_menu_node_id) {
				self.context_menu_node_id = None;
			}
		}

		// 다른 필드가 나중에 포커스를 얻었으면 포커스를 내준다. (엔진에 포커스는 하나)
		let latest_focus_serial = context.get_engine().get_input_manager().get_text_input().get_latest_focus_serial();
		if self.is_focused && latest_focus_serial > self.focus_serial {
			self.handle_blur(context);
		}
		self.blur_on_outside_press(context);

		// 문맥 메뉴 요청. (데스크탑 오른쪽 단추. 그 자리 맨 위가 이 필드일 때)
		let context_menu_request_position = context.get_engine().get_input_manager().get_text_input().get_context_menu_request_position();
		if let Some(context_menu_request_position) = context_menu_request_position {
			let node_id = context.get_node_id();
			let topmost_node_id = context.get_node_tree().find_topmost_interactable_node(context_menu_request_position);
			if self.is_attached && topmost_node_id == Some(node_id) {
				self.handle_focus();
				self.show_context_menu(context, context_menu_request_position);
			}
		}

		// 글 입력 줄에서 이 필드 몫 꺼내기.
		let accepts_focused_field_events = self.is_focused && self.is_attached;
		let event_kind_list = context.get_engine().get_input_manager().get_text_input().take_events_for_field(self.field_id, accepts_focused_field_events);
		// 키와 글 입력, 휠은 터치 손잡이와 빠른 메뉴를 지운다. (크롬 EnvEventObserver)
		let is_typing = event_kind_list.iter().any(|event_kind| matches!(event_kind, TextInputEventKind::Commit(_) | TextInputEventKind::Key { .. } | TextInputEventKind::Paste(_) | TextInputEventKind::Preedit { .. }));
		if (is_typing || context.get_engine().get_input_manager().has_wheel_delta()) && (self.is_touch_handle_visible || self.is_quick_menu_requested) {
			self.hide_touch_selection(context);
		}
		for event_kind in event_kind_list {
			self.handle_text_input_event(context, event_kind);
		}

		// 손가락 길게 누르기와 빠른 메뉴. (데스크탑)
		#[cfg(not(target_arch = "wasm32"))]
		{
			self.update_long_press(context, time_delta);
			if let Some(quick_menu_command) = self.pending_quick_menu_command.take() {
				self.perform_quick_menu_command(context, quick_menu_command);
			}
			self.update_quick_menu(context, time_delta);
		}

		// 여러 줄: 커서가 보이게 스크롤. (그리기 전에도 스크롤 자리가 맞게 tick 에서 한다)
		if self.is_multiline && self.caret_reveal_request.is_some() {
			let content_size = context.get_transform().get_content_size();
			self.ensure_text_area_layout_without_graphic(content_size);
			self.apply_caret_reveal(content_size.y);
		}

		// 커서 깜빡임.
		if self.is_focused {
			self.blink_timer += time_delta;
			if self.blink_timer >= CURSOR_BLINK_SECONDS {
				self.blink_timer = 0.0;
				self.is_cursor_visible = !self.is_cursor_visible;
			}
		}
		else {
			self.is_cursor_visible = true;
		}

		// 플랫폼에 올릴 상태. (노드 한 단위의 창 픽셀도 둔다)
		if self.is_attached || self.is_focused {
			let field_state = self.build_field_state(context);
			let content_width = context.get_transform().get_content_size().x;
			if content_width > 0.0 && field_state.field_rect.size.x > 0.0 {
				self.window_pixel_per_unit = field_state.field_rect.size.x / content_width;
			}
			context.get_engine().get_input_manager().get_text_input().register_field_state(field_state);
		}
	}

	//==============================================================================
	/// 누름. (조합 글을 확정하고 포커스를 얻는다. 누른 자리로 커서를 옮기고 이어 누른 수대로 낱말이나 문단을 고른다, 여러 줄 필드의 썸은 끌기 시작, 손가락은 길게 누르기를 잰다)
	//==============================================================================
	fn touch_press(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		// 조합 중에 다른 자리를 누르면 조합을 확정하고 IME 를 끊는다. (웹 DOM 입력은 브라우저가 한다)
		if !self.edit_buffer.get_composition().is_empty() && !self.is_dom_synchronized() {
			let composition = self.edit_buffer.get_composition().to_string();
			self.edit_buffer.set_composition("");
			self.is_composing = false;
			let text_input = context.get_engine().get_input_manager().get_text_input();
			text_input.set_ime_commit_guard(&composition);
			text_input.request_ime_reset();
			self.replace_selection(context, &composition);
		}
		self.focus();
		self.hide_context_menu(context);
		self.preferred_caret_x = None;
		self.scrollbar_drag_start = None;
		// 손가락: 손잡이를 누르면 끌고, 아니면 손잡이와 빠른 메뉴를 지우고 길게 누르기를 잰다. 마우스 누름은 지우기만.
		if self.is_long_press_target() && context.get_engine().get_input_manager().is_touch_input() {
			if self.is_touch_handle_visible {
				if let Some(touch_handle_drag) = self.find_touch_handle_at(context, view_input_position) {
					self.dragging_touch_handle = Some(touch_handle_drag);
					self.hide_quick_menu(context);
					return;
				}
			}
			self.hide_touch_selection(context);
			self.long_press_state = Some(LongPressState {
				start_view_position: view_input_position,
				elapsed: 0.0,
				is_fired: false,
			});
		}
		else {
			self.hide_touch_selection(context);
			self.long_press_state = None;
		}
		self.drag_granularity = None;
		let click_count = self.count_mouse_clicks(context, view_input_position);
		if self.is_multiline {
			// 썸을 누르면 끌어 스크롤한다. (선택은 그대로)
			let content_size = context.get_transform().get_content_size();
			if let Some(thumb_rect) = self.calculate_text_area_thumb_rect(content_size) {
				let local_position = context.get_node_tree().convert_view_position_to_node_position(context.get_node_id(), view_input_position).unwrap_or(Vector2::zero());
				let is_on_thumb = local_position.x >= thumb_rect.position.x && local_position.y >= thumb_rect.position.y && local_position.y <= thumb_rect.position.y + thumb_rect.size.y;
				if is_on_thumb {
					self.scrollbar_drag_start = Some((local_position.y, self.scroll_top));
					return;
				}
			}
			let (character_index, is_upstream) = self.find_text_area_index_at(context, view_input_position);
			self.drag_start_index = Some(character_index);
			self.edit_buffer.set_selection(character_index, character_index);
			self.is_caret_upstream = is_upstream;
			self.select_by_click_count(click_count, character_index, is_upstream);
			self.bump_value_revision();
			self.reset_cursor_blink();
			return;
		}
		let local_x = self.convert_view_position_to_local_x(context, view_input_position);
		let character_index = self.measure_character_index_at_local_x(local_x);
		self.drag_start_index = Some(character_index);
		self.edit_buffer.set_selection(character_index, character_index);
		self.select_by_click_count(click_count, character_index, false);
		self.bump_value_revision();
		self.reset_cursor_blink();
	}

	//==============================================================================
	/// 끌기. (터치 손잡이 끌기, 길게 누르기 허용 거리 재기, 썸 끌어 스크롤, 누른 자리에서 끈 자리까지 고르기)
	//==============================================================================
	fn touch_move(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if let Some(touch_handle_drag) = self.dragging_touch_handle {
			self.drag_touch_handle(context, view_input_position, touch_handle_drag);
			return;
		}
		if let Some(long_press_state) = self.long_press_state {
			// 길게 누른 뒤의 움직임은 선택을 바꾸지 않는다. 그 전에 허용 거리를 넘으면 길게 누르기가 아니다.
			if long_press_state.is_fired {
				return;
			}
			let view_scale = UIInputField::read_view_scale(context);
			let delta_x = (view_input_position.x - long_press_state.start_view_position.x) * view_scale.x;
			let delta_y = (view_input_position.y - long_press_state.start_view_position.y) * view_scale.y;
			if delta_x * delta_x + delta_y * delta_y > LONG_PRESS_TOUCH_SLOP * LONG_PRESS_TOUCH_SLOP {
				self.long_press_state = None;
			}
		}
		if let Some((start_y, start_scroll_top)) = self.scrollbar_drag_start {
			// 썸을 옮긴 만큼을 글 높이의 비로 넘긴다.
			let content_size = context.get_transform().get_content_size();
			let local_position = context.get_node_tree().convert_view_position_to_node_position(context.get_node_id(), view_input_position).unwrap_or(Vector2::zero());
			let scroll_height = self.text_area_layout.get_content_height() + self.padding * 2.0;
			let classic_metrics = if self.text_area_scrollbar_gutter > 0.0 {
				crate::web::pane::read_classic_scrollbar_metrics(false)
			}
			else {
				None
			};
			let track_room = match classic_metrics {
				Some(metrics) => {
					let thumb_room = (content_size.y - metrics.button_length * 2.0).max(1.0);
					let thumb_length = (thumb_room * content_size.y / scroll_height.max(1.0)).round().max(metrics.thumb_minimum_length).min(thumb_room);
					(thumb_room - thumb_length).max(1.0)
				}
				None => {
					let thumb_length = (content_size.y * content_size.y / scroll_height.max(1.0)).max(TEXT_AREA_SCROLLBAR_MINIMUM_THUMB_LENGTH).min(content_size.y);
					(content_size.y - thumb_length).max(1.0)
				}
			};
			let maximum_scroll_top = self.get_maximum_scroll_top(content_size.y);
			self.scroll_top = (start_scroll_top + (local_position.y - start_y) / track_room * maximum_scroll_top).clamp(0.0, maximum_scroll_top);
			return;
		}
		let drag_start_index = match self.drag_start_index {
			Some(drag_start_index) => {
				drag_start_index
			}
			None => {
				return;
			}
		};
		if self.is_multiline {
			// 끄는 끝이 보이게 스크롤한다. (크롬의 끌기 자동 스크롤 자리)
			let (character_index, is_upstream) = self.find_text_area_index_at(context, view_input_position);
			self.extend_drag_selection(drag_start_index, character_index, is_upstream);
			self.is_caret_upstream = is_upstream;
			self.request_caret_reveal(CaretRevealKind::IfNeeded);
			self.bump_value_revision();
			self.reset_cursor_blink();
			return;
		}
		let local_x = self.convert_view_position_to_local_x(context, view_input_position);
		let character_index = self.measure_character_index_at_local_x(local_x);
		self.extend_drag_selection(drag_start_index, character_index, false);
		self.bump_value_revision();
		self.reset_cursor_blink();
	}

	//==============================================================================
	/// 뗌. (끌기를 끝낸다. 윈도우는 길게 누른 손가락을 뗄 때 메뉴를 정한다)
	//==============================================================================
	fn touch_release(&mut self, context: &mut ComponentContext, _view_input_position: Vector2) {
		self.drag_start_index = None;
		self.scrollbar_drag_start = None;
		self.dragging_touch_handle = None;
		// 윈도우는 길게 누른 손가락을 뗄 때 메뉴를 정한다. (블링크 context_menu_on_mouse_up, LongTap)
		if let Some(long_press_state) = self.long_press_state.take() {
			#[cfg(not(target_arch = "wasm32"))]
			{
				if long_press_state.is_fired && cfg!(target_os = "windows") {
					self.handle_touch_context_menu(context, long_press_state.start_view_position);
				}
			}
			let _ = (long_press_state, &context);
		}
	}

	//==============================================================================
	/// 누름 취소. (끌기와 길게 누르기를 버린다)
	//==============================================================================
	fn touch_cancel(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		self.drag_start_index = None;
		self.scrollbar_drag_start = None;
		self.dragging_touch_handle = None;
		self.long_press_state = None;
	}

	//==============================================================================
	/// 휠을 받는지. (여러 줄 필드만)
	//==============================================================================
	#[reflect(skip)]
	fn is_wheel_receiver(&self) -> bool {
		self.is_multiline
	}

	//==============================================================================
	/// 휠. (여러 줄 필드: 더 넘길 수 있을 때만 받고, 끝이면 부모에게 넘긴다. 원본 textarea 스크롤 상자)
	//==============================================================================
	fn wheel(&mut self, context: &mut ComponentContext, wheel_delta: Vector2) -> bool {
		if !self.is_multiline {
			return false;
		}
		let content_size = context.get_transform().get_content_size();
		self.ensure_text_area_layout_without_graphic(content_size);
		let previous_scroll_top = self.scroll_top;
		self.scroll_top = (self.scroll_top + wheel_delta.y).clamp(0.0, self.get_maximum_scroll_top(content_size.y));
		self.scroll_top != previous_scroll_top
	}

	//==============================================================================
	/// 그리기. (바탕, 선택 영역, 글, 조합 밑줄, 커서, 포커스 테두리 차례, 원본 층 순서)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		// 1) 바탕.
		self.background_paint.draw_paint(transform, graphic);
		let content_size = transform.get_content_size();
		if content_size.x <= 0.0 || content_size.y <= 0.0 {
			return;
		}
		if self.is_multiline {
			self.draw_multiline(content_size, graphic);
			if self.is_focused && self.focus_border_width > 0.0 {
				let border_width = self.focus_border_width;
				let inset = border_width * 0.5;
				let round_size = (self.background_paint.get_round_size() - inset).max(0.0);
				graphic.set_stroke_color(self.focus_border_color);
				graphic.draw_stroke_round_rect(Rect::create(inset, inset, content_size.x - border_width, content_size.y - border_width), round_size, border_width);
			}
			return;
		}
		self.caret_local_top = 0.0;
		self.caret_local_height = content_size.y;
		let font_string = self.build_font_string();
		let draw_y = content_size.y * 0.5;
		let padding = self.padding;
		let font_size = self.font_size;

		// 2) 선택 영역. (조합 중엔 그리지 않는다)
		graphic.push_state();
		graphic.set_font_string(&font_string);
		self.update_character_offsets(graphic, &font_string);
		if self.edit_buffer.get_composition().is_empty() {
			if let Some((start_index, end_index)) = self.edit_buffer.get_selection_range() {
				let start_x = padding + self.get_character_offset(start_index);
				let end_x = padding + self.get_character_offset(end_index);
				let half_height = font_size * 0.65;
				graphic.set_fill_color(self.selection_color);
				graphic.draw_rect(Rect::create(start_x, draw_y - half_height, end_x - start_x, half_height * 2.0));
			}
		}
		graphic.pop_state();

		// 3) 글. (값이나 조합 글이 있으면 값, 없으면 안내 글. 원본 라벨 노드처럼 여백만큼 안쪽 영역에 왼쪽 정렬)
		let inner_size = Vector2::create((content_size.x - padding * 2.0).max(0.0), content_size.y);
		let mut inner_transform = NodeTransform::new();
		inner_transform.set_content_size(inner_size);
		let has_value = !self.edit_buffer.get_text().is_empty() || !self.edit_buffer.get_composition().is_empty();
		graphic.push_state();
		graphic.translate(padding, 0.0);
		if has_value {
			let display_text = self.edit_buffer.build_display_text();
			self.value_text.set_text(&display_text);
			self.value_text.draw(&inner_transform, graphic);
		}
		else {
			self.placeholder_text.set_text(&self.placeholder);
			self.placeholder_text.draw(&inner_transform, graphic);
		}
		graphic.pop_state();

		// 4) 조합 밑줄, 커서, 포커스 테두리.
		graphic.push_state();
		graphic.set_font_string(&font_string);
		let cursor_index = self.edit_buffer.get_cursor_index();
		let before_width = self.get_character_offset(cursor_index);
		let composition = self.edit_buffer.get_composition().to_string();
		let cursor_x = if composition.is_empty() {
			padding + before_width
		}
		else {
			let composition_width = graphic.measure_text(&composition).width;
			let underline_y = draw_y + font_size * 0.55;
			graphic.set_stroke_color(self.composition_underline_color);
			graphic.draw_line(&[Vector2::create(padding + before_width, underline_y), Vector2::create(padding + before_width + composition_width, underline_y)], 2.0);
			let cursor_before_text = format!("{}{}", self.edit_buffer.get_text_before_cursor(), composition);
			padding + graphic.measure_text(&cursor_before_text).width
		};
		self.caret_local_x = cursor_x;
		// 터치 선택 손잡이. (선택 사각형 아래 끝)
		{
			let selection_bottom = draw_y + font_size * 0.65;
			let selection_range = self.edit_buffer.get_selection_range();
			let (start_x, end_x) = match selection_range {
				Some((start_index, end_index)) => {
					(padding + self.get_character_offset(start_index), padding + self.get_character_offset(end_index))
				}
				None => {
					(cursor_x, cursor_x)
				}
			};
			self.place_and_draw_touch_handles(graphic, Vector2::create(start_x, selection_bottom), Vector2::create(end_x, selection_bottom), selection_range.is_some());
		}
		if self.is_focused && self.is_cursor_visible {
			let half_height = font_size * 0.6;
			graphic.set_stroke_color(self.cursor_color);
			graphic.draw_line(&[Vector2::create(cursor_x, draw_y - half_height), Vector2::create(cursor_x, draw_y + half_height)], 2.0);
		}
		if self.is_focused && self.focus_border_width > 0.0 {
			let border_width = self.focus_border_width;
			let inset = border_width * 0.5;
			let round_size = (self.background_paint.get_round_size() - inset).max(0.0);
			graphic.set_stroke_color(self.focus_border_color);
			graphic.draw_stroke_round_rect(Rect::create(inset, inset, content_size.x - border_width, content_size.y - border_width), round_size, border_width);
		}
		graphic.pop_state();
	}
}

//==============================================================================
/// 노드와 조상이 모두 보이는지. (원본 isVisibleInHierarchy)
//==============================================================================
fn is_node_visible_in_hierarchy(node_tree: &NodeTree, node_id: NodeId) -> bool {
	let mut current_id = Some(node_id);
	while let Some(candidate_id) = current_id {
		match node_tree.get_node(candidate_id) {
			Some(node) => {
				if !node.is_visible() || !node.is_active() {
					return false;
				}
			}
			None => {
				return false;
			}
		}
		current_id = node_tree.get_parent_id(candidate_id);
	}
	true
}

/// 문맥 메뉴 항목. (글, 항목, 원본 showContextMenu 의 차례)
const CONTEXT_MENU_ITEM_LIST: [(&str, UIInputFieldMenuAction); 4] = [
	("잘라내기", UIInputFieldMenuAction::Cut),
	("복사", UIInputFieldMenuAction::Copy),
	("붙여넣기", UIInputFieldMenuAction::Paste),
	("모두 선택", UIInputFieldMenuAction::SelectAll),
];

//==============================================================================
/// 캔버스 문맥 메뉴. (데스크탑. 원본 showContextMenu 의 DOM 메뉴를 같은 모양으로 그린다)
/// - 루트의 자식 노드에 붙는다. 항목을 누르면 주인 필드에 항목을 부탁하고(다음 tick 에 한다) 스스로 지운다.
/// - 메뉴 밖을 누르거나, 다른 자리에 문맥 메뉴를 부르거나, 주인 필드가 포커스를 잃거나 없어지면 스스로 지운다.
//==============================================================================
struct UIInputFieldContextMenu {
	component_base: ComponentBase,
	owner_node_id: NodeId,
	is_selection_available: bool,
	is_text_available: bool,
	hovered_item_index: Option<usize>,
	is_first_tick: bool,
	/// 그림자의 구운 그림.
	shadow: BlurredRoundRectShadow,
}

#[crate::reflect]
impl UIInputFieldContextMenu {
	//==============================================================================
	/// 생성. (선택이 없으면 잘라내기와 복사, 글이 없으면 모두 선택은 흐리게)
	//==============================================================================
	fn new(owner_node_id: NodeId, is_selection_available: bool, is_text_available: bool) -> UIInputFieldContextMenu {
		UIInputFieldContextMenu {
			component_base: ComponentBase::new(),
			owner_node_id,
			is_selection_available,
			is_text_available,
			hovered_item_index: None,
			is_first_tick: true,
			shadow: BlurredRoundRectShadow::default(),
		}
	}

	//==============================================================================
	/// 항목을 쓸 수 있는지.
	//==============================================================================
	fn is_item_enabled(&self, menu_action: UIInputFieldMenuAction) -> bool {
		match menu_action {
			UIInputFieldMenuAction::Cut | UIInputFieldMenuAction::Copy => {
				self.is_selection_available
			}
			UIInputFieldMenuAction::SelectAll => {
				self.is_text_available
			}
			UIInputFieldMenuAction::Paste => {
				true
			}
		}
	}

	//==============================================================================
	/// 뷰 좌표 아래 항목 자리. (없으면 None)
	//==============================================================================
	fn find_item_index(&self, context: &ComponentContext, view_position: Vector2) -> Option<usize> {
		let node_id = context.get_node_id();
		let local_position = context.get_node_tree().convert_view_position_to_node_position(node_id, view_position)?;
		if local_position.x < 0.0 || local_position.x > CONTEXT_MENU_WIDTH {
			return None;
		}
		let item_y = local_position.y - CONTEXT_MENU_PADDING_Y;
		if item_y < 0.0 {
			return None;
		}
		let item_index = (item_y / CONTEXT_MENU_ITEM_HEIGHT) as usize;
		if item_index >= CONTEXT_MENU_ITEM_LIST.len() {
			return None;
		}
		Some(item_index)
	}

	//==============================================================================
	/// 메뉴 닫기. (자기 노드를 지운다)
	//==============================================================================
	fn close(&self, context: &mut ComponentContext) {
		let node_id = context.get_node_id();
		context.get_node_tree_mut().destroy_node(node_id);
	}
}

#[crate::reflect]
impl Component for UIInputFieldContextMenu {
	//==============================================================================
	/// 컴포넌트 공통 칸 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("UIInputFieldContextMenu")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIInputFieldContextMenu"
	}

	//==============================================================================
	/// 트리에 붙을 때. (노드를 interactable 로 켠다)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		node_tree[node_id].set_interactable(true);
	}

	//==============================================================================
	/// 갱신. (필드가 포커스를 잃었거나 메뉴 밖을 누르면 닫고, 마우스 아래 항목을 둔다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		let is_owner_ready = match context.get_node_tree().get_node(self.owner_node_id).and_then(|owner_node| owner_node.get_component::<UIInputField>()) {
			Some(owner_field) => {
				owner_field.is_focused() && owner_field.is_attached()
			}
			None => {
				false
			}
		};
		if !is_owner_ready {
			self.close(context);
			return;
		}
		let input_manager = context.get_engine().get_input_manager();
		let view_input_position = input_manager.get_view_input_position();
		let is_touch_pressed = input_manager.is_touch_pressed();
		let context_menu_request_position = input_manager.get_text_input().get_context_menu_request_position();
		let node_id = context.get_node_id();
		self.hovered_item_index = self.find_item_index(context, view_input_position);
		if !self.is_first_tick {
			let is_pressed_outside = is_touch_pressed && !is_view_position_inside_node(context.get_node_tree(), node_id, view_input_position);
			let is_requested_outside = match context_menu_request_position {
				Some(request_position) => {
					!is_view_position_inside_node(context.get_node_tree(), node_id, request_position)
				}
				None => {
					false
				}
			};
			if is_pressed_outside || is_requested_outside {
				self.close(context);
				return;
			}
		}
		self.is_first_tick = false;
	}

	//==============================================================================
	/// 누름. (쓸 수 있는 항목이면 필드에 그 항목을 부탁하고 닫는다)
	//==============================================================================
	fn touch_press(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		let item_index = match self.find_item_index(context, view_input_position) {
			Some(item_index) => {
				item_index
			}
			None => {
				return;
			}
		};
		let menu_action = CONTEXT_MENU_ITEM_LIST[item_index].1;
		if !self.is_item_enabled(menu_action) {
			return;
		}
		let owner_node_id = self.owner_node_id;
		if let Some(owner_field) = context.get_node_tree_mut().get_node_mut(owner_node_id).and_then(|owner_node| owner_node.get_component_mut::<UIInputField>()) {
			owner_field.request_menu_action(menu_action);
		}
		self.close(context);
	}

	//==============================================================================
	/// 그리기. (그림자, 흰 판, 테두리, 항목 글, 마우스 아래 항목 바탕)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		let menu_rect = Rect::create(0.0, 0.0, content_size.x, content_size.y);
		graphic.push_state();
		// 원본 CSS box-shadow 0 4px 16px rgba(0, 0, 0, 0.18) 을 블링크처럼 시그마 8 의 가우스로. (테두리 상자 모양)
		self.shadow.draw(graphic, menu_rect, 8.0, Vector2::create(0.0, 4.0), css_blur_radius_to_sigma(16.0), Color::create(0.0, 0.0, 0.0, 0.18));
		graphic.set_fill_color("#ffffff");
		graphic.draw_round_rect(menu_rect, 8.0);
		graphic.set_stroke_color("#cccccc");
		graphic.draw_stroke_round_rect(Rect::create(0.5, 0.5, content_size.x - 1.0, content_size.y - 1.0), 7.5, 1.0);
		graphic.set_font_string(CONTEXT_MENU_FONT_STRING);
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Middle);
		for (item_index, (item_text, menu_action)) in CONTEXT_MENU_ITEM_LIST.iter().enumerate() {
			let item_top = CONTEXT_MENU_PADDING_Y + item_index as f32 * CONTEXT_MENU_ITEM_HEIGHT;
			let is_enabled = self.is_item_enabled(*menu_action);
			if is_enabled && self.hovered_item_index == Some(item_index) {
				graphic.set_fill_color("#eef2ff");
				graphic.draw_rect(Rect::create(1.0, item_top, content_size.x - 2.0, CONTEXT_MENU_ITEM_HEIGHT));
			}
			let text_color = if is_enabled {
				"#222222"
			}
			else {
				"#aaaaaa"
			};
			graphic.set_fill_color(text_color);
			graphic.draw_fill_text(item_text, CONTEXT_MENU_ITEM_PADDING_X, item_top + CONTEXT_MENU_ITEM_HEIGHT * 0.5);
		}
		graphic.pop_state();
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
	/// 시험 장면. (왼쪽 위 (100, 100), 크기 300 x 60 인 필드, DOM 입력 + attach)
	//==============================================================================
	fn create_field_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let field_id = node_tree.create_child(root_id, "field");
		node_tree[field_id].set_local_position(Vector2::create(100.0, 100.0));
		node_tree[field_id].set_content_size(Vector2::create(300.0, 60.0));
		let field = node_tree.add_component(field_id, UIInputField::new()).expect("field");
		field.set_use_dom_input(true);
		field.attach();
		(node_tree, field_id)
	}

	//==============================================================================
	/// 필드 반환.
	//==============================================================================
	fn get_field(node_tree: &mut NodeTree, field_id: NodeId) -> &mut UIInputField {
		node_tree[field_id].get_component_mut::<UIInputField>().expect("field")
	}

	//==============================================================================
	/// 포커스를 가진 필드에 갈 이벤트 넣기.
	//==============================================================================
	fn push_event(engine: &mut Engine, event_kind: TextInputEventKind) {
		engine.get_input_manager_mut().get_text_input_mut().push_focused_field_event(event_kind);
	}

	//==============================================================================
	/// 키 이벤트.
	//==============================================================================
	fn key_event(key: TextInputKey, is_shift: bool) -> TextInputEventKind {
		TextInputEventKind::Key {
			key,
			is_shift,
		}
	}

	//==============================================================================
	/// 한 프레임. (트리 tick 다음 엔진 프레임 끝처럼 글 입력 줄 마무리)
	//==============================================================================
	fn run_frame(node_tree: &mut NodeTree, engine: &mut Engine) {
		node_tree.tick(engine, 1.0 / 60.0);
		engine.get_input_manager_mut().get_text_input_mut().finish_frame();
	}

	//==============================================================================
	/// 되돌리기 묶음: 이어 친 글자는 한 번에, 뒤로 지우기는 새 묶음이고 그 뒤 친 글자는 그 묶음에 붙는다, 붙여넣기는 따로, 커서를 옮기면 끊긴다. (블링크 TypingCommand)
	//==============================================================================
	#[test]
	fn edit_buffer_undo_groups_like_blink() {
		let mut edit_buffer = TextEditBuffer::new();
		for character in ["a", "b", "c"] {
			edit_buffer.begin_edit(TextEditKind::InsertText);
			edit_buffer.replace_selection(character);
		}
		for _ in 0..2 {
			edit_buffer.begin_edit(TextEditKind::DeleteBackward);
			edit_buffer.delete_backward();
		}
		edit_buffer.begin_edit(TextEditKind::InsertText);
		edit_buffer.replace_selection("d");
		assert_eq!(edit_buffer.get_text(), "ad");
		assert!(edit_buffer.undo());
		assert_eq!(edit_buffer.get_text(), "abc");
		assert!(edit_buffer.undo());
		assert_eq!(edit_buffer.get_text(), "");
		assert!(!edit_buffer.undo());
		assert!(edit_buffer.redo());
		assert_eq!(edit_buffer.get_text(), "abc");
		assert!(edit_buffer.redo());
		assert_eq!(edit_buffer.get_text(), "ad");
		assert!(!edit_buffer.can_redo());

		// 커서를 옮기면 끊기고, 붙여넣기는 따로 한 묶음이며 새 편집은 다시 하기를 지운다.
		edit_buffer.move_cursor(0, false);
		edit_buffer.begin_edit(TextEditKind::InsertText);
		edit_buffer.replace_selection("x");
		edit_buffer.begin_edit(TextEditKind::Paste);
		edit_buffer.replace_selection("PP");
		edit_buffer.begin_edit(TextEditKind::InsertText);
		edit_buffer.replace_selection("y");
		assert_eq!(edit_buffer.get_text(), "xPPyad");
		assert!(edit_buffer.undo());
		assert_eq!(edit_buffer.get_text(), "xPPad");
		assert!(edit_buffer.undo());
		assert_eq!(edit_buffer.get_text(), "xad");
		assert!(edit_buffer.undo());
		assert_eq!(edit_buffer.get_text(), "ad");
		edit_buffer.begin_edit(TextEditKind::InsertText);
		edit_buffer.replace_selection("z");
		assert!(!edit_buffer.can_redo());
		edit_buffer.set_text("reset");
		assert!(!edit_buffer.can_undo());
	}

	//==============================================================================
	/// DOM 입력 필드의 되돌리기 키. (캔버스 모드는 원본처럼 되돌리지 않는다)
	//==============================================================================
	#[test]
	fn undo_keys_in_dom_input_field() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		get_field(&mut node_tree, field_id).focus();
		for character in ["h", "i"] {
			push_event(&mut engine, TextInputEventKind::Commit(String::from(character)));
		}
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "hi");
		assert!(get_field(&mut node_tree, field_id).can_undo());
		push_event(&mut engine, TextInputEventKind::Key { key: TextInputKey::Undo, is_shift: false });
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "");
		push_event(&mut engine, TextInputEventKind::Key { key: TextInputKey::Redo, is_shift: true });
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "hi");
		get_field(&mut node_tree, field_id).set_use_dom_input(false);
		push_event(&mut engine, TextInputEventKind::Key { key: TextInputKey::Undo, is_shift: false });
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "hi");
	}

	//==============================================================================
	/// 직접 누르는 DOM 입력 필드의 오른쪽 클릭도 원본처럼 엔진 문맥 메뉴(잘라내기, 복사, 붙여넣기, 모두 선택)를 연다. 모두 선택 항목은 글을 모두 고른다.
	//==============================================================================
	#[test]
	fn direct_touch_field_opens_engine_context_menu() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		let root_id = node_tree.get_root_id();
		get_field(&mut node_tree, field_id).set_use_direct_touch(true);
		get_field(&mut node_tree, field_id).set_text("hello");
		engine.get_input_manager_mut().get_text_input_mut().set_context_menu_request_position(Some(Vector2::create(200.0, 120.0)));
		run_frame(&mut node_tree, &mut engine);
		let menu_id = node_tree.find_child_by_name(root_id, "uiInputFieldContextMenu").expect("engine context menu");
		assert_eq!(node_tree[menu_id].get_content_size(), Vector2::create(CONTEXT_MENU_WIDTH, CONTEXT_MENU_PADDING_Y * 2.0 + CONTEXT_MENU_ITEM_HEIGHT * 4.0));
		get_field(&mut node_tree, field_id).request_menu_action(UIInputFieldMenuAction::SelectAll);
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_selection_text(), "hello");
	}

	//==============================================================================
	/// 손가락 길게 누르기: 650ms 동안 허용 거리 안이면 낱말을 고르고 손잡이를 보인다. 메뉴는 윈도우는 뗄 때, 그 밖은 그때 정하고,
	/// 고른 것이 있으면 뗀 뒤 100ms 에 빠른 메뉴(잘라내기, 복사, ..., 붙여넣기는 클립보드에 따라). 허용 거리를 넘으면 길게 누르기가 아니다.
	//==============================================================================
	#[test]
	fn long_press_selects_word_and_opens_quick_menu() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		let root_id = node_tree.get_root_id();
		{
			let field = get_field(&mut node_tree, field_id);
			field.set_use_direct_touch(true);
			field.set_font_size(20.0);
			field.set_text("hello world");
		}
		// 글자 경계를 재게 한 번 그린 것처럼 둔다. (테스트에는 Graphic 이 없어 Graphic 없는 재기로 같은 글자 경계를 만든다)
		{
			let field = get_field(&mut node_tree, field_id);
			let font_string = field.build_font_string();
			let value = field.edit_buffer.get_text().to_string();
			let mut character_offsets = vec![0.0];
			for (byte_index, character) in value.char_indices() {
				let prefix_end = byte_index + character.len_utf8();
				character_offsets.push(crate::core::textstringtexturecache::measure_text_without_graphic(&font_string, &value[..prefix_end]).map(|text_metrics| text_metrics.width).unwrap_or(prefix_end as f32 * 11.0));
			}
			field.character_offsets = character_offsets;
			field.measured_value = value;
			field.measured_font_string = font_string;
		}
		let word_offset = get_field(&mut node_tree, field_id).character_offsets[8];
		let press_position = Vector2::create(100.0 + 16.0 + word_offset, 130.0);
		let press = |node_tree: &mut NodeTree, engine: &mut Engine, position: Vector2| {
			let input_manager = engine.get_input_manager_mut();
			input_manager.set_touch_input(true);
			input_manager.set_view_input_position(position);
			input_manager.set_touch_down(true);
			node_tree.dispatch_touch(field_id, engine, TouchKind::Press, position);
		};
		press(&mut node_tree, &mut engine, press_position);
		for _ in 0..30 {
			node_tree.tick(&engine, 0.02);
		}
		assert!(!get_field(&mut node_tree, field_id).has_selection());
		for _ in 0..10 {
			node_tree.tick(&engine, 0.02);
		}
		// 0.8 초: 낱말 world 를 고르고 손잡이를 보인다.
		{
			let field = get_field(&mut node_tree, field_id);
			assert_eq!(field.get_selection_text(), "world");
			assert!(field.is_touch_handle_visible);
		}
		// 뗀다. 고른 것이 있으니 빠른 메뉴 부탁이고, 손가락이 떨어진 뒤 100ms 에 뜬다.
		engine.get_input_manager_mut().set_touch_down(false);
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Release, press_position);
		assert!(get_field(&mut node_tree, field_id).is_quick_menu_requested);
		node_tree.tick(&engine, 0.05);
		assert!(node_tree.find_child_by_name(root_id, "uiInputFieldQuickMenu").is_none());
		node_tree.tick(&engine, 0.06);
		let quick_menu_id = node_tree.find_child_by_name(root_id, "uiInputFieldQuickMenu").expect("quick menu");
		// 메뉴는 선택 위에 뜬다.
		assert!(node_tree.get_position(quick_menu_id).y < 100.0);
		// 키를 치면 손잡이와 메뉴가 사라진다.
		engine.get_input_manager_mut().get_text_input_mut().push_focused_field_event(TextInputEventKind::Key { key: TextInputKey::Right, is_shift: false });
		node_tree.tick(&engine, 0.02);
		node_tree.tick(&engine, 0.02);
		assert!(!get_field(&mut node_tree, field_id).is_touch_handle_visible);
		assert!(node_tree.find_child_by_name(root_id, "uiInputFieldQuickMenu").is_none());
		engine.get_input_manager_mut().get_text_input_mut().finish_frame();

		// 허용 거리(15)를 넘게 움직이면 길게 누르기가 아니다.
		press(&mut node_tree, &mut engine, press_position);
		let moved_position = Vector2::create(press_position.x + 20.0, press_position.y);
		engine.get_input_manager_mut().set_view_input_position(moved_position);
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Moved, moved_position);
		for _ in 0..40 {
			node_tree.tick(&engine, 0.02);
		}
		assert!(!get_field(&mut node_tree, field_id).is_touch_handle_visible);
	}

	//==============================================================================
	/// 편집 버퍼: 끼우기, 지우기, 선택, 이동, 최대 길이, 한글 자리.
	//==============================================================================
	#[test]
	fn edit_buffer_operations() {
		let mut edit_buffer = TextEditBuffer::new();
		edit_buffer.replace_selection("안녕");
		edit_buffer.replace_selection("하세요");
		assert_eq!(edit_buffer.get_text(), "안녕하세요");
		assert_eq!(edit_buffer.get_cursor_index(), 5);
		assert!(edit_buffer.delete_backward());
		assert_eq!(edit_buffer.get_text(), "안녕하세");
		edit_buffer.move_cursor(1, false);
		assert!(edit_buffer.delete_forward());
		assert_eq!(edit_buffer.get_text(), "안하세");
		assert_eq!(edit_buffer.get_cursor_index(), 1);
		edit_buffer.move_cursor(3, true);
		assert_eq!(edit_buffer.get_selection_range(), Some((1, 3)));
		assert_eq!(edit_buffer.get_selection_text(), "하세");
		edit_buffer.replace_selection("x");
		assert_eq!(edit_buffer.get_text(), "안x");
		assert_eq!(edit_buffer.get_cursor_index(), 2);
		edit_buffer.move_cursor(0, false);
		assert!(!edit_buffer.delete_backward());
		edit_buffer.move_cursor(99, false);
		assert_eq!(edit_buffer.get_cursor_index(), 2);
		assert!(!edit_buffer.delete_forward());
		edit_buffer.select_all();
		assert_eq!(edit_buffer.get_selection_text(), "안x");
		assert!(edit_buffer.delete_backward());
		assert_eq!(edit_buffer.get_text(), "");

		// 최대 길이: set_text 는 값을 자르고, 끼우기는 블링크 입력 칸처럼 남은 길이만큼만 넣는다. (서로게이트 쌍은 쪼개지 않는다)
		edit_buffer.set_max_length(4);
		edit_buffer.set_text("가나다라마바");
		assert_eq!(edit_buffer.get_text(), "가나다라");
		edit_buffer.move_cursor(2, false);
		edit_buffer.replace_selection("XYZ");
		assert_eq!(edit_buffer.get_text(), "가나다라");
		edit_buffer.set_selection(2, 4);
		edit_buffer.replace_selection("XYZ");
		assert_eq!(edit_buffer.get_text(), "가나XY");
		assert_eq!(edit_buffer.get_cursor_index(), 4);

		// 보이는 글: 커서 자리에 조합 글.
		edit_buffer.move_cursor(1, false);
		edit_buffer.set_composition("한");
		assert_eq!(edit_buffer.build_display_text(), "가한나XY");
		assert_eq!(edit_buffer.get_text_before_cursor(), "가");
		edit_buffer.set_composition("");
		edit_buffer.set_text("abc");
		edit_buffer.replace_selection("\u{1F600}");
		assert_eq!(edit_buffer.get_text(), "abc");
	}

	//==============================================================================
	/// 공개 함수의 글자 자리와 최대 길이는 원본처럼 UTF-16 단위다. (서로게이트 쌍은 쪼개지 않는다)
	//==============================================================================
	#[test]
	fn public_indices_count_utf16_units() {
		let (mut node_tree, field_id) = create_field_tree();
		let field = get_field(&mut node_tree, field_id);
		field.set_text("a😀b");
		assert_eq!(field.get_cursor_index(), 4);
		field.move_cursor(1, false);
		assert_eq!(field.get_cursor_index(), 1);
		field.move_cursor(3, true);
		assert_eq!(field.get_selection_range(), Some((1, 3)));
		assert_eq!(field.get_selection_text(), "😀");
		field.move_cursor(2, false);
		assert_eq!(field.get_cursor_index(), 3);
		assert_eq!(field.get_selection_start(), 3);
		field.set_max_length(3);
		field.set_text("😀😀");
		assert_eq!(field.get_text(), "😀");
		assert_eq!(field.get_cursor_index(), 2);
		assert_eq!(field.get_max_length(), 3);
	}

	//==============================================================================
	/// 누른 x → 글자 자리. (가까운 경계)
	//==============================================================================
	#[test]
	fn character_index_at_offset() {
		let character_offsets = [0.0, 10.0, 20.0, 30.0];
		assert_eq!(find_character_index_at_offset(&character_offsets, -5.0), 0);
		assert_eq!(find_character_index_at_offset(&character_offsets, 4.0), 0);
		assert_eq!(find_character_index_at_offset(&character_offsets, 6.0), 1);
		assert_eq!(find_character_index_at_offset(&character_offsets, 24.0), 2);
		assert_eq!(find_character_index_at_offset(&character_offsets, 99.0), 3);
		assert_eq!(find_character_index_at_offset(&[0.0], 50.0), 0);
		// 글자 묶음 가운데(결합 부호 앞)에는 서지 않는다. ("e\u{301}b": 경계 0, 2, 3)
		let combining_offsets = [0.0, 10.0, 10.0, 20.0];
		assert_eq!(find_boundary_index_at_offset(&combining_offsets, &[0, 2, 3], 6.0), 2);
		assert_eq!(find_boundary_index_at_offset(&combining_offsets, &[0, 2, 3], 4.0), 0);
		assert_eq!(find_boundary_index_at_offset(&combining_offsets, &[0, 2, 3], 99.0), 3);
	}

	//==============================================================================
	/// 붙이면 피봇이 왼쪽 위, 노드가 interactable 이 되고 content 가 생긴다.
	//==============================================================================
	#[test]
	fn attach_to_tree_sets_up_node() {
		let (mut node_tree, field_id) = create_field_tree();
		assert!(node_tree[field_id].is_interactable());
		assert_eq!(node_tree[field_id].get_pivot(), Pivot::TOP_LEFT);
		let content_id = get_field(&mut node_tree, field_id).get_content().expect("content");
		assert_eq!(node_tree.get_parent_id(content_id), Some(field_id));
	}

	//==============================================================================
	/// 누르면 포커스, 확정 글과 편집 키가 들어가고 바뀜 / 제출 이벤트가 불린다.
	//==============================================================================
	#[test]
	fn typing_and_edit_keys() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		let change_count = Rc::new(Cell::new(0));
		let submitted_text = Rc::new(RefCell::new(String::new()));
		{
			let field = get_field(&mut node_tree, field_id);
			let change_count_for_event = change_count.clone();
			field.set_on_change(Box::new(move |_field: &mut UIInputField, _context: &mut ComponentContext| {
				change_count_for_event.set(change_count_for_event.get() + 1);
			}));
			let submitted_text_for_event = submitted_text.clone();
			field.set_on_submit(Box::new(move |field: &mut UIInputField, _context: &mut ComponentContext| {
				*submitted_text_for_event.borrow_mut() = field.get_text().to_string();
			}));
		}
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Press, Vector2::create(150.0, 120.0));
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Release, Vector2::create(150.0, 120.0));
		assert!(get_field(&mut node_tree, field_id).is_focused());

		push_event(&mut engine, TextInputEventKind::Commit(String::from("abc")));
		push_event(&mut engine, key_event(TextInputKey::Left, false));
		push_event(&mut engine, key_event(TextInputKey::Backspace, false));
		push_event(&mut engine, TextInputEventKind::Commit(String::from("X")));
		push_event(&mut engine, key_event(TextInputKey::Home, true));
		run_frame(&mut node_tree, &mut engine);
		{
			let field = get_field(&mut node_tree, field_id);
			assert_eq!(field.get_text(), "aXc");
			assert_eq!(field.get_selection_range(), Some((0, 2)));
			assert_eq!(field.get_selection_text(), "aX");
		}
		assert_eq!(change_count.get(), 3);

		push_event(&mut engine, key_event(TextInputKey::Delete, false));
		push_event(&mut engine, key_event(TextInputKey::End, false));
		push_event(&mut engine, key_event(TextInputKey::Enter, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "c");
		assert_eq!(get_field(&mut node_tree, field_id).get_cursor_index(), 1);
		assert_eq!(*submitted_text.borrow(), "c");
		assert_eq!(change_count.get(), 4);
	}

	//==============================================================================
	/// IME: 조합 중에는 커서 자리에 조합 글이 보이고 값은 그대로, 확정하면 값에 들어간다.
	//==============================================================================
	#[test]
	fn ime_preedit_then_commit() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		{
			let field = get_field(&mut node_tree, field_id);
			field.set_text("가다");
			field.move_cursor(1, false);
			field.focus();
		}
		push_event(&mut engine, TextInputEventKind::CompositionStart);
		push_event(&mut engine, TextInputEventKind::Preedit {
			text: String::from("ㄴ"),
			cursor_index: Some(1),
		});
		push_event(&mut engine, TextInputEventKind::Preedit {
			text: String::from("나"),
			cursor_index: Some(1),
		});
		run_frame(&mut node_tree, &mut engine);
		{
			let field = get_field(&mut node_tree, field_id);
			assert!(field.is_composing());
			assert_eq!(field.get_text(), "가다");
			assert_eq!(field.get_composition(), "나");
			assert_eq!(field.get_display_text(), "가나다");
		}
		push_event(&mut engine, TextInputEventKind::Preedit {
			text: String::new(),
			cursor_index: None,
		});
		push_event(&mut engine, TextInputEventKind::CompositionEnd);
		push_event(&mut engine, TextInputEventKind::Commit(String::from("나")));
		run_frame(&mut node_tree, &mut engine);
		let field = get_field(&mut node_tree, field_id);
		assert!(!field.is_composing());
		assert_eq!(field.get_text(), "가나다");
		assert_eq!(field.get_display_text(), "가나다");
		assert_eq!(field.get_cursor_index(), 2);
	}

	//==============================================================================
	/// 조합 중에 필드 안을 누르면 조합 글을 확정하고, OS 의 같은 확정은 버리게 하고, IME 를 끊어 달라고 한다.
	//==============================================================================
	#[test]
	fn press_while_composing_commits() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		get_field(&mut node_tree, field_id).focus();
		push_event(&mut engine, TextInputEventKind::CompositionStart);
		push_event(&mut engine, TextInputEventKind::Preedit {
			text: String::from("글"),
			cursor_index: Some(1),
		});
		run_frame(&mut node_tree, &mut engine);
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Press, Vector2::create(110.0, 130.0));
		{
			let field = get_field(&mut node_tree, field_id);
			assert_eq!(field.get_text(), "글");
			assert_eq!(field.get_composition(), "");
			assert!(!field.is_composing());
			assert!(field.is_focused());
		}
		let text_input = engine.get_input_manager_mut().get_text_input_mut();
		assert!(text_input.take_ime_reset_request());
		assert!(!text_input.take_ime_reset_request());
		assert!(text_input.consume_ime_commit_guard("글"));
	}

	//==============================================================================
	/// 최대 길이: 확정 글이 넘치면 자른다.
	//==============================================================================
	#[test]
	fn max_length_limits_commit() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		{
			let field = get_field(&mut node_tree, field_id);
			field.set_max_length(5);
			field.focus();
		}
		push_event(&mut engine, TextInputEventKind::Commit(String::from("닉네임")));
		push_event(&mut engine, TextInputEventKind::Commit(String::from("길게씀")));
		push_event(&mut engine, TextInputEventKind::Paste(String::from("더")));
		run_frame(&mut node_tree, &mut engine);
		let field = get_field(&mut node_tree, field_id);
		assert_eq!(field.get_text(), "닉네임길게");
		assert_eq!(field.get_cursor_index(), 5);
	}

	//==============================================================================
	/// 포커스 바뀜: 나중에 포커스를 얻은 필드만 남는다. 필드 밖을 누르면 포커스를 잃고 조합 글은 확정된다.
	/// attach 하지 않은 필드는 글 입력을 받지 않는다.
	//==============================================================================
	#[test]
	fn focus_changes() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, first_field_id) = create_field_tree();
		let root_id = node_tree.get_root_id();
		let second_field_id = node_tree.create_child(root_id, "second");
		node_tree[second_field_id].set_local_position(Vector2::create(100.0, 300.0));
		node_tree[second_field_id].set_content_size(Vector2::create(300.0, 60.0));
		node_tree.add_component(second_field_id, UIInputField::new()).expect("field").set_use_dom_input(true);

		get_field(&mut node_tree, first_field_id).focus();
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(engine.get_input_manager().get_text_input().get_active_field_state().map(|field_state| field_state.field_id), Some(get_field(&mut node_tree, first_field_id).get_field_id()));

		// attach 하지 않은 두 번째 필드에 포커스: 첫 필드는 다음 tick 에 포커스를 내준다. 글은 아무도 받지 않는다.
		get_field(&mut node_tree, second_field_id).focus();
		run_frame(&mut node_tree, &mut engine);
		push_event(&mut engine, TextInputEventKind::Commit(String::from("z")));
		run_frame(&mut node_tree, &mut engine);
		assert!(!get_field(&mut node_tree, first_field_id).is_focused());
		assert!(get_field(&mut node_tree, second_field_id).is_focused());
		assert_eq!(get_field(&mut node_tree, first_field_id).get_text(), "");
		assert_eq!(get_field(&mut node_tree, second_field_id).get_text(), "");
		assert!(engine.get_input_manager().get_text_input().get_active_field_state().is_none());

		// 첫 필드를 다시 누르고 조합하다가 밖을 누른다.
		node_tree.dispatch_touch(first_field_id, &engine, TouchKind::Press, Vector2::create(150.0, 120.0));
		push_event(&mut engine, TextInputEventKind::CompositionStart);
		push_event(&mut engine, TextInputEventKind::Preedit {
			text: String::from("한"),
			cursor_index: Some(1),
		});
		run_frame(&mut node_tree, &mut engine);
		assert!(get_field(&mut node_tree, first_field_id).is_focused());
		assert_eq!(get_field(&mut node_tree, first_field_id).get_composition(), "한");
		{
			let input_manager = engine.get_input_manager_mut();
			input_manager.set_view_input_position(Vector2::create(700.0, 600.0));
			input_manager.set_touch_pressed(true);
		}
		run_frame(&mut node_tree, &mut engine);
		engine.get_input_manager_mut().set_touch_pressed(false);
		assert!(!get_field(&mut node_tree, second_field_id).is_focused());
		let first_field = get_field(&mut node_tree, first_field_id);
		assert!(!first_field.is_focused());
		assert_eq!(first_field.get_text(), "한");
		assert_eq!(first_field.get_composition(), "");
		// OS 가 같은 조합 글을 다시 보내면 버린다.
		assert!(engine.get_input_manager_mut().get_text_input_mut().consume_ime_commit_guard("한"));
	}

	//==============================================================================
	/// 웹 DOM 입력 동기: 값 바뀜(조합 중엔 무시), 선택 바뀜, DOM 포커스.
	//==============================================================================
	#[test]
	fn dom_synchronization_events() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		let field_id_value = get_field(&mut node_tree, field_id).get_field_id();
		let push_targeted_event = |engine: &mut Engine, event_kind: TextInputEventKind| {
			engine.get_input_manager_mut().get_text_input_mut().push_event(TextInputEvent {
				target_field_id: field_id_value,
				kind: event_kind,
			});
		};
		push_targeted_event(&mut engine, TextInputEventKind::FocusGained);
		push_targeted_event(&mut engine, TextInputEventKind::ReplaceValue {
			value: String::from("hello"),
			selection_start: 5,
			selection_end: 5,
		});
		push_targeted_event(&mut engine, TextInputEventKind::Selection {
			start: 1,
			end: 3,
		});
		push_targeted_event(&mut engine, TextInputEventKind::CompositionStart);
		push_targeted_event(&mut engine, TextInputEventKind::ReplaceValue {
			value: String::from("hello?"),
			selection_start: 6,
			selection_end: 6,
		});
		run_frame(&mut node_tree, &mut engine);
		{
			let field = get_field(&mut node_tree, field_id);
			assert!(field.is_focused());
			assert_eq!(field.get_text(), "hello");
			assert_eq!(field.get_selection_range(), Some((1, 3)));
			assert!(field.is_composing());
		}
		push_targeted_event(&mut engine, TextInputEventKind::CompositionEnd);
		push_targeted_event(&mut engine, TextInputEventKind::FocusLost);
		run_frame(&mut node_tree, &mut engine);
		assert!(!get_field(&mut node_tree, field_id).is_focused());
	}

	//==============================================================================
	/// 문맥 메뉴: 오른쪽 단추 요청이 필드 위면 포커스를 주고 메뉴 노드를 만든다. 메뉴 밖을 누르면 닫힌다.
	/// 붙여넣기 항목은 다음 tick 에 한다. (클립보드를 건드리지 않게 항목 부탁만 본다)
	//==============================================================================
	#[test]
	fn context_menu_opens_and_closes() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		let root_id = node_tree.get_root_id();
		engine.get_input_manager_mut().get_text_input_mut().set_context_menu_request_position(Some(Vector2::create(200.0, 120.0)));
		run_frame(&mut node_tree, &mut engine);
		assert!(get_field(&mut node_tree, field_id).is_focused());
		let menu_id = node_tree.find_child_by_name(root_id, "uiInputFieldContextMenu").expect("menu");
		assert_eq!(node_tree[menu_id].get_local_position(), Vector2::create(200.0, 120.0));
		assert!(node_tree[menu_id].is_interactable());
		run_frame(&mut node_tree, &mut engine);
		assert!(node_tree.contains(menu_id));
		{
			let input_manager = engine.get_input_manager_mut();
			input_manager.set_view_input_position(Vector2::create(900.0, 600.0));
			input_manager.set_touch_pressed(true);
		}
		run_frame(&mut node_tree, &mut engine);
		engine.get_input_manager_mut().set_touch_pressed(false);
		assert!(!node_tree.contains(menu_id));
		assert!(!get_field(&mut node_tree, field_id).is_focused());
	}

	//==============================================================================
	/// 이어 누른 수: 크롬 aura 처럼 시간과 가로, 세로 2 DIP 안이면 하나 더, 셋을 넘지 않는다.
	//==============================================================================
	#[test]
	fn repeated_clicks_count_like_chrome_aura() {
		let origin = Vector2::create(10.0, 10.0);
		assert_eq!(count_repeated_clicks(None, origin, 0.5, 2.0), 1);
		assert_eq!(count_repeated_clicks(Some((0.3, origin, 1)), Vector2::create(12.0, 8.0), 0.5, 2.0), 2);
		assert_eq!(count_repeated_clicks(Some((0.3, origin, 2)), origin, 0.5, 2.0), 3);
		assert_eq!(count_repeated_clicks(Some((0.3, origin, 3)), origin, 0.5, 2.0), 3);
		assert_eq!(count_repeated_clicks(Some((0.6, origin, 1)), origin, 0.5, 2.0), 1);
		assert_eq!(count_repeated_clicks(Some((0.1, origin, 1)), Vector2::create(12.5, 10.0), 0.5, 2.0), 1);
	}

	//==============================================================================
	/// 낱말과 문단 범위: 블링크 ExpandWithGranularity 처럼 공백을 누르면 공백, 문단 끝 자리는 줄바꿈, 글 끝은 앞 낱말, 문단은 줄바꿈까지.
	//==============================================================================
	#[test]
	fn word_and_paragraph_ranges_like_blink() {
		let text = "hello  world\nnext line";
		assert_eq!(find_word_range_at(text, 1, false), (0, 5));
		assert_eq!(find_word_range_at(text, 5, false), (5, 7));
		assert_eq!(find_word_range_at(text, 7, false), (7, 12));
		assert_eq!(find_word_range_at(text, 12, false), (12, 13));
		assert_eq!(find_word_range_at(text, 22, false), (18, 22));
		assert_eq!(find_word_range_at(text, 5, true), (0, 5));
		assert_eq!(skip_trailing_whitespace(text, 5), 7);
		assert_eq!(skip_trailing_whitespace(text, 12), 12);
		assert_eq!(find_paragraph_range_at(text, 3), (0, 13));
		assert_eq!(find_paragraph_range_at(text, 15), (13, 22));
		assert_eq!(find_paragraph_range_at("abc\n", 4), (0, 4));
		assert_eq!(find_paragraph_range_at("", 0), (0, 0));
	}

	//==============================================================================
	/// 두 번 누르면 낱말(윈도우는 뒤 공백까지), 세 번 누르면 문단(한 줄 필드는 글 전체), 두 번 누른 뒤 끌면 낱말 단위로 넓힌다. 캔버스 모드 필드는 원본처럼 커서만.
	//==============================================================================
	#[test]
	fn double_and_triple_click_select_word_and_paragraph() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		{
			let field = get_field(&mut node_tree, field_id);
			field.set_text("hello world again");
			let value = field.edit_buffer.get_text().to_string();
			field.character_offsets = (0..=value.chars().count()).map(|character_index| character_index as f32 * 10.0).collect();
			field.measured_value = value;
			field.measured_font_string = field.build_font_string();
		}
		// "world" 의 'o'. (padding 16, 글자 폭 10)
		let press_position = Vector2::create(100.0 + 16.0 + 74.0, 130.0);
		let click = |node_tree: &mut NodeTree| {
			node_tree.dispatch_touch(field_id, &engine, TouchKind::Press, press_position);
			node_tree.dispatch_touch(field_id, &engine, TouchKind::Release, press_position);
		};
		click(&mut node_tree);
		assert_eq!(get_field(&mut node_tree, field_id).get_selection_range(), None);
		click(&mut node_tree);
		let word_end = if cfg!(target_os = "windows") {
			12
		}
		else {
			11
		};
		assert_eq!(get_field(&mut node_tree, field_id).get_selection_range(), Some((6, word_end)));
		click(&mut node_tree);
		assert_eq!(get_field(&mut node_tree, field_id).get_selection_range(), Some((0, 17)));

		// 두 번 누른 뒤 "again" 쪽으로 끌면 그 낱말 끝까지.
		get_field(&mut node_tree, field_id).last_click = None;
		click(&mut node_tree);
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Press, press_position);
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Moved, Vector2::create(100.0 + 16.0 + 143.0, 130.0));
		assert_eq!(get_field(&mut node_tree, field_id).get_selection_range(), Some((6, 17)));
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Release, press_position);

		// 캔버스 모드는 원본처럼 누를 때마다 커서만 둔다.
		get_field(&mut node_tree, field_id).set_use_dom_input(false);
		click(&mut node_tree);
		click(&mut node_tree);
		assert_eq!(get_field(&mut node_tree, field_id).get_selection_range(), None);
	}

	//==============================================================================
	/// 글자 묶음: 화살표는 묶음 단위, 선택이 있으면 접기, 뒤로 지우기는 블링크 BackspaceStateMachine, Delete 는 묶음 하나.
	//==============================================================================
	#[test]
	fn edit_keys_follow_grapheme_clusters() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		{
			let field = get_field(&mut node_tree, field_id);
			field.focus();
			field.set_text("a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}e\u{301}");
		}
		run_frame(&mut node_tree, &mut engine);
		let press = |node_tree: &mut NodeTree, engine: &mut Engine, key: TextInputKey, is_shift: bool| -> (usize, usize) {
			push_event(engine, key_event(key, is_shift));
			run_frame(node_tree, engine);
			let field = get_field(node_tree, field_id);
			(field.edit_buffer.get_selection_start(), field.edit_buffer.get_cursor_index())
		};
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::Left, false), (6, 6));
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::Left, true), (6, 1));
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::Right, false), (6, 6));
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::Left, true), (6, 1));
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::Left, false), (1, 1));
		push_event(&mut engine, key_event(TextInputKey::Delete, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "ae\u{301}");
		push_event(&mut engine, key_event(TextInputKey::End, false));
		push_event(&mut engine, key_event(TextInputKey::Backspace, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "ae");
	}

	//==============================================================================
	/// 낱말 키: 옮기기는 커서 자리에서(오른쪽은 윈도우만 다음 낱말 앞), 맥은 넓히던 고르기가 고정 끝을 넘지 않는다, 글자가 있는 줄 앞에서는 앞 줄로 넘지 않고
	/// 낱말 지우기는 줄바꿈 하나를 지운다, 고른 것은 고른 것만 따로 한 묶음으로 지운다, 낱말 지우기 뒤 뒤로 지우기는 그 묶음에 붙는다. (헤드리스 크롬 textarea 와 같다)
	//==============================================================================
	#[test]
	fn word_keys_move_select_and_delete_like_chrome() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		{
			let field = get_field(&mut node_tree, field_id);
			field.set_multiline(true);
			field.focus();
			field.set_text("hello world foo\nbar");
		}
		run_frame(&mut node_tree, &mut engine);
		let press = |node_tree: &mut NodeTree, engine: &mut Engine, key: TextInputKey, is_shift: bool| -> (usize, usize) {
			push_event(engine, key_event(key, is_shift));
			run_frame(node_tree, engine);
			let field = get_field(node_tree, field_id);
			(field.edit_buffer.get_selection_start(), field.edit_buffer.get_cursor_index())
		};
		let set_selection = |node_tree: &mut NodeTree, selection_start: usize, cursor_index: usize| {
			let field = get_field(node_tree, field_id);
			field.move_cursor(selection_start, false);
			field.move_cursor(cursor_index, true);
		};
		let right_end = if cfg!(target_os = "windows") {
			12
		}
		else {
			11
		};
		set_selection(&mut node_tree, 8, 8);
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::WordLeft, false), (6, 6));
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::WordRight, false), (right_end, right_end));
		// 고른 것이 있어도 커서(focus)에서 찾는다.
		set_selection(&mut node_tree, 2, 8);
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::WordRight, false), (right_end, right_end));
		set_selection(&mut node_tree, 8, 2);
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::WordLeft, false), (0, 0));
		// 넓히기: 맥은 고정 끝을 넘으면 고정 끝에서 멈춘다.
		set_selection(&mut node_tree, 8, 8);
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::WordLeft, true), (8, 6));
		let across_anchor = if cfg!(target_os = "macos") {
			(8, 8)
		}
		else {
			(8, right_end)
		};
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::WordRight, true), across_anchor);
		// 글자가 있는 줄 앞: 왼쪽 낱말은 그대로, 낱말 지우기는 줄바꿈 하나.
		set_selection(&mut node_tree, 16, 16);
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::WordLeft, false), (16, 16));
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::DeleteWordBackward, false), (15, 15));
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "hello world foobar");

		// 되돌리기 묶음: [낱말 지우기 + 뒤로 지우기] / [낱말 지우기]
		get_field(&mut node_tree, field_id).set_text("one two three");
		run_frame(&mut node_tree, &mut engine);
		push_event(&mut engine, key_event(TextInputKey::DeleteWordBackward, false));
		push_event(&mut engine, key_event(TextInputKey::Backspace, false));
		push_event(&mut engine, key_event(TextInputKey::DeleteWordBackward, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "one ");
		push_event(&mut engine, key_event(TextInputKey::Undo, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "one two");
		push_event(&mut engine, key_event(TextInputKey::Undo, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "one two three");

		// 고른 것은 고른 것만 지우고 따로 한 묶음이다. 그 뒤 친 글자도 새 묶음.
		set_selection(&mut node_tree, 4, 7);
		push_event(&mut engine, key_event(TextInputKey::DeleteWordForward, false));
		push_event(&mut engine, TextInputEventKind::Commit(String::from("x")));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "one x three");
		push_event(&mut engine, key_event(TextInputKey::Undo, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "one  three");
		push_event(&mut engine, key_event(TextInputKey::Undo, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "one two three");

		// 앞으로 낱말 지우기: 맥과 리눅스는 낱말 끝까지, 윈도우는 다음 낱말 앞까지. (여기서는 공백 하나)
		set_selection(&mut node_tree, 3, 3);
		push_event(&mut engine, key_event(TextInputKey::DeleteWordForward, false));
		run_frame(&mut node_tree, &mut engine);
		let expected_text = if cfg!(target_os = "windows") {
			"onetwo three"
		}
		else {
			"one three"
		};
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), expected_text);
	}

	//==============================================================================
	/// 맥 Home, End, PageUp, PageDown: 여러 줄은 커서를 두고 넘기고, 넘길 것이 없으면 줄 앞과 끝으로 옮긴다. Shift 와 함께면 글 앞과 끝까지 고른다.
	//==============================================================================
	#[test]
	fn mac_scroll_keys_scroll_like_chrome() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		{
			let field = get_field(&mut node_tree, field_id);
			field.set_multiline(true);
			field.set_font_size(14.0);
			field.set_padding(4.0);
			field.set_line_height(Some(17.0));
			field.focus();
			field.set_text("line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8");
		}
		run_frame(&mut node_tree, &mut engine);
		let press = |node_tree: &mut NodeTree, engine: &mut Engine, key: TextInputKey, is_shift: bool| -> (usize, usize, f32) {
			push_event(engine, key_event(key, is_shift));
			run_frame(node_tree, engine);
			let field = get_field(node_tree, field_id);
			(field.edit_buffer.get_selection_start(), field.edit_buffer.get_cursor_index(), field.get_scroll_top())
		};
		// 글 끝의 커서가 보이게 넘겨 둔 상태에서 Home: 커서는 그대로, 맨 위로.
		let (_, _, scroll_top_at_end) = press(&mut node_tree, &mut engine, TextInputKey::DocumentEnd, false);
		assert!(scroll_top_at_end > 0.0);
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::ScrollToDocumentStart, false), (47, 47, 0.0));
		// 넘길 것이 없으면 줄 앞으로. (블링크 키 표 MoveToBeginningOfLine)
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::ScrollToDocumentStart, false).1, 42);
		// page 넘기기는 커서를 두고 한 화면만큼.
		let (_, cursor_index, scroll_top) = press(&mut node_tree, &mut engine, TextInputKey::ScrollPageUp, false);
		assert_eq!(cursor_index, 42);
		assert!(scroll_top < scroll_top_at_end);
		// Shift + Home 은 글 앞까지 고른다.
		assert_eq!(press(&mut node_tree, &mut engine, TextInputKey::ScrollToDocumentStart, true).0, 42);
		assert_eq!(get_field(&mut node_tree, field_id).edit_buffer.get_cursor_index(), 0);
	}

	//==============================================================================
	/// 여러 줄: Enter 는 줄을 넣고(제출 아님), 위아래는 가로 자리를 지키며 첫 줄 위는 글 앞, Home 과 End 는 줄 앞과 끝, 붙여넣기의 CRLF 는 LF, 넘치면 휠과 커서 따라 스크롤.
	//==============================================================================
	#[test]
	fn multiline_field_edits_like_textarea() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		let submit_count = Rc::new(Cell::new(0));
		{
			let field = get_field(&mut node_tree, field_id);
			field.set_multiline(true);
			field.set_font_size(14.0);
			field.set_padding(4.0);
			let submit_count_for_event = submit_count.clone();
			field.set_on_submit(Box::new(move |_field: &mut UIInputField, _context: &mut ComponentContext| {
				submit_count_for_event.set(submit_count_for_event.get() + 1);
			}));
		}
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Press, Vector2::create(150.0, 120.0));
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Release, Vector2::create(150.0, 120.0));
		push_event(&mut engine, TextInputEventKind::Commit(String::from("abc")));
		push_event(&mut engine, key_event(TextInputKey::Enter, false));
		push_event(&mut engine, TextInputEventKind::Commit(String::from("d")));
		push_event(&mut engine, key_event(TextInputKey::Enter, false));
		push_event(&mut engine, TextInputEventKind::Commit(String::from("fghij")));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "abc\nd\nfghij");
		assert_eq!(submit_count.get(), 0);
		// Enter 는 글자 넣기 묶음에 붙어 한 번에 되돌린다.
		assert!(get_field(&mut node_tree, field_id).can_undo());

		// 끝(셋째 줄 다섯 글자 뒤)에서 위로: 둘째 줄은 짧아 그 끝, 다시 위로: 가로 자리를 지켜 첫째 줄 끝, 아래로 둘: 셋째 줄 다섯 글자 뒤.
		let cursor_after = |node_tree: &mut NodeTree, engine: &mut Engine, key: TextInputKey| -> usize {
			push_event(engine, key_event(key, false));
			run_frame(node_tree, engine);
			get_field(node_tree, field_id).get_cursor_index()
		};
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::Up), 5);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::Up), 3);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::Down), 5);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::Down), 11);
		// 첫 줄 위는 글 앞, 마지막 줄 아래는 글 끝.
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::Up), 5);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::Up), 3);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::Up), 0);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::DocumentEnd), 11);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::Down), 11);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::Home), 6);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::End), 11);
		assert_eq!(cursor_after(&mut node_tree, &mut engine, TextInputKey::DocumentStart), 0);

		// 붙여넣기의 CR, CRLF 는 LF.
		push_event(&mut engine, TextInputEventKind::Paste(String::from("x\r\ny\rz")));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "x\ny\nzabc\nd\nfghij");

		// 줄이 높이를 넘치면 휠로 넘기고, 끝이면 부모에게 넘긴다. 글 끝으로 가면 커서가 보이게 스크롤한다.
		let field_center = Vector2::create(250.0, 130.0);
		assert_eq!(node_tree.find_wheel_target_node(field_center), Some(field_id));
		assert!(node_tree.dispatch_wheel_at(field_center, &engine, Vector2::create(0.0, 20.0)));
		assert!(get_field(&mut node_tree, field_id).get_scroll_top() > 0.0);
		assert!(node_tree.dispatch_wheel_at(field_center, &engine, Vector2::create(0.0, -1000.0)));
		assert_eq!(get_field(&mut node_tree, field_id).get_scroll_top(), 0.0);
		assert!(!node_tree.dispatch_wheel_at(field_center, &engine, Vector2::create(0.0, -10.0)));
		push_event(&mut engine, key_event(TextInputKey::DocumentEnd, false));
		run_frame(&mut node_tree, &mut engine);
		let field = get_field(&mut node_tree, field_id);
		let line_height = field.get_line_height();
		// 글 끝 커서 보이기는 블링크 RevealSelection 처럼 맥은 가운데 맞춤이라 끝(아래 안쪽 여백까지), 그 밖은 가장자리 맞춤이라 커서 아래가 보이는 데까지다.
		let expected_scroll_top = if cfg!(target_os = "macos") {
			(line_height * 5.0 + 8.0 - 60.0).max(0.0)
		}
		else {
			(line_height * 5.0 + 4.0 - 60.0).max(0.0)
		};
		assert!((field.get_scroll_top() - expected_scroll_top).abs() < 0.01, "스크롤 {} / 기대 {} (줄 높이 {})", field.get_scroll_top(), expected_scroll_top, line_height);
	}

	//==============================================================================
	/// 여러 줄: 폭을 넘는 글은 줄로 나뉘고, Home 과 End 는 보이는 줄의 앞과 끝, 폭으로 나뉜 자리의 End 는 그 줄 끝에 남는다.
	//==============================================================================
	#[test]
	fn multiline_field_wraps_visual_lines() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		{
			let field = get_field(&mut node_tree, field_id);
			field.set_multiline(true);
			field.set_font_size(14.0);
			field.set_padding(4.0);
		}
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Press, Vector2::create(150.0, 120.0));
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Release, Vector2::create(150.0, 120.0));
		let long_text = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor";
		push_event(&mut engine, TextInputEventKind::Commit(String::from(long_text)));
		push_event(&mut engine, key_event(TextInputKey::DocumentStart, false));
		push_event(&mut engine, key_event(TextInputKey::End, false));
		run_frame(&mut node_tree, &mut engine);
		let field = get_field(&mut node_tree, field_id);
		let line_list = field.text_area_layout.line_list.clone();
		assert!(line_list.len() >= 2);
		// 첫 줄은 공백 뒤에서 나뉘고, End 는 그 줄 끝(다음 줄 앞과 같은 자리)에 위쪽 친화로 남는다.
		assert_eq!(long_text.chars().nth(line_list[0].end - 1), Some(' '));
		assert_eq!(field.get_cursor_index(), line_list[0].end);
		assert!(field.is_caret_upstream);
		// Home 은 그 줄 앞(0), 오른쪽은 다음 줄 앞으로 친화를 푼다.
		push_event(&mut engine, key_event(TextInputKey::Home, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_cursor_index(), 0);
		push_event(&mut engine, key_event(TextInputKey::Down, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_cursor_index(), line_list[1].start);
	}

	//==============================================================================
	/// 한 줄 DOM 입력 칸의 넣는 글: 끝 줄바꿈은 지우고 가운데 줄바꿈은 공백. (블링크 TextFieldInputType) 위아래는 글 앞과 끝.
	//==============================================================================
	#[test]
	fn single_line_dom_field_sanitizes_line_breaks() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, field_id) = create_field_tree();
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Press, Vector2::create(150.0, 120.0));
		node_tree.dispatch_touch(field_id, &engine, TouchKind::Release, Vector2::create(150.0, 120.0));
		push_event(&mut engine, TextInputEventKind::Paste(String::from("a\r\nb\nc\n\r\n")));
		push_event(&mut engine, key_event(TextInputKey::Up, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_text(), "a b c");
		assert_eq!(get_field(&mut node_tree, field_id).get_cursor_index(), 0);
		push_event(&mut engine, key_event(TextInputKey::Down, false));
		run_frame(&mut node_tree, &mut engine);
		assert_eq!(get_field(&mut node_tree, field_id).get_cursor_index(), 5);
	}
}

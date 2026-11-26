//==============================================================================
//! UI 위젯. (원본 src/ui 의 WorldNode 노선)
//! - 원본처럼 위젯 대부분은 노드가 아니라 컴포넌트다. 노드에 붙이면 자기 노드(호스트)를 위젯으로 쓴다.
//! - 자식 노드(content, 스크롤바)나 짝 컴포넌트(Mask)를 만드는 위젯은 node_tree.add_component(node_id, ...) 로 붙인다.
//!   WorldNode::add_component 로 붙이면 attach_to_tree 가 불리지 않아 content 노드가 없고 터치도 켜지지 않는다.
//! - 원본 UIScrollBar 는 WorldNode 를 상속한 노드다. 러스트 판은 노드 하나에 붙는 UIScrollBar 컴포넌트다.
//! - UIInputField 는 엔진 글 입력 줄(core/textinput.rs)에서 키, IME, 웹 숨긴 DOM 입력의 이벤트를 받는다.
//! - 원본에서 WorldNode 를 상속한 위젯(UIDropdown, UIDialog, UIToast, UISpinner, UIContextMenu)도 호스트 노드에 붙는 컴포넌트다.
//!   붙을 때 원본 생성자처럼 호스트를 꾸미고 자식 노드를 만든다. 각 위젯의 create(node_tree, parent_id, options) 가 new + add_child 자리다.
//! - 옮기지 않은 것: UINode / 오토레이아웃 / UIScene 노선. (UIDocument 는 리플렉션 대신 종류 이름 표로 옮겼다)
//==============================================================================
pub mod buttonmotion;
pub mod popupmotion;
pub mod toastmotion;
pub mod touchrecognizer;
pub mod uibutton;
pub mod uichart;
pub mod uicontextmenu;
pub mod uicontrol;
pub mod uidialog;
pub mod uidraggable;
pub mod uidropdown;
pub mod uihitarea;
pub mod uiimageview;
pub mod textarealayout;
pub mod uiinputfield;
#[cfg(not(target_arch = "wasm32"))]
pub mod uiinputfieldmenu;
pub mod uilabel;
pub mod uilistview;
pub mod uiprogressview;
pub mod uiprogressring;
pub mod uiscrollbar;
pub mod uiscrollview;
pub mod uislider;
pub mod uisnapscrollview;
pub mod uispinner;
pub mod uitoast;
pub mod uitogglebutton;
pub mod uiview;
pub mod uidocument;
pub mod uiscale;
pub mod uiwindow;
pub mod uiwindowlayer;
pub mod uiconfirmwindow;
pub mod autolayout;
pub mod uinode;
pub mod uiscene;

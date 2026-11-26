//==============================================================================
//! UI 문서. (vanilla.js ui/uidocument.js 의 UIDocument)
//! - 노드 트리를 JSON 문서(.scene.json)로 내보내고 읽는다. vanilla.js 비주얼 에디터(tools/visualeditor)가 이 꼴로 씬을 저장하고,
//!   게임은 같은 파일을 읽어 NodeTree 에 노드와 컴포넌트를 짓는다. 러스트에서 지은 트리도 같은 꼴로 내보낸다.
//! - 문서는 { "version": 1, "root": 노드 } 이고, 노드는 type, name, active, position, scale, rotation, opacity, pivot, anchor,
//!   contentSize, interactable, components, children 이다. 위젯(content 노드를 만드는 컴포넌트)이 있으면 content 안의 자식만 contentChildren 으로 따로 둔다.
//!   에디터가 얹는 frameSize 같은 모르는 키는 무시한다. 키 차례와 들여쓰기(탭)는 원본 JSON.stringify 와 같게 쓴다. (serde_json preserve_order)
//! - 원본과 다른 것:
//!   - 원본은 정적 클래스와 모듈 전역 표다. 러스트 판은 표를 든 UIDocument 값을 만들어 쓴다. (엔진처럼 전역 상태를 두지 않는다) new 가 엔진 컴포넌트를 등록해 둔다.
//!   - 원본처럼 쓰기, 읽기 함수(원본 COMPONENT_PROPERTY_TABLE 의 propertyHandler)가 없는 종류는 반사로 쓰고 읽는다. (saveByReflection, loadByReflection)
//!     원본은 실행 중에 객체의 get, is, set 함수를 훑고, 러스트 판은 `#[reflect]` 매크로가 컴파일 때 지은 목록을 쓴다. (core/reflection.rs)
//!     반사로 다루는 종류는 `#[reflect]` 를 붙인 컴포넌트뿐이다. (엔진 컴포넌트는 모두 붙였다) 원본은 모든 객체가 반사된다.
//!     원본 표에서 반사로 저장하는 Outline, SoftShade, ShadowText, UIProgressRing 도 반사로 쓰고 읽는다. 원본 반사 결과와 같은 키(머리 대문자: Color, LineWidth,
//!     GizmoVisible, Enable, ComponentType 등)와 차례다. 원본 ShadowText 의 FontFace 는 글꼴 객체라 적히지 않는다. 러스트 판의 글꼴 이름 글(get_font_family)은
//!     `#[reflect(unsaved)]` 로 속성 목록에는 두고 저장에서 뺐다.
//!   - 트리를 받는 반사 세터(UISnapScrollView 의 SnapCurrentIndex 같은 것)도 원본처럼 읽는 그 자리에서 부른다. 읽는 컴포넌트를 트리에서 잠깐 꺼내 읽기 문맥에 트리를 쥐여 준다.
//!   - &mut self 게터가 있는 컴포넌트는 to_json_text_mut, serialize_node_mut 가 &mut 갈래로 읽어 적는다. (원본 게터에는 &self, &mut self 구별이 없다)
//!   - 종류 등록: register_component_type(이름, 짓는 함수)은 원본 registerComponentType 이다. (반사로 쓰고 읽는다) 쓰기, 읽기 함수가 따로 있는 종류는
//!     register_component_type_with_handler 로, 위젯(content 노드를 만드는 컴포넌트)은 register_widget_type, register_widget_type_with_handler 로 등록한다.
//!   - 등록하지 않은 종류는 원본처럼 읽을 때 건너뛰고(노드 종류는 WorldNode 로 읽고) 경고를 낸다. 쓸 때 type 은 원본처럼 등록한 이름이고, 등록하지 않은 종류는 반사로 적고 type 은 타입 이름이다. (원본 constructor.name)
//!     반사가 없는 컴포넌트(`#[reflect]` 를 붙이지 않은 게임 컴포넌트)는 적을 수 없어 경고를 내고 뺀다.
//!   - 쓰기, 읽기 함수가 있는 종류도 원본처럼 노드를 가리키는 반사 속성은 따로 { "$nodeRef": 경로 } 로 얹고 읽는다. (saveNodeReferences, loadNodeReferences)
//!   - ShadowImage 는 원본 표의 끝에도 같은 꼴(imagePath 뒤에 반사 저장 꼴)로 넣어 두 엔진이 같다.
//!   - 원본 UIImageView 는 붙을 때 호스트에 Sprite 를 붙여 문서에 UIImageView 다음 Sprite 가 적힌다. 러스트 판은 Sprite 를 품어서, 품은 Sprite 를 그 자리에 Sprite 로 적고
//!     읽을 때는 뒤따르는 Sprite 를 품은 Sprite 에 읽는다. (set_component_companions, 원본은 UIImageView 가 붙을 때 붙인 Sprite 에 읽는다)
//!   - 원본 노드 종류(WorldNode 하위 클래스, registerNodeType)는 러스트 판에서 노드에 붙는 컴포넌트다. register_node_type 으로 등록하면 원본 문서와 같은 꼴로 쓰고 읽는다:
//!     그 컴포넌트가 붙은 노드의 type 이 그 이름이고 컴포넌트 목록에는 적지 않는다.
//!     - 원본 노드 생성자처럼 붙을 때 자식과 컴포넌트를 실제로 짓는 종류(UIWindow, UIConfirmWindow, UIDialog, UIDropdown, UIToast, UISpinner, UIContextMenu 등)는
//!       지은 것이 그대로 적힌다.
//!     - 스스로 그리는 종류(UIInputField, UIScrollBar)는 UIDocumentNode 가 원본 생성자의 몫을 원본 꼴로 적고, 읽을 때는 그 몫을 설정으로 읽는다.
//!     - 원본은 숨을 때 노드를 끄지만 러스트 판은 tick 이 돌아야 해서 숨기기만 하는 종류(UIToast)는 UIDocumentNode 가 active 를 원본 뜻으로 바꿔 쓰고 읽는다.
//!   - 읽을 때 노드에 이미 있는 컴포넌트와 자식(노드 종류나 위젯이 붙을 때 지은 것, require 로 함께 붙은 것)은 문서의 앞에서부터 차례와 종류(자식은 이름)가 맞으면
//!     그것에 문서 값을 넣고 다시 짓지 않는다. (원본 fillNode 와 같다) 다시 쓰면 같은 문서다.
//!   - 원본 에디터(tools/uieditor)는 엔진이 내보내는 모든 Component 와 WorldNode 하위 클래스를 등록해 문서에 적는다. 원본처럼 기본 표에는 없고,
//!     게임이 register_editor_types(원본 registerEditorTypes) 한 줄로 같은 종류(컴포넌트 35, 노드 종류 12)를 등록한다. (종류마다 한 줄로도 된다)
//!   - 러스트 판의 노드는 WorldNode 하나다. 노드 종류로 등록한 컴포넌트가 없는 노드의 type 은 늘 "WorldNode" 로 적고, 등록하지 않은 type 은 WorldNode 로 읽는다.
//!   - 원본 getComponentType(이름 → 생성자)은 생성자를 값으로 돌려줄 수 없어(노드 트리는 구체 타입으로 붙인다) create_component(트리, 노드, 이름)가
//!     그 이름의 컴포넌트를 지어 노드에 붙이고 컴포넌트 차례 번호를 돌려준다. (원본에서 생성자를 받아 addComponent 하는 자리)
//!   - 그림 경로(imagePath)는 원본처럼 이미지 해석기(set_image_resolver)로 찾는다. 해석기를 주지 않으면 문서가 든 애셋 창고에서 찾는다.
//!     load_images 가 문서의 imagePath 를 ImageAsset 으로 요청하고(받은 읽기 손잡이는 씬이 track_load_handle 로 맡길 수 있다),
//!     아직 읽히지 않은 그림은 bind_pending_images 가 다 읽힌 뒤 붙인다. (러스트 애셋은 요청 뒤 나중에 다 읽힌다)
//!   - 노드 참조($nodeRef)의 기준 뿌리는 to_json_text, from_json_text 의 뿌리다. serialize_node, deserialize_node 를 바로 부르면 그 노드가 뿌리다.
//!     (원본은 serializeNode 만 부르면 참조를 적지 않고, deserializeNode 만 부르면 참조를 잇지 않는다)
//!   - 수는 f32 를 가장 짧은 십진으로 적는다. (0.22 는 0.22, 정수는 소수점 없이) 원본 f64 의 끝자리(0.1 + 0.2 같은)는 f32 로 줄어든다.
//! - Sprite 의 nineSlice 는 원본이 쓰는 꼴([왼쪽, 위, 왼쪽 + 오른쪽, 위 + 아래], Rect 의 left, top, right, bottom)을 원본처럼 거꾸로 풀어 읽는다.
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::asset::{Asset, AssetLoadHandle};
use crate::core::component::mask::Mask;
use crate::core::component::outline::Outline;
use crate::core::component::paint::Paint;
use crate::core::component::richtext::RichText;
use crate::core::component::shadowimage::ShadowImage;
use crate::core::component::shadowtext::ShadowText;
use crate::core::component::softshade::SoftShade;
use crate::core::component::sprite::{Sprite, SpriteDrawMode};
use crate::core::component::text::Text;
use crate::core::component::Component;
use crate::core::engine::Engine;
use crate::core::graphic::{BlendMode, TextAlign, TextBaseline};
use crate::core::image::Image;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::WorldNode;
use crate::core::reflection::{collect_reflected_properties, has_mutable_getters, read_reflected_property, read_reflected_property_mut, read_shortest_number, write_reflected_property, write_reflected_property_in_tree, ReflectValue, ReflectedProperty};
use crate::resource::imageasset::ImageAsset;
use crate::effect::heathazeeffect::HeatHazeEffect;
use crate::effect::particlesystem::ParticleSystem;
use crate::effect::shadersprite::ShaderSprite;
use crate::effect::trailrenderer::TrailRenderer;
use crate::graphics3d::toon::toonview3d::ToonView3D;
use crate::misc::floatingtext::FloatingText;
use crate::misc::platformerbody::PlatformerBody;
use crate::misc::spriteanimator::SpriteAnimator;
use crate::misc::toucheffect::TouchEffect;
use crate::misc::visual::Visual;
use crate::ui::buttonmotion::ButtonMotion;
use crate::ui::toastmotion::ToastMotion;
use crate::ui::uibutton::UIButton;
use crate::ui::uichart::{UIBarChart, UILineChart};
use crate::ui::uiconfirmwindow::{UIConfirmWindow, UIConfirmWindowOptions};
use crate::ui::uicontextmenu::UIContextMenu;
use crate::ui::uicontrol::UIControl;
use crate::ui::uidialog::UIDialog;
use crate::ui::uidraggable::UIDraggable;
use crate::ui::uidropdown::UIDropdown;
use crate::ui::uihitarea::UIHitArea;
use crate::ui::uiimageview::UIImageView;
use crate::ui::uiinputfield::UIInputField;
use crate::ui::uilabel::UILabel;
use crate::ui::uilistview::UIListView;
use crate::ui::uiprogressring::UIProgressRing;
use crate::ui::uiprogressview::UIProgressView;
use crate::ui::uiscrollbar::UIScrollBar;
use crate::ui::uiscrollview::{ScrollMode, UIScrollView};
use crate::ui::uislider::UISlider;
use crate::ui::uisnapscrollview::UISnapScrollView;
use crate::ui::uispinner::UISpinner;
use crate::ui::uitoast::UIToast;
use crate::ui::uitogglebutton::UIToggleButton;
use crate::ui::uiview::UIView;
use crate::ui::uiwindow::{UIWindow, UIWindowDefaultBehavior};
use crate::ui::uiwindowlayer::UIWindowLayer;
use serde_json::{Map, Value};
use std::any::TypeId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// UI 문서 포맷 버전.
pub const UIDOCUMENT_VERSION: u32 = 1;

/// 노드 참조를 문서에 적을 때 쓰는 표시.
const NODE_REFERENCE_KEY: &str = "$nodeRef";

/// 반사 속성에서 빼는 이름. (다른 객체를 가리키거나 문서로 옮길 수 없는 것, 원본 REFLECTED_PROPERTY_EXCLUDE_NAMES)
const REFLECTED_PROPERTY_EXCLUDE_NAMES: [&str; 13] = [
	"Parent", "Node", "Owner", "Scene", "Content", "Children", "AllComponents",
	"Component", "Components", "Root", "Graphic", "Camera", "Target",
];

/// 문서에 적는 노드 종류 이름. (러스트 판의 노드는 WorldNode 하나다)
const NODE_TYPE_NAME: &str = "WorldNode";

/// 원본 UIScrollBar 생성자가 만든 트랙과 썸 Paint 의 둥근 크기.
const SCROLL_BAR_PART_ROUND_SIZE: f32 = 4.0;

/// 정수로 적을 수 있는 가장 큰 크기. (JSON 수를 f64 로 읽어도 정확한 정수 범위, 2 의 53 제곱)
const MAXIMUM_SAFE_INTEGER: f64 = 9007199254740992.0;

/// 가로 정렬과 원본 이름(Canvas2D textAlign)의 짝.
const TEXT_ALIGN_NAME_TABLE: [(TextAlign, &str); 5] = [
	(TextAlign::Left, "left"),
	(TextAlign::Center, "center"),
	(TextAlign::Right, "right"),
	(TextAlign::Start, "start"),
	(TextAlign::End, "end"),
];

/// 세로 기준선과 원본 이름(Canvas2D textBaseline)의 짝.
const TEXT_BASELINE_NAME_TABLE: [(TextBaseline, &str); 6] = [
	(TextBaseline::Top, "top"),
	(TextBaseline::Middle, "middle"),
	(TextBaseline::Bottom, "bottom"),
	(TextBaseline::Ideographic, "ideographic"),
	(TextBaseline::Hanging, "hanging"),
	(TextBaseline::Alphabetic, "alphabetic"),
];

/// 스프라이트 출력 방식과 원본 이름(SpriteDrawMode)의 짝.
const SPRITE_DRAW_MODE_NAME_TABLE: [(SpriteDrawMode, &str); 3] = [
	(SpriteDrawMode::Simple, "simple"),
	(SpriteDrawMode::Sliced, "sliced"),
	(SpriteDrawMode::Tiled, "tiled"),
];

/// 스크롤 방식과 원본 이름(ScrollMode)의 짝.
const SCROLL_MODE_NAME_TABLE: [(ScrollMode, &str); 2] = [
	(ScrollMode::Clamp, "clamp"),
	(ScrollMode::Elastic, "elastic"),
];

/// 컴포넌트 종류 하나를 짓는 함수. (노드에 붙이고 컴포넌트 차례 번호를 돌려준다)
type AddComponentFunction = Box<dyn Fn(&mut NodeTree, NodeId) -> Option<usize>>;
/// 컴포넌트 하나를 문서 값으로 쓰는 함수.
type SaveComponentFunction = Box<dyn Fn(&dyn Component, &UIDocumentSaveContext) -> Map<String, Value>>;
/// 문서 값을 컴포넌트 하나에 읽어 넣는 함수.
type LoadComponentFunction = Box<dyn Fn(&mut dyn Component, &Map<String, Value>, &mut UIDocumentLoadContext)>;
/// 위젯의 content 노드를 읽는 함수.
type ReadContentFunction = Box<dyn Fn(&dyn Component) -> Option<NodeId>>;
/// 찾은 그림을 컴포넌트에 넣는 함수.
type ApplyImageFunction = Box<dyn Fn(&mut dyn Component, Rc<Image>)>;
/// 찾은 노드를 컴포넌트에 넣는 함수.
type ApplyNodeFunction = Box<dyn Fn(&mut dyn Component, NodeId)>;
/// &mut 갈래로 미리 읽어 둔 반사 속성 값. ((노드, 컴포넌트 차례) → (키, 값) 목록, to_json_text_mut)
type PrefetchedValueMap = HashMap<(NodeId, usize), Vec<(&'static str, ReflectValue)>>;
/// 컴포넌트가 이 노드 종류인지 가리는 함수. (종류만으로 가리지 못하는 것, UIWindow 의 behavior 같은 것)
type MatchNodeTypeFunction = Box<dyn Fn(&dyn Component) -> bool>;
/// 노드 종류 컴포넌트가 원본 노드 생성자의 몫을 쓰는 함수.
type SaveNodePartsFunction = Box<dyn Fn(&dyn Component, &UIDocumentSaveContext, NodeId) -> UIDocumentNodeParts>;
/// 노드 종류 컴포넌트가 원본 노드 생성자의 몫을 읽는 함수. (읽은 컴포넌트 수와 자식 수)
type LoadNodePartsFunction = Box<dyn Fn(&mut dyn Component, &[Value], &[Value]) -> (usize, usize)>;
/// 원본에서는 호스트 노드에 따로 붙는 컴포넌트를 러스트 판이 품은 것의 문서 값을 쓰는 함수. (그 컴포넌트 바로 뒤에 적는다, UIImageView 의 Sprite)
type SaveCompanionsFunction = Box<dyn Fn(&dyn Component, &UIDocumentSaveContext) -> Vec<Map<String, Value>>>;
/// 그 문서 값을 품은 것에 읽는 함수. (뒤따르는 컴포넌트 값들을 받아 읽은 수를 돌려준다)
type LoadCompanionsFunction = Box<dyn Fn(&mut dyn Component, &[Value], &mut UIDocumentLoadContext) -> usize>;
/// 노드 종류의 원본 active 를 읽는 함수. (노드의 active, 보임)
type ReadDocumentActiveFunction = Box<dyn Fn(&dyn Component, bool, bool) -> bool>;
/// 원본 active 를 노드 종류의 노드 상태로 바꾸는 함수. (노드의 active, 바꿀 보임)
type ConvertDocumentActiveFunction = Box<dyn Fn(&dyn Component, bool) -> (bool, Option<bool>)>;

//==============================================================================
/// 컴포넌트 종류 하나. (원본 COMPONENT_TYPE_TABLE 의 생성자와 COMPONENT_PROPERTY_TABLE 의 save, load 를 한데 모은 것)
/// - save_component, load_component 가 None 이면 원본의 propertyHandler 가 없는 종류라 반사로 쓰고 읽는다.
//==============================================================================
struct UIDocumentComponentType {
	type_name: String,
	type_id: TypeId,
	add_component: AddComponentFunction,
	save_component: Option<SaveComponentFunction>,
	load_component: Option<LoadComponentFunction>,
	read_content: Option<ReadContentFunction>,
	generated_type_names: Vec<String>,
	save_companions: Option<SaveCompanionsFunction>,
	load_companions: Option<LoadCompanionsFunction>,
}

//==============================================================================
/// 노드 종류 하나. (원본 NODE_TYPE_TABLE 의 한 줄. 원본 WorldNode 하위 클래스 자리를 맡은 러스트 컴포넌트)
//==============================================================================
struct UIDocumentNodeType {
	type_name: String,
	type_id: TypeId,
	match_component: Option<MatchNodeTypeFunction>,
	add_component: AddComponentFunction,
	save_node_parts: SaveNodePartsFunction,
	load_node_parts: LoadNodePartsFunction,
	read_content: ReadContentFunction,
	read_document_active: ReadDocumentActiveFunction,
	convert_document_active: ConvertDocumentActiveFunction,
}

//==============================================================================
/// 원본 노드 생성자가 만드는 컴포넌트와 자식의 문서 값. (그 노드의 components, children 앞에 들어간다)
/// - UIDocumentNode::save_node_parts 가 돌려주고, UIDocument 가 노드 문서 값의 앞머리에 그대로 넣는다.
///
/// # 예제
/// 썸 자식 하나와 Paint 하나를 든 몫
/// ```rust
/// use serde_json::{Map, Value};
/// use vanilla::ui::uidocument::UIDocumentNodeParts;
///
/// let mut node_parts = UIDocumentNodeParts::new();
/// let mut paint_data = Map::new();
/// paint_data.insert(String::from("roundSize"), Value::from(4));
/// paint_data.insert(String::from("type"), Value::from("Paint"));
/// node_parts.component_data_list.push(paint_data);
/// node_parts.child_data_list.push(serde_json::json!({ "type": "WorldNode", "name": "thumb" }));
///
/// assert_eq!(node_parts.component_data_list[0]["type"], "Paint");
/// assert_eq!(node_parts.child_data_list[0]["name"], "thumb");
/// ```
//==============================================================================
pub struct UIDocumentNodeParts {
	/// 노드의 components 앞에 들어갈 컴포넌트 문서 값. (값마다 "type" 키를 스스로 담는다)
	pub component_data_list: Vec<Map<String, Value>>,
	/// 노드의 children 앞에 들어갈 자식 노드 문서 값.
	pub child_data_list: Vec<Value>,
}

impl UIDocumentNodeParts {
	//==============================================================================
	/// 빈 몫. (생성자가 아무것도 만들지 않는 노드 종류)
	/// # 돌려줌
	/// 두 목록이 다 빈 몫.
	//==============================================================================
	pub fn new() -> UIDocumentNodeParts {
		UIDocumentNodeParts {
			component_data_list: Vec::new(),
			child_data_list: Vec::new(),
		}
	}
}

impl Default for UIDocumentNodeParts {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> UIDocumentNodeParts {
		UIDocumentNodeParts::new()
	}
}

//==============================================================================
/// 원본 노드 종류(WorldNode 하위 클래스)를 맡은 컴포넌트. (register_node_type 으로 등록한다)
/// - 원본은 그 노드의 생성자가 스스로 컴포넌트와 자식을 만들고, 문서에는 그것들이 보통 컴포넌트와 자식으로 적힌다.
///   러스트 판의 컴포넌트는 그것을 스스로 그리거나 품으므로, 쓸 때는 원본이 적을 꼴로 적고 읽을 때는 그 몫을 설정으로 읽고 다시 짓지 않는다.
/// - 생성자가 아무것도 만들지 않는 종류는 빈 구현(impl UIDocumentNode for 타입 {})으로 된다.
/// # 예제
/// 원본 생성자가 Paint 를 붙이는 노드 종류를 직접 그리는 컴포넌트로 맡기
/// ```rust
/// use serde_json::{Map, Value};
/// use vanilla::{Color, Component, ComponentBase, NodeId, NodeTree, Paint, UIDocument, UIDocumentSaveContext};
/// use vanilla::ui::uidocument::{UIDocumentNode, UIDocumentNodeParts};
///
/// struct Badge {
/// 	component_base: ComponentBase,
/// 	color: Color,
/// }
///
/// impl Component for Badge {
/// 	fn get_component_base(&self) -> &ComponentBase {
/// 		&self.component_base
/// 	}
///
/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// 		&mut self.component_base
/// 	}
///
/// 	fn get_component_type(&self) -> &'static str {
/// 		"Badge"
/// 	}
/// }
///
/// impl UIDocumentNode for Badge {
/// 	fn save_node_parts(&self, _save_context: &UIDocumentSaveContext, _node_id: NodeId) -> UIDocumentNodeParts {
/// 		let mut paint_data = Map::new();
/// 		paint_data.insert(String::from("color"), UIDocument::color_to_array(self.color));
/// 		paint_data.insert(String::from("type"), Value::from("Paint"));
/// 		let mut node_parts = UIDocumentNodeParts::new();
/// 		node_parts.component_data_list.push(paint_data);
/// 		node_parts
/// 	}
///
/// 	fn load_node_parts(&mut self, component_data_list: &[Value], _child_data_list: &[Value]) -> (usize, usize) {
/// 		match component_data_list.first().and_then(|component_data| UIDocument::array_to_color(&component_data["color"])) {
/// 			Some(color) => {
/// 				self.color = color;
/// 				(1, 0)
/// 			}
/// 			None => {
/// 				(0, 0)
/// 			}
/// 		}
/// 	}
/// }
///
/// fn create_badge() -> Badge {
/// 	Badge {
/// 		component_base: ComponentBase::new(),
/// 		color: Color::create(1.0, 1.0, 1.0, 1.0),
/// 	}
/// }
///
/// let mut document = UIDocument::new();
/// document.register_node_type("Badge", create_badge);
/// let mut node_tree = NodeTree::new();
/// let badge_id = node_tree.create_node("badge");
/// node_tree[badge_id].add_component(create_badge()).color = Color::create(1.0, 0.0, 0.0, 1.0);
///
/// let json_text = document.to_json_text(&node_tree, badge_id, false);
/// let document_data: Value = serde_json::from_str(&json_text).expect("json");
/// assert_eq!(document_data["root"]["type"], "Badge");
/// assert_eq!(document_data["root"]["components"][0]["type"], "Paint");
///
/// let loaded_id = document.from_json_text(&mut node_tree, &json_text).expect("document");
/// assert_eq!(node_tree[loaded_id].get_component::<Badge>().expect("badge").color, Color::create(1.0, 0.0, 0.0, 1.0));
/// assert!(node_tree[loaded_id].get_component::<Paint>().is_none());
/// ```
//==============================================================================
pub trait UIDocumentNode {
	//==============================================================================
	/// 원본 노드 생성자가 만드는 컴포넌트와 자식을 문서 꼴로. (node_id 는 이 컴포넌트가 붙은 노드)
	/// # 인자
	/// - `_save_context`: 쓰기 문맥. (몫의 컴포넌트 값을 쓸 때 노드 참조 경로의 기준이 된다)
	/// - `_node_id`: 이 컴포넌트가 붙은 노드.
	///
	/// # 돌려줌
	/// 원본 생성자가 만들었을 컴포넌트와 자식의 문서 값. (기본은 빈 몫)
	//==============================================================================
	fn save_node_parts(&self, _save_context: &UIDocumentSaveContext, _node_id: NodeId) -> UIDocumentNodeParts {
		UIDocumentNodeParts::new()
	}

	//==============================================================================
	/// 문서의 컴포넌트와 자식 목록 앞머리에서 원본 노드 생성자가 만든 몫을 읽어 넣는다. (읽은 컴포넌트 수와 자식 수, 그 몫은 짓지 않는다)
	/// # 인자
	/// - `_component_data_list`: 문서 노드의 components 전체.
	/// - `_child_data_list`: 문서 노드의 children 전체.
	///
	/// # 돌려줌
	/// (앞에서부터 읽은 컴포넌트 수, 읽은 자식 수). UIDocument 는 그만큼을 짓지 않고 건너뛴다. (기본 (0, 0))
	//==============================================================================
	fn load_node_parts(&mut self, _component_data_list: &[Value], _child_data_list: &[Value]) -> (usize, usize) {
		(0, 0)
	}

	//==============================================================================
	/// 문서에 적지 않는 content 노드. (러스트 판이 위젯처럼 만드는 것, 원본 노드에는 없다. 그 안의 자식은 contentChildren 으로 적는다)
	/// # 돌려줌
	/// 문서에 적지 않을 content 노드. 없으면 None. (기본 None)
	//==============================================================================
	fn get_document_content(&self) -> Option<NodeId> {
		None
	}

	//==============================================================================
	/// 원본 노드의 active. (원본은 숨을 때 노드를 끄고, 러스트 판은 tick 이 돌아야 해서 끄는 대신 숨기는 종류가 있다. 기본은 노드의 active)
	/// # 인자
	/// - `is_node_active`: 노드의 active.
	/// - `_is_node_visible`: 노드의 보임.
	///
	/// # 돌려줌
	/// 문서에 적을 active.
	//==============================================================================
	fn read_document_active(&self, is_node_active: bool, _is_node_visible: bool) -> bool {
		is_node_active
	}

	//==============================================================================
	/// 원본 active 를 이 종류의 노드 상태로. (노드의 active 와 바꿀 보임, 기본은 그대로이고 보임은 두다)
	/// # 인자
	/// - `is_active`: 문서에 적힌 active.
	///
	/// # 돌려줌
	/// (노드에 넣을 active, 노드에 넣을 보임). 보임이 None 이면 보임을 바꾸지 않는다. (기본 (is_active, None))
	//==============================================================================
	fn convert_document_active(&self, is_active: bool) -> (bool, Option<bool>) {
		(is_active, None)
	}
}

//==============================================================================
/// 아직 붙이지 못한 그림 하나. (노드, 그 노드의 컴포넌트 차례, 그림 경로)
//==============================================================================
struct PendingImage {
	node_id: NodeId,
	component_index: usize,
	image_path: String,
	apply_image: ApplyImageFunction,
}

//==============================================================================
/// 트리를 다 지은 뒤에 이을 노드 참조 하나. (원본 pendingNodeReferenceList 의 항목)
//==============================================================================
struct PendingNodeReference {
	node_id: NodeId,
	component_index: usize,
	path: String,
	apply_node: ApplyNodeFunction,
}

//==============================================================================
/// 그림 찾기 결과.
//==============================================================================
enum ImageLookup {
	Found(Rc<Image>),
	Waiting,
	Missing,
}

//==============================================================================
/// 쓰기 문맥. (컴포넌트 쓰기 함수가 받는다. 노드 참조를 뿌리 기준 경로로 적는다)
/// - 바깥에서 짓지 않는다. register_component_type_with_handler 로 등록한 쓰기 함수와 UIDocumentNode::save_node_parts 가 받는다.
///
/// # 예제
/// 쓰기 함수에서 노드 참조를 경로로 적기
/// ```rust
/// use serde_json::{Map, Value};
/// use vanilla::{Component, ComponentBase, NodeId, NodeTree, UIDocument, UIDocumentLoadContext, UIDocumentSaveContext};
///
/// struct Follow {
/// 	component_base: ComponentBase,
/// 	target: Option<NodeId>,
/// }
///
/// impl Component for Follow {
/// 	fn get_component_base(&self) -> &ComponentBase {
/// 		&self.component_base
/// 	}
///
/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// 		&mut self.component_base
/// 	}
///
/// 	fn get_component_type(&self) -> &'static str {
/// 		"Follow"
/// 	}
/// }
///
/// fn create_follow() -> Follow {
/// 	Follow {
/// 		component_base: ComponentBase::new(),
/// 		target: None,
/// 	}
/// }
///
/// fn save_follow(follow: &Follow, save_context: &UIDocumentSaveContext) -> Map<String, Value> {
/// 	let mut component_data = Map::new();
/// 	save_context.write_node_reference(&mut component_data, "target", follow.target);
/// 	component_data
/// }
///
/// fn load_follow(_follow: &mut Follow, _component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
/// }
///
/// let mut document = UIDocument::new();
/// document.register_component_type_with_handler("Follow", create_follow, save_follow, load_follow);
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.create_node("root");
/// let leader_id = node_tree.create_child(root_id, "leader");
/// let follower_id = node_tree.create_child(root_id, "follower");
/// node_tree[follower_id].add_component(create_follow()).target = Some(leader_id);
///
/// let json_text = document.to_json_text(&node_tree, root_id, false);
/// let document_data: Value = serde_json::from_str(&json_text).expect("json");
/// let component_data = &document_data["root"]["children"][1]["components"][0];
/// assert_eq!(component_data["target"]["$nodeRef"], "/leader");
/// assert_eq!(component_data["type"], "Follow");
/// ```
//==============================================================================
pub struct UIDocumentSaveContext<'a> {
	node_tree: &'a NodeTree,
	root_id: NodeId,
	node_id: NodeId,
	component_index: usize,
	prefetched_value_map: Option<&'a PrefetchedValueMap>,
}

impl<'a> UIDocumentSaveContext<'a> {
	//==============================================================================
	/// 노드 트리.
	/// # 돌려줌
	/// 쓰는 중인 노드 트리.
	//==============================================================================
	pub fn get_node_tree(&self) -> &NodeTree {
		self.node_tree
	}

	//==============================================================================
	/// 쓰는 중인 노드.
	/// # 돌려줌
	/// 쓰는 중인 노드. (serialize_component 로 부르면 root_id)
	//==============================================================================
	pub fn get_node_id(&self) -> NodeId {
		self.node_id
	}

	//==============================================================================
	/// 반사 속성 값 읽기. (&mut 갈래로 미리 읽어 둔 값이 있으면 그것, 없으면 &self 갈래로 읽는다. to_json_text_mut)
	//==============================================================================
	fn read_property(&self, component: &dyn Component, property_name: &str) -> Option<ReflectValue> {
		if let Some(prefetched_value_list) = self.prefetched_value_map.and_then(|prefetched_value_map| prefetched_value_map.get(&(self.node_id, self.component_index))) {
			return prefetched_value_list.iter().find(|(prefetched_name, _)| *prefetched_name == property_name).map(|(_, value)| value.clone());
		}
		read_reflected_property(component, property_name)
	}

	//==============================================================================
	/// 같은 노드의 다른 컴포넌트를 쓰는 문맥.
	//==============================================================================
	fn with_component_index(&self, component_index: usize) -> UIDocumentSaveContext<'a> {
		UIDocumentSaveContext {
			node_tree: self.node_tree,
			root_id: self.root_id,
			node_id: self.node_id,
			component_index,
			prefetched_value_map: self.prefetched_value_map,
		}
	}

	//==============================================================================
	/// 노드 참조 적기. ({ "$nodeRef": 뿌리 기준 경로 }, None 이거나 뿌리 밖의 노드면 적지 않는다. 원본 saveNodeReferences)
	/// # 인자
	/// - `component_data`: 값을 넣을 컴포넌트 문서 값.
	/// - `property_name`: 넣을 키.
	/// - `node_id`: 가리킬 노드. None 이거나 뿌리 밖이면 키를 넣지 않는다.
	//==============================================================================
	pub fn write_node_reference(&self, component_data: &mut Map<String, Value>, property_name: &str, node_id: Option<NodeId>) {
		let node_id = match node_id {
			Some(node_id) => {
				node_id
			}
			None => {
				return;
			}
		};
		if let Some(reference_path) = UIDocument::find_node_path(self.node_tree, self.root_id, node_id) {
			let mut reference_data = Map::new();
			reference_data.insert(String::from(NODE_REFERENCE_KEY), Value::from(reference_path));
			component_data.insert(String::from(property_name), Value::Object(reference_data));
		}
	}
}

//==============================================================================
/// 읽기 문맥. (컴포넌트 읽기 함수가 받는다. 그림과 노드 참조는 여기에 맡겨 두고 트리를 다 지은 뒤에 넣는다)
/// - 바깥에서 짓지 않는다. register_component_type_with_handler 로 등록한 읽기 함수가 받는다.
///
/// # 예제
/// 읽기 함수에서 노드 참조를 맡겨 트리를 다 지은 뒤 잇기
/// ```rust
/// use serde_json::{Map, Value};
/// use vanilla::{Component, ComponentBase, NodeId, NodeTree, UIDocument, UIDocumentLoadContext, UIDocumentSaveContext};
///
/// struct Follow {
/// 	component_base: ComponentBase,
/// 	target: Option<NodeId>,
/// }
///
/// impl Component for Follow {
/// 	fn get_component_base(&self) -> &ComponentBase {
/// 		&self.component_base
/// 	}
///
/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
/// 		&mut self.component_base
/// 	}
///
/// 	fn get_component_type(&self) -> &'static str {
/// 		"Follow"
/// 	}
/// }
///
/// fn create_follow() -> Follow {
/// 	Follow {
/// 		component_base: ComponentBase::new(),
/// 		target: None,
/// 	}
/// }
///
/// fn save_follow(follow: &Follow, save_context: &UIDocumentSaveContext) -> Map<String, Value> {
/// 	let mut component_data = Map::new();
/// 	save_context.write_node_reference(&mut component_data, "target", follow.target);
/// 	component_data
/// }
///
/// fn load_follow(_follow: &mut Follow, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext) {
/// 	load_context.bind_node_reference(component_data.get("target"), |follow: &mut Follow, node_id: NodeId| {
/// 		follow.target = Some(node_id);
/// 	});
/// }
///
/// let mut document = UIDocument::new();
/// document.register_component_type_with_handler("Follow", create_follow, save_follow, load_follow);
/// let json_text = r#"{"version":1,"root":{"name":"root","children":[
/// 	{"name":"leader"},
/// 	{"name":"follower","components":[{"target":{"$nodeRef":"/leader"},"type":"Follow"}]}
/// ]}}"#;
/// let mut node_tree = NodeTree::new();
/// let root_id = document.from_json_text(&mut node_tree, json_text).expect("document");
///
/// let leader_id = node_tree.find_child_by_name(root_id, "leader").expect("leader");
/// let follower_id = node_tree.find_child_by_name(root_id, "follower").expect("follower");
/// assert_eq!(node_tree[follower_id].get_component::<Follow>().expect("follow").target, Some(leader_id));
/// ```
//==============================================================================
pub struct UIDocumentLoadContext<'a> {
	node_id: NodeId,
	component_index: usize,
	node_tree: &'a mut NodeTree,
	pending_image_list: &'a mut Vec<PendingImage>,
	pending_node_reference_list: &'a mut Vec<PendingNodeReference>,
}

impl<'a> UIDocumentLoadContext<'a> {
	//==============================================================================
	/// 노드 트리. (읽는 컴포넌트는 트리에서 잠깐 꺼낸 채다. 그 자리에는 빈 자리표가 있다)
	/// # 돌려줌
	/// 노드 트리의 가변 참조. (읽는 컴포넌트는 get_component 로 찾을 수 없다)
	//==============================================================================
	pub fn get_node_tree_mut(&mut self) -> &mut NodeTree {
		self.node_tree
	}

	//==============================================================================
	/// 짓는 중인 노드.
	/// # 돌려줌
	/// 읽는 컴포넌트가 붙은 노드.
	//==============================================================================
	pub fn get_node_id(&self) -> NodeId {
		self.node_id
	}

	//==============================================================================
	/// 그림 맡기기. (경로의 그림을 찾으면 apply_image 로 이 컴포넌트에 넣는다. 원본 UIDocument.resolveImage + setImage)
	/// - 지금 찾을 수 있는 그림은 트리를 다 지은 자리에서, 아직 읽히는 그림은 bind_pending_images 에서 넣는다.
	/// # 인자
	/// - `image_path`: 그림 경로. (빈 글이면 맡기지 않는다)
	/// - `apply_image`: 찾은 그림을 컴포넌트에 넣는 함수. (그 자리의 컴포넌트가 T 가 아니면 부르지 않는다)
	//==============================================================================
	pub fn bind_image<T: Component + 'static>(&mut self, image_path: &str, apply_image: fn(&mut T, Rc<Image>)) {
		if image_path.is_empty() {
			return;
		}
		self.pending_image_list.push(PendingImage {
			node_id: self.node_id,
			component_index: self.component_index,
			image_path: String::from(image_path),
			apply_image: Box::new(move |component: &mut dyn Component, image: Rc<Image>| {
				if let Some(typed_component) = component.as_any_mut().downcast_mut::<T>() {
					apply_image(typed_component, image);
				}
			}),
		});
	}

	//==============================================================================
	/// 노드 참조 맡기기. (값이 { "$nodeRef": 경로 } 면 트리를 다 지은 뒤 그 노드를 apply_node 로 넣는다. 원본 loadNodeReferences)
	/// # 인자
	/// - `stored_value`: 문서의 그 키 값. None 이거나 { "$nodeRef": 경로 } 꼴이 아니면 맡기지 않는다.
	/// - `apply_node`: 찾은 노드를 컴포넌트에 넣는 함수. (경로의 노드를 찾지 못하거나 그 자리의 컴포넌트가 T 가 아니면 부르지 않는다)
	//==============================================================================
	pub fn bind_node_reference<T: Component + 'static>(&mut self, stored_value: Option<&Value>, apply_node: fn(&mut T, NodeId)) {
		let reference_path = match stored_value.and_then(|value| value.get(NODE_REFERENCE_KEY)).and_then(|value| value.as_str()) {
			Some(reference_path) => {
				reference_path
			}
			None => {
				return;
			}
		};
		self.pending_node_reference_list.push(PendingNodeReference {
			node_id: self.node_id,
			component_index: self.component_index,
			path: String::from(reference_path),
			apply_node: Box::new(move |component: &mut dyn Component, node_id: NodeId| {
				if let Some(typed_component) = component.as_any_mut().downcast_mut::<T>() {
					apply_node(typed_component, node_id);
				}
			}),
		});
	}

	//==============================================================================
	/// 반사 속성의 노드 참조 맡기기. (트리를 다 지은 뒤 그 노드를 반사 세터로 넣는다. 원본 pendingNodeReferenceList 에 setterName 을 넣는 자리)
	//==============================================================================
	fn bind_reflected_node_reference(&mut self, property_name: &str, reference_path: &str) {
		let property_name = String::from(property_name);
		self.pending_node_reference_list.push(PendingNodeReference {
			node_id: self.node_id,
			component_index: self.component_index,
			path: String::from(reference_path),
			apply_node: Box::new(move |component: &mut dyn Component, node_id: NodeId| {
				write_reflected_property(component, &property_name, &ReflectValue::Node(node_id));
			}),
		});
	}
}

//==============================================================================
/// UI 문서. (WorldNode 트리 ↔ JSON 상호 변환)
/// - 종류 표(컴포넌트 종류, 노드 종류)와 그림 찾기 상태를 든다. 쓸 때는 &self, 읽을 때는 &mut self 다.
///
/// # 예제
/// 러스트에서 지은 트리를 문서 글로 내보내고 다시 읽기
/// ```rust
/// use vanilla::{Color, NodeTree, Paint, UIDocument, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let panel_id = node_tree.create_node("panel");
/// node_tree[panel_id].set_content_size(Vector2::create(320.0, 200.0));
/// node_tree[panel_id].add_component(Paint::new()).set_color(Color::create(0.2, 0.4, 0.8, 1.0));
/// node_tree.create_child(panel_id, "title");
///
/// let mut document = UIDocument::new();
/// let json_text = document.to_json_text(&node_tree, panel_id, true);
/// let loaded_id = document.from_json_text(&mut node_tree, &json_text).expect("document");
/// assert_eq!(node_tree[loaded_id].get_content_size(), Vector2::create(320.0, 200.0));
/// assert_eq!(node_tree[loaded_id].get_component::<Paint>().expect("paint").get_color(), Color::create(0.2, 0.4, 0.8, 1.0));
/// assert!(node_tree.find_child_by_name(loaded_id, "title").is_some());
/// assert_eq!(document.to_json_text(&node_tree, loaded_id, true), json_text);
/// ```
///
/// 씬 파일을 읽어 씬 뿌리에 붙이기
/// ```rust
/// use vanilla::{NodeTree, UIDocument};
///
/// let json_text = std::fs::read_to_string("examples/assets/scenedocument.scene.json").expect("scene file");
/// let mut node_tree = NodeTree::new();
/// let mut document = UIDocument::new();
/// let scene_root_id = document.from_json_text(&mut node_tree, &json_text).expect("document");
/// let root_id = node_tree.get_root_id();
/// node_tree.add_child(root_id, scene_root_id);
/// assert_eq!(node_tree.get_parent_id(scene_root_id), Some(root_id));
/// assert!(node_tree.find_child_by_name(scene_root_id, "Panel").is_some());
/// ```
//==============================================================================
pub struct UIDocument {
	component_type_list: Vec<UIDocumentComponentType>,
	node_type_list: Vec<UIDocumentNodeType>,
	image_resolver: Option<Box<dyn FnMut(&str) -> Option<Rc<Image>>>>,
	image_asset_map: HashMap<String, ImageAsset>,
	pending_image_list: Vec<PendingImage>,
}

impl UIDocument {
	//==============================================================================
	/// 생성. (원본 표의 엔진 컴포넌트를 등록해 둔다)
	/// # 돌려줌
	/// 엔진 컴포넌트(Paint, Sprite, Text, RichText, Mask, Outline, SoftShade, ShadowText, UIProgressRing, UILabel, UIImageView, UIButton, UIToggleButton, UIProgressView, UISlider, UIScrollView, ShadowImage)를 등록한 새 문서. 노드 종류는 없고 이미지 해석기도 없다.
	//==============================================================================
	pub fn new() -> UIDocument {
		let mut document = UIDocument {
			component_type_list: Vec::new(),
			node_type_list: Vec::new(),
			image_resolver: None,
			image_asset_map: HashMap::new(),
			pending_image_list: Vec::new(),
		};
		document.register_engine_component_types();
		document
	}

	//==============================================================================
	/// 엔진 컴포넌트 등록. (원본 COMPONENT_TYPE_TABLE 차례, 끝에 러스트 판이 더한 ShadowImage)
	/// - 원본 COMPONENT_PROPERTY_TABLE 에 쓰기, 읽기 함수가 있는 종류는 그 함수와, 없는 종류(Outline, SoftShade, ShadowText, UIProgressRing)는 반사로 등록한다.
	//==============================================================================
	fn register_engine_component_types(&mut self) {
		self.register_component_type_with_handler("Paint", Paint::new, save_paint, load_paint);
		self.register_component_type_with_handler("Sprite", Sprite::new, save_sprite, load_sprite);
		self.register_component_type_with_handler("Text", Text::new, save_text, load_text);
		self.register_component_type_with_handler("RichText", RichText::new, save_rich_text, load_rich_text);
		self.register_component_type_with_handler("Mask", Mask::new, save_mask, load_mask);
		self.register_component_type("Outline", Outline::new);
		self.register_component_type("SoftShade", SoftShade::new);
		self.register_component_type("ShadowText", ShadowText::new);
		self.register_component_type("UIProgressRing", UIProgressRing::new);
		self.register_widget_type_with_handler("UILabel", UILabel::new, save_label, load_label, UILabel::get_content, &[]);
		self.register_widget_type_with_handler("UIImageView", UIImageView::new, save_image_view, load_image_view, UIImageView::get_content, &[]);
		self.set_component_companions("UIImageView", save_image_view_companions, load_image_view_companions);
		self.register_widget_type_with_handler("UIButton", UIButton::new, save_button, load_button, UIButton::get_content, &[]);
		self.register_widget_type_with_handler("UIToggleButton", UIToggleButton::new, save_toggle_button, load_toggle_button, UIToggleButton::get_content, &[]);
		self.register_widget_type_with_handler("UIProgressView", UIProgressView::new, save_progress_view, load_progress_view, UIProgressView::get_content, &[]);
		self.register_widget_type_with_handler("UISlider", UISlider::new, save_slider, load_slider, UISlider::get_content, &[]);
		self.register_widget_type_with_handler("UIScrollView", UIScrollView::new, save_scroll_view, load_scroll_view, UIScrollView::get_content, &["Mask"]);
		self.register_component_type_with_handler("ShadowImage", ShadowImage::new, save_shadow_image, load_shadow_image);
	}

	//==============================================================================
	/// 원본 에디터가 등록하는 종류 모두 등록. (원본 registerEditorTypes, 원본 tools/uieditor/uieditor.js 는 엔진이 내보내는 모든 Component 를 registerComponentType, WorldNode 하위 클래스를 registerNodeType 한다)
	/// - 원본처럼 내보낸 이름 차례(abc)로 등록한다. 이미 등록한 이름(원본 기본 표)은 원본처럼 쓰기, 읽기 함수를 그대로 둔다.
	/// - 원본도 기본 표에는 없어 게임이 등록해야 읽는다. 에디터가 저장한 문서를 읽는 게임은 이것을 한 번 부른다.
	//==============================================================================
	pub fn register_editor_types(&mut self) {
		self.register_editor_component_type("ButtonMotion", ButtonMotion::new);
		self.register_editor_component_type("HeatHazeEffect", HeatHazeEffect::new);
		self.register_editor_component_type("Mask", Mask::new);
		self.register_editor_component_type("Outline", Outline::new);
		self.register_editor_component_type("Paint", Paint::new);
		self.register_editor_component_type("ParticleSystem", ParticleSystem::new);
		self.register_editor_component_type("PlatformerBody", PlatformerBody::new);
		self.register_editor_component_type("RichText", RichText::new);
		self.register_editor_component_type("ShaderSprite", ShaderSprite::new);
		self.register_editor_component_type("ShadowImage", ShadowImage::new);
		self.register_editor_component_type("ShadowText", ShadowText::new);
		self.register_editor_component_type("SoftShade", SoftShade::new);
		self.register_editor_component_type("Sprite", Sprite::new);
		self.register_editor_component_type("SpriteAnimator", SpriteAnimator::new);
		self.register_editor_component_type("Text", Text::new);
		self.register_editor_component_type("ToastMotion", || ToastMotion::new(-1.0));
		self.register_editor_component_type("ToonView3D", ToonView3D::new);
		self.register_editor_component_type("TrailRenderer", TrailRenderer::new);
		self.register_editor_component_type("UIBarChart", UIBarChart::new);
		self.register_editor_widget_type("UIButton", UIButton::new, UIButton::get_content, &[]);
		self.register_editor_widget_type("UIControl", UIControl::new, UIControl::get_content, &[]);
		self.register_editor_widget_type("UIDraggable", UIDraggable::new, UIDraggable::get_content, &[]);
		self.register_editor_component_type("UIHitArea", UIHitArea::new);
		self.register_editor_widget_type("UIImageView", UIImageView::new, UIImageView::get_content, &[]);
		self.register_editor_widget_type("UILabel", UILabel::new, UILabel::get_content, &[]);
		self.register_editor_component_type("UILineChart", UILineChart::new);
		self.register_editor_component_type("UIListView", UIListView::new);
		self.register_editor_component_type("UIProgressRing", UIProgressRing::new);
		self.register_editor_widget_type("UIProgressView", UIProgressView::new, UIProgressView::get_content, &[]);
		self.register_editor_widget_type("UIScrollView", UIScrollView::new, UIScrollView::get_content, &["Mask"]);
		self.register_editor_widget_type("UISlider", UISlider::new, UISlider::get_content, &[]);
		self.register_editor_widget_type("UISnapScrollView", UISnapScrollView::new, UISnapScrollView::get_content, &["Mask"]);
		self.register_editor_widget_type("UIToggleButton", UIToggleButton::new, UIToggleButton::get_content, &[]);
		self.register_editor_widget_type("UIView", UIView::new, UIView::get_content, &[]);
		self.register_editor_component_type("Visual", Visual::new);
		self.register_node_type("FloatingText", FloatingText::new);
		self.register_node_type("TouchEffect", TouchEffect::new);
		self.register_node_type_with_match("UIConfirmWindow", || UIWindow::create_with_behavior(Box::new(UIConfirmWindow::create_with_options(UIConfirmWindowOptions::default()))), |window| window.get_behavior::<UIConfirmWindow>().is_some());
		self.register_node_type("UIContextMenu", UIContextMenu::new);
		self.register_node_type("UIDialog", UIDialog::new);
		self.register_node_type("UIDropdown", UIDropdown::new);
		self.register_node_type("UIInputField", UIInputField::new);
		self.register_node_type("UIScrollBar", UIScrollBar::new);
		self.register_node_type("UISpinner", UISpinner::new);
		self.register_node_type("UIToast", UIToast::new);
		self.register_node_type("UIWindow", || UIWindow::create_with_behavior(Box::new(UIWindowDefaultBehavior)));
		self.register_node_type("UIWindowLayer", UIWindowLayer::new);
	}

	//==============================================================================
	/// 에디터가 등록하는 컴포넌트 종류 하나. (이미 등록한 이름이면 그대로 둔다)
	//==============================================================================
	fn register_editor_component_type<T: Component + 'static>(&mut self, type_name: &str, create_component: fn() -> T) {
		if self.component_type_list.iter().any(|registered_type| registered_type.type_name == type_name) {
			return;
		}
		self.register_component_type(type_name, create_component);
	}

	//==============================================================================
	/// 에디터가 등록하는 위젯 종류 하나. (이미 등록한 이름이면 그대로 둔다)
	//==============================================================================
	fn register_editor_widget_type<T: Component + 'static>(&mut self, type_name: &str, create_component: fn() -> T, read_content: fn(&T) -> Option<NodeId>, generated_type_names: &[&str]) {
		if self.component_type_list.iter().any(|registered_type| registered_type.type_name == type_name) {
			return;
		}
		self.register_widget_type(type_name, create_component, read_content, generated_type_names);
	}

	//==============================================================================
	/// 컴포넌트 종류 등록. (원본 registerComponentType, 같은 이름이면 바꾼다)
	/// - 쓰기, 읽기 함수가 없어 원본처럼 반사로 쓰고 읽는다. (`#[reflect]` 를 붙인 컴포넌트, 붙이지 않았으면 Component 공통 속성만)
	/// - create_component 로 짓고 node_tree.add_component 로 붙인 뒤 문서 값을 넣는다. (원본처럼 붙인 다음 읽는다) 쓸 때는 값 끝에 "type" 을 더한다.
	/// # 인자
	/// - `type_name`: 문서의 "type" 이름. (같은 이름이 있으면 바꾼다)
	/// - `create_component`: 읽을 때 컴포넌트를 짓는 함수.
	///
	/// # 예제
	/// `#[vanilla::reflect]` 를 붙인 게임 컴포넌트를 등록 한 줄로 쓰고 읽기
	/// ```rust
	/// use vanilla::{Component, ComponentBase, NodeTree, UIDocument};
	///
	/// struct Spin {
	/// 	component_base: ComponentBase,
	/// 	degrees_per_second: f32,
	/// }
	///
	/// #[vanilla::reflect]
	/// impl Spin {
	/// 	pub fn new() -> Spin {
	/// 		Spin {
	/// 			component_base: ComponentBase::new(),
	/// 			degrees_per_second: 0.0,
	/// 		}
	/// 	}
	///
	/// 	pub fn set_degrees_per_second(&mut self, degrees_per_second: f32) {
	/// 		self.degrees_per_second = degrees_per_second;
	/// 	}
	///
	/// 	pub fn get_degrees_per_second(&self) -> f32 {
	/// 		self.degrees_per_second
	/// 	}
	/// }
	///
	/// #[vanilla::reflect]
	/// impl Component for Spin {
	/// 	fn get_component_base(&self) -> &ComponentBase {
	/// 		&self.component_base
	/// 	}
	///
	/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
	/// 		&mut self.component_base
	/// 	}
	///
	/// 	fn get_component_type(&self) -> &'static str {
	/// 		"Spin"
	/// 	}
	/// }
	///
	/// let mut document = UIDocument::new();
	/// document.register_component_type("Spin", Spin::new);
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.create_node("root");
	/// node_tree[root_id].add_component(Spin::new()).set_degrees_per_second(90.0);
	///
	/// let json_text = document.to_json_text(&node_tree, root_id, false);
	/// assert!(json_text.contains("\"DegreesPerSecond\":90"));
	/// let loaded_id = document.from_json_text(&mut node_tree, &json_text).expect("document");
	/// assert_eq!(node_tree[loaded_id].get_component::<Spin>().expect("spin").get_degrees_per_second(), 90.0);
	/// ```
	//==============================================================================
	pub fn register_component_type<T: Component + 'static>(&mut self, type_name: &str, create_component: fn() -> T) {
		let component_type = UIDocument::create_component_type(type_name, create_component, None, None);
		self.insert_component_type(component_type);
	}

	//==============================================================================
	/// 쓰기, 읽기 함수가 있는 컴포넌트 종류 등록. (원본 COMPONENT_TYPE_TABLE 과 COMPONENT_PROPERTY_TABLE 의 한 줄, 같은 이름이면 바꾼다)
	/// - save_component 가 돌려준 값 끝에 "type" 을 더해 적는다. 노드를 가리키는 반사 속성은 원본처럼 따로 얹고 읽는다.
	/// # 인자
	/// - `type_name`: 문서의 "type" 이름. (같은 이름이 있으면 바꾼다)
	/// - `create_component`: 읽을 때 컴포넌트를 짓는 함수.
	/// - `save_component`: 컴포넌트를 문서 값(키 → 값)으로 쓰는 함수. ("type" 은 넣지 않는다)
	/// - `load_component`: 붙인 컴포넌트에 문서 값을 읽어 넣는 함수. (그림과 노드 참조는 읽기 문맥에 맡긴다)
	///
	/// # 예제
	/// 반사 없이 쓰기, 읽기 함수로 다루는 게임 컴포넌트
	/// ```rust
	/// use serde_json::{Map, Value};
	/// use vanilla::{Component, ComponentBase, NodeTree, UIDocument, UIDocumentLoadContext, UIDocumentSaveContext};
	///
	/// struct Spin {
	/// 	component_base: ComponentBase,
	/// 	degrees_per_second: f32,
	/// }
	///
	/// impl Component for Spin {
	/// 	fn get_component_base(&self) -> &ComponentBase {
	/// 		&self.component_base
	/// 	}
	///
	/// 	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
	/// 		&mut self.component_base
	/// 	}
	///
	/// 	fn get_component_type(&self) -> &'static str {
	/// 		"Spin"
	/// 	}
	/// }
	///
	/// fn create_spin() -> Spin {
	/// 	Spin {
	/// 		component_base: ComponentBase::new(),
	/// 		degrees_per_second: 0.0,
	/// 	}
	/// }
	///
	/// fn save_spin(spin: &Spin, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	/// 	let mut component_data = Map::new();
	/// 	component_data.insert(String::from("degreesPerSecond"), UIDocument::number_to_value(spin.degrees_per_second));
	/// 	component_data
	/// }
	///
	/// fn load_spin(spin: &mut Spin, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	/// 	if let Some(degrees_per_second) = component_data.get("degreesPerSecond").and_then(|value| value.as_f64()) {
	/// 		spin.degrees_per_second = degrees_per_second as f32;
	/// 	}
	/// }
	///
	/// let mut document = UIDocument::new();
	/// document.register_component_type_with_handler("Spin", create_spin, save_spin, load_spin);
	/// let json_text = r#"{"version":1,"root":{"name":"root","components":[{"degreesPerSecond":90,"type":"Spin"}]}}"#;
	/// let mut node_tree = NodeTree::new();
	/// let root_id = document.from_json_text(&mut node_tree, json_text).expect("document");
	/// assert_eq!(node_tree[root_id].get_component::<Spin>().expect("spin").degrees_per_second, 90.0);
	/// assert!(document.to_json_text(&node_tree, root_id, false).contains("{\"degreesPerSecond\":90,\"type\":\"Spin\"}"));
	/// ```
	//==============================================================================
	pub fn register_component_type_with_handler<T: Component + 'static>(&mut self, type_name: &str, create_component: fn() -> T, save_component: fn(&T, &UIDocumentSaveContext) -> Map<String, Value>, load_component: fn(&mut T, &Map<String, Value>, &mut UIDocumentLoadContext)) {
		let component_type = UIDocument::create_component_type(type_name, create_component, Some(save_component), Some(load_component));
		self.insert_component_type(component_type);
	}

	//==============================================================================
	/// 위젯 종류 등록. (원본의 UIView 계열, 붙을 때 content 노드를 만드는 컴포넌트. 반사로 쓰고 읽는다)
	/// - read_content 가 돌려준 content 노드는 문서에 적지 않고, 그 안의 자식만 contentChildren 으로 적는다. 읽을 때 위젯이 다시 만든 content 에 붙인다.
	/// - generated_type_names 는 위젯이 붙을 때 호스트에 스스로 붙이는 컴포넌트 종류다. (원본 require, 문서에 적지 않는다)
	/// # 인자
	/// - `type_name`: 문서의 "type" 이름. (같은 이름이 있으면 바꾼다)
	/// - `create_component`: 읽을 때 위젯을 짓는 함수.
	/// - `read_content`: 위젯의 content 노드를 돌려주는 함수.
	/// - `generated_type_names`: 위젯이 붙을 때 호스트에 스스로 붙이는 컴포넌트 종류 이름. (없으면 빈 목록, UIScrollView 는 ["Mask"])
	///
	/// # 예제
	/// UIView 를 위젯으로 등록하면 content 안의 자식이 contentChildren 으로 적힌다
	/// ```rust
	/// use serde_json::Value;
	/// use vanilla::{NodeTree, UIDocument, UIView};
	///
	/// let mut document = UIDocument::new();
	/// document.register_widget_type("UIView", UIView::new, UIView::get_content, &[]);
	/// let mut node_tree = NodeTree::new();
	/// let view_id = node_tree.create_node("view");
	/// let content_id = node_tree.add_component(view_id, UIView::new()).expect("view").get_content().expect("content");
	/// node_tree.create_child(content_id, "item");
	///
	/// let json_text = document.to_json_text(&node_tree, view_id, false);
	/// let document_data: Value = serde_json::from_str(&json_text).expect("json");
	/// assert_eq!(document_data["root"]["children"], serde_json::json!([]));
	/// assert_eq!(document_data["root"]["contentChildren"][0]["name"], "item");
	///
	/// let loaded_id = document.from_json_text(&mut node_tree, &json_text).expect("document");
	/// let loaded_content_id = node_tree[loaded_id].get_component::<UIView>().expect("view").get_content().expect("content");
	/// assert!(node_tree.find_child_by_name(loaded_content_id, "item").is_some());
	/// ```
	//==============================================================================
	pub fn register_widget_type<T: Component + 'static>(&mut self, type_name: &str, create_component: fn() -> T, read_content: fn(&T) -> Option<NodeId>, generated_type_names: &[&str]) {
		let component_type = UIDocument::create_widget_type(type_name, create_component, None, None, read_content, generated_type_names);
		self.insert_component_type(component_type);
	}

	//==============================================================================
	/// 쓰기, 읽기 함수가 있는 위젯 종류 등록.
	/// - register_widget_type 과 같되 반사 대신 save_component, load_component 로 쓰고 읽는다.
	///
	/// # 인자
	/// - `type_name`: 문서의 "type" 이름. (같은 이름이 있으면 바꾼다)
	/// - `create_component`: 읽을 때 위젯을 짓는 함수.
	/// - `save_component`: 위젯을 문서 값으로 쓰는 함수. ("type" 은 넣지 않는다)
	/// - `load_component`: 붙인 위젯에 문서 값을 읽어 넣는 함수.
	/// - `read_content`: 위젯의 content 노드를 돌려주는 함수.
	/// - `generated_type_names`: 위젯이 붙을 때 호스트에 스스로 붙이는 컴포넌트 종류 이름.
	//==============================================================================
	pub fn register_widget_type_with_handler<T: Component + 'static>(&mut self, type_name: &str, create_component: fn() -> T, save_component: fn(&T, &UIDocumentSaveContext) -> Map<String, Value>, load_component: fn(&mut T, &Map<String, Value>, &mut UIDocumentLoadContext), read_content: fn(&T) -> Option<NodeId>, generated_type_names: &[&str]) {
		let component_type = UIDocument::create_widget_type(type_name, create_component, Some(save_component), Some(load_component), read_content, generated_type_names);
		self.insert_component_type(component_type);
	}

	//==============================================================================
	/// 노드 종류 등록. (원본 registerNodeType, 원본 WorldNode 하위 클래스를 맡은 러스트 컴포넌트, 같은 이름이면 바꾼다)
	/// - 그 컴포넌트가 붙은 노드는 type 을 이 이름으로 적고 컴포넌트 목록에는 그 컴포넌트를 적지 않는다. 읽을 때는 노드를 짓자마자(문서의 노드 값을 넣기 전에) 붙인다.
	///   (원본은 그 종류의 생성자로 노드를 짓고 노드 값을 넣는다)
	/// - 원본 생성자가 만드는 컴포넌트와 자식은 UIDocumentNode 가 쓰고 읽는다.
	/// # 인자
	/// - `type_name`: 문서 노드의 "type" 이름. (같은 이름이 있으면 바꾼다)
	/// - `create_component`: 읽을 때 노드를 짓자마자 붙일 컴포넌트를 짓는 함수.
	///
	/// # 예제
	/// UIScrollBar 노드를 원본 노드 꼴(트랙 Paint, 썸 자식)로 쓰고 읽기
	/// ```rust
	/// use serde_json::Value;
	/// use vanilla::{NodeTree, UIDocument, UIScrollBar};
	///
	/// let mut document = UIDocument::new();
	/// document.register_node_type("UIScrollBar", UIScrollBar::new);
	/// let mut node_tree = NodeTree::new();
	/// let scroll_bar_id = node_tree.create_node("scrollBar");
	/// node_tree.add_component(scroll_bar_id, UIScrollBar::new());
	///
	/// let json_text = document.to_json_text(&node_tree, scroll_bar_id, false);
	/// let document_data: Value = serde_json::from_str(&json_text).expect("json");
	/// assert_eq!(document_data["root"]["type"], "UIScrollBar");
	/// assert_eq!(document_data["root"]["components"][0]["type"], "Paint");
	/// assert_eq!(document_data["root"]["children"][0]["name"], "thumb");
	///
	/// let loaded_id = document.from_json_text(&mut node_tree, &json_text).expect("document");
	/// assert!(node_tree[loaded_id].get_component::<UIScrollBar>().is_some());
	/// assert!(node_tree.get_child_ids(loaded_id).is_empty());
	/// ```
	//==============================================================================
	pub fn register_node_type<T: Component + UIDocumentNode + 'static>(&mut self, type_name: &str, create_component: fn() -> T) {
		self.insert_node_type::<T>(type_name, create_component, None);
	}

	//==============================================================================
	/// 노드 종류 등록. (컴포넌트 종류만으로 가리지 못할 때 matches 가 참인 것만 이 노드 종류다. UIConfirmWindow 는 UIWindow 이고 behavior 로 가린다)
	/// - 가리는 함수가 있는 등록이 없는 등록보다 먼저 맞춰 본다.
	/// # 인자
	/// - `type_name`: 문서 노드의 "type" 이름. (같은 이름이 있으면 바꾼다)
	/// - `create_component`: 읽을 때 노드를 짓자마자 붙일 컴포넌트를 짓는 함수.
	/// - `matches`: 노드에 붙은 그 타입의 컴포넌트가 이 노드 종류인지 가리는 함수.
	//==============================================================================
	pub fn register_node_type_with_match<T: Component + UIDocumentNode + 'static>(&mut self, type_name: &str, create_component: fn() -> T, matches: fn(&T) -> bool) {
		let match_component: MatchNodeTypeFunction = Box::new(move |component: &dyn Component| {
			component.as_any().downcast_ref::<T>().map(matches).unwrap_or(false)
		});
		self.insert_node_type::<T>(type_name, create_component, Some(match_component));
	}

	//==============================================================================
	/// 노드 종류 넣기. (같은 이름이면 그 자리에서 바꾼다)
	//==============================================================================
	fn insert_node_type<T: Component + UIDocumentNode + 'static>(&mut self, type_name: &str, create_component: fn() -> T, match_component: Option<MatchNodeTypeFunction>) {
		let node_type = UIDocumentNodeType {
			type_name: String::from(type_name),
			type_id: TypeId::of::<T>(),
			match_component,
			add_component: UIDocument::create_add_component_function(create_component),
			save_node_parts: Box::new(|component: &dyn Component, save_context: &UIDocumentSaveContext, node_id: NodeId| {
				match component.as_any().downcast_ref::<T>() {
					Some(typed_component) => {
						typed_component.save_node_parts(save_context, node_id)
					}
					None => {
						UIDocumentNodeParts::new()
					}
				}
			}),
			load_node_parts: Box::new(|component: &mut dyn Component, component_data_list: &[Value], child_data_list: &[Value]| {
				match component.as_any_mut().downcast_mut::<T>() {
					Some(typed_component) => {
						typed_component.load_node_parts(component_data_list, child_data_list)
					}
					None => {
						(0, 0)
					}
				}
			}),
			read_content: Box::new(|component: &dyn Component| {
				component.as_any().downcast_ref::<T>().and_then(|typed_component| typed_component.get_document_content())
			}),
			read_document_active: Box::new(|component: &dyn Component, is_node_active: bool, is_node_visible: bool| {
				match component.as_any().downcast_ref::<T>() {
					Some(typed_component) => {
						typed_component.read_document_active(is_node_active, is_node_visible)
					}
					None => {
						is_node_active
					}
				}
			}),
			convert_document_active: Box::new(|component: &dyn Component, is_active: bool| {
				match component.as_any().downcast_ref::<T>() {
					Some(typed_component) => {
						typed_component.convert_document_active(is_active)
					}
					None => {
						(is_active, None)
					}
				}
			}),
		};
		match self.node_type_list.iter().position(|registered_type| registered_type.type_name == node_type.type_name) {
			Some(registered_index) => {
				self.node_type_list[registered_index] = node_type;
			}
			None => {
				self.node_type_list.push(node_type);
			}
		}
	}

	//==============================================================================
	/// 짓고 붙이는 함수. (붙인 컴포넌트의 차례 번호를 돌려준다)
	//==============================================================================
	fn create_add_component_function<T: Component + 'static>(create_component: fn() -> T) -> AddComponentFunction {
		Box::new(move |node_tree: &mut NodeTree, node_id: NodeId| {
			let component_index = node_tree.get_node(node_id)?.get_all_components().len();
			node_tree.add_component(node_id, create_component())?;
			Some(component_index)
		})
	}

	//==============================================================================
	/// 위젯 종류 하나 만들기.
	//==============================================================================
	fn create_widget_type<T: Component + 'static>(type_name: &str, create_component: fn() -> T, save_component: Option<fn(&T, &UIDocumentSaveContext) -> Map<String, Value>>, load_component: Option<fn(&mut T, &Map<String, Value>, &mut UIDocumentLoadContext)>, read_content: fn(&T) -> Option<NodeId>, generated_type_names: &[&str]) -> UIDocumentComponentType {
		let mut component_type = UIDocument::create_component_type(type_name, create_component, save_component, load_component);
		component_type.read_content = Some(Box::new(move |component: &dyn Component| {
			component.as_any().downcast_ref::<T>().and_then(read_content)
		}));
		component_type.generated_type_names = generated_type_names.iter().map(|generated_type_name| String::from(*generated_type_name)).collect();
		component_type
	}

	//==============================================================================
	/// 컴포넌트 종류 하나 만들기. (구체 타입 함수를 컴포넌트 트레이트 객체로 다루는 함수로 감싼다)
	//==============================================================================
	fn create_component_type<T: Component + 'static>(type_name: &str, create_component: fn() -> T, save_component: Option<fn(&T, &UIDocumentSaveContext) -> Map<String, Value>>, load_component: Option<fn(&mut T, &Map<String, Value>, &mut UIDocumentLoadContext)>) -> UIDocumentComponentType {
		UIDocumentComponentType {
			type_name: String::from(type_name),
			type_id: TypeId::of::<T>(),
			add_component: UIDocument::create_add_component_function(create_component),
			save_component: save_component.map(|save_component| -> SaveComponentFunction {
				Box::new(move |component: &dyn Component, save_context: &UIDocumentSaveContext| {
					match component.as_any().downcast_ref::<T>() {
						Some(typed_component) => {
							save_component(typed_component, save_context)
						}
						None => {
							Map::new()
						}
					}
				})
			}),
			load_component: load_component.map(|load_component| -> LoadComponentFunction {
				Box::new(move |component: &mut dyn Component, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext| {
					if let Some(typed_component) = component.as_any_mut().downcast_mut::<T>() {
						load_component(typed_component, component_data, load_context);
					}
				})
			}),
			read_content: None,
			generated_type_names: Vec::new(),
			save_companions: None,
			load_companions: None,
		}
	}

	//==============================================================================
	/// 원본에서는 호스트 노드에 따로 붙는 컴포넌트를 품은 종류의 그 몫 쓰기와 읽기 등록. (원본 문서에는 그 종류 바로 뒤에 따로 적힌다)
	/// - 원본 UIImageView 는 붙을 때 호스트에 Sprite 를 붙이고(getOrAddComponent) 문서에 그 Sprite 가 적힌다. 러스트 판은 Sprite 를 품는다.
	//==============================================================================
	fn set_component_companions<T: Component + 'static>(&mut self, type_name: &str, save_companions: fn(&T, &UIDocumentSaveContext) -> Vec<Map<String, Value>>, load_companions: fn(&mut T, &[Value], &mut UIDocumentLoadContext) -> usize) {
		let component_type = match self.component_type_list.iter_mut().find(|registered_type| registered_type.type_name == type_name) {
			Some(component_type) => {
				component_type
			}
			None => {
				return;
			}
		};
		component_type.save_companions = Some(Box::new(move |component: &dyn Component, save_context: &UIDocumentSaveContext| {
			match component.as_any().downcast_ref::<T>() {
				Some(typed_component) => {
					save_companions(typed_component, save_context)
				}
				None => {
					Vec::new()
				}
			}
		}));
		component_type.load_companions = Some(Box::new(move |component: &mut dyn Component, component_data_list: &[Value], load_context: &mut UIDocumentLoadContext| {
			match component.as_any_mut().downcast_mut::<T>() {
				Some(typed_component) => {
					load_companions(typed_component, component_data_list, load_context)
				}
				None => {
					0
				}
			}
		}));
	}

	//==============================================================================
	/// 컴포넌트 종류 넣기. (같은 이름이 있으면 그 자리에서 바꾸고, 없으면 끝에 더한다. 원본 객체 키 대입과 같은 차례)
	//==============================================================================
	fn insert_component_type(&mut self, component_type: UIDocumentComponentType) {
		match self.component_type_list.iter().position(|registered_type| registered_type.type_name == component_type.type_name) {
			Some(registered_index) => {
				self.component_type_list[registered_index] = component_type;
			}
			None => {
				self.component_type_list.push(component_type);
			}
		}
	}

	//==============================================================================
	/// 컴포넌트에 맞는 종류 찾기. (같은 타입을 여러 이름으로 등록했으면 나중 것)
	//==============================================================================
	fn find_component_type(&self, component: &dyn Component) -> Option<&UIDocumentComponentType> {
		let component_type_id = component.as_any().type_id();
		self.component_type_list.iter().rev().find(|registered_type| registered_type.type_id == component_type_id)
	}

	//==============================================================================
	/// 컴포넌트에 맞는 노드 종류 찾기. (같은 타입을 여러 이름으로 등록했으면 나중 것)
	//==============================================================================
	fn find_node_type(&self, component: &dyn Component) -> Option<&UIDocumentNodeType> {
		let component_type_id = component.as_any().type_id();
		let matched_type = self.node_type_list.iter().rev().find(|registered_type| {
			match &registered_type.match_component {
				Some(match_component) => {
					registered_type.type_id == component_type_id && match_component(component)
				}
				None => {
					false
				}
			}
		});
		matched_type.or_else(|| self.node_type_list.iter().rev().find(|registered_type| registered_type.match_component.is_none() && registered_type.type_id == component_type_id))
	}

	//==============================================================================
	/// 노드 종류 이름. (원본 node.constructor.name 자리, 저장할 때 적는 type 과 같다)
	/// - 등록한 노드 종류(register_node_type, 엔진 종류는 register_editor_types 가 등록)의 컴포넌트가 붙은 노드면 그 이름(앞에 붙은 것), 아니면 "WorldNode".
	///
	/// # 인자
	/// - `node`: 볼 노드.
	///
	/// # 돌려줌
	/// 노드 종류 이름.
	///
	/// # 예제
	/// 등록한 엔진 노드 종류는 그 이름, 맨 노드는 WorldNode
	/// ```rust
	/// use vanilla::{FloatingText, UIDocument, WorldNode};
	///
	/// let mut ui_document = UIDocument::new();
	/// ui_document.register_editor_types();
	/// let mut node = WorldNode::new("node");
	/// assert_eq!(ui_document.get_node_type_name(&node), "WorldNode");
	/// node.add_component(FloatingText::new());
	/// assert_eq!(ui_document.get_node_type_name(&node), "FloatingText");
	/// ```
	//==============================================================================
	pub fn get_node_type_name(&self, node: &WorldNode) -> &str {
		node.get_all_components().iter().find_map(|component| self.find_node_type(component.as_ref())).map(|node_type| node_type.type_name.as_str()).unwrap_or(NODE_TYPE_NAME)
	}

	//==============================================================================
	/// 지원 컴포넌트 종류 이름 목록. (원본 getComponentTypeNames, 등록 차례)
	/// # 돌려줌
	/// 등록한 컴포넌트 종류 이름. (등록 차례, 노드 종류 이름은 들지 않는다)
	//==============================================================================
	pub fn get_component_type_names(&self) -> Vec<&str> {
		self.component_type_list.iter().map(|registered_type| registered_type.type_name.as_str()).collect()
	}

	//==============================================================================
	/// 이름으로 컴포넌트를 지어 노드에 붙인다. (원본 getComponentType 으로 받은 생성자로 짓고 붙이는 자리, 등록하지 않은 이름이면 None)
	/// # 인자
	/// - `node_tree`: 노드가 든 트리.
	/// - `node_id`: 컴포넌트를 붙일 노드.
	/// - `type_name`: 등록한 컴포넌트 종류 이름.
	///
	/// # 돌려줌
	/// 붙인 컴포넌트의 차례 번호. (그 노드의 컴포넌트 목록 안) 등록하지 않은 이름이거나 노드가 없으면 None.
	///
	/// # 예제
	/// 이름으로 Paint 붙이기
	/// ```rust
	/// use vanilla::{NodeTree, Paint, UIDocument};
	///
	/// let document = UIDocument::new();
	/// let mut node_tree = NodeTree::new();
	/// let node_id = node_tree.create_node("node");
	/// assert_eq!(document.create_component(&mut node_tree, node_id, "Paint"), Some(0));
	/// assert!(node_tree[node_id].get_component::<Paint>().is_some());
	/// assert_eq!(document.create_component(&mut node_tree, node_id, "Unknown"), None);
	/// ```
	//==============================================================================
	pub fn create_component(&self, node_tree: &mut NodeTree, node_id: NodeId, type_name: &str) -> Option<usize> {
		let component_type = self.component_type_list.iter().find(|registered_type| registered_type.type_name == type_name)?;
		(component_type.add_component)(node_tree, node_id)
	}

	//==============================================================================
	/// 이미지 경로 해석기 등록. (원본 setImageResolver, 경로 → 그림. 등록하면 애셋 창고 대신 이것으로 찾는다)
	/// # 인자
	/// - `image_resolver`: 그림 경로를 받아 그림을 돌려주는 함수. None 을 돌려주면 그 그림은 찾지 못한 것이다.
	///
	/// # 예제
	/// 미리 가진 그림을 경로로 넣기
	/// ```rust
	/// use std::rc::Rc;
	/// use vanilla::{Image, NodeTree, Sprite, UIDocument};
	///
	/// let star_image = Image::create_from_pixels(1, 1, vec![255, 255, 255, 255]);
	/// let resolved_image = star_image.clone();
	/// let mut document = UIDocument::new();
	/// document.set_image_resolver(move |image_path: &str| {
	/// 	if image_path == "star.png" {
	/// 		Some(resolved_image.clone())
	/// 	}
	/// 	else {
	/// 		None
	/// 	}
	/// });
	/// let json_text = r#"{"version":1,"root":{"name":"star","components":[{"imagePath":"star.png","type":"Sprite"}]}}"#;
	/// let mut node_tree = NodeTree::new();
	/// let star_id = document.from_json_text(&mut node_tree, json_text).expect("document");
	/// let sprite = node_tree[star_id].get_component::<Sprite>().expect("sprite");
	/// assert!(sprite.get_image().is_some_and(|image| Rc::ptr_eq(image, &star_image)));
	/// assert!(document.resolve_image("missing.png").is_none());
	/// ```
	//==============================================================================
	pub fn set_image_resolver(&mut self, image_resolver: impl FnMut(&str) -> Option<Rc<Image>> + 'static) {
		self.image_resolver = Some(Box::new(image_resolver));
	}

	//==============================================================================
	/// 이미지 경로 해석. (원본 resolveImage, 찾지 못했거나 아직 읽히는 중이면 None)
	/// # 인자
	/// - `image_path`: 찾을 그림 경로.
	///
	/// # 돌려줌
	/// 찾은 그림. 해석기가 None 을 주었거나, 해석기가 없고 load_images 로 요청하지 않았거나 아직 읽히는 중이거나 읽기에 실패했으면 None.
	//==============================================================================
	pub fn resolve_image(&mut self, image_path: &str) -> Option<Rc<Image>> {
		match self.find_image(image_path) {
			ImageLookup::Found(image) => {
				Some(image)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	/// 그림 찾기. (해석기가 있으면 해석기, 없으면 load_images 로 요청한 애셋)
	//==============================================================================
	fn find_image(&mut self, image_path: &str) -> ImageLookup {
		if let Some(image_resolver) = self.image_resolver.as_mut() {
			return match image_resolver(image_path) {
				Some(image) => {
					ImageLookup::Found(image)
				}
				None => {
					ImageLookup::Missing
				}
			};
		}
		let image_asset = match self.image_asset_map.get_mut(image_path) {
			Some(image_asset) => {
				image_asset
			}
			None => {
				return ImageLookup::Missing;
			}
		};
		if let Some(image) = image_asset.get_image() {
			return ImageLookup::Found(image);
		}
		let is_load_finished = image_asset.get_asset_base().get_load_handle().is_none_or(|load_handle| load_handle.is_finished());
		if is_load_finished {
			ImageLookup::Missing
		}
		else {
			ImageLookup::Waiting
		}
	}

	//==============================================================================
	/// 문서의 그림 읽기 요청. (imagePath 마다 ImageAsset 을 요청해 창고에 든다. 이미 요청한 경로는 건너뛴다)
	/// - 새로 요청한 애셋의 읽기 손잡이를 돌려준다. 씬이 create 에서 부르면 track_load_handle 로 맡겨 initialize 를 기다리게 할 수 있다.
	/// - 데스크탑은 그 자리에서 읽히고, 웹은 나중에 읽힌다. 읽히기 전에 지은 노드의 그림은 bind_pending_images 가 붙인다.
	/// # 인자
	/// - `engine`: 애셋을 읽을 엔진. (데스크탑은 EngineConfiguration 의 asset_directory_paths 에서 찾는다)
	/// - `json_text`: 문서 글.
	///
	/// # 돌려줌
	/// 새로 요청한 애셋의 읽기 손잡이 목록. JSON 이 아니거나 새로 요청할 경로가 없으면 빈 목록.
	///
	/// # 예제
	/// 문서의 그림을 애셋 폴더에서 읽어 붙이기
	/// ```rust
	/// use std::path::PathBuf;
	/// use vanilla::{Engine, EngineConfiguration, NodeTree, Sprite, UIDocument};
	///
	/// let mut engine_configuration = EngineConfiguration::new();
	/// engine_configuration.asset_directory_paths = vec![PathBuf::from("examples/assets")];
	/// let engine = Engine::new(engine_configuration);
	/// let json_text = r#"{"version":1,"root":{"name":"icons","components":[{"imagePath":"atlas/icons.png","type":"Sprite"}]}}"#;
	///
	/// let mut document = UIDocument::new();
	/// let load_handle_list = document.load_images(&engine, json_text);
	/// assert_eq!(load_handle_list.len(), 1);
	/// let mut node_tree = NodeTree::new();
	/// let icons_id = document.from_json_text(&mut node_tree, json_text).expect("document");
	/// assert_eq!(document.bind_pending_images(&mut node_tree), 0);
	/// assert!(node_tree[icons_id].get_component::<Sprite>().expect("sprite").get_image().is_some());
	/// ```
	//==============================================================================
	pub fn load_images(&mut self, engine: &Engine, json_text: &str) -> Vec<AssetLoadHandle> {
		let document_data: Value = match serde_json::from_str(json_text) {
			Ok(document_data) => {
				document_data
			}
			Err(_) => {
				return Vec::new();
			}
		};
		let mut image_path_list: Vec<String> = Vec::new();
		collect_image_paths(&document_data, &mut image_path_list);
		let mut load_handle_list: Vec<AssetLoadHandle> = Vec::new();
		for image_path in image_path_list {
			if self.image_asset_map.contains_key(&image_path) {
				continue;
			}
			let mut image_asset = ImageAsset::new();
			image_asset.load(engine, &image_path);
			if let Some(load_handle) = image_asset.get_asset_base().get_load_handle() {
				load_handle_list.push(load_handle.clone());
			}
			self.image_asset_map.insert(image_path, image_asset);
		}
		load_handle_list
	}

	//==============================================================================
	/// 맡아 둔 그림 붙이기. (찾은 그림은 넣고, 아직 읽히는 그림은 남긴다. 남은 수를 돌려준다)
	/// - from_json_text, deserialize_node 가 끝에 한 번 부른다. 웹처럼 그림이 나중에 읽히면 씬의 tick 에서 남은 수가 0 이 될 때까지 부른다.
	/// - 찾지 못한 그림(해석기가 None, 애셋이 없거나 실패)은 경고를 내고 버린다. 그 컴포넌트는 원본처럼 그림 없이 남는다.
	/// # 인자
	/// - `node_tree`: 그림을 넣을 노드 트리. (맡긴 노드가 지워졌으면 그 그림은 버린다)
	///
	/// # 돌려줌
	/// 아직 읽히는 중이라 남은 그림 수. (0 이면 다 붙였거나 버렸다)
	//==============================================================================
	pub fn bind_pending_images(&mut self, node_tree: &mut NodeTree) -> usize {
		let pending_image_list = std::mem::take(&mut self.pending_image_list);
		for pending_image in pending_image_list {
			if !node_tree.contains(pending_image.node_id) {
				continue;
			}
			match self.find_image(&pending_image.image_path) {
				ImageLookup::Found(image) => {
					if let Some(component) = node_tree.get_node_mut(pending_image.node_id).and_then(|node| node.components.get_mut(pending_image.component_index)) {
						(pending_image.apply_image)(component.as_mut(), image);
					}
				}
				ImageLookup::Waiting => {
					self.pending_image_list.push(pending_image);
				}
				ImageLookup::Missing => {
					eprintln!("UIDocument: 그림을 찾지 못했습니다 ({})", pending_image.image_path);
				}
			}
		}
		self.pending_image_list.len()
	}

	//==============================================================================
	/// 노드 트리 → 문서 데이터. (원본 serializeNode, 노드 참조는 이 노드 기준 경로로 적는다)
	/// # 인자
	/// - `node_tree`: 노드가 든 트리.
	/// - `node_id`: 문서 값으로 쓸 노드. (자손까지)
	///
	/// # 돌려줌
	/// 노드 문서 값. ("version" 머리 없는 노드 객체) 노드가 없으면 Value::Null.
	///
	/// # 예제
	/// 노드를 문서 값으로 떠서 복제본 짓기
	/// ```rust
	/// use vanilla::{NodeTree, UIDocument, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let card_id = node_tree.create_node("card");
	/// node_tree[card_id].set_content_size(Vector2::create(120.0, 80.0));
	/// node_tree.create_child(card_id, "icon");
	///
	/// let mut document = UIDocument::new();
	/// let card_data = document.serialize_node(&node_tree, card_id);
	/// assert_eq!(card_data["name"], "card");
	/// let copy_id = document.deserialize_node(&mut node_tree, &card_data);
	/// assert_ne!(copy_id, card_id);
	/// assert_eq!(node_tree[copy_id].get_content_size(), Vector2::create(120.0, 80.0));
	/// assert!(node_tree.find_child_by_name(copy_id, "icon").is_some());
	/// assert_eq!(node_tree.get_parent_id(copy_id), None);
	/// ```
	//==============================================================================
	pub fn serialize_node(&self, node_tree: &NodeTree, node_id: NodeId) -> Value {
		self.serialize_node_from_root(node_tree, node_id, node_id, None)
	}

	//==============================================================================
	/// 노드 트리 → 문서 데이터. (&mut self 게터가 있는 컴포넌트는 &mut 갈래로 읽는다. 원본 게터에는 &self, &mut self 구별이 없다)
	/// # 인자
	/// - `node_tree`: 노드가 든 트리. (&mut 게터를 부르려고 가변으로 받는다)
	/// - `node_id`: 문서 값으로 쓸 노드. (자손까지)
	///
	/// # 돌려줌
	/// 노드 문서 값. 노드가 없으면 Value::Null.
	//==============================================================================
	pub fn serialize_node_mut(&self, node_tree: &mut NodeTree, node_id: NodeId) -> Value {
		let prefetched_value_map = UIDocument::prefetch_mutable_values(node_tree, node_id);
		self.serialize_node_from_root(node_tree, node_id, node_id, Some(&prefetched_value_map))
	}

	//==============================================================================
	/// &mut self 게터가 있는 컴포넌트의 반사 속성 값을 미리 읽기. (그 노드와 자손)
	//==============================================================================
	fn prefetch_mutable_values(node_tree: &mut NodeTree, root_id: NodeId) -> PrefetchedValueMap {
		let mut prefetched_value_map = PrefetchedValueMap::new();
		let mut pending_node_ids: Vec<NodeId> = vec![root_id];
		while let Some(node_id) = pending_node_ids.pop() {
			pending_node_ids.extend(node_tree.get_child_ids(node_id).iter().copied());
			let component_count = match node_tree.get_node(node_id) {
				Some(node) => {
					node.get_all_components().len()
				}
				None => {
					continue;
				}
			};
			for component_index in 0..component_count {
				let component: &mut dyn Component = node_tree[node_id].components[component_index].as_mut();
				if !has_mutable_getters(component) {
					continue;
				}
				let property_list = UIDocument::get_reflected_properties(component);
				let mut value_list: Vec<(&'static str, ReflectValue)> = Vec::new();
				for property in property_list.iter() {
					if let Some(value) = read_reflected_property_mut(component, property.name) {
						value_list.push((property.name, value));
					}
				}
				prefetched_value_map.insert((node_id, component_index), value_list);
			}
		}
		prefetched_value_map
	}

	//==============================================================================
	/// 노드 트리 → 문서 데이터. (재귀, 노드 참조는 root_id 기준 경로)
	//==============================================================================
	fn serialize_node_from_root(&self, node_tree: &NodeTree, node_id: NodeId, root_id: NodeId, prefetched_value_map: Option<&PrefetchedValueMap>) -> Value {
		let node = match node_tree.get_node(node_id) {
			Some(node) => {
				node
			}
			None => {
				return Value::Null;
			}
		};
		let node_save_context = UIDocumentSaveContext {
			node_tree,
			root_id,
			node_id,
			component_index: 0,
			prefetched_value_map,
		};
		let component_list = node.get_all_components();

		// 노드 종류. (그 종류로 등록한 컴포넌트가 붙은 노드, 앞에 붙은 것. 원본 constructor.name 자리)
		let node_type_entry = component_list.iter().enumerate().find_map(|(component_index, component)| {
			self.find_node_type(component.as_ref()).map(|node_type| (component_index, node_type))
		});
		let (node_type_name, is_document_active) = match node_type_entry {
			Some((component_index, node_type)) => {
				(node_type.type_name.as_str(), (node_type.read_document_active)(component_list[component_index].as_ref(), node.is_active(), node.is_visible()))
			}
			None => {
				(NODE_TYPE_NAME, node.is_active())
			}
		};
		let mut node_data = Map::new();
		write_value(&mut node_data, "type", Value::from(node_type_name));
		write_value(&mut node_data, "name", Value::from(node.get_name()));
		write_value(&mut node_data, "active", Value::from(is_document_active));
		write_value(&mut node_data, "position", UIDocument::vector2_to_array(node.get_local_position()));
		write_value(&mut node_data, "scale", UIDocument::vector2_to_array(node.get_local_scale()));
		write_value(&mut node_data, "rotation", UIDocument::number_to_value(node.get_local_rotation()));
		write_value(&mut node_data, "opacity", UIDocument::number_to_value(node.get_local_opacity()));
		write_value(&mut node_data, "pivot", UIDocument::vector2_to_array(node.get_pivot()));
		write_value(&mut node_data, "anchor", UIDocument::vector2_to_array(node.get_anchor()));
		write_value(&mut node_data, "contentSize", UIDocument::vector2_to_array(node.get_content_size()));
		write_value(&mut node_data, "interactable", Value::from(node.is_interactable()));

		// 노드 종류 컴포넌트가 스스로 그리는 원본 생성자 몫은 컴포넌트와 자식 목록 앞에 둔다. (원본 생성자는 사용자가 붙이는 것보다 먼저 만든다)
		// 원본 생성자처럼 자식과 컴포넌트를 실제로 짓는 종류는 그것들이 그대로 적힌다.
		let mut component_data_list: Vec<Value> = Vec::new();
		let mut child_data_list: Vec<Value> = Vec::new();
		let mut widget_content_id_set: HashSet<NodeId> = HashSet::new();
		let mut node_type_component_index: Option<usize> = None;
		if let Some((component_index, node_type)) = node_type_entry {
			let node_component: &dyn Component = component_list[component_index].as_ref();
			let node_parts = (node_type.save_node_parts)(node_component, &node_save_context.with_component_index(component_index), node_id);
			component_data_list.extend(node_parts.component_data_list.into_iter().map(Value::Object));
			child_data_list.extend(node_parts.child_data_list);
			if let Some(content_id) = (node_type.read_content)(node_component) {
				widget_content_id_set.insert(content_id);
			}
			node_type_component_index = Some(component_index);
		}

		// 위젯이 스스로 붙인 컴포넌트는 적지 않는다. (읽을 때 다시 붙는다)
		let generated_type_set = self.collect_generated_component_types(component_list);
		for (component_index, component) in component_list.iter().enumerate() {
			if node_type_component_index == Some(component_index) {
				continue;
			}
			let component_reference: &dyn Component = component.as_ref();
			let save_context = node_save_context.with_component_index(component_index);
			let (type_name, mut component_data) = match self.find_component_type(component_reference) {
				Some(component_type) => {
					if let Some(read_content) = &component_type.read_content {
						if let Some(content_id) = read_content(component_reference) {
							widget_content_id_set.insert(content_id);
						}
					}
					if generated_type_set.contains(&component_type.type_name) {
						continue;
					}
					let component_data = match &component_type.save_component {
						Some(save_component) => {
							// 쓰기 함수는 노드 참조를 다루지 않으므로 원본처럼 따로 얹는다.
							let mut component_data = save_component(component_reference, &save_context);
							UIDocument::save_node_references(component_reference, &mut component_data, &save_context);
							component_data
						}
						None => {
							UIDocument::save_by_reflection(component_reference, &save_context)
						}
					};
					(component_type.type_name.as_str(), component_data)
				}
				None => {
					// 등록하지 않은 종류도 원본처럼 반사로 적는다. (읽을 때는 등록한 종류만 짓는다)
					match component_reference.get_reflection() {
						Some(reflection) => {
							let reflected_type_name = reflection.get_reflected_type_name();
							if generated_type_set.contains(reflected_type_name) {
								continue;
							}
							(reflected_type_name, UIDocument::save_by_reflection(component_reference, &save_context))
						}
						None => {
							eprintln!("UIDocument: 반사도 등록도 없는 컴포넌트라 적지 않습니다 ({}, 노드 {}). impl 블록과 impl Component 블록에 #[vanilla::reflect] 를 붙이거나 register_component_type_with_handler 로 등록합니다", component_reference.get_component_type(), node.get_name());
							continue;
						}
					}
				}
			};
			write_value(&mut component_data, "type", Value::from(type_name));
			component_data_list.push(Value::Object(component_data));
			if let Some(save_companions) = self.find_component_type(component_reference).and_then(|component_type| component_type.save_companions.as_ref()) {
				component_data_list.extend(save_companions(component_reference, &save_context).into_iter().map(Value::Object));
			}
		}
		write_value(&mut node_data, "components", Value::Array(component_data_list));

		// 위젯의 content 노드는 읽을 때 위젯이 다시 만든다. 그 안의 자식만 남긴다.
		let mut content_child_data_list: Option<Vec<Value>> = None;
		for child_id in node.get_child_ids().iter() {
			if widget_content_id_set.contains(child_id) {
				let grand_child_ids = node_tree.get_child_ids(*child_id);
				if !grand_child_ids.is_empty() {
					content_child_data_list = Some(grand_child_ids.iter().map(|grand_child_id| self.serialize_node_from_root(node_tree, *grand_child_id, root_id, prefetched_value_map)).collect());
				}
				continue;
			}
			child_data_list.push(self.serialize_node_from_root(node_tree, *child_id, root_id, prefetched_value_map));
		}
		write_value(&mut node_data, "children", Value::Array(child_data_list));
		if let Some(content_child_data_list) = content_child_data_list {
			write_value(&mut node_data, "contentChildren", Value::Array(content_child_data_list));
		}
		Value::Object(node_data)
	}

	//==============================================================================
	/// 컴포넌트 하나 → 문서 데이터. ("type" 은 더하지 않는다. 등록한 종류면 그 꼴로, 등록하지 않았어도 반사가 있으면 반사로. 둘 다 없으면 경고 없이 None)
	/// - 노드 참조는 root_id 기준 경로로 적는다.
	/// # 인자
	/// - `node_tree`: 노드 참조를 찾을 트리.
	/// - `root_id`: 노드 참조 경로의 기준 뿌리.
	/// - `component`: 쓸 컴포넌트.
	///
	/// # 돌려줌
	/// 컴포넌트 문서 값. ("type" 없음) 등록하지 않았고 반사도 없으면 None.
	///
	/// # 예제
	/// Paint 하나의 문서 값
	/// ```rust
	/// use vanilla::{NodeTree, Paint, UIDocument};
	///
	/// let node_tree = NodeTree::new();
	/// let mut paint = Paint::new();
	/// paint.set_round_size(8.0);
	/// let document = UIDocument::new();
	/// let paint_data = document.serialize_component(&node_tree, node_tree.get_root_id(), &paint).expect("paint data");
	/// assert_eq!(paint_data["roundSize"], 8);
	/// assert!(paint_data.get("type").is_none());
	/// ```
	//==============================================================================
	pub fn serialize_component(&self, node_tree: &NodeTree, root_id: NodeId, component: &dyn Component) -> Option<Map<String, Value>> {
		let save_context = UIDocumentSaveContext {
			node_tree,
			root_id,
			node_id: root_id,
			component_index: 0,
			prefetched_value_map: None,
		};
		match self.find_component_type(component) {
			Some(component_type) => {
				match &component_type.save_component {
					Some(save_component) => {
						let mut component_data = save_component(component, &save_context);
						UIDocument::save_node_references(component, &mut component_data, &save_context);
						Some(component_data)
					}
					None => {
						Some(UIDocument::save_by_reflection(component, &save_context))
					}
				}
			}
			None => {
				component.get_reflection()?;
				Some(UIDocument::save_by_reflection(component, &save_context))
			}
		}
	}

	//==============================================================================
	/// 컴포넌트의 편집할 수 있는 속성 목록. (원본 getReflectedProperties, 짝이 맞는 get 또는 is 와 set. 다른 객체를 가리키는 이름은 뺀다)
	/// # 인자
	/// - `component`: 속성을 볼 컴포넌트.
	///
	/// # 돌려줌
	/// 반사 속성 목록. (반사 매크로가 지은 차례 뒤에 Component 공통 속성 GizmoVisible, Enable, ComponentType. 반사가 없는 컴포넌트는 공통 속성만)
	//==============================================================================
	pub fn get_reflected_properties(component: &dyn Component) -> Vec<ReflectedProperty> {
		collect_reflected_properties(component).into_iter().filter(|property| !REFLECTED_PROPERTY_EXCLUDE_NAMES.contains(&property.name)).collect()
	}

	//==============================================================================
	/// 짝이 맞는 get / set 속성으로 컴포넌트 저장. (원본 saveByReflection, 옮길 수 있는 값만 담는다: 수, 불, 글, Color, Vector2, 노드)
	/// # 인자
	/// - `component`: 쓸 컴포넌트.
	/// - `save_context`: 쓰기 문맥. (노드 참조 경로의 기준)
	///
	/// # 돌려줌
	/// 속성 이름(머리 대문자) → 문서 값. ("type" 없음, 저장하지 않는 속성과 옮길 수 없는 값은 뺀다)
	//==============================================================================
	pub fn save_by_reflection(component: &dyn Component, save_context: &UIDocumentSaveContext) -> Map<String, Value> {
		let mut component_data = Map::new();
		for property in UIDocument::get_reflected_properties(component) {
			if !property.is_saved {
				continue;
			}
			let current_value = match save_context.read_property(component, property.name) {
				Some(current_value) => {
					current_value
				}
				None => {
					continue;
				}
			};
			match current_value {
				ReflectValue::Boolean(boolean_value) => {
					write_value(&mut component_data, property.name, Value::from(boolean_value));
				}
				ReflectValue::Number(number) => {
					write_value(&mut component_data, property.name, UIDocument::double_to_value(number));
				}
				ReflectValue::String(text) => {
					write_value(&mut component_data, property.name, Value::from(text));
				}
				ReflectValue::Color(color) => {
					write_value(&mut component_data, property.name, UIDocument::color_to_array(color));
				}
				ReflectValue::Vector2(vector) => {
					write_value(&mut component_data, property.name, UIDocument::vector2_to_array(vector));
				}
				ReflectValue::Node(node_id) => {
					save_context.write_node_reference(&mut component_data, property.name, Some(node_id));
				}
				_ => {
				}
			}
		}
		component_data
	}

	//==============================================================================
	/// 짝이 맞는 get / set 속성으로 컴포넌트 복원. (원본 loadByReflection, 노드 참조는 트리를 다 지은 뒤 잇는다)
	/// - 원본은 지금 값의 꼴로 받을지 가린다. 러스트 판은 세터의 꼴로 가린다. (core/reflection.rs)
	/// - 트리를 받는 세터(set_snap_current_index(트리, 칸) 같은 것)도 읽기 문맥의 트리로 원본처럼 그 자리에서 부른다.
	/// # 인자
	/// - `component`: 값을 넣을 컴포넌트.
	/// - `component_data`: 컴포넌트 문서 값. (속성 이름 키)
	/// - `load_context`: 읽기 문맥. (노드 참조는 여기에 맡긴다)
	//==============================================================================
	pub fn load_by_reflection(component: &mut dyn Component, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext) {
		for property in UIDocument::get_reflected_properties(component) {
			if !property.is_saved {
				continue;
			}
			let stored_value = match component_data.get(property.name) {
				Some(stored_value) => {
					stored_value
				}
				None => {
					continue;
				}
			};
			if let Some(reference_path) = read_node_reference_path(stored_value) {
				load_context.bind_reflected_node_reference(property.name, reference_path);
				continue;
			}
			if let Some(value) = json_to_reflect_value(stored_value) {
				write_reflected_property_in_tree(component, load_context.node_tree, property.name, &value);
			}
		}
	}

	//==============================================================================
	/// 노드를 가리키는 반사 속성만 따로 적기. (원본 saveNodeReferences, 쓰기 함수가 있는 종류에 쓴다)
	//==============================================================================
	fn save_node_references(component: &dyn Component, component_data: &mut Map<String, Value>, save_context: &UIDocumentSaveContext) {
		for property in UIDocument::get_reflected_properties(component) {
			if let Some(ReflectValue::Node(node_id)) = save_context.read_property(component, property.name) {
				save_context.write_node_reference(component_data, property.name, Some(node_id));
			}
		}
	}

	//==============================================================================
	/// 노드를 가리키는 값만 따로 잇기. (원본 loadNodeReferences, 쓰기 함수가 있는 종류에 쓴다. 그 키의 반사 세터가 없으면 잇지 않는다)
	//==============================================================================
	fn load_node_references(component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext) {
		for (property_name, stored_value) in component_data.iter() {
			if let Some(reference_path) = read_node_reference_path(stored_value) {
				load_context.bind_reflected_node_reference(property_name, reference_path);
			}
		}
	}

	//==============================================================================
	/// 위젯이 붙인 컴포넌트 종류 이름 모으기. (원본 collectGeneratedComponentTypes, 원본 require 자리는 등록할 때 준 목록)
	/// # 인자
	/// - `component_list`: 노드의 컴포넌트 목록.
	///
	/// # 돌려줌
	/// 목록 안의 등록한 위젯들이 스스로 붙이는 컴포넌트 종류 이름 모음.
	//==============================================================================
	pub fn collect_generated_component_types(&self, component_list: &[Box<dyn Component>]) -> HashSet<String> {
		let mut generated_type_set: HashSet<String> = HashSet::new();
		for component in component_list.iter() {
			if let Some(component_type) = self.find_component_type(component.as_ref()) {
				for generated_type_name in component_type.generated_type_names.iter() {
					generated_type_set.insert(generated_type_name.clone());
				}
			}
		}
		generated_type_set
	}

	//==============================================================================
	/// 문서 데이터 → 노드 트리. (원본 deserializeNode, 어느 부모에도 붙지 않은 노드를 돌려준다)
	/// - 노드 참조는 이 노드 기준 경로로 잇고, 찾을 수 있는 그림을 넣는다.
	/// # 인자
	/// - `node_tree`: 노드를 지을 트리.
	/// - `node_data`: 노드 문서 값. (serialize_node 꼴, 객체가 아니면 빈 노드 하나를 짓는다)
	///
	/// # 돌려줌
	/// 지은 노드. (어느 부모에도 붙지 않았다)
	//==============================================================================
	pub fn deserialize_node(&mut self, node_tree: &mut NodeTree, node_data: &Value) -> NodeId {
		let mut pending_node_reference_list: Vec<PendingNodeReference> = Vec::new();
		let root_id = self.build_node(node_tree, node_data, &mut pending_node_reference_list);

		// 트리를 다 지은 뒤에 노드 참조를 잇는다.
		for pending_node_reference in pending_node_reference_list {
			let referenced_id = match UIDocument::find_node_by_path(node_tree, root_id, &pending_node_reference.path) {
				Some(referenced_id) => {
					referenced_id
				}
				None => {
					continue;
				}
			};
			if let Some(component) = node_tree.get_node_mut(pending_node_reference.node_id).and_then(|node| node.components.get_mut(pending_node_reference.component_index)) {
				(pending_node_reference.apply_node)(component.as_mut(), referenced_id);
			}
		}
		self.bind_pending_images(node_tree);
		root_id
	}

	//==============================================================================
	/// 노드 하나와 그 자손 짓기. (재귀)
	//==============================================================================
	fn build_node(&mut self, node_tree: &mut NodeTree, node_data: &Value, pending_node_reference_list: &mut Vec<PendingNodeReference>) -> NodeId {
		let empty_node_data = Map::new();
		let node_object = node_data.as_object().unwrap_or(&empty_node_data);
		let node_id = node_tree.create_node(read_string(node_object, "name").unwrap_or(""));

		// 노드 종류. (원본은 그 종류의 생성자로 짓고 나서 문서의 노드 값을 넣는다. 등록하지 않은 종류는 WorldNode 다)
		let node_type_name = read_string(node_object, "type").unwrap_or(NODE_TYPE_NAME);
		let node_type_entry = match self.node_type_list.iter().position(|registered_type| registered_type.type_name == node_type_name) {
			Some(node_type_index) => {
				(self.node_type_list[node_type_index].add_component)(node_tree, node_id).map(|component_index| (node_type_index, component_index))
			}
			None => {
				if node_type_name != NODE_TYPE_NAME {
					eprintln!("UIDocument: 등록되지 않은 노드 종류를 WorldNode 로 읽습니다 ({}). 에디터가 저장한 문서는 register_editor_types 를 먼저 부릅니다", node_type_name);
				}
				None
			}
		};
		self.fill_node(node_tree, node_id, node_object, node_type_entry, pending_node_reference_list);
		node_id
	}

	//==============================================================================
	/// 문서 값을 노드 하나에 넣기. (재귀)
	/// - 노드에 이미 있는 컴포넌트와 자식(노드 종류 컴포넌트나 위젯이 붙을 때 지은 것, 원본 노드 생성자가 만든 몫)은 문서의 앞에서부터
	///   차례와 종류(자식은 이름)가 맞으면 그것에 문서 값을 넣고 다시 짓지 않는다. 맞지 않는 데부터는 새로 짓는다.
	///   (원본 fillNode 와 같다)
	/// - require 로 함께 붙은 컴포넌트도 다음 문서 값을 받을 자리다. (원본도 같다)
	//==============================================================================
	fn fill_node(&mut self, node_tree: &mut NodeTree, node_id: NodeId, node_object: &Map<String, Value>, node_type_entry: Option<(usize, usize)>, pending_node_reference_list: &mut Vec<PendingNodeReference>) {
		{
			let node = &mut node_tree[node_id];
			if let Some(is_active) = read_bool(node_object, "active") {
				node.set_active(is_active);
			}
			if let Some(local_position) = read_vector2(node_object, "position") {
				node.set_local_position(local_position);
			}
			if let Some(local_scale) = read_vector2(node_object, "scale") {
				node.set_local_scale(local_scale);
			}
			if let Some(local_rotation) = read_number(node_object, "rotation") {
				node.set_local_rotation(local_rotation);
			}
			if let Some(local_opacity) = read_number(node_object, "opacity") {
				node.set_local_opacity(local_opacity);
			}
			if let Some(pivot) = read_vector2(node_object, "pivot") {
				node.set_pivot(pivot);
			}
			if let Some(anchor) = read_vector2(node_object, "anchor") {
				node.set_anchor(anchor);
			}
			if let Some(content_size) = read_vector2(node_object, "contentSize") {
				node.set_content_size(content_size);
			}
			if let Some(is_interactable) = read_bool(node_object, "interactable") {
				node.set_interactable(is_interactable);
			}
		}
		let component_data_list: &[Value] = node_object.get("components").and_then(|value| value.as_array()).map(|values| values.as_slice()).unwrap_or(&[]);
		let child_data_list: &[Value] = node_object.get("children").and_then(|value| value.as_array()).map(|values| values.as_slice()).unwrap_or(&[]);
		let mut component_data_index = 0;
		let mut child_data_index = 0;
		let mut widget_content_id_set: HashSet<NodeId> = HashSet::new();
		let mut widget_content_id: Option<NodeId> = None;

		// 노드 종류 컴포넌트가 스스로 그리는 원본 생성자 몫. (UIInputField, UIScrollBar) 원본 active 는 그 종류의 노드 상태로 바꿔 넣는다.
		let node_type_component_index = node_type_entry.map(|(_, component_index)| component_index);
		if let Some((node_type_index, component_index)) = node_type_entry {
			let node_type = &self.node_type_list[node_type_index];
			if let Some(is_active) = read_bool(node_object, "active") {
				let (is_node_active, node_visible) = (node_type.convert_document_active)(node_tree[node_id].components[component_index].as_ref(), is_active);
				let node = &mut node_tree[node_id];
				node.set_active(is_node_active);
				if let Some(is_node_visible) = node_visible {
					node.set_visible(is_node_visible);
				}
			}
			let component: &mut dyn Component = node_tree[node_id].components[component_index].as_mut();
			(component_data_index, child_data_index) = (node_type.load_node_parts)(component, component_data_list, child_data_list);
			if let Some(content_id) = (node_type.read_content)(node_tree[node_id].components[component_index].as_ref()) {
				widget_content_id_set.insert(content_id);
				widget_content_id = Some(content_id);
			}
		}

		// 이미 있는 컴포넌트와 자식. (위젯이 스스로 붙인 컴포넌트와 content 노드는 문서에 적히지 않는다)
		let component_count = node_tree[node_id].get_all_components().len();
		let generated_type_set = self.collect_generated_component_types(node_tree[node_id].get_all_components());
		let mut existing_component_indices: VecDeque<usize> = (0..component_count).filter(|component_index| {
			if Some(*component_index) == node_type_component_index {
				return false;
			}
			match self.find_component_type(node_tree[node_id].components[*component_index].as_ref()) {
				Some(component_type) => {
					!generated_type_set.contains(&component_type.type_name)
				}
				None => {
					true
				}
			}
		}).collect();
		for component_index in existing_component_indices.iter() {
			if let Some(content_id) = self.read_widget_content(node_tree[node_id].components[*component_index].as_ref()) {
				widget_content_id_set.insert(content_id);
				widget_content_id = Some(content_id);
			}
		}
		let existing_child_ids: Vec<NodeId> = node_tree.get_child_ids(node_id).iter().copied().filter(|child_id| !widget_content_id_set.contains(child_id)).collect();

		// 컴포넌트. (이미 있는 것 가운데 맨 앞이 같은 종류면 그것에 넣고, 아니면 붙인 다음 문서 값을 넣는다)
		while component_data_index < component_data_list.len() {
			let component_data = &component_data_list[component_data_index];
			component_data_index += 1;
			let component_object = match component_data.as_object() {
				Some(component_object) => {
					component_object
				}
				None => {
					continue;
				}
			};
			let type_name = read_string(component_object, "type").unwrap_or("");
			let component_type_index = match self.component_type_list.iter().position(|registered_type| registered_type.type_name == type_name) {
				Some(component_type_index) => {
					component_type_index
				}
				None => {
					eprintln!("UIDocument: 등록되지 않은 컴포넌트 종류를 건너뜁니다 ({})", type_name);
					continue;
				}
			};
			let is_existing_match = match existing_component_indices.front() {
				Some(front_index) => {
					self.find_component_type(node_tree[node_id].components[*front_index].as_ref()).map(|existing_type| existing_type.type_name == type_name).unwrap_or(false)
				}
				None => {
					false
				}
			};
			let component_index = if is_existing_match {
				match existing_component_indices.pop_front() {
					Some(component_index) => {
						component_index
					}
					None => {
						continue;
					}
				}
			}
			else {
				let previous_component_count = node_tree[node_id].get_all_components().len();
				let component_index = match (self.component_type_list[component_type_index].add_component)(node_tree, node_id) {
					Some(component_index) => {
						component_index
					}
					None => {
						continue;
					}
				};
				// 붙으면서 함께 붙인 컴포넌트. (위젯이 스스로 붙여 문서에 적히지 않는 것은 뺀다)
				let generated_type_set = self.collect_generated_component_types(node_tree[node_id].get_all_components());
				let current_component_count = node_tree[node_id].get_all_components().len();
				for required_index in previous_component_count..current_component_count {
					if required_index == component_index {
						continue;
					}
					let is_generated = self.find_component_type(node_tree[node_id].components[required_index].as_ref()).map(|required_type| generated_type_set.contains(&required_type.type_name)).unwrap_or(false);
					if !is_generated {
						existing_component_indices.push_back(required_index);
					}
				}
				component_index
			};
			component_data_index += self.load_component(node_tree, node_id, component_index, component_type_index, component_object, &component_data_list[component_data_index..], pending_node_reference_list);
			if let Some(content_id) = self.read_widget_content(node_tree[node_id].components[component_index].as_ref()) {
				widget_content_id = Some(content_id);
			}
		}

		// 자식.
		self.fill_children(node_tree, node_id, &child_data_list[child_data_index.min(child_data_list.len())..], &existing_child_ids, pending_node_reference_list);

		// 위젯 content 안의 자식. (스크롤 뷰의 목록 항목 같은 것)
		if let Some(content_id) = widget_content_id {
			if let Some(content_child_data_list) = node_object.get("contentChildren").and_then(|value| value.as_array()) {
				let existing_content_child_ids: Vec<NodeId> = node_tree.get_child_ids(content_id).to_vec();
				self.fill_children(node_tree, content_id, content_child_data_list, &existing_content_child_ids, pending_node_reference_list);
			}
		}
	}

	//==============================================================================
	/// 자식 문서 값 넣기. (이미 있는 자식은 차례와 이름이 맞으면 그것에 넣고, 아니면 지어 붙인다)
	//==============================================================================
	fn fill_children(&mut self, node_tree: &mut NodeTree, parent_id: NodeId, child_data_list: &[Value], existing_child_ids: &[NodeId], pending_node_reference_list: &mut Vec<PendingNodeReference>) {
		let empty_node_data = Map::new();
		let mut existing_child_index = 0;
		for child_data in child_data_list.iter() {
			let child_object = child_data.as_object().unwrap_or(&empty_node_data);
			let child_name = read_string(child_object, "name").unwrap_or("");
			let existing_child_id = existing_child_ids.get(existing_child_index).copied().filter(|existing_child_id| {
				node_tree.get_node(*existing_child_id).map(|existing_child| existing_child.get_name() == child_name).unwrap_or(false)
			});
			match existing_child_id {
				Some(existing_child_id) => {
					existing_child_index += 1;
					self.fill_node(node_tree, existing_child_id, child_object, None, pending_node_reference_list);
				}
				None => {
					let child_id = self.build_node(node_tree, child_data, pending_node_reference_list);
					node_tree.add_child(parent_id, child_id);
				}
			}
		}
	}

	//==============================================================================
	/// 컴포넌트 하나에 문서 값 넣기. (컴포넌트를 트리에서 잠깐 꺼내 읽기 문맥에 트리를 쥐여 준다. 트리를 받는 반사 세터를 그 자리에서 부를 수 있다)
	/// - 품은 몫(UIImageView 의 Sprite)이 뒤따르는 문서 값이면 그것도 읽고, 읽은 수를 돌려준다.
	//==============================================================================
	fn load_component(&mut self, node_tree: &mut NodeTree, node_id: NodeId, component_index: usize, component_type_index: usize, component_object: &Map<String, Value>, following_component_data_list: &[Value], pending_node_reference_list: &mut Vec<PendingNodeReference>) -> usize {
		let component_type = &self.component_type_list[component_type_index];
		let pending_image_list = &mut self.pending_image_list;
		let companion_count = node_tree.with_component_at_mut(node_id, component_index, |component, tree| {
			let mut load_context = UIDocumentLoadContext {
				node_id,
				component_index,
				node_tree: tree,
				pending_image_list,
				pending_node_reference_list,
			};
			match &component_type.load_component {
				Some(load_component) => {
					load_component(component, component_object, &mut load_context);
					UIDocument::load_node_references(component_object, &mut load_context);
				}
				None => {
					UIDocument::load_by_reflection(component, component_object, &mut load_context);
				}
			}
			match &component_type.load_companions {
				Some(load_companions) => {
					load_companions(component, following_component_data_list, &mut load_context)
				}
				None => {
					0
				}
			}
		});
		companion_count.unwrap_or(0)
	}

	//==============================================================================
	/// 등록한 위젯 컴포넌트의 content 노드.
	//==============================================================================
	fn read_widget_content(&self, component: &dyn Component) -> Option<NodeId> {
		let read_content = self.find_component_type(component)?.read_content.as_ref()?;
		read_content(component)
	}

	//==============================================================================
	/// 노드 트리 → JSON 글. (원본 toJsonText, is_pretty 면 탭으로 들여 쓴다)
	/// # 인자
	/// - `node_tree`: 노드가 든 트리.
	/// - `root_id`: 문서의 뿌리 노드. (노드 참조 경로의 기준)
	/// - `is_pretty`: true 면 탭으로 들여 쓴 여러 줄, false 면 한 줄.
	///
	/// # 돌려줌
	/// 문서 글. ({ "version": 1, "root": 노드 })
	//==============================================================================
	pub fn to_json_text(&self, node_tree: &NodeTree, root_id: NodeId, is_pretty: bool) -> String {
		UIDocument::write_document_text(self.serialize_node_from_root(node_tree, root_id, root_id, None), is_pretty)
	}

	//==============================================================================
	/// 노드 트리 → JSON 글. (&mut self 게터가 있는 컴포넌트는 &mut 갈래로 읽는다)
	/// # 인자
	/// - `node_tree`: 노드가 든 트리. (&mut 게터를 부르려고 가변으로 받는다)
	/// - `root_id`: 문서의 뿌리 노드.
	/// - `is_pretty`: true 면 탭으로 들여 쓴 여러 줄, false 면 한 줄.
	///
	/// # 돌려줌
	/// 문서 글.
	//==============================================================================
	pub fn to_json_text_mut(&self, node_tree: &mut NodeTree, root_id: NodeId, is_pretty: bool) -> String {
		let prefetched_value_map = UIDocument::prefetch_mutable_values(node_tree, root_id);
		UIDocument::write_document_text(self.serialize_node_from_root(node_tree, root_id, root_id, Some(&prefetched_value_map)), is_pretty)
	}

	//==============================================================================
	/// 문서 글 쓰기. ({ "version": 1, "root": 뿌리 })
	//==============================================================================
	fn write_document_text(root_data: Value, is_pretty: bool) -> String {
		let mut document_data = Map::new();
		write_value(&mut document_data, "version", Value::from(UIDOCUMENT_VERSION));
		write_value(&mut document_data, "root", root_data);
		let document_value = Value::Object(document_data);
		if is_pretty {
			let pretty_text = serde_json::to_string_pretty(&document_value).unwrap_or_default();
			indent_with_tabs(&pretty_text)
		}
		else {
			serde_json::to_string(&document_value).unwrap_or_default()
		}
	}

	//==============================================================================
	/// JSON 글 → 노드 트리. (원본 fromJsonText, 어느 부모에도 붙지 않은 뿌리를 돌려준다. 씬에 보이려면 add_child 로 붙인다)
	/// # 인자
	/// - `node_tree`: 노드를 지을 트리.
	/// - `json_text`: 문서 글.
	///
	/// # 돌려줌
	/// 지은 뿌리 노드. JSON 이 아니거나 "root" 객체가 없으면 Err(까닭 글).
	///
	/// # 예제
	/// 잘못된 문서는 Err
	/// ```rust
	/// use vanilla::{NodeTree, UIDocument};
	///
	/// let mut node_tree = NodeTree::new();
	/// let mut document = UIDocument::new();
	/// assert!(document.from_json_text(&mut node_tree, "{\"version\":1}").is_err());
	/// assert!(document.from_json_text(&mut node_tree, "not json").is_err());
	/// let root_id = document.from_json_text(&mut node_tree, r#"{"version":1,"root":{"name":"panel"}}"#).expect("document");
	/// assert_eq!(node_tree[root_id].get_name(), "panel");
	/// ```
	//==============================================================================
	pub fn from_json_text(&mut self, node_tree: &mut NodeTree, json_text: &str) -> Result<NodeId, String> {
		let document_data: Value = serde_json::from_str(json_text).map_err(|error| format!("UIDocument: {}", error))?;
		let root_data = match document_data.get("root") {
			Some(root_data) if root_data.is_object() => {
				root_data
			}
			_ => {
				return Err(String::from("UIDocument: invalid document."));
			}
		};
		Ok(self.deserialize_node(node_tree, root_data))
	}

	//==============================================================================
	/// 뿌리 기준 절대 경로 계산. (원본 findNodePath, "/" + 이름들. 뿌리 밖의 노드면 None)
	/// # 인자
	/// - `node_tree`: 노드가 든 트리.
	/// - `root_id`: 경로의 기준 뿌리.
	/// - `node_id`: 경로를 셀 노드.
	///
	/// # 돌려줌
	/// "/" 로 시작하는 이름 경로. (뿌리 자신은 "/") 뿌리의 자손이 아니면 None.
	///
	/// # 예제
	/// 경로 세기와 경로로 찾기
	/// ```rust
	/// use vanilla::{NodeTree, UIDocument};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.create_node("root");
	/// let panel_id = node_tree.create_child(root_id, "panel");
	/// let button_id = node_tree.create_child(panel_id, "button");
	/// let outside_id = node_tree.create_node("outside");
	///
	/// assert_eq!(UIDocument::find_node_path(&node_tree, root_id, button_id).as_deref(), Some("/panel/button"));
	/// assert_eq!(UIDocument::find_node_path(&node_tree, root_id, root_id).as_deref(), Some("/"));
	/// assert_eq!(UIDocument::find_node_path(&node_tree, root_id, outside_id), None);
	/// assert_eq!(UIDocument::find_node_by_path(&node_tree, root_id, "/panel/button"), Some(button_id));
	/// assert_eq!(UIDocument::find_node_by_path(&node_tree, root_id, "/panel/missing"), None);
	/// ```
	//==============================================================================
	pub fn find_node_path(node_tree: &NodeTree, root_id: NodeId, node_id: NodeId) -> Option<String> {
		let mut name_list: Vec<&str> = Vec::new();
		let mut current_id = node_id;
		while current_id != root_id {
			let current_node = node_tree.get_node(current_id)?;
			name_list.push(current_node.get_name());
			current_id = current_node.get_parent_id()?;
		}
		name_list.reverse();
		Some(format!("/{}", name_list.join("/")))
	}

	//==============================================================================
	/// 뿌리 기준 절대 경로로 노드 찾기. (원본 findNodeByPath, 같은 이름이면 앞의 자식)
	/// # 인자
	/// - `node_tree`: 노드가 든 트리.
	/// - `root_id`: 경로의 기준 뿌리.
	/// - `path_text`: "/이름/이름" 꼴 경로. (앞의 "/" 는 없어도 된다. 빈 글이나 "/" 면 뿌리)
	///
	/// # 돌려줌
	/// 찾은 노드. 이름이 맞는 자식이 없는 조각이 있으면 None.
	//==============================================================================
	pub fn find_node_by_path(node_tree: &NodeTree, root_id: NodeId, path_text: &str) -> Option<NodeId> {
		let trimmed_path = path_text.strip_prefix('/').unwrap_or(path_text);
		if trimmed_path.is_empty() {
			return Some(root_id);
		}
		let mut current_id = root_id;
		for node_name in trimmed_path.split('/') {
			current_id = node_tree.find_child_by_name(current_id, node_name)?;
		}
		Some(current_id)
	}

	//==============================================================================
	/// 수 → 문서 값. (f32 를 가장 짧은 십진으로, 정수면 소수점 없이. 원본 JSON.stringify 와 같은 글이 나온다. 수가 아니면 null)
	/// # 인자
	/// - `number`: 적을 수.
	///
	/// # 돌려줌
	/// JSON 수. (NaN, 무한이면 null)
	///
	/// # 예제
	/// 가장 짧은 십진
	/// ```rust
	/// use serde_json::Value;
	/// use vanilla::UIDocument;
	///
	/// assert_eq!(UIDocument::number_to_value(0.22).to_string(), "0.22");
	/// assert_eq!(UIDocument::number_to_value(24.0).to_string(), "24");
	/// assert_eq!(UIDocument::number_to_value(f32::NAN), Value::Null);
	/// ```
	//==============================================================================
	pub fn number_to_value(number: f32) -> Value {
		if !number.is_finite() {
			return Value::Null;
		}
		UIDocument::double_to_value(read_shortest_number(number))
	}

	//==============================================================================
	/// f64 수 → 문서 값. (정수면 소수점 없이, 수가 아니면 null. 원본 JSON.stringify 와 같다)
	//==============================================================================
	fn double_to_value(number: f64) -> Value {
		if !number.is_finite() {
			return Value::Null;
		}
		if number.fract() == 0.0 && number.abs() < MAXIMUM_SAFE_INTEGER {
			return Value::from(number as i64);
		}
		match serde_json::Number::from_f64(number) {
			Some(json_number) => {
				Value::Number(json_number)
			}
			None => {
				Value::Null
			}
		}
	}

	//==============================================================================
	/// 색 → 배열. (원본 colorToArray, [r, g, b, a])
	/// # 인자
	/// - `color`: 적을 색.
	///
	/// # 돌려줌
	/// [r, g, b, a] 배열. (0 ~ 1 값을 가장 짧은 십진으로)
	//==============================================================================
	pub fn color_to_array(color: Color) -> Value {
		Value::Array(vec![UIDocument::number_to_value(color.red), UIDocument::number_to_value(color.green), UIDocument::number_to_value(color.blue), UIDocument::number_to_value(color.alpha)])
	}

	//==============================================================================
	/// 배열 → 색. (원본 arrayToColor, 원소가 넷보다 적거나 앞 넷 가운데 수가 아닌 것이 있으면 None)
	///
	/// # 인자
	/// - `value`: [r, g, b, a] 꼴 문서 값.
	///
	/// # 돌려줌
	/// 색. 배열이 아니거나 원소가 넷보다 적거나 앞 넷 가운데 수가 아닌 것이 있으면 None. (다섯째부터는 보지 않는다)
	///
	/// # 예제
	/// 색 배열 왕복
	/// ```rust
	/// use vanilla::{Color, UIDocument};
	///
	/// let color = Color::create(1.0, 0.5, 0.0, 1.0);
	/// let color_value = UIDocument::color_to_array(color);
	/// assert_eq!(color_value, serde_json::json!([1, 0.5, 0, 1]));
	/// assert_eq!(UIDocument::array_to_color(&color_value), Some(color));
	/// assert_eq!(UIDocument::array_to_color(&serde_json::json!([1, 0.5])), None);
	/// ```
	//==============================================================================
	pub fn array_to_color(value: &Value) -> Option<Color> {
		let channels = value.as_array()?;
		if channels.len() < 4 {
			return None;
		}
		let red = channels[0].as_f64()? as f32;
		let green = channels[1].as_f64()? as f32;
		let blue = channels[2].as_f64()? as f32;
		let alpha = channels[3].as_f64()? as f32;
		Some(Color::create(red, green, blue, alpha))
	}

	//==============================================================================
	/// 벡터 → 배열. ([x, y])
	/// # 인자
	/// - `vector`: 적을 벡터.
	///
	/// # 돌려줌
	/// [x, y] 배열.
	//==============================================================================
	pub fn vector2_to_array(vector: Vector2) -> Value {
		Value::Array(vec![UIDocument::number_to_value(vector.x), UIDocument::number_to_value(vector.y)])
	}

	//==============================================================================
	/// 배열 → 벡터. (원소가 둘보다 적거나 앞 둘 가운데 수가 아닌 것이 있으면 None)
	/// # 인자
	/// - `value`: [x, y] 꼴 문서 값.
	///
	/// # 돌려줌
	/// 벡터. 배열이 아니거나 원소가 둘보다 적거나 앞 둘 가운데 수가 아닌 것이 있으면 None. (셋째부터는 보지 않는다)
	//==============================================================================
	pub fn array_to_vector2(value: &Value) -> Option<Vector2> {
		let values = value.as_array()?;
		if values.len() < 2 {
			return None;
		}
		let x = values[0].as_f64()? as f32;
		let y = values[1].as_f64()? as f32;
		Some(Vector2::create(x, y))
	}
}

impl Default for UIDocument {
	//==============================================================================
	/// 기본값. (new 와 같다, 엔진 컴포넌트를 등록한 문서)
	//==============================================================================
	fn default() -> UIDocument {
		UIDocument::new()
	}
}

//==============================================================================
/// 문서 값 하나 적기.
//==============================================================================
fn write_value(object_data: &mut Map<String, Value>, key: &str, value: Value) {
	object_data.insert(String::from(key), value);
}

//==============================================================================
/// 수 읽기.
//==============================================================================
fn read_number(object_data: &Map<String, Value>, key: &str) -> Option<f32> {
	object_data.get(key).and_then(|value| value.as_f64()).map(|number| number as f32)
}

//==============================================================================
/// 참거짓 읽기.
//==============================================================================
fn read_bool(object_data: &Map<String, Value>, key: &str) -> Option<bool> {
	object_data.get(key).and_then(|value| value.as_bool())
}

//==============================================================================
/// 글 읽기.
//==============================================================================
fn read_string<'a>(object_data: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
	object_data.get(key).and_then(|value| value.as_str())
}

//==============================================================================
/// 색 읽기.
//==============================================================================
fn read_color(object_data: &Map<String, Value>, key: &str) -> Option<Color> {
	object_data.get(key).and_then(UIDocument::array_to_color)
}

//==============================================================================
/// 벡터 읽기.
//==============================================================================
fn read_vector2(object_data: &Map<String, Value>, key: &str) -> Option<Vector2> {
	object_data.get(key).and_then(UIDocument::array_to_vector2)
}

//==============================================================================
/// 이름표에서 값의 이름 찾기.
//==============================================================================
fn find_name<T: PartialEq + Copy>(name_table: &[(T, &'static str)], value: T) -> &'static str {
	name_table.iter().find(|(table_value, _)| *table_value == value).map(|(_, name)| *name).unwrap_or("")
}

//==============================================================================
/// 이름표에서 이름의 값 찾기.
//==============================================================================
fn find_value<T: Copy>(name_table: &[(T, &'static str)], name: &str) -> Option<T> {
	name_table.iter().find(|(_, table_name)| *table_name == name).map(|(value, _)| *value)
}

//==============================================================================
/// 노드 참조의 경로. ({ "$nodeRef": 경로 } 면 그 경로)
//==============================================================================
fn read_node_reference_path(value: &Value) -> Option<&str> {
	value.as_object()?.get(NODE_REFERENCE_KEY)?.as_str()
}

//==============================================================================
/// 문서 값 → 반사 값. (배열은 List, 노드 참조가 아닌 객체는 옮기지 않는다)
//==============================================================================
fn json_to_reflect_value(value: &Value) -> Option<ReflectValue> {
	match value {
		Value::Null => {
			Some(ReflectValue::Null)
		}
		Value::Bool(boolean_value) => {
			Some(ReflectValue::Boolean(*boolean_value))
		}
		Value::Number(number) => {
			number.as_f64().map(ReflectValue::Number)
		}
		Value::String(text) => {
			Some(ReflectValue::String(text.clone()))
		}
		Value::Array(item_values) => {
			Some(ReflectValue::List(item_values.iter().filter_map(json_to_reflect_value).collect()))
		}
		Value::Object(_) => {
			None
		}
	}
}

//==============================================================================
/// 문서 안의 그림 경로 모으기. (imagePath 키의 빈 글이 아닌 값, 같은 경로는 한 번)
//==============================================================================
fn collect_image_paths(value: &Value, image_path_list: &mut Vec<String>) {
	match value {
		Value::Object(object_data) => {
			for (key, child_value) in object_data.iter() {
				if key == "imagePath" {
					if let Some(image_path) = child_value.as_str() {
						if !image_path.is_empty() && !image_path_list.iter().any(|collected_path| collected_path == image_path) {
							image_path_list.push(String::from(image_path));
						}
					}
					continue;
				}
				collect_image_paths(child_value, image_path_list);
			}
		}
		Value::Array(values) => {
			for child_value in values.iter() {
				collect_image_paths(child_value, image_path_list);
			}
		}
		_ => {
		}
	}
}

//==============================================================================
/// 두 칸 들여쓰기를 탭으로. (serde_json 의 보기 좋은 글은 두 칸이다. JSON 글 안에는 줄바꿈 글자가 없어 줄 머리의 빈칸은 모두 들여쓰기다)
//==============================================================================
fn indent_with_tabs(pretty_text: &str) -> String {
	let mut tab_text = String::with_capacity(pretty_text.len());
	for (line_index, line) in pretty_text.split('\n').enumerate() {
		if line_index > 0 {
			tab_text.push('\n');
		}
		let content = line.trim_start_matches(' ');
		let indent_count = (line.len() - content.len()) / 2;
		for _ in 0..indent_count {
			tab_text.push('\t');
		}
		tab_text.push_str(content);
	}
	tab_text
}

//==============================================================================
/// Paint 쓰기.
//==============================================================================
fn save_paint(paint: &Paint, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "color", UIDocument::color_to_array(paint.get_color()));
	write_value(&mut component_data, "roundSize", UIDocument::number_to_value(paint.get_round_size()));
	component_data
}

//==============================================================================
/// Paint 읽기.
//==============================================================================
fn load_paint(paint: &mut Paint, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(color) = read_color(component_data, "color") {
		paint.set_color(color);
	}
	if let Some(round_size) = read_number(component_data, "roundSize") {
		paint.set_round_size(round_size);
	}
}

//==============================================================================
/// Sprite 쓰기. (그림은 경로만 적는다. nineSlice 는 원본처럼 Rect 의 left, top, right, bottom)
//==============================================================================
fn save_sprite(sprite: &Sprite, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let nine_slice = sprite.get_nine_slice();
	let mut component_data = Map::new();
	write_value(&mut component_data, "color", UIDocument::color_to_array(sprite.get_color()));
	write_value(&mut component_data, "roundSize", UIDocument::number_to_value(sprite.get_round_size()));
	write_value(&mut component_data, "imagePath", Value::from(sprite.get_image_path()));
	write_value(&mut component_data, "nineSlice", Value::Array(vec![
		UIDocument::number_to_value(nine_slice.position.x),
		UIDocument::number_to_value(nine_slice.position.y),
		UIDocument::number_to_value(nine_slice.position.x + nine_slice.size.x),
		UIDocument::number_to_value(nine_slice.position.y + nine_slice.size.y),
	]));
	write_value(&mut component_data, "spriteDrawMode", Value::from(find_name(&SPRITE_DRAW_MODE_NAME_TABLE, sprite.get_sprite_draw_mode())));
	write_value(&mut component_data, "spriteBlendMode", Value::from(sprite.get_sprite_blend_mode().get_name()));
	component_data
}

//==============================================================================
/// Sprite 읽기.
//==============================================================================
fn load_sprite(sprite: &mut Sprite, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext) {
	if let Some(image_path) = load_sprite_fields(sprite, component_data) {
		load_context.bind_image::<Sprite>(&image_path, |sprite, image| sprite.set_image(Some(image)));
	}
}

//==============================================================================
/// Sprite 값 읽기. (그림은 경로만 적어 두고, 빈 글이 아닌 그림 경로를 돌려준다. 그림은 부르는 쪽이 맡긴다)
//==============================================================================
fn load_sprite_fields(sprite: &mut Sprite, component_data: &Map<String, Value>) -> Option<String> {
	if let Some(color) = read_color(component_data, "color") {
		sprite.set_color(color);
	}
	if let Some(round_size) = read_number(component_data, "roundSize") {
		sprite.set_round_size(round_size);
	}
	let mut bound_image_path: Option<String> = None;
	if let Some(image_path) = read_string(component_data, "imagePath") {
		if !image_path.is_empty() {
			sprite.set_image_path(image_path);
			bound_image_path = Some(String::from(image_path));
		}
	}
	if let Some(Value::Array(values)) = component_data.get("nineSlice") {
		let edges: Vec<f32> = values.iter().filter_map(|value| value.as_f64()).map(|value| value as f32).collect();
		if edges.len() == 4 {
			sprite.set_nine_slice(Rect::create(edges[0], edges[1], edges[2] - edges[0], edges[3] - edges[1]));
		}
	}
	if let Some(sprite_draw_mode) = read_string(component_data, "spriteDrawMode").and_then(|name| find_value(&SPRITE_DRAW_MODE_NAME_TABLE, name)) {
		sprite.set_sprite_draw_mode(sprite_draw_mode);
	}
	if let Some(sprite_blend_mode) = read_string(component_data, "spriteBlendMode").and_then(BlendMode::from_name) {
		sprite.set_sprite_blend_mode(sprite_blend_mode);
	}
	bound_image_path
}

//==============================================================================
/// 글 속성 쓰기. (원본 saveTextProperties, Text 와 RichText 가 같이 쓴다)
//==============================================================================
fn save_text_properties(text: &Text) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "text", Value::from(text.get_text()));
	write_value(&mut component_data, "fontSize", UIDocument::number_to_value(text.get_font_size()));
	write_value(&mut component_data, "textColor", UIDocument::color_to_array(text.get_text_color()));
	write_value(&mut component_data, "strokeColor", UIDocument::color_to_array(text.get_stroke_color()));
	write_value(&mut component_data, "strokeWidth", UIDocument::number_to_value(text.get_stroke_width()));
	write_value(&mut component_data, "textAlign", Value::from(find_name(&TEXT_ALIGN_NAME_TABLE, text.get_text_align())));
	write_value(&mut component_data, "textBaseline", Value::from(find_name(&TEXT_BASELINE_NAME_TABLE, text.get_text_baseline())));
	component_data
}

//==============================================================================
/// 글 속성 읽기. (원본 loadTextProperties)
//==============================================================================
fn load_text_properties(text: &mut Text, component_data: &Map<String, Value>) {
	if let Some(text_string) = read_string(component_data, "text") {
		text.set_text(text_string);
	}
	if let Some(font_size) = read_number(component_data, "fontSize") {
		text.set_font_size(font_size);
	}
	if let Some(text_color) = read_color(component_data, "textColor") {
		text.set_text_color(text_color);
	}
	if let Some(stroke_color) = read_color(component_data, "strokeColor") {
		text.set_stroke_color(stroke_color);
	}
	if let Some(stroke_width) = read_number(component_data, "strokeWidth") {
		text.set_stroke_width(stroke_width);
	}
	if let Some(text_align) = read_string(component_data, "textAlign").and_then(|name| find_value(&TEXT_ALIGN_NAME_TABLE, name)) {
		text.set_text_align(text_align);
	}
	if let Some(text_baseline) = read_string(component_data, "textBaseline").and_then(|name| find_value(&TEXT_BASELINE_NAME_TABLE, name)) {
		text.set_text_baseline(text_baseline);
	}
}

//==============================================================================
/// Text 쓰기.
//==============================================================================
fn save_text(text: &Text, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	save_text_properties(text)
}

//==============================================================================
/// Text 읽기.
//==============================================================================
fn load_text(text: &mut Text, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	load_text_properties(text, component_data);
}

//==============================================================================
/// RichText 쓰기. (품은 Text 의 속성, 글은 마크업이 든 원래 글)
//==============================================================================
fn save_rich_text(rich_text: &RichText, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	save_text_properties(rich_text.get_text_component())
}

//==============================================================================
/// RichText 읽기. (글은 RichText 의 set_text 로 다시 넣어 마크업을 푼다)
//==============================================================================
fn load_rich_text(rich_text: &mut RichText, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	load_text_properties(rich_text.get_text_component_mut(), component_data);
	if let Some(text_string) = read_string(component_data, "text") {
		rich_text.set_text(text_string);
	}
}

//==============================================================================
/// Mask 쓰기. (원본처럼 적는 속성이 없다)
//==============================================================================
fn save_mask(_mask: &Mask, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	Map::new()
}

//==============================================================================
/// Mask 읽기.
//==============================================================================
fn load_mask(_mask: &mut Mask, _component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
}

//==============================================================================
/// ShadowImage 쓰기. (그림 경로 뒤에 원본이 반사로 적을 꼴. 그림 색이 없으면 원본처럼 ImageColor 를 적지 않는다)
//==============================================================================
fn save_shadow_image(shadow_image: &ShadowImage, save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "imagePath", Value::from(shadow_image.get_image_path()));
	for (property_name, property_value) in UIDocument::save_by_reflection(shadow_image, save_context) {
		component_data.insert(property_name, property_value);
	}
	component_data
}

//==============================================================================
/// ShadowImage 읽기.
//==============================================================================
fn load_shadow_image(shadow_image: &mut ShadowImage, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext) {
	if let Some(image_path) = read_string(component_data, "imagePath") {
		if !image_path.is_empty() {
			shadow_image.set_image_path(image_path);
			load_context.bind_image::<ShadowImage>(image_path, |shadow_image, image| shadow_image.set_image(Some(image), None));
		}
	}
	UIDocument::load_by_reflection(shadow_image, component_data, load_context);
}

//==============================================================================
/// UILabel 쓰기.
//==============================================================================
fn save_label(label: &UILabel, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "text", Value::from(label.get_text()));
	write_value(&mut component_data, "fontSize", UIDocument::number_to_value(label.get_font_size()));
	write_value(&mut component_data, "textColor", UIDocument::color_to_array(label.get_text_color()));
	write_value(&mut component_data, "backgroundColor", UIDocument::color_to_array(label.get_background_color()));
	write_value(&mut component_data, "textAlign", Value::from(find_name(&TEXT_ALIGN_NAME_TABLE, label.get_text_align())));
	write_value(&mut component_data, "textBaseline", Value::from(find_name(&TEXT_BASELINE_NAME_TABLE, label.get_text_baseline())));
	component_data
}

//==============================================================================
/// UILabel 읽기.
//==============================================================================
fn load_label(label: &mut UILabel, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(text_string) = read_string(component_data, "text") {
		label.set_text(text_string);
	}
	if let Some(font_size) = read_number(component_data, "fontSize") {
		label.set_font_size(font_size);
	}
	if let Some(text_color) = read_color(component_data, "textColor") {
		label.set_text_color(text_color);
	}
	if let Some(background_color) = read_color(component_data, "backgroundColor") {
		label.set_background_color(background_color);
	}
	if let Some(text_align) = read_string(component_data, "textAlign").and_then(|name| find_value(&TEXT_ALIGN_NAME_TABLE, name)) {
		label.set_text_align(text_align);
	}
	if let Some(text_baseline) = read_string(component_data, "textBaseline").and_then(|name| find_value(&TEXT_BASELINE_NAME_TABLE, name)) {
		label.set_text_baseline(text_baseline);
	}
}

//==============================================================================
/// UIImageView 쓰기. (그림 경로만)
//==============================================================================
fn save_image_view(image_view: &UIImageView, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "imagePath", Value::from(image_view.get_image_path()));
	component_data
}

//==============================================================================
/// UIImageView 읽기.
//==============================================================================
fn load_image_view(image_view: &mut UIImageView, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext) {
	if let Some(image_path) = read_string(component_data, "imagePath") {
		if !image_path.is_empty() {
			image_view.set_image_path(image_path);
			load_context.bind_image::<UIImageView>(image_path, |image_view, image| image_view.set_image(Some(image)));
		}
	}
}

//==============================================================================
/// UIImageView 가 품은 Sprite 쓰기. (원본은 호스트에 붙인 Sprite 가 UIImageView 바로 뒤에 Sprite 로 적힌다)
//==============================================================================
fn save_image_view_companions(image_view: &UIImageView, save_context: &UIDocumentSaveContext) -> Vec<Map<String, Value>> {
	let mut sprite_data = save_sprite(image_view.get_sprite(), save_context);
	write_value(&mut sprite_data, "type", Value::from("Sprite"));
	vec![sprite_data]
}

//==============================================================================
/// UIImageView 가 품은 Sprite 읽기. (바로 뒤가 Sprite 면 그것을 품은 Sprite 에 읽는다. 원본은 UIImageView 가 붙을 때 붙인 Sprite 에 읽는다)
//==============================================================================
fn load_image_view_companions(image_view: &mut UIImageView, following_component_data_list: &[Value], load_context: &mut UIDocumentLoadContext) -> usize {
	let sprite_data = match find_part_component(following_component_data_list.first(), "Sprite") {
		Some(sprite_data) => {
			sprite_data
		}
		None => {
			return 0;
		}
	};
	if let Some(image_path) = load_sprite_fields(image_view.get_sprite_mut(), sprite_data) {
		load_context.bind_image::<UIImageView>(&image_path, |image_view, image| image_view.get_sprite_mut().set_image(Some(image)));
	}
	1
}

//==============================================================================
/// UIButton 쓰기.
//==============================================================================
fn save_button(button: &UIButton, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "pressedTintColor", UIDocument::color_to_array(button.get_pressed_tint_color()));
	component_data
}

//==============================================================================
/// UIButton 읽기.
//==============================================================================
fn load_button(button: &mut UIButton, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(pressed_tint_color) = read_color(component_data, "pressedTintColor") {
		button.set_pressed_tint_color(pressed_tint_color);
	}
}

//==============================================================================
/// UIToggleButton 쓰기. (품은 버튼의 눌림 틴트)
//==============================================================================
fn save_toggle_button(toggle_button: &UIToggleButton, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "pressedTintColor", UIDocument::color_to_array(toggle_button.get_uibutton().get_pressed_tint_color()));
	component_data
}

//==============================================================================
/// UIToggleButton 읽기.
//==============================================================================
fn load_toggle_button(toggle_button: &mut UIToggleButton, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(pressed_tint_color) = read_color(component_data, "pressedTintColor") {
		toggle_button.get_uibutton_mut().set_pressed_tint_color(pressed_tint_color);
	}
}

//==============================================================================
/// UIProgressView 쓰기.
//==============================================================================
fn save_progress_view(progress_view: &UIProgressView, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "value", UIDocument::number_to_value(progress_view.get_value()));
	write_value(&mut component_data, "minValue", UIDocument::number_to_value(progress_view.get_min_value()));
	write_value(&mut component_data, "maxValue", UIDocument::number_to_value(progress_view.get_max_value()));
	component_data
}

//==============================================================================
/// UIProgressView 읽기. (범위를 먼저 정하고 값을 넣는다)
//==============================================================================
fn load_progress_view(progress_view: &mut UIProgressView, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let (Some(min_value), Some(max_value)) = (read_number(component_data, "minValue"), read_number(component_data, "maxValue")) {
		progress_view.set_range(min_value, max_value);
	}
	if let Some(value) = read_number(component_data, "value") {
		progress_view.set_value(value);
	}
}

//==============================================================================
/// UISlider 쓰기. (손잡이 노드는 원본 saveNodeReferences 처럼 ThumbNode 참조로)
//==============================================================================
fn save_slider(slider: &UISlider, save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "value", UIDocument::number_to_value(slider.get_value()));
	write_value(&mut component_data, "thumbRadius", UIDocument::number_to_value(slider.get_thumb_radius()));
	write_value(&mut component_data, "trackThickness", UIDocument::number_to_value(slider.get_track_thickness()));
	save_context.write_node_reference(&mut component_data, "ThumbNode", slider.get_thumb_node());
	component_data
}

//==============================================================================
/// UISlider 읽기.
//==============================================================================
fn load_slider(slider: &mut UISlider, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext) {
	if let Some(value) = read_number(component_data, "value") {
		slider.set_value(value);
	}
	if let Some(thumb_radius) = read_number(component_data, "thumbRadius") {
		slider.set_thumb_radius(thumb_radius);
	}
	if let Some(track_thickness) = read_number(component_data, "trackThickness") {
		slider.set_track_thickness(track_thickness);
	}
	load_context.bind_node_reference::<UISlider>(component_data.get("ThumbNode"), |slider, thumb_node_id| slider.set_thumb_node(Some(thumb_node_id)));
}

//==============================================================================
/// UIScrollView 쓰기.
//==============================================================================
fn save_scroll_view(scroll_view: &UIScrollView, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "scrollContentSize", UIDocument::vector2_to_array(scroll_view.get_scroll_content_size()));
	write_value(&mut component_data, "scrollMode", Value::from(find_name(&SCROLL_MODE_NAME_TABLE, scroll_view.get_scroll_mode())));
	write_value(&mut component_data, "dragSensitivity", UIDocument::number_to_value(scroll_view.get_drag_sensitivity()));
	write_value(&mut component_data, "horizontal", Value::from(scroll_view.is_horizontal()));
	write_value(&mut component_data, "vertical", Value::from(scroll_view.is_vertical()));
	write_value(&mut component_data, "backgroundColor", UIDocument::color_to_array(scroll_view.get_background_color()));
	component_data
}

//==============================================================================
/// UIScrollView 읽기.
//==============================================================================
fn load_scroll_view(scroll_view: &mut UIScrollView, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(scroll_content_size) = read_vector2(component_data, "scrollContentSize") {
		scroll_view.set_scroll_content_size(scroll_content_size);
	}
	if let Some(is_horizontal) = read_bool(component_data, "horizontal") {
		scroll_view.set_horizontal(is_horizontal);
	}
	if let Some(is_vertical) = read_bool(component_data, "vertical") {
		scroll_view.set_vertical(is_vertical);
	}
	if let Some(scroll_mode) = read_string(component_data, "scrollMode").and_then(|name| find_value(&SCROLL_MODE_NAME_TABLE, name)) {
		scroll_view.set_scroll_mode(scroll_mode);
	}
	if let Some(drag_sensitivity) = read_number(component_data, "dragSensitivity") {
		scroll_view.set_drag_sensitivity(drag_sensitivity);
	}
	if let Some(background_color) = read_color(component_data, "backgroundColor") {
		scroll_view.set_background_color(background_color);
	}
}

//==============================================================================
/// 원본 노드 생성자가 만든 자식 노드 하나의 문서 값. (원본 UIScrollBar, UIInputField 의 자식 꼴: WorldNode, 크기 1, 돌림 0, 불투명 1, 피봇과 앵커 왼쪽 위, 누를 수 없음, 자식 없음)
//==============================================================================
fn create_part_node_data(name: &str, is_active: bool, position: Vector2, content_size: Vector2, component_data_list: Vec<Map<String, Value>>) -> Value {
	let mut node_data = Map::new();
	write_value(&mut node_data, "type", Value::from(NODE_TYPE_NAME));
	write_value(&mut node_data, "name", Value::from(name));
	write_value(&mut node_data, "active", Value::from(is_active));
	write_value(&mut node_data, "position", UIDocument::vector2_to_array(position));
	write_value(&mut node_data, "scale", UIDocument::vector2_to_array(Vector2::create(1.0, 1.0)));
	write_value(&mut node_data, "rotation", UIDocument::number_to_value(0.0));
	write_value(&mut node_data, "opacity", UIDocument::number_to_value(1.0));
	write_value(&mut node_data, "pivot", UIDocument::vector2_to_array(Vector2::zero()));
	write_value(&mut node_data, "anchor", UIDocument::vector2_to_array(Vector2::zero()));
	write_value(&mut node_data, "contentSize", UIDocument::vector2_to_array(content_size));
	write_value(&mut node_data, "interactable", Value::from(false));
	write_value(&mut node_data, "components", Value::Array(component_data_list.into_iter().map(Value::Object).collect()));
	write_value(&mut node_data, "children", Value::Array(Vec::new()));
	Value::Object(node_data)
}

//==============================================================================
/// 원본 생성자가 붙인 Paint 의 문서 값.
//==============================================================================
fn create_part_paint_data(color: Color, round_size: f32, save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut paint = Paint::new();
	paint.set_color(color);
	paint.set_round_size(round_size);
	let mut component_data = save_paint(&paint, save_context);
	write_value(&mut component_data, "type", Value::from("Paint"));
	component_data
}

//==============================================================================
/// 원본 UIInputField 가 만든 UILabel 의 문서 값. (바탕 투명, 왼쪽 정렬, 가운데 기준선)
//==============================================================================
fn create_part_label_data(text: &str, font_size: f32, text_color: Color, save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut label = UILabel::new();
	label.set_background_color(Color::create(0.0, 0.0, 0.0, 0.0));
	label.set_text(text);
	label.set_font_size(font_size);
	label.set_text_align(TextAlign::Left);
	label.set_text_baseline(TextBaseline::Middle);
	label.set_text_color(text_color);
	let mut component_data = save_label(&label, save_context);
	write_value(&mut component_data, "type", Value::from("UILabel"));
	component_data
}

//==============================================================================
/// 원본 UIInputField 의 그리기 층 컴포넌트 문서 값. (원본 BackgroundLayerRenderer, OverlayLayerRenderer 는 반사로 적혀 공통 속성만 남는다)
//==============================================================================
fn create_part_layer_data(component_type: &str, type_name: &str) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "GizmoVisible", Value::from(false));
	write_value(&mut component_data, "Enable", Value::from(true));
	write_value(&mut component_data, "ComponentType", Value::from(component_type));
	write_value(&mut component_data, "type", Value::from(type_name));
	component_data
}

//==============================================================================
/// 원본 생성자 몫의 컴포넌트 값. (그 종류면 값)
//==============================================================================
fn find_part_component<'a>(component_data: Option<&'a Value>, type_name: &str) -> Option<&'a Map<String, Value>> {
	let component_object = component_data?.as_object()?;
	if read_string(component_object, "type") != Some(type_name) {
		return None;
	}
	Some(component_object)
}

//==============================================================================
/// 원본 생성자 몫의 자식 노드의 첫 컴포넌트 값. (그 종류면 값)
//==============================================================================
fn find_part_child_component<'a>(child_data: Option<&'a Value>, type_name: &str) -> Option<&'a Map<String, Value>> {
	let first_component_data = child_data?.as_object()?.get("components")?.as_array()?.first();
	find_part_component(first_component_data, type_name)
}

//==============================================================================
/// 원본 노드 생성자처럼 자식과 컴포넌트를 붙을 때 실제로 짓는 노드 종류. (UIDocument 가 그것들을 그대로 쓰고, 읽을 때는 지은 것에 문서 값을 넣는다)
//==============================================================================
impl UIDocumentNode for UIWindow {
}

impl UIDocumentNode for UIDialog {
}

impl UIDocumentNode for UIDropdown {
}

//==============================================================================
/// UIToast 의 원본 노드 꼴. (원본은 숨을 때 노드를 끈다. 러스트 판은 tick 이 돌아야 해서 노드를 켠 채 숨긴다)
//==============================================================================
impl UIDocumentNode for UIToast {
	//==============================================================================
	/// 원본 active. (노드가 켜져 있고 보일 때만 true)
	//==============================================================================
	fn read_document_active(&self, is_node_active: bool, is_node_visible: bool) -> bool {
		is_node_active && is_node_visible
	}

	//==============================================================================
	/// 원본 active 를 노드 상태로. (노드는 늘 켜고, 원본 active 를 보임으로)
	//==============================================================================
	fn convert_document_active(&self, is_active: bool) -> (bool, Option<bool>) {
		(true, Some(is_active))
	}
}

impl UIDocumentNode for UISpinner {
}

impl UIDocumentNode for UIContextMenu {
}

impl UIDocumentNode for UIWindowLayer {
}

impl UIDocumentNode for FloatingText {
}

impl UIDocumentNode for TouchEffect {
}

//==============================================================================
/// UIInputField 의 원본 노드 꼴. (원본은 WorldNode 하위 클래스이고 생성자가 바탕 Paint, 선택 영역 층, 안내 글과 값 UILabel 자식, 오버레이 자식을 만든다)
/// - 글 라벨과 오버레이의 자리와 크기는 원본 refreshTextPositions 가 tick 에서 마지막으로 놓은 꼴이다. (그때의 노드 크기로 글 라벨은 (여백, 0) 에 (너비 - 여백 * 2, 높이),
///   오버레이는 노드 크기. 아직 놓지 않았으면 0) 읽을 때는 오버레이 크기를 그 크기로 되살린다.
/// - 읽을 때는 바탕 색과 둥근 크기, 안내 글과 그 색과 글 크기, 값 색, 켜진 값 라벨의 글(값)을 설정으로 읽는다. 커서 색과 최대 길이는 원본 문서에도 없다.
//==============================================================================
impl UIDocumentNode for UIInputField {
	//==============================================================================
	/// 원본 생성자 몫 쓰기. (바탕 Paint, 바탕 층, 안내 글과 값 라벨 자식, 오버레이 자식)
	//==============================================================================
	fn save_node_parts(&self, save_context: &UIDocumentSaveContext, _node_id: NodeId) -> UIDocumentNodeParts {
		let mut node_parts = UIDocumentNodeParts::new();
		node_parts.component_data_list.push(create_part_paint_data(self.get_background_color(), self.get_round_size(), save_context));
		node_parts.component_data_list.push(create_part_layer_data("UIInputFieldBgLayer", "BackgroundLayerRenderer"));
		let text_layout_size = self.get_text_layout_size();
		let mut text_position = Vector2::zero();
		let mut text_size = Vector2::zero();
		let mut overlay_size = Vector2::zero();
		if text_layout_size.x > 0.0 && text_layout_size.y > 0.0 {
			let padding = self.get_padding();
			text_position = Vector2::create(padding, 0.0);
			text_size = Vector2::create((text_layout_size.x - padding * 2.0).max(0.0), text_layout_size.y);
			overlay_size = text_layout_size;
		}
		let has_value = !self.get_text().is_empty() || !self.get_composition().is_empty();
		let display_text = if has_value {
			self.get_display_text()
		}
		else {
			String::new()
		};
		let placeholder_label_data = create_part_label_data(self.get_placeholder(), self.get_font_size(), self.get_placeholder_color(), save_context);
		let text_label_data = create_part_label_data(&display_text, self.get_font_size(), self.get_text_color(), save_context);
		node_parts.child_data_list.push(create_part_node_data("", !has_value, text_position, text_size, vec![placeholder_label_data]));
		node_parts.child_data_list.push(create_part_node_data("", has_value, text_position, text_size, vec![text_label_data]));
		node_parts.child_data_list.push(create_part_node_data("", true, Vector2::zero(), overlay_size, vec![create_part_layer_data("UIInputFieldOverlayLayer", "OverlayLayerRenderer")]));
		node_parts
	}

	//==============================================================================
	/// 원본 생성자 몫 읽기. (바탕 색과 둥근 크기, 안내 글과 그 색과 글 크기, 값 색과 값, 오버레이 크기)
	//==============================================================================
	fn load_node_parts(&mut self, component_data_list: &[Value], child_data_list: &[Value]) -> (usize, usize) {
		let mut component_count = 0;
		if let Some(background_data) = find_part_component(component_data_list.first(), "Paint") {
			if let Some(background_color) = read_color(background_data, "color") {
				self.set_background_color(background_color);
			}
			if let Some(round_size) = read_number(background_data, "roundSize") {
				self.set_round_size(round_size);
			}
			component_count = 1;
			if find_part_component(component_data_list.get(1), "BackgroundLayerRenderer").is_some() {
				component_count = 2;
			}
		}
		let mut child_count = 0;
		if let Some(placeholder_data) = find_part_child_component(child_data_list.first(), "UILabel") {
			if let Some(placeholder) = read_string(placeholder_data, "text") {
				self.set_placeholder(placeholder);
			}
			if let Some(font_size) = read_number(placeholder_data, "fontSize") {
				self.set_font_size(font_size);
			}
			if let Some(placeholder_color) = read_color(placeholder_data, "textColor") {
				self.set_placeholder_color(placeholder_color);
			}
			child_count = 1;
			if let Some(text_data) = find_part_child_component(child_data_list.get(1), "UILabel") {
				if let Some(text_color) = read_color(text_data, "textColor") {
					self.set_text_color(text_color);
				}
				// 값 라벨은 값이 있을 때만 켜진다. (원본 refreshTexts) 켜져 있으면 그 글이 값이다.
				let is_text_active = child_data_list.get(1).and_then(|child_data| child_data.get("active")).and_then(|value| value.as_bool()).unwrap_or(false);
				if is_text_active {
					if let Some(text) = read_string(text_data, "text") {
						self.set_text(text);
					}
				}
				child_count = 2;
				if find_part_child_component(child_data_list.get(2), "OverlayLayerRenderer").is_some() {
					// 오버레이 크기는 글 라벨을 마지막으로 놓은 노드 크기다.
					if let Some(overlay_size) = child_data_list.get(2).and_then(|child_data| child_data.as_object()).and_then(|child_object| read_vector2(child_object, "contentSize")) {
						self.set_text_layout_size(overlay_size);
					}
					child_count = 3;
				}
			}
		}
		(component_count, child_count)
	}

	//==============================================================================
	/// 문서에 적지 않는 content 노드. (UIInputField 의 content)
	//==============================================================================
	fn get_document_content(&self) -> Option<NodeId> {
		self.get_content()
	}
}

//==============================================================================
/// UIScrollBar 의 원본 노드 꼴. (원본은 WorldNode 하위 클래스이고 생성자가 트랙 Paint 와 썸 자식 노드(Paint)를 만든다)
/// - 썸 노드는 원본처럼 처음은 켜진 0 크기, tick 이 맞춘 뒤에는 그 사각형과 켜짐이다. (끌 때 사각형은 그대로 둔다)
/// - 읽을 때는 트랙 색과 썸 색을 설정으로 읽는다. 둥근 크기는 원본처럼 4 로 정해져 있다. 대상 스크롤 뷰는 원본 문서에도 없다.
//==============================================================================
impl UIDocumentNode for UIScrollBar {
	//==============================================================================
	/// 원본 생성자 몫 쓰기. (트랙 Paint, 썸 자식 노드)
	//==============================================================================
	fn save_node_parts(&self, save_context: &UIDocumentSaveContext, _node_id: NodeId) -> UIDocumentNodeParts {
		let mut node_parts = UIDocumentNodeParts::new();
		node_parts.component_data_list.push(create_part_paint_data(self.get_track_color(), SCROLL_BAR_PART_ROUND_SIZE, save_context));
		let (thumb_rect, is_thumb_active) = self.read_thumb_node_state();
		let thumb_paint_data = create_part_paint_data(self.get_thumb_color(), SCROLL_BAR_PART_ROUND_SIZE, save_context);
		node_parts.child_data_list.push(create_part_node_data("thumb", is_thumb_active, thumb_rect.position, thumb_rect.size, vec![thumb_paint_data]));
		node_parts
	}

	//==============================================================================
	/// 원본 생성자 몫 읽기. (트랙 색, 썸 색과 썸 사각형, 켜짐)
	//==============================================================================
	fn load_node_parts(&mut self, component_data_list: &[Value], child_data_list: &[Value]) -> (usize, usize) {
		let mut component_count = 0;
		if let Some(track_data) = find_part_component(component_data_list.first(), "Paint") {
			if let Some(track_color) = read_color(track_data, "color") {
				self.set_track_color(track_color);
			}
			component_count = 1;
		}
		let mut child_count = 0;
		let is_thumb_node = child_data_list.first().and_then(|child_data| child_data.get("name")).and_then(|value| value.as_str()) == Some("thumb");
		if is_thumb_node {
			if let Some(thumb_data) = find_part_child_component(child_data_list.first(), "Paint") {
				if let Some(thumb_color) = read_color(thumb_data, "color") {
					self.set_thumb_color(thumb_color);
				}
				if let Some(thumb_object) = child_data_list.first().and_then(|child_data| child_data.as_object()) {
					let thumb_position = read_vector2(thumb_object, "position").unwrap_or_default();
					let thumb_size = read_vector2(thumb_object, "contentSize").unwrap_or_default();
					let is_thumb_active = read_bool(thumb_object, "active").unwrap_or(true);
					self.apply_thumb_node_state(Rect::create(thumb_position.x, thumb_position.y, thumb_size.x, thumb_size.y), is_thumb_active);
				}
				child_count = 1;
			}
		}
		(component_count, child_count)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::graphic::TextDirection;

	/// vanilla.js UIDocument.toJsonText(뿌리, true) 가 낸 글. (에디터 처음 씬처럼 Panel 아래 Title 글, 그림 Sprite 하나)
	const EDITOR_SCENE_TEXT: &str = r#"{
	"version": 1,
	"root": {
		"type": "WorldNode",
		"name": "sceneRoot",
		"active": true,
		"position": [
			0,
			0
		],
		"scale": [
			1,
			1
		],
		"rotation": 0,
		"opacity": 1,
		"pivot": [
			0,
			0
		],
		"anchor": [
			0.5,
			0.5
		],
		"contentSize": [
			0,
			0
		],
		"interactable": false,
		"components": [],
		"children": [
			{
				"type": "WorldNode",
				"name": "Panel",
				"active": true,
				"position": [
					640,
					360
				],
				"scale": [
					1,
					1
				],
				"rotation": 0,
				"opacity": 1,
				"pivot": [
					0.5,
					0.5
				],
				"anchor": [
					0.5,
					0.5
				],
				"contentSize": [
					520,
					300
				],
				"interactable": false,
				"components": [
					{
						"color": [
							0.22,
							0.4,
							0.75,
							1
						],
						"roundSize": 24,
						"type": "Paint"
					}
				],
				"children": [
					{
						"type": "WorldNode",
						"name": "Title",
						"active": true,
						"position": [
							260,
							110
						],
						"scale": [
							1,
							1
						],
						"rotation": 0,
						"opacity": 1,
						"pivot": [
							0.5,
							0.5
						],
						"anchor": [
							0.5,
							0.5
						],
						"contentSize": [
							440,
							60
						],
						"interactable": false,
						"components": [
							{
								"text": "vanilla.js",
								"fontSize": 48,
								"textColor": [
									1,
									1,
									1,
									1
								],
								"strokeColor": [
									1,
									1,
									1,
									1
								],
								"strokeWidth": 0,
								"textAlign": "center",
								"textBaseline": "middle",
								"type": "Text"
							}
						],
						"children": []
					},
					{
						"type": "WorldNode",
						"name": "emblem",
						"active": true,
						"position": [
							260,
							210
						],
						"scale": [
							1.5,
							1.5
						],
						"rotation": 15,
						"opacity": 0.75,
						"pivot": [
							0.5,
							0.5
						],
						"anchor": [
							0,
							1
						],
						"contentSize": [
							96,
							96
						],
						"interactable": false,
						"components": [
							{
								"color": [
									0,
									0,
									0,
									0
								],
								"roundSize": 0,
								"imagePath": "emblem.png",
								"nineSlice": [
									0,
									0,
									0,
									0
								],
								"spriteDrawMode": "simple",
								"spriteBlendMode": "source-over",
								"type": "Sprite"
							}
						],
						"children": []
					}
				]
			}
		]
	}
}"#;

	/// vanilla.js UIDocument.toJsonText(뿌리, false) 가 낸 글. (반사 저장 종류, 위젯, content 안의 자식, 노드 참조)
	const WIDGET_SCENE_TEXT: &str = r#"{"version":1,"root":{"type":"WorldNode","name":"widgetRoot","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[0,0],"interactable":false,"components":[],"children":[{"type":"WorldNode","name":"outline","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"Color":[1,0.5,0.25,1],"LineWidth":3,"RoundSize":12,"Circle":true,"GizmoVisible":false,"Enable":true,"ComponentType":"Outline","type":"Outline"}],"children":[]},{"type":"WorldNode","name":"ring","active":false,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"Ratio":0.25,"RoundSize":20,"LineWidth":4,"FillColor":[0.5,1,0.5,1],"TrackColor":[0.2,0.2,0.25,1],"GizmoVisible":false,"Enable":true,"ComponentType":"UIProgressRing","type":"UIProgressRing"}],"children":[]},{"type":"WorldNode","name":"shade","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"GizmoVisible":false,"Enable":false,"ComponentType":"SoftShade","type":"SoftShade"}],"children":[]},{"type":"WorldNode","name":"mask","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"type":"Mask"}],"children":[]},{"type":"WorldNode","name":"rich","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"text":"<b>굵게</b> 보통","fontSize":20,"textColor":[1,1,1,1],"strokeColor":[0,0,0,1],"strokeWidth":2,"textAlign":"left","textBaseline":"top","type":"RichText"}],"children":[]},{"type":"WorldNode","name":"label","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"text":"라벨","fontSize":24,"textColor":[1,1,1,1],"backgroundColor":[0,0,0,0.5],"textAlign":"right","textBaseline":"middle","type":"UILabel"}],"children":[]},{"type":"WorldNode","name":"button","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"pressedTintColor":[0,0,0,0.5],"type":"UIButton"}],"children":[],"contentChildren":[{"type":"WorldNode","name":"caption","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"text":"눌러","fontSize":32,"textColor":[1,1,1,1],"strokeColor":[1,1,1,1],"strokeWidth":0,"textAlign":"center","textBaseline":"middle","type":"Text"}],"children":[]}]},{"type":"WorldNode","name":"toggle","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"pressedTintColor":[0,0,0,0.3],"type":"UIToggleButton"}],"children":[]},{"type":"WorldNode","name":"progress","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"value":50,"minValue":0,"maxValue":200,"type":"UIProgressView"}],"children":[]},{"type":"WorldNode","name":"slider","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"value":0.75,"thumbRadius":12,"trackThickness":6,"ThumbNode":{"$nodeRef":"/slider/thumb"},"type":"UISlider"}],"children":[{"type":"WorldNode","name":"thumb","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[],"children":[]}]},{"type":"WorldNode","name":"scroll","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"scrollContentSize":[200,600],"scrollMode":"elastic","dragSensitivity":1.5,"horizontal":false,"vertical":true,"backgroundColor":[1,1,1,0],"type":"UIScrollView"}],"children":[],"contentChildren":[{"type":"WorldNode","name":"item","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"color":[0.25,0.25,0.25,1],"roundSize":0,"type":"Paint"}],"children":[]}]},{"type":"WorldNode","name":"image","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"imagePath":"star.png","type":"UIImageView"},{"color":[0,0,0,0],"roundSize":0,"imagePath":"","nineSlice":[0,0,0,0],"spriteDrawMode":"simple","spriteBlendMode":"source-over","type":"Sprite"}],"children":[]}]}}"#;

	/// vanilla.js UIDocument.toJsonText(뿌리, false) 가 낸 글. (ShadowText 하나)
	const SHADOW_TEXT_SCENE_TEXT: &str = r#"{"version":1,"root":{"type":"WorldNode","name":"shadowRoot","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[0,0],"interactable":false,"components":[],"children":[{"type":"WorldNode","name":"shadowText","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[0,0],"interactable":false,"components":[{"Text":"그림자","TextAlign":"left","TextDirection":"rtl","Italic":true,"Underline":false,"Strikethrough":true,"FitMinimumRatio":0.5,"GizmoVisible":false,"Enable":true,"ComponentType":"ShadowText","type":"ShadowText"}],"children":[]}]}}"#;

	/// vanilla.js 가 처음 값의 Outline, SoftShade, ShadowText, UIProgressRing, ShadowImage 를 반사로 적은 컴포넌트 목록. (UIDocument.serializeNode 의 components)
	const REFLECTED_DEFAULTS_TEXT: &str = r#"[{"Color":[1,1,1,1],"LineWidth":1,"RoundSize":0,"Circle":false,"GizmoVisible":false,"Enable":true,"ComponentType":"Outline","type":"Outline"},{"GizmoVisible":false,"Enable":true,"ComponentType":"SoftShade","type":"SoftShade"},{"Text":"","TextAlign":"center","TextDirection":"ltr","Italic":false,"Underline":false,"Strikethrough":false,"FitMinimumRatio":0.6,"GizmoVisible":false,"Enable":true,"ComponentType":"ShadowText","type":"ShadowText"},{"Ratio":0,"RoundSize":0,"LineWidth":2,"FillColor":[1,1,1,1],"TrackColor":[0.2,0.2,0.25,1],"GizmoVisible":false,"Enable":true,"ComponentType":"UIProgressRing","type":"UIProgressRing"},{"GizmoVisible":false,"Enable":true,"ComponentType":"ShadowImage","type":"ShadowImage"}]"#;

	/// vanilla.js 에디터(tools/uieditor/uieditor.js addWidgetNode)와 같은 단계로 지은 inputField, scrollBar, snapScrollView 를 한 번 tick 하고 toJsonText(뿌리, false) 가 낸 글.
	const EDITOR_TYPES_SCENE_TEXT: &str = r#"{"version":1,"root":{"type":"WorldNode","name":"editorRoot","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[0,0],"interactable":false,"components":[],"children":[{"type":"UIInputField","name":"InputField1","active":true,"position":[60,60],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[260,40],"interactable":true,"components":[{"color":[1,1,1,1],"roundSize":8,"type":"Paint"},{"GizmoVisible":false,"Enable":true,"ComponentType":"UIInputFieldBgLayer","type":"BackgroundLayerRenderer"}],"children":[{"type":"WorldNode","name":"","active":true,"position":[16,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[228,40],"interactable":false,"components":[{"text":"","fontSize":28,"textColor":[0.6,0.6,0.6,1],"backgroundColor":[0,0,0,0],"textAlign":"left","textBaseline":"middle","type":"UILabel"}],"children":[]},{"type":"WorldNode","name":"","active":false,"position":[16,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[228,40],"interactable":false,"components":[{"text":"","fontSize":28,"textColor":[0.13333333333333333,0.13333333333333333,0.13333333333333333,1],"backgroundColor":[0,0,0,0],"textAlign":"left","textBaseline":"middle","type":"UILabel"}],"children":[]},{"type":"WorldNode","name":"","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[260,40],"interactable":false,"components":[{"GizmoVisible":false,"Enable":true,"ComponentType":"UIInputFieldOverlayLayer","type":"OverlayLayerRenderer"}],"children":[]}]},{"type":"UIScrollBar","name":"ScrollBar2","active":true,"position":[60,60],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[12,220],"interactable":true,"components":[{"color":[0,0,0,0.15],"roundSize":4,"type":"Paint"}],"children":[{"type":"WorldNode","name":"thumb","active":false,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[0,0],"interactable":false,"components":[{"color":[1,1,1,0.6],"roundSize":4,"type":"Paint"}],"children":[]}]},{"type":"WorldNode","name":"SnapScrollView3","active":true,"position":[60,60],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[320,220],"interactable":true,"components":[{"color":[0.965,0.965,0.965,1],"roundSize":16,"type":"Paint"},{"SnapToNearest":false,"VelocityThreshold":200,"PageChangeThreshold":0.35,"SnapCurrentIndex":0,"ScrollOffset":[0,0],"Horizontal":true,"Vertical":false,"DragSensitivity":1,"ScrollMode":"clamp","ScrollContentSize":[960,220],"BackgroundColor":[1,1,1,0],"GizmoVisible":false,"Enable":true,"ComponentType":"SnapScrollView","type":"UISnapScrollView"}],"children":[],"contentChildren":[{"type":"WorldNode","name":"page","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[320,220],"interactable":false,"components":[],"children":[]}]}]}}"#;

	/// vanilla.js 를 node 로 돌려 뽑은 기대값. (testdata/uidocument/generate.mjs 가 원본 저장소에서 다시 뽑는다)
	const EDITOR_COMPONENTS_TEXT: &str = include_str!("testdata/uidocument/editor_components.json");
	const EDITOR_NODE_TYPES_TEXT: &str = include_str!("testdata/uidocument/editor_node_types.json");
	const EDITOR_PALETTE_TEXT: &str = include_str!("testdata/uidocument/editor_palette.json");
	const EDITOR_PALETTE_BEFORE_TICK_TEXT: &str = include_str!("testdata/uidocument/editor_palette_before_tick.json");
	const INSPECTOR_GETTERS_TEXT: &str = include_str!("testdata/uidocument/inspector_getters.json");
	const REFLECTED_PROPERTIES_TEXT: &str = include_str!("testdata/uidocument/reflected_properties.json");

	/// 위와 같되 안내 글, 값, 글 크기, 둥근 크기, 색, 트랙과 썸 색, 스냅 설정을 바꾼 것.
	const EDITOR_TYPES_CUSTOM_SCENE_TEXT: &str = r#"{"version":1,"root":{"type":"WorldNode","name":"editorRoot","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[0,0],"interactable":false,"components":[],"children":[{"type":"UIInputField","name":"InputField1","active":true,"position":[60,60],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[260,40],"interactable":true,"components":[{"color":[0.9,0.95,1,1],"roundSize":4,"type":"Paint"},{"GizmoVisible":false,"Enable":true,"ComponentType":"UIInputFieldBgLayer","type":"BackgroundLayerRenderer"}],"children":[{"type":"WorldNode","name":"","active":false,"position":[16,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[228,40],"interactable":false,"components":[{"text":"이름을 넣어요","fontSize":20,"textColor":[0.5,0.5,0.5,1],"backgroundColor":[0,0,0,0],"textAlign":"left","textBaseline":"middle","type":"UILabel"}],"children":[]},{"type":"WorldNode","name":"","active":true,"position":[16,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[228,40],"interactable":false,"components":[{"text":"뚝백","fontSize":20,"textColor":[0.1,0.2,0.3,1],"backgroundColor":[0,0,0,0],"textAlign":"left","textBaseline":"middle","type":"UILabel"}],"children":[]},{"type":"WorldNode","name":"","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[260,40],"interactable":false,"components":[{"GizmoVisible":false,"Enable":true,"ComponentType":"UIInputFieldOverlayLayer","type":"OverlayLayerRenderer"}],"children":[]}]},{"type":"UIScrollBar","name":"ScrollBar2","active":true,"position":[60,60],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[12,220],"interactable":true,"components":[{"color":[0.2,0.2,0.2,0.5],"roundSize":4,"type":"Paint"}],"children":[{"type":"WorldNode","name":"thumb","active":false,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[0,0],"interactable":false,"components":[{"color":[1,0.5,0,1],"roundSize":4,"type":"Paint"}],"children":[]}]},{"type":"WorldNode","name":"SnapScrollView3","active":true,"position":[60,60],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0,0],"anchor":[0,0],"contentSize":[320,220],"interactable":true,"components":[{"color":[0.965,0.965,0.965,1],"roundSize":16,"type":"Paint"},{"SnapToNearest":true,"VelocityThreshold":300,"PageChangeThreshold":0.5,"SnapCurrentIndex":0,"ScrollOffset":[0,0],"Horizontal":true,"Vertical":false,"DragSensitivity":1.5,"ScrollMode":"clamp","ScrollContentSize":[960,220],"BackgroundColor":[1,1,1,0],"GizmoVisible":false,"Enable":true,"ComponentType":"SnapScrollView","type":"UISnapScrollView"}],"children":[],"contentChildren":[{"type":"WorldNode","name":"page","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[320,220],"interactable":false,"components":[],"children":[]}]}]}}"#;

	//==============================================================================
	/// 에디터 씬 파일(frameSize 를 얹은 것)을 읽어 다시 쓰면 원본이 낸 글과 한 글자도 다르지 않다.
	//==============================================================================
	#[test]
	fn editor_scene_round_trip_is_same_text() {
		let mut scene_file_data: Value = serde_json::from_str(EDITOR_SCENE_TEXT).expect("scene json");
		scene_file_data.as_object_mut().expect("scene object").insert(String::from("frameSize"), serde_json::json!([1280, 720]));
		let scene_file_text = serde_json::to_string(&scene_file_data).expect("scene text");
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		let scene_root_id = document.from_json_text(&mut node_tree, &scene_file_text).expect("document");
		assert_eq!(document.to_json_text(&node_tree, scene_root_id, true), EDITOR_SCENE_TEXT);

		let panel_id = node_tree.find_child_by_name(scene_root_id, "Panel").expect("panel");
		assert_eq!(node_tree[panel_id].get_component::<Paint>().expect("paint").get_round_size(), 24.0);
		let title_id = node_tree.find_child_by_name(panel_id, "Title").expect("title");
		assert_eq!(node_tree[title_id].get_component::<Text>().expect("text").get_text(), "vanilla.js");
		let emblem_id = node_tree.find_child_by_name(panel_id, "emblem").expect("emblem");
		assert_eq!(node_tree[emblem_id].get_component::<Sprite>().expect("sprite").get_image_path(), "emblem.png");
		assert_eq!(node_tree[emblem_id].get_local_rotation(), 15.0);
		assert_eq!(node_tree[emblem_id].get_anchor(), Vector2::create(0.0, 1.0));
	}

	//==============================================================================
	/// 러스트에서 같은 트리를 지어 내보내면 원본이 낸 글과 같다.
	//==============================================================================
	#[test]
	fn rust_built_tree_is_written_like_original() {
		let mut node_tree = NodeTree::new();
		let scene_root_id = node_tree.create_node("sceneRoot");
		node_tree[scene_root_id].set_pivot(Vector2::zero());
		let panel_id = node_tree.create_child(scene_root_id, "Panel");
		node_tree[panel_id].set_content_size(Vector2::create(520.0, 300.0));
		node_tree[panel_id].set_local_position(Vector2::create(640.0, 360.0));
		let paint = node_tree[panel_id].add_component(Paint::new());
		paint.set_color(Color::create(0.22, 0.4, 0.75, 1.0));
		paint.set_round_size(24.0);
		let title_id = node_tree.create_child(panel_id, "Title");
		node_tree[title_id].set_content_size(Vector2::create(440.0, 60.0));
		node_tree[title_id].set_local_position(Vector2::create(260.0, 110.0));
		node_tree[title_id].add_component(Text::create_with_text("vanilla.js", 48.0));
		let emblem_id = node_tree.create_child(panel_id, "emblem");
		let emblem = &mut node_tree[emblem_id];
		emblem.set_content_size(Vector2::create(96.0, 96.0));
		emblem.set_local_position(Vector2::create(260.0, 210.0));
		emblem.set_local_rotation(15.0);
		emblem.set_local_scale(Vector2::create(1.5, 1.5));
		emblem.set_local_opacity(0.75);
		emblem.set_anchor(Vector2::create(0.0, 1.0));
		emblem.add_component(Sprite::new()).set_image_path("emblem.png");
		let document = UIDocument::new();
		assert_eq!(document.to_json_text(&node_tree, scene_root_id, true), EDITOR_SCENE_TEXT);
	}

	//==============================================================================
	/// 위젯과 반사 저장 종류의 왕복. (content 노드는 다시 만들어지고, 그 안의 자식과 노드 참조가 이어진다)
	//==============================================================================
	#[test]
	fn widget_scene_round_trip_is_same_text() {
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		let widget_root_id = document.from_json_text(&mut node_tree, WIDGET_SCENE_TEXT).expect("document");
		assert_eq!(document.to_json_text(&node_tree, widget_root_id, false), WIDGET_SCENE_TEXT);

		let slider_id = node_tree.find_child_by_name(widget_root_id, "slider").expect("slider");
		let thumb_id = node_tree.find_child_by_name(slider_id, "thumb").expect("thumb");
		assert_eq!(node_tree[slider_id].get_component::<UISlider>().expect("slider").get_thumb_node(), Some(thumb_id));
		let scroll_id = node_tree.find_child_by_name(widget_root_id, "scroll").expect("scroll");
		assert!(node_tree[scroll_id].has_component::<Mask>());
		let scroll_content_id = node_tree[scroll_id].get_component::<UIScrollView>().expect("scroll view").get_content().expect("content");
		assert!(node_tree.find_child_by_name(scroll_content_id, "item").is_some());
		let button_id = node_tree.find_child_by_name(widget_root_id, "button").expect("button");
		let button_content_id = node_tree[button_id].get_component::<UIButton>().expect("button").get_content().expect("content");
		assert!(node_tree.find_child_by_name(button_content_id, "caption").is_some());
		let ring_id = node_tree.find_child_by_name(widget_root_id, "ring").expect("ring");
		assert!(!node_tree[ring_id].is_active());
		assert_eq!(node_tree[ring_id].get_component::<UIProgressRing>().expect("ring").get_ratio(), 0.25);
		let shade_id = node_tree.find_child_by_name(widget_root_id, "shade").expect("shade");
		assert!(!node_tree[shade_id].get_component::<SoftShade>().expect("shade").is_enable());
	}

	//==============================================================================
	/// ShadowText 는 원본 반사 저장과 같은 키로 쓰고 읽는다. (글 방향, 기울임, 밑줄, 가운데줄 포함)
	//==============================================================================
	#[test]
	fn shadow_text_round_trip_keeps_reflected_keys() {
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		let shadow_root_id = document.from_json_text(&mut node_tree, SHADOW_TEXT_SCENE_TEXT).expect("document");
		assert_eq!(document.to_json_text(&node_tree, shadow_root_id, false), SHADOW_TEXT_SCENE_TEXT);
		let shadow_text_id = node_tree.find_child_by_name(shadow_root_id, "shadowText").expect("shadow text");
		let shadow_text = node_tree[shadow_text_id].get_component::<ShadowText>().expect("shadow text");
		assert_eq!(shadow_text.get_text_direction(), TextDirection::Rtl);
		assert!(shadow_text.is_italic());
		assert!(shadow_text.is_strikethrough());

		// 이름으로 지어 붙이기. (원본 getComponentType)
		let created_index = document.create_component(&mut node_tree, shadow_root_id, "UIProgressRing").expect("created");
		assert!(node_tree[shadow_root_id].get_all_components()[created_index].as_any().is::<UIProgressRing>());
		assert!(document.create_component(&mut node_tree, shadow_root_id, "Unknown").is_none());
	}

	//==============================================================================
	/// 이미지 해석기가 찾은 그림을 Sprite 와 UIImageView 에 넣는다. 못 찾은 그림은 경로만 남는다.
	//==============================================================================
	#[test]
	fn image_resolver_sets_images() {
		let star_image = Image::create_from_pixels(1, 1, vec![255, 255, 255, 255]);
		let resolved_image = star_image.clone();
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		document.set_image_resolver(move |image_path| {
			if image_path == "star.png" {
				Some(resolved_image.clone())
			}
			else {
				None
			}
		});
		let widget_root_id = document.from_json_text(&mut node_tree, WIDGET_SCENE_TEXT).expect("document");
		let image_id = node_tree.find_child_by_name(widget_root_id, "image").expect("image");
		let image_view = node_tree[image_id].get_component::<UIImageView>().expect("image view");
		assert!(image_view.get_image().is_some_and(|image| Rc::ptr_eq(image, &star_image)));
		assert!(document.resolve_image("emblem.png").is_none());
		assert_eq!(document.bind_pending_images(&mut node_tree), 0);
	}

	//==============================================================================
	/// 등록하지 않은 종류는 건너뛰고, 게임이 등록한 종류는 짓고 쓰고 읽는다.
	//==============================================================================
	#[test]
	fn registered_game_component_round_trip() {
		struct Spin {
			component_base: crate::core::component::ComponentBase,
			degrees_per_second: f32,
		}
		impl Component for Spin {
			//==============================================================================
			/// 컴포넌트 공통 상태 반환.
			//==============================================================================
			fn get_component_base(&self) -> &crate::core::component::ComponentBase {
				&self.component_base
			}
			//==============================================================================
			/// 컴포넌트 공통 상태 반환. (변경용)
			//==============================================================================
			fn get_component_base_mut(&mut self) -> &mut crate::core::component::ComponentBase {
				&mut self.component_base
			}
			//==============================================================================
			/// 컴포넌트 종류 이름.
			//==============================================================================
			fn get_component_type(&self) -> &'static str {
				"Spin"
			}
		}
		let document_text = r#"{"version":1,"root":{"name":"root","components":[{"type":"Unknown"},{"degreesPerSecond":90,"type":"Spin"}],"children":[]}}"#;
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		let unknown_root_id = document.from_json_text(&mut node_tree, document_text).expect("document");
		assert_eq!(node_tree[unknown_root_id].get_all_components().len(), 0);

		document.register_component_type_with_handler("Spin", || Spin { component_base: crate::core::component::ComponentBase::new(), degrees_per_second: 0.0 }, |spin, _save_context| {
			let mut component_data = Map::new();
			component_data.insert(String::from("degreesPerSecond"), UIDocument::number_to_value(spin.degrees_per_second));
			component_data
		}, |spin, component_data, _load_context| {
			spin.degrees_per_second = component_data.get("degreesPerSecond").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
		});
		assert!(document.get_component_type_names().contains(&"Spin"));
		let spin_root_id = document.from_json_text(&mut node_tree, document_text).expect("document");
		assert_eq!(node_tree[spin_root_id].get_component::<Spin>().expect("spin").degrees_per_second, 90.0);
		let saved_data: Value = serde_json::from_str(&document.to_json_text(&node_tree, spin_root_id, false)).expect("saved json");
		assert_eq!(saved_data["root"]["components"], serde_json::json!([{ "degreesPerSecond": 90, "type": "Spin" }]));
	}

	//==============================================================================
	/// 잘못된 문서는 원본처럼 실패한다.
	//==============================================================================
	#[test]
	fn invalid_document_is_error() {
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		assert!(document.from_json_text(&mut node_tree, "{\"version\":1}").is_err());
		assert!(document.from_json_text(&mut node_tree, "not json").is_err());
	}

	//==============================================================================
	/// 수는 가장 짧은 십진으로, 정수는 소수점 없이 적는다.
	//==============================================================================
	#[test]
	fn number_text_is_shortest() {
		assert_eq!(UIDocument::number_to_value(0.22).to_string(), "0.22");
		assert_eq!(UIDocument::number_to_value(24.0).to_string(), "24");
		assert_eq!(UIDocument::number_to_value(-0.0).to_string(), "0");
		assert_eq!(UIDocument::number_to_value(f32::NAN), Value::Null);
	}

	//==============================================================================
	/// 노드 경로 계산과 찾기. (원본 findNodePath, findNodeByPath)
	//==============================================================================
	#[test]
	fn node_path_round_trip() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.create_node("root");
		let panel_id = node_tree.create_child(root_id, "panel");
		let button_id = node_tree.create_child(panel_id, "button");
		let outside_id = node_tree.create_node("outside");
		assert_eq!(UIDocument::find_node_path(&node_tree, root_id, button_id).as_deref(), Some("/panel/button"));
		assert_eq!(UIDocument::find_node_path(&node_tree, root_id, root_id).as_deref(), Some("/"));
		assert_eq!(UIDocument::find_node_path(&node_tree, root_id, outside_id), None);
		assert_eq!(UIDocument::find_node_by_path(&node_tree, root_id, "/panel/button"), Some(button_id));
		assert_eq!(UIDocument::find_node_by_path(&node_tree, root_id, "/"), Some(root_id));
		assert_eq!(UIDocument::find_node_by_path(&node_tree, root_id, "/panel/missing"), None);
	}

	//==============================================================================
	/// 시험용 게임 컴포넌트. (`#[vanilla::reflect]` 를 붙여 등록 한 줄로 쓰고 읽는다)
	//==============================================================================
	struct Follower {
		component_base: crate::core::component::ComponentBase,
		speed: f32,
		label: String,
		tint: Color,
		offset: Vector2,
		text_align: TextAlign,
		leader: Option<NodeId>,
		slot: usize,
		cached_area: f32,
		secret: i32,
		cache_count: usize,
	}

	#[vanilla::reflect]
	impl Follower {
		//==============================================================================
		/// 생성.
		//==============================================================================
		pub fn new() -> Follower {
			Follower {
				component_base: crate::core::component::ComponentBase::new(),
				speed: 1.0,
				label: String::new(),
				tint: Color::create(1.0, 1.0, 1.0, 1.0),
				offset: Vector2::zero(),
				text_align: TextAlign::Left,
				leader: None,
				slot: 0,
				cached_area: -1.0,
				secret: 0,
				cache_count: 0,
			}
		}

		//==============================================================================
		/// 속도 설정.
		//==============================================================================
		pub fn set_speed(&mut self, speed: f32) {
			self.speed = speed;
		}

		//==============================================================================
		/// 속도 반환.
		//==============================================================================
		pub fn get_speed(&self) -> f32 {
			self.speed
		}

		//==============================================================================
		/// 이름표 설정.
		//==============================================================================
		pub fn set_label(&mut self, label: &str) {
			self.label = String::from(label);
		}

		//==============================================================================
		/// 이름표 반환.
		//==============================================================================
		pub fn get_label(&self) -> &str {
			&self.label
		}

		//==============================================================================
		/// 색 설정.
		//==============================================================================
		pub fn set_tint(&mut self, tint: impl Into<Color>) {
			self.tint = tint.into();
		}

		//==============================================================================
		/// 색 반환.
		//==============================================================================
		pub fn get_tint(&self) -> Color {
			self.tint
		}

		//==============================================================================
		/// 떨어진 거리 설정.
		//==============================================================================
		pub fn set_offset(&mut self, offset: Vector2) {
			self.offset = offset;
		}

		//==============================================================================
		/// 떨어진 거리 반환.
		//==============================================================================
		pub fn get_offset(&self) -> Vector2 {
			self.offset
		}

		//==============================================================================
		/// 글 정렬 설정.
		//==============================================================================
		pub fn set_text_align(&mut self, text_align: TextAlign) {
			self.text_align = text_align;
		}

		//==============================================================================
		/// 글 정렬 반환.
		//==============================================================================
		pub fn get_text_align(&self) -> TextAlign {
			self.text_align
		}

		//==============================================================================
		/// 따라갈 노드 설정.
		//==============================================================================
		pub fn set_leader(&mut self, leader: Option<NodeId>) {
			self.leader = leader;
		}

		//==============================================================================
		/// 따라갈 노드 반환.
		//==============================================================================
		pub fn get_leader(&self) -> Option<NodeId> {
			self.leader
		}

		//==============================================================================
		/// 따라갈 노드 이름. (트리를 받는 게터)
		//==============================================================================
		pub fn get_leader_name<'a>(&self, node_tree: &'a NodeTree) -> &'a str {
			self.leader.and_then(|leader_id| node_tree.get_node(leader_id)).map(|leader_node| leader_node.get_name()).unwrap_or("")
		}

		//==============================================================================
		/// 자리 설정. (트리를 받는 세터, 노드 수로 자른다)
		//==============================================================================
		pub fn set_slot(&mut self, node_tree: &mut NodeTree, slot: usize) {
			self.slot = slot.min(node_tree.get_node_count());
		}

		//==============================================================================
		/// 자리 반환.
		//==============================================================================
		pub fn get_slot(&self) -> usize {
			self.slot
		}

		//==============================================================================
		/// 넓이 캐시 설정.
		//==============================================================================
		pub fn set_cached_area(&mut self, cached_area: f32) {
			self.cached_area = cached_area;
		}

		//==============================================================================
		/// 넓이 반환. (&mut self 게터, 처음 읽을 때 셈해 둔다)
		//==============================================================================
		pub fn get_cached_area(&mut self) -> f32 {
			if self.cached_area < 0.0 {
				self.cached_area = self.offset.x * self.offset.y;
			}
			self.cached_area
		}

		//==============================================================================
		/// 숨긴 값 설정. (반사에서 뺀다)
		//==============================================================================
		#[reflect(skip)]
		pub fn set_secret(&mut self, secret: i32) {
			self.secret = secret;
		}

		//==============================================================================
		/// 숨긴 값 반환. (반사에서 뺀다)
		//==============================================================================
		#[reflect(skip)]
		pub fn get_secret(&self) -> i32 {
			self.secret
		}

		//==============================================================================
		/// 캐시 수 설정.
		//==============================================================================
		pub fn set_cache_count(&mut self, cache_count: usize) {
			self.cache_count = cache_count;
		}

		//==============================================================================
		/// 캐시 수 반환. (저장하지 않는다)
		//==============================================================================
		#[reflect(transient)]
		pub fn get_cache_count(&self) -> usize {
			self.cache_count
		}

		//==============================================================================
		/// 따라가는 중인지.
		//==============================================================================
		pub fn is_following(&self) -> bool {
			self.leader.is_some()
		}
	}

	#[vanilla::reflect]
	impl Component for Follower {
		//==============================================================================
		/// 컴포넌트 공통 상태 반환.
		//==============================================================================
		fn get_component_base(&self) -> &crate::core::component::ComponentBase {
			&self.component_base
		}

		//==============================================================================
		/// 컴포넌트 공통 상태 반환. (변경용)
		//==============================================================================
		fn get_component_base_mut(&mut self) -> &mut crate::core::component::ComponentBase {
			&mut self.component_base
		}

		//==============================================================================
		/// 컴포넌트 종류 이름.
		//==============================================================================
		fn get_component_type(&self) -> &'static str {
			"Follower"
		}

		//==============================================================================
		/// tick 이 필요한지. (늘 false)
		//==============================================================================
		fn is_tick_needed(&self) -> bool {
			false
		}
	}

	//==============================================================================
	/// 두 문서 값이 같은지. (키 차례까지 같고, 수는 f32 로 줄여 비교한다. 원본 f64 의 끝자리는 러스트 판에서 f32 로 줄어든다)
	//==============================================================================
	fn assert_same_document_value(left_value: &Value, right_value: &Value, value_path: &str) {
		match (left_value, right_value) {
			(Value::Object(left_object), Value::Object(right_object)) => {
				let left_keys: Vec<&String> = left_object.keys().collect();
				let right_keys: Vec<&String> = right_object.keys().collect();
				assert_eq!(left_keys, right_keys, "키가 다르다: {}", value_path);
				for (key, left_child) in left_object.iter() {
					assert_same_document_value(left_child, &right_object[key], &format!("{}/{}", value_path, key));
				}
			}
			(Value::Array(left_items), Value::Array(right_items)) => {
				assert_eq!(left_items.len(), right_items.len(), "배열 길이가 다르다: {}", value_path);
				for (item_index, (left_item, right_item)) in left_items.iter().zip(right_items.iter()).enumerate() {
					assert_same_document_value(left_item, right_item, &format!("{}[{}]", value_path, item_index));
				}
			}
			(Value::Number(left_number), Value::Number(right_number)) => {
				let left_float = left_number.as_f64().unwrap_or(f64::NAN) as f32;
				let right_float = right_number.as_f64().unwrap_or(f64::NAN) as f32;
				assert_eq!(left_float, right_float, "수가 다르다: {}", value_path);
			}
			_ => {
				assert_eq!(left_value, right_value, "값이 다르다: {}", value_path);
			}
		}
	}

	//==============================================================================
	/// 원본이 반사로 저장하는 종류는 처음 값을 원본 saveByReflection 과 같은 키, 차례, 값으로 적는다. (ShadowImage 만 imagePath 가 앞에 더 붙는다)
	//==============================================================================
	#[test]
	fn reflected_components_match_original_defaults() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.create_node("root");
		node_tree.add_component(root_id, Outline::new());
		node_tree.add_component(root_id, SoftShade::new());
		node_tree.add_component(root_id, ShadowText::new());
		node_tree.add_component(root_id, UIProgressRing::new());
		node_tree.add_component(root_id, ShadowImage::new());
		let document = UIDocument::new();
		let written_data = document.serialize_node(&node_tree, root_id);
		let mut expected_data: Value = serde_json::from_str(REFLECTED_DEFAULTS_TEXT).expect("defaults json");
		let shadow_image_data = expected_data[4].as_object().expect("shadow image").clone();
		let mut extended_shadow_image_data = Map::new();
		extended_shadow_image_data.insert(String::from("imagePath"), Value::from(""));
		extended_shadow_image_data.extend(shadow_image_data);
		expected_data[4] = Value::Object(extended_shadow_image_data);
		assert_eq!(written_data["components"], expected_data);
	}

	//==============================================================================
	/// 원본 에디터가 저장한 UIInputField, UIScrollBar(원본 노드 종류)와 UISnapScrollView 를 register_editor_types 한 줄로 읽고, 다시 쓰면 원본 글과 같다.
	/// - 노드 종류는 노드의 type 이고, 원본 생성자가 만든 컴포넌트와 자식은 설정으로 읽어 다시 짓지 않는다.
	//==============================================================================
	#[test]
	fn editor_document_types_round_trip() {
		for document_text in [EDITOR_TYPES_SCENE_TEXT, EDITOR_TYPES_CUSTOM_SCENE_TEXT] {
			let mut node_tree = NodeTree::new();
			let mut document = UIDocument::new();
			document.register_editor_types();
			let root_id = document.from_json_text(&mut node_tree, document_text).expect("document");
			let written_data: Value = serde_json::from_str(&document.to_json_text(&node_tree, root_id, false)).expect("written json");
			let original_data: Value = serde_json::from_str(document_text).expect("original json");
			assert_same_document_value(&written_data, &original_data, "");

			// 노드 종류는 그 컴포넌트 하나만 붙고, 원본 생성자의 자식은 짓지 않는다. (UIInputField 는 러스트 판의 content 노드 하나)
			let field_id = node_tree.find_child_by_name(root_id, "InputField1").expect("field");
			assert_eq!(node_tree[field_id].get_all_components().len(), 1);
			assert_eq!(node_tree.get_child_ids(field_id).len(), 1);
			let scroll_bar_id = node_tree.find_child_by_name(root_id, "ScrollBar2").expect("scroll bar");
			assert_eq!(node_tree[scroll_bar_id].get_all_components().len(), 1);
			assert!(node_tree.get_child_ids(scroll_bar_id).is_empty());
			assert_eq!(node_tree[scroll_bar_id].get_content_size(), Vector2::create(12.0, 220.0));
			let snap_id = node_tree.find_child_by_name(root_id, "SnapScrollView3").expect("snap");
			assert!(node_tree[snap_id].has_component::<Mask>());
			let snap_view = node_tree[snap_id].get_component::<UISnapScrollView>().expect("snap view");
			assert_eq!(snap_view.get_scroll_content_size(), Vector2::create(960.0, 220.0));
			let snap_content_id = snap_view.get_content().expect("content");
			assert!(node_tree.find_child_by_name(snap_content_id, "page").is_some());
		}

		// 바꾼 값은 컴포넌트 설정으로 들어온다.
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		document.register_editor_types();
		let root_id = document.from_json_text(&mut node_tree, EDITOR_TYPES_CUSTOM_SCENE_TEXT).expect("document");
		let field_id = node_tree.find_child_by_name(root_id, "InputField1").expect("field");
		let field = node_tree[field_id].get_component::<UIInputField>().expect("input field");
		assert_eq!(field.get_placeholder(), "이름을 넣어요");
		assert_eq!(field.get_text(), "뚝백");
		assert_eq!(field.get_font_size(), 20.0);
		assert_eq!(field.get_round_size(), 4.0);
		assert_eq!(field.get_background_color(), Color::create(0.9, 0.95, 1.0, 1.0));
		assert_eq!(field.get_text_color(), Color::create(0.1, 0.2, 0.3, 1.0));
		assert_eq!(field.get_placeholder_color(), Color::create(0.5, 0.5, 0.5, 1.0));
		let scroll_bar_id = node_tree.find_child_by_name(root_id, "ScrollBar2").expect("scroll bar");
		let scroll_bar = node_tree[scroll_bar_id].get_component::<UIScrollBar>().expect("scroll bar");
		assert_eq!(scroll_bar.get_track_color(), Color::create(0.2, 0.2, 0.2, 0.5));
		assert_eq!(scroll_bar.get_thumb_color(), Color::create(1.0, 0.5, 0.0, 1.0));
		let snap_id = node_tree.find_child_by_name(root_id, "SnapScrollView3").expect("snap");
		let snap_view = node_tree[snap_id].get_component::<UISnapScrollView>().expect("snap view");
		assert!(snap_view.get_snap_to_nearest());
		assert_eq!(snap_view.get_velocity_threshold(), 300.0);
		assert_eq!(snap_view.get_page_change_threshold(), 0.5);
		assert_eq!(snap_view.get_scroll_view().get_drag_sensitivity(), 1.5);

		// 등록하지 않으면 원본 런타임처럼 노드 종류는 WorldNode 로, 모르는 컴포넌트는 건너뛰고 읽는다.
		let mut plain_tree = NodeTree::new();
		let mut plain_document = UIDocument::new();
		let plain_root_id = plain_document.from_json_text(&mut plain_tree, EDITOR_TYPES_SCENE_TEXT).expect("document");
		let plain_field_id = plain_tree.find_child_by_name(plain_root_id, "InputField1").expect("field");
		assert!(!plain_tree[plain_field_id].has_component::<UIInputField>());
		assert!(plain_tree[plain_field_id].has_component::<Paint>());
		let plain_snap_id = plain_tree.find_child_by_name(plain_root_id, "SnapScrollView3").expect("snap");
		assert!(!plain_tree[plain_snap_id].has_component::<UISnapScrollView>());
	}

	//==============================================================================
	/// `#[reflect]` 를 붙인 게임 컴포넌트는 등록 한 줄로 원본 반사 저장 꼴로 쓰고 읽는다. (노드 참조, impl Into 세터, 원본 글 열거, skip, transient)
	//==============================================================================
	#[test]
	fn reflected_game_component_round_trip() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.create_node("root");
		let leader_id = node_tree.create_child(root_id, "leader");
		let follower_id = node_tree.create_child(root_id, "follower");
		let mut follower = Follower::new();
		follower.set_speed(2.5);
		follower.set_label("앞");
		follower.set_tint(Color::create(1.0, 0.0, 0.0, 1.0));
		follower.set_offset(Vector2::create(3.0, 4.0));
		follower.set_text_align(TextAlign::Right);
		follower.set_leader(Some(leader_id));
		follower.set_secret(7);
		follower.set_cache_count(9);
		node_tree.add_component(follower_id, follower);

		let expected_data = serde_json::json!([{
			"Speed": 2.5,
			"Label": "앞",
			"Tint": [1, 0, 0, 1],
			"Offset": [3, 4],
			"TextAlign": "right",
			"Leader": { "$nodeRef": "/leader" },
			"Slot": 0,
			"GizmoVisible": false,
			"Enable": true,
			"ComponentType": "Follower",
			"type": "Follower"
		}]);

		// 등록하지 않아도 원본처럼 반사로 적는다. (type 은 타입 이름) 읽을 때는 건너뛴다.
		let plain_document = UIDocument::new();
		let plain_text = plain_document.to_json_text(&node_tree, root_id, false);
		let plain_data: Value = serde_json::from_str(&plain_text).expect("plain json");
		assert_eq!(plain_data["root"]["children"][1]["components"], expected_data);

		let mut document = UIDocument::new();
		document.register_component_type("Follower", Follower::new);
		let document_text = document.to_json_text(&node_tree, root_id, false);
		assert_eq!(document_text, plain_text);
		let mut loaded_tree = NodeTree::new();
		let loaded_root_id = document.from_json_text(&mut loaded_tree, &document_text).expect("document");
		let loaded_leader_id = loaded_tree.find_child_by_name(loaded_root_id, "leader").expect("leader");
		let loaded_follower_id = loaded_tree.find_child_by_name(loaded_root_id, "follower").expect("follower");
		let loaded_follower = loaded_tree[loaded_follower_id].get_component::<Follower>().expect("follower");
		assert_eq!(loaded_follower.get_speed(), 2.5);
		assert_eq!(loaded_follower.get_label(), "앞");
		assert_eq!(loaded_follower.get_tint(), Color::create(1.0, 0.0, 0.0, 1.0));
		assert_eq!(loaded_follower.get_offset(), Vector2::create(3.0, 4.0));
		assert_eq!(loaded_follower.get_text_align(), TextAlign::Right);
		assert_eq!(loaded_follower.get_leader(), Some(loaded_leader_id));
		assert_eq!(loaded_follower.get_secret(), 0);
		assert_eq!(loaded_follower.get_cache_count(), 0);
		assert_eq!(document.to_json_text(&loaded_tree, loaded_root_id, false), document_text);

		// 편집할 수 있는 속성 목록과 인스펙터 게터. (원본 getReflectedProperties, getComponentProperties)
		let loaded_component: &dyn Component = loaded_tree[loaded_follower_id].get_all_components()[0].as_ref();
		let property_names: Vec<&str> = UIDocument::get_reflected_properties(loaded_component).iter().map(|property| property.name).collect();
		assert_eq!(property_names, ["Speed", "Label", "Tint", "Offset", "TextAlign", "Leader", "Slot", "CachedArea", "GizmoVisible", "Enable", "ComponentType"]);
		let getter_names: Vec<&str> = crate::core::reflection::collect_reflected_getters(loaded_component).expect("getters").iter().map(|getter| getter.name).collect();
		assert_eq!(getter_names, ["isTickNeeded", "getSpeed", "getLabel", "getTint", "getOffset", "getTextAlign", "getLeader", "getSlot", "getCacheCount", "isFollowing"]);
	}

	//==============================================================================
	/// 난수라 두 엔진이 같을 수 없는 값을 지운다. (ShaderSprite 의 EffectSeed, 원본도 Math.random 이다)
	//==============================================================================
	fn clear_random_values(value: &mut Value) {
		match value {
			Value::Object(object_data) => {
				if let Some(seed_value) = object_data.get_mut("EffectSeed") {
					*seed_value = Value::from(0);
				}
				for child_value in object_data.values_mut() {
					clear_random_values(child_value);
				}
			}
			Value::Array(item_values) => {
				for item_value in item_values.iter_mut() {
					clear_random_values(item_value);
				}
			}
			_ => {
			}
		}
	}

	//==============================================================================
	/// 러스트로 지은 노드와 원본 글을 읽어 다시 쓴 노드가 둘 다 원본과 같은지.
	//==============================================================================
	fn assert_built_and_round_trip_match(document: &mut UIDocument, built_node_tree: &NodeTree, built_node_id: NodeId, expected_node_data: &Value, value_path: &str) {
		let mut expected_data = expected_node_data.clone();
		clear_random_values(&mut expected_data);
		let mut built_data = document.serialize_node(built_node_tree, built_node_id);
		clear_random_values(&mut built_data);
		assert_same_document_value(&built_data, &expected_data, &format!("{} 지은 것", value_path));
		let mut read_tree = NodeTree::new();
		let read_id = document.deserialize_node(&mut read_tree, expected_node_data);
		let mut round_trip_data = document.serialize_node(&read_tree, read_id);
		clear_random_values(&mut round_trip_data);
		assert_same_document_value(&round_trip_data, &expected_data, &format!("{} 왕복", value_path));
	}

	//==============================================================================
	/// 원본 에디터가 등록하는 컴포넌트 35 종을 빈 노드에 붙여 쓰면 원본과 같고, 원본 글을 읽어 다시 쓰면 원본과 같다.
	//==============================================================================
	#[test]
	fn editor_components_match_original() {
		let expected_data: Value = serde_json::from_str(EDITOR_COMPONENTS_TEXT).expect("components json");
		let mut document = UIDocument::new();
		document.register_editor_types();
		for (type_name, expected_node_data) in expected_data.as_object().expect("components").iter() {
			let mut node_tree = NodeTree::new();
			let node_id = node_tree.create_node(type_name);
			assert!(document.create_component(&mut node_tree, node_id, type_name).is_some(), "{} 를 지을 수 없다", type_name);
			assert_built_and_round_trip_match(&mut document, &node_tree, node_id, expected_node_data, type_name);
		}
		assert_eq!(expected_data.as_object().map(|components| components.len()), Some(35));
	}

	//==============================================================================
	/// 원본 에디터가 등록하는 노드 종류 12 종을 짓고 쓰면 원본과 같고(원본 생성자가 만든 컴포넌트와 자식까지), 원본 글을 읽어 다시 쓰면 원본과 같다.
	//==============================================================================
	#[test]
	fn editor_node_types_match_original() {
		let expected_data: Value = serde_json::from_str(EDITOR_NODE_TYPES_TEXT).expect("node types json");
		let mut document = UIDocument::new();
		document.register_editor_types();
		for (type_name, expected_node_data) in expected_data.as_object().expect("node types").iter() {
			let mut node_tree = NodeTree::new();
			let node_id = node_tree.create_node(expected_node_data["name"].as_str().unwrap_or(""));
			let node_type = document.node_type_list.iter().find(|registered_type| registered_type.type_name == *type_name).expect("node type");
			assert!((node_type.add_component)(&mut node_tree, node_id).is_some());
			assert_built_and_round_trip_match(&mut document, &node_tree, node_id, expected_node_data, type_name);
		}
		assert_eq!(expected_data.as_object().map(|node_types| node_types.len()), Some(12));
	}

	//==============================================================================
	/// 원본 에디터 팔레트 15 종을 원본 addWidgetNode 와 같은 단계로 지으면, tick 전후 모두 원본 문서와 같고 원본 문서를 읽어 다시 쓰면 같다.
	//==============================================================================
	#[test]
	fn editor_palette_matches_original() {
		let mut document = UIDocument::new();
		document.register_editor_types();
		let mut node_tree = NodeTree::new();
		let tree_root_id = node_tree.get_root_id();
		let document_root_id = node_tree.create_child(tree_root_id, "documentRoot");
		node_tree[document_root_id].set_pivot(Vector2::zero());
		node_tree[document_root_id].set_anchor(Vector2::zero());
		let palette_kinds = [("empty", "Node"), ("panel", "Panel"), ("divider", "Divider"), ("text", "Label"), ("richText", "RichText"), ("image", "Image"), ("button", "Button"), ("toggleButton", "Toggle"), ("inputField", "InputField"), ("progress", "Progress"), ("slider", "Slider"), ("scrollView", "ScrollView"), ("snapScrollView", "SnapScrollView"), ("scrollBar", "ScrollBar"), ("mask", "Mask")];
		let blue = Color::create(0.0, 0.471, 0.831, 1.0);
		let light_gray = Color::create(0.965, 0.965, 0.965, 1.0);
		for (palette_index, (widget_kind, name_prefix)) in palette_kinds.iter().enumerate() {
			let node_id = node_tree.create_child(document_root_id, &format!("{}{}", name_prefix, palette_index + 1));
			if *widget_kind == "inputField" {
				node_tree.add_component(node_id, UIInputField::new());
			}
			else if *widget_kind == "scrollBar" {
				node_tree.add_component(node_id, UIScrollBar::new());
			}
			node_tree[node_id].set_pivot(Vector2::zero());
			node_tree[node_id].set_anchor(Vector2::zero());
			let (content_width, content_height, is_interactable) = match *widget_kind {
				"empty" => {
					(120.0, 80.0, false)
				}
				"panel" => {
					(220.0, 140.0, false)
				}
				"divider" => {
					(260.0, 1.0, false)
				}
				"text" => {
					(200.0, 36.0, false)
				}
				"richText" => {
					(240.0, 40.0, false)
				}
				"image" => {
					(160.0, 160.0, false)
				}
				"button" | "toggleButton" => {
					(180.0, 52.0, true)
				}
				"inputField" => {
					(260.0, 40.0, true)
				}
				"progress" => {
					(260.0, 20.0, false)
				}
				"slider" => {
					(260.0, 24.0, true)
				}
				"scrollView" | "snapScrollView" => {
					(320.0, 220.0, true)
				}
				"scrollBar" => {
					(12.0, 220.0, true)
				}
				_ => {
					(240.0, 180.0, false)
				}
			};
			node_tree[node_id].set_content_size(Vector2::create(content_width, content_height));
			if is_interactable {
				node_tree[node_id].set_interactable(true);
			}
			match *widget_kind {
				"panel" | "divider" | "image" | "button" | "toggleButton" | "scrollView" | "snapScrollView" => {
					let (paint_color, round_size) = match *widget_kind {
						"divider" => {
							(Color::create(0.812, 0.812, 0.812, 1.0), 0.0)
						}
						"image" => {
							(Color::create(0.91, 0.91, 0.91, 1.0), 0.0)
						}
						"panel" => {
							(Color::create(0.91, 0.91, 0.91, 1.0), 16.0)
						}
						"button" | "toggleButton" => {
							(blue, 16.0)
						}
						_ => {
							(light_gray, 16.0)
						}
					};
					let paint = node_tree.add_component(node_id, Paint::new()).expect("paint");
					paint.set_color(paint_color);
					paint.set_round_size(round_size);
				}
				_ => {
				}
			}
			match *widget_kind {
				"text" => {
					let label = node_tree.add_component(node_id, UILabel::new()).expect("label");
					label.set_text("Label");
					label.set_font_size(20.0);
					label.set_text_color(Color::create(0.15, 0.15, 0.15, 1.0));
				}
				"button" | "toggleButton" => {
					if *widget_kind == "button" {
						node_tree.add_component(node_id, UIButton::new());
					}
					else {
						node_tree.add_component(node_id, UIToggleButton::new());
					}
					let label = node_tree.add_component(node_id, UILabel::new()).expect("label");
					label.set_text(if *widget_kind == "button" { "Button" } else { "Toggle" });
					label.set_font_size(18.0);
					label.set_text_color(Color::create(1.0, 1.0, 1.0, 1.0));
				}
				"scrollView" => {
					node_tree.add_component(node_id, UIScrollView::new()).expect("scroll view").set_scroll_content_size(Vector2::create(320.0, 660.0));
				}
				"progress" => {
					let progress_view = node_tree.add_component(node_id, UIProgressView::new()).expect("progress view");
					progress_view.set_range(0.0, 1.0);
					progress_view.set_value(0.6);
				}
				"slider" => {
					let slider = node_tree.add_component(node_id, UISlider::new()).expect("slider");
					slider.set_thumb_radius(9.0);
					slider.set_track_thickness(6.0);
				}
				"richText" => {
					let rich_text = node_tree.add_component(node_id, RichText::new()).expect("rich text");
					rich_text.set_text("Rich Text");
					rich_text.set_font_size(18.0);
				}
				"image" => {
					node_tree.add_component(node_id, UIImageView::new());
				}
				"snapScrollView" => {
					node_tree.add_component(node_id, UISnapScrollView::new()).expect("snap view").set_scroll_content_size(Vector2::create(960.0, 220.0));
				}
				"mask" => {
					node_tree.add_component(node_id, Mask::new());
				}
				_ => {
				}
			}
			let cascade_offset = (palette_index % 8) as f32 * 20.0;
			node_tree[node_id].set_local_position(Vector2::create(60.0 + cascade_offset, 60.0 + cascade_offset));
		}
		let expected_before_tick_data: Value = serde_json::from_str(EDITOR_PALETTE_BEFORE_TICK_TEXT).expect("palette json");
		assert_built_and_round_trip_match(&mut document, &node_tree, document_root_id, &expected_before_tick_data["root"], "팔레트 tick 전");

		// 원본 에디터처럼 한 번 tick 하면 입력 필드의 글 라벨 자리와 스크롤바 썸이 원본과 같게 바뀐다.
		let engine = crate::core::engine::Engine::new(crate::core::engine::EngineConfiguration::new());
		node_tree.tick(&engine, 0.016);
		let expected_data: Value = serde_json::from_str(EDITOR_PALETTE_TEXT).expect("palette json");
		assert_built_and_round_trip_match(&mut document, &node_tree, document_root_id, &expected_data["root"], "팔레트 tick 뒤");
	}

	//==============================================================================
	/// DevTools 인스펙터의 게터 목록은 원본 getComponentProperties 와 같다. (원본 getEngine 은 전역 엔진이 없어 러스트에 없다)
	//==============================================================================
	#[test]
	fn inspector_getters_match_original() {
		let expected_data: Value = serde_json::from_str(INSPECTOR_GETTERS_TEXT).expect("inspector json");
		let mut document = UIDocument::new();
		document.register_editor_types();
		for (type_name, expected_names) in expected_data.as_object().expect("inspector").iter() {
			let mut node_tree = NodeTree::new();
			let node_id = node_tree.create_node(type_name);
			let component_index = document.create_component(&mut node_tree, node_id, type_name).expect("component");
			let component: &dyn Component = node_tree[node_id].get_all_components()[component_index].as_ref();
			let getter_names: Vec<&str> = crate::core::reflection::collect_reflected_getters_in_tree(component, &node_tree).expect("getters").iter().map(|getter| getter.name).collect();
			let expected_getter_names: Vec<&str> = expected_names.as_array().expect("names").iter().filter_map(|name| name.as_str()).filter(|name| *name != "getEngine").collect();
			assert_eq!(getter_names, expected_getter_names, "{}", type_name);
		}
	}

	//==============================================================================
	/// 편집할 수 있는 속성 목록과 반사 저장은 원본 getReflectedProperties, saveByReflection 과 같다. (원본 Engine 은 전역 엔진이 없어 러스트에 없다)
	//==============================================================================
	#[test]
	fn reflected_properties_match_original() {
		let expected_data: Value = serde_json::from_str(REFLECTED_PROPERTIES_TEXT).expect("properties json");
		let mut document = UIDocument::new();
		document.register_editor_types();
		for (type_name, expected_entry) in expected_data.as_object().expect("properties").iter() {
			let mut node_tree = NodeTree::new();
			let node_id = node_tree.create_node(type_name);
			let component_index = document.create_component(&mut node_tree, node_id, type_name).expect("component");
			let component: &dyn Component = node_tree[node_id].get_all_components()[component_index].as_ref();
			let property_names: Vec<&str> = UIDocument::get_reflected_properties(component).iter().map(|property| property.name).collect();
			let expected_property_names: Vec<&str> = expected_entry["properties"].as_array().expect("names").iter().filter_map(|name| name.as_str()).filter(|name| *name != "Engine").collect();
			assert_eq!(property_names, expected_property_names, "{}", type_name);
			let save_context = UIDocumentSaveContext {
				node_tree: &node_tree,
				root_id: node_id,
				node_id,
				component_index,
				prefetched_value_map: None,
			};
			let mut saved_data = Value::Object(UIDocument::save_by_reflection(component, &save_context));
			let mut expected_saved_data = expected_entry["save"].clone();
			clear_random_values(&mut saved_data);
			clear_random_values(&mut expected_saved_data);
			assert_same_document_value(&saved_data, &expected_saved_data, type_name);
		}
	}

	//==============================================================================
	/// 트리를 받는 세터는 읽을 때 트리와 함께 원본처럼 그 자리에서 불리고, &mut self 게터는 &mut 갈래(to_json_text_mut)로 저장되고, 트리를 받는 게터는 인스펙터에 보인다.
	//==============================================================================
	#[test]
	fn tree_setters_and_mutable_getters_are_reflected() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.create_node("root");
		let leader_id = node_tree.create_child(root_id, "leader");
		let follower_id = node_tree.create_child(root_id, "follower");
		let mut follower = Follower::new();
		follower.set_offset(Vector2::create(3.0, 4.0));
		follower.set_leader(Some(leader_id));
		node_tree.add_component(follower_id, follower);
		let mut document = UIDocument::new();
		document.register_component_type("Follower", Follower::new);

		// &self 갈래는 &mut self 게터를 읽지 못해 CachedArea 를 적지 않는다. &mut 갈래는 적는다.
		let shared_data: Value = serde_json::from_str(&document.to_json_text(&node_tree, root_id, false)).expect("shared json");
		assert!(shared_data["root"]["children"][1]["components"][0].get("CachedArea").is_none());
		let mutable_text = document.to_json_text_mut(&mut node_tree, root_id, false);
		let mutable_data: Value = serde_json::from_str(&mutable_text).expect("mutable json");
		assert_eq!(mutable_data["root"]["children"][1]["components"][0]["CachedArea"], serde_json::json!(12));
		assert_eq!(mutable_data["root"]["children"][1]["components"][0]["Slot"], serde_json::json!(0));

		// 트리를 받는 세터는 트리 없이는 넣지 못하고, 읽을 때는 트리와 함께 넣는다.
		let component: &mut dyn Component = node_tree[follower_id].components[0].as_mut();
		assert!(!write_reflected_property(component, "Slot", &ReflectValue::Number(2.0)));
		let mut slot_data = mutable_data.clone();
		slot_data["root"]["children"][1]["components"][0]["Slot"] = serde_json::json!(2);
		let mut loaded_tree = NodeTree::new();
		let loaded_root_id = document.from_json_text(&mut loaded_tree, &slot_data.to_string()).expect("document");
		let loaded_follower_id = loaded_tree.find_child_by_name(loaded_root_id, "follower").expect("follower");
		assert_eq!(loaded_tree[loaded_follower_id].get_component::<Follower>().expect("follower").get_slot(), 2);

		// 인스펙터: 트리 갈래는 트리를 받는 게터까지, &mut 갈래는 &mut self 게터까지 보인다.
		let shared_component: &dyn Component = node_tree[follower_id].get_all_components()[0].as_ref();
		let shared_getter_names: Vec<&str> = crate::core::reflection::collect_reflected_getters(shared_component).expect("getters").iter().map(|getter| getter.name).collect();
		assert!(!shared_getter_names.contains(&"getLeaderName") && !shared_getter_names.contains(&"getCachedArea"));
		let in_tree_getter_list = crate::core::reflection::collect_reflected_getters_in_tree(shared_component, &node_tree).expect("getters");
		assert!(in_tree_getter_list.iter().any(|getter| getter.name == "getLeaderName" && getter.value == ReflectValue::String(String::from("leader"))));
		assert!(crate::core::reflection::has_mutable_getters(shared_component));
		let mutable_getter_list = node_tree.with_component_at_mut(follower_id, 0, |component, tree| crate::core::reflection::collect_reflected_getters_mut(component, tree)).flatten().expect("getters");
		let mutable_getter_names: Vec<&str> = mutable_getter_list.iter().map(|getter| getter.name).collect();
		assert_eq!(mutable_getter_names, ["isTickNeeded", "getSpeed", "getLabel", "getTint", "getOffset", "getTextAlign", "getLeader", "getLeaderName", "getSlot", "getCachedArea", "getCacheCount", "isFollowing"]);
	}
}

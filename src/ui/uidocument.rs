//==============================================================================
// UI 문서. (vanilla.js ui/uidocument.js 의 UIDocument)
// - 노드 트리를 JSON 문서(.scene.json)로 내보내고 읽는다. vanilla.js 비주얼 에디터(tools/visualeditor)가 이 꼴로 씬을 저장하고,
//   게임은 같은 파일을 읽어 NodeTree 에 노드와 컴포넌트를 짓는다. 러스트에서 지은 트리도 같은 꼴로 내보낸다.
// - 문서는 { "version": 1, "root": 노드 } 이고, 노드는 type, name, active, position, scale, rotation, opacity, pivot, anchor,
//   contentSize, interactable, components, children 이다. 위젯(content 노드를 만드는 컴포넌트)이 있으면 content 안의 자식만 contentChildren 으로 따로 둔다.
//   에디터가 얹는 frameSize 같은 모르는 키는 무시한다. 키 차례와 들여쓰기(탭)는 원본 JSON.stringify 와 같게 쓴다. (serde_json preserve_order)
// - 원본과 다른 것:
//   - 원본은 정적 클래스와 모듈 전역 표다. 러스트 판은 표를 든 UIDocument 값을 만들어 쓴다. (엔진처럼 전역 상태를 두지 않는다) new 가 엔진 컴포넌트를 등록해 둔다.
//   - 러스트엔 리플렉션이 없어 원본의 반사 저장(saveByReflection, loadByReflection, getReflectedProperties)이 없다.
//     종류 이름마다 짓기, 쓰기, 읽기 함수를 표에 등록한다. (register_component_type, 위젯은 register_widget_type) 게임도 자기 컴포넌트를 같은 길로 등록한다.
//     원본이 반사로 저장하는 Outline, SoftShade, ShadowText, UIProgressRing 은 원본 반사 결과와 같은 키(머리 대문자: Color, LineWidth, GizmoVisible, Enable, ComponentType 등)로 쓰고 읽는다.
//     러스트 판에 없는 속성(ShadowText 의 TextDirection, FontFace)은 쓰지 않고, 읽을 때도 버린다.
//   - 등록하지 않은 종류는 원본처럼 읽을 때 건너뛰고 경고를 낸다. 쓸 때 원본은 반사로 적지만, 러스트 판은 경고를 내고 뺀다.
//   - ShadowImage 는 원본 표에 없다. 러스트 판은 원본이 반사로 적을 꼴(ImageColor, GizmoVisible, Enable, ComponentType)에 Sprite 처럼 imagePath 를 더해 등록한다.
//   - 노드 종류는 WorldNode 하나뿐이라 노드 종류 표(registerNodeType)가 없다. type 은 늘 "WorldNode" 로 적고, 읽을 때는 보지 않는다.
//   - 원본 getComponentType(이름 → 생성자)은 에디터의 컴포넌트 더하기 메뉴용이라 옮기지 않았다. 이름 목록(get_component_type_names)만 둔다.
//   - 그림 경로(imagePath)는 원본처럼 이미지 해석기(set_image_resolver)로 찾는다. 해석기를 주지 않으면 문서가 든 애셋 창고에서 찾는다.
//     load_images 가 문서의 imagePath 를 ImageAsset 으로 요청하고(받은 읽기 손잡이는 씬이 track_load_handle 로 맡길 수 있다),
//     아직 읽히지 않은 그림은 bind_pending_images 가 다 읽힌 뒤 붙인다. (러스트 애셋은 요청 뒤 나중에 다 읽힌다)
//   - 노드 참조($nodeRef)의 기준 뿌리는 to_json_text, from_json_text 의 뿌리다. serialize_node, deserialize_node 를 바로 부르면 그 노드가 뿌리다.
//     (원본은 serializeNode 만 부르면 참조를 적지 않고, deserializeNode 만 부르면 참조를 잇지 않는다)
//   - 수는 f32 를 가장 짧은 십진으로 적는다. (0.22 는 0.22, 정수는 소수점 없이) 원본 f64 의 끝자리(0.1 + 0.2 같은)는 f32 로 줄어든다.
// - 원본 버그를 고친 것: Sprite 의 nineSlice 를 원본은 쓰기만 하고 읽지 않았다. 러스트 판은 원본이 쓰는 꼴([왼쪽, 위, 왼쪽 + 오른쪽, 위 + 아래], Rect 의 left, top, right, bottom)을 거꾸로 풀어 읽는다.
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
use crate::resource::imageasset::ImageAsset;
use crate::ui::uibutton::UIButton;
use crate::ui::uiimageview::UIImageView;
use crate::ui::uilabel::UILabel;
use crate::ui::uiprogressring::UIProgressRing;
use crate::ui::uiprogressview::UIProgressView;
use crate::ui::uiscrollview::{ScrollMode, UIScrollView};
use crate::ui::uislider::UISlider;
use crate::ui::uitogglebutton::UIToggleButton;
use serde_json::{Map, Value};
use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// UI 문서 포맷 버전.
pub const UIDOCUMENT_VERSION: u32 = 1;

// 노드 참조를 문서에 적을 때 쓰는 표시.
const NODE_REFERENCE_KEY: &str = "$nodeRef";

// 문서에 적는 노드 종류 이름. (러스트 판의 노드는 WorldNode 하나다)
const NODE_TYPE_NAME: &str = "WorldNode";

// 정수로 적을 수 있는 가장 큰 크기. (JSON 수를 f64 로 읽어도 정확한 정수 범위, 2 의 53 제곱)
const MAXIMUM_SAFE_INTEGER: f64 = 9007199254740992.0;

// 가로 정렬과 원본 이름(Canvas2D textAlign)의 짝.
const TEXT_ALIGN_NAME_TABLE: [(TextAlign, &str); 5] = [
	(TextAlign::Left, "left"),
	(TextAlign::Center, "center"),
	(TextAlign::Right, "right"),
	(TextAlign::Start, "start"),
	(TextAlign::End, "end"),
];

// 세로 기준선과 원본 이름(Canvas2D textBaseline)의 짝.
const TEXT_BASELINE_NAME_TABLE: [(TextBaseline, &str); 6] = [
	(TextBaseline::Top, "top"),
	(TextBaseline::Middle, "middle"),
	(TextBaseline::Bottom, "bottom"),
	(TextBaseline::Ideographic, "ideographic"),
	(TextBaseline::Hanging, "hanging"),
	(TextBaseline::Alphabetic, "alphabetic"),
];

// 스프라이트 출력 방식과 원본 이름(SpriteDrawMode)의 짝.
const SPRITE_DRAW_MODE_NAME_TABLE: [(SpriteDrawMode, &str); 3] = [
	(SpriteDrawMode::Simple, "simple"),
	(SpriteDrawMode::Sliced, "sliced"),
	(SpriteDrawMode::Tiled, "tiled"),
];

// 스크롤 방식과 원본 이름(ScrollMode)의 짝.
const SCROLL_MODE_NAME_TABLE: [(ScrollMode, &str); 2] = [
	(ScrollMode::Clamp, "clamp"),
	(ScrollMode::Elastic, "elastic"),
];

// 컴포넌트 종류 하나를 짓는 함수. (노드에 붙이고 컴포넌트 차례 번호를 돌려준다)
type AddComponentFunction = Box<dyn Fn(&mut NodeTree, NodeId) -> Option<usize>>;
// 컴포넌트 하나를 문서 값으로 쓰는 함수.
type SaveComponentFunction = Box<dyn Fn(&dyn Component, &UIDocumentSaveContext) -> Map<String, Value>>;
// 문서 값을 컴포넌트 하나에 읽어 넣는 함수.
type LoadComponentFunction = Box<dyn Fn(&mut dyn Component, &Map<String, Value>, &mut UIDocumentLoadContext)>;
// 위젯의 content 노드를 읽는 함수.
type ReadContentFunction = Box<dyn Fn(&dyn Component) -> Option<NodeId>>;
// 찾은 그림을 컴포넌트에 넣는 함수.
type ApplyImageFunction = Box<dyn Fn(&mut dyn Component, Rc<Image>)>;
// 찾은 노드를 컴포넌트에 넣는 함수.
type ApplyNodeFunction = Box<dyn Fn(&mut dyn Component, NodeId)>;

//==============================================================================
// 컴포넌트 종류 하나. (원본 COMPONENT_TYPE_TABLE 의 생성자와 COMPONENT_PROPERTY_TABLE 의 save, load 를 한데 모은 것)
//==============================================================================
struct UIDocumentComponentType {
	type_name: String,
	type_id: TypeId,
	add_component: AddComponentFunction,
	save_component: SaveComponentFunction,
	load_component: LoadComponentFunction,
	read_content: Option<ReadContentFunction>,
	generated_type_names: Vec<String>,
}

//==============================================================================
// 아직 붙이지 못한 그림 하나. (노드, 그 노드의 컴포넌트 차례, 그림 경로)
//==============================================================================
struct PendingImage {
	node_id: NodeId,
	component_index: usize,
	image_path: String,
	apply_image: ApplyImageFunction,
}

//==============================================================================
// 트리를 다 지은 뒤에 이을 노드 참조 하나. (원본 pendingNodeReferenceList 의 항목)
//==============================================================================
struct PendingNodeReference {
	node_id: NodeId,
	component_index: usize,
	path: String,
	apply_node: ApplyNodeFunction,
}

//==============================================================================
// 그림 찾기 결과.
//==============================================================================
enum ImageLookup {
	Found(Rc<Image>),
	Waiting,
	Missing,
}

//==============================================================================
// 쓰기 문맥. (컴포넌트 쓰기 함수가 받는다. 노드 참조를 뿌리 기준 경로로 적는다)
//==============================================================================
pub struct UIDocumentSaveContext<'a> {
	node_tree: &'a NodeTree,
	root_id: NodeId,
}

impl<'a> UIDocumentSaveContext<'a> {
	//==============================================================================
	// 노드 트리.
	//==============================================================================
	pub fn get_node_tree(&self) -> &NodeTree {
		self.node_tree
	}

	//==============================================================================
	// 노드 참조 적기. ({ "$nodeRef": 뿌리 기준 경로 }, None 이거나 뿌리 밖의 노드면 적지 않는다. 원본 saveNodeReferences)
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
// 읽기 문맥. (컴포넌트 읽기 함수가 받는다. 그림과 노드 참조는 여기에 맡겨 두고 트리를 다 지은 뒤에 넣는다)
//==============================================================================
pub struct UIDocumentLoadContext<'a> {
	node_id: NodeId,
	component_index: usize,
	pending_image_list: &'a mut Vec<PendingImage>,
	pending_node_reference_list: &'a mut Vec<PendingNodeReference>,
}

impl<'a> UIDocumentLoadContext<'a> {
	//==============================================================================
	// 짓는 중인 노드.
	//==============================================================================
	pub fn get_node_id(&self) -> NodeId {
		self.node_id
	}

	//==============================================================================
	// 그림 맡기기. (경로의 그림을 찾으면 apply_image 로 이 컴포넌트에 넣는다. 원본 UIDocument.resolveImage + setImage)
	// - 지금 찾을 수 있는 그림은 트리를 다 지은 자리에서, 아직 읽히는 그림은 bind_pending_images 에서 넣는다.
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
	// 노드 참조 맡기기. (값이 { "$nodeRef": 경로 } 면 트리를 다 지은 뒤 그 노드를 apply_node 로 넣는다. 원본 loadNodeReferences)
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
}

//==============================================================================
// UI 문서. (WorldNode 트리 ↔ JSON 상호 변환)
//==============================================================================
pub struct UIDocument {
	component_type_list: Vec<UIDocumentComponentType>,
	image_resolver: Option<Box<dyn FnMut(&str) -> Option<Rc<Image>>>>,
	image_asset_map: HashMap<String, ImageAsset>,
	pending_image_list: Vec<PendingImage>,
}

impl UIDocument {
	//==============================================================================
	// 생성. (원본 표의 엔진 컴포넌트를 등록해 둔다)
	//==============================================================================
	pub fn new() -> UIDocument {
		let mut document = UIDocument {
			component_type_list: Vec::new(),
			image_resolver: None,
			image_asset_map: HashMap::new(),
			pending_image_list: Vec::new(),
		};
		document.register_engine_component_types();
		document
	}

	//==============================================================================
	// 엔진 컴포넌트 등록. (원본 COMPONENT_TYPE_TABLE 차례, 끝에 러스트 판이 더한 ShadowImage)
	//==============================================================================
	fn register_engine_component_types(&mut self) {
		self.register_component_type("Paint", Paint::new, save_paint, load_paint);
		self.register_component_type("Sprite", Sprite::new, save_sprite, load_sprite);
		self.register_component_type("Text", Text::new, save_text, load_text);
		self.register_component_type("RichText", RichText::new, save_rich_text, load_rich_text);
		self.register_component_type("Mask", Mask::new, save_mask, load_mask);
		self.register_component_type("Outline", Outline::new, save_outline, load_outline);
		self.register_component_type("SoftShade", SoftShade::new, save_soft_shade, load_soft_shade);
		self.register_component_type("ShadowText", ShadowText::new, save_shadow_text, load_shadow_text);
		self.register_component_type("UIProgressRing", UIProgressRing::new, save_progress_ring, load_progress_ring);
		self.register_widget_type("UILabel", UILabel::new, save_label, load_label, UILabel::get_content, &[]);
		self.register_widget_type("UIImageView", UIImageView::new, save_image_view, load_image_view, UIImageView::get_content, &[]);
		self.register_widget_type("UIButton", UIButton::new, save_button, load_button, UIButton::get_content, &[]);
		self.register_widget_type("UIToggleButton", UIToggleButton::new, save_toggle_button, load_toggle_button, UIToggleButton::get_content, &[]);
		self.register_widget_type("UIProgressView", UIProgressView::new, save_progress_view, load_progress_view, UIProgressView::get_content, &[]);
		self.register_widget_type("UISlider", UISlider::new, save_slider, load_slider, UISlider::get_content, &[]);
		self.register_widget_type("UIScrollView", UIScrollView::new, save_scroll_view, load_scroll_view, UIScrollView::get_content, &["Mask"]);
		self.register_component_type("ShadowImage", ShadowImage::new, save_shadow_image, load_shadow_image);
	}

	//==============================================================================
	// 컴포넌트 종류 등록. (원본 registerComponentType 과 COMPONENT_PROPERTY_TABLE 의 한 줄, 같은 이름이면 바꾼다)
	// - create_component 로 짓고 node_tree.add_component 로 붙인 뒤 load_component 로 문서 값을 넣는다. (원본처럼 붙인 다음 읽는다)
	// - save_component 가 돌려준 값 끝에 "type" 을 더해 적는다.
	//==============================================================================
	pub fn register_component_type<T: Component + 'static>(&mut self, type_name: &str, create_component: fn() -> T, save_component: fn(&T, &UIDocumentSaveContext) -> Map<String, Value>, load_component: fn(&mut T, &Map<String, Value>, &mut UIDocumentLoadContext)) {
		let component_type = UIDocument::create_component_type(type_name, create_component, save_component, load_component);
		self.insert_component_type(component_type);
	}

	//==============================================================================
	// 위젯 종류 등록. (원본의 UIView 계열. 붙을 때 content 노드를 만드는 컴포넌트)
	// - read_content 가 돌려준 content 노드는 문서에 적지 않고, 그 안의 자식만 contentChildren 으로 적는다. 읽을 때 위젯이 다시 만든 content 에 붙인다.
	// - generated_type_names 는 위젯이 붙을 때 호스트에 스스로 붙이는 컴포넌트 종류다. (원본 require, 문서에 적지 않는다)
	//==============================================================================
	pub fn register_widget_type<T: Component + 'static>(&mut self, type_name: &str, create_component: fn() -> T, save_component: fn(&T, &UIDocumentSaveContext) -> Map<String, Value>, load_component: fn(&mut T, &Map<String, Value>, &mut UIDocumentLoadContext), read_content: fn(&T) -> Option<NodeId>, generated_type_names: &[&str]) {
		let mut component_type = UIDocument::create_component_type(type_name, create_component, save_component, load_component);
		component_type.read_content = Some(Box::new(move |component: &dyn Component| {
			component.as_any().downcast_ref::<T>().and_then(read_content)
		}));
		component_type.generated_type_names = generated_type_names.iter().map(|generated_type_name| String::from(*generated_type_name)).collect();
		self.insert_component_type(component_type);
	}

	//==============================================================================
	// 컴포넌트 종류 하나 만들기. (구체 타입 함수를 컴포넌트 트레이트 객체로 다루는 함수로 감싼다)
	//==============================================================================
	fn create_component_type<T: Component + 'static>(type_name: &str, create_component: fn() -> T, save_component: fn(&T, &UIDocumentSaveContext) -> Map<String, Value>, load_component: fn(&mut T, &Map<String, Value>, &mut UIDocumentLoadContext)) -> UIDocumentComponentType {
		UIDocumentComponentType {
			type_name: String::from(type_name),
			type_id: TypeId::of::<T>(),
			add_component: Box::new(move |node_tree: &mut NodeTree, node_id: NodeId| {
				node_tree.add_component(node_id, create_component())?;
				let component_count = node_tree.get_node(node_id)?.get_all_components().len();
				Some(component_count - 1)
			}),
			save_component: Box::new(move |component: &dyn Component, save_context: &UIDocumentSaveContext| {
				match component.as_any().downcast_ref::<T>() {
					Some(typed_component) => {
						save_component(typed_component, save_context)
					}
					None => {
						Map::new()
					}
				}
			}),
			load_component: Box::new(move |component: &mut dyn Component, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext| {
				if let Some(typed_component) = component.as_any_mut().downcast_mut::<T>() {
					load_component(typed_component, component_data, load_context);
				}
			}),
			read_content: None,
			generated_type_names: Vec::new(),
		}
	}

	//==============================================================================
	// 컴포넌트 종류 넣기. (같은 이름이 있으면 그 자리에서 바꾸고, 없으면 끝에 더한다. 원본 객체 키 대입과 같은 차례)
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
	// 컴포넌트에 맞는 종류 찾기. (같은 타입을 여러 이름으로 등록했으면 나중 것)
	//==============================================================================
	fn find_component_type(&self, component: &dyn Component) -> Option<&UIDocumentComponentType> {
		let component_type_id = component.as_any().type_id();
		self.component_type_list.iter().rev().find(|registered_type| registered_type.type_id == component_type_id)
	}

	//==============================================================================
	// 지원 컴포넌트 종류 이름 목록. (원본 getComponentTypeNames, 등록 차례)
	//==============================================================================
	pub fn get_component_type_names(&self) -> Vec<&str> {
		self.component_type_list.iter().map(|registered_type| registered_type.type_name.as_str()).collect()
	}

	//==============================================================================
	// 이미지 경로 해석기 등록. (원본 setImageResolver, 경로 → 그림. 등록하면 애셋 창고 대신 이것으로 찾는다)
	//==============================================================================
	pub fn set_image_resolver(&mut self, image_resolver: impl FnMut(&str) -> Option<Rc<Image>> + 'static) {
		self.image_resolver = Some(Box::new(image_resolver));
	}

	//==============================================================================
	// 이미지 경로 해석. (원본 resolveImage, 찾지 못했거나 아직 읽히는 중이면 None)
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
	// 그림 찾기. (해석기가 있으면 해석기, 없으면 load_images 로 요청한 애셋)
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
	// 문서의 그림 읽기 요청. (imagePath 마다 ImageAsset 을 요청해 창고에 든다. 이미 요청한 경로는 건너뛴다)
	// - 새로 요청한 애셋의 읽기 손잡이를 돌려준다. 씬이 create 에서 부르면 track_load_handle 로 맡겨 initialize 를 기다리게 할 수 있다.
	// - 데스크탑은 그 자리에서 읽히고, 웹은 나중에 읽힌다. 읽히기 전에 지은 노드의 그림은 bind_pending_images 가 붙인다.
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
	// 맡아 둔 그림 붙이기. (찾은 그림은 넣고, 아직 읽히는 그림은 남긴다. 남은 수를 돌려준다)
	// - from_json_text, deserialize_node 가 끝에 한 번 부른다. 웹처럼 그림이 나중에 읽히면 씬의 tick 에서 남은 수가 0 이 될 때까지 부른다.
	// - 찾지 못한 그림(해석기가 None, 애셋이 없거나 실패)은 경고를 내고 버린다. 그 컴포넌트는 원본처럼 그림 없이 남는다.
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
	// 노드 트리 → 문서 데이터. (원본 serializeNode, 노드 참조는 이 노드 기준 경로로 적는다)
	//==============================================================================
	pub fn serialize_node(&self, node_tree: &NodeTree, node_id: NodeId) -> Value {
		self.serialize_node_from_root(node_tree, node_id, node_id)
	}

	//==============================================================================
	// 노드 트리 → 문서 데이터. (재귀, 노드 참조는 root_id 기준 경로)
	//==============================================================================
	fn serialize_node_from_root(&self, node_tree: &NodeTree, node_id: NodeId, root_id: NodeId) -> Value {
		let node = match node_tree.get_node(node_id) {
			Some(node) => {
				node
			}
			None => {
				return Value::Null;
			}
		};
		let mut node_data = Map::new();
		write_value(&mut node_data, "type", Value::from(NODE_TYPE_NAME));
		write_value(&mut node_data, "name", Value::from(node.get_name()));
		write_value(&mut node_data, "active", Value::from(node.is_active()));
		write_value(&mut node_data, "position", UIDocument::vector2_to_array(node.get_local_position()));
		write_value(&mut node_data, "scale", UIDocument::vector2_to_array(node.get_local_scale()));
		write_value(&mut node_data, "rotation", UIDocument::number_to_value(node.get_local_rotation()));
		write_value(&mut node_data, "opacity", UIDocument::number_to_value(node.get_local_opacity()));
		write_value(&mut node_data, "pivot", UIDocument::vector2_to_array(node.get_pivot()));
		write_value(&mut node_data, "anchor", UIDocument::vector2_to_array(node.get_anchor()));
		write_value(&mut node_data, "contentSize", UIDocument::vector2_to_array(node.get_content_size()));
		write_value(&mut node_data, "interactable", Value::from(node.is_interactable()));

		// 위젯이 스스로 붙인 컴포넌트는 적지 않는다. (읽을 때 다시 붙는다)
		let save_context = UIDocumentSaveContext {
			node_tree,
			root_id,
		};
		let component_list = node.get_all_components();
		let generated_type_set = self.collect_generated_component_types(component_list);
		let mut widget_content_id_set: HashSet<NodeId> = HashSet::new();
		let mut component_data_list: Vec<Value> = Vec::new();
		for component in component_list.iter() {
			let component_reference: &dyn Component = component.as_ref();
			let component_type = match self.find_component_type(component_reference) {
				Some(component_type) => {
					component_type
				}
				None => {
					eprintln!("UIDocument: 등록되지 않은 컴포넌트라 적지 않습니다 ({}, 노드 {})", component_reference.get_component_type(), node.get_name());
					continue;
				}
			};
			if let Some(read_content) = &component_type.read_content {
				if let Some(content_id) = read_content(component_reference) {
					widget_content_id_set.insert(content_id);
				}
			}
			if generated_type_set.contains(&component_type.type_name) {
				continue;
			}
			let mut component_data = (component_type.save_component)(component_reference, &save_context);
			write_value(&mut component_data, "type", Value::from(component_type.type_name.as_str()));
			component_data_list.push(Value::Object(component_data));
		}
		write_value(&mut node_data, "components", Value::Array(component_data_list));

		// 위젯의 content 노드는 읽을 때 위젯이 다시 만든다. 그 안의 자식만 남긴다.
		let mut child_data_list: Vec<Value> = Vec::new();
		let mut content_child_data_list: Option<Vec<Value>> = None;
		for child_id in node.get_child_ids().iter() {
			if widget_content_id_set.contains(child_id) {
				let grand_child_ids = node_tree.get_child_ids(*child_id);
				if !grand_child_ids.is_empty() {
					content_child_data_list = Some(grand_child_ids.iter().map(|grand_child_id| self.serialize_node_from_root(node_tree, *grand_child_id, root_id)).collect());
				}
				continue;
			}
			child_data_list.push(self.serialize_node_from_root(node_tree, *child_id, root_id));
		}
		write_value(&mut node_data, "children", Value::Array(child_data_list));
		if let Some(content_child_data_list) = content_child_data_list {
			write_value(&mut node_data, "contentChildren", Value::Array(content_child_data_list));
		}
		Value::Object(node_data)
	}

	//==============================================================================
	// 컴포넌트 하나 → 문서 데이터. (등록한 종류면 쓰기 함수가 돌려준 값, "type" 은 더하지 않는다. 등록하지 않은 종류면 경고 없이 None)
	// - 노드 참조는 root_id 기준 경로로 적는다. (DevTools 인스펙터가 컴포넌트 속성을 보일 때 쓴다)
	//==============================================================================
	pub fn serialize_component(&self, node_tree: &NodeTree, root_id: NodeId, component: &dyn Component) -> Option<Map<String, Value>> {
		let component_type = self.find_component_type(component)?;
		let save_context = UIDocumentSaveContext {
			node_tree,
			root_id,
		};
		Some((component_type.save_component)(component, &save_context))
	}

	//==============================================================================
	// 위젯이 붙인 컴포넌트 종류 이름 모으기. (원본 collectGeneratedComponentTypes, 원본 require 자리는 등록할 때 준 목록)
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
	// 문서 데이터 → 노드 트리. (원본 deserializeNode, 어느 부모에도 붙지 않은 노드를 돌려준다)
	// - 노드 참조는 이 노드 기준 경로로 잇고, 찾을 수 있는 그림을 넣는다.
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
	// 노드 하나와 그 자손 짓기. (재귀)
	//==============================================================================
	fn build_node(&mut self, node_tree: &mut NodeTree, node_data: &Value, pending_node_reference_list: &mut Vec<PendingNodeReference>) -> NodeId {
		let empty_node_data = Map::new();
		let node_object = node_data.as_object().unwrap_or(&empty_node_data);
		let node_id = node_tree.create_node(read_string(node_object, "name").unwrap_or(""));
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

		// 컴포넌트. (붙인 다음 문서 값을 넣는다)
		let mut widget_content_id: Option<NodeId> = None;
		if let Some(component_data_list) = node_object.get("components").and_then(|value| value.as_array()) {
			for component_data in component_data_list.iter() {
				let component_object = match component_data.as_object() {
					Some(component_object) => {
						component_object
					}
					None => {
						continue;
					}
				};
				let type_name = read_string(component_object, "type").unwrap_or("");
				let component_type = match self.component_type_list.iter().find(|registered_type| registered_type.type_name == type_name) {
					Some(component_type) => {
						component_type
					}
					None => {
						eprintln!("UIDocument: 등록되지 않은 컴포넌트 종류를 건너뜁니다 ({})", type_name);
						continue;
					}
				};
				let component_index = match (component_type.add_component)(node_tree, node_id) {
					Some(component_index) => {
						component_index
					}
					None => {
						continue;
					}
				};
				let mut load_context = UIDocumentLoadContext {
					node_id,
					component_index,
					pending_image_list: &mut self.pending_image_list,
					pending_node_reference_list,
				};
				let component: &mut dyn Component = node_tree[node_id].components[component_index].as_mut();
				(component_type.load_component)(component, component_object, &mut load_context);
				if let Some(read_content) = &component_type.read_content {
					if let Some(content_id) = read_content(node_tree[node_id].components[component_index].as_ref()) {
						widget_content_id = Some(content_id);
					}
				}
			}
		}

		// 자식.
		if let Some(child_data_list) = node_object.get("children").and_then(|value| value.as_array()) {
			for child_data in child_data_list.iter() {
				let child_id = self.build_node(node_tree, child_data, pending_node_reference_list);
				node_tree.add_child(node_id, child_id);
			}
		}

		// 위젯 content 안의 자식. (스크롤 뷰의 목록 항목 같은 것)
		if let Some(content_id) = widget_content_id {
			if let Some(content_child_data_list) = node_object.get("contentChildren").and_then(|value| value.as_array()) {
				for content_child_data in content_child_data_list.iter() {
					let content_child_id = self.build_node(node_tree, content_child_data, pending_node_reference_list);
					node_tree.add_child(content_id, content_child_id);
				}
			}
		}
		node_id
	}

	//==============================================================================
	// 노드 트리 → JSON 글. (원본 toJsonText, is_pretty 면 탭으로 들여 쓴다)
	//==============================================================================
	pub fn to_json_text(&self, node_tree: &NodeTree, root_id: NodeId, is_pretty: bool) -> String {
		let mut document_data = Map::new();
		write_value(&mut document_data, "version", Value::from(UIDOCUMENT_VERSION));
		write_value(&mut document_data, "root", self.serialize_node_from_root(node_tree, root_id, root_id));
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
	// JSON 글 → 노드 트리. (원본 fromJsonText, 어느 부모에도 붙지 않은 뿌리를 돌려준다. 씬에 보이려면 add_child 로 붙인다)
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
	// 뿌리 기준 절대 경로 계산. (원본 findNodePath, "/" + 이름들. 뿌리 밖의 노드면 None)
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
	// 뿌리 기준 절대 경로로 노드 찾기. (원본 findNodeByPath, 같은 이름이면 앞의 자식)
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
	// 수 → 문서 값. (f32 를 가장 짧은 십진으로, 정수면 소수점 없이. 원본 JSON.stringify 와 같은 글이 나온다. 수가 아니면 null)
	//==============================================================================
	pub fn number_to_value(number: f32) -> Value {
		if !number.is_finite() {
			return Value::Null;
		}
		let shortest_number = format!("{}", number).parse::<f64>().unwrap_or(number as f64);
		if shortest_number.fract() == 0.0 && shortest_number.abs() < MAXIMUM_SAFE_INTEGER {
			return Value::from(shortest_number as i64);
		}
		match serde_json::Number::from_f64(shortest_number) {
			Some(json_number) => {
				Value::Number(json_number)
			}
			None => {
				Value::Null
			}
		}
	}

	//==============================================================================
	// 색 → 배열. (원본 colorToArray, [r, g, b, a])
	//==============================================================================
	pub fn color_to_array(color: Color) -> Value {
		Value::Array(vec![UIDocument::number_to_value(color.red), UIDocument::number_to_value(color.green), UIDocument::number_to_value(color.blue), UIDocument::number_to_value(color.alpha)])
	}

	//==============================================================================
	// 배열 → 색. (원본 arrayToColor, 수 넷이 아니면 None)
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
	// 벡터 → 배열. ([x, y])
	//==============================================================================
	pub fn vector2_to_array(vector: Vector2) -> Value {
		Value::Array(vec![UIDocument::number_to_value(vector.x), UIDocument::number_to_value(vector.y)])
	}

	//==============================================================================
	// 배열 → 벡터. (수 둘이 아니면 None)
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
	fn default() -> UIDocument {
		UIDocument::new()
	}
}

//==============================================================================
// 문서 값 하나 적기.
//==============================================================================
fn write_value(object_data: &mut Map<String, Value>, key: &str, value: Value) {
	object_data.insert(String::from(key), value);
}

//==============================================================================
// 수 읽기.
//==============================================================================
fn read_number(object_data: &Map<String, Value>, key: &str) -> Option<f32> {
	object_data.get(key).and_then(|value| value.as_f64()).map(|number| number as f32)
}

//==============================================================================
// 참거짓 읽기.
//==============================================================================
fn read_bool(object_data: &Map<String, Value>, key: &str) -> Option<bool> {
	object_data.get(key).and_then(|value| value.as_bool())
}

//==============================================================================
// 글 읽기.
//==============================================================================
fn read_string<'a>(object_data: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
	object_data.get(key).and_then(|value| value.as_str())
}

//==============================================================================
// 색 읽기.
//==============================================================================
fn read_color(object_data: &Map<String, Value>, key: &str) -> Option<Color> {
	object_data.get(key).and_then(UIDocument::array_to_color)
}

//==============================================================================
// 벡터 읽기.
//==============================================================================
fn read_vector2(object_data: &Map<String, Value>, key: &str) -> Option<Vector2> {
	object_data.get(key).and_then(UIDocument::array_to_vector2)
}

//==============================================================================
// 이름표에서 값의 이름 찾기.
//==============================================================================
fn find_name<T: PartialEq + Copy>(name_table: &[(T, &'static str)], value: T) -> &'static str {
	name_table.iter().find(|(table_value, _)| *table_value == value).map(|(_, name)| *name).unwrap_or("")
}

//==============================================================================
// 이름표에서 이름의 값 찾기.
//==============================================================================
fn find_value<T: Copy>(name_table: &[(T, &'static str)], name: &str) -> Option<T> {
	name_table.iter().find(|(_, table_name)| *table_name == name).map(|(value, _)| *value)
}

//==============================================================================
// 문서 안의 그림 경로 모으기. (imagePath 키의 빈 글이 아닌 값, 같은 경로는 한 번)
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
// 두 칸 들여쓰기를 탭으로. (serde_json 의 보기 좋은 글은 두 칸이다. JSON 글 안에는 줄바꿈 글자가 없어 줄 머리의 빈칸은 모두 들여쓰기다)
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
// 컴포넌트 공통 상태 적기. (원본 반사 저장 꼴의 끝 세 키)
//==============================================================================
fn write_component_base(component_data: &mut Map<String, Value>, component: &dyn Component) {
	write_value(component_data, "GizmoVisible", Value::from(component.is_gizmo_visible()));
	write_value(component_data, "Enable", Value::from(component.is_enable()));
	write_value(component_data, "ComponentType", Value::from(component.get_component_type()));
}

//==============================================================================
// 컴포넌트 공통 상태 읽기. (ComponentType 은 러스트 판에서 타입이 정하므로 읽지 않는다)
//==============================================================================
fn read_component_base(component: &mut dyn Component, component_data: &Map<String, Value>) {
	if let Some(is_gizmo_visible) = read_bool(component_data, "GizmoVisible") {
		component.set_gizmo_visible(is_gizmo_visible);
	}
	if let Some(is_enable) = read_bool(component_data, "Enable") {
		component.set_enable(is_enable);
	}
}

//==============================================================================
// Paint 쓰기.
//==============================================================================
fn save_paint(paint: &Paint, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "color", UIDocument::color_to_array(paint.get_color()));
	write_value(&mut component_data, "roundSize", UIDocument::number_to_value(paint.get_round_size()));
	component_data
}

//==============================================================================
// Paint 읽기.
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
// Sprite 쓰기. (그림은 경로만 적는다. nineSlice 는 원본처럼 Rect 의 left, top, right, bottom)
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
// Sprite 읽기.
//==============================================================================
fn load_sprite(sprite: &mut Sprite, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext) {
	if let Some(color) = read_color(component_data, "color") {
		sprite.set_color(color);
	}
	if let Some(round_size) = read_number(component_data, "roundSize") {
		sprite.set_round_size(round_size);
	}
	if let Some(image_path) = read_string(component_data, "imagePath") {
		if !image_path.is_empty() {
			sprite.set_image_path(image_path);
			load_context.bind_image::<Sprite>(image_path, |sprite, image| sprite.set_image(Some(image)));
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
}

//==============================================================================
// 글 속성 쓰기. (원본 saveTextProperties, Text 와 RichText 가 같이 쓴다)
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
// 글 속성 읽기. (원본 loadTextProperties)
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
// Text 쓰기.
//==============================================================================
fn save_text(text: &Text, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	save_text_properties(text)
}

//==============================================================================
// Text 읽기.
//==============================================================================
fn load_text(text: &mut Text, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	load_text_properties(text, component_data);
}

//==============================================================================
// RichText 쓰기. (품은 Text 의 속성, 글은 마크업이 든 원래 글)
//==============================================================================
fn save_rich_text(rich_text: &RichText, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	save_text_properties(rich_text.get_text_component())
}

//==============================================================================
// RichText 읽기. (글은 RichText 의 set_text 로 다시 넣어 마크업을 푼다)
//==============================================================================
fn load_rich_text(rich_text: &mut RichText, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	load_text_properties(rich_text.get_text_component_mut(), component_data);
	if let Some(text_string) = read_string(component_data, "text") {
		rich_text.set_text(text_string);
	}
}

//==============================================================================
// Mask 쓰기. (원본처럼 적는 속성이 없다)
//==============================================================================
fn save_mask(_mask: &Mask, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	Map::new()
}

//==============================================================================
// Mask 읽기.
//==============================================================================
fn load_mask(_mask: &mut Mask, _component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
}

//==============================================================================
// Outline 쓰기. (원본 반사 저장 꼴)
//==============================================================================
fn save_outline(outline: &Outline, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "Color", UIDocument::color_to_array(outline.get_color()));
	write_value(&mut component_data, "LineWidth", UIDocument::number_to_value(outline.get_line_width()));
	write_value(&mut component_data, "RoundSize", UIDocument::number_to_value(outline.get_round_size()));
	write_value(&mut component_data, "Circle", Value::from(outline.is_circle()));
	write_component_base(&mut component_data, outline);
	component_data
}

//==============================================================================
// Outline 읽기.
//==============================================================================
fn load_outline(outline: &mut Outline, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(color) = read_color(component_data, "Color") {
		outline.set_color(color);
	}
	if let Some(line_width) = read_number(component_data, "LineWidth") {
		outline.set_line_width(line_width);
	}
	if let Some(round_size) = read_number(component_data, "RoundSize") {
		outline.set_round_size(round_size);
	}
	if let Some(is_circle) = read_bool(component_data, "Circle") {
		outline.set_circle(is_circle);
	}
	read_component_base(outline, component_data);
}

//==============================================================================
// SoftShade 쓰기. (원본 반사 저장 꼴. 짝 맞는 get/set 이 없어 공통 상태만 적힌다)
//==============================================================================
fn save_soft_shade(soft_shade: &SoftShade, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_component_base(&mut component_data, soft_shade);
	component_data
}

//==============================================================================
// SoftShade 읽기.
//==============================================================================
fn load_soft_shade(soft_shade: &mut SoftShade, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	read_component_base(soft_shade, component_data);
}

//==============================================================================
// ShadowText 쓰기. (원본 반사 저장 꼴에서 러스트 판에 없는 FontFace, TextDirection 을 뺀 것)
//==============================================================================
fn save_shadow_text(shadow_text: &ShadowText, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "Text", Value::from(shadow_text.get_text()));
	write_value(&mut component_data, "TextAlign", Value::from(find_name(&TEXT_ALIGN_NAME_TABLE, shadow_text.get_text_align())));
	write_value(&mut component_data, "FitMinimumRatio", UIDocument::number_to_value(shadow_text.get_fit_minimum_ratio()));
	write_component_base(&mut component_data, shadow_text);
	component_data
}

//==============================================================================
// ShadowText 읽기.
//==============================================================================
fn load_shadow_text(shadow_text: &mut ShadowText, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(text_string) = read_string(component_data, "Text") {
		shadow_text.set_text(text_string);
	}
	if let Some(text_align) = read_string(component_data, "TextAlign").and_then(|name| find_value(&TEXT_ALIGN_NAME_TABLE, name)) {
		shadow_text.set_text_align(text_align);
	}
	if let Some(fit_minimum_ratio) = read_number(component_data, "FitMinimumRatio") {
		shadow_text.set_fit_minimum_ratio(fit_minimum_ratio);
	}
	read_component_base(shadow_text, component_data);
}

//==============================================================================
// UIProgressRing 쓰기. (원본 반사 저장 꼴)
//==============================================================================
fn save_progress_ring(progress_ring: &UIProgressRing, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "Ratio", UIDocument::number_to_value(progress_ring.get_ratio()));
	write_value(&mut component_data, "RoundSize", UIDocument::number_to_value(progress_ring.get_round_size()));
	write_value(&mut component_data, "LineWidth", UIDocument::number_to_value(progress_ring.get_line_width()));
	write_value(&mut component_data, "FillColor", UIDocument::color_to_array(progress_ring.get_fill_color()));
	write_value(&mut component_data, "TrackColor", UIDocument::color_to_array(progress_ring.get_track_color()));
	write_component_base(&mut component_data, progress_ring);
	component_data
}

//==============================================================================
// UIProgressRing 읽기.
//==============================================================================
fn load_progress_ring(progress_ring: &mut UIProgressRing, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(ratio) = read_number(component_data, "Ratio") {
		progress_ring.set_ratio(ratio);
	}
	if let Some(round_size) = read_number(component_data, "RoundSize") {
		progress_ring.set_round_size(round_size);
	}
	if let Some(line_width) = read_number(component_data, "LineWidth") {
		progress_ring.set_line_width(line_width);
	}
	if let Some(fill_color) = read_color(component_data, "FillColor") {
		progress_ring.set_fill_color(fill_color);
	}
	if let Some(track_color) = read_color(component_data, "TrackColor") {
		progress_ring.set_track_color(track_color);
	}
	read_component_base(progress_ring, component_data);
}

//==============================================================================
// ShadowImage 쓰기. (원본이 반사로 적을 꼴에 그림 경로를 더한 것. 그림 색이 없으면 원본처럼 ImageColor 를 적지 않는다)
//==============================================================================
fn save_shadow_image(shadow_image: &ShadowImage, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "imagePath", Value::from(shadow_image.get_image_path()));
	if let Some(image_color) = shadow_image.get_image_color() {
		write_value(&mut component_data, "ImageColor", UIDocument::color_to_array(image_color));
	}
	write_component_base(&mut component_data, shadow_image);
	component_data
}

//==============================================================================
// ShadowImage 읽기.
//==============================================================================
fn load_shadow_image(shadow_image: &mut ShadowImage, component_data: &Map<String, Value>, load_context: &mut UIDocumentLoadContext) {
	if let Some(image_path) = read_string(component_data, "imagePath") {
		if !image_path.is_empty() {
			shadow_image.set_image_path(image_path);
			load_context.bind_image::<ShadowImage>(image_path, |shadow_image, image| shadow_image.set_image(Some(image), None));
		}
	}
	if let Some(image_color) = read_color(component_data, "ImageColor") {
		shadow_image.set_image_color(Some(image_color));
	}
	read_component_base(shadow_image, component_data);
}

//==============================================================================
// UILabel 쓰기.
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
// UILabel 읽기.
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
// UIImageView 쓰기. (그림 경로만)
//==============================================================================
fn save_image_view(image_view: &UIImageView, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "imagePath", Value::from(image_view.get_image_path()));
	component_data
}

//==============================================================================
// UIImageView 읽기.
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
// UIButton 쓰기.
//==============================================================================
fn save_button(button: &UIButton, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "pressedTintColor", UIDocument::color_to_array(button.get_pressed_tint_color()));
	component_data
}

//==============================================================================
// UIButton 읽기.
//==============================================================================
fn load_button(button: &mut UIButton, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(pressed_tint_color) = read_color(component_data, "pressedTintColor") {
		button.set_pressed_tint_color(pressed_tint_color);
	}
}

//==============================================================================
// UIToggleButton 쓰기. (품은 버튼의 눌림 틴트)
//==============================================================================
fn save_toggle_button(toggle_button: &UIToggleButton, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "pressedTintColor", UIDocument::color_to_array(toggle_button.get_uibutton().get_pressed_tint_color()));
	component_data
}

//==============================================================================
// UIToggleButton 읽기.
//==============================================================================
fn load_toggle_button(toggle_button: &mut UIToggleButton, component_data: &Map<String, Value>, _load_context: &mut UIDocumentLoadContext) {
	if let Some(pressed_tint_color) = read_color(component_data, "pressedTintColor") {
		toggle_button.get_uibutton_mut().set_pressed_tint_color(pressed_tint_color);
	}
}

//==============================================================================
// UIProgressView 쓰기.
//==============================================================================
fn save_progress_view(progress_view: &UIProgressView, _save_context: &UIDocumentSaveContext) -> Map<String, Value> {
	let mut component_data = Map::new();
	write_value(&mut component_data, "value", UIDocument::number_to_value(progress_view.get_value()));
	write_value(&mut component_data, "minValue", UIDocument::number_to_value(progress_view.get_min_value()));
	write_value(&mut component_data, "maxValue", UIDocument::number_to_value(progress_view.get_max_value()));
	component_data
}

//==============================================================================
// UIProgressView 읽기. (범위를 먼저 정하고 값을 넣는다)
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
// UISlider 쓰기. (손잡이 노드는 원본 saveNodeReferences 처럼 ThumbNode 참조로)
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
// UISlider 읽기.
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
// UIScrollView 쓰기.
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
// UIScrollView 읽기.
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

#[cfg(test)]
mod tests {
	use super::*;

	// vanilla.js UIDocument.toJsonText(뿌리, true) 가 낸 글. (에디터 처음 씬처럼 Panel 아래 Title 글, 그림 Sprite 하나)
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

	// vanilla.js UIDocument.toJsonText(뿌리, false) 가 낸 글. (반사 저장 종류, 위젯, content 안의 자식, 노드 참조)
	const WIDGET_SCENE_TEXT: &str = r#"{"version":1,"root":{"type":"WorldNode","name":"widgetRoot","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[0,0],"interactable":false,"components":[],"children":[{"type":"WorldNode","name":"outline","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"Color":[1,0.5,0.25,1],"LineWidth":3,"RoundSize":12,"Circle":true,"GizmoVisible":false,"Enable":true,"ComponentType":"Outline","type":"Outline"}],"children":[]},{"type":"WorldNode","name":"ring","active":false,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"Ratio":0.25,"RoundSize":20,"LineWidth":4,"FillColor":[0.5,1,0.5,1],"TrackColor":[0.2,0.2,0.25,1],"GizmoVisible":false,"Enable":true,"ComponentType":"UIProgressRing","type":"UIProgressRing"}],"children":[]},{"type":"WorldNode","name":"shade","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"GizmoVisible":false,"Enable":false,"ComponentType":"SoftShade","type":"SoftShade"}],"children":[]},{"type":"WorldNode","name":"mask","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"type":"Mask"}],"children":[]},{"type":"WorldNode","name":"rich","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"text":"<b>굵게</b> 보통","fontSize":20,"textColor":[1,1,1,1],"strokeColor":[0,0,0,1],"strokeWidth":2,"textAlign":"left","textBaseline":"top","type":"RichText"}],"children":[]},{"type":"WorldNode","name":"label","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"text":"라벨","fontSize":24,"textColor":[1,1,1,1],"backgroundColor":[0,0,0,0.5],"textAlign":"right","textBaseline":"middle","type":"UILabel"}],"children":[]},{"type":"WorldNode","name":"button","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"pressedTintColor":[0,0,0,0.5],"type":"UIButton"}],"children":[],"contentChildren":[{"type":"WorldNode","name":"caption","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"text":"눌러","fontSize":32,"textColor":[1,1,1,1],"strokeColor":[1,1,1,1],"strokeWidth":0,"textAlign":"center","textBaseline":"middle","type":"Text"}],"children":[]}]},{"type":"WorldNode","name":"toggle","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"pressedTintColor":[0,0,0,0.3],"type":"UIToggleButton"}],"children":[]},{"type":"WorldNode","name":"progress","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"value":50,"minValue":0,"maxValue":200,"type":"UIProgressView"}],"children":[]},{"type":"WorldNode","name":"slider","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"value":0.75,"thumbRadius":12,"trackThickness":6,"ThumbNode":{"$nodeRef":"/slider/thumb"},"type":"UISlider"}],"children":[{"type":"WorldNode","name":"thumb","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[],"children":[]}]},{"type":"WorldNode","name":"scroll","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":true,"components":[{"scrollContentSize":[200,600],"scrollMode":"elastic","dragSensitivity":1.5,"horizontal":false,"vertical":true,"backgroundColor":[1,1,1,0],"type":"UIScrollView"}],"children":[],"contentChildren":[{"type":"WorldNode","name":"item","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"color":[0.25,0.25,0.25,1],"roundSize":0,"type":"Paint"}],"children":[]}]},{"type":"WorldNode","name":"image","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[200,40],"interactable":false,"components":[{"imagePath":"star.png","type":"UIImageView"},{"color":[0,0,0,0],"roundSize":0,"imagePath":"","nineSlice":[0,0,0,0],"spriteDrawMode":"simple","spriteBlendMode":"source-over","type":"Sprite"}],"children":[]}]}}"#;

	// vanilla.js UIDocument.toJsonText(뿌리, false) 가 낸 글. (ShadowText 하나)
	const SHADOW_TEXT_SCENE_TEXT: &str = r#"{"version":1,"root":{"type":"WorldNode","name":"shadowRoot","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[0,0],"interactable":false,"components":[],"children":[{"type":"WorldNode","name":"shadowText","active":true,"position":[0,0],"scale":[1,1],"rotation":0,"opacity":1,"pivot":[0.5,0.5],"anchor":[0.5,0.5],"contentSize":[0,0],"interactable":false,"components":[{"Text":"그림자","TextAlign":"left","TextDirection":"ltr","FitMinimumRatio":0.5,"GizmoVisible":false,"Enable":true,"ComponentType":"ShadowText","type":"ShadowText"}],"children":[]}]}}"#;

	//==============================================================================
	// 에디터 씬 파일(frameSize 를 얹은 것)을 읽어 다시 쓰면 원본이 낸 글과 한 글자도 다르지 않다.
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
	// 러스트에서 같은 트리를 지어 내보내면 원본이 낸 글과 같다.
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
	// 위젯과 반사 저장 종류의 왕복. (content 노드는 다시 만들어지고, 그 안의 자식과 노드 참조가 이어진다)
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
	// ShadowText 는 러스트 판에 없는 TextDirection 만 빠지고 같다.
	//==============================================================================
	#[test]
	fn shadow_text_round_trip_drops_text_direction() {
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		let shadow_root_id = document.from_json_text(&mut node_tree, SHADOW_TEXT_SCENE_TEXT).expect("document");
		let expected_text = SHADOW_TEXT_SCENE_TEXT.replace("\"TextDirection\":\"ltr\",", "");
		assert_eq!(document.to_json_text(&node_tree, shadow_root_id, false), expected_text);
	}

	//==============================================================================
	// 이미지 해석기가 찾은 그림을 Sprite 와 UIImageView 에 넣는다. 못 찾은 그림은 경로만 남는다.
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
	// 등록하지 않은 종류는 건너뛰고, 게임이 등록한 종류는 짓고 쓰고 읽는다.
	//==============================================================================
	#[test]
	fn registered_game_component_round_trip() {
		struct Spin {
			component_base: crate::core::component::ComponentBase,
			degrees_per_second: f32,
		}
		impl Component for Spin {
			fn get_component_base(&self) -> &crate::core::component::ComponentBase {
				&self.component_base
			}
			fn get_component_base_mut(&mut self) -> &mut crate::core::component::ComponentBase {
				&mut self.component_base
			}
			fn get_component_type(&self) -> &'static str {
				"Spin"
			}
		}
		let document_text = r#"{"version":1,"root":{"name":"root","components":[{"type":"Unknown"},{"degreesPerSecond":90,"type":"Spin"}],"children":[]}}"#;
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		let unknown_root_id = document.from_json_text(&mut node_tree, document_text).expect("document");
		assert_eq!(node_tree[unknown_root_id].get_all_components().len(), 0);

		document.register_component_type("Spin", || Spin { component_base: crate::core::component::ComponentBase::new(), degrees_per_second: 0.0 }, |spin, _save_context| {
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
	// 잘못된 문서는 원본처럼 실패한다.
	//==============================================================================
	#[test]
	fn invalid_document_is_error() {
		let mut node_tree = NodeTree::new();
		let mut document = UIDocument::new();
		assert!(document.from_json_text(&mut node_tree, "{\"version\":1}").is_err());
		assert!(document.from_json_text(&mut node_tree, "not json").is_err());
	}

	//==============================================================================
	// 수는 가장 짧은 십진으로, 정수는 소수점 없이 적는다.
	//==============================================================================
	#[test]
	fn number_text_is_shortest() {
		assert_eq!(UIDocument::number_to_value(0.22).to_string(), "0.22");
		assert_eq!(UIDocument::number_to_value(24.0).to_string(), "24");
		assert_eq!(UIDocument::number_to_value(-0.0).to_string(), "0");
		assert_eq!(UIDocument::number_to_value(f32::NAN), Value::Null);
	}

	//==============================================================================
	// 노드 경로 계산과 찾기. (원본 findNodePath, findNodeByPath)
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
}

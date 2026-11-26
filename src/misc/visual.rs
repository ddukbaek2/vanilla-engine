//==============================================================================
//! 비주얼 컴포넌트. (원본 misc/visual.js 의 Visual, 비주얼 에디터 문서를 정한 자리에 노드 트리로 짓는다)
//! - 문서는 비주얼 에디터(vanilla.js tools/visualeditor)가 저장하는 UIDocument JSON 글이다. ({ version, root, frameSize? }, 씬과 프리팹 모두)
//!   VisualAsset 이 파일을 읽어 문서를 든 Visual 을 만든다.
//! - create_node 가 문서의 노드 트리를 새로 지어 뿌리의 로컬 위치를 location 으로 둔다. (부르는 쪽이 원하는 부모에 붙인다, 부를 때마다 새 트리)
//! - 스스로 그리거나 갱신하지 않는다.
//! - 원본과 다른 점: 원본 createNode() 는 전역 UIDocument 로 짓고, 러스트 판 create_node 는 짓는 UIDocument 와 노드 트리를 받는다.
//!   (러스트 UIDocument 는 등록 표를 든 값이다) 원본이 문서가 없으면 null, 문서 꼴이 틀리면 예외인 것은 Ok(None), Err 다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::ui::uidocument::UIDocument;

//==============================================================================
/// 비주얼 컴포넌트.
/// - 비주얼 에디터 문서 글과 위치를 든다. create_node 가 그 문서로 새 노드 트리를 짓는다. 그리지도 갱신하지도 않는다. (is_tick_needed 가 false)
///
/// # 예제
/// 문서를 든 비주얼로 노드 트리 짓기
/// ```rust
/// use vanilla::{Component, NodeTree, UIDocument, Vector2, Visual};
///
/// let document_text = r#"{"version":1,"root":{"type":"WorldNode","name":"Badge","position":[300,200],"contentSize":[64,64],"components":[],"children":[]}}"#;
/// let mut visual = Visual::new();
/// visual.set_document_text(Some(document_text.to_string()));
/// visual.set_location(Vector2::create(40.0, 20.0));
///
/// let mut document = UIDocument::new();
/// let mut node_tree = NodeTree::new();
/// let parent_id = node_tree.create_node("parent");
/// let badge_id = visual.create_node(&mut document, &mut node_tree).expect("document").expect("root");
/// assert!(node_tree.add_child(parent_id, badge_id));
/// assert_eq!(node_tree[badge_id].get_name(), "Badge");
/// assert_eq!(node_tree[badge_id].get_local_position(), Vector2::create(40.0, 20.0));
/// assert_eq!(node_tree.get_parent_id(badge_id), Some(parent_id));
/// assert_eq!(visual.get_component_type(), "Visual");
/// ```
//==============================================================================
pub struct Visual {
	component_base: ComponentBase,
	location: Vector2,
	document_text: Option<String>,
}

#[crate::reflect]
impl Visual {
	//==============================================================================
	/// 생성. (위치 0)
	///
	/// # 돌려줌
	/// 문서 글이 없고 위치가 (0, 0) 인 비주얼.
	//==============================================================================
	pub fn new() -> Visual {
		Visual {
			component_base: ComponentBase::new(),
			location: Vector2::zero(),
			document_text: None,
		}
	}

	//==============================================================================
	/// 위치 설정.
	///
	/// # 인자
	/// - `location`: create_node 가 지은 뿌리 노드에 줄 로컬 위치. (부모 기준, 기본 0)
	//==============================================================================
	pub fn set_location(&mut self, location: Vector2) {
		self.location = location;
	}

	//==============================================================================
	/// 위치 반환.
	///
	/// # 돌려줌
	/// 지은 뿌리 노드에 줄 로컬 위치.
	//==============================================================================
	pub fn get_location(&self) -> Vector2 {
		self.location
	}

	//==============================================================================
	/// 문서 글 설정.
	///
	/// # 인자
	/// - `document_text`: 비주얼 에디터가 저장한 UIDocument JSON 글. (None 이면 비운다, 읽는 것은 create_node 때)
	//==============================================================================
	pub fn set_document_text(&mut self, document_text: Option<String>) {
		self.document_text = document_text;
	}

	//==============================================================================
	/// 문서 글 반환.
	///
	/// # 돌려줌
	/// 든 문서 글. 없으면 None.
	//==============================================================================
	pub fn get_document_text(&self) -> Option<&str> {
		self.document_text.as_deref()
	}

	//==============================================================================
	/// 문서로 노드 트리 짓기.
	///
	/// # 인자
	/// - `document`: 노드와 컴포넌트를 지을 UIDocument. (게임이 등록한 종류와 그림 해석기를 쓴다)
	/// - `node_tree`: 지을 노드 트리.
	///
	/// # 돌려줌
	/// 새로 지은 뿌리 노드. (어느 부모에도 붙지 않았고 로컬 위치가 location 이다) 문서가 없으면 Ok(None), 문서 꼴이 틀리면 Err.
	///
	/// # 예제
	/// 문서가 없을 때, 꼴이 틀릴 때, 같은 문서로 두 벌 짓기
	/// ```rust
	/// use vanilla::{NodeTree, UIDocument, Vector2, Visual};
	///
	/// let mut document = UIDocument::new();
	/// let mut node_tree = NodeTree::new();
	/// let mut visual = Visual::new();
	/// assert_eq!(visual.create_node(&mut document, &mut node_tree), Ok(None));
	/// visual.set_document_text(Some(String::from(r#"{"version":1}"#)));
	/// assert!(visual.create_node(&mut document, &mut node_tree).is_err());
	///
	/// visual.set_document_text(Some(String::from(r#"{"version":1,"root":{"type":"WorldNode","name":"Coin","contentSize":[32,32],"components":[],"children":[]}}"#)));
	/// let first_id = visual.create_node(&mut document, &mut node_tree).expect("document").expect("root");
	/// visual.set_location(Vector2::create(48.0, 0.0));
	/// let second_id = visual.create_node(&mut document, &mut node_tree).expect("document").expect("root");
	/// assert_ne!(first_id, second_id);
	/// assert_eq!(node_tree[second_id].get_name(), "Coin");
	/// assert_eq!(node_tree[first_id].get_local_position().x, 0.0);
	/// assert_eq!(node_tree[second_id].get_local_position().x, 48.0);
	/// assert!(node_tree.get_parent_id(second_id).is_none());
	/// ```
	//==============================================================================
	pub fn create_node(&self, document: &mut UIDocument, node_tree: &mut NodeTree) -> Result<Option<NodeId>, String> {
		let document_text = match self.document_text.as_deref() {
			Some(document_text) => {
				document_text
			}
			None => {
				return Ok(None);
			}
		};
		let root_id = document.from_json_text(node_tree, document_text)?;
		if let Some(root_node) = node_tree.get_node_mut(root_id) {
			root_node.set_local_position(self.location);
		}
		Ok(Some(root_id))
	}
}

impl Default for Visual {
	//==============================================================================
	/// 문서 글이 없고 위치가 0 인 비주얼. (new 와 같다)
	//==============================================================================
	fn default() -> Visual {
		Visual::new()
	}
}

#[crate::reflect]
impl Component for Visual {
	//==============================================================================
	/// 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 타입 이름. ("Visual")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"Visual"
	}

	//==============================================================================
	/// 갱신할 것이 없다.
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}
}

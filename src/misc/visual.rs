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
	fn default() -> Visual {
		Visual::new()
	}
}

#[crate::reflect]
impl Component for Visual {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

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

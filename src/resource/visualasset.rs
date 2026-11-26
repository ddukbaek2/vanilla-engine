//==============================================================================
//! 비주얼 애셋. (원본 resource/visualasset.js 의 VisualAsset, 비주얼 에디터가 저장한 문서 파일을 읽어 Visual 을 만든다)
//! - 파일은 비주얼 에디터(vanilla.js tools/visualeditor)가 저장하는 UIDocument JSON 이다. (씬 *.scene.json, 프리팹 *.prefab.json, { version, root, frameSize? })
//! - load 가 엔진의 애셋 받기로 바이트를 부탁한다. 바이트가 오면 처음 볼 때 JSON 객체이고 root 객체가 있는지 보고, 맞으면 읽힌 것이고 그 글을 든 Visual 을 담는다.
//!   받기에 실패하거나 문서 꼴이 아니면 읽히지 않은 것이다. (원본은 load 가 거부된다)
//! - 노드 트리는 Visual::create_node 가 짓는다. (부를 때마다 새 트리)
//! - 원본과 다른 점:
//!   - 원본 load(경로) 는 Promise 를 돌려주고, 러스트 판 load(엔진, 경로) 는 await 할 수 있는 읽기 손잡이를 돌려준다. (다른 애셋과 같다)
//!   - 원본 정적 createVisualFromText(글) 는 실패하면 예외를 던지고, 러스트 판 create_visual_from_text 는 Err 다.
//!   - 시험과 묶음 파일을 위해 load_from_bytes 가 있다. (TextAsset 과 같다)
//==============================================================================
use crate::core::asset::{Asset, AssetBase, AssetLoadHandle, AssetType};
use crate::core::engine::Engine;
use crate::misc::visual::Visual;
use std::cell::OnceCell;

//==============================================================================
/// 비주얼 애셋.
/// - 비주얼 에디터 문서 파일을 읽어 그 글을 든 Visual 을 담는다. 문서 꼴(JSON 객체, root 객체)이 아니면 읽히지 않은 것이다.
///
/// # 예제
/// 비주얼 에디터 프리팹을 읽어 노드 짓기
/// ```rust
/// use vanilla::{Asset, AssetType, NodeTree, UIDocument, VisualAsset};
///
/// let document_text = r#"{"version":1,"root":{"type":"WorldNode","name":"Panel","contentSize":[200,120],"components":[],"children":[]},"frameSize":[960,540]}"#;
/// let mut visual_asset = VisualAsset::new();
/// assert!(!visual_asset.is_loaded());
/// visual_asset.load_from_bytes("panel.prefab.json", document_text.as_bytes().to_vec());
/// assert!(visual_asset.is_loaded());
/// assert_eq!(visual_asset.get_asset_base().get_asset_type(), AssetType::Visual);
///
/// let visual = visual_asset.get_visual().expect("visual");
/// let mut document = UIDocument::new();
/// let mut node_tree = NodeTree::new();
/// let panel_id = visual.create_node(&mut document, &mut node_tree).expect("document").expect("root");
/// assert_eq!(node_tree[panel_id].get_name(), "Panel");
/// ```
//==============================================================================
pub struct VisualAsset {
	asset_base: AssetBase,
	/// 받은 문서 글. (바이트가 오면 처음 볼 때 한 번 푼다, 문서 꼴이 아니면 None)
	document_text: OnceCell<Option<String>>,
	/// 담긴 비주얼. (문서 글로 처음 볼 때 짓거나 set_visual 로 넣은 것)
	visual: OnceCell<Option<Visual>>,
}

impl VisualAsset {
	//==============================================================================
	/// 생성. (애셋 타입 visual)
	///
	/// # 돌려줌
	/// 요청 전의 빈 비주얼 애셋. (읽히지 않았고 담긴 비주얼이 없다)
	//==============================================================================
	pub fn new() -> VisualAsset {
		VisualAsset {
			asset_base: AssetBase::new(AssetType::Visual),
			document_text: OnceCell::new(),
			visual: OnceCell::new(),
		}
	}

	//==============================================================================
	/// 읽기 요청. (원본 load, await 할 수 있는 읽기 손잡이를 돌려준다)
	///
	/// # 인자
	/// - `engine`: 애셋을 받을 엔진.
	/// - `asset_path`: 비주얼 에디터가 저장한 문서 파일 경로.
	///
	/// # 돌려줌
	/// 읽기 손잡이. (바이트가 오면 끝난다, 문서 꼴은 is_loaded 와 get_visual 이 본다)
	///
	/// # 예제
	/// data: 주소로 문서 받기
	/// ```rust
	/// use vanilla::{Asset, Engine, EngineConfiguration, VisualAsset};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut visual_asset = VisualAsset::new();
	/// visual_asset.load(&engine, r#"data:application/json,{"version":1,"root":{"type":"WorldNode","name":"Icon","components":[],"children":[]}}"#);
	/// assert!(visual_asset.is_loaded());
	/// assert!(visual_asset.get_visual().is_some());
	///
	/// let mut broken_asset = VisualAsset::new();
	/// broken_asset.load(&engine, r#"data:application/json,{"version":1}"#);
	/// assert!(!broken_asset.is_loaded());
	/// assert!(broken_asset.get_visual().is_none());
	/// ```
	//==============================================================================
	pub fn load(&mut self, engine: &Engine, asset_path: &str) -> AssetLoadHandle {
		self.document_text = OnceCell::new();
		self.visual = OnceCell::new();
		self.asset_base.request_load(engine, asset_path)
	}

	//==============================================================================
	/// 이미 가진 바이트로 읽기.
	///
	/// # 인자
	/// - `asset_path`: 적어 둘 경로.
	/// - `bytes`: 문서 파일 바이트.
	//==============================================================================
	pub fn load_from_bytes(&mut self, asset_path: &str, bytes: Vec<u8>) {
		self.document_text = OnceCell::new();
		self.visual = OnceCell::new();
		self.asset_base.load_from_bytes(asset_path, bytes);
	}

	//==============================================================================
	/// 내리기. (담은 Visual 도 비운다)
	//==============================================================================
	pub fn unload(&mut self) {
		self.asset_base.unload();
		self.document_text = OnceCell::new();
		self.visual = OnceCell::from(None);
	}

	//==============================================================================
	/// 문서 글로 Visual 만들기.
	///
	/// # 인자
	/// - `document_text`: 비주얼 에디터가 저장한 UIDocument JSON 글.
	///
	/// # 돌려줌
	/// 그 글을 든 Visual. JSON 객체가 아니거나 root 객체가 없으면 Err.
	///
	/// # 예제
	/// 문서 글에서 바로 만들기
	/// ```rust
	/// use vanilla::{NodeTree, UIDocument, VisualAsset};
	///
	/// let document_text = r#"{"version":1,"root":{"type":"WorldNode","name":"Icon","components":[],"children":[]}}"#;
	/// let visual = VisualAsset::create_visual_from_text(document_text).expect("visual");
	/// assert_eq!(visual.get_document_text(), Some(document_text));
	/// let mut document = UIDocument::new();
	/// let mut node_tree = NodeTree::new();
	/// let icon_id = visual.create_node(&mut document, &mut node_tree).expect("document").expect("root");
	/// assert_eq!(node_tree[icon_id].get_name(), "Icon");
	///
	/// assert!(VisualAsset::create_visual_from_text("not json").is_err());
	/// assert!(VisualAsset::create_visual_from_text("[1, 2]").is_err());
	/// assert!(VisualAsset::create_visual_from_text(r#"{"version":1,"root":[1, 2]}"#).is_err());
	/// ```
	//==============================================================================
	pub fn create_visual_from_text(document_text: &str) -> Result<Visual, String> {
		let document_data: serde_json::Value = serde_json::from_str(document_text).map_err(|error| format!("VisualAsset: {}", error))?;
		if !document_data.get("root").is_some_and(|root_data| root_data.is_object()) {
			return Err(String::from("VisualAsset: invalid document."));
		}
		let mut visual = Visual::new();
		visual.set_document_text(Some(document_text.to_string()));
		Ok(visual)
	}

	//==============================================================================
	/// 받은 문서 글. (바이트가 오기 전이거나 문서 꼴이 아니면 None, 처음 볼 때 한 번 푼다)
	//==============================================================================
	fn read_document_text(&self) -> Option<&str> {
		if let Some(document_text) = self.document_text.get() {
			return document_text.as_deref();
		}
		let bytes = self.asset_base.get_bytes()?;
		self.document_text.get_or_init(|| {
			let document_text = String::from_utf8_lossy(&bytes).into_owned();
			match VisualAsset::create_visual_from_text(&document_text) {
				Ok(_) => {
					Some(document_text)
				}
				Err(error_message) => {
					eprintln!("Error loading visual: {} ({})", self.asset_base.get_asset_path(), error_message);
					None
				}
			}
		}).as_deref()
	}

	//==============================================================================
	/// 비주얼 설정.
	///
	/// # 인자
	/// - `visual`: 담을 비주얼. (None 이면 비운다, 읽힌 상태는 바꾸지 않는다)
	//==============================================================================
	pub fn set_visual(&mut self, visual: Option<Visual>) {
		self.visual = OnceCell::from(visual);
	}

	//==============================================================================
	/// 비주얼 반환.
	///
	/// # 돌려줌
	/// 담긴 비주얼. (읽은 문서를 든 것이거나 set_visual 로 넣은 것) 아직 읽히지 않았거나 문서 꼴이 아니고 넣은 것도 없으면 None.
	///
	/// # 예제
	/// 비주얼을 직접 넣고 비우기, 내리기
	/// ```rust
	/// use vanilla::{Asset, Vector2, Visual, VisualAsset};
	///
	/// let mut visual_asset = VisualAsset::new();
	/// assert!(visual_asset.get_visual().is_none());
	/// let mut visual = Visual::new();
	/// visual.set_location(Vector2::create(10.0, 20.0));
	/// visual_asset.set_visual(Some(visual));
	/// assert_eq!(visual_asset.get_visual().map(|visual| visual.get_location()), Some(Vector2::create(10.0, 20.0)));
	/// assert!(!visual_asset.is_loaded());
	///
	/// visual_asset.load_from_bytes("coin.prefab.json", br#"{"version":1,"root":{"name":"Coin"}}"#.to_vec());
	/// assert!(visual_asset.get_visual().is_some_and(|visual| visual.get_document_text().is_some()));
	/// visual_asset.set_visual(None);
	/// assert!(visual_asset.get_visual().is_none());
	/// assert!(visual_asset.is_loaded());
	/// visual_asset.unload();
	/// assert!(!visual_asset.is_loaded());
	/// assert!(visual_asset.get_visual().is_none());
	/// ```
	//==============================================================================
	pub fn get_visual(&self) -> Option<&Visual> {
		if let Some(visual) = self.visual.get() {
			return visual.as_ref();
		}
		let document_text = self.read_document_text()?;
		self.visual.get_or_init(|| {
			let mut visual = Visual::new();
			visual.set_document_text(Some(document_text.to_string()));
			Some(visual)
		}).as_ref()
	}
}

impl Default for VisualAsset {
	//==============================================================================
	/// 요청 전의 빈 비주얼 애셋. (new 와 같다)
	//==============================================================================
	fn default() -> VisualAsset {
		VisualAsset::new()
	}
}

impl Asset for VisualAsset {
	//==============================================================================
	/// 공통 상태.
	//==============================================================================
	fn get_asset_base(&self) -> &AssetBase {
		&self.asset_base
	}

	//==============================================================================
	/// 공통 상태. (변경용)
	//==============================================================================
	fn get_asset_base_mut(&mut self) -> &mut AssetBase {
		&mut self.asset_base
	}

	//==============================================================================
	/// 다 읽혔는지. (바이트가 오고 비주얼 에디터 문서 꼴일 때, 원본처럼 꼴이 아니면 거짓)
	//==============================================================================
	fn is_loaded(&self) -> bool {
		self.read_document_text().is_some()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::vector2::Vector2;
	use crate::core::node::nodetree::NodeTree;
	use crate::core::scene::SceneBase;
	use crate::resource::bunchasset::BunchAsset;
	use crate::ui::uidocument::UIDocument;

	/// 비주얼 에디터가 저장하는 꼴의 프리팹 문서. (뿌리 하나와 자식 하나, 에디터가 얹는 frameSize 포함)
	const PREFAB_DOCUMENT_TEXT: &str = r#"{"version":1,"root":{"type":"WorldNode","name":"Badge","position":[300,200],"contentSize":[64,64],"components":[],"children":[{"type":"WorldNode","name":"Icon","position":[8,8],"contentSize":[16,16],"components":[],"children":[]}]},"frameSize":[960,540]}"#;

	//==============================================================================
	/// 비주얼 애셋은 문서 파일을 읽어 문서를 든 Visual 을 담고, Visual 은 그 문서로 노드 트리를 location 자리에 짓는다.
	//==============================================================================
	#[test]
	fn visual_asset_reads_visual_editor_document() {
		let mut visual_asset = VisualAsset::new();
		assert!(!visual_asset.is_loaded());
		assert!(visual_asset.get_visual().is_none());
		assert_eq!(visual_asset.get_asset_base().get_asset_type(), AssetType::Visual);
		visual_asset.load_from_bytes("badge.prefab.json", PREFAB_DOCUMENT_TEXT.as_bytes().to_vec());
		assert!(visual_asset.is_loaded());
		assert_eq!(visual_asset.get_asset_path(), "badge.prefab.json");
		let mut scene_base = SceneBase::new();
		scene_base.track_asset(&visual_asset);
		assert!(scene_base.are_tracked_assets_finished());

		let visual = visual_asset.get_visual().expect("visual");
		assert_eq!(visual.get_document_text(), Some(PREFAB_DOCUMENT_TEXT));
		let mut document = UIDocument::new();
		let mut node_tree = NodeTree::new();
		let first_root_id = visual.create_node(&mut document, &mut node_tree).expect("create").expect("root");
		let first_root = node_tree.get_node(first_root_id).expect("root node");
		assert_eq!(first_root.get_name(), "Badge");
		assert_eq!(first_root.get_local_position(), Vector2::zero());
		assert!(first_root.get_parent_id().is_none());
		let icon_id = UIDocument::find_node_by_path(&node_tree, first_root_id, "/Icon").expect("icon");
		assert_eq!(node_tree.get_node(icon_id).expect("icon node").get_local_position(), Vector2::create(8.0, 8.0));

		let mut placed_visual = VisualAsset::create_visual_from_text(PREFAB_DOCUMENT_TEXT).expect("visual");
		placed_visual.set_location(Vector2::create(40.0, 20.0));
		let second_root_id = placed_visual.create_node(&mut document, &mut node_tree).expect("create").expect("root");
		assert_ne!(first_root_id, second_root_id);
		assert_eq!(node_tree.get_node(second_root_id).expect("root node").get_local_position(), Vector2::create(40.0, 20.0));
		assert_eq!(Visual::new().create_node(&mut document, &mut node_tree), Ok(None));

		visual_asset.set_visual(None);
		assert!(visual_asset.get_visual().is_none());
		assert!(visual_asset.is_loaded());
		visual_asset.unload();
		assert!(!visual_asset.is_loaded());
		assert!(visual_asset.get_visual().is_none());
	}

	//==============================================================================
	/// 문서 꼴이 아니면 읽히지 않은 것이고 Visual 이 없다. (JSON 이 아니거나 root 객체가 없을 때)
	//==============================================================================
	#[test]
	fn broken_visual_document_is_not_loaded() {
		let mut visual_asset = VisualAsset::new();
		visual_asset.load_from_bytes("broken.scene.json", b"{\"version\": 1, \"root\": ".to_vec());
		assert!(!visual_asset.is_loaded());
		assert!(visual_asset.get_visual().is_none());
		visual_asset.load_from_bytes("rootless.scene.json", b"{\"version\": 1, \"root\": [1, 2]}".to_vec());
		assert!(!visual_asset.is_loaded());
		assert!(VisualAsset::create_visual_from_text("[1, 2]").is_err());
		visual_asset.load_from_bytes("good.scene.json", PREFAB_DOCUMENT_TEXT.as_bytes().to_vec());
		assert!(visual_asset.is_loaded());
		assert!(visual_asset.get_visual().is_some());
	}

	//==============================================================================
	/// 묶음 애셋은 바이트를 들고 타입이 bunch 다.
	//==============================================================================
	#[test]
	fn bunch_asset_holds_bytes() {
		let mut bunch_asset = BunchAsset::new();
		bunch_asset.load_from_bytes("pack.bunch", vec![1, 2, 3]);
		assert!(bunch_asset.is_loaded());
		assert_eq!(bunch_asset.get_blob().map(|bytes| bytes.to_vec()), Some(vec![1, 2, 3]));
		assert_eq!(bunch_asset.get_asset_base().get_asset_type(), AssetType::Bunch);
	}
}

//==============================================================================
//! 스킨드 모델 자산. (원본 graphics3d/skinnedmodelasset.js)
//! - 같은 파일에서 지은 모델 인스턴스들이 나눠 쓰는 풀린 자료와 GPU 자원이다. 메시 버퍼와 버텍스 어레이, 머티리얼 원형과 그 텍스처,
//!   모프 텍스처, 노드와 스킨 원형, 애니메이션 클립을 한 벌만 든다. (유니티의 메시와 sharedMaterial, three.js 의 BufferGeometry 와 Material 을
//!   복제본들이 나눠 쓰는 것과 같다)
//! - 인스턴스(SkinnedModel)는 이것을 참조 수로 나눠 쓴다. 인스턴스마다 따로 갖는 것은 포즈, 애니메이션 재생 상태, 모프 가중치,
//!   갈라진 머티리얼(get_material), 와이어프레임 버퍼, add_morph_target 으로 갈라진 모프 텍스처다.
//! - 마지막 인스턴스가 destroy 하면(참조 수 0) GPU 자원을 지우고 캐시에서 빠진다. 참조 수는 Rc 의 수가 아니라 retain / release 로 센다.
//!   (원본과 같은 수, get_asset 으로 Rc 를 더 쥐어도 자원 수명은 인스턴스 수를 따른다)
//! - 캐시는 셰이더 프로그램과 자산 키(normalize_key 로 정규화한 주소나 자산 이름)로 찾는다. 받기 손잡이의 create_model,
//!   load_from_url_async 같은 받기가 이것을 쓴다. 캐시는 스레드마다 하나다. (GL 컨텍스트가 스레드에 묶여서다)
//!
//! # 예제
//! 키 정규화와 빈 캐시
//! ```rust
//! use vanilla::SkinnedModelAsset;
//!
//! assert_eq!(SkinnedModelAsset::normalize_key("./assets/models/../fox.glb"), "assets/fox.glb");
//! SkinnedModelAsset::clear_cache();
//! assert_eq!(SkinnedModelAsset::get_cached_count(), 0);
//! ```
//==============================================================================

//==============================================================================
// 포함 모듈 목록.
//==============================================================================
use crate::core::shaderprogram::ShaderProgram;
use crate::graphics3d::material::Material;
use crate::graphics3d::skinnedmodel::{AnimationClip, Drawable, Skin, SkinnedModelNode};
use glow::HasContext;
use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

//==============================================================================
// 전역 변수 목록.
//==============================================================================
thread_local! {
	/// 자산 캐시. ((자산 키, 셰이더 프로그램 주소) → 자산, 버텍스 어레이의 어트리뷰트 자리가 셰이더마다 달라 셰이더별로 따로 든다)
	static ASSET_CACHE_TABLE: RefCell<HashMap<(String, usize), Rc<SkinnedModelAsset>>> = RefCell::new(HashMap::new());
}

//==============================================================================
/// 셰이더 프로그램의 캐시 자리. (자산이 Rc 를 쥐고 있어 자산이 캐시에 있는 동안 주소가 바뀌지 않는다)
//==============================================================================
fn shader_program_address(shader_program: &Rc<ShaderProgram>) -> usize {
	Rc::as_ptr(shader_program) as *const () as usize
}

//==============================================================================
/// 자산에 넣을 자료. (SkinnedModel::upload_meshes_and_materials 가 올린 뒤 채운다, 원본 생성자의 content 객체)
//==============================================================================
pub struct SkinnedModelAssetContent {
	/// 원형 드로어블 목록. (인스턴스는 이것을 베껴 버퍼와 머티리얼 원형을 나눠 쓴다)
	pub drawable_list: Vec<Drawable>,
	/// 머티리얼 원형 목록. (기본 머티리얼이 마지막, 마지막 인스턴스가 destroy 할 때 지운다)
	pub material_list: Vec<Rc<Material>>,
	/// 노드 원형 목록. (올릴 때의 노드)
	pub node_list: Vec<SkinnedModelNode>,
	/// 루트 노드 번호 목록.
	pub root_node_indices: Vec<usize>,
	/// 스킨 원형 목록.
	pub skin_list: Vec<Skin>,
	/// 애니메이션 클립 목록. (인스턴스가 모두 같은 목록을 나눠 쓴다)
	pub animation_list: Rc<RefCell<Vec<AnimationClip>>>,
	/// 정점, 인덱스 버퍼와 모프 텍스처의 GPU 바이트 수.
	pub buffer_byte_count: usize,
}

//==============================================================================
/// 스킨드 모델 자산. (같은 파일에서 지은 모델 인스턴스들이 나눠 쓰는 풀린 자료와 GPU 자원, 참조 수로 수명을 센다)
///
/// # 예제
/// 받은 모델을 열 마리로 늘리고 자산 하나를 나눠 쓰는지 보기
/// ```rust,no_run
/// use vanilla::{SkinnedModel, SkinnedModelAsset};
///
/// fn spawn_herd(gl: &glow::Context, source_model: &SkinnedModel) -> Result<Vec<SkinnedModel>, String> {
/// 	let mut model_list = Vec::new();
/// 	for _ in 0..10 {
/// 		model_list.push(source_model.create_instance()?);
/// 	}
/// 	let skinned_model_asset = source_model.get_asset().ok_or("no asset")?;
/// 	assert_eq!(skinned_model_asset.get_reference_count(), 11);
/// 	println!("{} bytes on GPU", skinned_model_asset.get_gpu_byte_count());
/// 	let _ = gl;
/// 	Ok(model_list)
/// }
/// ```
//==============================================================================
pub struct SkinnedModelAsset {
	shader_program: Rc<ShaderProgram>,
	key: RefCell<Option<String>>,
	reference_count: Cell<usize>,
	is_destroyed: Cell<bool>,
	drawable_list: RefCell<Vec<Drawable>>,
	material_list: RefCell<Vec<Rc<Material>>>,
	node_list: Vec<SkinnedModelNode>,
	root_node_indices: Vec<usize>,
	skin_list: Vec<Skin>,
	animation_list: Rc<RefCell<Vec<AnimationClip>>>,
	buffer_byte_count: usize,
}

impl SkinnedModelAsset {
	//==============================================================================
	/// 생성. (SkinnedModel::upload_meshes_and_materials 가 올린 뒤 짓는다, 참조 수 1)
	///
	/// # 인자
	/// - `shader_program`: 버텍스 어레이와 머티리얼이 쓰는 셰이더 프로그램.
	/// - `content`: 나눠 쓸 자료. (드로어블은 원형으로 두고 인스턴스가 베낀다)
	///
	/// # 돌려줌
	/// 참조 수 1, 캐시 밖의 자산.
	//==============================================================================
	pub fn new(shader_program: Rc<ShaderProgram>, content: SkinnedModelAssetContent) -> SkinnedModelAsset {
		SkinnedModelAsset {
			shader_program,
			key: RefCell::new(None),
			reference_count: Cell::new(1),
			is_destroyed: Cell::new(false),
			drawable_list: RefCell::new(content.drawable_list),
			material_list: RefCell::new(content.material_list),
			node_list: content.node_list,
			root_node_indices: content.root_node_indices,
			skin_list: content.skin_list,
			animation_list: content.animation_list,
			buffer_byte_count: content.buffer_byte_count,
		}
	}

	//==============================================================================
	/// 자산 키 정규화. (역슬래시를 / 로, "." 과 ".." 마디를 풀고 빈 마디를 줄인다, 앞의 "주소틀://" 과 뒤의 ?, # 은 그대로)
	/// - 같은 파일을 "./a/b.glb", "a/c/../b.glb" 처럼 다르게 적어도 같은 키가 되게 한다. 기준 주소로 절대 주소를 만들지는 않는다.
	///
	/// # 인자
	/// - `path`: 자산 주소나 이름.
	///
	/// # 돌려줌
	/// 정규화한 키.
	///
	/// # 예제
	/// 다르게 적은 같은 경로
	/// ```rust
	/// use vanilla::SkinnedModelAsset;
	///
	/// assert_eq!(SkinnedModelAsset::normalize_key("./assets/models/../fox.glb"), "assets/fox.glb");
	/// assert_eq!(SkinnedModelAsset::normalize_key("assets\\fox.glb"), "assets/fox.glb");
	/// assert_eq!(SkinnedModelAsset::normalize_key("https://example.com/a/./b/../fox.glb?v=2"), "https://example.com/a/fox.glb?v=2");
	/// ```
	//==============================================================================
	pub fn normalize_key(path: &str) -> String {
		let replaced_path = path.replace('\\', "/");
		let (mut text, suffix) = match replaced_path.find(['?', '#']) {
			Some(suffix_index) => {
				(replaced_path[..suffix_index].to_string(), replaced_path[suffix_index..].to_string())
			}
			None => {
				(replaced_path.clone(), String::new())
			}
		};
		let mut prefix = String::new();
		if let Some(scheme_length) = find_scheme_prefix_length(&text) {
			prefix = text[..scheme_length].to_string();
			text = text[scheme_length..].to_string();
		}
		else if text.starts_with('/') {
			prefix = String::from("/");
		}
		let mut segment_list: Vec<&str> = Vec::new();
		for segment in text.split('/') {
			if segment.is_empty() || segment == "." {
				continue;
			}
			if segment == ".." && segment_list.last().is_some_and(|last_segment| *last_segment != "..") {
				segment_list.pop();
				continue;
			}
			segment_list.push(segment);
		}
		let joined_path = segment_list.join("/");
		if !prefix.is_empty() && prefix != "/" && !joined_path.is_empty() {
			return format!("{}/{}{}", prefix, joined_path, suffix);
		}
		format!("{}{}{}", prefix, joined_path, suffix)
	}

	//==============================================================================
	/// 캐시에서 찾기. (셰이더 프로그램과 키가 같고 살아 있는 자산)
	///
	/// # 인자
	/// - `asset_key`: 자산 키. (normalize_key 로 정규화한다)
	/// - `shader_program`: 모델을 그릴 셰이더 프로그램. (렌더러의 get_shader_program)
	///
	/// # 돌려줌
	/// 찾은 자산. (없으면 None)
	//==============================================================================
	pub fn find(asset_key: &str, shader_program: &Rc<ShaderProgram>) -> Option<Rc<SkinnedModelAsset>> {
		let cache_key = (SkinnedModelAsset::normalize_key(asset_key), shader_program_address(shader_program));
		ASSET_CACHE_TABLE.with(|asset_cache_table| asset_cache_table.borrow().get(&cache_key).cloned())
	}

	//==============================================================================
	/// 캐시에 넣기. (자산에 키를 붙인다. 같은 자리에 있던 자산은 캐시에서만 빠지고 쓰던 인스턴스는 그대로 쓴다)
	///
	/// # 인자
	/// - `asset_key`: 자산 키. (normalize_key 로 정규화한다)
	/// - `asset`: 넣을 자산.
	///
	/// # 돌려줌
	/// 넣었으면 Ok. 지워진 자산이면 Err.
	///
	/// # 예제
	/// 번들에서 꺼낸 바이트로 지은 모델을 이름으로 나눠 쓰기
	/// ```rust,no_run
	/// use vanilla::{SkinnedModel, SkinnedModelAsset, SkinnedModelRenderer};
	///
	/// fn load_named_model(gl: &glow::Context, bytes: &[u8], skinned_model_renderer: &SkinnedModelRenderer) -> Result<SkinnedModel, String> {
	/// 	if let Some(found_asset) = SkinnedModelAsset::find("fox", &skinned_model_renderer.get_shader_program()) {
	/// 		return SkinnedModel::create_from_asset(&found_asset);
	/// 	}
	/// 	let skinned_model = SkinnedModel::load_from_bytes(gl, bytes, skinned_model_renderer)?;
	/// 	SkinnedModelAsset::register("fox", skinned_model.get_asset().ok_or("no asset")?)?;
	/// 	Ok(skinned_model)
	/// }
	/// ```
	//==============================================================================
	pub fn register(asset_key: &str, asset: &Rc<SkinnedModelAsset>) -> Result<(), String> {
		if asset.is_destroyed() {
			return Err(String::from("SkinnedModelAsset: destroyed asset cannot be registered."));
		}
		let normalized_key = SkinnedModelAsset::normalize_key(asset_key);
		let cache_key = (normalized_key.clone(), shader_program_address(&asset.shader_program));
		let previous_key = asset.key.borrow().clone();
		if previous_key.as_deref().is_some_and(|previous_key| previous_key != normalized_key) {
			asset.remove_from_cache();
		}
		let previous_asset = ASSET_CACHE_TABLE.with(|asset_cache_table| asset_cache_table.borrow_mut().insert(cache_key, asset.clone()));
		if let Some(previous_asset) = previous_asset {
			if !Rc::ptr_eq(&previous_asset, asset) {
				*previous_asset.key.borrow_mut() = None;
			}
		}
		*asset.key.borrow_mut() = Some(normalized_key);
		Ok(())
	}

	//==============================================================================
	/// 캐시 비우기. (모든 자산을 캐시에서 뺀다. 쓰던 인스턴스는 그대로 그리고, 마지막 인스턴스가 destroy 할 때 GPU 자원이 지워진다)
	/// - 다음 받기는 파일을 새로 풀어 새 자산을 짓는다.
	///
	/// # 예제
	/// 판을 바꿀 때 캐시를 비우기
	/// ```rust
	/// use vanilla::SkinnedModelAsset;
	///
	/// SkinnedModelAsset::clear_cache();
	/// assert_eq!(SkinnedModelAsset::get_cached_count(), 0);
	/// ```
	//==============================================================================
	pub fn clear_cache() {
		let asset_list: Vec<Rc<SkinnedModelAsset>> = ASSET_CACHE_TABLE.with(|asset_cache_table| asset_cache_table.borrow_mut().drain().map(|(_, asset)| asset).collect());
		for asset in asset_list {
			*asset.key.borrow_mut() = None;
		}
	}

	//==============================================================================
	/// 캐시에 든 자산 수. (모든 셰이더 프로그램 합)
	///
	/// # 돌려줌
	/// 캐시에 든 자산 수.
	//==============================================================================
	pub fn get_cached_count() -> usize {
		ASSET_CACHE_TABLE.with(|asset_cache_table| asset_cache_table.borrow().len())
	}

	//==============================================================================
	/// 참조 늘리기. (인스턴스가 자산을 나눠 쓰기 시작할 때, SkinnedModel::create_from_asset 이 부른다)
	///
	/// # 돌려줌
	/// 늘렸으면 Ok. 지워진 자산이면 Err.
	//==============================================================================
	pub fn retain(&self) -> Result<(), String> {
		if self.is_destroyed.get() {
			return Err(String::from("SkinnedModelAsset: asset already destroyed."));
		}
		self.reference_count.set(self.reference_count.get() + 1);
		Ok(())
	}

	//==============================================================================
	/// 참조 줄이기. (인스턴스가 destroy 할 때, 0 이 되면 GPU 자원을 지우고 캐시에서 빠진다)
	///
	/// # 인자
	/// - `gl`: 자원을 지은 GL 컨텍스트.
	///
	/// # 돌려줌
	/// 이번에 자원을 지웠으면 true.
	//==============================================================================
	pub fn release(&self, gl: &glow::Context) -> bool {
		if self.is_destroyed.get() {
			return false;
		}
		let reference_count = self.reference_count.get().saturating_sub(1);
		self.reference_count.set(reference_count);
		if reference_count > 0 {
			return false;
		}
		self.remove_from_cache();
		self.destroy_resources(gl);
		true
	}

	//==============================================================================
	/// 캐시에서 이 자산 빼기. (같은 자리에 다른 자산이 들어 있으면 건드리지 않는다)
	//==============================================================================
	fn remove_from_cache(&self) {
		let key = self.key.borrow_mut().take();
		if let Some(key) = key {
			let cache_key = (key, shader_program_address(&self.shader_program));
			// 캐시가 든 Rc 는 빌림이 끝난 뒤 놓는다. (마지막 Rc 면 이 자산이 지워져 빌림이 겹치지 않게)
			let removed_asset = ASSET_CACHE_TABLE.with(|asset_cache_table| {
				let mut asset_cache_table = asset_cache_table.borrow_mut();
				let is_self = asset_cache_table.get(&cache_key).is_some_and(|cached_asset| std::ptr::eq(Rc::as_ptr(cached_asset), self));
				if is_self {
					asset_cache_table.remove(&cache_key)
				}
				else {
					None
				}
			});
			drop(removed_asset);
		}
	}

	//==============================================================================
	/// GPU 자원 지우기. (원형 드로어블의 버텍스 어레이, 버퍼, 모프 텍스처와 머티리얼 원형의 텍스처)
	//==============================================================================
	fn destroy_resources(&self, gl: &glow::Context) {
		for drawable in self.drawable_list.borrow_mut().drain(..) {
			unsafe {
				gl.delete_vertex_array(drawable.vertex_array);
				for vertex_buffer in drawable.vertex_buffer_list.iter() {
					gl.delete_buffer(*vertex_buffer);
				}
				if let Some(index_buffer) = drawable.index_buffer {
					gl.delete_buffer(index_buffer);
				}
				if let Some(morph_texture) = drawable.morph_texture {
					gl.delete_texture(morph_texture);
				}
			}
		}
		for material in self.material_list.borrow_mut().drain(..) {
			material.destroy(gl);
		}
		self.is_destroyed.set(true);
	}

	//==============================================================================
	/// 자산 키 반환.
	///
	/// # 돌려줌
	/// 캐시에 넣은 키. (넣지 않았거나 캐시에서 빠졌으면 None)
	//==============================================================================
	pub fn get_key(&self) -> Option<String> {
		self.key.borrow().clone()
	}

	//==============================================================================
	/// 참조 수 반환. (이 자산을 나눠 쓰는 살아 있는 인스턴스 수)
	///
	/// # 돌려줌
	/// 참조 수. (지워졌으면 0)
	//==============================================================================
	pub fn get_reference_count(&self) -> usize {
		self.reference_count.get()
	}

	//==============================================================================
	/// 지워졌는지.
	///
	/// # 돌려줌
	/// 마지막 인스턴스가 destroy 해 GPU 자원을 지웠으면 true.
	//==============================================================================
	pub fn is_destroyed(&self) -> bool {
		self.is_destroyed.get()
	}

	//==============================================================================
	/// GPU 바이트 수. (정점, 인덱스 버퍼와 모프 텍스처와 머티리얼 원형 텍스처, 밉맵 포함, 인스턴스 수와 상관없이 한 벌)
	///
	/// # 돌려줌
	/// 이 자산이 GPU 에 올린 바이트 수. (지워졌으면 0)
	//==============================================================================
	pub fn get_gpu_byte_count(&self) -> usize {
		if self.is_destroyed.get() {
			return 0;
		}
		let texture_byte_count: usize = self.material_list.borrow().iter().map(|material| material.get_texture_byte_count()).sum();
		self.buffer_byte_count + texture_byte_count
	}

	//==============================================================================
	/// 셰이더 프로그램 반환.
	///
	/// # 돌려줌
	/// 버텍스 어레이와 머티리얼이 쓰는 셰이더 프로그램.
	//==============================================================================
	pub fn get_shader_program(&self) -> &Rc<ShaderProgram> {
		&self.shader_program
	}

	//==============================================================================
	/// 원형 드로어블 목록 반환. (인스턴스는 이것을 베껴 버퍼와 머티리얼 원형을 나눠 쓴다)
	///
	/// # 돌려줌
	/// 원형 드로어블 목록. (지워졌으면 비어 있다)
	//==============================================================================
	pub fn get_drawable_list(&self) -> Ref<'_, Vec<Drawable>> {
		self.drawable_list.borrow()
	}

	//==============================================================================
	/// 머티리얼 원형 목록 반환. (기본 머티리얼이 마지막, 이 자산이 지었고 마지막 인스턴스가 destroy 할 때 지운다)
	///
	/// # 돌려줌
	/// 머티리얼 원형 목록. (sharedMaterial, 값을 바꾸면 이 자산을 쓰는 모든 인스턴스가 바뀐다)
	//==============================================================================
	pub fn get_material_list(&self) -> Ref<'_, Vec<Rc<Material>>> {
		self.material_list.borrow()
	}

	//==============================================================================
	/// 노드 원형 목록 반환. (올릴 때의 노드, 인스턴스는 이것을 베껴 포즈를 따로 든다)
	///
	/// # 돌려줌
	/// 노드 원형 목록.
	//==============================================================================
	pub fn get_node_list(&self) -> &[SkinnedModelNode] {
		&self.node_list
	}

	//==============================================================================
	/// 루트 노드 목록 반환.
	///
	/// # 돌려줌
	/// 루트 노드 번호 목록.
	//==============================================================================
	pub fn get_root_node_indices(&self) -> &[usize] {
		&self.root_node_indices
	}

	//==============================================================================
	/// 스킨 원형 목록 반환. (조인트와 역바인드 행렬을 인스턴스가 베낀다)
	///
	/// # 돌려줌
	/// 스킨 원형 목록.
	//==============================================================================
	pub fn get_skin_list(&self) -> &[Skin] {
		&self.skin_list
	}

	//==============================================================================
	/// 애니메이션 클립 목록 반환. (인스턴스가 모두 같은 목록을 나눠 쓴다, add_animations_from_fbx_* 로 더한 클립도 모두에게 보인다)
	///
	/// # 돌려줌
	/// 나눠 든 클립 목록.
	//==============================================================================
	pub fn get_animation_list(&self) -> &Rc<RefCell<Vec<AnimationClip>>> {
		&self.animation_list
	}
}

//==============================================================================
/// 주소틀 머리 길이. ("주소틀://호스트" 까지, 없으면 None)
//==============================================================================
fn find_scheme_prefix_length(text: &str) -> Option<usize> {
	let scheme_end = text.find("://")?;
	let scheme = &text[..scheme_end];
	let mut scheme_characters = scheme.chars();
	let first_character = scheme_characters.next()?;
	if !first_character.is_ascii_alphabetic() {
		return None;
	}
	if !scheme_characters.all(|character| character.is_ascii_alphanumeric() || character == '+' || character == '.' || character == '-') {
		return None;
	}
	let host_start = scheme_end + 3;
	let host_length = text[host_start..].find('/').unwrap_or(text.len() - host_start);
	Some(host_start + host_length)
}

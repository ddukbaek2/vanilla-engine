//==============================================================================
//! 툰 그리기 줄. (vanilla.js graphics3d/toon/toondrawqueue.js 의 ToonDrawQueue)
//! - 한 프레임에 그릴 모형을 담아 두었다가 ToonRenderer 로 차례를 맞춰 그린다.
//! - 차례: (화면 공간 바깥선의 선의 판) → 단계 BeforeOpaque → 불투명한 몸 → AfterOpaque → 불투명한 것의 바깥선 껍데기 → 화면 공간 바깥선 → AfterOutline
//!   → 비치는 것(카메라에서 먼 것부터, 깊이는 쓰지 않음) → AfterTransparent.
//!   몸을 다 그린 뒤 바깥선을 몰아 그려 프로그램을 덜 바꾸고, 비치는 것은 뒤의 것이 비쳐 보이게 맨 끝에 먼 것부터 섞는다.
//! - 비치는 것: 꼴의 alpha 나 tint_color 의 알파가 1 보다 작거나 blend_mode 가 Alpha 가 아닌 것. 바깥선도 함께(has_outline) 그린다.
//! - 묶음: 꼴의 group 이 같은 비치는 것은 한 덩어리로 가운데 깊이(담긴 것의 평균)로 정렬하고 담은 차례로 그린다.
//!   is_depth_prepassed 가 묶음 안에 하나라도 있으면 그 덩어리는 깊이만 먼저 그린 뒤 같은 깊이(LEQUAL)인 화소에만 색을 칠한다.
//!   여러 부품이 겹친 홀로그램이 두 겹으로 밝지 않고 한 덩어리로 비친다. 덩어리끼리는 먼 것부터라 서로 비친다.
//! - 화면 공간 바깥선: set_screen_outline(ToonScreenOutline) 을 주면 draw 가 몸 앞에 불투명한 것을 선의 판에 그리고, 바깥선 껍데기 뒤에 섞는다.
//!   번호는 꼴의 line_id (None 이면 담은 차례 + 1, 인스턴스는 ToonInstance 의 line_id 가 먼저), 모형마다 has_screen_outline, has_crease_line 으로 끄고 켠다.
//!   비치는 것은 선의 판에 그리지 않는다. 화면 사각형(ToonRenderer::set_screen_rect)이 있으면 그 사각형을 선의 판의 그리는 영역으로 쓴다.
//!   러스트 판은 바깥선을 &'a mut 로 빌려 RefCell 에 담는다. (draw(&self) 에서 고쳐 쓴다)
//! - 단계 콜백: add_pass(stage, callback) 의 callback(gl, renderer) 를 그 자리에서 부른다. 부른 뒤 렌더러가 프레임의 그리기 상태를 다시 건다.
//! - 인스턴스 묶음(add_mesh_instanced)은 불투명이면 몸과 바깥선을 한 번에 그리고, 비치면 인스턴스 자리의 가운데로 한 덩어리로 정렬한다.
//! - 그림자: draw_shadows(gl, renderer, shadow_map) 가 casts_shadow 가 true 인 불투명한 것만 그림자 깊이로 그린다. draw 보다 먼저 부른다.
//!   점광과 스포트라이트 그림자는 draw_local_shadows(gl, renderer, shadow_atlas) 가 아틀라스의 칸마다 같은 것을 그린다. (렌더러에 set_local_shadows 로 건다)
//! - 사용: 매 프레임 clear() → add_mesh / add_mesh_instanced / add_pass → (draw_shadows) → draw(gl, renderer, camera) 나 draw_with_matrices(gl, renderer, 보기 행렬, 투영 행렬)
//!   draw 는 renderer.begin_frame_with_camera 와 end_frame 을 스스로 부른다. draw_with_matrices 는 begin_frame_with_matrices 를 부른다. (직교 원 행렬 카메라)
//!   화면 사각형(ToonRenderer::set_screen_rect)은 그 전에 건다. clear 는 담은 모형과 단계 콜백을 모두 비운다. (화면 공간 바깥선은 그대로 둔다)
//! - 원본은 담은 것을 객체 참조로 든다. 러스트 판은 모형과 인스턴스 목록, 단계 콜백을 빌려 담으므로 줄이 그 수명 'a 를 갖는다. (프레임마다 새로 짓거나 clear 뒤 다시 담는다)
//!   단계 콜백은 FnMut 이라 RefCell 에 담아 draw(&self) 에서 부른다.
//==============================================================================
use std::cell::RefCell;
use std::collections::HashMap;


use crate::base::matrix4::Matrix4;
use crate::core::graphic::GlTexture;
use crate::graphics3d::camera3d::Camera3D;
use crate::graphics3d::shadowmap::{ShadowAtlas, ShadowMap};
use crate::graphics3d::toon::toonmaterial::ToonBlendMode;
use crate::graphics3d::toon::toonrenderer::{ToonDrawOptions, ToonInstance, ToonMesh, ToonRenderer};
use crate::graphics3d::toon::toonscreenoutline::{ToonLineDrawOptions, ToonScreenOutline, TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID};

//==============================================================================
/// 그리기 줄의 단계. (add_pass 의 콜백을 부를 자리)
/// - BeforeOpaque: 불투명한 몸 앞. (평면 거울에 비친 모습, 하늘)
/// - AfterOpaque: 불투명한 몸 뒤, 바깥선 껍데기 앞.
/// - AfterOutline: 바깥선 껍데기와 화면 공간 바깥선 뒤, 비치는 것 앞.
/// - AfterTransparent: 비치는 것 뒤. (효과)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonDrawStage {
	/// 불투명한 몸 앞. (평면 거울에 비친 모습, 하늘)
	BeforeOpaque,
	/// 불투명한 몸 뒤, 바깥선 껍데기 앞.
	AfterOpaque,
	/// 바깥선 껍데기와 화면 공간 바깥선 뒤, 비치는 것 앞.
	AfterOutline,
	/// 비치는 것 뒤. (효과)
	AfterTransparent,
}

//==============================================================================
/// 단계 콜백. (gl, 렌더러)
//==============================================================================
pub type ToonPassCallback<'a> = Box<dyn FnMut(&glow::Context, &mut ToonRenderer) + 'a>;

//==============================================================================
/// 담은 것 하나.
//==============================================================================
struct ToonDrawItem<'a> {
	mesh: &'a ToonMesh,
	model_matrix: Matrix4,
	instance_list: Option<&'a [ToonInstance]>,
	options: ToonDrawOptions<'a>,
	is_transparent: bool,
	depth_position: [f64; 3],
}

//==============================================================================
/// 비치는 것의 한 덩어리. (묶음이거나 하나, 깊이 합과 담은 차례)
//==============================================================================
struct ToonTransparentEntry {
	depth_sum: f64,
	item_index_list: Vec<usize>,
	is_depth_prepassed: bool,
}

//==============================================================================
/// 툰 그리기 줄. (머리말 참고)
///
/// # 예제
/// 모형마다 바깥선 꼴 정하기
/// ```rust,no_run
/// use vanilla::{create_box_geometry, create_faceted_chunk_geometry, Camera3D, Matrix4, MeshBuilder, ToonDrawOptions, ToonDrawQueue, ToonPartOptions, ToonRenderer, ToonScreenOutline, Vector3};
///
/// fn draw_scene(gl: &glow::Context) -> Result<(), String> {
/// 	let mut renderer = ToonRenderer::new(gl)?;
/// 	let mut floor_builder = MeshBuilder::new();
/// 	floor_builder.add_part(&create_box_geometry(8.0, 0.2, 8.0), &Matrix4::create_translation(0.0, -0.1, 0.0), "#708090", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
/// 	let floor_mesh = renderer.create_mesh(gl, &floor_builder.read_arrays()).ok_or("floor mesh")?;
/// 	let mut gem_builder = MeshBuilder::new();
/// 	gem_builder.add_part(&create_faceted_chunk_geometry(0.5, &[1.0, 0.9, 1.1], [1.0, 1.0, 1.0]), &Matrix4::create_translation(0.0, 0.5, 0.0), "#a0c0ff", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
/// 	let gem_mesh = renderer.create_mesh(gl, &gem_builder.read_arrays()).ok_or("gem mesh")?;
/// 	let mut camera = Camera3D::new();
/// 	camera.set_perspective(0.8, 0.1, 100.0);
/// 	camera.look_at(Vector3::create(0.0, 2.0, 4.0), Vector3::create(0.0, 0.4, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// 	let mut screen_outline = ToonScreenOutline::new(gl)?;
/// 	let mut queue = ToonDrawQueue::new();
/// 	queue.set_screen_outline(Some(&mut screen_outline));
/// 	queue.add_mesh(&floor_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(1), has_screen_outline: false, ..ToonDrawOptions::default() });
/// 	queue.add_mesh(&gem_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(2), has_crease_line: false, ..ToonDrawOptions::default() });
/// 	queue.draw(gl, &mut renderer, &camera);
/// 	Ok(())
/// }
/// ```
//==============================================================================
#[derive(Default)]
pub struct ToonDrawQueue<'a> {
	item_list: Vec<ToonDrawItem<'a>>,
	pass_list: RefCell<Vec<(ToonDrawStage, ToonPassCallback<'a>)>>,
	screen_outline: RefCell<Option<&'a mut ToonScreenOutline>>,
	/// 정렬한 불투명한 것의 차례. (원본 opaqueItemList, 담거나 비우면 None, 처음 그릴 때 만든다)
	opaque_index_list: RefCell<Option<Vec<usize>>>,
}

thread_local! {
	/// 정렬 번호. (원본 sortIdTable, 처음 본 차례로 1 부터, 재질은 번호, 모형은 주소, 텍스처는 GL 이름이 키)
	static SORT_ID_TABLE: RefCell<(u64, HashMap<u32, u64>, HashMap<usize, u64>, HashMap<GlTexture, u64>)> = RefCell::new((1, HashMap::new(), HashMap::new(), HashMap::new()));
}

//==============================================================================
/// 정렬 번호 셋 읽기. (원본 readSortId, 재질, 모형, 바탕색 텍스처, 없으면 0)
//==============================================================================
fn read_sort_ids(item: &ToonDrawItem) -> (u64, u64, u64) {
	SORT_ID_TABLE.with(|sort_id_table| {
		let mut sort_id_table = sort_id_table.borrow_mut();
		let (next_sort_id, material_table, mesh_table, texture_table) = &mut *sort_id_table;
		let material_id = match item.options.material {
			Some(material) => *material_table.entry(material.get_id()).or_insert_with(|| { *next_sort_id += 1; *next_sort_id - 1 }),
			None => 0,
		};
		let mesh_id = *mesh_table.entry(item.mesh as *const ToonMesh as usize).or_insert_with(|| { *next_sort_id += 1; *next_sort_id - 1 });
		let texture_id = match item.options.base_color_texture {
			Some(texture) => *texture_table.entry(texture).or_insert_with(|| { *next_sort_id += 1; *next_sort_id - 1 }),
			None => 0,
		};
		(material_id, mesh_id, texture_id)
	})
}

//==============================================================================
/// 그릴 꼴이 비치는지. (alpha 나 틴트의 알파가 1 보다 작거나 섞는 방식이 곧은 알파가 아님)
//==============================================================================
fn is_transparent_options(options: &ToonDrawOptions) -> bool {
	options.alpha < 1.0 || options.tint_color[3] < 1.0 || options.blend_mode != ToonBlendMode::Alpha
}

impl<'a> ToonDrawQueue<'a> {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 빈 그리기 줄. (화면 공간 바깥선을 맡기지 않은 것)
	//==============================================================================
	pub fn new() -> ToonDrawQueue<'a> {
		ToonDrawQueue { item_list: Vec::new(), pass_list: RefCell::new(Vec::new()), screen_outline: RefCell::new(None), opaque_index_list: RefCell::new(None) }
	}

	//==============================================================================
	/// 비우기. (단계 콜백도 비운다, 화면 공간 바깥선은 그대로)
	//==============================================================================
	pub fn clear(&mut self) {
		self.item_list.clear();
		*self.opaque_index_list.get_mut() = None;
		self.pass_list.borrow_mut().clear();
	}

	//==============================================================================
	/// 담은 수 반환.
	///
	/// # 돌려줌
	/// 담은 모형과 인스턴스 묶음 수.
	//==============================================================================
	pub fn get_item_count(&self) -> usize {
		self.item_list.len()
	}

	//==============================================================================
	/// 화면 공간 바깥선 맡기기. (draw 가 선의 판을 그리고 바깥선 껍데기 뒤에 섞는다, None 이면 맡기지 않음)
	///
	/// # 인자
	/// - `screen_outline`: ToonScreenOutline 이나 None. (줄의 수명 동안 빌린다)
	//==============================================================================
	pub fn set_screen_outline(&mut self, screen_outline: Option<&'a mut ToonScreenOutline>) {
		self.screen_outline = RefCell::new(screen_outline);
	}

	//==============================================================================
	/// 화면 공간 바깥선을 맡겼는지.
	///
	/// # 돌려줌
	/// 맡겼으면 true.
	//==============================================================================
	pub fn has_screen_outline(&self) -> bool {
		self.screen_outline.borrow().is_some()
	}

	//==============================================================================
	/// 단계 콜백 담기. (ToonDrawStage 의 자리에서 callback(gl, renderer) 를 부른다, 담은 차례로)
	///
	/// # 인자
	/// - `stage`: ToonDrawStage.
	/// - `callback`: (gl, renderer) 를 받는다. 부른 뒤 렌더러가 프레임의 그리기 상태를 다시 건다.
	//==============================================================================
	pub fn add_pass(&mut self, stage: ToonDrawStage, callback: impl FnMut(&glow::Context, &mut ToonRenderer) + 'a) {
		self.pass_list.borrow_mut().push((stage, Box::new(callback)));
	}

	//==============================================================================
	/// 모형 하나 담기.
	///
	/// # 인자
	/// - `mesh`: ToonRenderer::create_mesh 의 결과.
	/// - `model_matrix`: 모형 행렬. (담을 때 베껴 둔다)
	/// - `options`: ToonRenderer::draw_mesh 의 꼴. (casts_shadow, group, is_depth_prepassed, line_id, has_screen_outline, has_crease_line 도 이 줄이 본다)
	//==============================================================================
	pub fn add_mesh(&mut self, mesh: &'a ToonMesh, model_matrix: &Matrix4, options: ToonDrawOptions<'a>) {
		let (center, _) = mesh.get_bounding_sphere();
		let depth_point = model_matrix.transform_point(center[0], center[1], center[2]);
		*self.opaque_index_list.get_mut() = None;
		self.item_list.push(ToonDrawItem { mesh, model_matrix: *model_matrix, instance_list: None, options, is_transparent: is_transparent_options(&options), depth_position: [depth_point.x, depth_point.y, depth_point.z] });
	}

	//==============================================================================
	/// 같은 모형 여럿 담기. (GPU 인스턴싱, 비어 있으면 담지 않는다)
	///
	/// # 인자
	/// - `mesh`: ToonRenderer::create_mesh 의 결과.
	/// - `instance_list`: ToonInstance 의 목록. (모형 행렬, 틴트 색, 선 번호)
	/// - `options`: add_mesh 와 같다.
	//==============================================================================
	pub fn add_mesh_instanced(&mut self, mesh: &'a ToonMesh, instance_list: &'a [ToonInstance], options: ToonDrawOptions<'a>) {
		if instance_list.is_empty() {
			return;
		}
		let mut depth_position = [0.0f64; 3];
		for instance in instance_list {
			for (axis, value) in depth_position.iter_mut().enumerate() {
				*value += (instance.model_matrix.elements[12 + axis]) as f64;
			}
		}
		for value in depth_position.iter_mut() {
			*value /= instance_list.len() as f64;
		}
		*self.opaque_index_list.get_mut() = None;
		self.item_list.push(ToonDrawItem { mesh, model_matrix: Matrix4::create_identity(), instance_list: Some(instance_list), options, is_transparent: is_transparent_options(&options), depth_position });
	}

	//==============================================================================
	/// 그림자 깊이 그리기. (불투명하고 casts_shadow 가 true 인 것만, 앞의 프레임버퍼와 뷰포트는 되돌린다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `renderer`: ToonRenderer.
	/// - `shadow_map`: ShadowMap.
	//==============================================================================
	pub fn draw_shadows(&self, gl: &glow::Context, renderer: &mut ToonRenderer, shadow_map: &ShadowMap) {
		renderer.begin_shadow_pass(gl, shadow_map);
		for &item_index in self.read_opaque_index_list().iter() {
			let item = &self.item_list[item_index];
			if !item.options.casts_shadow {
				continue;
			}
			ToonDrawQueue::draw_item(gl, renderer, item, item.options);
		}
		renderer.end_shadow_pass(gl);
	}

	//==============================================================================
	/// 점광과 스포트라이트 그림자 깊이 그리기. (불투명하고 casts_shadow 가 true 인 것을 아틀라스의 칸마다, 앞의 프레임버퍼와 뷰포트는 되돌린다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `renderer`: ToonRenderer.
	/// - `shadow_atlas`: ShadowAtlas. (update_lights 를 부른 것)
	//==============================================================================
	pub fn draw_local_shadows(&self, gl: &glow::Context, renderer: &mut ToonRenderer, shadow_atlas: &ShadowAtlas) {
		renderer.begin_shadow_atlas_pass(gl, shadow_atlas);
		for tile_index in 0..shadow_atlas.get_tile_count() {
			renderer.set_shadow_atlas_tile(gl, shadow_atlas, tile_index);
			for &item_index in self.read_opaque_index_list().iter() {
				let item = &self.item_list[item_index];
				if !item.options.casts_shadow {
					continue;
				}
				ToonDrawQueue::draw_item(gl, renderer, item, item.options);
			}
		}
		renderer.end_shadow_pass(gl);
	}

	//==============================================================================
	/// 담은 것 그리기. (화면 공간 바깥선이 있으면 선의 판부터, begin_frame_with_camera 와 end_frame 을 스스로 부른다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `renderer`: ToonRenderer.
	/// - `camera`: Camera3D.
	//==============================================================================
	pub fn draw(&self, gl: &glow::Context, renderer: &mut ToonRenderer, camera: &Camera3D) {
		self.draw_line_ids(gl, renderer, &camera.get_view_matrix(), &camera.get_projection_matrix());
		renderer.begin_frame_with_camera(gl, camera);
		self.draw_stages(gl, renderer, &|position: [f64; 3]| camera.read_view_depth(position[0], position[1], position[2]));
		renderer.end_frame(gl);
	}

	//==============================================================================
	/// 보기와 투영 행렬로 담은 것 그리기. (Camera3D 없이, begin_frame_with_matrices 와 end_frame 을 스스로 부른다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `renderer`: ToonRenderer.
	/// - `view_matrix`: 보기 행렬.
	/// - `projection_matrix`: 투영 행렬.
	//==============================================================================
	pub fn draw_with_matrices(&self, gl: &glow::Context, renderer: &mut ToonRenderer, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
		self.draw_line_ids(gl, renderer, view_matrix, projection_matrix);
		renderer.begin_frame_with_matrices(gl, view_matrix, projection_matrix);
		let elements = view_matrix.read_elements();
		self.draw_stages(gl, renderer, &|position: [f64; 3]| -(elements[2] * position[0] + elements[6] * position[1] + elements[10] * position[2] + elements[14]));
		renderer.end_frame(gl);
	}

	//==============================================================================
	/// 화면 공간 바깥선의 선의 판 그리기. (맡긴 것이 없으면 아무것도 하지 않음, 불투명한 것만, 화면 사각형이 있으면 그 사각형을 그리는 영역으로)
	//==============================================================================
	fn draw_line_ids(&self, gl: &glow::Context, renderer: &ToonRenderer, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
		let mut screen_outline_slot = self.screen_outline.borrow_mut();
		let screen_outline = match screen_outline_slot.as_mut() {
			Some(screen_outline) => {
				screen_outline
			}
			None => {
				return;
			}
		};
		let mut viewport = [0i32; 4];
		unsafe {
			crate::core::glstatecache::get_parameter_i32_slice(gl, glow::VIEWPORT, &mut viewport);
			if let Some(screen_rect) = renderer.get_screen_rect() {
				let left = screen_rect.position.x.floor() as i32;
				let bottom = (viewport[3] as f64 - screen_rect.position.y - screen_rect.size.y).floor() as i32;
				crate::core::glstatecache::viewport(gl, viewport[0] + left, viewport[1] + bottom, (screen_rect.size.x.round() as i32).max(1), (screen_rect.size.y.round() as i32).max(1));
			}
		}
		if screen_outline.begin_line_ids(gl, view_matrix, projection_matrix) {
			for (item_index, item) in self.item_list.iter().enumerate() {
				if item.is_transparent {
					continue;
				}
				let line_id = item.options.line_id.unwrap_or(item_index as u32 % TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID + 1);
				let line_options = ToonLineDrawOptions {
					joint_matrices: item.options.joint_matrices,
					is_culled: item.options.is_culled,
					index_start: item.options.index_start,
					index_count: if item.options.index_count > 0 { Some(item.options.index_count) } else { None },
					is_mirrored: item.options.is_mirrored,
					has_screen_outline: item.options.has_screen_outline,
					has_crease_line: item.options.has_crease_line,
				};
				match item.instance_list {
					Some(instance_list) => {
						screen_outline.draw_mesh_instanced(gl, item.mesh, instance_list, line_id, line_options);
					}
					None => {
						screen_outline.draw_mesh(gl, item.mesh, &item.model_matrix, line_id, line_options);
					}
				}
			}
			screen_outline.end_line_ids(gl);
		}
		unsafe {
			crate::core::glstatecache::viewport(gl, viewport[0], viewport[1], viewport[2], viewport[3]);
		}
	}

	//==============================================================================
	/// 한 단계의 콜백 부르기. (부른 뒤 프레임의 그리기 상태를 다시 건다)
	//==============================================================================
	fn run_passes(&self, gl: &glow::Context, renderer: &mut ToonRenderer, stage: ToonDrawStage) {
		let mut is_run = false;
		for (pass_stage, callback) in self.pass_list.borrow_mut().iter_mut() {
			if *pass_stage != stage {
				continue;
			}
			callback(gl, renderer);
			// 콜백은 glow 로 상태를 바로 바꿀 수 있다. (상태 캐시를 거치지 않음, 뒤의 apply_frame_state 가 다시 건다)
			crate::core::glstatecache::reset(gl);
			is_run = true;
		}
		if is_run {
			renderer.apply_frame_state(gl);
		}
	}

	//==============================================================================
	/// 정렬한 불투명한 것의 차례 읽기. (원본 readOpaqueItemList, 재질, 모형, 바탕색 텍스처의 차례, 같으면 담은 차례, 담은 것이 그대로면 앞서 만든 것을 쓴다)
	/// - 그림자 깊이 판은 차례와 결과가 무관하다. 색 판은 깊이가 꼭 같은 면만 차례가 드러난다.
	//==============================================================================
	fn read_opaque_index_list(&self) -> std::cell::Ref<'_, Vec<usize>> {
		if self.opaque_index_list.borrow().is_none() {
			let mut key_list: Vec<((u64, u64, u64), usize)> = self.item_list.iter().enumerate().filter(|(_, item)| !item.is_transparent).map(|(item_index, item)| (read_sort_ids(item), item_index)).collect();
			key_list.sort_unstable();
			*self.opaque_index_list.borrow_mut() = Some(key_list.into_iter().map(|(_, item_index)| item_index).collect());
		}
		std::cell::Ref::map(self.opaque_index_list.borrow(), |opaque_index_list| opaque_index_list.as_ref().unwrap())
	}

	//==============================================================================
	/// 단계마다 그리기. (begin_frame 과 end_frame 사이, read_depth(자리) 는 카메라에서의 보기 깊이)
	//==============================================================================
	fn draw_stages(&self, gl: &glow::Context, renderer: &mut ToonRenderer, read_depth: &dyn Fn([f64; 3]) -> f64) {
		self.run_passes(gl, renderer, ToonDrawStage::BeforeOpaque);

		// 1. 불투명한 몸. (인스턴스 묶음은 바깥선까지 한 번에, 화면 밖이면 draw_mesh 가 걸러 센다)
		let mut outline_index_list: Vec<usize> = Vec::new();
		for &item_index in self.read_opaque_index_list().iter() {
			let item = &self.item_list[item_index];
			if item.instance_list.is_some() || (item.options.is_culled && !renderer.is_visible(item.mesh, &item.model_matrix)) {
				ToonDrawQueue::draw_item(gl, renderer, item, item.options);
				continue;
			}
			ToonDrawQueue::draw_item(gl, renderer, item, ToonDrawOptions { has_outline: false, ..item.options });
			if item.options.has_outline {
				outline_index_list.push(item_index);
			}
		}
		self.run_passes(gl, renderer, ToonDrawStage::AfterOpaque);

		// 2. 불투명한 것의 바깥선 껍데기와 화면 공간 바깥선.
		for item_index in outline_index_list {
			let item = &self.item_list[item_index];
			renderer.draw_mesh_outline(gl, item.mesh, &item.model_matrix, item.options);
		}
		if let Some(screen_outline) = self.screen_outline.borrow_mut().as_mut() {
			screen_outline.draw_outline(gl);
			renderer.apply_frame_state(gl);
		}
		self.run_passes(gl, renderer, ToonDrawStage::AfterOutline);

		// 3. 비치는 것. (묶음은 한 덩어리로, 먼 것부터, 깊이는 쓰지 않음)
		let mut entry_list: Vec<ToonTransparentEntry> = Vec::new();
		let mut group_table: HashMap<u32, usize> = HashMap::new();
		for (item_index, item) in self.item_list.iter().enumerate() {
			if !item.is_transparent {
				continue;
			}
			let depth = read_depth(item.depth_position);
			match item.options.group {
				Some(group) => {
					let entry_index = *group_table.entry(group).or_insert_with(|| {
						entry_list.push(ToonTransparentEntry { depth_sum: 0.0, item_index_list: Vec::new(), is_depth_prepassed: false });
						entry_list.len() - 1
					});
					let entry = &mut entry_list[entry_index];
					entry.depth_sum += depth;
					entry.item_index_list.push(item_index);
					entry.is_depth_prepassed = entry.is_depth_prepassed || item.options.is_depth_prepassed;
				}
				None => {
					entry_list.push(ToonTransparentEntry { depth_sum: depth, item_index_list: vec![item_index], is_depth_prepassed: item.options.is_depth_prepassed });
				}
			}
		}
		if !entry_list.is_empty() {
			let read_entry_depth = |entry: &ToonTransparentEntry| entry.depth_sum / entry.item_index_list.len() as f64;
			entry_list.sort_by(|first, second| read_entry_depth(second).total_cmp(&read_entry_depth(first)));
			unsafe {
				crate::core::glstatecache::depth_mask(gl, false);
			}
			for entry in entry_list.iter() {
				if entry.is_depth_prepassed {
					unsafe {
						crate::core::glstatecache::color_mask(gl, false, false, false, false);
						crate::core::glstatecache::depth_mask(gl, true);
					}
					for item_index in entry.item_index_list.iter() {
						let item = &self.item_list[*item_index];
						ToonDrawQueue::draw_item(gl, renderer, item, item.options);
					}
					unsafe {
						crate::core::glstatecache::color_mask(gl, true, true, true, true);
						crate::core::glstatecache::depth_mask(gl, false);
					}
				}
				for item_index in entry.item_index_list.iter() {
					let item = &self.item_list[*item_index];
					ToonDrawQueue::draw_item(gl, renderer, item, item.options);
				}
			}
			unsafe {
				crate::core::glstatecache::depth_mask(gl, true);
			}
		}
		self.run_passes(gl, renderer, ToonDrawStage::AfterTransparent);
	}

	//==============================================================================
	/// 하나 그리기. (모형 하나는 draw_mesh, 인스턴스 묶음은 draw_mesh_instanced)
	//==============================================================================
	fn draw_item(gl: &glow::Context, renderer: &mut ToonRenderer, item: &ToonDrawItem, options: ToonDrawOptions) {
		match item.instance_list {
			Some(instance_list) => {
				renderer.draw_mesh_instanced(gl, item.mesh, instance_list, options);
			}
			None => {
				renderer.draw_mesh(gl, item.mesh, &item.model_matrix, options);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 비치는 꼴 가리기. (기본 꼴은 불투명, alpha 나 틴트의 알파가 1 보다 작거나 섞는 방식이 곧은 알파가 아니면 비침)
	//==============================================================================
	#[test]
	fn transparent_options_follow_alpha_and_tint() {
		assert!(!is_transparent_options(&ToonDrawOptions::default()));
		assert!(is_transparent_options(&ToonDrawOptions { alpha: 0.5, ..ToonDrawOptions::default() }));
		assert!(is_transparent_options(&ToonDrawOptions { tint_color: [1.0, 1.0, 1.0, 0.9], ..ToonDrawOptions::default() }));
		assert!(is_transparent_options(&ToonDrawOptions { blend_mode: ToonBlendMode::Additive, ..ToonDrawOptions::default() }));
	}
}

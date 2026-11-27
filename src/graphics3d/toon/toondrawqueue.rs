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
//! - 모서리 어둠: set_post_process(ToonPostProcess) 를 주고 후처리에서 set_ambient_occlusion 을 켜면 선의 판 뒤, 몸 앞에 셈해 렌더러에 걸고 end_frame 뒤에 푼다. (바깥선이 없으면 선 없이 선의 판만 그린다)
//! - 단계 콜백: add_pass(stage, callback) 의 callback(gl, renderer) 를 그 자리에서 부른다. 부른 뒤 렌더러가 프레임의 그리기 상태를 다시 건다.
//! - 인스턴스 묶음(add_mesh_instanced)은 불투명이면 몸과 바깥선을 한 번에 그리고, 비치면 인스턴스 자리의 가운데로 한 덩어리로 정렬한다.
//! - 그림자: draw_shadows(gl, renderer, shadow_map) 가 casts_shadow 가 true 인 불투명한 것만 그림자 깊이로 그린다. draw 보다 먼저 부른다.
//!   점광과 스포트라이트 그림자는 draw_local_shadows(gl, renderer, shadow_atlas) 가 아틀라스의 칸마다 같은 것을 그린다. (렌더러에 set_local_shadows 로 건다)
//!   그림자 판의 set_update_mode(ShadowUpdateMode) 를 따른다. 기본 OnChange 는 빛과 드리우개가 그대로면 다시 그리지 않고, 오래 그대로인 것은 구워 두고 움직이는 것만 덧그린다. (화면은 매 프레임 그린 것과 같다)
//! - 사용: 매 프레임 clear() → add_mesh / add_mesh_instanced / add_pass → (draw_shadows) → draw(gl, renderer, camera) 나 draw_with_matrices(gl, renderer, 보기 행렬, 투영 행렬)
//!   draw 는 renderer.begin_frame_with_camera 와 end_frame 을 스스로 부른다. draw_with_matrices 는 begin_frame_with_matrices 를 부른다. (직교 원 행렬 카메라)
//!   화면 사각형(ToonRenderer::set_screen_rect)은 그 전에 건다. clear 는 담은 모형과 단계 콜백을 모두 비운다. (화면 공간 바깥선은 그대로 둔다)
//! - 원본은 담은 것을 객체 참조로 든다. 러스트 판은 모형과 인스턴스 목록, 단계 콜백을 빌려 담으므로 줄이 그 수명 'a 를 갖는다. (프레임마다 새로 짓거나 clear 뒤 다시 담는다)
//!   단계 콜백은 FnMut 이라 RefCell 에 담아 draw(&self) 에서 부른다.
//==============================================================================
use crate::core::glcontext::GlContext;
use std::cell::RefCell;
use std::collections::HashMap;


use crate::base::matrix4::Matrix4;
use crate::core::graphic::GlTexture;
use crate::graphics3d::camera3d::Camera3D;
use crate::graphics3d::shadowmap::{ShadowAtlas, ShadowCache, ShadowCasterRecord, ShadowMap, ShadowRenderTarget, ShadowUpdateMode};
use crate::graphics3d::toon::toonmaterial::ToonBlendMode;
use crate::graphics3d::toon::toonrenderer::{ToonDrawOptions, ToonInstance, ToonMesh, ToonRenderer};
use crate::graphics3d::toon::toonscreenoutline::{ToonLineDrawOptions, ToonScreenOutline, TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID};
use crate::graphics3d::toon::toonpostprocess::ToonPostProcess;

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
pub type ToonPassCallback<'a> = Box<dyn FnMut(&GlContext, &mut ToonRenderer) + 'a>;

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
/// fn draw_scene(gl: &vanilla::GlContext) -> Result<(), String> {
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
	/// 툰 후처리. (모서리 어둠을 몸 앞에 셈한다, 없으면 None)
	post_process: RefCell<Option<&'a mut ToonPostProcess>>,
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
/// 그림자 캐시에서 드리우개를 움직이지 않는 것으로 굽기까지 그대로여야 하는 프레임 수. (원본 SHADOW_STATIC_FRAME_COUNT, 자주 섰다 움직이는 것이 구운 판을 자꾸 다시 굽지 않게)
//==============================================================================
const SHADOW_STATIC_FRAME_COUNT: u32 = 30;

//==============================================================================
/// 드리우개 기록 쓰기. (원본 writeCasterRecord, 그림자 깊이에 닿는 값을 기록과 견주고 새 값으로 바꾼다, 다르면 true)
/// - 모형, 재질, 모형 행렬이나 인스턴스 행렬, 관절 행렬, 번호 버퍼의 범위, 화면 밖 거르기, 뒤집기. (틴트, 색, 바깥선 같은 것은 깊이에 닿지 않는다)
/// - 원본은 모형과 재질을 참조로 견준다. 러스트 판은 모형 번호와 재질 번호로 견준다. (지우고 다시 지은 것이 같은 주소에 와도 다르게 본다)
/// - 원본의 인스턴스 버퍼(addMeshInstanceBuffer 와 버퍼의 generation)는 러스트 ToonDrawQueue 에 아직 담는 길이 없어 견주지 않는다.
//==============================================================================
fn write_caster_record(record: &mut ShadowCasterRecord, item: &ToonDrawItem) -> bool {
	let options = &item.options;
	let matrix_count = item.instance_list.map_or(1, |instance_list| instance_list.len());
	let joint_count = options.joint_matrices.map_or(0, |joint_matrices| joint_matrices.len());
	let value_count = matrix_count * 16 + joint_count;
	let mesh_id = Some(item.mesh.get_mesh_id());
	let material_id = options.material.map(|material| material.get_id());
	let mirrored_value = match options.is_mirrored {
		None => {
			-1
		}
		Some(true) => {
			1
		}
		Some(false) => {
			0
		}
	};
	let header = [matrix_count as i64, options.index_start as i64, options.index_count as i64, options.is_culled as i64, mirrored_value, options.joint_matrices.map_or(-1, |joint_matrices| joint_matrices.len() as i64)];
	let mut is_changed = record.mesh_id != mesh_id || record.material_id != material_id || record.header != header;
	record.mesh_id = mesh_id;
	record.material_id = material_id;
	record.header = header;
	if record.values.len() != value_count {
		record.values = vec![0.0; value_count];
		is_changed = true;
	}
	let mut value_index = 0;
	let mut write = |value: f32| {
		// 값으로 견준다. (원본 !== 와 같이 NaN 은 늘 다르고, 0 과 -0 은 같다)
		if record.values[value_index] != value || value.is_nan() {
			record.values[value_index] = value;
			is_changed = true;
		}
		value_index += 1;
	};
	match item.instance_list {
		Some(instance_list) => {
			for instance in instance_list {
				for &element in instance.model_matrix.elements.iter() {
					write(element);
				}
			}
		}
		None => {
			for &element in item.model_matrix.elements.iter() {
				write(element);
			}
		}
	}
	if let Some(joint_matrices) = options.joint_matrices {
		for &joint_value in joint_matrices {
			write(joint_value);
		}
	}
	is_changed
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
		ToonDrawQueue { item_list: Vec::new(), pass_list: RefCell::new(Vec::new()), screen_outline: RefCell::new(None), post_process: RefCell::new(None), opaque_index_list: RefCell::new(None) }
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
	/// 툰 후처리 맡기기. (원본 setPostProcess, 모서리 어둠을 켰으면 draw 가 선의 판 뒤, 몸 앞에 셈해 렌더러에 걸고 end_frame 뒤에 푼다, None 이면 맡기지 않음)
	/// - 화면 공간 바깥선이 있으면 그 선의 판을, 없으면 후처리의 미리 그리기(get_ambient_occlusion_prepass)로 노멀과 깊이만 그린다.
	///
	/// # 인자
	/// - `post_process`: ToonPostProcess 나 None. (줄의 수명 동안 빌린다)
	//==============================================================================
	pub fn set_post_process(&mut self, post_process: Option<&'a mut ToonPostProcess>) {
		self.post_process = RefCell::new(post_process);
	}

	//==============================================================================
	/// 툰 후처리를 맡겼는지. (원본 getPostProcess 가 null 이 아닌지)
	///
	/// # 돌려줌
	/// 맡겼으면 true.
	//==============================================================================
	pub fn has_post_process(&self) -> bool {
		self.post_process.borrow().is_some()
	}

	//==============================================================================
	/// 모서리 어둠 셈하고 렌더러에 걸기. (원본 drawAmbientOcclusion, 켜지 않았으면 아무것도 하지 않고 false)
	//==============================================================================
	fn draw_ambient_occlusion(&self, gl: &GlContext, renderer: &mut ToonRenderer, view_matrix: &Matrix4, projection_matrix: &Matrix4) -> bool {
		let mut post_process_slot = self.post_process.borrow_mut();
		let post_process = match post_process_slot.as_mut() {
			Some(post_process) if post_process.is_ambient_occlusion_enabled() => {
				post_process
			}
			_ => {
				return false;
			}
		};
		let screen_outline_slot = self.screen_outline.borrow();
		let texture = match screen_outline_slot.as_ref() {
			Some(screen_outline) => {
				match screen_outline.get_line_target() {
					Some(line_target) => {
						post_process.draw_ambient_occlusion(gl, line_target, view_matrix, projection_matrix)
					}
					None => {
						None
					}
				}
			}
			None => {
				post_process.draw_ambient_occlusion_with_prepass(gl, view_matrix, projection_matrix)
			}
		};
		match texture {
			Some(texture) => {
				renderer.set_ambient_occlusion_texture(Some(texture), post_process.get_ambient_occlusion_direct_lighting_strength());
				true
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 단계 콜백 담기. (ToonDrawStage 의 자리에서 callback(gl, renderer) 를 부른다, 담은 차례로)
	///
	/// # 인자
	/// - `stage`: ToonDrawStage.
	/// - `callback`: (gl, renderer) 를 받는다. 부른 뒤 렌더러가 프레임의 그리기 상태를 다시 건다.
	//==============================================================================
	pub fn add_pass(&mut self, stage: ToonDrawStage, callback: impl FnMut(&GlContext, &mut ToonRenderer) + 'a) {
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
	/// - shadow_map.set_update_mode 의 갱신 방식을 따른다. (기본 OnChange 는 빛과 드리우개가 그대로면 그리지 않는다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `renderer`: ToonRenderer.
	/// - `shadow_map`: ShadowMap.
	//==============================================================================
	pub fn draw_shadows(&self, gl: &GlContext, renderer: &mut ToonRenderer, shadow_map: &ShadowMap) {
		let mut light_key = Vec::with_capacity(17);
		light_key.extend_from_slice(&shadow_map.get_light_view_projection_matrix().elements);
		light_key.push(shadow_map.get_resolution() as f32);
		self.draw_shadow_casters(gl, renderer, shadow_map.get_shadow_cache(), light_key, &|renderer: &mut ToonRenderer| {
			renderer.begin_shadow_pass(gl, shadow_map);
		}, &|renderer: &mut ToonRenderer, caster_index_list: &[usize]| {
			for &item_index in caster_index_list {
				let item = &self.item_list[item_index];
				ToonDrawQueue::draw_item(gl, renderer, item, item.options);
			}
		});
	}

	//==============================================================================
	/// 점광과 스포트라이트 그림자 깊이 그리기. (불투명하고 casts_shadow 가 true 인 것을 아틀라스의 칸마다, 앞의 프레임버퍼와 뷰포트는 되돌린다)
	/// - shadow_atlas.set_update_mode 의 갱신 방식을 따른다. (기본 OnChange 는 빛과 드리우개가 그대로면 그리지 않는다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `renderer`: ToonRenderer.
	/// - `shadow_atlas`: ShadowAtlas. (update_lights 를 부른 것)
	//==============================================================================
	pub fn draw_local_shadows(&self, gl: &GlContext, renderer: &mut ToonRenderer, shadow_atlas: &ShadowAtlas) {
		let tile_count = shadow_atlas.get_tile_count();
		let mut light_key = Vec::with_capacity(1 + tile_count * 19);
		light_key.push(shadow_atlas.get_resolution() as f32);
		for tile_index in 0..tile_count {
			let tile = shadow_atlas.get_tile(tile_index);
			light_key.extend_from_slice(&tile.view_projection_matrix.elements);
			light_key.push(tile.x as f32);
			light_key.push(tile.y as f32);
			light_key.push(tile.size as f32);
		}
		self.draw_shadow_casters(gl, renderer, shadow_atlas.get_shadow_cache(), light_key, &|renderer: &mut ToonRenderer| {
			renderer.begin_shadow_atlas_pass(gl, shadow_atlas);
		}, &|renderer: &mut ToonRenderer, caster_index_list: &[usize]| {
			for tile_index in 0..tile_count {
				renderer.set_shadow_atlas_tile(gl, shadow_atlas, tile_index);
				for &item_index in caster_index_list {
					let item = &self.item_list[item_index];
					ToonDrawQueue::draw_item(gl, renderer, item, item.options);
				}
			}
		});
	}

	//==============================================================================
	/// 그림자 드리우개 그리기. (원본 drawShadowCasters, 그림자 판의 갱신 방식대로 다시 그리거나 건너뛴다, 불투명하고 casts_shadow 가 true 인 것만)
	/// - EveryFrame: 판을 비우고 전부 그린다.
	/// - Interval: N 프레임마다(처음과 request_update 뒤에는 바로) 판을 비우고 전부 그린다. 사이의 프레임은 그리지 않는다.
	/// - OnChange: 드리우개를 담은 차례로 앞의 프레임과 견준다. SHADOW_STATIC_FRAME_COUNT 프레임 그대로인 것은 구운 판에 굽고,
	///   나머지(움직이는 것, 정점 조각이 있는 재질)는 매 프레임 구운 판을 판에 옮긴 뒤 그 위에 덧그린다.
	///   구운 것이 바뀌거나 빠지거나 새로 구울 것이 생기거나 request_update 를 부르면 다시 굽는다. 움직이는 것이 없고 판이 구운 판과 같으면 아무것도 그리지 않는다.
	///   빛 행렬이나 칸이 바뀐 프레임은 전부를 판에 바로 그리고, 빛이 멈춘 다음 프레임에 굽는다.
	///   깊이는 LEQUAL 로 가장 가까운 것이 남으므로 굽고 덧그린 판은 전부를 한 번에 그린 판과 같다.
	/// - 캐시는 RefCell 이라 그리기(begin_pass 가 begin_render 로 캐시를 빌린다) 전에 빌림을 놓는다.
	/// - 구운 판을 짓지 못하면(GL 오류) 그 프레임은 전부를 판에 바로 그린다. (원본은 짓기가 늘 된다)
	//==============================================================================
	fn draw_shadow_casters(&self, gl: &GlContext, renderer: &mut ToonRenderer, shadow_cache: &RefCell<ShadowCache>, light_key: Vec<f32>, begin_pass: &dyn Fn(&mut ToonRenderer), draw_casters: &dyn Fn(&mut ToonRenderer, &[usize])) {
		let caster_index_list: Vec<usize> = self.read_opaque_index_list().iter().copied().filter(|&item_index| self.item_list[item_index].options.casts_shadow).collect();
		let draw_pass = |renderer: &mut ToonRenderer, render_target: ShadowRenderTarget, index_list: &[usize]| {
			shadow_cache.borrow_mut().render_target = render_target;
			begin_pass(renderer);
			draw_casters(renderer, index_list);
			renderer.end_shadow_pass(gl);
		};
		let update_mode = shadow_cache.borrow().update_mode;
		match update_mode {
			ShadowUpdateMode::EveryFrame => {
				{
					let mut cache = shadow_cache.borrow_mut();
					cache.caster_record_list.clear();
					cache.light_key = None;
					cache.has_cache = false;
				}
				draw_pass(renderer, ShadowRenderTarget::Live, &caster_index_list);
				return;
			}
			ShadowUpdateMode::Interval => {
				let is_due = {
					let mut cache = shadow_cache.borrow_mut();
					let is_due = cache.is_update_requested || cache.frame_count % cache.update_interval as u64 == 0;
					cache.frame_count += 1;
					if is_due {
						cache.is_update_requested = false;
						cache.has_cache = false;
					}
					is_due
				};
				if is_due {
					draw_pass(renderer, ShadowRenderTarget::Live, &caster_index_list);
				}
				return;
			}
			ShadowUpdateMode::OnChange => {
			}
		}

		// 바뀔 때만. (드리우개를 담은 차례로 견준다, 정렬 차례는 재질 블록 같은 깊이에 닿지 않는 값으로도 바뀌어 그 차례로 견주면 바뀌지 않은 드리우개도 다르게 보인다)
		let mut static_index_list: Vec<usize> = Vec::new();
		let mut dynamic_index_list: Vec<usize> = Vec::new();
		let (is_light_changed, is_cache_stale) = {
			let mut cache_slot = shadow_cache.borrow_mut();
			let cache = &mut *cache_slot;
			let is_light_changed = cache.light_key.as_deref() != Some(light_key.as_slice());
			let mut is_cache_stale = cache.is_update_requested;
			cache.is_update_requested = false;
			cache.light_key = Some(light_key);
			let record_list = &mut cache.caster_record_list;
			if record_list.len() > caster_index_list.len() {
				is_cache_stale = is_cache_stale || record_list[caster_index_list.len()..].iter().any(|record| record.is_static);
				record_list.truncate(caster_index_list.len());
			}
			let mut is_static_caster_list = vec![false; self.item_list.len()];
			let mut caster_position = 0;
			for (item_index, item) in self.item_list.iter().enumerate() {
				if item.is_transparent || !item.options.casts_shadow {
					continue;
				}
				if caster_position >= record_list.len() {
					record_list.push(ShadowCasterRecord::default());
				}
				let record = &mut record_list[caster_position];
				caster_position += 1;
				let is_vertex_animated = item.options.material.is_some_and(|material| !material.get_vertex_chunk().is_empty());
				if write_caster_record(record, item) || is_vertex_animated {
					is_cache_stale = is_cache_stale || record.is_static;
					record.is_static = false;
					record.stable_frame_count = 0;
				}
				else {
					record.stable_frame_count += 1;
					if !record.is_static && record.stable_frame_count >= SHADOW_STATIC_FRAME_COUNT {
						record.is_static = true;
						is_cache_stale = true;
					}
				}
				is_static_caster_list[item_index] = record.is_static;
			}
			// 굽는 것과 덧그리는 것은 정렬 차례로 나눈다. (자동 인스턴싱으로 이어진 같은 것을 묶는다)
			for &item_index in caster_index_list.iter() {
				if is_static_caster_list[item_index] {
					static_index_list.push(item_index);
				}
				else {
					dynamic_index_list.push(item_index);
				}
			}
			if is_light_changed {
				cache.has_cache = false;
			}
			(is_light_changed, is_cache_stale)
		};
		if is_light_changed {
			// 빛이 바뀐 프레임은 전부를 판에 바로 그린다. (빛이 매 프레임 움직여도 굽고 옮기는 몫이 더 들지 않게, 구운 판은 빛이 멈춘 다음 프레임에 굽는다)
			draw_pass(renderer, ShadowRenderTarget::Live, &caster_index_list);
			return;
		}
		let has_cache = shadow_cache.borrow().has_cache;
		if is_cache_stale || (!has_cache && !static_index_list.is_empty()) {
			shadow_cache.borrow_mut().has_cache = !static_index_list.is_empty();
			if !static_index_list.is_empty() {
				draw_pass(renderer, ShadowRenderTarget::Cache, &static_index_list);
				if !shadow_cache.borrow().has_cache {
					// 구운 판을 짓지 못했다.
					draw_pass(renderer, ShadowRenderTarget::Live, &caster_index_list);
					return;
				}
			}
			shadow_cache.borrow_mut().is_depth_current = false;
		}
		let render_target = if shadow_cache.borrow().has_cache {
			ShadowRenderTarget::Copy
		}
		else {
			ShadowRenderTarget::Live
		};
		if !dynamic_index_list.is_empty() {
			draw_pass(renderer, render_target, &dynamic_index_list);
			return;
		}
		if !shadow_cache.borrow().is_depth_current {
			draw_pass(renderer, render_target, &dynamic_index_list);
			shadow_cache.borrow_mut().is_depth_current = true;
		}
	}

	//==============================================================================
	/// 담은 것 그리기. (화면 공간 바깥선이 있으면 선의 판부터, begin_frame_with_camera 와 end_frame 을 스스로 부른다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `renderer`: ToonRenderer.
	/// - `camera`: Camera3D.
	//==============================================================================
	pub fn draw(&self, gl: &GlContext, renderer: &mut ToonRenderer, camera: &Camera3D) {
		self.draw_line_ids(gl, renderer, &camera.get_view_matrix(), &camera.get_projection_matrix());
		let is_ambient_occlusion_applied = self.draw_ambient_occlusion(gl, renderer, &camera.get_view_matrix(), &camera.get_projection_matrix());
		renderer.begin_frame_with_camera(gl, camera);
		self.draw_stages(gl, renderer, &|position: [f64; 3]| camera.read_view_depth(position[0], position[1], position[2]));
		renderer.end_frame(gl);
		if is_ambient_occlusion_applied {
			renderer.set_ambient_occlusion_texture(None, 0.0);
		}
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
	pub fn draw_with_matrices(&self, gl: &GlContext, renderer: &mut ToonRenderer, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
		self.draw_line_ids(gl, renderer, view_matrix, projection_matrix);
		let is_ambient_occlusion_applied = self.draw_ambient_occlusion(gl, renderer, view_matrix, projection_matrix);
		renderer.begin_frame_with_matrices(gl, view_matrix, projection_matrix);
		let elements = view_matrix.read_elements();
		self.draw_stages(gl, renderer, &|position: [f64; 3]| -(elements[2] * position[0] + elements[6] * position[1] + elements[10] * position[2] + elements[14]));
		renderer.end_frame(gl);
		if is_ambient_occlusion_applied {
			renderer.set_ambient_occlusion_texture(None, 0.0);
		}
	}

	//==============================================================================
	/// 화면 공간 바깥선의 선의 판 그리기. (맡긴 것이 없으면 아무것도 하지 않음, 불투명한 것만, 화면 사각형이 있으면 그 사각형을 그리는 영역으로)
	//==============================================================================
	fn draw_line_ids(&self, gl: &GlContext, renderer: &ToonRenderer, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
		// 선의 판 주인. (원본 readLineSource, 화면 공간 바깥선, 없고 모서리 어둠을 켰으면 후처리의 미리 그리기, 둘 다 아니면 그리지 않는다)
		if let Some(screen_outline) = self.screen_outline.borrow_mut().as_mut() {
			self.draw_line_ids_into(gl, renderer, screen_outline, view_matrix, projection_matrix);
			return;
		}
		if let Some(post_process) = self.post_process.borrow_mut().as_mut() {
			if post_process.is_ambient_occlusion_enabled() {
				match post_process.get_ambient_occlusion_prepass(gl) {
					Ok(prepass) => {
						self.draw_line_ids_into(gl, renderer, prepass, view_matrix, projection_matrix);
					}
					Err(error) => {
						eprintln!("[ToonDrawQueue] 모서리 어둠의 미리 그리기를 짓지 못했다. {}", error);
					}
				}
			}
		}
	}

	//==============================================================================
	/// 선의 판 주인에 불투명한 것 그리기. (draw_line_ids 가 부른다)
	//==============================================================================
	fn draw_line_ids_into(&self, gl: &GlContext, renderer: &ToonRenderer, screen_outline: &mut ToonScreenOutline, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
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
	fn run_passes(&self, gl: &GlContext, renderer: &mut ToonRenderer, stage: ToonDrawStage) {
		let mut is_run = false;
		for (pass_stage, callback) in self.pass_list.borrow_mut().iter_mut() {
			if *pass_stage != stage {
				continue;
			}
			renderer.select_first_unit(gl);
			callback(gl, renderer);
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
	fn draw_stages(&self, gl: &GlContext, renderer: &mut ToonRenderer, read_depth: &dyn Fn([f64; 3]) -> f64) {
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
	fn draw_item(gl: &GlContext, renderer: &mut ToonRenderer, item: &ToonDrawItem, options: ToonDrawOptions) {
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

	//==============================================================================
	/// 툰 그리기의 텍스처 유닛 고르기. (원본 tests/graphics3d/toontextureunitchrome.test.mjs 와 같은 장면, 진짜 GL, 맥 CGL)
	/// - 모형 넷과 바탕색 텍스처 둘을 엇갈려 쓰는 상자 여덟을 해 그림자와 함께 그리기 줄로 그린다. 정렬한 차례에서 바탕색 텍스처가 그리기마다 바뀐다. (유닛 4 만 바뀐다)
	/// - 그리기마다 고른 유닛을 0 으로 되돌리지 않으므로 같은 유닛만 바꾸는 그리기는 active_texture 없이 bind_texture 하나다.
	/// - 단계 콜백과 몸 그리기의 on_before_draw 에는 유닛 0 을 고른 채 넘기고, 프레임이 끝나면 앞서 고른 유닛(3)과 유닛 0 의 텍스처를 돌려놓는지 GL 에 바로 물어 본다.
	//==============================================================================
	#[cfg(target_os = "macos")]
	#[test]
	fn draw_does_not_reset_texture_unit_each_draw() {
		use crate::graphics3d::toon::toonrenderer::{ToonProgramKind, ToonTextureOptions};
		use crate::graphics3d::shadowmap::{ShadowMap, ShadowUpdateMode};
		use crate::{create_box_geometry, create_octahedron_geometry, create_sphere_geometry, create_cylinder_geometry, Camera3D, MeshBuilder, ToonPartOptions, Vector3};
		use glow::HasContext;
		let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
			eprintln!("toondrawqueue: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
			return;
		};
		let _ = crate::core::glstatecache::attach(&glow_context);
		let gl: &GlContext = GlContext::from_context(&glow_context);
		let raw: &glow::Context = gl.get_glow_context_unchecked();
		let read_parameter = |name: u32| unsafe { raw.get_parameter_i32(name) };
		let (width, height) = (160i32, 120i32);
		let framebuffer = unsafe {
			let color_texture = crate::core::glstatecache::create_texture(gl).unwrap();
			crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(color_texture));
			raw.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, width, height, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
			let depth = raw.create_renderbuffer().unwrap();
			raw.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
			raw.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, width, height);
			let framebuffer = crate::core::glstatecache::create_framebuffer(gl).unwrap();
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(framebuffer));
			raw.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color_texture), 0);
			raw.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth));
			assert_eq!(raw.check_framebuffer_status(glow::FRAMEBUFFER), glow::FRAMEBUFFER_COMPLETE);
			framebuffer
		};
		let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
		let create_mesh = |geometry: &crate::ToonGeometry| {
			let mut builder = MeshBuilder::new();
			builder.add_part(geometry, &Matrix4::create_identity(), "#ffffff", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
			renderer.create_mesh(gl, &builder.read_arrays()).expect("모형")
		};
		let ground_mesh = create_mesh(&create_box_geometry(8.0, 0.2, 8.0));
		let mesh_list = [create_mesh(&create_box_geometry(0.6, 0.8, 0.6)), create_mesh(&create_sphere_geometry(0.35, 12, 8, std::f64::consts::PI)), create_mesh(&create_octahedron_geometry(0.35)), create_mesh(&create_cylinder_geometry(0.3, 0.3, 0.8, 12))];
		let create_pattern = |first: [u8; 4], second: [u8; 4]| {
			let pixels: Vec<u8> = (0..64).flat_map(|index| if index % 8 < 4 && index < 32 { second } else { first }).collect();
			renderer.create_texture(gl, 8, 8, &pixels, ToonTextureOptions { is_mipmapped: false, ..ToonTextureOptions::default() }).expect("텍스처")
		};
		let texture_list = [create_pattern([224, 160, 112, 255], [96, 48, 32, 255]), create_pattern([112, 160, 224, 255], [32, 48, 96, 255])];
		let mut camera = Camera3D::new();
		camera.set_perspective(0.9, 0.1, 50.0);
		camera.set_aspect_ratio(width as f64 / height as f64);
		camera.look_at(Vector3::create(0.0, 4.0, 5.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		let mut shadow_map = ShadowMap::new(gl, 512).expect("그림자 판");
		shadow_map.set_update_mode(ShadowUpdateMode::OnChange, 1);
		shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), Vector3::create(-0.5, -1.0, -0.4), 4.0, 0.1, 20.0);
		renderer.set_shadow(Some(&shadow_map), 0.7);
		let outside_texture = unsafe { crate::core::glstatecache::create_texture(gl) }.unwrap();
		let callback_unit_list: RefCell<Vec<i32>> = RefCell::new(Vec::new());
		let mut frame_list: Vec<(u32, u32, i32, bool, u32)> = Vec::new();
		for _frame in 0..6 {
			// 게임이 유닛 0 에 제 텍스처를 두고 유닛 3 을 고른 채 3D 를 그린다.
			unsafe {
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
				crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(outside_texture));
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE3);
			}
			let before_count = crate::core::glstatecache::read_texture_call_count();
			let on_before_draw = |_program: &crate::core::shaderprogram::ShaderProgram, _gl: &GlContext, kind: ToonProgramKind| {
				if kind == ToonProgramKind::Solid {
					callback_unit_list.borrow_mut().push(read_parameter(glow::ACTIVE_TEXTURE) - glow::TEXTURE0 as i32);
				}
			};
			let mut queue = ToonDrawQueue::new();
			queue.add_mesh(&ground_mesh, &Matrix4::create_translation(0.0, -0.1, 0.0), ToonDrawOptions::default());
			for index in 0..8usize {
				let mut options = ToonDrawOptions { base_color_texture: Some(texture_list[index % 2]), ..ToonDrawOptions::default() };
				if index == 5 {
					// 몸 그리기의 콜백만 본다. (그림자 깊이 패스는 텍스처를 묶지 않아 고른 유닛이 그대로다)
					options.on_before_draw = Some(&on_before_draw);
				}
				queue.add_mesh(&mesh_list[index / 2], &Matrix4::create_translation(-2.1 + index as f64 * 0.6, 0.4, 0.0), options);
			}
			let callback_ref = &callback_unit_list;
			queue.add_pass(ToonDrawStage::AfterOpaque, move |_gl: &GlContext, _renderer: &mut ToonRenderer| {
				callback_ref.borrow_mut().push(read_parameter(glow::ACTIVE_TEXTURE) - glow::TEXTURE0 as i32);
			});
			queue.draw_shadows(gl, &mut renderer, &shadow_map);
			unsafe {
				crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(framebuffer));
				crate::core::glstatecache::viewport(gl, 0, 0, width, height);
				raw.clear_color(0.1, 0.1, 0.1, 1.0);
				raw.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
			}
			queue.draw(gl, &mut renderer, &camera);
			let after_count = crate::core::glstatecache::read_texture_call_count();
			let active_unit = read_parameter(glow::ACTIVE_TEXTURE) - glow::TEXTURE0 as i32;
			unsafe {
				crate::core::glstatecache::active_texture(gl, glow::TEXTURE0);
			}
			let first_unit_texture = read_parameter(glow::TEXTURE_BINDING_2D) as u32;
			let error = unsafe { raw.get_error() };
			let is_restored = first_unit_texture == outside_texture.0.get();
			frame_list.push((after_count.0 - before_count.0, after_count.1 - before_count.1, active_unit, is_restored, error));
		}
		eprintln!("toondrawqueue: 프레임마다 (active_texture, bind_texture, 고른 유닛, 유닛 0 돌아옴, 오류) {:?}", frame_list);
		for (frame, result) in frame_list.iter().enumerate() {
			assert_eq!(result.4, 0, "{} 프레임 GL 오류", frame);
			assert_eq!(result.2, 3, "{} 프레임 뒤 고른 유닛이 3 으로 돌아옴", frame);
			assert!(result.3, "{} 프레임 뒤 유닛 0 의 텍스처가 돌아옴", frame);
		}
		// 같은 차례로 그리는 프레임(2 부터)은 바탕색 유닛만 바뀐다. 그리기마다 유닛을 0 으로 되돌리지 않는다. (고치기 전 스물여섯)
		for (frame, result) in frame_list.iter().enumerate().skip(2) {
			assert!(result.0 <= 12, "{} 프레임 active_texture 가 열둘 이하 ({})", frame, result.0);
			assert!(result.1 >= 8, "{} 프레임 바탕색 텍스처는 그리기마다 묶음 ({})", frame, result.1);
		}
		// 단계 콜백과 on_before_draw 는 유닛 0 을 고른 채 부른다.
		assert_eq!(callback_unit_list.borrow().len(), 12, "콜백을 프레임마다 둘 부름");
		for unit in callback_unit_list.borrow().iter() {
			assert_eq!(*unit, 0, "콜백이 유닛 0 을 고른 채 불림");
		}
	}
}

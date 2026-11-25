//==============================================================================
// 툰 그리기 줄. (vanilla.js experimental/graphics/toon/toondrawqueue.js 의 ToonDrawQueue)
// - 한 프레임에 그릴 모형을 담아 두었다가 ToonRenderer 로 차례를 맞춰 그린다.
// - 차례: 단계 BeforeOpaque → 불투명한 몸 → AfterOpaque → 불투명한 것의 바깥선 → AfterOutline → 비치는 것(카메라에서 먼 것부터, 깊이는 쓰지 않음) → AfterTransparent.
//   몸을 다 그린 뒤 바깥선을 몰아 그려 프로그램을 덜 바꾸고, 비치는 것은 뒤의 것이 비쳐 보이게 맨 끝에 먼 것부터 섞는다.
// - 비치는 것: 꼴의 alpha 나 tint_color 의 알파가 1 보다 작거나 blend_mode 가 Alpha 가 아닌 것. 바깥선도 함께(has_outline) 그린다.
// - 묶음: 꼴의 group 이 같은 비치는 것은 한 덩어리로 가운데 깊이(담긴 것의 평균)로 정렬하고 담은 차례로 그린다.
//   is_depth_prepassed 가 묶음 안에 하나라도 있으면 그 덩어리는 깊이만 먼저 그린 뒤 같은 깊이(LEQUAL)인 화소에만 색을 칠한다.
//   여러 부품이 겹친 홀로그램이 두 겹으로 밝지 않고 한 덩어리로 비친다. 덩어리끼리는 먼 것부터라 서로 비친다.
// - 단계 콜백: add_pass(stage, callback) 의 callback(gl, renderer) 를 그 자리에서 부른다. 부른 뒤 렌더러가 프레임의 그리기 상태를 다시 건다.
// - 인스턴스 묶음(add_mesh_instanced)은 불투명이면 몸과 바깥선을 한 번에 그리고, 비치면 인스턴스 자리의 가운데로 한 덩어리로 정렬한다.
// - 그림자: draw_shadows(gl, renderer, shadow_map) 가 casts_shadow 가 true 인 불투명한 것만 그림자 깊이로 그린다. draw 보다 먼저 부른다.
// - 사용: 매 프레임 clear() → add_mesh / add_mesh_instanced / add_pass → (draw_shadows) → draw(gl, renderer, camera) 나 draw_with_matrices(gl, renderer, 보기 행렬, 투영 행렬)
//   draw 는 renderer.begin_frame_with_camera 와 end_frame 을 스스로 부른다. draw_with_matrices 는 begin_frame_with_matrices 를 부른다. (직교 원 행렬 카메라)
//   화면 사각형(ToonRenderer::set_screen_rect)은 그 전에 건다. clear 는 담은 모형과 단계 콜백을 모두 비운다.
// - 원본은 담은 것을 객체 참조로 든다. 러스트 판은 모형과 인스턴스 목록, 단계 콜백을 빌려 담으므로 줄이 그 수명 'a 를 갖는다. (프레임마다 새로 짓거나 clear 뒤 다시 담는다)
//   단계 콜백은 FnMut 이라 RefCell 에 담아 draw(&self) 에서 부른다.
//==============================================================================
use std::cell::RefCell;
use std::collections::HashMap;

use glow::HasContext;

use crate::base::matrix4::Matrix4;
use crate::graphics3d::camera3d::Camera3D;
use crate::graphics3d::shadowmap::ShadowMap;
use crate::graphics3d::toon::toonmaterial::ToonBlendMode;
use crate::graphics3d::toon::toonrenderer::{ToonDrawOptions, ToonInstance, ToonMesh, ToonRenderer};

//==============================================================================
// 그리기 줄의 단계. (add_pass 의 콜백을 부를 자리)
// - BeforeOpaque: 불투명한 몸 앞. (평면 거울에 비친 모습, 하늘)
// - AfterOpaque: 불투명한 몸 뒤, 바깥선 껍데기 앞.
// - AfterOutline: 바깥선 껍데기 뒤, 비치는 것 앞. (화면 공간 바깥선 ToonScreenOutline::draw_outline 을 섞을 자리)
// - AfterTransparent: 비치는 것 뒤. (효과)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToonDrawStage {
	BeforeOpaque,
	AfterOpaque,
	AfterOutline,
	AfterTransparent,
}

//==============================================================================
// 단계 콜백. (gl, 렌더러)
//==============================================================================
pub type ToonPassCallback<'a> = Box<dyn FnMut(&glow::Context, &mut ToonRenderer) + 'a>;

//==============================================================================
// 담은 것 하나.
//==============================================================================
struct ToonDrawItem<'a> {
	mesh: &'a ToonMesh,
	model_matrix: Matrix4,
	instance_list: Option<&'a [ToonInstance]>,
	options: ToonDrawOptions<'a>,
	is_transparent: bool,
	depth_position: [f32; 3],
}

//==============================================================================
// 비치는 것의 한 덩어리. (묶음이거나 하나, 깊이 합과 담은 차례)
//==============================================================================
struct ToonTransparentEntry {
	depth_sum: f32,
	item_index_list: Vec<usize>,
	is_depth_prepassed: bool,
}

#[derive(Default)]
pub struct ToonDrawQueue<'a> {
	item_list: Vec<ToonDrawItem<'a>>,
	pass_list: RefCell<Vec<(ToonDrawStage, ToonPassCallback<'a>)>>,
}

//==============================================================================
// 그릴 꼴이 비치는지. (alpha 나 틴트의 알파가 1 보다 작거나 섞는 방식이 곧은 알파가 아님)
//==============================================================================
fn is_transparent_options(options: &ToonDrawOptions) -> bool {
	options.alpha < 1.0 || options.tint_color[3] < 1.0 || options.blend_mode != ToonBlendMode::Alpha
}

impl<'a> ToonDrawQueue<'a> {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> ToonDrawQueue<'a> {
		ToonDrawQueue { item_list: Vec::new(), pass_list: RefCell::new(Vec::new()) }
	}

	//==============================================================================
	// 비우기. (단계 콜백도 비운다)
	//==============================================================================
	pub fn clear(&mut self) {
		self.item_list.clear();
		self.pass_list.borrow_mut().clear();
	}

	//==============================================================================
	// 담은 수 반환.
	//==============================================================================
	pub fn get_item_count(&self) -> usize {
		self.item_list.len()
	}

	//==============================================================================
	// 단계 콜백 담기. (ToonDrawStage 의 자리에서 callback(gl, renderer) 를 부른다, 담은 차례로)
	//==============================================================================
	pub fn add_pass(&mut self, stage: ToonDrawStage, callback: impl FnMut(&glow::Context, &mut ToonRenderer) + 'a) {
		self.pass_list.borrow_mut().push((stage, Box::new(callback)));
	}

	//==============================================================================
	// 모형 하나 담기.
	//==============================================================================
	pub fn add_mesh(&mut self, mesh: &'a ToonMesh, model_matrix: &Matrix4, options: ToonDrawOptions<'a>) {
		let (center, _) = mesh.get_bounding_sphere();
		let depth_point = model_matrix.transform_point(center[0], center[1], center[2]);
		self.item_list.push(ToonDrawItem { mesh, model_matrix: *model_matrix, instance_list: None, options, is_transparent: is_transparent_options(&options), depth_position: [depth_point.x, depth_point.y, depth_point.z] });
	}

	//==============================================================================
	// 같은 모형 여럿 담기. (GPU 인스턴싱, 비어 있으면 담지 않는다)
	//==============================================================================
	pub fn add_mesh_instanced(&mut self, mesh: &'a ToonMesh, instance_list: &'a [ToonInstance], options: ToonDrawOptions<'a>) {
		if instance_list.is_empty() {
			return;
		}
		let mut depth_position = [0.0f32; 3];
		for instance in instance_list {
			for (axis, value) in depth_position.iter_mut().enumerate() {
				*value += instance.model_matrix.elements[12 + axis];
			}
		}
		for value in depth_position.iter_mut() {
			*value /= instance_list.len() as f32;
		}
		self.item_list.push(ToonDrawItem { mesh, model_matrix: Matrix4::create_identity(), instance_list: Some(instance_list), options, is_transparent: is_transparent_options(&options), depth_position });
	}

	//==============================================================================
	// 그림자 깊이 그리기. (불투명하고 casts_shadow 가 true 인 것만, 앞의 프레임버퍼와 뷰포트는 되돌린다)
	//==============================================================================
	pub fn draw_shadows(&self, gl: &glow::Context, renderer: &mut ToonRenderer, shadow_map: &ShadowMap) {
		renderer.begin_shadow_pass(gl, shadow_map);
		for item in self.item_list.iter() {
			if item.is_transparent || !item.options.casts_shadow {
				continue;
			}
			ToonDrawQueue::draw_item(gl, renderer, item, item.options);
		}
		renderer.end_shadow_pass(gl);
	}

	//==============================================================================
	// 담은 것 그리기. (begin_frame_with_camera 와 end_frame 을 스스로 부른다)
	//==============================================================================
	pub fn draw(&self, gl: &glow::Context, renderer: &mut ToonRenderer, camera: &Camera3D) {
		renderer.begin_frame_with_camera(gl, camera);
		self.draw_stages(gl, renderer, &|position: [f32; 3]| camera.read_view_depth(position[0], position[1], position[2]));
		renderer.end_frame(gl);
	}

	//==============================================================================
	// 보기와 투영 행렬로 담은 것 그리기. (Camera3D 없이, begin_frame_with_matrices 와 end_frame 을 스스로 부른다)
	//==============================================================================
	pub fn draw_with_matrices(&self, gl: &glow::Context, renderer: &mut ToonRenderer, view_matrix: &Matrix4, projection_matrix: &Matrix4) {
		renderer.begin_frame_with_matrices(gl, view_matrix, projection_matrix);
		let elements = view_matrix.elements;
		self.draw_stages(gl, renderer, &|position: [f32; 3]| -(elements[2] * position[0] + elements[6] * position[1] + elements[10] * position[2] + elements[14]));
		renderer.end_frame(gl);
	}

	//==============================================================================
	// 한 단계의 콜백 부르기. (부른 뒤 프레임의 그리기 상태를 다시 건다)
	//==============================================================================
	fn run_passes(&self, gl: &glow::Context, renderer: &mut ToonRenderer, stage: ToonDrawStage) {
		let mut is_run = false;
		for (pass_stage, callback) in self.pass_list.borrow_mut().iter_mut() {
			if *pass_stage != stage {
				continue;
			}
			callback(gl, renderer);
			is_run = true;
		}
		if is_run {
			renderer.apply_frame_state(gl);
		}
	}

	//==============================================================================
	// 단계마다 그리기. (begin_frame 과 end_frame 사이, read_depth(자리) 는 카메라에서의 보기 깊이)
	//==============================================================================
	fn draw_stages(&self, gl: &glow::Context, renderer: &mut ToonRenderer, read_depth: &dyn Fn([f32; 3]) -> f32) {
		self.run_passes(gl, renderer, ToonDrawStage::BeforeOpaque);

		// 1. 불투명한 몸. (인스턴스 묶음은 바깥선까지 한 번에, 화면 밖이면 draw_mesh 가 걸러 센다)
		let mut outline_index_list: Vec<usize> = Vec::new();
		for (item_index, item) in self.item_list.iter().enumerate() {
			if item.is_transparent {
				continue;
			}
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

		// 2. 불투명한 것의 바깥선.
		for item_index in outline_index_list {
			let item = &self.item_list[item_index];
			renderer.draw_mesh_outline(gl, item.mesh, &item.model_matrix, item.options);
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
			let read_entry_depth = |entry: &ToonTransparentEntry| entry.depth_sum / entry.item_index_list.len() as f32;
			entry_list.sort_by(|first, second| read_entry_depth(second).total_cmp(&read_entry_depth(first)));
			unsafe {
				gl.depth_mask(false);
			}
			for entry in entry_list.iter() {
				if entry.is_depth_prepassed {
					unsafe {
						gl.color_mask(false, false, false, false);
						gl.depth_mask(true);
					}
					for item_index in entry.item_index_list.iter() {
						let item = &self.item_list[*item_index];
						ToonDrawQueue::draw_item(gl, renderer, item, item.options);
					}
					unsafe {
						gl.color_mask(true, true, true, true);
						gl.depth_mask(false);
					}
				}
				for item_index in entry.item_index_list.iter() {
					let item = &self.item_list[*item_index];
					ToonDrawQueue::draw_item(gl, renderer, item, item.options);
				}
			}
			unsafe {
				gl.depth_mask(true);
			}
		}
		self.run_passes(gl, renderer, ToonDrawStage::AfterTransparent);
	}

	//==============================================================================
	// 하나 그리기. (모형 하나는 draw_mesh, 인스턴스 묶음은 draw_mesh_instanced)
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

	#[test]
	fn transparent_options_follow_alpha_and_tint() {
		assert!(!is_transparent_options(&ToonDrawOptions::default()));
		assert!(is_transparent_options(&ToonDrawOptions { alpha: 0.5, ..ToonDrawOptions::default() }));
		assert!(is_transparent_options(&ToonDrawOptions { tint_color: [1.0, 1.0, 1.0, 0.9], ..ToonDrawOptions::default() }));
		assert!(is_transparent_options(&ToonDrawOptions { blend_mode: ToonBlendMode::Additive, ..ToonDrawOptions::default() }));
	}
}

//==============================================================================
// 툰 그리기 줄. (vanilla.js experimental/graphics/toon/toondrawqueue.js 의 ToonDrawQueue)
// - 한 프레임에 그릴 모형을 담아 두었다가 ToonRenderer 로 차례를 맞춰 그린다.
// - 차례: 불투명한 몸 → 불투명한 것의 바깥선 → 비치는 것(카메라에서 먼 것부터, 깊이는 쓰지 않음).
//   몸을 다 그린 뒤 바깥선을 몰아 그려 프로그램을 덜 바꾸고, 비치는 것은 뒤의 것이 비쳐 보이게 맨 끝에 먼 것부터 섞는다.
// - 비치는 것: 꼴의 alpha 나 tint_color 의 알파가 1 보다 작은 것. 바깥선도 함께(has_outline) 그린다.
// - 인스턴스 묶음(add_mesh_instanced)은 불투명이면 몸과 바깥선을 한 번에 그리고, 비치면 인스턴스 자리의 가운데로 한 덩어리로 정렬한다.
// - 그림자: draw_shadows(gl, renderer, shadow_map) 가 casts_shadow 가 true 인 불투명한 것만 그림자 깊이로 그린다. draw 보다 먼저 부른다.
// - 사용: 매 프레임 clear() → add_mesh / add_mesh_instanced → (draw_shadows) → draw(gl, renderer, camera)
//   draw 는 renderer.begin_frame_with_camera 와 end_frame 을 스스로 부른다. 화면 사각형(ToonRenderer::set_screen_rect)은 그 전에 건다.
// - 원본은 담은 것을 객체 참조로 든다. 러스트 판은 모형과 인스턴스 목록을 빌려 담으므로 줄이 그 수명 'a 를 갖는다. (프레임마다 새로 짓거나 clear 뒤 다시 담는다)
//==============================================================================
use glow::HasContext;

use crate::base::matrix4::Matrix4;
use crate::graphics3d::camera3d::Camera3D;
use crate::graphics3d::shadowmap::ShadowMap;
use crate::graphics3d::toon::toonrenderer::{ToonDrawOptions, ToonInstance, ToonMesh, ToonRenderer};

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

#[derive(Default)]
pub struct ToonDrawQueue<'a> {
	item_list: Vec<ToonDrawItem<'a>>,
}

//==============================================================================
// 그릴 꼴이 비치는지. (alpha 나 틴트의 알파가 1 보다 작음)
//==============================================================================
fn is_transparent_options(options: &ToonDrawOptions) -> bool {
	options.alpha < 1.0 || options.tint_color[3] < 1.0
}

impl<'a> ToonDrawQueue<'a> {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> ToonDrawQueue<'a> {
		ToonDrawQueue { item_list: Vec::new() }
	}

	//==============================================================================
	// 비우기.
	//==============================================================================
	pub fn clear(&mut self) {
		self.item_list.clear();
	}

	//==============================================================================
	// 담은 수 반환.
	//==============================================================================
	pub fn get_item_count(&self) -> usize {
		self.item_list.len()
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
	// 담은 것 그리기. (불투명한 몸 → 바깥선 → 비치는 것을 먼 것부터, begin_frame_with_camera 와 end_frame 을 스스로 부른다)
	//==============================================================================
	pub fn draw(&self, gl: &glow::Context, renderer: &mut ToonRenderer, camera: &Camera3D) {
		renderer.begin_frame_with_camera(gl, camera);

		// 1. 불투명한 몸. (인스턴스 묶음은 바깥선까지 한 번에, 화면 밖이면 draw_mesh 가 걸러 센다)
		let mut outline_item_list: Vec<&ToonDrawItem> = Vec::new();
		for item in self.item_list.iter() {
			if item.is_transparent {
				continue;
			}
			if item.instance_list.is_some() || (item.options.is_culled && !renderer.is_visible(item.mesh, &item.model_matrix)) {
				ToonDrawQueue::draw_item(gl, renderer, item, item.options);
				continue;
			}
			ToonDrawQueue::draw_item(gl, renderer, item, ToonDrawOptions { has_outline: false, ..item.options });
			if item.options.has_outline {
				outline_item_list.push(item);
			}
		}

		// 2. 불투명한 것의 바깥선.
		for item in outline_item_list {
			renderer.draw_mesh_outline(gl, item.mesh, &item.model_matrix, item.options);
		}

		// 3. 비치는 것. (먼 것부터, 깊이는 쓰지 않음)
		let mut transparent_item_list: Vec<(f32, &ToonDrawItem)> = self.item_list.iter().filter(|item| item.is_transparent).map(|item| (camera.read_view_depth(item.depth_position[0], item.depth_position[1], item.depth_position[2]), item)).collect();
		if !transparent_item_list.is_empty() {
			transparent_item_list.sort_by(|first, second| second.0.total_cmp(&first.0));
			unsafe {
				gl.depth_mask(false);
			}
			for (_, item) in transparent_item_list {
				ToonDrawQueue::draw_item(gl, renderer, item, item.options);
			}
			unsafe {
				gl.depth_mask(true);
			}
		}
		renderer.end_frame(gl);
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
	}
}

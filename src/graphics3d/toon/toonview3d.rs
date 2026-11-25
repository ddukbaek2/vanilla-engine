//==============================================================================
// 툰 3D 를 노드 사각형 안에 그리는 컴포넌트. (vanilla.js experimental/graphics/toon/toonview3d.js 의 ToonView3D)
// - draw 에서: 노드의 내용 영역(0, 0, 내용 크기)을 지금 변환으로 드로잉 버퍼 픽셀의 사각형으로 옮기고(돌았으면 감싸는 사각형),
//   Graphic::run_external_pass 로 앞의 2D 묶음을 내보내고 상태 기억을 잊고 깊이를 비운 뒤, 렌더러의 화면 사각형(set_screen_rect)을 걸고 그리기 함수를 부른다.
//   끝나면 화면 사각형을 풀고 restore_render_state 로 2D 상태를 되돌린다. (그리기 함수가 패닉해도 둘 다 되돌린다)
// - 렌더러는 처음 그릴 때 짓는다. 셰이더를 짓지 못하는 기기면 다시 짓지 않고 대체 그리기 함수(set_fallback_draw_callback)로 2D 를 그린다.
//   여러 노드가 렌더러 하나를 함께 쓰려면 set_renderer 로 넣는다.
// - 카메라(get_camera_mut)의 가로 세로 비는 그릴 때마다 사각형의 비로 맞춘다. 3D 점 위의 2D 표시는 같은 카메라의 project_to_view 로 셈한다.
// - 깊이 비우기: 부모 Mask 의 자르기 안이면 Adreno 660 함정(Graphic::clear_depth_buffer 머리말)이 있다. 자르기를 거는 쪽이
//   begin_clip_rect 앞에서 clear_depth_buffer 를 부르고 이 컴포넌트는 set_depth_cleared(false) 로 둔다.
// - 그리기 함수: (렌더러, 카메라, 화면 사각형, GL, 드로잉 버퍼 높이). 화면 사각형은 드로잉 버퍼 픽셀이다.
//   보통 ToonDrawQueue 에 담아 queue.draw(gl, renderer, camera) 하거나 begin_frame_with_camera 와 end_frame 사이에 그린다.
//   ToonPostProcess 를 쓰면 renderer.clear_screen_rect() 를 부르고 실수 렌더 타겟에 그린 뒤 end 의 뷰포트를 사각형으로 준다.
// - 원본과 다른 것:
//   - 공유 렌더러는 Rc<RefCell<ToonRenderer>> 다. 원본은 객체 참조를 나눠 든다. 그리는 동안 렌더러를 빌려 그리기 함수에 넘긴다.
//   - 그리기 함수는 다섯째 인자로 드로잉 버퍼 높이를 받는다. 원본은 webGL2RenderingContext.drawingBufferHeight 로 읽지만 glow 컨텍스트에는 그 값이 없다.
//==============================================================================
use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;
use crate::graphics3d::camera3d::Camera3D;
use crate::graphics3d::toon::toonrenderer::ToonRenderer;

// 그리기 함수. (렌더러, 카메라, 화면 사각형, GL, 드로잉 버퍼 높이)
pub type ToonViewDrawCallback = Box<dyn FnMut(&mut ToonRenderer, &Camera3D, Rect, &glow::Context, i32)>;

// 대체 그리기 함수. (렌더러를 짓지 못한 기기에서 2D 로, 노드 변환이 걸린 Graphic 과 내용 크기)
pub type ToonViewFallbackDrawCallback = Box<dyn FnMut(&mut Graphic, Vector2)>;

//==============================================================================
// 그리는 동안 렌더러를 빌려 들고, 끝나면(패닉해도) 화면 사각형을 푸는 가드.
//==============================================================================
struct ScreenRectGuard<'a> {
	renderer: RefMut<'a, ToonRenderer>,
}

impl Drop for ScreenRectGuard<'_> {
	//==============================================================================
	// 화면 사각형 풀기.
	//==============================================================================
	fn drop(&mut self) {
		self.renderer.clear_screen_rect();
	}
}

pub struct ToonView3D {
	component_base: ComponentBase,
	renderer: Option<Rc<RefCell<ToonRenderer>>>,
	is_renderer_failed: bool,
	camera: Camera3D,
	draw_callback: Option<ToonViewDrawCallback>,
	fallback_draw_callback: Option<ToonViewFallbackDrawCallback>,
	is_depth_cleared: bool,
	screen_rect: Rect,
}

#[crate::reflect]
impl ToonView3D {
	//==============================================================================
	// 생성. (깊이를 비움)
	//==============================================================================
	pub fn new() -> ToonView3D {
		ToonView3D {
			component_base: ComponentBase::new(),
			renderer: None,
			is_renderer_failed: false,
			camera: Camera3D::new(),
			draw_callback: None,
			fallback_draw_callback: None,
			is_depth_cleared: true,
			screen_rect: Rect::zero(),
		}
	}

	//==============================================================================
	// 그리기 함수 설정. (None 이면 그리지 않음)
	//==============================================================================
	pub fn set_draw_callback(&mut self, draw_callback: Option<ToonViewDrawCallback>) {
		self.draw_callback = draw_callback;
	}

	//==============================================================================
	// 대체 그리기 함수 설정. (렌더러를 짓지 못한 기기에서 2D 로)
	//==============================================================================
	pub fn set_fallback_draw_callback(&mut self, fallback_draw_callback: Option<ToonViewFallbackDrawCallback>) {
		self.fallback_draw_callback = fallback_draw_callback;
	}

	//==============================================================================
	// 그리기 전에 깊이를 비울지 설정. (기본 true)
	//==============================================================================
	pub fn set_depth_cleared(&mut self, is_depth_cleared: bool) {
		self.is_depth_cleared = is_depth_cleared;
	}

	//==============================================================================
	// 그리기 전에 깊이를 비우는지 반환.
	//==============================================================================
	pub fn is_depth_cleared(&self) -> bool {
		self.is_depth_cleared
	}

	//==============================================================================
	// 렌더러 넣기. (여러 노드가 하나를 함께 쓸 때, None 이면 처음 그릴 때 짓는다)
	//==============================================================================
	pub fn set_renderer(&mut self, renderer: Option<Rc<RefCell<ToonRenderer>>>) {
		self.renderer = renderer;
		self.is_renderer_failed = false;
	}

	//==============================================================================
	// 렌더러 반환. (아직 짓지 않았거나 짓지 못했으면 None)
	//==============================================================================
	pub fn get_renderer(&self) -> Option<&Rc<RefCell<ToonRenderer>>> {
		self.renderer.as_ref()
	}

	//==============================================================================
	// 렌더러를 짓지 못했는지.
	//==============================================================================
	pub fn is_renderer_failed(&self) -> bool {
		self.is_renderer_failed
	}

	//==============================================================================
	// 카메라 반환.
	//==============================================================================
	pub fn get_camera(&self) -> &Camera3D {
		&self.camera
	}

	//==============================================================================
	// 카메라 반환. (투영과 보는 자리를 정할 때)
	//==============================================================================
	pub fn get_camera_mut(&mut self) -> &mut Camera3D {
		&mut self.camera
	}

	//==============================================================================
	// 지난 그리기의 화면 사각형 반환. (드로잉 버퍼 픽셀)
	//==============================================================================
	pub fn get_screen_rect(&self) -> Rect {
		self.screen_rect
	}

	//==============================================================================
	// 렌더러 읽기. (처음이면 짓고, 짓지 못하면 None 이고 다시 짓지 않는다)
	//==============================================================================
	pub fn read_renderer(&mut self, graphic: &mut Graphic) -> Option<Rc<RefCell<ToonRenderer>>> {
		if self.renderer.is_some() || self.is_renderer_failed {
			return self.renderer.clone();
		}
		graphic.flush_batch();
		match ToonRenderer::new(graphic.get_gl()) {
			Ok(renderer) => {
				self.renderer = Some(Rc::new(RefCell::new(renderer)));
			}
			Err(error) => {
				eprintln!("[ToonView3D] 툰 렌더러를 짓지 못해 대체 그리기로 갑니다. {}", error);
				self.is_renderer_failed = true;
				self.renderer = None;
			}
		}
		graphic.restore_render_state();
		self.renderer.clone()
	}

	//==============================================================================
	// 내용 영역을 드로잉 버퍼 픽셀의 사각형으로. (지금 변환으로 네 귀를 옮겨 감싸는 사각형)
	//==============================================================================
	pub fn refresh_screen_rect(&mut self, graphic: &Graphic, content_size: Vector2) {
		let transform_matrix = graphic.get_transform_matrix();
		let mut minimum = Vector2::create(f32::INFINITY, f32::INFINITY);
		let mut maximum = Vector2::create(f32::NEG_INFINITY, f32::NEG_INFINITY);
		for (corner_x, corner_y) in [(0.0, 0.0), (content_size.x, 0.0), (0.0, content_size.y), (content_size.x, content_size.y)] {
			let screen_position = transform_matrix.transform_point(corner_x, corner_y);
			minimum.x = minimum.x.min(screen_position.x);
			minimum.y = minimum.y.min(screen_position.y);
			maximum.x = maximum.x.max(screen_position.x);
			maximum.y = maximum.y.max(screen_position.y);
		}
		self.screen_rect = Rect::create(minimum.x, minimum.y, maximum.x - minimum.x, maximum.y - minimum.y);
	}
}

impl Default for ToonView3D {
	fn default() -> Self {
		ToonView3D::new()
	}
}

#[crate::reflect]
impl Component for ToonView3D {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"ToonView3D"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	// 내용 영역 안에서만 그리는지 반환. (가위로 자른다)
	//==============================================================================
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	// 출력. (렌더러가 없으면 대체 그리기, 있으면 run_external_pass 안에서 그리기 함수)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		if content_size.x <= 0.0 || content_size.y <= 0.0 {
			return;
		}
		let renderer = match self.read_renderer(graphic) {
			Some(renderer) => {
				renderer
			}
			None => {
				if let Some(fallback_draw_callback) = self.fallback_draw_callback.as_mut() {
					fallback_draw_callback(graphic, content_size);
				}
				return;
			}
		};
		if self.draw_callback.is_none() {
			return;
		}
		self.refresh_screen_rect(graphic, content_size);
		let screen_rect = self.screen_rect;
		if screen_rect.size.x <= 0.0 || screen_rect.size.y <= 0.0 {
			return;
		}
		self.camera.set_aspect_ratio(screen_rect.size.x / screen_rect.size.y);
		let camera = &self.camera;
		let draw_callback = match self.draw_callback.as_mut() {
			Some(draw_callback) => {
				draw_callback
			}
			None => {
				return;
			}
		};
		let drawing_buffer_height = graphic.get_drawing_buffer_height();
		graphic.run_external_pass(self.is_depth_cleared, |gl| {
			let mut guard = ScreenRectGuard { renderer: renderer.borrow_mut() };
			guard.renderer.set_screen_rect(screen_rect.position.x, screen_rect.position.y, screen_rect.size.x, screen_rect.size.y);
			draw_callback(&mut guard.renderer, camera, screen_rect, gl, drawing_buffer_height);
		});
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::node::nodetree::NodeTree;

	#[test]
	fn component_is_found_without_type_argument() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let node_id = node_tree.create_child(root_id, "view");
		let toon_view = node_tree[node_id].add_component(ToonView3D::new());
		toon_view.set_draw_callback(Some(Box::new(|_renderer, _camera, _screen_rect, _gl, _drawing_buffer_height| {
		})));
		toon_view.set_depth_cleared(false);
		let found = node_tree[node_id].get_component_mut::<ToonView3D>().expect("ToonView3D");
		assert!(!found.is_depth_cleared());
		assert!(found.get_renderer().is_none());
		assert!(!found.is_renderer_failed());
	}
}

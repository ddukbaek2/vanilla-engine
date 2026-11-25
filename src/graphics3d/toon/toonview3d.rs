//==============================================================================
// 툰 3D 를 노드 사각형 안에 그리는 컴포넌트. (vanilla.js experimental/graphics/toon/toonview3d.js 의 ToonView3D)
// - draw 에서: 노드의 내용 영역(0, 0, 내용 크기)을 지금 변환으로 드로잉 버퍼 픽셀의 사각형으로 옮기고(돌았으면 감싸는 사각형),
//   Graphic::run_external_pass 로 앞의 2D 묶음을 내보내고 상태 기억을 잊고 깊이를 비운 뒤, 렌더러의 화면 사각형(set_screen_rect)을 걸고 그리개를 부른다.
//   끝나면 화면 사각형을 풀고 restore_render_state 로 2D 상태를 되돌린다.
// - 렌더러는 처음 그릴 때 짓는다. 셰이더를 짓지 못하는 기기면 다시 짓지 않고 그리개의 draw_fallback 으로 2D 를 그린다.
// - 카메라(get_camera_mut)의 가로 세로 비는 그릴 때마다 사각형의 비로 맞춘다. 3D 점 위의 2D 표시는 같은 카메라의 project_to_view 로 셈한다.
// - 깊이 비우기: 부모 Mask 의 자르기 안이면 Adreno 660 함정(Graphic::clear_depth_buffer 머리말)이 있다. 자르기를 거는 쪽이
//   begin_clip_rect 앞에서 clear_depth_buffer 를 부르고 이 컴포넌트는 set_depth_cleared(false) 로 둔다.
// - 원본은 그리기 함수(setDrawCallback)와 대체 그리기 함수를 받는다. 러스트 판은 그리개(ToonViewDrawer 트레이트)를 타입 인자로 받아 들고,
//   씬은 node_tree[node_id].get_component_mut::<ToonView3D<그리개>>() 로 꺼내 get_drawer_mut 로 그릴 것을 넣는다.
// - 원본은 setRenderer 로 여러 노드가 렌더러 하나를 함께 쓴다. 러스트 판은 컴포넌트마다 렌더러를 든다. (공유 소유를 엔진에 들이지 않음)
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;
use crate::graphics3d::camera3d::Camera3D;
use crate::graphics3d::toon::toonrenderer::ToonRenderer;

//==============================================================================
// 그리개에 넘기는 한 번의 그리기. (GL, 렌더러, 카메라, 드로잉 버퍼 픽셀의 화면 사각형과 드로잉 버퍼 높이)
// - 렌더러에는 화면 사각형이 걸려 있다. 보통 ToonDrawQueue 에 담아 queue.draw(gl, renderer, camera) 하거나 begin_frame_with_camera 와 end_frame 사이에 그린다.
// - ToonPostProcess 를 쓰면 renderer.clear_screen_rect() 를 부르고 실수 렌더 타겟에 그린 뒤 end 의 뷰포트를 사각형으로 준다. (드로잉 버퍼 높이로 아래 원점 y 를 셈한다)
//==============================================================================
pub struct ToonViewPass<'a> {
	pub gl: &'a glow::Context,
	pub renderer: &'a mut ToonRenderer,
	pub camera: &'a Camera3D,
	pub screen_rect: Rect,
	pub drawing_buffer_height: i32,
}

//==============================================================================
// 그리개. (ToonView3D 가 그릴 때 부른다)
//==============================================================================
pub trait ToonViewDrawer: 'static {
	//==============================================================================
	// 툰 3D 그리기. (run_external_pass 안, 화면 사각형이 걸린 렌더러)
	//==============================================================================
	fn draw_toon(&mut self, pass: &mut ToonViewPass);

	//==============================================================================
	// 렌더러를 짓지 못한 기기에서 2D 로 그리기. (노드 변환이 걸린 Graphic, 원점은 내용 영역의 왼쪽 위, 기본은 그리지 않음)
	//==============================================================================
	fn draw_fallback(&mut self, _graphic: &mut Graphic, _content_size: Vector2) {
	}
}

pub struct ToonView3D<D: ToonViewDrawer> {
	component_base: ComponentBase,
	renderer: Option<ToonRenderer>,
	is_renderer_failed: bool,
	camera: Camera3D,
	drawer: D,
	is_depth_cleared: bool,
	screen_rect: Rect,
}

impl<D: ToonViewDrawer> ToonView3D<D> {
	//==============================================================================
	// 생성. (그리개를 받는다, 깊이를 비움)
	//==============================================================================
	pub fn new(drawer: D) -> ToonView3D<D> {
		ToonView3D {
			component_base: ComponentBase::new(),
			renderer: None,
			is_renderer_failed: false,
			camera: Camera3D::new(),
			drawer,
			is_depth_cleared: true,
			screen_rect: Rect::zero(),
		}
	}

	//==============================================================================
	// 그리개 반환.
	//==============================================================================
	pub fn get_drawer(&self) -> &D {
		&self.drawer
	}

	//==============================================================================
	// 그리개 반환. (그릴 것을 넣을 때)
	//==============================================================================
	pub fn get_drawer_mut(&mut self) -> &mut D {
		&mut self.drawer
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
	// 렌더러 반환. (아직 짓지 않았거나 짓지 못했으면 None)
	//==============================================================================
	pub fn get_renderer(&self) -> Option<&ToonRenderer> {
		self.renderer.as_ref()
	}

	//==============================================================================
	// 렌더러를 짓지 못했는지.
	//==============================================================================
	pub fn is_renderer_failed(&self) -> bool {
		self.is_renderer_failed
	}

	//==============================================================================
	// 지난 그리기의 화면 사각형 반환. (드로잉 버퍼 픽셀)
	//==============================================================================
	pub fn get_screen_rect(&self) -> Rect {
		self.screen_rect
	}

	//==============================================================================
	// 렌더러 마련하기. (처음이면 짓고, 짓지 못하면 다시 짓지 않는다)
	//==============================================================================
	fn prepare_renderer(&mut self, graphic: &mut Graphic) {
		if self.renderer.is_some() || self.is_renderer_failed {
			return;
		}
		graphic.flush_batch();
		match ToonRenderer::new(graphic.get_gl()) {
			Ok(renderer) => {
				self.renderer = Some(renderer);
			}
			Err(error) => {
				eprintln!("ToonView3D: 툰 렌더러를 짓지 못해 대체 그리기로 갑니다. {}", error);
				self.is_renderer_failed = true;
			}
		}
		graphic.restore_render_state();
	}

	//==============================================================================
	// 내용 영역을 드로잉 버퍼 픽셀의 사각형으로. (지금 변환으로 네 귀를 옮겨 감싸는 사각형)
	//==============================================================================
	fn calculate_screen_rect(graphic: &Graphic, content_size: Vector2) -> Rect {
		let transform_matrix = graphic.get_transform_matrix();
		let mut minimum = Vector2::create(f32::MAX, f32::MAX);
		let mut maximum = Vector2::create(f32::MIN, f32::MIN);
		for (corner_x, corner_y) in [(0.0, 0.0), (content_size.x, 0.0), (0.0, content_size.y), (content_size.x, content_size.y)] {
			let screen_position = transform_matrix.transform_point(corner_x, corner_y);
			minimum.x = minimum.x.min(screen_position.x);
			minimum.y = minimum.y.min(screen_position.y);
			maximum.x = maximum.x.max(screen_position.x);
			maximum.y = maximum.y.max(screen_position.y);
		}
		Rect::create(minimum.x, minimum.y, maximum.x - minimum.x, maximum.y - minimum.y)
	}
}

impl<D: ToonViewDrawer> Component for ToonView3D<D> {
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
	// 내용 영역 안에만 그린다. (가위로 자른다)
	//==============================================================================
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	// 출력. (렌더러가 없으면 대체 그리기, 있으면 run_external_pass 안에서 그리개)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		if content_size.x <= 0.0 || content_size.y <= 0.0 {
			return;
		}
		self.prepare_renderer(graphic);
		let renderer = match self.renderer.as_mut() {
			Some(renderer) => {
				renderer
			}
			None => {
				self.drawer.draw_fallback(graphic, content_size);
				return;
			}
		};
		let screen_rect = ToonView3D::<D>::calculate_screen_rect(graphic, content_size);
		self.screen_rect = screen_rect;
		if screen_rect.size.x <= 0.0 || screen_rect.size.y <= 0.0 {
			return;
		}
		self.camera.set_aspect_ratio(screen_rect.size.x / screen_rect.size.y);
		let camera = &self.camera;
		let drawer = &mut self.drawer;
		let drawing_buffer_height = graphic.get_drawing_buffer_height();
		graphic.run_external_pass(self.is_depth_cleared, |gl| {
			renderer.set_screen_rect(screen_rect.position.x, screen_rect.position.y, screen_rect.size.x, screen_rect.size.y);
			let mut pass = ToonViewPass { gl, renderer: &mut *renderer, camera, screen_rect, drawing_buffer_height };
			drawer.draw_toon(&mut pass);
			renderer.clear_screen_rect();
		});
	}
}

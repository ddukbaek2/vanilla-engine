//==============================================================================
//! 툰 3D 를 노드 사각형 안에 그리는 컴포넌트. (vanilla.js graphics3d/toon/toonview3d.js 의 ToonView3D)
//! - draw 에서: 노드의 내용 영역(0, 0, 내용 크기)을 지금 변환으로 드로잉 버퍼 픽셀의 사각형으로 옮기고(돌았으면 감싸는 사각형),
//!   Graphic::run_external_pass 로 앞의 2D 묶음을 내보내고 상태 기억을 잊고 깊이를 비운 뒤, 렌더러의 화면 사각형(set_screen_rect)을 걸고 그리기 함수를 부른다.
//!   끝나면 화면 사각형을 풀고 restore_render_state 로 2D 상태를 되돌린다. (그리기 함수가 패닉해도 둘 다 되돌린다)
//! - 렌더러는 처음 그릴 때 짓는다. 셰이더를 짓지 못하는 기기면 다시 짓지 않고 대체 그리기 함수(set_fallback_draw_callback)로 2D 를 그린다.
//!   여러 노드가 렌더러 하나를 함께 쓰려면 set_renderer 로 넣는다.
//! - 카메라(get_camera_mut)의 가로 세로 비는 그릴 때마다 사각형의 비로 맞춘다. 3D 점 위의 2D 표시는 같은 카메라의 project_to_view 로 셈한다.
//! - 깊이 비우기: 부모 Mask 의 자르기 안이면 Adreno 660 함정(Graphic::clear_depth_buffer 머리말)이 있다. 자르기를 거는 쪽이
//!   begin_clip_rect 앞에서 clear_depth_buffer 를 부르고 이 컴포넌트는 set_depth_cleared(false) 로 둔다.
//! - 그리기 함수: (렌더러, 카메라, 화면 사각형, GL, 출력). 화면 사각형은 지금 프레임 버퍼에서 그릴 사각형(픽셀, 왼쪽 위 원점)이다.
//!   출력(ToonViewOutput)은 지금 묶인 프레임 버퍼(None 이면 기본 프레임 버퍼)와 그 안의 GL 뷰포트([x, y, 너비, 높이], 정수, 왼쪽 아래 원점)다.
//!   보통 ToonDrawQueue 에 담아 queue.draw(gl, renderer, camera) 하거나 begin_frame_with_camera 와 end_frame 사이에 그린다.
//!   ToonPostProcess 를 쓰면 renderer.clear_screen_rect() 를 부르고 실수 렌더 타겟에 그린 뒤 end 에 출력의 프레임 버퍼와 뷰포트를 준다.
//!   (그래야 렌더 스케일, 투명도 층과 상관없이 맞는 자리에 낸다)
//! - 렌더 스케일(set_render_scale, 0.05 ~ 4, 기본 1): 1 이 아니면 3D 만 내용 영역의 화면 크기 x 배율의 판(깊이 24, 기본 프레임 버퍼가
//!   멀티샘플이면 표본 4)에 투명하게 비운 뒤 그리고, 2D 그리기 길로 선형 거르기, source-over 로 늘여 얹는다. (UI 는 기기 화소 그대로,
//!   유니티 URP renderScale, 언리얼 화면 비율과 같은 뜻) 판은 제 깊이를 쓰므로 set_depth_cleared(false) 로 앞의 3D 깊이를 이어 쓰던 것은 배율이 1 일 때만 된다.
//!   판은 노드에서 떨어질 때(detach)와 버려질 때(Drop) 지운다.
//! - 원본과 다른 것:
//!   - 공유 렌더러는 `Rc<RefCell<ToonRenderer>>` 다. 원본은 객체 참조를 나눠 든다. 그리는 동안 렌더러를 빌려 그리기 함수에 넘긴다.
//!   - 원본 화면 사각형 객체의 outputFramebuffer, outputViewport 는 다섯째 인자 ToonViewOutput 의 output_framebuffer, output_viewport 다.
//==============================================================================
use crate::core::glcontext::GlContext;
use std::cell::{RefCell, RefMut};
use std::rc::{Rc, Weak};


use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{BlendMode, Graphic, FLOATS_PER_VERTEX};
use crate::core::node::worldnode::NodeTransform;
use crate::graphics3d::camera3d::Camera3D;
use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
use crate::graphics3d::toon::toonrenderer::ToonRenderer;
use crate::graphics3d::{read_framebuffer_binding, GlFramebuffer};

/// 렌더 스케일 최솟값.
const MINIMUM_RENDER_SCALE: f64 = 0.05;
/// 렌더 스케일 최댓값.
const MAXIMUM_RENDER_SCALE: f64 = 4.0;
/// 렌더 스케일 판의 멀티샘플 표본 수. (기본 프레임 버퍼가 멀티샘플일 때, 데스크탑 캔버스의 표본 수와 같다)
const RENDER_SCALE_SAMPLE_COUNT: i32 = 4;

//==============================================================================
/// 그리기 함수. (렌더러, 카메라, 화면 사각형, GL, 출력)
//==============================================================================
pub type ToonViewDrawCallback = Box<dyn FnMut(&mut ToonRenderer, &Camera3D, Rect, &GlContext, ToonViewOutput)>;

//==============================================================================
/// 그리기 함수의 출력. (원본 화면 사각형 객체의 outputFramebuffer, outputViewport)
/// - ToonPostProcess::end 처럼 따로 내는 것은 이 둘로 내야 렌더 스케일, 투명도 층과 상관없이 맞는 자리에 낸다.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToonViewOutput {
	/// 지금 묶인 프레임 버퍼. (None 이면 기본 프레임 버퍼)
	pub output_framebuffer: Option<GlFramebuffer>,
	/// 그 안의 GL 뷰포트. ([x, y, 너비, 높이], 정수 픽셀, 왼쪽 아래 원점)
	pub output_viewport: [i32; 4],
}

//==============================================================================
/// 대체 그리기 함수. (렌더러를 짓지 못한 기기에서 2D 로, 노드 변환이 걸린 Graphic 과 내용 크기)
//==============================================================================
pub type ToonViewFallbackDrawCallback = Box<dyn FnMut(&mut Graphic, Vector2)>;

//==============================================================================
/// 렌더 스케일 판에 그리는 동안 들고, 끝나면(패닉해도) 판을 풀고 앞의 프레임 버퍼와 상태를 되돌리는 가드.
//==============================================================================
struct RenderScaleTargetGuard<'a> {
	gl: &'a GlContext,
	target: &'a RenderTarget,
	output_framebuffer: Option<GlFramebuffer>,
	color_write_mask: [bool; 4],
	is_stencil_test_enabled: bool,
	is_scissor_test_enabled: bool,
}

impl Drop for RenderScaleTargetGuard<'_> {
	//==============================================================================
	/// 판 풀기(리졸브), 앞의 프레임 버퍼 묶기, 색 쓰기 가리개와 스텐실, 가위 되돌리기.
	//==============================================================================
	fn drop(&mut self) {
		let gl = self.gl;
		self.target.resolve(gl);
		unsafe {
			crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, self.output_framebuffer);
			crate::core::glstatecache::color_mask(gl, self.color_write_mask[0], self.color_write_mask[1], self.color_write_mask[2], self.color_write_mask[3]);
			if self.is_stencil_test_enabled {
				crate::core::glstatecache::enable(gl, glow::STENCIL_TEST);
			}
			if self.is_scissor_test_enabled {
				crate::core::glstatecache::enable(gl, glow::SCISSOR_TEST);
			}
		}
	}
}

//==============================================================================
/// 그리는 동안 렌더러를 빌려 들고, 끝나면(패닉해도) 화면 사각형을 푸는 가드.
//==============================================================================
struct ScreenRectGuard<'a> {
	renderer: RefMut<'a, ToonRenderer>,
}

impl Drop for ScreenRectGuard<'_> {
	//==============================================================================
	/// 화면 사각형 풀기.
	//==============================================================================
	fn drop(&mut self) {
		self.renderer.clear_screen_rect();
	}
}

//==============================================================================
/// 툰 3D 를 노드 사각형 안에 그리는 컴포넌트. (머리말 참고)
///
/// # 예제
/// 노드 사각형 안에 툰 3D 장면 그리기
/// ```rust
/// use vanilla::{create_sphere_geometry, Camera3D, GlContext, Matrix4, MeshBuilder, NodeTree, Rect, ToonDrawOptions, ToonDrawQueue, ToonMesh, ToonPartOptions, ToonRenderer, ToonView3D, ToonViewOutput, Vector3};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let view_id = node_tree.create_child(root_id, "view");
/// node_tree[view_id].set_content_size_xy(320.0, 240.0);
/// let view = node_tree[view_id].add_component(ToonView3D::new());
/// view.get_camera_mut().set_perspective(0.8, 0.1, 100.0);
/// view.get_camera_mut().look_at(Vector3::create(0.0, 1.5, 3.0), Vector3::create(0.0, 0.5, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// let mut builder = MeshBuilder::new();
/// builder.add_part(&create_sphere_geometry(0.5, 24, 16, std::f64::consts::PI), &Matrix4::create_translation(0.0, 0.5, 0.0), "#f0b080", ToonPartOptions::default());
/// let arrays = builder.read_arrays();
/// let mut mesh: Option<ToonMesh> = None;
/// view.set_draw_callback(Some(Box::new(move |renderer: &mut ToonRenderer, camera: &Camera3D, _screen_rect: Rect, gl: &vanilla::GlContext, _output: ToonViewOutput| {
/// 	if mesh.is_none() {
/// 		mesh = renderer.create_mesh(gl, &arrays);
/// 	}
/// 	if let Some(mesh) = mesh.as_ref() {
/// 		let mut queue = ToonDrawQueue::new();
/// 		queue.add_mesh(mesh, &Matrix4::create_identity(), ToonDrawOptions::default());
/// 		queue.draw(gl, renderer, camera);
/// 	}
/// })));
/// assert!(view.get_renderer().is_none());
/// assert!(view.is_depth_cleared());
/// assert_eq!(view.get_render_scale(), 1.0);
/// ```
/// <!-- image: toon-view.webp -->
//==============================================================================
pub struct ToonView3D {
	component_base: ComponentBase,
	/// 툰 렌더러. (처음 그릴 때 짓거나 set_renderer 로 넣은 것, 여러 노드가 함께 쓸 수 있다)
	renderer: Option<Rc<RefCell<ToonRenderer>>>,
	/// 렌더러를 짓지 못함. (다시 짓지 않음)
	is_renderer_failed: bool,
	camera: Camera3D,
	draw_callback: Option<ToonViewDrawCallback>,
	fallback_draw_callback: Option<ToonViewFallbackDrawCallback>,
	is_depth_cleared: bool,
	/// 이번 그리기의 화면 사각형. (드로잉 버퍼 픽셀)
	screen_rect: Rect,
	/// 3D 를 그리는 판의 배율. (1 이면 화면에 바로, 아니면 그 배율의 판에 그려 선형으로 늘여 얹는다)
	render_scale: f64,
	/// 배율 판. (처음 쓸 때 짓는다)
	render_scale_target: Option<RenderTarget>,
	/// 배율 판을 지은 GL 컨텍스트의 약한 손잡이. (detach 와 Drop 이 판을 지울 때 쓴다)
	gl_context: Weak<glow::Context>,
}

#[crate::reflect]
impl ToonView3D {
	//==============================================================================
	/// 생성. (깊이를 비움)
	///
	/// # 돌려줌
	/// 새 컴포넌트. (렌더러는 처음 그릴 때 짓는다)
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
			render_scale: 1.0,
			render_scale_target: None,
			gl_context: Weak::new(),
		}
	}

	//==============================================================================
	/// 렌더 스케일 설정. (3D 만 이 배율의 판에 그려 선형 거르기로 늘여 얹는다, UI 는 기기 화소 그대로. 유니티 URP renderScale, 언리얼 화면 비율과 같은 뜻)
	/// - 1(기본)이면 화면에 바로 그린다. 1 이 아니면 판(내용 영역의 화면 크기 x 배율, 깊이 포함, 기본 프레임 버퍼가 멀티샘플이면 표본 4)에
	///   투명하게 비운 뒤 그리고, 2D 그리기 길로 source-over 로 얹는다. (2D 자르기를 따르고, 3D 가 덮지 않은 곳은 아래 2D 가 비친다)
	/// - 판은 제 깊이를 쓰므로 set_depth_cleared(false) 로 앞의 3D 깊이를 이어 쓰던 것은 배율이 1 일 때만 된다.
	/// - 그리기 함수는 넘겨받은 출력(ToonViewOutput)의 프레임 버퍼와 뷰포트로 내야 배율과 상관없이 맞는 자리에 낸다. (ToonPostProcess::end 같은 것)
	///
	/// # 인자
	/// - `render_scale`: 배율. (0.05 ~ 4, 1 이면 화면에 바로, 유한한 수가 아니면 1)
	///
	/// # 예제
	/// 3D 만 반 해상도로 그리기
	/// ```rust
	/// use vanilla::{NodeTree, ToonView3D};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let view_id = node_tree.create_child(root_id, "view");
	/// node_tree[view_id].set_content_size_xy(320.0, 240.0);
	/// let view = node_tree[view_id].add_component(ToonView3D::new());
	/// view.set_render_scale(0.5);
	/// assert_eq!(view.get_render_scale(), 0.5);
	/// ```
	//==============================================================================
	pub fn set_render_scale(&mut self, render_scale: f64) {
		if !render_scale.is_finite() {
			self.render_scale = 1.0;
			return;
		}
		self.render_scale = render_scale.clamp(MINIMUM_RENDER_SCALE, MAXIMUM_RENDER_SCALE);
	}

	//==============================================================================
	/// 렌더 스케일 반환.
	///
	/// # 돌려줌
	/// 배율. (1 이면 화면에 바로 그린다)
	//==============================================================================
	pub fn get_render_scale(&self) -> f64 {
		self.render_scale
	}

	//==============================================================================
	/// 그리기 함수 설정. (None 이면 그리지 않음)
	///
	/// # 인자
	/// - `draw_callback`: (렌더러, 카메라, 화면 사각형, GL, 출력) 를 받는 함수. (None 이면 그리지 않음)
	///   화면 사각형은 지금 프레임 버퍼에서 그릴 사각형(픽셀, 왼쪽 위 원점), 출력은 지금 묶인 프레임 버퍼와 그 안의 GL 뷰포트(왼쪽 아래 원점)다.
	//==============================================================================
	pub fn set_draw_callback(&mut self, draw_callback: Option<ToonViewDrawCallback>) {
		self.draw_callback = draw_callback;
	}

	//==============================================================================
	/// 대체 그리기 함수 설정. (렌더러를 짓지 못한 기기에서 2D 로)
	///
	/// # 인자
	/// - `fallback_draw_callback`: (graphic, content_size) 를 받는 함수. 렌더러를 짓지 못한 기기에서 부른다. (None 이면 그리지 않음)
	//==============================================================================
	pub fn set_fallback_draw_callback(&mut self, fallback_draw_callback: Option<ToonViewFallbackDrawCallback>) {
		self.fallback_draw_callback = fallback_draw_callback;
	}

	//==============================================================================
	/// 그리기 전에 깊이를 비울지 설정. (기본 true)
	///
	/// # 인자
	/// - `is_depth_cleared`: 그리기 전에 깊이를 비울지. (부모 Mask 의 자르기 안이면 false 로 두고 자르는 쪽이 비운다)
	//==============================================================================
	pub fn set_depth_cleared(&mut self, is_depth_cleared: bool) {
		self.is_depth_cleared = is_depth_cleared;
	}

	//==============================================================================
	/// 그리기 전에 깊이를 비우는지 반환.
	///
	/// # 돌려줌
	/// 그리기 전에 깊이를 비우면 true.
	//==============================================================================
	pub fn is_depth_cleared(&self) -> bool {
		self.is_depth_cleared
	}

	//==============================================================================
	/// 렌더러 넣기. (여러 노드가 하나를 함께 쓸 때, None 이면 처음 그릴 때 짓는다)
	///
	/// # 인자
	/// - `renderer`: 함께 쓸 렌더러. (None 이면 처음 그릴 때 짓는다)
	//==============================================================================
	pub fn set_renderer(&mut self, renderer: Option<Rc<RefCell<ToonRenderer>>>) {
		self.renderer = renderer;
		self.is_renderer_failed = false;
	}

	//==============================================================================
	/// 렌더러 반환. (아직 짓지 않았거나 짓지 못했으면 None)
	///
	/// # 돌려줌
	/// 렌더러. (아직 짓지 않았거나 짓지 못했으면 None)
	//==============================================================================
	pub fn get_renderer(&self) -> Option<&Rc<RefCell<ToonRenderer>>> {
		self.renderer.as_ref()
	}

	//==============================================================================
	/// 렌더러를 짓지 못했는지.
	///
	/// # 돌려줌
	/// 렌더러를 짓지 못했으면 true. (그때는 대체 그리기 함수로 그린다)
	//==============================================================================
	pub fn is_renderer_failed(&self) -> bool {
		self.is_renderer_failed
	}

	//==============================================================================
	/// 카메라 반환.
	///
	/// # 돌려줌
	/// 이 컴포넌트의 카메라. (가로 세로 비는 그릴 때마다 사각형에 맞춘다)
	//==============================================================================
	pub fn get_camera(&self) -> &Camera3D {
		&self.camera
	}

	//==============================================================================
	/// 카메라 반환. (투영과 보는 자리를 정할 때)
	///
	/// # 돌려줌
	/// 이 컴포넌트의 카메라. (가로 세로 비는 그릴 때마다 사각형에 맞춘다)
	//==============================================================================
	pub fn get_camera_mut(&mut self) -> &mut Camera3D {
		&mut self.camera
	}

	//==============================================================================
	/// 지난 그리기의 화면 사각형 반환. (드로잉 버퍼 픽셀)
	///
	/// # 돌려줌
	/// 드로잉 버퍼 픽셀의 사각형. (왼쪽 위가 원점, 아직 그리지 않았으면 빈 사각형)
	//==============================================================================
	pub fn get_screen_rect(&self) -> Rect {
		self.screen_rect
	}

	//==============================================================================
	/// 렌더러 읽기. (처음이면 짓고, 짓지 못하면 None 이고 다시 짓지 않는다)
	///
	/// # 인자
	/// - `graphic`: 그래픽. (처음이면 그 GL 로 렌더러를 짓는다)
	///
	/// # 돌려줌
	/// 렌더러. (짓지 못했으면 None)
	//==============================================================================
	pub fn read_renderer(&mut self, graphic: &mut Graphic) -> Option<Rc<RefCell<ToonRenderer>>> {
		if self.renderer.is_some() || self.is_renderer_failed {
			return self.renderer.clone();
		}
		graphic.flush_batch();
		match ToonRenderer::new(graphic.get_tracked_gl()) {
			Ok(renderer) => {
				self.renderer = Some(Rc::new(RefCell::new(renderer)));
			}
			Err(error) => {
				eprintln!("[ToonView3D] 툰 렌더러를 짓지 못해 대체 그리기로 갑니다. {}", error);
				self.renderer = None;
				// 셰이더 탓이면 게임에 한 번 알리고 다시 짓지 않는다. 컨텍스트를 잃은 탓이면 알리지도 기억하지도 않고, 되살린 뒤 다시 짓는다.
				if crate::core::shaderprogram::report_shader_build_failure(graphic.get_tracked_gl(), "ToonRenderer", &error) {
					self.is_renderer_failed = true;
				}
			}
		}
		graphic.restore_render_state();
		self.renderer.clone()
	}

	//==============================================================================
	/// 내용 영역을 드로잉 버퍼 픽셀의 사각형으로. (지금 변환으로 네 귀를 옮겨 감싸는 사각형)
	///
	/// # 인자
	/// - `graphic`: Graphic. (지금 변환을 읽는다)
	/// - `content_size`: 내용 영역의 크기.
	//==============================================================================
	pub fn refresh_screen_rect(&mut self, graphic: &Graphic, content_size: Vector2) {
		let transform_matrix = graphic.get_transform_matrix();
		let mut minimum = Vector2::create(f64::INFINITY, f64::INFINITY);
		let mut maximum = Vector2::create(f64::NEG_INFINITY, f64::NEG_INFINITY);
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
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Self {
		ToonView3D::new()
	}
}

#[crate::reflect]
impl Component for ToonView3D {
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
	/// 컴포넌트 이름.
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"ToonView3D"
	}

	//==============================================================================
	/// 갱신할 것이 없다.
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 내용 영역 안에서만 그리는지 반환. (가위로 자른다)
	//==============================================================================
	#[reflect(after = "getScreenRect")]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (렌더러가 없으면 대체 그리기, 있으면 run_external_pass 안에서 그리기 함수)
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
		if self.render_scale != 1.0 {
			self.draw_with_render_scale(graphic, &renderer);
			return;
		}
		let camera = &self.camera;
		let draw_callback = match self.draw_callback.as_mut() {
			Some(draw_callback) => {
				draw_callback
			}
			None => {
				return;
			}
		};
		graphic.run_external_pass(self.is_depth_cleared, |gl| {
			let output = read_output(gl, screen_rect.position.x, screen_rect.position.y, screen_rect.size.x, screen_rect.size.y);
			let mut guard = ScreenRectGuard { renderer: renderer.borrow_mut() };
			guard.renderer.set_screen_rect(screen_rect.position.x, screen_rect.position.y, screen_rect.size.x, screen_rect.size.y);
			draw_callback(&mut guard.renderer, camera, screen_rect, gl, output);
		});
	}

	//==============================================================================
	/// 노드에서 떨어짐. (배율 판을 지운다, 다시 붙으면 처음 쓸 때 다시 짓는다)
	//==============================================================================
	fn detach(&mut self, _transform: &mut NodeTransform) {
		self.destroy_render_scale_target();
	}
}

impl ToonView3D {
	//==============================================================================
	/// 배율 판에 그려 늘여 얹기.
	//==============================================================================
	fn draw_with_render_scale(&mut self, graphic: &mut Graphic, renderer: &Rc<RefCell<ToonRenderer>>) {
		let screen_rect = self.screen_rect;
		let render_scale = self.render_scale;
		let target_width = ((screen_rect.size.x * render_scale).round() as i32).max(1);
		let target_height = ((screen_rect.size.y * render_scale).round() as i32).max(1);
		let sample_count = if graphic.is_framebuffer_multisampled() { RENDER_SCALE_SAMPLE_COUNT } else { 0 };
		if let Some(gl_context) = crate::core::imagetexturecache::find_gl_context_handle(graphic.get_tracked_gl()) {
			self.gl_context = gl_context;
		}
		let camera = &self.camera;
		let render_scale_target = &mut self.render_scale_target;
		let render_scale_value = &mut self.render_scale;
		let draw_callback = match self.draw_callback.as_mut() {
			Some(draw_callback) => {
				draw_callback
			}
			None => {
				return;
			}
		};
		let color_texture = graphic.run_external_pass(false, |gl| {
			let target = read_render_scale_target(gl, render_scale_target, render_scale_value, target_width, target_height, sample_count)?;
			// 2D 자르기의 스텐실과 가위는 화면 것이라 판에는 걸지 않는다. (얹을 때 2D 길이 자른다)
			let (is_stencil_test_enabled, is_scissor_test_enabled, clear_color, color_write_mask) = unsafe {
				let mut clear_color = [0.0f32; 4];
				crate::core::glstatecache::get_parameter_f32_slice(gl, glow::COLOR_CLEAR_VALUE, &mut clear_color);
				(crate::core::glstatecache::is_enabled(gl, glow::STENCIL_TEST), crate::core::glstatecache::is_enabled(gl, glow::SCISSOR_TEST), clear_color, crate::core::glstatecache::get_parameter_bool_array::<4>(gl, glow::COLOR_WRITEMASK))
			};
			let _target_guard = RenderScaleTargetGuard {
				gl,
				target,
				output_framebuffer: read_framebuffer_binding(gl),
				color_write_mask,
				is_stencil_test_enabled,
				is_scissor_test_enabled,
			};
			unsafe {
				crate::core::glstatecache::disable(gl, glow::STENCIL_TEST);
				crate::core::glstatecache::disable(gl, glow::SCISSOR_TEST);
				target.bind(gl);
				crate::core::glstatecache::color_mask(gl, true, true, true, true);
				crate::core::glstatecache::depth_mask(gl, true);
				crate::core::glstatecache::clear_color(gl, 0.0, 0.0, 0.0, 0.0);
				gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
				crate::core::glstatecache::clear_color(gl, clear_color[0], clear_color[1], clear_color[2], clear_color[3]);
			}
			let output = read_output(gl, 0.0, 0.0, target_width as f64, target_height as f64);
			let target_rect = Rect::create(0.0, 0.0, target_width as f64, target_height as f64);
			{
				// 렌더러 가드가 먼저 풀려(화면 사각형) 그 뒤 판 가드가 판을 풀고 앞의 프레임 버퍼를 묶는다. (원본 finally 와 같은 차례)
				let mut guard = ScreenRectGuard { renderer: renderer.borrow_mut() };
				guard.renderer.set_screen_rect(0.0, 0.0, target_width as f64, target_height as f64);
				draw_callback(&mut guard.renderer, camera, target_rect, gl, output);
			}
			target.get_color_texture(0)
		});
		let color_texture = match color_texture {
			Some(color_texture) => {
				color_texture
			}
			None => {
				return;
			}
		};

		// 판을 화면 사각형에 단위 행렬, 흰색, 틴트 없이 source-over 로 얹는다. (판은 미리 곱한 알파, 텍스처는 아래가 0 행, 선형 거르기)
		// 3D 를 화면에 바로 그릴 때처럼 전역 알파는 걸지 않는다.
		graphic.push_state();
		graphic.reset_transform();
		graphic.set_global_alpha(1.0);
		graphic.set_image_tint_color(None);
		graphic.set_blend_mode(BlendMode::Normal);
		let float_count = graphic.write_quad(0, screen_rect.position.x, screen_rect.position.y, screen_rect.position.x + screen_rect.size.x, screen_rect.position.y + screen_rect.size.y, 0.0, 1.0, 1.0, 0.0);
		let white_color = graphic.get_white_color();
		graphic.draw_vertices(float_count / FLOATS_PER_VERTEX, color_texture, white_color);
		graphic.pop_state();
	}

	//==============================================================================
	/// 배율 판 지우기. (GL 컨텍스트가 이미 사라졌으면 판도 같이 사라졌으므로 놓기만 한다)
	//==============================================================================
	fn destroy_render_scale_target(&mut self) {
		let Some(mut render_scale_target) = self.render_scale_target.take() else {
			return;
		};
		if let Some(gl) = self.gl_context.upgrade() {
			render_scale_target.destroy(GlContext::from_context(&gl));
		}
	}
}

impl Drop for ToonView3D {
	//==============================================================================
	/// 버려질 때 배율 판 지우기.
	//==============================================================================
	fn drop(&mut self) {
		self.destroy_render_scale_target();
	}
}

//==============================================================================
/// 그리기 함수에 넘길 출력 읽기. (지금 묶인 프레임 버퍼와 그 안의 GL 뷰포트, 왼쪽 아래 원점, 원본 writeOutputScreenRect)
/// - 사각형은 지금 뷰포트 안의 픽셀(왼쪽 위 원점)이다. 정수 픽셀로 반올림한다. (GL 뷰포트는 정수)
//==============================================================================
fn read_output(gl: &GlContext, x: f64, y: f64, width: f64, height: f64) -> ToonViewOutput {
	let mut viewport = [0i32; 4];
	unsafe {
		crate::core::glstatecache::get_parameter_i32_slice(gl, glow::VIEWPORT, &mut viewport);
	}
	let output_width = (width.round() as i32).max(1);
	let output_height = (height.round() as i32).max(1);
	ToonViewOutput {
		output_framebuffer: read_framebuffer_binding(gl),
		output_viewport: [viewport[0] + x.round() as i32, viewport[1] + viewport[3] - y.round() as i32 - output_height, output_width, output_height],
	}
}

//==============================================================================
/// 배율 판 읽기. (없거나 크기가 다르면 짓거나 고친다, 못 지으면 렌더 스케일을 1 로 되돌리고 None)
//==============================================================================
fn read_render_scale_target<'a>(gl: &GlContext, render_scale_target: &'a mut Option<RenderTarget>, render_scale: &mut f64, width: i32, height: i32, sample_count: i32) -> Option<&'a RenderTarget> {
	if render_scale_target.is_none() {
		let options = RenderTargetOptions { sample_count, depth_bits: 24, ..RenderTargetOptions::default() };
		match RenderTarget::new(gl, width, height, true, Some(options)) {
			Ok(target) => {
				*render_scale_target = Some(target);
			}
			Err(error) => {
				eprintln!("[ToonView3D] 렌더 스케일 판을 짓지 못해 화면에 바로 그립니다. {}", error);
				*render_scale = 1.0;
				return None;
			}
		}
	}
	let target = render_scale_target.as_mut()?;
	if target.get_width() != width || target.get_height() != height {
		if let Err(error) = target.resize(gl, width, height) {
			eprintln!("[ToonView3D] 렌더 스케일 판 크기를 바꾸지 못했습니다. {}", error);
		}
	}
	Some(target)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::node::nodetree::NodeTree;

	//==============================================================================
	/// 노드에 붙인 컴포넌트를 다시 찾아 설정 읽기. (깊이 비우기를 끈 것, 렌더러는 아직 없음)
	//==============================================================================
	#[test]
	fn component_is_found_without_type_argument() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let node_id = node_tree.create_child(root_id, "view");
		let toon_view = node_tree[node_id].add_component(ToonView3D::new());
		toon_view.set_draw_callback(Some(Box::new(|_renderer, _camera, _screen_rect, _gl, _output| {
		})));
		toon_view.set_depth_cleared(false);
		let found = node_tree[node_id].get_component_mut::<ToonView3D>().expect("ToonView3D");
		assert!(!found.is_depth_cleared());
		assert!(found.get_renderer().is_none());
		assert!(!found.is_renderer_failed());
	}

	//==============================================================================
	/// 렌더 스케일: 기본 1, 0.05 ~ 4 로 자르고, 유한하지 않으면 1. (원본 setRenderScale 과 같다)
	//==============================================================================
	#[test]
	fn render_scale_is_clamped() {
		let mut toon_view = ToonView3D::new();
		assert_eq!(toon_view.get_render_scale(), 1.0);
		toon_view.set_render_scale(0.5);
		assert_eq!(toon_view.get_render_scale(), 0.5);
		toon_view.set_render_scale(0.01);
		assert_eq!(toon_view.get_render_scale(), 0.05);
		toon_view.set_render_scale(8.0);
		assert_eq!(toon_view.get_render_scale(), 4.0);
		toon_view.set_render_scale(f64::NAN);
		assert_eq!(toon_view.get_render_scale(), 1.0);
		toon_view.set_render_scale(f64::INFINITY);
		assert_eq!(toon_view.get_render_scale(), 1.0);
	}
}

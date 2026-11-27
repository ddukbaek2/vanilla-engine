//==============================================================================
//! GL 상태 캐시. (원본 WebGLStateCache)
//! - 원본은 컨텍스트 객체의 상태 함수(bindFramebuffer, viewport, enable, useProgram, bindTexture 같은 것)를 가로채 바뀐 값을 기억하고,
//!   getParameter, isEnabled 를 기억한 값으로 답한다. glow 는 가로챌 수 없어, 컨텍스트마다 하나인 캐시를 두고
//!   엔진의 상태 함수 호출이 이 모듈의 같은 이름 함수(첫 인자 gl)를 거쳐 GL 을 부른다.
//! - 상태를 바꾸는 함수는 늘 GL 을 부르고 바뀐 값을 기억한다. 같은 값이면 건너뛰는 것은 부르는 쪽이 is_program_in_use 같은 것으로 물어 한다. (원본과 같음)
//! - 읽는 함수(get_parameter_*, is_enabled, read_*)는 기억한 값으로 답한다. 모르는 값(처음, reset 뒤, 검사하지 못한 인자를 받은 뒤)만 GL 에 한 번 묻고 기억한다.
//! - 기억하지 않는 값(스텐실 함수, ARRAY_BUFFER 밖의 버퍼 묶음 같은 것)은 GL 에 그대로 묻는다. ARRAY_BUFFER 묶음은 견주기(is_array_buffer_bound)에만 쓰고 읽기는 GL 에 묻는다. (원본과 같음) 바뀌지 않는 한도 값(MAX_SAMPLES 같은 것)은 한 번 묻고 기억한다.
//! - 이 모듈을 거치지 않고 glow 로 상태를 바꾸면 reset 을 부른다. (Graphic::reset_gl_state_cache, 게임에 넘기는 GlContext 의 상태 함수는 이 모듈을 거친다)
//! - 검사 모드(VANILLA_GL_STATE_CHECK=1)는 그리기 앞마다 기억한 값을 진짜 GL 과 견준다. (아래 '검사 모드')
//! - 캐시를 걸지 않은 컨텍스트(Graphic 밖에서 만든 것)는 기억 없이 GL 을 그대로 부르고 묻는다.
//==============================================================================
use crate::core::glcontext::GlowContextSource;
use crate::core::graphic::{GlBuffer, GlFramebuffer, GlProgram, GlTexture, GlVertexArray};
use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

//==============================================================================
// 상수 목록.
//==============================================================================
/// 켜고 끄는 기능 가운데 기억하는 것. (WebGL2 의 enable, disable 이 받는 것 모두, 차례가 칸 번호)
const CAPABILITY_LIST: [u32; 10] = [glow::BLEND, glow::CULL_FACE, glow::DEPTH_TEST, glow::DITHER, glow::POLYGON_OFFSET_FILL, glow::SAMPLE_ALPHA_TO_COVERAGE, glow::SAMPLE_COVERAGE, glow::SCISSOR_TEST, glow::STENCIL_TEST, glow::RASTERIZER_DISCARD];

/// 텍스처 대상. (차례가 칸 번호)
const TEXTURE_TARGET_LIST: [u32; 4] = [glow::TEXTURE_2D, glow::TEXTURE_CUBE_MAP, glow::TEXTURE_3D, glow::TEXTURE_2D_ARRAY];

/// 버퍼 묶음 대상. (WebGL2 의 bindBuffer 가 받는 것 모두, 원본 BUFFER_TARGET_SET)
const BUFFER_TARGET_LIST: [u32; 8] = [glow::ARRAY_BUFFER, glow::ELEMENT_ARRAY_BUFFER, glow::COPY_READ_BUFFER, glow::COPY_WRITE_BUFFER, glow::TRANSFORM_FEEDBACK_BUFFER, glow::UNIFORM_BUFFER, glow::PIXEL_PACK_BUFFER, glow::PIXEL_UNPACK_BUFFER];

/// 섞기 인자로 받는 값.
const BLEND_FACTOR_LIST: [u32; 15] = [glow::ZERO, glow::ONE, glow::SRC_COLOR, glow::ONE_MINUS_SRC_COLOR, glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::DST_ALPHA, glow::ONE_MINUS_DST_ALPHA, glow::DST_COLOR, glow::ONE_MINUS_DST_COLOR, glow::SRC_ALPHA_SATURATE, glow::CONSTANT_COLOR, glow::ONE_MINUS_CONSTANT_COLOR, glow::CONSTANT_ALPHA, glow::ONE_MINUS_CONSTANT_ALPHA];

/// 섞기 식으로 받는 값. (원본 BLEND_EQUATION_SET)
const BLEND_EQUATION_LIST: [u32; 5] = [glow::FUNC_ADD, glow::FUNC_SUBTRACT, glow::FUNC_REVERSE_SUBTRACT, glow::MIN, glow::MAX];


/// 바뀌지 않는 한도 값 가운데 i32 하나로 읽는 것. (처음 읽을 때 한 번 묻고 기억한다)
const CONSTANT_PARAMETER_LIST: [u32; 17] = [glow::MAX_TEXTURE_SIZE, glow::MAX_CUBE_MAP_TEXTURE_SIZE, glow::MAX_3D_TEXTURE_SIZE, glow::MAX_ARRAY_TEXTURE_LAYERS, glow::MAX_RENDERBUFFER_SIZE, glow::MAX_SAMPLES, glow::MAX_TEXTURE_IMAGE_UNITS, glow::MAX_VERTEX_TEXTURE_IMAGE_UNITS, glow::MAX_COMBINED_TEXTURE_IMAGE_UNITS, glow::MAX_VERTEX_ATTRIBS, glow::MAX_VERTEX_UNIFORM_VECTORS, glow::MAX_FRAGMENT_UNIFORM_VECTORS, glow::MAX_VARYING_VECTORS, glow::MAX_DRAW_BUFFERS, glow::MAX_COLOR_ATTACHMENTS, glow::MAX_UNIFORM_BUFFER_BINDINGS, glow::MAX_UNIFORM_BLOCK_SIZE];

thread_local! {
	/// 컨텍스트마다 하나인 상태 캐시. (원본 stateCacheTable, 컨텍스트가 사라지면 다음에 걸 때 치운다)
	static STATE_CACHE_LIST: RefCell<Vec<(Weak<glow::Context>, Rc<GlStateCache>)>> = const { RefCell::new(Vec::new()) };
}

//==============================================================================
/// GL 상태 캐시. (원본 WebGLStateCache, 칸마다 None 이면 모른다)
//==============================================================================
pub(crate) struct GlStateCache {
	draw_framebuffer: Cell<Option<Option<GlFramebuffer>>>,
	read_framebuffer: Cell<Option<Option<GlFramebuffer>>>,
	viewport: Cell<Option<[i32; 4]>>,
	scissor_box: Cell<Option<[i32; 4]>>,
	clear_color: Cell<Option<[f32; 4]>>,
	color_write_mask: Cell<Option<[bool; 4]>>,
	blend_factors: Cell<Option<[u32; 4]>>,
	blend_equations: Cell<Option<[u32; 2]>>,
	depth_write_mask: Cell<Option<bool>>,
	depth_function: Cell<Option<u32>>,
	front_face: Cell<Option<u32>>,
	cull_face_mode: Cell<Option<u32>>,
	program: Cell<Option<Option<GlProgram>>>,
	vertex_array: Cell<Option<Option<GlVertexArray>>>,
	array_buffer: Cell<Option<Option<GlBuffer>>>, // ARRAY_BUFFER 에 묶은 버퍼. (버텍스 어레이에 들지 않는 전역 묶음, 견주기에만 쓰고 읽기는 GL 에 묻는다)
	active_texture: Cell<Option<u32>>,
	capability_list: [Cell<Option<bool>>; 10],
	texture_binding_table: RefCell<HashMap<u32, Option<GlTexture>>>, // 키 유닛 * 4 + 대상 칸. (없으면 모른다)
	pixel_store_table: RefCell<HashMap<u32, i32>>,
	constant_parameter_table: RefCell<HashMap<u32, i32>>,
	maximum_viewport_size: Cell<Option<[i32; 2]>>,
	texture_target_table: RefCell<HashMap<GlTexture, usize>>, // 텍스처마다 처음 묶은 대상 칸. (다른 대상으로 묶으면 GL 이 무시한다)
	program_link_status_table: RefCell<HashMap<GlProgram, bool>>, // 링크에 실패한 프로그램은 쓰게 해도 GL 이 무시한다.
	deleted_framebuffer_set: RefCell<HashSet<GlFramebuffer>>, // 지운 것. (묶으면 GL 이 무시한다, 이 모듈의 create_* 로 같은 이름을 다시 받으면 뺀다)
	deleted_texture_set: RefCell<HashSet<GlTexture>>,
	deleted_vertex_array_set: RefCell<HashSet<GlVertexArray>>,
	deleted_buffer_set: RefCell<HashSet<GlBuffer>>,
	buffer_element_table: RefCell<HashMap<GlBuffer, bool>>, // 버퍼마다 처음 묶은 대상이 ELEMENT_ARRAY_BUFFER 인지. (WebGL 은 둘 사이를 오가며 묶지 못하게 한다)
}

impl GlStateCache {
	//==============================================================================
	/// 생성. (모두 모름)
	//==============================================================================
	fn new() -> GlStateCache {
		GlStateCache {
			draw_framebuffer: Cell::new(None),
			read_framebuffer: Cell::new(None),
			viewport: Cell::new(None),
			scissor_box: Cell::new(None),
			clear_color: Cell::new(None),
			color_write_mask: Cell::new(None),
			blend_factors: Cell::new(None),
			blend_equations: Cell::new(None),
			depth_write_mask: Cell::new(None),
			depth_function: Cell::new(None),
			front_face: Cell::new(None),
			cull_face_mode: Cell::new(None),
			program: Cell::new(None),
			vertex_array: Cell::new(None),
			array_buffer: Cell::new(None),
			active_texture: Cell::new(None),
			capability_list: Default::default(),
			texture_binding_table: RefCell::new(HashMap::new()),
			pixel_store_table: RefCell::new(HashMap::new()),
			constant_parameter_table: RefCell::new(HashMap::new()),
			maximum_viewport_size: Cell::new(None),
			texture_target_table: RefCell::new(HashMap::new()),
			program_link_status_table: RefCell::new(HashMap::new()),
			deleted_framebuffer_set: RefCell::new(HashSet::new()),
			deleted_texture_set: RefCell::new(HashSet::new()),
			deleted_vertex_array_set: RefCell::new(HashSet::new()),
			deleted_buffer_set: RefCell::new(HashSet::new()),
			buffer_element_table: RefCell::new(HashMap::new()),
		}
	}

	//==============================================================================
	/// 기억 모두 잊기. (원본 reset, 다음에 읽을 때 GL 에 한 번씩 묻는다, 바뀌지 않는 한도 값과 객체마다의 기억은 그대로 둔다)
	//==============================================================================
	pub(crate) fn reset(&self) {
		self.draw_framebuffer.set(None);
		self.read_framebuffer.set(None);
		self.viewport.set(None);
		self.scissor_box.set(None);
		self.clear_color.set(None);
		self.color_write_mask.set(None);
		self.blend_factors.set(None);
		self.blend_equations.set(None);
		self.depth_write_mask.set(None);
		self.depth_function.set(None);
		self.front_face.set(None);
		self.cull_face_mode.set(None);
		self.program.set(None);
		self.vertex_array.set(None);
		self.array_buffer.set(None);
		self.active_texture.set(None);
		for capability in &self.capability_list {
			capability.set(None);
		}
		self.texture_binding_table.borrow_mut().clear();
		self.pixel_store_table.borrow_mut().clear();
	}

	//==============================================================================
	/// 바뀌지 않는 한도 값 읽기. (처음에 한 번 묻는다)
	//==============================================================================
	fn read_constant_parameter(&self, gl: &glow::Context, parameter: u32) -> i32 {
		if let Some(value) = self.constant_parameter_table.borrow().get(&parameter) {
			return *value;
		}
		let value = unsafe { gl.get_parameter_i32(parameter) };
		self.constant_parameter_table.borrow_mut().insert(parameter, value);
		value
	}

	//==============================================================================
	/// 뷰포트 크기 한도 읽기. (원본 MAX_VIEWPORT_DIMS)
	//==============================================================================
	fn read_maximum_viewport_size(&self, gl: &glow::Context) -> [i32; 2] {
		if let Some(value) = self.maximum_viewport_size.get() {
			return value;
		}
		let mut value = [0i32; 2];
		unsafe {
			gl.get_parameter_i32_slice(glow::MAX_VIEWPORT_DIMS, &mut value);
		}
		self.maximum_viewport_size.set(Some(value));
		value
	}

	//==============================================================================
	/// 켜고 끄는 기능의 칸 번호. (기억하지 않는 것이면 None)
	//==============================================================================
	fn find_capability_index(capability: u32) -> Option<usize> {
		CAPABILITY_LIST.iter().position(|&value| value == capability)
	}

	//==============================================================================
	/// 활성 텍스처 유닛 읽기. (TEXTURE0 + 번호)
	//==============================================================================
	fn read_active_texture(&self, gl: &glow::Context) -> u32 {
		if let Some(active_texture) = self.active_texture.get() {
			return active_texture;
		}
		let active_texture = unsafe { gl.get_parameter_i32(glow::ACTIVE_TEXTURE) } as u32;
		self.active_texture.set(Some(active_texture));
		active_texture
	}

	//==============================================================================
	/// 한 대상의 텍스처 묶음 기억을 모든 유닛에서 잊기.
	//==============================================================================
	fn forget_texture_target(&self, texture_target_index: usize) {
		self.texture_binding_table.borrow_mut().retain(|key, _| *key as usize % 4 != texture_target_index);
	}
}

//==============================================================================
/// 섞기 인자 짝을 GL 이 받는지. (원본 isBlendFactorPairValid)
//==============================================================================
fn is_blend_factor_pair_valid(source_factor: u32, destination_factor: u32) -> bool {
	if !BLEND_FACTOR_LIST.contains(&source_factor) || !BLEND_FACTOR_LIST.contains(&destination_factor) {
		return false;
	}
	// WebGL 은 상수 색과 상수 알파를 소스와 대상에 섞어 쓰지 못하게 한다. (INVALID_OPERATION)
	let is_constant_color = |factor: u32| factor == glow::CONSTANT_COLOR || factor == glow::ONE_MINUS_CONSTANT_COLOR;
	let is_constant_alpha = |factor: u32| factor == glow::CONSTANT_ALPHA || factor == glow::ONE_MINUS_CONSTANT_ALPHA;
	if (is_constant_color(source_factor) && is_constant_alpha(destination_factor)) || (is_constant_alpha(source_factor) && is_constant_color(destination_factor)) {
		return false;
	}
	// 대상 인자의 SRC_ALPHA_SATURATE 는 판마다 받는지가 달라 모른다로 둔다.
	destination_factor != glow::SRC_ALPHA_SATURATE
}

//==============================================================================
/// 컨텍스트에 상태 캐시 걸기. (원본 WebGLStateCache.attach, 같은 컨텍스트에는 하나만, Graphic::new 가 부른다)
//==============================================================================
pub(crate) fn attach(gl: &Rc<glow::Context>) -> Rc<GlStateCache> {
	if let Some(state_cache) = find(gl) {
		return state_cache;
	}
	let state_cache = Rc::new(GlStateCache::new());
	STATE_CACHE_LIST.with(|state_cache_list| {
		let mut state_cache_list = state_cache_list.borrow_mut();
		state_cache_list.retain(|(context, _)| context.strong_count() > 0);
		state_cache_list.push((Rc::downgrade(gl), state_cache.clone()));
	});
	state_cache
}

//==============================================================================
/// 컨텍스트의 상태 캐시 찾기. (원본 WebGLStateCache.find, 걸지 않았으면 None)
//==============================================================================
pub(crate) fn find(gl: &impl GlowContextSource) -> Option<Rc<GlStateCache>> {
	let gl = gl.read_glow_context();
	STATE_CACHE_LIST.with(|state_cache_list| {
		state_cache_list.borrow().iter().find(|(context, _)| std::ptr::eq(context.as_ptr(), gl) && context.strong_count() > 0).map(|(_, state_cache)| state_cache.clone())
	})
}

//==============================================================================
/// 컨텍스트의 상태 캐시로 하기. (걸지 않았으면 None 을 받는다)
//==============================================================================
fn with_state_cache<R>(gl: &glow::Context, callback: impl FnOnce(Option<&GlStateCache>) -> R) -> R {
	STATE_CACHE_LIST.with(|state_cache_list| {
		let state_cache_list = state_cache_list.borrow();
		let state_cache = state_cache_list.iter().find(|(context, _)| std::ptr::eq(context.as_ptr(), gl) && context.strong_count() > 0).map(|(_, state_cache)| state_cache.as_ref());
		callback(state_cache)
	})
}

//==============================================================================
/// 컨텍스트의 기억 모두 잊기. (원본 Graphic.resetGLStateCache, 이 모듈을 거치지 않고 glow 로 상태를 바꾼 뒤)
//==============================================================================
pub(crate) fn reset(gl: &impl GlowContextSource) {
	let gl = gl.read_glow_context();
	note_call(|| String::from("reset"));
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.reset();
		}
	});
}

//==============================================================================
// 프레임 버퍼.
//==============================================================================
/// 다 쓴 첨부 버리기. (원본 invalidateFramebuffer, 버리기는 그린 결과를 바꾸지 않는 힌트라 GL 4.3, ARB_invalidate_subdata, GLES 3 이 아니면(맥 데스크탑 GL 4.1) 부르지 않는다)
pub(crate) unsafe fn invalidate_framebuffer(gl: &impl GlowContextSource, target: u32, attachments: &[u32]) {
	let gl = gl.read_glow_context();
	let version = gl.version();
	let is_supported = (version.is_embedded && version.major >= 3) || version.major > 4 || (version.major == 4 && version.minor >= 3) || gl.supported_extensions().contains("GL_ARB_invalidate_subdata");
	if !is_supported {
		return;
	}
	note_call(|| format!("invalidate_framebuffer(0x{:X}, {:?})", target, attachments));
	unsafe {
		gl.invalidate_framebuffer(target, attachments);
	}
}

/// 프레임 버퍼 묶기. (원본 bindFramebuffer)
pub(crate) unsafe fn bind_framebuffer(gl: &impl GlowContextSource, target: u32, framebuffer: Option<GlFramebuffer>) {
	let gl = gl.read_glow_context();
	note_call(|| format!("bind_framebuffer(0x{:X}, {:?})", target, framebuffer));
	unsafe {
		gl.bind_framebuffer(target, framebuffer);
	}
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return;
		};
		if let Some(framebuffer) = framebuffer {
			if state_cache.deleted_framebuffer_set.borrow().contains(&framebuffer) {
				state_cache.draw_framebuffer.set(None);
				state_cache.read_framebuffer.set(None);
				return;
			}
		}
		if target == glow::FRAMEBUFFER {
			state_cache.draw_framebuffer.set(Some(framebuffer));
			state_cache.read_framebuffer.set(Some(framebuffer));
		}
		else if target == glow::DRAW_FRAMEBUFFER {
			state_cache.draw_framebuffer.set(Some(framebuffer));
		}
		else if target == glow::READ_FRAMEBUFFER {
			state_cache.read_framebuffer.set(Some(framebuffer));
		}
	});
}

/// 프레임 버퍼 만들기. (지운 이름을 GL 이 다시 주면 지운 기억에서 뺀다)
pub(crate) unsafe fn create_framebuffer(gl: &impl GlowContextSource) -> Result<GlFramebuffer, String> {
	let gl = gl.read_glow_context();
	note_call(|| String::from("create_framebuffer()"));
	let framebuffer = unsafe { gl.create_framebuffer() }?;
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.deleted_framebuffer_set.borrow_mut().remove(&framebuffer);
		}
	});
	Ok(framebuffer)
}

/// 프레임 버퍼 지우기. (원본 deleteFramebuffer, 묶인 것을 지우면 GL 이 기본 프레임 버퍼로 되돌린다)
pub(crate) unsafe fn delete_framebuffer(gl: &impl GlowContextSource, framebuffer: GlFramebuffer) {
	let gl = gl.read_glow_context();
	note_call(|| format!("delete_framebuffer({:?})", framebuffer));
	unsafe {
		gl.delete_framebuffer(framebuffer);
	}
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return;
		};
		state_cache.deleted_framebuffer_set.borrow_mut().insert(framebuffer);
		if state_cache.draw_framebuffer.get() == Some(Some(framebuffer)) {
			state_cache.draw_framebuffer.set(Some(None));
		}
		if state_cache.read_framebuffer.get() == Some(Some(framebuffer)) {
			state_cache.read_framebuffer.set(Some(None));
		}
	});
}

//==============================================================================
// 뷰포트와 가위. (너비나 높이가 음수면 GL 이 무시한다, 뷰포트 크기는 한도에 맞춰 줄인다)
//==============================================================================
/// 뷰포트. (원본 viewport)
pub(crate) unsafe fn viewport(gl: &impl GlowContextSource, x: i32, y: i32, width: i32, height: i32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("viewport({:?}, {:?}, {:?}, {:?})", x, y, width, height));
	unsafe {
		gl.viewport(x, y, width, height);
	}
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return;
		};
		if width < 0 || height < 0 {
			return;
		}
		let maximum_viewport_size = state_cache.read_maximum_viewport_size(gl);
		if maximum_viewport_size[0] <= 0 || maximum_viewport_size[1] <= 0 {
			state_cache.viewport.set(None);
			return;
		}
		state_cache.viewport.set(Some([x, y, width.min(maximum_viewport_size[0]), height.min(maximum_viewport_size[1])]));
	});
}

/// 가위 상자. (원본 scissor)
pub(crate) unsafe fn scissor(gl: &impl GlowContextSource, x: i32, y: i32, width: i32, height: i32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("scissor({:?}, {:?}, {:?}, {:?})", x, y, width, height));
	unsafe {
		gl.scissor(x, y, width, height);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			if width >= 0 && height >= 0 {
				state_cache.scissor_box.set(Some([x, y, width, height]));
			}
		}
	});
}

//==============================================================================
// 켜고 끄기.
//==============================================================================
/// 기능 켜기. (원본 enable)
pub(crate) unsafe fn enable(gl: &impl GlowContextSource, capability: u32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("enable(0x{:X})", capability));
	unsafe {
		gl.enable(capability);
	}
	set_capability(gl, capability, true);
}

/// 기능 끄기. (원본 disable)
pub(crate) unsafe fn disable(gl: &impl GlowContextSource, capability: u32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("disable(0x{:X})", capability));
	unsafe {
		gl.disable(capability);
	}
	set_capability(gl, capability, false);
}

fn set_capability(gl: &glow::Context, capability: u32, is_enabled: bool) {
	with_state_cache(gl, |state_cache| {
		if let (Some(state_cache), Some(capability_index)) = (state_cache, GlStateCache::find_capability_index(capability)) {
			state_cache.capability_list[capability_index].set(Some(is_enabled));
		}
	});
}

/// 기능이 켜졌는지. (원본 isEnabled, 기억한 값으로 답한다)
pub(crate) unsafe fn is_enabled(gl: &impl GlowContextSource, capability: u32) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let (Some(state_cache), Some(capability_index)) = (state_cache, GlStateCache::find_capability_index(capability)) else {
			return unsafe { gl.is_enabled(capability) };
		};
		if let Some(is_enabled) = state_cache.capability_list[capability_index].get() {
			return is_enabled;
		}
		let is_enabled = unsafe { gl.is_enabled(capability) };
		state_cache.capability_list[capability_index].set(Some(is_enabled));
		is_enabled
	})
}

//==============================================================================
// 비우기 색과 색 쓰기. (비우기 색은 0 ~ 1 로 자르지 않고 그대로 기억한다)
//==============================================================================
/// 비우기 색. (원본 clearColor)
pub(crate) unsafe fn clear_color(gl: &impl GlowContextSource, red: f32, green: f32, blue: f32, alpha: f32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("clear_color({:?}, {:?}, {:?}, {:?})", red, green, blue, alpha));
	unsafe {
		gl.clear_color(red, green, blue, alpha);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.clear_color.set(Some([red, green, blue, alpha]));
		}
	});
}

/// 색 쓰기 가림. (원본 colorMask)
pub(crate) unsafe fn color_mask(gl: &impl GlowContextSource, red: bool, green: bool, blue: bool, alpha: bool) {
	let gl = gl.read_glow_context();
	note_call(|| format!("color_mask({:?}, {:?}, {:?}, {:?})", red, green, blue, alpha));
	unsafe {
		gl.color_mask(red, green, blue, alpha);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.color_write_mask.set(Some([red, green, blue, alpha]));
		}
	});
}

//==============================================================================
// 섞기.
//==============================================================================
/// 섞기 인자. (원본 blendFunc)
pub(crate) unsafe fn blend_func(gl: &impl GlowContextSource, source_factor: u32, destination_factor: u32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("blend_func(0x{:X}, 0x{:X})", source_factor, destination_factor));
	unsafe {
		gl.blend_func(source_factor, destination_factor);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.blend_factors.set(if is_blend_factor_pair_valid(source_factor, destination_factor) { Some([source_factor, destination_factor, source_factor, destination_factor]) } else { None });
		}
	});
}

/// 섞기 인자 따로. (원본 blendFuncSeparate)
pub(crate) unsafe fn blend_func_separate(gl: &impl GlowContextSource, source_rgb: u32, destination_rgb: u32, source_alpha: u32, destination_alpha: u32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("blend_func_separate(0x{:X}, 0x{:X}, 0x{:X}, 0x{:X})", source_rgb, destination_rgb, source_alpha, destination_alpha));
	unsafe {
		gl.blend_func_separate(source_rgb, destination_rgb, source_alpha, destination_alpha);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			let is_valid = is_blend_factor_pair_valid(source_rgb, destination_rgb) && is_blend_factor_pair_valid(source_alpha, destination_alpha);
			state_cache.blend_factors.set(if is_valid { Some([source_rgb, destination_rgb, source_alpha, destination_alpha]) } else { None });
		}
	});
}

/// 섞기 식. (원본 blendEquation)
pub(crate) unsafe fn blend_equation(gl: &impl GlowContextSource, mode: u32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("blend_equation(0x{:X})", mode));
	unsafe {
		gl.blend_equation(mode);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.blend_equations.set(if BLEND_EQUATION_LIST.contains(&mode) { Some([mode, mode]) } else { None });
		}
	});
}

/// 섞기 식 따로. (원본 blendEquationSeparate)
pub(crate) unsafe fn blend_equation_separate(gl: &impl GlowContextSource, mode_rgb: u32, mode_alpha: u32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("blend_equation_separate(0x{:X}, 0x{:X})", mode_rgb, mode_alpha));
	unsafe {
		gl.blend_equation_separate(mode_rgb, mode_alpha);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			let is_valid = BLEND_EQUATION_LIST.contains(&mode_rgb) && BLEND_EQUATION_LIST.contains(&mode_alpha);
			state_cache.blend_equations.set(if is_valid { Some([mode_rgb, mode_alpha]) } else { None });
		}
	});
}

//==============================================================================
// 잊기. (원본 WebGL2 에 없는 데스크탑 GL 함수가 기억한 값을 바꿀 수 있을 때, 다음에 읽을 때 GL 에 묻는다)
//==============================================================================
/// 기억한 값 한 갈래 잊기.
pub(crate) fn forget(gl: &impl GlowContextSource, part: GlStatePart) {
	let gl = gl.read_glow_context();
	note_call(|| format!("forget({:?})", part));
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return;
		};
		match part {
			GlStatePart::Capability(capability) => {
				if let Some(capability_index) = GlStateCache::find_capability_index(capability) {
					state_cache.capability_list[capability_index].set(None);
				}
			}
			GlStatePart::BlendFactors => state_cache.blend_factors.set(None),
			GlStatePart::BlendEquations => state_cache.blend_equations.set(None),
			GlStatePart::ColorWriteMask => state_cache.color_write_mask.set(None),
			GlStatePart::Viewport => state_cache.viewport.set(None),
			GlStatePart::ScissorBox => state_cache.scissor_box.set(None),
			GlStatePart::TextureUnit(unit) => state_cache.texture_binding_table.borrow_mut().retain(|key, _| key / 4 != unit),
			#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
			GlStatePart::FramebufferBinding => {
				state_cache.draw_framebuffer.set(None);
				state_cache.read_framebuffer.set(None);
			}
			GlStatePart::Framebuffer(framebuffer) => {
				state_cache.deleted_framebuffer_set.borrow_mut().remove(&framebuffer);
			}
			GlStatePart::Program(program) => {
				state_cache.program_link_status_table.borrow_mut().remove(&program);
			}
			GlStatePart::VertexArray(vertex_array) => {
				state_cache.deleted_vertex_array_set.borrow_mut().remove(&vertex_array);
			}
			GlStatePart::Buffer(buffer) => {
				state_cache.deleted_buffer_set.borrow_mut().remove(&buffer);
				state_cache.buffer_element_table.borrow_mut().remove(&buffer);
			}
			GlStatePart::Texture(texture) => {
				state_cache.deleted_texture_set.borrow_mut().remove(&texture);
				state_cache.texture_target_table.borrow_mut().remove(&texture);
			}
		}
	});
}

/// 잊을 값의 갈래.
#[derive(Clone, Copy, Debug)]
pub(crate) enum GlStatePart {
	/// 켜고 끄는 기능 하나.
	Capability(u32),
	/// 섞기 인자.
	BlendFactors,
	/// 섞기 식.
	BlendEquations,
	/// 색 쓰기 가림.
	ColorWriteMask,
	/// 뷰포트.
	Viewport,
	/// 가위 상자.
	ScissorBox,
	/// 한 유닛(0 부터의 번호)의 텍스처 묶음 모두.
	TextureUnit(u32),
	/// 그리기와 읽기 프레임 버퍼 묶음. (웹의 bind_external_framebuffer 뒤)
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	FramebufferBinding,
	/// 프레임 버퍼 하나의 기억. (지웠는지, 새 이름을 받았을 때)
	Framebuffer(GlFramebuffer),
	/// 프로그램 하나의 기억. (링크 결과, 바이너리를 올렸을 때나 새 이름을 받았을 때)
	Program(GlProgram),
	/// 버텍스 어레이 하나의 기억. (지웠는지, 새 이름을 받았을 때)
	VertexArray(GlVertexArray),
	/// 버퍼 하나의 기억. (지웠는지와 처음 묶은 대상, 새 이름을 받았을 때)
	Buffer(GlBuffer),
	/// 텍스처 하나의 기억. (지웠는지와 처음 묶은 대상, 새 이름을 받았을 때)
	Texture(GlTexture),
}

//==============================================================================
// 깊이와 면.
//==============================================================================
/// 깊이 쓰기. (원본 depthMask)
pub(crate) unsafe fn depth_mask(gl: &impl GlowContextSource, value: bool) {
	let gl = gl.read_glow_context();
	note_call(|| format!("depth_mask({:?})", value));
	unsafe {
		gl.depth_mask(value);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.depth_write_mask.set(Some(value));
		}
	});
}

/// 깊이 견주기. (원본 depthFunc, NEVER ~ ALWAYS)
pub(crate) unsafe fn depth_func(gl: &impl GlowContextSource, function: u32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("depth_func(0x{:X})", function));
	unsafe {
		gl.depth_func(function);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.depth_function.set(if (glow::NEVER..=glow::ALWAYS).contains(&function) { Some(function) } else { None });
		}
	});
}

/// 앞면 감김 방향. (원본 frontFace, CW, CCW)
pub(crate) unsafe fn front_face(gl: &impl GlowContextSource, mode: u32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("front_face(0x{:X})", mode));
	unsafe {
		gl.front_face(mode);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.front_face.set(if mode == glow::CW || mode == glow::CCW { Some(mode) } else { None });
		}
	});
}

/// 가리는 면. (원본 cullFace, FRONT, BACK, FRONT_AND_BACK)
pub(crate) unsafe fn cull_face(gl: &impl GlowContextSource, mode: u32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("cull_face(0x{:X})", mode));
	unsafe {
		gl.cull_face(mode);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.cull_face_mode.set(if mode == glow::FRONT || mode == glow::BACK || mode == glow::FRONT_AND_BACK { Some(mode) } else { None });
		}
	});
}

//==============================================================================
// 프로그램과 버텍스 어레이. (링크에 실패한 프로그램을 쓰게 하면 GL 이 무시하므로, 링크 결과를 모를 때는 다음에 읽을 때 묻는다)
//==============================================================================
/// 프로그램 쓰기. (원본 useProgram)
pub(crate) unsafe fn use_program(gl: &impl GlowContextSource, program: Option<GlProgram>) {
	let gl = gl.read_glow_context();
	note_call(|| format!("use_program({:?})", program));
	unsafe {
		gl.use_program(program);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			let is_known = match program {
				Some(program) => state_cache.program_link_status_table.borrow().get(&program) == Some(&true),
				None => true,
			};
			state_cache.program.set(if is_known { Some(program) } else { None });
		}
	});
}

/// 프로그램 만들기. (지운 이름을 GL 이 다시 주면 그 이름의 링크 결과를 잊는다)
pub(crate) unsafe fn create_program(gl: &impl GlowContextSource) -> Result<GlProgram, String> {
	let gl = gl.read_glow_context();
	note_call(|| String::from("create_program()"));
	let program = unsafe { gl.create_program() }?;
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.program_link_status_table.borrow_mut().remove(&program);
		}
	});
	Ok(program)
}

/// 프로그램 링크. (원본 linkProgram, 다시 링크하면 결과를 다시 알아내야 한다)
pub(crate) unsafe fn link_program(gl: &impl GlowContextSource, program: GlProgram) {
	let gl = gl.read_glow_context();
	note_call(|| format!("link_program({:?})", program));
	unsafe {
		gl.link_program(program);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.program_link_status_table.borrow_mut().remove(&program);
		}
	});
}

/// 링크 결과 묻기. (원본 getProgramParameter(LINK_STATUS), 물은 것을 보고 기억한다)
pub(crate) unsafe fn get_program_link_status(gl: &impl GlowContextSource, program: GlProgram) -> bool {
	let gl = gl.read_glow_context();
	let is_linked = unsafe { gl.get_program_link_status(program) };
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			let mut program_link_status_table = state_cache.program_link_status_table.borrow_mut();
			if program_link_status_table.get(&program) != Some(&false) {
				program_link_status_table.insert(program, is_linked);
			}
		}
	});
	is_linked
}

/// 프로그램 지우기. (원본 deleteProgram, 쓰는 중인 것은 지워도 계속 쓰이고, 다시 쓰게 하면 GL 이 무시한다)
pub(crate) unsafe fn delete_program(gl: &impl GlowContextSource, program: GlProgram) {
	let gl = gl.read_glow_context();
	note_call(|| format!("delete_program({:?})", program));
	unsafe {
		gl.delete_program(program);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.program_link_status_table.borrow_mut().insert(program, false);
		}
	});
}

/// 버텍스 어레이 묶기. (원본 bindVertexArray)
pub(crate) unsafe fn bind_vertex_array(gl: &impl GlowContextSource, vertex_array: Option<GlVertexArray>) {
	let gl = gl.read_glow_context();
	note_call(|| format!("bind_vertex_array({:?})", vertex_array));
	unsafe {
		gl.bind_vertex_array(vertex_array);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			let is_known = match vertex_array {
				Some(vertex_array) => !state_cache.deleted_vertex_array_set.borrow().contains(&vertex_array),
				None => true,
			};
			state_cache.vertex_array.set(if is_known { Some(vertex_array) } else { None });
		}
	});
}

/// 버텍스 어레이 만들기. (지운 이름을 GL 이 다시 주면 지운 기억에서 뺀다)
pub(crate) unsafe fn create_vertex_array(gl: &impl GlowContextSource) -> Result<GlVertexArray, String> {
	let gl = gl.read_glow_context();
	note_call(|| String::from("create_vertex_array()"));
	let vertex_array = unsafe { gl.create_vertex_array() }?;
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.deleted_vertex_array_set.borrow_mut().remove(&vertex_array);
		}
	});
	Ok(vertex_array)
}

/// 버텍스 어레이 지우기. (원본 deleteVertexArray, 묶인 것을 지우면 GL 이 기본으로 되돌린다)
pub(crate) unsafe fn delete_vertex_array(gl: &impl GlowContextSource, vertex_array: GlVertexArray) {
	let gl = gl.read_glow_context();
	note_call(|| format!("delete_vertex_array({:?})", vertex_array));
	unsafe {
		gl.delete_vertex_array(vertex_array);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.deleted_vertex_array_set.borrow_mut().insert(vertex_array);
			if state_cache.vertex_array.get() == Some(Some(vertex_array)) {
				state_cache.vertex_array.set(Some(None));
			}
		}
	});
}

//==============================================================================
// 버퍼. (ARRAY_BUFFER 묶음만 기억한다, 버퍼는 ELEMENT_ARRAY_BUFFER 와 그 밖의 대상 사이를 오가며 묶지 못한다, 어기면 WebGL 이 무시한다)
//==============================================================================
/// 버퍼 묶기. (원본 bindBuffer)
pub(crate) unsafe fn bind_buffer(gl: &impl GlowContextSource, target: u32, buffer: Option<GlBuffer>) {
	let gl = gl.read_glow_context();
	note_call(|| format!("bind_buffer(0x{:X}, {:?})", target, buffer));
	unsafe {
		gl.bind_buffer(target, buffer);
	}
	if !BUFFER_TARGET_LIST.contains(&target) {
		return;
	}
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return;
		};
		let is_element_target = target == glow::ELEMENT_ARRAY_BUFFER;
		let is_buffer_object = buffer.is_some_and(|buffer| !state_cache.deleted_buffer_set.borrow().contains(&buffer));
		if let (true, Some(buffer)) = (is_buffer_object, buffer) {
			state_cache.buffer_element_table.borrow_mut().entry(buffer).or_insert(is_element_target);
		}
		if target != glow::ARRAY_BUFFER {
			return;
		}
		let Some(buffer) = buffer else {
			state_cache.array_buffer.set(Some(None));
			return;
		};
		let is_known = is_buffer_object && state_cache.buffer_element_table.borrow().get(&buffer) == Some(&false);
		state_cache.array_buffer.set(if is_known { Some(Some(buffer)) } else { None });
	});
}

/// 버퍼 만들기. (지운 이름을 GL 이 다시 주면 지운 기억과 처음 묶은 대상을 뺀다)
pub(crate) unsafe fn create_buffer(gl: &impl GlowContextSource) -> Result<GlBuffer, String> {
	let gl = gl.read_glow_context();
	note_call(|| String::from("create_buffer()"));
	let buffer = unsafe { gl.create_buffer() }?;
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.deleted_buffer_set.borrow_mut().remove(&buffer);
			state_cache.buffer_element_table.borrow_mut().remove(&buffer);
		}
	});
	Ok(buffer)
}

/// 버퍼 지우기. (원본 deleteBuffer, ARRAY_BUFFER 에 묶인 것을 지우면 GL 이 풀어 준다)
pub(crate) unsafe fn delete_buffer(gl: &impl GlowContextSource, buffer: GlBuffer) {
	let gl = gl.read_glow_context();
	note_call(|| format!("delete_buffer({:?})", buffer));
	unsafe {
		gl.delete_buffer(buffer);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.deleted_buffer_set.borrow_mut().insert(buffer);
			if state_cache.array_buffer.get() == Some(Some(buffer)) {
				state_cache.array_buffer.set(Some(None));
			}
		}
	});
}

//==============================================================================
// 텍스처.
//==============================================================================
// 시험이 GL 호출을 세는 값. (active_texture 부른 수, bind_texture 부른 수)
#[cfg(test)]
thread_local! {
	static TEXTURE_CALL_COUNT: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
}

/// 지금까지 부른 active_texture 와 bind_texture 수. (시험용)
#[cfg(test)]
pub(crate) fn read_texture_call_count() -> (u32, u32) {
	TEXTURE_CALL_COUNT.with(|count| count.get())
}

/// 활성 텍스처 유닛. (원본 activeTexture, TEXTURE0 + 번호)
pub(crate) unsafe fn active_texture(gl: &impl GlowContextSource, texture_unit: u32) {
	let gl = gl.read_glow_context();
	#[cfg(test)]
	TEXTURE_CALL_COUNT.with(|count| count.set((count.get().0 + 1, count.get().1)));
	note_call(|| format!("active_texture(0x{:X})", texture_unit));
	unsafe {
		gl.active_texture(texture_unit);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			let maximum_texture_unit_count = state_cache.read_constant_parameter(gl, glow::MAX_COMBINED_TEXTURE_IMAGE_UNITS).max(0) as u32;
			let is_valid = texture_unit >= glow::TEXTURE0 && texture_unit < glow::TEXTURE0 + maximum_texture_unit_count;
			state_cache.active_texture.set(if is_valid { Some(texture_unit) } else { None });
		}
	});
}

/// 텍스처 묶기. (원본 bindTexture, 텍스처는 처음 묶은 대상에만 묶인다)
pub(crate) unsafe fn bind_texture(gl: &impl GlowContextSource, target: u32, texture: Option<GlTexture>) {
	let gl = gl.read_glow_context();
	#[cfg(test)]
	TEXTURE_CALL_COUNT.with(|count| count.set((count.get().0, count.get().1 + 1)));
	note_call(|| format!("bind_texture(0x{:X}, {:?})", target, texture));
	unsafe {
		gl.bind_texture(target, texture);
	}
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return;
		};
		let Some(texture_target_index) = TEXTURE_TARGET_LIST.iter().position(|&value| value == target) else {
			return;
		};
		let Some(active_texture) = state_cache.active_texture.get() else {
			// 유닛을 모르면 그 대상의 기억을 모두 잊는다.
			state_cache.forget_texture_target(texture_target_index);
			return;
		};
		let key = (active_texture - glow::TEXTURE0) * 4 + texture_target_index as u32;
		let Some(texture) = texture else {
			state_cache.texture_binding_table.borrow_mut().insert(key, None);
			return;
		};
		if state_cache.deleted_texture_set.borrow().contains(&texture) {
			state_cache.texture_binding_table.borrow_mut().remove(&key);
			return;
		}
		let mut texture_target_table = state_cache.texture_target_table.borrow_mut();
		match texture_target_table.get(&texture) {
			None => {
				texture_target_table.insert(texture, texture_target_index);
			}
			Some(&first_target_index) => {
				if first_target_index != texture_target_index {
					return;
				}
			}
		}
		state_cache.texture_binding_table.borrow_mut().insert(key, Some(texture));
	});
}

/// 텍스처 만들기. (지운 이름을 GL 이 다시 주면 지운 기억과 처음 묶은 대상을 뺀다)
pub(crate) unsafe fn create_texture(gl: &impl GlowContextSource) -> Result<GlTexture, String> {
	let gl = gl.read_glow_context();
	note_call(|| String::from("create_texture()"));
	let texture = unsafe { gl.create_texture() }?;
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.deleted_texture_set.borrow_mut().remove(&texture);
			state_cache.texture_target_table.borrow_mut().remove(&texture);
		}
	});
	Ok(texture)
}

/// 텍스처 지우기. (원본 deleteTexture, 묶인 것을 지우면 GL 이 그 유닛에서 풀어 준다)
pub(crate) unsafe fn delete_texture(gl: &impl GlowContextSource, texture: GlTexture) {
	let gl = gl.read_glow_context();
	note_call(|| format!("delete_texture({:?})", texture));
	unsafe {
		gl.delete_texture(texture);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			state_cache.deleted_texture_set.borrow_mut().insert(texture);
			for bound_texture in state_cache.texture_binding_table.borrow_mut().values_mut() {
				if *bound_texture == Some(texture) {
					*bound_texture = None;
				}
			}
		}
	});
}

//==============================================================================
// 픽셀 올리기와 읽기 설정.
//==============================================================================
/// 픽셀 저장 설정. (원본 pixelStorei)
pub(crate) unsafe fn pixel_store_i32(gl: &impl GlowContextSource, parameter: u32, value: i32) {
	let gl = gl.read_glow_context();
	note_call(|| format!("pixel_store_i32(0x{:X}, {:?})", parameter, value));
	unsafe {
		gl.pixel_store_i32(parameter, value);
	}
	with_state_cache(gl, |state_cache| {
		if let Some(state_cache) = state_cache {
			if parameter == glow::PACK_ALIGNMENT || parameter == glow::UNPACK_ALIGNMENT {
				let mut pixel_store_table = state_cache.pixel_store_table.borrow_mut();
				if matches!(value, 1 | 2 | 4 | 8) {
					pixel_store_table.insert(parameter, value);
				}
				else {
					pixel_store_table.remove(&parameter);
				}
			}
		}
	});
}

//==============================================================================
// 읽기. (기억한 값으로 답하고 모르면 GL 에 한 번 묻고 기억한다)
//==============================================================================
/// 정수 하나 읽기. (원본 getParameter)
pub(crate) unsafe fn get_parameter_i32(gl: &impl GlowContextSource, parameter: u32) -> i32 {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let query = || unsafe { gl.get_parameter_i32(parameter) };
		let Some(state_cache) = state_cache else {
			return query();
		};
		match parameter {
			glow::BLEND_SRC_RGB | glow::BLEND_DST_RGB | glow::BLEND_SRC_ALPHA | glow::BLEND_DST_ALPHA => {
				let blend_factors = state_cache.blend_factors.get().unwrap_or_else(|| {
					let blend_factors = unsafe { [gl.get_parameter_i32(glow::BLEND_SRC_RGB) as u32, gl.get_parameter_i32(glow::BLEND_DST_RGB) as u32, gl.get_parameter_i32(glow::BLEND_SRC_ALPHA) as u32, gl.get_parameter_i32(glow::BLEND_DST_ALPHA) as u32] };
					state_cache.blend_factors.set(Some(blend_factors));
					blend_factors
				});
				let index = match parameter {
					glow::BLEND_SRC_RGB => 0,
					glow::BLEND_DST_RGB => 1,
					glow::BLEND_SRC_ALPHA => 2,
					_ => 3,
				};
				blend_factors[index] as i32
			}
			glow::BLEND_EQUATION_RGB | glow::BLEND_EQUATION_ALPHA => {
				let blend_equations = state_cache.blend_equations.get().unwrap_or_else(|| {
					let blend_equations = unsafe { [gl.get_parameter_i32(glow::BLEND_EQUATION_RGB) as u32, gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA) as u32] };
					state_cache.blend_equations.set(Some(blend_equations));
					blend_equations
				});
				blend_equations[if parameter == glow::BLEND_EQUATION_RGB { 0 } else { 1 }] as i32
			}
			glow::DEPTH_FUNC => remember_u32(&state_cache.depth_function, query) as i32,
			glow::FRONT_FACE => remember_u32(&state_cache.front_face, query) as i32,
			glow::CULL_FACE_MODE => remember_u32(&state_cache.cull_face_mode, query) as i32,
			glow::ACTIVE_TEXTURE => state_cache.read_active_texture(gl) as i32,
			glow::PACK_ALIGNMENT | glow::UNPACK_ALIGNMENT => {
				if let Some(value) = state_cache.pixel_store_table.borrow().get(&parameter) {
					return *value;
				}
				let value = query();
				state_cache.pixel_store_table.borrow_mut().insert(parameter, value);
				value
			}
			_ if CONSTANT_PARAMETER_LIST.contains(&parameter) => state_cache.read_constant_parameter(gl, parameter),
			_ => query(),
		}
	})
}

fn remember_u32(cell: &Cell<Option<u32>>, query: impl FnOnce() -> i32) -> u32 {
	if let Some(value) = cell.get() {
		return value;
	}
	let value = query() as u32;
	cell.set(Some(value));
	value
}

/// 참 거짓 하나 읽기. (원본 getParameter)
pub(crate) unsafe fn get_parameter_bool(gl: &impl GlowContextSource, parameter: u32) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| match (state_cache, parameter) {
		(Some(state_cache), glow::DEPTH_WRITEMASK) => {
			if let Some(value) = state_cache.depth_write_mask.get() {
				return value;
			}
			let value = unsafe { gl.get_parameter_bool(parameter) };
			state_cache.depth_write_mask.set(Some(value));
			value
		}
		_ => unsafe { gl.get_parameter_bool(parameter) },
	})
}

/// 정수 여럿 읽기. (원본 getParameter, VIEWPORT, SCISSOR_BOX, MAX_VIEWPORT_DIMS)
pub(crate) unsafe fn get_parameter_i32_slice(gl: &impl GlowContextSource, parameter: u32, out: &mut [i32]) {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let cell = match (state_cache, parameter) {
			(Some(state_cache), glow::VIEWPORT) if out.len() >= 4 => &state_cache.viewport,
			(Some(state_cache), glow::SCISSOR_BOX) if out.len() >= 4 => &state_cache.scissor_box,
			(Some(state_cache), glow::MAX_VIEWPORT_DIMS) if out.len() >= 2 => {
				out[..2].copy_from_slice(&state_cache.read_maximum_viewport_size(gl));
				return;
			}
			_ => {
				unsafe {
					gl.get_parameter_i32_slice(parameter, out);
				}
				return;
			}
		};
		let value = cell.get().unwrap_or_else(|| {
			let mut value = [0i32; 4];
			unsafe {
				gl.get_parameter_i32_slice(parameter, &mut value);
			}
			cell.set(Some(value));
			value
		});
		out[..4].copy_from_slice(&value);
	});
}

/// 실수 여럿 읽기. (원본 getParameter, COLOR_CLEAR_VALUE)
pub(crate) unsafe fn get_parameter_f32_slice(gl: &impl GlowContextSource, parameter: u32, out: &mut [f32]) {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache.filter(|_| parameter == glow::COLOR_CLEAR_VALUE && out.len() >= 4) else {
			unsafe {
				gl.get_parameter_f32_slice(parameter, out);
			}
			return;
		};
		let value = state_cache.clear_color.get().unwrap_or_else(|| {
			let mut value = [0f32; 4];
			unsafe {
				gl.get_parameter_f32_slice(parameter, &mut value);
			}
			state_cache.clear_color.set(Some(value));
			value
		});
		out[..4].copy_from_slice(&value);
	});
}

/// 참 거짓 넷 읽기. (원본 getParameter, COLOR_WRITEMASK)
pub(crate) unsafe fn get_parameter_bool_array<const N: usize>(gl: &impl GlowContextSource, parameter: u32) -> [bool; N] {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache.filter(|_| parameter == glow::COLOR_WRITEMASK && N == 4) else {
			return unsafe { gl.get_parameter_bool_array::<N>(parameter) };
		};
		let value = state_cache.color_write_mask.get().unwrap_or_else(|| {
			let value = unsafe { gl.get_parameter_bool_array::<4>(parameter) };
			state_cache.color_write_mask.set(Some(value));
			value
		});
		let mut out = [false; N];
		out.copy_from_slice(&value[..N]);
		out
	})
}

/// 그리는 프레임 버퍼 읽기. (원본 getDrawFramebuffer, 기본 프레임 버퍼면 None, query 는 모를 때 GL 에 묻는 길)
pub(crate) fn read_draw_framebuffer(gl: &impl GlowContextSource, query: impl FnOnce() -> Option<GlFramebuffer>) -> Option<GlFramebuffer> {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return query();
		};
		if let Some(framebuffer) = state_cache.draw_framebuffer.get() {
			return framebuffer;
		}
		let framebuffer = query();
		state_cache.draw_framebuffer.set(Some(framebuffer));
		framebuffer
	})
}

/// 읽는 프레임 버퍼 읽기. (원본 getParameter(READ_FRAMEBUFFER_BINDING), 기본 프레임 버퍼면 None)
pub(crate) fn read_reading_framebuffer(gl: &impl GlowContextSource, query: impl FnOnce() -> Option<GlFramebuffer>) -> Option<GlFramebuffer> {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return query();
		};
		if let Some(framebuffer) = state_cache.read_framebuffer.get() {
			return framebuffer;
		}
		let framebuffer = query();
		state_cache.read_framebuffer.set(Some(framebuffer));
		framebuffer
	})
}

/// 쓰는 프로그램 읽기. (원본 getParameter(CURRENT_PROGRAM))
pub(crate) fn read_program(gl: &impl GlowContextSource, query: impl FnOnce() -> Option<GlProgram>) -> Option<GlProgram> {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return query();
		};
		if let Some(program) = state_cache.program.get() {
			return program;
		}
		let program = query();
		state_cache.program.set(Some(program));
		program
	})
}

/// 묶인 버텍스 어레이 읽기. (원본 getParameter(VERTEX_ARRAY_BINDING))
pub(crate) fn read_vertex_array(gl: &impl GlowContextSource, query: impl FnOnce() -> Option<GlVertexArray>) -> Option<GlVertexArray> {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return query();
		};
		if let Some(vertex_array) = state_cache.vertex_array.get() {
			return vertex_array;
		}
		let vertex_array = query();
		state_cache.vertex_array.set(Some(vertex_array));
		vertex_array
	})
}

/// 활성 유닛에 묶인 2D 텍스처 읽기. (원본 getParameter(TEXTURE_BINDING_2D))
pub(crate) fn read_texture_binding_2d(gl: &impl GlowContextSource, query: impl FnOnce() -> Option<GlTexture>) -> Option<GlTexture> {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| {
		let Some(state_cache) = state_cache else {
			return query();
		};
		let key = (state_cache.read_active_texture(gl) - glow::TEXTURE0) * 4;
		if let Some(texture) = state_cache.texture_binding_table.borrow().get(&key) {
			return *texture;
		}
		let texture = query();
		state_cache.texture_binding_table.borrow_mut().insert(key, texture);
		texture
	})
}

//==============================================================================
// 견주기. (기억한 값이 같을 때만 참, 모르면 거짓, 부르는 쪽이 같은 값 호출을 건너뛸 때)
//==============================================================================
/// 그 프로그램을 쓰는 중인지. (원본 isProgramInUse)
pub(crate) fn is_program_in_use(gl: &impl GlowContextSource, program: Option<GlProgram>) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.program.get() == Some(program)))
}

/// 그 버텍스 어레이가 묶였는지. (원본 isVertexArrayBound)
pub(crate) fn is_vertex_array_bound(gl: &impl GlowContextSource, vertex_array: Option<GlVertexArray>) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.vertex_array.get() == Some(vertex_array)))
}

/// ARRAY_BUFFER 에 그 버퍼가 묶였는지. (원본 isArrayBufferBound)
pub(crate) fn is_array_buffer_bound(gl: &impl GlowContextSource, buffer: Option<GlBuffer>) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.array_buffer.get() == Some(buffer)))
}

/// 기능이 그렇게 켜졌거나 꺼졌는지. (원본 isCapabilitySet, GL 에 묻지 않는다)
pub(crate) fn is_capability_set(gl: &impl GlowContextSource, capability: u32, is_enabled: bool) -> bool {
	let gl = gl.read_glow_context();
	let Some(capability_index) = GlStateCache::find_capability_index(capability) else {
		return false;
	};
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.capability_list[capability_index].get() == Some(is_enabled)))
}

/// 섞기 인자가 그것인지. (원본 isBlendFunction, blend_func 로 건 꼴, RGB 와 알파가 같은 인자)
pub(crate) fn is_blend_function(gl: &impl GlowContextSource, source_factor: u32, destination_factor: u32) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.blend_factors.get() == Some([source_factor, destination_factor, source_factor, destination_factor])))
}

/// 섞기 식이 그것인지. (원본 isBlendEquation, RGB 와 알파가 같은 식)
pub(crate) fn is_blend_equation(gl: &impl GlowContextSource, mode: u32) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.blend_equations.get() == Some([mode, mode])))
}

/// 뷰포트가 그것인지. (원본 isViewport, GL 에 묻지 않는다)
pub(crate) fn is_viewport(gl: &impl GlowContextSource, x: i32, y: i32, width: i32, height: i32) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.viewport.get() == Some([x, y, width, height])))
}

/// 색 쓰기 가림이 그것인지. (GL 에 묻지 않는다, 데스크탑 기본 프레임버퍼에 알파를 쓰지 않는 Graphic 의 색 쓰기 마스크가 견준다)
pub(crate) fn is_color_mask(gl: &impl GlowContextSource, red: bool, green: bool, blue: bool, alpha: bool) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.color_write_mask.get() == Some([red, green, blue, alpha])))
}

/// 활성 텍스처 유닛이 그것인지. (원본 isActiveTexture, TEXTURE0 + 번호)
pub(crate) fn is_active_texture(gl: &impl GlowContextSource, texture_unit: u32) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.active_texture.get() == Some(texture_unit)))
}

/// 그 유닛과 대상에 그 텍스처가 묶였는지. (원본 isTextureBound)
pub(crate) fn is_texture_bound(gl: &impl GlowContextSource, texture_unit: u32, target: u32, texture: Option<GlTexture>) -> bool {
	let gl = gl.read_glow_context();
	let Some(texture_target_index) = TEXTURE_TARGET_LIST.iter().position(|&value| value == target) else {
		return false;
	};
	if texture_unit < glow::TEXTURE0 {
		return false;
	}
	let key = (texture_unit - glow::TEXTURE0) * 4 + texture_target_index as u32;
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.texture_binding_table.borrow().get(&key) == Some(&texture)))
}

/// 앞면 감김이 그것인지. (원본 isFrontFace)
pub(crate) fn is_front_face(gl: &impl GlowContextSource, mode: u32) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.front_face.get() == Some(mode)))
}

/// 가리는 면이 그것인지. (원본 isCullFace)
pub(crate) fn is_cull_face(gl: &impl GlowContextSource, mode: u32) -> bool {
	let gl = gl.read_glow_context();
	with_state_cache(gl, |state_cache| state_cache.is_some_and(|state_cache| state_cache.cull_face_mode.get() == Some(mode)))
}

//==============================================================================
// 검사 모드. (기본 꺼짐, 환경 변수 VANILLA_GL_STATE_CHECK=1 로 켠다)
// - 그리기 호출(이 모듈의 draw_* 를 거친 것) 바로 앞마다 기억한 값을 진짜 GL 과 견주어, 처음 어긋난 곳과 그 앞의 상태 호출 차례를 표준 오류에 남긴다.
// - 견주는 읽기는 상태를 바꾸지 않는다. (유닛마다 텍스처를 읽으려 활성 유닛을 잠깐 바꾸고 진짜 GL 이 가졌던 유닛으로 되돌린다)
// - 어긋난 갈래는 잊는다. (다음에 읽을 때 GL 에 묻는다, 뒤에 따로 생기는 어긋남도 잡으려고)
//==============================================================================
/// 남겨 두는 앞 상태 호출 수.
const CHECK_CALL_LOG_LENGTH: usize = 48;

/// 어긋남을 자세히 찍는 수. (그 뒤는 세기만 한다)
const CHECK_REPORT_LIMIT: usize = 8;

/// 텍스처 대상마다 묶음을 묻는 값. (TEXTURE_TARGET_LIST 와 같은 차례)
const TEXTURE_BINDING_PARAMETER_LIST: [u32; 4] = [glow::TEXTURE_BINDING_2D, glow::TEXTURE_BINDING_CUBE_MAP, glow::TEXTURE_BINDING_3D, glow::TEXTURE_BINDING_2D_ARRAY];

/// 검사 모드의 기록.
struct StateCheck {
	is_enabled: Option<bool>, // None 이면 아직 환경 변수를 읽지 않았다.
	call_log: std::collections::VecDeque<String>,
	draw_count: u64,
	mismatch_count: usize,
}

thread_local! {
	static STATE_CHECK: RefCell<StateCheck> = const { RefCell::new(StateCheck { is_enabled: None, call_log: std::collections::VecDeque::new(), draw_count: 0, mismatch_count: 0 }) };
}

/// 검사 모드가 켜졌는지. (처음 물을 때 환경 변수를 읽는다)
pub(crate) fn is_check_enabled() -> bool {
	STATE_CHECK.with(|state_check| {
		let mut state_check = state_check.borrow_mut();
		*state_check.is_enabled.get_or_insert_with(|| std::env::var("VANILLA_GL_STATE_CHECK").is_ok_and(|value| value == "1"))
	})
}

/// 검사 모드 켜고 끄기. (시험이 환경 변수 없이 켤 때, 기록도 비운다)
#[cfg(test)]
pub(crate) fn set_check_enabled(is_enabled: bool) {
	STATE_CHECK.with(|state_check| {
		let mut state_check = state_check.borrow_mut();
		state_check.is_enabled = Some(is_enabled);
		state_check.call_log.clear();
		state_check.draw_count = 0;
		state_check.mismatch_count = 0;
	});
}

/// 검사 모드가 지금까지 찾은 어긋남 수와 견준 그리기 수.
pub(crate) fn get_check_count() -> (usize, u64) {
	STATE_CHECK.with(|state_check| {
		let state_check = state_check.borrow();
		(state_check.mismatch_count, state_check.draw_count)
	})
}

/// 상태 호출 하나 적기. (검사 모드일 때만 글을 짓는다)
fn note_call(text: impl FnOnce() -> String) {
	if !is_check_enabled() {
		return;
	}
	STATE_CHECK.with(|state_check| {
		let mut state_check = state_check.borrow_mut();
		if state_check.call_log.len() >= CHECK_CALL_LOG_LENGTH {
			state_check.call_log.pop_front();
		}
		state_check.call_log.push_back(text());
	});
}

/// 견준 갈래 하나. (이름, 기억한 값, 진짜 값)
type CheckMismatch = (String, String, String);

/// 기억한 값과 진짜 GL 견주기. (어긋난 갈래를 모아 돌려주고, 그 갈래를 잊는다)
fn collect_check_mismatch_list(gl: &glow::Context, state_cache: &GlStateCache) -> Vec<CheckMismatch> {
	let mut mismatch_list: Vec<CheckMismatch> = Vec::new();
	unsafe {
		macro_rules! compare_cell {
			($name:expr, $cell:expr, $real:expr) => {
				if let Some(remembered) = $cell.get() {
					let real = $real;
					if remembered != real {
						mismatch_list.push((String::from($name), format!("{:?}", remembered), format!("{:?}", real)));
						$cell.set(None);
					}
				}
			};
		}
		compare_cell!("DRAW_FRAMEBUFFER_BINDING", state_cache.draw_framebuffer, gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING));
		compare_cell!("READ_FRAMEBUFFER_BINDING", state_cache.read_framebuffer, gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING));
		compare_cell!("VIEWPORT", state_cache.viewport, {
			let mut value = [0i32; 4];
			gl.get_parameter_i32_slice(glow::VIEWPORT, &mut value);
			value
		});
		compare_cell!("SCISSOR_BOX", state_cache.scissor_box, {
			let mut value = [0i32; 4];
			gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut value);
			value
		});
		compare_cell!("COLOR_CLEAR_VALUE", state_cache.clear_color, {
			let mut value = [0f32; 4];
			gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut value);
			value
		});
		compare_cell!("COLOR_WRITEMASK", state_cache.color_write_mask, gl.get_parameter_bool_array::<4>(glow::COLOR_WRITEMASK));
		compare_cell!("BLEND_FUNC", state_cache.blend_factors, [gl.get_parameter_i32(glow::BLEND_SRC_RGB) as u32, gl.get_parameter_i32(glow::BLEND_DST_RGB) as u32, gl.get_parameter_i32(glow::BLEND_SRC_ALPHA) as u32, gl.get_parameter_i32(glow::BLEND_DST_ALPHA) as u32]);
		compare_cell!("BLEND_EQUATION", state_cache.blend_equations, [gl.get_parameter_i32(glow::BLEND_EQUATION_RGB) as u32, gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA) as u32]);
		compare_cell!("DEPTH_WRITEMASK", state_cache.depth_write_mask, gl.get_parameter_bool(glow::DEPTH_WRITEMASK));
		compare_cell!("DEPTH_FUNC", state_cache.depth_function, gl.get_parameter_i32(glow::DEPTH_FUNC) as u32);
		compare_cell!("FRONT_FACE", state_cache.front_face, gl.get_parameter_i32(glow::FRONT_FACE) as u32);
		compare_cell!("CULL_FACE_MODE", state_cache.cull_face_mode, gl.get_parameter_i32(glow::CULL_FACE_MODE) as u32);
		compare_cell!("CURRENT_PROGRAM", state_cache.program, gl.get_parameter_program(glow::CURRENT_PROGRAM));
		compare_cell!("VERTEX_ARRAY_BINDING", state_cache.vertex_array, gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING));
		compare_cell!("ARRAY_BUFFER_BINDING", state_cache.array_buffer, gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING));
		let real_active_texture = gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
		compare_cell!("ACTIVE_TEXTURE", state_cache.active_texture, real_active_texture);
		for (capability_index, capability) in CAPABILITY_LIST.iter().enumerate() {
			compare_cell!(format!("isEnabled(0x{:X})", capability), state_cache.capability_list[capability_index], gl.is_enabled(*capability));
		}
		for parameter in [glow::PACK_ALIGNMENT, glow::UNPACK_ALIGNMENT] {
			let remembered = state_cache.pixel_store_table.borrow().get(&parameter).copied();
			if let Some(remembered) = remembered {
				let real = gl.get_parameter_i32(parameter);
				if remembered != real {
					mismatch_list.push((format!("pixelStore(0x{:X})", parameter), format!("{:?}", remembered), format!("{:?}", real)));
					state_cache.pixel_store_table.borrow_mut().remove(&parameter);
				}
			}
		}
		// 유닛마다 텍스처. (활성 유닛을 잠깐 바꿔 읽고 진짜 GL 이 가졌던 유닛으로 되돌린다)
		let mut texture_binding_list: Vec<(u32, Option<GlTexture>)> = state_cache.texture_binding_table.borrow().iter().map(|(key, texture)| (*key, *texture)).collect();
		texture_binding_list.sort_by_key(|(key, _)| *key);
		let mut current_texture_unit = real_active_texture;
		for (key, remembered) in texture_binding_list {
			let texture_unit = glow::TEXTURE0 + key / 4;
			if texture_unit != current_texture_unit {
				gl.active_texture(texture_unit);
				current_texture_unit = texture_unit;
			}
			let real = gl.get_parameter_texture(TEXTURE_BINDING_PARAMETER_LIST[key as usize % 4]);
			if remembered != real {
				mismatch_list.push((format!("unit {} {:?}", key / 4, ["TEXTURE_2D", "TEXTURE_CUBE_MAP", "TEXTURE_3D", "TEXTURE_2D_ARRAY"][key as usize % 4]), format!("{:?}", remembered), format!("{:?}", real)));
				state_cache.texture_binding_table.borrow_mut().remove(&key);
			}
		}
		if current_texture_unit != real_active_texture {
			gl.active_texture(real_active_texture);
		}
	}
	mismatch_list
}

/// 그리기 앞 견주기. (검사 모드일 때만, 어긋나면 표준 오류에 그 갈래와 앞의 상태 호출 차례를 남긴다)
pub(crate) fn check_before_draw(gl: &impl GlowContextSource, draw_name: &str) {
	let gl = gl.read_glow_context();
	if !is_check_enabled() {
		return;
	}
	let mismatch_list = with_state_cache(gl, |state_cache| match state_cache {
		Some(state_cache) => {
			collect_check_mismatch_list(gl, state_cache)
		}
		None => {
			Vec::new()
		}
	});
	STATE_CHECK.with(|state_check| {
		let mut state_check = state_check.borrow_mut();
		state_check.draw_count += 1;
		let draw_count = state_check.draw_count;
		state_check.call_log.push_back(format!("{} (그리기 {} 번째)", draw_name, draw_count));
		if state_check.call_log.len() > CHECK_CALL_LOG_LENGTH {
			state_check.call_log.pop_front();
		}
		if mismatch_list.is_empty() {
			return;
		}
		state_check.mismatch_count += 1;
		if state_check.mismatch_count > CHECK_REPORT_LIMIT {
			return;
		}
		let mut report = format!("glstatecache 검사: 그리기 {} 번째({}) 앞에서 어긋남 {} 번째\n", draw_count, draw_name, state_check.mismatch_count);
		for (name, remembered, real) in &mismatch_list {
			report.push_str(&format!("  {} 기억 {} 진짜 {}\n", name, remembered, real));
		}
		report.push_str("  앞의 상태 호출 (오래된 것부터):\n");
		for call in state_check.call_log.iter().take(state_check.call_log.len().saturating_sub(1)) {
			report.push_str(&format!("    {}\n", call));
		}
		eprint!("{}", report);
	});
}

//==============================================================================
// 그리기. (검사 모드일 때 그리기 앞에 견준다, 꺼져 있으면 GL 을 그대로 부른다)
//==============================================================================
/// 배열 그리기. (glow draw_arrays)
pub(crate) unsafe fn draw_arrays(gl: &impl GlowContextSource, mode: u32, first: i32, count: i32) {
	let gl = gl.read_glow_context();
	check_before_draw(gl, "draw_arrays");
	unsafe {
		gl.draw_arrays(mode, first, count);
	}
}

/// 배열 여럿 그리기. (glow draw_arrays_instanced)
pub(crate) unsafe fn draw_arrays_instanced(gl: &impl GlowContextSource, mode: u32, first: i32, count: i32, instance_count: i32) {
	let gl = gl.read_glow_context();
	check_before_draw(gl, "draw_arrays_instanced");
	unsafe {
		gl.draw_arrays_instanced(mode, first, count, instance_count);
	}
}

/// 인덱스 그리기. (glow draw_elements)
pub(crate) unsafe fn draw_elements(gl: &impl GlowContextSource, mode: u32, count: i32, element_type: u32, offset: i32) {
	let gl = gl.read_glow_context();
	check_before_draw(gl, "draw_elements");
	unsafe {
		gl.draw_elements(mode, count, element_type, offset);
	}
}

/// 인덱스 여럿 그리기. (glow draw_elements_instanced)
pub(crate) unsafe fn draw_elements_instanced(gl: &impl GlowContextSource, mode: u32, count: i32, element_type: u32, offset: i32, instance_count: i32) {
	let gl = gl.read_glow_context();
	check_before_draw(gl, "draw_elements_instanced");
	unsafe {
		gl.draw_elements_instanced(mode, count, element_type, offset, instance_count);
	}
}

//==============================================================================
// 시험. (원본 webglstatecachechrome.test.mjs 와 같은 꼴, 진짜 GL 컨텍스트를 맥의 CGL 로 창 없이 짓는다)
//==============================================================================
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod tests {
	use super::*;
	use crate::core::glcontext::GlContext;
	use std::ffi::{c_char, c_void};

	#[link(name = "OpenGL", kind = "framework")]
	extern "C" {
		fn CGLChoosePixelFormat(attribute_list: *const i32, pixel_format: *mut *mut c_void, pixel_format_count: *mut i32) -> i32;
		fn CGLCreateContext(pixel_format: *mut c_void, share_context: *mut c_void, context: *mut *mut c_void) -> i32;
		fn CGLDestroyPixelFormat(pixel_format: *mut c_void) -> i32;
		fn CGLSetCurrentContext(context: *mut c_void) -> i32;
	}

	extern "C" {
		fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
	}

	const VERTEX_SHADER_SOURCE: &str = "#version 300 es\nin vec2 vertexPosition;\nvoid main() {\n\tgl_Position = vec4(vertexPosition, 0.0, 1.0);\n}\n";
	const FRAGMENT_SHADER_SOURCE: &str = "#version 300 es\nprecision highp float;\nout vec4 outputColor;\nvoid main() {\n\toutputColor = vec4(1.0);\n}\n";

	/// 창 없는 GL 3.2 이상 코어 컨텍스트. (CGL 을 바로 불러 AppKit 과 주 스레드 없이 짓는다, 못 지으면 None, 시험을 건너뛴다)
	pub(crate) fn create_surfaceless_context() -> Option<Rc<glow::Context>> {
		// kCGLPFAOpenGLProfile, kCGLOGLPVersion_3_2_Core (맥은 4.1 코어를 준다), kCGLPFAAccelerated.
		let attribute_list: [i32; 4] = [99, 0x3200, 73, 0];
		let mut pixel_format: *mut c_void = std::ptr::null_mut();
		let mut pixel_format_count = 0i32;
		let mut context: *mut c_void = std::ptr::null_mut();
		unsafe {
			if CGLChoosePixelFormat(attribute_list.as_ptr(), &mut pixel_format, &mut pixel_format_count) != 0 || pixel_format.is_null() {
				return None;
			}
			let result = CGLCreateContext(pixel_format, std::ptr::null_mut(), &mut context);
			CGLDestroyPixelFormat(pixel_format);
			if result != 0 || context.is_null() || CGLSetCurrentContext(context) != 0 {
				return None;
			}
			// RTLD_DEFAULT 는 맥에서 -2.
			let gl = glow::Context::from_loader_function_cstr(|symbol_name| dlsym(-2isize as *mut c_void, symbol_name.as_ptr()) as *const _);
			Some(Rc::new(gl))
		}
	}

	/// 섞기 쉬운 수. (시험마다 같은 차례)
	struct NumberSequence(u64);

	impl NumberSequence {
		fn read(&mut self, count: usize) -> usize {
			self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
			((self.0 >> 33) as usize) % count
		}
	}

	/// 마구 부르기 뒤 캐시 답이 진짜 GL 과 같은지. (원본 600 번)
	#[test]
	fn cache_answers_match_real_gl_after_random_calls() {
		let Some(gl) = create_surfaceless_context() else {
			eprintln!("glstatecache: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
			return;
		};
		let _ = attach(&gl);
		let gl: &glow::Context = &gl;
		let mut number_sequence = NumberSequence(20261127);
		let mut mismatch_list: Vec<String> = Vec::new();
		unsafe {
			let program_list = [crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), VERTEX_SHADER_SOURCE, FRAGMENT_SHADER_SOURCE).unwrap().get_program(), crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), VERTEX_SHADER_SOURCE, FRAGMENT_SHADER_SOURCE).unwrap().get_program()];
			let failed_program = create_program(gl).unwrap();
			link_program(gl, failed_program);
			let _ = get_program_link_status(gl, failed_program);
			let mut framebuffer_list: Vec<GlFramebuffer> = (0..3).map(|_| create_framebuffer(gl).unwrap()).collect();
			let mut texture_list: Vec<GlTexture> = (0..4).map(|_| create_texture(gl).unwrap()).collect();
			let mut vertex_array_list: Vec<GlVertexArray> = (0..2).map(|_| create_vertex_array(gl).unwrap()).collect();
			let capability_list = [glow::BLEND, glow::CULL_FACE, glow::DEPTH_TEST, glow::SCISSOR_TEST, glow::STENCIL_TEST, glow::POLYGON_OFFSET_FILL];
			let factor_list = [glow::ZERO, glow::ONE, glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::DST_COLOR, glow::CONSTANT_COLOR, glow::CONSTANT_ALPHA, 0x1234];
			for step in 0..600 {
				match number_sequence.read(19) {
					0 => bind_framebuffer(gl, [glow::FRAMEBUFFER, glow::DRAW_FRAMEBUFFER, glow::READ_FRAMEBUFFER][number_sequence.read(3)], if number_sequence.read(4) == 0 { None } else { Some(framebuffer_list[number_sequence.read(framebuffer_list.len())]) }),
					1 => {
						let index = number_sequence.read(framebuffer_list.len());
						delete_framebuffer(gl, framebuffer_list[index]);
						framebuffer_list[index] = create_framebuffer(gl).unwrap();
					}
					2 => viewport(gl, number_sequence.read(50) as i32 - 10, number_sequence.read(50) as i32, number_sequence.read(4000) as i32 - 5, number_sequence.read(100000) as i32),
					3 => scissor(gl, number_sequence.read(50) as i32, number_sequence.read(50) as i32, number_sequence.read(300) as i32 - 5, number_sequence.read(300) as i32),
					4 => enable(gl, capability_list[number_sequence.read(capability_list.len())]),
					5 => disable(gl, capability_list[number_sequence.read(capability_list.len())]),
					6 => clear_color(gl, number_sequence.read(5) as f32 * 0.5 - 0.5, 0.25, number_sequence.read(3) as f32, 1.0),
					7 => color_mask(gl, number_sequence.read(2) == 0, number_sequence.read(2) == 0, true, number_sequence.read(2) == 0),
					8 => blend_func(gl, factor_list[number_sequence.read(factor_list.len())], factor_list[number_sequence.read(factor_list.len())]),
					9 => blend_func_separate(gl, factor_list[number_sequence.read(factor_list.len())], factor_list[number_sequence.read(factor_list.len())], factor_list[number_sequence.read(4)], factor_list[number_sequence.read(4)]),
					10 => depth_mask(gl, number_sequence.read(2) == 0),
					11 => depth_func(gl, [glow::LESS, glow::LEQUAL, glow::ALWAYS, 0x1234][number_sequence.read(4)]),
					12 => front_face(gl, [glow::CW, glow::CCW, 0x1234][number_sequence.read(3)]),
					13 => cull_face(gl, [glow::FRONT, glow::BACK, glow::FRONT_AND_BACK, 0x1234][number_sequence.read(4)]),
					14 => use_program(gl, [None, Some(program_list[0]), Some(program_list[1]), Some(failed_program)][number_sequence.read(4)]),
					15 => {
						if number_sequence.read(5) == 0 {
							let index = number_sequence.read(vertex_array_list.len());
							delete_vertex_array(gl, vertex_array_list[index]);
							vertex_array_list[index] = create_vertex_array(gl).unwrap();
						}
						bind_vertex_array(gl, if number_sequence.read(3) == 0 { None } else { Some(vertex_array_list[number_sequence.read(vertex_array_list.len())]) });
					}
					16 => active_texture(gl, glow::TEXTURE0 + number_sequence.read(6) as u32),
					17 => {
						if number_sequence.read(6) == 0 {
							let index = number_sequence.read(texture_list.len());
							delete_texture(gl, texture_list[index]);
							texture_list[index] = create_texture(gl).unwrap();
						}
						bind_texture(gl, [glow::TEXTURE_2D, glow::TEXTURE_2D, glow::TEXTURE_CUBE_MAP][number_sequence.read(3)], if number_sequence.read(4) == 0 { None } else { Some(texture_list[number_sequence.read(texture_list.len())]) });
					}
					_ => pixel_store_i32(gl, [glow::PACK_ALIGNMENT, glow::UNPACK_ALIGNMENT][number_sequence.read(2)], [1, 2, 3, 4, 8][number_sequence.read(5)]),
				}
				if number_sequence.read(40) == 0 {
					reset(gl);
				}
				// 캐시 답과 진짜 GL 견주기.
				let mut compare = |name: &str, cached: String, real: String| {
					if cached != real {
						mismatch_list.push(format!("{} 번째 {}: 캐시 {} 진짜 {}", step, name, cached, real));
					}
				};
				compare("FRAMEBUFFER_BINDING", format!("{:?}", read_draw_framebuffer(gl, || gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING))), format!("{:?}", gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING)));
				let mut cached_viewport = [0i32; 4];
				let mut real_viewport = [0i32; 4];
				get_parameter_i32_slice(gl, glow::VIEWPORT, &mut cached_viewport);
				gl.get_parameter_i32_slice(glow::VIEWPORT, &mut real_viewport);
				compare("VIEWPORT", format!("{:?}", cached_viewport), format!("{:?}", real_viewport));
				get_parameter_i32_slice(gl, glow::SCISSOR_BOX, &mut cached_viewport);
				gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut real_viewport);
				compare("SCISSOR_BOX", format!("{:?}", cached_viewport), format!("{:?}", real_viewport));
				let mut cached_color = [0f32; 4];
				let mut real_color = [0f32; 4];
				get_parameter_f32_slice(gl, glow::COLOR_CLEAR_VALUE, &mut cached_color);
				gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut real_color);
				compare("COLOR_CLEAR_VALUE", format!("{:?}", cached_color), format!("{:?}", real_color));
				compare("COLOR_WRITEMASK", format!("{:?}", get_parameter_bool_array::<4>(gl, glow::COLOR_WRITEMASK)), format!("{:?}", gl.get_parameter_bool_array::<4>(glow::COLOR_WRITEMASK)));
				compare("DEPTH_WRITEMASK", format!("{}", get_parameter_bool(gl, glow::DEPTH_WRITEMASK)), format!("{}", gl.get_parameter_bool(glow::DEPTH_WRITEMASK)));
				for parameter in [glow::BLEND_SRC_RGB, glow::BLEND_DST_RGB, glow::BLEND_SRC_ALPHA, glow::BLEND_DST_ALPHA, glow::DEPTH_FUNC, glow::FRONT_FACE, glow::CULL_FACE_MODE, glow::ACTIVE_TEXTURE, glow::PACK_ALIGNMENT, glow::UNPACK_ALIGNMENT, glow::MAX_TEXTURE_SIZE] {
					compare(&format!("0x{:04X}", parameter), format!("{}", get_parameter_i32(gl, parameter)), format!("{}", gl.get_parameter_i32(parameter)));
				}
				for capability in capability_list {
					compare(&format!("is_enabled 0x{:04X}", capability), format!("{}", is_enabled(gl, capability)), format!("{}", gl.is_enabled(capability)));
				}
				compare("CURRENT_PROGRAM", format!("{:?}", read_program(gl, || gl.get_parameter_program(glow::CURRENT_PROGRAM))), format!("{:?}", gl.get_parameter_program(glow::CURRENT_PROGRAM)));
				compare("VERTEX_ARRAY_BINDING", format!("{:?}", read_vertex_array(gl, || gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING))), format!("{:?}", gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING)));
				compare("TEXTURE_BINDING_2D", format!("{:?}", read_texture_binding_2d(gl, || gl.get_parameter_texture(glow::TEXTURE_BINDING_2D))), format!("{:?}", gl.get_parameter_texture(glow::TEXTURE_BINDING_2D)));
				// 견주기 함수가 참이면 진짜 GL 도 그 값이어야 한다.
				let real_program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
				for program in [None, Some(program_list[0]), Some(program_list[1]), Some(failed_program)] {
					if is_program_in_use(gl, program) && program != real_program {
						mismatch_list.push(format!("{} 번째 is_program_in_use 참인데 진짜 {:?}", step, real_program));
					}
				}
				let real_active_texture = gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
				for texture_unit in 0..6 {
					if is_active_texture(gl, glow::TEXTURE0 + texture_unit) && glow::TEXTURE0 + texture_unit != real_active_texture {
						mismatch_list.push(format!("{} 번째 is_active_texture 참인데 진짜 {}", step, real_active_texture));
					}
				}
				let real_texture = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
				for texture in texture_list.iter().map(|texture| Some(*texture)).chain([None]) {
					if is_texture_bound(gl, real_active_texture, glow::TEXTURE_2D, texture) && texture != real_texture {
						mismatch_list.push(format!("{} 번째 is_texture_bound 참인데 진짜 {:?}", step, real_texture));
					}
				}
				let real_vertex_array = gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING);
				for vertex_array in vertex_array_list.iter().map(|vertex_array| Some(*vertex_array)).chain([None]) {
					if is_vertex_array_bound(gl, vertex_array) && vertex_array != real_vertex_array {
						mismatch_list.push(format!("{} 번째 is_vertex_array_bound 참인데 진짜 {:?}", step, real_vertex_array));
					}
				}
				for mode in [glow::CW, glow::CCW] {
					if is_front_face(gl, mode) && mode as i32 != gl.get_parameter_i32(glow::FRONT_FACE) {
						mismatch_list.push(format!("{} 번째 is_front_face 틀림", step));
					}
				}
				for mode in [glow::FRONT, glow::BACK, glow::FRONT_AND_BACK] {
					if is_cull_face(gl, mode) && mode as i32 != gl.get_parameter_i32(glow::CULL_FACE_MODE) {
						mismatch_list.push(format!("{} 번째 is_cull_face 틀림", step));
					}
				}
			}
		}
		eprintln!("glstatecache: 마구 부르기 600 번, 틀린 것 {} 개", mismatch_list.len());
		assert!(mismatch_list.is_empty(), "틀린 것 {} 개: {:?}", mismatch_list.len(), &mismatch_list[..mismatch_list.len().min(8)]);
	}

	/// 게임 길(GlContext)과 엔진 길(이 모듈)을 한 번씩 골라 부르기. (인자 식은 고른 쪽에서 한 번만 돈다)
	macro_rules! call_either {
		($is_game:expr, $context:expr, $gl:expr, $name:ident($($argument:expr),*)) => {
			if $is_game {
				$context.$name($($argument),*)
			}
			else {
				$name($gl, $($argument),*)
			}
		};
	}

	/// 게임 코드처럼 GlContext 로 상태를 바꾸고 엔진 길을 섞어 불러도, 잊기 없이 GlContext 의 읽기 답이 진짜 GL 과 같은지.
	#[test]
	fn wrapped_context_answers_match_real_gl_without_reset() {
		let Some(gl) = create_surfaceless_context() else {
			eprintln!("glstatecache: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
			return;
		};
		let _ = attach(&gl);
		let gl: &glow::Context = &gl;
		let context = crate::core::glcontext::GlContext::from_context(gl);
		let mut number_sequence = NumberSequence(20261128);
		let mut mismatch_list: Vec<String> = Vec::new();
		unsafe {
			let shader_program_list = [crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), VERTEX_SHADER_SOURCE, FRAGMENT_SHADER_SOURCE).unwrap(), crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), VERTEX_SHADER_SOURCE, FRAGMENT_SHADER_SOURCE).unwrap()];
			let program_list = [shader_program_list[0].get_program(), shader_program_list[1].get_program()];
			let failed_program = context.create_program().unwrap();
			context.link_program(failed_program);
			let _ = context.get_program_parameter_i32(failed_program, glow::LINK_STATUS);
			let mut framebuffer_list: Vec<GlFramebuffer> = (0..3).map(|_| context.create_framebuffer().unwrap()).collect();
			let mut texture_list: Vec<GlTexture> = (0..4).map(|_| context.create_texture().unwrap()).collect();
			let mut vertex_array_list: Vec<GlVertexArray> = (0..2).map(|_| context.create_vertex_array().unwrap()).collect();
			let capability_list = [glow::BLEND, glow::CULL_FACE, glow::DEPTH_TEST, glow::SCISSOR_TEST, glow::STENCIL_TEST, glow::POLYGON_OFFSET_FILL];
			let factor_list = [glow::ZERO, glow::ONE, glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::DST_COLOR, glow::CONSTANT_COLOR, glow::CONSTANT_ALPHA, 0x1234];
			let equation_list = [glow::FUNC_ADD, glow::FUNC_SUBTRACT, glow::FUNC_REVERSE_SUBTRACT, glow::MIN, glow::MAX, 0x1234];
			let mut game_call_count = 0;
			for step in 0..800 {
				let is_game = number_sequence.read(3) != 0;
				game_call_count += is_game as usize;
				match number_sequence.read(30) {
					0 => call_either!(is_game, context, gl, bind_framebuffer([glow::FRAMEBUFFER, glow::DRAW_FRAMEBUFFER, glow::READ_FRAMEBUFFER][number_sequence.read(3)], if number_sequence.read(4) == 0 { None } else { Some(framebuffer_list[number_sequence.read(framebuffer_list.len())]) })),
					1 => {
						let index = number_sequence.read(framebuffer_list.len());
						call_either!(is_game, context, gl, delete_framebuffer(framebuffer_list[index]));
						framebuffer_list[index] = call_either!(is_game, context, gl, create_framebuffer()).unwrap();
					}
					2 => call_either!(is_game, context, gl, viewport(number_sequence.read(50) as i32 - 10, number_sequence.read(50) as i32, number_sequence.read(4000) as i32 - 5, number_sequence.read(100000) as i32)),
					3 => call_either!(is_game, context, gl, scissor(number_sequence.read(50) as i32, number_sequence.read(50) as i32, number_sequence.read(300) as i32 - 5, number_sequence.read(300) as i32)),
					4 => call_either!(is_game, context, gl, enable(capability_list[number_sequence.read(capability_list.len())])),
					5 => call_either!(is_game, context, gl, disable(capability_list[number_sequence.read(capability_list.len())])),
					6 => call_either!(is_game, context, gl, clear_color(number_sequence.read(5) as f32 * 0.5 - 0.5, 0.25, number_sequence.read(3) as f32, 1.0)),
					7 => call_either!(is_game, context, gl, color_mask(number_sequence.read(2) == 0, number_sequence.read(2) == 0, true, number_sequence.read(2) == 0)),
					8 => call_either!(is_game, context, gl, blend_func(factor_list[number_sequence.read(factor_list.len())], factor_list[number_sequence.read(factor_list.len())])),
					9 => call_either!(is_game, context, gl, blend_func_separate(factor_list[number_sequence.read(factor_list.len())], factor_list[number_sequence.read(factor_list.len())], factor_list[number_sequence.read(4)], factor_list[number_sequence.read(4)])),
					10 => call_either!(is_game, context, gl, depth_mask(number_sequence.read(2) == 0)),
					11 => call_either!(is_game, context, gl, depth_func([glow::LESS, glow::LEQUAL, glow::ALWAYS, 0x1234][number_sequence.read(4)])),
					12 => call_either!(is_game, context, gl, front_face([glow::CW, glow::CCW, 0x1234][number_sequence.read(3)])),
					13 => call_either!(is_game, context, gl, cull_face([glow::FRONT, glow::BACK, glow::FRONT_AND_BACK, 0x1234][number_sequence.read(4)])),
					14 => call_either!(is_game, context, gl, use_program([None, Some(program_list[0]), Some(program_list[1]), Some(failed_program)][number_sequence.read(4)])),
					15 => {
						if number_sequence.read(5) == 0 {
							let index = number_sequence.read(vertex_array_list.len());
							call_either!(is_game, context, gl, delete_vertex_array(vertex_array_list[index]));
							vertex_array_list[index] = call_either!(is_game, context, gl, create_vertex_array()).unwrap();
						}
						call_either!(is_game, context, gl, bind_vertex_array(if number_sequence.read(3) == 0 { None } else { Some(vertex_array_list[number_sequence.read(vertex_array_list.len())]) }));
					}
					16 => call_either!(is_game, context, gl, active_texture(glow::TEXTURE0 + number_sequence.read(6) as u32)),
					17 => {
						if number_sequence.read(6) == 0 {
							let index = number_sequence.read(texture_list.len());
							call_either!(is_game, context, gl, delete_texture(texture_list[index]));
							texture_list[index] = call_either!(is_game, context, gl, create_texture()).unwrap();
						}
						call_either!(is_game, context, gl, bind_texture([glow::TEXTURE_2D, glow::TEXTURE_2D, glow::TEXTURE_CUBE_MAP][number_sequence.read(3)], if number_sequence.read(4) == 0 { None } else { Some(texture_list[number_sequence.read(texture_list.len())]) }));
					}
					18 => call_either!(is_game, context, gl, blend_equation(equation_list[number_sequence.read(equation_list.len())])),
					19 => call_either!(is_game, context, gl, blend_equation_separate(equation_list[number_sequence.read(equation_list.len())], equation_list[number_sequence.read(equation_list.len())])),
					20 => call_either!(is_game, context, gl, pixel_store_i32([glow::PACK_ALIGNMENT, glow::UNPACK_ALIGNMENT][number_sequence.read(2)], [1, 2, 3, 4, 8][number_sequence.read(5)])),
					// 원본 WebGL2 에 없는 데스크탑 GL 함수. (게임 길만, 상태 캐시가 그 값을 잊는다)
					21 => context.enable_draw_buffer(glow::BLEND, number_sequence.read(2) as u32),
					22 => context.disable_draw_buffer(glow::BLEND, number_sequence.read(2) as u32),
					23 => context.color_mask_draw_buffer(number_sequence.read(2) as u32, number_sequence.read(2) == 0, true, number_sequence.read(2) == 0, true),
					24 => context.blend_func_draw_buffer(number_sequence.read(2) as u32, factor_list[number_sequence.read(4)], factor_list[number_sequence.read(4)]),
					25 => context.blend_func_separate_draw_buffer(number_sequence.read(2) as u32, factor_list[number_sequence.read(4)], factor_list[number_sequence.read(4)], glow::ONE, glow::ZERO),
					26 => context.blend_equation_draw_buffer(number_sequence.read(2) as u32, equation_list[number_sequence.read(5)]),
					27 => context.blend_equation_separate_draw_buffer(number_sequence.read(2) as u32, equation_list[number_sequence.read(5)], glow::FUNC_ADD),
					28 => context.viewport_f32_slice(number_sequence.read(2) as u32, 1, &[[number_sequence.read(40) as f32, 2.0, 100.0 + number_sequence.read(40) as f32, 60.0]]),
					_ => context.scissor_slice(number_sequence.read(2) as u32, 1, &[[number_sequence.read(40) as i32, 3, 50 + number_sequence.read(40) as i32, 70]]),
				}
				// GlContext 의 읽기(기억한 값)와 진짜 GL 견주기.
				let mut compare = |name: &str, cached: String, real: String| {
					if cached != real {
						mismatch_list.push(format!("{} 번째 {}: 캐시 {} 진짜 {}", step, name, cached, real));
					}
				};
				for parameter in [glow::DRAW_FRAMEBUFFER_BINDING, glow::READ_FRAMEBUFFER_BINDING] {
					compare(&format!("0x{:04X}", parameter), format!("{:?}", context.get_parameter_framebuffer(parameter)), format!("{:?}", gl.get_parameter_framebuffer(parameter)));
				}
				for parameter in [glow::VIEWPORT, glow::SCISSOR_BOX] {
					let mut cached_value = [0i32; 4];
					let mut real_value = [0i32; 4];
					context.get_parameter_i32_slice(parameter, &mut cached_value);
					gl.get_parameter_i32_slice(parameter, &mut real_value);
					compare(&format!("0x{:04X}", parameter), format!("{:?}", cached_value), format!("{:?}", real_value));
				}
				let mut cached_color = [0f32; 4];
				let mut real_color = [0f32; 4];
				context.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut cached_color);
				gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut real_color);
				compare("COLOR_CLEAR_VALUE", format!("{:?}", cached_color), format!("{:?}", real_color));
				compare("COLOR_WRITEMASK", format!("{:?}", context.get_parameter_bool_array::<4>(glow::COLOR_WRITEMASK)), format!("{:?}", gl.get_parameter_bool_array::<4>(glow::COLOR_WRITEMASK)));
				compare("DEPTH_WRITEMASK", format!("{}", context.get_parameter_bool(glow::DEPTH_WRITEMASK)), format!("{}", gl.get_parameter_bool(glow::DEPTH_WRITEMASK)));
				for parameter in [glow::BLEND_SRC_RGB, glow::BLEND_DST_RGB, glow::BLEND_SRC_ALPHA, glow::BLEND_DST_ALPHA, glow::BLEND_EQUATION_RGB, glow::BLEND_EQUATION_ALPHA, glow::DEPTH_FUNC, glow::FRONT_FACE, glow::CULL_FACE_MODE, glow::ACTIVE_TEXTURE, glow::PACK_ALIGNMENT, glow::UNPACK_ALIGNMENT, glow::MAX_TEXTURE_SIZE] {
					compare(&format!("0x{:04X}", parameter), format!("{}", context.get_parameter_i32(parameter)), format!("{}", gl.get_parameter_i32(parameter)));
				}
				for capability in capability_list {
					compare(&format!("is_enabled 0x{:04X}", capability), format!("{}", context.is_enabled(capability)), format!("{}", gl.is_enabled(capability)));
				}
				compare("CURRENT_PROGRAM", format!("{:?}", context.get_parameter_program(glow::CURRENT_PROGRAM)), format!("{:?}", gl.get_parameter_program(glow::CURRENT_PROGRAM)));
				compare("VERTEX_ARRAY_BINDING", format!("{:?}", context.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING)), format!("{:?}", gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING)));
				compare("TEXTURE_BINDING_2D", format!("{:?}", context.get_parameter_texture(glow::TEXTURE_BINDING_2D)), format!("{:?}", gl.get_parameter_texture(glow::TEXTURE_BINDING_2D)));
				// 엔진의 건너뛰기 견주기도 참이면 진짜 GL 이 그 값이어야 한다.
				let real_program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
				for program in [None, Some(program_list[0]), Some(program_list[1]), Some(failed_program)] {
					if is_program_in_use(gl, program) && program != real_program {
						mismatch_list.push(format!("{} 번째 is_program_in_use 참인데 진짜 {:?}", step, real_program));
					}
				}
				let real_active_texture = gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
				let real_texture = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
				for texture in texture_list.iter().map(|texture| Some(*texture)).chain([None]) {
					if is_texture_bound(gl, real_active_texture, glow::TEXTURE_2D, texture) && texture != real_texture {
						mismatch_list.push(format!("{} 번째 is_texture_bound 참인데 진짜 {:?}", step, real_texture));
					}
				}
				let real_vertex_array = gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING);
				for vertex_array in vertex_array_list.iter().map(|vertex_array| Some(*vertex_array)).chain([None]) {
					if is_vertex_array_bound(gl, vertex_array) && vertex_array != real_vertex_array {
						mismatch_list.push(format!("{} 번째 is_vertex_array_bound 참인데 진짜 {:?}", step, real_vertex_array));
					}
				}
			}
			eprintln!("glstatecache: GlContext 와 엔진 길 섞어 부르기 800 번(게임 길 {} 번), 잊기 없이 틀린 것 {} 개", game_call_count, mismatch_list.len());
		}
		assert!(mismatch_list.is_empty(), "틀린 것 {} 개: {:?}", mismatch_list.len(), &mismatch_list[..mismatch_list.len().min(8)]);
	}

	const FULLSCREEN_VERTEX_SHADER_SOURCE: &str = "#version 300 es\nvoid main() {\n\tvec2 position = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)) * 2.0 - 1.0;\n\tgl_Position = vec4(position, 0.0, 1.0);\n}\n";
	const RED_FRAGMENT_SHADER_SOURCE: &str = "#version 300 es\nprecision highp float;\nout vec4 outputColor;\nvoid main() {\n\toutputColor = vec4(1.0, 0.0, 0.0, 1.0);\n}\n";
	const GREEN_FRAGMENT_SHADER_SOURCE: &str = "#version 300 es\nprecision highp float;\nout vec4 outputColor;\nvoid main() {\n\toutputColor = vec4(0.0, 1.0, 0.0, 1.0);\n}\n";
	const TEXTURE_FRAGMENT_SHADER_SOURCE: &str = "#version 300 es\nprecision highp float;\nuniform sampler2D sourceTexture;\nout vec4 outputColor;\nvoid main() {\n\toutputColor = texture(sourceTexture, vec2(0.5));\n}\n";

	/// 1 x 1 색 텍스처 만들기.
	unsafe fn create_color_texture(context: &crate::core::glcontext::GlContext, color: [u8; 4]) -> GlTexture {
		unsafe {
			let texture = context.create_texture().unwrap();
			context.bind_texture(glow::TEXTURE_2D, Some(texture));
			context.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&color)));
			context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			context.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
			texture
		}
	}

	/// 왼쪽 아래 픽셀 읽기.
	unsafe fn read_corner_pixel(gl: &glow::Context) -> [u8; 4] {
		let mut pixel = [0u8; 4];
		unsafe {
			gl.read_pixels(0, 0, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixel)));
		}
		pixel
	}

	/// 게임 코드가 한 콜백 안에서 GlContext 로 프로그램, 텍스처, 섞기를 바꾼 직후 엔진 그리기를 이어 부르면,
	/// 엔진이 건너뛰지 않고 제 상태를 다시 걸어 그림이 맞는지. (날 glow 로 바꾸면 틀리는 것도 같이 보아 시험이 문제를 잡는지 확인한다)
	#[test]
	fn engine_draw_after_wrapped_context_change_draws_right() {
		let Some(gl) = create_surfaceless_context() else {
			eprintln!("glstatecache: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
			return;
		};
		let _ = attach(&gl);
		let gl: &glow::Context = &gl;
		let context = crate::core::glcontext::GlContext::from_context(gl);
		unsafe {
			// 4 x 4 렌더 타겟과 빈 버텍스 어레이. (꼭짓점은 셰이더가 번호로 짓는다)
			let target_texture = context.create_texture().unwrap();
			context.bind_texture(glow::TEXTURE_2D, Some(target_texture));
			context.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 4, 4, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
			let framebuffer = context.create_framebuffer().unwrap();
			context.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			context.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(target_texture), 0);
			assert_eq!(context.check_framebuffer_status(glow::FRAMEBUFFER), glow::FRAMEBUFFER_COMPLETE);
			context.viewport(0, 0, 4, 4);
			let vertex_array = context.create_vertex_array().unwrap();
			context.bind_vertex_array(Some(vertex_array));
			let red_program = crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), FULLSCREEN_VERTEX_SHADER_SOURCE, RED_FRAGMENT_SHADER_SOURCE).unwrap();
			let green_program = crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), FULLSCREEN_VERTEX_SHADER_SOURCE, GREEN_FRAGMENT_SHADER_SOURCE).unwrap();
			let texture_program = crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), FULLSCREEN_VERTEX_SHADER_SOURCE, TEXTURE_FRAGMENT_SHADER_SOURCE).unwrap();
			let blue_texture = create_color_texture(context, [0, 0, 255, 255]);
			let yellow_texture = create_color_texture(context, [255, 255, 0, 255]);

			// 엔진 그리기. (ShaderProgram::use_program 은 상태 캐시가 같은 프로그램이라 하면 건너뛴다)
			let draw_with_engine_program = |shader_program: &crate::core::shaderprogram::ShaderProgram| {
				shader_program.use_program(GlContext::from_context(gl));
				gl.draw_arrays(glow::TRIANGLES, 0, 3);
				read_corner_pixel(gl)
			};
			// 엔진 텍스처 묶기. (Graphic 처럼 상태 캐시가 묶였다 하면 건너뛴다)
			let draw_with_engine_texture = |texture: GlTexture| {
				if !is_active_texture(gl, glow::TEXTURE0) {
					active_texture(gl, glow::TEXTURE0);
				}
				if !is_texture_bound(gl, glow::TEXTURE0, glow::TEXTURE_2D, Some(texture)) {
					bind_texture(gl, glow::TEXTURE_2D, Some(texture));
				}
				draw_with_engine_program(&texture_program)
			};

			// 1. 프로그램: 엔진이 빨강을 쓰는 중에 게임이 초록으로 바꾸고, 엔진이 빨강 그리기를 잇는다.
			assert_eq!(draw_with_engine_program(&red_program), [255, 0, 0, 255]);
			context.use_program(Some(green_program.get_program()));
			assert_eq!(draw_with_engine_program(&red_program), [255, 0, 0, 255], "GlContext 로 프로그램을 바꾼 뒤 엔진이 건너뛰었다");
			// 날 glow 로 바꾸면 상태 캐시가 몰라 건너뛰어 초록이 나온다. (그래서 날 GL 뒤에는 reset 을 부른다)
			gl.use_program(Some(green_program.get_program()));
			assert_eq!(draw_with_engine_program(&red_program), [0, 255, 0, 255], "시험이 건너뛰기 문제를 잡지 못한다");
			reset(gl);
			assert_eq!(draw_with_engine_program(&red_program), [255, 0, 0, 255]);

			// 2. 텍스처: 엔진이 파랑을 묶은 중에 게임이 노랑을 묶고, 엔진이 파랑 그리기를 잇는다.
			assert_eq!(draw_with_engine_texture(blue_texture), [0, 0, 255, 255]);
			context.bind_texture(glow::TEXTURE_2D, Some(yellow_texture));
			assert_eq!(draw_with_engine_texture(blue_texture), [0, 0, 255, 255], "GlContext 로 텍스처를 바꾼 뒤 엔진이 건너뛰었다");
			context.active_texture(glow::TEXTURE3);
			context.bind_texture(glow::TEXTURE_2D, Some(yellow_texture));
			assert_eq!(draw_with_engine_texture(blue_texture), [0, 0, 255, 255], "GlContext 로 활성 유닛을 바꾼 뒤 엔진이 건너뛰었다");

			// 3. 섞기와 가위: 게임이 섞기를 켜고 끈 것을 엔진이 읽어 그대로 따른다. (엔진은 is_enabled 로 물어 건다)
			context.enable(glow::BLEND);
			context.blend_func(glow::ZERO, glow::ZERO);
			context.disable(glow::BLEND);
			if is_enabled(gl, glow::BLEND) {
				disable(gl, glow::BLEND);
			}
			assert_eq!(draw_with_engine_program(&green_program), [0, 255, 0, 255], "GlContext 로 섞기를 끈 것을 엔진이 몰랐다");
			context.enable(glow::SCISSOR_TEST);
			context.scissor(2, 2, 2, 2);
			let mut scissor_box = [0i32; 4];
			get_parameter_i32_slice(gl, glow::SCISSOR_BOX, &mut scissor_box);
			assert!(is_enabled(gl, glow::SCISSOR_TEST));
			assert_eq!(scissor_box, [2, 2, 2, 2]);
			assert_eq!(draw_with_engine_program(&red_program), [0, 255, 0, 255], "가위 밖(왼쪽 아래)은 앞 그림이 남아야 한다");
			eprintln!("glstatecache: GlContext 로 바꾼 뒤 엔진 그리기 픽셀 모두 맞음");
		}
	}

	/// ARRAY_BUFFER 기억이 진짜 GL 과 같은지. (버퍼를 마구 만들고 ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER, COPY_READ_BUFFER 에 묶고 지운다, 원본 isArrayBufferBound)
	/// - 처음 ELEMENT_ARRAY_BUFFER 에 묶은 버퍼는 WebGL 이 ARRAY_BUFFER 에 묶지 못하게 하므로 모름으로 둔다. (데스크탑 GL 은 묶어 주므로 모름이 맞다)
	#[test]
	fn array_buffer_binding_matches_real_gl() {
		let Some(gl) = create_surfaceless_context() else {
			eprintln!("glstatecache: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
			return;
		};
		let _ = attach(&gl);
		let gl: &glow::Context = &gl;
		let context = crate::core::glcontext::GlContext::from_context(gl);
		let mut number_sequence = NumberSequence(20261128);
		let mut mismatch_list: Vec<String> = Vec::new();
		unsafe {
			let vertex_array = context.create_vertex_array().unwrap();
			context.bind_vertex_array(Some(vertex_array));
			let mut buffer_list: Vec<GlBuffer> = (0..4).map(|_| context.create_buffer().unwrap()).collect();
			for step in 0..400 {
				match number_sequence.read(6) {
					0 | 1 => {
						let buffer = buffer_list[number_sequence.read(buffer_list.len())];
						context.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
					}
					2 => {
						let buffer = buffer_list[number_sequence.read(buffer_list.len())];
						context.bind_buffer([glow::ELEMENT_ARRAY_BUFFER, glow::COPY_READ_BUFFER][number_sequence.read(2)], Some(buffer));
					}
					3 => {
						context.bind_buffer(glow::ARRAY_BUFFER, None);
					}
					4 => {
						let buffer_index = number_sequence.read(buffer_list.len());
						context.delete_buffer(buffer_list[buffer_index]);
						buffer_list[buffer_index] = context.create_buffer().unwrap();
					}
					_ => {
						let buffer = buffer_list[number_sequence.read(buffer_list.len())];
						if !is_array_buffer_bound(gl, Some(buffer)) {
							context.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
						}
						let real = gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING);
						if real != Some(buffer) {
							mismatch_list.push(format!("{}: {:?} 기대, 진짜 {:?}", step, buffer, real));
						}
					}
				}
				let real = gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING);
				let remembered = find(gl).unwrap().array_buffer.get();
				if remembered.is_some_and(|remembered| remembered != real) {
					mismatch_list.push(format!("{}: 기억 {:?}, 진짜 {:?}", step, remembered, real));
				}
			}
		}
		assert!(mismatch_list.is_empty(), "틀린 것 {} 개: {:?}", mismatch_list.len(), &mismatch_list[..mismatch_list.len().min(8)]);
	}

	/// 검사 모드: GlContext 와 엔진 길로 마구 바꾸며 그리면 어긋남이 없고, 날 glow 로 바꾼 뒤 그리면 그 갈래를 잡아 세는지.
	#[test]
	fn check_mode_finds_only_bypassed_state_change() {
		let Some(gl) = create_surfaceless_context() else {
			eprintln!("glstatecache: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
			return;
		};
		let _ = attach(&gl);
		let gl: &glow::Context = &gl;
		let context = crate::core::glcontext::GlContext::from_context(gl);
		set_check_enabled(true);
		unsafe {
			let target_texture = context.create_texture().unwrap();
			context.bind_texture(glow::TEXTURE_2D, Some(target_texture));
			context.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 4, 4, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
			let framebuffer = context.create_framebuffer().unwrap();
			context.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			context.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(target_texture), 0);
			let vertex_array = context.create_vertex_array().unwrap();
			context.bind_vertex_array(Some(vertex_array));
			let program_list = [
				crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), FULLSCREEN_VERTEX_SHADER_SOURCE, RED_FRAGMENT_SHADER_SOURCE).unwrap(),
				crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), FULLSCREEN_VERTEX_SHADER_SOURCE, GREEN_FRAGMENT_SHADER_SOURCE).unwrap(),
				crate::core::shaderprogram::ShaderProgram::new(GlContext::from_context(gl), FULLSCREEN_VERTEX_SHADER_SOURCE, TEXTURE_FRAGMENT_SHADER_SOURCE).unwrap(),
			];
			let texture_list = [create_color_texture(context, [0, 0, 255, 255]), create_color_texture(context, [255, 255, 0, 255]), create_color_texture(context, [0, 255, 255, 255])];
			program_list[0].use_program(GlContext::from_context(gl));

			// 1. GlContext 와 엔진 길로 마구 바꾸고 열 번마다 그리기. (어긋남 0)
			let mut number_sequence = NumberSequence(20261127);
			let capability_list = [glow::BLEND, glow::CULL_FACE, glow::DEPTH_TEST, glow::SCISSOR_TEST];
			let factor_list = [glow::ONE, glow::ZERO, glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA];
			for call_index in 0..400 {
				let is_game = number_sequence.read(2) == 0;
				match number_sequence.read(11) {
					0 => {
						let capability = capability_list[number_sequence.read(4)];
						if number_sequence.read(2) == 0 {
							call_either!(is_game, context, gl, enable(capability));
						}
						else {
							call_either!(is_game, context, gl, disable(capability));
						}
					}
					1 => {
						call_either!(is_game, context, gl, viewport(number_sequence.read(3) as i32, number_sequence.read(3) as i32, 1 + number_sequence.read(4) as i32, 1 + number_sequence.read(4) as i32));
					}
					2 => {
						call_either!(is_game, context, gl, scissor(number_sequence.read(4) as i32, number_sequence.read(4) as i32, 2, 2));
					}
					3 => {
						call_either!(is_game, context, gl, blend_func(factor_list[number_sequence.read(4)], factor_list[number_sequence.read(4)]));
					}
					4 => {
						call_either!(is_game, context, gl, depth_mask(number_sequence.read(2) == 0));
					}
					5 => {
						call_either!(is_game, context, gl, depth_func([glow::LESS, glow::LEQUAL, glow::ALWAYS][number_sequence.read(3)]));
					}
					6 => {
						call_either!(is_game, context, gl, front_face([glow::CW, glow::CCW][number_sequence.read(2)]));
					}
					7 => {
						call_either!(is_game, context, gl, use_program(Some(program_list[number_sequence.read(3)].get_program())));
					}
					8 => {
						call_either!(is_game, context, gl, active_texture(glow::TEXTURE0 + number_sequence.read(4) as u32));
					}
					9 => {
						call_either!(is_game, context, gl, bind_texture(glow::TEXTURE_2D, Some(texture_list[number_sequence.read(3)])));
					}
					_ => {
						call_either!(is_game, context, gl, pixel_store_i32(glow::UNPACK_ALIGNMENT, [1, 2, 4, 8][number_sequence.read(4)]));
					}
				}
				if call_index % 10 == 9 {
					draw_arrays(gl, glow::TRIANGLES, 0, 3);
				}
			}
			let (mismatch_count, draw_count) = get_check_count();
			assert_eq!(draw_count, 40);
			assert_eq!(mismatch_count, 0, "상태 캐시를 거친 호출만 했는데 검사가 어긋남을 찾았다");

			// 2. 날 glow 로 프로그램을 바꾼 뒤 그리면 잡는다. (그 갈래를 잊으므로 다음 그리기에서는 다시 세지 않는다)
			program_list[0].use_program(GlContext::from_context(gl));
			gl.use_program(Some(program_list[1].get_program()));
			draw_arrays(gl, glow::TRIANGLES, 0, 3);
			assert_eq!(get_check_count().0, 1, "날 glow 로 바꾼 프로그램을 잡지 못했다");
			draw_arrays(gl, glow::TRIANGLES, 0, 3);
			assert_eq!(get_check_count().0, 1);

			// 3. 날 glow 로 다른 유닛의 텍스처를 바꾼 뒤 그리면 잡는다. (견주는 읽기가 활성 유닛을 되돌리는지도 본다)
			active_texture(gl, glow::TEXTURE2);
			bind_texture(gl, glow::TEXTURE_2D, Some(texture_list[0]));
			active_texture(gl, glow::TEXTURE0);
			gl.active_texture(glow::TEXTURE2);
			gl.bind_texture(glow::TEXTURE_2D, Some(texture_list[1]));
			gl.active_texture(glow::TEXTURE0);
			draw_arrays(gl, glow::TRIANGLES, 0, 3);
			assert_eq!(get_check_count().0, 2, "날 glow 로 바꾼 유닛 텍스처를 잡지 못했다");
			assert_eq!(gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32, glow::TEXTURE0, "견주는 읽기가 활성 유닛을 바꿨다");
		}
		set_check_enabled(false);
		eprintln!("glstatecache: 검사 모드가 날 glow 호출만 잡음");
	}

	/// 검사 모드: 게임 길(GlContext)로만 상태 함수 모두(손으로 둔 것과, 상태 캐시가 기억하는 값 곁을 건드리는 지은 것)를 마구 불러도 어긋남이 0 인지.
	/// - 데스크탑 GL 4.5 의 이름 붙은 만들기(create_named_*, bind_texture_unit)는 맥 GL 4.1 에 없어 빼고, 나머지는 맥 CGL 에서 진짜 GL 로 돈다.
	#[test]
	fn game_path_state_calls_pass_check_mode() {
		let Some(gl) = create_surfaceless_context() else {
			eprintln!("glstatecache: 창 없는 GL 컨텍스트를 짓지 못해 건너뜀");
			return;
		};
		let _ = attach(&gl);
		let context = GlContext::from_context(&gl);
		set_check_enabled(true);
		let mut number_sequence = NumberSequence(20261128);
		unsafe {
			let shader_program_list = [crate::core::shaderprogram::ShaderProgram::new(context, VERTEX_SHADER_SOURCE, FRAGMENT_SHADER_SOURCE).unwrap(), crate::core::shaderprogram::ShaderProgram::new(context, VERTEX_SHADER_SOURCE, FRAGMENT_SHADER_SOURCE).unwrap()];
			let mut program_list = vec![shader_program_list[0].get_program(), shader_program_list[1].get_program(), context.create_program().unwrap()];
			context.link_program(program_list[2]);
			let mut framebuffer_list: Vec<GlFramebuffer> = (0..3).map(|_| context.create_framebuffer().unwrap()).collect();
			let mut texture_list: Vec<GlTexture> = (0..4).map(|_| context.create_texture().unwrap()).collect();
			let mut vertex_array_list: Vec<GlVertexArray> = (0..2).map(|_| context.create_vertex_array().unwrap()).collect();
			let mut buffer_list: Vec<GlBuffer> = (0..3).map(|_| context.create_buffer().unwrap()).collect();
			let renderbuffer = context.create_renderbuffer().unwrap();
			let sampler = context.create_sampler().unwrap();
			let capability_list = [glow::BLEND, glow::CULL_FACE, glow::DEPTH_TEST, glow::SCISSOR_TEST, glow::STENCIL_TEST, glow::POLYGON_OFFSET_FILL, glow::DITHER, glow::SAMPLE_COVERAGE, glow::RASTERIZER_DISCARD];
			let factor_list = [glow::ZERO, glow::ONE, glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::DST_COLOR, glow::CONSTANT_COLOR, 0x1234];
			let equation_list = [glow::FUNC_ADD, glow::FUNC_SUBTRACT, glow::FUNC_REVERSE_SUBTRACT, glow::MIN, glow::MAX, 0x1234];
			let mut call_count_list = [0usize; 44];
			for _ in 0..2000 {
				let call_index = number_sequence.read(call_count_list.len());
				call_count_list[call_index] += 1;
				match call_index {
					0 => context.bind_framebuffer([glow::FRAMEBUFFER, glow::DRAW_FRAMEBUFFER, glow::READ_FRAMEBUFFER][number_sequence.read(3)], if number_sequence.read(4) == 0 { None } else { Some(framebuffer_list[number_sequence.read(framebuffer_list.len())]) }),
					1 => {
						let index = number_sequence.read(framebuffer_list.len());
						context.delete_framebuffer(framebuffer_list[index]);
						framebuffer_list[index] = context.create_framebuffer().unwrap();
					}
					2 => context.viewport(number_sequence.read(50) as i32 - 10, number_sequence.read(50) as i32, number_sequence.read(4000) as i32 - 5, number_sequence.read(100000) as i32),
					3 => context.viewport_f32_slice(number_sequence.read(2) as u32, 1, &[[number_sequence.read(40) as f32, 2.0, 100.0 + number_sequence.read(40) as f32, 60.0]]),
					4 => context.scissor(number_sequence.read(50) as i32, number_sequence.read(50) as i32, number_sequence.read(300) as i32 - 5, number_sequence.read(300) as i32),
					5 => context.scissor_slice(number_sequence.read(2) as u32, 1, &[[number_sequence.read(40) as i32, 3, 50 + number_sequence.read(40) as i32, 70]]),
					6 => context.enable(capability_list[number_sequence.read(capability_list.len())]),
					7 => context.disable(capability_list[number_sequence.read(capability_list.len())]),
					8 => context.enable_draw_buffer(glow::BLEND, number_sequence.read(2) as u32),
					9 => context.disable_draw_buffer(glow::BLEND, number_sequence.read(2) as u32),
					10 => context.clear_color(number_sequence.read(5) as f32 * 0.5 - 0.5, 0.25, number_sequence.read(3) as f32, 1.0),
					11 => context.color_mask(number_sequence.read(2) == 0, number_sequence.read(2) == 0, true, number_sequence.read(2) == 0),
					12 => context.color_mask_draw_buffer(number_sequence.read(2) as u32, number_sequence.read(2) == 0, true, number_sequence.read(2) == 0, true),
					13 => context.blend_func(factor_list[number_sequence.read(factor_list.len())], factor_list[number_sequence.read(factor_list.len())]),
					14 => context.blend_func_separate(factor_list[number_sequence.read(factor_list.len())], factor_list[number_sequence.read(factor_list.len())], factor_list[number_sequence.read(4)], factor_list[number_sequence.read(4)]),
					15 => context.blend_func_draw_buffer(number_sequence.read(2) as u32, factor_list[number_sequence.read(4)], factor_list[number_sequence.read(4)]),
					16 => context.blend_func_separate_draw_buffer(number_sequence.read(2) as u32, factor_list[number_sequence.read(4)], factor_list[number_sequence.read(4)], glow::ONE, glow::ZERO),
					17 => context.blend_equation(equation_list[number_sequence.read(equation_list.len())]),
					18 => context.blend_equation_separate(equation_list[number_sequence.read(equation_list.len())], equation_list[number_sequence.read(equation_list.len())]),
					19 => context.blend_equation_draw_buffer(number_sequence.read(2) as u32, equation_list[number_sequence.read(5)]),
					20 => context.blend_equation_separate_draw_buffer(number_sequence.read(2) as u32, equation_list[number_sequence.read(5)], glow::FUNC_ADD),
					21 => context.depth_mask(number_sequence.read(2) == 0),
					22 => context.depth_func([glow::LESS, glow::LEQUAL, glow::ALWAYS, 0x1234][number_sequence.read(4)]),
					23 => context.front_face([glow::CW, glow::CCW, 0x1234][number_sequence.read(3)]),
					24 => context.cull_face([glow::FRONT, glow::BACK, glow::FRONT_AND_BACK, 0x1234][number_sequence.read(4)]),
					25 => context.use_program(if number_sequence.read(4) == 0 { None } else { Some(program_list[number_sequence.read(program_list.len())]) }),
					26 => {
						let index = 2 + number_sequence.read(program_list.len() - 2);
						context.delete_program(program_list[index]);
						program_list[index] = context.create_program().unwrap();
						context.link_program(program_list[index]);
						let _ = context.get_program_parameter_i32(program_list[index], glow::LINK_STATUS);
					}
					27 => {
						if let Some(binary) = context.get_program_binary(program_list[0]) {
							context.program_binary(program_list[2], &binary);
						}
						let _ = context.get_program_link_status(program_list[2]);
					}
					28 => {
						if number_sequence.read(5) == 0 {
							let index = number_sequence.read(vertex_array_list.len());
							context.delete_vertex_array(vertex_array_list[index]);
							vertex_array_list[index] = context.create_vertex_array().unwrap();
						}
						context.bind_vertex_array(if number_sequence.read(3) == 0 { None } else { Some(vertex_array_list[number_sequence.read(vertex_array_list.len())]) });
					}
					29 => context.bind_buffer([glow::ARRAY_BUFFER, glow::ELEMENT_ARRAY_BUFFER, glow::UNIFORM_BUFFER, glow::COPY_READ_BUFFER][number_sequence.read(4)], if number_sequence.read(4) == 0 { None } else { Some(buffer_list[number_sequence.read(buffer_list.len())]) }),
					30 => {
						let index = number_sequence.read(buffer_list.len());
						context.delete_buffer(buffer_list[index]);
						buffer_list[index] = context.create_buffer().unwrap();
					}
					31 => context.active_texture(glow::TEXTURE0 + number_sequence.read(6) as u32),
					32 => {
						if number_sequence.read(6) == 0 {
							let index = number_sequence.read(texture_list.len());
							context.delete_texture(texture_list[index]);
							texture_list[index] = context.create_texture().unwrap();
						}
						context.bind_texture([glow::TEXTURE_2D, glow::TEXTURE_2D, glow::TEXTURE_CUBE_MAP, glow::TEXTURE_3D][number_sequence.read(4)], if number_sequence.read(4) == 0 { None } else { Some(texture_list[number_sequence.read(texture_list.len())]) });
					}
					33 => context.pixel_store_i32([glow::PACK_ALIGNMENT, glow::UNPACK_ALIGNMENT][number_sequence.read(2)], [1, 2, 3, 4, 8][number_sequence.read(5)]),
					34 => context.pixel_store_bool([glow::PACK_ALIGNMENT, glow::UNPACK_ALIGNMENT][number_sequence.read(2)], true),
					// 지은 함수 가운데 기억하는 값 곁을 건드리는 것. (상태 캐시가 기억하지 않는 묶음이라 어긋나지 않아야 한다)
					35 => context.bind_buffer_base(glow::UNIFORM_BUFFER, number_sequence.read(4) as u32, Some(buffer_list[number_sequence.read(buffer_list.len())])),
					36 => context.bind_renderbuffer(glow::RENDERBUFFER, if number_sequence.read(2) == 0 { None } else { Some(renderbuffer) }),
					37 => context.bind_sampler(number_sequence.read(4) as u32, if number_sequence.read(2) == 0 { None } else { Some(sampler) }),
					38 => context.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 2, 2, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None)),
					39 => context.blend_color(0.5, 0.25, 0.0, 1.0),
					40 => context.stencil_func(glow::ALWAYS, number_sequence.read(4) as i32, 0xFF),
					41 => context.polygon_offset(1.0, number_sequence.read(4) as f32),
					42 => context.clear(glow::COLOR_BUFFER_BIT),
					_ => {
						let _ = context.get_parameter_i32(glow::ACTIVE_TEXTURE);
						let _ = context.is_enabled(capability_list[number_sequence.read(capability_list.len())]);
						let _ = context.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
					}
				}
				let _ = context.get_error();
				check_before_draw(context, "game_path_state_calls_pass_check_mode");
			}
			let (mismatch_count, draw_count) = get_check_count();
			eprintln!("glstatecache: 게임 길로만 상태 함수 {} 갈래 2000 번, 검사 {} 번, 어긋남 {} 개 (갈래마다 {:?})", call_count_list.len(), draw_count, mismatch_count, call_count_list);
			assert_eq!(draw_count, 2000);
			assert_eq!(mismatch_count, 0, "게임 길로만 불렀는데 검사가 어긋남을 찾았다");
			assert!(call_count_list.iter().all(|&count| count > 0), "부르지 않은 갈래가 있다");
		}
		set_check_enabled(false);
	}
}

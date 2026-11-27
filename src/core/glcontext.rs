//==============================================================================
//! 엔진 GL 컨텍스트. (원본 Graphic.getWebGL2RenderingContext 가 주는 컨텍스트, 상태 함수를 가로챈 것)
//! - 원본은 컨텍스트 객체의 상태 함수를 가로채 게임이 직접 부른 GL 까지 상태 캐시가 기억한다. glow 의 HasContext 는 봉인된 트레이트라
//!   밖에서 구현할 수 없어, glow::Context 를 감싼 이 형이 HasContext 의 메서드 모두를 같은 이름, 같은 꼴의 고유 메서드로 둔다.
//! - 상태 함수(묶기, 켜고 끄기, 뷰포트, 가위, 섞기, 깊이, 면, 프로그램, 버텍스 어레이, 버퍼 묶기, 텍스처, 비우기 색, 픽셀 저장)는 이 파일에서 손으로 두어
//!   상태 캐시를 거쳐 부른다. 원본 WebGL2 에 없는 데스크탑 전용 상태 함수(glEnablei, glViewportArrayv 같은 것)는 부른 뒤 상태 캐시가 그 값을 잊는다.
//! - 그 밖의 것(그리기, 버퍼 채우기, 유니폼, 셰이더 같은 것)은 glcontext/generated.rs 에 기계로 지어 안의 glow::Context 를 그대로 부른다. (원본도 가로채지 않은 함수는 그대로다)
//! - 안의 glow::Context 를 꺼내는 공개 길은 없다. (Deref, AsRef 를 두지 않는다) 게임 도움 함수는 `&GlContext` 를 받는다.
//!   `&glow::Context` 를 받는 함수에는 넘기지 못해(컴파일 오류) 상태 캐시를 건너뛰는 길이 생기지 않는다. 엔진 안의 GL 초기화와 상태 캐시만 crate 안 길로 꺼낸다.
//! - `use glow::HasContext;` 는 없어도 된다. (있어도 고유 메서드가 골리고, GlContext 는 HasContext 를 구현하지 않는다)
//! - 엔진은 GL 을 넘기는 자리에서 상태 캐시를 스스로 잊지 않는다. (원본과 같음) 상태 캐시를 거치지 않은 호출은 VANILLA_GL_STATE_CHECK=1 로 돌려 찾는다.
//==============================================================================
use crate::core::glstatecache::{self, GlStatePart};
use crate::core::graphic::{GlBuffer, GlFramebuffer, GlProgram, GlTexture, GlVertexArray};
use glow::HasContext;
use std::rc::Rc;

mod generated;

//==============================================================================
/// 엔진 GL 컨텍스트. (원본 WebGL2RenderingContext, Graphic::get_gl 이 준다, 상태 함수가 상태 캐시를 거친다)
/// - glow 의 HasContext 메서드 모두를 같은 이름, 같은 꼴로 둔다. 상태 함수(묶기, 켜고 끄기, 뷰포트, 가위, 섞기, 깊이, 면, 프로그램, 버텍스 어레이, 버퍼 묶기, 텍스처, 비우기 색, 픽셀 저장)는
///   상태 캐시를 거쳐 GL 을 부르고, 읽기(get_parameter_*, is_enabled)는 기억한 값으로 답한다. 그 밖의 함수는 안의 glow::Context 를 그대로 부른다.
/// - 게임 코드가 이것으로 상태를 바꾼 직후 같은 콜백 안에서 엔진 그리기를 이어 불러도 엔진이 바뀐 상태를 안다.
/// - 안의 glow::Context 는 꺼낼 수 없다. 게임의 도움 함수는 `gl: &GlContext` 를 받는다. (`&glow::Context` 를 받는 함수에는 넘기지 못한다)
///
/// # 예제
/// 가위로 왼쪽 위 칸만 비운 뒤 엔진 그리기 잇기
/// ```rust,no_run
/// use vanilla::{Colors, Graphic, Rect};
///
/// fn draw(graphic: &mut Graphic) {
/// 	let gl = graphic.get_gl();
/// 	unsafe {
/// 		gl.enable(glow::SCISSOR_TEST);
/// 		gl.scissor(0, 0, 64, 64);
/// 		gl.clear_color(0.0, 0.0, 0.0, 1.0);
/// 		gl.clear(glow::COLOR_BUFFER_BIT);
/// 		gl.disable(glow::SCISSOR_TEST);
/// 		assert!(!gl.is_enabled(glow::SCISSOR_TEST));
/// 	}
/// 	graphic.set_fill_color(Colors::WHITE);
/// 	graphic.draw_rect(Rect::create(8.0, 8.0, 32.0, 32.0));
/// }
/// ```
//==============================================================================
#[repr(transparent)]
pub struct GlContext {
	context: glow::Context,
}

impl GlContext {
	//==============================================================================
	/// glow 컨텍스트를 엔진 컨텍스트로 보기. (같은 자리를 가리킨다, 상태 캐시는 Graphic::new 가 건다)
	//==============================================================================
	pub(crate) fn from_context(context: &glow::Context) -> &GlContext {
		// repr(transparent) 라 GlContext 와 glow::Context 는 같은 꼴이다.
		unsafe { &*(context as *const glow::Context as *const GlContext) }
	}

	//==============================================================================
	/// 안의 glow 컨텍스트 꺼내기. (엔진 안의 GL 초기화와 상태 캐시만 쓴다, 여기로 상태 함수를 부르면 상태 캐시를 건너뛴다)
	//==============================================================================
	pub(crate) fn get_glow_context_unchecked(&self) -> &glow::Context {
		&self.context
	}

	//==============================================================================
	/// 이름 붙은 프레임 버퍼 만들기. (데스크탑 GL 의 glCreateFramebuffers, 지운 이름을 GL 이 다시 주면 상태 캐시가 그 이름의 기억을 잊는다)
	///
	/// # 돌려줌
	/// 새 프레임 버퍼. 못 만들면 까닭.
	//==============================================================================
	pub unsafe fn create_named_framebuffer(&self) -> Result<GlFramebuffer, String> {
		let framebuffer = unsafe { self.context.create_named_framebuffer() }?;
		glstatecache::forget(&self.context, GlStatePart::Framebuffer(framebuffer));
		Ok(framebuffer)
	}

	//==============================================================================
	/// 이름 붙은 텍스처 만들기. (데스크탑 GL 의 glCreateTextures, 지운 이름을 GL 이 다시 주면 상태 캐시가 그 이름의 기억을 잊는다)
	///
	/// # 인자
	/// - `target`: 텍스처 대상. (TEXTURE_2D 같은 것)
	///
	/// # 돌려줌
	/// 새 텍스처. 못 만들면 까닭.
	//==============================================================================
	pub unsafe fn create_named_texture(&self, target: u32) -> Result<GlTexture, String> {
		let texture = unsafe { self.context.create_named_texture(target) }?;
		glstatecache::forget(&self.context, GlStatePart::Texture(texture));
		Ok(texture)
	}

	//==============================================================================
	/// 이름 붙은 버퍼 만들기. (데스크탑 GL 의 glCreateBuffers, 지운 이름을 GL 이 다시 주면 상태 캐시가 그 이름의 기억을 잊는다)
	///
	/// # 돌려줌
	/// 새 버퍼. 못 만들면 까닭.
	//==============================================================================
	pub unsafe fn create_named_buffer(&self) -> Result<GlBuffer, String> {
		let buffer = unsafe { self.context.create_named_buffer() }?;
		glstatecache::forget(&self.context, GlStatePart::Buffer(buffer));
		Ok(buffer)
	}

	//==============================================================================
	/// 이름 붙은 버텍스 어레이 만들기. (데스크탑 GL 의 glCreateVertexArrays, 지운 이름을 GL 이 다시 주면 상태 캐시가 그 이름의 기억을 잊는다)
	///
	/// # 돌려줌
	/// 새 버텍스 어레이. 못 만들면 까닭.
	//==============================================================================
	pub unsafe fn create_named_vertex_array(&self) -> Result<GlVertexArray, String> {
		let vertex_array = unsafe { self.context.create_named_vertex_array() }?;
		glstatecache::forget(&self.context, GlStatePart::VertexArray(vertex_array));
		Ok(vertex_array)
	}

	//==============================================================================
	/// 프로그램 바이너리 올리기. (GL ES 3 의 glProgramBinary, 링크 결과가 바뀌어 상태 캐시가 그 프로그램의 링크 결과를 잊는다)
	///
	/// # 인자
	/// - `program`: 올릴 프로그램.
	/// - `binary`: get_program_binary 로 받은 바이너리.
	//==============================================================================
	pub unsafe fn program_binary(&self, program: GlProgram, binary: &glow::ProgramBinary) {
		unsafe {
			self.context.program_binary(program, binary);
		}
		glstatecache::forget(&self.context, GlStatePart::Program(program));
	}

	//==============================================================================
	/// 픽셀 저장 설정을 참 거짓으로. (원본 pixelStorei 에 true, false 를 넘긴 것, glow 처럼 0 과 1 로 pixel_store_i32 를 거친다)
	///
	/// # 인자
	/// - `parameter`: 설정 이름. (UNPACK_FLIP_Y_WEBGL 같은 것)
	/// - `value`: 값.
	//==============================================================================
	pub unsafe fn pixel_store_bool(&self, parameter: u32, value: bool) {
		unsafe { glstatecache::pixel_store_i32(&self.context, parameter, value as i32) }
	}

	//==============================================================================
	/// 바깥에서 만든 프레임 버퍼 묶기. (웹 전용, glow 의 같은 이름, 상태 캐시가 프레임 버퍼 묶음을 잊는다)
	///
	/// # 인자
	/// - `target`: FRAMEBUFFER, DRAW_FRAMEBUFFER, READ_FRAMEBUFFER.
	/// - `framebuffer`: 묶을 WebGL 프레임 버퍼.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	pub unsafe fn bind_external_framebuffer(&self, target: u32, framebuffer: &web_sys::WebGlFramebuffer) {
		unsafe {
			self.context.bind_external_framebuffer(target, framebuffer);
		}
		glstatecache::forget(&self.context, GlStatePart::FramebufferBinding);
	}

	//==============================================================================
	/// 프레임 버퍼 만들기. (glow 의 같은 이름, 지운 이름을 GL 이 다시 주면 상태 캐시가 지운 기억에서 뺀다)
	///
	/// # 돌려줌
	/// 새 프레임 버퍼. 못 만들면 까닭.
	//==============================================================================
	pub unsafe fn create_framebuffer(&self) -> Result<GlFramebuffer, String> {
		unsafe { glstatecache::create_framebuffer(&self.context) }
	}

	//==============================================================================
	/// 프레임 버퍼 묶기. (원본 bindFramebuffer)
	///
	/// # 인자
	/// - `target`: FRAMEBUFFER, DRAW_FRAMEBUFFER, READ_FRAMEBUFFER.
	/// - `framebuffer`: 묶을 프레임 버퍼. None 이면 기본 프레임 버퍼.
	//==============================================================================
	pub unsafe fn bind_framebuffer(&self, target: u32, framebuffer: Option<GlFramebuffer>) {
		unsafe { glstatecache::bind_framebuffer(&self.context, target, framebuffer) }
	}

	//==============================================================================
	/// 프레임 버퍼 지우기. (원본 deleteFramebuffer, 묶인 것을 지우면 기본 프레임 버퍼로 돌아간다)
	///
	/// # 인자
	/// - `framebuffer`: 지울 프레임 버퍼.
	//==============================================================================
	pub unsafe fn delete_framebuffer(&self, framebuffer: GlFramebuffer) {
		unsafe { glstatecache::delete_framebuffer(&self.context, framebuffer) }
	}

	//==============================================================================
	/// 뷰포트. (원본 viewport)
	///
	/// # 인자
	/// - `x`: 왼쪽. (픽셀, 왼쪽 아래가 원점)
	/// - `y`: 아래쪽. (픽셀)
	/// - `width`: 너비. (픽셀)
	/// - `height`: 높이. (픽셀)
	//==============================================================================
	pub unsafe fn viewport(&self, x: i32, y: i32, width: i32, height: i32) {
		unsafe { glstatecache::viewport(&self.context, x, y, width, height) }
	}

	//==============================================================================
	/// 뷰포트 여럿. (데스크탑 GL 의 glViewportArrayv, 0 번을 바꾸면 상태 캐시가 뷰포트를 잊는다)
	///
	/// # 인자
	/// - `first`: 첫 뷰포트 번호.
	/// - `count`: 뷰포트 수.
	/// - `values`: 뷰포트마다 x, y, 너비, 높이.
	//==============================================================================
	pub unsafe fn viewport_f32_slice(&self, first: u32, count: i32, values: &[[f32; 4]]) {
		unsafe {
			self.context.viewport_f32_slice(first, count, values);
		}
		if first == 0 && count > 0 {
			glstatecache::forget(&self.context, GlStatePart::Viewport);
		}
	}

	//==============================================================================
	/// 가위 상자. (원본 scissor)
	///
	/// # 인자
	/// - `x`: 왼쪽. (픽셀, 왼쪽 아래가 원점)
	/// - `y`: 아래쪽. (픽셀)
	/// - `width`: 너비. (픽셀)
	/// - `height`: 높이. (픽셀)
	//==============================================================================
	pub unsafe fn scissor(&self, x: i32, y: i32, width: i32, height: i32) {
		unsafe { glstatecache::scissor(&self.context, x, y, width, height) }
	}

	//==============================================================================
	/// 가위 상자 여럿. (데스크탑 GL 의 glScissorArrayv, 0 번을 바꾸면 상태 캐시가 가위 상자를 잊는다)
	///
	/// # 인자
	/// - `first`: 첫 가위 상자 번호.
	/// - `count`: 가위 상자 수.
	/// - `scissors`: 가위 상자마다 x, y, 너비, 높이.
	//==============================================================================
	pub unsafe fn scissor_slice(&self, first: u32, count: i32, scissors: &[[i32; 4]]) {
		unsafe {
			self.context.scissor_slice(first, count, scissors);
		}
		if first == 0 && count > 0 {
			glstatecache::forget(&self.context, GlStatePart::ScissorBox);
		}
	}

	//==============================================================================
	/// 기능 켜기. (원본 enable)
	///
	/// # 인자
	/// - `parameter`: 켤 기능. (BLEND, DEPTH_TEST, SCISSOR_TEST 같은 것)
	//==============================================================================
	pub unsafe fn enable(&self, parameter: u32) {
		unsafe { glstatecache::enable(&self.context, parameter) }
	}

	//==============================================================================
	/// 기능 끄기. (원본 disable)
	///
	/// # 인자
	/// - `parameter`: 끌 기능. (BLEND, DEPTH_TEST, SCISSOR_TEST 같은 것)
	//==============================================================================
	pub unsafe fn disable(&self, parameter: u32) {
		unsafe { glstatecache::disable(&self.context, parameter) }
	}

	//==============================================================================
	/// 그리기 버퍼 하나의 기능 켜기. (데스크탑 GL 의 glEnablei, 상태 캐시가 그 기능을 잊는다)
	///
	/// # 인자
	/// - `parameter`: 켤 기능. (BLEND 같은 것)
	/// - `draw_buffer`: 그리기 버퍼 번호.
	//==============================================================================
	pub unsafe fn enable_draw_buffer(&self, parameter: u32, draw_buffer: u32) {
		unsafe {
			self.context.enable_draw_buffer(parameter, draw_buffer);
		}
		glstatecache::forget(&self.context, GlStatePart::Capability(parameter));
	}

	//==============================================================================
	/// 그리기 버퍼 하나의 기능 끄기. (데스크탑 GL 의 glDisablei, 상태 캐시가 그 기능을 잊는다)
	///
	/// # 인자
	/// - `parameter`: 끌 기능. (BLEND 같은 것)
	/// - `draw_buffer`: 그리기 버퍼 번호.
	//==============================================================================
	pub unsafe fn disable_draw_buffer(&self, parameter: u32, draw_buffer: u32) {
		unsafe {
			self.context.disable_draw_buffer(parameter, draw_buffer);
		}
		glstatecache::forget(&self.context, GlStatePart::Capability(parameter));
	}

	//==============================================================================
	/// 기능이 켜졌는지. (원본 isEnabled, 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: 물을 기능.
	///
	/// # 돌려줌
	/// 켜졌으면 true.
	//==============================================================================
	pub unsafe fn is_enabled(&self, parameter: u32) -> bool {
		unsafe { glstatecache::is_enabled(&self.context, parameter) }
	}

	//==============================================================================
	/// 비우기 색. (원본 clearColor)
	///
	/// # 인자
	/// - `red`: 빨강. (0 ~ 1)
	/// - `green`: 초록. (0 ~ 1)
	/// - `blue`: 파랑. (0 ~ 1)
	/// - `alpha`: 알파. (0 ~ 1)
	//==============================================================================
	pub unsafe fn clear_color(&self, red: f32, green: f32, blue: f32, alpha: f32) {
		unsafe { glstatecache::clear_color(&self.context, red, green, blue, alpha) }
	}

	//==============================================================================
	/// 색 쓰기 가림. (원본 colorMask)
	///
	/// # 인자
	/// - `red`: 빨강을 쓰는지.
	/// - `green`: 초록을 쓰는지.
	/// - `blue`: 파랑을 쓰는지.
	/// - `alpha`: 알파를 쓰는지.
	//==============================================================================
	pub unsafe fn color_mask(&self, red: bool, green: bool, blue: bool, alpha: bool) {
		unsafe { glstatecache::color_mask(&self.context, red, green, blue, alpha) }
	}

	//==============================================================================
	/// 그리기 버퍼 하나의 색 쓰기 가림. (데스크탑 GL 의 glColorMaski, 상태 캐시가 색 쓰기 가림을 잊는다)
	///
	/// # 인자
	/// - `buffer`: 그리기 버퍼 번호.
	/// - `red`: 빨강을 쓰는지.
	/// - `green`: 초록을 쓰는지.
	/// - `blue`: 파랑을 쓰는지.
	/// - `alpha`: 알파를 쓰는지.
	//==============================================================================
	pub unsafe fn color_mask_draw_buffer(&self, buffer: u32, red: bool, green: bool, blue: bool, alpha: bool) {
		unsafe {
			self.context.color_mask_draw_buffer(buffer, red, green, blue, alpha);
		}
		glstatecache::forget(&self.context, GlStatePart::ColorWriteMask);
	}

	//==============================================================================
	/// 섞기 인자. (원본 blendFunc)
	///
	/// # 인자
	/// - `src`: 소스 인자. (SRC_ALPHA 같은 것)
	/// - `dst`: 대상 인자. (ONE_MINUS_SRC_ALPHA 같은 것)
	//==============================================================================
	pub unsafe fn blend_func(&self, src: u32, dst: u32) {
		unsafe { glstatecache::blend_func(&self.context, src, dst) }
	}

	//==============================================================================
	/// 섞기 인자 따로. (원본 blendFuncSeparate)
	///
	/// # 인자
	/// - `src_rgb`: 색의 소스 인자.
	/// - `dst_rgb`: 색의 대상 인자.
	/// - `src_alpha`: 알파의 소스 인자.
	/// - `dst_alpha`: 알파의 대상 인자.
	//==============================================================================
	pub unsafe fn blend_func_separate(&self, src_rgb: u32, dst_rgb: u32, src_alpha: u32, dst_alpha: u32) {
		unsafe { glstatecache::blend_func_separate(&self.context, src_rgb, dst_rgb, src_alpha, dst_alpha) }
	}

	//==============================================================================
	/// 그리기 버퍼 하나의 섞기 인자. (데스크탑 GL 의 glBlendFunci, 상태 캐시가 섞기 인자를 잊는다)
	///
	/// # 인자
	/// - `draw_buffer`: 그리기 버퍼 번호.
	/// - `src`: 소스 인자.
	/// - `dst`: 대상 인자.
	//==============================================================================
	pub unsafe fn blend_func_draw_buffer(&self, draw_buffer: u32, src: u32, dst: u32) {
		unsafe {
			self.context.blend_func_draw_buffer(draw_buffer, src, dst);
		}
		glstatecache::forget(&self.context, GlStatePart::BlendFactors);
	}

	//==============================================================================
	/// 그리기 버퍼 하나의 섞기 인자 따로. (데스크탑 GL 의 glBlendFuncSeparatei, 상태 캐시가 섞기 인자를 잊는다)
	///
	/// # 인자
	/// - `draw_buffer`: 그리기 버퍼 번호.
	/// - `src_rgb`: 색의 소스 인자.
	/// - `dst_rgb`: 색의 대상 인자.
	/// - `src_alpha`: 알파의 소스 인자.
	/// - `dst_alpha`: 알파의 대상 인자.
	//==============================================================================
	pub unsafe fn blend_func_separate_draw_buffer(&self, draw_buffer: u32, src_rgb: u32, dst_rgb: u32, src_alpha: u32, dst_alpha: u32) {
		unsafe {
			self.context.blend_func_separate_draw_buffer(draw_buffer, src_rgb, dst_rgb, src_alpha, dst_alpha);
		}
		glstatecache::forget(&self.context, GlStatePart::BlendFactors);
	}

	//==============================================================================
	/// 섞기 식. (원본 blendEquation)
	///
	/// # 인자
	/// - `mode`: 섞기 식. (FUNC_ADD, FUNC_SUBTRACT, FUNC_REVERSE_SUBTRACT, MIN, MAX)
	//==============================================================================
	pub unsafe fn blend_equation(&self, mode: u32) {
		unsafe { glstatecache::blend_equation(&self.context, mode) }
	}

	//==============================================================================
	/// 섞기 식 따로. (원본 blendEquationSeparate)
	///
	/// # 인자
	/// - `mode_rgb`: 색의 섞기 식.
	/// - `mode_alpha`: 알파의 섞기 식.
	//==============================================================================
	pub unsafe fn blend_equation_separate(&self, mode_rgb: u32, mode_alpha: u32) {
		unsafe { glstatecache::blend_equation_separate(&self.context, mode_rgb, mode_alpha) }
	}

	//==============================================================================
	/// 그리기 버퍼 하나의 섞기 식. (데스크탑 GL 의 glBlendEquationi, 상태 캐시가 섞기 식을 잊는다)
	///
	/// # 인자
	/// - `draw_buffer`: 그리기 버퍼 번호.
	/// - `mode`: 섞기 식.
	//==============================================================================
	pub unsafe fn blend_equation_draw_buffer(&self, draw_buffer: u32, mode: u32) {
		unsafe {
			self.context.blend_equation_draw_buffer(draw_buffer, mode);
		}
		glstatecache::forget(&self.context, GlStatePart::BlendEquations);
	}

	//==============================================================================
	/// 그리기 버퍼 하나의 섞기 식 따로. (데스크탑 GL 의 glBlendEquationSeparatei, 상태 캐시가 섞기 식을 잊는다)
	///
	/// # 인자
	/// - `buffer`: 그리기 버퍼 번호.
	/// - `mode_rgb`: 색의 섞기 식.
	/// - `mode_alpha`: 알파의 섞기 식.
	//==============================================================================
	pub unsafe fn blend_equation_separate_draw_buffer(&self, buffer: u32, mode_rgb: u32, mode_alpha: u32) {
		unsafe {
			self.context.blend_equation_separate_draw_buffer(buffer, mode_rgb, mode_alpha);
		}
		glstatecache::forget(&self.context, GlStatePart::BlendEquations);
	}

	//==============================================================================
	/// 깊이 쓰기. (원본 depthMask)
	///
	/// # 인자
	/// - `value`: 깊이를 쓰는지.
	//==============================================================================
	pub unsafe fn depth_mask(&self, value: bool) {
		unsafe { glstatecache::depth_mask(&self.context, value) }
	}

	//==============================================================================
	/// 깊이 견주기. (원본 depthFunc)
	///
	/// # 인자
	/// - `func`: 견주기. (NEVER ~ ALWAYS)
	//==============================================================================
	pub unsafe fn depth_func(&self, func: u32) {
		unsafe { glstatecache::depth_func(&self.context, func) }
	}

	//==============================================================================
	/// 앞면 감김 방향. (원본 frontFace)
	///
	/// # 인자
	/// - `value`: CW 나 CCW.
	//==============================================================================
	pub unsafe fn front_face(&self, value: u32) {
		unsafe { glstatecache::front_face(&self.context, value) }
	}

	//==============================================================================
	/// 가리는 면. (원본 cullFace)
	///
	/// # 인자
	/// - `value`: FRONT, BACK, FRONT_AND_BACK.
	//==============================================================================
	pub unsafe fn cull_face(&self, value: u32) {
		unsafe { glstatecache::cull_face(&self.context, value) }
	}

	//==============================================================================
	/// 프로그램 만들기. (glow 의 같은 이름, 지운 이름을 GL 이 다시 주면 상태 캐시가 그 이름의 링크 결과를 잊는다)
	///
	/// # 돌려줌
	/// 새 프로그램. 못 만들면 까닭.
	//==============================================================================
	pub unsafe fn create_program(&self) -> Result<GlProgram, String> {
		unsafe { glstatecache::create_program(&self.context) }
	}

	//==============================================================================
	/// 프로그램 링크. (원본 linkProgram)
	///
	/// # 인자
	/// - `program`: 링크할 프로그램.
	//==============================================================================
	pub unsafe fn link_program(&self, program: GlProgram) {
		unsafe { glstatecache::link_program(&self.context, program) }
	}

	//==============================================================================
	/// 링크 결과 묻기. (원본 getProgramParameter(LINK_STATUS), 상태 캐시가 보고 기억한다)
	///
	/// # 인자
	/// - `program`: 물을 프로그램.
	///
	/// # 돌려줌
	/// 링크에 성공했으면 true.
	//==============================================================================
	pub unsafe fn get_program_link_status(&self, program: GlProgram) -> bool {
		unsafe { glstatecache::get_program_link_status(&self.context, program) }
	}

	//==============================================================================
	/// 프로그램 값 묻기. (원본 getProgramParameter, LINK_STATUS 는 상태 캐시가 보고 기억한다)
	///
	/// # 인자
	/// - `program`: 물을 프로그램.
	/// - `parameter`: 물을 값. (LINK_STATUS, ACTIVE_UNIFORMS 같은 것)
	///
	/// # 돌려줌
	/// 그 값.
	//==============================================================================
	pub unsafe fn get_program_parameter_i32(&self, program: GlProgram, parameter: u32) -> i32 {
		if parameter == glow::LINK_STATUS {
			return unsafe { glstatecache::get_program_link_status(&self.context, program) } as i32;
		}
		unsafe { self.context.get_program_parameter_i32(program, parameter) }
	}

	//==============================================================================
	/// 프로그램 쓰기. (원본 useProgram)
	///
	/// # 인자
	/// - `program`: 쓸 프로그램. None 이면 풀기.
	//==============================================================================
	pub unsafe fn use_program(&self, program: Option<GlProgram>) {
		unsafe { glstatecache::use_program(&self.context, program) }
	}

	//==============================================================================
	/// 프로그램 지우기. (원본 deleteProgram)
	///
	/// # 인자
	/// - `program`: 지울 프로그램.
	//==============================================================================
	pub unsafe fn delete_program(&self, program: GlProgram) {
		unsafe { glstatecache::delete_program(&self.context, program) }
	}

	//==============================================================================
	/// 버텍스 어레이 만들기. (glow 의 같은 이름, 지운 이름을 GL 이 다시 주면 상태 캐시가 지운 기억에서 뺀다)
	///
	/// # 돌려줌
	/// 새 버텍스 어레이. 못 만들면 까닭.
	//==============================================================================
	pub unsafe fn create_vertex_array(&self) -> Result<GlVertexArray, String> {
		unsafe { glstatecache::create_vertex_array(&self.context) }
	}

	//==============================================================================
	/// 버텍스 어레이 묶기. (원본 bindVertexArray)
	///
	/// # 인자
	/// - `vertex_array`: 묶을 버텍스 어레이. None 이면 풀기.
	//==============================================================================
	pub unsafe fn bind_vertex_array(&self, vertex_array: Option<GlVertexArray>) {
		unsafe { glstatecache::bind_vertex_array(&self.context, vertex_array) }
	}

	//==============================================================================
	/// 버텍스 어레이 지우기. (원본 deleteVertexArray)
	///
	/// # 인자
	/// - `vertex_array`: 지울 버텍스 어레이.
	//==============================================================================
	pub unsafe fn delete_vertex_array(&self, vertex_array: GlVertexArray) {
		unsafe { glstatecache::delete_vertex_array(&self.context, vertex_array) }
	}

	//==============================================================================
	/// 버퍼 만들기. (glow 의 같은 이름, 지운 이름을 GL 이 다시 주면 상태 캐시가 지운 기억과 처음 묶은 대상을 뺀다)
	///
	/// # 돌려줌
	/// 새 버퍼. 못 만들면 까닭.
	//==============================================================================
	pub unsafe fn create_buffer(&self) -> Result<GlBuffer, String> {
		unsafe { glstatecache::create_buffer(&self.context) }
	}

	//==============================================================================
	/// 버퍼 묶기. (원본 bindBuffer, 상태 캐시는 ARRAY_BUFFER 묶음을 기억해 엔진 2D 그리기가 제 정점 버퍼를 다시 묶을지 견준다)
	///
	/// # 인자
	/// - `target`: 묶을 대상. (ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER 같은 것)
	/// - `buffer`: 묶을 버퍼. None 이면 풀기.
	//==============================================================================
	pub unsafe fn bind_buffer(&self, target: u32, buffer: Option<GlBuffer>) {
		unsafe { glstatecache::bind_buffer(&self.context, target, buffer) }
	}

	//==============================================================================
	/// 버퍼 지우기. (원본 deleteBuffer)
	///
	/// # 인자
	/// - `buffer`: 지울 버퍼.
	//==============================================================================
	pub unsafe fn delete_buffer(&self, buffer: GlBuffer) {
		unsafe { glstatecache::delete_buffer(&self.context, buffer) }
	}

	//==============================================================================
	/// 텍스처 만들기. (glow 의 같은 이름, 지운 이름을 GL 이 다시 주면 상태 캐시가 지운 기억과 처음 묶은 대상을 뺀다)
	///
	/// # 돌려줌
	/// 새 텍스처. 못 만들면 까닭.
	//==============================================================================
	pub unsafe fn create_texture(&self) -> Result<GlTexture, String> {
		unsafe { glstatecache::create_texture(&self.context) }
	}

	//==============================================================================
	/// 활성 텍스처 유닛. (원본 activeTexture)
	///
	/// # 인자
	/// - `unit`: TEXTURE0 + 유닛 번호.
	//==============================================================================
	pub unsafe fn active_texture(&self, unit: u32) {
		unsafe { glstatecache::active_texture(&self.context, unit) }
	}

	//==============================================================================
	/// 텍스처 묶기. (원본 bindTexture, 활성 유닛에)
	///
	/// # 인자
	/// - `target`: TEXTURE_2D, TEXTURE_CUBE_MAP, TEXTURE_3D, TEXTURE_2D_ARRAY.
	/// - `texture`: 묶을 텍스처. None 이면 풀기.
	//==============================================================================
	pub unsafe fn bind_texture(&self, target: u32, texture: Option<GlTexture>) {
		unsafe { glstatecache::bind_texture(&self.context, target, texture) }
	}

	//==============================================================================
	/// 유닛에 텍스처 바로 묶기. (데스크탑 GL 의 glBindTextureUnit, 상태 캐시가 그 유닛의 묶음을 잊는다)
	///
	/// # 인자
	/// - `unit`: 유닛 번호. (0 부터)
	/// - `texture`: 묶을 텍스처. None 이면 풀기.
	//==============================================================================
	pub unsafe fn bind_texture_unit(&self, unit: u32, texture: Option<GlTexture>) {
		unsafe {
			self.context.bind_texture_unit(unit, texture);
		}
		glstatecache::forget(&self.context, GlStatePart::TextureUnit(unit));
	}

	//==============================================================================
	/// 텍스처 지우기. (원본 deleteTexture, 묶인 것을 지우면 그 유닛에서 풀린다)
	///
	/// # 인자
	/// - `texture`: 지울 텍스처.
	//==============================================================================
	pub unsafe fn delete_texture(&self, texture: GlTexture) {
		unsafe { glstatecache::delete_texture(&self.context, texture) }
	}

	//==============================================================================
	/// 픽셀 저장 설정. (원본 pixelStorei)
	///
	/// # 인자
	/// - `parameter`: 설정 이름. (UNPACK_ALIGNMENT, PACK_ALIGNMENT 같은 것)
	/// - `value`: 값.
	//==============================================================================
	pub unsafe fn pixel_store_i32(&self, parameter: u32, value: i32) {
		unsafe { glstatecache::pixel_store_i32(&self.context, parameter, value) }
	}

	//==============================================================================
	/// 정수 하나 읽기. (원본 getParameter, 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: 읽을 값. (DEPTH_FUNC, ACTIVE_TEXTURE, MAX_TEXTURE_SIZE 같은 것)
	///
	/// # 돌려줌
	/// 그 값.
	//==============================================================================
	pub unsafe fn get_parameter_i32(&self, parameter: u32) -> i32 {
		unsafe { glstatecache::get_parameter_i32(&self.context, parameter) }
	}

	//==============================================================================
	/// 참 거짓 하나 읽기. (원본 getParameter, 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: 읽을 값. (DEPTH_WRITEMASK 같은 것)
	///
	/// # 돌려줌
	/// 그 값.
	//==============================================================================
	pub unsafe fn get_parameter_bool(&self, parameter: u32) -> bool {
		unsafe { glstatecache::get_parameter_bool(&self.context, parameter) }
	}

	//==============================================================================
	/// 참 거짓 여럿 읽기. (원본 getParameter, COLOR_WRITEMASK 는 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: 읽을 값.
	///
	/// # 돌려줌
	/// 그 값들.
	//==============================================================================
	pub unsafe fn get_parameter_bool_array<const N: usize>(&self, parameter: u32) -> [bool; N] {
		unsafe { glstatecache::get_parameter_bool_array::<N>(&self.context, parameter) }
	}

	//==============================================================================
	/// 정수 여럿 읽기. (원본 getParameter, VIEWPORT, SCISSOR_BOX, MAX_VIEWPORT_DIMS 는 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: 읽을 값.
	/// - `out`: 값을 받을 자리.
	//==============================================================================
	pub unsafe fn get_parameter_i32_slice(&self, parameter: u32, out: &mut [i32]) {
		unsafe { glstatecache::get_parameter_i32_slice(&self.context, parameter, out) }
	}

	//==============================================================================
	/// 실수 여럿 읽기. (원본 getParameter, COLOR_CLEAR_VALUE 는 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: 읽을 값.
	/// - `out`: 값을 받을 자리.
	//==============================================================================
	pub unsafe fn get_parameter_f32_slice(&self, parameter: u32, out: &mut [f32]) {
		unsafe { glstatecache::get_parameter_f32_slice(&self.context, parameter, out) }
	}

	//==============================================================================
	/// 묶인 프레임 버퍼 읽기. (원본 getParameter, FRAMEBUFFER_BINDING, DRAW_FRAMEBUFFER_BINDING, READ_FRAMEBUFFER_BINDING 은 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: 읽을 묶음.
	///
	/// # 돌려줌
	/// 묶인 프레임 버퍼. 기본 프레임 버퍼면 None.
	//==============================================================================
	pub unsafe fn get_parameter_framebuffer(&self, parameter: u32) -> Option<GlFramebuffer> {
		match parameter {
			glow::DRAW_FRAMEBUFFER_BINDING => crate::core::graphic::read_framebuffer_binding(self),
			glow::READ_FRAMEBUFFER_BINDING => glstatecache::read_reading_framebuffer(&self.context, || unsafe { self.context.get_parameter_framebuffer(parameter) }),
			_ => unsafe { self.context.get_parameter_framebuffer(parameter) },
		}
	}

	//==============================================================================
	/// 쓰는 프로그램 읽기. (원본 getParameter(CURRENT_PROGRAM), 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: CURRENT_PROGRAM.
	///
	/// # 돌려줌
	/// 쓰는 프로그램. 없으면 None.
	//==============================================================================
	pub unsafe fn get_parameter_program(&self, parameter: u32) -> Option<GlProgram> {
		if parameter == glow::CURRENT_PROGRAM {
			return crate::core::graphic::read_program_binding(self);
		}
		unsafe { self.context.get_parameter_program(parameter) }
	}

	//==============================================================================
	/// 묶인 버텍스 어레이 읽기. (원본 getParameter(VERTEX_ARRAY_BINDING), 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: VERTEX_ARRAY_BINDING.
	///
	/// # 돌려줌
	/// 묶인 버텍스 어레이. 없으면 None.
	//==============================================================================
	pub unsafe fn get_parameter_vertex_array(&self, parameter: u32) -> Option<GlVertexArray> {
		if parameter == glow::VERTEX_ARRAY_BINDING {
			return crate::core::graphic::read_vertex_array_binding(self);
		}
		unsafe { self.context.get_parameter_vertex_array(parameter) }
	}

	//==============================================================================
	/// 활성 유닛에 묶인 텍스처 읽기. (원본 getParameter, TEXTURE_BINDING_2D 는 기억한 값으로 답한다)
	///
	/// # 인자
	/// - `parameter`: TEXTURE_BINDING_2D 같은 것.
	///
	/// # 돌려줌
	/// 묶인 텍스처. 없으면 None.
	//==============================================================================
	pub unsafe fn get_parameter_texture(&self, parameter: u32) -> Option<GlTexture> {
		if parameter == glow::TEXTURE_BINDING_2D {
			return crate::core::graphic::read_texture_binding(self);
		}
		unsafe { self.context.get_parameter_texture(parameter) }
	}
}

//==============================================================================
/// 상태 캐시 함수가 받는 컨텍스트. (glow::Context 와 GlContext 둘 다, crate 안에서만 보인다)
//==============================================================================
pub(crate) trait GlowContextSource {
	//==============================================================================
	/// 안의 glow 컨텍스트. (같은 자리를 가리킨다, 상태 캐시가 컨텍스트를 찾는 열쇠)
	//==============================================================================
	fn read_glow_context(&self) -> &glow::Context;
}

impl GlowContextSource for glow::Context {
	//==============================================================================
	/// 그 자신.
	//==============================================================================
	fn read_glow_context(&self) -> &glow::Context {
		self
	}
}

impl GlowContextSource for GlContext {
	//==============================================================================
	/// 감싼 glow 컨텍스트.
	//==============================================================================
	fn read_glow_context(&self) -> &glow::Context {
		&self.context
	}
}

impl<T: GlowContextSource + ?Sized> GlowContextSource for &T {
	//==============================================================================
	/// 가리키는 것의 glow 컨텍스트.
	//==============================================================================
	fn read_glow_context(&self) -> &glow::Context {
		(**self).read_glow_context()
	}
}

impl<T: GlowContextSource + ?Sized> GlowContextSource for Rc<T> {
	//==============================================================================
	/// 가리키는 것의 glow 컨텍스트.
	//==============================================================================
	fn read_glow_context(&self) -> &glow::Context {
		(**self).read_glow_context()
	}
}

//==============================================================================
// 컴파일 시험. (안의 glow::Context 를 꺼내는 공개 길이 없는지, cargo test --doc 이 돌린다)
//==============================================================================
/// `&glow::Context` 를 받는 함수에 GlContext 를 넘기지 못한다.
/// ```compile_fail
/// fn helper(gl: &glow::Context) {
/// 	let _ = gl;
/// }
/// fn draw(graphic: &mut vanilla::Graphic) {
/// 	helper(graphic.get_gl());
/// }
/// ```
///
/// GlContext 는 glow::HasContext 를 구현하지 않는다. (트레이트로 상태 함수를 바로 부르지 못한다)
/// ```compile_fail
/// fn draw(graphic: &mut vanilla::Graphic) {
/// 	unsafe {
/// 		glow::HasContext::enable(graphic.get_gl(), glow::BLEND);
/// 	}
/// }
/// ```
///
/// Deref, AsRef 로 꺼내지 못한다.
/// ```compile_fail
/// fn draw(graphic: &mut vanilla::Graphic) {
/// 	let raw: &glow::Context = graphic.get_gl().as_ref();
/// 	let _ = raw;
/// }
/// ```
///
/// ```compile_fail
/// fn draw(graphic: &mut vanilla::Graphic) {
/// 	let raw: &glow::Context = &*graphic.get_gl();
/// 	let _ = raw;
/// }
/// ```
///
/// crate 안 길은 밖에서 보이지 않는다.
/// ```compile_fail
/// fn draw(graphic: &mut vanilla::Graphic) {
/// 	let raw: &glow::Context = graphic.get_gl().get_glow_context_unchecked();
/// 	let _ = raw;
/// }
/// ```
///
/// 같은 이름의 고유 메서드로는 그대로 돈다. (`use glow::HasContext;` 가 없어도)
/// ```rust,no_run
/// fn helper(gl: &vanilla::GlContext) {
/// 	unsafe {
/// 		gl.enable(glow::BLEND);
/// 		gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
/// 		gl.draw_arrays(glow::TRIANGLES, 0, 3);
/// 	}
/// }
/// fn draw(graphic: &mut vanilla::Graphic) {
/// 	helper(graphic.get_gl());
/// }
/// ```
#[cfg(doctest)]
struct RawContextAccessTest;

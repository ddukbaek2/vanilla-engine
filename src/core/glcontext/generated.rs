//==============================================================================
//! GlContext 의 glow 넘기기 메서드. (기계로 지은 파일, 손으로 고치지 않는다)
//! - glow 0.18.0 의 HasContext 트레이트 메서드 모두(396 개)와 웹의 glow::Context 고유 메서드 가운데 &self 를 받는 것(45 개) 가운데,
//!   src/core/glcontext.rs 에 손으로 둔 것(상태 캐시를 거치거나 그 값을 잊게 하는 61 개)을 뺀 380 개다. 같은 이름, 같은 꼴로 안의 glow::Context 를 그대로 부른다.
//! - 웹 고유 메서드는 glow 와 같은 cfg(all(target_family = "wasm", not(target_os = "emscripten"))) 로 가른다.
//! - 지은 법: glow-0.18.0/src/lib.rs 의 `pub trait HasContext` 선언에서 메서드 선언(속성 #[doc(alias)] 와 함께)을 차례대로 읽고,
//!   glow-0.18.0/src/web_sys.rs 의 첫 `impl Context` 블록에서 `pub fn`, `pub unsafe fn` 가운데 첫 인자가 self 인 것을 읽는다.
//!   glcontext.rs 의 `impl GlContext` 에 있는 `pub fn`, `pub unsafe fn` 이름은 뺀다. 형의 `Self::X` 와 `<Self as HasContext>::X` 는 `glow::X`,
//!   web_sys 에서 가져온 형은 `web_sys::X`, HashSet 은 `std::collections::HashSet`, 그 밖의 glow 형은 `glow::X` 로 쓴다.
//!   메서드마다 `#[inline]` 을 달고 unsafe 면 `unsafe { self.context.이름(인자) }`, 아니면 `self.context.이름(인자)` 로 넘긴다.
//!   glow 판을 올리면 새 판의 소스에서 같은 규칙으로 다시 짓고, 새 상태 함수가 생겼으면 glcontext.rs 에 손으로 두어 상태 캐시를 거치게 한다.
//==============================================================================
#![allow(clippy::too_many_arguments)]
use super::GlContext;
use glow::HasContext;

impl GlContext {
	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub fn supported_extensions(&self) -> &std::collections::HashSet<String> {
		self.context.supported_extensions()
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub fn supports_debug(&self) -> bool {
		self.context.supports_debug()
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub fn version(&self) -> &glow::Version {
		self.context.version()
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn is_framebuffer(&self, framebuffer: glow::Framebuffer) -> bool {
		unsafe { self.context.is_framebuffer(framebuffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn create_query(&self) -> Result<glow::Query, String> {
		unsafe { self.context.create_query() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn create_renderbuffer(&self) -> Result<glow::Renderbuffer, String> {
		unsafe { self.context.create_renderbuffer() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `renderbuffer`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn is_renderbuffer(&self, renderbuffer: glow::Renderbuffer) -> bool {
		unsafe { self.context.is_renderbuffer(renderbuffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn create_sampler(&self) -> Result<glow::Sampler, String> {
		unsafe { self.context.create_sampler() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `shader_type`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn create_shader(&self, shader_type: u32) -> Result<glow::Shader, String> {
		unsafe { self.context.create_shader(shader_type) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `shader`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn is_shader(&self, shader: glow::Shader) -> bool {
		unsafe { self.context.is_shader(shader) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn is_texture(&self, texture: glow::Texture) -> bool {
		unsafe { self.context.is_texture(texture) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `shader`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn delete_shader(&self, shader: glow::Shader) {
		unsafe { self.context.delete_shader(shader) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `shader`: glow 의 같은 인자.
	/// - `source`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn shader_source(&self, shader: glow::Shader, source: &str) {
		unsafe { self.context.shader_source(shader, source) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `shader`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn compile_shader(&self, shader: glow::Shader) {
		unsafe { self.context.compile_shader(shader) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `shader`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_shader_completion_status(&self, shader: glow::Shader) -> bool {
		unsafe { self.context.get_shader_completion_status(shader) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `shader`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_shader_compile_status(&self, shader: glow::Shader) -> bool {
		unsafe { self.context.get_shader_compile_status(shader) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `shader`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_shader_info_log(&self, shader: glow::Shader) -> String {
		unsafe { self.context.get_shader_info_log(shader) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `shader_type`: glow 의 같은 인자.
	/// - `precision_mode`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_shader_precision_format(&self, shader_type: u32, precision_mode: u32) -> Option<glow::ShaderPrecisionFormat> {
		unsafe { self.context.get_shader_precision_format(shader_type, precision_mode) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_tex_image(&self, target: u32, level: i32, format: u32, ty: u32, pixels: glow::PixelPackData) {
		unsafe { self.context.get_tex_image(target, level, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn is_program(&self, program: glow::Program) -> bool {
		unsafe { self.context.is_program(program) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `shader`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn attach_shader(&self, program: glow::Program, shader: glow::Shader) {
		unsafe { self.context.attach_shader(program, shader) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `shader`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn detach_shader(&self, program: glow::Program, shader: glow::Shader) {
		unsafe { self.context.detach_shader(program, shader) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn validate_program(&self, program: glow::Program) {
		unsafe { self.context.validate_program(program) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_program_completion_status(&self, program: glow::Program) -> bool {
		unsafe { self.context.get_program_completion_status(program) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_program_validate_status(&self, program: glow::Program) -> bool {
		unsafe { self.context.get_program_validate_status(program) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_program_info_log(&self, program: glow::Program) -> String {
		unsafe { self.context.get_program_info_log(program) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `interface`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `properties`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_program_resource_i32(&self, program: glow::Program, interface: u32, index: u32, properties: &[u32]) -> Vec<i32> {
		unsafe { self.context.get_program_resource_i32(program, interface, index, properties) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_1_i32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: i32) {
		unsafe { self.context.program_uniform_1_i32(program, location, x) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_2_i32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: i32, y: i32) {
		unsafe { self.context.program_uniform_2_i32(program, location, x, y) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_3_i32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: i32, y: i32, z: i32) {
		unsafe { self.context.program_uniform_3_i32(program, location, x, y, z) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	/// - `w`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_4_i32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: i32, y: i32, z: i32, w: i32) {
		unsafe { self.context.program_uniform_4_i32(program, location, x, y, z, w) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_1_i32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[i32]) {
		unsafe { self.context.program_uniform_1_i32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_2_i32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[i32]) {
		unsafe { self.context.program_uniform_2_i32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_3_i32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[i32]) {
		unsafe { self.context.program_uniform_3_i32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_4_i32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[i32]) {
		unsafe { self.context.program_uniform_4_i32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_1_u32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: u32) {
		unsafe { self.context.program_uniform_1_u32(program, location, x) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_2_u32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: u32, y: u32) {
		unsafe { self.context.program_uniform_2_u32(program, location, x, y) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_3_u32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: u32, y: u32, z: u32) {
		unsafe { self.context.program_uniform_3_u32(program, location, x, y, z) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	/// - `w`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_4_u32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: u32, y: u32, z: u32, w: u32) {
		unsafe { self.context.program_uniform_4_u32(program, location, x, y, z, w) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_1_u32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[u32]) {
		unsafe { self.context.program_uniform_1_u32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_2_u32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[u32]) {
		unsafe { self.context.program_uniform_2_u32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_3_u32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[u32]) {
		unsafe { self.context.program_uniform_3_u32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_4_u32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[u32]) {
		unsafe { self.context.program_uniform_4_u32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_1_f32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: f32) {
		unsafe { self.context.program_uniform_1_f32(program, location, x) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_2_f32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: f32, y: f32) {
		unsafe { self.context.program_uniform_2_f32(program, location, x, y) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_3_f32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: f32, y: f32, z: f32) {
		unsafe { self.context.program_uniform_3_f32(program, location, x, y, z) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	/// - `w`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_4_f32(&self, program: glow::Program, location: Option<&glow::UniformLocation>, x: f32, y: f32, z: f32, w: f32) {
		unsafe { self.context.program_uniform_4_f32(program, location, x, y, z, w) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_1_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[f32]) {
		unsafe { self.context.program_uniform_1_f32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_2_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[f32]) {
		unsafe { self.context.program_uniform_2_f32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_3_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[f32]) {
		unsafe { self.context.program_uniform_3_f32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_4_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, v: &[f32]) {
		unsafe { self.context.program_uniform_4_f32_slice(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_matrix_2_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.program_uniform_matrix_2_f32_slice(program, location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_matrix_2x3_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.program_uniform_matrix_2x3_f32_slice(program, location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_matrix_2x4_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.program_uniform_matrix_2x4_f32_slice(program, location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_matrix_3x2_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.program_uniform_matrix_3x2_f32_slice(program, location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_matrix_3_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.program_uniform_matrix_3_f32_slice(program, location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_matrix_3x4_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.program_uniform_matrix_3x4_f32_slice(program, location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_matrix_4x2_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.program_uniform_matrix_4x2_f32_slice(program, location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_matrix_4x3_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.program_uniform_matrix_4x3_f32_slice(program, location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_uniform_matrix_4_f32_slice(&self, program: glow::Program, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.program_uniform_matrix_4_f32_slice(program, location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `value`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn program_binary_retrievable_hint(&self, program: glow::Program, value: bool) {
		unsafe { self.context.program_binary_retrievable_hint(program, value) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_program_binary(&self, program: glow::Program) -> Option<glow::ProgramBinary> {
		unsafe { self.context.get_program_binary(program) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_active_uniforms(&self, program: glow::Program) -> u32 {
		unsafe { self.context.get_active_uniforms(program) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `uniforms`: glow 의 같은 인자.
	/// - `pname`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[doc(alias = "GetActiveUniformsiv")]
	#[inline]
	pub unsafe fn get_active_uniforms_parameter(&self, program: glow::Program, uniforms: &[u32], pname: u32) -> Vec<i32> {
		unsafe { self.context.get_active_uniforms_parameter(program, uniforms, pname) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_active_uniform(&self, program: glow::Program, index: u32) -> Option<glow::ActiveUniform> {
		unsafe { self.context.get_active_uniform(program, index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `buffer`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn is_buffer(&self, buffer: glow::Buffer) -> bool {
		unsafe { self.context.is_buffer(buffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `buffer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn bind_buffer_base(&self, target: u32, index: u32, buffer: Option<glow::Buffer>) {
		unsafe { self.context.bind_buffer_base(target, index, buffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `buffer`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn bind_buffer_range(&self, target: u32, index: u32, buffer: Option<glow::Buffer>, offset: i32, size: i32) {
		unsafe { self.context.bind_buffer_range(target, index, buffer, offset, size) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `binding_index`: glow 의 같은 인자.
	/// - `buffer`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `stride`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn bind_vertex_buffer(&self, binding_index: u32, buffer: Option<glow::Buffer>, offset: i32, stride: i32) {
		unsafe { self.context.bind_vertex_buffer(binding_index, buffer, offset, stride) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `renderbuffer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn bind_renderbuffer(&self, target: u32, renderbuffer: Option<glow::Renderbuffer>) {
		unsafe { self.context.bind_renderbuffer(target, renderbuffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `src_x0`: glow 의 같은 인자.
	/// - `src_y0`: glow 의 같은 인자.
	/// - `src_x1`: glow 의 같은 인자.
	/// - `src_y1`: glow 의 같은 인자.
	/// - `dst_x0`: glow 의 같은 인자.
	/// - `dst_y0`: glow 의 같은 인자.
	/// - `dst_x1`: glow 의 같은 인자.
	/// - `dst_y1`: glow 의 같은 인자.
	/// - `mask`: glow 의 같은 인자.
	/// - `filter`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn blit_framebuffer(&self, src_x0: i32, src_y0: i32, src_x1: i32, src_y1: i32, dst_x0: i32, dst_y0: i32, dst_x1: i32, dst_y1: i32, mask: u32, filter: u32) {
		unsafe { self.context.blit_framebuffer(src_x0, src_y0, src_x1, src_y1, dst_x0, dst_y0, dst_x1, dst_y1, mask, filter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `read_buffer`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	/// - `src_x0`: glow 의 같은 인자.
	/// - `src_y0`: glow 의 같은 인자.
	/// - `src_x1`: glow 의 같은 인자.
	/// - `src_y1`: glow 의 같은 인자.
	/// - `dst_x0`: glow 의 같은 인자.
	/// - `dst_y0`: glow 의 같은 인자.
	/// - `dst_x1`: glow 의 같은 인자.
	/// - `dst_y1`: glow 의 같은 인자.
	/// - `mask`: glow 의 같은 인자.
	/// - `filter`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn blit_named_framebuffer(&self, read_buffer: Option<glow::Framebuffer>, draw_buffer: Option<glow::Framebuffer>, src_x0: i32, src_y0: i32, src_x1: i32, src_y1: i32, dst_x0: i32, dst_y0: i32, dst_x1: i32, dst_y1: i32, mask: u32, filter: u32) {
		unsafe { self.context.blit_named_framebuffer(read_buffer, draw_buffer, src_x0, src_y0, src_x1, src_y1, dst_x0, dst_y0, dst_x1, dst_y1, mask, filter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn supports_f64_precision(&self) -> bool {
		unsafe { self.context.supports_f64_precision() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `depth`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_depth_f64(&self, depth: f64) {
		unsafe { self.context.clear_depth_f64(depth) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `depth`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_depth_f32(&self, depth: f32) {
		unsafe { self.context.clear_depth_f32(depth) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `depth`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_depth(&self, depth: f64) {
		unsafe { self.context.clear_depth(depth) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `stencil`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_stencil(&self, stencil: i32) {
		unsafe { self.context.clear_stencil(stencil) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mask`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear(&self, mask: u32) {
		unsafe { self.context.clear(mask) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	/// - `value`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn patch_parameter_i32(&self, parameter: u32, value: i32) {
		unsafe { self.context.patch_parameter_i32(parameter, value) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_frag_data_location(&self, program: glow::Program, name: &str) -> i32 {
		unsafe { self.context.get_frag_data_location(program, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `color_number`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn bind_frag_data_location(&self, program: glow::Program, color_number: u32, name: &str) {
		unsafe { self.context.bind_frag_data_location(program, color_number, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `usage`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn buffer_data_size(&self, target: u32, size: i32, usage: u32) {
		unsafe { self.context.buffer_data_size(target, size, usage) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `buffer`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `usage`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_buffer_data_size(&self, buffer: glow::Buffer, size: i32, usage: u32) {
		unsafe { self.context.named_buffer_data_size(buffer, size, usage) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `data`: glow 의 같은 인자.
	/// - `usage`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn buffer_data_u8_slice(&self, target: u32, data: &[u8], usage: u32) {
		unsafe { self.context.buffer_data_u8_slice(target, data, usage) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `buffer`: glow 의 같은 인자.
	/// - `data`: glow 의 같은 인자.
	/// - `usage`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_buffer_data_u8_slice(&self, buffer: glow::Buffer, data: &[u8], usage: u32) {
		unsafe { self.context.named_buffer_data_u8_slice(buffer, data, usage) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `src_data`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn buffer_sub_data_u8_slice(&self, target: u32, offset: i32, src_data: &[u8]) {
		unsafe { self.context.buffer_sub_data_u8_slice(target, offset, src_data) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `buffer`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `src_data`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_buffer_sub_data_u8_slice(&self, buffer: glow::Buffer, offset: i32, src_data: &[u8]) {
		unsafe { self.context.named_buffer_sub_data_u8_slice(buffer, offset, src_data) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `dst_data`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_buffer_sub_data(&self, target: u32, offset: i32, dst_data: &mut [u8]) {
		unsafe { self.context.get_buffer_sub_data(target, offset, dst_data) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `buffer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_buffer(&self, target: u32, internal_format: u32, buffer: Option<glow::Buffer>) {
		unsafe { self.context.tex_buffer(target, internal_format, buffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data`: glow 의 같은 인자.
	/// - `flags`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn buffer_storage(&self, target: u32, size: i32, data: Option<&[u8]>, flags: u32) {
		unsafe { self.context.buffer_storage(target, size, data, flags) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data`: glow 의 같은 인자.
	/// - `flags`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_buffer_storage(&self, target: glow::Buffer, size: i32, data: Option<&[u8]>, flags: u32) {
		unsafe { self.context.named_buffer_storage(target, size, data, flags) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn check_framebuffer_status(&self, target: u32) -> u32 {
		unsafe { self.context.check_framebuffer_status(target) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `target`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn check_named_framebuffer_status(&self, framebuffer: Option<glow::Framebuffer>, target: u32) -> u32 {
		unsafe { self.context.check_named_framebuffer_status(framebuffer, target) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	/// - `values`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_buffer_i32_slice(&self, target: u32, draw_buffer: u32, values: &[i32]) {
		unsafe { self.context.clear_buffer_i32_slice(target, draw_buffer, values) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	/// - `values`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_buffer_u32_slice(&self, target: u32, draw_buffer: u32, values: &[u32]) {
		unsafe { self.context.clear_buffer_u32_slice(target, draw_buffer, values) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	/// - `values`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_buffer_f32_slice(&self, target: u32, draw_buffer: u32, values: &[f32]) {
		unsafe { self.context.clear_buffer_f32_slice(target, draw_buffer, values) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `stencil`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_buffer_depth_stencil(&self, target: u32, draw_buffer: u32, depth: f32, stencil: i32) {
		unsafe { self.context.clear_buffer_depth_stencil(target, draw_buffer, depth, stencil) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `target`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	/// - `values`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_named_framebuffer_i32_slice(&self, framebuffer: Option<glow::Framebuffer>, target: u32, draw_buffer: u32, values: &[i32]) {
		unsafe { self.context.clear_named_framebuffer_i32_slice(framebuffer, target, draw_buffer, values) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `target`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	/// - `values`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_named_framebuffer_u32_slice(&self, framebuffer: Option<glow::Framebuffer>, target: u32, draw_buffer: u32, values: &[u32]) {
		unsafe { self.context.clear_named_framebuffer_u32_slice(framebuffer, target, draw_buffer, values) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `target`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	/// - `values`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_named_framebuffer_f32_slice(&self, framebuffer: Option<glow::Framebuffer>, target: u32, draw_buffer: u32, values: &[f32]) {
		unsafe { self.context.clear_named_framebuffer_f32_slice(framebuffer, target, draw_buffer, values) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `target`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `stencil`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn clear_named_framebuffer_depth_stencil(&self, framebuffer: Option<glow::Framebuffer>, target: u32, draw_buffer: u32, depth: f32, stencil: i32) {
		unsafe { self.context.clear_named_framebuffer_depth_stencil(framebuffer, target, draw_buffer, depth, stencil) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `fence`: glow 의 같은 인자.
	/// - `flags`: glow 의 같은 인자.
	/// - `timeout`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn client_wait_sync(&self, fence: glow::Fence, flags: u32, timeout: i32) -> u32 {
		unsafe { self.context.client_wait_sync(fence, flags, timeout) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `fence`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_sync_parameter_i32(&self, fence: glow::Fence, parameter: u32) -> i32 {
		unsafe { self.context.get_sync_parameter_i32(fence, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `fence`: glow 의 같은 인자.
	/// - `flags`: glow 의 같은 인자.
	/// - `timeout`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn wait_sync(&self, fence: glow::Fence, flags: u32, timeout: u64) {
		unsafe { self.context.wait_sync(fence, flags, timeout) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `src_target`: glow 의 같은 인자.
	/// - `dst_target`: glow 의 같은 인자.
	/// - `src_offset`: glow 의 같은 인자.
	/// - `dst_offset`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn copy_buffer_sub_data(&self, src_target: u32, dst_target: u32, src_offset: i32, dst_offset: i32, size: i32) {
		unsafe { self.context.copy_buffer_sub_data(src_target, dst_target, src_offset, dst_offset, size) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `src_name`: glow 의 같은 인자.
	/// - `src_target`: glow 의 같은 인자.
	/// - `src_level`: glow 의 같은 인자.
	/// - `src_x`: glow 의 같은 인자.
	/// - `src_y`: glow 의 같은 인자.
	/// - `src_z`: glow 의 같은 인자.
	/// - `dst_name`: glow 의 같은 인자.
	/// - `dst_target`: glow 의 같은 인자.
	/// - `dst_level`: glow 의 같은 인자.
	/// - `dst_x`: glow 의 같은 인자.
	/// - `dst_y`: glow 의 같은 인자.
	/// - `dst_z`: glow 의 같은 인자.
	/// - `src_width`: glow 의 같은 인자.
	/// - `src_height`: glow 의 같은 인자.
	/// - `src_depth`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn copy_image_sub_data(&self, src_name: glow::Texture, src_target: u32, src_level: i32, src_x: i32, src_y: i32, src_z: i32, dst_name: glow::Texture, dst_target: u32, dst_level: i32, dst_x: i32, dst_y: i32, dst_z: i32, src_width: i32, src_height: i32, src_depth: i32) {
		unsafe { self.context.copy_image_sub_data(src_name, src_target, src_level, src_x, src_y, src_z, dst_name, dst_target, dst_level, dst_x, dst_y, dst_z, src_width, src_height, src_depth) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn copy_tex_image_2d(&self, target: u32, level: i32, internal_format: u32, x: i32, y: i32, width: i32, height: i32, border: i32) {
		unsafe { self.context.copy_tex_image_2d(target, level, internal_format, x, y, width, height, border) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn copy_tex_sub_image_2d(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, x: i32, y: i32, width: i32, height: i32) {
		unsafe { self.context.copy_tex_sub_image_2d(target, level, x_offset, y_offset, x, y, width, height) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn copy_tex_sub_image_3d(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, x: i32, y: i32, width: i32, height: i32) {
		unsafe { self.context.copy_tex_sub_image_3d(target, level, x_offset, y_offset, z_offset, x, y, width, height) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `query`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn delete_query(&self, query: glow::Query) {
		unsafe { self.context.delete_query(query) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `renderbuffer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn delete_renderbuffer(&self, renderbuffer: glow::Renderbuffer) {
		unsafe { self.context.delete_renderbuffer(renderbuffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn delete_sampler(&self, texture: glow::Sampler) {
		unsafe { self.context.delete_sampler(texture) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `fence`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn delete_sync(&self, fence: glow::Fence) {
		unsafe { self.context.delete_sync(fence) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn disable_vertex_attrib_array(&self, index: u32) {
		unsafe { self.context.disable_vertex_attrib_array(index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `groups_x`: glow 의 같은 인자.
	/// - `groups_y`: glow 의 같은 인자.
	/// - `groups_z`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn dispatch_compute(&self, groups_x: u32, groups_y: u32, groups_z: u32) {
		unsafe { self.context.dispatch_compute(groups_x, groups_y, groups_z) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn dispatch_compute_indirect(&self, offset: i32) {
		unsafe { self.context.dispatch_compute_indirect(offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `first`: glow 의 같은 인자.
	/// - `count`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_arrays(&self, mode: u32, first: i32, count: i32) {
		unsafe { self.context.draw_arrays(mode, first, count) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `first`: glow 의 같은 인자.
	/// - `count`: glow 의 같은 인자.
	/// - `instance_count`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_arrays_instanced(&self, mode: u32, first: i32, count: i32, instance_count: i32) {
		unsafe { self.context.draw_arrays_instanced(mode, first, count, instance_count) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `first`: glow 의 같은 인자.
	/// - `count`: glow 의 같은 인자.
	/// - `instance_count`: glow 의 같은 인자.
	/// - `base_instance`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_arrays_instanced_base_instance(&self, mode: u32, first: i32, count: i32, instance_count: i32, base_instance: u32) {
		unsafe { self.context.draw_arrays_instanced_base_instance(mode, first, count, instance_count, base_instance) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_arrays_indirect_offset(&self, mode: u32, offset: i32) {
		unsafe { self.context.draw_arrays_indirect_offset(mode, offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `buffer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_buffer(&self, buffer: u32) {
		unsafe { self.context.draw_buffer(buffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `draw_buffer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_framebuffer_draw_buffer(&self, framebuffer: Option<glow::Framebuffer>, draw_buffer: u32) {
		unsafe { self.context.named_framebuffer_draw_buffer(framebuffer, draw_buffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `buffers`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_framebuffer_draw_buffers(&self, framebuffer: Option<glow::Framebuffer>, buffers: &[u32]) {
		unsafe { self.context.named_framebuffer_draw_buffers(framebuffer, buffers) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `buffers`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_buffers(&self, buffers: &[u32]) {
		unsafe { self.context.draw_buffers(buffers) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `count`: glow 의 같은 인자.
	/// - `element_type`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_elements(&self, mode: u32, count: i32, element_type: u32, offset: i32) {
		unsafe { self.context.draw_elements(mode, count, element_type, offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `count`: glow 의 같은 인자.
	/// - `element_type`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `base_vertex`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_elements_base_vertex(&self, mode: u32, count: i32, element_type: u32, offset: i32, base_vertex: i32) {
		unsafe { self.context.draw_elements_base_vertex(mode, count, element_type, offset, base_vertex) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `count`: glow 의 같은 인자.
	/// - `element_type`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `instance_count`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_elements_instanced(&self, mode: u32, count: i32, element_type: u32, offset: i32, instance_count: i32) {
		unsafe { self.context.draw_elements_instanced(mode, count, element_type, offset, instance_count) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `count`: glow 의 같은 인자.
	/// - `element_type`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `instance_count`: glow 의 같은 인자.
	/// - `base_vertex`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_elements_instanced_base_vertex(&self, mode: u32, count: i32, element_type: u32, offset: i32, instance_count: i32, base_vertex: i32) {
		unsafe { self.context.draw_elements_instanced_base_vertex(mode, count, element_type, offset, instance_count, base_vertex) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `count`: glow 의 같은 인자.
	/// - `element_type`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `instance_count`: glow 의 같은 인자.
	/// - `base_vertex`: glow 의 같은 인자.
	/// - `base_instance`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_elements_instanced_base_vertex_base_instance(&self, mode: u32, count: i32, element_type: u32, offset: i32, instance_count: i32, base_vertex: i32, base_instance: u32) {
		unsafe { self.context.draw_elements_instanced_base_vertex_base_instance(mode, count, element_type, offset, instance_count, base_vertex, base_instance) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mode`: glow 의 같은 인자.
	/// - `element_type`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn draw_elements_indirect_offset(&self, mode: u32, element_type: u32, offset: i32) {
		unsafe { self.context.draw_elements_indirect_offset(mode, element_type, offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `vao`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn enable_vertex_array_attrib(&self, vao: glow::VertexArray, index: u32) {
		unsafe { self.context.enable_vertex_array_attrib(vao, index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn enable_vertex_attrib_array(&self, index: u32) {
		unsafe { self.context.enable_vertex_attrib_array(index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	//==============================================================================
	#[inline]
	pub unsafe fn flush(&self) {
		unsafe { self.context.flush() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `renderbuffer_target`: glow 의 같은 인자.
	/// - `renderbuffer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn framebuffer_renderbuffer(&self, target: u32, attachment: u32, renderbuffer_target: u32, renderbuffer: Option<glow::Renderbuffer>) {
		unsafe { self.context.framebuffer_renderbuffer(target, attachment, renderbuffer_target, renderbuffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn framebuffer_texture(&self, target: u32, attachment: u32, texture: Option<glow::Texture>, level: i32) {
		unsafe { self.context.framebuffer_texture(target, attachment, texture, level) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `texture_target`: glow 의 같은 인자.
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn framebuffer_texture_2d(&self, target: u32, attachment: u32, texture_target: u32, texture: Option<glow::Texture>, level: i32) {
		unsafe { self.context.framebuffer_texture_2d(target, attachment, texture_target, texture, level) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `texture_target`: glow 의 같은 인자.
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `samples`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn framebuffer_texture_2d_multisample(&self, target: u32, attachment: u32, texture_target: u32, texture: Option<glow::Texture>, level: i32, samples: i32) {
		unsafe { self.context.framebuffer_texture_2d_multisample(target, attachment, texture_target, texture, level, samples) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `texture_target`: glow 의 같은 인자.
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `layer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn framebuffer_texture_3d(&self, target: u32, attachment: u32, texture_target: u32, texture: Option<glow::Texture>, level: i32, layer: i32) {
		unsafe { self.context.framebuffer_texture_3d(target, attachment, texture_target, texture, level, layer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `layer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn framebuffer_texture_layer(&self, target: u32, attachment: u32, texture: Option<glow::Texture>, level: i32, layer: i32) {
		unsafe { self.context.framebuffer_texture_layer(target, attachment, texture, level, layer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `renderbuffer_target`: glow 의 같은 인자.
	/// - `renderbuffer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_framebuffer_renderbuffer(&self, framebuffer: Option<glow::Framebuffer>, attachment: u32, renderbuffer_target: u32, renderbuffer: Option<glow::Renderbuffer>) {
		unsafe { self.context.named_framebuffer_renderbuffer(framebuffer, attachment, renderbuffer_target, renderbuffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_framebuffer_texture(&self, framebuffer: Option<glow::Framebuffer>, attachment: u32, texture: Option<glow::Texture>, level: i32) {
		unsafe { self.context.named_framebuffer_texture(framebuffer, attachment, texture, level) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `layer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_framebuffer_texture_layer(&self, framebuffer: Option<glow::Framebuffer>, attachment: u32, texture: Option<glow::Texture>, level: i32, layer: i32) {
		unsafe { self.context.named_framebuffer_texture_layer(framebuffer, attachment, texture, level, layer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_error(&self) -> u32 {
		unsafe { self.context.get_error() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_tex_parameter_i32(&self, target: u32, parameter: u32) -> i32 {
		unsafe { self.context.get_tex_parameter_i32(target, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_tex_parameter_f32(&self, target: u32, parameter: u32) -> f32 {
		unsafe { self.context.get_tex_parameter_f32(target, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_texture_level_parameter_i32(&self, texture: glow::Texture, level: i32, parameter: u32) -> i32 {
		unsafe { self.context.get_texture_level_parameter_i32(texture, level, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_texture_level_parameter_f32(&self, texture: glow::Texture, level: i32, parameter: u32) -> f32 {
		unsafe { self.context.get_texture_level_parameter_f32(texture, level, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_tex_level_parameter_i32(&self, target: u32, level: i32, parameter: u32) -> i32 {
		unsafe { self.context.get_tex_level_parameter_i32(target, level, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_tex_level_parameter_f32(&self, target: u32, level: i32, parameter: u32) -> f32 {
		unsafe { self.context.get_tex_level_parameter_f32(target, level, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_buffer_parameter_i32(&self, target: u32, parameter: u32) -> i32 {
		unsafe { self.context.get_buffer_parameter_i32(target, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[doc(alias = "glGetInteger64v")]
	#[inline]
	pub unsafe fn get_parameter_i64(&self, parameter: u32) -> i64 {
		unsafe { self.context.get_parameter_i64(parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	/// - `out`: glow 의 같은 인자.
	//==============================================================================
	#[doc(alias = "glGetInteger64v")]
	#[inline]
	pub unsafe fn get_parameter_i64_slice(&self, parameter: u32, out: &mut [i64]) {
		unsafe { self.context.get_parameter_i64_slice(parameter, out) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[doc(alias = "glGetInteger64i_v")]
	#[inline]
	pub unsafe fn get_parameter_indexed_i64(&self, parameter: u32, index: u32) -> i64 {
		unsafe { self.context.get_parameter_indexed_i64(parameter, index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[doc(alias = "glGetFloatv")]
	#[inline]
	pub unsafe fn get_parameter_f32(&self, parameter: u32) -> f32 {
		unsafe { self.context.get_parameter_f32(parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[doc(alias = "glGetIntegeri_v")]
	#[inline]
	pub unsafe fn get_parameter_indexed_i32(&self, parameter: u32, index: u32) -> i32 {
		unsafe { self.context.get_parameter_indexed_i32(parameter, index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[doc(alias = "glGetStringi")]
	#[inline]
	pub unsafe fn get_parameter_indexed_string(&self, parameter: u32, index: u32) -> String {
		unsafe { self.context.get_parameter_indexed_string(parameter, index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[doc(alias = "glGetString")]
	#[inline]
	pub unsafe fn get_parameter_string(&self, parameter: u32) -> String {
		unsafe { self.context.get_parameter_string(parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_parameter_buffer(&self, parameter: u32) -> Option<glow::Buffer> {
		unsafe { self.context.get_parameter_buffer(parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_parameter_renderbuffer(&self, parameter: u32) -> Option<glow::Renderbuffer> {
		unsafe { self.context.get_parameter_renderbuffer(parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_parameter_sampler(&self, parameter: u32) -> Option<glow::Sampler> {
		unsafe { self.context.get_parameter_sampler(parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_parameter_transform_feedback(&self, parameter: u32) -> Option<glow::TransformFeedback> {
		unsafe { self.context.get_parameter_transform_feedback(parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_renderbuffer_parameter_i32(&self, target: u32, parameter: u32) -> i32 {
		unsafe { self.context.get_renderbuffer_parameter_i32(target, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_framebuffer_parameter_i32(&self, target: u32, parameter: u32) -> i32 {
		unsafe { self.context.get_framebuffer_parameter_i32(target, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_named_framebuffer_parameter_i32(&self, framebuffer: Option<glow::Framebuffer>, parameter: u32) -> i32 {
		unsafe { self.context.get_named_framebuffer_parameter_i32(framebuffer, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_framebuffer_attachment_parameter_i32(&self, target: u32, attachment: u32, parameter: u32) -> i32 {
		unsafe { self.context.get_framebuffer_attachment_parameter_i32(target, attachment, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_named_framebuffer_attachment_parameter_i32(&self, framebuffer: Option<glow::Framebuffer>, attachment: u32, parameter: u32) -> i32 {
		unsafe { self.context.get_named_framebuffer_attachment_parameter_i32(framebuffer, attachment, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `uniform_block_index`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_active_uniform_block_parameter_i32(&self, program: glow::Program, uniform_block_index: u32, parameter: u32) -> i32 {
		unsafe { self.context.get_active_uniform_block_parameter_i32(program, uniform_block_index, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `uniform_block_index`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	/// - `out`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_active_uniform_block_parameter_i32_slice(&self, program: glow::Program, uniform_block_index: u32, parameter: u32, out: &mut [i32]) {
		unsafe { self.context.get_active_uniform_block_parameter_i32_slice(program, uniform_block_index, parameter, out) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `uniform_block_index`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_active_uniform_block_name(&self, program: glow::Program, uniform_block_index: u32) -> String {
		unsafe { self.context.get_active_uniform_block_name(program, uniform_block_index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_uniform_location(&self, program: glow::Program, name: &str) -> Option<glow::UniformLocation> {
		unsafe { self.context.get_uniform_location(program, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_attrib_location(&self, program: glow::Program, name: &str) -> Option<u32> {
		unsafe { self.context.get_attrib_location(program, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn bind_attrib_location(&self, program: glow::Program, index: u32, name: &str) {
		unsafe { self.context.bind_attrib_location(program, index, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_active_attributes(&self, program: glow::Program) -> u32 {
		unsafe { self.context.get_active_attributes(program) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_active_attribute(&self, program: glow::Program, index: u32) -> Option<glow::ActiveAttribute> {
		unsafe { self.context.get_active_attribute(program, index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `fence`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_sync_status(&self, fence: glow::Fence) -> u32 {
		unsafe { self.context.get_sync_status(fence) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `fence`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn is_sync(&self, fence: glow::Fence) -> bool {
		unsafe { self.context.is_sync(fence) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn renderbuffer_storage(&self, target: u32, internal_format: u32, width: i32, height: i32) {
		unsafe { self.context.renderbuffer_storage(target, internal_format, width, height) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `samples`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn renderbuffer_storage_multisample(&self, target: u32, samples: i32, internal_format: u32, width: i32, height: i32) {
		unsafe { self.context.renderbuffer_storage_multisample(target, samples, internal_format, width, height) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `sampler`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	/// - `value`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn sampler_parameter_f32(&self, sampler: glow::Sampler, name: u32, value: f32) {
		unsafe { self.context.sampler_parameter_f32(sampler, name, value) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `sampler`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	/// - `value`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn sampler_parameter_f32_slice(&self, sampler: glow::Sampler, name: u32, value: &[f32]) {
		unsafe { self.context.sampler_parameter_f32_slice(sampler, name, value) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `sampler`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	/// - `value`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn sampler_parameter_i32(&self, sampler: glow::Sampler, name: u32, value: i32) {
		unsafe { self.context.sampler_parameter_i32(sampler, name, value) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `sampler`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_sampler_parameter_i32(&self, sampler: glow::Sampler, name: u32) -> i32 {
		unsafe { self.context.get_sampler_parameter_i32(sampler, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `sampler`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_sampler_parameter_f32(&self, sampler: glow::Sampler, name: u32) -> f32 {
		unsafe { self.context.get_sampler_parameter_f32(sampler, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `sampler`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	/// - `out`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_sampler_parameter_f32_slice(&self, sampler: glow::Sampler, name: u32, out: &mut [f32]) {
		unsafe { self.context.get_sampler_parameter_f32_slice(sampler, name, out) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn generate_mipmap(&self, target: u32) {
		unsafe { self.context.generate_mipmap(target) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn generate_texture_mipmap(&self, texture: glow::Texture) {
		unsafe { self.context.generate_texture_mipmap(texture) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_image_1d(&self, target: u32, level: i32, internal_format: i32, width: i32, border: i32, format: u32, ty: u32, pixels: glow::PixelUnpackData) {
		unsafe { self.context.tex_image_1d(target, level, internal_format, width, border, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `image_size`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn compressed_tex_image_1d(&self, target: u32, level: i32, internal_format: i32, width: i32, border: i32, image_size: i32, pixels: &[u8]) {
		unsafe { self.context.compressed_tex_image_1d(target, level, internal_format, width, border, image_size, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_image_2d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, border: i32, format: u32, ty: u32, pixels: glow::PixelUnpackData) {
		unsafe { self.context.tex_image_2d(target, level, internal_format, width, height, border, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `samples`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `fixed_sample_locations`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_image_2d_multisample(&self, target: u32, samples: i32, internal_format: i32, width: i32, height: i32, fixed_sample_locations: bool) {
		unsafe { self.context.tex_image_2d_multisample(target, samples, internal_format, width, height, fixed_sample_locations) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `image_size`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn compressed_tex_image_2d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, border: i32, image_size: i32, pixels: &[u8]) {
		unsafe { self.context.compressed_tex_image_2d(target, level, internal_format, width, height, border, image_size, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_image_3d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, pixels: glow::PixelUnpackData) {
		unsafe { self.context.tex_image_3d(target, level, internal_format, width, height, depth, border, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `image_size`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn compressed_tex_image_3d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, image_size: i32, pixels: &[u8]) {
		unsafe { self.context.compressed_tex_image_3d(target, level, internal_format, width, height, depth, border, image_size, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `levels`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_storage_1d(&self, target: u32, levels: i32, internal_format: u32, width: i32) {
		unsafe { self.context.tex_storage_1d(target, levels, internal_format, width) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `levels`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_storage_2d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32) {
		unsafe { self.context.tex_storage_2d(target, levels, internal_format, width, height) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	/// - `levels`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn texture_storage_2d(&self, texture: glow::Texture, levels: i32, internal_format: u32, width: i32, height: i32) {
		unsafe { self.context.texture_storage_2d(texture, levels, internal_format, width, height) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `samples`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `fixed_sample_locations`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_storage_2d_multisample(&self, target: u32, samples: i32, internal_format: u32, width: i32, height: i32, fixed_sample_locations: bool) {
		unsafe { self.context.tex_storage_2d_multisample(target, samples, internal_format, width, height, fixed_sample_locations) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `levels`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_storage_3d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32, depth: i32) {
		unsafe { self.context.tex_storage_3d(target, levels, internal_format, width, height, depth) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	/// - `levels`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn texture_storage_3d(&self, texture: glow::Texture, levels: i32, internal_format: u32, width: i32, height: i32, depth: i32) {
		unsafe { self.context.texture_storage_3d(texture, levels, internal_format, width, height, depth) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_uniform_i32(&self, program: glow::Program, location: &glow::UniformLocation, v: &mut [i32]) {
		unsafe { self.context.get_uniform_i32(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_uniform_u32(&self, program: glow::Program, location: &glow::UniformLocation, v: &mut [u32]) {
		unsafe { self.context.get_uniform_u32(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_uniform_f32(&self, program: glow::Program, location: &glow::UniformLocation, v: &mut [f32]) {
		unsafe { self.context.get_uniform_f32(program, location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_1_i32(&self, location: Option<&glow::UniformLocation>, x: i32) {
		unsafe { self.context.uniform_1_i32(location, x) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_2_i32(&self, location: Option<&glow::UniformLocation>, x: i32, y: i32) {
		unsafe { self.context.uniform_2_i32(location, x, y) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_3_i32(&self, location: Option<&glow::UniformLocation>, x: i32, y: i32, z: i32) {
		unsafe { self.context.uniform_3_i32(location, x, y, z) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	/// - `w`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_4_i32(&self, location: Option<&glow::UniformLocation>, x: i32, y: i32, z: i32, w: i32) {
		unsafe { self.context.uniform_4_i32(location, x, y, z, w) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_1_i32_slice(&self, location: Option<&glow::UniformLocation>, v: &[i32]) {
		unsafe { self.context.uniform_1_i32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_2_i32_slice(&self, location: Option<&glow::UniformLocation>, v: &[i32]) {
		unsafe { self.context.uniform_2_i32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_3_i32_slice(&self, location: Option<&glow::UniformLocation>, v: &[i32]) {
		unsafe { self.context.uniform_3_i32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_4_i32_slice(&self, location: Option<&glow::UniformLocation>, v: &[i32]) {
		unsafe { self.context.uniform_4_i32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_1_u32(&self, location: Option<&glow::UniformLocation>, x: u32) {
		unsafe { self.context.uniform_1_u32(location, x) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_2_u32(&self, location: Option<&glow::UniformLocation>, x: u32, y: u32) {
		unsafe { self.context.uniform_2_u32(location, x, y) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_3_u32(&self, location: Option<&glow::UniformLocation>, x: u32, y: u32, z: u32) {
		unsafe { self.context.uniform_3_u32(location, x, y, z) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	/// - `w`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_4_u32(&self, location: Option<&glow::UniformLocation>, x: u32, y: u32, z: u32, w: u32) {
		unsafe { self.context.uniform_4_u32(location, x, y, z, w) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_1_u32_slice(&self, location: Option<&glow::UniformLocation>, v: &[u32]) {
		unsafe { self.context.uniform_1_u32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_2_u32_slice(&self, location: Option<&glow::UniformLocation>, v: &[u32]) {
		unsafe { self.context.uniform_2_u32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_3_u32_slice(&self, location: Option<&glow::UniformLocation>, v: &[u32]) {
		unsafe { self.context.uniform_3_u32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_4_u32_slice(&self, location: Option<&glow::UniformLocation>, v: &[u32]) {
		unsafe { self.context.uniform_4_u32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_1_f32(&self, location: Option<&glow::UniformLocation>, x: f32) {
		unsafe { self.context.uniform_1_f32(location, x) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_2_f32(&self, location: Option<&glow::UniformLocation>, x: f32, y: f32) {
		unsafe { self.context.uniform_2_f32(location, x, y) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_3_f32(&self, location: Option<&glow::UniformLocation>, x: f32, y: f32, z: f32) {
		unsafe { self.context.uniform_3_f32(location, x, y, z) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	/// - `w`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_4_f32(&self, location: Option<&glow::UniformLocation>, x: f32, y: f32, z: f32, w: f32) {
		unsafe { self.context.uniform_4_f32(location, x, y, z, w) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_1_f32_slice(&self, location: Option<&glow::UniformLocation>, v: &[f32]) {
		unsafe { self.context.uniform_1_f32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_2_f32_slice(&self, location: Option<&glow::UniformLocation>, v: &[f32]) {
		unsafe { self.context.uniform_2_f32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_3_f32_slice(&self, location: Option<&glow::UniformLocation>, v: &[f32]) {
		unsafe { self.context.uniform_3_f32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_4_f32_slice(&self, location: Option<&glow::UniformLocation>, v: &[f32]) {
		unsafe { self.context.uniform_4_f32_slice(location, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_matrix_2_f32_slice(&self, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.uniform_matrix_2_f32_slice(location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_matrix_2x3_f32_slice(&self, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.uniform_matrix_2x3_f32_slice(location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_matrix_2x4_f32_slice(&self, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.uniform_matrix_2x4_f32_slice(location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_matrix_3x2_f32_slice(&self, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.uniform_matrix_3x2_f32_slice(location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_matrix_3_f32_slice(&self, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.uniform_matrix_3_f32_slice(location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_matrix_3x4_f32_slice(&self, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.uniform_matrix_3x4_f32_slice(location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_matrix_4x2_f32_slice(&self, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.uniform_matrix_4x2_f32_slice(location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_matrix_4x3_f32_slice(&self, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.uniform_matrix_4x3_f32_slice(location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `location`: glow 의 같은 인자.
	/// - `transpose`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_matrix_4_f32_slice(&self, location: Option<&glow::UniformLocation>, transpose: bool, v: &[f32]) {
		unsafe { self.context.uniform_matrix_4_f32_slice(location, transpose, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn unmap_buffer(&self, target: u32) {
		unsafe { self.context.unmap_buffer(target) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `red`: glow 의 같은 인자.
	/// - `green`: glow 의 같은 인자.
	/// - `blue`: glow 의 같은 인자.
	/// - `alpha`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn blend_color(&self, red: f32, green: f32, blue: f32, alpha: f32) {
		unsafe { self.context.blend_color(red, green, blue, alpha) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `width`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn line_width(&self, width: f32) {
		unsafe { self.context.line_width(width) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `length`: glow 의 같은 인자.
	/// - `access`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn map_buffer_range(&self, target: u32, offset: i32, length: i32, access: u32) -> *mut u8 {
		unsafe { self.context.map_buffer_range(target, offset, length, access) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `length`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn flush_mapped_buffer_range(&self, target: u32, offset: i32, length: i32) {
		unsafe { self.context.flush_mapped_buffer_range(target, offset, length) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `length`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn invalidate_buffer_sub_data(&self, target: u32, offset: i32, length: i32) {
		unsafe { self.context.invalidate_buffer_sub_data(target, offset, length) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachments`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn invalidate_framebuffer(&self, target: u32, attachments: &[u32]) {
		unsafe { self.context.invalidate_framebuffer(target, attachments) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachments`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn invalidate_sub_framebuffer(&self, target: u32, attachments: &[u32], x: i32, y: i32, width: i32, height: i32) {
		unsafe { self.context.invalidate_sub_framebuffer(target, attachments, x, y, width, height) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `factor`: glow 의 같은 인자.
	/// - `units`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn polygon_offset(&self, factor: f32, units: f32) {
		unsafe { self.context.polygon_offset(factor, units) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `face`: glow 의 같은 인자.
	/// - `mode`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn polygon_mode(&self, face: u32, mode: u32) {
		unsafe { self.context.polygon_mode(face, mode) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	//==============================================================================
	#[inline]
	pub unsafe fn finish(&self) {
		unsafe { self.context.finish() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `unit`: glow 의 같은 인자.
	/// - `sampler`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn bind_sampler(&self, unit: u32, sampler: Option<glow::Sampler>) {
		unsafe { self.context.bind_sampler(unit, sampler) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `condition`: glow 의 같은 인자.
	/// - `flags`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn fence_sync(&self, condition: u32, flags: u32) -> Result<glow::Fence, String> {
		unsafe { self.context.fence_sync(condition, flags) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	/// - `value`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_parameter_f32(&self, target: u32, parameter: u32, value: f32) {
		unsafe { self.context.tex_parameter_f32(target, parameter, value) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	/// - `value`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_parameter_i32(&self, target: u32, parameter: u32, value: i32) {
		unsafe { self.context.tex_parameter_i32(target, parameter, value) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	/// - `value`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn texture_parameter_i32(&self, texture: glow::Texture, parameter: u32, value: i32) {
		unsafe { self.context.texture_parameter_i32(texture, parameter, value) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	/// - `values`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_parameter_f32_slice(&self, target: u32, parameter: u32, values: &[f32]) {
		unsafe { self.context.tex_parameter_f32_slice(target, parameter, values) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	/// - `values`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_parameter_i32_slice(&self, target: u32, parameter: u32, values: &[i32]) {
		unsafe { self.context.tex_parameter_i32_slice(target, parameter, values) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_sub_image_2d(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, ty: u32, pixels: glow::PixelUnpackData) {
		unsafe { self.context.tex_sub_image_2d(target, level, x_offset, y_offset, width, height, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn texture_sub_image_2d(&self, texture: glow::Texture, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, ty: u32, pixels: glow::PixelUnpackData) {
		unsafe { self.context.texture_sub_image_2d(texture, level, x_offset, y_offset, width, height, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn compressed_tex_sub_image_2d(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, pixels: glow::CompressedPixelUnpackData) {
		unsafe { self.context.compressed_tex_sub_image_2d(target, level, x_offset, y_offset, width, height, format, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn tex_sub_image_3d(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, pixels: glow::PixelUnpackData) {
		unsafe { self.context.tex_sub_image_3d(target, level, x_offset, y_offset, z_offset, width, height, depth, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn texture_sub_image_3d(&self, texture: glow::Texture, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, pixels: glow::PixelUnpackData) {
		unsafe { self.context.texture_sub_image_3d(texture, level, x_offset, y_offset, z_offset, width, height, depth, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn compressed_tex_sub_image_3d(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, width: i32, height: i32, depth: i32, format: u32, pixels: glow::CompressedPixelUnpackData) {
		unsafe { self.context.compressed_tex_sub_image_3d(target, level, x_offset, y_offset, z_offset, width, height, depth, format, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `near`: glow 의 같은 인자.
	/// - `far`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn depth_range_f32(&self, near: f32, far: f32) {
		unsafe { self.context.depth_range_f32(near, far) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `near`: glow 의 같은 인자.
	/// - `far`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn depth_range_f64(&self, near: f64, far: f64) {
		unsafe { self.context.depth_range_f64(near, far) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `near`: glow 의 같은 인자.
	/// - `far`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn depth_range(&self, near: f64, far: f64) {
		unsafe { self.context.depth_range(near, far) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `first`: glow 의 같은 인자.
	/// - `count`: glow 의 같은 인자.
	/// - `values`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn depth_range_f64_slice(&self, first: u32, count: i32, values: &[[f64; 2]]) {
		unsafe { self.context.depth_range_f64_slice(first, count, values) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `vao`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `binding_index`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_array_attrib_binding_f32(&self, vao: glow::VertexArray, index: u32, binding_index: u32) {
		unsafe { self.context.vertex_array_attrib_binding_f32(vao, index, binding_index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `vao`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data_type`: glow 의 같은 인자.
	/// - `normalized`: glow 의 같은 인자.
	/// - `relative_offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_array_attrib_format_f32(&self, vao: glow::VertexArray, index: u32, size: i32, data_type: u32, normalized: bool, relative_offset: u32) {
		unsafe { self.context.vertex_array_attrib_format_f32(vao, index, size, data_type, normalized, relative_offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `vao`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data_type`: glow 의 같은 인자.
	/// - `relative_offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_array_attrib_format_i32(&self, vao: glow::VertexArray, index: u32, size: i32, data_type: u32, relative_offset: u32) {
		unsafe { self.context.vertex_array_attrib_format_i32(vao, index, size, data_type, relative_offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `vao`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data_type`: glow 의 같은 인자.
	/// - `relative_offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_array_attrib_format_f64(&self, vao: glow::VertexArray, index: u32, size: i32, data_type: u32, relative_offset: u32) {
		unsafe { self.context.vertex_array_attrib_format_f64(vao, index, size, data_type, relative_offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `vao`: glow 의 같은 인자.
	/// - `buffer`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_array_element_buffer(&self, vao: glow::VertexArray, buffer: Option<glow::Buffer>) {
		unsafe { self.context.vertex_array_element_buffer(vao, buffer) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `vao`: glow 의 같은 인자.
	/// - `binding_index`: glow 의 같은 인자.
	/// - `buffer`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	/// - `stride`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_array_vertex_buffer(&self, vao: glow::VertexArray, binding_index: u32, buffer: Option<glow::Buffer>, offset: i32, stride: i32) {
		unsafe { self.context.vertex_array_vertex_buffer(vao, binding_index, buffer, offset, stride) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `divisor`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_divisor(&self, index: u32, divisor: u32) {
		unsafe { self.context.vertex_attrib_divisor(index, divisor) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `pname`: glow 의 같은 인자.
	/// - `result`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_vertex_attrib_parameter_f32_slice(&self, index: u32, pname: u32, result: &mut [f32]) {
		unsafe { self.context.get_vertex_attrib_parameter_f32_slice(index, pname, result) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data_type`: glow 의 같은 인자.
	/// - `normalized`: glow 의 같은 인자.
	/// - `stride`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_pointer_f32(&self, index: u32, size: i32, data_type: u32, normalized: bool, stride: i32, offset: i32) {
		unsafe { self.context.vertex_attrib_pointer_f32(index, size, data_type, normalized, stride, offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data_type`: glow 의 같은 인자.
	/// - `stride`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_pointer_i32(&self, index: u32, size: i32, data_type: u32, stride: i32, offset: i32) {
		unsafe { self.context.vertex_attrib_pointer_i32(index, size, data_type, stride, offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data_type`: glow 의 같은 인자.
	/// - `stride`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_pointer_f64(&self, index: u32, size: i32, data_type: u32, stride: i32, offset: i32) {
		unsafe { self.context.vertex_attrib_pointer_f64(index, size, data_type, stride, offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data_type`: glow 의 같은 인자.
	/// - `normalized`: glow 의 같은 인자.
	/// - `relative_offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_format_f32(&self, index: u32, size: i32, data_type: u32, normalized: bool, relative_offset: u32) {
		unsafe { self.context.vertex_attrib_format_f32(index, size, data_type, normalized, relative_offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data_type`: glow 의 같은 인자.
	/// - `relative_offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_format_i32(&self, index: u32, size: i32, data_type: u32, relative_offset: u32) {
		unsafe { self.context.vertex_attrib_format_i32(index, size, data_type, relative_offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `size`: glow 의 같은 인자.
	/// - `data_type`: glow 의 같은 인자.
	/// - `relative_offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_format_f64(&self, index: u32, size: i32, data_type: u32, relative_offset: u32) {
		unsafe { self.context.vertex_attrib_format_f64(index, size, data_type, relative_offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_1_f32(&self, index: u32, x: f32) {
		unsafe { self.context.vertex_attrib_1_f32(index, x) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_2_f32(&self, index: u32, x: f32, y: f32) {
		unsafe { self.context.vertex_attrib_2_f32(index, x, y) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_3_f32(&self, index: u32, x: f32, y: f32, z: f32) {
		unsafe { self.context.vertex_attrib_3_f32(index, x, y, z) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	/// - `w`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_4_f32(&self, index: u32, x: f32, y: f32, z: f32, w: f32) {
		unsafe { self.context.vertex_attrib_4_f32(index, x, y, z, w) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	/// - `w`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_4_i32(&self, index: u32, x: i32, y: i32, z: i32, w: i32) {
		unsafe { self.context.vertex_attrib_4_i32(index, x, y, z, w) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `z`: glow 의 같은 인자.
	/// - `w`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_4_u32(&self, index: u32, x: u32, y: u32, z: u32, w: u32) {
		unsafe { self.context.vertex_attrib_4_u32(index, x, y, z, w) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_1_f32_slice(&self, index: u32, v: &[f32]) {
		unsafe { self.context.vertex_attrib_1_f32_slice(index, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_2_f32_slice(&self, index: u32, v: &[f32]) {
		unsafe { self.context.vertex_attrib_2_f32_slice(index, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_3_f32_slice(&self, index: u32, v: &[f32]) {
		unsafe { self.context.vertex_attrib_3_f32_slice(index, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `index`: glow 의 같은 인자.
	/// - `v`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_4_f32_slice(&self, index: u32, v: &[f32]) {
		unsafe { self.context.vertex_attrib_4_f32_slice(index, v) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `attrib_index`: glow 의 같은 인자.
	/// - `binding_index`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_attrib_binding(&self, attrib_index: u32, binding_index: u32) {
		unsafe { self.context.vertex_attrib_binding(attrib_index, binding_index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `binding_index`: glow 의 같은 인자.
	/// - `divisor`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn vertex_binding_divisor(&self, binding_index: u32, divisor: u32) {
		unsafe { self.context.vertex_binding_divisor(binding_index, divisor) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `func`: glow 의 같은 인자.
	/// - `reference`: glow 의 같은 인자.
	/// - `mask`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn stencil_func(&self, func: u32, reference: i32, mask: u32) {
		unsafe { self.context.stencil_func(func, reference, mask) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `face`: glow 의 같은 인자.
	/// - `func`: glow 의 같은 인자.
	/// - `reference`: glow 의 같은 인자.
	/// - `mask`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn stencil_func_separate(&self, face: u32, func: u32, reference: i32, mask: u32) {
		unsafe { self.context.stencil_func_separate(face, func, reference, mask) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `mask`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn stencil_mask(&self, mask: u32) {
		unsafe { self.context.stencil_mask(mask) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `face`: glow 의 같은 인자.
	/// - `mask`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn stencil_mask_separate(&self, face: u32, mask: u32) {
		unsafe { self.context.stencil_mask_separate(face, mask) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `stencil_fail`: glow 의 같은 인자.
	/// - `depth_fail`: glow 의 같은 인자.
	/// - `pass`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn stencil_op(&self, stencil_fail: u32, depth_fail: u32, pass: u32) {
		unsafe { self.context.stencil_op(stencil_fail, depth_fail, pass) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `face`: glow 의 같은 인자.
	/// - `stencil_fail`: glow 의 같은 인자.
	/// - `depth_fail`: glow 의 같은 인자.
	/// - `pass`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn stencil_op_separate(&self, face: u32, stencil_fail: u32, depth_fail: u32, pass: u32) {
		unsafe { self.context.stencil_op_separate(face, stencil_fail, depth_fail, pass) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `source`: glow 의 같은 인자.
	/// - `msg_type`: glow 의 같은 인자.
	/// - `severity`: glow 의 같은 인자.
	/// - `ids`: glow 의 같은 인자.
	/// - `enabled`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn debug_message_control(&self, source: u32, msg_type: u32, severity: u32, ids: &[u32], enabled: bool) {
		unsafe { self.context.debug_message_control(source, msg_type, severity, ids, enabled) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `source`: glow 의 같은 인자.
	/// - `msg_type`: glow 의 같은 인자.
	/// - `id`: glow 의 같은 인자.
	/// - `severity`: glow 의 같은 인자.
	/// - `msg`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn debug_message_insert<S>(&self, source: u32, msg_type: u32, id: u32, severity: u32, msg: S)
	where
		S: AsRef<str>,
	{
		unsafe { self.context.debug_message_insert(source, msg_type, id, severity, msg) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `callback`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn debug_message_callback<F>(&mut self, callback: F)
	where
		F: Fn(u32, u32, u32, u32, &str) + Send + Sync + 'static,
	{
		unsafe { self.context.debug_message_callback(callback) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `count`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_debug_message_log(&self, count: u32) -> Vec<glow::DebugMessageLogEntry> {
		unsafe { self.context.get_debug_message_log(count) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `source`: glow 의 같은 인자.
	/// - `id`: glow 의 같은 인자.
	/// - `message`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn push_debug_group<S>(&self, source: u32, id: u32, message: S)
	where
		S: AsRef<str>,
	{
		unsafe { self.context.push_debug_group(source, id, message) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	//==============================================================================
	#[inline]
	pub unsafe fn pop_debug_group(&self) {
		unsafe { self.context.pop_debug_group() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `identifier`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	/// - `label`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn object_label<S>(&self, identifier: u32, name: u32, label: Option<S>)
	where
		S: AsRef<str>,
	{
		unsafe { self.context.object_label(identifier, name, label) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `identifier`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_object_label(&self, identifier: u32, name: u32) -> String {
		unsafe { self.context.get_object_label(identifier, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `sync`: glow 의 같은 인자.
	/// - `label`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn object_ptr_label<S>(&self, sync: glow::Fence, label: Option<S>)
	where
		S: AsRef<str>,
	{
		unsafe { self.context.object_ptr_label(sync, label) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `sync`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_object_ptr_label(&self, sync: glow::Fence) -> String {
		unsafe { self.context.get_object_ptr_label(sync) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_uniform_block_index(&self, program: glow::Program, name: &str) -> Option<u32> {
		unsafe { self.context.get_uniform_block_index(program, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `names`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_uniform_indices(&self, program: glow::Program, names: &[&str]) -> Vec<Option<u32>> {
		unsafe { self.context.get_uniform_indices(program, names) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `binding`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn uniform_block_binding(&self, program: glow::Program, index: u32, binding: u32) {
		unsafe { self.context.uniform_block_binding(program, index, binding) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `name`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_shader_storage_block_index(&self, program: glow::Program, name: &str) -> Option<u32> {
		unsafe { self.context.get_shader_storage_block_index(program, name) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	/// - `binding`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn shader_storage_block_binding(&self, program: glow::Program, index: u32, binding: u32) {
		unsafe { self.context.shader_storage_block_binding(program, index, binding) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `src`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn read_buffer(&self, src: u32) {
		unsafe { self.context.read_buffer(src) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `framebuffer`: glow 의 같은 인자.
	/// - `src`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn named_framebuffer_read_buffer(&self, framebuffer: Option<glow::Framebuffer>, src: u32) {
		unsafe { self.context.named_framebuffer_read_buffer(framebuffer, src) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `x`: glow 의 같은 인자.
	/// - `y`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `gltype`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn read_pixels(&self, x: i32, y: i32, width: i32, height: i32, format: u32, gltype: u32, pixels: glow::PixelPackData) {
		unsafe { self.context.read_pixels(x, y, width, height, format, gltype, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `query`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn begin_query(&self, target: u32, query: glow::Query) {
		unsafe { self.context.begin_query(target, query) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn end_query(&self, target: u32) {
		unsafe { self.context.end_query(target) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `query`: glow 의 같은 인자.
	/// - `target`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn query_counter(&self, query: glow::Query, target: u32) {
		unsafe { self.context.query_counter(query, target) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `query`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_query_parameter_u32(&self, query: glow::Query, parameter: u32) -> u32 {
		unsafe { self.context.get_query_parameter_u32(query, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `query`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_query_parameter_u64(&self, query: glow::Query, parameter: u32) -> u64 {
		unsafe { self.context.get_query_parameter_u64(query, parameter) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `query`: glow 의 같은 인자.
	/// - `parameter`: glow 의 같은 인자.
	/// - `offset`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_query_parameter_u64_with_offset(&self, query: glow::Query, parameter: u32, offset: usize) {
		unsafe { self.context.get_query_parameter_u64_with_offset(query, parameter, offset) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `transform_feedback`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn delete_transform_feedback(&self, transform_feedback: glow::TransformFeedback) {
		unsafe { self.context.delete_transform_feedback(transform_feedback) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `transform_feedback`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn is_transform_feedback(&self, transform_feedback: glow::TransformFeedback) -> bool {
		unsafe { self.context.is_transform_feedback(transform_feedback) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn create_transform_feedback(&self) -> Result<glow::TransformFeedback, String> {
		unsafe { self.context.create_transform_feedback() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `transform_feedback`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn bind_transform_feedback(&self, target: u32, transform_feedback: Option<glow::TransformFeedback>) {
		unsafe { self.context.bind_transform_feedback(target, transform_feedback) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `primitive_mode`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn begin_transform_feedback(&self, primitive_mode: u32) {
		unsafe { self.context.begin_transform_feedback(primitive_mode) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	//==============================================================================
	#[inline]
	pub unsafe fn end_transform_feedback(&self) {
		unsafe { self.context.end_transform_feedback() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	//==============================================================================
	#[inline]
	pub unsafe fn pause_transform_feedback(&self) {
		unsafe { self.context.pause_transform_feedback() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	//==============================================================================
	#[inline]
	pub unsafe fn resume_transform_feedback(&self) {
		unsafe { self.context.resume_transform_feedback() }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `varyings`: glow 의 같은 인자.
	/// - `buffer_mode`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn transform_feedback_varyings(&self, program: glow::Program, varyings: &[&str], buffer_mode: u32) {
		unsafe { self.context.transform_feedback_varyings(program, varyings, buffer_mode) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `program`: glow 의 같은 인자.
	/// - `index`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[inline]
	pub unsafe fn get_transform_feedback_varying(&self, program: glow::Program, index: u32) -> Option<glow::ActiveTransformFeedback> {
		unsafe { self.context.get_transform_feedback_varying(program, index) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `barriers`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn memory_barrier(&self, barriers: u32) {
		unsafe { self.context.memory_barrier(barriers) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `barriers`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn memory_barrier_by_region(&self, barriers: u32) {
		unsafe { self.context.memory_barrier_by_region(barriers) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `unit`: glow 의 같은 인자.
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `layered`: glow 의 같은 인자.
	/// - `layer`: glow 의 같은 인자.
	/// - `access`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn bind_image_texture(&self, unit: u32, texture: Option<glow::Texture>, level: i32, layered: bool, layer: i32, access: u32, format: u32) {
		unsafe { self.context.bind_image_texture(unit, texture, level, layered, layer, access, format) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `count`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn max_shader_compiler_threads(&self, count: u32) {
		unsafe { self.context.max_shader_compiler_threads(count) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `mode`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn hint(&self, target: u32, mode: u32) {
		unsafe { self.context.hint(target, mode) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `value`: glow 의 같은 인자.
	/// - `invert`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn sample_coverage(&self, value: f32, invert: bool) {
		unsafe { self.context.sample_coverage(value, invert) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `pname`: glow 의 같은 인자.
	/// - `result`: glow 의 같은 인자.
	//==============================================================================
	#[inline]
	pub unsafe fn get_internal_format_i32_slice(&self, target: u32, internal_format: u32, pname: u32, result: &mut [i32]) {
		unsafe { self.context.get_internal_format_i32_slice(target, internal_format, pname, result) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_image_bitmap(&self, target: u32, level: i32, internal_format: i32, format: u32, ty: u32, pixels: &web_sys::ImageBitmap) {
		unsafe { self.context.tex_image_2d_with_image_bitmap(target, level, internal_format, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_image_bitmap_and_width_and_height(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, format: u32, ty: u32, pixels: &web_sys::ImageBitmap) {
		unsafe { self.context.tex_image_2d_with_image_bitmap_and_width_and_height(target, level, internal_format, width, height, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `canvas`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_html_canvas(&self, target: u32, level: i32, internal_format: i32, format: u32, ty: u32, canvas: &web_sys::HtmlCanvasElement) {
		unsafe { self.context.tex_image_2d_with_html_canvas(target, level, internal_format, format, ty, canvas) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `canvas`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_html_canvas_and_width_and_height(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, format: u32, ty: u32, canvas: &web_sys::HtmlCanvasElement) {
		unsafe { self.context.tex_image_2d_with_html_canvas_and_width_and_height(target, level, internal_format, width, height, format, ty, canvas) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_html_image(&self, target: u32, level: i32, internal_format: i32, format: u32, ty: u32, image: &web_sys::HtmlImageElement) {
		unsafe { self.context.tex_image_2d_with_html_image(target, level, internal_format, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_html_image_and_width_and_height(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, format: u32, ty: u32, image: &web_sys::HtmlImageElement) {
		unsafe { self.context.tex_image_2d_with_html_image_and_width_and_height(target, level, internal_format, width, height, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `video`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_html_video(&self, target: u32, level: i32, internal_format: i32, format: u32, ty: u32, video: &web_sys::HtmlVideoElement) {
		unsafe { self.context.tex_image_2d_with_html_video(target, level, internal_format, format, ty, video) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `video`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_html_video_and_width_and_height(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, format: u32, ty: u32, video: &web_sys::HtmlVideoElement) {
		unsafe { self.context.tex_image_2d_with_html_video_and_width_and_height(target, level, internal_format, width, height, format, ty, video) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `video_frame`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_video_frame(&self, target: u32, level: i32, internal_format: i32, format: u32, ty: u32, video_frame: &web_sys::VideoFrame) {
		unsafe { self.context.tex_image_2d_with_video_frame(target, level, internal_format, format, ty, video_frame) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `video_frame`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_video_frame_and_width_and_height(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, format: u32, ty: u32, video_frame: &web_sys::VideoFrame) {
		unsafe { self.context.tex_image_2d_with_video_frame_and_width_and_height(target, level, internal_format, width, height, format, ty, video_frame) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_image_data(&self, target: u32, level: i32, internal_format: i32, format: u32, ty: u32, pixels: &web_sys::ImageData) {
		unsafe { self.context.tex_image_2d_with_image_data(target, level, internal_format, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `pixels`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_2d_with_image_data_and_width_and_height(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, format: u32, ty: u32, pixels: &web_sys::ImageData) {
		unsafe { self.context.tex_image_2d_with_image_data_and_width_and_height(target, level, internal_format, width, height, format, ty, pixels) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_image_bitmap(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, format: u32, ty: u32, image: &web_sys::ImageBitmap) {
		unsafe { self.context.tex_sub_image_2d_with_image_bitmap(target, level, x_offset, y_offset, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_image_bitmap_and_width_and_height(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, ty: u32, image: &web_sys::ImageBitmap) {
		unsafe { self.context.tex_sub_image_2d_with_image_bitmap_and_width_and_height(target, level, x_offset, y_offset, width, height, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_html_canvas(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, format: u32, ty: u32, image: &web_sys::HtmlCanvasElement) {
		unsafe { self.context.tex_sub_image_2d_with_html_canvas(target, level, x_offset, y_offset, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_html_canvas_and_width_and_height(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, ty: u32, image: &web_sys::HtmlCanvasElement) {
		unsafe { self.context.tex_sub_image_2d_with_html_canvas_and_width_and_height(target, level, x_offset, y_offset, width, height, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_html_image(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, format: u32, ty: u32, image: &web_sys::HtmlImageElement) {
		unsafe { self.context.tex_sub_image_2d_with_html_image(target, level, x_offset, y_offset, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_html_image_and_width_and_height(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, ty: u32, image: &web_sys::HtmlImageElement) {
		unsafe { self.context.tex_sub_image_2d_with_html_image_and_width_and_height(target, level, x_offset, y_offset, width, height, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_html_video(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, format: u32, ty: u32, image: &web_sys::HtmlVideoElement) {
		unsafe { self.context.tex_sub_image_2d_with_html_video(target, level, x_offset, y_offset, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_html_video_and_width_and_height(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, ty: u32, image: &web_sys::HtmlVideoElement) {
		unsafe { self.context.tex_sub_image_2d_with_html_video_and_width_and_height(target, level, x_offset, y_offset, width, height, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `video_frame`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_video_frame(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, ty: u32, video_frame: &web_sys::VideoFrame) {
		unsafe { self.context.tex_sub_image_2d_with_video_frame(target, level, x_offset, y_offset, width, height, format, ty, video_frame) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `video_frame`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_video_frame_and_width_and_height(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, ty: u32, video_frame: &web_sys::VideoFrame) {
		unsafe { self.context.tex_sub_image_2d_with_video_frame_and_width_and_height(target, level, x_offset, y_offset, width, height, format, ty, video_frame) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_image_data(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, format: u32, ty: u32, image: &web_sys::ImageData) {
		unsafe { self.context.tex_sub_image_2d_with_image_data(target, level, x_offset, y_offset, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_2d_with_image_data_and_width_and_height(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, width: i32, height: i32, format: u32, ty: u32, image: &web_sys::ImageData) {
		unsafe { self.context.tex_sub_image_2d_with_image_data_and_width_and_height(target, level, x_offset, y_offset, width, height, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_3d_with_image_bitmap(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, image: &web_sys::ImageBitmap) {
		unsafe { self.context.tex_image_3d_with_image_bitmap(target, level, internal_format, width, height, depth, border, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_3d_with_html_canvas_element(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, image: &web_sys::HtmlCanvasElement) {
		unsafe { self.context.tex_image_3d_with_html_canvas_element(target, level, internal_format, width, height, depth, border, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_3d_with_html_image_element(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, image: &web_sys::HtmlImageElement) {
		unsafe { self.context.tex_image_3d_with_html_image_element(target, level, internal_format, width, height, depth, border, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_3d_with_html_video_element(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, image: &web_sys::HtmlVideoElement) {
		unsafe { self.context.tex_image_3d_with_html_video_element(target, level, internal_format, width, height, depth, border, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `video_frame`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_3d_with_video_frame(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, video_frame: &web_sys::VideoFrame) {
		unsafe { self.context.tex_image_3d_with_video_frame(target, level, internal_format, width, height, depth, border, format, ty, video_frame) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `internal_format`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `border`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_image_3d_with_image_data(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, image: &web_sys::ImageData) {
		unsafe { self.context.tex_image_3d_with_image_data(target, level, internal_format, width, height, depth, border, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_3d_with_image_bitmap(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, image: &web_sys::ImageBitmap) {
		unsafe { self.context.tex_sub_image_3d_with_image_bitmap(target, level, x_offset, y_offset, z_offset, width, height, depth, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_3d_with_html_canvas_element(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, image: &web_sys::HtmlCanvasElement) {
		unsafe { self.context.tex_sub_image_3d_with_html_canvas_element(target, level, x_offset, y_offset, z_offset, width, height, depth, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_3d_with_html_image_element(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, image: &web_sys::HtmlImageElement) {
		unsafe { self.context.tex_sub_image_3d_with_html_image_element(target, level, x_offset, y_offset, z_offset, width, height, depth, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_3d_with_html_video_element(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, image: &web_sys::HtmlVideoElement) {
		unsafe { self.context.tex_sub_image_3d_with_html_video_element(target, level, x_offset, y_offset, z_offset, width, height, depth, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `video_frame`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_3d_with_video_frame(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, video_frame: &web_sys::VideoFrame) {
		unsafe { self.context.tex_sub_image_3d_with_video_frame(target, level, x_offset, y_offset, z_offset, width, height, depth, format, ty, video_frame) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `x_offset`: glow 의 같은 인자.
	/// - `y_offset`: glow 의 같은 인자.
	/// - `z_offset`: glow 의 같은 인자.
	/// - `width`: glow 의 같은 인자.
	/// - `height`: glow 의 같은 인자.
	/// - `depth`: glow 의 같은 인자.
	/// - `format`: glow 의 같은 인자.
	/// - `ty`: glow 의 같은 인자.
	/// - `image`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn tex_sub_image_3d_with_image_data(&self, target: u32, level: i32, x_offset: i32, y_offset: i32, z_offset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, image: &web_sys::ImageData) {
		unsafe { self.context.tex_sub_image_3d_with_image_data(target, level, x_offset, y_offset, z_offset, width, height, depth, format, ty, image) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `target`: glow 의 같은 인자.
	/// - `attachment`: glow 의 같은 인자.
	/// - `texture`: glow 의 같은 인자.
	/// - `level`: glow 의 같은 인자.
	/// - `base_view_index`: glow 의 같은 인자.
	/// - `num_views`: glow 의 같은 인자.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn framebuffer_texture_multiview_ovr(&self, target: u32, attachment: u32, texture: Option<glow::Texture>, level: i32, base_view_index: i32, num_views: i32) {
		unsafe { self.context.framebuffer_texture_multiview_ovr(target, attachment, texture, level, base_view_index, num_views) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub fn compressed_texture_astc_supports_ldr_profile(&self) -> bool {
		self.context.compressed_texture_astc_supports_ldr_profile()
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub fn compressed_texture_astc_supports_hdr_profile(&self) -> bool {
		self.context.compressed_texture_astc_supports_hdr_profile()
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub fn lose_context(&self) {
		self.context.lose_context()
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub fn restore_context(&self) {
		self.context.restore_context()
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `handle`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub unsafe fn register_external_texture(&self, handle: web_sys::WebGlTexture) -> glow::WebTextureKey {
		unsafe { self.context.register_external_texture(handle) }
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `key`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub fn unregister_external_texture(&self, key: glow::WebTextureKey) -> Option<web_sys::WebGlTexture> {
		self.context.unregister_external_texture(key)
	}

	//==============================================================================
	/// glow 의 같은 이름 함수를 그대로 부른다. (웹 전용, 상태 캐시와 상관없는 GL 함수)
	///
	/// # 인자
	/// - `key`: glow 의 같은 인자.
	///
	/// # 돌려줌
	/// glow 가 돌려주는 값.
	//==============================================================================
	#[cfg(all(target_family = "wasm", not(target_os = "emscripten")))]
	#[inline]
	pub fn as_web_gl_texture(&self, key: glow::WebTextureKey) -> Option<web_sys::WebGlTexture> {
		self.context.as_web_gl_texture(key)
	}
}

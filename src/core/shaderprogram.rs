//==============================================================================
//! 셰이더 프로그램.
//! - 원본은 GLSL ES 3.00 (#version 300 es) 이다. 데스크탑 GL 3.3 코어에서 돌도록 머리만 #version 330 core 로 바꿔 컴파일한다.
//!   (precision 한정자는 GLSL 1.30 부터 의미 없이 받아 주므로 본문은 그대로 둔다)
//! - 유니폼 / 어트리뷰트 위치는 처음 물을 때 읽어 담아 둔다.
//==============================================================================
use crate::core::glcontext::GlContext;
use std::cell::RefCell;
use std::collections::HashMap;

//==============================================================================
/// 셰이더 프로그램. (버텍스, 프래그먼트 셰이더를 링크한 GL 프로그램과 유니폼, 어트리뷰트 위치 캐시)
/// - 소스는 GLSL ES 3.00 으로 쓴다. 데스크탑은 머리만 #version 330 core 로 바꿔 컴파일한다.
/// - GL 컨텍스트를 들지 않고 함수마다 받는다. (Graphic 의 get_gl)
///
/// # 예제
/// 단색 프로그램을 만들고 유니폼 올리기
/// ```rust,no_run
/// use vanilla::{Graphic, ShaderProgram};
///
/// const VERTEX_SHADER_SOURCE: &str = "#version 300 es
/// in vec2 vertexPosition;
/// void main() {
/// 	gl_Position = vec4(vertexPosition, 0.0, 1.0);
/// }
/// ";
///
/// const FRAGMENT_SHADER_SOURCE: &str = "#version 300 es
/// precision highp float;
/// uniform vec4 mainColor;
/// out vec4 outputColor;
/// void main() {
/// 	outputColor = mainColor;
/// }
/// ";
///
/// fn create_program(graphic: &Graphic) -> Result<ShaderProgram, String> {
/// 	let gl = graphic.get_gl();
/// 	let shader_program = ShaderProgram::create_with_attribute_locations(gl, VERTEX_SHADER_SOURCE, FRAGMENT_SHADER_SOURCE, &[("vertexPosition", 0)])?;
/// 	shader_program.use_program(gl);
/// 	let main_color_location = shader_program.get_uniform_location(gl, "mainColor");
/// 	unsafe {
/// 		gl.uniform_4_f32(main_color_location.as_ref(), 1.0, 0.5, 0.0, 1.0);
/// 	}
/// 	assert_eq!(shader_program.get_attribute_location(gl, "vertexPosition"), Some(0));
/// 	Ok(shader_program)
/// }
/// ```
//==============================================================================
pub struct ShaderProgram {
	program: crate::core::graphic::GlProgram,
	uniform_location_table: RefCell<HashMap<String, Option<crate::core::graphic::GlUniformLocation>>>,
	attribute_location_table: RefCell<HashMap<String, Option<u32>>>,
	/// 짓기를 맡긴 채 아직 끝내지 않은 셰이더. (맡기지 않았거나 끝냈으면 None, 원본 pendingVertexShader, pendingFragmentShader)
	pending_shader_list: RefCell<Option<(crate::core::graphic::GlShader, crate::core::graphic::GlShader)>>,
	/// 맡긴 짓기가 끝났다고 한 번 확인했는지. (끝남은 되돌지 않으니 다시 묻지 않는다, 원본 isBuildCompleted)
	is_build_completed: std::cell::Cell<bool>,
}

// 시험이 지은 프로그램 수를 셀 때. (그리기 프레임이 프로그램을 짓는지 보는 시험)
#[cfg(test)]
thread_local! {
	pub(crate) static BUILT_PROGRAM_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// 링크가 끝났는지 묻는 값. (KHR_parallel_shader_compile 의 COMPLETION_STATUS_KHR)
const COMPLETION_STATUS: u32 = 0x91B1;

impl ShaderProgram {
	//==============================================================================
	/// 생성. (컴파일 / 링크 실패면 로그를 담은 오류, 그때까지 지은 셰이더와 프로그램은 지운다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트. (Graphic 의 get_gl)
	/// - `vertex_shader_source`: 버텍스 셰이더 소스. (GLSL ES 3.00)
	/// - `fragment_shader_source`: 프래그먼트 셰이더 소스. (GLSL ES 3.00)
	///
	/// # 돌려줌
	/// 링크한 프로그램. 셰이더 컴파일이나 프로그램 링크에 실패하면 정보 로그를 담은 Err, GL 객체를 만들지 못하면 그 오류 글을 담은 Err.
	//==============================================================================
	pub fn new(gl: &GlContext, vertex_shader_source: &str, fragment_shader_source: &str) -> Result<ShaderProgram, String> {
		ShaderProgram::create_with_attribute_locations(gl, vertex_shader_source, fragment_shader_source, &[])
	}

	//==============================================================================
	/// 어트리뷰트 위치를 링크 전에 고정해 생성. (원본 생성자의 attributeLocationTable 인자, 다른 프로그램의 버텍스 어레이를 함께 쓸 때)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `vertex_shader_source`: 버텍스 셰이더 소스. (GLSL ES 3.00)
	/// - `fragment_shader_source`: 프래그먼트 셰이더 소스. (GLSL ES 3.00)
	/// - `attribute_location_list`: (어트리뷰트 이름, 위치) 목록. (링크 전에 bind_attrib_location 으로 건다, 비면 new 와 같다)
	///
	/// # 돌려줌
	/// 링크한 프로그램. 셰이더 컴파일이나 프로그램 링크에 실패하면 정보 로그를 담은 Err, GL 객체를 만들지 못하면 그 오류 글을 담은 Err.
	//==============================================================================
	pub fn create_with_attribute_locations(gl: &GlContext, vertex_shader_source: &str, fragment_shader_source: &str, attribute_location_list: &[(&str, u32)]) -> Result<ShaderProgram, String> {
		#[cfg(test)]
		BUILT_PROGRAM_COUNT.with(|count| count.set(count.get() + 1));
		let desktop_vertex_shader_source = convert_to_desktop_source(vertex_shader_source);
		let desktop_fragment_shader_source = convert_to_desktop_source(fragment_shader_source);
		unsafe {
			let vertex_shader = compile_shader(gl, glow::VERTEX_SHADER, &desktop_vertex_shader_source)?;
			let fragment_shader = match compile_shader(gl, glow::FRAGMENT_SHADER, &desktop_fragment_shader_source) {
				Ok(fragment_shader) => {
					fragment_shader
				}
				Err(error) => {
					gl.delete_shader(vertex_shader);
					return Err(error);
				}
			};
			let program = match crate::core::glstatecache::create_program(gl) {
				Ok(program) => {
					program
				}
				Err(error) => {
					gl.delete_shader(vertex_shader);
					gl.delete_shader(fragment_shader);
					return Err(error);
				}
			};
			gl.attach_shader(program, vertex_shader);
			gl.attach_shader(program, fragment_shader);
			for (attribute_name, attribute_location) in attribute_location_list {
				gl.bind_attrib_location(program, *attribute_location, attribute_name);
			}
			crate::core::glstatecache::link_program(gl, program);
			let is_linked = crate::core::glstatecache::get_program_link_status(gl, program);
			gl.detach_shader(program, vertex_shader);
			gl.detach_shader(program, fragment_shader);
			gl.delete_shader(vertex_shader);
			gl.delete_shader(fragment_shader);
			if !is_linked {
				let program_info_log = gl.get_program_info_log(program);
				crate::core::glstatecache::delete_program(gl, program);
				return Err(format!("셰이더 프로그램 링크 실패: {}", program_info_log));
			}
			Ok(ShaderProgram {
				program,
				uniform_location_table: RefCell::new(HashMap::new()),
				attribute_location_table: RefCell::new(HashMap::new()),
				pending_shader_list: RefCell::new(None),
				is_build_completed: std::cell::Cell::new(false),
			})
		}
	}

	//==============================================================================
	/// 컴파일과 링크를 맡기기만 하고 결과는 확인하지 않는 생성. (원본 생성자의 isDeferred, finish_build 를 불러야 쓸 수 있다)
	/// - KHR_parallel_shader_compile 이 있으면 드라이버가 뒤에서 짓고 is_build_complete 로 주 스레드를 막지 않고 끝을 물을 수 있다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `vertex_shader_source`: 버텍스 셰이더 소스. (GLSL ES 3.00)
	/// - `fragment_shader_source`: 프래그먼트 셰이더 소스. (GLSL ES 3.00)
	/// - `attribute_location_list`: (어트리뷰트 이름, 위치) 목록. (링크 전에 건다)
	///
	/// # 돌려줌
	/// 맡긴 프로그램. 컴파일이나 링크 오류는 finish_build 가 알린다. GL 객체를 만들지 못하면 그 오류 글을 담은 Err.
	///
	/// # 예제
	/// 맡겨 두었다가 끝나면 끝내 쓰기
	/// ```rust,no_run
	/// use vanilla::{Graphic, ShaderProgram};
	///
	/// const VERTEX_SHADER_SOURCE: &str = "#version 300 es
	/// in vec2 vertexPosition;
	/// void main() {
	/// 	gl_Position = vec4(vertexPosition, 0.0, 1.0);
	/// }
	/// ";
	///
	/// const FRAGMENT_SHADER_SOURCE: &str = "#version 300 es
	/// precision highp float;
	/// out vec4 outputColor;
	/// void main() {
	/// 	outputColor = vec4(1.0);
	/// }
	/// ";
	///
	/// fn build(graphic: &Graphic) -> Result<ShaderProgram, String> {
	/// 	let gl = graphic.get_gl();
	/// 	let shader_program = ShaderProgram::create_deferred(gl, VERTEX_SHADER_SOURCE, FRAGMENT_SHADER_SOURCE, &[("vertexPosition", 0)])?;
	/// 	assert!(shader_program.is_deferred());
	/// 	// Finish it once it is done. (finish_build waits until done if it is not)
	/// 	if shader_program.is_build_complete(gl) {
	/// 		shader_program.finish_build(gl)?;
	/// 	}
	/// 	Ok(shader_program)
	/// }
	/// ```
	//==============================================================================
	pub fn create_deferred(gl: &GlContext, vertex_shader_source: &str, fragment_shader_source: &str, attribute_location_list: &[(&str, u32)]) -> Result<ShaderProgram, String> {
		#[cfg(test)]
		BUILT_PROGRAM_COUNT.with(|count| count.set(count.get() + 1));
		let desktop_vertex_shader_source = convert_to_desktop_source(vertex_shader_source);
		let desktop_fragment_shader_source = convert_to_desktop_source(fragment_shader_source);
		unsafe {
			let vertex_shader = gl.create_shader(glow::VERTEX_SHADER)?;
			gl.shader_source(vertex_shader, &desktop_vertex_shader_source);
			gl.compile_shader(vertex_shader);
			let fragment_shader = match gl.create_shader(glow::FRAGMENT_SHADER) {
				Ok(fragment_shader) => {
					fragment_shader
				}
				Err(error) => {
					gl.delete_shader(vertex_shader);
					return Err(error);
				}
			};
			gl.shader_source(fragment_shader, &desktop_fragment_shader_source);
			gl.compile_shader(fragment_shader);
			let program = match crate::core::glstatecache::create_program(gl) {
				Ok(program) => {
					program
				}
				Err(error) => {
					gl.delete_shader(vertex_shader);
					gl.delete_shader(fragment_shader);
					return Err(error);
				}
			};
			gl.attach_shader(program, vertex_shader);
			gl.attach_shader(program, fragment_shader);
			for (attribute_name, attribute_location) in attribute_location_list {
				gl.bind_attrib_location(program, *attribute_location, attribute_name);
			}
			crate::core::glstatecache::link_program(gl, program);
			Ok(ShaderProgram {
				program,
				uniform_location_table: RefCell::new(HashMap::new()),
				attribute_location_table: RefCell::new(HashMap::new()),
				pending_shader_list: RefCell::new(Some((vertex_shader, fragment_shader))),
				is_build_completed: std::cell::Cell::new(false),
			})
		}
	}

	//==============================================================================
	/// 병렬 셰이더 컴파일 확장이 있는지. (원본 isParallelCompileSupported, KHR_parallel_shader_compile, 웹은 같은 확장)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	///
	/// # 돌려줌
	/// 있으면 true. (있으면 컴파일과 링크를 맡기고 주 스레드를 막지 않고 끝을 물을 수 있다)
	//==============================================================================
	pub fn is_parallel_compile_supported(gl: &GlContext) -> bool {
		gl.supported_extensions().iter().any(|extension_name| extension_name.contains("parallel_shader_compile"))
	}

	//==============================================================================
	/// 짓기를 맡긴 채 아직 끝내지 않았는지. (create_deferred 로 지었고 finish_build 를 아직 안 불렀으면 true)
	///
	/// # 돌려줌
	/// 맡긴 짓기가 아직 안 끝났으면 true.
	//==============================================================================
	pub fn is_deferred(&self) -> bool {
		self.pending_shader_list.borrow().is_some()
	}

	//==============================================================================
	/// 맡긴 짓기가 끝났는지. (원본 isBuildComplete, 맡기지 않았거나 이미 끝냈으면 true, 확장이 없으면 true, 물을 뿐 멈추지 않는다)
	///
	/// # 인자
	/// - `gl`: 프로그램을 만든 GL 컨텍스트.
	///
	/// # 돌려줌
	/// 끝났으면 true. (true 여도 finish_build 로 결과를 확인해야 쓴다)
	//==============================================================================
	pub fn is_build_complete(&self, gl: &GlContext) -> bool {
		if self.pending_shader_list.borrow().is_none() {
			return true;
		}
		if !ShaderProgram::is_parallel_compile_supported(gl) {
			return true;
		}
		if self.is_build_completed.get() {
			return true;
		}
		// 끝남을 묻는 일은 GPU 와 오가는 일이라 끝난 것은 다시 묻지 않는다.
		let is_complete = unsafe { gl.get_program_parameter_i32(self.program, COMPLETION_STATUS) != 0 };
		self.is_build_completed.set(is_complete);
		is_complete
	}

	//==============================================================================
	/// 맡긴 짓기 끝내기. (원본 finishBuild, 컴파일과 링크 결과를 확인한다, 안 끝났으면 끝날 때까지 기다린다, 맡기지 않았거나 이미 끝냈으면 아무것도 하지 않는다)
	/// - 실패하면 셰이더와 프로그램을 지우고 정보 로그를 담은 Err 를 돌려준다. (생성자와 같은 글)
	///
	/// # 인자
	/// - `gl`: 프로그램을 만든 GL 컨텍스트.
	///
	/// # 돌려줌
	/// 성공하면 Ok. 컴파일이나 링크에 실패하면 Err.
	//==============================================================================
	pub fn finish_build(&self, gl: &GlContext) -> Result<(), String> {
		let Some((vertex_shader, fragment_shader)) = self.pending_shader_list.borrow_mut().take() else {
			return Ok(());
		};
		unsafe {
			let mut error_message = None;
			if !gl.get_shader_compile_status(vertex_shader) {
				error_message = Some(format!("셰이더 컴파일 실패: {}", gl.get_shader_info_log(vertex_shader)));
			}
			else if !gl.get_shader_compile_status(fragment_shader) {
				error_message = Some(format!("셰이더 컴파일 실패: {}", gl.get_shader_info_log(fragment_shader)));
			}
			else if !crate::core::glstatecache::get_program_link_status(gl, self.program) {
				error_message = Some(format!("셰이더 프로그램 링크 실패: {}", gl.get_program_info_log(self.program)));
			}
			gl.detach_shader(self.program, vertex_shader);
			gl.detach_shader(self.program, fragment_shader);
			gl.delete_shader(vertex_shader);
			gl.delete_shader(fragment_shader);
			if let Some(error_message) = error_message {
				crate::core::glstatecache::delete_program(gl, self.program);
				return Err(error_message);
			}
		}
		Ok(())
	}

	//==============================================================================
	/// 사용. (원본 useProgram, 상태 캐시가 이미 이 프로그램을 쓴다고 기억하면 GL 에 다시 걸지 않는다)
	///
	/// # 인자
	/// - `gl`: 프로그램을 만든 GL 컨텍스트.
	//==============================================================================
	pub fn use_program(&self, gl: &GlContext) {
		if crate::core::glstatecache::is_program_in_use(gl, Some(self.program)) {
			return;
		}
		unsafe {
			crate::core::glstatecache::use_program(gl, Some(self.program));
		}
	}

	//==============================================================================
	/// 유니폼 위치 반환.
	//==============================================================================
	/// 웹 판 때문에 clone 을 쓴다. (데스크탑에서는 Copy 라 clippy 가 말린다)
	///
	/// # 인자
	/// - `gl`: 프로그램을 만든 GL 컨텍스트.
	/// - `name`: 유니폼 이름. (셰이더 소스의 이름 그대로)
	///
	/// # 돌려줌
	/// 유니폼 위치. 그 이름의 활성 유니폼이 없으면 None. (처음 물을 때 읽고, 없다는 것까지 담아 둔다)
	#[allow(clippy::clone_on_copy)]
	pub fn get_uniform_location(&self, gl: &GlContext, name: &str) -> Option<crate::core::graphic::GlUniformLocation> {
		let mut uniform_location_table = self.uniform_location_table.borrow_mut();
		if let Some(location) = uniform_location_table.get(name) {
			// 웹의 위치 객체는 Copy 가 아니라 복제한다. (데스크탑은 정수 하나라 복사와 같다)
			return location.clone();
		}
		let location = unsafe {
			gl.get_uniform_location(self.program, name)
		};
		uniform_location_table.insert(name.to_string(), location.clone());
		location
	}

	//==============================================================================
	/// 어트리뷰트 위치 반환.
	///
	/// # 인자
	/// - `gl`: 프로그램을 만든 GL 컨텍스트.
	/// - `name`: 어트리뷰트 이름. (셰이더 소스의 이름 그대로)
	///
	/// # 돌려줌
	/// 어트리뷰트 위치. 그 이름의 활성 어트리뷰트가 없으면 None. (처음 물을 때 읽고 담아 둔다)
	//==============================================================================
	pub fn get_attribute_location(&self, gl: &GlContext, name: &str) -> Option<u32> {
		let mut attribute_location_table = self.attribute_location_table.borrow_mut();
		if let Some(location) = attribute_location_table.get(name) {
			return *location;
		}
		let location = unsafe {
			gl.get_attrib_location(self.program, name)
		};
		attribute_location_table.insert(name.to_string(), location);
		location
	}

	//==============================================================================
	/// GL 프로그램 반환.
	///
	/// # 돌려줌
	/// 링크한 GL 프로그램 객체.
	//==============================================================================
	pub fn get_program(&self) -> crate::core::graphic::GlProgram {
		self.program
	}
}

//==============================================================================
/// GLSL ES 3.00 머리를 데스크탑 GLSL 3.30 으로 바꾼다. (웹 WebGL2 에서는 원본 그대로)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn convert_to_desktop_source(source: &str) -> String {
	source.replacen("#version 300 es", "#version 330 core", 1)
}

//==============================================================================
/// GLSL ES 3.00 소스를 그대로 돌려준다. (웹 WebGL2 는 원본 머리를 그대로 받는다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
pub(crate) fn convert_to_desktop_source(source: &str) -> String {
	source.to_string()
}

//==============================================================================
/// 셰이더 짓기 실패 알리기. (엔진이 짓는 프로그램이 실패해 건너뛰기로 할 때 부른다, 원본 reportShaderBuildFailure)
/// - 컨텍스트를 이미 잃은 뒤의 실패는 셰이더 탓이 아니다. 알리지 않고 false 를 돌려, 부른 쪽이 실패를 기억하지 않고 되살린 뒤 다시 짓게 한다.
/// - 그 밖의 실패는 엔진의 그래픽 오류 리스너(add_graphic_error_listener)에 { kind, name, error } 를 한 번 알리고 true 를 돌려 기억하게 한다.
///
/// # 인자
/// - `gl`: 프로그램을 짓던 컨텍스트.
/// - `name`: 짓지 못한 것의 이름.
/// - `error`: 실패한 까닭.
///
/// # 돌려줌
/// 셰이더 탓이라 실패를 기억해도 되면 true. (컨텍스트를 잃은 탓이면 false)
//==============================================================================
pub(crate) fn report_shader_build_failure(gl: &GlContext, name: &str, error: &str) -> bool {
	if gl.is_context_lost() {
		return false;
	}
	if let Ok(engine) = crate::core::globalengine::get_global_engine() {
		engine.dispatch_graphic_error(&crate::core::engine::GraphicError { kind: crate::core::engine::GraphicErrorKind::ShaderBuildFailed, name: String::from(name), error: String::from(error) });
	}
	true
}

//==============================================================================
/// 셰이더 하나 컴파일.
//==============================================================================
unsafe fn compile_shader(gl: &GlContext, shader_type: u32, source: &str) -> Result<crate::core::graphic::GlShader, String> {
	let shader = gl.create_shader(shader_type)?;
	gl.shader_source(shader, source);
	gl.compile_shader(shader);
	if !gl.get_shader_compile_status(shader) {
		let shader_info_log = gl.get_shader_info_log(shader);
		gl.delete_shader(shader);
		return Err(format!("셰이더 컴파일 실패: {}", shader_info_log));
	}
	Ok(shader)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;
	use crate::core::imagetexturecache::fake_gl::{self, FakeGlObject};

	//==============================================================================
	/// 셰이더 짓기 실패는 그래픽 오류 리스너에 한 번 알리고 기억하게 하고, 컨텍스트를 잃은 뒤의 실패는 알리지도 기억하게 하지도 않는다. (가짜 GL)
	//==============================================================================
	#[test]
	fn reports_shader_build_failure_unless_context_lost() {
		use crate::core::engine::{Engine, EngineConfiguration, GraphicError, GraphicErrorKind, GraphicErrorListener};
		use std::rc::Rc;
		let engine = Engine::new(EngineConfiguration::new());
		let received_list: Rc<RefCell<Vec<GraphicError>>> = Rc::new(RefCell::new(Vec::new()));
		let listener_received_list = received_list.clone();
		let graphic_error_listener: GraphicErrorListener = Rc::new(RefCell::new(move |graphic_error: &GraphicError| {
			listener_received_list.borrow_mut().push(graphic_error.clone());
		}));
		engine.add_graphic_error_listener(&graphic_error_listener);
		let gl = fake_gl::create_context();
		let gl_context = GlContext::from_context(&gl);
		assert!(report_shader_build_failure(gl_context, "stripe", "compile failed"));
		assert_eq!(*received_list.borrow(), vec![GraphicError { kind: GraphicErrorKind::ShaderBuildFailed, name: String::from("stripe"), error: String::from("compile failed") }]);
		GlContext::set_context_lost(true);
		assert!(gl_context.is_context_lost());
		assert!(!report_shader_build_failure(gl_context, "stripe", "lost"));
		assert_eq!(received_list.borrow().len(), 1);
		GlContext::set_context_lost(false);
		assert!(!gl_context.is_context_lost());
	}

	//==============================================================================
	/// 프래그먼트 셰이더나 프로그램을 못 지으면 먼저 지은 셰이더를 지운다. (가짜 GL)
	//==============================================================================
	#[test]
	fn failure_deletes_created_shaders() {
		let gl = fake_gl::create_context();
		fake_gl::fail_after(FakeGlObject::Shader, 1);
		assert!(ShaderProgram::new(GlContext::from_context(&gl), "", "").is_err());
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Shader), 0);
		let gl = fake_gl::create_context();
		fake_gl::fail_after(FakeGlObject::Program, 0);
		assert!(ShaderProgram::new(GlContext::from_context(&gl), "", "").is_err());
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Shader), 0);
		let gl = fake_gl::create_context();
		let shader_program = ShaderProgram::new(GlContext::from_context(&gl), "", "").expect("shader program");
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Shader), 0);
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Program), 1);
		unsafe {
			crate::core::glstatecache::delete_program(&gl, shader_program.get_program());
		}
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Program), 0);
	}
}

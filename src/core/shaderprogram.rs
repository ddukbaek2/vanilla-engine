//==============================================================================
//! 셰이더 프로그램.
//! - 원본은 GLSL ES 3.00 (#version 300 es) 이다. 데스크탑 GL 3.3 코어에서 돌도록 머리만 #version 330 core 로 바꿔 컴파일한다.
//!   (precision 한정자는 GLSL 1.30 부터 의미 없이 받아 주므로 본문은 그대로 둔다)
//! - 유니폼 / 어트리뷰트 위치는 처음 물을 때 읽어 담아 둔다.
//==============================================================================
use glow::HasContext;
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
/// use glow::HasContext;
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
}

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
	pub fn new(gl: &glow::Context, vertex_shader_source: &str, fragment_shader_source: &str) -> Result<ShaderProgram, String> {
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
	pub fn create_with_attribute_locations(gl: &glow::Context, vertex_shader_source: &str, fragment_shader_source: &str, attribute_location_list: &[(&str, u32)]) -> Result<ShaderProgram, String> {
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
			})
		}
	}

	//==============================================================================
	/// 사용. (원본 useProgram, 상태 캐시가 이미 이 프로그램을 쓴다고 기억하면 GL 에 다시 걸지 않는다)
	///
	/// # 인자
	/// - `gl`: 프로그램을 만든 GL 컨텍스트.
	//==============================================================================
	pub fn use_program(&self, gl: &glow::Context) {
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
	pub fn get_uniform_location(&self, gl: &glow::Context, name: &str) -> Option<crate::core::graphic::GlUniformLocation> {
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
	pub fn get_attribute_location(&self, gl: &glow::Context, name: &str) -> Option<u32> {
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
/// 셰이더 하나 컴파일.
//==============================================================================
unsafe fn compile_shader(gl: &glow::Context, shader_type: u32, source: &str) -> Result<crate::core::graphic::GlShader, String> {
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
	/// 프래그먼트 셰이더나 프로그램을 못 지으면 먼저 지은 셰이더를 지운다. (가짜 GL)
	//==============================================================================
	#[test]
	fn failure_deletes_created_shaders() {
		let gl = fake_gl::create_context();
		fake_gl::fail_after(FakeGlObject::Shader, 1);
		assert!(ShaderProgram::new(&gl, "", "").is_err());
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Shader), 0);
		let gl = fake_gl::create_context();
		fake_gl::fail_after(FakeGlObject::Program, 0);
		assert!(ShaderProgram::new(&gl, "", "").is_err());
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Shader), 0);
		let gl = fake_gl::create_context();
		let shader_program = ShaderProgram::new(&gl, "", "").expect("shader program");
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Shader), 0);
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Program), 1);
		unsafe {
			crate::core::glstatecache::delete_program(&gl, shader_program.get_program());
		}
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Program), 0);
	}
}

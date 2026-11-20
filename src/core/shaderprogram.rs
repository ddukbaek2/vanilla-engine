//==============================================================================
// 셰이더 프로그램.
// - 원본은 GLSL ES 3.00 (#version 300 es) 이다. 데스크탑 GL 3.3 코어에서 돌도록 머리만 #version 330 core 로 바꿔 컴파일한다.
//   (precision 한정자는 GLSL 1.30 부터 의미 없이 받아 주므로 본문은 그대로 둔다)
// - 유니폼 / 어트리뷰트 위치는 처음 물을 때 읽어 담아 둔다.
//==============================================================================
use glow::HasContext;
use std::cell::RefCell;
use std::collections::HashMap;

pub struct ShaderProgram {
	program: crate::core::graphic::GlProgram,
	uniform_location_table: RefCell<HashMap<String, Option<crate::core::graphic::GlUniformLocation>>>,
	attribute_location_table: RefCell<HashMap<String, Option<u32>>>,
}

impl ShaderProgram {
	//==============================================================================
	// 생성. (컴파일 / 링크 실패면 로그를 담은 오류)
	//==============================================================================
	pub fn new(gl: &glow::Context, vertex_shader_source: &str, fragment_shader_source: &str) -> Result<ShaderProgram, String> {
		ShaderProgram::create_with_attribute_locations(gl, vertex_shader_source, fragment_shader_source, &[])
	}

	//==============================================================================
	// 어트리뷰트 위치를 링크 전에 고정해 생성. (원본 생성자의 attribute_location_table 인자, 다른 프로그램의 버텍스 어레이를 함께 쓸 때)
	//==============================================================================
	pub fn create_with_attribute_locations(gl: &glow::Context, vertex_shader_source: &str, fragment_shader_source: &str, attribute_location_list: &[(&str, u32)]) -> Result<ShaderProgram, String> {
		let desktop_vertex_shader_source = convert_to_desktop_source(vertex_shader_source);
		let desktop_fragment_shader_source = convert_to_desktop_source(fragment_shader_source);
		unsafe {
			let vertex_shader = compile_shader(gl, glow::VERTEX_SHADER, &desktop_vertex_shader_source)?;
			let fragment_shader = compile_shader(gl, glow::FRAGMENT_SHADER, &desktop_fragment_shader_source)?;
			let program = gl.create_program()?;
			gl.attach_shader(program, vertex_shader);
			gl.attach_shader(program, fragment_shader);
			for (attribute_name, attribute_location) in attribute_location_list {
				gl.bind_attrib_location(program, *attribute_location, attribute_name);
			}
			gl.link_program(program);
			let is_linked = gl.get_program_link_status(program);
			gl.detach_shader(program, vertex_shader);
			gl.detach_shader(program, fragment_shader);
			gl.delete_shader(vertex_shader);
			gl.delete_shader(fragment_shader);
			if !is_linked {
				let program_info_log = gl.get_program_info_log(program);
				gl.delete_program(program);
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
	// 사용. (원본 use(). 러스트에서 use 는 예약어라 이름을 바꿨다)
	//==============================================================================
	pub fn use_program(&self, gl: &glow::Context) {
		unsafe {
			gl.use_program(Some(self.program));
		}
	}

	//==============================================================================
	// 유니폼 위치 반환.
	//==============================================================================
	// 웹 판 때문에 clone 을 쓴다. (데스크탑에서는 Copy 라 clippy 가 말린다)
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
	// 어트리뷰트 위치 반환.
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
	// GL 프로그램 반환.
	//==============================================================================
	pub fn get_program(&self) -> crate::core::graphic::GlProgram {
		self.program
	}
}

//==============================================================================
// GLSL ES 3.00 머리를 데스크탑 GLSL 3.30 으로 바꾼다. (웹 WebGL2 에서는 원본 그대로)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn convert_to_desktop_source(source: &str) -> String {
	source.replacen("#version 300 es", "#version 330 core", 1)
}

#[cfg(target_arch = "wasm32")]
fn convert_to_desktop_source(source: &str) -> String {
	source.to_string()
}

//==============================================================================
// 셰이더 하나 컴파일.
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

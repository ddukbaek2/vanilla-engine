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
	uniformLocationTable: RefCell<HashMap<String, Option<crate::core::graphic::GlUniformLocation>>>,
	attributeLocationTable: RefCell<HashMap<String, Option<u32>>>,
}

impl ShaderProgram {
	//==============================================================================
	// 생성. (컴파일 / 링크 실패면 로그를 담은 오류)
	//==============================================================================
	pub fn new(gl: &glow::Context, vertexShaderSource: &str, fragmentShaderSource: &str) -> Result<ShaderProgram, String> {
		ShaderProgram::createWithAttributeLocations(gl, vertexShaderSource, fragmentShaderSource, &[])
	}

	//==============================================================================
	// 어트리뷰트 위치를 링크 전에 고정해 생성. (원본 생성자의 attributeLocationTable 인자, 다른 프로그램의 버텍스 어레이를 함께 쓸 때)
	//==============================================================================
	pub fn createWithAttributeLocations(gl: &glow::Context, vertexShaderSource: &str, fragmentShaderSource: &str, attributeLocationList: &[(&str, u32)]) -> Result<ShaderProgram, String> {
		let desktopVertexShaderSource = convertToDesktopSource(vertexShaderSource);
		let desktopFragmentShaderSource = convertToDesktopSource(fragmentShaderSource);
		unsafe {
			let vertexShader = compileShader(gl, glow::VERTEX_SHADER, &desktopVertexShaderSource)?;
			let fragmentShader = compileShader(gl, glow::FRAGMENT_SHADER, &desktopFragmentShaderSource)?;
			let program = gl.create_program()?;
			gl.attach_shader(program, vertexShader);
			gl.attach_shader(program, fragmentShader);
			for (attributeName, attributeLocation) in attributeLocationList {
				gl.bind_attrib_location(program, *attributeLocation, attributeName);
			}
			gl.link_program(program);
			let isLinked = gl.get_program_link_status(program);
			gl.detach_shader(program, vertexShader);
			gl.detach_shader(program, fragmentShader);
			gl.delete_shader(vertexShader);
			gl.delete_shader(fragmentShader);
			if !isLinked {
				let programInfoLog = gl.get_program_info_log(program);
				gl.delete_program(program);
				return Err(format!("셰이더 프로그램 링크 실패: {}", programInfoLog));
			}
			Ok(ShaderProgram {
				program,
				uniformLocationTable: RefCell::new(HashMap::new()),
				attributeLocationTable: RefCell::new(HashMap::new()),
			})
		}
	}

	//==============================================================================
	// 사용. (원본 use(). 러스트에서 use 는 예약어라 이름을 바꿨다)
	//==============================================================================
	pub fn useProgram(&self, gl: &glow::Context) {
		unsafe {
			gl.use_program(Some(self.program));
		}
	}

	//==============================================================================
	// 유니폼 위치 반환.
	//==============================================================================
	// 웹 판 때문에 clone 을 쓴다. (데스크탑에서는 Copy 라 clippy 가 말린다)
	#[allow(clippy::clone_on_copy)]
	pub fn getUniformLocation(&self, gl: &glow::Context, name: &str) -> Option<crate::core::graphic::GlUniformLocation> {
		let mut uniformLocationTable = self.uniformLocationTable.borrow_mut();
		if let Some(location) = uniformLocationTable.get(name) {
			// 웹의 위치 객체는 Copy 가 아니라 복제한다. (데스크탑은 정수 하나라 복사와 같다)
			return location.clone();
		}
		let location = unsafe {
			gl.get_uniform_location(self.program, name)
		};
		uniformLocationTable.insert(name.to_string(), location.clone());
		location
	}

	//==============================================================================
	// 어트리뷰트 위치 반환.
	//==============================================================================
	pub fn getAttributeLocation(&self, gl: &glow::Context, name: &str) -> Option<u32> {
		let mut attributeLocationTable = self.attributeLocationTable.borrow_mut();
		if let Some(location) = attributeLocationTable.get(name) {
			return *location;
		}
		let location = unsafe {
			gl.get_attrib_location(self.program, name)
		};
		attributeLocationTable.insert(name.to_string(), location);
		location
	}

	//==============================================================================
	// GL 프로그램 반환.
	//==============================================================================
	pub fn getProgram(&self) -> crate::core::graphic::GlProgram {
		self.program
	}
}

//==============================================================================
// GLSL ES 3.00 머리를 데스크탑 GLSL 3.30 으로 바꾼다. (웹 WebGL2 에서는 원본 그대로)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn convertToDesktopSource(source: &str) -> String {
	source.replacen("#version 300 es", "#version 330 core", 1)
}

#[cfg(target_arch = "wasm32")]
fn convertToDesktopSource(source: &str) -> String {
	source.to_string()
}

//==============================================================================
// 셰이더 하나 컴파일.
//==============================================================================
unsafe fn compileShader(gl: &glow::Context, shaderType: u32, source: &str) -> Result<crate::core::graphic::GlShader, String> {
	let shader = gl.create_shader(shaderType)?;
	gl.shader_source(shader, source);
	gl.compile_shader(shader);
	if !gl.get_shader_compile_status(shader) {
		let shaderInfoLog = gl.get_shader_info_log(shader);
		gl.delete_shader(shader);
		return Err(format!("셰이더 컴파일 실패: {}", shaderInfoLog));
	}
	Ok(shader)
}

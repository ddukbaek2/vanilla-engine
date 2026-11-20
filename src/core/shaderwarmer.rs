//==============================================================================
// 셰이더 미리 링크. (vanilla.js core/graphic/shaderwarmer.js 의 ShaderWarmer)
// - ShaderProgram 은 링크하자마자 결과를 물어 링크가 끝날 때까지 멈춘다. 셰이더 여럿을 처음 그릴 때 한꺼번에 지으면 그 프레임이 멈칫한다.
// - 켤 때(로고, 로딩 동안) 같은 소스를 KHR_parallel_shader_compile 로 뒤에서 링크해 두면 드라이버(웹은 브라우저)의 프로그램 캐시에 남아,
//   나중에 ShaderProgram 이 같은 소스를 지을 때 금방 끝난다.
// - 확장이 없으면 아무것도 하지 않는다. (확장 없이 링크를 걸어 두면 그 뒤의 그리기 명령이 링크를 기다려 오히려 멈출 수 있다)
// - 링크 결과는 묻지 않고, 다 링크되면 프로그램과 셰이더를 지운다. (캐시는 남는다, 소스가 틀려도 여기서는 오류를 내지 않는다)
// - 사용: ShaderWarmer::new(gl, &[(정점 소스, 조각 소스, 어트리뷰트 위치 목록)]) 뒤 매 프레임 update(gl).
//   어트리뷰트 위치는 ShaderProgram::create_with_attribute_locations 에 넘길 것과 같게 줘야 캐시에 걸린다.
//==============================================================================
use glow::HasContext;

use crate::core::graphic::{GlProgram, GlShader};
use crate::core::shaderprogram::convert_to_desktop_source;

// 링크가 끝났는지 묻는 값. (KHR_parallel_shader_compile 의 COMPLETION_STATUS_KHR)
const COMPLETION_STATUS: u32 = 0x91B1;

// 미리 링크할 소스 하나. (정점 셰이더, 조각 셰이더, 어트리뷰트 위치 목록)
pub type ShaderWarmerSource<'a> = (&'a str, &'a str, &'a [(&'a str, u32)]);

pub struct ShaderWarmer {
	is_supported: bool,
	pending_program_list: Vec<(GlProgram, GlShader, GlShader)>,
}

impl ShaderWarmer {
	//==============================================================================
	// 생성. (확장이 있으면 소스를 모두 컴파일하고 링크를 걸어 둔다)
	//==============================================================================
	pub fn new(gl: &glow::Context, program_source_list: &[ShaderWarmerSource]) -> ShaderWarmer {
		let is_supported = gl.supported_extensions().iter().any(|extension_name| extension_name.contains("parallel_shader_compile"));
		let mut shader_warmer = ShaderWarmer { is_supported, pending_program_list: Vec::new() };
		if !is_supported {
			return shader_warmer;
		}
		for (vertex_shader_source, fragment_shader_source, attribute_location_list) in program_source_list {
			if let Some(pending_program) = ShaderWarmer::link_program(gl, vertex_shader_source, fragment_shader_source, attribute_location_list) {
				shader_warmer.pending_program_list.push(pending_program);
			}
		}
		shader_warmer
	}

	//==============================================================================
	// 하나를 컴파일하고 링크 걸기. (ShaderProgram 과 같은 차례, 결과는 묻지 않는다)
	//==============================================================================
	fn link_program(gl: &glow::Context, vertex_shader_source: &str, fragment_shader_source: &str, attribute_location_list: &[(&str, u32)]) -> Option<(GlProgram, GlShader, GlShader)> {
		let desktop_vertex_shader_source = convert_to_desktop_source(vertex_shader_source);
		let desktop_fragment_shader_source = convert_to_desktop_source(fragment_shader_source);
		unsafe {
			let vertex_shader = gl.create_shader(glow::VERTEX_SHADER).ok()?;
			gl.shader_source(vertex_shader, &desktop_vertex_shader_source);
			gl.compile_shader(vertex_shader);
			let fragment_shader = gl.create_shader(glow::FRAGMENT_SHADER).ok()?;
			gl.shader_source(fragment_shader, &desktop_fragment_shader_source);
			gl.compile_shader(fragment_shader);
			let program = gl.create_program().ok()?;
			gl.attach_shader(program, vertex_shader);
			gl.attach_shader(program, fragment_shader);
			for (attribute_name, attribute_location) in attribute_location_list {
				gl.bind_attrib_location(program, *attribute_location, attribute_name);
			}
			gl.link_program(program);
			Some((program, vertex_shader, fragment_shader))
		}
	}

	//==============================================================================
	// 매 프레임. (링크가 끝난 것을 지운다, 끝났는지 묻는 것은 기다리지 않는다)
	//==============================================================================
	pub fn update(&mut self, gl: &glow::Context) {
		if self.pending_program_list.is_empty() {
			return;
		}
		self.pending_program_list.retain(|(program, vertex_shader, fragment_shader)| {
			let is_completed = unsafe { gl.get_program_parameter_i32(*program, COMPLETION_STATUS) != 0 };
			if !is_completed {
				return true;
			}
			unsafe {
				gl.delete_program(*program);
				gl.delete_shader(*vertex_shader);
				gl.delete_shader(*fragment_shader);
			}
			false
		});
	}

	//==============================================================================
	// 다 링크했는지. (확장이 없어 하지 않았으면 처음부터 true)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.pending_program_list.is_empty()
	}

	//==============================================================================
	// 확장을 쓸 수 있는지. (false 면 미리 링크를 하지 않는다)
	//==============================================================================
	pub fn is_supported(&self) -> bool {
		self.is_supported
	}
}

//==============================================================================
//! 풀스크린 패스. (화면 전체를 덮는 삼각형 1개 + 프래그먼트 셰이더)
//! - 원본 core/fullscreenpass.js 를 옮긴 것이다. 배경 / 후처리 같은 화면 단위 효과의 기본 단위. (ScreenEffect 가 쓴다)
//! - 버텍스 버퍼 없이 gl_VertexID 로 삼각형을 만든다. 빈 버텍스 어레이를 묶어 그린다. (GL 3.3 코어도 WebGL2 도 버텍스 어레이만 묶여 있으면 된다)
//! - 셰이더는 원본과 같은 GLSL ES 3.00 이다. 데스크탑은 ShaderProgram 이 머리만 #version 330 core 로 바꿔 컴파일한다.
//! - 원본과 다른 점: GL 컨텍스트를 들지 않고 함수마다 받는다. 만들기 실패는 예외 대신 Err 로 돌려준다. GL 자원은 destroy 로 지운다.
//==============================================================================
use crate::core::graphic::{GlUniformLocation, GlVertexArray};
use crate::core::shaderprogram::ShaderProgram;
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// 풀스크린 삼각형 버텍스 셰이더. (gl_VertexID 사용, 버텍스 버퍼 불필요)
/// - fragmentCoordinate: 클립 좌표(-1..1), fragmentTextureCoordinate: 텍스처 좌표(0..1).
const FULLSCREEN_VERTEXSHADER_SOURCE: &str = "#version 300 es
out vec2 fragmentCoordinate;
out vec2 fragmentTextureCoordinate;
void main() {
	vec2 positions[3] = vec2[3](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
	vec2 position = positions[gl_VertexID];
	fragmentCoordinate = position;
	fragmentTextureCoordinate = position * 0.5 + 0.5;
	gl_Position = vec4(position, 0.999, 1.0);
}
";

//==============================================================================
/// 풀스크린 패스. (화면 전체를 덮는 삼각형 하나를 프래그먼트 셰이더로 칠한다)
/// - 프래그먼트 셰이더는 fragmentCoordinate(클립 좌표, -1 ~ 1)와 fragmentTextureCoordinate(텍스처 좌표, 0 ~ 1)를 in 으로 받을 수 있다.
/// - 쓰는 차례: use_program, 유니폼 올리기, draw. 다 쓰면 destroy 로 GL 자원을 지운다.
///
/// # 예제
/// 화면 가장자리를 어둡게 덮기
/// ```rust,no_run
/// use glow::HasContext;
/// use vanilla::{FullscreenPass, Graphic};
///
/// const VIGNETTE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
/// precision highp float;
/// in vec2 fragmentTextureCoordinate;
/// uniform float strength;
/// out vec4 outputColor;
/// void main() {
/// 	float centerDistance = length(fragmentTextureCoordinate - 0.5);
/// 	outputColor = vec4(0.0, 0.0, 0.0, clamp(centerDistance * strength, 0.0, 1.0));
/// }
/// ";
///
/// fn create_vignette(graphic: &Graphic) -> Result<FullscreenPass, String> {
/// 	FullscreenPass::new(graphic.get_gl(), VIGNETTE_FRAGMENTSHADER_SOURCE)
/// }
///
/// fn draw_vignette(graphic: &mut Graphic, fullscreen_pass: &FullscreenPass) {
/// 	graphic.run_external_pass(false, |gl| {
/// 		fullscreen_pass.use_program(gl);
/// 		unsafe {
/// 			gl.uniform_1_f32(fullscreen_pass.get_uniform_location(gl, "strength").as_ref(), 1.2);
/// 		}
/// 		fullscreen_pass.draw(gl);
/// 	});
/// }
///
/// fn destroy_vignette(graphic: &Graphic, fullscreen_pass: FullscreenPass) {
/// 	fullscreen_pass.destroy(graphic.get_gl());
/// }
/// ```
//==============================================================================
pub struct FullscreenPass {
	shader_program: ShaderProgram,
	vertex_array: GlVertexArray,
}

impl FullscreenPass {
	//==============================================================================
	/// 생성. (프래그먼트 셰이더 컴파일 / 링크 실패면 로그를 담은 오류)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트. (Graphic 의 get_gl)
	/// - `fragment_shader_source`: 프래그먼트 셰이더 소스. (GLSL ES 3.00, 앞뒤 빈칸은 뗀다)
	///
	/// # 돌려줌
	/// 만든 패스. 셰이더 컴파일이나 링크, 버텍스 어레이 만들기에 실패하면 그 오류 글을 담은 Err.
	//==============================================================================
	pub fn new(gl: &glow::Context, fragment_shader_source: &str) -> Result<FullscreenPass, String> {
		let shader_program = ShaderProgram::new(gl, FULLSCREEN_VERTEXSHADER_SOURCE.trim(), fragment_shader_source.trim())?;
		let vertex_array = unsafe {
			gl.create_vertex_array()?
		};
		Ok(FullscreenPass {
			shader_program,
			vertex_array,
		})
	}

	//==============================================================================
	/// 패스 사용 시작. (프로그램 + 빈 버텍스 어레이 바인드, 원본 useProgram)
	///
	/// # 인자
	/// - `gl`: 패스를 만든 GL 컨텍스트.
	//==============================================================================
	pub fn use_program(&self, gl: &glow::Context) {
		self.shader_program.use_program(gl);
		unsafe {
			gl.bind_vertex_array(Some(self.vertex_array));
		}
	}

	//==============================================================================
	/// 출력. (use_program 과 유니폼 설정 이후 호출)
	///
	/// # 인자
	/// - `gl`: 패스를 만든 GL 컨텍스트. (정점 3 개로 삼각형 하나를 그린다)
	//==============================================================================
	pub fn draw(&self, gl: &glow::Context) {
		unsafe {
			gl.draw_arrays(glow::TRIANGLES, 0, 3);
		}
	}

	//==============================================================================
	/// 유니폼 위치 반환. (캐시)
	///
	/// # 인자
	/// - `gl`: 패스를 만든 GL 컨텍스트.
	/// - `uniform_name`: 유니폼 이름. (셰이더 소스의 이름 그대로)
	///
	/// # 돌려줌
	/// 유니폼 위치. 그 이름의 활성 유니폼이 없으면 None.
	//==============================================================================
	pub fn get_uniform_location(&self, gl: &glow::Context, uniform_name: &str) -> Option<GlUniformLocation> {
		self.shader_program.get_uniform_location(gl, uniform_name)
	}

	//==============================================================================
	/// 셰이더 프로그램 반환.
	///
	/// # 돌려줌
	/// 패스의 셰이더 프로그램.
	//==============================================================================
	pub fn get_shader_program(&self) -> &ShaderProgram {
		&self.shader_program
	}

	//==============================================================================
	/// 버텍스 어레이 반환.
	///
	/// # 돌려줌
	/// 그릴 때 묶는 빈 버텍스 어레이.
	//==============================================================================
	pub fn get_vertex_array(&self) -> GlVertexArray {
		self.vertex_array
	}

	//==============================================================================
	/// 파괴. (GL 프로그램과 버텍스 어레이를 지운다)
	///
	/// # 인자
	/// - `gl`: 패스를 만든 GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&self, gl: &glow::Context) {
		unsafe {
			gl.delete_program(self.shader_program.get_program());
			gl.delete_vertex_array(self.vertex_array);
		}
	}
}

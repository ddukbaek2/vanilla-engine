//==============================================================================
// 풀스크린 패스. (화면 전체를 덮는 삼각형 1개 + 프래그먼트 셰이더)
// - 원본 experimental/graphics/fullscreenpass.js 를 옮긴 것이다. 배경 / 후처리 같은 화면 단위 효과의 기본 단위. (ScreenEffect 가 쓴다)
// - 버텍스 버퍼 없이 gl_VertexID 로 삼각형을 만든다. 빈 버텍스 어레이를 묶어 그린다. (GL 3.3 코어도 WebGL2 도 버텍스 어레이만 묶여 있으면 된다)
// - 셰이더는 원본과 같은 GLSL ES 3.00 이다. 데스크탑은 ShaderProgram 이 머리만 #version 330 core 로 바꿔 컴파일한다.
// - 원본과 다른 점: GL 컨텍스트를 들지 않고 함수마다 받는다. 만들기 실패는 예외 대신 Err 로 돌려준다. GL 자원은 destroy 로 지운다.
//==============================================================================
use crate::core::graphic::{GlUniformLocation, GlVertexArray};
use crate::core::shaderprogram::ShaderProgram;
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 풀스크린 삼각형 버텍스 셰이더. (gl_VertexID 사용, 버텍스 버퍼 불필요)
// - fragmentCoordinate: 클립 좌표(-1..1), fragmentTextureCoordinate: 텍스처 좌표(0..1).
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

pub struct FullscreenPass {
	shaderProgram: ShaderProgram,
	vertexArray: GlVertexArray,
}

impl FullscreenPass {
	//==============================================================================
	// 생성. (프래그먼트 셰이더 컴파일 / 링크 실패면 로그를 담은 오류)
	//==============================================================================
	pub fn new(gl: &glow::Context, fragmentShaderSource: &str) -> Result<FullscreenPass, String> {
		let shaderProgram = ShaderProgram::new(gl, FULLSCREEN_VERTEXSHADER_SOURCE.trim(), fragmentShaderSource.trim())?;
		let vertexArray = unsafe {
			gl.create_vertex_array()?
		};
		Ok(FullscreenPass {
			shaderProgram,
			vertexArray,
		})
	}

	//==============================================================================
	// 패스 사용 시작. (프로그램 + 빈 버텍스 어레이 바인드, 원본 use())
	//==============================================================================
	pub fn useProgram(&self, gl: &glow::Context) {
		self.shaderProgram.useProgram(gl);
		unsafe {
			gl.bind_vertex_array(Some(self.vertexArray));
		}
	}

	//==============================================================================
	// 출력. (useProgram 과 유니폼 설정 이후 호출)
	//==============================================================================
	pub fn draw(&self, gl: &glow::Context) {
		unsafe {
			gl.draw_arrays(glow::TRIANGLES, 0, 3);
		}
	}

	//==============================================================================
	// 유니폼 위치 반환. (캐시)
	//==============================================================================
	pub fn getUniformLocation(&self, gl: &glow::Context, uniformName: &str) -> Option<GlUniformLocation> {
		self.shaderProgram.getUniformLocation(gl, uniformName)
	}

	//==============================================================================
	// 셰이더 프로그램 반환.
	//==============================================================================
	pub fn getShaderProgram(&self) -> &ShaderProgram {
		&self.shaderProgram
	}

	//==============================================================================
	// 버텍스 어레이 반환.
	//==============================================================================
	pub fn getVertexArray(&self) -> GlVertexArray {
		self.vertexArray
	}

	//==============================================================================
	// 파괴. (GL 프로그램과 버텍스 어레이를 지운다)
	//==============================================================================
	pub fn destroy(&self, gl: &glow::Context) {
		unsafe {
			gl.delete_program(self.shaderProgram.getProgram());
			gl.delete_vertex_array(self.vertexArray);
		}
	}
}

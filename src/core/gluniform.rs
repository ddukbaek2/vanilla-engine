//==============================================================================
//! GPU 경계의 수 바꾸기. (로직 셈의 f64 를 셰이더 유니폼과 정점 속성의 f32 로)
//! - 원본은 자바스크립트 수(f64)로 셈하고 WebGL 의 uniform1f, uniform4f, vertexAttrib4f 에 넘기면 브라우저가 float 로 바꾼다.
//!   러스트 판도 엔진 셈은 f64 로 하고, 이 함수들이 GPU 로 넘기는 자리에서만 f32 로 바꾼다. (언리얼 5 의 큰 세계 좌표처럼 CPU 는 double, 셰이더는 float)
//! - 이름의 f64 는 받는 수의 꼴이다. GPU 에 가는 값은 glow 의 f32 함수 그대로다.
//==============================================================================
use glow::HasContext;

//==============================================================================
/// f64 를 받아 f32 유니폼과 정점 속성으로 넘기는 GL 함수들. (glow::Context 에 붙는다, glow 의 GL 함수처럼 unsafe 다)
//==============================================================================
pub(crate) trait GlUniformF64 {
	/// float 유니폼 하나. (원본 gl.uniform1f)
	unsafe fn uniform_1_f64(&self, location: Option<&glow::UniformLocation>, x: f64);
	/// vec2 유니폼. (원본 gl.uniform2f)
	unsafe fn uniform_2_f64(&self, location: Option<&glow::UniformLocation>, x: f64, y: f64);
	/// vec3 유니폼. (원본 gl.uniform3f)
	unsafe fn uniform_3_f64(&self, location: Option<&glow::UniformLocation>, x: f64, y: f64, z: f64);
	/// vec4 유니폼. (원본 gl.uniform4f)
	unsafe fn uniform_4_f64(&self, location: Option<&glow::UniformLocation>, x: f64, y: f64, z: f64, w: f64);
	/// 배열을 끄지 않은 정점 속성의 고정 값. (원본 gl.vertexAttrib4f)
	unsafe fn vertex_attrib_4_f64(&self, index: u32, x: f64, y: f64, z: f64, w: f64);
}

impl GlUniformF64 for glow::Context {
	unsafe fn uniform_1_f64(&self, location: Option<&glow::UniformLocation>, x: f64) {
		unsafe {
			self.uniform_1_f32(location, x as f32);
		}
	}

	unsafe fn uniform_2_f64(&self, location: Option<&glow::UniformLocation>, x: f64, y: f64) {
		unsafe {
			self.uniform_2_f32(location, x as f32, y as f32);
		}
	}

	unsafe fn uniform_3_f64(&self, location: Option<&glow::UniformLocation>, x: f64, y: f64, z: f64) {
		unsafe {
			self.uniform_3_f32(location, x as f32, y as f32, z as f32);
		}
	}

	unsafe fn uniform_4_f64(&self, location: Option<&glow::UniformLocation>, x: f64, y: f64, z: f64, w: f64) {
		unsafe {
			self.uniform_4_f32(location, x as f32, y as f32, z as f32, w as f32);
		}
	}

	unsafe fn vertex_attrib_4_f64(&self, index: u32, x: f64, y: f64, z: f64, w: f64) {
		unsafe {
			self.vertex_attrib_4_f32(index, x as f32, y as f32, z as f32, w as f32);
		}
	}
}

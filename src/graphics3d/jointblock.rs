//==============================================================================
//! 뼈 행렬 유니폼 블록. (원본 graphics3d/jointblock.js)
//! - 뼈 행렬 96 개는 vec4 384 칸이라 WebGL2 가 보장하는 최소 정점 유니폼 256 칸(MAX_VERTEX_UNIFORM_VECTORS)을 넘는다.
//!   데스크탑 GPU 는 4096 칸이라 넘어도 돌지만 안드로이드 크롬(갤럭시 Z 폴드3)은 링크에 실패한다. 큰 배열은 유니폼 블록으로 넘긴다.
//!   (모바일에서 유니티는 스키닝 행렬을 상수 버퍼로, 언리얼은 뼈 버퍼를 읽어 넘기는 것과 같은 길이다)
//! - 블록은 std140 이라 mat4 는 열 우선 vec4 넷이고 f32 16 개가 그대로 들어간다. 셰이더 본문은 jointMatrices[번호] 그대로 쓴다.
//! - 바인딩 자리는 ShaderProgram 이 링크 뒤에 JOINT_BLOCK_BINDING 으로 묶는다.
//! - SkinnedModelRenderer 에 대체 버텍스 셰이더를 줄 때는 `uniform mat4 jointMatrices[96];` 대신 JOINT_BLOCK_GLSL 을 끼운다.
//!
//! # 예제
//! 대체 스키닝 버텍스 셰이더에 뼈 행렬 블록 끼우기
//! ```rust
//! use vanilla::{JOINT_BLOCK_GLSL, JOINT_BLOCK_MAXIMUM};
//! let vertex_shader_source = format!("#version 300 es\nlayout(location = 0) in vec3 vertexPosition;\nlayout(location = 3) in uvec4 vertexJoints;\nlayout(location = 4) in vec4 vertexWeights;\n{}\nvoid main() {{\n\tmat4 skinMatrix = vertexWeights.x * jointMatrices[vertexJoints.x] + vertexWeights.y * jointMatrices[vertexJoints.y];\n\tgl_Position = skinMatrix * vec4(vertexPosition, 1.0);\n}}", JOINT_BLOCK_GLSL);
//! assert!(vertex_shader_source.contains("uniform JointBlock"));
//! assert_eq!(JOINT_BLOCK_MAXIMUM, 96);
//! ```
//==============================================================================
use crate::core::graphic::GlBuffer;
use crate::core::glcontext::GlContext;
use crate::core::shaderprogram::JOINT_BLOCK_BINDING;
use std::cell::{Cell, RefCell};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// 뼈 행렬 블록의 뼈 최대 수. (툰 렌더러 TOON_MAXIMUM_JOINTS 와 스키닝 렌더러가 같다)
pub const JOINT_BLOCK_MAXIMUM: usize = 96;

/// 모프 가중치 배열의 vec4 칸 수. (MORPH_TARGET_MAXIMUM 96 개를 네 개씩)
pub const MORPH_WEIGHT_VECTOR_COUNT: usize = 24;

/// 버텍스 셰이더에 끼우는 뼈 행렬 블록 선언. (원본 JOINT_BLOCK_GLSL 과 글자까지 같다)
macro_rules! joint_block_glsl_source {
	() => {
		"layout(std140) uniform JointBlock {
	mat4 jointMatrices[96];
};"
	};
}
pub(crate) use joint_block_glsl_source;

/// 뼈 행렬 블록 선언. (원본 JOINT_BLOCK_GLSL)
pub const JOINT_BLOCK_GLSL: &str = joint_block_glsl_source!();

//==============================================================================
/// 뼈 행렬 블록 버퍼. (원본 JointBlockBuffer, 컨텍스트마다 렌더러가 하나씩 갖는다)
///
/// # 예제
/// ```rust
/// use vanilla::JOINT_BLOCK_MAXIMUM;
/// // 버퍼는 GL 컨텍스트가 있어야 지으므로 여기서는 크기만 본다. (뼈 96 개 x mat4 64 바이트 = 6144 바이트, MAX_UNIFORM_BLOCK_SIZE 최소 16384 안)
/// assert!(JOINT_BLOCK_MAXIMUM * 64 <= 16384);
/// ```
//==============================================================================
pub struct JointBlockBuffer {
	buffer: GlBuffer,
	/// 마지막으로 올린 값. (같으면 올리지 않는다)
	last_values: RefCell<Vec<f32>>,
	/// 마지막으로 올린 수. (0 이면 아직 안 올림)
	last_count: Cell<usize>,
}

impl JointBlockBuffer {
	//==============================================================================
	/// 생성. (버퍼를 짓지 못하면 그 오류 글을 담은 Err)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	///
	/// # 돌려줌
	/// 뼈 행렬 6144 바이트를 담을 유니폼 버퍼. 버퍼를 짓지 못하면 Err(오류 글).
	//==============================================================================
	pub fn new(gl: &GlContext) -> Result<JointBlockBuffer, String> {
		unsafe {
			let buffer = crate::core::glstatecache::create_buffer(gl)?;
			crate::core::glstatecache::bind_buffer(gl, glow::UNIFORM_BUFFER, Some(buffer));
			gl.buffer_data_size(glow::UNIFORM_BUFFER, (JOINT_BLOCK_MAXIMUM * 64) as i32, glow::DYNAMIC_DRAW);
			crate::core::glstatecache::bind_buffer(gl, glow::UNIFORM_BUFFER, None);
			Ok(JointBlockBuffer { buffer, last_values: RefCell::new(vec![0.0; JOINT_BLOCK_MAXIMUM * 16]), last_count: Cell::new(0) })
		}
	}

	//==============================================================================
	/// 뼈 행렬 올리고 바인딩 자리에 묶기. (JOINT_BLOCK_MAXIMUM 개를 넘는 뒤쪽은 버린다, 같은 값이면 올리지 않고 묶기만 한다)
	///
	/// # 인자
	/// - `gl`: 버퍼를 만든 GL 컨텍스트.
	/// - `values`: 뼈 행렬의 수. (16 의 배수)
	//==============================================================================
	pub fn upload(&self, gl: &GlContext, values: &[f32]) {
		let count = values.len().min(JOINT_BLOCK_MAXIMUM * 16) / 16 * 16;
		let source = &values[..count];
		let mut last_values = self.last_values.borrow_mut();
		let is_same = count == self.last_count.get() && last_values[..count] == *source;
		unsafe {
			gl.bind_buffer_base(glow::UNIFORM_BUFFER, JOINT_BLOCK_BINDING, Some(self.buffer));
			if is_same {
				return;
			}
			gl.buffer_sub_data_u8_slice(glow::UNIFORM_BUFFER, 0, bytes_of(source));
		}
		last_values[..count].copy_from_slice(source);
		self.last_count.set(count);
	}

	//==============================================================================
	/// 지우기.
	///
	/// # 인자
	/// - `gl`: 버퍼를 만든 GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&self, gl: &GlContext) {
		unsafe {
			crate::core::glstatecache::delete_buffer(gl, self.buffer);
		}
	}
}

//==============================================================================
// f32 조각을 바이트로 본다.
//==============================================================================
fn bytes_of(values: &[f32]) -> &[u8] {
	unsafe { std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values)) }
}

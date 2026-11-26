//==============================================================================
//! 블룸 이펙트. (원본 graphics3d/bloomeffect.js)
//! - 밝은 영역 추출 → 하프 해상도 가우시안 블러 핑퐁. render() 결과 텍스처를 최종 합성 패스에서 가산하는 방식으로 쓴다.
//! - 셰이더와 패스 순서, 기본값(임계 0.5 ~ 0.95, 반복 2)은 원본과 같다.
//! - 주고받는 판은 기본 RGBA8 이라 1 을 넘는 밝은 값이 잘린다. set_use_float_color(true) 면 RGBA16F 로 지어 HDR 값을 그대로 흐린다.
//! - GL 을 쓰는 함수는 gl 을 받고, 생성과 크기 변경은 GL 객체를 못 만들면 오류(Result)를 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::graphics3d::fullscreenpass::FullscreenPass;
use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// 밝은 영역 추출 패스. (씬 → 하프 해상도)
const BRIGHTPASS_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
uniform vec2 thresholdRange;
out vec4 outputColor;
void main() {
	vec3 sceneColor = texture(sceneTexture, fragmentTextureCoordinate).rgb;
	float brightness = max(max(sceneColor.r, sceneColor.g), sceneColor.b);
	float extractFactor = smoothstep(thresholdRange.x, thresholdRange.y, brightness);
	outputColor = vec4(sceneColor * extractFactor, 1.0);
}";

/// 9탭 가우시안 블러 패스. (가로/세로 분리)
const BLUR_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform vec2 blurDirection; // (1/width, 0) 또는 (0, 1/height)
out vec4 outputColor;
void main() {
	float weights[5] = float[5](0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
	vec3 color = texture(sourceTexture, fragmentTextureCoordinate).rgb * weights[0];
	for (int tapIndex = 1; tapIndex < 5; ++tapIndex) {
		vec2 tapOffset = blurDirection * float(tapIndex) * 1.6;
		color += texture(sourceTexture, fragmentTextureCoordinate + tapOffset).rgb * weights[tapIndex];
		color += texture(sourceTexture, fragmentTextureCoordinate - tapOffset).rgb * weights[tapIndex];
	}
	outputColor = vec4(color, 1.0);
}";

//==============================================================================
/// 블룸 이펙트.
///
/// # 예제
/// 씬 렌더 타겟에서 블룸을 만들고 결과를 합성 패스의 1 번 유닛에 묶기
/// ```rust,no_run
/// use glow::HasContext;
/// use vanilla::{BloomEffect, RenderTarget};
///
/// fn create_bloom(gl: &glow::Context) -> Result<BloomEffect, String> {
/// 	let mut bloom_effect = BloomEffect::new(gl)?;
/// 	bloom_effect.set_threshold(0.7, 0.95);
/// 	Ok(bloom_effect)
/// }
///
/// fn render_bloom(gl: &glow::Context, bloom_effect: &mut BloomEffect, scene_render_target: &RenderTarget) -> Result<(), String> {
/// 	bloom_effect.resize(gl, scene_render_target.get_width(), scene_render_target.get_height())?;
/// 	bloom_effect.render(gl, scene_render_target.get_color_texture(0));
/// 	unsafe {
/// 		gl.active_texture(glow::TEXTURE1);
/// 		gl.bind_texture(glow::TEXTURE_2D, bloom_effect.get_result_texture());
/// 		gl.active_texture(glow::TEXTURE0);
/// 	}
/// 	Ok(())
/// }
/// ```
//==============================================================================
pub struct BloomEffect {
	bright_pass: FullscreenPass,
	blur_pass: FullscreenPass,
	ping_render_target: RenderTarget,
	pong_render_target: RenderTarget,
	threshold_start: f32,
	threshold_end: f32,
	iteration_count: u32,
}

impl BloomEffect {
	//==============================================================================
	/// 생성. (패스 2개 + 2x2 렌더 타겟 2장, resize 로 크기를 맞춘다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트. (Graphic::get_gl 로 꺼낸다)
	///
	/// # 돌려줌
	/// 새 블룸 이펙트. (임계 0.5 ~ 0.95, 반복 2, RGBA8 판) 셰이더 컴파일이나 GL 객체 만들기에 실패하면 Err(오류 글).
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<BloomEffect, String> {
		let bright_pass = FullscreenPass::new(gl, BRIGHTPASS_FRAGMENTSHADER_SOURCE)?;
		let blur_pass = match FullscreenPass::new(gl, BLUR_FRAGMENTSHADER_SOURCE) {
			Ok(blur_pass) => {
				blur_pass
			}
			Err(error) => {
				bright_pass.destroy(gl);
				return Err(error);
			}
		};
		let (ping_render_target, pong_render_target) = match create_render_target_pair(gl, 2, 2, None) {
			Ok(render_target_pair) => {
				render_target_pair
			}
			Err(error) => {
				bright_pass.destroy(gl);
				blur_pass.destroy(gl);
				return Err(error);
			}
		};
		Ok(BloomEffect {
			bright_pass,
			blur_pass,
			ping_render_target,
			pong_render_target,
			threshold_start: 0.5,
			threshold_end: 0.95,
			iteration_count: 2,
		})
	}

	//==============================================================================
	/// 크기 변경. (씬 해상도 기준, 내부는 하프 해상도)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `scene_width`: 씬 가로 크기. (픽셀, 판은 그 절반, 최소 1)
	/// - `scene_height`: 씬 세로 크기. (픽셀, 판은 그 절반, 최소 1)
	///
	/// # 돌려줌
	/// 판을 맞췄으면 Ok. 다시 짓다 GL 객체를 못 만들면 Err(GL 의 오류 글).
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, scene_width: i32, scene_height: i32) -> Result<(), String> {
		let half_width = (scene_width >> 1).max(1);
		let half_height = (scene_height >> 1).max(1);
		self.ping_render_target.resize(gl, half_width, half_height)?;
		self.pong_render_target.resize(gl, half_width, half_height)?;
		Ok(())
	}

	//==============================================================================
	/// 블룸 렌더링. (호출 후 결과는 get_result_texture())
	/// - 부수 효과: 깊이/블렌드 비활성, TEXTURE0 활성, 프레임버퍼 바인드가 변경된다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `scene_color_texture`: 밝은 영역을 뽑을 씬 컬러 텍스처. (보통 씬 RenderTarget 의 get_color_texture(0))
	//==============================================================================
	pub fn render(&self, gl: &glow::Context, scene_color_texture: Option<GlTexture>) {
		unsafe {
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
			gl.active_texture(glow::TEXTURE0);
		}

		// 1. 밝은 영역 추출. (씬 → 핑)
		let ping_render_target = &self.ping_render_target;
		let pong_render_target = &self.pong_render_target;
		ping_render_target.bind(gl);
		let bright_pass = &self.bright_pass;
		bright_pass.use_program(gl);
		let bright_scene_texture_location = bright_pass.get_uniform_location(gl, "sceneTexture");
		let bright_threshold_range_location = bright_pass.get_uniform_location(gl, "thresholdRange");
		unsafe {
			gl.uniform_1_i32(bright_scene_texture_location.as_ref(), 0);
			gl.uniform_2_f32(bright_threshold_range_location.as_ref(), self.threshold_start, self.threshold_end);
			gl.bind_texture(glow::TEXTURE_2D, scene_color_texture);
		}
		bright_pass.draw(gl);

		// 2. 가로/세로 가우시안 블러 핑퐁.
		let blur_pass = &self.blur_pass;
		blur_pass.use_program(gl);
		let blur_source_texture_location = blur_pass.get_uniform_location(gl, "sourceTexture");
		let blur_direction_location = blur_pass.get_uniform_location(gl, "blurDirection");
		unsafe {
			gl.uniform_1_i32(blur_source_texture_location.as_ref(), 0);
		}
		for _blur_index in 0..self.iteration_count {
			// 가로. (핑 → 퐁)
			pong_render_target.bind(gl);
			let ping_width = ping_render_target.get_width() as f32;
			unsafe {
				gl.uniform_2_f32(blur_direction_location.as_ref(), 1.0 / ping_width, 0.0);
				gl.bind_texture(glow::TEXTURE_2D, ping_render_target.get_color_texture(0));
			}
			blur_pass.draw(gl);

			// 세로. (퐁 → 핑)
			ping_render_target.bind(gl);
			let pong_height = pong_render_target.get_height() as f32;
			unsafe {
				gl.uniform_2_f32(blur_direction_location.as_ref(), 0.0, 1.0 / pong_height);
				gl.bind_texture(glow::TEXTURE_2D, pong_render_target.get_color_texture(0));
			}
			blur_pass.draw(gl);
		}
	}

	//==============================================================================
	/// 결과 텍스처 반환. (render() 이후)
	///
	/// # 돌려줌
	/// 블러를 마친 텍스처. (핑 판의 컬러, 씬의 절반 크기) destroy 뒤면 None.
	//==============================================================================
	pub fn get_result_texture(&self) -> Option<GlTexture> {
		self.ping_render_target.get_color_texture(0)
	}

	//==============================================================================
	/// 추출 임계값 설정. (씬 밝기 특성에 맞게 조정)
	///
	/// # 인자
	/// - `threshold_start`: 뽑기 시작 밝기. (세 채널 가운데 가장 큰 값, 이보다 어두우면 뽑지 않는다, 기본 0.5)
	/// - `threshold_end`: 다 뽑는 밝기. (이보다 밝으면 그대로, 사이는 smoothstep, 기본 0.95)
	//==============================================================================
	pub fn set_threshold(&mut self, threshold_start: f32, threshold_end: f32) {
		self.threshold_start = threshold_start;
		self.threshold_end = threshold_end;
	}

	//==============================================================================
	/// 블러 반복 횟수 설정.
	///
	/// # 인자
	/// - `iteration_count`: 가로와 세로 블러 한 쌍을 되풀이할 횟수. (기본 2, 0 이면 밝은 영역 뽑기만)
	//==============================================================================
	pub fn set_iteration_count(&mut self, iteration_count: u32) {
		self.iteration_count = iteration_count;
	}

	//==============================================================================
	/// 실수 컬러 설정. (true 면 주고받는 판을 RGBA16F 로 다시 지어 1 을 넘는 밝은 값이 잘리지 않는다, 기본 false 는 RGBA8, 같으면 그대로)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `use_float_color`: true 면 RGBA16F, false 면 RGBA8.
	///
	/// # 돌려줌
	/// 바꿨거나 이미 같으면 Ok. 새 판을 못 지으면 Err(GL 의 오류 글, 앞의 판은 그대로 둔다).
	///
	/// # 예제
	/// HDR 씬에서 1 을 넘는 빛을 잘리지 않게 흐리기
	/// ```rust,no_run
	/// use vanilla::BloomEffect;
	///
	/// fn create_hdr_bloom(gl: &glow::Context) -> Result<BloomEffect, String> {
	/// 	let mut bloom_effect = BloomEffect::new(gl)?;
	/// 	bloom_effect.set_use_float_color(gl, true)?;
	/// 	bloom_effect.set_threshold(1.0, 1.5);
	/// 	assert!(bloom_effect.get_use_float_color());
	/// 	Ok(bloom_effect)
	/// }
	/// ```
	//==============================================================================
	pub fn set_use_float_color(&mut self, gl: &glow::Context, use_float_color: bool) -> Result<(), String> {
		if self.get_use_float_color() == use_float_color {
			return Ok(());
		}
		let width = self.ping_render_target.get_width().max(1);
		let height = self.ping_render_target.get_height().max(1);
		let options = RenderTargetOptions { use_float_color, ..RenderTargetOptions::default() };
		let (ping_render_target, pong_render_target) = create_render_target_pair(gl, width, height, Some(options))?;
		self.ping_render_target.destroy(gl);
		self.pong_render_target.destroy(gl);
		self.ping_render_target = ping_render_target;
		self.pong_render_target = pong_render_target;
		Ok(())
	}

	//==============================================================================
	/// 실수 컬러 사용 여부 반환.
	///
	/// # 돌려줌
	/// 판이 RGBA16F 면 true, RGBA8 이면 false.
	//==============================================================================
	pub fn get_use_float_color(&self) -> bool {
		self.ping_render_target.get_use_float_color()
	}

	//==============================================================================
	/// 블러 반복 횟수 반환.
	///
	/// # 돌려줌
	/// 블러 반복 횟수.
	//==============================================================================
	pub fn get_iteration_count(&self) -> u32 {
		self.iteration_count
	}

	//==============================================================================
	/// 파괴. (GL 리소스 해제)
	///
	/// # 인자
	/// - `gl`: 만들 때 쓴 GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.ping_render_target.destroy(gl);
		self.pong_render_target.destroy(gl);
		self.bright_pass.destroy(gl);
		self.blur_pass.destroy(gl);
	}

	//==============================================================================
	/// 밝은 영역 추출 패스 반환.
	///
	/// # 돌려줌
	/// 밝은 영역 추출 패스. (유니폼 sceneTexture, thresholdRange)
	//==============================================================================
	pub fn get_bright_pass(&self) -> &FullscreenPass {
		&self.bright_pass
	}

	//==============================================================================
	/// 블러 패스 반환.
	///
	/// # 돌려줌
	/// 9 탭 가우시안 블러 패스. (유니폼 sourceTexture, blurDirection)
	//==============================================================================
	pub fn get_blur_pass(&self) -> &FullscreenPass {
		&self.blur_pass
	}

	//==============================================================================
	/// 핑 렌더 타겟 반환. (최종 결과 보관)
	///
	/// # 돌려줌
	/// 핑 렌더 타겟. (밝은 영역과 세로 블러를 받는 판, 결과 텍스처의 주인)
	//==============================================================================
	pub fn get_ping_render_target(&self) -> &RenderTarget {
		&self.ping_render_target
	}

	//==============================================================================
	/// 퐁 렌더 타겟 반환.
	///
	/// # 돌려줌
	/// 퐁 렌더 타겟. (가로 블러를 받는 판)
	//==============================================================================
	pub fn get_pong_render_target(&self) -> &RenderTarget {
		&self.pong_render_target
	}
}

//==============================================================================
/// 주고받는 렌더 타겟 한 쌍 생성. (깊이 없음)
///
/// # 돌려줌
/// (핑, 퐁). 둘째를 못 지으면 먼저 지은 첫째를 지우고 Err.
//==============================================================================
fn create_render_target_pair(gl: &glow::Context, width: i32, height: i32, options: Option<RenderTargetOptions>) -> Result<(RenderTarget, RenderTarget), String> {
	let mut ping_render_target = RenderTarget::new(gl, width, height, false, options)?;
	match RenderTarget::new(gl, width, height, false, options) {
		Ok(pong_render_target) => {
			Ok((ping_render_target, pong_render_target))
		}
		Err(error) => {
			ping_render_target.destroy(gl);
			Err(error)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::imagetexturecache::fake_gl::{self, FakeGlObject};

	//==============================================================================
	/// 지은 GL 객체 수. (텍스처, 프레임버퍼, 프로그램, 버텍스 어레이)
	//==============================================================================
	fn read_live_counts() -> [usize; 4] {
		[fake_gl::get_live_count(FakeGlObject::Texture), fake_gl::get_live_count(FakeGlObject::Framebuffer), fake_gl::get_live_count(FakeGlObject::Program), fake_gl::get_live_count(FakeGlObject::VertexArray)]
	}

	//==============================================================================
	/// 생성이나 실수 컬러 바꾸기가 중간에 실패해도 먼저 지은 GL 객체를 지우고, 바꾸기 실패는 예전 판을 그대로 둔다. (가짜 GL)
	//==============================================================================
	#[test]
	fn failure_deletes_partially_created_objects() {
		let gl = fake_gl::create_context();
		fake_gl::fail_after(FakeGlObject::Shader, 2);
		assert!(BloomEffect::new(&gl).is_err());
		assert_eq!(read_live_counts(), [0, 0, 0, 0]);

		let gl = fake_gl::create_context();
		fake_gl::fail_after(FakeGlObject::Framebuffer, 1);
		assert!(BloomEffect::new(&gl).is_err());
		assert_eq!(read_live_counts(), [0, 0, 0, 0]);

		let gl = fake_gl::create_context();
		let mut bloom_effect = BloomEffect::new(&gl).expect("bloom effect");
		assert_eq!(read_live_counts(), [2, 2, 2, 2]);
		fake_gl::fail_after(FakeGlObject::Framebuffer, 1);
		assert!(bloom_effect.set_use_float_color(&gl, true).is_err());
		assert_eq!(read_live_counts(), [2, 2, 2, 2]);
		assert!(!bloom_effect.get_use_float_color());
		fake_gl::fail_after(FakeGlObject::Framebuffer, 2);
		bloom_effect.set_use_float_color(&gl, true).expect("float color");
		assert!(bloom_effect.get_use_float_color());
		assert_eq!(read_live_counts(), [2, 2, 2, 2]);
		bloom_effect.destroy(&gl);
		assert_eq!(read_live_counts(), [0, 0, 0, 0]);
	}
}

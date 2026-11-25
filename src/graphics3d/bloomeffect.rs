//==============================================================================
//! 블룸 이펙트. (원본 experimental/graphics/bloomeffect.js)
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
/// 전역 상수 목록.
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
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<BloomEffect, String> {
		Ok(BloomEffect {
			bright_pass: FullscreenPass::new(gl, BRIGHTPASS_FRAGMENTSHADER_SOURCE)?,
			blur_pass: FullscreenPass::new(gl, BLUR_FRAGMENTSHADER_SOURCE)?,
			ping_render_target: RenderTarget::new(gl, 2, 2, false, None)?,
			pong_render_target: RenderTarget::new(gl, 2, 2, false, None)?,
			threshold_start: 0.5,
			threshold_end: 0.95,
			iteration_count: 2,
		})
	}

	//==============================================================================
	/// 크기 변경. (씬 해상도 기준, 내부는 하프 해상도)
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
	//==============================================================================
	pub fn get_result_texture(&self) -> Option<GlTexture> {
		self.ping_render_target.get_color_texture(0)
	}

	//==============================================================================
	/// 추출 임계값 설정. (씬 밝기 특성에 맞게 조정)
	//==============================================================================
	pub fn set_threshold(&mut self, threshold_start: f32, threshold_end: f32) {
		self.threshold_start = threshold_start;
		self.threshold_end = threshold_end;
	}

	//==============================================================================
	/// 블러 반복 횟수 설정.
	//==============================================================================
	pub fn set_iteration_count(&mut self, iteration_count: u32) {
		self.iteration_count = iteration_count;
	}

	//==============================================================================
	/// 실수 컬러 설정. (true 면 주고받는 판을 RGBA16F 로 다시 지어 1 을 넘는 밝은 값이 잘리지 않는다, 기본 false 는 RGBA8, 같으면 그대로)
	//==============================================================================
	pub fn set_use_float_color(&mut self, gl: &glow::Context, use_float_color: bool) -> Result<(), String> {
		if self.get_use_float_color() == use_float_color {
			return Ok(());
		}
		let width = self.ping_render_target.get_width().max(1);
		let height = self.ping_render_target.get_height().max(1);
		let options = RenderTargetOptions { use_float_color, ..RenderTargetOptions::default() };
		let ping_render_target = RenderTarget::new(gl, width, height, false, Some(options))?;
		let pong_render_target = RenderTarget::new(gl, width, height, false, Some(options))?;
		self.ping_render_target.destroy(gl);
		self.pong_render_target.destroy(gl);
		self.ping_render_target = ping_render_target;
		self.pong_render_target = pong_render_target;
		Ok(())
	}

	//==============================================================================
	/// 실수 컬러 사용 여부 반환.
	//==============================================================================
	pub fn get_use_float_color(&self) -> bool {
		self.ping_render_target.get_use_float_color()
	}

	//==============================================================================
	/// 블러 반복 횟수 반환.
	//==============================================================================
	pub fn get_iteration_count(&self) -> u32 {
		self.iteration_count
	}

	//==============================================================================
	/// 파괴. (GL 리소스 해제)
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.ping_render_target.destroy(gl);
		self.pong_render_target.destroy(gl);
		self.bright_pass.destroy(gl);
		self.blur_pass.destroy(gl);
	}

	//==============================================================================
	/// 밝은 영역 추출 패스 반환.
	//==============================================================================
	pub fn get_bright_pass(&self) -> &FullscreenPass {
		&self.bright_pass
	}

	//==============================================================================
	/// 블러 패스 반환.
	//==============================================================================
	pub fn get_blur_pass(&self) -> &FullscreenPass {
		&self.blur_pass
	}

	//==============================================================================
	/// 핑 렌더 타겟 반환. (최종 결과 보관)
	//==============================================================================
	pub fn get_ping_render_target(&self) -> &RenderTarget {
		&self.ping_render_target
	}

	//==============================================================================
	/// 퐁 렌더 타겟 반환.
	//==============================================================================
	pub fn get_pong_render_target(&self) -> &RenderTarget {
		&self.pong_render_target
	}
}

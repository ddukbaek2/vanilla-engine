//==============================================================================
// 화면 공간 서브서피스 스캐터링. (원본 experimental/graphics/subsurfacescatteringeffect.js)
// - 분리형, 확산 조도 버퍼를 깊이 인식 가로/세로 블러. 피부 확산 프로파일(가우시안 합)로 채널별 커널을 만들고,
//   산란 폭(월드 단위)을 깊이에 맞춰 화면에 투영한다.
// - render() 입력은 알파에 산란 마스크를 담은 조도 텍스처와 같은 패스의 깊이 텍스처. 결과는 get_result_texture().
// - GLSL, 커널 식, 기본값(산란 폭 0.2, 폴오프 (1, 0.37, 0.3), 강도 (0.48, 0.41, 0.28))은 원본과 같다.
//   원본 ${KERNEL_SAMPLE_COUNT} / ${KERNEL_RANGE} 는 숫자 17 / 3 으로 옮겼다.
// - GL 을 쓰는 함수는 gl 을 받고, 생성과 크기 변경은 GL 객체를 못 만들면 오류(Result)를 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::graphics3d::fullscreenpass::FullscreenPass;
use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 커널 샘플 수. (중앙 1 + 양측, 홀수, 셰이더의 17 과 같아야 한다)
pub const KERNEL_SAMPLE_COUNT: usize = 17;

// 커널 오프셋 범위. (확산 프로파일 단위, 셰이더에서 산란 폭으로 정규화, 셰이더의 3 과 같아야 한다)
pub const KERNEL_RANGE: f64 = 3.0;

// 분리형 산란 블러 패스. (깊이 인식, 표면이 갈라지는 곳은 중앙 색으로 되돌린다)
const SCATTER_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform sampler2D depthTexture;
uniform vec2 blurDirection;
uniform vec4 kernel[17];
uniform float scatterWidth;
uniform float distanceToProjectionWindow;
uniform float aspectRatio;
uniform vec2 depthRange;
out vec4 outputColor;

// 화면 높이 대비 최대 산란 폭. (커널 표본 수가 고정이라 이보다 넓으면 표본 간격이 벌어진다)
const float MAXIMUM_PROJECTED_WIDTH = 0.05;

float linearizeDepth(float depthSample) {
	float nearDistance = depthRange.x;
	float farDistance = depthRange.y;
	float normalizedDepth = depthSample * 2.0 - 1.0;
	return 2.0 * nearDistance * farDistance / (farDistance + nearDistance - normalizedDepth * (farDistance - nearDistance));
}

void main() {
	vec4 centerSample = texture(sourceTexture, fragmentTextureCoordinate);
	float centerMask = centerSample.a;
	if (centerMask < 0.001) {
		outputColor = centerSample;
		return;
	}
	float centerDepth = linearizeDepth(texture(depthTexture, fragmentTextureCoordinate).r);

	// 화면 공간 스텝. (산란 폭을 투영 창 거리 / 깊이로 화면 크기에 맞춤, 가로는 화면비 보정, 근접 시 표본 간격이 벌어져 띠가 지지 않도록 화면 높이 기준 폭을 제한)
	float projectedScale = distanceToProjectionWindow / centerDepth;
	float projectedWidth = min(scatterWidth * projectedScale, MAXIMUM_PROJECTED_WIDTH);
	vec2 finalStep = projectedWidth * blurDirection * centerMask / float(3);
	finalStep.x /= aspectRatio;

	vec3 color = centerSample.rgb * kernel[0].rgb;
	for (int sampleIndex = 1; sampleIndex < 17; ++sampleIndex) {
		vec2 sampleCoordinate = fragmentTextureCoordinate + kernel[sampleIndex].w * finalStep;
		vec4 sampleColor = texture(sourceTexture, sampleCoordinate);
		float sampleDepth = linearizeDepth(texture(depthTexture, sampleCoordinate).r);
		float depthDelta = abs(centerDepth - sampleDepth);
		float surfaceBreak = smoothstep(0.0, scatterWidth * 2.0, depthDelta);
		surfaceBreak = max(surfaceBreak, 1.0 - sampleColor.a);
		vec3 blendedColor = mix(sampleColor.rgb, centerSample.rgb, surfaceBreak);
		color += blendedColor * kernel[sampleIndex].rgb;
	}
	outputColor = vec4(color, centerMask);
}";

//==============================================================================
// 화면 공간 서브서피스 스캐터링.
//==============================================================================
pub struct SubsurfaceScatteringEffect {
	scatter_pass: FullscreenPass,
	ping_render_target: RenderTarget,
	pong_render_target: RenderTarget,
	kernel: [f32; KERNEL_SAMPLE_COUNT * 4],
	scatter_width: f32,
	falloff_color: [f32; 3],
	strength_color: [f32; 3],
}

impl SubsurfaceScatteringEffect {
	//==============================================================================
	// 생성. (패스 1개 + 반정밀 실수 타겟 2장, resize 로 크기를 맞춘다)
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<SubsurfaceScatteringEffect, String> {
		let float_color_options = RenderTargetOptions {
			use_float_color: true,
			..RenderTargetOptions::default()
		};
		let mut subsurface_scattering_effect = SubsurfaceScatteringEffect {
			scatter_pass: FullscreenPass::new(gl, SCATTER_FRAGMENTSHADER_SOURCE)?,
			ping_render_target: RenderTarget::new(gl, 2, 2, false, Some(float_color_options))?,
			pong_render_target: RenderTarget::new(gl, 2, 2, false, Some(float_color_options))?,
			kernel: [0.0; KERNEL_SAMPLE_COUNT * 4],
			scatter_width: 0.2,
			falloff_color: [1.0, 0.37, 0.3],
			strength_color: [0.48, 0.41, 0.28],
		};
		subsurface_scattering_effect.compute_kernel();
		Ok(subsurface_scattering_effect)
	}

	//==============================================================================
	// 커널 계산. (피부 확산 프로파일을 오프셋 구간 넓이로 적분, 중앙 샘플을 0번에)
	//==============================================================================
	pub fn compute_kernel(&mut self) {
		self.kernel = SubsurfaceScatteringEffect::build_kernel(self.falloff_color, self.strength_color);
	}

	//==============================================================================
	// 커널 만들기. (GL 무관, 표본마다 rgb 가중치 + w 오프셋)
	//==============================================================================
	pub fn build_kernel(falloff_color: [f32; 3], strength_color: [f32; 3]) -> [f32; KERNEL_SAMPLE_COUNT * 4] {
		let sample_count = KERNEL_SAMPLE_COUNT;

		// 채널별 확산 프로파일. (가우시안 합, 분산은 폴오프 색으로 채널마다 늘어난다)
		let evaluate_gaussian = |variance: f64, distance: f64, channel_index: usize| -> f64 {
			let scaled_distance = distance / (0.001 + falloff_color[channel_index] as f64);
			(-(scaled_distance * scaled_distance) / (2.0 * variance)).exp() / (2.0 * std::f64::consts::PI * variance)
		};
		let evaluate_profile = |distance: f64, channel_index: usize| -> f64 {
			0.100 * evaluate_gaussian(0.0484, distance, channel_index)
				+ 0.118 * evaluate_gaussian(0.187, distance, channel_index)
				+ 0.113 * evaluate_gaussian(0.567, distance, channel_index)
				+ 0.358 * evaluate_gaussian(1.99, distance, channel_index)
				+ 0.078 * evaluate_gaussian(7.41, distance, channel_index)
		};

		// 오프셋. (중앙에 촘촘하게, 부호 있는 제곱 분포)
		let mut offsets = Vec::with_capacity(sample_count);
		let step = (KERNEL_RANGE * 2.0) / (sample_count - 1) as f64;
		for sample_index in 0..sample_count {
			let linear_offset = -KERNEL_RANGE + sample_index as f64 * step;
			let sign = if linear_offset < 0.0 {
				-1.0
			}
			else {
				1.0
			};
			let normalized_offset = linear_offset.abs() / KERNEL_RANGE;
			offsets.push(KERNEL_RANGE * sign * normalized_offset * normalized_offset);
		}

		// 가중치. (프로파일 x 구간 넓이)
		let mut weights: Vec<[f64; 3]> = Vec::with_capacity(sample_count);
		for sample_index in 0..sample_count {
			let left_width = if sample_index > 0 {
				(offsets[sample_index] - offsets[sample_index - 1]).abs()
			}
			else {
				0.0
			};
			let right_width = if sample_index < sample_count - 1 {
				(offsets[sample_index] - offsets[sample_index + 1]).abs()
			}
			else {
				0.0
			};
			let area = (left_width + right_width) * 0.5;
			let mut weight = [0.0f64; 3];
			for (channel_index, channel_weight) in weight.iter_mut().enumerate() {
				*channel_weight = evaluate_profile(offsets[sample_index], channel_index) * area;
			}
			weights.push(weight);
		}

		// 중앙 샘플을 0번으로 이동.
		let center_index = (sample_count - 1) / 2;
		let mut ordered_offsets = vec![offsets[center_index]];
		let mut ordered_weights = vec![weights[center_index]];
		for sample_index in 0..sample_count {
			if sample_index != center_index {
				ordered_offsets.push(offsets[sample_index]);
				ordered_weights.push(weights[sample_index]);
			}
		}

		// 채널별 정규화 + 강도 적용. (중앙 = (1 - 강도) + 강도 x 가중, 나머지 = 강도 x 가중)
		let mut kernel = [0.0f32; KERNEL_SAMPLE_COUNT * 4];
		for channel_index in 0..3 {
			let weight_sum: f64 = ordered_weights.iter().map(|weight| weight[channel_index]).sum();
			let strength = strength_color[channel_index] as f64;
			for sample_index in 0..sample_count {
				let normalized_weight = ordered_weights[sample_index][channel_index] / weight_sum;
				let final_weight = if sample_index == 0 {
					(1.0 - strength) + strength * normalized_weight
				}
				else {
					strength * normalized_weight
				};
				kernel[sample_index * 4 + channel_index] = final_weight as f32;
			}
		}
		for sample_index in 0..sample_count {
			kernel[sample_index * 4 + 3] = ordered_offsets[sample_index] as f32;
		}
		kernel
	}

	//==============================================================================
	// 크기 변경. (조도 버퍼 해상도 기준, 전체 해상도 실수 타겟 2장)
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
		self.ping_render_target.resize(gl, width, height)?;
		self.pong_render_target.resize(gl, width, height)?;
		Ok(())
	}

	//==============================================================================
	// 산란 렌더링. (가로 → 핑, 세로 → 퐁, 결과는 get_result_texture())
	// - 부수 효과: 깊이/블렌드 비활성, TEXTURE0 활성, 프레임버퍼 바인드가 변경된다.
	//==============================================================================
	pub fn render(&self, gl: &glow::Context, irradiance_texture: Option<GlTexture>, depth_texture: Option<GlTexture>, field_of_view_radian: f32, aspect_ratio: f32, near_distance: f32, far_distance: f32) {
		unsafe {
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
		}

		let scatter_pass = &self.scatter_pass;
		scatter_pass.use_program(gl);
		let blur_direction_location = scatter_pass.get_uniform_location(gl, "blurDirection");
		unsafe {
			gl.uniform_1_i32(scatter_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
			gl.uniform_1_i32(scatter_pass.get_uniform_location(gl, "depthTexture").as_ref(), 1);
			gl.uniform_4_f32_slice(scatter_pass.get_uniform_location(gl, "kernel[0]").as_ref(), &self.kernel);
			gl.uniform_1_f32(scatter_pass.get_uniform_location(gl, "scatterWidth").as_ref(), self.scatter_width);
			gl.uniform_1_f32(scatter_pass.get_uniform_location(gl, "distanceToProjectionWindow").as_ref(), 1.0 / (field_of_view_radian * 0.5).tan());
			gl.uniform_1_f32(scatter_pass.get_uniform_location(gl, "aspectRatio").as_ref(), aspect_ratio);
			gl.uniform_2_f32(scatter_pass.get_uniform_location(gl, "depthRange").as_ref(), near_distance, far_distance);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, depth_texture);
			gl.active_texture(glow::TEXTURE0);
		}

		// 1. 가로. (조도 → 핑)
		let ping_render_target = &self.ping_render_target;
		ping_render_target.bind(gl);
		unsafe {
			gl.uniform_2_f32(blur_direction_location.as_ref(), 1.0, 0.0);
			gl.bind_texture(glow::TEXTURE_2D, irradiance_texture);
		}
		scatter_pass.draw(gl);

		// 2. 세로. (핑 → 퐁)
		let pong_render_target = &self.pong_render_target;
		pong_render_target.bind(gl);
		unsafe {
			gl.uniform_2_f32(blur_direction_location.as_ref(), 0.0, 1.0);
			gl.bind_texture(glow::TEXTURE_2D, ping_render_target.get_color_texture(0));
		}
		scatter_pass.draw(gl);

		// 깊이 텍스처 해제. (유닛 1 은 섀도우 샘플러 전용, 비교 모드 없는 텍스처가 남으면 이후 드로우가 무효화된다)
		unsafe {
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
		}
	}

	//==============================================================================
	// 결과 텍스처 반환. (render() 이후)
	//==============================================================================
	pub fn get_result_texture(&self) -> Option<GlTexture> {
		self.pong_render_target.get_color_texture(0)
	}

	//==============================================================================
	// 산란 폭 설정. (월드 단위, 커널 전체 반경)
	//==============================================================================
	pub fn set_scatter_width(&mut self, scatter_width: f32) {
		self.scatter_width = scatter_width;
	}

	//==============================================================================
	// 산란 폭 반환.
	//==============================================================================
	pub fn get_scatter_width(&self) -> f32 {
		self.scatter_width
	}

	//==============================================================================
	// 확산 프로파일 설정. (채널별 폴오프 / 강도, 피부 기본 (1, 0.37, 0.3) / (0.48, 0.41, 0.28))
	//==============================================================================
	pub fn set_profile(&mut self, falloff_color: [f32; 3], strength_color: [f32; 3]) {
		self.falloff_color = falloff_color;
		self.strength_color = strength_color;
		self.compute_kernel();
	}

	//==============================================================================
	// 폴오프 색 반환.
	//==============================================================================
	pub fn get_falloff_color(&self) -> [f32; 3] {
		self.falloff_color
	}

	//==============================================================================
	// 강도 색 반환.
	//==============================================================================
	pub fn get_strength_color(&self) -> [f32; 3] {
		self.strength_color
	}

	//==============================================================================
	// 파괴. (GL 리소스 해제)
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.ping_render_target.destroy(gl);
		self.pong_render_target.destroy(gl);
		self.scatter_pass.destroy(gl);
	}

	//==============================================================================
	// 산란 패스 반환.
	//==============================================================================
	pub fn get_scatter_pass(&self) -> &FullscreenPass {
		&self.scatter_pass
	}

	//==============================================================================
	// 핑 렌더 타겟 반환. (가로 블러 결과)
	//==============================================================================
	pub fn get_ping_render_target(&self) -> &RenderTarget {
		&self.ping_render_target
	}

	//==============================================================================
	// 퐁 렌더 타겟 반환. (최종 결과 보관)
	//==============================================================================
	pub fn get_pong_render_target(&self) -> &RenderTarget {
		&self.pong_render_target
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 커널: 채널마다 가중치 합 1, 0번은 중앙 오프셋 0, 오프셋 범위 ±3, 강도 0 이면 중앙만 1.
	//==============================================================================
	#[test]
	fn kernel_is_normalized_and_centered() {
		let kernel = SubsurfaceScatteringEffect::build_kernel([1.0, 0.37, 0.3], [0.48, 0.41, 0.28]);
		for channel_index in 0..3 {
			let weight_sum: f32 = (0..KERNEL_SAMPLE_COUNT).map(|sample_index| kernel[sample_index * 4 + channel_index]).sum();
			assert!((weight_sum - 1.0).abs() < 1.0e-5, "channel {} sum {}", channel_index, weight_sum);
		}
		assert_eq!(kernel[3], 0.0);
		let maximum_offset = (0..KERNEL_SAMPLE_COUNT).map(|sample_index| kernel[sample_index * 4 + 3].abs()).fold(0.0f32, f32::max);
		assert!((maximum_offset - 3.0).abs() < 1.0e-6);

		let identity_kernel = SubsurfaceScatteringEffect::build_kernel([1.0, 0.37, 0.3], [0.0, 0.0, 0.0]);
		assert_eq!(identity_kernel[0], 1.0);
		assert_eq!(identity_kernel[4], 0.0);
	}
}

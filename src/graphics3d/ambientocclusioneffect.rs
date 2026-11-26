//==============================================================================
//! 화면 공간 앰비언트 오클루전. (원본 graphics3d/ambientocclusioneffect.js)
//! - 깊이 + 노멀 버퍼 → 반구 표본 차폐 → 깊이 인식 블러. 결과는 get_result_texture() 의 r 채널 가시율(1 = 차폐 없음).
//! - render() 입력은 본 패스의 깊이 텍스처와 월드 노멀 텍스처(알파 = 커버리지), 카메라의 뷰 회전 / 투영 파라미터.
//! - GLSL 과 커널, 기본값(반경 0.03, 강도 1, 바이어스 0.002, 배율 0.5)은 원본과 같다. 원본 ${SAMPLE_COUNT} 는 숫자 16 으로 옮겼다.
//! - GL 을 쓰는 함수는 gl 을 받고, 생성과 크기 변경은 GL 객체를 못 만들면 오류(Result)를 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::graphics3d::fullscreenpass::FullscreenPass;
use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
use glow::HasContext;
use crate::core::gluniform::GlUniformF64;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// 반구 표본 수. (프래그먼트마다 회전 노이즈로 흩는다, 셰이더의 16 과 같아야 한다)
pub const SAMPLE_COUNT: usize = 16;

/// 차폐 패스. (깊이 + 월드 노멀 → 뷰 공간 반구 표본으로 차폐율, 결과 r 에 가시율)
const OCCLUSION_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D depthTexture;
uniform sampler2D normalTexture;
uniform mat3 viewRotation;
uniform vec2 depthRange;
uniform vec2 projectionScale;
uniform vec3 sampleKernel[16];
uniform float radius;
uniform float intensity;
uniform float bias;
out vec4 outputColor;

const float PI = 3.14159265;

float linearizeDepth(float depthSample) {
	float nearDistance = depthRange.x;
	float farDistance = depthRange.y;
	float normalizedDepth = depthSample * 2.0 - 1.0;
	return 2.0 * nearDistance * farDistance / (farDistance + nearDistance - normalizedDepth * (farDistance - nearDistance));
}

// 화면 좌표 + 선형 깊이 → 뷰 공간 위치. (카메라가 -z 를 본다)
vec3 reconstructViewPosition(vec2 textureCoordinate, float linearDepth) {
	vec2 normalized = textureCoordinate * 2.0 - 1.0;
	return vec3(normalized * projectionScale * linearDepth, -linearDepth);
}

// 인터리브드 그래디언트 잡음. (Jorge Jimenez, SIGGRAPH 2014 Next Generation Post Processing in Call of Duty: Advanced Warfare 발표에 실린 식)
float interleavedGradientNoise(vec2 screenPosition) {
	return fract(52.9829189 * fract(dot(screenPosition, vec2(0.06711056, 0.00583715))));
}

void main() {
	float depthSample = texture(depthTexture, fragmentTextureCoordinate).r;
	if (depthSample >= 1.0) {
		outputColor = vec4(1.0);
		return;
	}
	float linearDepth = linearizeDepth(depthSample);
	vec3 viewPosition = reconstructViewPosition(fragmentTextureCoordinate, linearDepth);
	vec4 normalSample = texture(normalTexture, fragmentTextureCoordinate);
	if (normalSample.a < 0.5) {
		outputColor = vec4(1.0);
		return;
	}
	vec3 viewNormal = normalize(viewRotation * normalSample.xyz);

	// 노멀 기준 반구 프레임. (회전 노이즈로 표본 방향을 흩는다)
	float angle = interleavedGradientNoise(gl_FragCoord.xy) * 2.0 * PI;
	vec3 randomVector = vec3(cos(angle), sin(angle), 0.0);
	vec3 tangent = normalize(randomVector - viewNormal * dot(randomVector, viewNormal));
	vec3 bitangent = cross(viewNormal, tangent);
	mat3 hemisphereFrame = mat3(tangent, bitangent, viewNormal);

	float occlusion = 0.0;
	for (int sampleIndex = 0; sampleIndex < 16; ++sampleIndex) {
		vec3 samplePosition = viewPosition + hemisphereFrame * sampleKernel[sampleIndex] * radius;
		// 표본을 화면에 투영해 그 자리의 실제 깊이와 비교
		vec2 sampleCoordinate = (samplePosition.xy / projectionScale) / -samplePosition.z * 0.5 + 0.5;
		if (sampleCoordinate.x < 0.0 || sampleCoordinate.x > 1.0 || sampleCoordinate.y < 0.0 || sampleCoordinate.y > 1.0) {
			continue;
		}
		float sceneDepth = linearizeDepth(texture(depthTexture, sampleCoordinate).r);
		float rangeCheck = smoothstep(0.0, 1.0, radius / max(abs(linearDepth - sceneDepth), 0.0001));
		occlusion += (sceneDepth <= -samplePosition.z - bias ? 1.0 : 0.0) * rangeCheck;
	}
	float visibility = 1.0 - occlusion / float(16) * intensity;
	outputColor = vec4(clamp(visibility, 0.0, 1.0), linearDepth, 0.0, 1.0);
}";

/// 분리형 깊이 인식 블러. (노이즈 제거, 깊이 차가 큰 표본은 제외)
const BLUR_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform vec2 blurDirection;
uniform vec2 texelSize;
out vec4 outputColor;

void main() {
	vec2 center = texture(sourceTexture, fragmentTextureCoordinate).rg;
	float weightSum = 1.0;
	float visibilitySum = center.r;
	for (int offset = 1; offset <= 4; ++offset) {
		for (int sign = -1; sign <= 1; sign += 2) {
			vec2 sampleCoordinate = fragmentTextureCoordinate + blurDirection * texelSize * float(offset * sign);
			vec2 sampleValue = texture(sourceTexture, sampleCoordinate).rg;
			float depthWeight = 1.0 - smoothstep(0.0, center.g * 0.05, abs(sampleValue.g - center.g));
			visibilitySum += sampleValue.r * depthWeight;
			weightSum += depthWeight;
		}
	}
	outputColor = vec4(visibilitySum / weightSum, center.g, 0.0, 1.0);
}";

//==============================================================================
/// 화면 공간 앰비언트 오클루전.
/// - 판 셋(차폐, 핑, 퐁)은 RGBA16F 다. 차폐 판의 r 은 가시율, g 는 선형 깊이다.
///
/// # 예제
/// 노멀을 1 번 어태치먼트에, 깊이를 깊이 텍스처에 그린 씬에서 가시율 만들기
/// ```rust,no_run
/// use vanilla::{AmbientOcclusionEffect, Camera3D, RenderTarget, RenderTargetOptions};
///
/// fn create_resources(gl: &glow::Context) -> Result<(RenderTarget, AmbientOcclusionEffect), String> {
/// 	let options = RenderTargetOptions { color_attachment_count: 2, use_float_color: true, use_depth_texture: true, ..RenderTargetOptions::default() };
/// 	let scene_render_target = RenderTarget::new(gl, 1280, 720, true, Some(options))?;
/// 	let mut ambient_occlusion_effect = AmbientOcclusionEffect::new(gl)?;
/// 	ambient_occlusion_effect.set_parameters(0.05, 1.2, 0.002);
/// 	ambient_occlusion_effect.set_scale(0.5);
/// 	ambient_occlusion_effect.resize(gl, 1280, 720)?;
/// 	Ok((scene_render_target, ambient_occlusion_effect))
/// }
///
/// fn render_occlusion(gl: &glow::Context, ambient_occlusion_effect: &AmbientOcclusionEffect, scene_render_target: &RenderTarget, camera: &Camera3D) {
/// 	let view_matrix = camera.get_view_matrix();
/// 	ambient_occlusion_effect.render(gl, scene_render_target.get_depth_texture(), scene_render_target.get_color_texture(1), view_matrix.get_elements(), camera.get_field_of_view(), camera.get_aspect_ratio(), camera.get_near_distance(), camera.get_far_distance());
/// 	let visibility_texture = ambient_occlusion_effect.get_result_texture();
/// 	assert!(visibility_texture.is_some());
/// }
/// ```
//==============================================================================
pub struct AmbientOcclusionEffect {
	occlusion_pass: FullscreenPass,
	blur_pass: FullscreenPass,
	occlusion_render_target: RenderTarget,
	ping_render_target: RenderTarget,
	pong_render_target: RenderTarget,
	sample_kernel: [f32; SAMPLE_COUNT * 3],
	radius: f64,
	intensity: f64,
	bias: f64,
	scale: f64,
}

impl AmbientOcclusionEffect {
	//==============================================================================
	/// 생성. (패스 2개 + 반정밀 실수 타겟 3장, resize 로 크기를 맞춘다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트. (Graphic::get_gl 로 꺼낸다)
	///
	/// # 돌려줌
	/// 새 이펙트. (반경 0.03, 강도 1, 바이어스 0.002, 배율 0.5, 커널까지 셈한 것) 셰이더 컴파일이나 판 만들기에 실패하면 Err(오류 글).
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<AmbientOcclusionEffect, String> {
		let float_color_options = RenderTargetOptions {
			use_float_color: true,
			..RenderTargetOptions::default()
		};
		let mut ambient_occlusion_effect = AmbientOcclusionEffect {
			occlusion_pass: FullscreenPass::new(gl, OCCLUSION_FRAGMENTSHADER_SOURCE)?,
			blur_pass: FullscreenPass::new(gl, BLUR_FRAGMENTSHADER_SOURCE)?,
			occlusion_render_target: RenderTarget::new(gl, 2, 2, false, Some(float_color_options))?,
			ping_render_target: RenderTarget::new(gl, 2, 2, false, Some(float_color_options))?,
			pong_render_target: RenderTarget::new(gl, 2, 2, false, Some(float_color_options))?,
			sample_kernel: [0.0; SAMPLE_COUNT * 3],
			radius: 0.03,
			intensity: 1.0,
			bias: 0.002,
			scale: 0.5,
		};
		ambient_occlusion_effect.compute_kernel();
		Ok(ambient_occlusion_effect)
	}

	//==============================================================================
	/// 표본 커널 계산. (반구 안에 고르게, 중심 쪽에 더 촘촘히)
	//==============================================================================
	pub fn compute_kernel(&mut self) {
		self.sample_kernel = AmbientOcclusionEffect::build_sample_kernel();
	}

	//==============================================================================
	/// 표본 커널 만들기. (GL 무관, 황금각 나선, 높이 고르게, 크기 0.15 ~ 1 제곱 분포)
	///
	/// # 돌려줌
	/// 표본 SAMPLE_COUNT(16) 개의 x, y, z 를 차례로 이은 배열. (z 가 양수인 반구 안, 표본 길이는 0.15 ~ 1)
	///
	/// # 예제
	/// 모든 표본이 반지름 1 의 위쪽 반구 안에 든다
	/// ```rust
	/// use vanilla::graphics3d::ambientocclusioneffect::SAMPLE_COUNT;
	/// use vanilla::AmbientOcclusionEffect;
	///
	/// let sample_kernel = AmbientOcclusionEffect::build_sample_kernel();
	/// assert_eq!(sample_kernel.len(), SAMPLE_COUNT * 3);
	/// for sample_index in 0..SAMPLE_COUNT {
	/// 	let x = sample_kernel[sample_index * 3];
	/// 	let y = sample_kernel[sample_index * 3 + 1];
	/// 	let z = sample_kernel[sample_index * 3 + 2];
	/// 	assert!(z > 0.0);
	/// 	assert!((x * x + y * y + z * z).sqrt() <= 1.0 + 1.0e-5);
	/// }
	/// ```
	//==============================================================================
	pub fn build_sample_kernel() -> [f32; SAMPLE_COUNT * 3] {
		let mut sample_kernel = [0.0f32; SAMPLE_COUNT * 3];
		for sample_index in 0..SAMPLE_COUNT {
			let golden_angle = 2.39996323f64;
			let angle = sample_index as f64 * golden_angle;
			let height = (sample_index as f64 + 0.5) / SAMPLE_COUNT as f64;
			let ring = (1.0 - height * height).sqrt();
			let mut scale = (sample_index as f64 + 1.0) / SAMPLE_COUNT as f64;
			scale = 0.15 + 0.85 * scale * scale;
			sample_kernel[sample_index * 3] = ((angle.cos() * ring * scale)) as f32;
			sample_kernel[sample_index * 3 + 1] = ((angle.sin() * ring * scale)) as f32;
			sample_kernel[sample_index * 3 + 2] = ((height * scale)) as f32;
		}
		sample_kernel
	}

	//==============================================================================
	/// 크기 변경. (본 패스 해상도, 내부는 축소 배율 적용)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `width`: 본 패스 가로 크기. (픽셀, 판은 배율을 곱해 내림, 최소 1)
	/// - `height`: 본 패스 세로 크기. (픽셀, 판은 배율을 곱해 내림, 최소 1)
	///
	/// # 돌려줌
	/// 판을 맞췄으면 Ok. 다시 짓다 GL 객체를 못 만들면 Err(GL 의 오류 글).
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
		let scaled_width = ((width as f64 * self.scale).floor() as i32).max(1);
		let scaled_height = ((height as f64 * self.scale).floor() as i32).max(1);
		self.occlusion_render_target.resize(gl, scaled_width, scaled_height)?;
		self.ping_render_target.resize(gl, scaled_width, scaled_height)?;
		self.pong_render_target.resize(gl, scaled_width, scaled_height)?;
		Ok(())
	}

	//==============================================================================
	/// 차폐 렌더링. (차폐 → 가로 블러 → 세로 블러, 결과는 get_result_texture())
	/// - view_matrix_elements 는 뷰 행렬 원소(열 우선 4x4), field_of_view_radian 은 세로 시야각.
	/// - 부수 효과: 깊이/블렌드 비활성, TEXTURE0 활성, 프레임버퍼 바인드가 변경된다.
	/// - 깊이는 0 번, 노멀은 2 번 유닛에 묶었다가 2 번은 끝에 풀어 둔다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `depth_texture`: 본 패스의 깊이 텍스처. (하드웨어 깊이 0 ~ 1, 1 이면 하늘로 보고 가림 없음)
	/// - `normal_texture`: 월드 노멀 텍스처. (rgb 에 -1 ~ 1 노멀을 그대로, 알파 0.5 밑은 빈 곳으로 보고 가림 없음)
	/// - `view_matrix_elements`: 뷰 행렬 원소. (열 우선 4x4, 회전 부분만 쓴다)
	/// - `field_of_view_radian`: 세로 시야각. (라디안)
	/// - `aspect_ratio`: 가로 세로 비. (너비 / 높이)
	/// - `near_distance`: 투영의 가까운 거리. (깊이를 선형으로 풀 때)
	/// - `far_distance`: 투영의 먼 거리.
	//==============================================================================
	pub fn render(&self, gl: &glow::Context, depth_texture: Option<GlTexture>, normal_texture: Option<GlTexture>, view_matrix_elements: &[f32; 16], field_of_view_radian: f64, aspect_ratio: f64, near_distance: f64, far_distance: f64) {
		unsafe {
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
		}

		// 1. 차폐.
		let occlusion_pass = &self.occlusion_pass;
		occlusion_pass.use_program(gl);
		let view_rotation = [view_matrix_elements[0], view_matrix_elements[1], view_matrix_elements[2], view_matrix_elements[4], view_matrix_elements[5], view_matrix_elements[6], view_matrix_elements[8], view_matrix_elements[9], view_matrix_elements[10]];
		let tangent_half_field_of_view = (field_of_view_radian * 0.5).tan();
		unsafe {
			gl.uniform_1_i32(occlusion_pass.get_uniform_location(gl, "depthTexture").as_ref(), 0);
			gl.uniform_1_i32(occlusion_pass.get_uniform_location(gl, "normalTexture").as_ref(), 2);
			gl.uniform_matrix_3_f32_slice(occlusion_pass.get_uniform_location(gl, "viewRotation").as_ref(), false, &view_rotation);
			gl.uniform_2_f64(occlusion_pass.get_uniform_location(gl, "depthRange").as_ref(), near_distance, far_distance);
			gl.uniform_2_f64(occlusion_pass.get_uniform_location(gl, "projectionScale").as_ref(), tangent_half_field_of_view * aspect_ratio, tangent_half_field_of_view);
			gl.uniform_3_f32_slice(occlusion_pass.get_uniform_location(gl, "sampleKernel[0]").as_ref(), &self.sample_kernel);
			gl.uniform_1_f64(occlusion_pass.get_uniform_location(gl, "radius").as_ref(), self.radius);
			gl.uniform_1_f64(occlusion_pass.get_uniform_location(gl, "intensity").as_ref(), self.intensity);
			gl.uniform_1_f64(occlusion_pass.get_uniform_location(gl, "bias").as_ref(), self.bias);
			gl.active_texture(glow::TEXTURE2);
			gl.bind_texture(glow::TEXTURE_2D, normal_texture);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, depth_texture);
		}
		let occlusion_render_target = &self.occlusion_render_target;
		occlusion_render_target.bind(gl);
		occlusion_pass.draw(gl);

		// 2. 블러. (가로 → 핑, 세로 → 퐁)
		let blur_pass = &self.blur_pass;
		blur_pass.use_program(gl);
		let blur_direction_location = blur_pass.get_uniform_location(gl, "blurDirection");
		let occlusion_width = occlusion_render_target.get_width() as f64;
		let occlusion_height = occlusion_render_target.get_height() as f64;
		unsafe {
			gl.uniform_1_i32(blur_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
			gl.uniform_2_f64(blur_pass.get_uniform_location(gl, "texelSize").as_ref(), 1.0 / occlusion_width, 1.0 / occlusion_height);
		}
		let ping_render_target = &self.ping_render_target;
		ping_render_target.bind(gl);
		unsafe {
			gl.uniform_2_f64(blur_direction_location.as_ref(), 1.0, 0.0);
			gl.bind_texture(glow::TEXTURE_2D, occlusion_render_target.get_color_texture(0));
		}
		blur_pass.draw(gl);
		let pong_render_target = &self.pong_render_target;
		pong_render_target.bind(gl);
		unsafe {
			gl.uniform_2_f64(blur_direction_location.as_ref(), 0.0, 1.0);
			gl.bind_texture(glow::TEXTURE_2D, ping_render_target.get_color_texture(0));
		}
		blur_pass.draw(gl);

		unsafe {
			gl.active_texture(glow::TEXTURE2);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
		}
	}

	//==============================================================================
	/// 결과 텍스처 반환. (render() 이후, r 가시율)
	///
	/// # 돌려줌
	/// 블러를 마친 가시율 텍스처. (퐁 판, r 이 1 이면 가림 없음, g 는 선형 깊이) destroy 뒤면 None.
	//==============================================================================
	pub fn get_result_texture(&self) -> Option<GlTexture> {
		self.pong_render_target.get_color_texture(0)
	}

	//==============================================================================
	/// 파라미터 설정. (표본 반경(월드) / 강도 / 깊이 바이어스)
	///
	/// # 인자
	/// - `radius`: 표본 반경. (월드 단위, 기본 0.03)
	/// - `intensity`: 차폐 강도. (가림 몫에 곱한다, 기본 1)
	/// - `bias`: 깊이 바이어스. (월드 단위, 자기 가림을 막는다, 기본 0.002)
	//==============================================================================
	pub fn set_parameters(&mut self, radius: f64, intensity: f64, bias: f64) {
		self.radius = radius;
		self.intensity = intensity;
		self.bias = bias;
	}

	//==============================================================================
	/// 내부 해상도 배율 설정. (resize 전에 호출)
	///
	/// # 인자
	/// - `scale`: 판의 해상도 배율. (본 패스 크기에 곱한다, 기본 0.5, 다음 resize 부터 쓴다)
	//==============================================================================
	pub fn set_scale(&mut self, scale: f64) {
		self.scale = scale;
	}

	//==============================================================================
	/// 파괴. (GL 리소스 해제)
	///
	/// # 인자
	/// - `gl`: 만들 때 쓴 GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.occlusion_render_target.destroy(gl);
		self.ping_render_target.destroy(gl);
		self.pong_render_target.destroy(gl);
		self.occlusion_pass.destroy(gl);
		self.blur_pass.destroy(gl);
	}

	//==============================================================================
	/// 차폐 패스 반환.
	///
	/// # 돌려줌
	/// 차폐 패스. (깊이와 노멀로 반구 표본 차폐를 셈하는 셰이더)
	//==============================================================================
	pub fn get_occlusion_pass(&self) -> &FullscreenPass {
		&self.occlusion_pass
	}

	//==============================================================================
	/// 블러 패스 반환.
	///
	/// # 돌려줌
	/// 깊이 인식 블러 패스. (가로, 세로 둘 다 이 패스)
	//==============================================================================
	pub fn get_blur_pass(&self) -> &FullscreenPass {
		&self.blur_pass
	}

	//==============================================================================
	/// 차폐 렌더 타겟 반환.
	///
	/// # 돌려줌
	/// 차폐 렌더 타겟. (블러 전 가시율)
	//==============================================================================
	pub fn get_occlusion_render_target(&self) -> &RenderTarget {
		&self.occlusion_render_target
	}

	//==============================================================================
	/// 핑 렌더 타겟 반환. (가로 블러 결과)
	///
	/// # 돌려줌
	/// 핑 렌더 타겟. (가로 블러 결과)
	//==============================================================================
	pub fn get_ping_render_target(&self) -> &RenderTarget {
		&self.ping_render_target
	}

	//==============================================================================
	/// 퐁 렌더 타겟 반환. (최종 결과 보관)
	///
	/// # 돌려줌
	/// 퐁 렌더 타겟. (세로 블러 결과, 결과 텍스처의 주인)
	//==============================================================================
	pub fn get_pong_render_target(&self) -> &RenderTarget {
		&self.pong_render_target
	}

	//==============================================================================
	/// 표본 반경 반환.
	///
	/// # 돌려줌
	/// 표본 반경. (월드 단위)
	//==============================================================================
	pub fn get_radius(&self) -> f64 {
		self.radius
	}

	//==============================================================================
	/// 강도 반환.
	///
	/// # 돌려줌
	/// 차폐 강도.
	//==============================================================================
	pub fn get_intensity(&self) -> f64 {
		self.intensity
	}

	//==============================================================================
	/// 깊이 바이어스 반환.
	///
	/// # 돌려줌
	/// 깊이 바이어스. (월드 단위)
	//==============================================================================
	pub fn get_bias(&self) -> f64 {
		self.bias
	}

	//==============================================================================
	/// 내부 해상도 배율 반환.
	///
	/// # 돌려줌
	/// 판의 해상도 배율.
	//==============================================================================
	pub fn get_scale(&self) -> f64 {
		self.scale
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 커널이 반구 안(z > 0, 길이 1 이하)에 있고 첫 표본이 원본 식과 같은지.
	//==============================================================================
	#[test]
	fn sample_kernel_stays_in_hemisphere() {
		let sample_kernel = AmbientOcclusionEffect::build_sample_kernel();
		for sample in sample_kernel.as_chunks::<3>().0 {
			assert!(sample[2] > 0.0);
			let length = (sample[0] * sample[0] + sample[1] * sample[1] + sample[2] * sample[2]).sqrt();
			assert!(length <= 1.0 + 1.0e-5);
		}
		// 0 번: 각 0, 높이 1/32, 크기 0.15 + 0.85 / 256.
		let first_scale = 0.15 + 0.85 / 256.0;
		let first_height = 0.5 / 16.0;
		assert!((sample_kernel[0] - (((1.0f64 - first_height * first_height).sqrt() * first_scale)) as f32).abs() < 1.0e-6);
		assert!(sample_kernel[1].abs() < 1.0e-6);
		assert!((sample_kernel[2] - ((first_height * first_scale)) as f32).abs() < 1.0e-6);
	}
}

//==============================================================================
// 화면 공간 앰비언트 오클루전. (원본 experimental/graphics/ambientocclusioneffect.js)
// - 깊이 + 노멀 버퍼 → 반구 표본 차폐 → 깊이 인식 블러. 결과는 getResultTexture() 의 r 채널 가시율(1 = 차폐 없음).
// - render() 입력은 본 패스의 깊이 텍스처와 월드 노멀 텍스처(알파 = 커버리지), 카메라의 뷰 회전 / 투영 파라미터.
// - GLSL 과 커널, 기본값(반경 0.03, 강도 1, 바이어스 0.002, 배율 0.5)은 원본과 같다. 원본 ${SAMPLE_COUNT} 는 숫자 16 으로 옮겼다.
// - GL 을 쓰는 함수는 gl 을 받고, 생성과 크기 변경은 GL 객체를 못 만들면 오류(Result)를 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::graphics3d::fullscreenpass::FullscreenPass;
use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 반구 표본 수. (프래그먼트마다 회전 노이즈로 흩는다, 셰이더의 16 과 같아야 한다)
pub const SAMPLE_COUNT: usize = 16;

// 차폐 패스. (깊이 + 월드 노멀 → 뷰 공간 반구 표본으로 차폐율, 결과 r 에 가시율)
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

// 분리형 깊이 인식 블러. (노이즈 제거, 깊이 차가 큰 표본은 제외)
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
// 화면 공간 앰비언트 오클루전.
//==============================================================================
pub struct AmbientOcclusionEffect {
	occlusionPass: FullscreenPass,
	blurPass: FullscreenPass,
	occlusionRenderTarget: RenderTarget,
	pingRenderTarget: RenderTarget,
	pongRenderTarget: RenderTarget,
	sampleKernel: [f32; SAMPLE_COUNT * 3],
	radius: f32,
	intensity: f32,
	bias: f32,
	scale: f32,
}

impl AmbientOcclusionEffect {
	//==============================================================================
	// 생성. (패스 2개 + 반정밀 실수 타겟 3장, resize 로 크기를 맞춘다)
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<AmbientOcclusionEffect, String> {
		let floatColorOptions = RenderTargetOptions {
			useFloatColor: true,
			..RenderTargetOptions::default()
		};
		let mut ambientOcclusionEffect = AmbientOcclusionEffect {
			occlusionPass: FullscreenPass::new(gl, OCCLUSION_FRAGMENTSHADER_SOURCE)?,
			blurPass: FullscreenPass::new(gl, BLUR_FRAGMENTSHADER_SOURCE)?,
			occlusionRenderTarget: RenderTarget::new(gl, 2, 2, false, Some(floatColorOptions))?,
			pingRenderTarget: RenderTarget::new(gl, 2, 2, false, Some(floatColorOptions))?,
			pongRenderTarget: RenderTarget::new(gl, 2, 2, false, Some(floatColorOptions))?,
			sampleKernel: [0.0; SAMPLE_COUNT * 3],
			radius: 0.03,
			intensity: 1.0,
			bias: 0.002,
			scale: 0.5,
		};
		ambientOcclusionEffect.computeKernel();
		Ok(ambientOcclusionEffect)
	}

	//==============================================================================
	// 표본 커널 계산. (반구 안에 고르게, 중심 쪽에 더 촘촘히)
	//==============================================================================
	pub fn computeKernel(&mut self) {
		self.sampleKernel = AmbientOcclusionEffect::buildSampleKernel();
	}

	//==============================================================================
	// 표본 커널 만들기. (GL 무관, 황금각 나선, 높이 고르게, 크기 0.15 ~ 1 제곱 분포)
	//==============================================================================
	pub fn buildSampleKernel() -> [f32; SAMPLE_COUNT * 3] {
		let mut sampleKernel = [0.0f32; SAMPLE_COUNT * 3];
		for sampleIndex in 0..SAMPLE_COUNT {
			let goldenAngle = 2.39996323f64;
			let angle = sampleIndex as f64 * goldenAngle;
			let height = (sampleIndex as f64 + 0.5) / SAMPLE_COUNT as f64;
			let ring = (1.0 - height * height).sqrt();
			let mut scale = (sampleIndex as f64 + 1.0) / SAMPLE_COUNT as f64;
			scale = 0.15 + 0.85 * scale * scale;
			sampleKernel[sampleIndex * 3] = (angle.cos() * ring * scale) as f32;
			sampleKernel[sampleIndex * 3 + 1] = (angle.sin() * ring * scale) as f32;
			sampleKernel[sampleIndex * 3 + 2] = (height * scale) as f32;
		}
		sampleKernel
	}

	//==============================================================================
	// 크기 변경. (본 패스 해상도, 내부는 축소 배율 적용)
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
		let scaledWidth = ((width as f32 * self.scale).floor() as i32).max(1);
		let scaledHeight = ((height as f32 * self.scale).floor() as i32).max(1);
		self.occlusionRenderTarget.resize(gl, scaledWidth, scaledHeight)?;
		self.pingRenderTarget.resize(gl, scaledWidth, scaledHeight)?;
		self.pongRenderTarget.resize(gl, scaledWidth, scaledHeight)?;
		Ok(())
	}

	//==============================================================================
	// 차폐 렌더링. (차폐 → 가로 블러 → 세로 블러, 결과는 getResultTexture())
	// - viewMatrixElements 는 뷰 행렬 원소(열 우선 4x4), fieldOfViewRadian 은 세로 시야각.
	// - 부수 효과: 깊이/블렌드 비활성, TEXTURE0 활성, 프레임버퍼 바인드가 변경된다.
	//==============================================================================
	pub fn render(&self, gl: &glow::Context, depthTexture: Option<GlTexture>, normalTexture: Option<GlTexture>, viewMatrixElements: &[f32; 16], fieldOfViewRadian: f32, aspectRatio: f32, nearDistance: f32, farDistance: f32) {
		unsafe {
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
		}

		// 1. 차폐.
		let occlusionPass = &self.occlusionPass;
		occlusionPass.useProgram(gl);
		let viewRotation = [viewMatrixElements[0], viewMatrixElements[1], viewMatrixElements[2], viewMatrixElements[4], viewMatrixElements[5], viewMatrixElements[6], viewMatrixElements[8], viewMatrixElements[9], viewMatrixElements[10]];
		let tangentHalfFieldOfView = (fieldOfViewRadian * 0.5).tan();
		unsafe {
			gl.uniform_1_i32(occlusionPass.getUniformLocation(gl, "depthTexture").as_ref(), 0);
			gl.uniform_1_i32(occlusionPass.getUniformLocation(gl, "normalTexture").as_ref(), 2);
			gl.uniform_matrix_3_f32_slice(occlusionPass.getUniformLocation(gl, "viewRotation").as_ref(), false, &viewRotation);
			gl.uniform_2_f32(occlusionPass.getUniformLocation(gl, "depthRange").as_ref(), nearDistance, farDistance);
			gl.uniform_2_f32(occlusionPass.getUniformLocation(gl, "projectionScale").as_ref(), tangentHalfFieldOfView * aspectRatio, tangentHalfFieldOfView);
			gl.uniform_3_f32_slice(occlusionPass.getUniformLocation(gl, "sampleKernel[0]").as_ref(), &self.sampleKernel);
			gl.uniform_1_f32(occlusionPass.getUniformLocation(gl, "radius").as_ref(), self.radius);
			gl.uniform_1_f32(occlusionPass.getUniformLocation(gl, "intensity").as_ref(), self.intensity);
			gl.uniform_1_f32(occlusionPass.getUniformLocation(gl, "bias").as_ref(), self.bias);
			gl.active_texture(glow::TEXTURE2);
			gl.bind_texture(glow::TEXTURE_2D, normalTexture);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, depthTexture);
		}
		let occlusionRenderTarget = &self.occlusionRenderTarget;
		occlusionRenderTarget.bind(gl);
		occlusionPass.draw(gl);

		// 2. 블러. (가로 → 핑, 세로 → 퐁)
		let blurPass = &self.blurPass;
		blurPass.useProgram(gl);
		let blurDirectionLocation = blurPass.getUniformLocation(gl, "blurDirection");
		let occlusionWidth = occlusionRenderTarget.getWidth() as f32;
		let occlusionHeight = occlusionRenderTarget.getHeight() as f32;
		unsafe {
			gl.uniform_1_i32(blurPass.getUniformLocation(gl, "sourceTexture").as_ref(), 0);
			gl.uniform_2_f32(blurPass.getUniformLocation(gl, "texelSize").as_ref(), 1.0 / occlusionWidth, 1.0 / occlusionHeight);
		}
		let pingRenderTarget = &self.pingRenderTarget;
		pingRenderTarget.bind(gl);
		unsafe {
			gl.uniform_2_f32(blurDirectionLocation.as_ref(), 1.0, 0.0);
			gl.bind_texture(glow::TEXTURE_2D, occlusionRenderTarget.getColorTexture(0));
		}
		blurPass.draw(gl);
		let pongRenderTarget = &self.pongRenderTarget;
		pongRenderTarget.bind(gl);
		unsafe {
			gl.uniform_2_f32(blurDirectionLocation.as_ref(), 0.0, 1.0);
			gl.bind_texture(glow::TEXTURE_2D, pingRenderTarget.getColorTexture(0));
		}
		blurPass.draw(gl);

		unsafe {
			gl.active_texture(glow::TEXTURE2);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
		}
	}

	//==============================================================================
	// 결과 텍스처 반환. (render() 이후, r 가시율)
	//==============================================================================
	pub fn getResultTexture(&self) -> Option<GlTexture> {
		self.pongRenderTarget.getColorTexture(0)
	}

	//==============================================================================
	// 파라미터 설정. (표본 반경(월드) / 강도 / 깊이 바이어스)
	//==============================================================================
	pub fn setParameters(&mut self, radius: f32, intensity: f32, bias: f32) {
		self.radius = radius;
		self.intensity = intensity;
		self.bias = bias;
	}

	//==============================================================================
	// 내부 해상도 배율 설정. (resize 전에 호출)
	//==============================================================================
	pub fn setScale(&mut self, scale: f32) {
		self.scale = scale;
	}

	//==============================================================================
	// 파괴. (GL 리소스 해제)
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.occlusionRenderTarget.destroy(gl);
		self.pingRenderTarget.destroy(gl);
		self.pongRenderTarget.destroy(gl);
	}

	//==============================================================================
	// 차폐 패스 반환.
	//==============================================================================
	pub fn getOcclusionPass(&self) -> &FullscreenPass {
		&self.occlusionPass
	}

	//==============================================================================
	// 블러 패스 반환.
	//==============================================================================
	pub fn getBlurPass(&self) -> &FullscreenPass {
		&self.blurPass
	}

	//==============================================================================
	// 차폐 렌더 타겟 반환.
	//==============================================================================
	pub fn getOcclusionRenderTarget(&self) -> &RenderTarget {
		&self.occlusionRenderTarget
	}

	//==============================================================================
	// 핑 렌더 타겟 반환. (가로 블러 결과)
	//==============================================================================
	pub fn getPingRenderTarget(&self) -> &RenderTarget {
		&self.pingRenderTarget
	}

	//==============================================================================
	// 퐁 렌더 타겟 반환. (최종 결과 보관)
	//==============================================================================
	pub fn getPongRenderTarget(&self) -> &RenderTarget {
		&self.pongRenderTarget
	}

	//==============================================================================
	// 표본 반경 반환.
	//==============================================================================
	pub fn getRadius(&self) -> f32 {
		self.radius
	}

	//==============================================================================
	// 강도 반환.
	//==============================================================================
	pub fn getIntensity(&self) -> f32 {
		self.intensity
	}

	//==============================================================================
	// 깊이 바이어스 반환.
	//==============================================================================
	pub fn getBias(&self) -> f32 {
		self.bias
	}

	//==============================================================================
	// 내부 해상도 배율 반환.
	//==============================================================================
	pub fn getScale(&self) -> f32 {
		self.scale
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 커널이 반구 안(z > 0, 길이 1 이하)에 있고 첫 표본이 원본 식과 같은지.
	//==============================================================================
	#[test]
	fn sampleKernelStaysInHemisphere() {
		let sampleKernel = AmbientOcclusionEffect::buildSampleKernel();
		for sample in sampleKernel.as_chunks::<3>().0 {
			assert!(sample[2] > 0.0);
			let length = (sample[0] * sample[0] + sample[1] * sample[1] + sample[2] * sample[2]).sqrt();
			assert!(length <= 1.0 + 1.0e-5);
		}
		// 0 번: 각 0, 높이 1/32, 크기 0.15 + 0.85 / 256.
		let firstScale = 0.15 + 0.85 / 256.0;
		let firstHeight = 0.5 / 16.0;
		assert!((sampleKernel[0] - ((1.0f64 - firstHeight * firstHeight).sqrt() * firstScale) as f32).abs() < 1.0e-6);
		assert!(sampleKernel[1].abs() < 1.0e-6);
		assert!((sampleKernel[2] - (firstHeight * firstScale) as f32).abs() < 1.0e-6);
	}
}

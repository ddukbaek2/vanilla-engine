//==============================================================================
// 블룸 이펙트. (원본 experimental/graphics/bloomeffect.js)
// - 밝은 영역 추출 → 하프 해상도 가우시안 블러 핑퐁. render() 결과 텍스처를 최종 합성 패스에서 가산하는 방식으로 쓴다.
// - 셰이더와 패스 순서, 기본값(임계 0.5 ~ 0.95, 반복 2)은 원본과 같다.
// - GL 을 쓰는 함수는 gl 을 받고, 생성과 크기 변경은 GL 객체를 못 만들면 오류(Result)를 돌려준다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::graphics3d::fullscreenpass::FullscreenPass;
use crate::graphics3d::rendertarget::RenderTarget;
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 밝은 영역 추출 패스. (씬 → 하프 해상도)
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

// 9탭 가우시안 블러 패스. (가로/세로 분리)
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
// 블룸 이펙트.
//==============================================================================
pub struct BloomEffect {
	brightPass: FullscreenPass,
	blurPass: FullscreenPass,
	pingRenderTarget: RenderTarget,
	pongRenderTarget: RenderTarget,
	thresholdStart: f32,
	thresholdEnd: f32,
	iterationCount: u32,
}

impl BloomEffect {
	//==============================================================================
	// 생성. (패스 2개 + 2x2 렌더 타겟 2장, resize 로 크기를 맞춘다)
	//==============================================================================
	pub fn new(gl: &glow::Context) -> Result<BloomEffect, String> {
		Ok(BloomEffect {
			brightPass: FullscreenPass::new(gl, BRIGHTPASS_FRAGMENTSHADER_SOURCE)?,
			blurPass: FullscreenPass::new(gl, BLUR_FRAGMENTSHADER_SOURCE)?,
			pingRenderTarget: RenderTarget::new(gl, 2, 2, false, None)?,
			pongRenderTarget: RenderTarget::new(gl, 2, 2, false, None)?,
			thresholdStart: 0.5,
			thresholdEnd: 0.95,
			iterationCount: 2,
		})
	}

	//==============================================================================
	// 크기 변경. (씬 해상도 기준, 내부는 하프 해상도)
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, sceneWidth: i32, sceneHeight: i32) -> Result<(), String> {
		let halfWidth = (sceneWidth >> 1).max(1);
		let halfHeight = (sceneHeight >> 1).max(1);
		self.pingRenderTarget.resize(gl, halfWidth, halfHeight)?;
		self.pongRenderTarget.resize(gl, halfWidth, halfHeight)?;
		Ok(())
	}

	//==============================================================================
	// 블룸 렌더링. (호출 후 결과는 getResultTexture())
	// - 부수 효과: 깊이/블렌드 비활성, TEXTURE0 활성, 프레임버퍼 바인드가 변경된다.
	//==============================================================================
	pub fn render(&self, gl: &glow::Context, sceneColorTexture: Option<GlTexture>) {
		unsafe {
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
			gl.active_texture(glow::TEXTURE0);
		}

		// 1. 밝은 영역 추출. (씬 → 핑)
		let pingRenderTarget = &self.pingRenderTarget;
		let pongRenderTarget = &self.pongRenderTarget;
		pingRenderTarget.bind(gl);
		let brightPass = &self.brightPass;
		brightPass.useProgram(gl);
		let brightSceneTextureLocation = brightPass.getUniformLocation(gl, "sceneTexture");
		let brightThresholdRangeLocation = brightPass.getUniformLocation(gl, "thresholdRange");
		unsafe {
			gl.uniform_1_i32(brightSceneTextureLocation.as_ref(), 0);
			gl.uniform_2_f32(brightThresholdRangeLocation.as_ref(), self.thresholdStart, self.thresholdEnd);
			gl.bind_texture(glow::TEXTURE_2D, sceneColorTexture);
		}
		brightPass.draw(gl);

		// 2. 가로/세로 가우시안 블러 핑퐁.
		let blurPass = &self.blurPass;
		blurPass.useProgram(gl);
		let blurSourceTextureLocation = blurPass.getUniformLocation(gl, "sourceTexture");
		let blurDirectionLocation = blurPass.getUniformLocation(gl, "blurDirection");
		unsafe {
			gl.uniform_1_i32(blurSourceTextureLocation.as_ref(), 0);
		}
		for _blurIndex in 0..self.iterationCount {
			// 가로. (핑 → 퐁)
			pongRenderTarget.bind(gl);
			let pingWidth = pingRenderTarget.getWidth() as f32;
			unsafe {
				gl.uniform_2_f32(blurDirectionLocation.as_ref(), 1.0 / pingWidth, 0.0);
				gl.bind_texture(glow::TEXTURE_2D, pingRenderTarget.getColorTexture(0));
			}
			blurPass.draw(gl);

			// 세로. (퐁 → 핑)
			pingRenderTarget.bind(gl);
			let pongHeight = pongRenderTarget.getHeight() as f32;
			unsafe {
				gl.uniform_2_f32(blurDirectionLocation.as_ref(), 0.0, 1.0 / pongHeight);
				gl.bind_texture(glow::TEXTURE_2D, pongRenderTarget.getColorTexture(0));
			}
			blurPass.draw(gl);
		}
	}

	//==============================================================================
	// 결과 텍스처 반환. (render() 이후)
	//==============================================================================
	pub fn getResultTexture(&self) -> Option<GlTexture> {
		self.pingRenderTarget.getColorTexture(0)
	}

	//==============================================================================
	// 추출 임계값 설정. (씬 밝기 특성에 맞게 조정)
	//==============================================================================
	pub fn setThreshold(&mut self, thresholdStart: f32, thresholdEnd: f32) {
		self.thresholdStart = thresholdStart;
		self.thresholdEnd = thresholdEnd;
	}

	//==============================================================================
	// 블러 반복 횟수 설정.
	//==============================================================================
	pub fn setIterationCount(&mut self, iterationCount: u32) {
		self.iterationCount = iterationCount;
	}

	//==============================================================================
	// 블러 반복 횟수 반환.
	//==============================================================================
	pub fn getIterationCount(&self) -> u32 {
		self.iterationCount
	}

	//==============================================================================
	// 파괴. (GL 리소스 해제)
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.pingRenderTarget.destroy(gl);
		self.pongRenderTarget.destroy(gl);
	}

	//==============================================================================
	// 밝은 영역 추출 패스 반환.
	//==============================================================================
	pub fn getBrightPass(&self) -> &FullscreenPass {
		&self.brightPass
	}

	//==============================================================================
	// 블러 패스 반환.
	//==============================================================================
	pub fn getBlurPass(&self) -> &FullscreenPass {
		&self.blurPass
	}

	//==============================================================================
	// 핑 렌더 타겟 반환. (최종 결과 보관)
	//==============================================================================
	pub fn getPingRenderTarget(&self) -> &RenderTarget {
		&self.pingRenderTarget
	}

	//==============================================================================
	// 퐁 렌더 타겟 반환.
	//==============================================================================
	pub fn getPongRenderTarget(&self) -> &RenderTarget {
		&self.pongRenderTarget
	}
}

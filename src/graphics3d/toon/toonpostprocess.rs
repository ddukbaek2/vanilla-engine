//==============================================================================
// 툰 HDR 마무리. (실수 렌더 타겟에 그린 선형 색을 블룸, 노출, 톤매핑, 감마로 화면에 낸다)
// - 엔진의 RenderTarget(반정밀 실수 색, 깊이), BloomEffect, FullscreenPass 를 엮는다.
// - 사용: ToonRenderer::set_hdr_output(true) 뒤
//   begin(gl, 바탕 선형 색) → ToonRenderer 로 그리기 → end(gl, 낼 프레임버퍼, 뷰포트)
//   end 는 블룸을 셈하고 톤매핑한 색을 낼 프레임버퍼(None 이면 화면)에 그린다. 2D 와 섞으려면 그 뒤 Graphic::restore_render_state.
// - 톤매핑은 ACES 근사(Narkowicz)다. 노출(set_exposure)을 곱한 뒤 매핑하고 sRGB 감마를 건다.
// - 색 보정: 감마를 건 뒤 채도, 색조(밝기 1 로 맞춘 색을 곱함), 안개빛(어두운 곳을 그 색 쪽으로), 대비, 가장자리 어둡게 차례로 건다. (set_color_grade, set_vignette)
//   기본은 채도 1, 대비 1, 색조 0, 안개빛 0, 가장자리 0 이라 보정이 없다.
// - 노드 사각형 안에 낼 때: resize(사각형 너비, 높이) → begin → 그리기(ToonRenderer 의 화면 사각형은 풀어 둔다) → end(None, [x, 드로잉 버퍼 높이 - y - 높이, 너비, 높이]).
//==============================================================================
use glow::HasContext;

use crate::core::fullscreenpass::FullscreenPass;
use crate::graphics3d::bloomeffect::BloomEffect;
use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
use crate::graphics3d::GlFramebuffer;

const COMPOSITE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
uniform sampler2D bloomTexture;
uniform float bloomStrength;
uniform float exposure;
uniform float saturation;
uniform float contrast;
uniform vec3 toneColor;
uniform float toneAmount;
uniform vec3 hazeColor;
uniform float hazeAmount;
uniform float vignetteStrength;
out vec4 fragmentColor;
vec3 mapTone(vec3 color) {
	color *= exposure;
	return clamp((color * (2.51 * color + 0.03)) / (color * (2.43 * color + 0.59) + 0.14), 0.0, 1.0);
}
void main() {
	vec3 scene = texture(sceneTexture, fragmentTextureCoordinate).rgb;
	vec3 bloom = texture(bloomTexture, fragmentTextureCoordinate).rgb;
	vec3 color = pow(mapTone(scene + bloom * bloomStrength), vec3(1.0 / 2.2));
	float luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));
	color = mix(vec3(luminance), color, saturation);
	color = mix(color, color * toneColor, toneAmount);
	color = mix(color, hazeColor, hazeAmount * (1.0 - luminance));
	color = (color - 0.5) * contrast + 0.5;
	vec2 centered = fragmentTextureCoordinate * 2.0 - 1.0;
	color *= 1.0 - vignetteStrength * smoothstep(0.45, 1.42, length(centered));
	fragmentColor = vec4(clamp(color, 0.0, 1.0), 1.0);
}
";

pub struct ToonPostProcess {
	scene_render_target: RenderTarget,
	bloom_effect: BloomEffect,
	composite_pass: FullscreenPass,
	exposure: f32,
	bloom_strength: f32,
	is_bloom_enabled: bool,
	saturation: f32,
	contrast: f32,
	tone_color: [f32; 3],
	tone_amount: f32,
	haze_color: [f32; 3],
	haze_amount: f32,
	vignette_strength: f32,
}

// 밝기의 무게. (Rec. 709)
const LUMINANCE_WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];

impl ToonPostProcess {
	//==============================================================================
	// 생성. (화면 픽셀 크기)
	//==============================================================================
	pub fn new(gl: &glow::Context, width: i32, height: i32) -> Result<ToonPostProcess, String> {
		let options = RenderTargetOptions { color_attachment_count: 1, use_float_color: true, use_depth_texture: false };
		let scene_render_target = RenderTarget::new(gl, width.max(1), height.max(1), true, Some(options))?;
		let mut bloom_effect = BloomEffect::new(gl)?;
		bloom_effect.resize(gl, width.max(1), height.max(1))?;
		bloom_effect.set_threshold(0.9, 1.6);
		let composite_pass = FullscreenPass::new(gl, COMPOSITE_FRAGMENT_SHADER)?;
		Ok(ToonPostProcess {
			scene_render_target,
			bloom_effect,
			composite_pass,
			exposure: 1.0,
			bloom_strength: 0.6,
			is_bloom_enabled: true,
			saturation: 1.0,
			contrast: 1.0,
			tone_color: [1.0; 3],
			tone_amount: 0.0,
			haze_color: [0.0; 3],
			haze_amount: 0.0,
			vignette_strength: 0.0,
		})
	}

	//==============================================================================
	// 크기 바꾸기. (같으면 그대로)
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
		if self.scene_render_target.get_width() == width.max(1) && self.scene_render_target.get_height() == height.max(1) {
			return Ok(());
		}
		self.scene_render_target.resize(gl, width.max(1), height.max(1))?;
		self.bloom_effect.resize(gl, width.max(1), height.max(1))
	}

	//==============================================================================
	// 씬 그리기 시작. (실수 렌더 타겟을 묶고 바탕 선형 색과 깊이로 비운다)
	//==============================================================================
	pub fn begin(&self, gl: &glow::Context, clear_color: [f32; 3]) {
		self.scene_render_target.bind(gl);
		unsafe {
			gl.viewport(0, 0, self.scene_render_target.get_width(), self.scene_render_target.get_height());
			gl.depth_mask(true);
			gl.clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		}
	}

	//==============================================================================
	// 마무리. (블룸 → 톤매핑해 낼 프레임버퍼에, viewport 는 [x, y, 너비, 높이])
	//==============================================================================
	pub fn end(&self, gl: &glow::Context, output_framebuffer: Option<GlFramebuffer>, viewport: [i32; 4]) {
		let scene_texture = self.scene_render_target.get_color_texture(0);
		if self.is_bloom_enabled {
			self.bloom_effect.render(gl, scene_texture);
		}
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, output_framebuffer);
			gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
		}
		let composite_pass = &self.composite_pass;
		composite_pass.use_program(gl);
		unsafe {
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, scene_texture);
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, if self.is_bloom_enabled { self.bloom_effect.get_result_texture() } else { scene_texture });
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "bloomTexture").as_ref(), 1);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "bloomStrength").as_ref(), if self.is_bloom_enabled { self.bloom_strength } else { 0.0 });
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "exposure").as_ref(), self.exposure);
			let tone_color = self.tone_color;
			let haze_color = self.haze_color;
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "saturation").as_ref(), self.saturation);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "contrast").as_ref(), self.contrast);
			gl.uniform_3_f32(composite_pass.get_uniform_location(gl, "toneColor").as_ref(), tone_color[0], tone_color[1], tone_color[2]);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "toneAmount").as_ref(), self.tone_amount);
			gl.uniform_3_f32(composite_pass.get_uniform_location(gl, "hazeColor").as_ref(), haze_color[0], haze_color[1], haze_color[2]);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "hazeAmount").as_ref(), self.haze_amount);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "vignetteStrength").as_ref(), self.vignette_strength);
			gl.active_texture(glow::TEXTURE0);
		}
		composite_pass.draw(gl);
	}

	//==============================================================================
	// 노출 설정. (톤매핑 전에 곱한다, 1 이 기본)
	//==============================================================================
	pub fn set_exposure(&mut self, exposure: f32) {
		self.exposure = exposure.max(0.0);
	}

	//==============================================================================
	// 블룸 설정. (켜기, 세기, 밝은 곳을 뽑는 문턱의 시작과 끝)
	//==============================================================================
	pub fn set_bloom(&mut self, is_bloom_enabled: bool, bloom_strength: f32, threshold_start: f32, threshold_end: f32) {
		self.is_bloom_enabled = is_bloom_enabled;
		self.bloom_strength = bloom_strength.max(0.0);
		self.bloom_effect.set_threshold(threshold_start, threshold_end);
	}

	//==============================================================================
	// 색 보정 설정. (채도와 대비는 1 이 그대로, 색조와 안개빛은 sRGB [r, g, b] 0 ~ 1 과 섞는 양 0 ~ 1)
	// - 색조 색은 밝기(Rec. 709)를 1 로 맞춰 곱하므로 밝기는 그대로 두고 빛깔만 물든다.
	// - 안개빛은 어두운 곳일수록 그 색 쪽으로 끌어올린다. (양 x (1 - 밝기))
	//==============================================================================
	pub fn set_color_grade(&mut self, saturation: f32, contrast: f32, tone_color: [f32; 3], tone_amount: f32, haze_color: [f32; 3], haze_amount: f32) {
		let tone_luminance = tone_color[0] * LUMINANCE_WEIGHTS[0] + tone_color[1] * LUMINANCE_WEIGHTS[1] + tone_color[2] * LUMINANCE_WEIGHTS[2];
		let tone_scale = if tone_luminance > 0.0 { 1.0 / tone_luminance } else { 1.0 };
		self.saturation = saturation.max(0.0);
		self.contrast = contrast.max(0.0);
		self.tone_color = [tone_color[0] * tone_scale, tone_color[1] * tone_scale, tone_color[2] * tone_scale];
		self.tone_amount = tone_amount.clamp(0.0, 1.0);
		self.haze_color = haze_color;
		self.haze_amount = haze_amount.clamp(0.0, 1.0);
	}

	//==============================================================================
	// 가장자리 어둡게 설정. (0 이 없음, 1 이면 구석이 까맣게)
	//==============================================================================
	pub fn set_vignette(&mut self, vignette_strength: f32) {
		self.vignette_strength = vignette_strength.max(0.0);
	}

	//==============================================================================
	// 씬 렌더 타겟. (다른 후처리를 끼울 때)
	//==============================================================================
	pub fn get_scene_render_target(&self) -> &RenderTarget {
		&self.scene_render_target
	}

	//==============================================================================
	// 지우기.
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.scene_render_target.destroy(gl);
		self.bloom_effect.destroy(gl);
		self.composite_pass.destroy(gl);
	}
}

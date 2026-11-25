//==============================================================================
// 툰 HDR 마무리. (실수 렌더 타겟에 그린 선형 색을 블룸, 노출, 톤매핑, 감마로 화면에 낸다)
// - 엔진의 RenderTarget(반정밀 실수 색, 깊이), BloomEffect(또는 MultiLevelBloomEffect), FullscreenPass 를 엮는다.
// - 사용: ToonRenderer::set_hdr_output(true) 뒤
//   begin(gl, 바탕 선형 색) → ToonRenderer 로 그리기 → end(gl, 낼 프레임버퍼, 뷰포트)
//   end 는 (멀티샘플이면 풀고) 블룸을 셈하고 톤매핑한 색을 낼 프레임버퍼(None 이면 화면)에 그린다. 2D 와 섞으려면 그 뒤 Graphic::restore_render_state.
// - 생성 옵션: new_with_options(gl, 너비, 높이, ToonPostProcessOptions { sample_count, use_float_color, depth_bits, use_depth_texture }) (원본 생성자의 options, new 는 기본 옵션으로 지금과 같은 RGBA16F, 깊이 16, 멀티샘플 없음)
//   sample_count 가 2 이상이면 씬을 멀티샘플(MSAA) 렌더버퍼에 그리고 end 앞에서 텍스처로 푼다. 기기가 받는 수로 줄인다. (get_sample_count, set_sample_count)
//   depth_bits 24 는 깊이 24 비트. (먼 면이 먼 장면) use_depth_texture 는 씬 깊이를 텍스처로 남긴다. (set_lit_area 가 쓴다)
// - 색 공간: 씬 렌더 타겟에는 선형 색을 쓴다. (ToonRenderer::set_hdr_output(true), ToonScreenOutline::set_hdr_output(true), ToonVfx 색도 선형으로 본다)
//   감마를 건 색 g 를 실수 판에 쓰고 합성에서 g^2.2 로 되돌리던 게임은 그 값을 미리 선형으로 바꿔 넘긴다. 색은 c^2.2, 감마 쪽에서 곱하던 밝기 배 k 는 선형 k^2.2 다.
//   (예: 1.37 은 선형 2, 1.6 은 2.81, 2.08 은 5, 밝기 1.6 에 배 1.6 이면 (1.6 x 1.6)^2.2 = 7.9) 블룸 문턱과 무릎은 선형 밝기다.
// - 실수 판을 못 쓰는 기기: use_float_color(기본 true)여도 is_float_color_supported 가 false 면 RGBA8 판으로 짓는다. (get_use_float_color 가 false)
//   그때와 use_float_color: false 는 LDR 길이다. 씬에는 화면 색(감마를 건 색, ToonRenderer::set_hdr_output(false))을 그리고, 합성은 톤매핑과 감마 없이 블룸을 더해 색 보정만 건다.
// - 블룸: 기본은 반 해상도 9 탭 가우시안(BloomEffect, 문턱 0.9 ~ 1.6, 주고받는 판 RGBA8, set_bloom_float_color 로 RGBA16F).
//   set_bloom_levels(gl, 단 수, 문턱, 무릎) 로 단 수를 1 이상 주면 MultiLevelBloomEffect(반씩 줄였다 천막 꼴로 키우며 더함, 씬과 같은 정밀도의 판)로 바꾼다. 0 이면 기본으로 돌린다. 세기는 set_bloom 이 둘 다에 쓴다.
// - 톤매핑은 ACES 근사(Narkowicz)다. 노출(set_exposure)을 곱한 뒤 매핑하고 sRGB 감마를 건다.
// - 색 보정: 감마를 건 뒤 밝기(set_brightness, 밝힌 자리 set_lit_area 안은 1 쪽으로), 채도, 색조(밝기 1 로 맞춘 색을 곱함), 안개빛(어두운 곳을 그 색 쪽으로), 대비, 가장자리 어둡게 차례로 건다. (set_color_grade, set_vignette)
//   기본은 밝기 1, 채도 1, 대비 1, 색조 0, 안개빛 0, 가장자리 0 이라 보정이 없다.
// - 해상도 배율: set_resolution_scale(gl, 배율) 이면 씬 판을 출력 크기 x 배율로 짓고 end 의 뷰포트로 키워(선형 거르기) 낸다. (3D 만 낮춰 그리고 UI 는 기기 화소 그대로 둘 때, 기본 1)
// - 노드 사각형 안에 낼 때: resize(사각형 너비, 높이) → begin → 그리기(ToonRenderer 의 화면 사각형은 풀어 둔다) → end(None, [x, 드로잉 버퍼 높이 - y - 높이, 너비, 높이]).
// - 원본과 다른 점: GL 객체를 다시 짓는 설정(set_sample_count, set_resolution_scale, set_bloom_levels, set_bloom_float_color)은 gl 을 받고 Result 를 돌려준다.
//==============================================================================
use glow::HasContext;

use crate::base::matrix4::Matrix4;
use crate::core::fullscreenpass::FullscreenPass;
use crate::graphics3d::bloomeffect::BloomEffect;
use crate::graphics3d::multilevelbloomeffect::MultiLevelBloomEffect;
use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
use crate::graphics3d::{read_framebuffer_binding, read_texture_binding, GlFramebuffer};

const COMPOSITE_FRAGMENT_SHADER: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
uniform sampler2D bloomTexture;
uniform sampler2D depthTexture;
uniform float bloomStrength;
uniform float exposure;
uniform float hdrInput;
uniform float brightness;
uniform vec4 litCenter;
uniform float litInnerRadius;
uniform mat4 inverseViewProjectionMatrix;
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
	vec3 color = hdrInput > 0.5 ? pow(mapTone(scene + bloom * bloomStrength), vec3(1.0 / 2.2)) : clamp(scene + bloom * bloomStrength, 0.0, 1.0);
	float localBrightness = brightness;
	if (litCenter.w > 0.0) {
		float depth = texture(depthTexture, fragmentTextureCoordinate).r;
		vec4 worldPoint = inverseViewProjectionMatrix * vec4(fragmentTextureCoordinate * 2.0 - 1.0, depth * 2.0 - 1.0, 1.0);
		float litDistance = length(worldPoint.xz / worldPoint.w - litCenter.xz);
		localBrightness = mix(brightness, 1.0, 1.0 - smoothstep(litInnerRadius, litCenter.w, litDistance));
	}
	color *= localBrightness;
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

//==============================================================================
// 생성 옵션. (원본 생성자의 options, 기본값은 멀티샘플 없음 / 실수 판 / 깊이 16 / 깊이 텍스처 없음)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToonPostProcessOptions {
	// 멀티샘플 표본 수. (2 밑이면 없음)
	pub sample_count: i32,
	// 실수 판을 쓸지. (기기가 못 쓰면 false 로 짓는다)
	pub use_float_color: bool,
	// 깊이 렌더버퍼 비트 수. (16 이나 24)
	pub depth_bits: i32,
	// 씬 깊이를 텍스처로 남길지. (set_lit_area)
	pub use_depth_texture: bool,
}

impl Default for ToonPostProcessOptions {
	fn default() -> ToonPostProcessOptions {
		ToonPostProcessOptions { sample_count: 0, use_float_color: true, depth_bits: 16, use_depth_texture: false }
	}
}

//==============================================================================
// 밝힌 자리. (가운데, 안쪽과 바깥 반지름, 보기와 투영의 역행렬)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct LitArea {
	center: [f32; 3],
	inner_radius: f32,
	outer_radius: f32,
	inverse_view_projection_matrix: Matrix4,
}

pub struct ToonPostProcess {
	scene_render_target: RenderTarget,
	bloom_effect: BloomEffect,
	// 단 수를 주면 쓰는 다단 블룸.
	multi_level_bloom_effect: Option<MultiLevelBloomEffect>,
	composite_pass: FullscreenPass,
	// 씬 판이 실수인지. (HDR 길)
	use_float_color: bool,
	requested_sample_count: i32,
	depth_bits: i32,
	use_depth_texture: bool,
	// resize 로 받은 출력 크기.
	output_width: i32,
	output_height: i32,
	resolution_scale: f32,
	exposure: f32,
	bloom_strength: f32,
	is_bloom_enabled: bool,
	// 다단 블룸의 문턱과 무릎.
	bloom_threshold: f32,
	bloom_knee: f32,
	brightness: f32,
	lit_area: Option<LitArea>,
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
	// 생성. (화면 픽셀 크기, 기본 옵션)
	//==============================================================================
	pub fn new(gl: &glow::Context, width: i32, height: i32) -> Result<ToonPostProcess, String> {
		ToonPostProcess::new_with_options(gl, width, height, ToonPostProcessOptions::default())
	}

	//==============================================================================
	// 옵션을 주어 생성. (원본 new ToonPostProcess(gl, 너비, 높이, options))
	//==============================================================================
	pub fn new_with_options(gl: &glow::Context, width: i32, height: i32, options: ToonPostProcessOptions) -> Result<ToonPostProcess, String> {
		let use_float_color = options.use_float_color && ToonPostProcess::is_float_color_supported(gl);
		let depth_bits = if options.depth_bits == 24 {
			24
		}
		else {
			16
		};
		let scene_options = RenderTargetOptions { color_attachment_count: 1, use_float_color, use_depth_texture: options.use_depth_texture, sample_count: options.sample_count.max(0), depth_bits };
		let scene_render_target = RenderTarget::new(gl, width.max(1), height.max(1), true, Some(scene_options))?;
		let mut bloom_effect = BloomEffect::new(gl)?;
		bloom_effect.resize(gl, width.max(1), height.max(1))?;
		bloom_effect.set_threshold(0.9, 1.6);
		let composite_pass = FullscreenPass::new(gl, COMPOSITE_FRAGMENT_SHADER)?;
		Ok(ToonPostProcess {
			scene_render_target,
			bloom_effect,
			multi_level_bloom_effect: None,
			composite_pass,
			use_float_color,
			requested_sample_count: options.sample_count.max(0),
			depth_bits,
			use_depth_texture: options.use_depth_texture,
			output_width: width.max(1),
			output_height: height.max(1),
			resolution_scale: 1.0,
			exposure: 1.0,
			bloom_strength: 0.6,
			is_bloom_enabled: true,
			bloom_threshold: 1.0,
			bloom_knee: 0.25,
			brightness: 1.0,
			lit_area: None,
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
	// 실수 컬러 판을 그릴 수 있는지. (웹은 EXT_color_buffer_float 나 EXT_color_buffer_half_float 가 있어야, 1 x 1 RGBA16F 를 붙인 프레임버퍼가 완성되는지)
	//==============================================================================
	pub fn is_float_color_supported(gl: &glow::Context) -> bool {
		#[cfg(target_arch = "wasm32")]
		{
			let supported_extensions = gl.supported_extensions();
			if !supported_extensions.contains("EXT_color_buffer_float") && !supported_extensions.contains("EXT_color_buffer_half_float") {
				return false;
			}
		}
		let previous_framebuffer = read_framebuffer_binding(gl);
		let previous_texture = read_texture_binding(gl);
		unsafe {
			let Ok(test_texture) = gl.create_texture() else {
				return false;
			};
			let Ok(test_framebuffer) = gl.create_framebuffer() else {
				gl.delete_texture(test_texture);
				return false;
			};
			gl.bind_texture(glow::TEXTURE_2D, Some(test_texture));
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA16F as i32, 1, 1, 0, glow::RGBA, glow::HALF_FLOAT, glow::PixelUnpackData::Slice(None));
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(test_framebuffer));
			gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(test_texture), 0);
			let is_complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
			gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
			gl.bind_texture(glow::TEXTURE_2D, previous_texture);
			gl.delete_framebuffer(test_framebuffer);
			gl.delete_texture(test_texture);
			is_complete
		}
	}

	//==============================================================================
	// 씬 렌더 타겟 짓기. (출력 크기 x 해상도 배율, 지금 옵션으로)
	//==============================================================================
	fn create_scene_render_target(&self, gl: &glow::Context) -> Result<RenderTarget, String> {
		let options = RenderTargetOptions { color_attachment_count: 1, use_float_color: self.use_float_color, use_depth_texture: self.use_depth_texture, sample_count: self.requested_sample_count, depth_bits: self.depth_bits };
		RenderTarget::new(gl, self.read_scene_width(), self.read_scene_height(), true, Some(options))
	}

	//==============================================================================
	// 씬 판의 너비. (출력 너비 x 해상도 배율, 1 이상)
	//==============================================================================
	fn read_scene_width(&self) -> i32 {
		((self.output_width as f32 * self.resolution_scale).round() as i32).max(1)
	}

	//==============================================================================
	// 씬 판의 높이. (출력 높이 x 해상도 배율, 1 이상)
	//==============================================================================
	fn read_scene_height(&self) -> i32 {
		((self.output_height as f32 * self.resolution_scale).round() as i32).max(1)
	}

	//==============================================================================
	// 크기 바꾸기. (출력 크기, 씬 판은 해상도 배율을 곱함, 같으면 그대로)
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
		self.output_width = width.max(1);
		self.output_height = height.max(1);
		let scene_width = self.read_scene_width();
		let scene_height = self.read_scene_height();
		if self.scene_render_target.get_width() == scene_width && self.scene_render_target.get_height() == scene_height {
			return Ok(());
		}
		self.scene_render_target.resize(gl, scene_width, scene_height)?;
		self.bloom_effect.resize(gl, scene_width, scene_height)?;
		if let Some(multi_level_bloom_effect) = self.multi_level_bloom_effect.as_mut() {
			multi_level_bloom_effect.resize(gl, scene_width, scene_height)?;
		}
		Ok(())
	}

	//==============================================================================
	// 해상도 배율 설정. (씬 판 = 출력 크기 x 배율, 0.05 ~ 4, 기본 1)
	//==============================================================================
	pub fn set_resolution_scale(&mut self, gl: &glow::Context, resolution_scale: f32) -> Result<(), String> {
		self.resolution_scale = resolution_scale.clamp(0.05, 4.0);
		self.resize(gl, self.output_width, self.output_height)
	}

	//==============================================================================
	// 해상도 배율 반환.
	//==============================================================================
	pub fn get_resolution_scale(&self) -> f32 {
		self.resolution_scale
	}

	//==============================================================================
	// 멀티샘플 표본 수 설정. (2 밑이면 없음, 바뀌면 씬 판을 다시 짓는다)
	//==============================================================================
	pub fn set_sample_count(&mut self, gl: &glow::Context, sample_count: i32) -> Result<(), String> {
		let safe_sample_count = sample_count.max(0);
		if safe_sample_count == self.requested_sample_count {
			return Ok(());
		}
		self.requested_sample_count = safe_sample_count;
		let scene_render_target = self.create_scene_render_target(gl)?;
		self.scene_render_target.destroy(gl);
		self.scene_render_target = scene_render_target;
		Ok(())
	}

	//==============================================================================
	// 실제로 쓰는 멀티샘플 표본 수 반환. (0 이면 없음)
	//==============================================================================
	pub fn get_sample_count(&self) -> i32 {
		self.scene_render_target.get_sample_count()
	}

	//==============================================================================
	// 씬 판이 실수인지 반환. (false 면 LDR 길)
	//==============================================================================
	pub fn get_use_float_color(&self) -> bool {
		self.use_float_color
	}

	//==============================================================================
	// 씬 그리기 시작. (씬 렌더 타겟을 묶고 바탕 색과 깊이로 비운다, HDR 길은 선형 색, LDR 길은 화면 색)
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
	// 마무리. (멀티샘플이면 풂 → 블룸 → 톤매핑해 낼 프레임버퍼에, viewport 는 [x, y, 너비, 높이])
	//==============================================================================
	pub fn end(&self, gl: &glow::Context, output_framebuffer: Option<GlFramebuffer>, viewport: [i32; 4]) {
		self.scene_render_target.resolve(gl);
		let scene_texture = self.scene_render_target.get_color_texture(0);
		let mut bloom_texture = scene_texture;
		if self.is_bloom_enabled {
			if let Some(multi_level_bloom_effect) = self.multi_level_bloom_effect.as_ref() {
				multi_level_bloom_effect.render(gl, scene_texture);
				bloom_texture = multi_level_bloom_effect.get_result_texture();
			}
			else {
				self.bloom_effect.render(gl, scene_texture);
				bloom_texture = self.bloom_effect.get_result_texture();
			}
		}
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, output_framebuffer);
			gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
		}
		let composite_pass = &self.composite_pass;
		composite_pass.use_program(gl);
		let depth_texture = self.scene_render_target.get_depth_texture();
		let applied_lit_area = if depth_texture.is_some() {
			self.lit_area
		}
		else {
			None
		};
		unsafe {
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, scene_texture);
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, bloom_texture);
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "bloomTexture").as_ref(), 1);
			gl.active_texture(glow::TEXTURE2);
			gl.bind_texture(glow::TEXTURE_2D, if applied_lit_area.is_some() { depth_texture } else { None });
			gl.uniform_1_i32(composite_pass.get_uniform_location(gl, "depthTexture").as_ref(), 2);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "bloomStrength").as_ref(), if self.is_bloom_enabled { self.bloom_strength } else { 0.0 });
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "exposure").as_ref(), self.exposure);
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "hdrInput").as_ref(), if self.use_float_color { 1.0 } else { 0.0 });
			gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "brightness").as_ref(), self.brightness);
			if let Some(lit_area) = applied_lit_area {
				let center = lit_area.center;
				gl.uniform_4_f32(composite_pass.get_uniform_location(gl, "litCenter").as_ref(), center[0], center[1], center[2], lit_area.outer_radius);
				gl.uniform_1_f32(composite_pass.get_uniform_location(gl, "litInnerRadius").as_ref(), lit_area.inner_radius);
				gl.uniform_matrix_4_f32_slice(composite_pass.get_uniform_location(gl, "inverseViewProjectionMatrix").as_ref(), false, &lit_area.inverse_view_projection_matrix.elements);
			}
			else {
				gl.uniform_4_f32(composite_pass.get_uniform_location(gl, "litCenter").as_ref(), 0.0, 0.0, 0.0, 0.0);
			}
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
		unsafe {
			gl.active_texture(glow::TEXTURE2);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
		}
	}

	//==============================================================================
	// 노출 설정. (톤매핑 전에 곱한다, 1 이 기본)
	//==============================================================================
	pub fn set_exposure(&mut self, exposure: f32) {
		self.exposure = exposure.max(0.0);
	}

	//==============================================================================
	// 블룸 설정. (켜기, 세기, 밝은 곳을 뽑는 문턱의 시작과 끝, 문턱은 기본 9 탭 블룸에만 쓴다)
	//==============================================================================
	pub fn set_bloom(&mut self, is_bloom_enabled: bool, bloom_strength: f32, threshold_start: f32, threshold_end: f32) {
		self.is_bloom_enabled = is_bloom_enabled;
		self.bloom_strength = bloom_strength.max(0.0);
		self.bloom_effect.set_threshold(threshold_start, threshold_end);
	}

	//==============================================================================
	// 블룸 단 수 설정. (0 이면 기본 9 탭 블룸, 1 이상이면 그 단 수의 다단 블룸, 문턱과 무릎은 다단 블룸의 선형 밝기)
	// - 다단 블룸 판은 씬 판과 같은 정밀도다. (HDR 길은 RGBA16F)
	//==============================================================================
	pub fn set_bloom_levels(&mut self, gl: &glow::Context, level_count: usize, threshold: f32, knee: f32) -> Result<(), String> {
		self.bloom_threshold = threshold.max(0.0);
		self.bloom_knee = knee.max(0.0);
		if level_count == 0 {
			if let Some(mut multi_level_bloom_effect) = self.multi_level_bloom_effect.take() {
				multi_level_bloom_effect.destroy(gl);
			}
			return Ok(());
		}
		if self.multi_level_bloom_effect.is_none() {
			let mut multi_level_bloom_effect = MultiLevelBloomEffect::new(gl, level_count, self.use_float_color)?;
			multi_level_bloom_effect.resize(gl, self.read_scene_width(), self.read_scene_height())?;
			self.multi_level_bloom_effect = Some(multi_level_bloom_effect);
		}
		if let Some(multi_level_bloom_effect) = self.multi_level_bloom_effect.as_mut() {
			multi_level_bloom_effect.set_level_count(gl, level_count)?;
			multi_level_bloom_effect.set_threshold(self.bloom_threshold, self.bloom_knee);
		}
		Ok(())
	}

	//==============================================================================
	// 블룸 단 수 반환. (0 이면 기본 9 탭 블룸)
	//==============================================================================
	pub fn get_bloom_level_count(&self) -> usize {
		self.multi_level_bloom_effect.as_ref().map_or(0, |multi_level_bloom_effect| multi_level_bloom_effect.get_level_count())
	}

	//==============================================================================
	// 기본 9 탭 블룸의 주고받는 판을 실수로. (true 면 RGBA16F, 기본 false 는 RGBA8, 씬 판이 실수가 아니면 무시)
	//==============================================================================
	pub fn set_bloom_float_color(&mut self, gl: &glow::Context, is_float_color: bool) -> Result<(), String> {
		let use_float_color = is_float_color && self.use_float_color;
		self.bloom_effect.set_use_float_color(gl, use_float_color)
	}

	//==============================================================================
	// 밝기 설정. (색 보정 맨 앞에서 곱한다, 1 이 그대로)
	//==============================================================================
	pub fn set_brightness(&mut self, brightness: f32) {
		self.brightness = brightness.max(0.0);
	}

	//==============================================================================
	// 밝힌 자리 설정. (그 자리에서 수평 거리 inner_radius 안은 밝기 1, outer_radius 밖은 set_brightness 의 밝기, 사이는 부드럽게)
	// - 씬 깊이 텍스처(옵션 use_depth_texture)와 그 프레임의 보기와 투영의 역행렬로 화소마다 월드 자리를 되짚는다. 깊이 텍스처가 없으면 무시한다.
	// - 카메라가 움직이면 프레임마다 다시 준다. 아무것도 그리지 않은 바탕은 먼 면의 자리로 셈한다.
	//==============================================================================
	pub fn set_lit_area(&mut self, center: [f32; 3], inner_radius: f32, outer_radius: f32, inverse_view_projection_matrix: &Matrix4) {
		let safe_outer_radius = outer_radius.max(0.0001);
		self.lit_area = Some(LitArea { center, inner_radius: inner_radius.min(safe_outer_radius).max(0.0), outer_radius: safe_outer_radius, inverse_view_projection_matrix: *inverse_view_projection_matrix });
	}

	//==============================================================================
	// 밝힌 자리 풀기.
	//==============================================================================
	pub fn clear_lit_area(&mut self) {
		self.lit_area = None;
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
		if let Some(mut multi_level_bloom_effect) = self.multi_level_bloom_effect.take() {
			multi_level_bloom_effect.destroy(gl);
		}
		self.composite_pass.destroy(gl);
	}
}

//==============================================================================
//! 다단 블룸 이펙트. (원본 graphics3d/multilevelbloomeffect.js)
//! - 밝은 몫을 반 크기로 뽑아 반씩 줄여 가며 흐리고, 거꾸로 두 배씩 키우며 위 단에 더한다. 첫 단(씬의 반 크기)이 결과다. (get_result_texture)
//! - 뽑기: 원본 네 점의 평균에서 밝기(세 채널 가운데 가장 큰 값)가 문턱을 넘는 몫. 문턱 둘레 무릎 폭만큼은 부드럽게 들어온다. (set_threshold(문턱, 무릎))
//! - 줄이기는 가운데 넷과 네 귀(대각 한 칸)를 섞는 이중 거르개, 키우기는 천막 꼴 아홉 점이다. 셰이더는 원본과 글자까지 같다.
//! - 판은 use_float_color 면 RGBA16F 라 1 을 넘는 밝은 값이 그대로 흐려진다. (HDR 씬에 쓴다, RGBA8 이면 1 에서 잘린다)
//! - 사용: resize(gl, 씬 너비, 높이) → render(gl, 씬 텍스처) → get_result_texture() 를 합성 패스에서 세기를 곱해 더한다.
//! - 부수 효과: 깊이 시험과 섞기를 끄고, TEXTURE0 을 활성으로, 프레임버퍼 바인드가 바뀐다. 섞기 함수는 앞의 것으로 돌린다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::graphics3d::fullscreenpass::FullscreenPass;
use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
use glow::HasContext;

/// 밝은 몫 뽑기. (반 크기로 줄이며 원본 네 점을 고르게, 밝기는 세 채널 가운데 가장 큰 값, 문턱 둘레는 부드러운 무릎)
const EXTRACT_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform vec2 sourceTexel;
uniform float threshold;
uniform float knee;
out vec4 outputColor;
vec3 readSource(vec2 offset) {
	return max(texture(sourceTexture, fragmentTextureCoordinate + sourceTexel * offset).rgb, vec3(0.0));
}
void main() {
	vec3 color = (readSource(vec2(-0.5, -0.5)) + readSource(vec2(0.5, -0.5)) + readSource(vec2(-0.5, 0.5)) + readSource(vec2(0.5, 0.5))) * 0.25;
	float brightness = max(color.r, max(color.g, color.b));
	float soft = clamp(brightness - threshold + knee, 0.0, 2.0 * knee);
	soft = soft * soft / (4.0 * knee + 0.00001);
	float contribution = max(soft, brightness - threshold) / max(brightness, 0.00001);
	outputColor = vec4(color * contribution, 1.0);
}
";

/// 반으로 줄이기. (가운데 넷과 네 귀를 섞는 이중 거르개)
const DOWNSAMPLE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform vec2 sourceTexel;
out vec4 outputColor;
void main() {
	vec3 color = texture(sourceTexture, fragmentTextureCoordinate).rgb * 4.0;
	color += texture(sourceTexture, fragmentTextureCoordinate + sourceTexel * vec2(-1.0, -1.0)).rgb;
	color += texture(sourceTexture, fragmentTextureCoordinate + sourceTexel * vec2(1.0, -1.0)).rgb;
	color += texture(sourceTexture, fragmentTextureCoordinate + sourceTexel * vec2(-1.0, 1.0)).rgb;
	color += texture(sourceTexture, fragmentTextureCoordinate + sourceTexel * vec2(1.0, 1.0)).rgb;
	outputColor = vec4(color / 8.0, 1.0);
}
";

/// 두 배로 키우기. (천막 꼴 아홉 점, 더하기 섞기로 위 단에 얹는다)
const UPSAMPLE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform vec2 sourceTexel;
out vec4 outputColor;
void main() {
	vec2 offset = sourceTexel;
	vec3 color = texture(sourceTexture, fragmentTextureCoordinate).rgb * 4.0;
	color += (texture(sourceTexture, fragmentTextureCoordinate + vec2(-offset.x, 0.0)).rgb + texture(sourceTexture, fragmentTextureCoordinate + vec2(offset.x, 0.0)).rgb + texture(sourceTexture, fragmentTextureCoordinate + vec2(0.0, -offset.y)).rgb + texture(sourceTexture, fragmentTextureCoordinate + vec2(0.0, offset.y)).rgb) * 2.0;
	color += texture(sourceTexture, fragmentTextureCoordinate + vec2(-offset.x, -offset.y)).rgb + texture(sourceTexture, fragmentTextureCoordinate + vec2(offset.x, -offset.y)).rgb + texture(sourceTexture, fragmentTextureCoordinate + vec2(-offset.x, offset.y)).rgb + texture(sourceTexture, fragmentTextureCoordinate + vec2(offset.x, offset.y)).rgb;
	outputColor = vec4(color / 16.0, 1.0);
}
";

//==============================================================================
/// 다단 블룸 이펙트.
//==============================================================================
pub struct MultiLevelBloomEffect {
	extract_pass: FullscreenPass,
	downsample_pass: FullscreenPass,
	upsample_pass: FullscreenPass,
	/// 단마다의 판. (첫 단이 씬의 반 크기)
	level_render_target_list: Vec<RenderTarget>,
	level_count: usize,
	use_float_color: bool,
	/// 뽑는 문턱. (밝기)
	threshold: f32,
	/// 문턱 둘레의 부드러운 폭.
	knee: f32,
	scene_width: i32,
	scene_height: i32,
}

impl MultiLevelBloomEffect {
	//==============================================================================
	/// 생성. (단 수, 실수 판인지, 원본 기본값은 5 와 true)
	//==============================================================================
	pub fn new(gl: &glow::Context, level_count: usize, use_float_color: bool) -> Result<MultiLevelBloomEffect, String> {
		let mut multi_level_bloom_effect = MultiLevelBloomEffect {
			extract_pass: FullscreenPass::new(gl, EXTRACT_FRAGMENTSHADER_SOURCE)?,
			downsample_pass: FullscreenPass::new(gl, DOWNSAMPLE_FRAGMENTSHADER_SOURCE)?,
			upsample_pass: FullscreenPass::new(gl, UPSAMPLE_FRAGMENTSHADER_SOURCE)?,
			level_render_target_list: Vec::new(),
			level_count: level_count.max(1),
			use_float_color,
			threshold: 1.0,
			knee: 0.25,
			scene_width: 2,
			scene_height: 2,
		};
		multi_level_bloom_effect.create_levels(gl)?;
		Ok(multi_level_bloom_effect)
	}

	//==============================================================================
	/// 단마다의 판 다시 짓기. (씬 크기와 단 수로)
	//==============================================================================
	fn create_levels(&mut self, gl: &glow::Context) -> Result<(), String> {
		self.destroy_levels(gl);
		let options = RenderTargetOptions { use_float_color: self.use_float_color, ..RenderTargetOptions::default() };
		let mut level_width = self.scene_width;
		let mut level_height = self.scene_height;
		for _level_index in 0..self.level_count {
			level_width = (level_width / 2).max(1);
			level_height = (level_height / 2).max(1);
			let level_render_target = RenderTarget::new(gl, level_width, level_height, false, Some(options))?;
			self.level_render_target_list.push(level_render_target);
		}
		Ok(())
	}

	//==============================================================================
	/// 단마다의 판 지우기.
	//==============================================================================
	fn destroy_levels(&mut self, gl: &glow::Context) {
		for level_render_target in self.level_render_target_list.iter_mut() {
			level_render_target.destroy(gl);
		}
		self.level_render_target_list.clear();
	}

	//==============================================================================
	/// 크기 변경. (씬 해상도 기준, 같으면 그대로)
	//==============================================================================
	pub fn resize(&mut self, gl: &glow::Context, scene_width: i32, scene_height: i32) -> Result<(), String> {
		let safe_width = scene_width.max(1);
		let safe_height = scene_height.max(1);
		if safe_width == self.scene_width && safe_height == self.scene_height {
			return Ok(());
		}
		self.scene_width = safe_width;
		self.scene_height = safe_height;
		self.create_levels(gl)
	}

	//==============================================================================
	/// 단 수 설정. (1 이상, 같으면 그대로)
	//==============================================================================
	pub fn set_level_count(&mut self, gl: &glow::Context, level_count: usize) -> Result<(), String> {
		let safe_level_count = level_count.max(1);
		if safe_level_count == self.level_count {
			return Ok(());
		}
		self.level_count = safe_level_count;
		self.create_levels(gl)
	}

	//==============================================================================
	/// 단 수 반환.
	//==============================================================================
	pub fn get_level_count(&self) -> usize {
		self.level_count
	}

	//==============================================================================
	/// 실수 판 사용 여부 반환.
	//==============================================================================
	pub fn get_use_float_color(&self) -> bool {
		self.use_float_color
	}

	//==============================================================================
	/// 뽑는 문턱 설정. (밝기, 무릎은 문턱 둘레의 부드러운 폭, 0 이면 딱 자름)
	//==============================================================================
	pub fn set_threshold(&mut self, threshold: f32, knee: f32) {
		self.threshold = threshold.max(0.0);
		self.knee = knee.max(0.0);
	}

	//==============================================================================
	/// 블룸 렌더링. (호출 후 결과는 get_result_texture())
	//==============================================================================
	pub fn render(&self, gl: &glow::Context, scene_color_texture: Option<GlTexture>) {
		let level_render_target_list = &self.level_render_target_list;
		let level_count = level_render_target_list.len();
		if level_count == 0 {
			return;
		}
		unsafe {
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
			gl.active_texture(glow::TEXTURE0);
		}

		// 1. 밝은 몫을 반 크기로 뽑기. (씬 → 첫 단)
		let extract_pass = &self.extract_pass;
		level_render_target_list[0].bind(gl);
		extract_pass.use_program(gl);
		unsafe {
			gl.uniform_1_i32(extract_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
			gl.uniform_2_f32(extract_pass.get_uniform_location(gl, "sourceTexel").as_ref(), 1.0 / self.scene_width as f32, 1.0 / self.scene_height as f32);
			gl.uniform_1_f32(extract_pass.get_uniform_location(gl, "threshold").as_ref(), self.threshold);
			gl.uniform_1_f32(extract_pass.get_uniform_location(gl, "knee").as_ref(), self.knee);
			gl.bind_texture(glow::TEXTURE_2D, scene_color_texture);
		}
		extract_pass.draw(gl);

		// 2. 반씩 줄이기.
		let downsample_pass = &self.downsample_pass;
		downsample_pass.use_program(gl);
		let downsample_texel_location = downsample_pass.get_uniform_location(gl, "sourceTexel");
		unsafe {
			gl.uniform_1_i32(downsample_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
		}
		for level_index in 1..level_count {
			let source_render_target = &level_render_target_list[level_index - 1];
			level_render_target_list[level_index].bind(gl);
			unsafe {
				gl.uniform_2_f32(downsample_texel_location.as_ref(), 1.0 / source_render_target.get_width() as f32, 1.0 / source_render_target.get_height() as f32);
				gl.bind_texture(glow::TEXTURE_2D, source_render_target.get_color_texture(0));
			}
			downsample_pass.draw(gl);
		}

		// 3. 거꾸로 두 배씩 키우며 위 단에 더하기.
		let (blend_source_rgb, blend_destination_rgb, blend_source_alpha, blend_destination_alpha) = unsafe {
			(gl.get_parameter_i32(glow::BLEND_SRC_RGB) as u32, gl.get_parameter_i32(glow::BLEND_DST_RGB) as u32, gl.get_parameter_i32(glow::BLEND_SRC_ALPHA) as u32, gl.get_parameter_i32(glow::BLEND_DST_ALPHA) as u32)
		};
		let upsample_pass = &self.upsample_pass;
		upsample_pass.use_program(gl);
		let upsample_texel_location = upsample_pass.get_uniform_location(gl, "sourceTexel");
		unsafe {
			gl.uniform_1_i32(upsample_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
			gl.enable(glow::BLEND);
			gl.blend_func(glow::ONE, glow::ONE);
		}
		for level_index in (1..level_count).rev() {
			let source_render_target = &level_render_target_list[level_index];
			level_render_target_list[level_index - 1].bind(gl);
			unsafe {
				gl.uniform_2_f32(upsample_texel_location.as_ref(), 1.0 / source_render_target.get_width() as f32, 1.0 / source_render_target.get_height() as f32);
				gl.bind_texture(glow::TEXTURE_2D, source_render_target.get_color_texture(0));
			}
			upsample_pass.draw(gl);
		}
		unsafe {
			gl.disable(glow::BLEND);
			gl.blend_func_separate(blend_source_rgb, blend_destination_rgb, blend_source_alpha, blend_destination_alpha);
		}
	}

	//==============================================================================
	/// 결과 텍스처 반환. (render() 이후, 첫 단)
	//==============================================================================
	pub fn get_result_texture(&self) -> Option<GlTexture> {
		self.level_render_target_list.first().and_then(|level_render_target| level_render_target.get_color_texture(0))
	}

	//==============================================================================
	/// 파괴. (GL 리소스 해제)
	//==============================================================================
	pub fn destroy(&mut self, gl: &glow::Context) {
		self.destroy_levels(gl);
		self.extract_pass.destroy(gl);
		self.downsample_pass.destroy(gl);
		self.upsample_pass.destroy(gl);
	}
}

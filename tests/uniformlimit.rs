//==============================================================================
//! 정점 유니폼 칸 한도 시험. (원본 vanilla.js tests/graphics3d/uniformlimitchrome.test.mjs 와 같은 검사)
//! - WebGL2 가 보장하는 최솟값은 MAX_VERTEX_UNIFORM_VECTORS 256, MAX_FRAGMENT_UNIFORM_VECTORS 224 칸이다.
//!   데스크탑은 4096 이라 넘어도 돌지만 안드로이드(갤럭시 Z 폴드3)는 256 에서 링크에 실패한다.
//! - 엔진이 짓는 프로그램마다 기본 블록 유니폼을 vec4 칸으로 센다. (mat4 4 칸, 배열은 원소마다 한 칸 이상, 유니폼 블록은 따로 센다)
//!   툰 몸 기능 비트를 전부 켠 것(TOON_FEATURE_ALL)과 하나씩 뺀 것, 인스턴스 꼴, 모서리 어둠, 바깥선, 그림자 깊이, 선의 판 아홉, 스키닝 렌더러, 피부 렌더러.
//!   뼈 행렬 배열은 유니폼 블록(JointBlock)으로 넘겨 기본 블록에 들지 않는다.
//! - Engine::new 가 지은 숨긴 창의 GL 컨텍스트(창을 띄우지 않는다)에서 돌린다. GL 컨텍스트를 짓지 못하면 실패한다.
//! - 실행: cargo test --test uniformlimit
//==============================================================================
use vanilla::{ColorSpace, Engine, EngineConfiguration, GlContext, HumanSkinRenderer, ShaderProgram, SkinnedModelRenderer, ToonInstanceLayout, ToonMaterial, ToonMaterialChunks, ToonRenderer, ToonScreenOutline, compose_toon_program_sources};

/// 센 결과 하나. (정점, 조각 칸)
struct Count {
	vertex_rows: f64,
	fragment_rows: f64,
	has_joint_block: bool,
}

//==============================================================================
/// 한 유니폼이 쓰는 vec4 칸. (원본 시험의 readRows 와 같다, 샘플러는 0)
//==============================================================================
fn read_rows(uniform_type: u32, size: i32) -> f64 {
	let matrix_rows = match uniform_type {
		glow::FLOAT_MAT2 => Some(2.0),
		glow::FLOAT_MAT3 => Some(3.0),
		glow::FLOAT_MAT4 => Some(4.0),
		_ => None,
	};
	if let Some(matrix_rows) = matrix_rows {
		return matrix_rows * size as f64;
	}
	let scalar_size = match uniform_type {
		glow::FLOAT | glow::INT | glow::UNSIGNED_INT | glow::BOOL => 0.25,
		glow::FLOAT_VEC2 | glow::INT_VEC2 => 0.5,
		glow::FLOAT_VEC3 | glow::FLOAT_VEC4 | glow::INT_VEC3 | glow::INT_VEC4 => 1.0,
		_ => return 0.0,
	};
	if size > 1 { size as f64 } else { scalar_size }
}

//==============================================================================
/// 프로그램의 기본 블록 유니폼을 센다. (이름이 그 단계 소스에 나오면 그 단계의 것으로 본다, 소스를 모르면 둘 다에 센다)
//==============================================================================
fn count_program(gl: &GlContext, program: &ShaderProgram, vertex_source: Option<&str>, fragment_source: Option<&str>) -> Count {
	let native = program.get_program();
	let mut vertex_rows = 0.0;
	let mut fragment_rows = 0.0;
	unsafe {
		let count = gl.get_active_uniforms(native);
		let indices: Vec<u32> = (0..count).collect();
		let blocks = if count > 0 { gl.get_active_uniforms_parameter(native, &indices, glow::UNIFORM_BLOCK_INDEX) } else { Vec::new() };
		for index in 0..count {
			if blocks[index as usize] != -1 {
				continue;
			}
			let info = gl.get_active_uniform(native, index).unwrap();
			let base_name = info.name.trim_end_matches("[0]");
			let rows = read_rows(info.utype, info.size);
			let in_source = |source: Option<&str>| match source {
				Some(source) => contains_word(source, base_name),
				None => true,
			};
			if in_source(vertex_source) {
				vertex_rows += rows;
			}
			if in_source(fragment_source) {
				fragment_rows += rows;
			}
		}
		let has_joint_block = gl.get_uniform_block_index(native, "JointBlock").is_some();
		Count { vertex_rows: vertex_rows.ceil(), fragment_rows: fragment_rows.ceil(), has_joint_block }
	}
}

//==============================================================================
/// 식별자가 소스에 낱말로 나오는지.
//==============================================================================
fn contains_word(source: &str, word: &str) -> bool {
	let bytes = source.as_bytes();
	let is_identifier = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
	let mut start = 0;
	while let Some(found) = source[start..].find(word) {
		let begin = start + found;
		let end = begin + word.len();
		if (begin == 0 || !is_identifier(bytes[begin - 1])) && (end >= bytes.len() || !is_identifier(bytes[end])) {
			return true;
		}
		start = begin + 1;
	}
	false
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let graphic = engine.get_graphic().expect("GL 컨텍스트를 짓지 못함 (화면 서버)");
	let gl = graphic.get_gl();
	let mut count_list: Vec<(String, Count)> = Vec::new();
	let mut toon_count = 0;

	// 툰 몸 기능 비트: 전부 켠 것(TOON_FEATURE_ALL), 하나씩 뺀 것, 전부 뺀 것. 재질과 인스턴스 꼴, 모서리 어둠의 조합.
	let feature_name_list = ["baseTexture", "emissiveTexture", "decalTexture", "matcap", "angelRing", "rampTexture", "shadeStops", "rim", "shadeRim", "shadow", "intensityLimit"];
	let mut feature_set_list: Vec<Vec<&str>> = vec![Vec::new(), feature_name_list.to_vec()];
	for name in feature_name_list {
		feature_set_list.push(vec![name]);
	}
	let layout = ToonInstanceLayout::new(&[("instanceMatrix", 16), ("instanceTint", 4), ("instanceLineId", 1)]).unwrap();
	let material = ToonMaterial::new(ToonMaterialChunks { name: String::from("probe"), surface_chunk: String::from("void applySurface(inout ToonSurface surface) { surface.baseColor *= 0.8; }"), ..ToonMaterialChunks::default() });
	for use_material in [false, true] {
		for use_layout in [false, true] {
			for is_ambient_occlusion in [false, true] {
				for disabled_features in &feature_set_list {
					let sources = compose_toon_program_sources(if use_material { Some(&material) } else { None }, if use_layout { Some(&layout) } else { None }, is_ambient_occlusion, ColorSpace::Gamma, disabled_features);
					let attributes: Vec<(&str, u32)> = sources.attribute_locations.iter().map(|(name, location)| (name.as_str(), *location)).collect();
					let mut pairs = vec![("solid", &sources.solid_vertex_shader, &sources.solid_fragment_shader)];
					if disabled_features.is_empty() {
						pairs.push(("outline", &sources.outline_vertex_shader, &sources.outline_fragment_shader));
						pairs.push(("depth", &sources.depth_vertex_shader, &sources.depth_fragment_shader));
					}
					for (kind, vertex_source, fragment_source) in pairs {
						let program = ShaderProgram::create_with_attribute_locations(gl, vertex_source, fragment_source, &attributes).expect("툰 프로그램 링크");
						count_list.push((format!("툰 {} 재질 {} 꼴 {} 어둠 {} 끈 기능 {:?}", kind, use_material, use_layout, is_ambient_occlusion, disabled_features), count_program(gl, &program, Some(vertex_source), Some(fragment_source))));
						toon_count += 1;
					}
				}
			}
		}
	}

	// 선의 판 아홉 프로그램.
	for (index, (vertex_source, fragment_source)) in ToonScreenOutline::read_program_sources(ColorSpace::Gamma).iter().enumerate() {
		let attributes: Vec<(&str, u32)> = if index == 0 { vanilla::TOON_ATTRIBUTE_LOCATIONS.to_vec().into_iter().chain([("instanceLineId", vanilla::TOON_INSTANCE_LINE_ID_LOCATION)]).collect() } else { Vec::new() };
		let program = ShaderProgram::create_with_attribute_locations(gl, vertex_source, fragment_source, &attributes).expect("선의 판 프로그램 링크");
		count_list.push((format!("선의 판 {}", index), count_program(gl, &program, Some(vertex_source), Some(fragment_source))));
	}

	// 렌더러가 짓는 기본 프로그램. (소스를 따로 들지 않아 모든 유니폼을 두 단계에 다 센다)
	let toon_renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	count_list.push((String::from("툰 렌더러 몸"), count_program(gl, toon_renderer.get_solid_program(), None, None)));
	count_list.push((String::from("툰 렌더러 껍데기"), count_program(gl, toon_renderer.get_outline_program(), None, None)));
	count_list.push((String::from("툰 렌더러 깊이"), count_program(gl, toon_renderer.get_depth_program(), None, None)));
	let skinned_renderer = SkinnedModelRenderer::new(gl, None, None).expect("스키닝 렌더러");
	count_list.push((String::from("스키닝 몸"), count_program(gl, &skinned_renderer.get_shader_program(), None, None)));
	count_list.push((String::from("스키닝 깊이"), count_program(gl, skinned_renderer.get_depth_shader_program(), None, None)));
	count_list.push((String::from("스키닝 선"), count_program(gl, skinned_renderer.get_wireframe_shader_program(), None, None)));
	let human_renderer = HumanSkinRenderer::new(gl).expect("피부 렌더러");
	count_list.push((String::from("피부 몸"), count_program(gl, &human_renderer.get_shader_program(), None, None)));
	count_list.push((String::from("피부 컷아웃 깊이"), count_program(gl, human_renderer.get_cutout_depth_shader_program(), None, None)));
	count_list.push((String::from("피부 깊이"), count_program(gl, human_renderer.get_depth_shader_program(), None, None)));

	let mut check_count = 0u32;
	let mut joint_program_count = 0;
	let mut worst_vertex = 0.0f64;
	let mut worst_fragment = 0.0f64;
	for (name, count) in &count_list {
		check_count += 2;
		assert!(count.vertex_rows <= 256.0, "시험 실패: {} 정점 유니폼 {} 칸이 WebGL2 최솟값 256 칸 안", name, count.vertex_rows);
		assert!(count.fragment_rows <= 224.0, "시험 실패: {} 조각 유니폼 {} 칸이 WebGL2 최솟값 224 칸 안", name, count.fragment_rows);
		worst_vertex = worst_vertex.max(count.vertex_rows);
		worst_fragment = worst_fragment.max(count.fragment_rows);
		joint_program_count += if count.has_joint_block { 1 } else { 0 };
	}
	assert!(toon_count > 100, "시험 실패: 툰 몸 기능 변형을 센 수 ({})", toon_count);
	assert!(joint_program_count >= 40, "시험 실패: 뼈 행렬 블록(JointBlock)을 쓰는 프로그램 수 ({})", joint_program_count);
	println!("유니폼 칸 한도 시험 통과 (프로그램 {} 개, 정점 최대 {} 칸, 조각 최대 {} 칸, {} 개 확인)", count_list.len(), worst_vertex, worst_fragment, check_count);
}

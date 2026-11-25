//==============================================================================
// 툰 머티리얼과 인스턴스 꼴. (vanilla.js experimental/graphics/toon/toonmaterial.js)
// - ToonMaterial: ToonRenderer 의 몸 셰이더에 끼울 셰이더 조각, 게임마다의 확장 자리.
//   엔진의 표준 기능(텍스처, 빛, 틴트, 섞기)으로 되지 않는 무늬나 연출만 조각으로 넣는다. 조각이 없는 칸은 엔진의 빈 함수가 들어간다.
//   - uniform_chunk: 조각 셰이더의 맨 위 칸에 들어간다. 유니폼, 상수, 함수, 정점 조각이 내보낸 in 을 적는다.
//   - surface_chunk: void applySurface(inout ToonSurface surface) 를 정의한다. 엔진이 꼭짓점 색, 텍스처, 데칼을 셈한 뒤, 틴트와 빛 전에 부른다.
//     ToonSurface { vec3 baseColor (sRGB), float alpha, vec3 emission (선형), vec3 normal (월드, 단위), float glow (꼭짓점 빛남), bool isUnlit,
//                   vec4 detail (꼭짓점 무늬 번호와 무늬 색), vec2 uv, vec3 localPosition (모형 좌표), vec3 worldPosition, vec3 viewDirection (면에서 카메라 쪽, 단위) }
//   - light_chunk: void applyLight(ToonSurface surface, inout ToonLighting lighting) 를 정의한다. 엔진이 해, 바탕빛, 채움, 림, 점광, 정반사를 모은 뒤 합치기 전에 부른다.
//     ToonLighting { vec3 irradiance (바탕색에 곱할 빛), vec3 specular (더할 빛), float shadow (0 ~ 1) }
//   - vertex_chunk: 정점 셰이더(몸, 바깥선, 그림자 깊이 셋 모두)의 맨 위 칸에 들어가고 void applyVertex(inout ToonVertex vertex) 를 정의한다. 월드로 옮기기 전에 부른다.
//     ToonVertex { vec3 localPosition, vec3 localNormal, vec3 color, vec2 material, vec4 detail, vec2 uv, vec4 tint, mat4 modelMatrix }
//     인스턴스 꼴(ToonInstanceLayout)의 사용자 어트리뷰트는 그 이름의 in 으로 읽는다. 셋 모두에 들어가므로 몸에만 있는 out 은 쓰지 않는다. (꼭짓점 어트리뷰트 이름 대신 vertex 의 칸을 쓴다)
//     노멀은 mat3(modelMatrix) 로 옮긴다. 크기가 고르지 않으면 localNormal 을 그 크기의 제곱으로 나눠 둔다.
//   - 셰이더에는 엔진의 toLinear(vec3) 와 RECIPROCAL_PI 가 있다.
//   - 텍스처는 그리기의 textures([(유니폼 이름, 텍스처)])로 넘긴다. 엔진이 쓰지 않는 유닛(7 부터)을 차례로 묶고 그 이름의 sampler2D 유니폼에 유닛을 넣는다.
//   - 그리기마다 다른 유니폼은 on_before_draw(program, gl, ToonProgramKind) 에서 쓴다. 엔진의 유니폼은 엔진이 다시 쓰므로 머티리얼의 것만 쓴다.
// - ToonInstanceLayout: 인스턴스 하나의 어트리뷰트 차례. (ToonRenderer::create_instance_buffer 와 draw_mesh_instance_buffer 가 쓴다)
//   어트리뷰트마다 (이름, 크기). 이름이 instanceMatrix(16) 면 모형 행렬, instanceTint(4) 면 곱하는 색으로 엔진이 읽는다.
//   그 밖의 이름은 사용자 어트리뷰트다. TOON_MAXIMUM_INSTANCE_ATTRIBUTES 개까지, 크기 1 ~ 4. 정점 조각이 그 이름의 in 으로 읽는다. (자리 13 부터)
//   instanceMatrix 가 없으면 모형 행렬은 단위 행렬이다. 정점 조각이 vertex.modelMatrix 를 지어 넣는다. instanceTint 가 없으면 틴트는 1 이다.
// - 원본은 머티리얼을 꼴 객체({ uniformChunk, ... })로 짓는다. 러스트 판은 ToonMaterialChunks 를 넘긴다.
//==============================================================================
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

// 인스턴스 꼴에 넣을 수 있는 사용자 어트리뷰트 수의 상한. (어트리뷰트 자리 13, 14, 15)
pub const TOON_MAXIMUM_INSTANCE_ATTRIBUTES: usize = 3;

// 사용자 인스턴스 어트리뷰트의 첫 자리.
pub const TOON_FIRST_INSTANCE_ATTRIBUTE_LOCATION: u32 = 13;

// 인스턴스 꼴에서 엔진이 읽는 이름. (행렬 16 은 모형 행렬, 틴트 4 는 곱하는 색)
pub const TOON_INSTANCE_MATRIX_NAME: &str = "instanceMatrix";
pub const TOON_INSTANCE_TINT_NAME: &str = "instanceTint";

// 머티리얼과 인스턴스 꼴마다 다른 번호를 주는 셈. (렌더러의 프로그램 캐시 열쇠, 0 은 기본)
static NEXT_MATERIAL_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_INSTANCE_LAYOUT_ID: AtomicU32 = AtomicU32::new(1);

//==============================================================================
// 섞는 방식. (Alpha 는 곧은 알파 SRC_ALPHA, Premultiplied 는 미리 곱한 알파 ONE, Additive 는 더하기 ONE, ONE)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToonBlendMode {
	#[default]
	Alpha,
	Premultiplied,
	Additive,
}

//==============================================================================
// 인스턴스 버퍼의 쓰임. (Static 은 한 번 올리고 오래 그림, Dynamic 은 자주 고침)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToonBufferUsage {
	#[default]
	Static,
	Dynamic,
}

//==============================================================================
// 머티리얼의 셰이더 조각. (빈 글이면 엔진의 빈 함수)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ToonMaterialChunks {
	pub name: String,
	pub uniform_chunk: String,
	pub surface_chunk: String,
	pub light_chunk: String,
	pub vertex_chunk: String,
}

#[derive(Clone, Debug)]
pub struct ToonMaterial {
	id: u32,
	chunks: ToonMaterialChunks,
}

impl ToonMaterial {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(chunks: ToonMaterialChunks) -> ToonMaterial {
		ToonMaterial { id: NEXT_MATERIAL_ID.fetch_add(1, Ordering::Relaxed), chunks }
	}

	//==============================================================================
	// 번호 반환. (머티리얼마다 다르다)
	//==============================================================================
	pub fn get_id(&self) -> u32 {
		self.id
	}

	//==============================================================================
	// 이름 반환. (알림 글에 쓴다)
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.chunks.name
	}

	//==============================================================================
	// 유니폼 조각 반환.
	//==============================================================================
	pub fn get_uniform_chunk(&self) -> &str {
		&self.chunks.uniform_chunk
	}

	//==============================================================================
	// 표면 조각 반환.
	//==============================================================================
	pub fn get_surface_chunk(&self) -> &str {
		&self.chunks.surface_chunk
	}

	//==============================================================================
	// 빛 조각 반환.
	//==============================================================================
	pub fn get_light_chunk(&self) -> &str {
		&self.chunks.light_chunk
	}

	//==============================================================================
	// 정점 조각 반환.
	//==============================================================================
	pub fn get_vertex_chunk(&self) -> &str {
		&self.chunks.vertex_chunk
	}
}

//==============================================================================
// 인스턴스 어트리뷰트 하나. (offset 은 수 단위, location 은 사용자 어트리뷰트만 Some)
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToonInstanceAttribute {
	pub name: String,
	pub size: usize,
	pub offset: usize,
	pub location: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ToonInstanceLayout {
	id: u32,
	attribute_list: Vec<ToonInstanceAttribute>,
	float_count: usize,
	has_matrix: bool,
	has_tint: bool,
}

//==============================================================================
// 셰이더 이름으로 쓸 수 있는지. (글자나 _ 로 시작하고 글자, 숫자, _ 만)
//==============================================================================
fn is_shader_name(name: &str) -> bool {
	let mut characters = name.chars();
	match characters.next() {
		Some(first) if first.is_ascii_alphabetic() || first == '_' => {
			characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
		}
		_ => {
			false
		}
	}
}

impl ToonInstanceLayout {
	//==============================================================================
	// 생성. ((이름, 크기) 목록, 이름이 겹치거나, 크기가 맞지 않거나, 사용자 어트리뷰트가 너무 많으면 Err)
	//==============================================================================
	pub fn new(attributes: &[(&str, usize)]) -> Result<ToonInstanceLayout, String> {
		let mut attribute_list = Vec::new();
		let mut float_count = 0;
		let mut has_matrix = false;
		let mut has_tint = false;
		let mut name_set = HashSet::new();
		let mut custom_count = 0usize;
		for (name, size) in attributes.iter().copied() {
			if !is_shader_name(name) || !name_set.insert(name) {
				return Err(format!("인스턴스 어트리뷰트 이름이 맞지 않는다: {}", name));
			}
			let mut location = None;
			if name == TOON_INSTANCE_MATRIX_NAME {
				if size != 16 {
					return Err(String::from("instanceMatrix 의 크기는 16 이다."));
				}
				has_matrix = true;
			}
			else if name == TOON_INSTANCE_TINT_NAME {
				if size != 4 {
					return Err(String::from("instanceTint 의 크기는 4 이다."));
				}
				has_tint = true;
			}
			else {
				if !(1..=4).contains(&size) {
					return Err(format!("인스턴스 어트리뷰트 {} 의 크기는 1 ~ 4 이다.", name));
				}
				if custom_count >= TOON_MAXIMUM_INSTANCE_ATTRIBUTES {
					return Err(format!("사용자 인스턴스 어트리뷰트는 {} 개까지다.", TOON_MAXIMUM_INSTANCE_ATTRIBUTES));
				}
				location = Some(TOON_FIRST_INSTANCE_ATTRIBUTE_LOCATION + custom_count as u32);
				custom_count += 1;
			}
			attribute_list.push(ToonInstanceAttribute { name: String::from(name), size, offset: float_count, location });
			float_count += size;
		}
		Ok(ToonInstanceLayout { id: NEXT_INSTANCE_LAYOUT_ID.fetch_add(1, Ordering::Relaxed), attribute_list, float_count, has_matrix, has_tint })
	}

	//==============================================================================
	// 번호 반환. (꼴마다 다르다)
	//==============================================================================
	pub fn get_id(&self) -> u32 {
		self.id
	}

	//==============================================================================
	// 어트리뷰트 목록 반환.
	//==============================================================================
	pub fn get_attributes(&self) -> &[ToonInstanceAttribute] {
		&self.attribute_list
	}

	//==============================================================================
	// 인스턴스 하나의 수 개수.
	//==============================================================================
	pub fn get_float_count(&self) -> usize {
		self.float_count
	}

	//==============================================================================
	// 모형 행렬이 있는지.
	//==============================================================================
	pub fn has_matrix(&self) -> bool {
		self.has_matrix
	}

	//==============================================================================
	// 틴트가 있는지.
	//==============================================================================
	pub fn has_tint(&self) -> bool {
		self.has_tint
	}

	//==============================================================================
	// 사용자 어트리뷰트의 셰이더 선언. (in float / vec2 / vec3 / vec4 이름;)
	//==============================================================================
	pub fn read_declaration_source(&self) -> String {
		let type_names = ["", "float", "vec2", "vec3", "vec4"];
		let mut source = String::new();
		for attribute in self.attribute_list.iter() {
			if attribute.location.is_none() {
				continue;
			}
			source.push_str(&format!("in {} {};\n", type_names[attribute.size], attribute.name));
		}
		source
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn instance_layout_places_custom_attributes() {
		let layout = ToonInstanceLayout::new(&[("instanceShape", 4), ("instanceOffset", 3), ("instanceColor", 3)]).unwrap();
		assert_eq!(layout.get_float_count(), 10);
		assert!(!layout.has_matrix());
		assert_eq!(layout.get_attributes()[1].offset, 4);
		assert_eq!(layout.get_attributes()[2].location, Some(15));
		assert_eq!(layout.read_declaration_source(), "in vec4 instanceShape;\nin vec3 instanceOffset;\nin vec3 instanceColor;\n");
		let matrix_layout = ToonInstanceLayout::new(&[("instanceMatrix", 16), ("instanceTint", 4)]).unwrap();
		assert!(matrix_layout.has_matrix() && matrix_layout.has_tint());
		assert_eq!(matrix_layout.read_declaration_source(), "");
		assert!(ToonInstanceLayout::new(&[("instanceMatrix", 4)]).is_err());
		assert!(ToonInstanceLayout::new(&[("first", 1), ("second", 1), ("third", 1), ("fourth", 1)]).is_err());
		assert!(ToonInstanceLayout::new(&[("1bad", 1)]).is_err());
		assert!(ToonInstanceLayout::new(&[("twice", 1), ("twice", 2)]).is_err());
	}
}

//==============================================================================
// 툰 3D 모형 짓기. (vanilla.js experimental/graphics/toon/meshbuilder.js 의 MeshBuilder 와 기본 도형)
// - 툰 3D(점토 같은 한 색 재질과 바깥선)를 그릴 꼭짓점 배열을 짓는다. 외계행성 광부 키우기의 판 3D 에서 왔다. (three.js 의 기본 도형과 같은 꼴)
// - 도형은 가운데가 원점인 자기 좌표로 짓는다. 부품을 더할 때 자리, 돌림, 크기를 곱해 한 덩어리에 담는다.
// - 꼭짓점 하나는 자리 3, 노멀 3, 색 3(sRGB), 재질 2(빛남의 세기, 빛을 받지 않음), 무늬 4(무늬 번호, 무늬 색) 이다.
//   무늬 번호와 색의 뜻은 그리는 셰이더가 정한다. (0 이면 무늬 없음)
// - 텍스처 좌표(UV)는 꼭짓점 배열 밖의 uv_data 에 꼭짓점마다 2 씩 따로 담는다. UV 를 준 꼭짓점이 하나라도 있으면 나머지는 0 으로 채운다. (도형의 uvs, add_triangles_with_uvs)
// - 2 번 UV 도 같은 꼴로 uv1_data 에 담는다. (도형의 uvs1, add_triangles_with_uvs 의 uvs1, GLB 의 TEXCOORD_1, 천사 고리처럼 두 번째 펼침을 읽는 것)
//   (0, 0) 이 그림의 왼쪽 위다. (glTF 와 같음) 상자는 면마다 0 ~ 1, 구는 경도와 위도로 0 ~ 1 이다.
// - 재질 범위(material_ranges)는 몸의 꼭짓점 번호 가운데 한 재질이 든 자리다. GLB 읽기가 프리미티브마다 담는다. (재질 이름, extras, 텍스처, ToonDrawOptions 의 index_start, index_count)
// - 바깥선은 부품의 점마다 같은 굵기로 민 껍데기다. 뒷면만 그리므로 따로 담는다. (inverted hull)
// - 삼각형의 감기는 꼭짓점 노멀을 보고 바깥이 앞면이 되게 맞춘다. (뒷면 가리기와 바깥선이 감기에 기댄다)
//==============================================================================
use std::collections::HashMap;
use std::f32::consts::PI;

use serde_json::Value;

use crate::base::matrix4::Matrix4;
use crate::graphics3d::toon::glbmeshreader::ToonSkeleton;

// 꼭짓점 하나의 수 개수. (자리 3, 노멀 3, 색 3, 재질 2, 무늬 4)
pub const VERTEX_FLOAT_COUNT: usize = 15;
// 뼈대가 있는 모형의 꼭짓점마다 따로 담는 수 개수. (뼈대 번호 4, 무게 4)
pub const SKIN_FLOAT_COUNT: usize = 8;
// 바깥선의 기본 굵기.
pub const DEFAULT_OUTLINE_WIDTH: f32 = 0.02;
// 바깥선을 모서리에서 더 밀 때 쓰는 가장 작은 코사인. (뾰족한 끝이 굵기의 1.18 배보다 튀지 않게)
const MINIMUM_OUTLINE_MITER_COSINE: f32 = 0.85;
// 감기를 맞출 때 넓이가 이보다 작은 삼각형은 그대로 둔다. (구의 극처럼 겹친 점)
const DEGENERATE_AREA: f32 = 1e-12;

//==============================================================================
// 기본 도형. (가운데가 원점인 자리, 노멀, 꼭짓점 번호)
// - uvs 는 점마다 텍스처 좌표 둘이다. (상자와 구만 짓는다, 비면 없음)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ToonGeometry {
	// 점마다 자리 셋.
	pub positions: Vec<f32>,
	// 점마다 단위 노멀 셋.
	pub normals: Vec<f32>,
	// 삼각형마다 점 번호 셋.
	pub indices: Vec<u32>,
	pub uvs: Vec<f32>,
	/// 점마다 2 번 텍스처 좌표 둘. (비면 없음)
	pub uvs1: Vec<f32>,
}

impl ToonGeometry {
	//==============================================================================
	// 점 하나 담기. (자리와 노멀)
	//==============================================================================
	fn push_point(&mut self, position: [f32; 3], normal: [f32; 3]) {
		self.positions.extend_from_slice(&position);
		self.normals.extend_from_slice(&normal);
	}

	//==============================================================================
	// 점 하나의 텍스처 좌표 담기.
	//==============================================================================
	fn push_uv(&mut self, u: f32, v: f32) {
		self.uvs.push(u);
		self.uvs.push(v);
	}

	//==============================================================================
	// 지금까지 담은 점 수. (다음에 담을 점의 번호)
	//==============================================================================
	fn get_point_count(&self) -> u32 {
		(self.positions.len() / 3) as u32
	}
}

//==============================================================================
// 부품을 더할 때의 꼴. (빛남, 빛을 받지 않음, 바깥선 여부와 굵기)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonPartOptions {
	// 빛남의 세기. (꼭짓점의 재질 첫 칸, 0 이면 빛나지 않음)
	pub glow: f32,
	// 빛을 받지 않음. (꼭짓점의 재질 둘째 칸, 그늘 없이 제 색)
	pub is_unlit: bool,
	// 바깥선 껍데기를 담을지.
	pub has_outline: bool,
	// 바깥선 굵기. (기본 DEFAULT_OUTLINE_WIDTH)
	pub outline_width: f32,
}

impl Default for ToonPartOptions {
	//==============================================================================
	// 기본 꼴. (빛남 0, 빛을 받음, 바깥선 기본 굵기)
	//==============================================================================
	fn default() -> ToonPartOptions {
		ToonPartOptions { glow: 0.0, is_unlit: false, has_outline: true, outline_width: DEFAULT_OUTLINE_WIDTH }
	}
}

//==============================================================================
// 네모 면 하나. (네 점은 둘레 차례, 면의 바깥쪽 노멀)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonQuad {
	pub corners: [[f32; 3]; 4],
	pub normal: [f32; 3],
}

//==============================================================================
// 다 지은 배열. (몸, 바깥선 껍데기, 뼈대 번호와 무게, 뼈대)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ToonMeshArrays {
	// 몸의 꼭짓점. (VERTEX_FLOAT_COUNT 씩)
	pub vertex_data: Vec<f32>,
	pub index_data: Vec<u32>,
	// 바깥선 껍데기의 자리. (3 씩)
	pub outline_vertex_data: Vec<f32>,
	pub outline_index_data: Vec<u32>,
	// 바깥선 껍데기의 점마다 밀기 전 몸의 자리. (3 씩, 받은 껍데기는 그 자리 그대로)
	pub outline_base_data: Vec<f32>,
	// 꼭짓점마다 뼈대 번호 넷과 무게 넷. (뼈대가 있는 모형만, SKIN_FLOAT_COUNT 씩)
	pub skin_data: Vec<f32>,
	// 바깥선 껍데기의 점마다 뼈대 번호 넷과 무게 넷.
	pub outline_skin_data: Vec<f32>,
	// 뼈대. (read_glb_arrays 가 채운다, MeshBuilder 는 None)
	pub skeleton: Option<ToonSkeleton>,
	// GLB 의 안쪽 껍데기와 안쪽 몸이 든 자리. ((시작, 개수) 들, read_glb_arrays 가 채운다)
	pub inner_outline_ranges: Vec<(usize, usize)>,
	pub inner_body_ranges: Vec<(usize, usize)>,
	// 꼭짓점마다 텍스처 좌표 둘. (UV 를 준 꼭짓점이 없으면 빈 것)
	pub uv_data: Vec<f32>,
	/// 꼭짓점마다 2 번 텍스처 좌표 둘. (2 번 UV 를 준 꼭짓점이 없으면 빈 것)
	pub uv1_data: Vec<f32>,
	// 재질 범위. (GLB 프리미티브마다)
	pub material_ranges: Vec<ToonMaterialRange>,
}

//==============================================================================
// 재질 범위. (몸의 꼭짓점 번호 가운데 한 재질이 든 자리, GLB 읽기가 프리미티브마다 담는다)
// - name 재질 이름, material_index 재질 차례(없으면 None), node_name 노드 이름, extras 재질 extras, alpha_mode "OPAQUE" | "MASK" | "BLEND", alpha_cutoff (MASK 의 문턱)
// - base_color_factor, emissive_factor 는 선형 색이다. base_color_image, emissive_image 는 read_glb_images 의 차례다. (없으면 None)
// - 부품 안에서 칸을 갈라 칠할 때(강조색 칸, 표시판) 재질 이름이나 extras 로 고르고 ToonDrawOptions 의 index_start, index_count 로 그 칸만 그린다.
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ToonMaterialRange {
	pub name: String,
	pub material_index: Option<usize>,
	pub node_name: String,
	pub index_start: usize,
	pub index_count: usize,
	pub extras: Value,
	pub alpha_mode: String,
	pub alpha_cutoff: f32,
	pub base_color_factor: [f32; 4],
	pub emissive_factor: [f32; 3],
	pub base_color_image: Option<usize>,
	pub emissive_image: Option<usize>,
	// 이 프리미티브에 텍스처 좌표가 있는지.
	pub has_uv: bool,
}

//==============================================================================
// 잇는 조각 하나. (MeshBuilder::combine 에 넘기는 배열과 옮김, 원본 { arrays, offset })
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ToonMeshPiece<'a> {
	pub arrays: &'a ToonMeshArrays,
	pub offset: [f32; 3],
}

//==============================================================================
// 색 글자를 sRGB 세 수로. ("#rrggbb" → [r, g, b], 0 ~ 1)
//==============================================================================
pub fn read_color_values(color_text: &str) -> [f32; 3] {
	let hex_text = color_text.trim_start_matches('#');
	let value = u32::from_str_radix(hex_text, 16).unwrap_or(0xffffff);
	[((value >> 16) & 255) as f32 / 255.0, ((value >> 8) & 255) as f32 / 255.0, (value & 255) as f32 / 255.0]
}

//==============================================================================
// 자리, 돌림, 크기로 변환 행렬 짓기. (부모 행렬 뒤에 곱함, 돌림은 three.js 의 기본 차례 XYZ)
//==============================================================================
pub fn create_part_matrix(parent_matrix: Option<&Matrix4>, position: [f32; 3], rotation: Option<[f32; 3]>, scale: Option<[f32; 3]>) -> Matrix4 {
	let mut matrix = match parent_matrix {
		Some(parent_matrix) => {
			*parent_matrix
		}
		None => {
			Matrix4::default()
		}
	};
	matrix.translate(position[0], position[1], position[2]);
	if let Some(rotation) = rotation {
		matrix.rotate_x(rotation[0]);
		matrix.rotate_y(rotation[1]);
		matrix.rotate_z(rotation[2]);
	}
	if let Some(scale) = scale {
		matrix.scale(scale[0], scale[1], scale[2]);
	}
	matrix
}

//==============================================================================
// 네모 상자. (면마다 꼭짓점을 따로 두어 모서리가 날카롭다)
//==============================================================================
pub fn create_box_geometry(width: f32, height: f32, depth: f32) -> ToonGeometry {
	let mut geometry = ToonGeometry::default();
	let half_sizes = [width * 0.5, height * 0.5, depth * 0.5];
	for axis_index in 0..3 {
		for sign in [-1.0f32, 1.0] {
			let first_axis = (axis_index + 1) % 3;
			let second_axis = (axis_index + 2) % 3;
			let start_index = geometry.get_point_count();
			for (first_sign, second_sign) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
				let mut point = [0.0; 3];
				point[axis_index] = half_sizes[axis_index] * sign;
				point[first_axis] = half_sizes[first_axis] * first_sign;
				point[second_axis] = half_sizes[second_axis] * second_sign;
				let mut normal = [0.0; 3];
				normal[axis_index] = sign;
				geometry.push_point(point, normal);
				geometry.push_uv((first_sign + 1.0) * 0.5, (1.0 - second_sign) * 0.5);
			}
			geometry.indices.extend_from_slice(&[start_index, start_index + 1, start_index + 2, start_index, start_index + 2, start_index + 3]);
		}
	}
	geometry
}

//==============================================================================
// 모서리가 둥근 상자. (면마다 격자를 깔고 안쪽 상자에서 반지름만큼 밀어낸다)
//==============================================================================
pub fn create_rounded_box_geometry(width: f32, height: f32, depth: f32, segments: u32, radius: f32) -> ToonGeometry {
	let mut geometry = ToonGeometry::default();
	let half_sizes = [width * 0.5, height * 0.5, depth * 0.5];
	let inner_sizes = half_sizes.map(|half_size| (half_size - radius).max(0.0));
	let segments = segments.max(1);
	// 축마다 격자의 자리. (둥근 곳은 마디 수만큼, 가운데 평평한 곳은 한 칸)
	let axis_samples: Vec<Vec<f32>> = (0..3).map(|axis_index| {
		let half_size = half_sizes[axis_index];
		let inner_size = inner_sizes[axis_index];
		let mut samples = Vec::new();
		for segment_index in 0..=segments {
			samples.push(-half_size + (half_size - inner_size) * segment_index as f32 / segments as f32);
		}
		for segment_index in 0..=segments {
			samples.push(inner_size + (half_size - inner_size) * segment_index as f32 / segments as f32);
		}
		samples
	}).collect();
	for axis_index in 0..3 {
		for sign in [-1.0f32, 1.0] {
			let first_axis = (axis_index + 1) % 3;
			let second_axis = (axis_index + 2) % 3;
			let first_samples = &axis_samples[first_axis];
			let second_samples = &axis_samples[second_axis];
			let start_index = geometry.get_point_count();
			for second_value in second_samples {
				for first_value in first_samples {
					let mut point = [0.0; 3];
					point[axis_index] = half_sizes[axis_index] * sign;
					point[first_axis] = *first_value;
					point[second_axis] = *second_value;
					let inner = [point[0].clamp(-inner_sizes[0], inner_sizes[0]), point[1].clamp(-inner_sizes[1], inner_sizes[1]), point[2].clamp(-inner_sizes[2], inner_sizes[2])];
					let direction = [point[0] - inner[0], point[1] - inner[1], point[2] - inner[2]];
					let length = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
					let mut normal = if length > 0.0 { [direction[0] / length, direction[1] / length, direction[2] / length] } else { [0.0; 3] };
					if length <= 0.0 {
						normal[axis_index] = sign;
					}
					geometry.push_point([inner[0] + normal[0] * radius, inner[1] + normal[1] * radius, inner[2] + normal[2] * radius], normal);
				}
			}
			let row_length = first_samples.len() as u32;
			for second_index in 0..(second_samples.len() as u32 - 1) {
				for first_index in 0..(row_length - 1) {
					let corner = start_index + second_index * row_length + first_index;
					geometry.indices.extend_from_slice(&[corner, corner + 1, corner + row_length + 1, corner, corner + row_length + 1, corner + row_length]);
				}
			}
		}
	}
	geometry
}

//==============================================================================
// 원기둥. (위와 아래 반지름이 다르면 깔때기, 위가 0 이면 원뿔, 위아래 뚜껑 포함)
//==============================================================================
pub fn create_cylinder_geometry(radius_top: f32, radius_bottom: f32, height: f32, radial_segments: u32) -> ToonGeometry {
	let mut geometry = ToonGeometry::default();
	let half_height = height * 0.5;
	let slope = (radius_bottom - radius_top) / height;
	let normal_length = (1.0 + slope * slope).sqrt();
	for segment_index in 0..=radial_segments {
		let angle = segment_index as f32 / radial_segments as f32 * PI * 2.0;
		let (sine, cosine) = angle.sin_cos();
		let normal = [sine / normal_length, slope / normal_length, cosine / normal_length];
		geometry.push_point([radius_top * sine, half_height, radius_top * cosine], normal);
		geometry.push_point([radius_bottom * sine, -half_height, radius_bottom * cosine], normal);
	}
	for segment_index in 0..radial_segments {
		let top = segment_index * 2;
		geometry.indices.extend_from_slice(&[top, top + 1, top + 3, top, top + 3, top + 2]);
	}
	for (radius, sign) in [(radius_top, 1.0f32), (radius_bottom, -1.0f32)] {
		if radius <= 0.0 {
			continue;
		}
		let center_index = geometry.get_point_count();
		geometry.push_point([0.0, half_height * sign, 0.0], [0.0, sign, 0.0]);
		for segment_index in 0..=radial_segments {
			let angle = segment_index as f32 / radial_segments as f32 * PI * 2.0;
			geometry.push_point([radius * angle.sin(), half_height * sign, radius * angle.cos()], [0.0, sign, 0.0]);
		}
		for segment_index in 0..radial_segments {
			geometry.indices.extend_from_slice(&[center_index, center_index + 1 + segment_index, center_index + 2 + segment_index]);
		}
	}
	geometry
}

//==============================================================================
// 구. (위에서부터 theta_length 까지, π/2 면 위쪽 반구)
//==============================================================================
pub fn create_sphere_geometry(radius: f32, width_segments: u32, height_segments: u32, theta_length: f32) -> ToonGeometry {
	let mut geometry = ToonGeometry::default();
	for height_index in 0..=height_segments {
		let theta = height_index as f32 / height_segments as f32 * theta_length;
		for width_index in 0..=width_segments {
			let phi = width_index as f32 / width_segments as f32 * PI * 2.0;
			let normal = [theta.sin() * phi.sin(), theta.cos(), theta.sin() * phi.cos()];
			geometry.push_point([normal[0] * radius, normal[1] * radius, normal[2] * radius], normal);
			geometry.push_uv(width_index as f32 / width_segments as f32, height_index as f32 / height_segments as f32);
		}
	}
	let row_length = width_segments + 1;
	for height_index in 0..height_segments {
		for width_index in 0..width_segments {
			let corner = height_index * row_length + width_index;
			geometry.indices.extend_from_slice(&[corner, corner + row_length, corner + row_length + 1, corner, corner + row_length + 1, corner + 1]);
		}
	}
	geometry
}

//==============================================================================
// 돌려 깎기. (옆모습의 점 [x, y] 들을 Y 축으로 돌림, 노멀은 옆모습의 기울기로)
//==============================================================================
pub fn create_lathe_geometry(points: &[[f32; 2]], segments: u32) -> ToonGeometry {
	let mut geometry = ToonGeometry::default();
	let point_count = points.len();
	let profile_normals: Vec<[f32; 2]> = (0..point_count).map(|point_index| {
		let previous = points[point_index.saturating_sub(1)];
		let next = points[(point_index + 1).min(point_count - 1)];
		let tangent_x = next[0] - previous[0];
		let tangent_y = next[1] - previous[1];
		let length = (tangent_x * tangent_x + tangent_y * tangent_y).sqrt();
		if length <= 0.0 { [1.0, 0.0] } else { [tangent_y / length, -tangent_x / length] }
	}).collect();
	for segment_index in 0..=segments {
		let phi = segment_index as f32 / segments as f32 * PI * 2.0;
		let (sine, cosine) = phi.sin_cos();
		for point_index in 0..point_count {
			let point = points[point_index];
			let profile_normal = profile_normals[point_index];
			geometry.push_point([point[0] * sine, point[1], point[0] * cosine], [profile_normal[0] * sine, profile_normal[1], profile_normal[0] * cosine]);
		}
	}
	let row_length = point_count as u32;
	for segment_index in 0..segments {
		for point_index in 0..(row_length - 1) {
			let corner = segment_index * row_length + point_index;
			geometry.indices.extend_from_slice(&[corner, corner + row_length, corner + row_length + 1, corner, corner + row_length + 1, corner + 1]);
		}
	}
	geometry
}

//==============================================================================
// 캡슐. (가운데 길이 length 의 원기둥 위아래에 반구, 전체 높이는 length + 반지름 둘)
//==============================================================================
pub fn create_capsule_geometry(radius: f32, length: f32, cap_segments: u32, radial_segments: u32) -> ToonGeometry {
	let mut points = Vec::new();
	let half_length = length * 0.5;
	for segment_index in 0..=cap_segments {
		let angle = -PI * 0.5 + segment_index as f32 / cap_segments as f32 * PI * 0.5;
		points.push([angle.cos() * radius, -half_length + angle.sin() * radius]);
	}
	for segment_index in 0..=cap_segments {
		let angle = segment_index as f32 / cap_segments as f32 * PI * 0.5;
		points.push([angle.cos() * radius, half_length + angle.sin() * radius]);
	}
	create_lathe_geometry(&points, radial_segments)
}

//==============================================================================
// 고리. (XY 평면에 누운 고리, arc 만큼만)
//==============================================================================
pub fn create_torus_geometry(radius: f32, tube: f32, radial_segments: u32, tubular_segments: u32, arc: f32) -> ToonGeometry {
	let mut geometry = ToonGeometry::default();
	for radial_index in 0..=radial_segments {
		let tube_angle = radial_index as f32 / radial_segments as f32 * PI * 2.0;
		for tubular_index in 0..=tubular_segments {
			let ring_angle = tubular_index as f32 / tubular_segments as f32 * arc;
			let center_x = radius * ring_angle.cos();
			let center_y = radius * ring_angle.sin();
			let normal = [tube_angle.cos() * ring_angle.cos(), tube_angle.cos() * ring_angle.sin(), tube_angle.sin()];
			geometry.push_point([center_x + normal[0] * tube, center_y + normal[1] * tube, normal[2] * tube], normal);
		}
	}
	let row_length = tubular_segments + 1;
	for radial_index in 0..radial_segments {
		for tubular_index in 0..tubular_segments {
			let corner = radial_index * row_length + tubular_index;
			geometry.indices.extend_from_slice(&[corner, corner + row_length, corner + row_length + 1, corner, corner + row_length + 1, corner + 1]);
		}
	}
	geometry
}

//==============================================================================
// 팔면체. (면마다 꼭짓점을 따로 두어 각진 결정)
//==============================================================================
pub fn create_octahedron_geometry(radius: f32) -> ToonGeometry {
	let mut geometry = ToonGeometry::default();
	let normal_length = 3.0f32.sqrt();
	for sign_x in [-1.0f32, 1.0] {
		for sign_y in [-1.0f32, 1.0] {
			for sign_z in [-1.0f32, 1.0] {
				let corners = [[radius * sign_x, 0.0, 0.0], [0.0, radius * sign_y, 0.0], [0.0, 0.0, radius * sign_z]];
				let start_index = geometry.get_point_count();
				for corner in corners {
					geometry.push_point(corner, [sign_x / normal_length, sign_y / normal_length, sign_z / normal_length]);
				}
				geometry.indices.extend_from_slice(&[start_index, start_index + 1, start_index + 2]);
			}
		}
	}
	geometry
}

//==============================================================================
// 각진 돌덩이. (이십면체의 꼭짓점 열둘을 저마다 늘이고 줄여, 면마다 꼭짓점을 따로 둔 깎은 돌, 원본 createFacetedChunkGeometry)
// - vertex_scales 는 꼭짓점 열둘의 반지름 배다. 모자라면 처음부터 되풀이하고, 비면 1 이다. axis_scale 은 [x, y, z] 배.
//==============================================================================
pub fn create_faceted_chunk_geometry(radius: f32, vertex_scales: &[f32], axis_scale: [f32; 3]) -> ToonGeometry {
	let mut geometry = ToonGeometry::default();
	let golden = (1.0 + 5.0f32.sqrt()) / 2.0;
	let base_corners: [[f32; 3]; 12] = [[-1.0, golden, 0.0], [1.0, golden, 0.0], [-1.0, -golden, 0.0], [1.0, -golden, 0.0], [0.0, -1.0, golden], [0.0, 1.0, golden], [0.0, -1.0, -golden], [0.0, 1.0, -golden], [golden, 0.0, -1.0], [golden, 0.0, 1.0], [-golden, 0.0, -1.0], [-golden, 0.0, 1.0]];
	let faces: [[usize; 3]; 20] = [[0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8], [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1]];
	let mut corners = [[0.0f32; 3]; 12];
	for (corner_index, corner) in base_corners.iter().enumerate() {
		let length = (corner[0] * corner[0] + corner[1] * corner[1] + corner[2] * corner[2]).sqrt();
		let vertex_scale = if vertex_scales.is_empty() { 1.0 } else { vertex_scales[corner_index % vertex_scales.len()] };
		let scale = radius * vertex_scale / length;
		corners[corner_index] = [corner[0] * scale * axis_scale[0], corner[1] * scale * axis_scale[1], corner[2] * scale * axis_scale[2]];
	}
	for face in faces {
		let first = corners[face[0]];
		let mut second = corners[face[1]];
		let mut third = corners[face[2]];
		let edge_a = [second[0] - first[0], second[1] - first[1], second[2] - first[2]];
		let edge_b = [third[0] - first[0], third[1] - first[1], third[2] - first[2]];
		let mut normal = [edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1], edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2], edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0]];
		let centroid = [first[0] + second[0] + third[0], first[1] + second[1] + third[1], first[2] + second[2] + third[2]];
		// 바깥을 보게 차례를 맞춘다.
		if normal[0] * centroid[0] + normal[1] * centroid[1] + normal[2] * centroid[2] < 0.0 {
			std::mem::swap(&mut second, &mut third);
			normal = [-normal[0], -normal[1], -normal[2]];
		}
		let normal_length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt().max(0.000001);
		let unit_normal = [normal[0] / normal_length, normal[1] / normal_length, normal[2] / normal_length];
		let start_index = geometry.get_point_count();
		for corner in [first, second, third] {
			geometry.push_point(corner, unit_normal);
		}
		geometry.indices.extend_from_slice(&[start_index, start_index + 1, start_index + 2]);
	}
	geometry
}

//==============================================================================
// 노멀에 곱할 3x3. (행렬 왼쪽 위 3x3 의 역의 전치, 행 차례)
//==============================================================================
pub fn read_normal_matrix(elements: &[f32; 16]) -> [f32; 9] {
	let m00 = elements[0];
	let m10 = elements[1];
	let m20 = elements[2];
	let m01 = elements[4];
	let m11 = elements[5];
	let m21 = elements[6];
	let m02 = elements[8];
	let m12 = elements[9];
	let m22 = elements[10];
	let cofactor00 = m11 * m22 - m12 * m21;
	let cofactor01 = m12 * m20 - m10 * m22;
	let cofactor02 = m10 * m21 - m11 * m20;
	let determinant = m00 * cofactor00 + m01 * cofactor01 + m02 * cofactor02;
	if determinant.abs() < 1e-20 {
		return [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
	}
	let inverse_determinant = 1.0 / determinant;
	// 역행렬의 전치는 여인수 행렬을 행렬식으로 나눈 것이다.
	[
		cofactor00 * inverse_determinant,
		cofactor01 * inverse_determinant,
		cofactor02 * inverse_determinant,
		(m02 * m21 - m01 * m22) * inverse_determinant,
		(m00 * m22 - m02 * m20) * inverse_determinant,
		(m01 * m20 - m00 * m21) * inverse_determinant,
		(m01 * m12 - m02 * m11) * inverse_determinant,
		(m02 * m10 - m00 * m12) * inverse_determinant,
		(m00 * m11 - m01 * m10) * inverse_determinant,
	]
}

//==============================================================================
// 자리와 노멀을 행렬로 옮기기. (자리 배열, 단위 노멀 배열)
//==============================================================================
fn transform_points(positions: &[f32], normals: &[f32], matrix: &Matrix4) -> (Vec<f32>, Vec<f32>) {
	let normal_matrix = read_normal_matrix(matrix.get_elements());
	let mut world_positions = Vec::with_capacity(positions.len());
	let mut world_normals = Vec::with_capacity(positions.len());
	for point_index in (0..positions.len()).step_by(3) {
		let transformed_point = matrix.transform_point(positions[point_index], positions[point_index + 1], positions[point_index + 2]);
		let normal_x = normal_matrix[0] * normals[point_index] + normal_matrix[1] * normals[point_index + 1] + normal_matrix[2] * normals[point_index + 2];
		let normal_y = normal_matrix[3] * normals[point_index] + normal_matrix[4] * normals[point_index + 1] + normal_matrix[5] * normals[point_index + 2];
		let normal_z = normal_matrix[6] * normals[point_index] + normal_matrix[7] * normals[point_index + 1] + normal_matrix[8] * normals[point_index + 2];
		let normal_length = (normal_x * normal_x + normal_y * normal_y + normal_z * normal_z).sqrt();
		world_positions.extend_from_slice(&[transformed_point.x, transformed_point.y, transformed_point.z]);
		if normal_length > 0.0 {
			world_normals.extend_from_slice(&[normal_x / normal_length, normal_y / normal_length, normal_z / normal_length]);
		}
		else {
			world_normals.extend_from_slice(&[0.0, 1.0, 0.0]);
		}
	}
	(world_positions, world_normals)
}

//==============================================================================
// 세 점의 면 노멀. (넓이의 두 배 길이)
//==============================================================================
fn read_face_normal(first: [f32; 3], second: [f32; 3], third: [f32; 3]) -> [f32; 3] {
	let first_edge = [second[0] - first[0], second[1] - first[1], second[2] - first[2]];
	let second_edge = [third[0] - first[0], third[1] - first[1], third[2] - first[2]];
	[first_edge[1] * second_edge[2] - first_edge[2] * second_edge[1], first_edge[2] * second_edge[0] - first_edge[0] * second_edge[2], first_edge[0] * second_edge[1] - first_edge[1] * second_edge[0]]
}

//==============================================================================
// 배열에서 점 하나의 세 수 읽기.
//==============================================================================
fn read_point(values: &[f32], point_index: u32) -> [f32; 3] {
	let offset = point_index as usize * 3;
	[values[offset], values[offset + 1], values[offset + 2]]
}

//==============================================================================
// 세 수 벡터의 내적.
//==============================================================================
fn dot(first: [f32; 3], second: [f32; 3]) -> f32 {
	first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
}

//==============================================================================
// 자리를 1e-4 로 반올림한 열쇠. (같은 자리의 점을 함께 민다)
//==============================================================================
fn read_point_key(point: [f32; 3]) -> (i64, i64, i64) {
	((point[0] * 10000.0).round() as i64, (point[1] * 10000.0).round() as i64, (point[2] * 10000.0).round() as i64)
}

//==============================================================================
// 노멀을 1e-3 으로 반올림한 열쇠. (같은 자리에 같은 법선을 두 번 더하지 않는다)
//==============================================================================
fn read_normal_key(normal: [f32; 3]) -> (i64, i64, i64) {
	((normal[0] * 1000.0).round() as i64, (normal[1] * 1000.0).round() as i64, (normal[2] * 1000.0).round() as i64)
}

//==============================================================================
// 모형 한 덩어리를 짓는 것.
//==============================================================================
#[derive(Default)]
pub struct MeshBuilder {
	// 꼭짓점의 수. (VERTEX_FLOAT_COUNT 씩)
	vertices: Vec<f32>,
	indices: Vec<u32>,
	// 바깥선 껍데기의 자리. (3 씩)
	outline_positions: Vec<f32>,
	// 바깥선 껍데기의 점마다 밀기 전 몸의 자리. (3 씩, 받은 껍데기는 그 자리 그대로)
	outline_base_positions: Vec<f32>,
	outline_indices: Vec<u32>,
	// 꼭짓점마다 뼈대 번호 넷과 무게 넷. (뼈대가 있는 모형만, SKIN_FLOAT_COUNT 씩)
	skin_values: Vec<f32>,
	outline_skin_values: Vec<f32>,
	// 꼭짓점마다 텍스처 좌표 둘. (UV 를 준 꼭짓점이 있을 때만, 없는 꼭짓점은 0)
	uv_values: Vec<f32>,
	/// 꼭짓점마다 2 번 텍스처 좌표 둘. (같은 꼴)
	uv1_values: Vec<f32>,
	// 재질 범위. (add_material_range 로 담은 차례)
	material_ranges: Vec<ToonMaterialRange>,
}

impl MeshBuilder {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> MeshBuilder {
		MeshBuilder::default()
	}

	//==============================================================================
	// 비었는지.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.indices.is_empty()
	}

	//==============================================================================
	// 지금까지 담은 몸의 꼭짓점 번호 수. (한 부위의 몸이 든 자리를 따로 그릴 때 그 앞뒤로 읽는다)
	//==============================================================================
	pub fn get_index_count(&self) -> usize {
		self.indices.len()
	}

	//==============================================================================
	// 지금까지 담은 바깥선 껍데기의 꼭짓점 번호 수.
	//==============================================================================
	pub fn get_outline_index_count(&self) -> usize {
		self.outline_indices.len()
	}

	//==============================================================================
	// 지금까지 담은 몸의 꼭짓점 수. (다음에 담을 꼭짓점의 번호)
	//==============================================================================
	fn get_vertex_count(&self) -> u32 {
		(self.vertices.len() / VERTEX_FLOAT_COUNT) as u32
	}

	//==============================================================================
	// 지금까지 담은 바깥선 껍데기의 점 수. (다음에 담을 점의 번호)
	//==============================================================================
	fn get_outline_point_count(&self) -> u32 {
		(self.outline_positions.len() / 3) as u32
	}

	//==============================================================================
	// 부품 하나 더하기. (도형에 행렬을 곱해 담고, 바깥선이면 점마다 민 껍데기도 담는다)
	//==============================================================================
	pub fn add_part(&mut self, geometry: &ToonGeometry, matrix: &Matrix4, color_text: &str, options: ToonPartOptions) {
		let color = read_color_values(color_text);
		let unlit = if options.is_unlit { 1.0 } else { 0.0 };
		let (world_positions, world_normals) = transform_points(&geometry.positions, &geometry.normals, matrix);
		let start_index = self.get_vertex_count();
		for point_index in (0..world_positions.len()).step_by(3) {
			self.vertices.extend_from_slice(&world_positions[point_index..point_index + 3]);
			self.vertices.extend_from_slice(&world_normals[point_index..point_index + 3]);
			self.vertices.extend_from_slice(&[color[0], color[1], color[2], options.glow, unlit, 0.0, 0.0, 0.0, 0.0]);
		}
		let point_count = world_positions.len() / 3;
		self.push_uv_values(if geometry.uvs.len() >= point_count * 2 { Some(&geometry.uvs) } else { None }, point_count);
		self.push_uv1_values(if geometry.uvs1.len() >= point_count * 2 { Some(&geometry.uvs1) } else { None }, point_count);
		let flips = MeshBuilder::read_triangle_flips(&geometry.indices, &world_positions, &world_normals);
		MeshBuilder::push_triangles(&mut self.indices, &geometry.indices, Some(&flips), start_index);
		if !options.has_outline {
			return;
		}
		self.push_outline_shell(&world_positions, &world_normals, &geometry.indices, Some(&flips), options.outline_width);
	}

	//==============================================================================
	// 네모 면 하나 더하기. (블록 지형과 바닥 같은 면, 네 점은 둘레 차례, 바깥선 없음)
	//==============================================================================
	pub fn add_quad(&mut self, corners: [[f32; 3]; 4], normal: [f32; 3], color: [f32; 3], pattern: f32, pattern_color: Option<[f32; 3]>, is_unlit: bool) {
		let start_index = self.get_vertex_count();
		let pattern_color = pattern_color.unwrap_or([0.0; 3]);
		let unlit = if is_unlit { 1.0 } else { 0.0 };
		for corner in corners {
			self.vertices.extend_from_slice(&[corner[0], corner[1], corner[2], normal[0], normal[1], normal[2], color[0], color[1], color[2], 0.0, unlit, pattern, pattern_color[0], pattern_color[1], pattern_color[2]]);
		}
		self.push_uv_values(None, 4);
		self.push_uv1_values(None, 4);
		// 첫 삼각형의 면 노멀이 주어진 노멀과 반대면 감기를 뒤집는다.
		let is_flipped = dot(read_face_normal(corners[0], corners[1], corners[2]), normal) < 0.0;
		MeshBuilder::push_quad_indices(&mut self.indices, start_index, is_flipped);
	}

	//==============================================================================
	// 네모 면 하나의 삼각형 둘 담기. (뒤집으면 감기를 거꾸로)
	//==============================================================================
	fn push_quad_indices(target: &mut Vec<u32>, start_index: u32, is_flipped: bool) {
		if is_flipped {
			target.extend_from_slice(&[start_index, start_index + 2, start_index + 1, start_index, start_index + 3, start_index + 2]);
		}
		else {
			target.extend_from_slice(&[start_index, start_index + 1, start_index + 2, start_index, start_index + 2, start_index + 3]);
		}
	}

	//==============================================================================
	// 네모 면들의 바깥선 껍데기 담기. (면으로 지은 지형 덩어리처럼)
	// - 같은 자리의 꼭짓점마다 서로 다른 면 법선을 한 번씩 더해 정규화하고, 그쪽으로 굵기만큼 민다.
	//==============================================================================
	pub fn add_quad_shell(&mut self, quads: &[ToonQuad], outline_width: f32) {
		let mut point_normals: HashMap<(i64, i64, i64), (Vec<(i64, i64, i64)>, [f32; 3])> = HashMap::new();
		for quad in quads {
			let normal_key = read_normal_key(quad.normal);
			for corner in quad.corners {
				let entry = point_normals.entry(read_point_key(corner)).or_insert_with(|| (Vec::new(), [0.0; 3]));
				if entry.0.contains(&normal_key) {
					continue;
				}
				entry.0.push(normal_key);
				entry.1[0] += quad.normal[0];
				entry.1[1] += quad.normal[1];
				entry.1[2] += quad.normal[2];
			}
		}
		for quad in quads {
			let start_index = self.get_outline_point_count();
			for corner in quad.corners {
				let sum = point_normals.get(&read_point_key(corner)).map(|entry| entry.1).unwrap_or([0.0; 3]);
				let sum_length = dot(sum, sum).sqrt();
				let scale = if sum_length > 0.0 { outline_width / sum_length } else { 0.0 };
				self.outline_positions.extend_from_slice(&[corner[0] + sum[0] * scale, corner[1] + sum[1] * scale, corner[2] + sum[2] * scale]);
				self.outline_base_positions.extend_from_slice(&corner);
			}
			let is_flipped = dot(read_face_normal(quad.corners[0], quad.corners[1], quad.corners[2]), quad.normal) < 0.0;
			MeshBuilder::push_quad_indices(&mut self.outline_indices, start_index, is_flipped);
		}
	}

	//==============================================================================
	// 받은 모형의 삼각형 담기. (GLB 의 프리미티브 하나, 감기는 그대로)
	//==============================================================================
	pub fn add_triangles(&mut self, positions: &[f32], normals: &[f32], indices: &[u32], matrix: &Matrix4, color: [f32; 3], glow: f32, is_unlit: bool, joints: Option<&[f32]>, weights: Option<&[f32]>) {
		self.add_triangles_with_uvs(positions, normals, indices, matrix, color, glow, is_unlit, joints, weights, None, None);
	}

	//==============================================================================
	// 받은 모형의 삼각형을 텍스처 좌표와 함께 담기. (uvs, uvs1 은 꼭짓점마다 둘, 원본 addTriangles 의 마지막 두 선택 인자)
	//==============================================================================
	pub fn add_triangles_with_uvs(&mut self, positions: &[f32], normals: &[f32], indices: &[u32], matrix: &Matrix4, color: [f32; 3], glow: f32, is_unlit: bool, joints: Option<&[f32]>, weights: Option<&[f32]>, uvs: Option<&[f32]>, uvs1: Option<&[f32]>) {
		let unlit = if is_unlit { 1.0 } else { 0.0 };
		let start_index = self.get_vertex_count();
		let (world_positions, world_normals) = transform_points(positions, normals, matrix);
		for point_index in (0..world_positions.len()).step_by(3) {
			self.vertices.extend_from_slice(&world_positions[point_index..point_index + 3]);
			self.vertices.extend_from_slice(&world_normals[point_index..point_index + 3]);
			self.vertices.extend_from_slice(&[color[0], color[1], color[2], glow, unlit, 0.0, 0.0, 0.0, 0.0]);
		}
		self.push_uv_values(uvs, positions.len() / 3);
		self.push_uv1_values(uvs1, positions.len() / 3);
		self.indices.extend(indices.iter().map(|index| start_index + index));
		MeshBuilder::push_skin_values(&mut self.skin_values, joints, weights, positions.len() / 3);
	}

	//==============================================================================
	// 방금 담은 꼭짓점들의 텍스처 좌표 담기. (uvs 가 None 이면 UV 가 이미 있을 때만 0 으로 채운다, 앞 꼭짓점의 빈자리도 0 으로)
	//==============================================================================
	fn push_uv_values(&mut self, uvs: Option<&[f32]>, vertex_count: usize) {
		let start_vertex = self.vertices.len() / VERTEX_FLOAT_COUNT - vertex_count;
		MeshBuilder::push_uv_target(&mut self.uv_values, uvs, vertex_count, start_vertex);
	}

	//==============================================================================
	/// 방금 담은 꼭짓점들의 2 번 텍스처 좌표 담기. (push_uv_values 와 같은 꼴)
	//==============================================================================
	fn push_uv1_values(&mut self, uvs1: Option<&[f32]>, vertex_count: usize) {
		let start_vertex = self.vertices.len() / VERTEX_FLOAT_COUNT - vertex_count;
		MeshBuilder::push_uv_target(&mut self.uv1_values, uvs1, vertex_count, start_vertex);
	}

	//==============================================================================
	/// 텍스처 좌표 칸에 담기. (값이 None 이고 칸이 비면 아무것도 하지 않는다, 앞 꼭짓점의 빈자리는 0)
	//==============================================================================
	fn push_uv_target(target: &mut Vec<f32>, uvs: Option<&[f32]>, vertex_count: usize, start_vertex: usize) {
		if uvs.is_none() && target.is_empty() {
			return;
		}
		target.resize(target.len().max(start_vertex * 2), 0.0);
		for value_index in 0..vertex_count * 2 {
			let value = uvs.and_then(|uvs| uvs.get(value_index)).copied().unwrap_or(0.0);
			target.push(value);
		}
	}

	//==============================================================================
	// 재질 범위 담기. (몸의 꼭짓점 번호 자리, GLB 읽기가 프리미티브마다 담는다)
	//==============================================================================
	pub fn add_material_range(&mut self, material_range: ToonMaterialRange) {
		self.material_ranges.push(material_range);
	}

	//==============================================================================
	// 받은 모형의 바깥선 껍데기 담기. (이미 부풀린 것, 감기는 그대로)
	//==============================================================================
	pub fn add_outline_triangles(&mut self, positions: &[f32], indices: &[u32], matrix: &Matrix4, joints: Option<&[f32]>, weights: Option<&[f32]>) {
		let start_index = self.get_outline_point_count();
		for point_index in (0..positions.len()).step_by(3) {
			let transformed_point = matrix.transform_point(positions[point_index], positions[point_index + 1], positions[point_index + 2]);
			self.outline_positions.extend_from_slice(&[transformed_point.x, transformed_point.y, transformed_point.z]);
			self.outline_base_positions.extend_from_slice(&[transformed_point.x, transformed_point.y, transformed_point.z]);
		}
		self.outline_indices.extend(indices.iter().map(|index| start_index + index));
		MeshBuilder::push_skin_values(&mut self.outline_skin_values, joints, weights, positions.len() / 3);
	}

	//==============================================================================
	// 받은 모형의 몸으로 바깥선 껍데기 짓기. (add_part 의 바깥선과 같은 셈, 감기는 그대로, 뼈대 번호와 무게는 몸의 것 그대로)
	// - 여러 프리미티브를 이어 붙여 한 번에 넘기면 프리미티브 사이의 같은 자리 점도 함께 민다.
	//==============================================================================
	pub fn add_triangle_shell(&mut self, positions: &[f32], normals: &[f32], indices: &[u32], matrix: &Matrix4, outline_width: f32, joints: Option<&[f32]>, weights: Option<&[f32]>) {
		let (world_positions, world_normals) = transform_points(positions, normals, matrix);
		self.push_outline_shell(&world_positions, &world_normals, indices, None, outline_width);
		MeshBuilder::push_skin_values(&mut self.outline_skin_values, joints, weights, positions.len() / 3);
	}

	//==============================================================================
	// 바깥선 껍데기의 점과 삼각형 담기.
	//==============================================================================
	fn push_outline_shell(&mut self, world_positions: &[f32], world_normals: &[f32], indices: &[u32], flips: Option<&[bool]>, outline_width: f32) {
		let offsets = MeshBuilder::read_outline_offsets(world_positions, world_normals, outline_width);
		let start_index = self.get_outline_point_count();
		for point_index in (0..world_positions.len()).step_by(3) {
			self.outline_positions.extend_from_slice(&[world_positions[point_index] + offsets[point_index], world_positions[point_index + 1] + offsets[point_index + 1], world_positions[point_index + 2] + offsets[point_index + 2]]);
			self.outline_base_positions.extend_from_slice(&world_positions[point_index..point_index + 3]);
		}
		MeshBuilder::push_shell_triangles(&mut self.outline_indices, indices, flips, start_index, world_positions, &offsets);
	}

	//==============================================================================
	// 뼈대 번호와 무게 담기. (뼈대가 없으면 담지 않는다)
	//==============================================================================
	fn push_skin_values(target: &mut Vec<f32>, joints: Option<&[f32]>, weights: Option<&[f32]>, vertex_count: usize) {
		let (joints, weights) = match (joints, weights) {
			(Some(joints), Some(weights)) => {
				(joints, weights)
			}
			_ => {
				return;
			}
		};
		for vertex_index in 0..vertex_count {
			let offset = vertex_index * 4;
			target.extend_from_slice(&joints[offset..offset + 4]);
			target.extend_from_slice(&weights[offset..offset + 4]);
		}
	}

	//==============================================================================
	// 다 지은 배열.
	//==============================================================================
	pub fn read_arrays(&self) -> ToonMeshArrays {
		let vertex_count = self.vertices.len() / VERTEX_FLOAT_COUNT;
		let mut uv_data = Vec::new();
		if !self.uv_values.is_empty() {
			uv_data = self.uv_values.clone();
			uv_data.resize(vertex_count * 2, 0.0);
		}
		let mut uv1_data = Vec::new();
		if !self.uv1_values.is_empty() {
			uv1_data = self.uv1_values.clone();
			uv1_data.resize(vertex_count * 2, 0.0);
		}
		ToonMeshArrays {
			vertex_data: self.vertices.clone(),
			index_data: self.indices.clone(),
			outline_vertex_data: self.outline_positions.clone(),
			outline_index_data: self.outline_indices.clone(),
			outline_base_data: self.outline_base_positions.clone(),
			skin_data: self.skin_values.clone(),
			outline_skin_data: self.outline_skin_values.clone(),
			skeleton: None,
			inner_outline_ranges: Vec::new(),
			inner_body_ranges: Vec::new(),
			uv_data,
			uv1_data,
			material_ranges: self.material_ranges.clone(),
		}
	}

	//==============================================================================
	// 삼각형마다 감기를 뒤집을지. (면 노멀이 꼭짓점 노멀의 합과 반대면 뒤집는다)
	//==============================================================================
	fn read_triangle_flips(indices: &[u32], world_positions: &[f32], world_normals: &[f32]) -> Vec<bool> {
		indices.chunks(3).map(|triangle| {
			let face_normal = read_face_normal(read_point(world_positions, triangle[0]), read_point(world_positions, triangle[1]), read_point(world_positions, triangle[2]));
			if dot(face_normal, face_normal) < DEGENERATE_AREA {
				return false;
			}
			let first_normal = read_point(world_normals, triangle[0]);
			let second_normal = read_point(world_normals, triangle[1]);
			let third_normal = read_point(world_normals, triangle[2]);
			let normal_sum = [first_normal[0] + second_normal[0] + third_normal[0], first_normal[1] + second_normal[1] + third_normal[1], first_normal[2] + second_normal[2] + third_normal[2]];
			dot(face_normal, normal_sum) < 0.0
		}).collect()
	}

	//==============================================================================
	// 삼각형 담기. (뒤집을 것은 둘째와 셋째를 바꾼다)
	//==============================================================================
	fn push_triangles(target: &mut Vec<u32>, indices: &[u32], flips: Option<&[bool]>, start_index: u32) {
		for (triangle_index, triangle) in indices.chunks(3).enumerate() {
			let is_flipped = flips.map(|flips| flips[triangle_index]).unwrap_or(false);
			if is_flipped {
				target.extend_from_slice(&[start_index + triangle[0], start_index + triangle[2], start_index + triangle[1]]);
			}
			else {
				target.extend_from_slice(&[start_index + triangle[0], start_index + triangle[1], start_index + triangle[2]]);
			}
		}
	}

	//==============================================================================
	// 바깥선 껍데기의 삼각형 담기. (뒤집을 것은 둘째와 셋째를 바꾸고, 밀고 나서 면이 뒤집힌 삼각형은 뺀다)
	// - 오목한 곳에서 굵기보다 좁게 굽은 면은 밀면 껍데기가 접혀 면이 뒤집힌다. 그런 면은 몸 위에 짙은 줄로 비치므로 뺀다.
	//==============================================================================
	fn push_shell_triangles(target: &mut Vec<u32>, indices: &[u32], flips: Option<&[bool]>, start_index: u32, world_positions: &[f32], offsets: &[f32]) {
		let read_shell_point = |point_index: u32| {
			let point = read_point(world_positions, point_index);
			let offset = read_point(offsets, point_index);
			[point[0] + offset[0], point[1] + offset[1], point[2] + offset[2]]
		};
		for (triangle_index, triangle) in indices.chunks(3).enumerate() {
			let body_normal = read_face_normal(read_point(world_positions, triangle[0]), read_point(world_positions, triangle[1]), read_point(world_positions, triangle[2]));
			let shell_normal = read_face_normal(read_shell_point(triangle[0]), read_shell_point(triangle[1]), read_shell_point(triangle[2]));
			if dot(body_normal, body_normal) >= DEGENERATE_AREA && dot(body_normal, shell_normal) <= 0.0 {
				continue;
			}
			let is_flipped = flips.map(|flips| flips[triangle_index]).unwrap_or(false);
			if is_flipped {
				target.extend_from_slice(&[start_index + triangle[0], start_index + triangle[2], start_index + triangle[1]]);
			}
			else {
				target.extend_from_slice(&[start_index + triangle[0], start_index + triangle[1], start_index + triangle[2]]);
			}
		}
	}

	//==============================================================================
	// 바깥선 껍데기의 점마다 밀어낼 만큼. (같은 자리의 점끼리 함께 민다)
	// - 같은 자리의 점(날카로운 모서리, 이음매, 극)은 서로 다른 법선을 한 번씩 더한 쪽으로 함께 밀어 껍데기가 벌어지지 않게 한다.
	// - 모서리에서는 맞닿은 면마다 굵기만큼 떨어지게 더 민다. (그 쪽과 면 법선의 가장 작은 코사인으로 나눈다, 0.85 밑으로 내리지 않는다)
	//==============================================================================
	pub fn read_outline_offsets(world_positions: &[f32], world_normals: &[f32], outline_width: f32) -> Vec<f32> {
		let point_count = world_positions.len() / 3;
		let mut point_keys = Vec::with_capacity(point_count);
		let mut point_normals: HashMap<(i64, i64, i64), (Vec<(i64, i64, i64)>, Vec<[f32; 3]>)> = HashMap::new();
		for point_index in 0..point_count as u32 {
			let point_key = read_point_key(read_point(world_positions, point_index));
			let normal = read_point(world_normals, point_index);
			let normal_key = read_normal_key(normal);
			point_keys.push(point_key);
			let entry = point_normals.entry(point_key).or_insert_with(|| (Vec::new(), Vec::new()));
			if entry.0.contains(&normal_key) {
				continue;
			}
			entry.0.push(normal_key);
			entry.1.push(normal);
		}
		let mut point_offsets: HashMap<(i64, i64, i64), Option<[f32; 3]>> = HashMap::new();
		for (point_key, (_, normals)) in point_normals.iter() {
			let mut sum = [0.0f32; 3];
			for normal in normals {
				sum[0] += normal[0];
				sum[1] += normal[1];
				sum[2] += normal[2];
			}
			let sum_length = dot(sum, sum).sqrt();
			if sum_length <= 0.0 {
				point_offsets.insert(*point_key, None);
				continue;
			}
			let direction = [sum[0] / sum_length, sum[1] / sum_length, sum[2] / sum_length];
			let minimum_cosine = normals.iter().map(|normal| dot(direction, *normal)).fold(1.0f32, f32::min);
			let distance = outline_width / minimum_cosine.max(MINIMUM_OUTLINE_MITER_COSINE);
			point_offsets.insert(*point_key, Some([direction[0] * distance, direction[1] * distance, direction[2] * distance]));
		}
		let mut offsets = Vec::with_capacity(world_positions.len());
		for point_index in 0..point_count {
			match point_offsets.get(&point_keys[point_index]).copied().flatten() {
				Some(offset) => {
					offsets.extend_from_slice(&offset);
				}
				None => {
					// 맞선 법선이 서로 지워진 점은 제 법선 쪽으로 민다.
					let normal = read_point(world_normals, point_index as u32);
					offsets.extend_from_slice(&[normal[0] * outline_width, normal[1] * outline_width, normal[2] * outline_width]);
				}
			}
		}
		offsets
	}

	//==============================================================================
	// 지어 둔 배열 여럿을 옮겨 한 배열로 잇기. (움직이지 않는 조각을 한 메시로, 돌리거나 키우지 않음, 원본 MeshBuilder.combine)
	// - 조각마다 배열과 옮김. 자리에 옮김을 더하고 번호에 앞 조각의 꼭짓점 수를 더한다. 바깥선 껍데기와 껍데기 기준 자리, 안쪽 범위도 같이 옮겨 잇는다.
	// - 뼈대는 잇지 않는다. 뼈대가 있는 조각도 쉬는 자세 그대로 움직이지 않는 몸이 된다.
	//==============================================================================
	pub fn combine(piece_list: &[ToonMeshPiece]) -> ToonMeshArrays {
		let mut combined = ToonMeshArrays::default();
		let has_uv = piece_list.iter().any(|piece| !piece.arrays.uv_data.is_empty());
		let has_uv1 = piece_list.iter().any(|piece| !piece.arrays.uv1_data.is_empty());
		for piece in piece_list {
			let arrays = piece.arrays;
			let offset = piece.offset;
			let start_vertex = (combined.vertex_data.len() / VERTEX_FLOAT_COUNT) as u32;
			let index_offset = combined.index_data.len();
			let outline_index_offset = combined.outline_index_data.len();
			let start_outline_vertex = (combined.outline_vertex_data.len() / 3) as u32;
			for vertex in arrays.vertex_data.chunks(VERTEX_FLOAT_COUNT) {
				let start = combined.vertex_data.len();
				combined.vertex_data.extend_from_slice(vertex);
				for axis in 0..3.min(vertex.len()) {
					combined.vertex_data[start + axis] += offset[axis];
				}
			}
			combined.index_data.extend(arrays.index_data.iter().map(|index| index + start_vertex));
			if has_uv {
				let piece_vertex_count = arrays.vertex_data.len() / VERTEX_FLOAT_COUNT;
				combined.uv_data.resize(start_vertex as usize * 2, 0.0);
				combined.uv_data.extend(arrays.uv_data.iter().take(piece_vertex_count * 2));
				combined.uv_data.resize((start_vertex as usize + piece_vertex_count) * 2, 0.0);
			}
			if has_uv1 {
				let piece_vertex_count = arrays.vertex_data.len() / VERTEX_FLOAT_COUNT;
				combined.uv1_data.resize(start_vertex as usize * 2, 0.0);
				combined.uv1_data.extend(arrays.uv1_data.iter().take(piece_vertex_count * 2));
				combined.uv1_data.resize((start_vertex as usize + piece_vertex_count) * 2, 0.0);
			}
			combined.material_ranges.extend(arrays.material_ranges.iter().map(|range| ToonMaterialRange { index_start: range.index_start + index_offset, ..range.clone() }));
			let outline_base_data = if arrays.outline_base_data.len() == arrays.outline_vertex_data.len() { &arrays.outline_base_data } else { &arrays.outline_vertex_data };
			for (point_index, point) in arrays.outline_vertex_data.chunks(3).enumerate() {
				for axis in 0..point.len() {
					combined.outline_vertex_data.push(point[axis] + offset[axis]);
					combined.outline_base_data.push(outline_base_data[point_index * 3 + axis] + offset[axis]);
				}
			}
			combined.outline_index_data.extend(arrays.outline_index_data.iter().map(|index| index + start_outline_vertex));
			combined.inner_outline_ranges.extend(arrays.inner_outline_ranges.iter().map(|(start, count)| (start + outline_index_offset, *count)));
			combined.inner_body_ranges.extend(arrays.inner_body_ranges.iter().map(|(start, count)| (start + index_offset, *count)));
		}
		combined
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn box_faces_point_outward() {
		let mut mesh_builder = MeshBuilder::new();
		mesh_builder.add_part(&create_box_geometry(1.0, 1.0, 1.0), &Matrix4::default(), "#ff8844", ToonPartOptions::default());
		let arrays = mesh_builder.read_arrays();
		assert_eq!(arrays.vertex_data.len(), 24 * VERTEX_FLOAT_COUNT);
		assert_eq!(arrays.index_data.len(), 36);
		assert_eq!(arrays.outline_index_data.len(), 36);

		// 바깥선 껍데기의 꼭짓점은 몸보다 바깥에 있다. (상자 모서리의 점은 0.5 보다 멀다)
		for point in arrays.outline_vertex_data.chunks(3) {
			assert!(point[0].abs() > 0.5 && point[1].abs() > 0.5 && point[2].abs() > 0.5);
		}
	}

	//==============================================================================
	/// 2 번 UV 가 앞 꼭짓점을 0 으로 채우고 합칠 때 자리를 지키는지.
	//==============================================================================
	#[test]
	fn second_uv_values_pad_and_combine() {
		let mut builder = MeshBuilder::new();
		let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
		let normals = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
		builder.add_triangles(&positions, &normals, &[0, 1, 2], &Matrix4::create_identity(), [1.0; 3], 0.0, false, None, None);
		builder.add_triangles_with_uvs(&positions, &normals, &[0, 1, 2], &Matrix4::create_identity(), [1.0; 3], 0.0, false, None, None, Some(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0]), Some(&[0.5, 0.5, 0.6, 0.5, 0.5, 0.6]));
		let arrays = builder.read_arrays();
		assert_eq!(arrays.uv1_data.len(), 12);
		assert_eq!(&arrays.uv1_data[..6], &[0.0; 6]);
		assert_eq!(arrays.uv1_data[6], 0.5);
		let plain = MeshBuilder::new().read_arrays();
		let combined = MeshBuilder::combine(&[ToonMeshPiece { arrays: &plain, offset: [0.0; 3] }, ToonMeshPiece { arrays: &arrays, offset: [0.0; 3] }]);
		assert_eq!(combined.uv1_data, arrays.uv1_data);
	}

	#[test]
	fn uv_values_pad_and_combine() {
		let mut mesh_builder = MeshBuilder::new();
		mesh_builder.add_quad([[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]], [0.0, 0.0, 1.0], [1.0; 3], 0.0, None, false);
		assert!(mesh_builder.read_arrays().uv_data.is_empty());
		mesh_builder.add_part(&create_box_geometry(1.0, 1.0, 1.0), &Matrix4::default(), "#ffffff", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
		mesh_builder.add_quad([[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]], [0.0, 0.0, 1.0], [1.0; 3], 0.0, None, false);
		let arrays = mesh_builder.read_arrays();
		let vertex_count = arrays.vertex_data.len() / VERTEX_FLOAT_COUNT;
		assert_eq!(arrays.uv_data.len(), vertex_count * 2);
		assert_eq!(&arrays.uv_data[0..8], &[0.0; 8]);
		assert_eq!(&arrays.uv_data[8..10], &[0.0, 1.0]);
		mesh_builder.add_material_range(ToonMaterialRange { name: String::from("plate"), index_start: 6, index_count: 6, ..ToonMaterialRange::default() });
		let ranged = mesh_builder.read_arrays();
		let plain = { let mut builder = MeshBuilder::new(); builder.add_quad([[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]], [0.0, 0.0, 1.0], [1.0; 3], 0.0, None, false); builder.read_arrays() };
		let combined = MeshBuilder::combine(&[ToonMeshPiece { arrays: &plain, offset: [0.0; 3] }, ToonMeshPiece { arrays: &ranged, offset: [0.0; 3] }]);
		assert_eq!(combined.uv_data.len(), (plain.vertex_data.len() + ranged.vertex_data.len()) / VERTEX_FLOAT_COUNT * 2);
		assert_eq!(combined.material_ranges[0].index_start, 6 + plain.index_data.len());
	}

	#[test]
	fn faceted_chunk_and_combine() {
		let mut mesh_builder = MeshBuilder::new();
		mesh_builder.add_part(&create_faceted_chunk_geometry(0.5, &[1.0, 0.9, 1.1], [1.0, 0.8, 1.0]), &Matrix4::default(), "#a08c78", ToonPartOptions::default());
		let arrays = mesh_builder.read_arrays();
		assert_eq!(arrays.index_data.len(), 60);
		let combined = MeshBuilder::combine(&[ToonMeshPiece { arrays: &arrays, offset: [0.0; 3] }, ToonMeshPiece { arrays: &arrays, offset: [2.0, 0.0, 0.0] }]);
		let vertex_count = (arrays.vertex_data.len() / VERTEX_FLOAT_COUNT) as u32;
		assert_eq!(combined.vertex_data.len(), arrays.vertex_data.len() * 2);
		assert_eq!(combined.outline_index_data.len(), arrays.outline_index_data.len() * 2);
		assert_eq!(combined.index_data[arrays.index_data.len()], arrays.index_data[0] + vertex_count);
		assert!((combined.vertex_data[arrays.vertex_data.len()] - (arrays.vertex_data[0] + 2.0)).abs() < 0.0001);
		assert!((combined.outline_base_data[arrays.outline_base_data.len()] - (arrays.outline_base_data[0] + 2.0)).abs() < 0.0001);
		assert!(combined.skin_data.is_empty());
	}
}

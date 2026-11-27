//==============================================================================
//! glTF KHR_mesh_quantization 읽기 시험. (원본 tests/graphics3d/meshquantization.test.mjs 와 같은 모형, 같은 기대값)
//! - 시험 자료 tests/assets/glbquantization/ 는 원본 시험의 GLB 짓기 함수를 node 로 돌려 뽑은 GLB 와, 원본 읽기(readGlbArrays 들, SkinnedModel)가 낸 값(expected.json)이다. (make.mjs 로 다시 짓는다)
//!   - float: 실수 모형. quantized_a: 위치 UNSIGNED_SHORT(정규화 아님, 칸 8 바이트) + 되풀이 잎 노드, 노멀 BYTE 정규화, UV UNSIGNED_SHORT 정규화 + KHR_texture_transform.
//!   - quantized_b: 위치 SHORT 정규화, 노멀 SHORT 정규화(-32768 은 -1 로 자름), UV BYTE(정규화 아님) + KHR_texture_transform.
//!   - skinned_float, skinned_quantized: 본 하나, 위치 UNSIGNED_SHORT 를 inverseBindMatrices 로 되풀이, 무게 UNSIGNED_BYTE 정규화, 돌림 움직임 SHORT 정규화.
//!   - required_*: extensionsRequired 에 읽지 못하는 확장을 더한 것. fallback_buffer: bufferView 가 바이너리 덩어리 밖 buffers[1](meshopt 의 fallback 버퍼)을 가리키는 것.
//! - 두 가지를 본다. 원본과 같은 값을 내는지(32비트 실수 한두 칸까지 받는다, 맥에서는 차이 0), 원본 시험처럼 양자화 칸 크기 안에서 실수 모형과 같은 꼭짓점을 내는지.
//! - 노멀이 없는 프리미티브의 평평한 노멀(glTF 2.0 명세)도 본다. (툰과 PBR 둘 다 꼭짓점을 삼각형마다 푼다, 원본 meshquantization.test.mjs 와 같은 검사)
//==============================================================================
use serde_json::Value;
use vanilla::graphics3d::skinnedmodel::SkinnedModel;
use vanilla::graphics3d::toon::meshbuilder::VERTEX_FLOAT_COUNT;
use vanilla::{read_glb_arrays, read_glb_node_arrays, read_glb_node_infos, write_skin_pose, ToonOutlineOptions};

/// 원본 시험의 실수 모형 꼭짓점.
const POSITIONS: [[f64; 3]; 4] = [[0.1, -0.3, 0.5], [1.2, 0.4, -0.2], [-0.7, 0.9, 0.3], [0.5, 0.5, 1.1]];
const NORMALS: [[f64; 3]; 4] = [[0.0, 0.0, 1.0], [0.6, 0.0, 0.8], [0.0, -1.0, 0.0], [-0.48, 0.6, 0.64]];
const UVS: [[f64; 2]; 4] = [[0.1, 0.2], [0.9, 0.3], [0.4, 0.8], [0.6, 0.55]];
/// 원본 값과 견줄 때 받는 차이. (32비트 실수 한두 칸)
const ORIGINAL_TOLERANCE: f64 = 1e-6;

//==============================================================================
/// 시험 자료 GLB 읽기.
//==============================================================================
fn read_asset(name: &str) -> Vec<u8> {
	std::fs::read(format!("{}/tests/assets/glbquantization/{}", env!("CARGO_MANIFEST_DIR"), name)).unwrap()
}

//==============================================================================
/// 원본 읽기가 낸 값. (expected.json)
//==============================================================================
fn read_expected() -> Value {
	serde_json::from_slice(&read_asset("expected.json")).unwrap()
}

//==============================================================================
/// JSON 수 배열을 f64 목록으로.
//==============================================================================
fn read_numbers(value: &Value) -> Vec<f64> {
	value.as_array().unwrap().iter().map(|number| number.as_f64().unwrap()).collect()
}

//==============================================================================
/// 두 목록이 길이가 같고 칸마다 tolerance 안인지. (다르면 칸과 값을 글로)
//==============================================================================
fn check_near(actual: &[f64], expected: &[f64], tolerance: f64, label: &str) {
	assert_eq!(actual.len(), expected.len(), "{} 길이", label);
	for (value_index, (actual_value, expected_value)) in actual.iter().zip(expected.iter()).enumerate() {
		assert!((actual_value - expected_value).abs() <= tolerance, "{} [{}]: {} 와 {} (차이 {}, 받는 차이 {})", label, value_index, actual_value, expected_value, (actual_value - expected_value).abs(), tolerance);
	}
}

//==============================================================================
/// f32 목록을 f64 로.
//==============================================================================
fn widen(values: &[f32]) -> Vec<f64> {
	values.iter().map(|value| *value as f64).collect()
}

//==============================================================================
/// 꼭짓점 배열의 자리(first_column 0) 또는 노멀(3). (MeshBuilder 꼭짓점 VERTEX_FLOAT_COUNT 칸의 앞 여섯)
//==============================================================================
fn read_vertex_columns(vertex_data: &[f32], first_column: usize) -> Vec<f64> {
	vertex_data.chunks(VERTEX_FLOAT_COUNT).flat_map(|vertex| (0..3).map(move |axis| vertex[first_column + axis] as f64)).collect()
}

//==============================================================================
/// 본 행렬 하나로 꼭짓점 자리 옮기기. (열 차례 4x4)
//==============================================================================
fn skin_points(positions: &[f64], joint_matrix: &[f32]) -> Vec<f64> {
	let matrix = widen(joint_matrix);
	let mut points = Vec::with_capacity(positions.len());
	for point in positions.chunks(3) {
		for row in 0..3 {
			points.push(matrix[row] * point[0] + matrix[4 + row] * point[1] + matrix[8 + row] * point[2] + matrix[12 + row]);
		}
	}
	points
}

//==============================================================================
/// GLB 를 SkinnedModel 로 해석하기. (GL 없이)
//==============================================================================
fn interpret_pbr(bytes: &[u8]) -> Result<(SkinnedModel, Vec<vanilla::graphics3d::skinnedmodel::MeshDescription>), String> {
	let (json, binary_buffer) = SkinnedModel::read_glb_chunks(bytes)?;
	let mut skinned_model = SkinnedModel::create_without_shader_program();
	let (mesh_description_list, _) = skinned_model.interpret_content(&json, &binary_buffer)?;
	Ok((skinned_model, mesh_description_list))
}

//==============================================================================
/// 툰 읽기: 실수 모형과 양자화 두 갈래가 원본과 같은 꼭짓점, UV, 부품 이름, extras, 바깥선, 가운데를 내는지.
//==============================================================================
#[test]
fn toon_reader_matches_original_for_quantized_glb() {
	let expected = read_expected();
	let float_arrays = read_glb_arrays(&read_asset("float.glb"), None, None).unwrap();
	check_near(&read_vertex_columns(&float_arrays.vertex_data, 0), &POSITIONS.concat(), 1e-6, "실수 모형 위치");
	for (name, position_cell_size, uv_cell_size, normal_tolerance) in [("quantized_a", 1.9 / 16383.0, 0.8 / 65535.0, 1.5 / 127.0), ("quantized_b", 0.95 / 32767.0, 0.8 / 127.0, 1.5 / 32767.0)] {
		let bytes = read_asset(&format!("{}.glb", name));
		let model_expected = &expected[name];
		let arrays = read_glb_arrays(&bytes, None, None).unwrap();
		// 원본과 같은 값.
		check_near(&widen(&arrays.vertex_data), &read_numbers(&model_expected["vertexData"]), ORIGINAL_TOLERANCE, &format!("{} 꼭짓점 (원본)", name));
		check_near(&widen(&arrays.uv_data), &read_numbers(&model_expected["uvData"]), ORIGINAL_TOLERANCE, &format!("{} UV (원본)", name));
		assert_eq!(arrays.material_ranges[0].node_name, model_expected["nodeName"].as_str().unwrap(), "{} 재질 범위의 부품 이름", name);
		// 원본 시험과 같은 칸 크기 안.
		check_near(&read_vertex_columns(&arrays.vertex_data, 0), &read_vertex_columns(&float_arrays.vertex_data, 0), position_cell_size, &format!("{} 툰 위치", name));
		check_near(&read_vertex_columns(&arrays.vertex_data, 3), &read_vertex_columns(&float_arrays.vertex_data, 3), normal_tolerance * 2.0, &format!("{} 툰 노멀", name));
		check_near(&widen(&arrays.uv_data), &UVS.concat(), uv_cell_size, &format!("{} 툰 UV (KHR_texture_transform)", name));
		assert_eq!(arrays.material_ranges[0].node_name, "hull", "{} 이름 없는 되풀이 잎 노드는 부모 부품", name);
		let outline_options = ToonOutlineOptions { width: 0.02, added_parts: vec![String::from("hull")], ..ToonOutlineOptions::default() };
		let part_arrays = read_glb_node_arrays(&bytes, Some(&outline_options), None).unwrap();
		let mut part_names: Vec<&String> = part_arrays.keys().collect();
		part_names.sort();
		assert_eq!(part_names, vec!["hull"], "{} 부품 이름", name);
		assert_eq!(part_arrays["hull"].extras["line"].as_f64(), Some(0.06), "{} 부품 extras", name);
		check_near(&widen(&part_arrays["hull"].arrays.vertex_data), &read_numbers(&model_expected["partVertexData"]), ORIGINAL_TOLERANCE, &format!("{} 부품 꼭짓점 (원본)", name));
		check_near(&widen(&part_arrays["hull"].arrays.outline_vertex_data), &read_numbers(&model_expected["partOutlineVertexData"]), ORIGINAL_TOLERANCE, &format!("{} 부품 바깥선 껍데기 (원본)", name));
		let node_infos = read_glb_node_infos(&bytes).unwrap();
		check_near(&node_infos["hull"].center, &read_numbers(&model_expected["center"]), ORIGINAL_TOLERANCE, &format!("{} 부품 가운데 (원본)", name));
		assert_eq!(node_infos.len(), 1, "{} 노드 정보는 이름 있는 부품 하나", name);
	}
}

//==============================================================================
/// 툰 읽기: 뼈대 모형의 정수 위치(inverseBindMatrices 로 되풀이), 정규화 무게, SHORT 정규화 돌림, 새로 지은 바깥선 굵기가 원본과 같은지.
//==============================================================================
#[test]
fn toon_reader_matches_original_for_quantized_skinned_glb() {
	let expected = read_expected();
	let outline_options = ToonOutlineOptions { width: 0.05, added_parts: vec![String::from("skin")], ..ToonOutlineOptions::default() };
	let mut posed = Vec::new();
	for name in ["skinned_float", "skinned_quantized"] {
		let bytes = read_asset(&format!("{}.glb", name));
		let model_expected = &expected[name];
		let arrays = read_glb_arrays(&bytes, None, None).unwrap();
		let skeleton = arrays.skeleton.as_ref().unwrap();
		let mut joint_matrices = [0.0f32; 16];
		write_skin_pose(skeleton, "Turn", 0.5, &mut joint_matrices, None, None, None);
		check_near(&widen(&arrays.vertex_data), &read_numbers(&model_expected["vertexData"]), ORIGINAL_TOLERANCE, &format!("{} 꼭짓점 (원본)", name));
		check_near(&widen(&arrays.skin_data), &read_numbers(&model_expected["skinData"]), ORIGINAL_TOLERANCE, &format!("{} 뼈대 번호와 무게 (원본)", name));
		check_near(&widen(&skeleton.inverse_bind_matrices), &read_numbers(&model_expected["inverseBindMatrices"]), ORIGINAL_TOLERANCE, &format!("{} 거꾸로 묶는 행렬 (원본)", name));
		check_near(&skeleton.animations["Turn"].channels[0].values, &read_numbers(&model_expected["toonRotationValues"]), ORIGINAL_TOLERANCE, &format!("{} 돌림 움직임 값 (원본)", name));
		check_near(&widen(&joint_matrices), &read_numbers(&model_expected["jointMatrices"]), ORIGINAL_TOLERANCE, &format!("{} 본 행렬 (원본)", name));
		let outlined = read_glb_arrays(&bytes, Some(&outline_options), None).unwrap();
		assert!(!outlined.outline_vertex_data.is_empty(), "{} 뼈대 바깥선 껍데기", name);
		check_near(&widen(&outlined.outline_vertex_data), &read_numbers(&model_expected["outlineVertexData"]), ORIGINAL_TOLERANCE, &format!("{} 바깥선 껍데기 (원본)", name));
		posed.push((skin_points(&read_vertex_columns(&arrays.vertex_data, 0), &joint_matrices), skin_points(&widen(&outlined.outline_vertex_data), &joint_matrices), skeleton.animations["Turn"].channels[0].values.clone(), arrays.skin_data.clone()));
	}
	// 원본 시험처럼 실수 모형과 칸 크기 안.
	let position_cell_size = 1.9 / 16383.0;
	check_near(&posed[1].0, &posed[0].0, position_cell_size * 2.0, "뼈대 툰 자세 위치");
	check_near(&posed[1].1, &posed[0].1, 0.05 * 0.05 + position_cell_size * 2.0, "뼈대 바깥선 껍데기 자리 (굵기 0.05)");
	check_near(&posed[1].2, &posed[0].2, 1.0 / 32767.0, "툰 SHORT 정규화 돌림 값");
	check_near(&widen(&posed[1].3[..8]), &widen(&posed[0].3[..8]), 1e-6, "툰 UNSIGNED_BYTE 정규화 무게");
}

//==============================================================================
/// PBR 읽기(SkinnedModel): 위치는 되풀이 변환 전 정수 값, 노멀과 UV 는 정규화와 KHR_texture_transform 을 건 값이 원본과 같은지.
//==============================================================================
#[test]
fn pbr_reader_matches_original_for_quantized_glb() {
	let expected = read_expected();
	for (name, position_cell_size, uv_cell_size, normal_tolerance) in [("float", 1e-6, 1e-6, 1e-6), ("quantized_a", 1.9 / 16383.0, 0.8 / 65535.0, 1.5 / 127.0), ("quantized_b", 0.95 / 32767.0, 0.8 / 127.0, 1.5 / 32767.0)] {
		let bytes = read_asset(&format!("{}.glb", name));
		let model_expected = &expected[name];
		let (_, mesh_description_list) = interpret_pbr(&bytes).unwrap();
		let mesh_description = &mesh_description_list[0];
		let normals = mesh_description.normals.as_ref().unwrap();
		let texture_coordinates = mesh_description.texture_coordinates.as_ref().unwrap();
		check_near(&mesh_description.positions, &read_numbers(&model_expected["pbrPositions"]), ORIGINAL_TOLERANCE, &format!("{} PBR 위치 (원본)", name));
		check_near(normals, &read_numbers(&model_expected["pbrNormals"]), ORIGINAL_TOLERANCE, &format!("{} PBR 노멀 (원본)", name));
		check_near(texture_coordinates, &read_numbers(&model_expected["pbrTextureCoordinates"]), ORIGINAL_TOLERANCE, &format!("{} PBR UV (원본)", name));
		// 원본 시험처럼: 위치는 잎 노드의 되풀이 변환을 곱하면 칸 안, 노멀과 UV 는 그대로 칸 안.
		let (json, _) = SkinnedModel::read_glb_chunks(&bytes).unwrap();
		let leaf_node = &json["nodes"][if name == "float" { 0 } else { 1 }];
		let scale: Vec<f64> = leaf_node["scale"].as_array().map(|values| values.iter().map(|value| value.as_f64().unwrap()).collect()).unwrap_or(vec![1.0; 3]);
		let translation: Vec<f64> = leaf_node["translation"].as_array().map(|values| values.iter().map(|value| value.as_f64().unwrap()).collect()).unwrap_or(vec![0.0; 3]);
		let dequantized_positions: Vec<f64> = mesh_description.positions.iter().enumerate().map(|(value_index, value)| value * scale[value_index % 3] + translation[value_index % 3]).collect();
		check_near(&dequantized_positions, &POSITIONS.concat(), position_cell_size, &format!("{} PBR 위치", name));
		check_near(normals, &NORMALS.concat(), normal_tolerance, &format!("{} PBR 노멀", name));
		check_near(texture_coordinates, &UVS.concat(), uv_cell_size, &format!("{} PBR UV (KHR_texture_transform)", name));
	}
	// 부호 있는 정규화의 -1 자르기. (-32768 / 32767 은 -1 아래라 -1)
	let (_, mesh_description_list) = interpret_pbr(&read_asset("quantized_b.glb")).unwrap();
	assert_eq!(mesh_description_list[0].normals.as_ref().unwrap()[7], -1.0, "SHORT 정규화 -32768 은 -1");
	// 뼈대: SHORT 정규화 돌림 움직임은 원본과 같고 실수 모형과 칸 안.
	let mut rotation_values = Vec::new();
	for name in ["skinned_float", "skinned_quantized"] {
		let (skinned_model, _) = interpret_pbr(&read_asset(&format!("{}.glb", name))).unwrap();
		let values = skinned_model.get_animation_list()[0].channels[0].values.clone();
		check_near(&values, &read_numbers(&expected[name]["pbrRotationValues"]), ORIGINAL_TOLERANCE, &format!("{} PBR 돌림 움직임 값 (원본)", name));
		rotation_values.push(values);
	}
	check_near(&rotation_values[1], &rotation_values[0], 1.0 / 32767.0, "PBR SHORT 정규화 돌림 값");
}

//==============================================================================
/// 필수 확장(meshopt 압축, Draco 압축, 모르는 확장)과 바이너리 덩어리 밖 버퍼는 원본과 같은 글의 Err.
//==============================================================================
#[test]
fn unsupported_extension_and_external_buffer_are_errors() {
	let expected = read_expected();
	for (case_name, file_name) in [("EXT_meshopt_compression", "required_EXT_meshopt_compression.glb"), ("KHR_draco_mesh_compression", "required_KHR_draco_mesh_compression.glb"), ("VENDOR_unknown_thing", "required_VENDOR_unknown_thing.glb"), ("fallback", "fallback_buffer.glb")] {
		let bytes = read_asset(file_name);
		let toon_message = expected["messages"][case_name]["toon"].as_str().unwrap();
		let pbr_message = expected["messages"][case_name]["pbr"].as_str().unwrap();
		assert_eq!(read_glb_arrays(&bytes, None, None).err().as_deref(), Some(toon_message), "{} read_glb_arrays", case_name);
		assert_eq!(read_glb_node_arrays(&bytes, None, None).err().as_deref(), Some(toon_message), "{} read_glb_node_arrays", case_name);
		assert_eq!(read_glb_node_infos(&bytes).err().as_deref(), Some(toon_message), "{} read_glb_node_infos", case_name);
		assert_eq!(interpret_pbr(&bytes).err().as_deref(), Some(pbr_message), "{} SkinnedModel", case_name);
	}
}

//==============================================================================
/// JSON 과 바이너리 덩어리로 GLB 짓기. (JSON 은 4 바이트 맞춤 공백)
//==============================================================================
fn create_glb(json: &Value, binary_buffer: &[u8]) -> Vec<u8> {
	let mut json_bytes = serde_json::to_vec(json).unwrap();
	while json_bytes.len() % 4 != 0 {
		json_bytes.push(b' ');
	}
	let total_length = 12 + 8 + json_bytes.len() + 8 + binary_buffer.len();
	let mut glb = Vec::with_capacity(total_length);
	for value in [0x46546c67u32, 2, total_length as u32, json_bytes.len() as u32, 0x4e4f534a] {
		glb.extend_from_slice(&value.to_le_bytes());
	}
	glb.extend_from_slice(&json_bytes);
	glb.extend_from_slice(&(binary_buffer.len() as u32).to_le_bytes());
	glb.extend_from_slice(&0x004e4942u32.to_le_bytes());
	glb.extend_from_slice(binary_buffer);
	glb
}

//==============================================================================
/// 툰 읽기: 노멀이 없는 프리미티브는 glTF 2.0 명세대로 평평한 노멀(삼각형마다 하나)로 짓는다. (꼭짓점을 삼각형마다 풀고, 새로 짓는 바깥선도 같다)
//==============================================================================
#[test]
fn toon_reader_builds_flat_normals_when_normals_are_missing() {
	let (mut json, binary_buffer) = SkinnedModel::read_glb_chunks(&read_asset("float.glb")).unwrap();
	json["meshes"][0]["primitives"][0]["attributes"].as_object_mut().unwrap().remove("NORMAL");
	let bytes = create_glb(&json, &binary_buffer);
	let arrays = read_glb_arrays(&bytes, None, None).unwrap();
	let indices = [0usize, 1, 2, 0, 2, 3];
	assert_eq!(arrays.vertex_data.len(), indices.len() * VERTEX_FLOAT_COUNT, "꼭짓점을 삼각형마다 따로 푼다");
	let mut expected_positions = Vec::new();
	let mut expected_normals = Vec::new();
	for triangle in indices.chunks(3) {
		let [first, second, third] = [POSITIONS[triangle[0]], POSITIONS[triangle[1]], POSITIONS[triangle[2]]];
		let first_edge = [second[0] - first[0], second[1] - first[1], second[2] - first[2]];
		let second_edge = [third[0] - first[0], third[1] - first[1], third[2] - first[2]];
		let cross = [first_edge[1] * second_edge[2] - first_edge[2] * second_edge[1], first_edge[2] * second_edge[0] - first_edge[0] * second_edge[2], first_edge[0] * second_edge[1] - first_edge[1] * second_edge[0]];
		let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
		for corner in triangle.iter() {
			expected_positions.extend_from_slice(&POSITIONS[*corner]);
			expected_normals.extend_from_slice(&[cross[0] / length, cross[1] / length, cross[2] / length]);
		}
	}
	check_near(&read_vertex_columns(&arrays.vertex_data, 0), &expected_positions, 1e-6, "평평한 노멀 모형의 위치");
	check_near(&read_vertex_columns(&arrays.vertex_data, 3), &expected_normals, 1e-6, "평평한 노멀");
	check_near(&widen(&arrays.uv_data), &indices.iter().flat_map(|corner| UVS[*corner]).collect::<Vec<f64>>(), 1e-6, "텍스처 좌표도 삼각형마다 푼다");
	let outline_options = ToonOutlineOptions { width: 0.02, added_parts: vec![String::from("hull")], ..ToonOutlineOptions::default() };
	assert!(!read_glb_arrays(&bytes, Some(&outline_options), None).unwrap().outline_vertex_data.is_empty(), "노멀이 없어도 몸으로 바깥선 껍데기를 짓는다");
}

//==============================================================================
/// 삼각형마다의 평평한 노멀 기대값. ((p1 - p0) x (p2 - p0) 을 길이 1 로, 위치는 GLB 에 담긴 32 비트 실수, 원본 시험의 createExpectedFlatNormals)
//==============================================================================
fn create_expected_flat_normals(indices: &[usize]) -> Vec<f64> {
	let mut normals = Vec::new();
	for triangle in indices.chunks(3) {
		let [first, second, third] = [0, 1, 2].map(|corner| POSITIONS[triangle[corner]].map(|value| value as f32 as f64));
		let first_edge = [second[0] - first[0], second[1] - first[1], second[2] - first[2]];
		let second_edge = [third[0] - first[0], third[1] - first[1], third[2] - first[2]];
		let cross = [first_edge[1] * second_edge[2] - first_edge[2] * second_edge[1], first_edge[2] * second_edge[0] - first_edge[0] * second_edge[2], first_edge[0] * second_edge[1] - first_edge[1] * second_edge[0]];
		let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
		for _ in 0..3 {
			normals.extend_from_slice(&[cross[0] / length, cross[1] / length, cross[2] / length]);
		}
	}
	normals
}

//==============================================================================
/// json 에서 노멀을 뺀 GLB. (glTF 2.0 명세: 노멀이 없으면 평평한 노멀, 원본 시험의 removeNormals)
//==============================================================================
fn remove_normals(bytes: &[u8]) -> Vec<u8> {
	let (mut json, binary_buffer) = SkinnedModel::read_glb_chunks(bytes).unwrap();
	for mesh in json["meshes"].as_array_mut().unwrap().iter_mut() {
		for primitive in mesh["primitives"].as_array_mut().unwrap().iter_mut() {
			primitive["attributes"].as_object_mut().unwrap().remove("NORMAL");
		}
	}
	create_glb(&json, &binary_buffer)
}

//==============================================================================
/// PBR 읽기(SkinnedModel): 노멀이 없는 프리미티브는 꼭짓점을 삼각형마다 풀어 평평한 노멀을 짓고 번호 없이 그린다. (고치기 전에는 노멀이 없어 업로드 때 부드러운 노멀, 원본 시험의 "노멀 없는 PBR")
//==============================================================================
#[test]
fn pbr_reader_builds_flat_normals_when_normals_are_missing() {
	let indices = [0usize, 1, 2, 0, 2, 3];
	let (_, mesh_description_list) = interpret_pbr(&remove_normals(&read_asset("float.glb"))).unwrap();
	let mesh_description = &mesh_description_list[0];
	assert!(mesh_description.normals.is_some(), "노멀 없는 PBR 노멀");
	assert!(mesh_description.indices.is_none(), "노멀 없는 PBR 은 번호 없이");
	check_near(&mesh_description.positions, &indices.iter().flat_map(|corner| POSITIONS[*corner]).collect::<Vec<f64>>(), 1e-6, "노멀 없는 PBR 위치");
	check_near(mesh_description.normals.as_deref().unwrap_or(&[]), &create_expected_flat_normals(&indices), 1e-6, "노멀 없는 PBR 평평한 노멀");
	check_near(mesh_description.texture_coordinates.as_deref().unwrap_or(&[]), &indices.iter().flat_map(|corner| UVS[*corner]).collect::<Vec<f64>>(), 1e-6, "노멀 없는 PBR 텍스처 좌표");
	assert_eq!(mesh_description.weights.len(), indices.len() * 4, "노멀 없는 PBR 가중치 수");

	// 노멀 없는 뼈대 모형: 조인트와 가중치도 삼각형마다 푼다. (원본 시험의 "노멀 없는 뼈대 PBR")
	let (_, skinned_description_list) = interpret_pbr(&remove_normals(&read_asset("skinned_float.glb"))).unwrap();
	let skinned_description = &skinned_description_list[0];
	check_near(skinned_description.normals.as_deref().unwrap_or(&[]), &create_expected_flat_normals(&indices), 1e-6, "노멀 없는 뼈대 PBR 평평한 노멀");
	assert_eq!(skinned_description.joints, indices.iter().flat_map(|_| [0u16, 0, 0, 0]).collect::<Vec<u16>>(), "노멀 없는 뼈대 PBR 조인트");
	check_near(&skinned_description.weights, &indices.iter().flat_map(|_| [1.0, 0.0, 0.0, 0.0]).collect::<Vec<f64>>(), 1e-6, "노멀 없는 뼈대 PBR 가중치");
}

//==============================================================================
/// 툰 읽기: 바이너리 덩어리 밖을 읽는 접근자는 0 으로 채우지 않고 Err, bufferView 가 없는 접근자는 glTF 명세대로 0 으로 채운다. (원본 SkinnedModel 의 접근자 읽기와 같은 뜻, 원본 툰 readAccessor 에 넣을 검사는 HANDOFF)
//==============================================================================
#[test]
fn toon_reader_rejects_accessor_outside_binary_chunk() {
	let (json, binary_buffer) = SkinnedModel::read_glb_chunks(&read_asset("float.glb")).unwrap();
	let position_accessor = json["meshes"][0]["primitives"][0]["attributes"]["POSITION"].as_u64().unwrap() as usize;
	let mut far_json = json.clone();
	far_json["accessors"][position_accessor]["byteOffset"] = Value::from(100000);
	let far_bytes = create_glb(&far_json, &binary_buffer);
	let expected_message = format!("GLB 접근자 {} 가 바이너리 덩어리 밖을 읽습니다. (바이트 100000 ~ 100048, 바이너리 덩어리 {} 바이트)", position_accessor, binary_buffer.len());
	assert_eq!(read_glb_arrays(&far_bytes, None, None).err().as_deref(), Some(expected_message.as_str()), "버퍼 밖 바이트는 Err");
	let mut tail_json = json.clone();
	tail_json["accessors"][position_accessor]["byteOffset"] = Value::from(binary_buffer.len() - 8);
	assert!(read_glb_arrays(&create_glb(&tail_json, &binary_buffer), None, None).is_err(), "끝이 덩어리를 넘는 접근자도 Err");
	let mut missing_json = json.clone();
	missing_json["accessors"][position_accessor].as_object_mut().unwrap().remove("bufferView");
	let missing_arrays = read_glb_arrays(&create_glb(&missing_json, &binary_buffer), None, None).unwrap();
	assert!(!missing_arrays.vertex_data.is_empty() && read_vertex_columns(&missing_arrays.vertex_data, 0).iter().all(|value| *value == 0.0), "bufferView 없는 접근자는 0 으로 채운다 (bufferView 0 을 읽지 않는다)");
}

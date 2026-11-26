//==============================================================================
//! 2D 뼈대의 엔진 자체 JSON 읽기. (원본 experimental/skeleton2d/skeleton2djsonreader.js, Skeleton2DData::create_from_json 이 부른다)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 꼴은 Skeleton2DData::create_from_json 의 머리 주석에 있다. 꼴이 틀리면 `"[Skeleton2D]"` 로 시작하는 Err 이다. (원본과 같은 글)
//! - 공개 API 는 Skeleton2DData::create_from_json 이다. 이 모듈의 함수는 크레이트 안에서만 쓴다.
//==============================================================================
use crate::base::color::Color;
use crate::experimental::jsmath::js_max;
use crate::core::graphic::BlendMode;
use crate::experimental::skeleton2d::skeleton2danimation::{Skeleton2DAnimation, Skeleton2DCurveType, Skeleton2DEvent, Skeleton2DTimeline, Skeleton2DTimelineType};
use crate::experimental::skeleton2d::skeleton2ddata::{Skeleton2DAttachment, Skeleton2DBoneData, Skeleton2DData, Skeleton2DIkConstraintData, Skeleton2DMeshAttachment, Skeleton2DRegionAttachment, Skeleton2DSkin, Skeleton2DSlotData};
use serde_json::{Map, Value};

/// 읽기 결과. (실패는 오류 글)
type ReadResult<T> = Result<T, String>;

//==============================================================================
/// 오류 글 짓기. (머리를 붙인다)
//==============================================================================
fn create_read_error(message: &str) -> String {
	format!("[Skeleton2D] {}", message)
}

//==============================================================================
/// 숫자 칸 읽기. (없거나 숫자가 아니면 기본값)
//==============================================================================
fn read_number(source: &Map<String, Value>, key: &str, default_value: f64) -> f64 {
	match source.get(key).and_then(|value| value.as_f64()) {
		Some(number) => {
			number
		}
		None => {
			default_value
		}
	}
}

//==============================================================================
/// 글 칸 읽기. (없거나 글이 아니면 None)
//==============================================================================
fn read_string<'a>(source: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
	source.get(key).and_then(|value| value.as_str())
}

//==============================================================================
/// 배열 칸 읽기. (없거나 null 이면 빈 배열, 배열이 아니면 오류)
//==============================================================================
fn read_array<'a>(source: &'a Map<String, Value>, key: &str, place: &str) -> ReadResult<&'a [Value]> {
	match source.get(key) {
		None | Some(Value::Null) => {
			Ok(&[])
		}
		Some(Value::Array(values)) => {
			Ok(values.as_slice())
		}
		Some(_) => {
			Err(create_read_error(&format!("{} 의 {} 가 배열이 아니다.", place, key)))
		}
	}
}

//==============================================================================
/// 숫자 배열 칸 읽기. (없으면 빈 배열, 숫자가 아닌 것이 있으면 오류)
//==============================================================================
fn read_number_array(source: &Map<String, Value>, key: &str, place: &str) -> ReadResult<Vec<f64>> {
	let values = read_array(source, key, place)?;
	let mut numbers = Vec::with_capacity(values.len());
	for value in values {
		match value.as_f64() {
			Some(number) => {
				numbers.push(number);
			}
			None => {
				return Err(create_read_error(&format!("{} 의 {} 에 숫자가 아닌 값이 있다.", place, key)));
			}
		}
	}
	Ok(numbers)
}

//==============================================================================
/// 색 글 읽기. ("#rrggbb" 나 "#rrggbbaa", 0 ~ 1 의 빨강, 초록, 파랑, 알파, 원본처럼 f64)
//==============================================================================
fn read_skeleton2d_color(color_text: &str, place: &str) -> ReadResult<[f64; 4]> {
	let hex_text = color_text.strip_prefix('#').unwrap_or(color_text);
	let is_valid = (hex_text.len() == 6 || hex_text.len() == 8) && hex_text.chars().all(|character| character.is_ascii_hexdigit());
	if !is_valid {
		return Err(create_read_error(&format!("{} 의 색 \"{}\" 을 읽지 못했다.", place, color_text)));
	}
	let read_channel = |start: usize| -> f64 { u8::from_str_radix(&hex_text[start..start + 2], 16).unwrap_or(0) as f64 / 255.0 };
	let alpha = if hex_text.len() == 8 { read_channel(6) } else { 1.0 };
	Ok([read_channel(0), read_channel(2), read_channel(4), alpha])
}

//==============================================================================
/// 키의 곡선 읽기. ((곡선 종류, 베지어 제어점))
//==============================================================================
fn read_curve(key_json: &Map<String, Value>, place: &str) -> ReadResult<(Skeleton2DCurveType, Option<[f64; 4]>)> {
	match key_json.get("curve") {
		None | Some(Value::Null) => {
			Ok((Skeleton2DCurveType::Linear, None))
		}
		Some(Value::String(curve_text)) if curve_text == "linear" => {
			Ok((Skeleton2DCurveType::Linear, None))
		}
		Some(Value::String(curve_text)) if curve_text == "stepped" => {
			Ok((Skeleton2DCurveType::Stepped, None))
		}
		Some(Value::Array(curve_values)) if curve_values.len() == 4 && curve_values.iter().all(|value| value.is_number()) => {
			let read_value = |value_index: usize| -> f64 { curve_values[value_index].as_f64().unwrap_or(0.0) };
			Ok((Skeleton2DCurveType::Bezier, Some([read_value(0), read_value(1), read_value(2), read_value(3)])))
		}
		Some(_) => {
			Err(create_read_error(&format!("{} 의 curve 를 읽지 못했다. (\"linear\", \"stepped\", [x1, y1, x2, y2])", place)))
		}
	}
}

//==============================================================================
/// 키 시각 읽기. (앞 키보다 이르면 오류, 키 객체도 돌려준다)
//==============================================================================
fn read_key_time<'a>(key_json: &'a Value, previous_time: f64, place: &str) -> ReadResult<(&'a Map<String, Value>, f64)> {
	let key_object = match key_json.as_object() {
		Some(key_object) => {
			key_object
		}
		None => {
			return Err(create_read_error(&format!("{} 의 키가 객체가 아니다.", place)));
		}
	};
	let time = read_number(key_object, "time", 0.0);
	if time < previous_time {
		return Err(create_read_error(&format!("{} 의 키 시각 {} 이 앞 키보다 이르다.", place, time)));
	}
	Ok((key_object, time))
}

//==============================================================================
/// 뼈 읽기.
//==============================================================================
fn read_bones(skeleton_data: &mut Skeleton2DData, json: &Map<String, Value>) -> ReadResult<()> {
	for bone_json in read_array(json, "bones", "뼈대")? {
		let bone_object = match bone_json.as_object() {
			Some(bone_object) => {
				bone_object
			}
			None => {
				return Err(create_read_error("뼈가 객체가 아니다."));
			}
		};
		let name = match read_string(bone_object, "name") {
			Some(name) => {
				name
			}
			None => {
				return Err(create_read_error("이름 없는 뼈가 있다."));
			}
		};
		if skeleton_data.find_bone_index(name).is_some() {
			return Err(create_read_error(&format!("뼈 이름 \"{}\" 이 겹친다.", name)));
		}
		let parent_index = match read_string(bone_object, "parent") {
			Some(parent_name) => {
				match skeleton_data.find_bone_index(parent_name) {
					Some(parent_index) => {
						Some(parent_index)
					}
					None => {
						return Err(create_read_error(&format!("뼈 \"{}\" 의 부모 \"{}\" 가 앞에 없다. (부모가 앞에 와야 한다)", name, parent_name)));
					}
				}
			}
			None => {
				None
			}
		};
		let mut bone_data = Skeleton2DBoneData::new(skeleton_data.get_bones().len(), name, parent_index);
		bone_data.set_transform(
			read_number(bone_object, "x", 0.0),
			read_number(bone_object, "y", 0.0),
			read_number(bone_object, "rotation", 0.0),
			read_number(bone_object, "scaleX", 1.0),
			read_number(bone_object, "scaleY", 1.0),
			read_number(bone_object, "shearX", 0.0),
			read_number(bone_object, "shearY", 0.0),
		);
		bone_data.set_length(read_number(bone_object, "length", 0.0));
		skeleton_data.add_bone(bone_data);
	}
	Ok(())
}

//==============================================================================
/// 슬롯 읽기.
//==============================================================================
fn read_slots(skeleton_data: &mut Skeleton2DData, json: &Map<String, Value>) -> ReadResult<()> {
	for slot_json in read_array(json, "slots", "뼈대")? {
		let slot_object = match slot_json.as_object() {
			Some(slot_object) => {
				slot_object
			}
			None => {
				return Err(create_read_error("슬롯이 객체가 아니다."));
			}
		};
		let name = match read_string(slot_object, "name") {
			Some(name) => {
				name
			}
			None => {
				return Err(create_read_error("이름 없는 슬롯이 있다."));
			}
		};
		if skeleton_data.find_slot_index(name).is_some() {
			return Err(create_read_error(&format!("슬롯 이름 \"{}\" 이 겹친다.", name)));
		}
		let bone_name = read_string(slot_object, "bone");
		let bone_index = match bone_name.and_then(|bone_name| skeleton_data.find_bone_index(bone_name)) {
			Some(bone_index) => {
				bone_index
			}
			None => {
				return Err(create_read_error(&format!("슬롯 \"{}\" 의 뼈 \"{}\" 가 없다.", name, bone_name.unwrap_or("null"))));
			}
		};
		let mut slot_data = Skeleton2DSlotData::new(skeleton_data.get_slots().len(), name, bone_index);
		if let Some(color_text) = read_string(slot_object, "color") {
			let color = read_skeleton2d_color(color_text, &format!("슬롯 \"{}\"", name))?;
			slot_data.set_color(Color::create(color[0] as f64, color[1] as f64, color[2] as f64, color[3] as f64));
		}
		slot_data.set_attachment_name(read_string(slot_object, "attachment"));
		let blend_mode = read_string(slot_object, "blendMode").and_then(BlendMode::from_name).unwrap_or(BlendMode::Normal);
		slot_data.set_blend_mode(blend_mode);
		skeleton_data.add_slot(slot_data);
	}
	Ok(())
}

//==============================================================================
/// IK 제약 읽기.
//==============================================================================
fn read_ik_constraints(skeleton_data: &mut Skeleton2DData, json: &Map<String, Value>) -> ReadResult<()> {
	for ik_json in read_array(json, "ik", "뼈대")? {
		let ik_object = match ik_json.as_object() {
			Some(ik_object) => {
				ik_object
			}
			None => {
				return Err(create_read_error("IK 가 객체가 아니다."));
			}
		};
		let name = read_string(ik_object, "name").unwrap_or("");
		let place = format!("IK \"{}\"", name);
		let bone_names = read_array(ik_object, "bones", &place)?;
		if bone_names.is_empty() || bone_names.len() > 2 {
			return Err(create_read_error(&format!("IK \"{}\" 의 뼈는 하나나 둘이어야 한다.", name)));
		}
		let mut bone_indices = Vec::with_capacity(bone_names.len());
		for bone_name in bone_names {
			match bone_name.as_str().and_then(|bone_name| skeleton_data.find_bone_index(bone_name)) {
				Some(bone_index) => {
					bone_indices.push(bone_index);
				}
				None => {
					return Err(create_read_error(&format!("IK \"{}\" 의 뼈 \"{}\" 가 없다.", name, bone_name.as_str().unwrap_or("null"))));
				}
			}
		}
		if bone_indices.len() == 2 && skeleton_data.get_bones()[bone_indices[1]].get_parent_index() != Some(bone_indices[0]) {
			return Err(create_read_error(&format!("IK \"{}\" 의 둘째 뼈는 첫째 뼈의 자식이어야 한다.", name)));
		}
		let target_name = read_string(ik_object, "target");
		let target_index = match target_name.and_then(|target_name| skeleton_data.find_bone_index(target_name)) {
			Some(target_index) => {
				target_index
			}
			None => {
				return Err(create_read_error(&format!("IK \"{}\" 의 목표 뼈 \"{}\" 가 없다.", name, target_name.unwrap_or("null"))));
			}
		};
		let mut ik_constraint_data = Skeleton2DIkConstraintData::new(skeleton_data.get_ik_constraints().len(), name, bone_indices, target_index);
		ik_constraint_data.set_bend_positive(ik_object.get("bendPositive").and_then(|value| value.as_bool()) != Some(false));
		ik_constraint_data.set_mix(read_number(ik_object, "mix", 1.0));
		skeleton_data.add_ik_constraint(ik_constraint_data);
	}
	Ok(())
}

//==============================================================================
/// 메시 붙임 읽기. (무게가 있으면 처음 자세의 월드 행렬로 묶은 자리를 셈한다)
//==============================================================================
fn read_mesh_attachment(skeleton_data: &Skeleton2DData, world_matrices: &[f64], slot_index: usize, attachment_name: &str, attachment_json: &Map<String, Value>) -> ReadResult<Skeleton2DMeshAttachment> {
	let place = format!("메시 \"{}\"", attachment_name);
	let frame_name = read_string(attachment_json, "frame").unwrap_or(attachment_name);
	let mut mesh_attachment = Skeleton2DMeshAttachment::new(attachment_name, frame_name);
	let uvs = read_number_array(attachment_json, "uvs", &place)?;
	let triangle_values = read_number_array(attachment_json, "triangles", &place)?;
	let vertices = read_number_array(attachment_json, "vertices", &place)?;
	let vertex_count = uvs.len() / 2;
	if vertices.len() != vertex_count * 2 {
		return Err(create_read_error(&format!("{} 의 vertices 와 uvs 의 꼭짓점 수가 다르다.", place)));
	}
	if triangle_values.len() % 3 != 0 {
		return Err(create_read_error(&format!("{} 의 triangles 가 셋씩 나뉘지 않는다.", place)));
	}
	let mut triangles = Vec::with_capacity(triangle_values.len());
	for vertex_value in triangle_values {
		if vertex_value < 0.0 || vertex_value as usize >= vertex_count || vertex_value.fract() != 0.0 {
			return Err(create_read_error(&format!("{} 의 triangles 에 없는 꼭짓점 {} 이 있다.", place, vertex_value)));
		}
		triangles.push(vertex_value as usize);
	}
	mesh_attachment.set_geometry(uvs, triangles);

	let weight_json_list = read_array(attachment_json, "weights", &place)?;
	if weight_json_list.is_empty() {
		mesh_attachment.set_vertices(vertices);
		return Ok(mesh_attachment);
	}
	if weight_json_list.len() != vertex_count {
		return Err(create_read_error(&format!("{} 의 weights 수가 꼭짓점 수와 다르다.", place)));
	}

	// 슬롯 뼈 기준 꼭짓점을 처음 자세의 월드 자리로 옮긴 뒤, 영향 뼈마다 그 뼈 기준 자리로 되돌린다.
	let slot_bone_offset = skeleton_data.get_slots()[slot_index].get_bone_index() * 6;
	let mut influence_counts = Vec::with_capacity(vertex_count);
	let mut influence_bone_indices = Vec::new();
	let mut influence_values = Vec::new();
	for (vertex_index, weight_json) in weight_json_list.iter().enumerate() {
		let local_x = vertices[vertex_index * 2];
		let local_y = vertices[vertex_index * 2 + 1];
		let world_x = world_matrices[slot_bone_offset] * local_x + world_matrices[slot_bone_offset + 2] * local_y + world_matrices[slot_bone_offset + 4];
		let world_y = world_matrices[slot_bone_offset + 1] * local_x + world_matrices[slot_bone_offset + 3] * local_y + world_matrices[slot_bone_offset + 5];
		let weight_values = match weight_json.as_array() {
			Some(weight_values) if weight_values.len() >= 2 && weight_values.len() % 2 == 0 => {
				weight_values
			}
			_ => {
				return Err(create_read_error(&format!("{} 의 꼭짓점 {} 무게는 [뼈 이름, 무게, ...] 꼴이어야 한다.", place, vertex_index)));
			}
		};
		let total_weight: f64 = weight_values.iter().skip(1).step_by(2).map(|value| value.as_f64().unwrap_or(0.0)).sum();
		if total_weight <= 0.0 {
			return Err(create_read_error(&format!("{} 의 꼭짓점 {} 무게 합이 0 이다.", place, vertex_index)));
		}
		influence_counts.push(weight_values.len() / 2);
		for pair in weight_values.chunks(2) {
			let bone_name = pair[0].as_str();
			let bone_index = match bone_name.and_then(|bone_name| skeleton_data.find_bone_index(bone_name)) {
				Some(bone_index) => {
					bone_index
				}
				None => {
					return Err(create_read_error(&format!("{} 의 무게 뼈 \"{}\" 가 없다.", place, bone_name.unwrap_or("null"))));
				}
			};
			let bone_offset = bone_index * 6;
			let bone_a = world_matrices[bone_offset];
			let bone_b = world_matrices[bone_offset + 1];
			let bone_c = world_matrices[bone_offset + 2];
			let bone_d = world_matrices[bone_offset + 3];
			let determinant = bone_a * bone_d - bone_b * bone_c;
			if determinant == 0.0 {
				return Err(create_read_error(&format!("{} 의 무게 뼈 \"{}\" 의 처음 행렬을 되돌릴 수 없다. (크기 0)", place, bone_name.unwrap_or("null"))));
			}
			let difference_x = world_x - world_matrices[bone_offset + 4];
			let difference_y = world_y - world_matrices[bone_offset + 5];
			let bind_x = (bone_d * difference_x - bone_c * difference_y) / determinant;
			let bind_y = (bone_a * difference_y - bone_b * difference_x) / determinant;
			let weight = pair[1].as_f64().unwrap_or(0.0);
			influence_bone_indices.push(bone_index);
			influence_values.extend_from_slice(&[bind_x, bind_y, weight / total_weight]);
		}
	}
	mesh_attachment.set_weights(influence_counts, influence_bone_indices, influence_values);
	Ok(mesh_attachment)
}

//==============================================================================
/// 스킨 읽기.
//==============================================================================
fn read_skins(skeleton_data: &mut Skeleton2DData, json: &Map<String, Value>) -> ReadResult<()> {
	let skin_json_list = read_array(json, "skins", "뼈대")?;
	if skin_json_list.is_empty() {
		return Ok(());
	}
	let world_matrices = Skeleton2DData::compute_setup_world_matrices(skeleton_data.get_bones());
	let mut skins = Vec::with_capacity(skin_json_list.len());
	for skin_json in skin_json_list {
		let skin_object = match skin_json.as_object() {
			Some(skin_object) => {
				skin_object
			}
			None => {
				return Err(create_read_error("스킨이 객체가 아니다."));
			}
		};
		let skin_name = read_string(skin_object, "name").unwrap_or("default");
		let mut skin = Skeleton2DSkin::new(skin_name);
		let empty_object = Map::new();
		let slot_attachments_json = skin_object.get("attachments").and_then(|value| value.as_object()).unwrap_or(&empty_object);
		for (slot_name, attachments_json) in slot_attachments_json {
			let slot_index = match skeleton_data.find_slot_index(slot_name) {
				Some(slot_index) => {
					slot_index
				}
				None => {
					return Err(create_read_error(&format!("스킨 \"{}\" 의 슬롯 \"{}\" 가 없다.", skin_name, slot_name)));
				}
			};
			let attachments_object = match attachments_json.as_object() {
				Some(attachments_object) => {
					attachments_object
				}
				None => {
					return Err(create_read_error(&format!("스킨 \"{}\" 의 슬롯 \"{}\" 붙임이 객체가 아니다.", skin_name, slot_name)));
				}
			};
			for (attachment_name, attachment_json) in attachments_object {
				let attachment_object = match attachment_json.as_object() {
					Some(attachment_object) => {
						attachment_object
					}
					None => {
						return Err(create_read_error(&format!("붙임 \"{}\" 이 객체가 아니다.", attachment_name)));
					}
				};
				let attachment_type = read_string(attachment_object, "type").unwrap_or("region");
				match attachment_type {
					"region" => {
						let frame_name = read_string(attachment_object, "frame").unwrap_or(attachment_name);
						let mut region_attachment = Skeleton2DRegionAttachment::new(attachment_name, frame_name);
						region_attachment.set_transform(
							read_number(attachment_object, "x", 0.0),
							read_number(attachment_object, "y", 0.0),
							read_number(attachment_object, "rotation", 0.0),
							read_number(attachment_object, "scaleX", 1.0),
							read_number(attachment_object, "scaleY", 1.0),
						);
						region_attachment.set_size(read_number(attachment_object, "width", 0.0), read_number(attachment_object, "height", 0.0));
						region_attachment.set_pivot(read_number(attachment_object, "pivotX", 0.5), read_number(attachment_object, "pivotY", 0.5));
						skin.set_attachment(slot_index, attachment_name, Skeleton2DAttachment::Region(region_attachment));
					}
					"mesh" => {
						let mesh_attachment = read_mesh_attachment(skeleton_data, &world_matrices, slot_index, attachment_name, attachment_object)?;
						skin.set_attachment(slot_index, attachment_name, Skeleton2DAttachment::Mesh(mesh_attachment));
					}
					_ => {
						return Err(create_read_error(&format!("붙임 \"{}\" 의 종류 \"{}\" 를 모른다. (region, mesh)", attachment_name, attachment_type)));
					}
				}
			}
		}
		skins.push(skin);
	}
	for skin in skins {
		skeleton_data.add_skin(skin);
	}
	Ok(())
}

//==============================================================================
/// 뼈 시간줄 읽기. (rotate, translate, scale, shear, 마지막 키 시각을 돌려준다)
//==============================================================================
fn read_bone_timeline(animation: &mut Skeleton2DAnimation, bone_index: usize, timeline_name: &str, key_json_list: &Value, place: &str) -> ReadResult<f64> {
	let timeline_type = match timeline_name {
		"rotate" => {
			Skeleton2DTimelineType::Rotate
		}
		"translate" => {
			Skeleton2DTimelineType::Translate
		}
		"scale" => {
			Skeleton2DTimelineType::Scale
		}
		"shear" => {
			Skeleton2DTimelineType::Shear
		}
		_ => {
			return Err(create_read_error(&format!("{} 의 시간줄 \"{}\" 를 모른다. (rotate, translate, scale, shear)", place, timeline_name)));
		}
	};
	let key_values = match key_json_list.as_array() {
		Some(key_values) => {
			key_values
		}
		None => {
			return Err(create_read_error(&format!("{} 의 {} 가 배열이 아니다.", place, timeline_name)));
		}
	};
	let mut timeline = Skeleton2DTimeline::new(timeline_type, Some(bone_index));
	let default_value = if timeline_type == Skeleton2DTimelineType::Scale { 1.0 } else { 0.0 };
	let mut previous_time = 0.0f64;
	for key_json in key_values {
		let (key_object, time) = read_key_time(key_json, previous_time, place)?;
		let (curve_type, curve_values) = read_curve(key_object, place)?;
		if timeline_type == Skeleton2DTimelineType::Rotate {
			timeline.add_value_frame(time, &[read_number(key_object, "value", 0.0)], curve_type, curve_values);
		}
		else {
			let x = read_number(key_object, "x", default_value);
			let y = read_number(key_object, "y", default_value);
			timeline.add_value_frame(time, &[x, y], curve_type, curve_values);
		}
		previous_time = time;
	}
	animation.add_timeline(timeline);
	Ok(previous_time)
}

//==============================================================================
/// 클립 하나 읽기.
//==============================================================================
fn read_animation(skeleton_data: &Skeleton2DData, animation_name: &str, animation_json: &Value) -> ReadResult<Skeleton2DAnimation> {
	let place = format!("클립 \"{}\"", animation_name);
	let animation_object = match animation_json.as_object() {
		Some(animation_object) => {
			animation_object
		}
		None => {
			return Err(create_read_error(&format!("{} 가 객체가 아니다.", place)));
		}
	};
	let mut animation = Skeleton2DAnimation::new(animation_name, 0.0);
	let mut last_time = 0.0f64;

	// 뼈 시간줄.
	if let Some(bones_object) = animation_object.get("bones").and_then(|value| value.as_object()) {
		for (bone_name, timelines_json) in bones_object {
			let bone_index = match skeleton_data.find_bone_index(bone_name) {
				Some(bone_index) => {
					bone_index
				}
				None => {
					return Err(create_read_error(&format!("{} 의 뼈 \"{}\" 가 없다.", place, bone_name)));
				}
			};
			let timelines_object = match timelines_json.as_object() {
				Some(timelines_object) => {
					timelines_object
				}
				None => {
					return Err(create_read_error(&format!("{} 의 뼈 \"{}\" 시간줄이 객체가 아니다.", place, bone_name)));
				}
			};
			let bone_place = format!("{} 뼈 \"{}\"", place, bone_name);
			for (timeline_name, key_json_list) in timelines_object {
				let timeline_last_time = read_bone_timeline(&mut animation, bone_index, timeline_name, key_json_list, &bone_place)?;
				last_time = js_max(last_time, timeline_last_time);
			}
		}
	}

	// 슬롯 시간줄.
	if let Some(slots_object) = animation_object.get("slots").and_then(|value| value.as_object()) {
		for (slot_name, timelines_json) in slots_object {
			let slot_index = match skeleton_data.find_slot_index(slot_name) {
				Some(slot_index) => {
					slot_index
				}
				None => {
					return Err(create_read_error(&format!("{} 의 슬롯 \"{}\" 가 없다.", place, slot_name)));
				}
			};
			let timelines_object = match timelines_json.as_object() {
				Some(timelines_object) => {
					timelines_object
				}
				None => {
					return Err(create_read_error(&format!("{} 의 슬롯 \"{}\" 시간줄이 객체가 아니다.", place, slot_name)));
				}
			};
			let slot_place = format!("{} 슬롯 \"{}\"", place, slot_name);
			for timeline_name in timelines_object.keys() {
				let key_json_list = read_array(timelines_object, timeline_name, &slot_place)?;
				let mut previous_time = 0.0f64;
				match timeline_name.as_str() {
					"color" => {
						let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Color, Some(slot_index));
						for key_json in key_json_list {
							let (key_object, time) = read_key_time(key_json, previous_time, &slot_place)?;
							let (curve_type, curve_values) = read_curve(key_object, &slot_place)?;
							let color = read_skeleton2d_color(read_string(key_object, "color").unwrap_or("#ffffffff"), &slot_place)?;
							timeline.add_value_frame(time, &color, curve_type, curve_values);
							previous_time = time;
						}
						animation.add_timeline(timeline);
					}
					"attachment" => {
						let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Attachment, Some(slot_index));
						for key_json in key_json_list {
							let (key_object, time) = read_key_time(key_json, previous_time, &slot_place)?;
							timeline.add_attachment_frame(time, read_string(key_object, "name"));
							previous_time = time;
						}
						animation.add_timeline(timeline);
					}
					_ => {
						return Err(create_read_error(&format!("{} 의 시간줄 \"{}\" 를 모른다. (color, attachment)", slot_place, timeline_name)));
					}
				}
				last_time = js_max(last_time, previous_time);
			}
		}
	}

	// 그리기 차례 시간줄.
	let draw_order_json_list = read_array(animation_object, "drawOrder", &place)?;
	if !draw_order_json_list.is_empty() {
		let slot_count = skeleton_data.get_slots().len();
		let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::DrawOrder, None);
		let mut previous_time = 0.0f64;
		let draw_order_place = format!("{} drawOrder", place);
		for key_json in draw_order_json_list {
			let (key_object, time) = read_key_time(key_json, previous_time, &draw_order_place)?;
			match key_object.get("order") {
				None | Some(Value::Null) => {
					timeline.add_draw_order_frame(time, None);
				}
				Some(order_json) => {
					let order_values = match order_json.as_array() {
						Some(order_values) if order_values.len() == slot_count => {
							order_values
						}
						_ => {
							return Err(create_read_error(&format!("{} 의 drawOrder 키는 슬롯 이름을 모두 담아야 한다.", place)));
						}
					};
					let mut draw_order: Vec<usize> = Vec::with_capacity(slot_count);
					for slot_name_json in order_values {
						match slot_name_json.as_str().and_then(|slot_name| skeleton_data.find_slot_index(slot_name)) {
							Some(slot_index) if !draw_order.contains(&slot_index) => {
								draw_order.push(slot_index);
							}
							_ => {
								let slot_name_text = slot_name_json.as_str().map(|slot_name| slot_name.to_string()).unwrap_or_else(|| slot_name_json.to_string());
								return Err(create_read_error(&format!("{} 의 drawOrder 슬롯 \"{}\" 가 없거나 겹친다.", place, slot_name_text)));
							}
						}
					}
					timeline.add_draw_order_frame(time, Some(draw_order));
				}
			}
			previous_time = time;
		}
		animation.add_timeline(timeline);
		last_time = js_max(last_time, previous_time);
	}

	// 사건 시간줄.
	let event_json_list = read_array(animation_object, "events", &place)?;
	if !event_json_list.is_empty() {
		let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Event, None);
		let mut previous_time = 0.0f64;
		let event_place = format!("{} events", place);
		for key_json in event_json_list {
			let (key_object, time) = read_key_time(key_json, previous_time, &event_place)?;
			let name = read_string(key_object, "name").unwrap_or("");
			let int_value = read_number(key_object, "int", 0.0) as i32;
			let float_value = read_number(key_object, "float", 0.0);
			let string_value = read_string(key_object, "string").unwrap_or("");
			timeline.add_event_frame(Skeleton2DEvent::new(name, time, int_value, float_value, string_value));
			previous_time = time;
		}
		animation.add_timeline(timeline);
		last_time = js_max(last_time, previous_time);
	}

	animation.set_duration(read_number(animation_object, "duration", last_time));
	Ok(animation)
}

//==============================================================================
/// 엔진 자체 JSON 읽기.
///
/// # 인자
/// - `json`: JSON 값.
///
/// # 돌려줌
/// 지은 자료. 꼴이 틀리면 Err.
//==============================================================================
pub(crate) fn read_skeleton2d_json(json: &Value) -> Result<Skeleton2DData, String> {
	let json_object = match json.as_object() {
		Some(json_object) => {
			json_object
		}
		None => {
			return Err(create_read_error("뼈대 JSON 이 객체가 아니다."));
		}
	};
	let mut skeleton_data = Skeleton2DData::new(read_string(json_object, "name").unwrap_or(""));
	skeleton_data.set_frame_rate(read_number(json_object, "frameRate", 30.0));
	read_bones(&mut skeleton_data, json_object)?;
	read_slots(&mut skeleton_data, json_object)?;
	read_ik_constraints(&mut skeleton_data, json_object)?;
	read_skins(&mut skeleton_data, json_object)?;
	match json_object.get("animations") {
		None | Some(Value::Null) => {}
		Some(Value::Object(animations_object)) => {
			let mut animations = Vec::with_capacity(animations_object.len());
			for (animation_name, animation_json) in animations_object {
				animations.push(read_animation(&skeleton_data, animation_name, animation_json)?);
			}
			for animation in animations {
				skeleton_data.add_animation(animation);
			}
		}
		Some(_) => {
			return Err(create_read_error("animations 가 객체가 아니다. ({ 이름: 클립 })"));
		}
	}
	Ok(skeleton_data)
}

//==============================================================================
//! DragonBones 5.x JSON 읽기. (원본 experimental/skeleton2d/dragonbonesreader.js, Skeleton2DData::create_from_dragon_bones 가 부른다)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - DragonBones 는 MIT 라이선스이고, 키 이름과 뜻은 DragonBones 5.5 형식 명세(DragonBones/Tools 의 dragonbones_json_format_5.5.md)와
//!   DragonBonesJS 의 ObjectDataParser 를 보고 맞췄다. 코드는 옮기지 않고 엔진 자료 꼴로 새로 짰다.
//! - 좌표는 엔진과 같이 y 가 아래로 커진다. (DragonBones 코어의 yDown 기본값)
//! - 뼈 transform: 회전 = skY(또는 rotate), 기울임 y = skX - skY(또는 skew), 기울임 x = 0. 엔진 뼈 행렬과 DragonBones Transform.toMatrix 가 같은 값이 된다.
//! - 무게 있는 메시: 꼭짓점을 slotPose 로 뼈대 공간에 옮기고, 영향 뼈마다 bonePose 의 되돌림 행렬로 그 뼈 기준 묶은 자리를 셈한다.
//! - 무게 없는 메시와 그림은 display 의 transform 을 슬롯 뼈 기준으로 건다.
//! - 프레임 수는 frameRate 로 초가 된다. 키프레임의 duration 기본은 1 프레임이다.
//! - 공개 API 는 Skeleton2DData::create_from_dragon_bones 이다. 원본과 같은 꼴, 같은 오류 글이다.
//==============================================================================
use crate::base::color::Color;
use crate::core::graphic::BlendMode;
use crate::experimental::skeleton2d::skeleton2danimation::{Skeleton2DAnimation, Skeleton2DCurveType, Skeleton2DEvent, Skeleton2DTimeline, Skeleton2DTimelineType};
use crate::experimental::skeleton2d::skeleton2ddata::{Skeleton2DAttachment, Skeleton2DBoneData, Skeleton2DData, Skeleton2DIkConstraintData, Skeleton2DMeshAttachment, Skeleton2DRegionAttachment, Skeleton2DSkin, Skeleton2DSlotData};
use serde_json::{Map, Value};

/// 읽기 결과. (실패는 오류 글)
type ReadResult<T> = Result<T, String>;

/// 키프레임 곡선. (종류, 베지어 제어점)
type FrameCurve = (Skeleton2DCurveType, Option<[f32; 4]>);

//==============================================================================
/// 뼈 transform. (도)
//==============================================================================
struct DragonBonesTransform {
	x: f32,
	y: f32,
	rotation: f32,
	shear_y: f32,
	scale_x: f32,
	scale_y: f32,
}

//==============================================================================
/// 오류 글 짓기. (머리를 붙인다)
//==============================================================================
fn create_read_error(message: &str) -> String {
	format!("[Skeleton2D] DragonBones {}", message)
}

//==============================================================================
/// 숫자 칸 읽기. (없거나 숫자가 아니면 기본값)
//==============================================================================
fn read_number(source: &Map<String, Value>, key: &str, default_value: f32) -> f32 {
	match source.get(key).and_then(|value| value.as_f64()) {
		Some(number) => {
			number as f32
		}
		None => {
			default_value
		}
	}
}

//==============================================================================
/// 글 칸 읽기. (없거나 글이 아니면 기본값)
//==============================================================================
fn read_string<'a>(source: &'a Map<String, Value>, key: &str, default_value: &'a str) -> &'a str {
	source.get(key).and_then(|value| value.as_str()).unwrap_or(default_value)
}

//==============================================================================
/// 배열 칸 읽기. (없거나 배열이 아니면 빈 배열)
//==============================================================================
fn read_array<'a>(source: &'a Map<String, Value>, key: &str) -> &'a [Value] {
	match source.get(key) {
		Some(Value::Array(values)) => {
			values.as_slice()
		}
		_ => {
			&[]
		}
	}
}

//==============================================================================
/// 숫자 배열 칸 읽기. (없으면 빈 배열, 숫자가 아닌 값은 0)
//==============================================================================
fn read_number_array(source: &Map<String, Value>, key: &str) -> Vec<f32> {
	read_array(source, key).iter().map(|value| value.as_f64().unwrap_or(0.0) as f32).collect()
}

//==============================================================================
/// 빈 객체. (객체가 아닌 칸 자리)
//==============================================================================
fn empty_object() -> &'static Map<String, Value> {
	static EMPTY_OBJECT: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
	EMPTY_OBJECT.get_or_init(Map::new)
}

//==============================================================================
/// transform 읽기. (도)
//==============================================================================
fn read_transform(transform_json: Option<&Value>) -> DragonBonesTransform {
	let mut transform = DragonBonesTransform {
		x: 0.0,
		y: 0.0,
		rotation: 0.0,
		shear_y: 0.0,
		scale_x: 1.0,
		scale_y: 1.0,
	};
	let transform_object = match transform_json.and_then(|value| value.as_object()) {
		Some(transform_object) => {
			transform_object
		}
		None => {
			return transform;
		}
	};
	transform.x = read_number(transform_object, "x", 0.0);
	transform.y = read_number(transform_object, "y", 0.0);
	if transform_object.contains_key("rotate") || transform_object.contains_key("skew") {
		transform.rotation = read_number(transform_object, "rotate", 0.0);
		transform.shear_y = read_number(transform_object, "skew", 0.0);
	}
	else {
		let skew_x = read_number(transform_object, "skX", 0.0);
		let skew_y = read_number(transform_object, "skY", 0.0);
		transform.rotation = skew_y;
		transform.shear_y = skew_x - skew_y;
	}
	transform.scale_x = read_number(transform_object, "scX", 1.0);
	transform.scale_y = read_number(transform_object, "scY", 1.0);
	transform
}

//==============================================================================
/// 색 변환의 곱 읽기. (aM, rM, gM, bM 은 0 ~ 100, 더하기 aO 등은 읽지 않는다)
//==============================================================================
fn read_color_transform(color_json: Option<&Value>) -> Color {
	match color_json.and_then(|value| value.as_object()) {
		Some(color_object) => {
			Color::create(
				read_number(color_object, "rM", 100.0) * 0.01,
				read_number(color_object, "gM", 100.0) * 0.01,
				read_number(color_object, "bM", 100.0) * 0.01,
				read_number(color_object, "aM", 100.0) * 0.01,
			)
		}
		None => {
			Color::create(1.0, 1.0, 1.0, 1.0)
		}
	}
}

//==============================================================================
/// 키프레임의 곡선 읽기.
/// - curve 가 넷이면 베지어, 더 길면(여러 토막) 선형. 없으면 tweenEasing: 없음은 계단, 0 은 선형,
///   음수는 QuadIn, 0 ~ 1 은 QuadOut, 1 초과는 QuadInOut 을 그 세기로 선형과 섞은 것. Quad 둘은 같은 베지어로, QuadInOut 은 smoothstep 베지어로 담는다.
//==============================================================================
fn read_frame_curve(frame_json: &Map<String, Value>) -> FrameCurve {
	if let Some(Value::Array(curve_values)) = frame_json.get("curve") {
		if curve_values.len() == 4 {
			let read_value = |value_index: usize| -> f32 { curve_values[value_index].as_f64().unwrap_or(0.0) as f32 };
			return (Skeleton2DCurveType::Bezier, Some([read_value(0), read_value(1), read_value(2), read_value(3)]));
		}
		return (Skeleton2DCurveType::Linear, None);
	}
	let tween_easing = match frame_json.get("tweenEasing").and_then(|value| value.as_f64()) {
		Some(tween_easing) => {
			tween_easing as f32
		}
		None => {
			return (Skeleton2DCurveType::Stepped, None);
		}
	};
	if tween_easing == 0.0 {
		return (Skeleton2DCurveType::Linear, None);
	}
	let one_third = 1.0 / 3.0;
	let two_thirds = 2.0 / 3.0;
	if tween_easing < 0.0 {
		let easing = -tween_easing;
		return (Skeleton2DCurveType::Bezier, Some([one_third, (1.0 - easing) / 3.0, two_thirds, (2.0 - easing) / 3.0]));
	}
	if tween_easing <= 1.0 {
		let easing = tween_easing;
		return (Skeleton2DCurveType::Bezier, Some([one_third, (1.0 + easing) / 3.0, two_thirds, (2.0 + easing) / 3.0]));
	}
	let easing = tween_easing - 1.0;
	(Skeleton2DCurveType::Bezier, Some([one_third, (1.0 - easing) / 3.0, two_thirds, (2.0 + easing) / 3.0]))
}

//==============================================================================
/// 슬롯의 display 목록에서 붙임 이름 짓기. (같은 이름이 겹치면 "#번호", 읽지 않는 종류는 None)
//==============================================================================
fn read_display_names(display_json_list: &[Value]) -> Vec<Option<String>> {
	let mut display_names: Vec<Option<String>> = Vec::with_capacity(display_json_list.len());
	for (display_index, display_json) in display_json_list.iter().enumerate() {
		let display_object = match display_json.as_object() {
			Some(display_object) => {
				display_object
			}
			None => {
				display_names.push(None);
				continue;
			}
		};
		let display_type = read_string(display_object, "type", "image");
		if display_type != "image" && display_type != "mesh" {
			display_names.push(None);
			continue;
		}
		let mut name = read_string(display_object, "name", "").to_string();
		if name.is_empty() {
			name = match display_object.get("path").and_then(|value| value.as_str()) {
				Some(path) => {
					path.to_string()
				}
				None => {
					format!("display{}", display_index)
				}
			};
		}
		if display_names.iter().any(|display_name| display_name.as_deref() == Some(name.as_str())) {
			name = format!("{}#{}", name, display_index);
		}
		display_names.push(Some(name));
	}
	display_names
}

//==============================================================================
/// 메시 display 읽기.
//==============================================================================
fn read_mesh_display(skeleton_data: &Skeleton2DData, attachment_name: &str, frame_name: &str, display_json: &Map<String, Value>) -> ReadResult<Skeleton2DMeshAttachment> {
	let place = format!("메시 \"{}\"", attachment_name);
	let vertices = read_number_array(display_json, "vertices");
	let uvs = read_number_array(display_json, "uvs");
	let triangle_values = read_number_array(display_json, "triangles");
	let vertex_count = vertices.len() / 2;
	if uvs.len() != vertex_count * 2 {
		return Err(create_read_error(&format!("{} 의 uvs 와 vertices 의 꼭짓점 수가 다르다.", place)));
	}
	if triangle_values.len() % 3 != 0 {
		return Err(create_read_error(&format!("{} 의 triangles 가 셋씩 나뉘지 않는다.", place)));
	}
	let mut mesh_attachment = Skeleton2DMeshAttachment::new(attachment_name, frame_name);
	mesh_attachment.set_geometry(uvs, triangle_values.iter().map(|value| *value as usize).collect());

	let weights = read_number_array(display_json, "weights");
	if weights.is_empty() {
		// 무게 없는 꼭짓점은 display transform 을 걸어 슬롯 뼈 기준으로 담는다.
		let transform = read_transform(display_json.get("transform"));
		let x_axis_radian = transform.rotation.to_radians();
		let y_axis_radian = (transform.rotation + transform.shear_y).to_radians();
		let local_a = x_axis_radian.cos() * transform.scale_x;
		let local_b = x_axis_radian.sin() * transform.scale_x;
		let local_c = -y_axis_radian.sin() * transform.scale_y;
		let local_d = y_axis_radian.cos() * transform.scale_y;
		let mut bone_local_vertices = Vec::with_capacity(vertex_count * 2);
		for vertex_index in 0..vertex_count {
			let vertex_x = vertices[vertex_index * 2];
			let vertex_y = vertices[vertex_index * 2 + 1];
			bone_local_vertices.push(local_a * vertex_x + local_c * vertex_y + transform.x);
			bone_local_vertices.push(local_b * vertex_x + local_d * vertex_y + transform.y);
		}
		mesh_attachment.set_vertices(bone_local_vertices);
		return Ok(mesh_attachment);
	}

	let slot_pose = read_number_array(display_json, "slotPose");
	let bone_pose = read_number_array(display_json, "bonePose");
	if slot_pose.len() < 6 || bone_pose.len() < 7 {
		return Err(create_read_error(&format!("{} 의 무게는 slotPose 와 bonePose 가 있어야 한다.", place)));
	}
	let bone_count = skeleton_data.get_bones().len();
	let pose_bone_indices: Vec<f32> = bone_pose.chunks_exact(7).map(|pose| pose[0]).collect();
	let mut influence_counts = Vec::with_capacity(vertex_count);
	let mut influence_bone_indices = Vec::new();
	let mut influence_values = Vec::new();
	let mut weight_index = 0usize;
	for vertex_index in 0..vertex_count {
		let vertex_x = vertices[vertex_index * 2];
		let vertex_y = vertices[vertex_index * 2 + 1];
		let armature_x = slot_pose[0] * vertex_x + slot_pose[2] * vertex_y + slot_pose[4];
		let armature_y = slot_pose[1] * vertex_x + slot_pose[3] * vertex_y + slot_pose[5];
		let influence_value = weights.get(weight_index).copied().unwrap_or(0.0);
		weight_index += 1;
		let influence_count = influence_value as usize;
		if influence_value <= 0.0 || weight_index + influence_count * 2 > weights.len() {
			return Err(create_read_error(&format!("{} 의 꼭짓점 {} 무게를 읽지 못했다.", place, vertex_index)));
		}
		influence_counts.push(influence_count);
		for _ in 0..influence_count {
			let bone_value = weights[weight_index];
			let weight = weights[weight_index + 1];
			weight_index += 2;
			let pose_index = pose_bone_indices.iter().position(|pose_bone_index| *pose_bone_index == bone_value);
			let pose_index = match pose_index {
				Some(pose_index) if bone_value >= 0.0 && (bone_value as usize) < bone_count => {
					pose_index
				}
				_ => {
					return Err(create_read_error(&format!("{} 의 무게 뼈 {} 가 bonePose 에 없다.", place, bone_value)));
				}
			};
			let pose_offset = pose_index * 7 + 1;
			let pose_a = bone_pose[pose_offset];
			let pose_b = bone_pose[pose_offset + 1];
			let pose_c = bone_pose[pose_offset + 2];
			let pose_d = bone_pose[pose_offset + 3];
			let determinant = pose_a * pose_d - pose_b * pose_c;
			if determinant == 0.0 {
				return Err(create_read_error(&format!("{} 의 bonePose 를 되돌릴 수 없다. (크기 0)", place)));
			}
			let difference_x = armature_x - bone_pose[pose_offset + 4];
			let difference_y = armature_y - bone_pose[pose_offset + 5];
			let bind_x = (pose_d * difference_x - pose_c * difference_y) / determinant;
			let bind_y = (pose_a * difference_y - pose_b * difference_x) / determinant;
			influence_bone_indices.push(bone_value as usize);
			influence_values.extend_from_slice(&[bind_x, bind_y, weight]);
		}
	}
	mesh_attachment.set_weights(influence_counts, influence_bone_indices, influence_values);
	Ok(mesh_attachment)
}

//==============================================================================
/// 시간줄 키프레임 시각 목록. (프레임 수를 쌓아 초로, duration 기본 1)
//==============================================================================
fn read_frame_times(frame_json_list: &[Value], frame_rate: f32) -> Vec<f32> {
	let mut frame_times = Vec::with_capacity(frame_json_list.len());
	let mut frame_start = 0.0f32;
	for frame_json in frame_json_list {
		frame_times.push(frame_start / frame_rate);
		let duration = match frame_json.as_object() {
			Some(frame_object) => {
				read_number(frame_object, "duration", 1.0)
			}
			None => {
				1.0
			}
		};
		frame_start += duration;
	}
	frame_times
}

//==============================================================================
/// 그리기 차례 오프셋 풀기. ([슬롯 번호, 옮김, ...] → 그리는 차례의 슬롯 번호 모두, 비었으면 None)
//==============================================================================
fn read_z_order(z_order_values: &[f32], slot_count: usize, place: &str) -> ReadResult<Option<Vec<usize>>> {
	if z_order_values.is_empty() {
		return Ok(None);
	}
	let mut draw_order: Vec<Option<usize>> = vec![None; slot_count];
	let mut unchanged_slots: Vec<usize> = Vec::new();
	let mut original_index = 0usize;
	for pair in z_order_values.chunks_exact(2) {
		let slot_value = pair[0];
		if slot_value < original_index as f32 || slot_value >= slot_count as f32 {
			return Err(create_read_error(&format!("{} 의 zOrder 슬롯 번호 {} 가 차례가 아니거나 없다.", place, slot_value)));
		}
		let slot_index = slot_value as usize;
		while original_index != slot_index {
			unchanged_slots.push(original_index);
			original_index += 1;
		}
		let order_value = original_index as f32 + pair[1];
		if order_value < 0.0 || order_value >= slot_count as f32 || draw_order[order_value as usize].is_some() {
			return Err(create_read_error(&format!("{} 의 zOrder 옮김이 자리를 벗어나거나 겹친다.", place)));
		}
		draw_order[order_value as usize] = Some(original_index);
		original_index += 1;
	}
	while original_index < slot_count {
		unchanged_slots.push(original_index);
		original_index += 1;
	}
	let mut resolved_draw_order = vec![0usize; slot_count];
	for order_index in (0..slot_count).rev() {
		resolved_draw_order[order_index] = match draw_order[order_index] {
			Some(slot_index) => {
				slot_index
			}
			None => {
				unchanged_slots.pop().unwrap_or(0)
			}
		};
	}
	Ok(Some(resolved_draw_order))
}

//==============================================================================
/// 키프레임 객체 읽기. (객체가 아니면 빈 객체)
//==============================================================================
fn read_frame_object(frame_json: &Value) -> &Map<String, Value> {
	frame_json.as_object().unwrap_or_else(|| empty_object())
}

//==============================================================================
/// 회전 키를 앞 키에서 이어지는 값으로 펴기. (clockwise 0 은 가까운 쪽, 양수는 시계 방향, 음수는 반시계 방향으로 그 바퀴 수)
//==============================================================================
fn unwrap_rotation(rotation: f32, previous_rotation: f32, previous_clockwise: &mut f32) -> f32 {
	if *previous_clockwise == 0.0 {
		let mut difference = (rotation - previous_rotation) % 360.0;
		if difference > 180.0 {
			difference -= 360.0;
		}
		else if difference < -180.0 {
			difference += 360.0;
		}
		return previous_rotation + difference;
	}
	let is_passed = if *previous_clockwise > 0.0 { rotation >= previous_rotation } else { rotation <= previous_rotation };
	if is_passed {
		*previous_clockwise = if *previous_clockwise > 0.0 { *previous_clockwise - 1.0 } else { *previous_clockwise + 1.0 };
	}
	rotation + 360.0 * *previous_clockwise
}

//==============================================================================
/// 클립 하나 읽기.
//==============================================================================
fn read_animation(skeleton_data: &Skeleton2DData, animation_json: &Map<String, Value>, frame_rate: f32, slot_display_names: &[Vec<Option<String>>]) -> ReadResult<Skeleton2DAnimation> {
	let mut animation_name = read_string(animation_json, "name", "default");
	if animation_name.is_empty() {
		animation_name = "default";
	}
	let place = format!("클립 \"{}\"", animation_name);
	let duration = read_number(animation_json, "duration", 0.0) / frame_rate;
	let mut animation = Skeleton2DAnimation::new(animation_name, duration);

	// 뼈 시간줄.
	for bone_timeline_json in read_array(animation_json, "bone") {
		let Some(bone_timeline_object) = bone_timeline_json.as_object() else {
			continue;
		};
		let Some(bone_index) = skeleton_data.find_bone_index(read_string(bone_timeline_object, "name", "")) else {
			continue;
		};
		let translate_frame_list = read_array(bone_timeline_object, "translateFrame");
		if !translate_frame_list.is_empty() {
			let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Translate, Some(bone_index));
			let frame_times = read_frame_times(translate_frame_list, frame_rate);
			for (frame_json, frame_time) in translate_frame_list.iter().zip(frame_times.iter()) {
				let frame_object = read_frame_object(frame_json);
				let (curve_type, curve_values) = read_frame_curve(frame_object);
				timeline.add_value_frame(*frame_time, &[read_number(frame_object, "x", 0.0), read_number(frame_object, "y", 0.0)], curve_type, curve_values);
			}
			animation.add_timeline(timeline);
		}
		let rotate_frame_list = read_array(bone_timeline_object, "rotateFrame");
		if !rotate_frame_list.is_empty() {
			let mut rotate_timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Rotate, Some(bone_index));
			let mut shear_timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Shear, Some(bone_index));
			let frame_times = read_frame_times(rotate_frame_list, frame_rate);
			let mut previous_rotation = 0.0f32;
			let mut previous_clockwise = 0.0f32;
			let mut has_skew = false;
			for (frame_index, (frame_json, frame_time)) in rotate_frame_list.iter().zip(frame_times.iter()).enumerate() {
				let frame_object = read_frame_object(frame_json);
				let (curve_type, curve_values) = read_frame_curve(frame_object);
				let mut rotation = read_number(frame_object, "rotate", 0.0);
				if frame_index > 0 {
					rotation = unwrap_rotation(rotation, previous_rotation, &mut previous_clockwise);
				}
				previous_clockwise = read_number(frame_object, "clockwise", 0.0);
				previous_rotation = rotation;
				let skew = read_number(frame_object, "skew", 0.0);
				if skew != 0.0 {
					has_skew = true;
				}
				rotate_timeline.add_value_frame(*frame_time, &[rotation], curve_type, curve_values);
				shear_timeline.add_value_frame(*frame_time, &[0.0, skew], curve_type, curve_values);
			}
			animation.add_timeline(rotate_timeline);
			if has_skew {
				animation.add_timeline(shear_timeline);
			}
		}
		let scale_frame_list = read_array(bone_timeline_object, "scaleFrame");
		if !scale_frame_list.is_empty() {
			let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Scale, Some(bone_index));
			let frame_times = read_frame_times(scale_frame_list, frame_rate);
			for (frame_json, frame_time) in scale_frame_list.iter().zip(frame_times.iter()) {
				let frame_object = read_frame_object(frame_json);
				let (curve_type, curve_values) = read_frame_curve(frame_object);
				timeline.add_value_frame(*frame_time, &[read_number(frame_object, "x", 1.0), read_number(frame_object, "y", 1.0)], curve_type, curve_values);
			}
			animation.add_timeline(timeline);
		}
	}

	// 슬롯 시간줄.
	for slot_timeline_json in read_array(animation_json, "slot") {
		let Some(slot_timeline_object) = slot_timeline_json.as_object() else {
			continue;
		};
		let Some(slot_index) = skeleton_data.find_slot_index(read_string(slot_timeline_object, "name", "")) else {
			continue;
		};
		let display_frame_list = read_array(slot_timeline_object, "displayFrame");
		if !display_frame_list.is_empty() {
			let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Attachment, Some(slot_index));
			let frame_times = read_frame_times(display_frame_list, frame_rate);
			let display_names = &slot_display_names[slot_index];
			for (frame_json, frame_time) in display_frame_list.iter().zip(frame_times.iter()) {
				let frame_object = read_frame_object(frame_json);
				let display_index = if frame_object.contains_key("value") { read_number(frame_object, "value", 0.0) } else { read_number(frame_object, "displayIndex", 0.0) };
				let attachment_name = if display_index >= 0.0 && (display_index as usize) < display_names.len() { display_names[display_index as usize].as_deref() } else { None };
				timeline.add_attachment_frame(*frame_time, attachment_name);
			}
			animation.add_timeline(timeline);
		}
		let color_frame_list = read_array(slot_timeline_object, "colorFrame");
		if !color_frame_list.is_empty() {
			let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Color, Some(slot_index));
			let frame_times = read_frame_times(color_frame_list, frame_rate);
			for (frame_json, frame_time) in color_frame_list.iter().zip(frame_times.iter()) {
				let frame_object = read_frame_object(frame_json);
				let (curve_type, curve_values) = read_frame_curve(frame_object);
				let color_json = if frame_object.contains_key("value") { frame_object.get("value") } else { frame_object.get("color") };
				let color = read_color_transform(color_json);
				timeline.add_value_frame(*frame_time, &[color.red, color.green, color.blue, color.alpha], curve_type, curve_values);
			}
			animation.add_timeline(timeline);
		}
	}

	// 그리기 차례.
	if let Some(z_order_object) = animation_json.get("zOrder").and_then(|value| value.as_object()) {
		let z_order_frame_list = read_array(z_order_object, "frame");
		if !z_order_frame_list.is_empty() {
			let slot_count = skeleton_data.get_slots().len();
			let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::DrawOrder, None);
			let frame_times = read_frame_times(z_order_frame_list, frame_rate);
			for (frame_json, frame_time) in z_order_frame_list.iter().zip(frame_times.iter()) {
				let frame_object = read_frame_object(frame_json);
				let z_order_values = read_number_array(frame_object, "zOrder");
				let draw_order = read_z_order(&z_order_values, slot_count, &place)?;
				timeline.add_draw_order_frame(*frame_time, draw_order);
			}
			animation.add_timeline(timeline);
		}
	}

	// 사건. (frame 의 event 글, events 의 name, ints, floats, strings 첫 값)
	let event_frame_list = read_array(animation_json, "frame");
	if !event_frame_list.is_empty() {
		let mut timeline = Skeleton2DTimeline::new(Skeleton2DTimelineType::Event, None);
		let frame_times = read_frame_times(event_frame_list, frame_rate);
		for (frame_json, frame_time) in event_frame_list.iter().zip(frame_times.iter()) {
			let frame_object = read_frame_object(frame_json);
			if let Some(event_name) = frame_object.get("event").and_then(|value| value.as_str()) {
				timeline.add_event_frame(Skeleton2DEvent::new(event_name, *frame_time, 0, 0.0, ""));
			}
			for event_json in read_array(frame_object, "events") {
				let Some(event_object) = event_json.as_object() else {
					continue;
				};
				let name = read_string(event_object, "name", "");
				let int_value = read_array(event_object, "ints").first().and_then(|value| value.as_f64()).unwrap_or(0.0) as i32;
				let float_value = read_array(event_object, "floats").first().and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
				let string_value = read_array(event_object, "strings").first().and_then(|value| value.as_str()).unwrap_or("");
				timeline.add_event_frame(Skeleton2DEvent::new(name, *frame_time, int_value, float_value, string_value));
			}
		}
		if timeline.get_frame_count() > 0 {
			animation.add_timeline(timeline);
		}
	}
	Ok(animation)
}

//==============================================================================
/// DragonBones 섞기 이름을 엔진 섞기로. (없는 것은 Normal)
//==============================================================================
fn read_blend_mode(blend_mode_name: &str) -> BlendMode {
	match blend_mode_name {
		"add" => {
			BlendMode::Lighter
		}
		"darken" => {
			BlendMode::Darken
		}
		"difference" => {
			BlendMode::Difference
		}
		"hardlight" => {
			BlendMode::HardLight
		}
		"lighten" => {
			BlendMode::Lighten
		}
		"multiply" => {
			BlendMode::Multiply
		}
		"overlay" => {
			BlendMode::Overlay
		}
		"screen" => {
			BlendMode::Screen
		}
		_ => {
			BlendMode::Normal
		}
	}
}

//==============================================================================
/// DragonBones 5.x JSON 읽기.
///
/// # 인자
/// - `json`: DragonBones JSON 값.
/// - `armature_name`: 읽을 뼈대 이름. (None 이면 첫 뼈대)
///
/// # 돌려줌
/// 지은 자료. 꼴이 틀리면 Err.
//==============================================================================
pub(crate) fn read_dragon_bones_json(json: &Value, armature_name: Option<&str>) -> Result<Skeleton2DData, String> {
	let json_object = match json.as_object() {
		Some(json_object) => {
			json_object
		}
		None => {
			return Err(create_read_error("JSON 이 객체가 아니다."));
		}
	};
	let armature_json = read_array(json_object, "armature").iter().filter_map(|candidate_json| candidate_json.as_object()).find(|candidate_object| {
		match armature_name {
			Some(armature_name) => {
				read_string(candidate_object, "name", "") == armature_name
			}
			None => {
				true
			}
		}
	});
	let armature_json = match armature_json {
		Some(armature_json) => {
			armature_json
		}
		None => {
			return Err(create_read_error(&format!("뼈대 \"{}\" 가 없다.", armature_name.unwrap_or("null"))));
		}
	};
	let root_frame_rate = read_number(json_object, "frameRate", 24.0);
	let mut frame_rate = read_number(armature_json, "frameRate", root_frame_rate);
	if frame_rate <= 0.0 {
		frame_rate = 24.0;
	}
	let mut skeleton_data = Skeleton2DData::new(read_string(armature_json, "name", ""));
	skeleton_data.set_frame_rate(frame_rate);

	// 뼈.
	for bone_json in read_array(armature_json, "bone") {
		let bone_object = match bone_json.as_object() {
			Some(bone_object) => {
				bone_object
			}
			None => {
				return Err(create_read_error("뼈가 객체가 아니다."));
			}
		};
		let bone_name = read_string(bone_object, "name", "");
		let parent_name = read_string(bone_object, "parent", "");
		let parent_index = if parent_name.is_empty() {
			None
		}
		else {
			match skeleton_data.find_bone_index(parent_name) {
				Some(parent_index) => {
					Some(parent_index)
				}
				None => {
					return Err(create_read_error(&format!("뼈 \"{}\" 의 부모 \"{}\" 가 앞에 없다.", bone_name, parent_name)));
				}
			}
		};
		let mut bone_data = Skeleton2DBoneData::new(skeleton_data.get_bones().len(), bone_name, parent_index);
		let transform = read_transform(bone_object.get("transform"));
		bone_data.set_transform(transform.x, transform.y, transform.rotation, transform.scale_x, transform.scale_y, 0.0, transform.shear_y);
		bone_data.set_length(read_number(bone_object, "length", 0.0));
		skeleton_data.add_bone(bone_data);
	}

	// 슬롯. (처음 붙임은 스킨을 읽은 뒤 displayIndex 로 정하고 그 뒤에 자료에 넣는다)
	let mut slot_data_list: Vec<Skeleton2DSlotData> = Vec::new();
	let mut display_index_list: Vec<f32> = Vec::new();
	for slot_json in read_array(armature_json, "slot") {
		let slot_object = match slot_json.as_object() {
			Some(slot_object) => {
				slot_object
			}
			None => {
				return Err(create_read_error("슬롯이 객체가 아니다."));
			}
		};
		let slot_name = read_string(slot_object, "name", "");
		let bone_name = read_string(slot_object, "parent", "");
		let bone_index = match skeleton_data.find_bone_index(bone_name) {
			Some(bone_index) => {
				bone_index
			}
			None => {
				return Err(create_read_error(&format!("슬롯 \"{}\" 의 뼈 \"{}\" 가 없다.", slot_name, bone_name)));
			}
		};
		let mut slot_data = Skeleton2DSlotData::new(slot_data_list.len(), slot_name, bone_index);
		slot_data.set_color(read_color_transform(slot_object.get("color")));
		slot_data.set_blend_mode(read_blend_mode(read_string(slot_object, "blendMode", "normal")));
		slot_data_list.push(slot_data);
		display_index_list.push(read_number(slot_object, "displayIndex", 0.0));
	}
	let find_slot_index = |slot_name: &str| -> Option<usize> { slot_data_list.iter().position(|slot_data| slot_data.get_name() == slot_name) };

	// 스킨. (슬롯마다 display 목록, 이름 없는 스킨은 "default")
	let mut slot_display_names: Vec<Vec<Option<String>>> = vec![Vec::new(); slot_data_list.len()];
	let mut skins: Vec<Skeleton2DSkin> = Vec::new();
	for skin_json in read_array(armature_json, "skin") {
		let Some(skin_object) = skin_json.as_object() else {
			continue;
		};
		let mut skin_name = read_string(skin_object, "name", "default");
		if skin_name.is_empty() {
			skin_name = "default";
		}
		let mut skin = Skeleton2DSkin::new(skin_name);
		for skin_slot_json in read_array(skin_object, "slot") {
			let Some(skin_slot_object) = skin_slot_json.as_object() else {
				continue;
			};
			let Some(slot_index) = find_slot_index(read_string(skin_slot_object, "name", "")) else {
				continue;
			};
			let display_json_list = read_array(skin_slot_object, "display");
			let display_names = read_display_names(display_json_list);
			for (display_json, attachment_name) in display_json_list.iter().zip(display_names.iter()) {
				let Some(attachment_name) = attachment_name else {
					continue;
				};
				let display_object = read_frame_object(display_json);
				let display_type = read_string(display_object, "type", "image");
				let display_name = read_string(display_object, "name", "");
				let frame_name = read_string(display_object, "path", display_name);
				if display_type == "image" {
					let mut region_attachment = Skeleton2DRegionAttachment::new(attachment_name, frame_name);
					let transform = read_transform(display_object.get("transform"));
					region_attachment.set_transform(transform.x, transform.y, transform.rotation, transform.scale_x, transform.scale_y);
					if let Some(pivot_object) = display_object.get("pivot").and_then(|value| value.as_object()) {
						region_attachment.set_pivot(read_number(pivot_object, "x", 0.0), read_number(pivot_object, "y", 0.0));
					}
					skin.set_attachment(slot_index, attachment_name, Skeleton2DAttachment::Region(region_attachment));
				}
				else {
					let mesh_attachment = read_mesh_display(&skeleton_data, attachment_name, frame_name, display_object)?;
					skin.set_attachment(slot_index, attachment_name, Skeleton2DAttachment::Mesh(mesh_attachment));
				}
			}
			if skin_name == "default" || slot_display_names[slot_index].is_empty() {
				slot_display_names[slot_index] = display_names;
			}
		}
		skins.push(skin);
	}
	for (slot_index, mut slot_data) in slot_data_list.into_iter().enumerate() {
		let display_value = display_index_list[slot_index];
		let display_names = &slot_display_names[slot_index];
		let attachment_name = if display_value >= 0.0 && (display_value as usize) < display_names.len() { display_names[display_value as usize].as_deref() } else { None };
		slot_data.set_attachment_name(attachment_name);
		skeleton_data.add_slot(slot_data);
	}
	for skin in skins {
		skeleton_data.add_skin(skin);
	}

	// IK. (chain 이 0 보다 크고 부모가 있으면 부모와 그 뼈 둘)
	for ik_json in read_array(armature_json, "ik") {
		let Some(ik_object) = ik_json.as_object() else {
			continue;
		};
		let ik_name = read_string(ik_object, "name", "");
		let bone_index = skeleton_data.find_bone_index(read_string(ik_object, "bone", ""));
		let target_index = skeleton_data.find_bone_index(read_string(ik_object, "target", ""));
		let (Some(bone_index), Some(target_index)) = (bone_index, target_index) else {
			return Err(create_read_error(&format!("IK \"{}\" 의 뼈나 목표가 없다.", ik_name)));
		};
		let chain = read_number(ik_object, "chain", 0.0);
		let bone_indices = match skeleton_data.get_bones()[bone_index].get_parent_index() {
			Some(parent_index) if chain > 0.0 => {
				vec![parent_index, bone_index]
			}
			_ => {
				vec![bone_index]
			}
		};
		let mut ik_constraint_data = Skeleton2DIkConstraintData::new(skeleton_data.get_ik_constraints().len(), ik_name, bone_indices, target_index);
		ik_constraint_data.set_bend_positive(ik_object.get("bendPositive").and_then(|value| value.as_bool()) != Some(false));
		ik_constraint_data.set_mix(read_number(ik_object, "weight", 1.0));
		skeleton_data.add_ik_constraint(ik_constraint_data);
	}

	// 클립.
	let mut animations = Vec::new();
	for animation_json in read_array(armature_json, "animation") {
		let Some(animation_object) = animation_json.as_object() else {
			continue;
		};
		animations.push(read_animation(&skeleton_data, animation_object, frame_rate, &slot_display_names)?);
	}
	for animation in animations {
		skeleton_data.add_animation(animation);
	}
	Ok(skeleton_data)
}

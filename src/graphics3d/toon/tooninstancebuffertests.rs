//==============================================================================
//! 툰 인스턴스 버퍼 길 시험. (원본 vanilla.js tests/graphics3d/tooninstancebufferchrome.test.mjs 와 같은 검사, 같은 장면 toonminerscene.mjs)
//! - 숨긴 GL 컨텍스트(창을 띄우지 않는다)에서 광부 mid 꼴 장면을 240 x 160 프레임버퍼에 두 길로 그린다.
//!   array 는 ToonDrawQueue::add_mesh_instanced, buffer 는 add_mesh_instance_buffer. 몸, 바깥선 껍데기, 해 그림자(OnChange), 선의 판(인스턴스마다 다른 선 번호, set_line_color 색)을 켠다.
//! - 프레임마다 두 길의 화소가 바이트까지 같은지 본다. 바위 하나를 옮기고 틴트와 선 번호를 바꾼 뒤(buffer 는 update_instance_buffer_range 하나) 다음 프레임에 두 길 모두 바뀌고 같은지 본다.
//! - 그림자 캐시: 구워진 뒤 그대로인 프레임은 그림자 깊이를 그리지 않고, 버퍼를 고친 프레임은 다시 굽는다. (버퍼의 generation)
//! - 올림 수와 드로 콜 수를 세어 buffer 길이 프레임마다 인스턴스를 올리지 않는지 본다.
//! - 원본의 지운 버퍼로 그리기 시험은 러스트 소유권이 막으므로 없다. (delete_instance_buffer 는 버퍼를 가져간다)
//==============================================================================
use super::toondrawqueue::ToonDrawQueue;
use super::toonrenderer::{ToonDrawOptions, ToonInstance, ToonInstanceBuffer, ToonMesh, ToonRenderer, DISABLE_FEATURE_VARIANTS, INSTANCE_UPLOAD_COUNT, UPLOAD_COUNTS};
use super::toonscreenoutline::{ToonOutlineWidthUnit, ToonScreenOutline};
use crate::base::matrix4::Matrix4;
use crate::core::glcontext::GlContext;
use crate::graphics3d::camera3d::Camera3D;
use crate::graphics3d::shadowmap::{ShadowMap, ShadowUpdateMode};
use crate::{create_box_geometry, create_cylinder_geometry, create_faceted_chunk_geometry, create_octahedron_geometry, create_rounded_box_geometry, MeshBuilder, ToonBufferUsage, ToonPartOptions, Vector3};

const WIDTH: i32 = 240;
const HEIGHT: i32 = 160;
const GRID_WIDTH: usize = 24;
const GRID_DEPTH: usize = 20;
const BUILDING_COUNT: usize = 20;
const ROCK_KIND_COUNT: usize = 3;

//==============================================================================
/// 시험 판. (RGBA8 색, 깊이 24, 창 없는 컨텍스트)
//==============================================================================
struct Target<'a> {
	gl: &'a GlContext,
	raw: &'a glow::Context,
	framebuffer: glow::Framebuffer,
}

fn create_target<'a>(gl: &'a GlContext) -> Target<'a> {
	use glow::HasContext;
	let raw: &glow::Context = gl.get_glow_context_unchecked();
	let framebuffer = unsafe {
		let color_texture = crate::core::glstatecache::create_texture(gl).unwrap();
		crate::core::glstatecache::bind_texture(gl, glow::TEXTURE_2D, Some(color_texture));
		raw.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, WIDTH, HEIGHT, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
		let depth = raw.create_renderbuffer().unwrap();
		raw.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
		raw.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, WIDTH, HEIGHT);
		let framebuffer = crate::core::glstatecache::create_framebuffer(gl).unwrap();
		crate::core::glstatecache::bind_framebuffer(gl, glow::FRAMEBUFFER, Some(framebuffer));
		raw.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color_texture), 0);
		raw.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth));
		assert_eq!(raw.check_framebuffer_status(glow::FRAMEBUFFER), glow::FRAMEBUFFER_COMPLETE);
		framebuffer
	};
	Target { gl, raw, framebuffer }
}

impl Target<'_> {
	fn clear(&self) {
		use glow::HasContext;
		unsafe {
			crate::core::glstatecache::bind_framebuffer(self.gl, glow::FRAMEBUFFER, Some(self.framebuffer));
			crate::core::glstatecache::viewport(self.gl, 0, 0, WIDTH, HEIGHT);
			self.raw.clear_color(0.42, 0.55, 0.62, 1.0);
			self.raw.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
		}
	}

	fn read_pixels(&self) -> Vec<u8> {
		use glow::HasContext;
		let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
		unsafe {
			crate::core::glstatecache::bind_framebuffer(self.gl, glow::FRAMEBUFFER, Some(self.framebuffer));
			self.raw.read_pixels(0, 0, WIDTH, HEIGHT, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
		}
		pixels
	}
}

unsafe fn glow_context_error(glow_context: &glow::Context) -> u32 {
	use glow::HasContext;
	unsafe { glow_context.get_error() }
}

fn create_mesh(gl: &GlContext, renderer: &mut ToonRenderer, geometry: &crate::ToonGeometry, matrix: &Matrix4, color: &str, has_outline: bool) -> ToonMesh {
	let mut builder = MeshBuilder::new();
	builder.add_part(geometry, matrix, color, ToonPartOptions { has_outline, ..ToonPartOptions::default() });
	renderer.create_mesh(gl, &builder.read_arrays()).expect("모형")
}

//==============================================================================
/// 한 프레임의 결과. (화소, 올림 수, 그림자 드로 콜 수, 드로 콜 수)
//==============================================================================
struct FrameResult {
	pixels: Vec<u8>,
	upload_count: u32,
	shadow_draw_count: u32,
	draw_call_count: u32,
	changed_pixel_count: usize,
	blue_line_count: usize,
}

//==============================================================================
/// 바위 하나의 값. (원본 장면의 인스턴스 객체)
//==============================================================================
#[derive(Clone)]
struct Rock {
	model_matrix: Matrix4,
	tint_color: [f64; 4],
	line_id: u32,
	emissive_color: Option<[f64; 3]>,
	emissive_intensity: f64,
}

//==============================================================================
/// 바위 값을 인스턴스 버퍼 한 칸(수 21 개)으로. (원본 writeInstance)
//==============================================================================
fn write_instance(rock: &Rock) -> Vec<f32> {
	let mut data = rock.model_matrix.elements.to_vec();
	let emissive = rock.emissive_color.unwrap_or([0.0; 3]);
	data[3] = emissive[0] as f32;
	data[7] = emissive[1] as f32;
	data[11] = emissive[2] as f32;
	data[15] = if rock.emissive_color.is_some() { rock.emissive_intensity as f32 } else { 1.0 };
	data.extend(rock.tint_color.iter().map(|&value| value as f32));
	data.push(rock.line_id as f32);
	data
}

fn count_blue_line(pixels: &[u8]) -> usize {
	pixels.chunks(4).filter(|pixel| pixel[2] as i32 > pixel[0] as i32 + 90 && pixel[2] > 110).count()
}

fn count_different(first: &[u8], second: &[u8]) -> usize {
	first.chunks(4).zip(second.chunks(4)).filter(|(first_pixel, second_pixel)| first_pixel != second_pixel).count()
}

fn read_upload_total() -> u32 {
	UPLOAD_COUNTS.with(|counts| {
		let list = counts.get();
		list[0] + list[3]
	}) + INSTANCE_UPLOAD_COUNT.with(|count| count.get())
}

//==============================================================================
/// 광부 mid 꼴 장면을 한 길로 frame_count 프레임 그리기. (원본 createMinerScene 의 장면, edit_frame 에서 바위 하나를 고친다)
/// - 돌려줌: 프레임마다의 결과.
//==============================================================================
fn run_scene(gl: &GlContext, target: &Target, is_buffer: bool, frame_count: usize, edit_frame: usize, chunk_count: usize) -> Vec<FrameResult> {
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let ground_mesh = create_mesh(gl, &mut renderer, &create_box_geometry(GRID_WIDTH as f64 + 2.0, 0.2, GRID_DEPTH as f64 + 2.0), &Matrix4::create_translation(0.0, -0.1, 0.0), "#8a7a64", false);
	let rock_mesh_list = [
		create_mesh(gl, &mut renderer, &create_octahedron_geometry(0.32), &Matrix4::create_translation(0.0, 0.26, 0.0), "#9a948c", true),
		create_mesh(gl, &mut renderer, &create_faceted_chunk_geometry(0.34, &[1.0, 0.8, 1.1], [1.0, 1.0, 1.0]), &Matrix4::create_translation(0.0, 0.24, 0.0), "#b08a60", true),
		create_mesh(gl, &mut renderer, &create_rounded_box_geometry(0.5, 0.4, 0.5, 2, 0.08), &Matrix4::create_translation(0.0, 0.2, 0.0), "#7c8a96", true),
	];
	let building_mesh_list = [
		create_mesh(gl, &mut renderer, &create_rounded_box_geometry(1.4, 1.2, 1.2, 3, 0.12), &Matrix4::create_translation(0.0, 0.6, 0.0), "#d8c8a8", true),
		create_mesh(gl, &mut renderer, &create_cylinder_geometry(0.5, 0.6, 1.6, 12), &Matrix4::create_translation(0.0, 0.8, 0.0), "#a8b8c8", true),
	];
	// 건물과 바위 칸. (칸마다 선 번호, 건물이 놓인 칸은 비운다)
	let mut building_cell_list: Vec<usize> = Vec::new();
	let mut building_list: Vec<(usize, Matrix4, u32)> = Vec::new();
	for index in 0..BUILDING_COUNT {
		let cell_x = (index * 7) % GRID_WIDTH;
		let cell_z = (index * 5 + index / 4) % GRID_DEPTH;
		building_cell_list.push(cell_z * GRID_WIDTH + cell_x);
		building_list.push((index % building_mesh_list.len(), Matrix4::create_translation(cell_x as f64 - GRID_WIDTH as f64 / 2.0 + 0.5, 0.0, cell_z as f64 - GRID_DEPTH as f64 / 2.0 + 0.5), 2000 + index as u32));
	}
	let batch_count = ROCK_KIND_COUNT * chunk_count;
	let mut rock_list: Vec<Vec<Rock>> = vec![Vec::new(); batch_count];
	for cell_z in 0..GRID_DEPTH {
		for cell_x in 0..GRID_WIDTH {
			let cell_index = cell_z * GRID_WIDTH + cell_x;
			if building_cell_list.contains(&cell_index) {
				continue;
			}
			let kind = (cell_x * 3 + cell_z * 5) % ROCK_KIND_COUNT;
			let turn = ((cell_x * 13 + cell_z * 7) % 16) as f64 / 16.0 * std::f64::consts::PI * 2.0;
			let scale = 0.85 + ((cell_x * 5 + cell_z * 11) % 7) as f64 * 0.05;
			let mut model_matrix = Matrix4::create_translation(cell_x as f64 - GRID_WIDTH as f64 / 2.0 + 0.5, 0.0, cell_z as f64 - GRID_DEPTH as f64 / 2.0 + 0.5);
			model_matrix.multiply(&Matrix4::create_rotation_y(turn));
			model_matrix.multiply(&Matrix4::create_scale(scale, scale, scale));
			let shade = 0.8 + ((cell_x + cell_z) % 5) as f64 * 0.05;
			let mut rock = Rock { model_matrix, tint_color: [shade, shade, shade * 0.95, 1.0], line_id: 1 + cell_index as u32, emissive_color: None, emissive_intensity: 1.0 };
			if cell_index % 37 == 0 {
				rock.emissive_color = Some([1.0, 0.8, 0.3]);
				rock.emissive_intensity = 0.4;
			}
			let chunk = (chunk_count - 1).min(cell_x * chunk_count / GRID_WIDTH);
			rock_list[chunk * ROCK_KIND_COUNT + kind].push(rock);
		}
	}
	// 인스턴스 버퍼. (행렬 16, 틴트 4, 선 번호 1, draw_mesh_instanced 가 올리는 꼴과 같은 수)
	let instance_layout = renderer.create_instance_layout(&[("instanceMatrix", 16), ("instanceTint", 4), ("instanceLineId", 1)]).expect("꼴");
	let instance_list: Vec<Vec<ToonInstance>> = rock_list.iter().map(|batch| batch.iter().map(|rock| ToonInstance { model_matrix: rock.model_matrix, tint_color: rock.tint_color, line_id: Some(rock.line_id), emissive_color: rock.emissive_color, emissive_intensity: rock.emissive_intensity }).collect()).collect();
	let mut instance_list = instance_list;
	let buffer_list: Vec<Option<ToonInstanceBuffer>> = rock_list.iter().map(|batch| {
		if !is_buffer {
			return None;
		}
		let data: Vec<f32> = batch.iter().flat_map(write_instance).collect();
		Some(renderer.create_instance_buffer(gl, &instance_layout, &data, ToonBufferUsage::Dynamic).expect("인스턴스 버퍼"))
	}).collect();
	// 바깥선, 그림자, 카메라.
	let mut outline = ToonScreenOutline::new(gl).expect("화면 바깥선");
	outline.set_width(2.0, ToonOutlineWidthUnit::Pixel);
	outline.set_crease(40.0, 1.0, 0.8);
	for index in (0..BUILDING_COUNT).step_by(3) {
		outline.set_line_color(2000 + index as u32, [0.55, 0.1, 0.05, 1.0]);
	}
	for line_id in (1..(GRID_WIDTH * GRID_DEPTH) as u32).step_by(29) {
		outline.set_line_color(line_id, [0.05, 0.2, 0.6, 1.0]);
	}
	let mut shadow_map = ShadowMap::new(gl, 1024).expect("그림자 판");
	shadow_map.set_update_mode(ShadowUpdateMode::OnChange, 1);
	shadow_map.update_light_matrix(Vector3::create(0.0, 0.0, 0.0), Vector3::create(-0.45, -1.0, -0.35), 16.0, 0.1, 40.0);
	renderer.set_shadow(Some(&shadow_map), 0.6);
	let mut camera = Camera3D::new();
	camera.set_perspective(0.75, 0.1, 80.0);
	camera.set_aspect_ratio(WIDTH as f64 / HEIGHT as f64);
	camera.look_at(Vector3::create(0.0, 13.0, 13.0), Vector3::create(0.0, 0.0, 0.5), Vector3::create(0.0, 1.0, 0.0));

	let mut result_list: Vec<FrameResult> = Vec::new();
	let mut previous_pixels: Option<Vec<u8>> = None;
	for frame in 0..frame_count {
		let upload_before = read_upload_total();
		if frame == edit_frame {
			// 바위 하나를 옮기고 틴트와 선 번호를 바꾼다. (선 번호는 색 표의 파란 번호로, 원본 setRock(4, 12))
			let batch = 4.min(batch_count - 1);
			let rock = &mut rock_list[batch][12];
			let elements = rock.model_matrix.elements;
			rock.model_matrix = Matrix4::create_translation(elements[12] as f64 + 0.45, 0.5, elements[14] as f64);
			rock.tint_color = [1.0, 0.35, 0.3, 1.0];
			rock.line_id = 30;
			instance_list[batch][12] = ToonInstance { model_matrix: rock.model_matrix, tint_color: rock.tint_color, line_id: Some(rock.line_id), emissive_color: rock.emissive_color, emissive_intensity: rock.emissive_intensity };
			if let Some(instance_buffer) = buffer_list[batch].as_ref() {
				renderer.update_instance_buffer_range(gl, instance_buffer, 12, &write_instance(rock)).expect("일부 고치기");
			}
		}
		let mut queue = ToonDrawQueue::new();
		queue.set_screen_outline(Some(&mut outline));
		queue.add_mesh(&ground_mesh, &Matrix4::create_identity(), ToonDrawOptions { line_id: Some(1), has_screen_outline: false, has_crease_line: false, ..ToonDrawOptions::default() });
		for batch in 0..batch_count {
			let rock_mesh = &rock_mesh_list[batch % ROCK_KIND_COUNT];
			let options = ToonDrawOptions { specular_scale: 0.0, ..ToonDrawOptions::default() };
			match buffer_list[batch].as_ref() {
				Some(instance_buffer) => {
					queue.add_mesh_instance_buffer(rock_mesh, instance_buffer, rock_list[batch].len(), options);
				}
				None => {
					queue.add_mesh_instanced(rock_mesh, &instance_list[batch], options);
				}
			}
		}
		for (mesh_index, model_matrix, line_id) in building_list.iter() {
			queue.add_mesh(&building_mesh_list[*mesh_index], model_matrix, ToonDrawOptions { line_id: Some(*line_id), ..ToonDrawOptions::default() });
		}
		let shadow_before = renderer.get_frame_statistics().draw_call_count;
		queue.draw_shadows(gl, &mut renderer, &shadow_map);
		let shadow_draw_count = renderer.get_frame_statistics().draw_call_count - shadow_before;
		target.clear();
		queue.draw(gl, &mut renderer, &camera);
		let draw_call_count = renderer.get_frame_statistics().draw_call_count;
		let pixels = target.read_pixels();
		let changed_pixel_count = previous_pixels.as_ref().map_or(0, |previous| count_different(previous, &pixels));
		previous_pixels = Some(pixels.clone());
		result_list.push(FrameResult { blue_line_count: count_blue_line(&pixels), pixels, upload_count: read_upload_total() - upload_before, shadow_draw_count, draw_call_count, changed_pixel_count });
		drop(queue);
	}
	for instance_buffer in buffer_list.into_iter().flatten() {
		renderer.delete_instance_buffer(gl, instance_buffer);
	}
	result_list
}

fn create_context() -> std::rc::Rc<glow::Context> {
	let Some(glow_context) = crate::core::glstatecache::tests::create_surfaceless_context() else {
		panic!("창 없는 GL 컨텍스트를 짓지 못함");
	};
	let _ = crate::core::glstatecache::attach(&glow_context);
	glow_context
}

//==============================================================================
/// 두 길의 화소가 프레임마다 바이트까지 같고, 버퍼를 고친 프레임이 바로 반영되고, 올림이 줄고, 그림자 캐시가 generation 으로 고친 것을 안다. (원본 tooninstancebufferchrome.test.mjs)
//==============================================================================
#[test]
fn instance_buffer_path_matches_array_path() {
	let glow_context = create_context();
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let array_frames = run_scene(gl, &target, false, 75, 40, 2);
	let buffer_frames = run_scene(gl, &target, true, 75, 40, 2);
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
	eprintln!("툰 인스턴스 버퍼: 프레임 0 올림 array {} / buffer {}, 프레임 40 올림 array {} / buffer {}, 프레임 50 올림 array {} / buffer {}", array_frames[0].upload_count, buffer_frames[0].upload_count, array_frames[40].upload_count, buffer_frames[40].upload_count, array_frames[50].upload_count, buffer_frames[50].upload_count);
	eprintln!("툰 인스턴스 버퍼: 드로 콜 프레임 0 array {} / buffer {}, 프레임 50 array {} / buffer {}", array_frames[0].draw_call_count, buffer_frames[0].draw_call_count, array_frames[50].draw_call_count, buffer_frames[50].draw_call_count);
	for frame in 0..75 {
		let array_frame = &array_frames[frame];
		let buffer_frame = &buffer_frames[frame];
		assert_eq!(count_different(&array_frame.pixels, &buffer_frame.pixels), 0, "{} 프레임 두 길의 화소가 바이트까지 같음", frame);
		assert!(buffer_frame.blue_line_count > 20, "{} 프레임 set_line_color 의 파란 선이 보임 ({})", frame, buffer_frame.blue_line_count);
		assert!(buffer_frame.upload_count < array_frame.upload_count, "{} 프레임 buffer 길이 덜 올림 ({} < {})", frame, buffer_frame.upload_count, array_frame.upload_count);
		assert_eq!(array_frame.draw_call_count, buffer_frame.draw_call_count, "{} 프레임 드로 콜 같음", frame);
	}
	// 버퍼를 고친 프레임은 바로 반영된다. (두 길이 같고 앞 프레임과 다르다)
	assert!(buffer_frames[40].changed_pixel_count > 0, "버퍼를 고친 다음 프레임에 화면이 바뀜 ({})", buffer_frames[40].changed_pixel_count);
	assert_eq!(buffer_frames[40].upload_count, buffer_frames[39].upload_count + 1, "고친 프레임의 올림은 buffer_sub_data 하나만 더 ({} → {})", buffer_frames[39].upload_count, buffer_frames[40].upload_count);
	// 그림자 캐시: 첫 프레임은 전부, 구워진 뒤(30 프레임) 그대로면 그리지 않고, 고친 프레임은 다시 그린다.
	assert!(buffer_frames[0].shadow_draw_count > 0, "첫 프레임은 그림자를 그림");
	for frame in 32..40 {
		assert_eq!(buffer_frames[frame].shadow_draw_count, 0, "{} 프레임 buffer 길의 그림자 캐시가 그리지 않음", frame);
		assert_eq!(array_frames[frame].shadow_draw_count, 0, "{} 프레임 array 길의 그림자 캐시가 그리지 않음", frame);
	}
	assert!(buffer_frames[40].shadow_draw_count > 0, "버퍼를 고친 프레임에 그림자 캐시가 다시 그림 ({})", buffer_frames[40].shadow_draw_count);
	assert_eq!(buffer_frames[74].shadow_draw_count, 0, "다시 구운 뒤 그대로면 그리지 않음 ({})", buffer_frames[74].shadow_draw_count);
}

//==============================================================================
/// 기능 갈래 변형(몸 프로그램)과 함께 돌고, 변형을 끈 기본 프로그램과 화면이 같다. (인스턴스 꼴의 몸 프로그램, 선 번호 칸 선언이 들어간 꼴)
//==============================================================================
#[test]
fn instance_buffer_works_with_body_variants() {
	let glow_context = create_context();
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let with_variants = run_scene(gl, &target, true, 2, 1, 1);
	DISABLE_FEATURE_VARIANTS.with(|flag| flag.set(true));
	let without_variants = run_scene(gl, &target, true, 2, 1, 1);
	DISABLE_FEATURE_VARIANTS.with(|flag| flag.set(false));
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
	for frame in 0..2 {
		assert_eq!(count_different(&with_variants[frame].pixels, &without_variants[frame].pixels), 0, "{} 프레임 변형과 기본 프로그램의 화소가 같음", frame);
	}
	assert!(with_variants[0].pixels.chunks(4).any(|pixel| pixel[0] != 107), "무엇인가 그려졌다");
}

//==============================================================================
/// 버퍼 경계와 통째 거르기. (화면 밖 버퍼는 그리지 않고 센다, is_culled 가 false 면 그린다, 행렬 없는 꼴은 거르지 않는다, 범위를 넘는 고치기는 Err)
//==============================================================================
#[test]
fn instance_buffer_bounds_and_culling() {
	let glow_context = create_context();
	let gl: &GlContext = GlContext::from_context(&glow_context);
	let target = create_target(gl);
	let mut renderer = ToonRenderer::new(gl).expect("툰 렌더러");
	let mesh = create_mesh(gl, &mut renderer, &create_box_geometry(1.0, 1.0, 1.0), &Matrix4::create_identity(), "#c08060", true);
	let layout = renderer.create_instance_layout(&[("instanceMatrix", 16)]).unwrap();
	let mut data: Vec<f32> = Vec::new();
	for position in [-3.0, 3.0] {
		data.extend_from_slice(Matrix4::create_translation(position, 0.0, 0.0).get_elements());
	}
	let buffer = renderer.create_instance_buffer(gl, &layout, &data, ToonBufferUsage::Dynamic).unwrap();
	let bounds = buffer.get_bounds().expect("경계");
	assert_eq!(bounds.center, [0.0, 0.0, 0.0]);
	assert_eq!(bounds.radius, 3.0);
	assert_eq!(bounds.scale, 1.0);
	let sphere = super::toonrenderer::read_instance_buffer_sphere(&mesh, &buffer).expect("구");
	assert!(sphere[0] == 0.0 && sphere[3] >= 3.0 + 0.5, "구 반지름은 버퍼 반지름에 모형 도달 거리를 더한 것 ({})", sphere[3]);
	let view_projection = {
		let mut matrix = Matrix4::create_perspective(0.8, 1.0, 0.1, 50.0);
		matrix.multiply(&Matrix4::create_look_at(Vector3::create(0.0, 0.0, 5.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0)));
		matrix
	};
	let planes = super::toonrenderer::extract_frustum_planes(&view_projection);
	assert!(super::toonrenderer::is_sphere_in_frustum(&planes, &[0.0, 0.0, 0.0, 1.0]));
	assert!(!super::toonrenderer::is_sphere_in_frustum(&planes, &[0.0, 0.0, 20.0, 1.0]));
	// 고치기: 범위 안은 사본과 경계가 따라가고 generation 이 는다, 범위 밖은 Err 이고 그대로.
	assert_eq!(buffer.get_generation(), 0);
	let moved: Vec<f32> = Matrix4::create_translation(9.0, 0.0, 0.0).get_elements().to_vec();
	renderer.update_instance_buffer_range(gl, &buffer, 1, &moved).expect("범위 안");
	assert_eq!(buffer.get_generation(), 1);
	assert_eq!(buffer.get_bounds().unwrap().center, [3.0, 0.0, 0.0]);
	assert!(renderer.update_instance_buffer_range(gl, &buffer, 2, &moved).is_err(), "범위를 넘는 고치기는 Err");
	assert!(renderer.update_instance_buffer_range(gl, &buffer, 0, &moved[..10]).is_err(), "꼴 수 개수의 배수가 아니면 Err");
	assert_eq!(buffer.get_generation(), 1);
	renderer.update_instance_buffer_range(gl, &buffer, 1, &Matrix4::create_translation(3.0, 0.0, 0.0).get_elements().to_vec()).unwrap();
	// 카메라 뒤(z 20)로 옮기면 통째 거른다.
	let far_data: Vec<f32> = (0..2).flat_map(|_| Matrix4::create_translation(0.0, 0.0, 30.0).get_elements().to_vec()).collect();
	renderer.update_instance_buffer(gl, &buffer, &far_data);
	assert_eq!(buffer.get_generation(), 3);
	target.clear();
	renderer.begin_frame(gl, &view_projection);
	renderer.draw_mesh_instance_buffer(gl, &mesh, &buffer, 2, ToonDrawOptions::default());
	let culled = renderer.get_frame_statistics();
	renderer.end_frame(gl);
	assert_eq!((culled.drawn_object_count, culled.culled_object_count, culled.draw_call_count), (0, 2, 0), "화면 밖 버퍼는 통째 걸러 센다");
	renderer.begin_frame(gl, &view_projection);
	renderer.draw_mesh_instance_buffer(gl, &mesh, &buffer, 2, ToonDrawOptions { is_culled: false, ..ToonDrawOptions::default() });
	let forced = renderer.get_frame_statistics();
	renderer.end_frame(gl);
	assert!(forced.drawn_object_count == 2 && forced.draw_call_count > 0, "is_culled 가 false 면 거르지 않는다 ({}, {})", forced.drawn_object_count, forced.draw_call_count);
	// 행렬 없는 꼴은 경계가 없고 거르지 않는다.
	let point_layout = renderer.create_instance_layout(&[("instanceOffset", 3)]).unwrap();
	let point_buffer = renderer.create_instance_buffer(gl, &point_layout, &[0.0, 0.0, 30.0], ToonBufferUsage::Static).unwrap();
	assert!(point_buffer.get_bounds().is_none() && point_buffer.get_data().is_none());
	assert_eq!(unsafe { glow_context_error(&glow_context) }, 0, "GL 오류");
	renderer.delete_instance_buffer(gl, point_buffer);
	renderer.delete_instance_buffer(gl, buffer);
}

//==============================================================================
/// 인스턴스 꼴 몸, 깊이, 껍데기 정점 셰이더 글과 선의 판 정점 셰이더 글이 원본과 글자까지 같다. (원본 저장소를 node 로 불러 composeToonProgramSources 와 ToonScreenOutline.readProgramSources 결과를 견준다)
//==============================================================================
#[test]
fn instance_layout_shader_sources_match_original() {
	use crate::{ColorSpace, ToonScreenOutline};
	let path = std::env::var("VANILLA_JS_PATH").unwrap_or_else(|_| format!("{}/../vanilla.js", env!("CARGO_MANIFEST_DIR")));
	if !std::path::Path::new(&format!("{}/import.js", path)).exists() {
		println!("(원본 저장소가 없어 인스턴스 꼴 셰이더 글 견주기는 건너뜀)");
		return;
	}
	let probe = format!("{}/tests/assets/toonshaderprobe.mjs", env!("CARGO_MANIFEST_DIR"));
	let output = std::process::Command::new("node").arg(&probe).arg(&path).output().expect("node 를 부르지 못함");
	assert!(output.status.success(), "원본 셰이더 뽑기 실패: {}", String::from_utf8_lossy(&output.stderr));
	let text = String::from_utf8(output.stdout).unwrap();
	let read_field = |name: &str| -> String {
		let line = text.lines().find(|line| line.starts_with(&format!("{}\t", name))).unwrap_or_else(|| panic!("{} 줄 없음", name));
		serde_like_unescape(&line[name.len() + 1..])
	};
	let layout = crate::ToonInstanceLayout::new(&[("instanceMatrix", 16), ("instanceTint", 4), ("instanceLineId", 1), ("instanceShape", 2)]).unwrap();
	let sources = crate::compose_toon_program_sources(None, Some(&layout), false, ColorSpace::Gamma, &[]);
	assert_eq!(sources.solid_vertex_shader, read_field("solid"), "몸 정점 셰이더 글");
	assert_eq!(sources.outline_vertex_shader, read_field("outline"), "껍데기 정점 셰이더 글");
	assert_eq!(sources.depth_vertex_shader, read_field("depth"), "깊이 정점 셰이더 글");
	let line_sources = ToonScreenOutline::read_program_sources(ColorSpace::Gamma);
	assert_eq!(line_sources[0].0, read_field("line"), "선의 판 정점 셰이더 글");
	assert!(sources.solid_vertex_shader.contains("in float instanceLineId;\nin vec2 instanceShape;\n"));
}

//==============================================================================
/// 탭과 줄바꿈을 \t, \n, \\ 로 적은 한 줄 풀기. (셰이더 뽑기 도구의 줄 꼴)
//==============================================================================
fn serde_like_unescape(text: &str) -> String {
	let mut result = String::new();
	let mut characters = text.chars();
	while let Some(character) = characters.next() {
		if character != '\\' {
			result.push(character);
			continue;
		}
		match characters.next() {
			Some('n') => result.push('\n'),
			Some('t') => result.push('\t'),
			Some('\\') => result.push('\\'),
			Some(other) => {
				result.push('\\');
				result.push(other);
			}
			None => result.push('\\'),
		}
	}
	result
}

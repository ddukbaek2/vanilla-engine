//==============================================================================
//! 이미지 텍스처 캐시.
//! - 원본은 `WeakMap<image, texture>` 로 원본 이미지가 수거되면 텍스처도 같이 사라진다.
//! - 여기서는 `Weak<Image>` 를 들고, sweep() 이 원본이 사라진 텍스처를 지운다.
//! - 올린 뒤에는 이미지의 CPU 픽셀을 놓는다. (Image.set_pixels_retained 로 남기게 한 그림은 둔다)
//! - 놓은 픽셀이 다시 필요하면(Canvas2D 의 drawImage, createPattern) 그림이 기억한 텍스처를 GPU 에서 되읽는다. (restore_image_pixels_from_texture)
//!   크롬이 GPU 에 있는 그림을 CPU 캔버스에 그릴 때 텍스처를 되읽는 것과 같은 길이다. 되읽을 GL 컨텍스트는 Graphic 이 만들 때 건다.
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::core::image::Image;
use glow::HasContext;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

thread_local! {
	/// 그림 텍스처를 되읽을 GL 컨텍스트. (Graphic 이 만들 때 건다, Graphic 이 사라지면 비어 있다)
	/// - GL 자원을 든 쪽이 Drop 에서 지울 손잡이로도 쓴다. (find_gl_context_handle)
	static PIXEL_READ_CONTEXT: RefCell<Weak<glow::Context>> = RefCell::new(Weak::new());
}

//==============================================================================
/// 그림 텍스처를 되읽을 GL 컨텍스트 걸기. (Graphic::new)
//==============================================================================
pub(crate) fn register_pixel_read_context(gl: &Rc<glow::Context>) {
	PIXEL_READ_CONTEXT.with(|context_cell| {
		*context_cell.borrow_mut() = Rc::downgrade(gl);
	});
}

//==============================================================================
/// GL 컨텍스트의 약한 손잡이 찾기. (Graphic 을 받지 않는 곳에서 GL 자원을 지울 때 쥔다)
/// - ScreenEffect, HeatHazeEffect 가 처음 GL 자원을 만들 때 쥐고, destroy 와 Drop 에서 올려 지운다.
///   Graphic 이 사라졌으면 손잡이를 올릴 수 없고, 그 컨텍스트의 GL 자원도 같이 사라졌으므로 지울 것이 없다.
///
/// # 돌려줌
/// `gl` 이 Graphic 이 건 컨텍스트와 같으면 그 손잡이. 걸린 컨텍스트가 없거나 다른 컨텍스트면 None.
//==============================================================================
pub(crate) fn find_gl_context_handle(gl: &glow::Context) -> Option<Weak<glow::Context>> {
	PIXEL_READ_CONTEXT.with(|context_cell| {
		let context_handle = context_cell.borrow().clone();
		let registered_gl = context_handle.upgrade()?;
		if std::ptr::eq(Rc::as_ptr(&registered_gl), gl) {
			Some(context_handle)
		}
		else {
			None
		}
	})
}

//==============================================================================
/// 텍스처 픽셀 읽기. (올린 그대로의 프리멀티플라이드 RGBA, 위에서 아래 차례. 못 읽으면 None)
/// - 텍스처를 잠깐 만든 프레임버퍼에 붙여 읽고, 그 전에 묶여 있던 프레임버퍼로 되돌린다. 묶인 텍스처는 바꾸지 않는다.
//==============================================================================
pub(crate) fn read_texture_pixels(gl: &glow::Context, texture: GlTexture, width: u32, height: u32) -> Option<Vec<u8>> {
	let mut pixels = vec![0u8; width as usize * height as usize * 4];
	unsafe {
		let previous_framebuffer = crate::core::graphic::read_framebuffer_binding(gl);
		let framebuffer = gl.create_framebuffer().ok()?;
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
		let is_complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
		if is_complete {
			gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
			gl.read_pixels(0, 0, width as i32, height as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut pixels)));
		}
		gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
		gl.delete_framebuffer(framebuffer);
		if !is_complete {
			return None;
		}
	}
	Some(pixels)
}

//==============================================================================
/// GPU 에 올리며 놓은 그림의 CPU 픽셀을 그림이 기억한 텍스처에서 되읽어 되돌린다. (픽셀이 있으면 그대로 true)
/// - 올린 적이 없거나, Graphic 이 없거나, 읽지 못하면 false. 되돌린 픽셀은 그림이 계속 든다. (다음에 또 되읽지 않는다)
//==============================================================================
pub(crate) fn restore_image_pixels_from_texture(image: &Image) -> bool {
	if image.has_pixels() {
		return true;
	}
	let texture = match image.get_uploaded_texture() {
		Some(texture) => {
			texture
		}
		None => {
			return false;
		}
	};
	let gl = match PIXEL_READ_CONTEXT.with(|context_cell| context_cell.borrow().upgrade()) {
		Some(gl) => {
			gl
		}
		None => {
			return false;
		}
	};
	match read_texture_pixels(&gl, texture, image.get_width(), image.get_height()) {
		Some(pixels) => {
			image.restore_pixels(pixels);
			true
		}
		None => {
			false
		}
	}
}

struct ImageTextureEntry {
	image: Weak<Image>,
	texture: GlTexture,
	is_smoothing: bool,
	is_mipmap: bool,
}

//==============================================================================
/// 이미지 텍스처 캐시. (이미지마다 GPU 텍스처 하나, Graphic 이 하나 들고 처음 그릴 때 올린다)
/// - 이미지를 `Weak` 로 들어, 이미지가 사라지면 sweep 이 텍스처를 지운다. (Graphic 은 600 프레임마다 부른다)
///
/// # 예제
/// 빈 캐시
/// ```rust
/// use vanilla::core::imagetexturecache::ImageTextureCache;
///
/// let image_texture_cache = ImageTextureCache::new();
/// assert_eq!(image_texture_cache.get_texture_count(), 0);
/// ```
///
/// 텍스처를 올리고 이미지가 사라진 뒤 치우기
/// ```rust,no_run
/// use std::path::Path;
/// use vanilla::{Graphic, Image};
/// use vanilla::core::imagetexturecache::ImageTextureCache;
///
/// fn upload(graphic: &Graphic, image_texture_cache: &mut ImageTextureCache) -> Result<(), String> {
/// 	let image = Image::load(Path::new("examples/assets/atlas/icons.png"))?;
/// 	let texture = image_texture_cache.get_texture(graphic.get_gl(), &image, true, false);
/// 	assert!(texture.is_some());
/// 	assert!(!image.has_pixels());
/// 	drop(image);
/// 	image_texture_cache.sweep(graphic.get_gl());
/// 	assert_eq!(image_texture_cache.get_texture_count(), 0);
/// 	Ok(())
/// }
/// ```
//==============================================================================
pub struct ImageTextureCache {
	entry_table: HashMap<u64, ImageTextureEntry>,
}

impl ImageTextureCache {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 빈 캐시.
	//==============================================================================
	pub fn new() -> ImageTextureCache {
		ImageTextureCache {
			entry_table: HashMap::new(),
		}
	}

	//==============================================================================
	/// 이미 올라가 있고 필터 설정도 같은 텍스처 찾기. (GL 을 건드리지 않는다, 없으면 None)
	//==============================================================================
	pub(crate) fn find_texture(&self, image: &Image, is_smoothing: bool, is_mipmap: bool) -> Option<crate::core::graphic::GlTexture> {
		let entry = self.entry_table.get(&image.get_instance_id())?;
		if entry.is_smoothing != is_smoothing || entry.is_mipmap != is_mipmap {
			return None;
		}
		Some(entry.texture)
	}

	//==============================================================================
	/// 이미 올라가 있는 텍스처 찾기. (필터 설정은 보지 않는다, GL 을 건드리지 않는다)
	//==============================================================================
	pub(crate) fn find_uploaded_texture(&self, image: &Image) -> Option<crate::core::graphic::GlTexture> {
		self.entry_table.get(&image.get_instance_id()).map(|entry| entry.texture)
	}

	//==============================================================================
	/// 이미지의 텍스처 반환. (없으면 올리고 CPU 픽셀을 놓는다, 필터 설정이 바뀌었으면 다시 건다)
	/// - 다른 캐시가 올리며 픽셀을 놓은 그림은 그 텍스처에서 되읽어 올린다.
	/// - 픽셀 길이가 너비x높이x4 가 아니면(되읽지도 못하면) 올리지 않고 None.
	///
	/// # 인자
	/// - `gl`: 텍스처를 만들 GL 컨텍스트.
	/// - `image`: 텍스처를 받을 이미지. (처음 올릴 때 CPU 픽셀을 놓는다, set_pixels_retained(true) 면 남긴다)
	/// - `is_smoothing`: true 면 LINEAR, false 면 NEAREST 필터.
	/// - `is_mipmap`: true 면 줄일 때 밉맵 필터를 쓴다. (밉맵은 올릴 때 늘 만든다)
	///
	/// # 돌려줌
	/// 이미지의 텍스처. GL 텍스처를 만들지 못하면 None.
	//==============================================================================
	pub fn get_texture(&mut self, gl: &glow::Context, image: &Rc<Image>, is_smoothing: bool, is_mipmap: bool) -> Option<crate::core::graphic::GlTexture> {
		let instance_id = image.get_instance_id();
		if let Some(entry) = self.entry_table.get_mut(&instance_id) {
			if entry.is_smoothing != is_smoothing || entry.is_mipmap != is_mipmap {
				unsafe {
					gl.bind_texture(glow::TEXTURE_2D, Some(entry.texture));
					apply_filter(gl, is_smoothing, is_mipmap);
				}
				entry.is_smoothing = is_smoothing;
				entry.is_mipmap = is_mipmap;
			}
			return Some(entry.texture);
		}

		if !has_complete_pixels(image) {
			restore_image_pixels_from_texture(image);
		}
		if !has_complete_pixels(image) {
			eprintln!("ImageTextureCache: pixel length {} does not match {}x{} RGBA", image.get_pixels().len(), image.get_width(), image.get_height());
			return None;
		}
		let texture = unsafe {
			let texture = gl.create_texture().ok()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			let pixels = image.get_pixels();
			gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
			gl.tex_image_2d(
				glow::TEXTURE_2D,
				0,
				glow::RGBA8 as i32,
				image.get_width() as i32,
				image.get_height() as i32,
				0,
				glow::RGBA,
				glow::UNSIGNED_BYTE,
				glow::PixelUnpackData::Slice(Some(&pixels)),
			);
			drop(pixels);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.generate_mipmap(glow::TEXTURE_2D);
			apply_filter(gl, is_smoothing, is_mipmap);
			texture
		};
		image.release_pixels(texture);
		self.entry_table.insert(instance_id, ImageTextureEntry {
			image: Rc::downgrade(image),
			texture,
			is_smoothing,
			is_mipmap,
		});
		Some(texture)
	}

	//==============================================================================
	/// 원본 이미지가 사라진 텍스처 지우기.
	///
	/// # 인자
	/// - `gl`: 텍스처를 만든 GL 컨텍스트.
	//==============================================================================
	pub fn sweep(&mut self, gl: &glow::Context) {
		self.entry_table.retain(|_, entry| {
			let is_alive = entry.image.strong_count() > 0;
			if !is_alive {
				unsafe {
					gl.delete_texture(entry.texture);
				}
			}
			is_alive
		});
	}

	//==============================================================================
	/// 올라가 있는 텍스처 수.
	///
	/// # 돌려줌
	/// 캐시에 든 텍스처 수. (이미지가 사라졌어도 sweep 전이면 센다)
	//==============================================================================
	pub fn get_texture_count(&self) -> usize {
		self.entry_table.len()
	}
}

impl Default for ImageTextureCache {
	//==============================================================================
	/// 빈 캐시 생성. (new 와 같다)
	//==============================================================================
	fn default() -> ImageTextureCache {
		ImageTextureCache::new()
	}
}

//==============================================================================
/// 그림의 픽셀 길이가 너비x높이x4 인지. (텍스처로 올릴 수 있는지)
//==============================================================================
fn has_complete_pixels(image: &Image) -> bool {
	image.get_pixels().len() == image.get_width() as usize * image.get_height() as usize * 4
}

//==============================================================================
/// 텍스처 필터 적용. (스무딩 끄면 도트가 또렷한 NEAREST)
//==============================================================================
unsafe fn apply_filter(gl: &glow::Context, is_smoothing: bool, is_mipmap: bool) {
	let magnify_filter = if is_smoothing {
		glow::LINEAR
	}
	else {
		glow::NEAREST
	};
	let minify_filter = if is_mipmap {
		if is_smoothing {
			glow::LINEAR_MIPMAP_LINEAR
		}
		else {
			glow::NEAREST_MIPMAP_NEAREST
		}
	}
	else {
		magnify_filter
	};
	gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, minify_filter as i32);
	gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, magnify_filter as i32);
}

//==============================================================================
/// 시험용 가짜 GL. (창과 GPU 없이 GL 자원을 만들고 지우는 것을 센다)
/// - GL 객체 번호를 나눠 주고 종류마다 살아 있는 번호를 들고, 정한 수만큼 만든 뒤로는 만들기를 실패시킨다. (번호 0)
/// - 셰이더 컴파일, 프로그램 링크, 프레임버퍼 완성은 늘 성공이고, 그리기와 상태 함수는 아무것도 하지 않는다. 싣지 않은 함수를 부르면 glow 가 패닉한다.
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) mod fake_gl {
	use std::cell::RefCell;
	use std::collections::HashMap;
	use std::ffi::{c_char, c_void, CStr};
	use std::rc::Rc;

	/// GL 객체 종류.
	#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
	pub(crate) enum FakeGlObject {
		Texture,
		Framebuffer,
		Renderbuffer,
		Shader,
		Program,
		VertexArray,
		Buffer,
	}

	/// 가짜 GL 상태. (다음 번호, 종류별 살아 있는 번호, 종류별 남은 성공 수)
	#[derive(Default)]
	struct FakeGlState {
		last_name: u32,
		live_table: HashMap<FakeGlObject, Vec<u32>>,
		remaining_success_table: HashMap<FakeGlObject, u32>,
	}

	thread_local! {
		static FAKE_GL_STATE: RefCell<FakeGlState> = RefCell::new(FakeGlState::default());
	}

	//==============================================================================
	/// 가짜 GL 컨텍스트 생성. (센 것과 실패 설정을 비운다)
	//==============================================================================
	pub(crate) fn create_context() -> Rc<glow::Context> {
		FAKE_GL_STATE.with(|state_cell| {
			*state_cell.borrow_mut() = FakeGlState::default();
		});
		let gl = unsafe {
			glow::Context::from_loader_function_cstr(|symbol_name| find_function(symbol_name))
		};
		Rc::new(gl)
	}

	//==============================================================================
	/// 그 종류를 success_count 개 더 만든 뒤로는 만들기를 실패시킨다.
	//==============================================================================
	pub(crate) fn fail_after(object: FakeGlObject, success_count: u32) {
		FAKE_GL_STATE.with(|state_cell| {
			state_cell.borrow_mut().remaining_success_table.insert(object, success_count);
		});
	}

	//==============================================================================
	/// 그 종류의 살아 있는 객체 수.
	//==============================================================================
	pub(crate) fn get_live_count(object: FakeGlObject) -> usize {
		FAKE_GL_STATE.with(|state_cell| state_cell.borrow().live_table.get(&object).map(|name_list| name_list.len()).unwrap_or(0))
	}

	//==============================================================================
	/// 객체 하나 만들기. (실패면 0)
	//==============================================================================
	fn create_object(object: FakeGlObject) -> u32 {
		FAKE_GL_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			if let Some(remaining_success_count) = state.remaining_success_table.get_mut(&object) {
				if *remaining_success_count == 0 {
					return 0;
				}
				*remaining_success_count -= 1;
			}
			state.last_name += 1;
			let name = state.last_name;
			state.live_table.entry(object).or_default().push(name);
			name
		})
	}

	//==============================================================================
	/// 객체 하나 지우기. (0 이나 없는 번호는 GL 처럼 넘긴다)
	//==============================================================================
	fn delete_object(object: FakeGlObject, name: u32) {
		FAKE_GL_STATE.with(|state_cell| {
			if let Some(name_list) = state_cell.borrow_mut().live_table.get_mut(&object) {
				name_list.retain(|live_name| *live_name != name);
			}
		});
	}

	//==============================================================================
	/// glGen* 자리. (count 개를 만들어 names 에 쓴다)
	//==============================================================================
	unsafe fn generate_objects(object: FakeGlObject, count: i32, names: *mut u32) {
		for index in 0..count.max(0) as usize {
			*names.add(index) = create_object(object);
		}
	}

	//==============================================================================
	/// glDelete* 자리. (names 의 count 개를 지운다)
	//==============================================================================
	unsafe fn delete_objects(object: FakeGlObject, count: i32, names: *const u32) {
		for index in 0..count.max(0) as usize {
			delete_object(object, *names.add(index));
		}
	}

	// 가짜 GL 함수. (glow 가 find_function 으로 기호 이름에 싣는다, 인자 꼴은 GL 함수와 같다)
	extern "system" fn get_string(name: u32) -> *const u8 {
		if name == glow::VERSION {
			b"2.1\0".as_ptr()
		}
		else {
			b"\0".as_ptr()
		}
	}

	//==============================================================================
	/// glGenTextures 자리.
	//==============================================================================
	extern "system" fn gen_textures(count: i32, names: *mut u32) {
		unsafe {
			generate_objects(FakeGlObject::Texture, count, names);
		}
	}

	//==============================================================================
	/// glDeleteTextures 자리.
	//==============================================================================
	extern "system" fn delete_textures(count: i32, names: *const u32) {
		unsafe {
			delete_objects(FakeGlObject::Texture, count, names);
		}
	}

	//==============================================================================
	/// glGenFramebuffers 자리.
	//==============================================================================
	extern "system" fn gen_framebuffers(count: i32, names: *mut u32) {
		unsafe {
			generate_objects(FakeGlObject::Framebuffer, count, names);
		}
	}

	//==============================================================================
	/// glDeleteFramebuffers 자리.
	//==============================================================================
	extern "system" fn delete_framebuffers(count: i32, names: *const u32) {
		unsafe {
			delete_objects(FakeGlObject::Framebuffer, count, names);
		}
	}

	//==============================================================================
	/// glGenRenderbuffers 자리.
	//==============================================================================
	extern "system" fn gen_renderbuffers(count: i32, names: *mut u32) {
		unsafe {
			generate_objects(FakeGlObject::Renderbuffer, count, names);
		}
	}

	//==============================================================================
	/// glDeleteRenderbuffers 자리.
	//==============================================================================
	extern "system" fn delete_renderbuffers(count: i32, names: *const u32) {
		unsafe {
			delete_objects(FakeGlObject::Renderbuffer, count, names);
		}
	}

	//==============================================================================
	/// glGenVertexArrays 자리.
	//==============================================================================
	extern "system" fn gen_vertex_arrays(count: i32, names: *mut u32) {
		unsafe {
			generate_objects(FakeGlObject::VertexArray, count, names);
		}
	}

	//==============================================================================
	/// glDeleteVertexArrays 자리.
	//==============================================================================
	extern "system" fn delete_vertex_arrays(count: i32, names: *const u32) {
		unsafe {
			delete_objects(FakeGlObject::VertexArray, count, names);
		}
	}

	//==============================================================================
	/// glGenBuffers 자리.
	//==============================================================================
	extern "system" fn gen_buffers(count: i32, names: *mut u32) {
		unsafe {
			generate_objects(FakeGlObject::Buffer, count, names);
		}
	}

	//==============================================================================
	/// glDeleteBuffers 자리.
	//==============================================================================
	extern "system" fn delete_buffers(count: i32, names: *const u32) {
		unsafe {
			delete_objects(FakeGlObject::Buffer, count, names);
		}
	}

	//==============================================================================
	/// glBufferData 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn buffer_data(_target: u32, _size: isize, _data: *const c_void, _usage: u32) {
	}

	//==============================================================================
	/// glVertexAttribPointer 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn vertex_attrib_pointer(_index: u32, _size: i32, _component_type: u32, _normalized: u8, _stride: i32, _pointer: *const c_void) {
	}

	//==============================================================================
	/// glCreateShader 자리.
	//==============================================================================
	extern "system" fn create_shader(_shader_type: u32) -> u32 {
		create_object(FakeGlObject::Shader)
	}

	//==============================================================================
	/// glDeleteShader 자리.
	//==============================================================================
	extern "system" fn delete_shader(name: u32) {
		delete_object(FakeGlObject::Shader, name);
	}

	//==============================================================================
	/// glCreateProgram 자리.
	//==============================================================================
	extern "system" fn create_program() -> u32 {
		create_object(FakeGlObject::Program)
	}

	//==============================================================================
	/// glDeleteProgram 자리.
	//==============================================================================
	extern "system" fn delete_program(name: u32) {
		delete_object(FakeGlObject::Program, name);
	}

	//==============================================================================
	/// glGetShaderiv, glGetProgramiv 자리. (늘 1, 컴파일과 링크가 됐다고 본다)
	//==============================================================================
	extern "system" fn get_object_parameter(_name: u32, _parameter: u32, value: *mut i32) {
		unsafe {
			*value = 1;
		}
	}

	//==============================================================================
	/// glCheckFramebufferStatus 자리. (늘 완성)
	//==============================================================================
	extern "system" fn check_framebuffer_status(_target: u32) -> u32 {
		glow::FRAMEBUFFER_COMPLETE
	}

	//==============================================================================
	/// glShaderSource 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn shader_source(_shader: u32, _count: i32, _source_list: *const *const c_char, _length_list: *const i32) {
	}

	//==============================================================================
	/// glTexImage2D 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn tex_image_2d(_target: u32, _level: i32, _internal_format: i32, _width: i32, _height: i32, _border: i32, _format: u32, _component_type: u32, _pixels: *const c_void) {
	}

	//==============================================================================
	/// glFramebufferTexture2D 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn framebuffer_texture_2d(_target: u32, _attachment: u32, _texture_target: u32, _texture: u32, _level: i32) {
	}

	//==============================================================================
	/// glRenderbufferStorage 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn renderbuffer_storage(_target: u32, _internal_format: u32, _width: i32, _height: i32) {
	}

	//==============================================================================
	/// glFramebufferRenderbuffer 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn framebuffer_renderbuffer(_target: u32, _attachment: u32, _renderbuffer_target: u32, _renderbuffer: u32) {
	}

	//==============================================================================
	/// glPixelStorei 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn set_enum_integer(_first: u32, _second: i32) {
	}

	//==============================================================================
	/// glTexParameteri 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn set_enum_enum_integer(_first: u32, _second: u32, _third: i32) {
	}

	//==============================================================================
	/// glBindTexture, glBindFramebuffer, glBindRenderbuffer, glAttachShader, glDetachShader 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn set_name_pair(_first: u32, _second: u32) {
	}

	//==============================================================================
	/// glCompileShader, glLinkProgram, glGenerateMipmap 자리. (아무것도 하지 않는다)
	//==============================================================================
	extern "system" fn set_name(_first: u32) {
	}

	//==============================================================================
	/// glGetIntegerv 자리. (늘 0)
	//==============================================================================
	extern "system" fn get_integer(_parameter: u32, value: *mut i32) {
		unsafe {
			*value = 0;
		}
	}

	//==============================================================================
	/// glReadPixels 자리. (아무것도 쓰지 않는다)
	//==============================================================================
	extern "system" fn read_pixels(_x: i32, _y: i32, _width: i32, _height: i32, _format: u32, _component_type: u32, _pixels: *mut c_void) {
	}

	//==============================================================================
	/// 기호 이름 → 가짜 함수. (없으면 null, glow 가 부를 때 패닉한다)
	//==============================================================================
	fn find_function(symbol_name: &CStr) -> *const c_void {
		match symbol_name.to_bytes() {
			b"glGetString" => get_string as *const c_void,
			b"glGenTextures" => gen_textures as *const c_void,
			b"glDeleteTextures" => delete_textures as *const c_void,
			b"glGenFramebuffers" => gen_framebuffers as *const c_void,
			b"glDeleteFramebuffers" => delete_framebuffers as *const c_void,
			b"glGenRenderbuffers" => gen_renderbuffers as *const c_void,
			b"glDeleteRenderbuffers" => delete_renderbuffers as *const c_void,
			b"glGenVertexArrays" => gen_vertex_arrays as *const c_void,
			b"glDeleteVertexArrays" => delete_vertex_arrays as *const c_void,
			b"glGenBuffers" => gen_buffers as *const c_void,
			b"glDeleteBuffers" => delete_buffers as *const c_void,
			b"glBufferData" => buffer_data as *const c_void,
			b"glVertexAttribPointer" => vertex_attrib_pointer as *const c_void,
			b"glCreateShader" => create_shader as *const c_void,
			b"glDeleteShader" => delete_shader as *const c_void,
			b"glCreateProgram" => create_program as *const c_void,
			b"glDeleteProgram" => delete_program as *const c_void,
			b"glGetShaderiv" | b"glGetProgramiv" => get_object_parameter as *const c_void,
			b"glCheckFramebufferStatus" => check_framebuffer_status as *const c_void,
			b"glShaderSource" => shader_source as *const c_void,
			b"glTexImage2D" => tex_image_2d as *const c_void,
			b"glFramebufferTexture2D" => framebuffer_texture_2d as *const c_void,
			b"glRenderbufferStorage" => renderbuffer_storage as *const c_void,
			b"glFramebufferRenderbuffer" => framebuffer_renderbuffer as *const c_void,
			b"glPixelStorei" => set_enum_integer as *const c_void,
			b"glTexParameteri" => set_enum_enum_integer as *const c_void,
			b"glBindTexture" | b"glBindFramebuffer" | b"glBindRenderbuffer" | b"glAttachShader" | b"glDetachShader" | b"glBindBuffer" | b"glVertexAttribDivisor" => set_name_pair as *const c_void,
			b"glCompileShader" | b"glLinkProgram" | b"glGenerateMipmap" | b"glBindVertexArray" | b"glEnableVertexAttribArray" => set_name as *const c_void,
			b"glGetIntegerv" => get_integer as *const c_void,
			b"glReadPixels" => read_pixels as *const c_void,
			_ => std::ptr::null(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 픽셀을 놓은 그림은 올릴 수 없는 그림으로 가리고, 되돌리면 다시 올릴 수 있다.
	//==============================================================================
	#[test]
	fn released_pixels_are_not_complete() {
		let image = Image::create_from_pixels(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]);
		assert!(has_complete_pixels(&image));
		image.release_pixels(glow::NativeTexture(std::num::NonZeroU32::new(1).expect("texture name")));
		assert!(!has_complete_pixels(&image));
		assert!(!restore_image_pixels_from_texture(&image));
		assert!(!has_complete_pixels(&image));
		image.restore_pixels(vec![0; 8]);
		assert!(has_complete_pixels(&image));
		let empty_image = Image::create_from_premultiplied_pixels(0, 0, Vec::new());
		assert!(has_complete_pixels(&empty_image));
	}

	//==============================================================================
	/// 다른 캐시가 올리며 픽셀을 놓은 그림: 되읽을 컨텍스트가 없으면 올리지 않고(빈 배열로 GL 을 부르지 않는다), 있으면 텍스처에서 되읽어 올린다.
	//==============================================================================
	#[test]
	fn upload_requires_complete_pixels() {
		let gl = fake_gl::create_context();
		let image = Image::create_from_pixels(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]);
		let mut first_cache = ImageTextureCache::new();
		assert!(first_cache.get_texture(&gl, &image, true, false).is_some());
		assert!(!image.has_pixels());
		let mut second_cache = ImageTextureCache::new();
		assert!(second_cache.get_texture(&gl, &image, true, false).is_none());
		assert_eq!(fake_gl::get_live_count(fake_gl::FakeGlObject::Texture), 1);
		register_pixel_read_context(&gl);
		assert!(second_cache.get_texture(&gl, &image, true, false).is_some());
		assert_eq!(fake_gl::get_live_count(fake_gl::FakeGlObject::Texture), 2);
		assert_eq!(fake_gl::get_live_count(fake_gl::FakeGlObject::Framebuffer), 0);
		drop(image);
		first_cache.sweep(&gl);
		second_cache.sweep(&gl);
		assert_eq!(fake_gl::get_live_count(fake_gl::FakeGlObject::Texture), 0);
	}

	//==============================================================================
	/// GL 컨텍스트 손잡이: Graphic 이 건 컨텍스트와 같을 때만 주고, 그 컨텍스트가 사라지면 올릴 수 없다.
	//==============================================================================
	#[test]
	fn gl_context_handle_matches_registered_context() {
		let gl = fake_gl::create_context();
		assert!(find_gl_context_handle(&gl).is_none());
		register_pixel_read_context(&gl);
		let context_handle = find_gl_context_handle(&gl).expect("handle");
		let other_gl = fake_gl::create_context();
		assert!(find_gl_context_handle(&other_gl).is_none());
		drop(gl);
		assert!(context_handle.upgrade().is_none());
		assert!(find_gl_context_handle(&other_gl).is_none());
	}
}

//==============================================================================
//! GPU 메모리 셈. (원본 GpuMemoryTracker 의 짝)
//! - 텍스처, 렌더버퍼, 버퍼를 짓고 크기를 정하는 GlContext 의 함수(tex_image_2d, tex_storage_2d, renderbuffer_storage_multisample, buffer_data_size 같은 것)가
//!   이 파일의 길을 불러 객체마다 바이트를 적는다. 지우면 빼고, 컨텍스트를 잃으면 모두 비운다.
//! - 바이트는 형식과 크기로 셈한 이름값이다. (드라이버가 맞추는 줄 정렬, 타일 메모리, 멀티샘플 압축은 모른다. 멀티샘플 렌더버퍼는 샘플 수를 곱한다)
//! - 주인은 객체를 지은 호출 스택의 위쪽 세 칸이다. (데스크탑은 지을 때 스택 칸만 잡고 이름은 읽을 때 한 번 푼다. 웹(wasm)은 스택이 없어 지은 소스 자리로 대신한다)
//! - 기본은 꺼 둔다. 꺼져 있으면 GlContext 의 짓기, 크기 정하기 함수는 깃발 하나만 보고 지나가고 프레임마다 드는 일은 0 이다.
//!   켜는 길은 EngineConfiguration::track_gpu_memory, Graphic::enable_gpu_memory_tracking, Engine::enable_gpu_memory_tracking 이고 켠 뒤에 지은 것부터 센다.
//!
//! # 예제
//! 추적을 켜고 쓰는 GPU 메모리 합 읽기
//! ```rust
//! use vanilla::{Engine, EngineConfiguration};
//!
//! let engine = Engine::new(EngineConfiguration::new());
//! engine.enable_gpu_memory_tracking();
//! if let Some(usage) = engine.read_gpu_memory_usage() {
//! 	assert!(usage.total_bytes >= usage.canvas_bytes);
//! }
//! ```
//==============================================================================
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};

/// 셈하는 GL 객체의 열쇠. (객체 이름)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GpuMemoryKey {
	Texture(glow::Texture),
	Renderbuffer(glow::Renderbuffer),
	Buffer(glow::Buffer),
}

//==============================================================================
/// GPU 메모리 종류. (원본 GpuMemoryKind)
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum GpuMemoryKind {
	/// 텍스처.
	Texture,
	/// 렌더버퍼. (멀티샘플 렌더 타겟의 색, 깊이)
	Renderbuffer,
	/// 버퍼. (정점, 인덱스, 인스턴스, 유니폼)
	Buffer,
}

impl GpuMemoryKind {
	//==============================================================================
	/// 종류 이름. (원본 GpuMemoryKind 의 값 "texture", "renderbuffer", "buffer")
	///
	/// # 돌려줌
	/// 이름 글.
	///
	/// # 예제
	/// 종류 이름 읽기
	/// ```rust
	/// use vanilla::GpuMemoryKind;
	///
	/// assert_eq!(GpuMemoryKind::Texture.get_name(), "texture");
	/// ```
	//==============================================================================
	pub fn get_name(self) -> &'static str {
		match self {
			GpuMemoryKind::Texture => "texture",
			GpuMemoryKind::Renderbuffer => "renderbuffer",
			GpuMemoryKind::Buffer => "buffer",
		}
	}
}

//==============================================================================
/// 종류 하나의 GPU 메모리. (원본 kindTable 의 { count, bytes })
///
/// # 예제
/// 종류 칸의 기본값은 0 개 0 바이트
/// ```rust
/// use vanilla::GpuMemoryKindUsage;
///
/// let usage = GpuMemoryKindUsage::default();
/// assert_eq!(usage.count, 0);
/// assert_eq!(usage.bytes, 0);
/// ```
//==============================================================================
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]

pub struct GpuMemoryKindUsage {
	/// 살아 있는 객체 수.
	pub count: u64,
	/// 바이트.
	pub bytes: u64,
}

//==============================================================================
/// 주인 하나의 GPU 메모리. (원본 ownerList 의 칸)
///
/// # 예제
/// 주인 칸 하나 만들기
/// ```rust
/// use vanilla::{GpuMemoryKind, GpuMemoryOwnerUsage};
///
/// let usage = GpuMemoryOwnerUsage {
/// 	owner: String::from("Graphic::new"),
/// 	kind: GpuMemoryKind::Texture,
/// 	count: 2,
/// 	bytes: 4096,
/// };
/// assert_eq!(usage.kind, GpuMemoryKind::Texture);
/// assert_eq!(usage.bytes, 4096);
/// ```
//==============================================================================
#[derive(Clone, PartialEq, Eq, Debug)]

pub struct GpuMemoryOwnerUsage {
	/// 주인. (지은 호출 스택의 위쪽 세 칸을 " < " 로 이은 이름)
	pub owner: String,
	/// 종류.
	pub kind: GpuMemoryKind,
	/// 객체 수.
	pub count: u64,
	/// 바이트.
	pub bytes: u64,
}

//==============================================================================
/// 지금 쓰는 GPU 메모리. (원본 readGpuMemoryUsage 가 주는 { totalBytes, canvasBytes, kindTable, ownerList })
///
/// # 예제
/// 엔진이 그래픽을 가졌으면 합을 읽는다
/// ```rust
/// use vanilla::{Engine, EngineConfiguration};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// if let Some(usage) = engine.read_gpu_memory_usage() {
/// 	assert!(usage.total_bytes >= usage.canvas_bytes);
/// }
/// ```
//==============================================================================
#[derive(Clone, PartialEq, Eq, Debug)]

pub struct GpuMemoryUsage {
	/// 합. (캔버스 기본 프레임 버퍼 몫 포함)
	pub total_bytes: u64,
	/// 캔버스 기본 프레임 버퍼 몫. (색 4 바이트 + 깊이와 스텐실 4 바이트, 멀티샘플이면 샘플 수 4 를 곱하고 풀어 낸 색 한 장을 더한다)
	pub canvas_bytes: u64,
	/// 종류별 몫. (세 종류 모두 칸이 있다)
	pub kind_table: BTreeMap<GpuMemoryKind, GpuMemoryKindUsage>,
	/// 주인별 몫. (큰 것부터)
	pub owner_list: Vec<GpuMemoryOwnerUsage>,
}

/// 적은 객체 하나.
struct GpuMemoryEntry {
	kind: GpuMemoryKind,
	owner: Option<String>,
	trace: Option<OwnerTrace>,
	bytes: u64,
	level_table: HashMap<(u32, i32), u64>,
	width: i32,
	height: i32,
	internal_format: u32,
}

#[cfg(not(target_arch = "wasm32"))]
type OwnerTrace = std::backtrace::Backtrace;
#[cfg(target_arch = "wasm32")]
type OwnerTrace = &'static std::panic::Location<'static>;

thread_local! {
	/// 켜졌는지. (꺼져 있으면 짓기와 크기 정하기 함수는 이 깃발만 본다)
	static IS_ENABLED: Cell<bool> = const { Cell::new(false) };
	/// 적은 객체 모두.
	static ENTRY_TABLE: RefCell<HashMap<GpuMemoryKey, GpuMemoryEntry>> = RefCell::new(HashMap::new());
}

#[cfg(not(target_arch = "wasm32"))]
const STACK_DEPTH: usize = 3;

/// 켜졌는지. (GlContext 의 짓기와 크기 정하기가 먼저 본다)
#[inline]
pub(crate) fn is_enabled() -> bool {
	IS_ENABLED.with(|is_enabled| is_enabled.get())
}

/// 켜기. (켠 뒤에 지은 것부터 센다, 이미 켰으면 아무것도 하지 않는다)
pub(crate) fn enable() {
	IS_ENABLED.with(|is_enabled| is_enabled.set(true));
}

/// 컨텍스트를 잃으면 GPU 의 객체가 모두 사라진다. (옛 객체를 나중에 지워도 빼지 않게 모두 잊는다)
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn forget_all() {
	ENTRY_TABLE.with(|entry_table| entry_table.borrow_mut().clear());
}

/// 객체 적기. (켜져 있을 때만)
#[track_caller]
pub(crate) fn register(key: GpuMemoryKey, kind: GpuMemoryKind) {
	if !is_enabled() {
		return;
	}
	#[cfg(not(target_arch = "wasm32"))]
	let trace: OwnerTrace = std::backtrace::Backtrace::force_capture();
	#[cfg(target_arch = "wasm32")]
	let trace: OwnerTrace = std::panic::Location::caller();
	let entry = GpuMemoryEntry { kind, owner: None, trace: Some(trace), bytes: 0, level_table: HashMap::new(), width: 0, height: 0, internal_format: 0 };
	ENTRY_TABLE.with(|entry_table| {
		entry_table.borrow_mut().insert(key, entry);
	});
}

/// 객체 빼기. (켜져 있을 때만)
pub(crate) fn unregister(key: GpuMemoryKey) {
	if !is_enabled() {
		return;
	}
	ENTRY_TABLE.with(|entry_table| {
		entry_table.borrow_mut().remove(&key);
	});
}

/// 객체의 바이트 정하기. (level_key 가 있으면 텍스처의 대상과 단계마다 따로 적고 합한다)
pub(crate) fn set_bytes(key: GpuMemoryKey, level_key: Option<(u32, i32)>, bytes: u64) {
	ENTRY_TABLE.with(|entry_table| {
		let mut entry_table = entry_table.borrow_mut();
		let Some(entry) = entry_table.get_mut(&key) else {
			return;
		};
		match level_key {
			Some(level_key) if entry.kind == GpuMemoryKind::Texture => {
				entry.level_table.insert(level_key, bytes);
				entry.bytes = entry.level_table.values().sum();
			}
			_ => entry.bytes = bytes,
		}
	});
}

/// 텍스처의 바이트 정하기. (tex_image_2d, tex_storage_2d 같은 것이 부른다, 지금 묶인 텍스처가 대상)
pub(crate) fn set_texture_level(texture: Option<glow::Texture>, target: u32, level: i32, internal_format: u32, width: i32, height: i32, depth: i32, ty: u32) {
	let Some(texture) = texture else {
		return;
	};
	let key = GpuMemoryKey::Texture(texture);
	ENTRY_TABLE.with(|entry_table| {
		if let Some(entry) = entry_table.borrow_mut().get_mut(&key) {
			if level == 0 {
				entry.width = width;
				entry.height = height;
				entry.internal_format = internal_format;
			}
		}
	});
	let byte_count = (width.max(0) as u64) * (height.max(0) as u64) * (depth.max(0) as u64) * read_format_byte_count(internal_format, ty);
	set_bytes(key, Some((target, level)), byte_count);
}

/// 밉맵을 만든 뒤의 바이트. (0 단계 크기에서 1x1 까지 절반씩)
pub(crate) fn set_generated_mipmap(texture: Option<glow::Texture>, target: u32) {
	let Some(texture) = texture else {
		return;
	};
	let key = GpuMemoryKey::Texture(texture);
	let (width, height, internal_format) = ENTRY_TABLE.with(|entry_table| entry_table.borrow().get(&key).map(|entry| (entry.width, entry.height, entry.internal_format)).unwrap_or((0, 0, 0)));
	if width <= 0 {
		return;
	}
	let mut level_width = width;
	let mut level_height = height;
	let mut level = 0;
	while level_width > 1 || level_height > 1 {
		level_width = (level_width >> 1).max(1);
		level_height = (level_height >> 1).max(1);
		level += 1;
		set_bytes(key, Some((target, level)), (level_width as u64) * (level_height as u64) * read_format_byte_count(internal_format, 0));
	}
}

/// 형식 하나의 화소당 바이트. (원본 readFormatByteCount, 크기 없는 DEPTH_COMPONENT 만 올리는 값 형식을 본다)
pub(crate) fn read_format_byte_count(internal_format: u32, ty: u32) -> u64 {
	match internal_format {
		0x8229 | 0x8F94 | 0x8231 | 0x8232 | 0x8D48 => 1, // R8, R8_SNORM, R8I, R8UI, STENCIL_INDEX8
		0x822B | 0x8F95 | 0x8238 | 0x822D | 0x8234 | 0x81A5 | 0x8D62 | 0x8056 | 0x8057 => 2, // RG8, RG8_SNORM, RG8UI, R16F, R16UI, DEPTH16, RGB565, RGBA4, RGB5_A1
		0x8051 | 0x8C41 | 0x8F96 => 3, // RGB8, SRGB8, RGB8_SNORM
		0x8058 | 0x8C43 | 0x8F97 | 0x8D7C | 0x8D8E | 0x8059 | 0x8C3A | 0x8C3D => 4, // RGBA8, SRGB8_ALPHA8, RGBA8_SNORM, RGBA8UI, RGBA8I, RGB10_A2, R11F_G11F_B10F, RGB9_E5
		0x822F | 0x822E | 0x8236 | 0x823A | 0x81A6 | 0x8CAC | 0x88F0 => 4, // RG16F, R32F, R32UI, RG16UI, DEPTH24, DEPTH32F, DEPTH24_STENCIL8
		0x881B => 6, // RGB16F
		0x881A | 0x8230 | 0x823C | 0x8D76 | 0x8D88 | 0x8CAD => 8, // RGBA16F, RG32F, RG32UI, RGBA16UI, RGBA16I, DEPTH32F_STENCIL8
		0x8815 => 12, // RGB32F
		0x8814 | 0x8D70 | 0x8D82 => 16, // RGBA32F, RGBA32UI, RGBA32I
		0x1902 => { // DEPTH_COMPONENT
			if ty == glow::UNSIGNED_SHORT {
				2
			}
			else {
				4
			}
		}
		0x1908 => 4, // RGBA
		0x1907 => 3, // RGB
		0x190A => 2, // LUMINANCE_ALPHA
		0x1909 | 0x1906 => 1, // LUMINANCE, ALPHA
		0x84F9 => 4, // DEPTH_STENCIL
		_ => 4,
	}
}

/// 스택 글에서 주인 이름 읽기. (이 파일과 GlContext, 상태 캐시, 표준 라이브러리 칸을 건너뛴 위쪽 세 칸)
#[cfg(not(target_arch = "wasm32"))]
fn read_owner_name(trace: &OwnerTrace) -> String {
	let text = format!("{}", trace);
	let mut name_list: Vec<String> = Vec::new();
	for line in text.lines() {
		let trimmed = line.trim_start();
		let Some((index_text, name)) = trimmed.split_once(": ") else {
			continue;
		};
		if index_text.is_empty() || !index_text.bytes().all(|byte| byte.is_ascii_digit()) {
			continue;
		}
		let name = name.trim();
		let is_internal = name.contains("gpumemorytracker") || name.contains("GlContext") || name.contains("glstatecache") || name.starts_with("std::") || name.starts_with("core::") || name.starts_with("__") || name.starts_with("<std::") || name.starts_with("<core::") || name == "main" || name.starts_with("_");
		if is_internal {
			continue;
		}
		// 끝의 해시(::h0123456789abcdef)는 떼고 긴 이름은 줄인다.
		let name = match name.rfind("::h") {
			Some(position) if name.len() - position == 19 => &name[..position],
			_ => name,
		};
		let name = if name.chars().count() > 60 {
			format!("{}...", name.chars().take(60).collect::<String>())
		}
		else {
			name.to_string()
		};
		name_list.push(name);
		if name_list.len() >= STACK_DEPTH {
			break;
		}
	}
	if name_list.is_empty() {
		String::from("unknown")
	}
	else {
		name_list.join(" < ")
	}
}

//==============================================================================
/// 주인 이름 칸을 읽어 글자로 돌려준다.
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn read_owner_name(trace: &OwnerTrace) -> String {
	format!("{}:{}", trace.file(), trace.line())
}

/// 지금 쓰는 GPU 메모리 읽기. (캔버스 몫은 부른 쪽이 더한다)
pub(crate) fn read_usage(canvas_bytes: u64) -> GpuMemoryUsage {
	let mut kind_table: BTreeMap<GpuMemoryKind, GpuMemoryKindUsage> = BTreeMap::new();
	for kind in [GpuMemoryKind::Texture, GpuMemoryKind::Renderbuffer, GpuMemoryKind::Buffer] {
		kind_table.insert(kind, GpuMemoryKindUsage::default());
	}
	let mut owner_table: HashMap<(GpuMemoryKind, String), GpuMemoryOwnerUsage> = HashMap::new();
	let mut total_bytes: u64 = 0;
	ENTRY_TABLE.with(|entry_table| {
		for entry in entry_table.borrow_mut().values_mut() {
			if let Some(kind_usage) = kind_table.get_mut(&entry.kind) {
				kind_usage.count += 1;
				kind_usage.bytes += entry.bytes;
			}
			total_bytes += entry.bytes;
			if entry.owner.is_none() { // 지을 때 미룬 스택 풀기. (한 번만)
				entry.owner = Some(match entry.trace.take() {
					Some(trace) => read_owner_name(&trace),
					None => String::from("unknown"),
				});
			}
			let owner = entry.owner.clone().unwrap_or_default();
			let owner_usage = owner_table.entry((entry.kind, owner.clone())).or_insert(GpuMemoryOwnerUsage { owner, kind: entry.kind, count: 0, bytes: 0 });
			owner_usage.count += 1;
			owner_usage.bytes += entry.bytes;
		}
	});
	let mut owner_list: Vec<GpuMemoryOwnerUsage> = owner_table.into_values().collect();
	owner_list.sort_by(|left, right| right.bytes.cmp(&left.bytes).then_with(|| left.owner.cmp(&right.owner)));
	GpuMemoryUsage { total_bytes: total_bytes + canvas_bytes, canvas_bytes, kind_table, owner_list }
}

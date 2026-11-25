//==============================================================================
// 뷰 매니저.
// - canvas_native_size:	창 안쪽의 논리 크기. (물리 픽셀 / 배율)
// - canvas_pixel_size:	창 안쪽의 실제 렌더링 픽셀 크기. (드로잉 버퍼)
// - view_native_rect:	실제 사용되는 뷰 영역. (canvas_native_size 기준의 부분 영역)
// - view_size:			실제 사용되는 뷰 크기. (씬이 그리는 좌표계의 크기)
// - 원본의 device_pixel_ratio 는 창의 배율(scale factor)이다.
// - 렌더 배율 상한(set_max_render_pixel_ratio): 원본은 바로 뷰 영역을 다시 셈해 캔버스 크기를 줄인다.
//   러스트 판은 캔버스를 플랫폼이 맞추므로 다음 프레임 첫머리에 상한을 건 배율로 다시 맞춘다. 웹은 캔버스 드로잉 버퍼를 줄이고,
//   데스크탑은 기본 프레임버퍼의 왼쪽 아래에 줄인 크기로 그린 뒤 창 크기로 늘인다. (platform/desktop/renderscaler.rs)
// - 세이프 에어리어 인셋(get_safe_area_insets, set_safe_area_override)은 원본 { top, right, bottom, left } 처럼 이름 붙은 칸을 가진 SafeAreaInsets 다.
//   주소의 ?safeArea=위,오른쪽,아래,왼쪽 자리는 데스크탑에서 환경 변수 VANILLA_SAFE_AREA=위,오른쪽,아래,왼쪽 이다.
//   CSS env(safe-area-inset-*) 는 웹에서 읽고 데스크탑은 0 이다. 크롬 데스크탑도 0 이다. (0 이 아닌 값은 안드로이드에서만 들어온다, platform/desktop.rs read_safe_area_insets)
//   set_safe_area_override 는 둘 다에서 먹는다.
// - get_client_native_size: 원본처럼 페이지 전체 크기(window.innerWidth, innerHeight)다. 데스크탑은 페이지가 없어 창 안쪽 논리 크기다.
// - convert_client_to_view: 화면(client) 좌표를 뷰 좌표로. 웹은 캔버스의 지금 자리를 빼고, 데스크탑은 창의 논리 좌표가 곧 client 좌표다.
// - 무시할 세이프 에어리어 쪽(set_ignored_safe_area_sides)은 원본의 "top" 같은 글 대신 SafeAreaSide 열거형으로 받는다.
// - get_css_pixel_scale 의 캔버스 CSS 높이는 canvas_native_size.y 다. (웹은 캔버스의 CSS 높이, 데스크탑은 창의 논리 높이)
// - apply_aspect_view_scale_mode 는 캔버스 논리 크기로 비를 잰다. (원본은 캔버스 크기가 0 이면 창 안쪽 크기를 쓴다.
//   러스트 판은 씬이 돌기 전에 플랫폼이 캔버스 크기를 맞춰 두어 0 일 때는 창도 0 이다)
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;
use crate::platform;

//==============================================================================
// 세이프 에어리어 인셋. (원본 { top, right, bottom, left })
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SafeAreaInsets {
	pub top: f32,
	pub right: f32,
	pub bottom: f32,
	pub left: f32,
}

impl SafeAreaInsets {
	//==============================================================================
	// 생성. (원본 { top, right, bottom, left } 의 차례)
	//==============================================================================
	pub const fn create(top: f32, right: f32, bottom: f32, left: f32) -> SafeAreaInsets {
		SafeAreaInsets {
			top,
			right,
			bottom,
			left,
		}
	}
}

//==============================================================================
// 세이프 에어리어의 쪽. (원본 "top", "right", "bottom", "left")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafeAreaSide {
	Top,
	Right,
	Bottom,
	Left,
}

//==============================================================================
// 뷰 스케일 모드.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewScaleMode {
	// 사용안함 (창 크기가 바뀌면 뷰 영역도 바뀐다)
	None,
	// 기준해상도로 뷰 영역 정의 (양쪽 축이 잘리거나 남을 수 있음)
	ReferenceResolution,
	// 기준해상도 + 비율을 유지한채 가로축으로 늘여붙임. (반대 축은 잘리거나 남을 수 있음)
	StretchWidth,
	// 기준해상도 + 비율을 유지한채 세로축으로 늘여붙임. (반대 축은 잘리거나 남을 수 있음)
	StretchHeight,
	// 기준해상도 + 비율을 유지한채 가로세로 중에서 짧은 축으로 늘여붙임. (반대 축은 남을 수 있음)
	StretchShort,
	// 가로를 화면 전체에 늘여붙임. 세로는 화면 비율에 따라 자동 산출. 여백 없음.
	StretchWidthExpandHeight,
	// 세로를 화면 전체에 늘여붙임. 가로는 화면 비율에 따라 자동 산출. 여백 없음.
	StretchHeightExpandWidth,
	// 짧은 축 기준 스케일. 긴 축도 화면 전체에 늘여붙임. 여백 없음.
	StretchShortExpandLong,
}

pub struct ViewManager {
	device_pixel_ratio: f32,
	target_resolution_scale: f32,
	client_native_size: Vector2,
	canvas_native_size: Vector2,
	canvas_pixel_size: Vector2,
	reference_resolution_size: Vector2,
	view_scale_mode: ViewScaleMode,
	view_native_rect: Rect,
	view_size: Vector2,
	// 렌더에 쓸 배율 상한. (0 이면 제한 없음)
	max_render_pixel_ratio: f32,
	// 세이프 에어리어 강제값. (CSS 픽셀, 위 / 오른쪽 / 아래 / 왼쪽)
	safe_area_override: Option<SafeAreaInsets>,
	// 무시할 세이프 에어리어 쪽. (get_safe_area_insets 가 0 으로 돌려준다)
	ignored_safe_area_sides: Vec<SafeAreaSide>,
}

impl ViewManager {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(reference_resolution_size: Vector2, view_scale_mode: ViewScaleMode) -> ViewManager {
		ViewManager {
			device_pixel_ratio: 1.0,
			target_resolution_scale: 1.0,
			client_native_size: Vector2::zero(),
			canvas_native_size: Vector2::zero(),
			canvas_pixel_size: Vector2::zero(),
			reference_resolution_size,
			view_scale_mode,
			view_native_rect: Rect::zero(),
			view_size: Vector2::zero(),
			max_render_pixel_ratio: 0.0,
			safe_area_override: None,
			ignored_safe_area_sides: Vec::new(),
		}
	}

	//==============================================================================
	// 뷰 영역 계산. (창 크기나 배율이 바뀔 때 엔진이 부른다)
	//==============================================================================
	pub fn calculate_view_rect(&mut self, physical_width: u32, physical_height: u32, scale_factor: f32) {
		let device_pixel_ratio = if scale_factor > 0.0 {
			scale_factor
		}
		else {
			1.0
		};
		let canvas_native_size = Vector2::create((physical_width as f32 / device_pixel_ratio).round(), (physical_height as f32 / device_pixel_ratio).round());
		let canvas_pixel_size = Vector2::create(physical_width as f32, physical_height as f32);
		let reference_resolution_size = self.get_reference_resolution_size();
		self.device_pixel_ratio = device_pixel_ratio;
		self.client_native_size = platform::read_client_native_size().unwrap_or(canvas_native_size);
		self.canvas_native_size = canvas_native_size;
		self.canvas_pixel_size = canvas_pixel_size;

		// 뷰 영역 설정.
		match self.view_scale_mode {
			ViewScaleMode::None => {
				self.target_resolution_scale = 1.0;
				self.view_native_rect = Rect::create(0.0, 0.0, canvas_native_size.x.round(), canvas_native_size.y.round());
				self.view_size = canvas_native_size;
			}
			ViewScaleMode::ReferenceResolution => {
				let view_width = reference_resolution_size.x.round();
				let view_height = reference_resolution_size.y.round();
				let view_x = ((canvas_native_size.x - view_width) * 0.5).floor();
				let view_y = ((canvas_native_size.y - view_height) * 0.5).floor();
				self.target_resolution_scale = 1.0;
				self.view_native_rect = Rect::create(view_x, view_y, view_width, view_height);
				self.view_size = reference_resolution_size;
			}
			ViewScaleMode::StretchWidth => {
				let target_resolution_scale = canvas_native_size.x / reference_resolution_size.x;
				let view_width = canvas_native_size.x.round();
				let view_height = (reference_resolution_size.y * target_resolution_scale).round();
				let view_y = ((canvas_native_size.y - view_height) * 0.5).round();
				self.target_resolution_scale = target_resolution_scale;
				self.view_native_rect = Rect::create(0.0, view_y, view_width, view_height);
				self.view_size = reference_resolution_size;
			}
			ViewScaleMode::StretchHeight => {
				let target_resolution_scale = canvas_native_size.y / reference_resolution_size.y;
				let view_width = (reference_resolution_size.x * target_resolution_scale).round();
				let view_height = canvas_native_size.y.round();
				let view_x = ((canvas_native_size.x - view_width) * 0.5).round();
				self.target_resolution_scale = target_resolution_scale;
				self.view_native_rect = Rect::create(view_x, 0.0, view_width, view_height);
				self.view_size = reference_resolution_size;
			}
			ViewScaleMode::StretchShort => {
				let target_resolution_scale = (canvas_native_size.x / reference_resolution_size.x).min(canvas_native_size.y / reference_resolution_size.y);
				let view_width = (reference_resolution_size.x * target_resolution_scale).round();
				let view_height = (reference_resolution_size.y * target_resolution_scale).round();
				let view_x = ((canvas_native_size.x - view_width) * 0.5).round();
				let view_y = ((canvas_native_size.y - view_height) * 0.5).round();
				self.target_resolution_scale = target_resolution_scale;
				self.view_native_rect = Rect::create(view_x, view_y, view_width, view_height);
				self.view_size = reference_resolution_size;
			}
			ViewScaleMode::StretchWidthExpandHeight => {
				let target_resolution_scale = canvas_native_size.x / reference_resolution_size.x;
				self.apply_expanded_view(target_resolution_scale, canvas_native_size);
			}
			ViewScaleMode::StretchHeightExpandWidth => {
				let target_resolution_scale = canvas_native_size.y / reference_resolution_size.y;
				self.apply_expanded_view(target_resolution_scale, canvas_native_size);
			}
			ViewScaleMode::StretchShortExpandLong => {
				let target_resolution_scale = (canvas_native_size.x / reference_resolution_size.x).min(canvas_native_size.y / reference_resolution_size.y);
				self.apply_expanded_view(target_resolution_scale, canvas_native_size);
			}
		}
	}

	//==============================================================================
	// 여백 없이 창 전체를 쓰는 모드의 뷰 영역. (한 축 스케일 고정, 뷰 크기는 창 비율로 산출)
	//==============================================================================
	fn apply_expanded_view(&mut self, target_resolution_scale: f32, canvas_native_size: Vector2) {
		let safe_target_resolution_scale = if target_resolution_scale > 0.0 {
			target_resolution_scale
		}
		else {
			1.0
		};
		self.target_resolution_scale = safe_target_resolution_scale;
		self.view_native_rect = Rect::create(0.0, 0.0, canvas_native_size.x.round(), canvas_native_size.y.round());
		let view_size_x = (self.view_native_rect.size.x / safe_target_resolution_scale).ceil();
		let view_size_y = (self.view_native_rect.size.y / safe_target_resolution_scale).ceil();
		self.view_size = Vector2::create(view_size_x, view_size_y);
	}

	//==============================================================================
	// 좌표계 적용.
	//==============================================================================
	pub fn apply_transform(&self, graphic: &mut Graphic, scale_x: f32, scale_y: f32, skew_x: f32, skew_y: f32, translate_x: f32, translate_y: f32) {
		graphic.set_transform(scale_x, skew_y, skew_x, scale_y, translate_x, translate_y);
	}

	//==============================================================================
	// 화면 전체 영역 적용. (좌표 단위 = canvas_native_size)
	//==============================================================================
	pub fn apply_canvas_native_rect(&self, graphic: &mut Graphic) {
		let device_pixel_ratio = self.get_device_pixel_ratio();
		self.apply_transform(graphic, device_pixel_ratio, device_pixel_ratio, 0.0, 0.0, 0.0, 0.0);
	}

	//==============================================================================
	// 뷰 영역 적용. (좌표 단위 = get_view_size(), 모드를 가리지 않고 같은 코드로 그린다)
	//==============================================================================
	pub fn apply_view_rect(&self, graphic: &mut Graphic) {
		let device_pixel_ratio = self.get_device_pixel_ratio();
		let target_resolution_scale = self.get_target_resolution_scale();
		let view_native_rect = self.get_view_native_rect();
		let scale_x = target_resolution_scale * device_pixel_ratio;
		let scale_y = target_resolution_scale * device_pixel_ratio;
		let translate_x = view_native_rect.position.x * device_pixel_ratio;
		let translate_y = view_native_rect.position.y * device_pixel_ratio;
		self.apply_transform(graphic, scale_x, scale_y, 0.0, 0.0, translate_x, translate_y);
	}

	//==============================================================================
	// 기준 해상도 재설정.
	//==============================================================================
	pub fn apply_reference_resolution_size(&mut self, reference_resolution_size: Vector2) {
		self.reference_resolution_size = reference_resolution_size;
	}

	//==============================================================================
	// 배율. (원본 device_pixel_ratio)
	//==============================================================================
	pub fn get_device_pixel_ratio(&self) -> f32 {
		self.device_pixel_ratio
	}

	//==============================================================================
	// 뷰 스케일 모드 설정. (다음 calculate_view_rect 부터 반영)
	//==============================================================================
	pub fn set_view_scale_mode(&mut self, view_scale_mode: ViewScaleMode) {
		self.view_scale_mode = view_scale_mode;
	}

	//==============================================================================
	// 뷰 스케일 모드 반환.
	//==============================================================================
	pub fn get_view_scale_mode(&self) -> ViewScaleMode {
		self.view_scale_mode
	}

	//==============================================================================
	// 페이지 전체 크기. (원본 window.innerWidth, innerHeight, 데스크탑은 창 안쪽 논리 크기)
	//==============================================================================
	pub fn get_client_native_size(&self) -> Vector2 {
		self.client_native_size
	}

	//==============================================================================
	// 창 안쪽 논리 크기.
	//==============================================================================
	pub fn get_canvas_native_size(&self) -> Vector2 {
		self.canvas_native_size
	}

	//==============================================================================
	// 창 안쪽 물리 픽셀 크기.
	//==============================================================================
	pub fn get_canvas_pixel_size(&self) -> Vector2 {
		self.canvas_pixel_size
	}

	//==============================================================================
	// 기준 해상도.
	//==============================================================================
	pub fn get_reference_resolution_size(&self) -> Vector2 {
		self.reference_resolution_size
	}

	//==============================================================================
	// 뷰 크기.
	//==============================================================================
	pub fn get_view_size(&self) -> Vector2 {
		self.view_size
	}

	//==============================================================================
	// 뷰 영역. (창 안쪽 논리 좌표)
	//==============================================================================
	pub fn get_view_native_rect(&self) -> Rect {
		self.view_native_rect
	}

	//==============================================================================
	// 기준 해상도 → 화면 배율.
	//==============================================================================
	pub fn get_target_resolution_scale(&self) -> f32 {
		self.target_resolution_scale
	}

	//==============================================================================
	// 뷰 한 단위가 화면에서 몇 CSS 픽셀인지. (캔버스의 CSS 높이 / 뷰 높이, 못 재면 1)
	// - 뷰의 고정된 쪽은 늘 같은 길이라, 작은 화면(포털의 PC iframe 등)에서는 한 단위가 1 CSS 픽셀보다 한참 작아져 글자가 읽히지 않는다.
	//   UI 층만 키울 배율을 셈할 때 쓴다. (예: 최소 CSS 배율 / 이 값)
	//==============================================================================
	pub fn get_css_pixel_scale(&self) -> f32 {
		let view_size = self.get_view_size();
		let canvas_height = self.canvas_native_size.y;
		if view_size.y > 0.0 && canvas_height > 0.0 {
			canvas_height / view_size.y
		}
		else {
			1.0
		}
	}

	//==============================================================================
	// 렌더 배율 상한 설정. (고DPI 기기에서 캔버스가 지나치게 커지지 않게, 0 이면 제한하지 않는다)
	// - 웹은 다음 프레임 첫머리에 캔버스와 뷰 영역을 다시 맞춘다. 데스크탑은 쓰지 않는다.
	//==============================================================================
	pub fn set_max_render_pixel_ratio(&mut self, max_render_pixel_ratio: f32) {
		self.max_render_pixel_ratio = max_render_pixel_ratio.max(0.0);
	}

	//==============================================================================
	// 렌더 배율 상한 반환.
	//==============================================================================
	pub fn get_max_render_pixel_ratio(&self) -> f32 {
		self.max_render_pixel_ratio
	}

	//==============================================================================
	// 장치 배율에 렌더 배율 상한 걸기. (상한이 0 이거나 배율이 상한 이하면 그대로, 웹 플랫폼이 캔버스 크기를 정할 때 부른다)
	//==============================================================================
	pub fn limit_render_pixel_ratio(&self, device_pixel_ratio: f32) -> f32 {
		if self.max_render_pixel_ratio > 0.0 && device_pixel_ratio > self.max_render_pixel_ratio {
			return self.max_render_pixel_ratio;
		}
		device_pixel_ratio
	}

	//==============================================================================
	// 종횡비에 맞는 뷰 스케일 모드 자동 선택. (바뀌면 바로 뷰 영역을 다시 셈한다)
	// - 화면이 기준 해상도보다 세로로 길면 가로를 고정하고 세로를 늘리고(StretchWidthExpandHeight),
	//   가로로 길면 세로를 고정하고 가로를 늘린다(StretchHeightExpandWidth). (여러 게임이 복붙하던 로직)
	//==============================================================================
	pub fn apply_aspect_view_scale_mode(&mut self) {
		let reference_resolution_size = self.get_reference_resolution_size();
		let canvas_native_size = self.get_canvas_native_size();
		let screen_aspect = canvas_native_size.x / canvas_native_size.y;
		let reference_aspect = reference_resolution_size.x / reference_resolution_size.y;
		let view_scale_mode = if screen_aspect < reference_aspect {
			ViewScaleMode::StretchWidthExpandHeight
		}
		else {
			ViewScaleMode::StretchHeightExpandWidth
		};
		if self.view_scale_mode == view_scale_mode {
			return;
		}
		self.view_scale_mode = view_scale_mode;
		let canvas_pixel_size = self.get_canvas_pixel_size();
		if canvas_pixel_size.x > 0.0 && canvas_pixel_size.y > 0.0 {
			let device_pixel_ratio = self.get_device_pixel_ratio();
			self.calculate_view_rect(canvas_pixel_size.x as u32, canvas_pixel_size.y as u32, device_pixel_ratio);
		}
	}

	//==============================================================================
	// 세이프 에어리어 강제값 설정. (CSS 픽셀, 데스크탑에서 노치를 흉내 낼 때, None 이면 풀기)
	//==============================================================================
	pub fn set_safe_area_override(&mut self, insets: Option<SafeAreaInsets>) {
		self.safe_area_override = insets;
	}

	//==============================================================================
	// 세이프 에어리어 인셋. (뷰 좌표)
	// - 우선순위: set_safe_area_override > 주소 ?safeArea=위,오른쪽,아래,왼쪽 > CSS env(safe-area-inset-*) 실측.
	// - CSS 픽셀을 기준 해상도 배율로 나눠 뷰 좌표로 바꾼다.
	//==============================================================================
	pub fn get_safe_area_insets(&self) -> SafeAreaInsets {
		let css_insets = match self.safe_area_override {
			Some(safe_area_override) => {
				safe_area_override
			}
			None => {
				match platform::read_page_query_value("safeArea").and_then(|safe_area_text| parse_safe_area_text(&safe_area_text)) {
					Some(query_insets) => {
						query_insets
					}
					None => {
						let [top, right, bottom, left] = platform::read_safe_area_insets();
						SafeAreaInsets::create(top, right, bottom, left)
					}
				}
			}
		};
		let target_resolution_scale = self.get_target_resolution_scale();
		let scale = if target_resolution_scale > 0.0 {
			1.0 / target_resolution_scale
		}
		else {
			1.0
		};
		let mut insets = SafeAreaInsets::create(css_insets.top * scale, css_insets.right * scale, css_insets.bottom * scale, css_insets.left * scale);
		for ignored_side in &self.ignored_safe_area_sides {
			match ignored_side {
				SafeAreaSide::Top => {
					insets.top = 0.0;
				}
				SafeAreaSide::Right => {
					insets.right = 0.0;
				}
				SafeAreaSide::Bottom => {
					insets.bottom = 0.0;
				}
				SafeAreaSide::Left => {
					insets.left = 0.0;
				}
			}
		}
		insets
	}

	//==============================================================================
	// 무시할 세이프 에어리어 쪽 설정. (get_safe_area_insets 가 그 쪽을 0 으로 돌려준다, 빈 목록이면 모두 쓴다)
	// - 플랫폼의 앱이 그 쪽을 이미 비켜 그리는 웹뷰처럼, 어느 쪽을 무시할지는 게임이 정해 넘긴다.
	//==============================================================================
	pub fn set_ignored_safe_area_sides(&mut self, sides: &[SafeAreaSide]) {
		self.ignored_safe_area_sides = sides.to_vec();
	}

	//==============================================================================
	// 무시할 세이프 에어리어 쪽 반환.
	//==============================================================================
	pub fn get_ignored_safe_area_sides(&self) -> &[SafeAreaSide] {
		&self.ignored_safe_area_sides
	}

	//==============================================================================
	// 창 안쪽 논리 좌표 → 뷰 좌표.
	//==============================================================================
	pub fn canvas_position_to_view_position(&self, canvas_position: Vector2) -> Vector2 {
		let device_pixel_ratio = self.get_device_pixel_ratio();
		let target_resolution_scale = self.get_target_resolution_scale();
		let total_scale = target_resolution_scale * device_pixel_ratio;
		if total_scale == 0.0 {
			return Vector2::zero();
		}

		let view_native_rect = self.get_view_native_rect();
		let view_x = ((canvas_position.x * device_pixel_ratio - view_native_rect.position.x * device_pixel_ratio) / total_scale).round();
		let view_y = ((canvas_position.y * device_pixel_ratio - view_native_rect.position.y * device_pixel_ratio) / total_scale).round();
		Vector2::create(view_x, view_y)
	}

	//==============================================================================
	// 화면(client) 좌표를 뷰 좌표로. (캔버스의 지금 자리를 빼서 canvas_position_to_view_position 으로 바꾼다)
	//==============================================================================
	pub fn convert_client_to_view(&self, client_x: f32, client_y: f32) -> Vector2 {
		let canvas_client_position = platform::read_canvas_client_position();
		let canvas_position = Vector2::create(client_x - canvas_client_position.x, client_y - canvas_client_position.y);
		self.canvas_position_to_view_position(canvas_position)
	}
}

//==============================================================================
// 주소의 safeArea 값 읽기. ("44,0,34,0" → 위 44, 오른쪽 0, 아래 34, 왼쪽 0, 빈 글이면 None)
// - 원본처럼 쉼표로 나눈 조각마다 parseFloat 하고, 읽지 못한 조각과 모자란 조각은 0 이다.
//==============================================================================
fn parse_safe_area_text(safe_area_text: &str) -> Option<SafeAreaInsets> {
	if safe_area_text.is_empty() {
		return None;
	}
	let mut insets = [0.0f32; 4];
	for (part_index, part) in safe_area_text.split(',').take(4).enumerate() {
		let value = parse_leading_float(part);
		if !value.is_nan() {
			insets[part_index] = value as f32;
		}
	}
	Some(SafeAreaInsets::create(insets[0], insets[1], insets[2], insets[3]))
}

//==============================================================================
// 글 앞의 실수 읽기. (원본 parseFloat: 앞 공백을 넘고 부호, 숫자, 소수점, 지수, Infinity 를 읽을 수 있는 데까지, 못 읽으면 NaN)
//==============================================================================
pub(crate) fn parse_leading_float(text: &str) -> f64 {
	let trimmed_text = text.trim_start_matches(|character: char| character.is_whitespace() || character == '\u{feff}');
	let text_bytes = trimmed_text.as_bytes();
	let mut end_index = 0;
	if end_index < text_bytes.len() && (text_bytes[end_index] == b'+' || text_bytes[end_index] == b'-') {
		end_index += 1;
	}
	if trimmed_text[end_index..].starts_with("Infinity") {
		let is_negative = text_bytes.first() == Some(&b'-');
		if is_negative {
			return f64::NEG_INFINITY;
		}
		return f64::INFINITY;
	}
	let mut digit_count = 0;
	while end_index < text_bytes.len() && text_bytes[end_index].is_ascii_digit() {
		end_index += 1;
		digit_count += 1;
	}
	if end_index < text_bytes.len() && text_bytes[end_index] == b'.' {
		end_index += 1;
		while end_index < text_bytes.len() && text_bytes[end_index].is_ascii_digit() {
			end_index += 1;
			digit_count += 1;
		}
	}
	if digit_count == 0 {
		return f64::NAN;
	}
	// 지수는 뒤에 숫자가 하나라도 있을 때만 읽는다.
	if end_index < text_bytes.len() && (text_bytes[end_index] == b'e' || text_bytes[end_index] == b'E') {
		let mut exponent_end_index = end_index + 1;
		if exponent_end_index < text_bytes.len() && (text_bytes[exponent_end_index] == b'+' || text_bytes[exponent_end_index] == b'-') {
			exponent_end_index += 1;
		}
		let exponent_digit_start_index = exponent_end_index;
		while exponent_end_index < text_bytes.len() && text_bytes[exponent_end_index].is_ascii_digit() {
			exponent_end_index += 1;
		}
		if exponent_end_index > exponent_digit_start_index {
			end_index = exponent_end_index;
		}
	}
	trimmed_text[..end_index].parse::<f64>().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 1280x720 기준, 배율 2 의 창으로 뷰 영역을 맞춘 뷰 매니저.
	//==============================================================================
	fn create_view_manager(physical_width: u32, physical_height: u32, view_scale_mode: ViewScaleMode) -> ViewManager {
		let mut view_manager = ViewManager::new(Vector2::create(1280.0, 720.0), view_scale_mode);
		view_manager.calculate_view_rect(physical_width, physical_height, 2.0);
		view_manager
	}

	//==============================================================================
	// 렌더 배율 상한: 0 이면 그대로, 상한보다 크면 상한, 음수는 0 으로.
	//==============================================================================
	#[test]
	fn max_render_pixel_ratio_limits_ratio() {
		let mut view_manager = create_view_manager(2560, 1440, ViewScaleMode::StretchShort);
		assert_eq!(view_manager.limit_render_pixel_ratio(3.0), 3.0);
		view_manager.set_max_render_pixel_ratio(2.0);
		assert_eq!(view_manager.get_max_render_pixel_ratio(), 2.0);
		assert_eq!(view_manager.limit_render_pixel_ratio(3.0), 2.0);
		assert_eq!(view_manager.limit_render_pixel_ratio(1.5), 1.5);
		view_manager.set_max_render_pixel_ratio(-1.0);
		assert_eq!(view_manager.get_max_render_pixel_ratio(), 0.0);
		assert_eq!(view_manager.limit_render_pixel_ratio(3.0), 3.0);
	}

	//==============================================================================
	// 종횡비 모드: 기준보다 세로로 길면 가로 고정, 가로로 길면 세로 고정, 바꾸면 뷰 크기를 바로 다시 셈한다.
	//==============================================================================
	#[test]
	fn aspect_view_scale_mode_follows_screen() {
		let mut view_manager = create_view_manager(1500, 2000, ViewScaleMode::StretchShort);
		view_manager.apply_aspect_view_scale_mode();
		assert_eq!(view_manager.get_view_scale_mode(), ViewScaleMode::StretchWidthExpandHeight);
		assert_eq!(view_manager.get_view_size(), Vector2::create(1280.0, 1707.0));

		let mut view_manager = create_view_manager(4000, 1440, ViewScaleMode::StretchShort);
		view_manager.apply_aspect_view_scale_mode();
		assert_eq!(view_manager.get_view_scale_mode(), ViewScaleMode::StretchHeightExpandWidth);
		assert_eq!(view_manager.get_view_size(), Vector2::create(2000.0, 720.0));
	}

	//==============================================================================
	// CSS 픽셀 배율: 캔버스 CSS 높이 / 뷰 높이, 뷰가 없으면 1.
	//==============================================================================
	#[test]
	fn css_pixel_scale_is_canvas_height_over_view_height() {
		let view_manager = create_view_manager(1280, 720, ViewScaleMode::StretchShort);
		assert_eq!(view_manager.get_css_pixel_scale(), 0.5);
		let empty_view_manager = ViewManager::new(Vector2::create(1280.0, 720.0), ViewScaleMode::StretchShort);
		assert_eq!(empty_view_manager.get_css_pixel_scale(), 1.0);
	}

	//==============================================================================
	// 세이프 에어리어: 강제값을 기준 해상도 배율로 나눠 뷰 좌표로, 풀면 플랫폼 값(데스크탑은 0).
	//==============================================================================
	#[test]
	fn safe_area_override_scales_to_view() {
		let mut view_manager = create_view_manager(1280, 720, ViewScaleMode::StretchShort);
		assert_eq!(view_manager.get_target_resolution_scale(), 0.5);
		view_manager.set_safe_area_override(Some(SafeAreaInsets::create(44.0, 0.0, 34.0, 10.0)));
		assert_eq!(view_manager.get_safe_area_insets(), SafeAreaInsets::create(88.0, 0.0, 68.0, 20.0));
		view_manager.set_safe_area_override(None);
		assert_eq!(view_manager.get_safe_area_insets(), SafeAreaInsets::create(0.0, 0.0, 0.0, 0.0));
	}

	//==============================================================================
	// 주소 safeArea 값과 parseFloat. (원본처럼 못 읽은 조각과 모자란 조각은 0)
	//==============================================================================
	#[test]
	fn safe_area_text_and_leading_float() {
		assert_eq!(parse_safe_area_text("44,0,34,0"), Some(SafeAreaInsets::create(44.0, 0.0, 34.0, 0.0)));
		assert_eq!(parse_safe_area_text("44px, abc ,12.5"), Some(SafeAreaInsets::create(44.0, 0.0, 12.5, 0.0)));
		assert_eq!(parse_safe_area_text(""), None);
		assert_eq!(parse_leading_float("  -12.5e1px"), -125.0);
		assert_eq!(parse_leading_float(".5"), 0.5);
		assert_eq!(parse_leading_float("5."), 5.0);
		assert_eq!(parse_leading_float("3e"), 3.0);
		assert_eq!(parse_leading_float("Infinityx"), f64::INFINITY);
		assert_eq!(parse_leading_float("-Infinity"), f64::NEG_INFINITY);
		assert!(parse_leading_float("px").is_nan());
		assert!(parse_leading_float("-").is_nan());
		assert!(parse_leading_float(".").is_nan());
	}

	//==============================================================================
	// 무시할 쪽은 강제값이 있어도 0 이고, 빈 목록이면 다시 모두 쓴다.
	//==============================================================================
	#[test]
	fn ignored_safe_area_sides_are_zero() {
		let mut view_manager = create_view_manager(2560, 1440, ViewScaleMode::None);
		view_manager.set_safe_area_override(Some(SafeAreaInsets::create(40.0, 10.0, 30.0, 20.0)));
		view_manager.set_ignored_safe_area_sides(&[SafeAreaSide::Top, SafeAreaSide::Left]);
		assert_eq!(view_manager.get_ignored_safe_area_sides(), &[SafeAreaSide::Top, SafeAreaSide::Left]);
		assert_eq!(view_manager.get_safe_area_insets(), SafeAreaInsets::create(0.0, 10.0, 30.0, 0.0));
		view_manager.set_ignored_safe_area_sides(&[]);
		assert_eq!(view_manager.get_safe_area_insets(), SafeAreaInsets::create(40.0, 10.0, 30.0, 20.0));
	}

	//==============================================================================
	// 화면 좌표 → 뷰 좌표, 페이지 전체 크기. (데스크탑은 캔버스가 창 안쪽 전체라 창 좌표와 같다)
	// - 렌더 배율을 줄여 그려도(배율 1 로 640x360 픽셀) 같은 논리 좌표는 같은 뷰 좌표가 된다.
	//==============================================================================
	#[test]
	fn client_to_view_and_client_size() {
		let view_manager = create_view_manager(1280, 720, ViewScaleMode::StretchShort);
		assert_eq!(view_manager.get_client_native_size(), Vector2::create(640.0, 360.0));
		assert_eq!(view_manager.convert_client_to_view(320.0, 180.0), Vector2::create(640.0, 360.0));
		let mut reduced_view_manager = ViewManager::new(Vector2::create(1280.0, 720.0), ViewScaleMode::StretchShort);
		reduced_view_manager.calculate_view_rect(640, 360, 1.0);
		assert_eq!(reduced_view_manager.get_canvas_native_size(), Vector2::create(640.0, 360.0));
		assert_eq!(reduced_view_manager.convert_client_to_view(320.0, 180.0), Vector2::create(640.0, 360.0));
	}
}

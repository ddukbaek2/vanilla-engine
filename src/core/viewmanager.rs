//==============================================================================
// 뷰 매니저.
// - canvas_native_size:	창 안쪽의 논리 크기. (물리 픽셀 / 배율)
// - canvas_pixel_size:	창 안쪽의 실제 렌더링 픽셀 크기. (드로잉 버퍼)
// - view_native_rect:	실제 사용되는 뷰 영역. (canvas_native_size 기준의 부분 영역)
// - view_size:			실제 사용되는 뷰 크기. (씬이 그리는 좌표계의 크기)
// - 원본의 device_pixel_ratio 는 창의 배율(scale factor)이다.
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;

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
	canvas_native_size: Vector2,
	canvas_pixel_size: Vector2,
	reference_resolution_size: Vector2,
	view_scale_mode: ViewScaleMode,
	view_native_rect: Rect,
	view_size: Vector2,
}

impl ViewManager {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(reference_resolution_size: Vector2, view_scale_mode: ViewScaleMode) -> ViewManager {
		ViewManager {
			device_pixel_ratio: 1.0,
			target_resolution_scale: 1.0,
			canvas_native_size: Vector2::zero(),
			canvas_pixel_size: Vector2::zero(),
			reference_resolution_size,
			view_scale_mode,
			view_native_rect: Rect::zero(),
			view_size: Vector2::zero(),
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
}

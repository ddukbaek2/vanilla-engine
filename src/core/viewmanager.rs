//==============================================================================
// 뷰 매니저.
// - canvasNativeSize:	창 안쪽의 논리 크기. (물리 픽셀 / 배율)
// - canvasPixelSize:	창 안쪽의 실제 렌더링 픽셀 크기. (드로잉 버퍼)
// - viewNativeRect:	실제 사용되는 뷰 영역. (canvasNativeSize 기준의 부분 영역)
// - viewSize:			실제 사용되는 뷰 크기. (씬이 그리는 좌표계의 크기)
// - 원본의 devicePixelRatio 는 창의 배율(scale factor)이다.
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
	none,
	// 기준해상도로 뷰 영역 정의 (양쪽 축이 잘리거나 남을 수 있음)
	referenceResolution,
	// 기준해상도 + 비율을 유지한채 가로축으로 늘여붙임. (반대 축은 잘리거나 남을 수 있음)
	stretchWidth,
	// 기준해상도 + 비율을 유지한채 세로축으로 늘여붙임. (반대 축은 잘리거나 남을 수 있음)
	stretchHeight,
	// 기준해상도 + 비율을 유지한채 가로세로 중에서 짧은 축으로 늘여붙임. (반대 축은 남을 수 있음)
	stretchShort,
	// 가로를 화면 전체에 늘여붙임. 세로는 화면 비율에 따라 자동 산출. 여백 없음.
	stretchWidthExpandHeight,
	// 세로를 화면 전체에 늘여붙임. 가로는 화면 비율에 따라 자동 산출. 여백 없음.
	stretchHeightExpandWidth,
	// 짧은 축 기준 스케일. 긴 축도 화면 전체에 늘여붙임. 여백 없음.
	stretchShortExpandLong,
}

pub struct ViewManager {
	devicePixelRatio: f32,
	targetResolutionScale: f32,
	canvasNativeSize: Vector2,
	canvasPixelSize: Vector2,
	referenceResolutionSize: Vector2,
	viewScaleMode: ViewScaleMode,
	viewNativeRect: Rect,
	viewSize: Vector2,
}

impl ViewManager {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(referenceResolutionSize: Vector2, viewScaleMode: ViewScaleMode) -> ViewManager {
		ViewManager {
			devicePixelRatio: 1.0,
			targetResolutionScale: 1.0,
			canvasNativeSize: Vector2::zero(),
			canvasPixelSize: Vector2::zero(),
			referenceResolutionSize,
			viewScaleMode,
			viewNativeRect: Rect::zero(),
			viewSize: Vector2::zero(),
		}
	}

	//==============================================================================
	// 뷰 영역 계산. (창 크기나 배율이 바뀔 때 엔진이 부른다)
	//==============================================================================
	pub fn calculateViewRect(&mut self, physicalWidth: u32, physicalHeight: u32, scaleFactor: f32) {
		let devicePixelRatio = if scaleFactor > 0.0 {
			scaleFactor
		}
		else {
			1.0
		};
		let canvasNativeSize = Vector2::create((physicalWidth as f32 / devicePixelRatio).round(), (physicalHeight as f32 / devicePixelRatio).round());
		let canvasPixelSize = Vector2::create(physicalWidth as f32, physicalHeight as f32);
		let referenceResolutionSize = self.getReferenceResolutionSize();
		self.devicePixelRatio = devicePixelRatio;
		self.canvasNativeSize = canvasNativeSize;
		self.canvasPixelSize = canvasPixelSize;

		// 뷰 영역 설정.
		match self.viewScaleMode {
			ViewScaleMode::none => {
				self.targetResolutionScale = 1.0;
				self.viewNativeRect = Rect::create(0.0, 0.0, canvasNativeSize.x.round(), canvasNativeSize.y.round());
				self.viewSize = canvasNativeSize;
			}
			ViewScaleMode::referenceResolution => {
				let viewWidth = referenceResolutionSize.x.round();
				let viewHeight = referenceResolutionSize.y.round();
				let viewX = ((canvasNativeSize.x - viewWidth) * 0.5).floor();
				let viewY = ((canvasNativeSize.y - viewHeight) * 0.5).floor();
				self.targetResolutionScale = 1.0;
				self.viewNativeRect = Rect::create(viewX, viewY, viewWidth, viewHeight);
				self.viewSize = referenceResolutionSize;
			}
			ViewScaleMode::stretchWidth => {
				let targetResolutionScale = canvasNativeSize.x / referenceResolutionSize.x;
				let viewWidth = canvasNativeSize.x.round();
				let viewHeight = (referenceResolutionSize.y * targetResolutionScale).round();
				let viewY = ((canvasNativeSize.y - viewHeight) * 0.5).round();
				self.targetResolutionScale = targetResolutionScale;
				self.viewNativeRect = Rect::create(0.0, viewY, viewWidth, viewHeight);
				self.viewSize = referenceResolutionSize;
			}
			ViewScaleMode::stretchHeight => {
				let targetResolutionScale = canvasNativeSize.y / referenceResolutionSize.y;
				let viewWidth = (referenceResolutionSize.x * targetResolutionScale).round();
				let viewHeight = canvasNativeSize.y.round();
				let viewX = ((canvasNativeSize.x - viewWidth) * 0.5).round();
				self.targetResolutionScale = targetResolutionScale;
				self.viewNativeRect = Rect::create(viewX, 0.0, viewWidth, viewHeight);
				self.viewSize = referenceResolutionSize;
			}
			ViewScaleMode::stretchShort => {
				let targetResolutionScale = (canvasNativeSize.x / referenceResolutionSize.x).min(canvasNativeSize.y / referenceResolutionSize.y);
				let viewWidth = (referenceResolutionSize.x * targetResolutionScale).round();
				let viewHeight = (referenceResolutionSize.y * targetResolutionScale).round();
				let viewX = ((canvasNativeSize.x - viewWidth) * 0.5).round();
				let viewY = ((canvasNativeSize.y - viewHeight) * 0.5).round();
				self.targetResolutionScale = targetResolutionScale;
				self.viewNativeRect = Rect::create(viewX, viewY, viewWidth, viewHeight);
				self.viewSize = referenceResolutionSize;
			}
			ViewScaleMode::stretchWidthExpandHeight => {
				let targetResolutionScale = canvasNativeSize.x / referenceResolutionSize.x;
				self.applyExpandedView(targetResolutionScale, canvasNativeSize);
			}
			ViewScaleMode::stretchHeightExpandWidth => {
				let targetResolutionScale = canvasNativeSize.y / referenceResolutionSize.y;
				self.applyExpandedView(targetResolutionScale, canvasNativeSize);
			}
			ViewScaleMode::stretchShortExpandLong => {
				let targetResolutionScale = (canvasNativeSize.x / referenceResolutionSize.x).min(canvasNativeSize.y / referenceResolutionSize.y);
				self.applyExpandedView(targetResolutionScale, canvasNativeSize);
			}
		}
	}

	//==============================================================================
	// 여백 없이 창 전체를 쓰는 모드의 뷰 영역. (한 축 스케일 고정, 뷰 크기는 창 비율로 산출)
	//==============================================================================
	fn applyExpandedView(&mut self, targetResolutionScale: f32, canvasNativeSize: Vector2) {
		let safeTargetResolutionScale = if targetResolutionScale > 0.0 {
			targetResolutionScale
		}
		else {
			1.0
		};
		self.targetResolutionScale = safeTargetResolutionScale;
		self.viewNativeRect = Rect::create(0.0, 0.0, canvasNativeSize.x.round(), canvasNativeSize.y.round());
		let viewSizeX = (self.viewNativeRect.size.x / safeTargetResolutionScale).ceil();
		let viewSizeY = (self.viewNativeRect.size.y / safeTargetResolutionScale).ceil();
		self.viewSize = Vector2::create(viewSizeX, viewSizeY);
	}

	//==============================================================================
	// 좌표계 적용.
	//==============================================================================
	pub fn applyTransform(&self, graphic: &mut Graphic, scaleX: f32, scaleY: f32, skewX: f32, skewY: f32, translateX: f32, translateY: f32) {
		graphic.setTransform(scaleX, skewY, skewX, scaleY, translateX, translateY);
	}

	//==============================================================================
	// 화면 전체 영역 적용. (좌표 단위 = canvasNativeSize)
	//==============================================================================
	pub fn applyCanvasNativeRect(&self, graphic: &mut Graphic) {
		let devicePixelRatio = self.getDevicePixelRatio();
		self.applyTransform(graphic, devicePixelRatio, devicePixelRatio, 0.0, 0.0, 0.0, 0.0);
	}

	//==============================================================================
	// 뷰 영역 적용. (좌표 단위 = getViewSize(), 모드를 가리지 않고 같은 코드로 그린다)
	//==============================================================================
	pub fn applyViewRect(&self, graphic: &mut Graphic) {
		let devicePixelRatio = self.getDevicePixelRatio();
		let targetResolutionScale = self.getTargetResolutionScale();
		let viewNativeRect = self.getViewNativeRect();
		let scaleX = targetResolutionScale * devicePixelRatio;
		let scaleY = targetResolutionScale * devicePixelRatio;
		let translateX = viewNativeRect.position.x * devicePixelRatio;
		let translateY = viewNativeRect.position.y * devicePixelRatio;
		self.applyTransform(graphic, scaleX, scaleY, 0.0, 0.0, translateX, translateY);
	}

	//==============================================================================
	// 기준 해상도 재설정.
	//==============================================================================
	pub fn applyReferenceResolutionSize(&mut self, referenceResolutionSize: Vector2) {
		self.referenceResolutionSize = referenceResolutionSize;
	}

	//==============================================================================
	// 배율. (원본 devicePixelRatio)
	//==============================================================================
	pub fn getDevicePixelRatio(&self) -> f32 {
		self.devicePixelRatio
	}

	//==============================================================================
	// 뷰 스케일 모드 설정. (다음 calculateViewRect 부터 반영)
	//==============================================================================
	pub fn setViewScaleMode(&mut self, viewScaleMode: ViewScaleMode) {
		self.viewScaleMode = viewScaleMode;
	}

	//==============================================================================
	// 뷰 스케일 모드 반환.
	//==============================================================================
	pub fn getViewScaleMode(&self) -> ViewScaleMode {
		self.viewScaleMode
	}

	//==============================================================================
	// 창 안쪽 논리 크기.
	//==============================================================================
	pub fn getCanvasNativeSize(&self) -> Vector2 {
		self.canvasNativeSize
	}

	//==============================================================================
	// 창 안쪽 물리 픽셀 크기.
	//==============================================================================
	pub fn getCanvasPixelSize(&self) -> Vector2 {
		self.canvasPixelSize
	}

	//==============================================================================
	// 기준 해상도.
	//==============================================================================
	pub fn getReferenceResolutionSize(&self) -> Vector2 {
		self.referenceResolutionSize
	}

	//==============================================================================
	// 뷰 크기.
	//==============================================================================
	pub fn getViewSize(&self) -> Vector2 {
		self.viewSize
	}

	//==============================================================================
	// 뷰 영역. (창 안쪽 논리 좌표)
	//==============================================================================
	pub fn getViewNativeRect(&self) -> Rect {
		self.viewNativeRect
	}

	//==============================================================================
	// 기준 해상도 → 화면 배율.
	//==============================================================================
	pub fn getTargetResolutionScale(&self) -> f32 {
		self.targetResolutionScale
	}

	//==============================================================================
	// 창 안쪽 논리 좌표 → 뷰 좌표.
	//==============================================================================
	pub fn canvasPositionToViewPosition(&self, canvasPosition: Vector2) -> Vector2 {
		let devicePixelRatio = self.getDevicePixelRatio();
		let targetResolutionScale = self.getTargetResolutionScale();
		let totalScale = targetResolutionScale * devicePixelRatio;
		if totalScale == 0.0 {
			return Vector2::zero();
		}

		let viewNativeRect = self.getViewNativeRect();
		let viewX = ((canvasPosition.x * devicePixelRatio - viewNativeRect.position.x * devicePixelRatio) / totalScale).round();
		let viewY = ((canvasPosition.y * devicePixelRatio - viewNativeRect.position.y * devicePixelRatio) / totalScale).round();
		Vector2::create(viewX, viewY)
	}
}

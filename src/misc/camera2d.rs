//==============================================================================
// 2D 카메라.
// - 원본 misc/camera2d.js 를 옮긴 것이다. 추적, 월드 경계 클램프, 드래그 팬, 커서 고정 줌, 관성, 이동 트윈을 한 곳에 모았다.
// - position 은 카메라가 바라보는 월드 좌표(화면 가운데), zoom 은 확대 배율이다.
// - 원본처럼 컴포넌트가 아니라 그냥 값이다. 들고 있는 쪽(씬 등)이 tick 을 부르고, 월드를 그리기 전에 applyTransform 을 건다.
// - 사용:
//     camera.setViewSize(viewSize);                    // 매 프레임 또는 리사이즈 시
//     camera.follow(&[heroPosition], 6.0, timeDelta);  // 추적 (선택)
//     camera.tick(timeDelta);
//     graphic.pushState(); camera.applyTransform(graphic); ...월드 렌더... graphic.popState();
// - 원본과 다른 점:
//   이름을 줄이지 않는 규칙에 따라 minZoom / maxZoom 을 minimumZoom / maximumZoom 으로 쓴다. (setZoomRange 의 뜻은 같다)
//   원본 base/math.js 의 approach 는 러스트 판 base/math 에 아직 없어서 이 파일 안에 같은 식으로 둔다.
//   tweenTo 의 기본 길이 0.4 초는 DEFAULT_TWEEN_DURATION 상수로 넘긴다. (러스트는 기본 인자가 없다)
//==============================================================================
use crate::base::math;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;

//==============================================================================
// 이동 트윈 상태. (원본 #tweenState 객체)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct CameraTweenState {
	fromPosition: Vector2,
	toPosition: Vector2,
	fromZoom: f32,
	toZoom: f32,
	duration: f32,
	elapsedSeconds: f32,
}

//==============================================================================
// 2D 카메라.
//==============================================================================
#[derive(Clone, Debug)]
pub struct Camera2D {
	position: Vector2,
	zoom: f32,
	viewSize: Vector2,
	worldBounds: Option<Rect>,
	velocity: Vector2,
	inertiaDamping: f32,
	minimumZoom: f32,
	maximumZoom: f32,
	tweenState: Option<CameraTweenState>,
}

//==============================================================================
// 값을 목표로 지수 감쇠하며 다가가게 한다. (프레임 독립, 원본 base/math.js 의 approach)
//==============================================================================
fn approach(currentValue: f32, targetValue: f32, rate: f32, timeDelta: f32) -> f32 {
	let blendRatio = 1.0 - (-rate * timeDelta).exp();
	currentValue + (targetValue - currentValue) * blendRatio
}

impl Camera2D {
	// tweenTo 의 기본 길이. (초, 원본 duration = 0.4)
	pub const DEFAULT_TWEEN_DURATION: f32 = 0.4;

	//==============================================================================
	// 생성. (위치 0, 배율 1, 뷰 960x640, 경계 없음, 관성 감쇠 6, 배율 범위 0.1 ~ 8)
	//==============================================================================
	pub fn new() -> Camera2D {
		Camera2D {
			position: Vector2::zero(),
			zoom: 1.0,
			viewSize: Vector2::create(960.0, 640.0),
			worldBounds: None,
			velocity: Vector2::zero(),
			inertiaDamping: 6.0,
			minimumZoom: 0.1,
			maximumZoom: 8.0,
			tweenState: None,
		}
	}

	//==============================================================================
	// 갱신. (관성 이동, 이동 트윈, 경계 클램프 순서)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		// 관성.
		if self.velocity.x != 0.0 || self.velocity.y != 0.0 {
			self.position = Vector2::create(self.position.x + self.velocity.x * timeDelta, self.position.y + self.velocity.y * timeDelta);
			let damping = (-self.inertiaDamping * timeDelta).exp();
			self.velocity = Vector2::create(self.velocity.x * damping, self.velocity.y * damping);
			if self.velocity.x.abs() < 1.0 && self.velocity.y.abs() < 1.0 {
				self.velocity = Vector2::zero();
			}
		}

		// 이동 트윈. (줌은 등비 보간으로 미끄러짐 없이)
		if let Some(mut tweenState) = self.tweenState {
			tweenState.elapsedSeconds += timeDelta;
			let linearRatio = (tweenState.elapsedSeconds / tweenState.duration).min(1.0);
			let easedRatio = 1.0 - (1.0 - linearRatio).powi(3);
			self.zoom = tweenState.fromZoom * (tweenState.toZoom / tweenState.fromZoom).powf(easedRatio);
			let positionX = math::lerp(tweenState.fromPosition.x, tweenState.toPosition.x, easedRatio);
			let positionY = math::lerp(tweenState.fromPosition.y, tweenState.toPosition.y, easedRatio);
			self.position = Vector2::create(positionX, positionY);
			if linearRatio >= 1.0 {
				self.tweenState = None;
			}
			else {
				self.tweenState = Some(tweenState);
			}
		}

		self.clampToBounds();
	}

	//==============================================================================
	// 대상 추적. (여러 대상이면 중점을 따라간다. 프레임 독립 지수 감쇠, rate 가 클수록 빨리 붙는다)
	//==============================================================================
	pub fn follow(&mut self, targetPositions: &[Vector2], rate: f32, timeDelta: f32) {
		if targetPositions.is_empty() {
			return;
		}
		let mut centerX = 0.0;
		let mut centerY = 0.0;
		for targetPosition in targetPositions {
			centerX += targetPosition.x;
			centerY += targetPosition.y;
		}
		let targetCount = targetPositions.len() as f32;
		centerX /= targetCount;
		centerY /= targetCount;
		let positionX = approach(self.position.x, centerX, rate, timeDelta);
		let positionY = approach(self.position.y, centerY, rate, timeDelta);
		self.position = Vector2::create(positionX, positionY);
	}

	//==============================================================================
	// 화면 픽셀 단위 팬. (드래그 팬. 줌을 반영해 월드 이동량으로 바꾼다)
	//==============================================================================
	pub fn panByScreen(&mut self, screenDeltaX: f32, screenDeltaY: f32) {
		self.position = Vector2::create(self.position.x - screenDeltaX / self.zoom, self.position.y - screenDeltaY / self.zoom);
		self.clampToBounds();
	}

	//==============================================================================
	// 관성 던지기. (드래그를 놓을 때 화면 픽셀 속도를 넘긴다)
	//==============================================================================
	pub fn fling(&mut self, screenVelocityX: f32, screenVelocityY: f32) {
		self.velocity = Vector2::create(-screenVelocityX / self.zoom, -screenVelocityY / self.zoom);
	}

	//==============================================================================
	// 화면 기준점을 고정한 줌. (휠 / 핀치. anchorScreen 아래의 월드 지점이 움직이지 않는다, 1.1 = 10% 확대)
	//==============================================================================
	pub fn zoomAt(&mut self, anchorScreen: Vector2, zoomFactor: f32) {
		let previousZoom = self.zoom;
		let nextZoom = math::clamp(previousZoom * zoomFactor, self.minimumZoom, self.maximumZoom);
		if nextZoom == previousZoom {
			return;
		}
		let anchorWorld = self.screenToWorld(anchorScreen);
		self.zoom = nextZoom;
		let anchorWorldAfter = self.screenToWorld(anchorScreen);
		self.position = Vector2::create(self.position.x + (anchorWorld.x - anchorWorldAfter.x), self.position.y + (anchorWorld.y - anchorWorldAfter.y));
		self.clampToBounds();
	}

	//==============================================================================
	// 위치 / 줌으로 이동 트윈 시작. (관성은 멈춘다. 기본 길이는 DEFAULT_TWEEN_DURATION)
	//==============================================================================
	pub fn tweenTo(&mut self, targetPosition: Vector2, targetZoom: f32, duration: f32) {
		self.velocity = Vector2::zero();
		self.tweenState = Some(CameraTweenState {
			fromPosition: self.position,
			toPosition: targetPosition,
			fromZoom: self.zoom,
			toZoom: math::clamp(targetZoom, self.minimumZoom, self.maximumZoom),
			duration: duration.max(0.0001),
			elapsedSeconds: 0.0,
		});
	}

	//==============================================================================
	// 그리기 변환 적용. (부른 쪽에서 pushState / popState 로 감싼다)
	//==============================================================================
	pub fn applyTransform(&self, graphic: &mut Graphic) {
		graphic.translate(self.viewSize.x * 0.5, self.viewSize.y * 0.5);
		graphic.scale(self.zoom, self.zoom);
		graphic.translate(-self.position.x, -self.position.y);
	}

	//==============================================================================
	// 화면 좌표 → 월드 좌표.
	//==============================================================================
	pub fn screenToWorld(&self, screenPosition: Vector2) -> Vector2 {
		let worldX = (screenPosition.x - self.viewSize.x * 0.5) / self.zoom + self.position.x;
		let worldY = (screenPosition.y - self.viewSize.y * 0.5) / self.zoom + self.position.y;
		Vector2::create(worldX, worldY)
	}

	//==============================================================================
	// 월드 좌표 → 화면 좌표.
	//==============================================================================
	pub fn worldToScreen(&self, worldPosition: Vector2) -> Vector2 {
		let screenX = (worldPosition.x - self.position.x) * self.zoom + self.viewSize.x * 0.5;
		let screenY = (worldPosition.y - self.position.y) * self.zoom + self.viewSize.y * 0.5;
		Vector2::create(screenX, screenY)
	}

	//==============================================================================
	// 월드 경계로 클램프. (뷰가 경계보다 크면(줌아웃) 그 축은 가운데에 맞춘다)
	//==============================================================================
	pub fn clampToBounds(&mut self) {
		let bounds = match self.worldBounds {
			Some(bounds) => {
				bounds
			}
			None => {
				return;
			}
		};
		let halfViewWidth = self.viewSize.x * 0.5 / self.zoom;
		let halfViewHeight = self.viewSize.y * 0.5 / self.zoom;
		let clampedX = if halfViewWidth * 2.0 >= bounds.size.x {
			bounds.position.x + bounds.size.x * 0.5
		}
		else {
			math::clamp(self.position.x, bounds.position.x + halfViewWidth, bounds.position.x + bounds.size.x - halfViewWidth)
		};
		let clampedY = if halfViewHeight * 2.0 >= bounds.size.y {
			bounds.position.y + bounds.size.y * 0.5
		}
		else {
			math::clamp(self.position.y, bounds.position.y + halfViewHeight, bounds.position.y + bounds.size.y - halfViewHeight)
		};
		self.position = Vector2::create(clampedX, clampedY);
	}

	//==============================================================================
	// 위치 설정. (경계로 클램프한다)
	//==============================================================================
	pub fn setPosition(&mut self, position: Vector2) {
		self.position = position;
		self.clampToBounds();
	}

	//==============================================================================
	// 위치 반환.
	//==============================================================================
	pub fn getPosition(&self) -> Vector2 {
		self.position
	}

	//==============================================================================
	// 배율 설정. (배율 범위로 자르고 경계로 클램프한다)
	//==============================================================================
	pub fn setZoom(&mut self, zoom: f32) {
		self.zoom = math::clamp(zoom, self.minimumZoom, self.maximumZoom);
		self.clampToBounds();
	}

	//==============================================================================
	// 배율 반환.
	//==============================================================================
	pub fn getZoom(&self) -> f32 {
		self.zoom
	}

	//==============================================================================
	// 뷰 크기 설정. (화면 가운데를 정하는 크기)
	//==============================================================================
	pub fn setViewSize(&mut self, viewSize: Vector2) {
		self.viewSize = viewSize;
	}

	//==============================================================================
	// 뷰 크기 반환.
	//==============================================================================
	pub fn getViewSize(&self) -> Vector2 {
		self.viewSize
	}

	//==============================================================================
	// 월드 경계 설정. (None 이면 경계 없음)
	//==============================================================================
	pub fn setWorldBounds(&mut self, worldBounds: Option<Rect>) {
		self.worldBounds = worldBounds;
		self.clampToBounds();
	}

	//==============================================================================
	// 월드 경계 반환.
	//==============================================================================
	pub fn getWorldBounds(&self) -> Option<Rect> {
		self.worldBounds
	}

	//==============================================================================
	// 배율 범위 설정. (지금 배율도 새 범위로 다시 자른다)
	//==============================================================================
	pub fn setZoomRange(&mut self, minimumZoom: f32, maximumZoom: f32) {
		self.minimumZoom = minimumZoom;
		self.maximumZoom = maximumZoom;
		let zoom = self.zoom;
		self.setZoom(zoom);
	}

	//==============================================================================
	// 관성 감쇠 설정. (클수록 빨리 멈춘다)
	//==============================================================================
	pub fn setInertiaDamping(&mut self, inertiaDamping: f32) {
		self.inertiaDamping = inertiaDamping;
	}

	//==============================================================================
	// 이동 트윈 중인지.
	//==============================================================================
	pub fn isTweening(&self) -> bool {
		self.tweenState.is_some()
	}

	//==============================================================================
	// 관성 정지.
	//==============================================================================
	pub fn stopInertia(&mut self) {
		self.velocity = Vector2::zero();
	}
}

impl Default for Camera2D {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Camera2D {
		Camera2D::new()
	}
}

//==============================================================================
// 시험. (GL 없이 돈다)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 두 값이 거의 같은지.
	//==============================================================================
	fn isNear(left: f32, right: f32) -> bool {
		(left - right).abs() < 0.001
	}

	//==============================================================================
	// 추적은 1 - exp(-rate * dt) 만큼 중점으로 다가간다.
	//==============================================================================
	#[test]
	fn followApproachesCenter() {
		let mut camera = Camera2D::new();
		let targetPositions = [Vector2::create(100.0, 0.0), Vector2::create(300.0, 200.0)];
		camera.follow(&targetPositions, 6.0, 0.1);
		let expectedRatio = 1.0 - (-0.6f32).exp();
		let position = camera.getPosition();
		assert!(isNear(position.x, 200.0 * expectedRatio));
		assert!(isNear(position.y, 100.0 * expectedRatio));

		// 대상이 없으면 그대로.
		camera.follow(&[], 6.0, 0.1);
		assert_eq!(camera.getPosition(), position);
	}

	//==============================================================================
	// 경계 클램프. (뷰 절반만큼 안쪽, 뷰가 경계보다 크면 가운데)
	//==============================================================================
	#[test]
	fn clampKeepsViewInsideBounds() {
		let mut camera = Camera2D::new();
		camera.setViewSize(Vector2::create(200.0, 100.0));
		camera.setWorldBounds(Some(Rect::create(0.0, 0.0, 1000.0, 500.0)));
		camera.setPosition(Vector2::create(-50.0, 900.0));
		assert_eq!(camera.getPosition(), Vector2::create(100.0, 450.0));

		// 줌아웃해서 뷰(200/0.1 = 2000)가 경계보다 커지면 가운데.
		camera.setZoom(0.1);
		assert_eq!(camera.getPosition(), Vector2::create(500.0, 250.0));
	}

	//==============================================================================
	// 화면 → 월드 → 화면 왕복과 커서 고정 줌.
	//==============================================================================
	#[test]
	fn zoomAtKeepsAnchorWorldPoint() {
		let mut camera = Camera2D::new();
		camera.setViewSize(Vector2::create(800.0, 600.0));
		camera.setPosition(Vector2::create(50.0, 70.0));
		let anchorScreen = Vector2::create(600.0, 150.0);
		let anchorWorldBefore = camera.screenToWorld(anchorScreen);
		let screenAgain = camera.worldToScreen(anchorWorldBefore);
		assert!(isNear(screenAgain.x, anchorScreen.x) && isNear(screenAgain.y, anchorScreen.y));

		camera.zoomAt(anchorScreen, 2.0);
		assert!(isNear(camera.getZoom(), 2.0));
		let anchorWorldAfter = camera.screenToWorld(anchorScreen);
		assert!(isNear(anchorWorldAfter.x, anchorWorldBefore.x));
		assert!(isNear(anchorWorldAfter.y, anchorWorldBefore.y));

		// 범위 끝에서는 움직이지 않는다.
		camera.setZoomRange(0.5, 2.0);
		let positionBefore = camera.getPosition();
		camera.zoomAt(anchorScreen, 3.0);
		assert_eq!(camera.getPosition(), positionBefore);
	}

	//==============================================================================
	// 팬, 관성, 트윈.
	//==============================================================================
	#[test]
	fn panFlingAndTween() {
		let mut camera = Camera2D::new();
		camera.setZoom(2.0);
		camera.panByScreen(20.0, -10.0);
		assert_eq!(camera.getPosition(), Vector2::create(-10.0, 5.0));

		camera.fling(100.0, 0.0);
		camera.tick(0.1);
		assert!(isNear(camera.getPosition().x, -10.0 - 50.0 * 0.1));
		for _ in 0..200 {
			camera.tick(0.05);
		}
		let restingPosition = camera.getPosition();
		camera.tick(0.05);
		assert_eq!(camera.getPosition(), restingPosition);

		camera.tweenTo(Vector2::create(300.0, 400.0), 4.0, 0.5);
		assert!(camera.isTweening());
		camera.tick(0.25);
		assert!(camera.isTweening());
		camera.tick(0.25);
		assert!(!camera.isTweening());
		assert!(isNear(camera.getPosition().x, 300.0));
		assert!(isNear(camera.getPosition().y, 400.0));
		assert!(isNear(camera.getZoom(), 4.0));
	}
}

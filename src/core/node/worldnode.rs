//==============================================================================
// 월드 노드.
// - 부모/자식 계층 + 활성화 (원본 Node)
// - 컴포넌트 보유 + tick (원본 ComponentNode)
// - 로컬 TRS + 렌더 재귀 (원본 TransformNode)
// - pivot / anchor / contentSize + 터치 (원본 WorldNode)
// - 노드는 씬의 NodeTree 저장소에 들어 있고, 부모와 자식은 핸들(NodeId)로 잇는다.
//   트리를 바꾸는 일(자식 붙이기, 지우기, 찾기)과 트리를 도는 일(tick, draw, 터치)은 NodeTree 가 한다.
//==============================================================================
use crate::base::color::Color;
use crate::base::identifier;
use crate::base::math;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::transformmatrix::TransformMatrix;
use crate::base::vector2::Vector2;
use crate::core::component::Component;
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::NodeId;

//==============================================================================
// 노드 트랜스폼. (컴포넌트가 tick / draw 에서 받아 읽고 바꾸는 노드 상태)
// - localPosition 은 부모 영역 안에서 자기 피봇 점의 좌표다. (anchor 는 anchoredPosition 계산용)
// - localRotation 은 도 단위. (원본과 같음)
//==============================================================================
#[derive(Clone, Debug)]
pub struct NodeTransform {
	localPosition: Vector2,
	localScale: Vector2,
	localRotation: f32,
	localOpacity: f32,
	pivot: Vector2,
	contentSize: Vector2,
	anchor: Vector2,
	isVisible: bool,
}

impl NodeTransform {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> NodeTransform {
		NodeTransform {
			localPosition: Vector2::zero(),
			localScale: Vector2::one(),
			localRotation: 0.0,
			localOpacity: 1.0,
			pivot: Pivot::middleCenter,
			contentSize: Vector2::zero(),
			anchor: Pivot::middleCenter,
			isVisible: true,
		}
	}

	//==============================================================================
	// 로컬 위치 설정.
	//==============================================================================
	pub fn setLocalPosition(&mut self, localPosition: Vector2) {
		self.localPosition = localPosition;
	}

	//==============================================================================
	// 로컬 위치 반환.
	//==============================================================================
	pub fn getLocalPosition(&self) -> Vector2 {
		self.localPosition
	}

	//==============================================================================
	// 로컬 크기 설정.
	//==============================================================================
	pub fn setLocalScale(&mut self, localScale: Vector2) {
		self.localScale = localScale;
	}

	//==============================================================================
	// 로컬 크기 반환.
	//==============================================================================
	pub fn getLocalScale(&self) -> Vector2 {
		self.localScale
	}

	//==============================================================================
	// 로컬 회전 설정. (도)
	//==============================================================================
	pub fn setLocalRotation(&mut self, localRotation: f32) {
		self.localRotation = localRotation;
	}

	//==============================================================================
	// 로컬 회전 반환. (도)
	//==============================================================================
	pub fn getLocalRotation(&self) -> f32 {
		self.localRotation
	}

	//==============================================================================
	// 로컬 투명도 설정. (자식에게 곱해져 내려간다)
	//==============================================================================
	pub fn setLocalOpacity(&mut self, localOpacity: f32) {
		self.localOpacity = math::clamp01(localOpacity);
	}

	//==============================================================================
	// 로컬 투명도 반환.
	//==============================================================================
	pub fn getLocalOpacity(&self) -> f32 {
		self.localOpacity
	}

	//==============================================================================
	// 피봇 설정. (contentSize 안에서 기준점의 비율, 기본 가운데)
	//==============================================================================
	pub fn setPivot(&mut self, pivot: Vector2) {
		self.pivot = pivot;
	}

	//==============================================================================
	// 피봇 반환.
	//==============================================================================
	pub fn getPivot(&self) -> Vector2 {
		self.pivot
	}

	//==============================================================================
	// 컨텐츠 크기 설정.
	//==============================================================================
	pub fn setContentSize(&mut self, contentSize: Vector2) {
		self.contentSize = contentSize;
	}

	//==============================================================================
	// 컨텐츠 크기 반환.
	//==============================================================================
	pub fn getContentSize(&self) -> Vector2 {
		self.contentSize
	}

	//==============================================================================
	// 앵커 설정. (부모 contentSize 안의 비율)
	//==============================================================================
	pub fn setAnchor(&mut self, anchor: Vector2) {
		self.anchor = anchor;
	}

	//==============================================================================
	// 앵커 반환.
	//==============================================================================
	pub fn getAnchor(&self) -> Vector2 {
		self.anchor
	}

	//==============================================================================
	// 앵커 기준 위치 설정. (부모 contentSize 를 알아야 한다)
	//==============================================================================
	pub fn setAnchoredPosition(&mut self, anchoredPosition: Vector2, parentContentSize: Vector2) {
		let anchorPoint = parentContentSize.multiply(self.anchor);
		self.localPosition = anchorPoint.add(anchoredPosition);
	}

	//==============================================================================
	// 앵커 기준 위치 반환.
	//==============================================================================
	pub fn getAnchoredPosition(&self, parentContentSize: Vector2) -> Vector2 {
		let anchorPoint = parentContentSize.multiply(self.anchor);
		self.localPosition.subtract(anchorPoint)
	}

	//==============================================================================
	// 보이기 설정. (꺼도 tick 은 돈다. 원본 isVisible)
	//==============================================================================
	pub fn setVisible(&mut self, isVisible: bool) {
		self.isVisible = isVisible;
	}

	//==============================================================================
	// 보이기 여부.
	//==============================================================================
	pub fn isVisible(&self) -> bool {
		self.isVisible
	}

	//==============================================================================
	// 로컬 변환 행렬. (pushTransform 과 같은 순서: 이동 → 회전 → 크기 → 피봇 보정)
	//==============================================================================
	pub fn calculateLocalMatrix(&self) -> TransformMatrix {
		let mut localMatrix = TransformMatrix::identity();
		localMatrix.translate(self.localPosition.x, self.localPosition.y);
		localMatrix.rotate(math::degreeToRadian(self.localRotation));
		localMatrix.scale(self.localScale.x, self.localScale.y);
		localMatrix.translate(-self.contentSize.x * self.pivot.x, -self.contentSize.y * self.pivot.y);
		localMatrix
	}

	//==============================================================================
	// 부모 영역에서 본 경계 사각형. (회전이 없을 때 정확하다. 충돌 검사 등)
	//==============================================================================
	pub fn getLocalBounds(&self) -> Rect {
		let width = self.contentSize.x * self.localScale.x.abs();
		let height = self.contentSize.y * self.localScale.y.abs();
		Rect::create(self.localPosition.x - width * self.pivot.x, self.localPosition.y - height * self.pivot.y, width, height)
	}
}

impl Default for NodeTransform {
	fn default() -> NodeTransform {
		NodeTransform::new()
	}
}


//==============================================================================
// 월드 노드.
//==============================================================================
pub struct WorldNode {
	instanceId: u64,
	name: String,
	isActive: bool,
	isInteractable: bool,
	isGizmoVisible: bool,
	transform: NodeTransform,
	pub(crate) components: Vec<Box<dyn Component>>,
	pub(crate) parentId: Option<NodeId>,
	pub(crate) childIds: Vec<NodeId>,
}

impl WorldNode {
	//==============================================================================
	// 생성. (트리에 넣을 때는 NodeTree::createNode 를 쓴다)
	//==============================================================================
	pub fn new(name: &str) -> WorldNode {
		WorldNode {
			instanceId: identifier::createInstanceId(),
			name: name.to_string(),
			isActive: true,
			isInteractable: false,
			isGizmoVisible: false,
			transform: NodeTransform::new(),
			components: Vec::new(),
			parentId: None,
			childIds: Vec::new(),
		}
	}

	//==============================================================================
	// 인스턴스 식별자.
	//==============================================================================
	pub fn getInstanceId(&self) -> u64 {
		self.instanceId
	}

	//==============================================================================
	// 이름 설정.
	//==============================================================================
	pub fn setName(&mut self, name: &str) {
		self.name = name.to_string();
	}

	//==============================================================================
	// 이름 반환.
	//==============================================================================
	pub fn getName(&self) -> &str {
		&self.name
	}

	//==============================================================================
	// 부모 핸들. (루트이거나 아직 붙지 않았으면 None)
	//==============================================================================
	pub fn getParentId(&self) -> Option<NodeId> {
		self.parentId
	}

	//==============================================================================
	// 자식 핸들 목록. (그리는 순서)
	//==============================================================================
	pub fn getChildIds(&self) -> &[NodeId] {
		&self.childIds
	}

	//==============================================================================
	// 자식 수.
	//==============================================================================
	pub fn getChildCount(&self) -> usize {
		self.childIds.len()
	}

	//==============================================================================
	// 활성화 설정. (꺼지면 tick 도 draw 도 하지 않는다)
	//==============================================================================
	pub fn setActive(&mut self, isActive: bool) {
		self.isActive = isActive;
	}

	//==============================================================================
	// 활성화 여부.
	//==============================================================================
	pub fn isActive(&self) -> bool {
		self.isActive
	}

	//==============================================================================
	// 터치를 받을지 설정.
	//==============================================================================
	pub fn setInteractable(&mut self, isInteractable: bool) {
		self.isInteractable = isInteractable;
	}

	//==============================================================================
	// 터치를 받는지.
	//==============================================================================
	pub fn isInteractable(&self) -> bool {
		self.isInteractable
	}

	//==============================================================================
	// 기즈모 표시 설정.
	//==============================================================================
	pub fn setGizmoVisible(&mut self, isGizmoVisible: bool) {
		self.isGizmoVisible = isGizmoVisible;
	}

	//==============================================================================
	// 기즈모 표시 여부.
	//==============================================================================
	pub fn isGizmoVisible(&self) -> bool {
		self.isGizmoVisible
	}

	//==============================================================================
	// 트랜스폼 반환.
	//==============================================================================
	pub fn getTransform(&self) -> &NodeTransform {
		&self.transform
	}

	//==============================================================================
	// 트랜스폼 반환. (변경용)
	//==============================================================================
	pub fn getTransformMut(&mut self) -> &mut NodeTransform {
		&mut self.transform
	}

	//==============================================================================
	// 트랜스폼 바로가기.
	//==============================================================================
	pub fn setLocalPosition(&mut self, localPosition: Vector2) {
		self.transform.setLocalPosition(localPosition);
	}

	pub fn getLocalPosition(&self) -> Vector2 {
		self.transform.getLocalPosition()
	}

	pub fn setLocalScale(&mut self, localScale: Vector2) {
		self.transform.setLocalScale(localScale);
	}

	pub fn getLocalScale(&self) -> Vector2 {
		self.transform.getLocalScale()
	}

	pub fn setLocalRotation(&mut self, localRotation: f32) {
		self.transform.setLocalRotation(localRotation);
	}

	pub fn getLocalRotation(&self) -> f32 {
		self.transform.getLocalRotation()
	}

	pub fn setLocalOpacity(&mut self, localOpacity: f32) {
		self.transform.setLocalOpacity(localOpacity);
	}

	pub fn getLocalOpacity(&self) -> f32 {
		self.transform.getLocalOpacity()
	}

	pub fn setPivot(&mut self, pivot: Vector2) {
		self.transform.setPivot(pivot);
	}

	pub fn getPivot(&self) -> Vector2 {
		self.transform.getPivot()
	}

	pub fn setContentSize(&mut self, contentSize: Vector2) {
		self.transform.setContentSize(contentSize);
	}

	pub fn getContentSize(&self) -> Vector2 {
		self.transform.getContentSize()
	}

	pub fn setAnchor(&mut self, anchor: Vector2) {
		self.transform.setAnchor(anchor);
	}

	pub fn getAnchor(&self) -> Vector2 {
		self.transform.getAnchor()
	}

	pub fn setVisible(&mut self, isVisible: bool) {
		self.transform.setVisible(isVisible);
	}

	pub fn isVisible(&self) -> bool {
		self.transform.isVisible()
	}

	//==============================================================================
	// 컴포넌트 추가. (추가한 컴포넌트를 돌려준다)
	//==============================================================================
	pub fn addComponent<T: Component + 'static>(&mut self, component: T) -> &mut T {
		let mut boxedComponent: Box<dyn Component> = Box::new(component);
		boxedComponent.attach(&mut self.transform);
		self.components.push(boxedComponent);
		let lastComponent = self.components.last_mut().expect("added component");
		let componentReference: &mut dyn Component = lastComponent.as_mut();
		componentReference.asAnyMut().downcast_mut::<T>().expect("added component type")
	}

	//==============================================================================
	// 컴포넌트 반환. (그 타입의 첫 번째)
	//==============================================================================
	pub fn getComponent<T: Component + 'static>(&self) -> Option<&T> {
		for component in self.components.iter() {
			let componentReference: &dyn Component = component.as_ref();
			if let Some(found) = componentReference.asAny().downcast_ref::<T>() {
				return Some(found);
			}
		}
		None
	}

	//==============================================================================
	// 컴포넌트 반환. (변경용, 그 타입의 첫 번째)
	//==============================================================================
	pub fn getComponentMut<T: Component + 'static>(&mut self) -> Option<&mut T> {
		for component in self.components.iter_mut() {
			let componentReference: &mut dyn Component = component.as_mut();
			if let Some(found) = componentReference.asAnyMut().downcast_mut::<T>() {
				return Some(found);
			}
		}
		None
	}

	//==============================================================================
	// 컴포넌트가 있는지.
	//==============================================================================
	pub fn hasComponent<T: Component + 'static>(&self) -> bool {
		self.getComponent::<T>().is_some()
	}

	//==============================================================================
	// 컴포넌트 제거. (그 타입 모두, 제거한 수)
	//==============================================================================
	pub fn removeComponent<T: Component + 'static>(&mut self) -> usize {
		let beforeCount = self.components.len();
		self.components.retain(|component| {
			let componentReference: &dyn Component = component.as_ref();
			!componentReference.asAny().is::<T>()
		});
		beforeCount - self.components.len()
	}

	//==============================================================================
	// 모든 컴포넌트.
	//==============================================================================
	pub fn getAllComponents(&self) -> &Vec<Box<dyn Component>> {
		&self.components
	}

	//==============================================================================
	// 출력 상태 시작. (이동 → 회전 → 크기 → 피봇 보정 → 투명도 곱)
	//==============================================================================
	pub fn pushTransform(&self, graphic: &mut Graphic) {
		graphic.pushState();

		// 트랜스폼 반영.
		// localPosition 은 부모 영역 내 자기 객체의 기준점 좌표 (anchor 영향 없음).
		let localPosition = self.transform.getLocalPosition();
		let localRotation = self.transform.getLocalRotation();
		let radian = math::degreeToRadian(localRotation);
		let localScale = self.transform.getLocalScale();
		graphic.translate(localPosition.x, localPosition.y);
		graphic.rotate(radian);
		graphic.scale(localScale.x, localScale.y);

		// 현재 노드의 피봇 반영.
		// 기준점을 좌상으로 여기고 우하방향으로 그림을 그리므로, 컨텐트사이즈 기준 피봇만큼 당긴다.
		let pivot = self.transform.getPivot();
		let contentSize = self.transform.getContentSize();
		graphic.translate(-contentSize.x * pivot.x, -contentSize.y * pivot.y);

		// 투명도 반영.
		let localOpacity = self.transform.getLocalOpacity();
		graphic.multiplyGlobalAlpha(localOpacity);
	}

	//==============================================================================
	// 컴포넌트 출력. (자식은 NodeTree 가 그린다)
	// - 돌려주는 것: (Mask 가 있으면 둥근 크기, 화면 밖이라 건너뛰었는지)
	// - 켜진 컴포넌트가 모두 contentSize 안에만 그리면(isDrawnInsideContent) 영역이 화면 밖일 때 그리지 않는다.
	//==============================================================================
	pub fn drawComponents(&mut self, graphic: &mut Graphic) -> (Option<f32>, bool) {
		let mut isCulled = false;
		let contentSize = self.transform.getContentSize();
		if !self.components.is_empty() && graphic.isCullingEnabled() && contentSize.x > 0.0 && contentSize.y > 0.0 {
			let isAllInsideContent = self.components.iter().all(|component| !component.isEnable() || component.isDrawnInsideContent());
			if isAllInsideContent {
				let contentRect = Rect::create(0.0, 0.0, contentSize.x, contentSize.y);
				isCulled = !graphic.isLocalRectVisible(contentRect);
			}
		}

		let mut clipRoundSize: Option<f32> = None;
		for component in self.components.iter_mut() {
			if !component.isEnable() {
				continue;
			}
			if !isCulled {
				component.draw(&self.transform, graphic);
			}
			if let Some(roundSize) = component.getClipRoundSize() {
				clipRoundSize = Some(roundSize);
			}
		}
		(clipRoundSize, isCulled)
	}

	//==============================================================================
	// 기즈모 출력. (contentSize 테두리와 피봇 점)
	//==============================================================================
	pub fn drawGizmos(&mut self, graphic: &mut Graphic) {
		let originalAlpha = graphic.getGlobalAlpha();
		graphic.setGlobalAlpha(1.0);

		let contentSize = self.transform.getContentSize();
		let pivot = self.transform.getPivot();
		graphic.setStrokeColor(Color::create(0.2, 1.0, 0.4, 1.0));
		graphic.drawStrokeRect(Rect::create(0.0, 0.0, contentSize.x, contentSize.y), 1.0);
		graphic.setFillColor(Color::create(1.0, 0.3, 0.3, 1.0));
		graphic.drawCircle(contentSize.multiply(pivot), 3.0);

		for component in self.components.iter_mut() {
			if component.isGizmoVisible() {
				component.drawGizmos(&self.transform, graphic);
			}
		}

		graphic.setGlobalAlpha(originalAlpha);
	}

	//==============================================================================
	// 출력 상태 종료.
	//==============================================================================
	pub fn popTransform(&self, graphic: &mut Graphic) {
		graphic.popState();
	}

	//==============================================================================
	// 점이 이 노드의 contentSize 영역 안에 있는지. (worldMatrix 는 이 노드까지 누적한 행렬)
	//==============================================================================
	pub fn containsPoint(&self, worldMatrix: &TransformMatrix, viewPosition: Vector2) -> bool {
		let inverseMatrix = match worldMatrix.invert() {
			Some(inverseMatrix) => {
				inverseMatrix
			}
			None => {
				return false;
			}
		};
		let localPoint = inverseMatrix.transformPoint(viewPosition.x, viewPosition.y);
		let contentSize = self.transform.getContentSize();
		localPoint.x >= 0.0 && localPoint.y >= 0.0 && localPoint.x <= contentSize.x && localPoint.y <= contentSize.y
	}
}

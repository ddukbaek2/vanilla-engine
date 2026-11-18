//==============================================================================
// 트레일 렌더러 컴포넌트.
// - 원본 effect/trailrenderer.js 를 옮긴 것이다. 노드가 지나간 자리를 점으로 기록해 굵기 / 색이 잦아드는 띠로 그린다. (유니티 TrailRenderer 의 2D 대응)
// - 점은 노드의 로컬 위치(부모 좌표)로 기록하고, 그릴 때는 노드 원점 기준 상대 좌표로 바꿔 노드 트랜스폼 아래에서 그린다. (원본과 같다)
// - 세그먼트마다 drawLine 한 번이라 묶어 그리기로 한 드로우 콜에 모인다. (더하기 합성이면 블렌드가 바뀌는 앞뒤로 끊긴다)
// - 사용:
//     let trail = node.addComponent(TrailRenderer::new());
//     trail.setTime(0.5);
//     trail.setWidth(14.0, 0.0);
//     trail.setColors(Color::create(0.4, 0.8, 1.0, 0.9), Color::create(0.4, 0.8, 1.0, 0.0));
// - 원본과 다른 점: 원본은 색을 "rgba(r, g, b, a)" 문자열로 넘겨 RGB 가 0 ~ 255 정수로 반올림된다. 러스트 판도 같은 반올림을 해서 Color 로 넘긴다.
//==============================================================================
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{BlendMode, Graphic};
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
// 기록한 점.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct TrailPoint {
	x: f32,
	y: f32,
	age: f32,
}

pub struct TrailRenderer {
	componentBase: ComponentBase,
	pointList: Vec<TrailPoint>,
	// 점이 남는 시간. (초)
	pointLifetime: f32,
	// 이보다 가까우면 점을 추가하지 않는다.
	minVertexDistance: f32,
	startWidth: f32,
	endWidth: f32,
	startColor: Color,
	endColor: Color,
	isAdditive: bool,
	isEmitting: bool,
}

impl TrailRenderer {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> TrailRenderer {
		TrailRenderer {
			componentBase: ComponentBase::new(),
			pointList: Vec::new(),
			pointLifetime: 0.5,
			minVertexDistance: 3.0,
			startWidth: 10.0,
			endWidth: 0.0,
			startColor: Color::create(1.0, 1.0, 1.0, 0.9),
			endColor: Color::create(1.0, 1.0, 1.0, 0.0),
			isAdditive: false,
			isEmitting: true,
		}
	}

	//==============================================================================
	// 흔적 지우기.
	//==============================================================================
	pub fn clear(&mut self) {
		self.pointList.clear();
	}

	//==============================================================================
	// 점이 남는 시간 설정. (초)
	//==============================================================================
	pub fn setTime(&mut self, pointLifetime: f32) {
		self.pointLifetime = pointLifetime;
	}

	//==============================================================================
	// 점 사이 최소 거리 설정.
	//==============================================================================
	pub fn setMinVertexDistance(&mut self, minVertexDistance: f32) {
		self.minVertexDistance = minVertexDistance;
	}

	//==============================================================================
	// 굵기 설정. (새 점 쪽, 오래된 점 쪽)
	//==============================================================================
	pub fn setWidth(&mut self, startWidth: f32, endWidth: f32) {
		self.startWidth = startWidth;
		self.endWidth = endWidth;
	}

	//==============================================================================
	// 색 설정. (새 점 쪽, 오래된 점 쪽)
	//==============================================================================
	pub fn setColors(&mut self, startColor: impl Into<Color>, endColor: impl Into<Color>) {
		self.startColor = startColor.into();
		self.endColor = endColor.into();
	}

	//==============================================================================
	// 더하기 합성 여부 설정. (켜면 lighter)
	//==============================================================================
	pub fn setAdditive(&mut self, isAdditive: bool) {
		self.isAdditive = isAdditive;
	}

	//==============================================================================
	// 점 만들기 여부 설정. (거짓이면 새 점을 만들지 않는다. 남은 띠는 잦아든다)
	//==============================================================================
	pub fn setEmitting(&mut self, isEmitting: bool) {
		self.isEmitting = isEmitting;
	}

	//==============================================================================
	// 남아 있는 점 수 반환.
	//==============================================================================
	pub fn getPointCount(&self) -> usize {
		self.pointList.len()
	}
}

impl Default for TrailRenderer {
	fn default() -> TrailRenderer {
		TrailRenderer::new()
	}
}

impl Component for TrailRenderer {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"TrailRenderer"
	}

	//==============================================================================
	// 갱신. (점 기록 + 노화)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		let localPosition = context.getTransform().getLocalPosition();

		let pointLifetime = self.pointLifetime;
		for point in self.pointList.iter_mut() {
			point.age += timeDelta;
		}
		self.pointList.retain(|point| point.age < pointLifetime);

		if !self.isEmitting {
			return;
		}
		if let Some(lastPoint) = self.pointList.last() {
			let deltaX = localPosition.x - lastPoint.x;
			let deltaY = localPosition.y - lastPoint.y;
			if deltaX * deltaX + deltaY * deltaY < self.minVertexDistance * self.minVertexDistance {
				return;
			}
		}
		self.pointList.push(TrailPoint {
			x: localPosition.x,
			y: localPosition.y,
			age: 0.0,
		});
	}

	//==============================================================================
	// 출력. (세그먼트마다 굵기 / 색 보간, 노드 원점 기준 상대 좌표로 그린다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.pointList.len() < 2 {
			return;
		}
		if self.isAdditive {
			graphic.setBlendMode(BlendMode::lighter);
		}
		let nodePosition = transform.getLocalPosition();
		let startColor = self.startColor;
		let endColor = self.endColor;

		for pointIndex in 0..self.pointList.len() - 1 {
			let point = self.pointList[pointIndex];
			let nextPoint = self.pointList[pointIndex + 1];
			let ageRatio = point.age / self.pointLifetime;
			let freshRatio = 1.0 - ageRatio;
			let segmentWidth = (self.endWidth + (self.startWidth - self.endWidth) * freshRatio).max(0.1);
			let red = ((endColor.red + (startColor.red - endColor.red) * freshRatio) * 255.0).round() / 255.0;
			let green = ((endColor.green + (startColor.green - endColor.green) * freshRatio) * 255.0).round() / 255.0;
			let blue = ((endColor.blue + (startColor.blue - endColor.blue) * freshRatio) * 255.0).round() / 255.0;
			let alpha = endColor.alpha + (startColor.alpha - endColor.alpha) * freshRatio;
			graphic.setStrokeColor(Color::create(red, green, blue, alpha));
			graphic.drawLine(&[
				Vector2::create(point.x - nodePosition.x, point.y - nodePosition.y),
				Vector2::create(nextPoint.x - nodePosition.x, nextPoint.y - nodePosition.y),
			], segmentWidth);
		}

		if self.isAdditive {
			graphic.setBlendMode(BlendMode::normal);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::NodeTree;

	//==============================================================================
	// 움직인 만큼 점이 쌓이고, 가까우면 건너뛰고, 수명이 지나면 사라진다.
	//==============================================================================
	#[test]
	fn pointsFollowNodeAndExpire() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let trailId = nodeTree.createChild(rootId, "trail");
		let trail = nodeTree[trailId].addComponent(TrailRenderer::new());
		trail.setTime(0.5);
		trail.setMinVertexDistance(3.0);

		nodeTree.tick(&engine, 0.1);
		nodeTree[trailId].setLocalPosition(Vector2::create(1.0, 0.0));
		nodeTree.tick(&engine, 0.1);
		nodeTree[trailId].setLocalPosition(Vector2::create(10.0, 0.0));
		nodeTree.tick(&engine, 0.1);
		assert_eq!(nodeTree[trailId].getComponent::<TrailRenderer>().map(|trail| trail.getPointCount()), Some(2));

		if let Some(trail) = nodeTree[trailId].getComponentMut::<TrailRenderer>() {
			trail.setEmitting(false);
		}
		nodeTree.tick(&engine, 0.6);
		assert_eq!(nodeTree[trailId].getComponent::<TrailRenderer>().map(|trail| trail.getPointCount()), Some(0));
	}
}

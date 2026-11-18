//==============================================================================
// 비주얼 컴포넌트. (원본 experimental/visual/visual.js 의 Visual)
// - 원본은 위치(location) 하나만 드는 뼈대 컴포넌트다. 그리지도 갱신하지도 않는다. 러스트 판도 그대로다.
// - 원본 getLocation(location) 은 쓰지 않는 인자를 받는다. 러스트 판은 받지 않는다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};

//==============================================================================
// 비주얼 컴포넌트.
//==============================================================================
pub struct Visual {
	componentBase: ComponentBase,
	location: Vector2,
}

impl Visual {
	//==============================================================================
	// 생성. (위치 0)
	//==============================================================================
	pub fn new() -> Visual {
		Visual {
			componentBase: ComponentBase::new(),
			location: Vector2::zero(),
		}
	}

	//==============================================================================
	// 위치 설정.
	//==============================================================================
	pub fn setLocation(&mut self, location: Vector2) {
		self.location = location;
	}

	//==============================================================================
	// 위치 반환.
	//==============================================================================
	pub fn getLocation(&self) -> Vector2 {
		self.location
	}
}

impl Default for Visual {
	fn default() -> Visual {
		Visual::new()
	}
}

impl Component for Visual {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"Visual"
	}

	//==============================================================================
	// 갱신할 것이 없다.
	//==============================================================================
	fn isTickNeeded(&self) -> bool {
		false
	}
}

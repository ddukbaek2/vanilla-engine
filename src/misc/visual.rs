//==============================================================================
// 비주얼 컴포넌트. (원본 experimental/visual/visual.js 의 Visual)
// - 원본은 위치(location) 하나만 드는 뼈대 컴포넌트다. 그리지도 갱신하지도 않는다. 러스트 판도 그대로다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};

//==============================================================================
// 비주얼 컴포넌트.
//==============================================================================
pub struct Visual {
	component_base: ComponentBase,
	location: Vector2,
}

#[crate::reflect]
impl Visual {
	//==============================================================================
	// 생성. (위치 0)
	//==============================================================================
	pub fn new() -> Visual {
		Visual {
			component_base: ComponentBase::new(),
			location: Vector2::zero(),
		}
	}

	//==============================================================================
	// 위치 설정.
	//==============================================================================
	pub fn set_location(&mut self, location: Vector2) {
		self.location = location;
	}

	//==============================================================================
	// 위치 반환.
	//==============================================================================
	pub fn get_location(&self) -> Vector2 {
		self.location
	}
}

impl Default for Visual {
	fn default() -> Visual {
		Visual::new()
	}
}

#[crate::reflect]
impl Component for Visual {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Visual"
	}

	//==============================================================================
	// 갱신할 것이 없다.
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}
}

//==============================================================================
//! 2D 물리의 아이디. (몸, 모양, 관절)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/physicsid2d.js 와 같은 API 다.
//! - 월드의 칸 번호와 세대다. 없앤 칸은 다시 쓰이지만 세대가 올라가, 옛 아이디로는 새 것을 찾지 못한다. (Box2D 3 의 b2BodyId 와 같은 꼴)
//! - 원본은 같은 아이디 객체를 돌려줘 === 로 견주고, 러스트는 값 비교(==)다.
//==============================================================================

//==============================================================================
/// 몸 아이디.
///
/// # 예제
/// 몸 아이디로 몸 찾기
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, PhysicsWorld2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let body_id = world.create_body(&BodyDefinition2D::new());
/// assert_eq!(body_id.get_index(), 0);
/// assert!(world.get_body(body_id).is_some());
/// world.destroy_body(body_id);
/// assert!(world.get_body(body_id).is_none());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId2D {
	index: u32,
	generation: u32,
}

impl BodyId2D {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `index`: 칸 번호.
	/// - `generation`: 세대.
	///
	/// # 돌려줌
	/// 새 아이디.
	//==============================================================================
	pub fn new(index: usize, generation: u32) -> BodyId2D {
		BodyId2D {
			index: index as u32,
			generation,
		}
	}

	//==============================================================================
	/// 칸 번호 반환.
	///
	/// # 돌려줌
	/// 칸 번호.
	//==============================================================================
	pub fn get_index(&self) -> usize {
		self.index as usize
	}

	//==============================================================================
	/// 세대 반환.
	///
	/// # 돌려줌
	/// 세대.
	//==============================================================================
	pub fn get_generation(&self) -> u32 {
		self.generation
	}
}

//==============================================================================
/// 모양 아이디.
///
/// # 예제
/// 모양 아이디로 모양 찾기
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, PhysicsWorld2D, Polygon2D, ShapeDefinition2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let body_id = world.create_body(&BodyDefinition2D::new());
/// let shape_id = world.create_shape(body_id, &ShapeDefinition2D::new(), Polygon2D::make_box(1.0, 1.0)).expect("shape");
/// assert_eq!(world.get_shape(shape_id).expect("shape").get_body_id(), body_id);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShapeId2D {
	index: u32,
	generation: u32,
}

impl ShapeId2D {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `index`: 칸 번호.
	/// - `generation`: 세대.
	///
	/// # 돌려줌
	/// 새 아이디.
	//==============================================================================
	pub fn new(index: usize, generation: u32) -> ShapeId2D {
		ShapeId2D {
			index: index as u32,
			generation,
		}
	}

	//==============================================================================
	/// 칸 번호 반환.
	///
	/// # 돌려줌
	/// 칸 번호.
	//==============================================================================
	pub fn get_index(&self) -> usize {
		self.index as usize
	}

	//==============================================================================
	/// 세대 반환.
	///
	/// # 돌려줌
	/// 세대.
	//==============================================================================
	pub fn get_generation(&self) -> u32 {
		self.generation
	}
}

//==============================================================================
/// 관절 아이디.
///
/// # 예제
/// 관절 아이디로 관절 찾기
/// ```rust
/// use vanilla::experimental::physics2d::{BodyDefinition2D, BodyType2D, JointType2D, PhysicsWorld2D, RevoluteJointDefinition2D, WorldDefinition2D};
///
/// let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
/// let ground_id = world.create_body(&BodyDefinition2D::new());
/// let mut body_definition = BodyDefinition2D::new();
/// body_definition.body_type = BodyType2D::Dynamic;
/// let body_id = world.create_body(&body_definition);
/// let mut joint_definition = RevoluteJointDefinition2D::new();
/// joint_definition.body_id_a = Some(ground_id);
/// joint_definition.body_id_b = Some(body_id);
/// let joint_id = world.create_revolute_joint(&joint_definition).expect("joint");
/// assert_eq!(world.get_joint(joint_id).expect("joint").get_type(), JointType2D::Revolute);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JointId2D {
	index: u32,
	generation: u32,
}

impl JointId2D {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `index`: 칸 번호.
	/// - `generation`: 세대.
	///
	/// # 돌려줌
	/// 새 아이디.
	//==============================================================================
	pub fn new(index: usize, generation: u32) -> JointId2D {
		JointId2D {
			index: index as u32,
			generation,
		}
	}

	//==============================================================================
	/// 칸 번호 반환.
	///
	/// # 돌려줌
	/// 칸 번호.
	//==============================================================================
	pub fn get_index(&self) -> usize {
		self.index as usize
	}

	//==============================================================================
	/// 세대 반환.
	///
	/// # 돌려줌
	/// 세대.
	//==============================================================================
	pub fn get_generation(&self) -> u32 {
		self.generation
	}
}

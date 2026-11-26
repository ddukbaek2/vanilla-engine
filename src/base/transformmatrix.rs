//==============================================================================
//! 2D 아핀 변환 행렬. (Canvas2D 의 a, b, c, d, e, f 와 같은 배치)
//! | a c e |
//! | b d f |
//! | 0 0 1 |
//! - 원본 base/transformmatrix.js. 원본의 a ~ f 읽기 프로퍼티는 공개 필드로, clone 은 복사(Copy)로 갈음한다.
//! - invert 와 identity 는 러스트 판에서 먼저 만들었다.
//==============================================================================
use crate::base::vector2::Vector2;

//==============================================================================
/// 2D 아핀 변환 행렬. (a ~ f 를 공개 필드로 둔 Copy 값, 기본값은 단위 행렬)
/// - translate, rotate, scale 은 자신을 바꾸고 오른쪽에 곱한다. multiply 는 새 행렬을 돌려준다.
///
/// # 예제
/// 옮기고 키운 뒤 점 변환과 역변환
/// ```rust
/// use vanilla::{TransformMatrix, Vector2};
///
/// let mut transform = TransformMatrix::identity();
/// transform.translate(100.0, 50.0);
/// transform.scale(2.0, 2.0);
/// let point = transform.transform_point(10.0, 5.0);
/// assert_eq!(point, Vector2::create(120.0, 60.0));
///
/// let inverse = transform.invert().unwrap();
/// assert_eq!(inverse.transform_point(120.0, 60.0), Vector2::create(10.0, 5.0));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformMatrix {
	/// x 축 벡터의 x. (새 x = a * x + c * y + e)
	pub a: f64,
	/// x 축 벡터의 y. (새 y = b * x + d * y + f)
	pub b: f64,
	/// y 축 벡터의 x.
	pub c: f64,
	/// y 축 벡터의 y.
	pub d: f64,
	/// 이동 x.
	pub e: f64,
	/// 이동 y.
	pub f: f64,
}

impl Default for TransformMatrix {
	//==============================================================================
	/// 단위 행렬.
	//==============================================================================
	fn default() -> TransformMatrix {
		TransformMatrix::identity()
	}
}

impl TransformMatrix {
	//==============================================================================
	/// 단위 행렬.
	///
	/// # 돌려줌
	/// a = d = 1, 나머지가 0 인 행렬.
	//==============================================================================
	pub const fn identity() -> TransformMatrix {
		TransformMatrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
	}

	//==============================================================================
	/// 단위 행렬로 초기화.
	//==============================================================================
	pub fn set_identity(&mut self) {
		*self = TransformMatrix::identity();
	}

	//==============================================================================
	/// 값 직접 설정.
	///
	/// # 인자
	/// - `a`: x 축 벡터의 x.
	/// - `b`: x 축 벡터의 y.
	/// - `c`: y 축 벡터의 x.
	/// - `d`: y 축 벡터의 y.
	/// - `e`: 이동 x.
	/// - `f`: 이동 y.
	//==============================================================================
	pub fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
		self.a = a;
		self.b = b;
		self.c = c;
		self.d = d;
		self.e = e;
		self.f = f;
	}

	//==============================================================================
	/// 다른 행렬 값 복사.
	///
	/// # 인자
	/// - `other`: 복사해 올 행렬.
	//==============================================================================
	pub fn copy_from(&mut self, other: &TransformMatrix) {
		*self = *other;
	}

	//==============================================================================
	/// 이동 누적. (오른쪽에 곱한다)
	///
	/// # 인자
	/// - `x`: x 이동량. (지금 변환의 로컬 좌표)
	/// - `y`: y 이동량. (지금 변환의 로컬 좌표)
	//==============================================================================
	pub fn translate(&mut self, x: f64, y: f64) {
		self.e += self.a * x + self.c * y;
		self.f += self.b * x + self.d * y;
	}

	//==============================================================================
	/// 회전 누적. (라디안, 오른쪽에 곱한다)
	///
	/// # 인자
	/// - `radian`: 회전 각. (라디안, 양수면 +X 를 +Y 쪽으로 돌린다, 0 이면 아무것도 하지 않는다)
	///
	/// # 예제
	/// 90 도 돌리면 +X 가 +Y 로 간다
	/// ```rust
	/// use vanilla::{math, TransformMatrix};
	///
	/// let mut transform = TransformMatrix::identity();
	/// transform.rotate(math::PI * 0.5);
	/// let point = transform.transform_point(1.0, 0.0);
	/// assert!(point.x.abs() < 1e-6);
	/// assert!((point.y - 1.0).abs() < 1e-6);
	/// ```
	//==============================================================================
	pub fn rotate(&mut self, radian: f64) {
		if radian == 0.0 {
			return;
		}
		let cosine = radian.cos();
		let sine = radian.sin();
		let a = self.a * cosine + self.c * sine;
		let b = self.b * cosine + self.d * sine;
		let c = self.c * cosine - self.a * sine;
		let d = self.d * cosine - self.b * sine;
		self.a = a;
		self.b = b;
		self.c = c;
		self.d = d;
	}

	//==============================================================================
	/// 크기 누적. (오른쪽에 곱한다)
	///
	/// # 인자
	/// - `x`: x 배율. (1 이면 그대로)
	/// - `y`: y 배율. (1 이면 그대로)
	//==============================================================================
	pub fn scale(&mut self, x: f64, y: f64) {
		self.a *= x;
		self.b *= x;
		self.c *= y;
		self.d *= y;
	}

	//==============================================================================
	/// 행렬 곱. (self * other)
	///
	/// # 인자
	/// - `other`: 오른쪽에 곱할 행렬. (점에는 other 가 먼저 걸린다)
	///
	/// # 돌려줌
	/// self x other 인 새 행렬. (자신은 바뀌지 않는다)
	///
	/// # 예제
	/// 부모 변환에 자식 변환 곱하기
	/// ```rust
	/// use vanilla::{TransformMatrix, Vector2};
	///
	/// let mut parent = TransformMatrix::identity();
	/// parent.translate(100.0, 0.0);
	/// let mut child = TransformMatrix::identity();
	/// child.scale(2.0, 2.0);
	/// let world = parent.multiply(&child);
	/// assert_eq!(world.transform_point(10.0, 10.0), Vector2::create(120.0, 20.0));
	/// ```
	//==============================================================================
	pub fn multiply(&self, other: &TransformMatrix) -> TransformMatrix {
		TransformMatrix {
			a: self.a * other.a + self.c * other.b,
			b: self.b * other.a + self.d * other.b,
			c: self.a * other.c + self.c * other.d,
			d: self.b * other.c + self.d * other.d,
			e: self.a * other.e + self.c * other.f + self.e,
			f: self.b * other.e + self.d * other.f + self.f,
		}
	}

	//==============================================================================
	/// 역행렬. (행렬식이 0 이면 None)
	///
	/// # 돌려줌
	/// 역행렬. 행렬식(a * d - b * c)이 0 이면 None.
	///
	/// # 예제
	/// 한 축 배율이 0 이면 뒤집을 수 없다
	/// ```rust
	/// use vanilla::TransformMatrix;
	///
	/// let mut flat = TransformMatrix::identity();
	/// flat.scale(0.0, 1.0);
	/// assert!(flat.invert().is_none());
	/// assert!(TransformMatrix::identity().invert().is_some());
	/// ```
	//==============================================================================
	pub fn invert(&self) -> Option<TransformMatrix> {
		let determinant = self.a * self.d - self.b * self.c;
		if determinant == 0.0 {
			return None;
		}
		let inverse_determinant = 1.0 / determinant;
		Some(TransformMatrix {
			a: self.d * inverse_determinant,
			b: -self.b * inverse_determinant,
			c: -self.c * inverse_determinant,
			d: self.a * inverse_determinant,
			e: (self.c * self.f - self.d * self.e) * inverse_determinant,
			f: (self.b * self.e - self.a * self.f) * inverse_determinant,
		})
	}

	//==============================================================================
	/// 점 변환.
	///
	/// # 인자
	/// - `x`: 점의 x.
	/// - `y`: 점의 y.
	///
	/// # 돌려줌
	/// 변환한 점. (a * x + c * y + e, b * x + d * y + f)
	//==============================================================================
	pub fn transform_point(&self, x: f64, y: f64) -> Vector2 {
		Vector2::create(self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
	}

	//==============================================================================
	/// 두 축 가운데 큰 쪽의 크기 배율.
	///
	/// # 돌려줌
	/// x 축 벡터(a, b)와 y 축 벡터(c, d)의 길이 가운데 큰 값.
	//==============================================================================
	pub fn get_maximum_scale(&self) -> f64 {
		let scale_x = (self.a * self.a + self.b * self.b).sqrt();
		let scale_y = (self.c * self.c + self.d * self.d).sqrt();
		scale_x.max(scale_y)
	}

	//==============================================================================
	/// 셰이더 mat3 (열 우선) 배열로 기록. (원본 writeToFloat32Array)
	///
	/// # 인자
	/// - `array`: 옮겨 쓸 배열. (열 우선 9 개: a, b, 0, c, d, 0, e, f, 1)
	///
	/// # 예제
	/// 이동 행렬을 mat3 배열로
	/// ```rust
	/// use vanilla::TransformMatrix;
	///
	/// let mut transform = TransformMatrix::identity();
	/// transform.translate(3.0, 4.0);
	/// let mut array = [0.0f32; 9];
	/// transform.write_to_float32_array(&mut array);
	/// assert_eq!(array, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 3.0, 4.0, 1.0]);
	/// ```
	//==============================================================================
	pub fn write_to_float32_array(&self, array: &mut [f32; 9]) {
		array[0] = self.a as f32;
		array[1] = self.b as f32;
		array[2] = 0.0;
		array[3] = self.c as f32;
		array[4] = self.d as f32;
		array[5] = 0.0;
		array[6] = self.e as f32;
		array[7] = self.f as f32;
		array[8] = 1.0;
	}
}

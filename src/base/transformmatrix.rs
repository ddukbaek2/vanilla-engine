//==============================================================================
// 2D 아핀 변환 행렬. (Canvas2D 의 a, b, c, d, e, f 와 같은 배치)
// | a c e |
// | b d f |
// | 0 0 1 |
//==============================================================================
use crate::base::vector2::Vector2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformMatrix {
	pub a: f32,
	pub b: f32,
	pub c: f32,
	pub d: f32,
	pub e: f32,
	pub f: f32,
}

impl Default for TransformMatrix {
	fn default() -> TransformMatrix {
		TransformMatrix::identity()
	}
}

impl TransformMatrix {
	//==============================================================================
	// 단위 행렬.
	//==============================================================================
	pub const fn identity() -> TransformMatrix {
		TransformMatrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
	}

	//==============================================================================
	// 단위 행렬로 초기화.
	//==============================================================================
	pub fn setIdentity(&mut self) {
		*self = TransformMatrix::identity();
	}

	//==============================================================================
	// 값 직접 설정.
	//==============================================================================
	pub fn setTransform(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
		self.a = a;
		self.b = b;
		self.c = c;
		self.d = d;
		self.e = e;
		self.f = f;
	}

	//==============================================================================
	// 이동 누적. (오른쪽에 곱한다)
	//==============================================================================
	pub fn translate(&mut self, x: f32, y: f32) {
		self.e += self.a * x + self.c * y;
		self.f += self.b * x + self.d * y;
	}

	//==============================================================================
	// 회전 누적. (라디안, 오른쪽에 곱한다)
	//==============================================================================
	pub fn rotate(&mut self, radian: f32) {
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
	// 크기 누적. (오른쪽에 곱한다)
	//==============================================================================
	pub fn scale(&mut self, x: f32, y: f32) {
		self.a *= x;
		self.b *= x;
		self.c *= y;
		self.d *= y;
	}

	//==============================================================================
	// 행렬 곱. (self * other)
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
	// 역행렬. (행렬식이 0 이면 None)
	//==============================================================================
	pub fn invert(&self) -> Option<TransformMatrix> {
		let determinant = self.a * self.d - self.b * self.c;
		if determinant == 0.0 {
			return None;
		}
		let inverseDeterminant = 1.0 / determinant;
		Some(TransformMatrix {
			a: self.d * inverseDeterminant,
			b: -self.b * inverseDeterminant,
			c: -self.c * inverseDeterminant,
			d: self.a * inverseDeterminant,
			e: (self.c * self.f - self.d * self.e) * inverseDeterminant,
			f: (self.b * self.e - self.a * self.f) * inverseDeterminant,
		})
	}

	//==============================================================================
	// 점 변환.
	//==============================================================================
	pub fn transformPoint(&self, x: f32, y: f32) -> Vector2 {
		Vector2::create(self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
	}

	//==============================================================================
	// 두 축 가운데 큰 쪽의 크기 배율.
	//==============================================================================
	pub fn getMaximumScale(&self) -> f32 {
		let scaleX = (self.a * self.a + self.b * self.b).sqrt();
		let scaleY = (self.c * self.c + self.d * self.d).sqrt();
		scaleX.max(scaleY)
	}

	//==============================================================================
	// 셰이더 mat3 (열 우선) 배열로 기록.
	//==============================================================================
	pub fn writeToArray(&self, array: &mut [f32; 9]) {
		array[0] = self.a;
		array[1] = self.b;
		array[2] = 0.0;
		array[3] = self.c;
		array[4] = self.d;
		array[5] = 0.0;
		array[6] = self.e;
		array[7] = self.f;
		array[8] = 1.0;
	}
}

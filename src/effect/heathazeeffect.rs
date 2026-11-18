//==============================================================================
// 아지랑이 효과 컴포넌트. (원본 experimental/heathazeeffect.js 의 HeatHazeEffect)
// - 이 노드가 그려지기 전까지 화면에 그려진 것을 노드 영역만큼 떠 와서, 한 줄씩 사인 곡선을 따라 좌우로 흔들어 다시 그린다.
//   그래서 씬의 맨 위(마지막)에 그려지는 노드에 붙여야 보인다. 자식 노드는 효과 위에 그려진다.
// - 원본은 Canvas2D 로 캔버스 자신을 줄마다 drawImage 한다. 러스트 판은 묶음을 내보낸 뒤 노드의 화면 영역(픽셀)을
//   copyTexSubImage2D 로 텍스처에 떠 오고(ScreenEffect 와 같은 방식), 줄마다 쿼드 한 장씩 정점 색 쿼드(drawColoredQuads)로 그린다.
// - 원본과 다른 점:
//   - 원본은 TransformNode 를 상속한 노드다. 러스트 판은 노드에 붙이는 컴포넌트다. (노드 하나 = 효과 하나)
//   - 원본 생성자는 노드 contentSize 를 300 x 300 으로 정한다. 러스트 판은 붙을 때 노드 contentSize 가 비어 있을 때만 정한다.
//   - 떠 온 텍스처는 GL 자원이라 다 쓰면 destroy(graphic) 를 부른다. (ScreenEffect 와 같다. 부르지 않으면 컨텍스트가 없어질 때까지 남는다)
//   - 줄 수는 한 번에 그릴 수 있는 정점 수(8192 = 1365 줄)로 막는다. 노드가 그보다 높으면 한 줄이 1 픽셀보다 두꺼워진다.
//   - 노드 영역이 화면 밖으로 나가면 원본은 그 부분을 비워 두고, 러스트 판은 화면 가장자리 픽셀을 늘인다. (CLAMP_TO_EDGE)
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{GlTexture, Graphic, PARTICLE_FLOATS_PER_VERTEX};
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;
use glow::HasContext;

// 붙을 때 contentSize 가 비어 있으면 쓰는 크기. (원본 생성자)
const DEFAULT_CONTENT_SIZE: f32 = 300.0;

// 한 번에 그리는 줄 수 한도. (drawColoredQuads 정점 한도 8192 / 줄마다 6 정점)
const MAXIMUM_ROW_COUNT: usize = 1365;

//==============================================================================
// 아지랑이 효과 컴포넌트.
//==============================================================================
pub struct HeatHazeEffect {
	componentBase: ComponentBase,
	timer: f32,
	// 흔들리는 속도.
	speed: f32,
	// 흔들리는 폭. (강도, 픽셀)
	amplitude: f32,
	// 흔들리는 빈도. (물결의 촘촘함)
	frequency: f32,
	captureTexture: Option<GlTexture>,
	captureTextureWidth: i32,
	captureTextureHeight: i32,
}

impl HeatHazeEffect {
	//==============================================================================
	// 생성. (속도 2, 폭 3, 빈도 0.05)
	//==============================================================================
	pub fn new() -> HeatHazeEffect {
		HeatHazeEffect {
			componentBase: ComponentBase::new(),
			timer: 0.0,
			speed: 2.0,
			amplitude: 3.0,
			frequency: 0.05,
			captureTexture: None,
			captureTextureWidth: 0,
			captureTextureHeight: 0,
		}
	}

	//==============================================================================
	// 속도 설정.
	//==============================================================================
	pub fn setSpeed(&mut self, value: f32) {
		self.speed = value;
	}

	//==============================================================================
	// 속도 반환.
	//==============================================================================
	pub fn getSpeed(&self) -> f32 {
		self.speed
	}

	//==============================================================================
	// 강도 설정.
	//==============================================================================
	pub fn setAmplitude(&mut self, value: f32) {
		self.amplitude = value;
	}

	//==============================================================================
	// 강도 반환.
	//==============================================================================
	pub fn getAmplitude(&self) -> f32 {
		self.amplitude
	}

	//==============================================================================
	// 빈도 설정.
	//==============================================================================
	pub fn setFrequency(&mut self, value: f32) {
		self.frequency = value;
	}

	//==============================================================================
	// 빈도 반환.
	//==============================================================================
	pub fn getFrequency(&self) -> f32 {
		self.frequency
	}

	//==============================================================================
	// 흔들림 시계 반환. (tick 마다 timeDelta * 속도씩 흐른다)
	//==============================================================================
	pub fn getTimer(&self) -> f32 {
		self.timer
	}

	//==============================================================================
	// 줄의 좌우 흔들림. (원본: sin(시계 + 줄 위치 * 빈도) * 폭)
	//==============================================================================
	pub fn calculateRowOffset(&self, localY: f32) -> f32 {
		(self.timer + localY * self.frequency).sin() * self.amplitude
	}

	//==============================================================================
	// GL 자원 해제. (떠 온 텍스처)
	//==============================================================================
	pub fn destroy(&mut self, graphic: &mut Graphic) {
		if let Some(captureTexture) = self.captureTexture.take() {
			unsafe {
				graphic.getGL().delete_texture(captureTexture);
			}
		}
		self.captureTextureWidth = 0;
		self.captureTextureHeight = 0;
	}

	//==============================================================================
	// 떠 올 텍스처 준비. (크기가 바뀌면 다시 잡는다. 실패하면 None)
	//==============================================================================
	fn ensureCaptureTexture(&mut self, graphic: &Graphic, width: i32, height: i32) -> Option<GlTexture> {
		let gl = graphic.getGL();
		if let Some(captureTexture) = self.captureTexture {
			if self.captureTextureWidth == width && self.captureTextureHeight == height {
				return Some(captureTexture);
			}
			unsafe {
				gl.delete_texture(captureTexture);
			}
			self.captureTexture = None;
		}
		let captureTexture = unsafe {
			let captureTexture = match gl.create_texture() {
				Ok(captureTexture) => {
					captureTexture
				}
				Err(error) => {
					eprintln!("HeatHazeEffect: {}", error);
					return None;
				}
			};
			gl.bind_texture(glow::TEXTURE_2D, Some(captureTexture));
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGB8 as i32, width, height, 0, glow::RGB, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			captureTexture
		};
		self.captureTexture = Some(captureTexture);
		self.captureTextureWidth = width;
		self.captureTextureHeight = height;
		Some(captureTexture)
	}
}

impl Default for HeatHazeEffect {
	fn default() -> HeatHazeEffect {
		HeatHazeEffect::new()
	}
}

impl Component for HeatHazeEffect {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"HeatHazeEffect"
	}

	//==============================================================================
	// 붙을 때. (노드 contentSize 가 비어 있으면 300 x 300)
	//==============================================================================
	fn attach(&mut self, transform: &mut NodeTransform) {
		let contentSize = transform.getContentSize();
		if contentSize.x <= 0.0 && contentSize.y <= 0.0 {
			transform.setContentSize(Vector2::create(DEFAULT_CONTENT_SIZE, DEFAULT_CONTENT_SIZE));
		}
	}

	//==============================================================================
	// 갱신. (흔들림 시계를 흘린다)
	//==============================================================================
	fn tick(&mut self, _context: &mut ComponentContext, timeDelta: f32) {
		self.timer += timeDelta * self.speed;
	}

	//==============================================================================
	// 출력. (지금까지 그려진 화면의 노드 영역을 떠 와서 줄마다 흔들어 그린다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let contentSize = transform.getContentSize();
		if contentSize.x <= 0.0 || contentSize.y <= 0.0 {
			return;
		}
		// 앞서 그린 것이 화면에 다 나가야 떠 올 수 있다.
		graphic.flushBatch();

		// 노드 영역의 화면 바운딩 박스. (드로잉 버퍼 픽셀, 왼쪽 위 원점. 원본 getWorldBounds)
		let transformMatrix = graphic.getTransformMatrix();
		let corners = [
			transformMatrix.transformPoint(0.0, 0.0),
			transformMatrix.transformPoint(contentSize.x, 0.0),
			transformMatrix.transformPoint(0.0, contentSize.y),
			transformMatrix.transformPoint(contentSize.x, contentSize.y),
		];
		let minimumX = corners.iter().map(|corner| corner.x).fold(f32::INFINITY, f32::min);
		let maximumX = corners.iter().map(|corner| corner.x).fold(f32::NEG_INFINITY, f32::max);
		let minimumY = corners.iter().map(|corner| corner.y).fold(f32::INFINITY, f32::min);
		let maximumY = corners.iter().map(|corner| corner.y).fold(f32::NEG_INFINITY, f32::max);
		let boundsWidth = maximumX - minimumX;
		let boundsHeight = maximumY - minimumY;
		if boundsWidth <= 0.0 || boundsHeight <= 0.0 {
			return;
		}

		// 화면 안으로 자른 복사 영역.
		let bufferWidth = graphic.getDrawingBufferWidth();
		let bufferHeight = graphic.getDrawingBufferHeight();
		let regionX = (minimumX.floor() as i32).max(0);
		let regionY = (minimumY.floor() as i32).max(0);
		let regionRight = (maximumX.ceil() as i32).min(bufferWidth);
		let regionBottom = (maximumY.ceil() as i32).min(bufferHeight);
		let regionWidth = regionRight - regionX;
		let regionHeight = regionBottom - regionY;
		if regionWidth <= 0 || regionHeight <= 0 {
			return;
		}

		// 떠 오기. (GL 은 아래가 0 이라 텍스처 0 줄이 영역 아래쪽이다)
		let captureTexture = match self.ensureCaptureTexture(graphic, regionWidth, regionHeight) {
			Some(captureTexture) => {
				captureTexture
			}
			None => {
				return;
			}
		};
		unsafe {
			let gl = graphic.getGL();
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, Some(captureTexture));
			gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, regionX, bufferHeight - regionBottom, regionWidth, regionHeight);
		}

		// 줄마다 쿼드. (x, y, u, v, r, g, b, a)
		let rowCount = (contentSize.y.ceil() as usize).clamp(1, MAXIMUM_ROW_COUNT);
		let rowHeight = contentSize.y / rowCount as f32;
		let leftU = (minimumX - regionX as f32) / regionWidth as f32;
		let rightU = (maximumX - regionX as f32) / regionWidth as f32;
		let mut vertexValues: Vec<f32> = Vec::with_capacity(rowCount * 6 * PARTICLE_FLOATS_PER_VERTEX);
		for rowIndex in 0..rowCount {
			let topY = rowIndex as f32 * rowHeight;
			let bottomY = topY + rowHeight;
			let offsetX = self.calculateRowOffset(topY);
			let sourceTopY = minimumY + topY / contentSize.y * boundsHeight;
			let sourceBottomY = minimumY + bottomY / contentSize.y * boundsHeight;
			let topV = 1.0 - (sourceTopY - regionY as f32) / regionHeight as f32;
			let bottomV = 1.0 - (sourceBottomY - regionY as f32) / regionHeight as f32;
			let leftX = offsetX;
			let rightX = offsetX + contentSize.x;
			let corners: [(f32, f32, f32, f32); 4] = [
				(leftX, topY, leftU, topV),
				(rightX, topY, rightU, topV),
				(leftX, bottomY, leftU, bottomV),
				(rightX, bottomY, rightU, bottomV),
			];
			for cornerIndex in [0usize, 1, 2, 2, 1, 3] {
				let (cornerX, cornerY, cornerU, cornerV) = corners[cornerIndex];
				vertexValues.extend_from_slice(&[cornerX, cornerY, cornerU, cornerV, 1.0, 1.0, 1.0, 1.0]);
			}
		}
		let particleVertexData = graphic.getParticleVertexData(vertexValues.len());
		let writableFloatCount = particleVertexData.len().min(vertexValues.len());
		particleVertexData[..writableFloatCount].copy_from_slice(&vertexValues[..writableFloatCount]);
		graphic.drawColoredQuads(writableFloatCount / PARTICLE_FLOATS_PER_VERTEX, captureTexture);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 붙으면 빈 노드를 300 x 300 으로 정하고, 이미 크기가 있으면 두며, 시계는 속도만큼 흐르고 줄 흔들림은 사인 곡선이다.
	//==============================================================================
	#[test]
	fn attachSizeAndRowOffset() {
		let mut emptyTransform = NodeTransform::new();
		let mut heatHazeEffect = HeatHazeEffect::new();
		heatHazeEffect.attach(&mut emptyTransform);
		assert_eq!(emptyTransform.getContentSize(), Vector2::create(300.0, 300.0));
		let mut sizedTransform = NodeTransform::new();
		sizedTransform.setContentSize(Vector2::create(64.0, 32.0));
		heatHazeEffect.attach(&mut sizedTransform);
		assert_eq!(sizedTransform.getContentSize(), Vector2::create(64.0, 32.0));

		heatHazeEffect.setSpeed(4.0);
		heatHazeEffect.setAmplitude(5.0);
		heatHazeEffect.setFrequency(0.1);
		heatHazeEffect.timer += 0.25 * heatHazeEffect.getSpeed();
		assert_eq!(heatHazeEffect.getTimer(), 1.0);
		assert!((heatHazeEffect.calculateRowOffset(10.0) - 2.0f32.sin() * 5.0).abs() < 0.00001);
	}
}

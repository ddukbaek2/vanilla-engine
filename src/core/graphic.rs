//==============================================================================
// 그래픽.
// - GL 컨텍스트로 출력하는 렌더러. 원본(WebGL2)과 같은 셰이더와 같은 상태 모델을 쓴다.
// - Canvas2D 시절의 좌표계(좌상 원점, 픽셀 단위)와 상태 모델(변환/알파 스택)을 그대로 재현한다.
// - 모든 드로우는 drawVertices() 한 지점으로 수렴한다. 단색 도형도 1x1 흰색 텍스처로 텍스처 경로와 통일.
// - 클리핑은 스텐실 버퍼 기반. 텍스트는 문자열 텍스처로 구워 출력.
// - 묶어 그리기(기본 켬): 같은 텍스처를 잇달아 그리면 정점을 CPU 에서 변환해 모았다가 한 번에 그린다.
//   정점마다 색을 싣는 파티클 셰이더를 같이 쓴다. 틴트가 걸린 그리기와 클립 영역 기록은 묶지 않는다.
//   블렌드, 클립, 파티클 그리기, 프레임 끝에서 모은 것을 먼저 내보낸다(flushBatch). 원본은 setSpriteBatchingEnabled 로 켜야 묶는다.
// - 화면 밖 거르기(기본 켬): NodeTree 가 노드를 그리기 전에 isLocalRectVisible 로 묻는다. 원본은 전부 그린다.
// - 셰이더 바꿔 끼우기(setShaderProgramOverride): ShaderSprite 같은 효과가 기본 프로그램 대신 제 프로그램으로 그린다. 바꿔 끼운 동안은 묶지 않는다.
//   바꿔 끼울 프로그램은 resolveShaderProgram 이 이름으로 담아 두고, 어트리뷰트 위치를 기본 프로그램과 같게 고정해 기본 버텍스 어레이를 같이 쓴다.
// - 원본에서 옮기지 않은 것: 오른쪽에서 왼쪽 글자 방향, 밉맵 바이어스 설정.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::rect::Rect;
use crate::base::transformmatrix::TransformMatrix;
use crate::base::vector2::Vector2;
use crate::core::image::Image;
use crate::core::imagetexturecache::ImageTextureCache;
use crate::core::shaderprogram::ShaderProgram;
use crate::core::textstringtexturecache::{TextDrawKind, TextEntry, TextMetrics, TextStringTextureCache};
use glow::HasContext;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

// GL 객체 타입. (데스크탑은 glow::Native*, 웹은 WebGL 객체. glow 컨텍스트가 정한 것을 그대로 쓴다)
pub type GlTexture = <glow::Context as glow::HasContext>::Texture;
pub type GlBuffer = <glow::Context as glow::HasContext>::Buffer;
pub type GlVertexArray = <glow::Context as glow::HasContext>::VertexArray;
pub type GlProgram = <glow::Context as glow::HasContext>::Program;
pub type GlShader = <glow::Context as glow::HasContext>::Shader;
pub type GlUniformLocation = <glow::Context as glow::HasContext>::UniformLocation;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 버텍스 셰이더. (픽셀 좌표 → 클립 좌표)
const VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexPosition;
in vec2 vertexTextureCoordinate;
uniform mat3 projectionMatrix;
uniform mat3 modelMatrix;
out vec2 fragmentTextureCoordinate;
void main() {
	vec3 transformedPosition = projectionMatrix * (modelMatrix * vec3(vertexPosition, 1.0));
	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	fragmentTextureCoordinate = vertexTextureCoordinate;
}
";

// 프래그먼트 셰이더. (프리멀티플라이드 알파 출력)
// - 단색 도형은 1x1 흰색 텍스처를 바인드해 같은 경로로 처리한다.
// - tintColor 는 이미지 실루엣(알파) 안쪽에만 색을 덮는 틴트. (alpha 0 이면 비활성)
const FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D mainTexture;
uniform vec4 mainColor;
uniform vec4 tintColor;
uniform float globalAlpha;
uniform float textureLodBias;
out vec4 outputColor;
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate, textureLodBias);
	vec3 tintedColor = mix(textureColor.rgb, tintColor.rgb * textureColor.a, tintColor.a);
	float finalAlpha = textureColor.a * mainColor.a * globalAlpha;
	vec3 finalColor = tintedColor * mainColor.rgb * mainColor.a * globalAlpha;
	outputColor = vec4(finalColor, finalAlpha);
}
";

// 파티클 배치 버텍스 셰이더. (정점마다 색을 실어 한 번의 드로우로 수백 개를 그린다)
const PARTICLE_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexPosition;
in vec2 vertexTextureCoordinate;
in vec4 vertexColor;
uniform mat3 projectionMatrix;
uniform mat3 modelMatrix;
out vec2 fragmentTextureCoordinate;
out vec4 fragmentColor;
void main() {
	vec3 transformedPosition = projectionMatrix * (modelMatrix * vec3(vertexPosition, 1.0));
	gl_Position = vec4(transformedPosition.xy, 0.0, 1.0);
	fragmentTextureCoordinate = vertexTextureCoordinate;
	fragmentColor = vertexColor;
}
";

// 파티클 배치 프래그먼트 셰이더. (프리멀티플라이드 알파 출력)
const PARTICLE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
in vec4 fragmentColor;
uniform sampler2D mainTexture;
uniform float globalAlpha;
uniform float textureLodBias;
out vec4 outputColor;
void main() {
	vec4 textureColor = texture(mainTexture, fragmentTextureCoordinate, textureLodBias);
	float finalAlpha = textureColor.a * fragmentColor.a * globalAlpha;
	vec3 finalColor = textureColor.rgb * fragmentColor.rgb * fragmentColor.a * globalAlpha;
	outputColor = vec4(finalColor, finalAlpha);
}
";

// 버텍스 1개당 float 수. (x, y, u, v)
pub const FLOATS_PER_VERTEX: usize = 4;
// 파티클 버텍스 1개당 float 수. (x, y, u, v, r, g, b, a)
pub const PARTICLE_FLOATS_PER_VERTEX: usize = 8;
const PARTICLE_VERTEX_CAPACITY: usize = 8192;

// 스크래치 버텍스 버퍼 용량. (버텍스 수)
const VERTEX_CAPACITY: usize = 4096;

// 묶어 그리기 버퍼 용량. (버텍스 수, 넘으면 내보내고 다시 모은다)
const BATCH_VERTEX_CAPACITY: usize = 6 * 8192;

// 라운드 사각형 / 원의 코너 아크 분할 수.
const CORNER_SEGMENT_COUNT: usize = 8;
const ROUND_RECT_POSITION_COUNT: usize = 4 * (CORNER_SEGMENT_COUNT + 1);

// 둥근 사각형 꼭짓점의 단위 원 좌표 표. (코너 4 개 x 분할 9 개, 우상 → 우하 → 좌하 → 좌상)
fn buildRoundRectUnitTable() -> [(f32, f32); ROUND_RECT_POSITION_COUNT] {
	let startRadianList: [f32; 4] = [-math::PI / 2.0, 0.0, math::PI / 2.0, math::PI];
	let mut unitTable = [(0.0, 0.0); ROUND_RECT_POSITION_COUNT];
	for (cornerIndex, startRadian) in startRadianList.into_iter().enumerate() {
		for segmentIndex in 0..=CORNER_SEGMENT_COUNT {
			let radian = startRadian + (math::PI / 2.0) * (segmentIndex as f32 / CORNER_SEGMENT_COUNT as f32);
			unitTable[cornerIndex * (CORNER_SEGMENT_COUNT + 1) + segmentIndex] = (radian.cos(), radian.sin());
		}
	}
	unitTable
}

// 텍스트 베이크 스케일 상한. (과도한 확대 굽기 방지)
const MAXIMUM_TEXT_BAKE_SCALE: f32 = 8.0;
const DEFAULT_TEXT_BAKE_SCALE_STEP: f32 = 0.25;

// 부드러운 원 텍스처 크기.
const SOFT_DISC_TEXTURE_SIZE: usize = 64;

// 원본 이미지가 사라진 텍스처를 치우는 간격. (프레임)
const IMAGE_TEXTURE_SWEEP_INTERVAL: u64 = 600;

//==============================================================================
// 블렌드 모드. (원본 SpriteBlendMode, Canvas2D globalCompositeOperation 대응)
// - 고정 블렌딩으로 재현 가능한 9 종만 실제로 적용하고, 나머지는 normal(source-over)로 폴백한다. (원본과 같음)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
	normal,
	sourceIn,
	sourceOut,
	sourceAtop,
	destinationOver,
	destinationIn,
	destinationOut,
	destinationAtop,
	lighter,
	copy,
	xor,
	multiply,
	screen,
	overlay,
	darken,
	lighten,
	colorDodge,
	colorBurn,
	hardLight,
	softLight,
	difference,
	exclusion,
	hue,
	saturation,
	color,
	luminosity,
}

// 블렌드 모드와 원본 이름(globalCompositeOperation 문자열)의 짝. (선언 차례)
const BLEND_MODE_NAME_TABLE: [(BlendMode, &str); 26] = [
	(BlendMode::normal, "source-over"),
	(BlendMode::sourceIn, "source-in"),
	(BlendMode::sourceOut, "source-out"),
	(BlendMode::sourceAtop, "source-atop"),
	(BlendMode::destinationOver, "destination-over"),
	(BlendMode::destinationIn, "destination-in"),
	(BlendMode::destinationOut, "destination-out"),
	(BlendMode::destinationAtop, "destination-atop"),
	(BlendMode::lighter, "lighter"),
	(BlendMode::copy, "copy"),
	(BlendMode::xor, "xor"),
	(BlendMode::multiply, "multiply"),
	(BlendMode::screen, "screen"),
	(BlendMode::overlay, "overlay"),
	(BlendMode::darken, "darken"),
	(BlendMode::lighten, "lighten"),
	(BlendMode::colorDodge, "color-dodge"),
	(BlendMode::colorBurn, "color-burn"),
	(BlendMode::hardLight, "hard-light"),
	(BlendMode::softLight, "soft-light"),
	(BlendMode::difference, "difference"),
	(BlendMode::exclusion, "exclusion"),
	(BlendMode::hue, "hue"),
	(BlendMode::saturation, "saturation"),
	(BlendMode::color, "color"),
	(BlendMode::luminosity, "luminosity"),
];

impl BlendMode {
	//==============================================================================
	// 원본 이름 반환. ("source-over" 같은 문자열)
	//==============================================================================
	pub fn getName(&self) -> &'static str {
		BLEND_MODE_NAME_TABLE[*self as usize].1
	}

	//==============================================================================
	// 원본 이름으로 찾기. (모르는 이름이면 None. 타임라인, 파티클 서술처럼 JSON 에서 읽을 때 쓴다)
	//==============================================================================
	pub fn fromName(name: &str) -> Option<BlendMode> {
		BLEND_MODE_NAME_TABLE.iter().find(|(_, blendModeName)| *blendModeName == name).map(|(blendMode, _)| *blendMode)
	}
}

//==============================================================================
// 가로 정렬.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
	left,
	center,
	right,
	start,
	end,
}

//==============================================================================
// 세로 기준선.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextBaseline {
	top,
	middle,
	bottom,
	ideographic,
	hanging,
	alphabetic,
}

//==============================================================================
// pushState 로 저장하는 상태. (Canvas2D save 대응)
//==============================================================================
#[derive(Clone)]
struct GraphicState {
	transformMatrix: TransformMatrix,
	globalAlpha: f32,
	blendMode: BlendMode,
	fillColor: Color,
	strokeColor: Color,
	imageTintColor: Option<Color>,
	fontString: String,
	textAlign: TextAlign,
	textBaseline: TextBaseline,
}

//==============================================================================
// 클립 스택 항목.
//==============================================================================
struct ClipEntry {
	rect: Rect,
	roundSize: f32,
	transformMatrix: TransformMatrix,
}

pub struct Graphic {
	gl: glow::Context,
	isForceGizmosVisible: bool,
	isImageSmoothingEnabled: bool,
	isImageMipmapEnabled: bool,
	shaderProgram: ShaderProgram,
	// 드로우에 잠시 바꿔 끼우는 프로그램. (ShaderSprite 등)
	shaderProgramOverride: Option<Rc<ShaderProgram>>,
	// 이름으로 담아 둔 바꿔 끼울 프로그램. ((묶음 이름, 프로그램 이름) → 프로그램, 만들기에 실패했으면 None)
	shaderProgramCacheTable: HashMap<(&'static str, &'static str), Option<Rc<ShaderProgram>>>,
	// 아래 유니폼 자리를 찾아 둔 셰이더 프로그램.
	uniformLocationProgram: Option<GlProgram>,
	modelMatrixLocation: Option<GlUniformLocation>,
	mainColorLocation: Option<GlUniformLocation>,
	tintColorLocation: Option<GlUniformLocation>,
	globalAlphaLocation: Option<GlUniformLocation>,
	// 기본 프로그램에 마지막으로 올린 틴트와 전역 투명도를 알고 있는지.
	isUploadedUniformKnown: bool,
	// 기본 프로그램에 마지막으로 올린 틴트. (r, g, b, a)
	uploadedTintColorValueList: [f32; 4],
	// 기본 프로그램에 마지막으로 올린 전역 투명도.
	uploadedGlobalAlpha: f32,
	vertexArray: GlVertexArray,
	vertexBuffer: GlBuffer,
	vertexData: Vec<f32>,
	whiteTexture: GlTexture,
	particleShaderProgram: Option<ShaderProgram>,
	particleVertexArray: Option<GlVertexArray>,
	particleVertexBuffer: Option<GlBuffer>,
	particleVertexData: Vec<f32>,
	softDiscTexture: Option<GlTexture>,
	imageTextureCache: ImageTextureCache,
	textStringTextureCache: TextStringTextureCache,
	transformMatrix: TransformMatrix,
	stateStack: Vec<GraphicState>,
	globalAlpha: f32,
	blendMode: BlendMode,
	warnedBlendModes: HashSet<BlendMode>,
	fillColor: Color,
	strokeColor: Color,
	whiteColor: Color,
	imageTintColor: Option<Color>,
	fontString: String,
	textAlign: TextAlign,
	textBaseline: TextBaseline,
	clipStack: Vec<ClipEntry>,
	projectionMatrixArray: [f32; 9],
	modelMatrixArray: [f32; 9],
	appliedViewportWidth: i32,
	appliedViewportHeight: i32,
	textBakeScaleStep: f32,
	isTextPixelSnapEnabled: bool,
	drawCallCount: u32,
	drawVertexCount: u32,
	lastFrameDrawCallCount: u32,
	lastFrameDrawVertexCount: u32,
	culledNodeCount: u32,
	lastFrameCulledNodeCount: u32,
	frameNumber: u64,
	roundRectUnitTable: [(f32, f32); ROUND_RECT_POSITION_COUNT],
	isBatchingEnabled: bool,
	batchSuspendDepth: u32,
	batchVertexData: Vec<f32>,
	batchVertexCount: usize,
	batchTexture: Option<GlTexture>,
	isCullingEnabled: bool,
}

impl Graphic {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(gl: glow::Context) -> Result<Graphic, String> {
		// 셰이더 프로그램.
		let shaderProgram = ShaderProgram::new(&gl, VERTEXSHADER_SOURCE, FRAGMENTSHADER_SOURCE)?;

		// 버텍스 어레이 / 버텍스 버퍼. (스트리밍, 인터리브 x/y/u/v)
		let vertexData = vec![0.0f32; VERTEX_CAPACITY * FLOATS_PER_VERTEX];
		let (vertexArray, vertexBuffer, whiteTexture) = unsafe {
			let vertexArray = gl.create_vertex_array()?;
			let vertexBuffer = gl.create_buffer()?;
			gl.bind_vertex_array(Some(vertexArray));
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertexBuffer));
			gl.buffer_data_size(glow::ARRAY_BUFFER, (vertexData.len() * 4) as i32, glow::DYNAMIC_DRAW);
			let strideBytes = (FLOATS_PER_VERTEX * 4) as i32;
			let vertexPositionLocation = shaderProgram.getAttributeLocation(&gl, "vertexPosition");
			let vertexTextureCoordinateLocation = shaderProgram.getAttributeLocation(&gl, "vertexTextureCoordinate");
			if let Some(vertexPositionLocation) = vertexPositionLocation {
				gl.enable_vertex_attrib_array(vertexPositionLocation);
				gl.vertex_attrib_pointer_f32(vertexPositionLocation, 2, glow::FLOAT, false, strideBytes, 0);
			}
			if let Some(vertexTextureCoordinateLocation) = vertexTextureCoordinateLocation {
				gl.enable_vertex_attrib_array(vertexTextureCoordinateLocation);
				gl.vertex_attrib_pointer_f32(vertexTextureCoordinateLocation, 2, glow::FLOAT, false, strideBytes, 8);
			}
			gl.bind_vertex_array(None);

			// 단색 도형용 1x1 흰색 텍스처.
			let whiteTexture = gl.create_texture()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(whiteTexture));
			let whitePixel: [u8; 4] = [255, 255, 255, 255];
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&whitePixel)));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);

			// 고정 렌더 상태.
			gl.enable(glow::BLEND);
			gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::CULL_FACE);

			(vertexArray, vertexBuffer, whiteTexture)
		};

		// 텍스처 유닛 고정.
		shaderProgram.useProgram(&gl);
		let mainTextureLocation = shaderProgram.getUniformLocation(&gl, "mainTexture");
		let textureLodBiasLocation = shaderProgram.getUniformLocation(&gl, "textureLodBias");
		unsafe {
			gl.uniform_1_i32(mainTextureLocation.as_ref(), 0);
			gl.uniform_1_f32(textureLodBiasLocation.as_ref(), 0.0);
			gl.active_texture(glow::TEXTURE0);
		}

		Ok(Graphic {
			gl,
			isForceGizmosVisible: false,
			isImageSmoothingEnabled: true,
			isImageMipmapEnabled: false,
			shaderProgram,
			shaderProgramOverride: None,
			shaderProgramCacheTable: HashMap::new(),
			uniformLocationProgram: None,
			modelMatrixLocation: None,
			mainColorLocation: None,
			tintColorLocation: None,
			globalAlphaLocation: None,
			isUploadedUniformKnown: false,
			uploadedTintColorValueList: [0.0; 4],
			uploadedGlobalAlpha: 1.0,
			vertexArray,
			vertexBuffer,
			vertexData,
			whiteTexture,
			particleShaderProgram: None,
			particleVertexArray: None,
			particleVertexBuffer: None,
			particleVertexData: Vec::new(),
			softDiscTexture: None,
			imageTextureCache: ImageTextureCache::new(),
			textStringTextureCache: TextStringTextureCache::new(),
			transformMatrix: TransformMatrix::identity(),
			stateStack: Vec::new(),
			globalAlpha: 1.0,
			blendMode: BlendMode::normal,
			warnedBlendModes: HashSet::new(),
			fillColor: Color::black(),
			strokeColor: Color::black(),
			whiteColor: Color::white(),
			imageTintColor: None,
			fontString: String::from("10px sans-serif"),
			textAlign: TextAlign::start,
			textBaseline: TextBaseline::alphabetic,
			clipStack: Vec::new(),
			projectionMatrixArray: [0.0; 9],
			modelMatrixArray: [0.0; 9],
			appliedViewportWidth: 0,
			appliedViewportHeight: 0,
			textBakeScaleStep: DEFAULT_TEXT_BAKE_SCALE_STEP,
			isTextPixelSnapEnabled: false,
			drawCallCount: 0,
			drawVertexCount: 0,
			lastFrameDrawCallCount: 0,
			lastFrameDrawVertexCount: 0,
			culledNodeCount: 0,
			lastFrameCulledNodeCount: 0,
			frameNumber: 0,
			roundRectUnitTable: buildRoundRectUnitTable(),
			isBatchingEnabled: true,
			batchSuspendDepth: 0,
			batchVertexData: vec![0.0f32; BATCH_VERTEX_CAPACITY * PARTICLE_FLOATS_PER_VERTEX],
			batchVertexCount: 0,
			batchTexture: None,
			isCullingEnabled: true,
		})
	}

	//==============================================================================
	// 설정 반영. (매 프레임 시작 시 엔진이 호출)
	// - 드로잉 버퍼 크기 변화 감지 및 뷰포트/프로젝션 갱신.
	// - 프레임 시작 기준 상태로 리셋.
	// - 브라우저는 preserveDrawingBuffer 가 꺼져 있으면 프레임마다 화면을 비워 준다. 창에서는 직접 비운다.
	//==============================================================================
	pub fn applySettings(&mut self, drawingBufferWidth: i32, drawingBufferHeight: i32) {
		self.frameNumber += 1;
		self.textStringTextureCache.beginFrame();
		self.shaderProgramOverride = None;

		// 뷰포트/프로젝션 갱신.
		if drawingBufferWidth != self.appliedViewportWidth || drawingBufferHeight != self.appliedViewportHeight {
			unsafe {
				self.gl.viewport(0, 0, drawingBufferWidth, drawingBufferHeight);
			}
			let safeWidth = drawingBufferWidth.max(1) as f32;
			let safeHeight = drawingBufferHeight.max(1) as f32;
			self.projectionMatrixArray = [2.0 / safeWidth, 0.0, 0.0, 0.0, -2.0 / safeHeight, 0.0, -1.0, 1.0, 1.0];
			self.appliedViewportWidth = drawingBufferWidth;
			self.appliedViewportHeight = drawingBufferHeight;
		}

		// 프로그램/버퍼 바인딩 및 프로젝션 업로드.
		self.shaderProgram.useProgram(&self.gl);
		let projectionMatrixLocation = self.shaderProgram.getUniformLocation(&self.gl, "projectionMatrix");
		unsafe {
			self.gl.bind_vertex_array(Some(self.vertexArray));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertexBuffer));
			self.gl.uniform_matrix_3_f32_slice(projectionMatrixLocation.as_ref(), false, &self.projectionMatrixArray);
		}

		// 프레임 시작 기준 상태로 리셋.
		self.stateStack.clear();
		self.isUploadedUniformKnown = false;
		self.transformMatrix.setIdentity();
		self.globalAlpha = 1.0;
		self.imageTintColor = None;
		self.batchVertexCount = 0;
		self.batchTexture = None;
		self.applyBlendMode(BlendMode::normal);

		// 클리핑 상태 리셋. (비정상 종료로 스택이 남아 있어도 복구)
		unsafe {
			if !self.clipStack.is_empty() {
				self.clipStack.clear();
				self.gl.disable(glow::STENCIL_TEST);
			}
			self.gl.color_mask(true, true, true, true);
			self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
			self.gl.clear_stencil(0);
			self.gl.clear(glow::COLOR_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
		}
	}

	//==============================================================================
	// 프레임 마무리. (엔진이 화면을 넘기기 전에 호출)
	//==============================================================================
	pub fn endFrame(&mut self) {
		self.flushBatch();
		if self.frameNumber.is_multiple_of(IMAGE_TEXTURE_SWEEP_INTERVAL) {
			self.imageTextureCache.sweep(&self.gl);
		}
	}

	//==============================================================================
	// 이미지 스무딩 활성화 여부 설정.
	// - 도트(픽셀) 스프라이트 게임은 false 로 설정해 선명하게 렌더링.
	//==============================================================================
	pub fn setImageSmoothingEnabled(&mut self, value: bool) {
		self.isImageSmoothingEnabled = value;
	}

	//==============================================================================
	// 이미지 스무딩 활성화 여부 반환.
	//==============================================================================
	pub fn isImageSmoothingEnabled(&self) -> bool {
		self.isImageSmoothingEnabled
	}

	//==============================================================================
	// 이미지 밉맵 사용 여부 설정. (크게 줄여 그리는 이미지의 자글거림을 줄인다)
	//==============================================================================
	pub fn setImageMipmapEnabled(&mut self, value: bool) {
		self.isImageMipmapEnabled = value;
	}

	//==============================================================================
	// 이미지 밉맵 사용 여부 반환.
	//==============================================================================
	pub fn isImageMipmapEnabled(&self) -> bool {
		self.isImageMipmapEnabled
	}

	//==============================================================================
	// 모든 노드의 기즈모를 강제로 보일지 설정.
	//==============================================================================
	pub fn setForceGizmosVisible(&mut self, isForceGizmosVisible: bool) {
		self.isForceGizmosVisible = isForceGizmosVisible;
	}

	//==============================================================================
	// 모든 노드의 기즈모를 강제로 보일지 반환.
	//==============================================================================
	pub fn isForceGizmosVisible(&self) -> bool {
		self.isForceGizmosVisible
	}

	//==============================================================================
	// 상태 저장. (Canvas2D save 대응)
	//==============================================================================
	pub fn pushState(&mut self) {
		let stateSnapshot = GraphicState {
			transformMatrix: self.transformMatrix,
			globalAlpha: self.globalAlpha,
			blendMode: self.blendMode,
			fillColor: self.fillColor,
			strokeColor: self.strokeColor,
			imageTintColor: self.imageTintColor,
			fontString: self.fontString.clone(),
			textAlign: self.textAlign,
			textBaseline: self.textBaseline,
		};
		self.stateStack.push(stateSnapshot);
	}

	//==============================================================================
	// 상태 복원. (Canvas2D restore 대응)
	//==============================================================================
	pub fn popState(&mut self) {
		let stateSnapshot = match self.stateStack.pop() {
			Some(stateSnapshot) => {
				stateSnapshot
			}
			None => {
				return;
			}
		};

		self.transformMatrix = stateSnapshot.transformMatrix;
		self.globalAlpha = stateSnapshot.globalAlpha;
		self.fillColor = stateSnapshot.fillColor;
		self.strokeColor = stateSnapshot.strokeColor;
		self.imageTintColor = stateSnapshot.imageTintColor;
		self.fontString = stateSnapshot.fontString;
		self.textAlign = stateSnapshot.textAlign;
		self.textBaseline = stateSnapshot.textBaseline;
		if self.blendMode != stateSnapshot.blendMode {
			self.setBlendMode(stateSnapshot.blendMode);
		}
	}

	//==============================================================================
	// 이동 변환 누적.
	//==============================================================================
	pub fn translate(&mut self, x: f32, y: f32) {
		self.transformMatrix.translate(x, y);
	}

	//==============================================================================
	// 회전 변환 누적.
	//==============================================================================
	pub fn rotate(&mut self, radian: f32) {
		self.transformMatrix.rotate(radian);
	}

	//==============================================================================
	// 크기 변환 누적.
	//==============================================================================
	pub fn scale(&mut self, x: f32, y: f32) {
		self.transformMatrix.scale(x, y);
	}

	//==============================================================================
	// 변환 행렬 직접 설정. (Canvas2D setTransform(a, b, c, d, e, f) 대응)
	//==============================================================================
	pub fn setTransform(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
		self.transformMatrix.setTransform(a, b, c, d, e, f);
	}

	//==============================================================================
	// 변환 행렬 초기화.
	//==============================================================================
	pub fn resetTransform(&mut self) {
		self.transformMatrix.setIdentity();
	}

	//==============================================================================
	// 현재 변환 행렬 반환.
	//==============================================================================
	pub fn getTransformMatrix(&self) -> TransformMatrix {
		self.transformMatrix
	}

	//==============================================================================
	// 전역 투명도 설정.
	//==============================================================================
	pub fn setGlobalAlpha(&mut self, value: f32) {
		self.globalAlpha = math::clamp01(value);
	}

	//==============================================================================
	// 전역 투명도 반환.
	//==============================================================================
	pub fn getGlobalAlpha(&self) -> f32 {
		self.globalAlpha
	}

	//==============================================================================
	// 전역 투명도 곱셈 누적. (노드 트리의 투명도 상속용)
	//==============================================================================
	pub fn multiplyGlobalAlpha(&mut self, value: f32) {
		self.globalAlpha = math::clamp01(self.globalAlpha * value);
	}

	//==============================================================================
	// 블렌드 모드 설정.
	// - 고정 블렌딩으로 재현 불가능한 모드는 normal(source-over) 로 폴백.
	//==============================================================================
	pub fn setBlendMode(&mut self, blendMode: BlendMode) {
		if blendMode == self.blendMode {
			return;
		}
		self.flushBatch();
		self.applyBlendMode(blendMode);
	}

	//==============================================================================
	// 블렌드 모드를 GL 에 건다. (같은 모드여도 건다)
	//==============================================================================
	fn applyBlendMode(&mut self, blendMode: BlendMode) {
		let blendFunction = match blendMode {
			BlendMode::normal => {
				Some((glow::ONE, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::lighter => {
				Some((glow::ONE, glow::ONE))
			}
			BlendMode::multiply => {
				Some((glow::DST_COLOR, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::screen => {
				Some((glow::ONE, glow::ONE_MINUS_SRC_COLOR))
			}
			BlendMode::destinationOver => {
				Some((glow::ONE_MINUS_DST_ALPHA, glow::ONE))
			}
			BlendMode::destinationOut => {
				Some((glow::ZERO, glow::ONE_MINUS_SRC_ALPHA))
			}
			BlendMode::sourceIn => {
				Some((glow::DST_ALPHA, glow::ZERO))
			}
			BlendMode::destinationIn => {
				Some((glow::ZERO, glow::SRC_ALPHA))
			}
			BlendMode::copy => {
				Some((glow::ONE, glow::ZERO))
			}
			_ => {
				None
			}
		};
		let (sourceFactor, destinationFactor) = match blendFunction {
			Some(blendFunction) => {
				blendFunction
			}
			None => {
				if !self.warnedBlendModes.contains(&blendMode) {
					self.warnedBlendModes.insert(blendMode);
					eprintln!("Graphic: unsupported blend mode \"{:?}\", falling back to \"normal\".", blendMode);
				}
				(glow::ONE, glow::ONE_MINUS_SRC_ALPHA)
			}
		};
		self.blendMode = blendMode;
		unsafe {
			self.gl.blend_func(sourceFactor, destinationFactor);
		}
	}

	//==============================================================================
	// 블렌드 모드 반환.
	//==============================================================================
	pub fn getBlendMode(&self) -> BlendMode {
		self.blendMode
	}

	//==============================================================================
	// 이미지 틴트 색상 설정. (None 이면 해제)
	// - 이미지 실루엣(알파) 안쪽에만 tint.alpha 비율로 색을 덮는다.
	//==============================================================================
	pub fn setImageTintColor(&mut self, color: Option<Color>) {
		self.imageTintColor = color;
	}

	//==============================================================================
	// 이미지 틴트 색상 반환.
	//==============================================================================
	pub fn getImageTintColor(&self) -> Option<Color> {
		self.imageTintColor
	}

	//==============================================================================
	// 채우기 색상 설정. (Color 또는 "#rrggbb" 같은 문자열)
	//==============================================================================
	pub fn setFillColor(&mut self, color: impl Into<Color>) {
		self.fillColor = color.into();
	}

	//==============================================================================
	// 채우기 색상 반환.
	//==============================================================================
	pub fn getFillColor(&self) -> Color {
		self.fillColor
	}

	//==============================================================================
	// 선 색상 설정. (Color 또는 "#rrggbb" 같은 문자열)
	//==============================================================================
	pub fn setStrokeColor(&mut self, color: impl Into<Color>) {
		self.strokeColor = color.into();
	}

	//==============================================================================
	// 선 색상 반환.
	//==============================================================================
	pub fn getStrokeColor(&self) -> Color {
		self.strokeColor
	}

	//==============================================================================
	// 버텍스 데이터에 쿼드(삼각형 2개, 버텍스 6개) 기록.
	// - 기록 후의 오프셋(플로트 단위)을 반환.
	//==============================================================================
	pub fn writeQuad(&mut self, offset: usize, leftX: f32, topY: f32, rightX: f32, bottomY: f32, leftU: f32, topV: f32, rightU: f32, bottomV: f32) -> usize {
		let quadValues: [f32; 24] = [
			leftX, topY, leftU, topV,
			rightX, topY, rightU, topV,
			leftX, bottomY, leftU, bottomV,
			leftX, bottomY, leftU, bottomV,
			rightX, topY, rightU, topV,
			rightX, bottomY, rightU, bottomV,
		];
		self.vertexData[offset..offset + 24].copy_from_slice(&quadValues);
		offset + 24
	}

	//==============================================================================
	// 기록된 버텍스 출력. (모든 드로우가 수렴하는 단일 지점)
	//==============================================================================
	pub fn drawVertices(&mut self, vertexCount: usize, texture: GlTexture, color: Color) {
		if vertexCount == 0 {
			return;
		}

		// 묶어 그리기. (틴트가 없고 클립 영역 기록 중이 아니고 바꿔 끼운 셰이더가 없을 때. 바꿔 끼운 셰이더는 제 유니폼을 쓰므로 바로 그린다)
		let isBatchable = self.isBatchingEnabled && self.batchSuspendDepth == 0 && self.imageTintColor.is_none() && self.shaderProgramOverride.is_none();
		if isBatchable {
			self.appendBatchVertices(vertexCount, texture, color);
			return;
		}
		self.flushBatch();

		// 버텍스 업로드. (쓸 만큼만 새 저장소를 잡으며 올린다. 지난 드로우와 저장소가 겹치지 않는다)
		let floatCount = vertexCount * FLOATS_PER_VERTEX;
		let vertexBytes = floatSliceToBytes(&self.vertexData[..floatCount]);
		unsafe {
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertexBytes, glow::DYNAMIC_DRAW);
		}

		// 유니폼 업로드.
		self.uploadDrawUniforms(color);

		// 텍스처 바인드 및 출력.
		unsafe {
			self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			self.gl.draw_arrays(glow::TRIANGLES, 0, vertexCount as i32);
		}
		self.drawCallCount += 1;
		self.drawVertexCount += vertexCount as u32;
	}

	//==============================================================================
	// 드로우 콜 하나의 유니폼 업로드. (drawVertices 가 부른다)
	// - 유니폼 자리는 셰이더 프로그램이 바뀔 때만 다시 찾는다.
	// - 기본 프로그램의 틴트와 전역 투명도는 지난번에 올린 값과 같으면 다시 올리지 않는다. (프레임 시작과 프로그램이 바뀔 때 잊는다)
	// - 바꿔 끼운 셰이더 프로그램에는 늘 올린다.
	//==============================================================================
	fn uploadDrawUniforms(&mut self, color: Color) {
		let isDefaultProgram = self.shaderProgramOverride.is_none();
		let shaderProgram = match &self.shaderProgramOverride {
			Some(shaderProgramOverride) => {
				shaderProgramOverride.as_ref()
			}
			None => {
				&self.shaderProgram
			}
		};
		let program = shaderProgram.getProgram();
		if self.uniformLocationProgram != Some(program) {
			let modelMatrixLocation = shaderProgram.getUniformLocation(&self.gl, "modelMatrix");
			let mainColorLocation = shaderProgram.getUniformLocation(&self.gl, "mainColor");
			let tintColorLocation = shaderProgram.getUniformLocation(&self.gl, "tintColor");
			let globalAlphaLocation = shaderProgram.getUniformLocation(&self.gl, "globalAlpha");
			self.uniformLocationProgram = Some(program);
			self.modelMatrixLocation = modelMatrixLocation;
			self.mainColorLocation = mainColorLocation;
			self.tintColorLocation = tintColorLocation;
			self.globalAlphaLocation = globalAlphaLocation;
			self.isUploadedUniformKnown = false;
		}

		self.transformMatrix.writeToArray(&mut self.modelMatrixArray);

		// 틴트. (없으면 0, 0, 0, 0)
		let tintColorValueList = match self.imageTintColor {
			Some(imageTintColor) => {
				[imageTintColor.red, imageTintColor.green, imageTintColor.blue, imageTintColor.alpha]
			}
			None => {
				[0.0, 0.0, 0.0, 0.0]
			}
		};
		let globalAlpha = self.globalAlpha;

		let isUploadedUniformKnown = isDefaultProgram && self.isUploadedUniformKnown;
		let isSameTintColor = isUploadedUniformKnown && self.uploadedTintColorValueList == tintColorValueList;
		let isSameGlobalAlpha = isUploadedUniformKnown && self.uploadedGlobalAlpha == globalAlpha;
		unsafe {
			self.gl.uniform_matrix_3_f32_slice(self.modelMatrixLocation.as_ref(), false, &self.modelMatrixArray);
			self.gl.uniform_4_f32(self.mainColorLocation.as_ref(), color.red, color.green, color.blue, color.alpha);
			if !isSameTintColor {
				self.gl.uniform_4_f32(self.tintColorLocation.as_ref(), tintColorValueList[0], tintColorValueList[1], tintColorValueList[2], tintColorValueList[3]);
			}
			if !isSameGlobalAlpha {
				self.gl.uniform_1_f32(self.globalAlphaLocation.as_ref(), globalAlpha);
			}
		}

		if isDefaultProgram {
			self.uploadedTintColorValueList = tintColorValueList;
			self.uploadedGlobalAlpha = globalAlpha;
			self.isUploadedUniformKnown = true;
		}
	}

	//==============================================================================
	// 묶음에 정점 더하기. (현재 변환을 CPU 에서 곱하고, 색에 전역 투명도를 접어 정점마다 싣는다)
	//==============================================================================
	fn appendBatchVertices(&mut self, vertexCount: usize, texture: GlTexture, color: Color) {
		if self.batchTexture != Some(texture) || self.batchVertexCount + vertexCount > BATCH_VERTEX_CAPACITY {
			self.flushBatch();
			self.batchTexture = Some(texture);
		}
		let matrix = self.transformMatrix;
		let alpha = color.alpha * self.globalAlpha;
		let mut writeOffset = self.batchVertexCount * PARTICLE_FLOATS_PER_VERTEX;
		for vertexIndex in 0..vertexCount {
			let readOffset = vertexIndex * FLOATS_PER_VERTEX;
			let positionX = self.vertexData[readOffset];
			let positionY = self.vertexData[readOffset + 1];
			let batchVertex = &mut self.batchVertexData[writeOffset..writeOffset + PARTICLE_FLOATS_PER_VERTEX];
			batchVertex[0] = matrix.a * positionX + matrix.c * positionY + matrix.e;
			batchVertex[1] = matrix.b * positionX + matrix.d * positionY + matrix.f;
			batchVertex[2] = self.vertexData[readOffset + 2];
			batchVertex[3] = self.vertexData[readOffset + 3];
			batchVertex[4] = color.red;
			batchVertex[5] = color.green;
			batchVertex[6] = color.blue;
			batchVertex[7] = alpha;
			writeOffset += PARTICLE_FLOATS_PER_VERTEX;
		}
		self.batchVertexCount += vertexCount;
	}

	//==============================================================================
	// 모아 둔 묶음 내보내기. (GL 상태를 바꾸기 전, 프레임 끝, GL 을 직접 쓰기 전에 부른다)
	//==============================================================================
	pub fn flushBatch(&mut self) {
		if self.batchVertexCount == 0 {
			return;
		}
		let vertexCount = self.batchVertexCount;
		self.batchVertexCount = 0;
		let texture = match self.batchTexture.take() {
			Some(texture) => {
				texture
			}
			None => {
				return;
			}
		};
		if let Err(error) = self.ensureParticleBatchResources() {
			eprintln!("Graphic: batch resources failed: {}", error);
			return;
		}
		let particleShaderProgram = match &self.particleShaderProgram {
			Some(particleShaderProgram) => {
				particleShaderProgram
			}
			None => {
				return;
			}
		};
		let identityMatrixArray: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
		particleShaderProgram.useProgram(&self.gl);
		let projectionMatrixLocation = particleShaderProgram.getUniformLocation(&self.gl, "projectionMatrix");
		let modelMatrixLocation = particleShaderProgram.getUniformLocation(&self.gl, "modelMatrix");
		let globalAlphaLocation = particleShaderProgram.getUniformLocation(&self.gl, "globalAlpha");
		let vertexBytes = floatSliceToBytes(&self.batchVertexData[..vertexCount * PARTICLE_FLOATS_PER_VERTEX]);
		unsafe {
			self.gl.bind_vertex_array(self.particleVertexArray);
			self.gl.bind_buffer(glow::ARRAY_BUFFER, self.particleVertexBuffer);
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertexBytes, glow::DYNAMIC_DRAW);
			self.gl.uniform_matrix_3_f32_slice(projectionMatrixLocation.as_ref(), false, &self.projectionMatrixArray);
			self.gl.uniform_matrix_3_f32_slice(modelMatrixLocation.as_ref(), false, &identityMatrixArray);
			self.gl.uniform_1_f32(globalAlphaLocation.as_ref(), 1.0);
			self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			self.gl.draw_arrays(glow::TRIANGLES, 0, vertexCount as i32);

			// 기본 프로그램으로 복귀.
			self.gl.bind_vertex_array(Some(self.vertexArray));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertexBuffer));
		}
		self.getShaderProgram().useProgram(&self.gl);
		self.drawCallCount += 1;
		self.drawVertexCount += vertexCount as u32;
	}

	//==============================================================================
	// 묶어 그리기 켜고 끄기. (기본 켬. 끄면 원본처럼 그리기 하나에 드로우 콜 하나)
	//==============================================================================
	pub fn setBatchingEnabled(&mut self, isBatchingEnabled: bool) {
		if !isBatchingEnabled {
			self.flushBatch();
		}
		self.isBatchingEnabled = isBatchingEnabled;
	}

	//==============================================================================
	// 묶어 그리기 여부.
	//==============================================================================
	pub fn isBatchingEnabled(&self) -> bool {
		self.isBatchingEnabled
	}

	//==============================================================================
	// 화면 밖 거르기 켜고 끄기. (기본 켬)
	//==============================================================================
	pub fn setCullingEnabled(&mut self, isCullingEnabled: bool) {
		self.isCullingEnabled = isCullingEnabled;
	}

	//==============================================================================
	// 화면 밖 거르기 여부.
	//==============================================================================
	pub fn isCullingEnabled(&self) -> bool {
		self.isCullingEnabled
	}

	//==============================================================================
	// 현재 변환 기준의 사각형이 화면(드로잉 버퍼)에 걸치는지. (네 꼭짓점을 변환한 경계 상자로 본다)
	//==============================================================================
	pub fn isLocalRectVisible(&self, rect: Rect) -> bool {
		let matrix = self.transformMatrix;
		let leftX = rect.position.x;
		let topY = rect.position.y;
		let rightX = rect.position.x + rect.size.x;
		let bottomY = rect.position.y + rect.size.y;
		let cornerList: [(f32, f32); 4] = [(leftX, topY), (rightX, topY), (leftX, bottomY), (rightX, bottomY)];
		let mut minimumX = f32::MAX;
		let mut minimumY = f32::MAX;
		let mut maximumX = f32::MIN;
		let mut maximumY = f32::MIN;
		for (cornerX, cornerY) in cornerList {
			let transformedX = matrix.a * cornerX + matrix.c * cornerY + matrix.e;
			let transformedY = matrix.b * cornerX + matrix.d * cornerY + matrix.f;
			minimumX = minimumX.min(transformedX);
			minimumY = minimumY.min(transformedY);
			maximumX = maximumX.max(transformedX);
			maximumY = maximumY.max(transformedY);
		}
		let viewportWidth = self.appliedViewportWidth as f32;
		let viewportHeight = self.appliedViewportHeight as f32;
		maximumX >= 0.0 && maximumY >= 0.0 && minimumX <= viewportWidth && minimumY <= viewportHeight
	}

	//==============================================================================
	// 이번 프레임에 거른 노드 수 더하기. (NodeTree 가 부른다)
	//==============================================================================
	pub fn addCulledNodeCount(&mut self, culledNodeCount: u32) {
		self.culledNodeCount += culledNodeCount;
	}

	//==============================================================================
	// 지난 프레임에 화면 밖이라 거른 노드 수.
	//==============================================================================
	pub fn getLastFrameCulledNodeCount(&self) -> u32 {
		self.lastFrameCulledNodeCount
	}

	//==============================================================================
	// 프레임 그리기 통계 시작.
	// - 엔진이 매 프레임 씬을 그리기 전에 부른다. 지난 프레임의 드로우 콜 / 정점 수를 넘겨 두고 이번 프레임 값을 비운다.
	//==============================================================================
	pub fn beginFrameStatistics(&mut self) {
		self.flushBatch();
		self.lastFrameDrawCallCount = self.drawCallCount;
		self.lastFrameDrawVertexCount = self.drawVertexCount;
		self.lastFrameCulledNodeCount = self.culledNodeCount;
		self.drawCallCount = 0;
		self.drawVertexCount = 0;
		self.culledNodeCount = 0;
	}

	//==============================================================================
	// 지난 프레임의 드로우 콜 수.
	//==============================================================================
	pub fn getLastFrameDrawCallCount(&self) -> u32 {
		self.lastFrameDrawCallCount
	}

	//==============================================================================
	// 지난 프레임에 그린 정점 수.
	//==============================================================================
	pub fn getLastFrameVertexCount(&self) -> u32 {
		self.lastFrameDrawVertexCount
	}

	//==============================================================================
	// 지난 프레임에 그린 삼각형 수.
	//==============================================================================
	pub fn getLastFrameTriangleCount(&self) -> u32 {
		self.lastFrameDrawVertexCount / 3
	}

	//==============================================================================
	// 파티클 배치 자원 준비. (처음 쓸 때 만든다)
	//==============================================================================
	fn ensureParticleBatchResources(&mut self) -> Result<(), String> {
		if self.particleShaderProgram.is_some() {
			return Ok(());
		}
		let particleShaderProgram = ShaderProgram::new(&self.gl, PARTICLE_VERTEXSHADER_SOURCE, PARTICLE_FRAGMENTSHADER_SOURCE)?;
		unsafe {
			let particleVertexArray = self.gl.create_vertex_array()?;
			let particleVertexBuffer = self.gl.create_buffer()?;
			self.gl.bind_vertex_array(Some(particleVertexArray));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(particleVertexBuffer));
			self.gl.buffer_data_size(glow::ARRAY_BUFFER, (PARTICLE_VERTEX_CAPACITY * PARTICLE_FLOATS_PER_VERTEX * 4) as i32, glow::DYNAMIC_DRAW);
			let strideBytes = (PARTICLE_FLOATS_PER_VERTEX * 4) as i32;
			let attributeLayout: [(&str, i32, i32); 3] = [("vertexPosition", 2, 0), ("vertexTextureCoordinate", 2, 8), ("vertexColor", 4, 16)];
			for (attributeName, componentCount, byteOffset) in attributeLayout {
				let attributeLocation = particleShaderProgram.getAttributeLocation(&self.gl, attributeName);
				if let Some(attributeLocation) = attributeLocation {
					self.gl.enable_vertex_attrib_array(attributeLocation);
					self.gl.vertex_attrib_pointer_f32(attributeLocation, componentCount, glow::FLOAT, false, strideBytes, byteOffset);
				}
			}
			self.gl.bind_vertex_array(Some(self.vertexArray));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertexBuffer));
			self.particleVertexArray = Some(particleVertexArray);
			self.particleVertexBuffer = Some(particleVertexBuffer);
		}
		particleShaderProgram.useProgram(&self.gl);
		let mainTextureLocation = particleShaderProgram.getUniformLocation(&self.gl, "mainTexture");
		let textureLodBiasLocation = particleShaderProgram.getUniformLocation(&self.gl, "textureLodBias");
		unsafe {
			self.gl.uniform_1_i32(mainTextureLocation.as_ref(), 0);
			self.gl.uniform_1_f32(textureLodBiasLocation.as_ref(), 0.0);
		}
		self.getShaderProgram().useProgram(&self.gl);
		self.particleShaderProgram = Some(particleShaderProgram);
		Ok(())
	}

	//==============================================================================
	// 파티클 버텍스 기록용 배열 반환. (minimumFloatCount 이상, 용량을 넘으면 용량까지)
	//==============================================================================
	pub fn getParticleVertexData(&mut self, minimumFloatCount: usize) -> &mut [f32] {
		let capacityFloatCount = PARTICLE_VERTEX_CAPACITY * PARTICLE_FLOATS_PER_VERTEX;
		let floatCount = minimumFloatCount.min(capacityFloatCount);
		if self.particleVertexData.len() < floatCount {
			self.particleVertexData.resize(floatCount, 0.0);
		}
		&mut self.particleVertexData[..floatCount]
	}

	//==============================================================================
	// 정점마다 색을 실은 쿼드 출력. (getParticleVertexData 에 기록한 것)
	//==============================================================================
	pub fn drawColoredQuads(&mut self, vertexCount: usize, texture: GlTexture) {
		if vertexCount == 0 {
			return;
		}
		self.flushBatch();
		if let Err(error) = self.ensureParticleBatchResources() {
			eprintln!("Graphic: particle resources failed: {}", error);
			return;
		}
		let vertexCount = vertexCount.min(PARTICLE_VERTEX_CAPACITY).min(self.particleVertexData.len() / PARTICLE_FLOATS_PER_VERTEX);
		let particleShaderProgram = match &self.particleShaderProgram {
			Some(particleShaderProgram) => {
				particleShaderProgram
			}
			None => {
				return;
			}
		};
		self.transformMatrix.writeToArray(&mut self.modelMatrixArray);
		particleShaderProgram.useProgram(&self.gl);
		let projectionMatrixLocation = particleShaderProgram.getUniformLocation(&self.gl, "projectionMatrix");
		let modelMatrixLocation = particleShaderProgram.getUniformLocation(&self.gl, "modelMatrix");
		let globalAlphaLocation = particleShaderProgram.getUniformLocation(&self.gl, "globalAlpha");
		let vertexBytes = floatSliceToBytes(&self.particleVertexData[..vertexCount * PARTICLE_FLOATS_PER_VERTEX]);
		unsafe {
			self.gl.bind_vertex_array(self.particleVertexArray);
			self.gl.bind_buffer(glow::ARRAY_BUFFER, self.particleVertexBuffer);
			self.gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertexBytes, glow::DYNAMIC_DRAW);
			self.gl.uniform_matrix_3_f32_slice(projectionMatrixLocation.as_ref(), false, &self.projectionMatrixArray);
			self.gl.uniform_matrix_3_f32_slice(modelMatrixLocation.as_ref(), false, &self.modelMatrixArray);
			self.gl.uniform_1_f32(globalAlphaLocation.as_ref(), self.globalAlpha);
			self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			self.gl.draw_arrays(glow::TRIANGLES, 0, vertexCount as i32);

			// 기본 프로그램으로 복귀.
			self.gl.bind_vertex_array(Some(self.vertexArray));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertexBuffer));
		}
		self.getShaderProgram().useProgram(&self.gl);
		self.drawCallCount += 1;
		self.drawVertexCount += vertexCount as u32;
	}

	//==============================================================================
	// 부드러운 원 텍스처 반환. (가운데가 불투명하고 가장자리로 갈수록 투명. 지연 생성)
	//==============================================================================
	pub fn getSoftDiscTexture(&mut self) -> GlTexture {
		if let Some(softDiscTexture) = self.softDiscTexture {
			return softDiscTexture;
		}
		let mut pixels = vec![0u8; SOFT_DISC_TEXTURE_SIZE * SOFT_DISC_TEXTURE_SIZE * 4];
		let halfSize = SOFT_DISC_TEXTURE_SIZE as f32 * 0.5;
		for pixelY in 0..SOFT_DISC_TEXTURE_SIZE {
			for pixelX in 0..SOFT_DISC_TEXTURE_SIZE {
				let deltaX = (pixelX as f32 + 0.5 - halfSize) / halfSize;
				let deltaY = (pixelY as f32 + 0.5 - halfSize) / halfSize;
				let distance = (deltaX * deltaX + deltaY * deltaY).sqrt();
				let falloff = math::clamp01(1.0 - distance);
				let alpha = (falloff * falloff * 255.0).round() as u8;
				let pixelIndex = (pixelY * SOFT_DISC_TEXTURE_SIZE + pixelX) * 4;
				pixels[pixelIndex] = alpha;
				pixels[pixelIndex + 1] = alpha;
				pixels[pixelIndex + 2] = alpha;
				pixels[pixelIndex + 3] = alpha;
			}
		}
		let softDiscTexture = unsafe {
			let texture = self.gl.create_texture().expect("soft disc texture");
			self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			self.gl.tex_image_2d(
				glow::TEXTURE_2D,
				0,
				glow::RGBA8 as i32,
				SOFT_DISC_TEXTURE_SIZE as i32,
				SOFT_DISC_TEXTURE_SIZE as i32,
				0,
				glow::RGBA,
				glow::UNSIGNED_BYTE,
				glow::PixelUnpackData::Slice(Some(&pixels)),
			);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			texture
		};
		self.softDiscTexture = Some(softDiscTexture);
		softDiscTexture
	}

	//==============================================================================
	// 이미지의 텍스처 반환. (파티클처럼 직접 쿼드를 쓰는 곳에서)
	//==============================================================================
	pub fn getImageTexture(&mut self, image: &Rc<Image>) -> Option<GlTexture> {
		let isImageSmoothingEnabled = self.isImageSmoothingEnabled();
		let isImageMipmapEnabled = self.isImageMipmapEnabled();
		self.imageTextureCache.getTexture(&self.gl, image, isImageSmoothingEnabled, isImageMipmapEnabled)
	}

	//==============================================================================
	// 선 출력.
	// - setStrokeColor()
	//==============================================================================
	pub fn drawLine(&mut self, positions: &[Vector2], size: f32) {
		self.drawPolyline(positions, size, false);
	}

	//==============================================================================
	// 닫힌 선 출력. (마지막 점과 첫 점을 잇는 루프)
	// - setStrokeColor()
	//==============================================================================
	pub fn drawLineLoop(&mut self, positions: &[Vector2], size: f32) {
		self.drawPolyline(positions, size, true);
	}

	//==============================================================================
	// 폴리라인 출력. (세그먼트별 두께 확장 쿼드)
	// - 내부 조인트는 반두께 연장으로 이음새를 메운다.
	//==============================================================================
	pub fn drawPolyline(&mut self, positions: &[Vector2], size: f32, isClosed: bool) {
		if positions.len() < 2 {
			return;
		}

		let halfSize = size / 2.0;
		let segmentCount = if isClosed {
			positions.len()
		}
		else {
			positions.len() - 1
		};
		let mut offset = 0;
		let mut vertexCount = 0;
		for segmentIndex in 0..segmentCount {
			let startPosition = positions[segmentIndex];
			let endPosition = positions[(segmentIndex + 1) % positions.len()];
			let deltaX = endPosition.x - startPosition.x;
			let deltaY = endPosition.y - startPosition.y;
			let length = (deltaX * deltaX + deltaY * deltaY).sqrt();
			if length <= 0.0 {
				continue;
			}

			let directionX = deltaX / length;
			let directionY = deltaY / length;
			let normalX = -directionY * halfSize;
			let normalY = directionX * halfSize;

			// 내부 조인트 이음새 메움. (열린 선의 양 끝은 연장하지 않음)
			let hasStartJoint = isClosed || segmentIndex > 0;
			let hasEndJoint = isClosed || segmentIndex < segmentCount - 1;
			let startExtension = if hasStartJoint {
				halfSize
			}
			else {
				0.0
			};
			let endExtension = if hasEndJoint {
				halfSize
			}
			else {
				0.0
			};
			let extendedStartX = startPosition.x - directionX * startExtension;
			let extendedStartY = startPosition.y - directionY * startExtension;
			let extendedEndX = endPosition.x + directionX * endExtension;
			let extendedEndY = endPosition.y + directionY * endExtension;

			// 쿼드(삼각형 2개) 기록.
			if (vertexCount + 6) * FLOATS_PER_VERTEX > self.vertexData.len() {
				break;
			}
			let segmentValues: [f32; 24] = [
				extendedStartX + normalX, extendedStartY + normalY, 0.5, 0.5,
				extendedEndX + normalX, extendedEndY + normalY, 0.5, 0.5,
				extendedStartX - normalX, extendedStartY - normalY, 0.5, 0.5,
				extendedStartX - normalX, extendedStartY - normalY, 0.5, 0.5,
				extendedEndX + normalX, extendedEndY + normalY, 0.5, 0.5,
				extendedEndX - normalX, extendedEndY - normalY, 0.5, 0.5,
			];
			self.vertexData[offset..offset + 24].copy_from_slice(&segmentValues);
			offset += 24;
			vertexCount += 6;
		}

		let strokeColor = self.getStrokeColor();
		let whiteTexture = self.getWhiteTexture();
		self.drawVertices(vertexCount, whiteTexture, strokeColor);
	}

	//==============================================================================
	// 사각형 출력.
	// - setFillColor()
	//==============================================================================
	pub fn drawRect(&mut self, rect: Rect) {
		let leftX = rect.position.x;
		let topY = rect.position.y;
		let rightX = leftX + rect.size.x;
		let bottomY = topY + rect.size.y;
		let vertexCountOffset = self.writeQuad(0, leftX, topY, rightX, bottomY, 0.5, 0.5, 0.5, 0.5);
		let fillColor = self.getFillColor();
		let whiteTexture = self.getWhiteTexture();
		self.drawVertices(vertexCountOffset / FLOATS_PER_VERTEX, whiteTexture, fillColor);
	}

	//==============================================================================
	// 사각형 외곽선 출력. (Canvas2D strokeRect 대응, 선 중심이 경계선 위)
	// - setStrokeColor()
	//==============================================================================
	pub fn drawStrokeRect(&mut self, rect: Rect, lineWidth: f32) {
		let leftX = rect.position.x;
		let topY = rect.position.y;
		let rightX = leftX + rect.size.x;
		let bottomY = topY + rect.size.y;
		let positions = [
			Vector2::create(leftX, topY),
			Vector2::create(rightX, topY),
			Vector2::create(rightX, bottomY),
			Vector2::create(leftX, bottomY),
		];
		self.drawLineLoop(&positions, lineWidth);
	}

	//==============================================================================
	// 라운드 사각형 경로 좌표 목록 생성. (시계 방향, 볼록 다각형)
	//==============================================================================
	pub fn buildRoundRectPositions(&self, rect: Rect, roundSize: f32) -> [Vector2; ROUND_RECT_POSITION_COUNT] {
		let leftX = rect.position.x;
		let topY = rect.position.y;
		let rightX = leftX + rect.size.x;
		let bottomY = topY + rect.size.y;
		let clampedRoundSize = roundSize.min(rect.size.x.abs().min(rect.size.y.abs()) / 2.0);

		// 코너 중심 4개. (우상 → 우하 → 좌하 → 좌상)
		let cornerCenters: [(f32, f32); 4] = [
			(rightX - clampedRoundSize, topY + clampedRoundSize),
			(rightX - clampedRoundSize, bottomY - clampedRoundSize),
			(leftX + clampedRoundSize, bottomY - clampedRoundSize),
			(leftX + clampedRoundSize, topY + clampedRoundSize),
		];

		// 꼭짓점 각도는 늘 같으니 처음에 구해 둔 cos / sin 표를 쓴다. (힙 할당 없이 스택 배열로 돌려준다)
		let mut positions = [Vector2::create(0.0, 0.0); ROUND_RECT_POSITION_COUNT];
		for (cornerIndex, (cornerCenterX, cornerCenterY)) in cornerCenters.into_iter().enumerate() {
			for segmentIndex in 0..=CORNER_SEGMENT_COUNT {
				let positionIndex = cornerIndex * (CORNER_SEGMENT_COUNT + 1) + segmentIndex;
				let (cosine, sine) = self.roundRectUnitTable[positionIndex];
				positions[positionIndex] = Vector2::create(cornerCenterX + cosine * clampedRoundSize, cornerCenterY + sine * clampedRoundSize);
			}
		}
		positions
	}

	//==============================================================================
	// 볼록 다각형 채움 출력. (첫 점 기준 팬 분할)
	//==============================================================================
	pub fn drawConvexPolygon(&mut self, positions: &[Vector2], color: Color) {
		if positions.len() < 3 {
			return;
		}

		let mut offset = 0;
		let mut vertexCount = 0;
		let firstPosition = positions[0];
		for positionIndex in 1..positions.len() - 1 {
			if (vertexCount + 3) * FLOATS_PER_VERTEX > self.vertexData.len() {
				break;
			}
			let secondPosition = positions[positionIndex];
			let thirdPosition = positions[positionIndex + 1];
			let triangleValues: [f32; 12] = [
				firstPosition.x, firstPosition.y, 0.5, 0.5,
				secondPosition.x, secondPosition.y, 0.5, 0.5,
				thirdPosition.x, thirdPosition.y, 0.5, 0.5,
			];
			self.vertexData[offset..offset + 12].copy_from_slice(&triangleValues);
			offset += 12;
			vertexCount += 3;
		}

		let whiteTexture = self.getWhiteTexture();
		self.drawVertices(vertexCount, whiteTexture, color);
	}

	//==============================================================================
	// 라운드 사각형 출력.
	// - setFillColor()
	//==============================================================================
	pub fn drawRoundRect(&mut self, rect: Rect, roundSize: f32) {
		if roundSize > 0.0 {
			let positions = self.buildRoundRectPositions(rect, roundSize);
			let fillColor = self.getFillColor();
			self.drawConvexPolygon(&positions, fillColor);
		}
		else {
			self.drawRect(rect);
		}
	}

	//==============================================================================
	// 라운드 사각형 외곽선 출력.
	// - setStrokeColor()
	//==============================================================================
	pub fn drawStrokeRoundRect(&mut self, rect: Rect, roundSize: f32, lineWidth: f32) {
		if roundSize > 0.0 {
			let positions = self.buildRoundRectPositions(rect, roundSize);
			self.drawLineLoop(&positions, lineWidth);
		}
		else {
			self.drawStrokeRect(rect, lineWidth);
		}
	}

	//==============================================================================
	// 원 둘레 좌표 목록. (화면상 크기에 비례한 분할 수, 16 ~ 64)
	//==============================================================================
	fn buildCirclePositions(&self, center: Vector2, radius: f32) -> Vec<Vector2> {
		let maximumScale = self.transformMatrix.getMaximumScale();
		let segmentCount = math::clamp((radius * maximumScale * 0.5).ceil(), 16.0, 64.0) as usize;
		let mut positions = Vec::with_capacity(segmentCount);
		for segmentIndex in 0..segmentCount {
			let radian = (math::PI * 2.0) * (segmentIndex as f32 / segmentCount as f32);
			let positionX = center.x + radian.cos() * radius;
			let positionY = center.y + radian.sin() * radius;
			positions.push(Vector2::create(positionX, positionY));
		}
		positions
	}

	//==============================================================================
	// 원 출력.
	// - setFillColor()
	//==============================================================================
	pub fn drawCircle(&mut self, center: Vector2, radius: f32) {
		if radius <= 0.0 {
			return;
		}
		let positions = self.buildCirclePositions(center, radius);
		let fillColor = self.getFillColor();
		self.drawConvexPolygon(&positions, fillColor);
	}

	//==============================================================================
	// 원 외곽선 출력.
	// - setStrokeColor()
	//==============================================================================
	pub fn drawStrokeCircle(&mut self, center: Vector2, radius: f32, lineWidth: f32) {
		if radius <= 0.0 {
			return;
		}
		let positions = self.buildCirclePositions(center, radius);
		self.drawLineLoop(&positions, lineWidth);
	}

	//==============================================================================
	// 이미지 출력.
	//==============================================================================
	pub fn drawImage(&mut self, image: &Rc<Image>, position: Vector2, contentSize: Vector2) {
		let imageWidth = image.getWidth() as f32;
		let imageHeight = image.getHeight() as f32;
		self.drawImageWithSourceAndDestination(image, 0.0, 0.0, imageWidth, imageHeight, position.x, position.y, contentSize.x, contentSize.y);
	}

	//==============================================================================
	// 이미지의 일부만 출력. (imageRect 가 비었으면 전체)
	//==============================================================================
	pub fn drawImageWithImageRect(&mut self, image: &Rc<Image>, position: Vector2, contentSize: Vector2, imageRect: Rect) {
		let sourceRect = if imageRect.isZero() {
			Rect::create(0.0, 0.0, image.getWidth() as f32, image.getHeight() as f32)
		}
		else {
			imageRect
		};
		self.drawImageWithSourceAndDestination(image, sourceRect.position.x, sourceRect.position.y, sourceRect.size.x, sourceRect.size.y, position.x, position.y, contentSize.x, contentSize.y);
	}

	//==============================================================================
	// 이미지의 소스 영역을 지정 영역에 출력. (Canvas2D drawImage 9인자 대응)
	//==============================================================================
	pub fn drawImageWithSourceAndDestination(&mut self, image: &Rc<Image>, sourceX: f32, sourceY: f32, sourceWidth: f32, sourceHeight: f32, destinationX: f32, destinationY: f32, destinationWidth: f32, destinationHeight: f32) {
		let texture = match self.getImageTexture(image) {
			Some(texture) => {
				texture
			}
			None => {
				return;
			}
		};

		let imageWidth = image.getWidth() as f32;
		let imageHeight = image.getHeight() as f32;
		let leftU = sourceX / imageWidth;
		let topV = sourceY / imageHeight;
		let rightU = (sourceX + sourceWidth) / imageWidth;
		let bottomV = (sourceY + sourceHeight) / imageHeight;
		let vertexCountOffset = self.writeQuad(0, destinationX, destinationY, destinationX + destinationWidth, destinationY + destinationHeight, leftU, topV, rightU, bottomV);
		let whiteColor = self.getWhiteColor();
		self.drawVertices(vertexCountOffset / FLOATS_PER_VERTEX, texture, whiteColor);
	}

	//==============================================================================
	// 이미지 나인패치 출력.
	// - nineSlice 의 position.x / position.y / size.x / size.y 가 각각 왼쪽 / 위 / 오른쪽 / 아래 두께.
	//==============================================================================
	pub fn drawImageWithNineSlice(&mut self, image: &Rc<Image>, position: Vector2, size: Vector2, nineSlice: Rect) {
		let sourceWidth = image.getWidth() as f32;
		let sourceHeight = image.getHeight() as f32;
		let destinationX = position.x.floor();
		let destinationY = position.y.floor();
		let left = nineSlice.position.x;
		let top = nineSlice.position.y;
		let right = nineSlice.size.x;
		let bottom = nineSlice.size.y;

		// 최소 사이즈 제한 적용.
		let destinationWidth = size.x.ceil().max(left + right);
		let destinationHeight = size.y.ceil().max(top + bottom);

		let hasHorizontal = left > 0.0 || right > 0.0;
		let hasVertical = top > 0.0 || bottom > 0.0;

		if hasHorizontal && hasVertical {
			// 가로세로 다 쪼개기.
			let centerSourceWidth = sourceWidth - left - right;
			let centerSourceHeight = sourceHeight - top - bottom;
			let centerDestinationWidth = destinationWidth - left - right;
			let centerDestinationHeight = destinationHeight - top - bottom;

			// 위쪽.
			self.drawImageWithSourceAndDestination(image, 0.0, 0.0, left, top, destinationX, destinationY, left + 1.0, top + 1.0);
			self.drawImageWithSourceAndDestination(image, left, 0.0, centerSourceWidth, top, destinationX + left, destinationY, centerDestinationWidth + 1.0, top + 1.0);
			self.drawImageWithSourceAndDestination(image, sourceWidth - right, 0.0, right, top, destinationX + destinationWidth - right, destinationY, right, top + 1.0);

			// 가운데쪽.
			self.drawImageWithSourceAndDestination(image, 0.0, top, left, centerSourceHeight, destinationX, destinationY + top, left + 1.0, centerDestinationHeight + 1.0);
			self.drawImageWithSourceAndDestination(image, left, top, centerSourceWidth, centerSourceHeight, destinationX + left, destinationY + top, centerDestinationWidth + 1.0, centerDestinationHeight + 1.0);
			self.drawImageWithSourceAndDestination(image, sourceWidth - right, top, right, centerSourceHeight, destinationX + destinationWidth - right, destinationY + top, right, centerDestinationHeight + 1.0);

			// 아래쪽.
			self.drawImageWithSourceAndDestination(image, 0.0, sourceHeight - bottom, left, bottom, destinationX, destinationY + destinationHeight - bottom, left + 1.0, bottom);
			self.drawImageWithSourceAndDestination(image, left, sourceHeight - bottom, centerSourceWidth, bottom, destinationX + left, destinationY + destinationHeight - bottom, centerDestinationWidth + 1.0, bottom);
			self.drawImageWithSourceAndDestination(image, sourceWidth - right, sourceHeight - bottom, right, bottom, destinationX + destinationWidth - right, destinationY + destinationHeight - bottom, right, bottom);
		}
		else if hasHorizontal {
			// 가로만 쪼개기.
			let centerSourceWidth = sourceWidth - left - right;
			let centerDestinationWidth = destinationWidth - left - right;

			self.drawImageWithSourceAndDestination(image, 0.0, 0.0, left, sourceHeight, destinationX, destinationY, left + 1.0, destinationHeight);
			self.drawImageWithSourceAndDestination(image, left, 0.0, centerSourceWidth, sourceHeight, destinationX + left, destinationY, centerDestinationWidth + 1.0, destinationHeight);
			self.drawImageWithSourceAndDestination(image, sourceWidth - right, 0.0, right, sourceHeight, destinationX + destinationWidth - right, destinationY, right, destinationHeight);
		}
		else if hasVertical {
			// 세로만 쪼개기.
			let centerSourceHeight = sourceHeight - top - bottom;
			let centerDestinationHeight = destinationHeight - top - bottom;

			self.drawImageWithSourceAndDestination(image, 0.0, 0.0, sourceWidth, top, destinationX, destinationY, destinationWidth, top + 1.0);
			self.drawImageWithSourceAndDestination(image, 0.0, top, sourceWidth, centerSourceHeight, destinationX, destinationY + top, destinationWidth, centerDestinationHeight + 1.0);
			self.drawImageWithSourceAndDestination(image, 0.0, sourceHeight - bottom, sourceWidth, bottom, destinationX, destinationY + destinationHeight - bottom, destinationWidth, bottom);
		}
		else {
			// 쪼개기 없음.
			self.drawImageWithSourceAndDestination(image, 0.0, 0.0, sourceWidth, sourceHeight, destinationX, destinationY, destinationWidth, destinationHeight);
		}
	}

	//==============================================================================
	// 폰트 문자열 설정. (Canvas2D font 대응, 예: "16px DOSGothic")
	//==============================================================================
	pub fn setFontString(&mut self, fontString: &str) {
		if !fontString.is_empty() {
			self.fontString = fontString.to_string();
		}
	}

	//==============================================================================
	// 폰트 문자열 반환.
	//==============================================================================
	pub fn getFontString(&self) -> &str {
		&self.fontString
	}

	//==============================================================================
	// 가로 정렬 설정.
	//==============================================================================
	pub fn setTextAlign(&mut self, textAlign: TextAlign) {
		self.textAlign = textAlign;
	}

	//==============================================================================
	// 가로 정렬 반환.
	//==============================================================================
	pub fn getTextAlign(&self) -> TextAlign {
		self.textAlign
	}

	//==============================================================================
	// "start" / "end" 를 실제 방향으로 푼다. (이 판은 왼쪽에서 오른쪽만 다룬다)
	//==============================================================================
	pub fn resolveTextAlign(&self, textAlign: TextAlign) -> TextAlign {
		match textAlign {
			TextAlign::start => {
				TextAlign::left
			}
			TextAlign::end => {
				TextAlign::right
			}
			_ => {
				textAlign
			}
		}
	}

	//==============================================================================
	// 세로 기준선 설정.
	//==============================================================================
	pub fn setTextBaseline(&mut self, textBaseline: TextBaseline) {
		self.textBaseline = textBaseline;
	}

	//==============================================================================
	// 세로 기준선 반환.
	//==============================================================================
	pub fn getTextBaseline(&self) -> TextBaseline {
		self.textBaseline
	}

	//==============================================================================
	// 문자열 측정. (현재 폰트 문자열 기준)
	//==============================================================================
	pub fn measureText(&self, text: &str) -> TextMetrics {
		let fontString = self.getFontString();
		self.textStringTextureCache.measureText(fontString, text)
	}

	//==============================================================================
	// 텍스트 베이크 스케일 양자화 단위 설정.
	// - 기본 0.25. 확대 축소가 잦은 글자가 스케일마다 새로 구워지지 않게 묶는다.
	// - 0 이면 양자화하지 않고 실제 스케일 그대로 굽는다.
	//==============================================================================
	pub fn setTextBakeScaleStep(&mut self, textBakeScaleStep: f32) {
		self.textBakeScaleStep = textBakeScaleStep.max(0.0);
	}

	//==============================================================================
	// 텍스트 베이크 스케일 양자화 단위 반환.
	//==============================================================================
	pub fn getTextBakeScaleStep(&self) -> f32 {
		self.textBakeScaleStep
	}

	//==============================================================================
	// 텍스트 픽셀 스냅 설정.
	// - 켜면 회전 / 기울임이 없는 변환에서 글자 쿼드의 왼쪽 위를 디바이스 픽셀 격자에 맞춘다.
	//==============================================================================
	pub fn setTextPixelSnapEnabled(&mut self, isTextPixelSnapEnabled: bool) {
		self.isTextPixelSnapEnabled = isTextPixelSnapEnabled;
	}

	//==============================================================================
	// 텍스트 픽셀 스냅 여부 반환.
	//==============================================================================
	pub fn isTextPixelSnapEnabled(&self) -> bool {
		self.isTextPixelSnapEnabled
	}

	//==============================================================================
	// 텍스트 베이크 스케일 계산. (현재 변환 행렬 기반, 양자화 단위는 setTextBakeScaleStep)
	//==============================================================================
	pub fn calculateTextBakeScale(&self) -> f32 {
		let maximumScale = self.transformMatrix.getMaximumScale();
		let textBakeScaleStep = self.getTextBakeScaleStep();
		let quantizedScale = if textBakeScaleStep > 0.0 {
			(maximumScale / textBakeScaleStep).round() * textBakeScaleStep
		}
		else {
			maximumScale
		};
		math::clamp(quantizedScale, 0.25, MAXIMUM_TEXT_BAKE_SCALE)
	}

	//==============================================================================
	// 문자열 텍스처 엔트리를 정렬/베이스라인에 맞춰 쿼드로 출력.
	//==============================================================================
	pub fn drawTextEntry(&mut self, entry: TextEntry, x: f32, y: f32, color: Option<Color>) {
		// 가로 정렬 오프셋.
		let textAlign = self.resolveTextAlign(self.getTextAlign());
		let alignOffset = match textAlign {
			TextAlign::center => {
				-entry.advanceWidth / 2.0
			}
			TextAlign::right => {
				-entry.advanceWidth
			}
			_ => {
				0.0
			}
		};

		// 요청 베이스라인 → 알파벳 베이스라인 오프셋.
		let textBaseline = self.getTextBaseline();
		let baselineOffset = match textBaseline {
			TextBaseline::top => {
				entry.fontAscent
			}
			TextBaseline::hanging => {
				entry.fontAscent * 0.8
			}
			TextBaseline::middle => {
				(entry.fontAscent - entry.fontDescent) / 2.0
			}
			TextBaseline::ideographic | TextBaseline::bottom => {
				-entry.fontDescent
			}
			TextBaseline::alphabetic => {
				0.0
			}
		};

		let mut quadX = x + alignOffset - entry.penOffsetX;
		let mut quadY = y + baselineOffset - entry.baselineOffsetY;

		// 픽셀 스냅. (회전 / 기울임이 없을 때만, 쿼드의 왼쪽 위를 디바이스 픽셀에 맞춘다)
		let isTextPixelSnapEnabled = self.isTextPixelSnapEnabled();
		let transformMatrix = self.transformMatrix;
		if isTextPixelSnapEnabled && transformMatrix.b == 0.0 && transformMatrix.c == 0.0 && transformMatrix.a > 0.0 && transformMatrix.d > 0.0 {
			let devicePoint = transformMatrix.transformPoint(quadX, quadY);
			quadX += (devicePoint.x.round() - devicePoint.x) / transformMatrix.a;
			quadY += (devicePoint.y.round() - devicePoint.y) / transformMatrix.d;
		}

		let vertexCountOffset = self.writeQuad(0, quadX, quadY, quadX + entry.quadWidth, quadY + entry.quadHeight, 0.0, 0.0, 1.0, 1.0);
		let whiteColor = self.getWhiteColor();
		let drawColor = color.unwrap_or(whiteColor);
		self.drawVertices(vertexCountOffset / FLOATS_PER_VERTEX, entry.texture, drawColor);
	}

	//==============================================================================
	// 텍스트 채움 출력. (Canvas2D fillText 대응)
	// - setFillColor() / setFontString() / setTextAlign() / setTextBaseline()
	//==============================================================================
	pub fn drawFillText(&mut self, text: &str, x: f32, y: f32) {
		if text.is_empty() {
			return;
		}

		// 글자는 흰색으로 굽고 그릴 때 색을 곱한다.
		let fillColor = self.getFillColor();
		let bakeScale = self.calculateTextBakeScale();
		let entry = self.textStringTextureCache.getEntry(&self.gl, TextDrawKind::fill, text, &self.fontString, 0.0, bakeScale);
		if let Some(entry) = entry {
			self.drawTextEntry(entry, x, y, Some(fillColor));
		}
	}

	//==============================================================================
	// 텍스트 외곽선 출력. (Canvas2D strokeText 대응)
	// - setStrokeColor() / setFontString() / setTextAlign() / setTextBaseline()
	//==============================================================================
	pub fn drawStrokeText(&mut self, text: &str, x: f32, y: f32, lineWidth: f32) {
		if text.is_empty() {
			return;
		}

		let strokeColor = self.getStrokeColor();
		let bakeScale = self.calculateTextBakeScale();
		let entry = self.textStringTextureCache.getEntry(&self.gl, TextDrawKind::stroke, text, &self.fontString, lineWidth, bakeScale);
		if let Some(entry) = entry {
			self.drawTextEntry(entry, x, y, Some(strokeColor));
		}
	}

	//==============================================================================
	// 출력 영역 제한 시작.
	// - 스텐실 버퍼 기반이라 회전/스케일이 걸린 영역도 정확히 잘린다.
	// - roundSize 를 주면 둥근 사각형으로 잘린다.
	//==============================================================================
	pub fn beginClipRect(&mut self, rect: Rect, roundSize: f32) {
		// 모아 둔 것은 지금 스텐실 상태로 먼저 내보낸다.
		self.flushBatch();

		// 상태 저장. (Canvas2D save 대응)
		self.pushState();

		let previousClipDepth = self.clipStack.len() as i32;
		self.clipStack.push(ClipEntry {
			rect,
			roundSize,
			transformMatrix: self.transformMatrix,
		});

		unsafe {
			if previousClipDepth == 0 {
				self.gl.enable(glow::STENCIL_TEST);
			}

			// 스텐실에 클립 영역 기록. (색상 출력 없이 INCR)
			self.gl.color_mask(false, false, false, false);
			self.gl.stencil_func(glow::ALWAYS, 0, 0xFF);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::INCR);
		}
		self.batchSuspendDepth += 1;
		if roundSize > 0.0 {
			self.drawRoundRect(rect, roundSize);
		}
		else {
			self.drawRect(rect);
		}
		self.batchSuspendDepth -= 1;
		unsafe {
			self.gl.color_mask(true, true, true, true);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);
			self.gl.stencil_func(glow::EQUAL, previousClipDepth + 1, 0xFF);
		}
	}

	//==============================================================================
	// 출력 영역 제한 종료.
	//==============================================================================
	pub fn endClipRect(&mut self) {
		let clipEntry = match self.clipStack.pop() {
			Some(clipEntry) => {
				clipEntry
			}
			None => {
				return;
			}
		};

		// 클립 안에서 모아 둔 것을 먼저 내보낸다.
		self.flushBatch();

		// 기록 시점의 변환 행렬로 같은 영역을 DECR 해 스텐실 원복.
		// (begin 과 end 사이에 변환이 바뀌었을 수 있으므로 스냅샷 사용)
		self.transformMatrix = clipEntry.transformMatrix;
		unsafe {
			self.gl.color_mask(false, false, false, false);
			self.gl.stencil_func(glow::ALWAYS, 0, 0xFF);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::DECR);
		}
		self.batchSuspendDepth += 1;
		if clipEntry.roundSize > 0.0 {
			self.drawRoundRect(clipEntry.rect, clipEntry.roundSize);
		}
		else {
			self.drawRect(clipEntry.rect);
		}
		self.batchSuspendDepth -= 1;
		unsafe {
			self.gl.color_mask(true, true, true, true);
			self.gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);

			let clipDepth = self.clipStack.len() as i32;
			if clipDepth == 0 {
				self.gl.disable(glow::STENCIL_TEST);
			}
			else {
				self.gl.stencil_func(glow::EQUAL, clipDepth, 0xFF);
			}
		}

		// 상태 복원. (Canvas2D restore 대응)
		self.popState();
	}

	//==============================================================================
	// 셰이더 프로그램 반환. (바꿔 끼운 것이 있으면 그것, 없으면 기본 프로그램)
	//==============================================================================
	pub fn getShaderProgram(&self) -> &ShaderProgram {
		match &self.shaderProgramOverride {
			Some(shaderProgramOverride) => {
				shaderProgramOverride
			}
			None => {
				&self.shaderProgram
			}
		}
	}

	//==============================================================================
	// 셰이더 프로그램 바꿔 끼우기. (None 이면 기본 프로그램으로 복귀)
	// - 기본 프로그램과 같은 어트리뷰트 위치 / 유니폼 이름(projectionMatrix, modelMatrix, mainColor, tintColor, globalAlpha, mainTexture)을 갖춰야 한다.
	// - 바꿔 끼운 프로그램이 쓰이는 동안 그 프로그램이 사용 중이다. 효과 유니폼은 바꿔 끼운 바로 뒤에 GL 로 직접 올린다.
	//==============================================================================
	pub fn setShaderProgramOverride(&mut self, shaderProgram: Option<Rc<ShaderProgram>>) {
		self.flushBatch();
		self.shaderProgramOverride = shaderProgram;
		let activeShaderProgram = self.getShaderProgram();
		activeShaderProgram.useProgram(&self.gl);
		let projectionMatrixLocation = activeShaderProgram.getUniformLocation(&self.gl, "projectionMatrix");
		let mainTextureLocation = activeShaderProgram.getUniformLocation(&self.gl, "mainTexture");
		unsafe {
			self.gl.uniform_matrix_3_f32_slice(projectionMatrixLocation.as_ref(), false, &self.projectionMatrixArray);
			self.gl.uniform_1_i32(mainTextureLocation.as_ref(), 0);
		}
	}

	//==============================================================================
	// 이름으로 담아 둔 바꿔 끼울 셰이더 프로그램 반환. (없으면 createShaderSource 가 준 버텍스 / 프래그먼트 소스로 만들어 담는다)
	// - 원본 ShaderSprite 의 컨텍스트별 프로그램 캐시 자리. 러스트 판은 GL 컨텍스트를 든 Graphic 이 담는다.
	// - 어트리뷰트 위치를 기본 프로그램과 같게 고정해 링크하므로 기본 버텍스 어레이를 그대로 쓴다.
	// - 만들기에 실패하면 한 번만 알리고 None 을 담아 두어 다시 만들지 않는다.
	//==============================================================================
	pub fn resolveShaderProgram(&mut self, programGroupName: &'static str, programName: &'static str, createShaderSource: impl FnOnce() -> (String, String)) -> Option<Rc<ShaderProgram>> {
		let cacheKey = (programGroupName, programName);
		if let Some(cachedShaderProgram) = self.shaderProgramCacheTable.get(&cacheKey) {
			return cachedShaderProgram.clone();
		}
		let (vertexShaderSource, fragmentShaderSource) = createShaderSource();
		let mut attributeLocationList: Vec<(&str, u32)> = Vec::new();
		for attributeName in ["vertexPosition", "vertexTextureCoordinate"] {
			if let Some(attributeLocation) = self.shaderProgram.getAttributeLocation(&self.gl, attributeName) {
				attributeLocationList.push((attributeName, attributeLocation));
			}
		}
		let shaderProgram = match ShaderProgram::createWithAttributeLocations(&self.gl, &vertexShaderSource, &fragmentShaderSource, &attributeLocationList) {
			Ok(shaderProgram) => {
				Some(Rc::new(shaderProgram))
			}
			Err(error) => {
				eprintln!("Graphic: shader program \"{}.{}\" failed: {}", programGroupName, programName, error);
				None
			}
		};
		self.shaderProgramCacheTable.insert(cacheKey, shaderProgram.clone());
		shaderProgram
	}

	//==============================================================================
	// 렌더 상태 복구. (외부 GL 패스가 프로그램 / 버텍스 어레이 / 뷰포트 / 블렌드를 바꾼 뒤 Graphic 출력을 이어갈 때)
	// - 원본과 같고, 클립 중이면 스텐실 테스트도 다시 켠다. (원본은 켜지 않는다)
	//==============================================================================
	pub fn restoreRenderState(&mut self) {
		let shaderProgram = self.getShaderProgram();
		shaderProgram.useProgram(&self.gl);
		let projectionMatrixLocation = shaderProgram.getUniformLocation(&self.gl, "projectionMatrix");
		let mainTextureLocation = shaderProgram.getUniformLocation(&self.gl, "mainTexture");
		unsafe {
			self.gl.viewport(0, 0, self.appliedViewportWidth, self.appliedViewportHeight);
			self.gl.bind_vertex_array(Some(self.vertexArray));
			self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertexBuffer));
			self.gl.uniform_matrix_3_f32_slice(projectionMatrixLocation.as_ref(), false, &self.projectionMatrixArray);
			self.gl.uniform_1_i32(mainTextureLocation.as_ref(), 0);
			self.gl.active_texture(glow::TEXTURE0);
			self.gl.enable(glow::BLEND);
			self.gl.disable(glow::DEPTH_TEST);
			self.gl.disable(glow::CULL_FACE);
			if !self.clipStack.is_empty() {
				self.gl.enable(glow::STENCIL_TEST);
			}
		}
		let blendMode = self.getBlendMode();
		self.applyBlendMode(blendMode);
	}

	//==============================================================================
	// 드로잉 버퍼 너비 반환. (픽셀, 원본 webGL2RenderingContext.drawingBufferWidth)
	//==============================================================================
	pub fn getDrawingBufferWidth(&self) -> i32 {
		self.appliedViewportWidth
	}

	//==============================================================================
	// 드로잉 버퍼 높이 반환. (픽셀, 원본 webGL2RenderingContext.drawingBufferHeight)
	//==============================================================================
	pub fn getDrawingBufferHeight(&self) -> i32 {
		self.appliedViewportHeight
	}

	//==============================================================================
	// 지금 그린 화면 읽기. (너비, 높이, 위에서 아래로 놓인 RGBA. 화면을 넘기기 전에 부른다)
	// - 원본 캔버스처럼 불투명으로 본다. (알파 255)
	//==============================================================================
	pub fn readFramebufferPixels(&self) -> (u32, u32, Vec<u8>) {
		let width = self.appliedViewportWidth.max(0) as usize;
		let height = self.appliedViewportHeight.max(0) as usize;
		let rowByteCount = width * 4;
		let mut bottomUpPixels = vec![0u8; rowByteCount * height];
		unsafe {
			self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
			self.gl.read_pixels(0, 0, width as i32, height as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut bottomUpPixels)));
		}
		let mut topDownPixels = vec![0u8; rowByteCount * height];
		for rowIndex in 0..height {
			let sourceStart = (height - 1 - rowIndex) * rowByteCount;
			let destinationStart = rowIndex * rowByteCount;
			topDownPixels[destinationStart..destinationStart + rowByteCount].copy_from_slice(&bottomUpPixels[sourceStart..sourceStart + rowByteCount]);
		}
		for pixel in topDownPixels.as_chunks_mut::<4>().0 {
			pixel[3] = 255;
		}
		(width as u32, height as u32, topDownPixels)
	}

	//==============================================================================
	// GL 컨텍스트 반환.
	//==============================================================================
	pub fn getGL(&self) -> &glow::Context {
		&self.gl
	}

	//==============================================================================
	// 흰색 텍스처 반환.
	//==============================================================================
	pub fn getWhiteTexture(&self) -> GlTexture {
		self.whiteTexture
	}

	//==============================================================================
	// 흰색 반환.
	//==============================================================================
	pub fn getWhiteColor(&self) -> Color {
		self.whiteColor
	}

	//==============================================================================
	// 이미지 텍스처 캐시 반환.
	//==============================================================================
	pub fn getImageTextureCache(&self) -> &ImageTextureCache {
		&self.imageTextureCache
	}

	//==============================================================================
	// 문자열 텍스처 캐시 반환. (폰트 등록)
	//==============================================================================
	pub fn getTextStringTextureCache(&mut self) -> &mut TextStringTextureCache {
		&mut self.textStringTextureCache
	}
}

//==============================================================================
// f32 배열을 바이트 배열로 본다. (GL 버퍼 업로드용)
//==============================================================================
fn floatSliceToBytes(values: &[f32]) -> &[u8] {
	unsafe {
		std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
	}
}

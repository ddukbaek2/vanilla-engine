//==============================================================================
// 3D 모델 보기 예제. (데스크탑, 원본 examples/ancientmountain 의 3D 경로를 엔진 루프 안에서 돌린다)
// - 원본 예제의 soldier.glb, fox.glb(노멀 없음), Remy.fbx + anims/idle.fbx, walk.fbx 를 BlobAsset 으로 읽는다.
//   애셋 폴더 후보에 원본 저장소의 절대 경로를 넣는다. (읽기만 한다)
// - 땅은 코드로 만든 GLB 다. (정적 메시 → 합성 스킨 경로)
// - 원본 3D 씬은 자체 rAF 를 돌리지만, 여기서는 씬의 preDraw 에서 그린다:
//   섀도우 깊이 패스 → 씬 패스(RenderTarget) → 블룸 → 합성(화면) → graphic.restoreRenderState() → draw 에서 2D HUD.
// - 모델은 tick 에서 update 한다. (update 는 GL 을 쓰지 않는다) GL 리소스는 첫 preDraw 에서 만든다. (create 에는 GL 이 없다)
// - 실행: cargo run --release --example modelviewer (갱신 600번 뒤 스스로 끝난다, 60fps 면 10초)
//   화면 저장: VANILLA_CAPTURE_PATH=shots/model_{frame}.png VANILLA_CAPTURE_FRAMES=60,180 VANILLA_CAPTURE_EXIT=1
//==============================================================================
#![allow(non_snake_case)]

use std::path::PathBuf;
use vanilla::{
	AmbientOcclusionEffect, BlobAsset, BloomEffect, Color, Engine, EngineConfiguration, FullscreenPass, Graphic, Matrix4, Rect, RenderTarget, Scene, SceneBase, ShadowMap, SkinnedModel, SkinnedModelRenderer, SubsurfaceScatteringEffect, TextAlign, TextBaseline, Vector3,
};
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 원본 예제 애셋 폴더. (vanilla.js 저장소를 이 저장소 옆에 받아 둔 것으로 본다. 읽기만 한다)
const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/ancientmountain/assets";

// 섀도우 맵 해상도.
const SHADOWMAP_RESOLUTION: i32 = 2048;

// 스스로 끝내는 시각. (초)
const EXIT_SECONDS: f32 = 10.0;

// 갱신 걸음. (화면 저장이 프레임 번호로 같은 장면을 잡도록 프레임마다 1/60초씩 고정으로 나아간다)
const FIXED_TIME_STEP: f32 = 1.0 / 60.0;

// 병사 클립 순서와 바꾸는 간격. (초)
const SOLDIER_CLIP_NAMES: [&str; 3] = ["Idle", "Walk", "Run"];
const SOLDIER_CLIP_SECONDS: f32 = 2.5;

// 합성 패스. (씬 + 블룸 가산 + 약한 비네트)
const COMPOSITE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
uniform sampler2D bloomTexture;
uniform float bloomStrength;
out vec4 outputColor;
void main() {
	vec3 sceneColor = texture(sceneTexture, fragmentTextureCoordinate).rgb;
	vec3 bloomColor = texture(bloomTexture, fragmentTextureCoordinate).rgb;
	vec2 centered = fragmentTextureCoordinate - 0.5;
	float vignette = 1.0 - dot(centered, centered) * 0.6;
	outputColor = vec4((sceneColor + bloomColor * bloomStrength) * vignette, 1.0);
}";

//==============================================================================
// 그려 둘 모델 하나. (모델 + 놓을 자리 + 크기 배율)
//==============================================================================
struct PlacedModel {
	skinnedModel: SkinnedModel,
	positionX: f32,
	positionZ: f32,
	rotationY: f32,
	scale: f32,
	label: String,
}

impl PlacedModel {
	//==============================================================================
	// 모델 행렬. (이동 x Y 회전 x 크기)
	//==============================================================================
	fn createModelMatrix(&self) -> Matrix4 {
		let mut modelMatrix = Matrix4::createTranslation(self.positionX, 0.0, self.positionZ);
		modelMatrix.rotateY(self.rotationY);
		modelMatrix.scale(self.scale, self.scale, self.scale);
		modelMatrix
	}
}

//==============================================================================
// 첫 preDraw 에서 만드는 GL 리소스.
//==============================================================================
struct ViewerResources {
	skinnedModelRenderer: SkinnedModelRenderer,
	groundModel: SkinnedModel,
	placedModelList: Vec<PlacedModel>,
	shadowMap: ShadowMap,
	sceneRenderTarget: RenderTarget,
	bloomEffect: BloomEffect,
	compositePass: FullscreenPass,
	ambientOcclusionEffect: AmbientOcclusionEffect,
	subsurfaceScatteringEffect: SubsurfaceScatteringEffect,
	isFirstFrameChecked: bool,
}

//==============================================================================
// 모델 보기 씬.
//==============================================================================
struct ModelViewerScene {
	sceneBase: SceneBase,
	soldierAsset: BlobAsset,
	foxAsset: BlobAsset,
	remyAsset: BlobAsset,
	idleAnimationAsset: BlobAsset,
	walkAnimationAsset: BlobAsset,
	resources: Option<ViewerResources>,
	loadErrorMessage: Option<String>,
	elapsedTime: f32,
	soldierClipIndex: usize,
	remyClipName: String,
	loadSeconds: f32,
}

impl ModelViewerScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> ModelViewerScene {
		ModelViewerScene {
			sceneBase: SceneBase::new(),
			soldierAsset: BlobAsset::new(),
			foxAsset: BlobAsset::new(),
			remyAsset: BlobAsset::new(),
			idleAnimationAsset: BlobAsset::new(),
			walkAnimationAsset: BlobAsset::new(),
			resources: None,
			loadErrorMessage: None,
			elapsedTime: 0.0,
			soldierClipIndex: 0,
			remyClipName: String::from("Idle"),
			loadSeconds: 0.0,
		}
	}

	//==============================================================================
	// GL 리소스와 모델 만들기. (BlobAsset 바이트 → 렌더러, 모델, 그림자, 렌더 타겟, 이펙트)
	//==============================================================================
	fn createResources(&self, gl: &glow::Context) -> Result<ViewerResources, String> {
		let mut skinnedModelRenderer = SkinnedModelRenderer::new(gl, None, None)?;
		let sunDirection = Vector3::create(0.5, 0.8, 0.35).normalize();
		skinnedModelRenderer.setSunDirection(sunDirection.x, sunDirection.y, sunDirection.z);
		skinnedModelRenderer.setFogColor(0.62, 0.72, 0.86);
		skinnedModelRenderer.setFogRange(25.0, 80.0);

		let readBytes = |blobAsset: &BlobAsset, label: &str| blobAsset.getBlob().ok_or_else(|| format!("{} 읽기 실패", label));
		let mut placedModelList = Vec::new();

		// 병사. (GLB, 인덱스 + 노멀맵)
		let soldierBytes = readBytes(&self.soldierAsset, "soldier.glb")?;
		let mut soldierModel = SkinnedModel::loadFromBytes(gl, &soldierBytes, &skinnedModelRenderer)?;
		soldierModel.playAnimation(SOLDIER_CLIP_NAMES[0], 0.0);
		placedModelList.push(PlacedModel { scale: measureModelScale(&mut soldierModel, 1.8), skinnedModel: soldierModel, positionX: -1.0, positionZ: 0.0, rotationY: std::f32::consts::PI + 0.4, label: String::from("soldier.glb") });

		// 여우. (GLB, 노멀 없음 → 스무스 노멀, 인덱스 없음)
		let foxBytes = readBytes(&self.foxAsset, "fox.glb")?;
		let mut foxModel = SkinnedModel::loadFromBytes(gl, &foxBytes, &skinnedModelRenderer)?;
		foxModel.playAnimation("Walk", 0.0);
		placedModelList.push(PlacedModel { scale: measureModelScale(&mut foxModel, 0.7), skinnedModel: foxModel, positionX: 0.1, positionZ: 1.1, rotationY: 1.2, label: String::from("fox.glb") });

		// 레미. (FBX + 애니메이션 FBX 두 개를 본 이름으로 붙인다)
		let remyBytes = readBytes(&self.remyAsset, "Remy.fbx")?;
		let mut remyModel = SkinnedModel::loadFromFbxBytes(gl, &remyBytes, &skinnedModelRenderer)?;
		remyModel.addAnimationsFromFbxBytes(&readBytes(&self.idleAnimationAsset, "idle.fbx")?, "Idle", false)?;
		remyModel.addAnimationsFromFbxBytes(&readBytes(&self.walkAnimationAsset, "walk.fbx")?, "Walk", true)?;
		remyModel.playAnimation("Idle", 0.0);
		placedModelList.push(PlacedModel { scale: measureModelScale(&mut remyModel, 1.75), skinnedModel: remyModel, positionX: 1.0, positionZ: -0.2, rotationY: -0.4, label: String::from("Remy.fbx") });

		// 땅. (코드로 만든 GLB, 스킨 없는 정적 메시)
		let mut groundModel = SkinnedModel::loadFromBytes(gl, &buildGroundGlb(6.0, [0.46, 0.52, 0.4]), &skinnedModelRenderer)?;
		groundModel.update(0.0);

		// 그림자, 씬 렌더 타겟, 블룸, 합성.
		let shadowMap = ShadowMap::new(gl, SHADOWMAP_RESOLUTION)?;
		let sceneRenderTarget = RenderTarget::new(gl, 2, 2, true, None)?;
		let mut bloomEffect = BloomEffect::new(gl)?;
		bloomEffect.setThreshold(0.7, 0.95);
		let compositePass = FullscreenPass::new(gl, COMPOSITE_FRAGMENTSHADER_SOURCE)?;

		// 쓰지는 않지만 셰이더가 데스크탑 GL 에서 컴파일되는지 본다.
		let mut ambientOcclusionEffect = AmbientOcclusionEffect::new(gl)?;
		ambientOcclusionEffect.resize(gl, 64, 64)?;
		let mut subsurfaceScatteringEffect = SubsurfaceScatteringEffect::new(gl)?;
		subsurfaceScatteringEffect.resize(gl, 64, 64)?;

		Ok(ViewerResources {
			skinnedModelRenderer,
			groundModel,
			placedModelList,
			shadowMap,
			sceneRenderTarget,
			bloomEffect,
			compositePass,
			ambientOcclusionEffect,
			subsurfaceScatteringEffect,
			isFirstFrameChecked: false,
		})
	}

	//==============================================================================
	// 3D 그리기. (그림자 → 씬 → 블룸 → 합성, 2D 상태는 호출자가 되돌린다)
	//==============================================================================
	fn renderScene(resources: &mut ViewerResources, gl: &glow::Context, drawingBufferWidth: i32, drawingBufferHeight: i32, elapsedTime: f32) -> Result<(), String> {
		resources.sceneRenderTarget.resize(gl, drawingBufferWidth, drawingBufferHeight)?;
		resources.bloomEffect.resize(gl, drawingBufferWidth, drawingBufferHeight)?;

		// 카메라. (천천히 도는 궤도)
		let orbitAngle = 0.35 + elapsedTime * 0.2;
		let cameraPosition = Vector3::create(orbitAngle.sin() * 4.6, 1.7, orbitAngle.cos() * 4.6);
		let cameraTarget = Vector3::create(0.0, 0.8, 0.2);
		let viewMatrix = Matrix4::createLookAt(cameraPosition, cameraTarget, Vector3::up());
		let aspectRatio = drawingBufferWidth as f32 / drawingBufferHeight.max(1) as f32;
		let mut viewProjectionMatrix = Matrix4::createPerspective(std::f32::consts::PI / 3.5, aspectRatio, 0.1, 100.0);
		viewProjectionMatrix.multiply(&viewMatrix);
		let modelMatrixList: Vec<Matrix4> = resources.placedModelList.iter().map(|placedModel| placedModel.createModelMatrix()).collect();
		let identityMatrix = Matrix4::createIdentity();

		// 1. 섀도우 깊이 패스.
		let sunDirection = Vector3::create(0.5, 0.8, 0.35).normalize();
		let lightTravelDirection = sunDirection.multiplyScalar(-1.0);
		resources.shadowMap.updateLightMatrix(Vector3::create(0.0, 0.8, 0.2), lightTravelDirection, 4.0, 0.5, 30.0);
		let lightViewProjectionElements = *resources.shadowMap.getLightViewProjectionMatrix().getElements();
		resources.shadowMap.beginRender(gl);
		unsafe {
			gl.disable(glow::BLEND);
			gl.disable(glow::CULL_FACE);
		}
		for (placedModel, modelMatrix) in resources.placedModelList.iter_mut().zip(modelMatrixList.iter()) {
			resources.skinnedModelRenderer.drawDepth(gl, &mut placedModel.skinnedModel, &lightViewProjectionElements, modelMatrix.getElements());
		}
		let shadowDepthTexture = resources.shadowMap.getDepthTexture();
		resources.skinnedModelRenderer.setShadowMap(Some(shadowDepthTexture), Some(&lightViewProjectionElements), 1.0 / SHADOWMAP_RESOLUTION as f32);
		resources.skinnedModelRenderer.setShadowStrength(1.0);

		// 2. 씬 패스. (렌더 타겟에 하늘색으로 비우고 땅과 모델)
		resources.sceneRenderTarget.bind(gl);
		unsafe {
			gl.clear_color(0.62, 0.72, 0.86, 1.0);
			gl.depth_mask(true);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
			gl.enable(glow::DEPTH_TEST);
			gl.enable(glow::CULL_FACE);
		}
		resources.skinnedModelRenderer.draw(gl, &mut resources.groundModel, viewProjectionMatrix.getElements(), identityMatrix.getElements(), cameraPosition.x, cameraPosition.y, cameraPosition.z);
		for (placedModel, modelMatrix) in resources.placedModelList.iter_mut().zip(modelMatrixList.iter()) {
			resources.skinnedModelRenderer.draw(gl, &mut placedModel.skinnedModel, viewProjectionMatrix.getElements(), modelMatrix.getElements(), cameraPosition.x, cameraPosition.y, cameraPosition.z);
		}
		unsafe {
			gl.disable(glow::CULL_FACE);
		}

		// 3. 블룸.
		let sceneColorTexture = resources.sceneRenderTarget.getColorTexture(0);
		resources.bloomEffect.render(gl, sceneColorTexture);

		// 4. 합성 → 화면.
		let compositePass = &resources.compositePass;
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.viewport(0, 0, drawingBufferWidth, drawingBufferHeight);
		}
		compositePass.useProgram(gl);
		unsafe {
			gl.uniform_1_i32(compositePass.getUniformLocation(gl, "sceneTexture").as_ref(), 0);
			gl.uniform_1_i32(compositePass.getUniformLocation(gl, "bloomTexture").as_ref(), 1);
			gl.uniform_1_f32(compositePass.getUniformLocation(gl, "bloomStrength").as_ref(), 0.8);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, resources.bloomEffect.getResultTexture());
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, sceneColorTexture);
		}
		compositePass.draw(gl);
		unsafe {
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);
		}

		// 첫 프레임에 GL 오류를 한 번 본다.
		if !resources.isFirstFrameChecked {
			resources.isFirstFrameChecked = true;
			let errorCode = unsafe {
				gl.get_error()
			};
			println!("modelviewer: 첫 3D 프레임 GL 오류 코드 0x{:X}", errorCode);
			let _ = (resources.ambientOcclusionEffect.getResultTexture(), resources.subsurfaceScatteringEffect.getResultTexture());
		}
		Ok(())
	}
}

impl Scene for ModelViewerScene {
	fn getSceneBase(&self) -> &SceneBase {
		&self.sceneBase
	}

	fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		&mut self.sceneBase
	}

	//==============================================================================
	// 생성. (모델 바이트 요청, 데스크탑은 그 자리에서 읽힌다)
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.soldierAsset.load(engine, "soldier.glb");
		self.foxAsset.load(engine, "fox.glb");
		self.remyAsset.load(engine, "Remy.fbx");
		self.idleAnimationAsset.load(engine, "anims/idle.fbx");
		self.walkAnimationAsset.load(engine, "anims/walk.fbx");
		self.sceneBase.trackAsset(&self.soldierAsset);
		self.sceneBase.trackAsset(&self.foxAsset);
		self.sceneBase.trackAsset(&self.remyAsset);
		self.sceneBase.trackAsset(&self.idleAnimationAsset);
		self.sceneBase.trackAsset(&self.walkAnimationAsset);
	}

	//==============================================================================
	// 갱신. (클립 바꾸기 + 모델 포즈 계산, 정해진 시각에 끝내기)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, _timeDelta: f32) {
		let deltaTime = FIXED_TIME_STEP;
		self.elapsedTime += deltaTime;
		if self.elapsedTime >= EXIT_SECONDS {
			engine.exitApplication();
		}
		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return;
			}
		};
		let soldierClipIndex = ((self.elapsedTime / SOLDIER_CLIP_SECONDS) as usize) % SOLDIER_CLIP_NAMES.len();
		if soldierClipIndex != self.soldierClipIndex {
			self.soldierClipIndex = soldierClipIndex;
			resources.placedModelList[0].skinnedModel.playAnimation(SOLDIER_CLIP_NAMES[soldierClipIndex], 0.3);
		}
		let remyClipName = if ((self.elapsedTime / 2.0) as usize).is_multiple_of(2) {
			"Idle"
		}
		else {
			"Walk"
		};
		if remyClipName != self.remyClipName {
			self.remyClipName = remyClipName.to_string();
			resources.placedModelList[2].skinnedModel.playAnimation(remyClipName, 0.3);
		}
		for placedModel in resources.placedModelList.iter_mut() {
			placedModel.skinnedModel.update(deltaTime);
		}
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 3D 패스 → 2D 상태 복구)
	//==============================================================================
	fn preDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		graphic.flushBatch();
		if self.resources.is_none() && self.loadErrorMessage.is_none() {
			let loadStartInstant = std::time::Instant::now();
			match self.createResources(graphic.getGL()) {
				Ok(resources) => {
					self.loadSeconds = loadStartInstant.elapsed().as_secs_f32();
					println!("modelviewer: 모델 준비 {:.2}초", self.loadSeconds);
					for placedModel in resources.placedModelList.iter() {
						println!("modelviewer: {} 드로어블 {} 노드 {} 클립 {:?} 배율 {:.4}", placedModel.label, placedModel.skinnedModel.getDrawableList().len(), placedModel.skinnedModel.getNodeList().len(), placedModel.skinnedModel.getAnimationNameList(), placedModel.scale);
					}
					self.resources = Some(resources);
				}
				Err(error) => {
					eprintln!("modelviewer: {}", error);
					self.loadErrorMessage = Some(error);
				}
			}
		}
		let canvasPixelSize = engine.getViewManager().getCanvasPixelSize();
		let elapsedTime = self.elapsedTime;
		if let Some(resources) = &mut self.resources {
			if let Err(error) = ModelViewerScene::renderScene(resources, graphic.getGL(), canvasPixelSize.x as i32, canvasPixelSize.y as i32, elapsedTime) {
				eprintln!("modelviewer: {}", error);
			}
		}
		graphic.restoreRenderState();
	}

	//==============================================================================
	// 2D HUD 출력. (3D 위에 반투명 판과 글)
	//==============================================================================
	fn draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		engine.getViewManager().applyCanvasNativeRect(graphic);
		let mut lineList = vec![String::from("바닐라엔진 graphics3d modelviewer")];
		match &self.resources {
			Some(resources) => {
				lineList.push(format!("soldier.glb  {}", SOLDIER_CLIP_NAMES[self.soldierClipIndex]));
				lineList.push(String::from("fox.glb  Walk (smooth normals)"));
				lineList.push(format!("Remy.fbx  {} (anims/*.fbx retarget)", self.remyClipName));
				lineList.push(format!("shadow {} + bloom, load {:.1}s, t {:.1}s", resources.shadowMap.getResolution(), self.loadSeconds, self.elapsedTime));
			}
			None => {
				lineList.push(self.loadErrorMessage.clone().unwrap_or_else(|| String::from("loading...")));
			}
		}
		graphic.setFillColor(Color::create(0.0, 0.0, 0.0, 0.55));
		graphic.drawRect(Rect::create(12.0, 12.0, 430.0, 18.0 + lineList.len() as f32 * 22.0));
		graphic.setFontString("16px default");
		graphic.setTextAlign(TextAlign::left);
		graphic.setTextBaseline(TextBaseline::middle);
		for (lineIndex, line) in lineList.iter().enumerate() {
			let textColor = if lineIndex == 0 {
				Color::create(1.0, 0.85, 0.45, 1.0)
			}
			else {
				Color::white()
			};
			graphic.setFillColor(textColor);
			graphic.drawFillText(line, 24.0, 32.0 + lineIndex as f32 * 22.0);
		}
	}
}

//==============================================================================
// 모델 크기 배율 구하기. (바인드/첫 포즈에서 노드 월드 위치의 가장 높은 Y 를 targetHeight 로 맞춘다)
//==============================================================================
fn measureModelScale(skinnedModel: &mut SkinnedModel, targetHeight: f32) -> f32 {
	skinnedModel.update(0.0);
	let maximumHeight = skinnedModel.getNodeList().iter().map(|node| node.worldMatrix.elements[13]).fold(0.0f32, f32::max);
	if maximumHeight > 0.0 {
		targetHeight / maximumHeight
	}
	else {
		1.0
	}
}

//==============================================================================
// 땅 GLB 만들기. (가로세로 2 x halfSize 사각형, 위쪽 노멀, 16비트 인덱스, 단색 머티리얼)
//==============================================================================
fn buildGroundGlb(halfSize: f32, baseColor: [f32; 3]) -> Vec<u8> {
	let positionValues = [-halfSize, 0.0, -halfSize, -halfSize, 0.0, halfSize, halfSize, 0.0, halfSize, halfSize, 0.0, -halfSize];
	let normalValues = [0.0f32, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0];
	let indexValues = [0u16, 1, 2, 0, 2, 3];
	let mut binaryBytes: Vec<u8> = Vec::new();
	binaryBytes.extend(positionValues.iter().flat_map(|value| value.to_le_bytes()));
	binaryBytes.extend(normalValues.iter().flat_map(|value| value.to_le_bytes()));
	binaryBytes.extend(indexValues.iter().flat_map(|value| value.to_le_bytes()));
	while !binaryBytes.len().is_multiple_of(4) {
		binaryBytes.push(0);
	}
	let jsonText = format!(
		r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"name":"Ground","mesh":0}}],
		"materials":[{{"name":"ground","pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},1],"metallicFactor":0,"roughnessFactor":1}}}}],
		"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"indices":2,"material":0}}]}}],
		"accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"}},{{"bufferView":2,"componentType":5123,"count":6,"type":"SCALAR"}}],
		"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":48}},{{"buffer":0,"byteOffset":96,"byteLength":12}}],
		"buffers":[{{"byteLength":{}}}]}}"#,
		baseColor[0], baseColor[1], baseColor[2], binaryBytes.len()
	);
	let mut jsonBytes = jsonText.into_bytes();
	while !jsonBytes.len().is_multiple_of(4) {
		jsonBytes.push(b' ');
	}
	let totalLength = 12 + 8 + jsonBytes.len() + 8 + binaryBytes.len();
	let mut glbBytes = Vec::with_capacity(totalLength);
	glbBytes.extend_from_slice(&0x46546c67u32.to_le_bytes());
	glbBytes.extend_from_slice(&2u32.to_le_bytes());
	glbBytes.extend_from_slice(&(totalLength as u32).to_le_bytes());
	glbBytes.extend_from_slice(&(jsonBytes.len() as u32).to_le_bytes());
	glbBytes.extend_from_slice(&0x4e4f534au32.to_le_bytes());
	glbBytes.extend_from_slice(&jsonBytes);
	glbBytes.extend_from_slice(&(binaryBytes.len() as u32).to_le_bytes());
	glbBytes.extend_from_slice(&0x004e4942u32.to_le_bytes());
	glbBytes.extend_from_slice(&binaryBytes);
	glbBytes
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engineConfiguration = EngineConfiguration::new();
	engineConfiguration.title = String::from("바닐라엔진 graphics3d modelviewer");
	engineConfiguration.windowSize = vanilla::Vector2::create(1280.0, 720.0);
	engineConfiguration.assetDirectoryPaths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engineConfiguration);
	if let Err(error) = engine.run(Box::new(ModelViewerScene::new())) {
		eprintln!("{}", error);
	}
}

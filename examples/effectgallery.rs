//==============================================================================
// 효과 갤러리 예제. (ShaderSprite, ScreenEffect, TrailRenderer 눈으로 확인)
// - 왼쪽: ShaderSpriteEffect 50 가지를 칸마다 하나씩 건다. (none 은 원본)
//   그림은 코드로 만든 두 칸짜리 시트이고 모든 칸이 왼쪽 칸(imageRect)만 쓴다. 오른쪽 칸은 (경계 두 줄을 빼고) 새빨간 사각형이라 textureRect 자르기가 틀리면 빨강이 번져 보인다.
// - 오른쪽: 둥근 모서리 무대에만 ScreenEffect 를 건다. (setRegion) 30 프레임마다 효과 묶음이 바뀌고, 120 프레임 묶음은 화면 전체(clearRegion)에 건다.
//   3 프레임째에는 모든 효과를 한꺼번에 켜서 패스 셰이더가 다 컴파일되는지 본다. (실패하면 stderr 에 ScreenEffect: pass ... failed)
// - 아래: TrailRenderer 둘. (보통 합성, 더하기 합성) 무대 안에도 더하기 합성 트레일이 하나 돈다.
// - 실행: cargo run --release --example effectgallery
//   화면 저장: VANILLA_CAPTURE_PATH=shots/effect_{frame}.png VANILLA_CAPTURE_FRAMES=60,120 VANILLA_CAPTURE_EXIT=1
//==============================================================================
#![allow(non_snake_case)]

use std::rc::Rc;
use vanilla::core::graphic::TextAlign;
use vanilla::{math, Color, Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, Graphic, Image, NodeId, Paint, Pivot, Rect, Scene, SceneBase, ScreenEffect, ScreenEffectType, ShaderSprite, ShaderSpriteEffect, Sprite, Text, TrailRenderer, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const BACKGROUND_COLOR: &str = "#1b1f27";
const CELL_COLOR: &str = "#3a4150";
const LABEL_COLOR: &str = "#c9d1e0";
const GRID_COLUMN_COUNT: usize = 10;
const GRID_LEFT: f32 = 15.0;
const GRID_TOP: f32 = 44.0;
const CELL_WIDTH: f32 = 84.0;
const CELL_HEIGHT: f32 = 101.0;
const SPRITE_SIZE: f32 = 64.0;
const SHEET_FRAME_SIZE: u32 = 80;
const SHEET_GUTTER_SIZE: u32 = 2;
const STAGE_X: f32 = 870.0;
const STAGE_Y: f32 = 44.0;
const STAGE_WIDTH: f32 = 395.0;
const STAGE_HEIGHT: f32 = 505.0;
const STAGE_CORNER_RADIUS: f32 = 16.0;
const EFFECT_SET_FRAME_COUNT: u64 = 30;
const EFFECT_SET_COUNT: usize = 8;
const COMPILE_CHECK_FRAME: u64 = 3;
// 움직이는 노드가 프레임마다 가는 시간. (창이 뜨기 전 첫 프레임들은 수직 동기 없이 빨리 돌아서, 실제 시간으로 움직이면 저장한 프레임 모양이 실행마다 다르다)
const MOVER_FRAME_STEP: f32 = 1.0 / 60.0;

//==============================================================================
// 원을 따라 도는 노드. (무대 안 트레일)
//==============================================================================
struct OrbitMover {
	componentBase: ComponentBase,
	center: Vector2,
	radius: f32,
	angularSpeed: f32,
	angle: f32,
}

impl Component for OrbitMover {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"OrbitMover"
	}

	fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
		self.angle += self.angularSpeed * MOVER_FRAME_STEP;
		let radiusScale = 1.0 + 0.25 * (self.angle * 2.0).sin();
		let positionX = self.center.x + self.angle.cos() * self.radius * radiusScale;
		let positionY = self.center.y + self.angle.sin() * self.radius * 0.8;
		context.getTransformMut().setLocalPosition(Vector2::create(positionX, positionY));
	}
}

//==============================================================================
// 물결치며 좌우로 오가는 노드. (아래 트레일)
//==============================================================================
struct WaveMover {
	componentBase: ComponentBase,
	elapsedTime: f32,
	startX: f32,
	endX: f32,
	horizontalFrequency: f32,
	baseY: f32,
	amplitude: f32,
	frequency: f32,
}

impl Component for WaveMover {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"WaveMover"
	}

	fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
		self.elapsedTime += MOVER_FRAME_STEP;
		let travelRatio = 0.5 - 0.5 * (self.elapsedTime * self.horizontalFrequency).cos();
		let positionX = self.startX + (self.endX - self.startX) * travelRatio;
		let positionY = self.baseY + (self.elapsedTime * self.frequency).sin() * self.amplitude;
		context.getTransformMut().setLocalPosition(Vector2::create(positionX, positionY));
	}
}

//==============================================================================
// 두 칸짜리 그림 시트 만들기. (왼쪽: 안테나 달린 둥근 캐릭터, 오른쪽: 새빨간 사각형)
//==============================================================================
fn createCharacterSheet() -> Rc<Image> {
	let frameSize = SHEET_FRAME_SIZE;
	let sheetWidth = frameSize * 2;
	let mut pixels = vec![0u8; (sheetWidth * frameSize * 4) as usize];
	let center = frameSize as f32 * 0.5;
	for pixelY in 0..frameSize {
		for pixelX in 0..sheetWidth {
			let pixelIndex = ((pixelY * sheetWidth + pixelX) * 4) as usize;
			if pixelX >= frameSize {
				// 맞닿은 두 줄은 비워 둔다. (쌍선형 필터가 칸 경계에서 옆 칸을 섞는 것은 효과와 상관없는 아틀라스 번짐이라 빼고 본다)
				if pixelX >= frameSize + SHEET_GUTTER_SIZE {
					pixels[pixelIndex] = 255;
					pixels[pixelIndex + 3] = 255;
				}
				continue;
			}
			let sampleX = pixelX as f32 + 0.5;
			let sampleY = pixelY as f32 + 0.5;

			// 몸통. (가장자리 부드럽게, 위왼쪽 하늘색 → 아래오른쪽 보라)
			let bodyDistance = ((sampleX - center).powi(2) + (sampleY - center - 4.0).powi(2)).sqrt();
			let bodyCoverage = math::clamp01(28.0 - bodyDistance + 0.5);
			let gradient = (sampleX + sampleY) / (frameSize as f32 * 2.0);
			let mut color = Color::lerp(Color::create(0.35, 0.88, 1.0, 1.0), Color::create(0.78, 0.32, 0.95, 1.0), gradient);
			if bodyDistance > 23.0 {
				color = Color::create(color.red * 0.6, color.green * 0.6, color.blue * 0.6, 1.0);
			}

			// 하이라이트, 눈.
			let highlightDistance = ((sampleX - 30.0).powi(2) + (sampleY - 30.0).powi(2)).sqrt();
			if highlightDistance < 6.0 {
				color = Color::lerp(color, Color::white(), 0.8);
			}
			let leftEyeDistance = ((sampleX - 32.0).powi(2) + (sampleY - 46.0).powi(2)).sqrt();
			let rightEyeDistance = ((sampleX - 48.0).powi(2) + (sampleY - 46.0).powi(2)).sqrt();
			if leftEyeDistance < 4.0 || rightEyeDistance < 4.0 {
				color = Color::create(0.05, 0.05, 0.1, 1.0);
			}

			// 안테나. (줄기 + 노란 공)
			let stemCoverage = if (sampleX - center).abs() < 1.5 && sampleY > 8.0 && sampleY < 18.0 {
				1.0
			}
			else {
				0.0
			};
			let ballDistance = ((sampleX - center).powi(2) + (sampleY - 9.0).powi(2)).sqrt();
			let ballCoverage = math::clamp01(5.0 - ballDistance + 0.5);
			let mut coverage = bodyCoverage;
			if stemCoverage > 0.0 && bodyCoverage < 1.0 {
				color = Color::create(0.9, 0.9, 0.95, 1.0);
				coverage = 1.0;
			}
			if ballCoverage > 0.0 && bodyCoverage <= 0.0 {
				color = Color::create(1.0, 0.85, 0.2, 1.0);
				coverage = coverage.max(ballCoverage);
			}
			if coverage <= 0.0 {
				continue;
			}
			pixels[pixelIndex] = (math::clamp01(color.red) * 255.0) as u8;
			pixels[pixelIndex + 1] = (math::clamp01(color.green) * 255.0) as u8;
			pixels[pixelIndex + 2] = (math::clamp01(color.blue) * 255.0) as u8;
			pixels[pixelIndex + 3] = (coverage * 255.0) as u8;
		}
	}
	Image::createFromPixels(sheetWidth, frameSize, pixels)
}

//==============================================================================
// 칸 효과의 진행도. (사라짐 / 드러남은 절반, 광택은 띠가 가운데, 섬광은 조금 덜)
//==============================================================================
fn getGalleryEffectProgress(effect: ShaderSpriteEffect) -> f32 {
	match effect {
		ShaderSpriteEffect::dissolve | ShaderSpriteEffect::burn | ShaderSpriteEffect::noiseFade | ShaderSpriteEffect::pixelDissolve | ShaderSpriteEffect::wipe | ShaderSpriteEffect::iris | ShaderSpriteEffect::diamondWipe | ShaderSpriteEffect::clockWipe | ShaderSpriteEffect::blinds | ShaderSpriteEffect::checkerWipe => {
			0.5
		}
		ShaderSpriteEffect::shine => {
			0.45
		}
		ShaderSpriteEffect::flash => {
			0.7
		}
		_ => {
			1.0
		}
	}
}

//==============================================================================
// 효과 갤러리 씬.
//==============================================================================
struct EffectGalleryScene {
	sceneBase: SceneBase,
	screenEffect: ScreenEffect,
	appliedEffectSetIndex: Option<usize>,
	isFullScreenEffect: bool,
	effectSetTextId: Option<NodeId>,
}

impl EffectGalleryScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> EffectGalleryScene {
		EffectGalleryScene {
			sceneBase: SceneBase::new(),
			screenEffect: ScreenEffect::new(),
			appliedEffectSetIndex: None,
			isFullScreenEffect: false,
			effectSetTextId: None,
		}
	}

	//==============================================================================
	// 글 노드 하나 붙이기.
	//==============================================================================
	fn addLabel(&mut self, parentId: NodeId, position: Vector2, size: Vector2, text: &str, fontSize: f32, textAlign: TextAlign) -> NodeId {
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let labelId = nodeTree.createChild(parentId, "label");
		let labelNode = &mut nodeTree[labelId];
		if textAlign == TextAlign::left {
			labelNode.setPivot(Pivot::middleLeft);
		}
		labelNode.setLocalPosition(position);
		labelNode.setContentSize(size);
		let label = labelNode.addComponent(Text::createWithText(text, fontSize));
		label.setTextColor(LABEL_COLOR);
		label.setTextAlign(textAlign);
		labelId
	}

	//==============================================================================
	// ShaderSprite 칸 50 개.
	//==============================================================================
	fn createShaderSpriteGrid(&mut self, rootId: NodeId, sheetImage: &Rc<Image>) {
		let frameRect = Rect::create(0.0, 0.0, SHEET_FRAME_SIZE as f32, SHEET_FRAME_SIZE as f32);
		for (effectIndex, effect) in ShaderSpriteEffect::getEffectList().iter().enumerate() {
			let columnIndex = effectIndex % GRID_COLUMN_COUNT;
			let rowIndex = effectIndex / GRID_COLUMN_COUNT;
			let cellLeft = GRID_LEFT + columnIndex as f32 * CELL_WIDTH;
			let cellTop = GRID_TOP + rowIndex as f32 * CELL_HEIGHT;
			let cellCenterX = cellLeft + CELL_WIDTH * 0.5;

			let nodeTree = self.sceneBase.getNodeTreeMut();
			let cellId = nodeTree.createChild(rootId, "cell");
			let cellNode = &mut nodeTree[cellId];
			cellNode.setLocalPosition(Vector2::create(cellCenterX, cellTop + CELL_HEIGHT * 0.5));
			cellNode.setContentSize(Vector2::create(CELL_WIDTH - 6.0, CELL_HEIGHT - 6.0));
			let cellPaint = cellNode.addComponent(Paint::createWithColor(CELL_COLOR));
			cellPaint.setRoundSize(8.0);

			let spriteId = nodeTree.createChild(rootId, effect.getName());
			let spriteNode = &mut nodeTree[spriteId];
			spriteNode.setLocalPosition(Vector2::create(cellCenterX, cellTop + 42.0));
			spriteNode.setContentSize(Vector2::create(SPRITE_SIZE, SPRITE_SIZE));
			let shaderSprite = spriteNode.addComponent(ShaderSprite::createWithImage(sheetImage.clone()));
			shaderSprite.setImageRect(frameRect);
			shaderSprite.setEffect(*effect);
			shaderSprite.setEffectProgress(getGalleryEffectProgress(*effect));
			shaderSprite.setEffectSeed(effectIndex as f32 * 7.31);

			self.addLabel(rootId, Vector2::create(cellCenterX, cellTop + 86.0), Vector2::create(CELL_WIDTH, 14.0), effect.getName(), 11.0, TextAlign::center);
		}
	}

	//==============================================================================
	// ScreenEffect 무대. (둥근 판, 해, 땅, 캐릭터 셋, 도는 트레일)
	//==============================================================================
	fn createStage(&mut self, rootId: NodeId, sheetImage: &Rc<Image>) {
		let frameRect = Rect::create(0.0, 0.0, SHEET_FRAME_SIZE as f32, SHEET_FRAME_SIZE as f32);
		let nodeTree = self.sceneBase.getNodeTreeMut();
		let stageId = nodeTree.createChild(rootId, "stage");
		let stageNode = &mut nodeTree[stageId];
		stageNode.setPivot(Pivot::topLeft);
		stageNode.setLocalPosition(Vector2::create(STAGE_X, STAGE_Y));
		stageNode.setContentSize(Vector2::create(STAGE_WIDTH, STAGE_HEIGHT));
		let stagePaint = stageNode.addComponent(Paint::createWithColor("#16283c"));
		stagePaint.setRoundSize(STAGE_CORNER_RADIUS);

		let sunId = nodeTree.createChild(stageId, "sun");
		let sunNode = &mut nodeTree[sunId];
		sunNode.setLocalPosition(Vector2::create(290.0, 110.0));
		sunNode.setContentSize(Vector2::create(90.0, 90.0));
		let sunPaint = sunNode.addComponent(Paint::createWithColor("#fff2b0"));
		sunPaint.setRoundSize(45.0);

		let groundId = nodeTree.createChild(stageId, "ground");
		let groundNode = &mut nodeTree[groundId];
		groundNode.setLocalPosition(Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT - 45.0));
		groundNode.setContentSize(Vector2::create(STAGE_WIDTH - 20.0, 70.0));
		let groundPaint = groundNode.addComponent(Paint::createWithColor("#2f5d3a"));
		groundPaint.setRoundSize(10.0);

		let characterList: [(f32, f32, f32, ShaderSpriteEffect); 3] = [(90.0, 400.0, 96.0, ShaderSpriteEffect::none), (200.0, 330.0, 72.0, ShaderSpriteEffect::glow), (310.0, 400.0, 96.0, ShaderSpriteEffect::hologram)];
		for (positionX, positionY, size, effect) in characterList {
			let characterId = nodeTree.createChild(stageId, "character");
			let characterNode = &mut nodeTree[characterId];
			characterNode.setLocalPosition(Vector2::create(positionX, positionY));
			characterNode.setContentSize(Vector2::create(size, size));
			if effect == ShaderSpriteEffect::none {
				let sprite = characterNode.addComponent(Sprite::createWithImage(sheetImage.clone()));
				sprite.setImageRect(frameRect);
			}
			else {
				let shaderSprite = characterNode.addComponent(ShaderSprite::createWithImage(sheetImage.clone()));
				shaderSprite.setImageRect(frameRect);
				shaderSprite.setEffect(effect);
				shaderSprite.setEffectProgress(1.0);
			}
		}

		// 도는 트레일. (머리 점은 자식 노드라 트레일 원점과 겹친다. 움직이는 컴포넌트를 먼저 붙여 트레일이 옮긴 뒤의 위치를 기록하게 한다)
		let orbitCenter = Vector2::create(STAGE_WIDTH * 0.5, 220.0);
		let orbitRadius = 130.0;
		let orbitId = nodeTree.createChild(stageId, "orbit");
		let orbitNode = &mut nodeTree[orbitId];
		orbitNode.setLocalPosition(Vector2::create(orbitCenter.x + orbitRadius, orbitCenter.y));
		orbitNode.addComponent(OrbitMover {
			componentBase: ComponentBase::new(),
			center: orbitCenter,
			radius: orbitRadius,
			angularSpeed: 3.2,
			angle: 0.0,
		});
		let trail = orbitNode.addComponent(TrailRenderer::new());
		trail.setTime(0.6);
		trail.setWidth(18.0, 0.0);
		trail.setAdditive(true);
		trail.setColors(Color::create(0.4, 0.85, 1.0, 1.0), Color::create(0.2, 0.3, 1.0, 0.0));
		let headId = nodeTree.createChild(orbitId, "head");
		let headNode = &mut nodeTree[headId];
		headNode.setContentSize(Vector2::create(14.0, 14.0));
		let headPaint = headNode.addComponent(Paint::createWithColor(Color::white()));
		headPaint.setRoundSize(7.0);

		self.addLabel(stageId, Vector2::create(STAGE_WIDTH * 0.5, 24.0), Vector2::create(STAGE_WIDTH, 20.0), "ScreenEffect 무대", 16.0, TextAlign::center);
	}

	//==============================================================================
	// 아래 트레일 둘과 설명 글.
	//==============================================================================
	fn createTrailShowcase(&mut self, rootId: NodeId) {
		let trailSettingList: [(f32, f32, f32, f32, bool, Color, Color); 2] = [
			(610.0, 32.0, 5.0, 16.0, false, Color::create(0.4, 0.8, 1.0, 0.9), Color::create(0.4, 0.8, 1.0, 0.0)),
			(670.0, 28.0, 4.0, 26.0, true, Color::create(1.0, 0.92, 0.6, 1.0), Color::create(1.0, 0.55, 0.2, 0.0)),
		];
		for (baseY, amplitude, frequency, width, isAdditive, startColor, endColor) in trailSettingList {
			let nodeTree = self.sceneBase.getNodeTreeMut();
			let moverId = nodeTree.createChild(rootId, "trailMover");
			let moverNode = &mut nodeTree[moverId];
			moverNode.setLocalPosition(Vector2::create(GRID_LEFT + 40.0, baseY));
			moverNode.addComponent(WaveMover {
				componentBase: ComponentBase::new(),
				elapsedTime: 0.0,
				startX: GRID_LEFT + 40.0,
				endX: GRID_LEFT + CELL_WIDTH * GRID_COLUMN_COUNT as f32 - 20.0,
				horizontalFrequency: 1.3,
				baseY,
				amplitude,
				frequency,
			});
			let trail = moverNode.addComponent(TrailRenderer::new());
			trail.setTime(0.7);
			trail.setWidth(width, 0.0);
			trail.setAdditive(isAdditive);
			trail.setColors(startColor, endColor);
		}
		self.addLabel(rootId, Vector2::create(GRID_LEFT + 4.0, 575.0), Vector2::create(400.0, 16.0), "TrailRenderer: 위 보통 합성, 아래 더하기 합성", 13.0, TextAlign::left);
		let effectSetTextId = self.addLabel(rootId, Vector2::create(STAGE_X + 4.0, 575.0), Vector2::create(STAGE_WIDTH, 16.0), "", 13.0, TextAlign::left);
		self.effectSetTextId = Some(effectSetTextId);
	}

	//==============================================================================
	// 효과 묶음 적용. (30 프레임마다 바뀐다. 4 번은 화면 전체)
	//==============================================================================
	fn applyEffectSet(&mut self, effectSetIndex: usize) {
		let screenEffect = &mut self.screenEffect;
		screenEffect.resetAll();
		self.isFullScreenEffect = false;
		let description = match effectSetIndex {
			0 => {
				screenEffect.setEnabled(ScreenEffectType::bloom, true);
				screenEffect.setStrength(ScreenEffectType::bloom, 0.9);
				screenEffect.setEnabled(ScreenEffectType::vignette, true);
				"bloom + vignette"
			}
			1 => {
				screenEffect.setEnabled(ScreenEffectType::glitch, true);
				screenEffect.setEnabled(ScreenEffectType::chromatic, true);
				screenEffect.setParameters(ScreenEffectType::chromatic, 2.0, 0.0, 0.0, 0.0);
				"glitch + chromatic"
			}
			2 => {
				screenEffect.setEnabled(ScreenEffectType::bloom, true);
				screenEffect.setEnabled(ScreenEffectType::chromatic, true);
				screenEffect.setStrength(ScreenEffectType::chromatic, 0.8);
				screenEffect.setEnabled(ScreenEffectType::vignette, true);
				screenEffect.setEnabled(ScreenEffectType::colorGrade, true);
				screenEffect.setParameters(ScreenEffectType::colorGrade, 1.15, 1.2, 1.05, 0.0);
				"bloom + chromatic + vignette + colorGrade"
			}
			3 => {
				screenEffect.setEnabled(ScreenEffectType::pixelate, true);
				screenEffect.setStrength(ScreenEffectType::pixelate, 0.7);
				screenEffect.setEnabled(ScreenEffectType::posterize, true);
				"pixelate + posterize"
			}
			4 => {
				self.isFullScreenEffect = true;
				screenEffect.setEnabled(ScreenEffectType::shockwave, true);
				screenEffect.setEnabled(ScreenEffectType::scanline, true);
				screenEffect.setStrength(ScreenEffectType::scanline, 0.6);
				screenEffect.setEnabled(ScreenEffectType::vignette, true);
				screenEffect.setStrength(ScreenEffectType::vignette, 0.8);
				"화면 전체: shockwave + scanline + vignette"
			}
			5 => {
				screenEffect.setEnabled(ScreenEffectType::blur, true);
				screenEffect.setEnabled(ScreenEffectType::colorGrade, true);
				screenEffect.setParameters(ScreenEffectType::colorGrade, 1.2, 0.3, 1.05, 0.0);
				"blur + colorGrade"
			}
			6 => {
				screenEffect.setEnabled(ScreenEffectType::godRays, true);
				screenEffect.setParameters(ScreenEffectType::godRays, 0.73, 0.22, 0.5, 0.94);
				screenEffect.setEnabled(ScreenEffectType::rain, true);
				"godRays + rain"
			}
			_ => {
				screenEffect.setEnabled(ScreenEffectType::irisWipe, true);
				screenEffect.setStrength(ScreenEffectType::irisWipe, 0.5);
				screenEffect.setEnabled(ScreenEffectType::halftone, true);
				screenEffect.setStrength(ScreenEffectType::halftone, 0.6);
				"irisWipe + halftone"
			}
		};
		let effectSetText = format!("ScreenEffect: {}", description);
		if let Some(effectSetTextId) = self.effectSetTextId {
			if let Some(label) = self.sceneBase.getNodeTreeMut()[effectSetTextId].getComponentMut::<Text>() {
				label.setText(&effectSetText);
			}
		}
	}

	//==============================================================================
	// 모든 효과 켜기. (한 프레임 동안 패스 셰이더를 다 컴파일해 본다)
	//==============================================================================
	fn enableAllEffects(&mut self) {
		for effectType in ScreenEffect::getEffectOrder() {
			self.screenEffect.setEnabled(effectType, true);
			self.screenEffect.setStrength(effectType, 0.5);
		}
		self.isFullScreenEffect = false;
		self.appliedEffectSetIndex = None;
	}

	//==============================================================================
	// 무대 영역을 캔버스 픽셀로 바꿔 ScreenEffect 영역으로. (원본 예제 applyStageRegion)
	//==============================================================================
	fn applyStageRegion(&mut self, engine: &Engine) {
		if self.isFullScreenEffect {
			self.screenEffect.clearRegion();
			return;
		}
		let viewManager = engine.getViewManager();
		let viewNativeRect = viewManager.getViewNativeRect();
		let targetResolutionScale = viewManager.getTargetResolutionScale();
		let devicePixelRatio = viewManager.getDevicePixelRatio();
		let pixelScale = targetResolutionScale * devicePixelRatio;
		self.screenEffect.setRegion(
			viewNativeRect.position.x * devicePixelRatio + STAGE_X * pixelScale,
			viewNativeRect.position.y * devicePixelRatio + STAGE_Y * pixelScale,
			STAGE_WIDTH * pixelScale,
			STAGE_HEIGHT * pixelScale,
			STAGE_CORNER_RADIUS * pixelScale,
		);
	}
}

impl Scene for EffectGalleryScene {
	fn getSceneBase(&self) -> &SceneBase {
		&self.sceneBase
	}

	fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		&mut self.sceneBase
	}

	//==============================================================================
	// 초기화. (그림은 코드로 만든다)
	//==============================================================================
	fn initialize(&mut self, _engine: &mut Engine) {
		self.sceneBase.setSceneBackgroundColor(BACKGROUND_COLOR);
		let rootId = self.sceneBase.getRootId();
		self.sceneBase.getRootMut().setContentSize(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let sheetImage = createCharacterSheet();
		self.createShaderSpriteGrid(rootId, &sheetImage);
		self.createStage(rootId, &sheetImage);
		self.createTrailShowcase(rootId);
		self.addLabel(rootId, Vector2::create(GRID_LEFT + 430.0, 26.0), Vector2::create(400.0, 18.0), "ShaderSpriteEffect 50 가지", 15.0, TextAlign::left);
	}

	//==============================================================================
	// 갱신. (효과 묶음 바꾸기, 충격파 반지름 움직이기)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, timeDelta: f32) {
		let frameNumber = engine.getFrameNumber();
		if frameNumber == COMPILE_CHECK_FRAME {
			self.enableAllEffects();
		}
		else {
			let effectSetIndex = ((frameNumber / EFFECT_SET_FRAME_COUNT) as usize) % EFFECT_SET_COUNT;
			if self.appliedEffectSetIndex != Some(effectSetIndex) {
				self.applyEffectSet(effectSetIndex);
				self.appliedEffectSetIndex = Some(effectSetIndex);
			}
		}
		let shockwaveRadius = (frameNumber % EFFECT_SET_FRAME_COUNT) as f32 / EFFECT_SET_FRAME_COUNT as f32 * 1.2 + 0.1;
		let stageCenterX = (STAGE_X + STAGE_WIDTH * 0.5) / VIEW_WIDTH;
		let stageCenterY = (STAGE_Y + STAGE_HEIGHT * 0.45) / VIEW_HEIGHT;
		self.screenEffect.setParameters(ScreenEffectType::shockwave, stageCenterX, stageCenterY, shockwaveRadius, 0.12);
		self.screenEffect.tick(timeDelta);
		self.sceneBase.tick(engine, timeDelta);
	}

	//==============================================================================
	// 이전 출력. (영역을 정하고 효과 시작, 배경)
	//==============================================================================
	fn preDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.applyStageRegion(engine);
		self.screenEffect.begin(graphic);
		self.sceneBase.drawSceneBackground(engine, graphic);
	}

	//==============================================================================
	// 이후 출력. (화면을 떠 와 효과를 건다)
	//==============================================================================
	fn postDraw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.screenEffect.end(graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engineConfiguration = EngineConfiguration::new();
	engineConfiguration.title = String::from("효과 갤러리 (바닐라엔진)");
	engineConfiguration.referenceResolutionSize = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engineConfiguration.viewScaleMode = ViewScaleMode::stretchShort;
	engineConfiguration.useStatistics = true;

	let engine = Engine::new(engineConfiguration);
	if let Err(error) = engine.run(Box::new(EffectGalleryScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}

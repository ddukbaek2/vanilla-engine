//==============================================================================
// misc 도구 둘러보기 예제. (데스크탑 창, 화면 확인용)
// - ImageScroller(흐르는 배경), Camera2D + Shaker(월드 격자와 PathFinder 경로), NodeLayout(패널), Typewriter + Localization(대사),
//   FloatingText(떠오르는 숫자), TouchEffect(빛 입자), SpriteAnimator(시트 애니메이션), VirtualPad(조이스틱)를 한 화면에 그린다.
// - 실행: cargo run --example misctour
//   검증용으로 스스로 끝내려면 VANILLA_CAPTURE_PATH=<폴더>/shot_{frame}.png VANILLA_CAPTURE_FRAMES=90 VANILLA_CAPTURE_EXIT=1 을 준다.
//==============================================================================
#![allow(non_snake_case)]

use std::rc::Rc;
use vanilla::{
	math, Camera2D, Color, Engine, EngineConfiguration, FloatingText, FloatingTextOptions, Graphic, Grid, GridCell, Image, ImageScroller, Localization, NodeId, NodeLayout, Paint, PathFinder, Pivot, Rect, Scene, SceneBase, Shaker,
	ShakerOptions, Sprite, SpriteAnimator, SpriteAnimatorPlayOptions, Text, TouchEffect, Typewriter, TypewriterOptions, Vector2, ViewScaleMode, VirtualPad,
};

// 화면 크기.
const VIEW_WIDTH: f32 = 960.0;
const VIEW_HEIGHT: f32 = 640.0;
// 월드 격자.
const GRID_COLUMN_COUNT: i32 = 12;
const GRID_ROW_COUNT: i32 = 8;
const TILE_SIZE: f32 = 48.0;
// 격자의 벽. ('#')
const GRID_MAP: [&str; 8] = [
	"............",
	"..####......",
	".....#..###.",
	".....#....#.",
	"..#..#....#.",
	"..#.......#.",
	"..#######.#.",
	"............",
];

//==============================================================================
// 격자 칸이 막혔는지.
//==============================================================================
fn isWall(column: i32, row: i32) -> bool {
	GRID_MAP[row as usize].as_bytes()[column as usize] == b'#'
}

//==============================================================================
// 줄무늬 배경 그림을 만든다. (ImageScroller 용, 64x64)
//==============================================================================
fn createStripeImage() -> Rc<Image> {
	let imageSize = 64usize;
	let mut pixels = vec![0u8; imageSize * imageSize * 4];
	for pixelY in 0..imageSize {
		for pixelX in 0..imageSize {
			let isStripe = ((pixelX + pixelY) / 16) % 2 == 0;
			let brightness = if isStripe {
				48
			}
			else {
				36
			};
			let pixelIndex = (pixelY * imageSize + pixelX) * 4;
			pixels[pixelIndex] = brightness;
			pixels[pixelIndex + 1] = brightness;
			pixels[pixelIndex + 2] = brightness + 16;
			pixels[pixelIndex + 3] = 255;
		}
	}
	Image::createFromPixels(imageSize as u32, imageSize as u32, pixels)
}

//==============================================================================
// 색 칸 네 개짜리 스프라이트 시트를 만든다. (SpriteAnimator 용, 128x32)
//==============================================================================
fn createSheetImage() -> Rc<Image> {
	let frameColors: [[u8; 3]; 4] = [[255, 120, 120], [255, 220, 120], [120, 230, 140], [120, 170, 255]];
	let imageWidth = 128usize;
	let imageHeight = 32usize;
	let mut pixels = vec![0u8; imageWidth * imageHeight * 4];
	for pixelY in 0..imageHeight {
		for pixelX in 0..imageWidth {
			let frameColor = frameColors[pixelX / 32];
			let localX = (pixelX % 32) as f32 - 15.5;
			let localY = pixelY as f32 - 15.5;
			let isInside = localX * localX + localY * localY <= 14.0 * 14.0;
			let pixelIndex = (pixelY * imageWidth + pixelX) * 4;
			if isInside {
				pixels[pixelIndex] = frameColor[0];
				pixels[pixelIndex + 1] = frameColor[1];
				pixels[pixelIndex + 2] = frameColor[2];
				pixels[pixelIndex + 3] = 255;
			}
		}
	}
	Image::createFromPixels(imageWidth as u32, imageHeight as u32, pixels)
}

//==============================================================================
// 둘러보기 씬.
//==============================================================================
struct MiscTourScene {
	sceneBase: SceneBase,
	camera: Camera2D,
	shaker: Shaker,
	typewriter: Typewriter,
	virtualPad: VirtualPad,
	imageScroller: ImageScroller,
	path: Vec<GridCell>,
	labelId: Option<NodeId>,
	floatingTextId: Option<NodeId>,
	touchEffectId: Option<NodeId>,
	elapsedSeconds: f32,
	spawnSeconds: f32,
}

impl MiscTourScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> MiscTourScene {
		MiscTourScene {
			sceneBase: SceneBase::new(),
			camera: Camera2D::new(),
			shaker: Shaker::new(ShakerOptions::default()),
			typewriter: Typewriter::new(TypewriterOptions {
				charactersPerSecond: 16.0,
				..TypewriterOptions::default()
			}),
			virtualPad: VirtualPad::new(),
			imageScroller: ImageScroller::new(),
			path: Vec::new(),
			labelId: None,
			floatingTextId: None,
			touchEffectId: None,
			elapsedSeconds: 0.0,
			spawnSeconds: 0.0,
		}
	}

	//==============================================================================
	// 월드 격자와 경로 그리기. (카메라 좌표계)
	//==============================================================================
	fn drawWorld(&self, graphic: &mut Graphic) {
		for row in 0..GRID_ROW_COUNT {
			for column in 0..GRID_COLUMN_COUNT {
				let tileRect = Rect::create(column as f32 * TILE_SIZE, row as f32 * TILE_SIZE, TILE_SIZE - 2.0, TILE_SIZE - 2.0);
				let tileColor = if isWall(column, row) {
					Color::create(0.45, 0.32, 0.25, 1.0)
				}
				else {
					Color::create(0.18, 0.22, 0.3, 1.0)
				};
				graphic.setFillColor(tileColor);
				graphic.drawRect(tileRect);
			}
		}
		let mut pathPositions: Vec<Vector2> = vec![Grid::toWorldCenter(0, 7, TILE_SIZE)];
		for cell in self.path.iter() {
			let cellCenter = Grid::toWorldCenter(cell.column, cell.row, TILE_SIZE);
			pathPositions.push(cellCenter);
		}
		graphic.setStrokeColor(Color::create(1.0, 0.85, 0.3, 1.0));
		graphic.drawLine(&pathPositions, 6.0);
	}
}

impl Scene for MiscTourScene {
	//==============================================================================
	// 씬 공통 상태 반환.
	//==============================================================================
	fn getSceneBase(&self) -> &SceneBase {
		&self.sceneBase
	}

	//==============================================================================
	// 씬 공통 상태 반환. (변경용)
	//==============================================================================
	fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		&mut self.sceneBase
	}

	//==============================================================================
	// 생성. (노드와 도구들)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
		self.sceneBase.setSceneBackgroundColor(Color::create(0.08, 0.08, 0.12, 1.0));
		self.sceneBase.getRootMut().setContentSize(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));

		// 흐르는 배경.
		self.imageScroller.setImage(Some(createStripeImage()));
		self.imageScroller.setViewRect(Rect::create(0.0, 0.0, VIEW_WIDTH, VIEW_HEIGHT));
		self.imageScroller.setHorizontalScrollSpeed(-40.0);
		self.imageScroller.setVerticalScrollSpeed(20.0);

		// 카메라와 길찾기.
		self.camera.setViewSize(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		self.camera.setWorldBounds(Some(Rect::create(0.0, 0.0, GRID_COLUMN_COUNT as f32 * TILE_SIZE, GRID_ROW_COUNT as f32 * TILE_SIZE)));
		self.camera.setZoom(1.2);
		self.path = PathFinder::findPath(GRID_COLUMN_COUNT, GRID_ROW_COUNT, GridCell::create(0, 7), GridCell::create(11, 3), isWall).unwrap_or_default();
		self.shaker.addShake(6.0);

		// 대사 패널. (번역 표의 대사를 타자기로)
		Localization::setTable("en", [("{0}번째 misc 도구 둘러보기", "Misc tour number {0}: camera, path, dialogue")]);
		Localization::setLanguage("en");
		let rootId = self.sceneBase.getRootId();
		let nodeTree = self.sceneBase.getNodeTreeMut();
		NodeLayout::create()
			.name("panel")
			.pivot(Pivot::bottomCenter)
			.contentSize(620.0, 110.0)
			.localPosition(VIEW_WIDTH * 0.5, VIEW_HEIGHT - 24.0)
			.componentWith(Paint::new(), |paint| {
				paint.setColor(Color::create(0.0, 0.0, 0.0, 0.7));
				paint.setRoundSize(16.0);
			})
			.children([NodeLayout::create().name("label").pivot(Pivot::topLeft).contentSize(580.0, 90.0).localPosition(20.0, 10.0).componentWith(Text::new(), |text| {
				text.setFontSize(24.0);
				text.setWordWrapWidth(580.0);
			})])
			.build(nodeTree, Some(rootId));
		let labelId = nodeTree.findDescendantByName(rootId, "label");
		self.labelId = labelId;
		if let Some(label) = labelId.and_then(|labelId| nodeTree[labelId].getComponentMut::<Text>()) {
			let dialogueText = Localization::text("{0}번째 misc 도구 둘러보기", &[&1]);
			self.typewriter.start(label, Some(&dialogueText));
		}

		// 시트 애니메이션.
		let sheetImage = createSheetImage();
		let heroId = nodeTree.createChild(rootId, "hero");
		let heroNode = &mut nodeTree[heroId];
		heroNode.setLocalPosition(Vector2::create(120.0, 120.0));
		heroNode.setContentSize(Vector2::create(64.0, 64.0));
		heroNode.addComponent(Sprite::new());
		let animator = heroNode.addComponent(SpriteAnimator::new());
		let frameRects: Vec<Rect> = (0..4).map(|frameIndex| Rect::create(frameIndex as f32 * 32.0, 0.0, 32.0, 32.0)).collect();
		animator.addClip("spin", sheetImage, &frameRects, 6.0, true);
		SpriteAnimator::playOnNode(heroNode, "spin", SpriteAnimatorPlayOptions::default());

		// 떠오르는 숫자와 빛 입자.
		self.floatingTextId = Some(FloatingText::create(nodeTree, rootId));
		let touchEffectId = nodeTree.createChild(rootId, "TouchEffect");
		nodeTree[touchEffectId].setPivot(Pivot::topLeft);
		nodeTree[touchEffectId].addComponent(TouchEffect::new());
		self.touchEffectId = Some(touchEffectId);

		// 조이스틱. (누른 채 오른쪽 위로 끈 모습)
		self.virtualPad.setPadRadius(70.0);
		self.virtualPad.setBallRadius(28.0);
		self.virtualPad.setPadColor(Color::create(1.0, 1.0, 1.0, 0.15));
		self.virtualPad.setBallColor(Color::create(1.0, 1.0, 1.0, 0.5));
		self.virtualPad.setPadPosition(Vector2::create(VIEW_WIDTH - 120.0, VIEW_HEIGHT - 220.0));
		let padPosition = self.virtualPad.getPadPosition();
		self.virtualPad.touchPress(padPosition);
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, timeDelta: f32) {
		self.elapsedSeconds += timeDelta;
		self.spawnSeconds += timeDelta;

		self.imageScroller.tick(timeDelta);
		let followTarget = Grid::toWorldCenter(11, 3, TILE_SIZE);
		self.camera.follow(&[followTarget], 1.5, timeDelta);
		self.camera.tick(timeDelta);
		self.shaker.tick(timeDelta);

		// 조이스틱 구슬을 원을 그리며 끈다.
		let padPosition = self.virtualPad.getPadPosition();
		let dragAngle = self.elapsedSeconds * 2.0 - 0.8;
		let dragPosition = Vector2::create(padPosition.x + dragAngle.cos() * 120.0, padPosition.y + dragAngle.sin() * 120.0);
		self.virtualPad.touchMove(dragPosition);
		self.virtualPad.tick(timeDelta);

		let nodeTree = self.sceneBase.getNodeTreeMut();
		if let Some(label) = self.labelId.and_then(|labelId| nodeTree[labelId].getComponentMut::<Text>()) {
			self.typewriter.tick(label, timeDelta);
		}
		if self.spawnSeconds >= 0.2 {
			self.spawnSeconds = 0.0;
			let spawnX = math::randomRange(300.0, 660.0);
			let spawnPosition = Vector2::create(spawnX, 260.0);
			if let Some(floatingTextId) = self.floatingTextId {
				let damageText = format!("-{}", (math::random() * 300.0).round());
				let floatingTextOptions = FloatingTextOptions {
					color: Color::create(1.0, 0.45, 0.45, 1.0),
					fontSize: 28.0,
					..FloatingTextOptions::default()
				};
				FloatingText::spawn(nodeTree, floatingTextId, &damageText, spawnPosition, floatingTextOptions);
			}
			if let Some(touchEffect) = self.touchEffectId.and_then(|touchEffectId| nodeTree[touchEffectId].getComponentMut::<TouchEffect>()) {
				for _ in 0..6 {
					touchEffect.createTouchParticle(spawnX, 340.0);
				}
			}
		}
		self.sceneBase.tick(engine, timeDelta);
	}

	//==============================================================================
	// 이전 출력. (배경색, 흐르는 줄무늬)
	//==============================================================================
	fn preDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.sceneBase.drawSceneBackground(engine, graphic);
		self.imageScroller.draw(graphic);
	}

	//==============================================================================
	// 출력. (카메라 + 흔들림으로 월드, 그 위에 노드)
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		graphic.pushState();
		let shakeOffset = self.shaker.getOffset();
		graphic.translate(shakeOffset.x, shakeOffset.y);
		self.camera.applyTransform(graphic);
		self.drawWorld(graphic);
		graphic.popState();
		self.sceneBase.draw(graphic);
	}

	//==============================================================================
	// 이후 출력. (조이스틱)
	//==============================================================================
	fn postDraw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.virtualPad.draw(graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engineConfiguration = EngineConfiguration::new();
	engineConfiguration.title = String::from("misc tour (바닐라엔진)");
	engineConfiguration.referenceResolutionSize = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engineConfiguration.viewScaleMode = ViewScaleMode::stretchShort;
	let engine = Engine::new(engineConfiguration);
	if let Err(error) = engine.run(Box::new(MiscTourScene::new())) {
		eprintln!("misctour: {}", error);
		std::process::exit(1);
	}
}

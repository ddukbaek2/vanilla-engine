//==============================================================================
// 스프라이트 애니메이터 컴포넌트.
// - 원본 misc/spriteanimator.js 를 옮긴 것이다. 이름을 붙인 클립(프레임 시퀀스)을 등록해 두고 play("run") 으로 바꾼다.
// - 같은 클립을 다시 play 해도 처음부터 하지 않는다. (restartIfSame 으로 바꿀 수 있다)
// - 클립이 끝나면 onComplete 콜백으로 다음 상태를 이어 간다. (FSM 과 묶어 쓴다)
// - 같은 노드의 Sprite 컴포넌트에 지금 프레임(이미지 + 영역)을 넣는다. tick 때 context.getNodeMut().getComponentMut::<Sprite>() 로 찾는다.
// - 사용:
//     let animator = nodeTree[heroId].addComponent(SpriteAnimator::new());
//     animator.addClip("idle", image.clone(), &idleRects, 10.0, true);
//     animator.addClip("attack", image.clone(), &attackRects, 12.0, false);
//     SpriteAnimator::playOnNode(&mut nodeTree[heroId], "attack", SpriteAnimatorPlayOptions::createWithOnComplete(|animator| {
//         animator.play("idle", SpriteAnimatorPlayOptions::default());
//     }));
// - 원본과 다른 점:
//   play 는 원본처럼 그 자리에서 Sprite 에 프레임을 넣지 못한다. (러스트는 컴포넌트 하나를 빌린 동안 형제 컴포넌트를 함께 빌릴 수 없다)
//     그래서 다음 tick 에 넣는다. 그 자리에서 넣으려면 노드를 받는 playOnNode 를 쓴다.
//   onComplete 는 애니메이터를 받는 한 번짜리 콜백(FnOnce(&mut SpriteAnimator))이다. 원본은 클로저가 애니메이터를 붙잡아 썼다.
//   fps 는 이름을 줄이지 않아 framePerSecond 다. 기본값(10, 반복)은 DEFAULT_FRAME_PER_SECOND 와 true 를 넘긴다.
//   아틀라스 JSON 은 serde_json::Value 로 받는다. 원본 findSprite(찾아서 들고 있기)는 형제를 들고 있을 수 없어 옮기지 않았다.
//==============================================================================
use crate::base::rect::Rect;
use crate::core::animation::Animation;
use crate::core::component::sprite::Sprite;
use crate::core::component::{Component, ComponentBase};
use crate::core::image::Image;
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::WorldNode;
use std::collections::HashMap;
use std::rc::Rc;

// 클립이 끝났을 때 한 번 불린다. (애니메이터를 받아 다음 클립을 틀 수 있다)
type SpriteAnimatorCompleteEvent = Box<dyn FnOnce(&mut SpriteAnimator)>;

//==============================================================================
// 클립. (시트 한 장, 칸 영역 목록, 초당 프레임 수, 반복)
//==============================================================================
#[derive(Clone)]
struct SpriteAnimatorClip {
	image: Rc<Image>,
	frameRects: Vec<Rect>,
	framePerSecond: f32,
	isLoop: bool,
}

//==============================================================================
// 재생 설정. (원본 options = { onComplete?, restartIfSame = false })
//==============================================================================
#[derive(Default)]
pub struct SpriteAnimatorPlayOptions {
	// 클립이 끝났을 때 한 번 불린다. (반복하지 않는 클립만 끝난다)
	pub onComplete: Option<SpriteAnimatorCompleteEvent>,
	// 같은 클립이 도는 중이어도 처음부터 다시 틀지.
	pub restartIfSame: bool,
}

impl SpriteAnimatorPlayOptions {
	//==============================================================================
	// 완료 콜백만 정한 설정.
	//==============================================================================
	pub fn createWithOnComplete(onComplete: impl FnOnce(&mut SpriteAnimator) + 'static) -> SpriteAnimatorPlayOptions {
		SpriteAnimatorPlayOptions {
			onComplete: Some(Box::new(onComplete)),
			restartIfSame: false,
		}
	}
}

//==============================================================================
// 스프라이트 애니메이터 컴포넌트.
//==============================================================================
pub struct SpriteAnimator {
	componentBase: ComponentBase,
	clipTable: HashMap<String, SpriteAnimatorClip>,
	clipName: Option<String>,
	animation: Animation,
	onComplete: Option<SpriteAnimatorCompleteEvent>,
}

impl SpriteAnimator {
	// 클립의 기본 초당 프레임 수. (원본 fps = 10)
	pub const DEFAULT_FRAME_PER_SECOND: f32 = 10.0;

	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> SpriteAnimator {
		SpriteAnimator {
			componentBase: ComponentBase::new(),
			clipTable: HashMap::new(),
			clipName: None,
			animation: Animation::new(),
			onComplete: None,
		}
	}

	//==============================================================================
	// 클립 등록. (같은 이름이 있으면 바꾼다)
	//==============================================================================
	pub fn addClip(&mut self, clipName: &str, image: Rc<Image>, frameRects: &[Rect], framePerSecond: f32, isLoop: bool) {
		let clip = SpriteAnimatorClip {
			image,
			frameRects: frameRects.to_vec(),
			framePerSecond,
			isLoop,
		};
		self.clipTable.insert(clipName.to_string(), clip);
	}

	//==============================================================================
	// 아틀라스 JSON 으로 클립 등록. (TexturePacker 꼴 { frames: { 이름: { frame: { x, y, w, h } } } }, 없는 칸 이름은 건너뛴다)
	//==============================================================================
	pub fn addClipFromAtlas(&mut self, clipName: &str, image: Rc<Image>, atlasJson: &serde_json::Value, frameIds: &[&str], framePerSecond: f32, isLoop: bool) {
		let mut frameRects: Vec<Rect> = Vec::new();
		for frameId in frameIds {
			let frameData = match atlasJson.get("frames").and_then(|frames| frames.get(*frameId)) {
				Some(frameData) => {
					frameData
				}
				None => {
					continue;
				}
			};
			let rectData = frameData.get("frame").unwrap_or(frameData);
			let readNumber = |names: &[&str]| -> f32 {
				for name in names {
					if let Some(numberValue) = rectData.get(*name).and_then(|value| value.as_f64()) {
						return numberValue as f32;
					}
				}
				0.0
			};
			let x = readNumber(&["x"]);
			let y = readNumber(&["y"]);
			let width = readNumber(&["w", "width"]);
			let height = readNumber(&["h", "height"]);
			frameRects.push(Rect::create(x, y, width, height));
		}
		self.addClip(clipName, image, &frameRects, framePerSecond, isLoop);
	}

	//==============================================================================
	// 재생. (처음부터 틀었으면 true)
	// - 같은 클립이 이미 도는 중이면 다시 틀지 않고 완료 콜백만 바꾼다. (restartIfSame 이면 처음부터)
	// - Sprite 에는 다음 tick 에 들어간다. 그 자리에서 넣으려면 playOnNode.
	//==============================================================================
	pub fn play(&mut self, clipName: &str, options: SpriteAnimatorPlayOptions) -> bool {
		let clip = match self.clipTable.get(clipName) {
			Some(clip) => {
				clip
			}
			None => {
				return false;
			}
		};
		let isSameClip = self.clipName.as_deref() == Some(clipName);
		if isSameClip && self.animation.isPlaying() && !options.restartIfSame {
			// 완주 콜백만 갈아 끼운다.
			self.onComplete = options.onComplete;
			return false;
		}
		self.animation.stop();
		self.animation.setFramesFromRects(clip.image.clone(), &clip.frameRects);
		self.animation.setAnimationSpeed(clip.framePerSecond);
		self.animation.setLoop(clip.isLoop);
		self.clipName = Some(clipName.to_string());
		self.onComplete = options.onComplete;
		self.animation.gotoAndPlay(0);
		true
	}

	//==============================================================================
	// 노드의 애니메이터로 재생하고 같은 노드의 Sprite 에 그 자리에서 첫 프레임을 넣는다. (원본 play 와 같은 효과)
	// - 애니메이터가 없으면 false.
	//==============================================================================
	pub fn playOnNode(node: &mut WorldNode, clipName: &str, options: SpriteAnimatorPlayOptions) -> bool {
		let (isStarted, currentFrame) = match node.getComponentMut::<SpriteAnimator>() {
			Some(spriteAnimator) => {
				let isStarted = spriteAnimator.play(clipName, options);
				let currentFrame = spriteAnimator.animation.getCurrentFrame().cloned();
				(isStarted, currentFrame)
			}
			None => {
				return false;
			}
		};
		if let (Some(currentFrame), Some(sprite)) = (currentFrame, node.getComponentMut::<Sprite>()) {
			let frameImage = currentFrame.getImage().clone();
			let frameRect = currentFrame.getImageRect();
			sprite.setImage(Some(frameImage));
			sprite.setImageRect(frameRect);
		}
		isStarted
	}

	//==============================================================================
	// 정지. (첫 프레임으로 되돌리고, 완료 콜백은 부르지 않는다)
	//==============================================================================
	pub fn stop(&mut self) {
		self.animation.stop();
	}

	//==============================================================================
	// 지금 프레임을 스프라이트에 넣기. (프레임이 없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn applyCurrentFrame(&self, sprite: &mut Sprite) {
		let currentFrame = match self.animation.getCurrentFrame() {
			Some(currentFrame) => {
				currentFrame
			}
			None => {
				return;
			}
		};
		let frameImage = currentFrame.getImage().clone();
		sprite.setImage(Some(frameImage));
		let frameRect = currentFrame.getImageRect();
		sprite.setImageRect(frameRect);
	}

	//==============================================================================
	// 지금 클립 이름 반환. (아직 틀지 않았으면 None)
	//==============================================================================
	pub fn getClipName(&self) -> Option<&str> {
		self.clipName.as_deref()
	}

	//==============================================================================
	// 재생 중인지.
	//==============================================================================
	pub fn isPlaying(&self) -> bool {
		self.animation.isPlaying()
	}

	//==============================================================================
	// 클립이 있는지.
	//==============================================================================
	pub fn hasClip(&self, clipName: &str) -> bool {
		self.clipTable.contains_key(clipName)
	}

	//==============================================================================
	// 지금 프레임 번호. (러스트 판에서 더한 것, 시험과 디버그용)
	//==============================================================================
	pub fn getCurrentFrameIndex(&self) -> usize {
		self.animation.getCurrentFrameIndex()
	}
}

impl Default for SpriteAnimator {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> SpriteAnimator {
		SpriteAnimator::new()
	}
}

impl Component for SpriteAnimator {
	//==============================================================================
	// 공통 상태 반환.
	//==============================================================================
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	//==============================================================================
	// 공통 상태 반환. (변경용)
	//==============================================================================
	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	//==============================================================================
	// 컴포넌트 이름.
	//==============================================================================
	fn getComponentType(&self) -> &'static str {
		"SpriteAnimator"
	}

	//==============================================================================
	// 갱신. (프레임을 넘기고, 반복하지 않는 클립이 끝났으면 완료 콜백, 그다음 같은 노드의 Sprite 에 지금 프레임을 넣는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		let wasPlaying = self.animation.isPlaying();
		self.animation.tick(timeDelta);
		let isCompleted = wasPlaying && !self.animation.isPlaying();
		if isCompleted {
			if let Some(onComplete) = self.onComplete.take() {
				onComplete(self);
			}
		}
		if let Some(sprite) = context.getNodeMut().getComponentMut::<Sprite>() {
			self.applyCurrentFrame(sprite);
		}
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::NodeTree;
	use std::cell::Cell;

	//==============================================================================
	// 가로로 16x16 칸이 frameCount 개 있는 시트 영역.
	//==============================================================================
	fn createSheetRects(firstColumn: usize, frameCount: usize) -> Vec<Rect> {
		(firstColumn..firstColumn + frameCount).map(|column| Rect::create(column as f32 * 16.0, 0.0, 16.0, 16.0)).collect()
	}

	//==============================================================================
	// 그 자리 반영, 프레임 넘김, 같은 클립 다시 틀기, 끝나면 콜백으로 다음 클립.
	//==============================================================================
	#[test]
	fn playAdvanceAndChainOnComplete() {
		let engine = Engine::new(EngineConfiguration::new());
		let image = Image::createFromPixels(128, 16, vec![0u8; 128 * 16 * 4]);
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let heroId = nodeTree.createChild(rootId, "hero");
		nodeTree[heroId].addComponent(Sprite::new());
		let animator = nodeTree[heroId].addComponent(SpriteAnimator::new());
		animator.addClip("idle", image.clone(), &createSheetRects(0, 2), 10.0, true);
		animator.addClip("attack", image.clone(), &createSheetRects(4, 3), 10.0, false);
		assert!(animator.hasClip("idle"));
		assert!(!animator.play("missing", SpriteAnimatorPlayOptions::default()));

		let completeCount = std::rc::Rc::new(Cell::new(0));
		let completeCountForEvent = completeCount.clone();
		let isStarted = SpriteAnimator::playOnNode(&mut nodeTree[heroId], "attack", SpriteAnimatorPlayOptions::createWithOnComplete(move |animator| {
			completeCountForEvent.set(completeCountForEvent.get() + 1);
			animator.play("idle", SpriteAnimatorPlayOptions::default());
		}));
		assert!(isStarted);
		assert_eq!(nodeTree[heroId].getComponent::<Sprite>().expect("sprite").getImageRect(), Rect::create(64.0, 0.0, 16.0, 16.0));

		// 같은 클립은 다시 틀지 않는다.
		let animator = nodeTree[heroId].getComponentMut::<SpriteAnimator>().expect("animator");
		assert!(!animator.play("attack", SpriteAnimatorPlayOptions::createWithOnComplete(|animator| {
			animator.play("idle", SpriteAnimatorPlayOptions::default());
		})));

		nodeTree.tick(&engine, 0.1);
		assert_eq!(nodeTree[heroId].getComponent::<Sprite>().expect("sprite").getImageRect(), Rect::create(80.0, 0.0, 16.0, 16.0));
		nodeTree.tick(&engine, 0.1);
		nodeTree.tick(&engine, 0.1);

		// 갈아 끼운 콜백이 불려 idle 로 넘어갔다. (처음 콜백은 불리지 않는다)
		assert_eq!(completeCount.get(), 0);
		let animator = nodeTree[heroId].getComponent::<SpriteAnimator>().expect("animator");
		assert_eq!(animator.getClipName(), Some("idle"));
		assert!(animator.isPlaying());
		assert_eq!(nodeTree[heroId].getComponent::<Sprite>().expect("sprite").getImageRect(), Rect::create(0.0, 0.0, 16.0, 16.0));

		// idle 은 반복한다.
		nodeTree.tick(&engine, 0.1);
		nodeTree.tick(&engine, 0.1);
		assert_eq!(nodeTree[heroId].getComponent::<SpriteAnimator>().expect("animator").getCurrentFrameIndex(), 0);
	}

	//==============================================================================
	// 아틀라스 JSON 으로 클립 만들기. (frame 아래, 또는 바로, w/h 또는 width/height)
	//==============================================================================
	#[test]
	fn addClipFromAtlasReadsRects() {
		let image = Image::createFromPixels(64, 64, vec![0u8; 64 * 64 * 4]);
		let atlasJson = serde_json::json!({
			"frames": {
				"run_0": { "frame": { "x": 0, "y": 0, "w": 16, "h": 20 } },
				"run_1": { "x": 16, "y": 0, "width": 16, "height": 20 }
			}
		});
		let engine = Engine::new(EngineConfiguration::new());
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let heroId = nodeTree.createChild(rootId, "hero");
		nodeTree[heroId].addComponent(Sprite::new());
		let animator = nodeTree[heroId].addComponent(SpriteAnimator::new());
		animator.addClipFromAtlas("run", image, &atlasJson, &["run_0", "missing", "run_1"], SpriteAnimator::DEFAULT_FRAME_PER_SECOND, true);
		assert!(animator.play("run", SpriteAnimatorPlayOptions::default()));

		// play 만 하면 다음 tick 에 들어간다.
		assert!(nodeTree[heroId].getComponent::<Sprite>().expect("sprite").getImage().is_none());
		nodeTree.tick(&engine, 0.0);
		assert_eq!(nodeTree[heroId].getComponent::<Sprite>().expect("sprite").getImageRect(), Rect::create(0.0, 0.0, 16.0, 20.0));
		nodeTree.tick(&engine, 0.1);
		assert_eq!(nodeTree[heroId].getComponent::<Sprite>().expect("sprite").getImageRect(), Rect::create(16.0, 0.0, 16.0, 20.0));
	}
}

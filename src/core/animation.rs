//==============================================================================
// 애니메이션 처리기.
// - 원본 core/animation.js 를 옮긴 것이다. 프레임(Frame) 목록을 초당 프레임 수(animationSpeed)로 넘기고,
//   반복하지 않으면 마지막 프레임에서 멈추고 완료 콜백을 부른다.
// - 원본도 컴포넌트가 아니라 그냥 객체다. 노드에 붙지 않고, 들고 있는 쪽(컴포넌트 등)이 tick 을 불러 주고
//   getCurrentImage / getCurrentRect 로 지금 프레임을 꺼내 Sprite 에 넣는다.
// - 원본의 pause / resume 은 비어 있다. 그래서 gotoAndStop 도 프레임만 옮기고 재생을 멈추지 않는다. 그대로 옮겼다.
// - 원본의 setFrames 계열은 배열이 아니거나 비면 그냥 돌아간다. 러스트 판은 타입이 정해져 있어서 빈 목록만 거른다.
//   (setFrames 는 원본처럼 빈 목록도 받는다)
// - 프레임 번호는 usize 다. 원본 gotoAndPlay(-1) 처럼 음수를 0 으로 맞추는 일은 타입이 대신한다.
//==============================================================================
use crate::base::rect::Rect;
use crate::core::frame::Frame;
use crate::core::image::Image;
use std::rc::Rc;

//==============================================================================
// 애니메이션 처리기.
//==============================================================================
pub struct Animation {
	frames: Vec<Frame>,
	isPlaying: bool,
	currentFrameIndex: usize,
	frameTimeCounter: f32,
	animationSpeed: f32,
	isLoop: bool,
	onComplete: Option<Box<dyn FnMut()>>,
}

impl Animation {
	//==============================================================================
	// 생성. (초당 10 프레임, 반복, 정지 상태)
	//==============================================================================
	pub fn new() -> Animation {
		Animation {
			frames: Vec::new(),
			isPlaying: false,
			currentFrameIndex: 0,
			frameTimeCounter: 0.0,
			animationSpeed: 10.0,
			isLoop: true,
			onComplete: None,
		}
	}

	//==============================================================================
	// 갱신. (쌓인 시간만큼 프레임을 넘긴다. 한 번에 여러 프레임을 넘길 수 있다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		if !self.isPlaying || self.frames.is_empty() || self.animationSpeed <= 0.0 {
			return;
		}

		let frameDuration = 1.0 / self.animationSpeed;
		self.frameTimeCounter += timeDelta;

		if self.frameTimeCounter >= frameDuration {
			let framesToAdvance = (self.frameTimeCounter / frameDuration).floor() as usize;
			self.frameTimeCounter %= frameDuration;
			self.currentFrameIndex += framesToAdvance;

			let frameCount = self.frames.len();
			if self.currentFrameIndex >= frameCount {
				// 반복.
				if self.isLoop {
					self.currentFrameIndex %= frameCount;
				}
				else {
					self.currentFrameIndex = frameCount - 1;
					self.isPlaying = false;
					if let Some(onComplete) = self.onComplete.as_mut() {
						onComplete();
					}
				}
			}
		}
	}

	//==============================================================================
	// 재생. (이미 재생 중이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn play(&mut self) {
		let isPlaying = self.isPlaying();
		if isPlaying {
			return;
		}

		self.isPlaying = true;
	}

	//==============================================================================
	// 일시정지. (원본도 비어 있다)
	//==============================================================================
	pub fn pause(&mut self) {
	}

	//==============================================================================
	// 재개. (원본도 비어 있다)
	//==============================================================================
	pub fn resume(&mut self) {
	}

	//==============================================================================
	// 정지. (재생 중일 때만 첫 프레임으로 되돌린다)
	//==============================================================================
	pub fn stop(&mut self) {
		let isPlaying = self.isPlaying();
		if !isPlaying {
			return;
		}

		self.isPlaying = false;
		self.currentFrameIndex = 0;
		self.frameTimeCounter = 0.0;
	}

	//==============================================================================
	// 프레임 설정. (설정한 뒤 정지한다)
	//==============================================================================
	pub fn setFrames(&mut self, frames: Vec<Frame>) {
		self.frames = frames;
		self.stop();
	}

	//==============================================================================
	// 프레임 설정. (낱장의 스프라이트 이미지 목록, 비면 무시한다)
	//==============================================================================
	pub fn setFramesFromImages(&mut self, images: &[Rc<Image>]) {
		if images.is_empty() {
			return;
		}
		let frames: Vec<Frame> = images.iter().map(|image| Frame::new(image.clone(), None)).collect();
		self.setFrames(frames);
	}

	//==============================================================================
	// 프레임 설정. (스프라이트 시트 한 장과 칸 영역 목록, 비면 무시한다)
	//==============================================================================
	pub fn setFramesFromRects(&mut self, image: Rc<Image>, rects: &[Rect]) {
		if rects.is_empty() {
			return;
		}
		let frames: Vec<Frame> = rects.iter().map(|rect| Frame::new(image.clone(), Some(*rect))).collect();
		self.setFrames(frames);
	}

	//==============================================================================
	// 초당 프레임 수 설정. (예: 60 이면 초당 이미지가 60 번 바뀐다. 0 이하면 넘기지 않는다)
	//==============================================================================
	pub fn setAnimationSpeed(&mut self, animationSpeed: f32) {
		self.animationSpeed = animationSpeed;
	}

	//==============================================================================
	// 반복 여부 설정.
	//==============================================================================
	pub fn setLoop(&mut self, isLoop: bool) {
		self.isLoop = isLoop;
	}

	//==============================================================================
	// 애니메이션 완료 콜백 설정. (반복하지 않을 때 마지막 프레임에 닿으면 불린다)
	//==============================================================================
	pub fn setOnComplete(&mut self, callback: impl FnMut() + 'static) {
		self.onComplete = Some(Box::new(callback));
	}

	//==============================================================================
	// 특정 프레임으로 이동하여 재생한다. (번호는 프레임 수 안으로 맞춘다)
	//==============================================================================
	pub fn gotoAndPlay(&mut self, index: usize) {
		self.currentFrameIndex = self.clampFrameIndex(index);
		self.frameTimeCounter = 0.0;
		self.play();
	}

	//==============================================================================
	// 특정 프레임으로 이동하여 정지한다. (원본처럼 pause 를 부르는데 pause 가 비어 있어 재생은 이어진다)
	//==============================================================================
	pub fn gotoAndStop(&mut self, index: usize) {
		self.currentFrameIndex = self.clampFrameIndex(index);
		self.frameTimeCounter = 0.0;
		self.pause();
	}

	//==============================================================================
	// 프레임 번호를 0 ~ 프레임 수 - 1 로 맞춘다. (프레임이 없으면 0)
	//==============================================================================
	fn clampFrameIndex(&self, index: usize) -> usize {
		let frameCount = self.frames.len();
		if frameCount == 0 {
			return 0;
		}
		index.min(frameCount - 1)
	}

	//==============================================================================
	// 재생 중인지 여부 반환.
	//==============================================================================
	pub fn isPlaying(&self) -> bool {
		self.isPlaying
	}

	//==============================================================================
	// 현재 프레임 반환. (없으면 None)
	//==============================================================================
	pub fn getCurrentFrame(&self) -> Option<&Frame> {
		self.frames.get(self.currentFrameIndex)
	}

	//==============================================================================
	// 현재 이미지 반환.
	//==============================================================================
	pub fn getCurrentImage(&self) -> Option<&Rc<Image>> {
		let frame = self.getCurrentFrame();
		frame.map(|frame| frame.getImage())
	}

	//==============================================================================
	// 현재 영역 반환.
	//==============================================================================
	pub fn getCurrentRect(&self) -> Option<Rect> {
		let frame = self.getCurrentFrame();
		frame.map(|frame| frame.getImageRect())
	}

	//==============================================================================
	// 현재 프레임 번호 반환.
	//==============================================================================
	pub fn getCurrentFrameIndex(&self) -> usize {
		self.currentFrameIndex
	}

	//==============================================================================
	// 모든 프레임 수 반환.
	//==============================================================================
	pub fn getTotalFrameCount(&self) -> usize {
		self.frames.len()
	}
}

impl Default for Animation {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Animation {
		Animation::new()
	}
}

//==============================================================================
// 시험. (GL 없이 돈다)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::Cell;

	//==============================================================================
	// 가로로 칸이 frameCount 개 있는 시트로 애니메이션을 만든다. (칸 하나 16x16)
	//==============================================================================
	fn createSheetAnimation(frameCount: usize) -> Animation {
		let sheetWidth = 16 * frameCount as u32;
		let pixels = vec![0u8; (sheetWidth * 16 * 4) as usize];
		let image = Image::createFromPixels(sheetWidth, 16, pixels);
		let rects: Vec<Rect> = (0..frameCount).map(|frameIndex| Rect::create(16.0 * frameIndex as f32, 0.0, 16.0, 16.0)).collect();
		let mut animation = Animation::new();
		animation.setFramesFromRects(image, &rects);
		animation
	}

	//==============================================================================
	// 재생하지 않으면 tick 이 프레임을 넘기지 않는다.
	//==============================================================================
	#[test]
	fn tickDoesNothingUntilPlay() {
		let mut animation = createSheetAnimation(4);
		animation.tick(10.0);
		assert_eq!(animation.getCurrentFrameIndex(), 0);
		assert!(!animation.isPlaying());
	}

	//==============================================================================
	// 초당 프레임 수대로 넘기고, 남은 시간은 다음 tick 으로 넘긴다.
	//==============================================================================
	#[test]
	fn tickAdvancesBySpeedAndKeepsRemainder() {
		let mut animation = createSheetAnimation(4);
		animation.setAnimationSpeed(4.0);
		animation.play();

		animation.tick(0.125);
		assert_eq!(animation.getCurrentFrameIndex(), 0);
		animation.tick(0.125);
		assert_eq!(animation.getCurrentFrameIndex(), 1);
		assert_eq!(animation.getCurrentRect(), Some(Rect::create(16.0, 0.0, 16.0, 16.0)));

		// 한 번에 두 프레임 반. (두 프레임 넘기고 반 프레임은 남는다)
		animation.tick(0.625);
		assert_eq!(animation.getCurrentFrameIndex(), 3);
		animation.tick(0.125);
		assert_eq!(animation.getCurrentFrameIndex(), 0);
	}

	//==============================================================================
	// 반복하면 프레임 수로 나눈 나머지로 돈다.
	//==============================================================================
	#[test]
	fn loopWrapsAround() {
		let mut animation = createSheetAnimation(3);
		animation.setAnimationSpeed(2.0);
		animation.play();
		animation.tick(2.5);
		assert_eq!(animation.getCurrentFrameIndex(), 2);
		animation.tick(0.5);
		assert_eq!(animation.getCurrentFrameIndex(), 0);
		assert!(animation.isPlaying());
	}

	//==============================================================================
	// 반복하지 않으면 마지막 프레임에서 멈추고 완료 콜백을 한 번 부른다.
	//==============================================================================
	#[test]
	fn noLoopStopsAtLastFrameAndCompletesOnce() {
		let mut animation = createSheetAnimation(3);
		let completeCount = Rc::new(Cell::new(0));
		let completeCountInCallback = completeCount.clone();
		animation.setAnimationSpeed(2.0);
		animation.setLoop(false);
		animation.setOnComplete(move || {
			let previousCount = completeCountInCallback.get();
			completeCountInCallback.set(previousCount + 1);
		});
		animation.play();
		animation.tick(5.0);
		assert_eq!(animation.getCurrentFrameIndex(), 2);
		assert!(!animation.isPlaying());
		assert_eq!(completeCount.get(), 1);

		animation.tick(5.0);
		assert_eq!(completeCount.get(), 1);
		assert_eq!(animation.getCurrentFrameIndex(), 2);
	}

	//==============================================================================
	// stop 은 재생 중일 때만 첫 프레임으로 되돌린다.
	//==============================================================================
	#[test]
	fn stopResetsOnlyWhilePlaying() {
		let mut animation = createSheetAnimation(4);
		animation.setAnimationSpeed(1.0);
		animation.play();
		animation.tick(2.0);
		assert_eq!(animation.getCurrentFrameIndex(), 2);
		animation.stop();
		assert_eq!(animation.getCurrentFrameIndex(), 0);
		assert!(!animation.isPlaying());

		animation.gotoAndStop(3);
		animation.stop();
		assert_eq!(animation.getCurrentFrameIndex(), 3);
	}

	//==============================================================================
	// goto 계열은 번호를 프레임 수 안으로 맞춘다. gotoAndStop 은 원본처럼 재생을 멈추지 않는다.
	//==============================================================================
	#[test]
	fn gotoClampsIndex() {
		let mut animation = createSheetAnimation(4);
		animation.gotoAndPlay(10);
		assert_eq!(animation.getCurrentFrameIndex(), 3);
		assert!(animation.isPlaying());

		animation.gotoAndStop(1);
		assert_eq!(animation.getCurrentFrameIndex(), 1);
		assert!(animation.isPlaying());

		let mut emptyAnimation = Animation::new();
		emptyAnimation.gotoAndPlay(5);
		assert_eq!(emptyAnimation.getCurrentFrameIndex(), 0);
		assert!(emptyAnimation.getCurrentFrame().is_none());
		assert!(emptyAnimation.getCurrentImage().is_none());
		assert!(emptyAnimation.getCurrentRect().is_none());
		emptyAnimation.tick(1.0);
		assert_eq!(emptyAnimation.getCurrentFrameIndex(), 0);
	}

	//==============================================================================
	// 속도가 0 이하면 넘기지 않는다. 빈 목록은 setFramesFrom... 이 무시한다.
	//==============================================================================
	#[test]
	fn zeroSpeedAndEmptyListsAreIgnored() {
		let mut animation = createSheetAnimation(4);
		animation.setAnimationSpeed(0.0);
		animation.play();
		animation.tick(10.0);
		assert_eq!(animation.getCurrentFrameIndex(), 0);

		let pixels = vec![0u8; 4];
		let image = Image::createFromPixels(1, 1, pixels);
		animation.setFramesFromRects(image, &[]);
		animation.setFramesFromImages(&[]);
		assert_eq!(animation.getTotalFrameCount(), 4);
	}

	//==============================================================================
	// 낱장 이미지 목록으로 만들면 프레임마다 그 이미지 전체가 영역이다.
	//==============================================================================
	#[test]
	fn framesFromImagesUseWholeImages() {
		let firstImage = Image::createFromPixels(8, 4, vec![0u8; 8 * 4 * 4]);
		let secondImage = Image::createFromPixels(2, 6, vec![0u8; 2 * 6 * 4]);
		let mut animation = Animation::new();
		animation.setFramesFromImages(&[firstImage.clone(), secondImage.clone()]);
		assert_eq!(animation.getTotalFrameCount(), 2);
		assert_eq!(animation.getCurrentRect(), Some(Rect::create(0.0, 0.0, 8.0, 4.0)));
		animation.gotoAndPlay(1);
		assert_eq!(animation.getCurrentRect(), Some(Rect::create(0.0, 0.0, 2.0, 6.0)));
		let currentImage = animation.getCurrentImage().expect("현재 이미지");
		assert!(Rc::ptr_eq(currentImage, &secondImage));
	}
}

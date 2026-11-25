//==============================================================================
// 애니메이션 처리기.
// - 원본 core/animation.js 를 옮긴 것이다. 프레임(Frame) 목록을 초당 프레임 수(animation_speed)로 넘기고,
//   반복하지 않으면 마지막 프레임에서 멈추고 완료 콜백을 부른다.
// - 원본도 컴포넌트가 아니라 그냥 객체다. 노드에 붙지 않고, 들고 있는 쪽(컴포넌트 등)이 tick 을 불러 주고
//   get_current_image / get_current_rect 로 지금 프레임을 꺼내 Sprite 에 넣는다.
// - 원본의 set_frames 계열은 배열이 아니거나 비면 그냥 돌아간다. 러스트 판은 타입이 정해져 있어서 빈 목록만 거른다.
//   (set_frames 는 원본처럼 빈 목록도 받는다)
// - 프레임 번호는 usize 다. 원본 goto_and_play(-1) 처럼 음수를 0 으로 맞추는 일은 타입이 대신한다.
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
	is_playing: bool,
	current_frame_index: usize,
	frame_time_counter: f32,
	animation_speed: f32,
	is_loop: bool,
	on_complete: Option<Box<dyn FnMut()>>,
}

impl Animation {
	//==============================================================================
	// 생성. (초당 10 프레임, 반복, 정지 상태)
	//==============================================================================
	pub fn new() -> Animation {
		Animation {
			frames: Vec::new(),
			is_playing: false,
			current_frame_index: 0,
			frame_time_counter: 0.0,
			animation_speed: 10.0,
			is_loop: true,
			on_complete: None,
		}
	}

	//==============================================================================
	// 갱신. (쌓인 시간만큼 프레임을 넘긴다. 한 번에 여러 프레임을 넘길 수 있다)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		if !self.is_playing || self.frames.is_empty() || self.animation_speed <= 0.0 {
			return;
		}

		let frame_duration = 1.0 / self.animation_speed;
		self.frame_time_counter += time_delta;

		if self.frame_time_counter >= frame_duration {
			let frames_to_advance = (self.frame_time_counter / frame_duration).floor() as usize;
			self.frame_time_counter %= frame_duration;
			self.current_frame_index += frames_to_advance;

			let frame_count = self.frames.len();
			if self.current_frame_index >= frame_count {
				// 반복.
				if self.is_loop {
					self.current_frame_index %= frame_count;
				}
				else {
					self.current_frame_index = frame_count - 1;
					self.is_playing = false;
					if let Some(on_complete) = self.on_complete.as_mut() {
						on_complete();
					}
				}
			}
		}
	}

	//==============================================================================
	// 재생. (이미 재생 중이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn play(&mut self) {
		let is_playing = self.is_playing();
		if is_playing {
			return;
		}

		self.is_playing = true;
	}

	//==============================================================================
	// 일시정지. (지금 프레임과 쌓인 시간을 그대로 둔다)
	//==============================================================================
	pub fn pause(&mut self) {
		self.is_playing = false;
	}

	//==============================================================================
	// 재개. (멈춘 프레임에서 이어 간다)
	//==============================================================================
	pub fn resume(&mut self) {
		self.is_playing = true;
	}

	//==============================================================================
	// 정지. (첫 프레임으로 되돌린다. 멈춰 있어도 되돌린다)
	//==============================================================================
	pub fn stop(&mut self) {
		self.is_playing = false;
		self.current_frame_index = 0;
		self.frame_time_counter = 0.0;
	}

	//==============================================================================
	// 프레임 설정. (설정한 뒤 정지한다)
	//==============================================================================
	pub fn set_frames(&mut self, frames: Vec<Frame>) {
		self.frames = frames;
		self.stop();
	}

	//==============================================================================
	// 프레임 설정. (낱장의 스프라이트 이미지 목록, 비면 무시한다)
	//==============================================================================
	pub fn set_frames_from_images(&mut self, images: &[Rc<Image>]) {
		if images.is_empty() {
			return;
		}
		let frames: Vec<Frame> = images.iter().map(|image| Frame::new(image.clone(), None)).collect();
		self.set_frames(frames);
	}

	//==============================================================================
	// 프레임 설정. (스프라이트 시트 한 장과 칸 영역 목록, 비면 무시한다)
	//==============================================================================
	pub fn set_frames_from_rects(&mut self, image: Rc<Image>, rects: &[Rect]) {
		if rects.is_empty() {
			return;
		}
		let frames: Vec<Frame> = rects.iter().map(|rect| Frame::new(image.clone(), Some(*rect))).collect();
		self.set_frames(frames);
	}

	//==============================================================================
	// 초당 프레임 수 설정. (예: 60 이면 초당 이미지가 60 번 바뀐다. 0 이하면 넘기지 않는다)
	//==============================================================================
	pub fn set_animation_speed(&mut self, animation_speed: f32) {
		self.animation_speed = animation_speed;
	}

	//==============================================================================
	// 반복 여부 설정.
	//==============================================================================
	pub fn set_loop(&mut self, is_loop: bool) {
		self.is_loop = is_loop;
	}

	//==============================================================================
	// 애니메이션 완료 콜백 설정. (반복하지 않을 때 마지막 프레임에 닿으면 불린다)
	//==============================================================================
	pub fn set_on_complete(&mut self, callback: impl FnMut() + 'static) {
		self.on_complete = Some(Box::new(callback));
	}

	//==============================================================================
	// 특정 프레임으로 이동하여 재생한다. (번호는 프레임 수 안으로 맞춘다)
	//==============================================================================
	pub fn goto_and_play(&mut self, index: usize) {
		self.current_frame_index = self.clamp_frame_index(index);
		self.frame_time_counter = 0.0;
		self.play();
	}

	//==============================================================================
	// 특정 프레임으로 이동하여 정지한다. (번호는 프레임 수 안으로 맞춘다)
	//==============================================================================
	pub fn goto_and_stop(&mut self, index: usize) {
		self.current_frame_index = self.clamp_frame_index(index);
		self.frame_time_counter = 0.0;
		self.pause();
	}

	//==============================================================================
	// 프레임 번호를 0 ~ 프레임 수 - 1 로 맞춘다. (프레임이 없으면 0)
	//==============================================================================
	fn clamp_frame_index(&self, index: usize) -> usize {
		let frame_count = self.frames.len();
		if frame_count == 0 {
			return 0;
		}
		index.min(frame_count - 1)
	}

	//==============================================================================
	// 재생 중인지 여부 반환.
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.is_playing
	}

	//==============================================================================
	// 현재 프레임 반환. (없으면 None)
	//==============================================================================
	pub fn get_current_frame(&self) -> Option<&Frame> {
		self.frames.get(self.current_frame_index)
	}

	//==============================================================================
	// 현재 이미지 반환.
	//==============================================================================
	pub fn get_current_image(&self) -> Option<&Rc<Image>> {
		let frame = self.get_current_frame();
		frame.map(|frame| frame.get_image())
	}

	//==============================================================================
	// 현재 영역 반환.
	//==============================================================================
	pub fn get_current_rect(&self) -> Option<Rect> {
		let frame = self.get_current_frame();
		frame.map(|frame| frame.get_image_rect())
	}

	//==============================================================================
	// 현재 프레임 번호 반환.
	//==============================================================================
	pub fn get_current_frame_index(&self) -> usize {
		self.current_frame_index
	}

	//==============================================================================
	// 모든 프레임 수 반환.
	//==============================================================================
	pub fn get_total_frame_count(&self) -> usize {
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
	// 가로로 칸이 frame_count 개 있는 시트로 애니메이션을 만든다. (칸 하나 16x16)
	//==============================================================================
	fn create_sheet_animation(frame_count: usize) -> Animation {
		let sheet_width = 16 * frame_count as u32;
		let pixels = vec![0u8; (sheet_width * 16 * 4) as usize];
		let image = Image::create_from_pixels(sheet_width, 16, pixels);
		let rects: Vec<Rect> = (0..frame_count).map(|frame_index| Rect::create(16.0 * frame_index as f32, 0.0, 16.0, 16.0)).collect();
		let mut animation = Animation::new();
		animation.set_frames_from_rects(image, &rects);
		animation
	}

	//==============================================================================
	// 재생하지 않으면 tick 이 프레임을 넘기지 않는다.
	//==============================================================================
	#[test]
	fn tick_does_nothing_until_play() {
		let mut animation = create_sheet_animation(4);
		animation.tick(10.0);
		assert_eq!(animation.get_current_frame_index(), 0);
		assert!(!animation.is_playing());
	}

	//==============================================================================
	// 초당 프레임 수대로 넘기고, 남은 시간은 다음 tick 으로 넘긴다.
	//==============================================================================
	#[test]
	fn tick_advances_by_speed_and_keeps_remainder() {
		let mut animation = create_sheet_animation(4);
		animation.set_animation_speed(4.0);
		animation.play();

		animation.tick(0.125);
		assert_eq!(animation.get_current_frame_index(), 0);
		animation.tick(0.125);
		assert_eq!(animation.get_current_frame_index(), 1);
		assert_eq!(animation.get_current_rect(), Some(Rect::create(16.0, 0.0, 16.0, 16.0)));

		// 한 번에 두 프레임 반. (두 프레임 넘기고 반 프레임은 남는다)
		animation.tick(0.625);
		assert_eq!(animation.get_current_frame_index(), 3);
		animation.tick(0.125);
		assert_eq!(animation.get_current_frame_index(), 0);
	}

	//==============================================================================
	// 반복하면 프레임 수로 나눈 나머지로 돈다.
	//==============================================================================
	#[test]
	fn loop_wraps_around() {
		let mut animation = create_sheet_animation(3);
		animation.set_animation_speed(2.0);
		animation.play();
		animation.tick(2.5);
		assert_eq!(animation.get_current_frame_index(), 2);
		animation.tick(0.5);
		assert_eq!(animation.get_current_frame_index(), 0);
		assert!(animation.is_playing());
	}

	//==============================================================================
	// 반복하지 않으면 마지막 프레임에서 멈추고 완료 콜백을 한 번 부른다.
	//==============================================================================
	#[test]
	fn no_loop_stops_at_last_frame_and_completes_once() {
		let mut animation = create_sheet_animation(3);
		let complete_count = Rc::new(Cell::new(0));
		let complete_count_in_callback = complete_count.clone();
		animation.set_animation_speed(2.0);
		animation.set_loop(false);
		animation.set_on_complete(move || {
			let previous_count = complete_count_in_callback.get();
			complete_count_in_callback.set(previous_count + 1);
		});
		animation.play();
		animation.tick(5.0);
		assert_eq!(animation.get_current_frame_index(), 2);
		assert!(!animation.is_playing());
		assert_eq!(complete_count.get(), 1);

		animation.tick(5.0);
		assert_eq!(complete_count.get(), 1);
		assert_eq!(animation.get_current_frame_index(), 2);
	}

	//==============================================================================
	// stop 은 늘 첫 프레임으로 되돌린다. (프레임을 바꾸면 멈춰 있던 번호도 0 이 된다)
	//==============================================================================
	#[test]
	fn stop_always_resets() {
		let mut animation = create_sheet_animation(4);
		animation.set_animation_speed(1.0);
		animation.play();
		animation.tick(2.0);
		assert_eq!(animation.get_current_frame_index(), 2);
		animation.stop();
		assert_eq!(animation.get_current_frame_index(), 0);
		assert!(!animation.is_playing());

		animation.goto_and_stop(3);
		animation.stop();
		assert_eq!(animation.get_current_frame_index(), 0);

		animation.goto_and_stop(3);
		let pixels = vec![0u8; 16 * 16 * 4];
		let image = Image::create_from_pixels(16, 16, pixels);
		animation.set_frames_from_rects(image, &[Rect::create(0.0, 0.0, 16.0, 16.0)]);
		assert_eq!(animation.get_current_frame_index(), 0);
		assert!(animation.get_current_frame().is_some());
	}

	//==============================================================================
	// pause 는 프레임과 쌓인 시간을 두고 멈추고, resume 은 그 자리에서 이어 간다.
	//==============================================================================
	#[test]
	fn pause_and_resume_keep_frame() {
		let mut animation = create_sheet_animation(4);
		animation.set_animation_speed(4.0);
		animation.play();
		animation.tick(0.375);
		assert_eq!(animation.get_current_frame_index(), 1);
		animation.pause();
		assert!(!animation.is_playing());
		animation.tick(1.0);
		assert_eq!(animation.get_current_frame_index(), 1);
		animation.resume();
		assert!(animation.is_playing());
		animation.tick(0.125);
		assert_eq!(animation.get_current_frame_index(), 2);
	}

	//==============================================================================
	// goto 계열은 번호를 프레임 수 안으로 맞춘다. goto_and_stop 은 그 프레임에서 멈춘다.
	//==============================================================================
	#[test]
	fn goto_clamps_index() {
		let mut animation = create_sheet_animation(4);
		animation.goto_and_play(10);
		assert_eq!(animation.get_current_frame_index(), 3);
		assert!(animation.is_playing());

		animation.goto_and_stop(1);
		assert_eq!(animation.get_current_frame_index(), 1);
		assert!(!animation.is_playing());

		let mut empty_animation = Animation::new();
		empty_animation.goto_and_play(5);
		assert_eq!(empty_animation.get_current_frame_index(), 0);
		assert!(empty_animation.get_current_frame().is_none());
		assert!(empty_animation.get_current_image().is_none());
		assert!(empty_animation.get_current_rect().is_none());
		empty_animation.tick(1.0);
		assert_eq!(empty_animation.get_current_frame_index(), 0);
	}

	//==============================================================================
	// 속도가 0 이하면 넘기지 않는다. 빈 목록은 setFramesFrom... 이 무시한다.
	//==============================================================================
	#[test]
	fn zero_speed_and_empty_lists_are_ignored() {
		let mut animation = create_sheet_animation(4);
		animation.set_animation_speed(0.0);
		animation.play();
		animation.tick(10.0);
		assert_eq!(animation.get_current_frame_index(), 0);

		let pixels = vec![0u8; 4];
		let image = Image::create_from_pixels(1, 1, pixels);
		animation.set_frames_from_rects(image, &[]);
		animation.set_frames_from_images(&[]);
		assert_eq!(animation.get_total_frame_count(), 4);
	}

	//==============================================================================
	// 낱장 이미지 목록으로 만들면 프레임마다 그 이미지 전체가 영역이다.
	//==============================================================================
	#[test]
	fn frames_from_images_use_whole_images() {
		let first_image = Image::create_from_pixels(8, 4, vec![0u8; 8 * 4 * 4]);
		let second_image = Image::create_from_pixels(2, 6, vec![0u8; 2 * 6 * 4]);
		let mut animation = Animation::new();
		animation.set_frames_from_images(&[first_image.clone(), second_image.clone()]);
		assert_eq!(animation.get_total_frame_count(), 2);
		assert_eq!(animation.get_current_rect(), Some(Rect::create(0.0, 0.0, 8.0, 4.0)));
		animation.goto_and_play(1);
		assert_eq!(animation.get_current_rect(), Some(Rect::create(0.0, 0.0, 2.0, 6.0)));
		let current_image = animation.get_current_image().expect("현재 이미지");
		assert!(Rc::ptr_eq(current_image, &second_image));
	}
}

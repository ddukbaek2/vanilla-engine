//==============================================================================
// 애니메이션 클립. (스프라이트 시트 프레임 영역 목록 + 반복 여부 + 길이)
// - 원본 resource/animationclip.js 를 옮긴 것이다. Animator 의 AnimationState 가 이것을 든다.
// - 원본 주석은 프레임 목록을 Frame[] 이라 하지만 fromAtlas 가 넣는 것은 Rect 다. 러스트 판은 Rect 목록으로 정했다.
//   (Frame 은 이미지를 꼭 들어야 해서, 이미지 없는 아틀라스 영역을 담을 수 없다)
// - 원본과 다른 점:
//   - getFrame 은 범위를 넘으면 None 이다. (원본 undefined)
//   - fromAtlas 의 기본 인자(fps = 10, isLoop = true)는 러스트에 없어서 늘 넘긴다.
//     frameIds 는 Display 되는 무엇이든(문자열, 정수) 받는다. 배열 아틀라스는 그 글을 번호로 읽는다.
//   - 3D 의 AnimationClip(graphics3d::skinnedmodel)과 이름이 같아서 lib.rs 에서 다시 내보내지 않는다.
//     vanilla::resource::animationclip::AnimationClip 으로 쓴다.
//==============================================================================
use crate::base::rect::Rect;
use serde_json::Value;
use std::fmt::Display;

//==============================================================================
// 애니메이션 클립.
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimationClip {
	frames: Vec<Rect>,
	isLoop: bool,
	duration: f32,
}

impl AnimationClip {
	//==============================================================================
	// 생성. (빈 프레임, 길이 0, 반복 안 함)
	//==============================================================================
	pub fn new() -> AnimationClip {
		AnimationClip {
			frames: Vec::new(),
			isLoop: false,
			duration: 0.0,
		}
	}

	//==============================================================================
	// 아틀라스 JSON 으로 클립 생성. (정적)
	// - TexturePacker 형식({ frames: { 이름: { frame: {x,y,w,h} } } })과 배열 형식([{x,y,w,h}, ...]) 둘 다 받는다.
	// - frameIds 차례로 프레임을 뽑아 duration = 프레임 수 / fps 로 만든다. 없는 이름은 건너뛴다.
	//==============================================================================
	pub fn fromAtlas<T: Display>(atlasJson: &Value, frameIds: &[T], fps: f32, isLoop: bool) -> AnimationClip {
		let mut clip = AnimationClip::new();
		let mut frameRects: Vec<Rect> = Vec::new();
		for frameId in frameIds.iter() {
			let frameKey = frameId.to_string();
			let frameData = if atlasJson.is_array() {
				findAtlasEntry(atlasJson, &frameKey)
			}
			else {
				atlasJson.get("frames").and_then(|frames| findAtlasEntry(frames, &frameKey))
			};
			let frameData = match frameData {
				Some(frameData) => {
					frameData
				}
				None => {
					continue;
				}
			};
			let rectData = match frameData.get("frame") {
				Some(rectData) if !rectData.is_null() => {
					rectData
				}
				_ => {
					frameData
				}
			};
			let readNumber = |shortName: &str, longName: &str| -> f32 {
				let value = match rectData.get(shortName) {
					Some(value) => {
						Some(value)
					}
					None => {
						rectData.get(longName)
					}
				};
				value.and_then(|value| value.as_f64()).unwrap_or(0.0) as f32
			};
			frameRects.push(Rect::create(readNumber("x", "x"), readNumber("y", "y"), readNumber("w", "width"), readNumber("h", "height")));
		}
		let frameCount = frameRects.len();
		clip.setFrames(frameRects);
		clip.setLoop(isLoop);
		clip.setDuration(if fps > 0.0 {
			frameCount as f32 / fps
		}
		else {
			0.0
		});
		clip
	}

	//==============================================================================
	// 프레임 목록 설정.
	//==============================================================================
	pub fn setFrames(&mut self, frames: Vec<Rect>) {
		self.frames = frames;
	}

	//==============================================================================
	// 반복 재생 여부 설정.
	//==============================================================================
	pub fn setLoop(&mut self, isLoop: bool) {
		self.isLoop = isLoop;
	}

	//==============================================================================
	// 애니메이션 지속 시간 설정. (초)
	//==============================================================================
	pub fn setDuration(&mut self, duration: f32) {
		self.duration = duration;
	}

	//==============================================================================
	// 반복 재생 여부 반환.
	//==============================================================================
	pub fn isLoop(&self) -> bool {
		self.isLoop
	}

	//==============================================================================
	// 대상 번호의 프레임 반환. (범위를 넘으면 None)
	//==============================================================================
	pub fn getFrame(&self, index: usize) -> Option<&Rect> {
		self.frames.get(index)
	}

	//==============================================================================
	// 프레임 목록 반환.
	//==============================================================================
	pub fn getFrames(&self) -> &[Rect] {
		&self.frames
	}

	//==============================================================================
	// 전체 프레임 수 반환.
	//==============================================================================
	pub fn getFrameCount(&self) -> usize {
		self.frames.len()
	}

	//==============================================================================
	// 애니메이션 지속 시간 반환. (초)
	//==============================================================================
	pub fn getDuration(&self) -> f32 {
		self.duration
	}
}

//==============================================================================
// 아틀라스 항목 찾기. (배열이면 글을 번호로 읽고, 객체면 열쇠로)
//==============================================================================
fn findAtlasEntry<'a>(container: &'a Value, frameKey: &str) -> Option<&'a Value> {
	match container {
		Value::Array(entries) => {
			let entryIndex = frameKey.parse::<usize>().ok()?;
			entries.get(entryIndex)
		}
		Value::Object(entries) => {
			entries.get(frameKey)
		}
		_ => {
			None
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	//==============================================================================
	// TexturePacker 형식에서 이름 차례로 프레임을 뽑고 길이를 fps 로 정한다. (없는 이름은 건너뛴다)
	//==============================================================================
	#[test]
	fn fromAtlasReadsTexturePackerFrames() {
		let atlasJson = json!({ "frames": {
			"run0": { "frame": { "x": 0, "y": 0, "w": 32, "h": 48 } },
			"run1": { "frame": { "x": 32, "y": 0, "w": 32, "h": 48 } },
		} });
		let clip = AnimationClip::fromAtlas(&atlasJson, &["run1", "missing", "run0"], 4.0, true);
		assert_eq!(clip.getFrameCount(), 2);
		assert_eq!(clip.getFrame(0), Some(&Rect::create(32.0, 0.0, 32.0, 48.0)));
		assert_eq!(clip.getFrame(1), Some(&Rect::create(0.0, 0.0, 32.0, 48.0)));
		assert_eq!(clip.getFrame(2), None);
		assert_eq!(clip.getDuration(), 0.5);
		assert!(clip.isLoop());
	}

	//==============================================================================
	// 배열 형식은 번호로 찾고 width / height 이름도 읽는다. fps 가 0 이면 길이 0.
	//==============================================================================
	#[test]
	fn fromAtlasReadsArrayFrames() {
		let atlasJson = json!([{ "x": 0, "y": 0, "width": 16, "height": 16 }, { "x": 16, "y": 0, "w": 16, "h": 16 }]);
		let clip = AnimationClip::fromAtlas(&atlasJson, &[1, 0, 5], 0.0, false);
		assert_eq!(clip.getFrames(), &[Rect::create(16.0, 0.0, 16.0, 16.0), Rect::create(0.0, 0.0, 16.0, 16.0)]);
		assert_eq!(clip.getDuration(), 0.0);
		assert!(!clip.isLoop());
	}
}

//==============================================================================
//! 로딩 진행률. (원본 misc/loadingprogress.js)
//! - 로딩의 단계가 알려 준 정도(set_progress, 0 ~ 1, 줄어들지 않음)를 보이는 정도(get_shown_progress)가 부드럽게 따라간다.
//!   받기가 몰려 진행률이 한 번에 뛰어도 막대가 튀지 않고 미끄러진다.
//! - 매 프레임 update(time_delta) 를 부른다. 따라가는 몫은 1 - exp(-빠르기 * 시간) 이라 프레임 간격이 달라도 같은 빠르기다.
//==============================================================================

/// 알려 준 정도를 따라가는 빠르기의 기본값. (초당 남은 거리의 비율)
const DEFAULT_FOLLOW_RATE: f64 = 6.0;

/// 이만큼 가까워지면 알려 준 정도에 딱 붙인다.
const SNAP_DISTANCE: f64 = 0.0005;

//==============================================================================
/// 로딩 진행률.
/// - 받기 단계가 set_progress 로 알리고, 매 프레임 update 한 뒤 get_shown_progress 로 막대를 그린다.
///
/// # 예제
/// 알린 정도를 부드럽게 따라가기
/// ```rust
/// use vanilla::LoadingProgress;
///
/// let mut loading_progress = LoadingProgress::new();
/// loading_progress.set_progress(0.5);
/// loading_progress.update(1.0 / 60.0);
/// let shown_progress = loading_progress.get_shown_progress();
/// assert!(shown_progress > 0.0 && shown_progress < 0.5);
/// loading_progress.set_progress(1.0);
/// for _ in 0..600 {
/// 	loading_progress.update(1.0 / 60.0);
/// }
/// assert!(loading_progress.is_finished());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct LoadingProgress {
	target_progress: f64,
	shown_progress: f64,
	follow_rate: f64,
}

impl LoadingProgress {
	//==============================================================================
	/// 생성. (따라가는 빠르기는 초당 남은 거리의 6 배)
	///
	/// # 돌려줌
	/// 알린 정도와 보이는 정도가 0 인 새 진행률.
	//==============================================================================
	pub fn new() -> LoadingProgress {
		LoadingProgress {
			target_progress: 0.0,
			shown_progress: 0.0,
			follow_rate: DEFAULT_FOLLOW_RATE,
		}
	}

	//==============================================================================
	/// 알려 준 정도 설정. (0 ~ 1 로 자르고, 앞서 알린 것보다 작거나 NaN 이면 무시한다)
	///
	/// # 인자
	/// - `progress`: 알린 정도. (0 ~ 1)
	//==============================================================================
	pub fn set_progress(&mut self, progress: f64) {
		if progress.is_nan() {
			return;
		}
		let clamped_progress = progress.clamp(0.0, 1.0);
		self.target_progress = self.target_progress.max(clamped_progress);
	}

	//==============================================================================
	/// 알려 준 정도 반환.
	///
	/// # 돌려줌
	/// 지금까지 알린 가장 큰 정도. (0 ~ 1)
	//==============================================================================
	pub fn get_progress(&self) -> f64 {
		self.target_progress
	}

	//==============================================================================
	/// 보이는 정도 반환. (그릴 때 읽는다)
	///
	/// # 돌려줌
	/// 알린 정도를 따라가는 중인 정도. (0 ~ 1)
	//==============================================================================
	pub fn get_shown_progress(&self) -> f64 {
		self.shown_progress
	}

	//==============================================================================
	/// 갱신. (보이는 정도가 알려 준 정도를 따라간다. 시간이 0 이하이거나 유한하지 않으면 그대로)
	///
	/// # 인자
	/// - `time_delta`: 지난 시간. (초, 남은 거리의 1 - exp(-빠르기 * time_delta) 만큼 다가간다)
	//==============================================================================
	pub fn update(&mut self, time_delta: f64) {
		if !time_delta.is_finite() || time_delta <= 0.0 {
			return;
		}
		let follow_ratio = 1.0 - (-self.follow_rate * time_delta).exp();
		self.shown_progress += (self.target_progress - self.shown_progress) * follow_ratio;
		if (self.target_progress - self.shown_progress).abs() < SNAP_DISTANCE {
			self.shown_progress = self.target_progress;
		}
	}

	//==============================================================================
	/// 따라가는 빠르기 설정. (초당 남은 거리의 비율, 0 보다 크고 유한해야 한다. 아니면 그대로 둔다)
	///
	/// # 인자
	/// - `follow_rate`: 새 빠르기. (기본 6)
	//==============================================================================
	pub fn set_follow_rate(&mut self, follow_rate: f64) {
		if !follow_rate.is_finite() || follow_rate <= 0.0 {
			return;
		}
		self.follow_rate = follow_rate;
	}

	//==============================================================================
	/// 따라가는 빠르기 반환.
	///
	/// # 돌려줌
	/// 지금 빠르기. (초당 남은 거리의 비율)
	//==============================================================================
	pub fn get_follow_rate(&self) -> f64 {
		self.follow_rate
	}

	//==============================================================================
	/// 보이는 정도가 끝(1)에 닿았는지.
	///
	/// # 돌려줌
	/// 보이는 정도가 1 이상이면 true.
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.shown_progress >= 1.0
	}

	//==============================================================================
	/// 처음으로. (알려 준 정도와 보이는 정도를 0 으로)
	//==============================================================================
	pub fn reset(&mut self) {
		self.target_progress = 0.0;
		self.shown_progress = 0.0;
	}
}

impl Default for LoadingProgress {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> LoadingProgress {
		LoadingProgress::new()
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 줄지 않고, 부드럽게 따라가고, 결국 딱 붙는다.
	//==============================================================================
	#[test]
	fn shown_progress_follows_target() {
		let mut loading_progress = LoadingProgress::new();
		loading_progress.set_progress(0.8);
		loading_progress.set_progress(0.3);
		loading_progress.set_progress(f64::NAN);
		assert_eq!(loading_progress.get_progress(), 0.8);
		loading_progress.update(1.0 / 60.0);
		let first_shown_progress = loading_progress.get_shown_progress();
		assert!(first_shown_progress > 0.0 && first_shown_progress < 0.2);
		for _ in 0..300 {
			loading_progress.update(1.0 / 60.0);
		}
		assert_eq!(loading_progress.get_shown_progress(), 0.8);
		assert!(!loading_progress.is_finished());
		loading_progress.set_progress(2.0);
		for _ in 0..300 {
			loading_progress.update(1.0 / 60.0);
		}
		assert!(loading_progress.is_finished());
		loading_progress.reset();
		assert_eq!(loading_progress.get_shown_progress(), 0.0);
	}
}

//==============================================================================
// AudioParam 자동화. (데스크탑 믹서, 크로미엄 audio_param_handler.cc(옛 audio_param_timeline.cc)를 옮긴 것, Chromium Authors BSD-3-Clause)
// - 원본이 쓰는 value 대입, setValueAtTime, linearRampToValueAtTime, exponentialRampToValueAtTime, setTargetAtTime, cancelScheduledValues 를 옮겼다.
//   (setValueCurveAtTime, cancelAndHoldAtTime 은 원본이 쓰지 않아 옮기지 않았다)
// - 값은 렌더 퀀텀마다 크롬처럼 셈한다. GainNode 의 gain 은 a-rate 라 표본마다 셈하고(CalculateSampleAccurateValues),
//   자동화가 없는 동안은 퀀텀마다 하나(Value)다. 마지막에 셈한 값이 내재 값(intrinsic value)이 되어 원본 gain.value 가 읽는 값이다.
// - value 대입은 크롬처럼 내재 값을 바꾸고 그 때 setValueAtTime 을 하나 넣는다. (AudioParam::setValue)
// - 사건은 넣을 때의 시각(컨텍스트 시계)보다 앞이면 그 시각으로 당기고, 그리기 전에 한 번 더 지금 퀀텀으로 당긴다. (ClampNewEventsToCurrentTime)
// - 표본마다의 셈과 합 차례는 aarch64 의 크롬(스칼라)과 같고, x86_64 는 크롬의 SSE 길(4 줄 묶음)과 같게 셈한다.
//==============================================================================

// 목표를 따라가다 다 왔다고 칠 시간 상수 수와 남은 비. (kTimeConstantsToConverge, kSetTargetThreshold)
const TIME_CONSTANTS_TO_CONVERGE: f32 = 10.0;
const SET_TARGET_THRESHOLD: f32 = 4.539992976248485e-05;

//==============================================================================
// 사건 종류.
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum ParamEventType {
	SetValue,
	LinearRampToValue,
	ExponentialRampToValue,
	SetTarget,
}

//==============================================================================
// 자동화 사건 하나.
//==============================================================================
#[derive(Clone, Debug)]
struct ParamEvent {
	event_type: ParamEventType,
	value: f32,
	time: f64,
	// 램프를 넣을 때의 내재 값. (사건이 없을 때 앞에 SetValue 로 넣는다)
	initial_value: f32,
	time_constant: f64,
	// 넣은 뒤 아직 지금 시각으로 당기지 않았는지.
	is_new: bool,
}

impl ParamEvent {
	fn new(event_type: ParamEventType, value: f32, time: f64) -> ParamEvent {
		ParamEvent {
			event_type,
			value,
			time,
			initial_value: 0.0,
			time_constant: 0.0,
			is_new: true,
		}
	}
}

//==============================================================================
// 원본 AudioParam 의 자동화 호출. (믹서 명령으로 보낸다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub(crate) enum ParamAutomation {
	SetValue {
		value: f32,
	},
	SetValueAtTime {
		value: f32,
		time: f64,
	},
	LinearRampToValueAtTime {
		value: f32,
		time: f64,
	},
	ExponentialRampToValueAtTime {
		value: f32,
		time: f64,
	},
	SetTargetAtTime {
		target: f32,
		time: f64,
		time_constant: f64,
	},
	CancelScheduledValues {
		time: f64,
	},
}

//==============================================================================
// AudioParam 하나. (내재 값과 사건 목록)
//==============================================================================
pub(crate) struct AudioParam {
	intrinsic_value: f32,
	minimum_value: f32,
	maximum_value: f32,
	event_list: Vec<ParamEvent>,
}

impl AudioParam {
	//==============================================================================
	// 생성. (GainNode 의 gain 은 기본 1, 범위는 f32 전체)
	//==============================================================================
	pub(crate) fn new(default_value: f32, minimum_value: f32, maximum_value: f32) -> AudioParam {
		AudioParam {
			intrinsic_value: default_value,
			minimum_value,
			maximum_value,
			event_list: Vec::new(),
		}
	}

	//==============================================================================
	// 내재 값. (원본 gain.value 가 읽는 값)
	//==============================================================================
	pub(crate) fn get_intrinsic_value(&self) -> f32 {
		self.intrinsic_value
	}

	fn set_intrinsic_value(&mut self, value: f32) {
		self.intrinsic_value = value.clamp(self.minimum_value, self.maximum_value);
	}

	//==============================================================================
	// 자동화 호출 받기. (current_frame 은 지금 그릴 퀀텀의 첫 프레임)
	//==============================================================================
	pub(crate) fn apply_automation(&mut self, automation: ParamAutomation, current_frame: u64, sample_rate: f64) {
		let current_time = current_frame as f64 / sample_rate;
		match automation {
			ParamAutomation::SetValue { value } => {
				// AudioParam::setValue: 내재 값을 바꾸고 그 값으로 지금 setValueAtTime.
				self.set_intrinsic_value(value);
				let intrinsic_value = self.intrinsic_value;
				self.insert_event(ParamEvent::new(ParamEventType::SetValue, intrinsic_value, current_time), current_frame, sample_rate);
			}
			ParamAutomation::SetValueAtTime { value, time } => {
				self.insert_event(ParamEvent::new(ParamEventType::SetValue, value, time.max(current_time)), current_frame, sample_rate);
			}
			ParamAutomation::LinearRampToValueAtTime { value, time } => {
				let mut event = ParamEvent::new(ParamEventType::LinearRampToValue, value, time.max(current_time));
				event.initial_value = self.intrinsic_value;
				self.insert_event(event, current_frame, sample_rate);
			}
			ParamAutomation::ExponentialRampToValueAtTime { value, time } => {
				if value == 0.0 {
					return;
				}
				let mut event = ParamEvent::new(ParamEventType::ExponentialRampToValue, value, time.max(current_time));
				event.initial_value = self.intrinsic_value;
				self.insert_event(event, current_frame, sample_rate);
			}
			ParamAutomation::SetTargetAtTime { target, time, time_constant } => {
				let start_time = time.max(current_time);
				if time_constant == 0.0 {
					self.insert_event(ParamEvent::new(ParamEventType::SetValue, target, start_time), current_frame, sample_rate);
				}
				else {
					let mut event = ParamEvent::new(ParamEventType::SetTarget, target, start_time);
					event.time_constant = time_constant;
					self.insert_event(event, current_frame, sample_rate);
				}
			}
			ParamAutomation::CancelScheduledValues { time } => {
				let cancel_time = time.max(current_time);
				if let Some(first_index) = self.event_list.iter().position(|event| event.time >= cancel_time) {
					self.event_list.truncate(first_index);
				}
			}
		}
	}

	//==============================================================================
	// 사건 넣기. (InsertEvent: 지난 사건을 치우고, 비었는데 램프면 앞에 내재 값을 둔 SetValue, 같은 시각이면 뒤에)
	//==============================================================================
	fn insert_event(&mut self, event: ParamEvent, current_frame: u64, sample_rate: f64) {
		if !self.event_list.is_empty() {
			let mut skipped_event_count = 0;
			for event_index in 0..self.event_list.len() {
				let next_event = self.event_list.get(event_index + 1);
				if is_event_current(&self.event_list[event_index], next_event, current_frame as f64, sample_rate) {
					break;
				}
				skipped_event_count += 1;
			}
			if skipped_event_count > 0 {
				self.remove_old_events(skipped_event_count);
			}
		}
		if self.event_list.is_empty() && (event.event_type == ParamEventType::LinearRampToValue || event.event_type == ParamEventType::ExponentialRampToValue) {
			self.event_list.push(ParamEvent::new(ParamEventType::SetValue, event.initial_value, current_frame as f64 / sample_rate));
		}
		if self.event_list.is_empty() {
			self.event_list.push(event);
			return;
		}
		let insert_time = event.time;
		let insertion_index = if self.event_list.last().map(|last| last.time > insert_time).unwrap_or(false) {
			self.event_list.iter().position(|entry| insert_time < entry.time).unwrap_or(self.event_list.len())
		}
		else {
			self.event_list.len()
		};
		self.event_list.insert(insertion_index, event);
	}

	//==============================================================================
	// 지난 사건 치우기. (RemoveOldEvents, 마지막 하나는 남긴다)
	//==============================================================================
	fn remove_old_events(&mut self, event_count: usize) {
		let event_list_length = self.event_list.len();
		if event_list_length > 1 {
			let remove_count = event_count.min(event_list_length - 1);
			self.event_list.drain(0..remove_count);
		}
	}

	//==============================================================================
	// 이 퀀텀에서 표본마다 셈해야 하는지. (HasSampleAccurateValues)
	//==============================================================================
	pub(crate) fn has_sample_accurate_values(&self, current_frame: u64, sample_rate: f64, render_quantum_frames: usize) -> bool {
		let Some(first_event) = self.event_list.first() else {
			return false;
		};
		let quantum_end_time = (current_frame + render_quantum_frames as u64) as f64 / sample_rate;
		if first_event.time > quantum_end_time {
			match first_event.event_type {
				ParamEventType::SetTarget | ParamEventType::SetValue => {
					return false;
				}
				_ => {}
			}
		}
		if self.event_list.len() >= 2 {
			return true;
		}
		match first_event.event_type {
			ParamEventType::SetTarget => {
				first_event.time <= quantum_end_time
			}
			ParamEventType::SetValue | ParamEventType::LinearRampToValue | ParamEventType::ExponentialRampToValue => {
				!(first_event.time < current_frame as f64 / sample_rate)
			}
		}
	}

	//==============================================================================
	// 표본마다의 값. (CalculateSampleAccurateValues → CalculateTimelineValues, 마지막 값이 내재 값이 된다)
	//==============================================================================
	pub(crate) fn calculate_sample_accurate_values(&mut self, value_list: &mut [f32], current_frame: u64, sample_rate: f64) {
		let start_frame = current_frame as usize;
		let end_frame = start_frame + value_list.len();
		let intrinsic_value = self.intrinsic_value;
		let last_value = self.calculate_values_for_frame_range(start_frame, end_frame, intrinsic_value, value_list, sample_rate, sample_rate);
		self.set_intrinsic_value(last_value);
	}

	//==============================================================================
	// 퀀텀의 값 하나. (Value: 사건이 시작했으면 퀀텀 첫 프레임의 값을 control rate 로 셈한다)
	//==============================================================================
	pub(crate) fn calculate_quantum_value(&mut self, current_frame: u64, sample_rate: f64, render_quantum_frames: usize) -> f32 {
		let mut value = self.intrinsic_value;
		let current_time = current_frame as f64 / sample_rate;
		if self.event_list.first().map(|first_event| current_time >= first_event.time).unwrap_or(false) {
			let mut value_list = [0.0f32];
			let start_frame = current_frame as usize;
			value = self.calculate_values_for_frame_range(start_frame, start_frame + 1, value, &mut value_list, sample_rate, sample_rate / render_quantum_frames as f64);
		}
		self.set_intrinsic_value(value);
		self.intrinsic_value
	}

	//==============================================================================
	// 프레임 범위의 값. (ValuesForFrameRange, 범위로 자른 뒤 마지막 값)
	//==============================================================================
	fn calculate_values_for_frame_range(&mut self, start_frame: usize, end_frame: usize, default_value: f32, value_list: &mut [f32], sample_rate: f64, control_rate: f64) -> f32 {
		self.calculate_values_for_frame_range_inner(start_frame, end_frame, default_value, value_list, sample_rate, control_rate);
		let (minimum_value, maximum_value) = (self.minimum_value, self.maximum_value);
		for value in value_list.iter_mut() {
			*value = value.clamp(minimum_value, maximum_value);
		}
		value_list.last().copied().unwrap_or(default_value)
	}

	//==============================================================================
	// ValuesForFrameRangeImpl.
	//==============================================================================
	fn calculate_values_for_frame_range_inner(&mut self, start_frame: usize, end_frame: usize, mut default_value: f32, value_list: &mut [f32], sample_rate: f64, control_rate: f64) {
		if self.event_list.is_empty() || end_frame as f64 / sample_rate <= self.event_list[0].time {
			value_list.fill(default_value);
			return;
		}
		let event_count = self.event_list.len();
		if self.event_list.iter().any(|event| event.is_new) {
			self.clamp_new_events_to_current_time(start_frame as f64 / sample_rate);
		}
		if self.handle_all_events_in_the_past(start_frame as f64 / sample_rate, sample_rate, &mut default_value, value_list) {
			return;
		}
		let (mut current_frame, mut write_index) = self.handle_first_event(value_list, default_value, start_frame, end_frame, sample_rate);
		let mut value = default_value;
		let mut skipped_event_count = 0;
		let mut event_index = 0;
		while event_index < event_count && write_index < value_list.len() {
			let next_event = self.event_list.get(event_index + 1).cloned();
			if !is_event_current(&self.event_list[event_index], next_event.as_ref(), current_frame as f64, sample_rate) {
				skipped_event_count += 1;
				event_index += 1;
				continue;
			}
			self.process_set_target_followed_by_ramp(event_index, next_event.as_ref(), current_frame, sample_rate, control_rate, &mut value);
			let event = self.event_list[event_index].clone();
			let value1 = event.value;
			let time1 = event.time;
			let value2 = next_event.as_ref().map(|next_event| next_event.value).unwrap_or(value1);
			let time2 = next_event.as_ref().map(|next_event| next_event.time).unwrap_or(end_frame as f64 / sample_rate + 1.0);
			let next_event_type = next_event.as_ref().map(|next_event| next_event.event_type);
			let fill_to_end_frame = if end_frame as f64 > time2 * sample_rate { (time2 * sample_rate).ceil() as usize } else { end_frame };
			let fill_to_frame = (fill_to_end_frame - start_frame).min(value_list.len());
			match next_event_type {
				Some(ParamEventType::LinearRampToValue) => {
					(current_frame, value, write_index) = process_linear_ramp(fill_to_frame, time1, time2, value1, value2, sample_rate, value_list, current_frame, value, write_index);
				}
				Some(ParamEventType::ExponentialRampToValue) => {
					(current_frame, value, write_index) = process_exponential_ramp(fill_to_frame, time1, time2, value1, value2, sample_rate, value_list, current_frame, write_index);
				}
				_ => {
					match event.event_type {
						ParamEventType::SetValue | ParamEventType::LinearRampToValue | ParamEventType::ExponentialRampToValue => {
							current_frame = fill_to_end_frame;
							value = event.value;
							value_list[write_index..fill_to_frame].fill(value);
							write_index = fill_to_frame;
						}
						ParamEventType::SetTarget => {
							(current_frame, value, write_index) = process_set_target(fill_to_frame, time1, value1, sample_rate, control_rate, fill_to_end_frame, &event, value_list, current_frame, value, write_index);
						}
					}
				}
			}
			event_index += 1;
		}
		if skipped_event_count > 0 {
			self.remove_old_events(skipped_event_count);
		}
		value_list[write_index..].fill(value);
	}

	//==============================================================================
	// 새 사건을 지금 시각으로 당기기. (ClampNewEventsToCurrentTime)
	//==============================================================================
	fn clamp_new_events_to_current_time(&mut self, current_time: f64) {
		let mut is_clamped = false;
		for event in self.event_list.iter_mut() {
			if event.is_new {
				if event.time < current_time {
					event.time = current_time;
					is_clamped = true;
				}
				event.is_new = false;
			}
		}
		if is_clamped {
			self.event_list.sort_by(|left, right| left.time.partial_cmp(&right.time).unwrap_or(std::cmp::Ordering::Equal));
		}
	}

	//==============================================================================
	// 모든 사건이 지난 경우. (HandleAllEventsInThePast)
	//==============================================================================
	fn handle_all_events_in_the_past(&mut self, current_time: f64, sample_rate: f64, default_value: &mut f32, value_list: &mut [f32]) -> bool {
		let Some(last_event) = self.event_list.last() else {
			return false;
		};
		if last_event.time + 1.5 * super::mixer::RENDER_QUANTUM_FRAMES as f64 / sample_rate < current_time {
			if last_event.event_type == ParamEventType::SetTarget {
				if has_set_target_converged(*default_value, last_event.value, current_time, last_event.time, last_event.time_constant) {
					*default_value = last_event.value;
				}
				else {
					return false;
				}
			}
			value_list.fill(*default_value);
			let event_count = self.event_list.len();
			self.remove_old_events(event_count);
			return true;
		}
		false
	}

	//==============================================================================
	// 첫 사건 앞을 채우기. (HandleFirstEvent)
	//==============================================================================
	fn handle_first_event(&self, value_list: &mut [f32], default_value: f32, start_frame: usize, end_frame: usize, sample_rate: f64) -> (usize, usize) {
		let mut current_frame = start_frame;
		let mut write_index = 0;
		let first_event_time = self.event_list[0].time;
		if first_event_time > start_frame as f64 / sample_rate {
			let mut fill_to_end_frame = end_frame;
			let first_event_frame = (first_event_time * sample_rate).ceil();
			if end_frame as f64 > first_event_frame {
				fill_to_end_frame = first_event_frame as usize;
			}
			let fill_to_frame = (fill_to_end_frame - start_frame).min(value_list.len());
			value_list[write_index..fill_to_frame].fill(default_value);
			write_index = fill_to_frame;
			current_frame += fill_to_frame;
		}
		(current_frame, write_index)
	}

	//==============================================================================
	// 목표 따라가기 뒤에 램프가 오면 지금 값으로 SetValue 를 둔다. (ProcessSetTargetFollowedByRamp)
	//==============================================================================
	fn process_set_target_followed_by_ramp(&mut self, event_index: usize, next_event: Option<&ParamEvent>, current_frame: usize, sample_rate: f64, control_rate: f64, value: &mut f32) {
		let event = &self.event_list[event_index];
		let Some(next_event) = next_event else {
			return;
		};
		if event.event_type != ParamEventType::SetTarget || !(next_event.event_type == ParamEventType::LinearRampToValue || next_event.event_type == ParamEventType::ExponentialRampToValue) {
			return;
		}
		let start_time = (current_frame as f64 / sample_rate).clamp(event.time, next_event.time);
		if (2.0 * sample_rate * event.time - 2.0 * current_frame as f64 + 1.0).abs() <= 1.0 {
			*value = (event.value as f64 + (*value - event.value) as f64 * (-(start_time - event.time) / event.time_constant).exp()) as f32;
		}
		else {
			let discrete_time_constant = discrete_time_constant_for_sample_rate(event.time_constant, control_rate) as f32;
			*value += (event.value - *value) * discrete_time_constant;
		}
		self.event_list[event_index] = ParamEvent {
			is_new: false,
			..ParamEvent::new(ParamEventType::SetValue, *value, start_time)
		};
	}
}

//==============================================================================
// 지금 쓸 사건인지. (IsEventCurrent)
//==============================================================================
fn is_event_current(event: &ParamEvent, next_event: Option<&ParamEvent>, current_frame: f64, sample_rate: f64) -> bool {
	let Some(next_event) = next_event else {
		return true;
	};
	if next_event.time * sample_rate >= current_frame {
		return true;
	}
	let event_frame = event.time * sample_rate;
	event.event_type == ParamEventType::SetValue && event_frame <= current_frame && current_frame < event_frame + 1.0
}

//==============================================================================
// 목표에 다 왔는지. (HasSetTargetConverged)
//==============================================================================
fn has_set_target_converged(value: f32, target: f32, current_time: f64, start_time: f64, time_constant: f64) -> bool {
	if current_time > start_time + TIME_CONSTANTS_TO_CONVERGE as f64 * time_constant {
		return true;
	}
	if target == 0.0 && value.abs() < SET_TARGET_THRESHOLD {
		return true;
	}
	if target != 0.0 && (target - value).abs() < SET_TARGET_THRESHOLD * value.abs() {
		return true;
	}
	false
}

//==============================================================================
// 이산 시간 상수. (audio_utilities::DiscreteTimeConstantForSampleRate, 1 - exp(-1 / (표본율 x 시간 상수)))
//==============================================================================
fn discrete_time_constant_for_sample_rate(time_constant: f64, sample_rate: f64) -> f64 {
	1.0 - (-1.0 / (sample_rate * time_constant)).exp()
}

//==============================================================================
// 선형 램프. (ProcessLinearRamp, x86_64 는 크롬의 SSE 4 줄 묶음과 같게)
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn process_linear_ramp(fill_to_frame: usize, time1: f64, time2: f64, value1: f32, value2: f32, sample_rate: f64, value_list: &mut [f32], mut current_frame: usize, mut value: f32, write_index: usize) -> (usize, f32, usize) {
	// x86_64 의 4 줄 묶음 길만 앞으로 옮긴다.
	#[cfg_attr(not(target_arch = "x86_64"), allow(unused_mut))]
	let mut write_index = write_index;
	let delta_time = time2 - time1;
	let k: f32 = if delta_time <= f32::MIN_POSITIVE as f64 { 0.0 } else { (1.0 / delta_time) as f32 };
	let value_delta = value2 - value1;
	#[cfg(target_arch = "x86_64")]
	{
		if fill_to_frame > write_index {
			let inverse_sample_rate = (1.0 / sample_rate) as f32;
			let offset = (current_frame as f64 / sample_rate - time1) as f32;
			let scale = k * value_delta;
			let mut lane_value_list = [0.0f32; 4];
			for (lane, lane_value) in lane_value_list.iter_mut().enumerate() {
				*lane_value = (inverse_sample_rate * lane as f32 + offset) * scale + value1;
			}
			let increment = (4.0 / sample_rate * k as f64 * value_delta as f64) as f32;
			let truncated_fill_to_frame = write_index + ((fill_to_frame - write_index) / 4) * 4;
			current_frame += truncated_fill_to_frame - write_index;
			while write_index < truncated_fill_to_frame {
				value_list[write_index..write_index + 4].copy_from_slice(&lane_value_list);
				for lane_value in lane_value_list.iter_mut() {
					*lane_value += increment;
				}
				write_index += 4;
			}
		}
		if write_index >= 1 {
			value = value_list[write_index - 1];
		}
	}
	for slot in value_list[write_index..fill_to_frame].iter_mut() {
		let x = ((current_frame as f64 / sample_rate - time1) * k as f64) as f32;
		value = value1 + x * value_delta;
		current_frame += 1;
		*slot = value;
	}
	(current_frame, value, fill_to_frame)
}

//==============================================================================
// 지수 램프. (ProcessExponentialRamp)
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn process_exponential_ramp(fill_to_frame: usize, time1: f64, time2: f64, value1: f32, value2: f32, sample_rate: f64, value_list: &mut [f32], mut current_frame: usize, write_index: usize) -> (usize, f32, usize) {
	let mut write_index = write_index;
	let mut value;
	if value1 * value2 <= 0.0 || time1 >= time2 {
		value = value1;
		value_list[write_index..fill_to_frame].fill(value);
		write_index = fill_to_frame;
	}
	else {
		let delta_time = time2 - time1;
		let sample_frame_count = delta_time * sample_rate;
		let multiplier = (value2 as f64 / value1 as f64).powf(1.0 / sample_frame_count);
		value = (value1 as f64 * (value2 as f64 / value1 as f64).powf((current_frame as f64 / sample_rate - time1) / delta_time)) as f32;
		let mut accumulator = value as f64;
		for slot in value_list[write_index..fill_to_frame].iter_mut() {
			value = accumulator as f32;
			accumulator *= multiplier;
			current_frame += 1;
			*slot = value;
		}
		write_index = fill_to_frame;
		if current_frame as f64 > time2 * sample_rate - 0.5 {
			value = value2;
		}
	}
	(current_frame, value, write_index)
}

//==============================================================================
// 목표 따라가기. (ProcessSetTarget, x86_64 는 크롬의 SSE 4 줄 묶음과 같게)
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn process_set_target(fill_to_frame: usize, time1: f64, value1: f32, sample_rate: f64, control_rate: f64, fill_to_end_frame: usize, event: &ParamEvent, value_list: &mut [f32], mut current_frame: usize, mut value: f32, mut write_index: usize) -> (usize, f32, usize) {
	let target = value1;
	let time_constant = event.time_constant as f32;
	let discrete_time_constant = discrete_time_constant_for_sample_rate(time_constant as f64, control_rate) as f32;
	let ramp_start_frame = time1 * sample_rate;
	if ramp_start_frame <= current_frame as f64 && (current_frame as f64) < ramp_start_frame + 1.0 {
		value = (target as f64 + (value - target) as f64 * (-(current_frame as f64 / sample_rate - time1) / time_constant as f64).exp()) as f32;
	}
	else {
		value += (target - value) * discrete_time_constant;
	}
	if has_set_target_converged(value, target, current_frame as f64 / sample_rate, time1, time_constant as f64) {
		current_frame += fill_to_frame - write_index;
		value_list[write_index..fill_to_frame].fill(target);
		write_index = fill_to_frame;
	}
	else {
		#[cfg(target_arch = "x86_64")]
		{
			if fill_to_frame > write_index {
				let c0 = discrete_time_constant;
				let c1 = c0 * (2.0 - c0);
				let c2 = c0 * ((c0 - 3.0) * c0 + 3.0);
				let c3 = c0 * (c0 * ((4.0 - c0) * c0 - 6.0) + 4.0);
				let coefficient_list = [0.0f32, c0, c1, c2];
				let truncated_fill_to_frame = write_index + ((fill_to_frame - write_index) / 4) * 4;
				while write_index < truncated_fill_to_frame {
					let delta = target - value;
					for lane in 0..4 {
						value_list[write_index + lane] = value + delta * coefficient_list[lane];
					}
					value += delta * c3;
					write_index += 4;
				}
			}
		}
		for slot in value_list[write_index..fill_to_frame].iter_mut() {
			let current_value = value;
			value += (target - value) * discrete_time_constant;
			*slot = current_value;
		}
		write_index = fill_to_frame;
		if write_index >= 1 {
			value = value_list[write_index - 1];
		}
		current_frame = fill_to_end_frame;
	}
	(current_frame, value, write_index)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// setValueAtTime 뒤 linearRampToValueAtTime: 선형으로 내려가고 끝에서 멈춘다.
	//==============================================================================
	#[test]
	fn linear_ramp_follows_chromium() {
		let mut audio_param = AudioParam::new(1.0, f32::MIN, f32::MAX);
		audio_param.apply_automation(ParamAutomation::SetValueAtTime { value: 1.0, time: 0.0 }, 0, 100.0);
		audio_param.apply_automation(ParamAutomation::LinearRampToValueAtTime { value: 0.0, time: 1.0 }, 0, 100.0);
		let mut value_list = [0.0f32; 128];
		assert!(audio_param.has_sample_accurate_values(0, 100.0, 128));
		audio_param.calculate_sample_accurate_values(&mut value_list, 0, 100.0);
		assert_eq!(value_list[0], 1.0);
		assert!((value_list[50] - 0.5).abs() < 1e-6);
		assert_eq!(value_list[127], 0.0);
		assert_eq!(audio_param.get_intrinsic_value(), 0.0);
	}

	//==============================================================================
	// setTargetAtTime: 시간 상수 하나 뒤 1/e 만큼 남는다. value 대입은 지금 SetValue 를 하나 넣는다.
	//==============================================================================
	#[test]
	fn set_target_and_value_setter() {
		let mut audio_param = AudioParam::new(1.0, f32::MIN, f32::MAX);
		audio_param.apply_automation(ParamAutomation::SetTargetAtTime { target: 0.0, time: 0.0, time_constant: 0.5 }, 0, 100.0);
		let mut value_list = [0.0f32; 128];
		audio_param.calculate_sample_accurate_values(&mut value_list, 0, 100.0);
		assert_eq!(value_list[0], 1.0);
		assert!((value_list[50] - (-1.0f32).exp()).abs() < 1e-3);
		audio_param.apply_automation(ParamAutomation::SetValue { value: 0.25 }, 128, 100.0);
		assert_eq!(audio_param.get_intrinsic_value(), 0.25);
		audio_param.calculate_sample_accurate_values(&mut value_list, 128, 100.0);
		assert!(value_list.iter().all(|value| *value == 0.25));
	}
}

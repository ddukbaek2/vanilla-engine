//==============================================================================
//! 소리 스레드 우선순위. (desktop.rs 의 출력 콜백이 그 스레드의 첫 부름에서 한 번 부른다)
//! - 원본 크롬은 소리를 그리는 스레드를 ThreadType::kRealtimeAudio 로 둔다. 엔진은 cpal 콜백 스레드가 크롬의 그 스레드(장치 스레드와 WebAudio 그리기)를 맡아 같은 일을 운영체제 API 로 직접 한다.
//! - 윈도우: 크롬 media/audio/win/audio_low_latency_output_win.cc 의 wasapi_render_thread 는 kRealtimeAudio(base/threading/platform_thread_win.cc, SetThreadPriority THREAD_PRIORITY_TIME_CRITICAL)로 짓고
//!   WASAPIAudioOutputStream::Run 에서 MMCSS AvSetMmThreadCharacteristics(L"Pro Audio") 와 AvSetMmThreadPriority(AVRT_PRIORITY_CRITICAL) 를 건다. 스레드가 끝날 때 AvRevertMmThreadCharacteristics 로 되돌린다.
//! - 리눅스: 크롬 base/threading/platform_thread_linux.cc SetThreadTypeLinux 는 sched_setscheduler(SCHED_RR, 우선순위 8 (platform_thread.h kRealTimeAudioPrio)) 를 걸고,
//!   안 되면(RLIMIT_RTPRIO 가 0 이고 root 가 아님) setpriority 로 nice -10 을 건다. (크롬 OS 의 cgroup 옮기기는 크롬 자신의 cgroup 관리라 하지 않는다)
//! - 맥: 크롬 base/threading/platform_thread_apple.mm SetPriorityRealtimeAudio 는 thread_policy_set 으로 THREAD_EXTENDED_POLICY(timeshare 0), THREAD_PRECEDENCE_POLICY(importance 63),
//!   THREAD_TIME_CONSTRAINT_POLICY 를 차례로 건다. 맥에서 켜진 kOptimizedRealtimeThreadingMac 의 기본 값으로 주기는 버퍼 길이(2.9ms 보다 짧으면 2.9ms),
//!   constraint 는 주기 x 1.0, computation 은 주기 x 0.5, preemptible 은 참이다. (주기는 media/audio/audio_device_thread.cc GetRealtimePeriod 의 buffer_duration)
//!   크롬도 CoreAudio 가 지은 출력 IO 스레드(AUHAL 콜백)에는 걸지 않는다. 그 스레드는 CoreAudio 가 이미 시간 제약 정책으로 두므로, 이미 시간 제약 정책인 스레드는 그대로 두고 아닌 스레드에만 건다.
//! - 실패는 크롬처럼 조용히 넘어간다. (부하가 높으면 커널이 거절할 수 있다)
//==============================================================================

//==============================================================================
/// 지금 스레드를 소리 스레드 우선순위로 올린다. (buffer_frames 는 콜백 한 번의 프레임 수)
//==============================================================================
pub(super) fn promote_current_thread(buffer_frames: usize, sample_rate: u32) {
	platform::promote_current_thread(buffer_frames, sample_rate);
}

#[cfg(target_os = "windows")]
mod platform {
	use windows_sys::Win32::Foundation::HANDLE;
	use windows_sys::Win32::System::Threading::{AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, AvSetMmThreadPriority, GetCurrentThread, SetThreadPriority, AVRT_PRIORITY_CRITICAL, THREAD_PRIORITY_TIME_CRITICAL};

	/// MMCSS 손잡이. (스레드가 끝날 때 되돌린다)
	struct MmcssTask {
		handle: HANDLE,
	}

	impl Drop for MmcssTask {
		//==============================================================================
		/// MMCSS 작업을 풀어 놓는다.
		//==============================================================================
		fn drop(&mut self) {
			unsafe {
				AvRevertMmThreadCharacteristics(self.handle);
			}
		}
	}

	thread_local! {
		static MMCSS_TASK: std::cell::RefCell<Option<MmcssTask>> = const { std::cell::RefCell::new(None) };
	}

	//==============================================================================
	/// 지금 스레드의 우선순위를 오디오용으로 올린다. (이 운영체제에서는 할 일이 없거나 해당 호출을 쓴다)
	//==============================================================================
	pub(super) fn promote_current_thread(_buffer_frames: usize, _sample_rate: u32) {
		unsafe {
			SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
		}
		let task_name: Vec<u16> = "Pro Audio".encode_utf16().chain(std::iter::once(0)).collect();
		let mut task_index = 0u32;
		let handle = unsafe { AvSetMmThreadCharacteristicsW(task_name.as_ptr(), &mut task_index) };
		if handle == 0 {
			return;
		}
		unsafe {
			AvSetMmThreadPriority(handle, AVRT_PRIORITY_CRITICAL);
		}
		MMCSS_TASK.with(|task| {
			*task.borrow_mut() = Some(MmcssTask { handle });
		});
	}
}

#[cfg(target_os = "linux")]
mod platform {
	/// struct sched_param. (glibc, musl 모두 int 하나)
	#[repr(C)]
	struct SchedParam {
		sched_priority: i32,
	}

	const SCHED_RR: i32 = 2;
	const PRIO_PROCESS: i32 = 0;
	/// 크롬 PlatformThreadLinux::kRealTimeAudioPrio.
	const REALTIME_AUDIO_PRIORITY: i32 = 8;
	/// 크롬 ThreadTypeToNiceValue(kRealtimeAudio).
	const REALTIME_AUDIO_NICE: i32 = -10;

	unsafe extern "C" {
		fn sched_setscheduler(pid: i32, policy: i32, param: *const SchedParam) -> i32;
		fn setpriority(which: i32, who: u32, priority: i32) -> i32;
	}

	//==============================================================================
	/// 지금 스레드의 우선순위를 오디오용으로 올린다. (이 운영체제에서는 할 일이 없거나 해당 호출을 쓴다)
	//==============================================================================
	pub(super) fn promote_current_thread(_buffer_frames: usize, _sample_rate: u32) {
		// pid, who 가 0 이면 리눅스는 부른 스레드에 건다. (크롬도 지금 스레드는 0 으로 부른다)
		let parameter = SchedParam { sched_priority: REALTIME_AUDIO_PRIORITY };
		if unsafe { sched_setscheduler(0, SCHED_RR, &parameter) } == 0 {
			return;
		}
		unsafe {
			setpriority(PRIO_PROCESS, 0, REALTIME_AUDIO_NICE);
		}
	}
}

#[cfg(target_os = "macos")]
mod platform {
	type MachPort = u32;

	#[repr(C)]
	struct MachTimebaseInfo {
		numer: u32,
		denom: u32,
	}

	const THREAD_EXTENDED_POLICY: u32 = 1;
	const THREAD_TIME_CONSTRAINT_POLICY: u32 = 2;
	const THREAD_PRECEDENCE_POLICY: u32 = 3;
	/// 크롬 GetTimeConstraints 의 가장 짧은 주기. (2.9ms, 44.1kHz 의 128 프레임쯤)
	const MINIMUM_PERIOD_NANOSECONDS: f64 = 2_900_000.0;
	/// kOptimizedRealtimeThreadingMac 의 busy, busy_limit 기본 값.
	const BUSY: f64 = 0.5;
	const BUSY_LIMIT: f64 = 1.0;

	unsafe extern "C" {
		fn pthread_self() -> *mut std::ffi::c_void;
		fn pthread_mach_thread_np(thread: *mut std::ffi::c_void) -> MachPort;
		fn thread_policy_set(thread: MachPort, flavor: u32, policy_info: *mut i32, count: u32) -> i32;
		fn thread_policy_get(thread: MachPort, flavor: u32, policy_info: *mut i32, count: *mut u32, get_default: *mut i32) -> i32;
		fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
	}

	//==============================================================================
	/// 지금 스레드의 우선순위를 오디오용으로 올린다. (이 운영체제에서는 할 일이 없거나 해당 호출을 쓴다)
	//==============================================================================
	pub(super) fn promote_current_thread(buffer_frames: usize, sample_rate: u32) {
		let thread = unsafe { pthread_mach_thread_np(pthread_self()) };
		// [period, computation, constraint, preemptible] (thread_time_constraint_policy_data_t)
		let mut constraints = [0i32; 4];
		let mut count = 4u32;
		let mut get_default = 0i32;
		let result = unsafe { thread_policy_get(thread, THREAD_TIME_CONSTRAINT_POLICY, constraints.as_mut_ptr(), &mut count, &mut get_default) };
		if result == 0 && get_default == 0 {
			// CoreAudio IO 스레드처럼 이미 시간 제약 정책이다
			return;
		}
		let mut timeshare = [0i32; 1];
		if unsafe { thread_policy_set(thread, THREAD_EXTENDED_POLICY, timeshare.as_mut_ptr(), 1) } != 0 {
			return;
		}
		let mut importance = [63i32; 1];
		if unsafe { thread_policy_set(thread, THREAD_PRECEDENCE_POLICY, importance.as_mut_ptr(), 1) } != 0 {
			return;
		}
		let mut timebase = MachTimebaseInfo { numer: 0, denom: 0 };
		unsafe {
			mach_timebase_info(&mut timebase);
		}
		if timebase.numer == 0 || sample_rate == 0 {
			return;
		}
		let period_nanoseconds = (buffer_frames as f64 * 1.0e9 / sample_rate as f64).max(MINIMUM_PERIOD_NANOSECONDS);
		let period = (period_nanoseconds * (timebase.denom as f64 / timebase.numer as f64)).min(u32::MAX as f64) as u32;
		let constraint = period.min((period as f64 * BUSY_LIMIT) as u32);
		let computation = constraint.min((period as f64 * BUSY) as u32);
		let mut policy = [period as i32, computation as i32, constraint as i32, 1];
		unsafe {
			thread_policy_set(thread, THREAD_TIME_CONSTRAINT_POLICY, policy.as_mut_ptr(), 4);
		}
	}
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod platform {
	//==============================================================================
	/// 지금 스레드의 우선순위를 오디오용으로 올린다. (이 운영체제에서는 할 일이 없거나 해당 호출을 쓴다)
	//==============================================================================
	pub(super) fn promote_current_thread(_buffer_frames: usize, _sample_rate: u32) {}
}

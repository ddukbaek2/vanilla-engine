# 소리

받은 소리 파일을 틀고, 배경음악과 효과음을 나눠 크기와 음소거를 다루는 방법을 봅니다. 소리 하나를 트는 `AudioPlayer`, 여러 소리를 한 길로 모으는 `AudioBus`, 소리 표로 배경음악과 효과음을 한꺼번에 다루는 `SoundSystem`, 같은 효과음을 겹쳐 트는 `SoundEffectPool`, 파일 없이 짧은 음을 합성하는 `BeepPlayer` 차례입니다. 소리 파일을 받는 `AudioAsset` 은 "애셋" 장에 있습니다.

모든 소리는 엔진의 `AudioManager` 가 가진 AudioContext 하나로 납니다. `engine.get_audio_manager()` 로 꺼냅니다. AudioContext 자리는 플랫폼마다 다릅니다.

- 데스크탑: cpal 로 기본 출력 장치를 열고, 그 표본율로 엔진의 믹서를 돌립니다. 믹서는 WebAudio 와 같은 노드 그래프(소스, GainNode, StereoPannerNode, 버스, 비프의 OscillatorNode)를 128 프레임씩 그리고, 셈은 크롬과 같습니다. 장치가 빠지면 같은 믹서로 기본 장치를 다시 엽니다.
- 웹: 원본과 같은 WebAudio AudioContext 입니다.

## AudioContext 와 첫 입력

`AudioManager` 는 AudioContext 를 처음 쓸 때 만듭니다. 엔진을 만든 직후에는 아직 없습니다.

```rust
use vanilla::{Engine, EngineConfiguration};

let engine = Engine::new(EngineConfiguration::new());
let audio_manager = engine.get_audio_manager();
assert!(!audio_manager.has_audio_context());
assert!(audio_manager.get_audio_context().is_none());
assert_eq!(audio_manager.get_context_time(), 0.0);
```

- `create_audio_player()` 와 `create_audio_bus()` 는 AudioContext 가 없으면 그 자리에서 만듭니다. 만들지 못하면 소리 없는 플레이어와 버스가 됩니다.
- `resume_context()` 는 첫 사용자 입력(user gesture) 전에는 아무것도 하지 않습니다. 그 뒤에는 AudioContext 가 없으면 만들고 멈춰 있으면 깨웁니다.
- 데스크탑에는 브라우저의 자동 재생 정책이 없어서 처음부터 사용자 입력이 있었던 것으로 둡니다. (`has_user_gesture()` 가 늘 true) 소리 장치는 플레이어나 버스를 처음 만들 때나 `resume_context()` 에서 엽니다. 출력 장치가 없으면 한 번 알리고 다시 열지 않습니다.
- 웹은 원본처럼 창의 `click`, `touchstart`, `keydown` 에서 엔진이 사용자 입력을 알리고(`mark_user_gesture`) AudioContext 를 깨웁니다. 창이 포커스를 얻거나 페이지가 다시 보일 때도 깨우기를 해 봅니다. (첫 입력 뒤에만 됩니다)
- `EngineConfiguration.resume_audio_on_touch_end` 를 true 로 두면 웹에서 `touchend` 와 `pointerup` 에서도 깨웁니다. 기본은 false 입니다.
- 웹의 iOS 에서는 깨운 AudioContext 가 "running" 이어도 0.5 초 동안 시계가 실제 시간의 1/4 보다 덜 흐르면 멈췄다가 다시 깨웁니다.
- `get_context_time()` 은 AudioContext 를 만든 뒤 흐른 소리 시간(초)이고, `is_context_running()` 은 AudioContext 가 있고 멈춰 있지 않은지입니다. `get_audio_context()` 는 빌린 AudioContext(`sample_rate()`, `current_time()` 이 있습니다)를 돌려주고, 빌린 동안에는 매니저의 다른 함수를 부르지 않습니다.
- `AudioManager` 는 나눠 든 상태의 손잡이라 메서드가 모두 `&self` 입니다. `clone()` 하면 같은 AudioContext 를 보는 손잡이가 하나 더 생깁니다.

```rust,no_run
use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, Vector2};

struct MainScene {
	scene_base: SceneBase,
}

impl Scene for MainScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn touch_press(&mut self, engine: &mut Engine, _view_input_position: Vector2) {
		let audio_manager = engine.get_audio_manager();
		println!("{} {}", audio_manager.has_user_gesture(), audio_manager.get_context_time());
	}
}

fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.resume_audio_on_touch_end = true;
	let engine = Engine::new(engine_configuration);
	let main_scene = MainScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(main_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

## AudioPlayer 로 소리 하나 틀기

`AudioPlayer` 는 `AudioAsset` 하나를 틉니다. `set_audio_asset` 으로 애셋을 싣고 `play(is_loop)` 로 틉니다. 아래 예제는 `create` 에서 소리를 받아 씬에 맡기고, 처음 누를 때 플레이어를 만들어 누를 때마다 틉니다.

저장소에는 예제용 소리 파일이 따로 없어서 이 장의 예제는 해석기 시험 파일(`tests/assets/audio` 아래, 모두 0.5 초 안쪽의 짧은 소리)을 씁니다. 게임에서는 애셋 폴더에 둔 파일 경로를 줍니다.

```rust,no_run
use std::rc::Rc;
use vanilla::{AudioAsset, AudioPlayer, Engine, EngineConfiguration, Scene, SceneBase, Vector2};

struct SoundScene {
	scene_base: SceneBase,
	audio_asset: Option<Rc<AudioAsset>>,
	audio_player: Option<AudioPlayer>,
}

impl Scene for SoundScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		match engine.load_audio_asset("tests/assets/audio/vorbis_stereo.ogg") {
			Ok(audio_asset) => {
				self.scene_base.track_audio_asset(&audio_asset);
				self.audio_asset = Some(audio_asset);
			}
			Err(error) => {
				eprintln!("{}", error);
			}
		}
	}

	fn touch_press(&mut self, engine: &mut Engine, _view_input_position: Vector2) {
		if self.audio_player.is_none() {
			if let Some(audio_asset) = &self.audio_asset {
				let mut audio_player = engine.get_audio_manager().create_audio_player();
				audio_player.set_audio_asset(audio_asset.clone());
				audio_player.set_volume(0.8);
				self.audio_player = Some(audio_player);
			}
		}
		if let Some(audio_player) = self.audio_player.as_mut() {
			audio_player.play(false);
		}
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let sound_scene = SoundScene {
		scene_base: SceneBase::new(),
		audio_asset: None,
		audio_player: None,
	};
	if let Err(error) = engine.run(Box::new(sound_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

- `engine.load_audio_asset(경로)` 는 `Rc<AudioAsset>` 을 돌려줍니다. 데스크탑은 그 자리에서 읽고 디코딩하고 실패하면 `Err` 입니다. 웹은 받기와 디코딩을 걸어 두고 바로 돌아오므로, `scene_base.track_audio_asset` 으로 맡겨 디코딩이 끝난 뒤 `initialize` 가 불리게 합니다. 디코딩이 끝나기 전에 플레이어에 실어도 되고, 그동안 `play` 는 아무것도 하지 않습니다.
- `play(is_loop)` 는 이미 나고 있으면 끊고 처음부터(또는 `set_time` 으로 정한 자리부터) 다시 틉니다. AudioContext 가 멈춰 있으면 깨운 뒤 틉니다. (웹은 깨우기를 기다렸다가, 데스크탑은 그 자리에서) 소리 없는 플레이어이거나 애셋이 없거나 아직 읽히지 않았으면 아무것도 하지 않습니다.
- `stop()`, `is_playing()`, `get_duration()`(초, 애셋이 없거나 읽히지 않았으면 0) 이 있습니다.
- `set_volume(volume)` 은 0 ~ 1 로 자르고, 유한한 수가 아니면 1 입니다. `mute()` 하면 크기가 0 이 되고 `unmute()` 하면 설정한 크기로 돌아갑니다. 음소거 중에 바꾼 크기는 음소거를 풀 때 걸립니다. 소리 없는 플레이어에는 음소거가 걸리지 않습니다. (`is_muted()` 가 false)
- `set_playback_rate(playback_rate)` 는 빠르기와 음 높이를 함께 바꿉니다. 다음 `play` 부터 걸리고, 0 이하이거나 유한한 수가 아니면 1 입니다.
- `set_time(time)` 은 `play` 가 시작할 자리(초)를 정합니다. 나고 있는 중이면 그 자리부터 바로 다시 틉니다. `get_time()` 은 정한 시작 자리를 돌려주고, 재생하면서 늘지는 않습니다.
- `set_audio_asset` 은 나고 있는 소리를 바꾸지 않습니다. 다음 `play` 부터 그 애셋을 틉니다.
- 플레이어는 만든 쪽이 소유합니다. 플레이어를 버리면 나던 소리도 멈춥니다. (원본은 가비지 수거와 상관없이 소리가 끝까지 납니다) 위 예제처럼 씬의 필드에 들고 있습니다.
- `create_audio_player()` 는 `AudioPlayer::new(&audio_manager, None)` 과 같습니다. 둘째 인자는 소리를 내보낼 곳이고 None 이면 출력입니다.

## AudioBus 로 묶기

`AudioBus` 는 여러 플레이어의 소리를 한 길로 모아 크기와 음소거를 한꺼번에 겁니다. `audio_manager.create_audio_bus()` 로 만들고, 그 버스로 트는 플레이어는 `bus.create_audio_player()` 로 만듭니다. 크기와 음소거 규칙은 `AudioPlayer` 와 같습니다.

```rust,no_run
use std::rc::Rc;
use vanilla::{AudioAsset, AudioBus, AudioPlayer, Engine, EngineConfiguration, Scene, SceneBase, Vector2};

struct BusScene {
	scene_base: SceneBase,
	audio_asset: Option<Rc<AudioAsset>>,
	effect_bus: Option<AudioBus>,
	effect_player: Option<AudioPlayer>,
}

impl Scene for BusScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		if let Ok(audio_asset) = engine.load_audio_asset("tests/assets/audio/wav_s16_stereo.wav") {
			self.scene_base.track_audio_asset(&audio_asset);
			self.audio_asset = Some(audio_asset);
		}
	}

	fn touch_press(&mut self, engine: &mut Engine, _view_input_position: Vector2) {
		if self.effect_bus.is_none() {
			let effect_bus = engine.get_audio_manager().create_audio_bus();
			effect_bus.set_volume(0.5);
			let mut effect_player = effect_bus.create_audio_player();
			if let Some(audio_asset) = &self.audio_asset {
				effect_player.set_audio_asset(audio_asset.clone());
			}
			self.effect_bus = Some(effect_bus);
			self.effect_player = Some(effect_player);
		}
		if let Some(effect_player) = self.effect_player.as_mut() {
			effect_player.play(false);
		}
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let bus_scene = BusScene {
		scene_base: SceneBase::new(),
		audio_asset: None,
		effect_bus: None,
		effect_player: None,
	};
	if let Err(error) = engine.run(Box::new(bus_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

- `AudioBus` 도 나눠 든 상태의 손잡이라 메서드가 모두 `&self` 이고, `clone()` 한 손잡이는 같은 버스를 봅니다. 버스로 만든 플레이어는 버스 손잡이를 하나 들어 버스를 살려 둡니다. 손잡이를 모두 버리면 버스가 출력에서 떨어집니다.
- `is_available()` 은 AudioContext 가 있을 때 만든 버스인지 알려 줍니다. AudioContext 없이 만든 버스와 그 버스로 만든 플레이어는 소리를 내지 않습니다.
- `get_gain_node()` 는 버스의 GainNode 손잡이(소리 없는 버스면 None)를 돌려줍니다. 원본처럼 버스 노드를 내보낼 곳으로 넘겨 `AudioPlayer::new(&audio_manager, audio_bus.get_gain_node().as_deref())` 로 플레이어를 만들 수 있고, 이 꼴은 두 플랫폼에서 같습니다. 노드에 직접 건 크기는 버스의 크기 칸과 음소거 칸을 바꾸지 않습니다.

## SoundSystem 으로 배경음악과 효과음 다루기

게임 하나의 소리를 모두 맡는 타입입니다. 소리 표를 넘기면 배경음악 버스와 효과음 버스를 만들고, 설정 크기, 음소거, AudioContext 깨우기를 알아서 챙깁니다.

소리 표의 줄은 `SoundRow` 이고 칸은 모두 공개 필드입니다.

| 칸 | 뜻 |
| --- | --- |
| `id` | 소리 이름 |
| `paths` | 같은 소리를 여러 번 녹음한 무리. 효과음은 낼 때마다 받아 둔 것 가운데 하나를 고르고 바로 앞과 같은 것은 고르지 않습니다. 배경음악은 첫 파일만 씁니다 |
| `volume` | 소리마다의 크기 (0 ~ 1) |
| `is_music` | 배경음악이면 true |
| `minimum_interval` | 같은 효과음을 다시 낼 수 있는 간격 (초) |
| `maximum_voices` | 같은 효과음이 한꺼번에 날 수 있는 수. 넘으면 가장 오래된 것을 끊습니다. None 이면 효과음 전체 상한(12)까지, `Some(0)` 은 하나만 납니다 |
| `pitch_variation` | 낼 때마다 음 높이를 흔드는 비. 0.05 면 위아래 5% |
| `play_seconds`, `fade_out_seconds` | 효과음의 앞 `play_seconds` 초만 틀고 `fade_out_seconds` 초 동안 줄여 끕니다. 0 이면 끝까지 |

`SoundRow::create_effect(id, paths, volume)` 과 `SoundRow::create_music(id, path, volume)` 은 나머지 칸을 기본값(간격 0, 상한 None, 흔들기 0, 끝까지)으로 둔 줄을 만듭니다. 바꿀 칸은 만든 뒤 필드에 넣습니다.

```rust
use vanilla::SoundRow;

let music_row = SoundRow::create_music("music", "tests/assets/audio/vorbis_stereo.ogg", 0.4);
let mut click_row = SoundRow::create_effect("click", &["tests/assets/audio/wav_s16_stereo.wav", "tests/assets/audio/flac_stereo48.flac"], 1.0);
click_row.minimum_interval = 0.05;
click_row.maximum_voices = Some(4);
click_row.pitch_variation = 0.05;
assert!(music_row.is_music);
assert_eq!(music_row.paths.len(), 1);
assert!(!click_row.is_music);
assert_eq!(click_row.paths.len(), 2);
assert_eq!(click_row.play_seconds, 0.0);
assert_eq!(click_row.maximum_voices, Some(4));
```

아래 예제는 `create` 에서 소리 표로 `SoundSystem` 을 만들어 함께 쓰는 자리에 두고, 소리를 다 받기를 작업으로 기다린 뒤 `initialize` 에서 배경음악을 틉니다. 누르면 누른 자리의 좌우로 효과음을 냅니다.

```rust,no_run
use std::cell::RefCell;
use std::rc::Rc;
use vanilla::{read_sound_system, set_sound_system, Engine, EngineConfiguration, Scene, SceneBase, SceneContext, SoundRow, SoundSystem, Vector2};

fn create_sound_table() -> Vec<SoundRow> {
	let music_row = SoundRow::create_music("music", "tests/assets/audio/vorbis_stereo.ogg", 0.4);
	let mut click_row = SoundRow::create_effect("click", &["tests/assets/audio/wav_s16_stereo.wav"], 1.0);
	click_row.minimum_interval = 0.05;
	click_row.maximum_voices = Some(4);
	click_row.pitch_variation = 0.05;
	vec![music_row, click_row]
}

struct MusicScene {
	scene_base: SceneBase,
}

impl Scene for MusicScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		let mut sound_system = SoundSystem::new(engine, create_sound_table());
		sound_system.set_music_volume(0.8);
		sound_system.set_sound_effect_volume(1.0);
		let sound_load_handle = sound_system.load_sounds(engine);
		set_sound_system(Some(Rc::new(RefCell::new(sound_system))));
		let task_handle = engine.spawn_scene_task(self, |_scene_context: SceneContext<MusicScene>| async move {
			sound_load_handle.await;
		});
		self.scene_base.track_task(&task_handle);
	}

	fn initialize(&mut self, _engine: &mut Engine) {
		if let Some(sound_system) = read_sound_system() {
			sound_system.borrow_mut().play_music("music", 1.0, false);
		}
	}

	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.scene_base.tick(engine, time_delta);
		if let Some(sound_system) = read_sound_system() {
			sound_system.borrow_mut().update();
		}
	}

	fn touch_press(&mut self, engine: &mut Engine, view_input_position: Vector2) {
		let view_size = engine.get_view_manager().get_view_size();
		let pan = (view_input_position.x / view_size.x) * 2.0 - 1.0;
		if let Some(sound_system) = read_sound_system() {
			sound_system.borrow_mut().play_effect("click", 1.0, pan);
		}
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let music_scene = MusicScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(music_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

- 생성자는 `SoundSystem::new(engine, sound_table)` 입니다. 원본은 `AudioManager` 를 받지만 러스트 판은 엔진을 받습니다. 엔진의 `AudioManager` 손잡이를 나눠 들고, 페이지가 가려지거나 포커스를 잃는 일을 엔진의 페이지 이벤트(`PageEvent`)로 받기 때문입니다.
- 줄을 더 넣을 때는 `add_sounds(sound_table)`(같은 `id` 는 바꿈)를 쓰고, 파일로 받지 않는 소리는 `add_sound_asset(sound_id, audio_asset)` 으로 그 `id` 의 무리에 덧붙입니다. 크기와 간격은 같은 `id` 의 표 줄을 따르므로 줄이 있어야 납니다.
- `load_sounds(engine)` 는 효과음을 먼저, 배경음악을 나중에 받습니다. 경로는 애셋 폴더 기준이고, 데스크탑은 그 자리에서 읽습니다. 못 받은 파일은 알리기만 하고 건너뛰고, 이미 받은 줄은 다시 받지 않습니다. 돌려주는 `SoundLoadHandle` 은 걸어 둔 소리를 다 받거나 실패할 때까지 await 할 수 있습니다. (실패해도 거부되지 않습니다) `wait_for_load()` 도 같은 것을 기다리는 퓨처를 돌려줍니다. 줄 하나만 받을 때는 `load_sound(engine, &sound_row)` 입니다.
- 소리 받기는 놀이를 막지 않으므로 기다리지 않아도 됩니다. 받기 전에 부른 `play_music` 은 기억해 두었다가 받은 뒤에 틉니다.
- `update()` 는 매 프레임 직접 불러야 합니다. 엔진이 대신 부르지 않습니다. 누르기 전 깨워 보기(첫 번만), 첫 입력 뒤 버스 만들기, 음소거 따르기, AudioContext 멈추기와 다시 깨우기(0.5 초마다), 배경음악 틀기와 바꾸기, 끝난 소리 치우기가 여기서 일어납니다.
- `play_music(sound_id, fade_seconds, is_resumed)` 는 배경음악을 반복해 틉니다. `fade_seconds` 동안 앞 음악은 줄고 새 음악은 0 에서 오릅니다. (0 이면 바로 바꿈) `is_resumed` 가 true 면 그 음악을 멈췄던 자리에서 잇습니다. `""` 를 주면 끕니다. 지금 틀어 달라고 한 것과 같은 `id` 면 아무것도 하지 않습니다. 첫 파일을 받지 못했으면 틀지 않습니다. 원본과 달리 인자에 기본값이 없어 셋 다 줍니다.
- `play_effect(sound_id, volume, pan)` 은 효과음을 냅니다. `volume` 은 이번 크기(0 ~ 1), `pan` 은 좌우(-1 왼쪽 ~ 1 오른쪽, 0 가운데, 범위 밖은 자르고 유한한 수가 아니면 0)입니다. 첫 입력 전(버스가 없음), 음소거, AudioContext 가 돌지 않음, 모르는 `id`, 받기 전, 크기가 0.001 이하, `minimum_interval` 안이면 내지 않고 넘어갑니다. 효과음은 모두 합쳐 12 개까지 나고, 넘으면 가장 오래된 것을 끊습니다.
- `stop_effects()` 는 나는 효과음을 모두 끊고, `fade_out_effects(sound_id, fade_seconds)` 는 지금 크기에서 0 으로 줄여 끕니다. (`sound_id` 가 `""` 이면 모두, 초가 0 이면 바로 끊음) `get_effect_voice_count()` 는 나고 있는 효과음 수입니다. (줄여 끄는 중인 것은 빼고)
- `set_sound_system(Some(sound_system))` 과 `read_sound_system()` 은 함께 쓰는 `SoundSystem` 하나(`Rc<RefCell<SoundSystem>>`)를 담아 두는 모듈 함수입니다. 스레드마다 하나이고 `None` 을 주면 비웁니다. 씬이 바뀌어도 같은 소리를 쓸 때 편합니다.

### 크기

- 배경음악 크기는 `set_music_volume`(설정의 음악 크기) x `set_music_scale`(음악의 비) x 줄의 `volume` 입니다. 음악의 비는 멈춤 창이 떠 있는 동안 반으로 줄이는 것처럼 설정과 따로 줄일 때 씁니다.
- 효과음 크기는 `set_sound_effect_volume` x 줄의 `volume` x `play_effect` 의 `volume` 입니다.
- 모두 0 ~ 1 로 자르고 유한한 수가 아니면 0 입니다. 설정 크기는 버스에 걸고, 바꿀 때 시간 상수 0.05 초로 부드럽게 따라가 툭 끊기지 않습니다.
- `get_music_volume()`, `get_music_scale()`, `get_sound_effect_volume()`, `get_music_id()`(틀어 달라고 한 음악, 없으면 `""`)로 읽습니다.
- `get_music_bus()`, `get_effect_bus()` 로 버스를 빌릴 수 있습니다. (첫 입력 전에는 None) 효과음 버스로 만든 플레이어도 설정 크기와 음소거를 따릅니다. 설정 크기는 버스의 GainNode 에 직접 걸므로, 버스의 `set_volume`, `mute` 로 건 값은 `SoundSystem` 이 크기를 다시 걸 때까지만 갑니다.

### 음소거 규칙

음소거는 설정한 크기보다 먼저입니다. 아래 가운데 하나라도 맞으면 크기 설정과 상관없이 소리가 나지 않습니다.

- 바깥의 음소거. `set_external_mute_provider(Some(함수))` 로 준 함수가 true 를 돌려줄 때입니다. (플랫폼 SDK 의 음소거 설정, 보상형 광고를 보이는 동안 같은 것) 매 프레임 `update` 와 페이지 이벤트 때 읽습니다.
- 사용자의 음소거 단추. `set_user_muted(true)`.
- 페이지가 가려짐. 웹은 다른 탭이나 앱 전환(visibilitychange)이고, 데스크탑은 창이 가려졌다고 winit 이 알릴 때입니다. (맥과 X11 만 알리고 윈도우와 웨이랜드는 알리지 않습니다)
- 터치가 주된 기기에서 창이 포커스를 잃음. 아이폰은 페이지가 가려졌다는 알림이 앱이 내려간 뒤에 와서, 그보다 먼저 오는 포커스 잃음을 따릅니다. 따를지는 만들 때 정합니다. (`is_blur_mute_enabled()`, 웹은 `(pointer: coarse)` 가 맞을 때, 데스크탑은 윈도우의 태블릿 모드나 마우스 없는 터치 기기, 리눅스의 터치스크린만 있는 기기, 맥은 따르지 않음) 포커스가 돌아오거나 화면을 누르거나 페이지가 다시 보이면 풀립니다.

음소거가 되면 나는 효과음을 끊고, 배경음악은 소리 없이 계속 돌아 풀리면 이어 들립니다. 음소거인 동안은 AudioContext 도 멈추고 풀리면 깨웁니다. (데스크탑은 믹서가 그리기를 멈춰 소리와 시계가 그 자리에서 섭니다) 음소거가 아닌데 AudioContext 가 돌지 않으면 0.5 초마다 다시 깨워 봅니다. `is_user_muted()`, `is_page_hidden()`, `is_window_blurred()`, `is_context_paused()`, `read_muted()` 로 상태를 읽습니다.

`set_autoplay_allowed_provider(Some(함수))` 는 첫 입력 전에 한 번 깨워 봐도 되는지 정합니다. 주지 않으면(None) 허락으로 치고 첫 `update` 에서 한 번 깨워 봅니다. 웹뷰가 막는 플랫폼이면 false 를 돌려주는 함수를 줍니다. 막히면 엔진이 첫 입력에 깨웁니다.

아래 예제는 소리 장치 없이 음소거 판정만 봅니다. 광고 상태를 `Cell` 에 두고 바깥의 음소거 함수가 읽게 하며, 페이지 이벤트는 엔진에 바로 넣어 봅니다. (`dispatch_page_event` 는 보통 플랫폼이 부릅니다)

```rust
use std::cell::Cell;
use std::rc::Rc;
use vanilla::{Engine, EngineConfiguration, PageEvent, SoundRow, SoundSystem};

let mut engine = Engine::new(EngineConfiguration::new());
let sound_table = vec![SoundRow::create_effect("click", &["tests/assets/audio/wav_s16_stereo.wav"], 1.0)];
let mut sound_system = SoundSystem::new(&engine, sound_table);
let is_ad_showing = Rc::new(Cell::new(false));
let is_ad_showing_for_provider = is_ad_showing.clone();
sound_system.set_external_mute_provider(Some(Rc::new(move || is_ad_showing_for_provider.get())));
sound_system.set_autoplay_allowed_provider(Some(Rc::new(|| true)));
sound_system.set_music_scale(0.5);
assert_eq!(sound_system.get_music_scale(), 0.5);
assert!(!sound_system.read_muted());

is_ad_showing.set(true);
assert!(sound_system.read_muted());
is_ad_showing.set(false);

sound_system.set_user_muted(true);
assert!(sound_system.is_user_muted());
assert!(sound_system.read_muted());
sound_system.set_user_muted(false);

engine.dispatch_page_event(PageEvent::Hidden);
assert!(sound_system.is_page_hidden());
assert!(sound_system.read_muted());
engine.dispatch_page_event(PageEvent::Visible);
assert!(!sound_system.read_muted());
```

## SoundEffectPool

같은 효과음이 연달아 겹쳐 나도 끊기지 않게 `AudioPlayer` 여러 개를 돌려 가며 틉니다. `SoundEffectPool::new(audio_manager, voice_count)` 로 만들고, `play(&audio_asset, volume)` 은 다음 차례 플레이어에 애셋을 실어 틉니다. 그 플레이어가 나고 있었으면 끊고 새로 틉니다.

- 이번 크기(0 ~ 1)에 풀 전체 크기(`set_volume`)를 곱합니다. 풀 크기를 바꾸면 나고 있는 것에도 바로 곱해집니다. (유한한 수가 아니면 1)
- 음소거 중이거나 보이스가 0 이면 `play` 는 아무것도 하지 않고 차례도 넘기지 않습니다.
- `set_muted`, `is_muted`, `stop_all`, `get_voice_count`, `get_volume` 도 있습니다.
- 원본과 달리 보이스 수와 `play` 의 크기에 기본값이 없어 늘 줍니다. 원본의 기본 보이스 수 6 은 `vanilla::core::soundeffectpool::DEFAULT_VOICE_COUNT` 입니다.

생성자가 플레이어를 바로 만들므로 이때 AudioContext 가 생깁니다. (데스크탑은 소리 장치를 엽니다) 웹에서는 아래처럼 첫 입력 뒤에 만듭니다.

```rust,no_run
use std::rc::Rc;
use vanilla::{AudioAsset, Engine, EngineConfiguration, Scene, SceneBase, SoundEffectPool, Vector2};

struct PoolScene {
	scene_base: SceneBase,
	audio_asset: Option<Rc<AudioAsset>>,
	sound_effect_pool: Option<SoundEffectPool>,
}

impl Scene for PoolScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		if let Ok(audio_asset) = engine.load_audio_asset("tests/assets/audio/wav_s16_stereo.wav") {
			self.scene_base.track_audio_asset(&audio_asset);
			self.audio_asset = Some(audio_asset);
		}
	}

	fn touch_press(&mut self, engine: &mut Engine, _view_input_position: Vector2) {
		if self.sound_effect_pool.is_none() {
			let mut sound_effect_pool = SoundEffectPool::new(engine.get_audio_manager(), 6);
			sound_effect_pool.set_volume(0.8);
			self.sound_effect_pool = Some(sound_effect_pool);
		}
		if let (Some(sound_effect_pool), Some(audio_asset)) = (self.sound_effect_pool.as_mut(), &self.audio_asset) {
			sound_effect_pool.play(audio_asset, 0.5);
		}
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let pool_scene = PoolScene {
		scene_base: SceneBase::new(),
		audio_asset: None,
		sound_effect_pool: None,
	};
	if let Err(error) = engine.run(Box::new(pool_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

보이스가 0 인 풀은 플레이어를 만들지 않으므로 AudioContext 도 만들지 않고, 크기와 음소거만 듭니다.

```rust
use vanilla::{AudioManager, SoundEffectPool};

let audio_manager = AudioManager::new();
let mut sound_effect_pool = SoundEffectPool::new(&audio_manager, 0);
assert_eq!(sound_effect_pool.get_voice_count(), 0);
sound_effect_pool.set_volume(1.5);
assert_eq!(sound_effect_pool.get_volume(), 1.0);
sound_effect_pool.set_muted(true);
assert!(sound_effect_pool.is_muted());
assert!(!audio_manager.has_audio_context());
```

## BeepPlayer

소리 파일 없이 OscillatorNode 와 GainNode 로 UI 효과음을 합성합니다. 소리가 툭 튀지 않게 시작과 끝에 짧은 오르내림(시작 min(0.005, 길이 x 0.25) 초, 끝 min(0.02, 길이 x 0.5) 초)을 겁니다. 웹은 원본처럼 WebAudio 노드를 만들고, 데스크탑은 믹서에 같은 그래프를 지어 크로미엄과 같은 셈으로 냅니다.

- `BeepPlayer::new(audio_manager, volume)` 로 만듭니다. 원본의 기본 크기 0.35 는 `BeepPlayer::DEFAULT_VOLUME` 입니다. 생성자는 크기를 자르지 않고, `set_volume` 은 0 ~ 1 로 자릅니다.
- `play_preset(preset_name)` 의 이름은 `"click"`, `"confirm"`, `"cancel"`, `"error"`, `"success"`, `"warning"`, `"notification"` 입니다. 대소문자를 가리고, 모르는 이름이면 `"click"` 과 같은 소리를 냅니다.
- `play_tone(frequency, duration_seconds, waveform, delay_seconds, volume_scale)` 은 음 하나를 냅니다. 주파수는 Hz, 길이와 지연은 초이고, 가장 큰 크기는 기본 크기 x `volume_scale` 입니다. (0.0001 보다 작으면 0.0001) 원본과 달리 인자에 기본값이 없어 다섯 다 주고, 파형은 글 대신 `BeepWaveform::Sine`, `Square`, `Sawtooth`, `Triangle` 입니다.
- `play_sequence(&tone_list)` 는 `BeepTone` 목록을 차례로 잇습니다. 톤마다 앞 톤들의 길이 합만큼 늦춰 시작합니다. `BeepTone::create(frequency, duration)` 은 사인파, 배율 1 인 톤이고, `BeepTone::create_with_waveform(frequency, duration, waveform, volume_scale)` 은 파형과 배율까지 정합니다.
- `set_volume`, `get_volume`, `set_muted`, `is_muted` 가 있습니다.

```rust
use vanilla::{BeepTone, BeepWaveform};

let tone = BeepTone::create(523.0, 0.1);
assert_eq!(tone.waveform, BeepWaveform::Sine);
assert_eq!(tone.volume_scale, 1.0);

let triangle_tone = BeepTone::create_with_waveform(659.0, 0.1, BeepWaveform::Triangle, 1.0);
assert_eq!(triangle_tone.frequency, 659.0);
assert_eq!(triangle_tone.waveform, BeepWaveform::Triangle);
```

```rust,no_run
use vanilla::{BeepPlayer, BeepTone, BeepWaveform, Engine, EngineConfiguration, Scene, SceneBase, Vector2};

struct BeepScene {
	scene_base: SceneBase,
	beep_player: Option<BeepPlayer>,
}

impl Scene for BeepScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn create(&mut self, engine: &mut Engine) {
		self.beep_player = Some(BeepPlayer::new(engine.get_audio_manager(), 0.3));
	}

	fn touch_press(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
		if let Some(beep_player) = &self.beep_player {
			beep_player.play_preset("confirm");
			beep_player.play_sequence(&[
				BeepTone::create(523.0, 0.1),
				BeepTone::create_with_waveform(659.0, 0.1, BeepWaveform::Triangle, 1.0),
				BeepTone::create_with_waveform(784.0, 0.2, BeepWaveform::Sine, 0.6),
			]);
		}
	}
}

fn main() {
	let engine = Engine::new(EngineConfiguration::new());
	let beep_scene = BeepScene {
		scene_base: SceneBase::new(),
		beep_player: None,
	};
	if let Err(error) = engine.run(Box::new(beep_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
```

`BeepPlayer` 는 부를 때 AudioContext 깨우기를 해 보고(데스크탑은 이때 소리 장치를 엽니다), 돌고 있을 때만 소리를 냅니다. 음소거 중이면 깨우지도 않고 돌아갑니다. 소리를 버스를 거치지 않고 출력에 바로 내므로 `SoundSystem` 의 크기와 음소거를 따르지 않습니다. 만들기만 해서는 AudioContext 가 생기지 않으므로 `create` 에서 만들어도 됩니다.

## 주의할 점

- `AudioPlayer` 와 버스 손잡이를 버리면 소리가 멈춥니다. 소리를 내는 동안은 씬의 필드처럼 살아 있는 곳에 둡니다. `SoundSystem` 은 나는 소리의 플레이어를 스스로 들고 있습니다.
- `SoundSystem::update()` 를 부르지 않으면 버스가 만들어지지 않아 아무 소리도 나지 않습니다.
- 소리 표 줄의 `volume` 이 0 이면(크기가 0.001 이하이면) 그 효과음은 나지 않습니다. `create_effect`, `create_music` 에 크기를 꼭 줍니다.
- `audio_manager.create_audio_player()` 로 만든 플레이어, `SoundEffectPool`, `BeepPlayer` 는 출력에 바로 이어지므로 `SoundSystem` 의 설정 크기와 음소거를 따르지 않습니다. 따르게 하려면 `sound_system.get_effect_bus()` 의 버스로 `create_audio_player()` 를 부릅니다.
- 크기를 자르는 규칙이 둘입니다. `AudioPlayer`, `AudioBus`, `SoundEffectPool` 은 유한한 수가 아니면 1 로, `SoundSystem` 은 0 으로 칩니다.
- `AudioAsset` 은 첫 입력 전에 받아도 됩니다. 디코딩은 소리 장치를 잡지 않습니다. (데스크탑은 엔진의 해석기가 크롬 decodeAudioData 와 같은 결과로 풀고, 웹은 브라우저가 OfflineAudioContext 로, 없으면 잠시 쓰는 AudioContext 로 풉니다) 트는 것만 첫 입력 뒤에 됩니다.
- 소리 파일의 꼴은 확장자가 아니라 내용으로 가립니다. (webm, ogg 의 Opus 와 Vorbis, mp3, wav, flac, m4a 와 aac 의 AAC)

## 관련 타입

AudioManager, AudioAsset, AudioPlayer, AudioBus, SoundSystem, SoundRow, SoundLoadHandle, SoundLoadFuture, SoundEffectPool, BeepPlayer, BeepTone, BeepWaveform, PageEvent, EngineConfiguration, Engine

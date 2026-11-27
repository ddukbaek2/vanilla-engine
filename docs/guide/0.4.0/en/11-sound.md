# Sound

This chapter shows how to play loaded sound files and how to handle volume and muting separately for music and sound effects. It covers `AudioPlayer` for playing one sound, `AudioBus` for routing many sounds through one path, `SoundSystem` for handling music and effects together from a sound table, `SoundEffectPool` for overlapping the same effect, and `BeepPlayer` for synthesizing short tones without files. `AudioAsset`, which loads sound files, is in the "Assets" chapter.

All sounds go through the single AudioContext owned by the engine's `AudioManager`. Get it with `engine.get_audio_manager()`. What stands behind the AudioContext depends on the platform.

- Desktop: the engine opens the default output device with cpal and runs its own mixer at that device's sample rate. The mixer renders the same node graph as WebAudio (sources, GainNode, StereoPannerNode, buses, and the OscillatorNode for beeps) in blocks of 128 frames, with the same math as Chrome. If the device goes away, the same mixer reopens the default device.
- Web: the same WebAudio AudioContext as the original.

## The AudioContext and the first input

`AudioManager` creates the AudioContext the first time it is needed. Right after the engine is created, there is none yet.

```rust
use vanilla::{Engine, EngineConfiguration};

let engine = Engine::new(EngineConfiguration::new());
let audio_manager = engine.get_audio_manager();
assert!(!audio_manager.has_audio_context());
assert!(audio_manager.get_audio_context().is_none());
assert_eq!(audio_manager.get_context_time(), 0.0);
```

- `create_audio_player()` and `create_audio_bus()` create the AudioContext on the spot if it does not exist. If it cannot be created, you get a silent player or bus.
- `resume_context()` does nothing before the first user input (user gesture). After that, it creates the AudioContext if needed and wakes it if it is suspended.
- The desktop has no browser autoplay policy, so it treats user input as having happened from the start (`has_user_gesture()` is always true). The sound device is opened when the first player or bus is created, or in `resume_context()`. If there is no output device, the engine reports it once and does not try again.
- On the web, as in the original, the engine reports user input (`mark_user_gesture`) and wakes the AudioContext on the window's `click`, `touchstart`, and `keydown`. It also tries to wake it when the window gains focus or the page becomes visible again (this only works after the first input).
- Setting `EngineConfiguration.resume_audio_on_touch_end` to true also wakes it on `touchend` and `pointerup` on the web. The default is false.
- On iOS on the web, even when a woken AudioContext reports "running", the engine suspends and wakes it again if its clock advances less than a quarter of real time over 0.5 seconds.
- `get_context_time()` is the sound time in seconds since the AudioContext was created, and `is_context_running()` tells whether the AudioContext exists and is not suspended. `get_audio_context()` returns a borrowed AudioContext (it has `sample_rate()` and `current_time()`); do not call other manager functions while you hold it.
- `AudioManager` is a handle to shared state, so all of its methods take `&self`. `clone()` gives another handle to the same AudioContext.

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

	fn touch_press(&mut self, _view_input_position: Vector2) {
		let engine = Engine::get_engine().unwrap();
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

## Playing one sound with AudioPlayer

`AudioPlayer` plays one `AudioAsset`. Load an asset into it with `set_audio_asset` and play it with `play(is_loop)`. The example below loads a sound in `load` and hands it to the scene, then creates a player on the first press and plays it on every press.

The repository has no sound files meant for examples, so the examples in this chapter use the decoder test files (under `tests/assets/audio`, all short sounds under 0.5 seconds). In a game, pass the path of a file in your asset folder.

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

	fn load(&mut self, engine: &Engine) {
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

	fn touch_press(&mut self, _view_input_position: Vector2) {
		let engine = Engine::get_engine().unwrap();
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

- `engine.load_audio_asset(path)` returns an `Rc<AudioAsset>`. On the desktop it reads and decodes the file on the spot and returns `Err` on failure. On the web it starts the download and decoding and returns right away, so hand the asset to `scene_base.track_audio_asset` and `initialize` is called after decoding finishes. You may load the asset into a player before decoding finishes; until then `play` does nothing.
- `play(is_loop)` stops the sound if it is already playing and starts again from the beginning (or from the position set with `set_time`). If the AudioContext is suspended, it wakes it first and then plays (the web waits for the wake, the desktop does it on the spot). It does nothing for a silent player, without an asset, or when the asset is not loaded yet.
- There are also `stop()`, `is_playing()`, and `get_duration()` (seconds, 0 without an asset or before it is loaded).
- `set_volume(volume)` clamps to 0 to 1, and a value that is not finite becomes 1. `mute()` sets the volume to 0, and `unmute()` returns to the set volume. A volume changed while muted takes effect when unmuted. Muting does not apply to a silent player (`is_muted()` stays false).
- `set_playback_rate(playback_rate)` changes speed and pitch together. It applies from the next `play`, and a value of 0 or less or one that is not finite becomes 1.
- `set_time(time)` sets where `play` starts, in seconds. If the sound is playing, it restarts from there immediately. `get_time()` returns that start position and does not advance during playback.
- `set_audio_asset` does not change a sound that is already playing. The new asset plays from the next `play`.
- The player is owned by whoever created it. Dropping the player also stops its sound (in the original, the sound plays to the end regardless of garbage collection). Keep it in a scene field as in the example above.
- `create_audio_player()` is the same as `AudioPlayer::new(&audio_manager, None)`. The second argument is where the sound goes, and None means the output.

## Grouping with AudioBus

`AudioBus` collects the sound of several players into one path and applies volume and mute to all of them at once. Create it with `audio_manager.create_audio_bus()`, and create players that play through it with `bus.create_audio_player()`. The volume and mute rules are the same as for `AudioPlayer`.

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

	fn load(&mut self, engine: &Engine) {
		if let Ok(audio_asset) = engine.load_audio_asset("tests/assets/audio/wav_s16_stereo.wav") {
			self.scene_base.track_audio_asset(&audio_asset);
			self.audio_asset = Some(audio_asset);
		}
	}

	fn touch_press(&mut self, _view_input_position: Vector2) {
		let engine = Engine::get_engine().unwrap();
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

- `AudioBus` is also a handle to shared state, so all of its methods take `&self`, and a `clone()` of the handle sees the same bus. A player created from a bus holds one bus handle and keeps the bus alive. When every handle is dropped, the bus is disconnected from the output.
- `is_available()` tells whether the bus was created while an AudioContext existed. A bus created without an AudioContext, and the players created from it, make no sound.
- `get_gain_node()` returns a handle to the bus's GainNode (None for a silent bus). As in the original, you can pass the bus node as the destination with `AudioPlayer::new(&audio_manager, audio_bus.get_gain_node().as_deref())`, and this form is the same on both platforms. A volume set directly on the node does not change the bus's volume and mute fields.

## Music and effects with SoundSystem

This type handles all of a game's sound. Give it a sound table and it creates a music bus and an effects bus, and takes care of the settings volume, muting, and waking the AudioContext.

A row of the sound table is a `SoundRow`, and all of its fields are public.

| Field | Meaning |
| --- | --- |
| `id` | Sound name |
| `paths` | A group of recordings of the same sound. Each time an effect plays, one of the loaded files is picked, never the same as the previous one. Music uses only the first file |
| `volume` | Per-sound volume (0 to 1) |
| `is_music` | true for music |
| `minimum_interval` | How soon the same effect may play again (seconds) |
| `maximum_voices` | How many copies of the same effect may play at once. Beyond that, the oldest is cut. None means up to the overall effect limit (12), and `Some(0)` lets only one play |
| `pitch_variation` | Ratio by which the pitch is varied on each play. 0.05 means up to 5% up or down |
| `play_seconds`, `fade_out_seconds` | Play only the first `play_seconds` seconds of an effect, then fade it out over `fade_out_seconds` seconds. 0 plays to the end |

`SoundRow::create_effect(id, paths, volume)` and `SoundRow::create_music(id, path, volume)` create a row with the other fields at their defaults (interval 0, limit None, variation 0, play to the end). Set any other field after creating the row.

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

The example below creates a `SoundSystem` from the sound table in `load`, puts it in the shared slot, waits for all sounds to load in a task, and starts the music in `initialize`. A press plays an effect panned to where you pressed.

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

	fn load(&mut self, engine: &Engine) {
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

	fn initialize(&mut self, _engine: &Engine) {
		if let Some(sound_system) = read_sound_system() {
			sound_system.borrow_mut().play_music("music", 1.0, false);
		}
	}

	fn tick(&mut self, time_delta: f64) {
		self.scene_base.tick(time_delta);
		if let Some(sound_system) = read_sound_system() {
			sound_system.borrow_mut().update();
		}
	}

	fn touch_press(&mut self, view_input_position: Vector2) {
		let engine = Engine::get_engine().unwrap();
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

- The constructor is `SoundSystem::new(engine, sound_table)`. The original takes an `AudioManager`, but the Rust version takes the engine. It shares the engine's `AudioManager` handle and receives page hiding and focus loss through the engine's page events (`PageEvent`).
- To add more rows, use `add_sounds(sound_table)` (rows with the same `id` are replaced). To add a sound that is not loaded from a file, use `add_sound_asset(sound_id, audio_asset)`, which appends it to the group of that `id`. Volume and interval follow the table row with the same `id`, so the row must exist for it to play.
- `load_sounds(engine)` loads effects first and music after. Paths are relative to the asset folder, and the desktop reads them on the spot. Files that fail to load are reported and skipped, and rows already loaded are not loaded again. The returned `SoundLoadHandle` can be awaited until every requested sound has loaded or failed (failures do not reject). `wait_for_load()` returns a future that waits for the same thing. To load a single row, use `load_sound(engine, &sound_row)`.
- Loading sounds does not block play, so you do not have to wait. A `play_music` called before loading is remembered and plays once the music has loaded.
- You must call `update()` yourself every frame. The engine does not call it for you. It tries waking before the first input (only once), creates the buses after the first input, follows muting, suspends the AudioContext and wakes it again (every 0.5 seconds), starts and switches music, and cleans up finished sounds.
- `play_music(sound_id, fade_seconds, is_resumed)` plays music on a loop. Over `fade_seconds`, the previous music fades down and the new one rises from 0 (0 switches at once). If `is_resumed` is true, the music continues from where it was stopped. Passing `""` stops the music. Asking again for the `id` that is already requested does nothing. If the first file failed to load, the music does not play. Unlike the original, the arguments have no defaults, so pass all three.
- `play_effect(sound_id, volume, pan)` plays an effect. `volume` is this play's volume (0 to 1), and `pan` is left to right (-1 left to 1 right, 0 center; values out of range are clamped, and a value that is not finite becomes 0). It skips the play before the first input (no bus yet), while muted, when the AudioContext is not running, for an unknown `id`, before loading, when the volume is 0.001 or less, or within `minimum_interval`. At most 12 effects play at once in total, and beyond that the oldest is cut.
- `stop_effects()` cuts all playing effects, and `fade_out_effects(sound_id, fade_seconds)` fades them from their current volume down to 0 (`""` means all, and 0 seconds cuts at once). `get_effect_voice_count()` is the number of playing effects (not counting those fading out).
- `set_sound_system(Some(sound_system))` and `read_sound_system()` are module functions that hold one shared `SoundSystem` (`Rc<RefCell<SoundSystem>>`). There is one per thread, and passing `None` clears it. They are handy when scenes change but the sounds stay the same.

### Volume

- Music volume is `set_music_volume` (the music volume setting) x `set_music_scale` (the music ratio) x the row's `volume`. The music ratio is for lowering music apart from the setting, for example halving it while a pause window is open.
- Effect volume is `set_sound_effect_volume` x the row's `volume` x the `volume` given to `play_effect`.
- All of these clamp to 0 to 1, and a value that is not finite becomes 0. The settings volume is applied to the buses and follows changes smoothly with a 0.05 second time constant so the sound does not click.
- Read them with `get_music_volume()`, `get_music_scale()`, `get_sound_effect_volume()`, and `get_music_id()` (the requested music, `""` if none).
- You can borrow the buses with `get_music_bus()` and `get_effect_bus()` (None before the first input). Players created from the effects bus also follow the settings volume and muting. The settings volume is applied directly to the bus's GainNode, so a value set with the bus's `set_volume` or `mute` lasts only until `SoundSystem` applies the volume again.

### Mute rules

Muting comes before the set volume. If any of the following holds, no sound plays regardless of the volume settings.

- External mute. When the function given with `set_external_mute_provider(Some(function))` returns true (a platform SDK's mute setting, while a rewarded ad is shown, and so on). It is read every frame in `update` and on page events.
- The user's mute button. `set_user_muted(true)`.
- The page is hidden. On the web this is another tab or an app switch (visibilitychange); on the desktop it is when winit reports that the window is occluded (only macOS and X11 report it; Windows and Wayland do not).
- The window loses focus on a device whose primary input is touch. On iPhone the notice that the page is hidden arrives after the app has gone to the background, so the engine follows the focus loss that arrives earlier. Whether to follow it is decided at creation (`is_blur_mute_enabled()`: on the web when `(pointer: coarse)` matches; on the desktop for Windows in tablet mode or touch devices without a mouse, and Linux devices with only a touchscreen; never on macOS). It is lifted when focus returns, the screen is pressed, or the page becomes visible again.

When muting starts, playing effects are cut, and music keeps running silently so it is heard again when unmuted. While muted, the AudioContext is also suspended, and it is woken when unmuted (on the desktop, the mixer stops rendering, so sound and clock stop in place). If the AudioContext is not running while not muted, the engine tries waking it every 0.5 seconds. Read the state with `is_user_muted()`, `is_page_hidden()`, `is_window_blurred()`, `is_context_paused()`, and `read_muted()`.

`set_autoplay_allowed_provider(Some(function))` decides whether waking once before the first input is allowed. Without it (None), waking is allowed and tried once in the first `update`. On a platform whose web view blocks it, give a function that returns false. If waking is blocked, the engine wakes the AudioContext on the first input.

The example below checks only the mute decision, without a sound device. The ad state is kept in a `Cell` that the external mute function reads, and page events are fed to the engine directly (`dispatch_page_event` is normally called by the platform).

```rust
use std::cell::Cell;
use std::rc::Rc;
use vanilla::{Engine, EngineConfiguration, PageEvent, SoundRow, SoundSystem};

let engine = Engine::new(EngineConfiguration::new());
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

Plays several `AudioPlayer`s in turn so the same effect playing in quick succession is not cut off. Create it with `SoundEffectPool::new(audio_manager, voice_count)`; `play(&audio_asset, volume)` loads the asset into the next player in turn and plays it. If that player was playing, it is cut and started anew.

- This play's volume (0 to 1) is multiplied by the pool volume (`set_volume`). Changing the pool volume also multiplies into sounds already playing (a value that is not finite becomes 1).
- While muted or with 0 voices, `play` does nothing and does not advance the turn.
- There are also `set_muted`, `is_muted`, `stop_all`, `get_voice_count`, and `get_volume`.
- Unlike the original, the voice count and the `play` volume have no defaults, so always pass them. The original default of 6 voices is `vanilla::core::soundeffectpool::DEFAULT_VOICE_COUNT`.

The constructor creates the players immediately, so the AudioContext is created at that point (the desktop opens the sound device). On the web, create it after the first input as below.

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

	fn load(&mut self, engine: &Engine) {
		if let Ok(audio_asset) = engine.load_audio_asset("tests/assets/audio/wav_s16_stereo.wav") {
			self.scene_base.track_audio_asset(&audio_asset);
			self.audio_asset = Some(audio_asset);
		}
	}

	fn touch_press(&mut self, _view_input_position: Vector2) {
		let engine = Engine::get_engine().unwrap();
		if self.sound_effect_pool.is_none() {
			let mut sound_effect_pool = SoundEffectPool::new(&engine.get_audio_manager(), 6);
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

A pool with 0 voices creates no players, so it creates no AudioContext either and only holds its volume and mute state.

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

Synthesizes UI sounds with an OscillatorNode and a GainNode, without sound files. It applies a short rise and fall at the start and end (start min(0.005, length x 0.25) seconds, end min(0.02, length x 0.5) seconds) so the sound does not click. The web builds WebAudio nodes as in the original, and the desktop builds the same graph in the mixer and renders it with the same math as Chromium.

- Create it with `BeepPlayer::new(audio_manager, volume)`. The original default volume of 0.35 is `BeepPlayer::DEFAULT_VOLUME`. The constructor does not clamp the volume, while `set_volume` clamps to 0 to 1.
- The names for `play_preset(preset_name)` are `"click"`, `"confirm"`, `"cancel"`, `"error"`, `"success"`, `"warning"`, and `"notification"`. They are case sensitive, and an unknown name plays the same sound as `"click"`.
- `play_tone(frequency, duration_seconds, waveform, delay_seconds, volume_scale)` plays one tone. Frequency is in Hz, length and delay in seconds, and the peak volume is the base volume x `volume_scale` (0.0001 if smaller). Unlike the original, the arguments have no defaults, so pass all five, and the waveform is `BeepWaveform::Sine`, `Square`, `Sawtooth`, or `Triangle` instead of a string.
- `play_sequence(&tone_list)` plays a list of `BeepTone`s one after another. Each tone starts after the total length of the tones before it. `BeepTone::create(frequency, duration)` is a sine tone with scale 1, and `BeepTone::create_with_waveform(frequency, duration, waveform, volume_scale)` also sets the waveform and scale.
- There are also `set_volume`, `get_volume`, `set_muted`, and `is_muted`.

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

	fn load(&mut self, engine: &Engine) {
		self.beep_player = Some(BeepPlayer::new(&engine.get_audio_manager(), 0.3));
	}

	fn touch_press(&mut self, _view_input_position: Vector2) {
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

`BeepPlayer` tries to wake the AudioContext when called (the desktop opens the sound device at that point) and makes sound only while it is running. While muted, it returns without waking anything. Its sound goes straight to the output without a bus, so it does not follow `SoundSystem`'s volume and muting. Creating it alone does not create the AudioContext, so it is fine to create it in `load`.

## Things to watch out for

- Dropping an `AudioPlayer` or the bus handles stops the sound. Keep them somewhere that stays alive while the sound plays, such as a scene field. `SoundSystem` holds the players of its playing sounds by itself.
- If you never call `SoundSystem::update()`, the buses are never created and no sound plays.
- If a sound table row's `volume` is 0 (a volume of 0.001 or less), that effect does not play. Always pass a volume to `create_effect` and `create_music`.
- Players created with `audio_manager.create_audio_player()`, `SoundEffectPool`, and `BeepPlayer` connect straight to the output, so they do not follow `SoundSystem`'s settings volume and muting. To make a player follow them, call `create_audio_player()` on the bus from `sound_system.get_effect_bus()`.
- There are two clamping rules for volume. `AudioPlayer`, `AudioBus`, and `SoundEffectPool` treat a value that is not finite as 1, while `SoundSystem` treats it as 0.
- You may load an `AudioAsset` before the first input. Decoding does not take the sound device (on the desktop, the engine's decoder produces the same result as Chrome's decodeAudioData; on the web, the browser decodes with an OfflineAudioContext, or a temporary AudioContext if that is missing). Only playback has to wait for the first input.
- The format of a sound file is detected from its content, not its extension (Opus and Vorbis in webm and ogg, mp3, wav, flac, and AAC in m4a and aac).

## Related types

AudioManager, AudioAsset, AudioPlayer, AudioBus, SoundSystem, SoundRow, SoundLoadHandle, SoundLoadFuture, SoundEffectPool, BeepPlayer, BeepTone, BeepWaveform, PageEvent, EngineConfiguration, Engine

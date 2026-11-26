//==============================================================================
//! UI 글꼴. (원본 misc/uifont.js, 화면 위의 모든 글자, 여러 문자의 글꼴 조각을 한 이름 아래 묶음)
//! - 글꼴 이름, 목록 파일과 폴더, 맨 뒤 묶음, 굵기, 기다리는 시간은 configure_ui_font 로 정한다. (처음은 GameUI, ./assets/fonts/fonts.json, latin, 500 과 700)
//! - 목록은 fonts.json(색인 { fonts: [{ group, manifest }] })과 fonts-<묶음>.json(조각 { faces: [{ file, weight, style, unicodeRange }] })이다.
//! - 화면의 모든 언어를 한 글꼴 이름 아래에 모아 그린다. 글꼴 파일을 합치지 않고 조각마다 글꼴 얼굴을 만들어 그 조각의 글자 범위로 올린다.
//!   그래서 화면에 쓰는 글자가 든 조각만 받는다. (한, 중, 일 글꼴은 조각이 수백 개이다)
//!   한자처럼 여러 글꼴이 함께 가진 글자는 나중에 올린 글꼴이 그린다. 고른 언어의 묶음을 라틴 바로 앞(뒤에서 둘째)에 두어
//!   일본어면 일본 글꼴, 중국어 번체면 번체 글꼴의 한자가 나오게 한다. 라틴은 늘 맨 뒤라 숫자와 영문은 어느 언어에서나 라틴 글꼴이다.
//! - 모든 조각을 configure_ui_font 의 weights(처음은 500, 700) 두 굵기로만 올린다. 가변 굵기 조각은 두 번(500 과 700), 400 뿐인 조각은 500 으로 올린다.
//! - 쓸 글자의 조각은 미리 받는다. (load_ui_font_text) 조각을 받으면 엔진(EngineConfiguration.rebake_text_on_font_load)이 그 조각의 글자로 구운 글자만 버려 다시 굽는다.
//!   미리 받지 않은 글자도 그 글꼴로 재거나 그리면 update 가 그 글자의 조각을 받는다. (원본 브라우저는 올려 둔 FontFace 의 범위에 든 글자를 그릴 때 그 얼굴을 받는다)
//!   받기 전에는 원본 캔버스처럼 다른 글꼴로 그리고, 받으면 (rebake_text_on_font_load 를 켰을 때) 다시 굽는다.
//! - 처음에는 고른 언어와 라틴의 목록만 받고, 나머지 묶음은 놀이를 시작하고 조금 뒤(remaining_delay_milliseconds)에 받는다.
//!   처음의 조각은 wait_milliseconds 까지만 기다린다. (is_ui_font_ready 가 그 시간이 지나거나 다 받으면 참)
//! - 원본과 다른 것:
//!   원본은 FontFace 를 만들어 document.fonts 에 넣고 async 로 기다린다. 러스트 판은 데스크탑과 웹 모두 조각 파일의 바이트를 받아
//!   엔진의 글자 캐시에 같은 이름의 얼굴로 올린다. (Engine.register_font_face_when_loaded, 차례는 Engine.set_font_face_order)
//!   그래서 웹에서도 브라우저 글꼴이 아니라 엔진의 모양 짓기로 그린다. 받는 일은 요청만 하고 update(engine) 를 프레임마다 불러 이어 간다.
//!   load_ui_font 는 원본처럼 await 할 수 있는 UiFontLoadHandle 을 돌려준다. (기다리는 동안 update 를 불러 주고, 처음 글자를 다 올렸거나 wait_milliseconds 가 지나면 끝난다)
//!   load_ui_font_text 는 그 글자의 얼굴들을 글꼴 목록에 올리면 끝나는 UiFontTextLoadHandle 을 돌려준다. (원본 await loadUiFontText(글자))
//!   원본 readUiFontFace(FontFace) 는 read_ui_font_face(올린 이름, 못 불러왔으면 None) 다. options 객체는 UiFontOptions 구조체다.
//==============================================================================
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::Engine;
use crate::core::task;
use crate::core::textshaping::{parse_unicode_range, FontFaceDescriptors};
use crate::resource::fontasset::FontLoadHandle;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::core::textstringtexturecache;
use std::cell::RefCell;
use std::collections::HashSet;
use web_time::Instant;

//==============================================================================
/// UI 글꼴 설정. (원본 configureUiFont 의 options, None 인 칸은 그대로)
/// - 기본값은 모든 칸이 None 이다. 바꿀 칸만 Some 으로 채우고 ..UiFontOptions::default() 로 나머지를 비운다.
///
/// # 예제
/// 글꼴 이름과 목록 경로 바꾸기
/// ```rust
/// use vanilla::{uifont, UiFontOptions};
///
/// uifont::configure_ui_font(UiFontOptions {
/// 	family: Some(String::from("MenuFont")),
/// 	manifest_path: Some(String::from("./assets/menufonts/fonts.json")),
/// 	directory_path: Some(String::from("./assets/menufonts/")),
/// 	..UiFontOptions::default()
/// });
/// assert_eq!(uifont::read_ui_font_family(), "MenuFont");
/// assert_eq!(uifont::build_ui_font_string(700, 24.0), "700 24px \"MenuFont\", sans-serif");
/// ```
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiFontOptions {
	/// 올릴 글꼴 이름. (처음은 "GameUI", 글자 컴포넌트의 set_font_family 에 줄 이름)
	pub family: Option<String>,
	/// 글꼴 색인 파일 경로. (애셋 경로, 처음은 "./assets/fonts/fonts.json")
	pub manifest_path: Option<String>,
	/// 묶음 목록과 조각 파일이 있는 폴더. (뒤에 파일 이름을 그대로 붙이므로 / 로 끝낸다, 처음은 "./assets/fonts/")
	pub directory_path: Option<String>,
	/// 맨 뒤에 둘 묶음. (처음은 "latin", 숫자와 영문을 늘 이 묶음으로 그린다)
	pub primary_group: Option<String>,
	/// 조각을 올릴 굵기 목록. (처음은 500 과 700)
	pub weights: Option<Vec<u16>>,
	/// 처음 글자의 조각을 기다리는 시간. (밀리초, 처음은 1500)
	pub wait_milliseconds: Option<f64>,
	/// 처음 불러오기 뒤 나머지 묶음을 받기까지 쉬는 시간. (밀리초, 처음은 3000)
	pub remaining_delay_milliseconds: Option<f64>,
}

//==============================================================================
/// 조각 하나의 한 굵기. (같은 파일을 두 굵기로 올리면 둘이다)
//==============================================================================
#[derive(Clone)]
struct UiFontFace {
	file: String,
	descriptors: FontFaceDescriptors,
	weight: u16,
	ranges: Vec<(u32, u32)>,
	load_handle: Option<AssetLoadHandle>,
}

//==============================================================================
/// 글꼴 묶음 하나. (order 는 색인의 차례)
//==============================================================================
struct UiFontGroup {
	group: String,
	order: usize,
	faces: Vec<UiFontFace>,
}

//==============================================================================
/// UI 글꼴 전역 상태. (원본 모듈 변수)
//==============================================================================
struct UiFontState {
	family: String,
	manifest_path: String,
	directory_path: String,
	primary_group: String,
	weights: Vec<u16>,
	wait_milliseconds: f64,
	remaining_delay_milliseconds: f64,
	font_groups: Vec<UiFontGroup>,
	font_index: Option<Vec<(String, String)>>,
	index_handle: Option<AssetLoadHandle>,
	group_handle_list: Vec<(String, usize, AssetLoadHandle)>,
	current_group: String,
	/// 처음 불러오기: 고른 묶음과 미리 받을 글자. (목록을 다 받으면 앞세우고 글자를 받는다)
	pending_initial_load: Option<(String, String)>,
	initial_load_instant: Option<Instant>,
	is_remaining_groups_requested: bool,
	remaining_groups_due: Option<Instant>,
	is_remaining_groups_loading: bool,
	is_loaded: bool,
	requested_characters: HashSet<char>,
	/// 처음 글자로 받기를 청한 얼굴들. (다 올리면 준비된 것이다)
	initial_text_handle_list: Vec<FontLoadHandle>,
}

thread_local! {
	static UI_FONT_STATE: RefCell<UiFontState> = RefCell::new(UiFontState {
		family: String::from("GameUI"),
		manifest_path: String::from("./assets/fonts/fonts.json"),
		directory_path: String::from("./assets/fonts/"),
		primary_group: String::from("latin"),
		weights: vec![500, 700],
		wait_milliseconds: 1500.0,
		remaining_delay_milliseconds: 3000.0,
		font_groups: Vec::new(),
		font_index: None,
		index_handle: None,
		group_handle_list: Vec::new(),
		current_group: String::from("latin"),
		pending_initial_load: None,
		initial_load_instant: None,
		is_remaining_groups_requested: false,
		remaining_groups_due: None,
		is_remaining_groups_loading: false,
		is_loaded: false,
		requested_characters: HashSet::new(),
		initial_text_handle_list: Vec::new(),
	});
}

//==============================================================================
/// 전역 상태 읽고 쓰기.
//==============================================================================
fn with_state<R>(visitor: impl FnOnce(&mut UiFontState) -> R) -> R {
	UI_FONT_STATE.with(|state_cell| visitor(&mut state_cell.borrow_mut()))
}

//==============================================================================
/// 조각의 굵기 → 올릴 굵기. ("400 700" → [500, 700], "400" → `[500]`, "700" → `[700]`, 원본 readTargetWeights)
/// - 범위면 그 안의 굵기 모두, 한 값이면 가장 가까운 하나이다.
/// - 숫자가 없으면 400 으로 본다. 가까운 굵기가 둘이면 weights 의 앞 것이다.
///
/// # 인자
/// - `weight_descriptor`: 조각 목록의 weight 글. ("400" 같은 한 값, "100 900" 같은 범위)
/// - `weights`: 올릴 굵기 목록.
///
/// # 돌려줌
/// 올릴 굵기 목록. 범위면 범위 안(양 끝 포함)의 굵기 모두, 한 값이면 가장 가까운 하나. weights 가 비면 빈 목록.
///
/// # 예제
/// 가변 굵기 조각과 한 굵기 조각
/// ```rust
/// use vanilla::uifont;
///
/// let weights = [500, 700];
/// assert_eq!(uifont::read_target_weights("400 700", &weights), vec![500, 700]);
/// assert_eq!(uifont::read_target_weights("400", &weights), vec![500]);
/// assert_eq!(uifont::read_target_weights("700", &weights), vec![700]);
/// assert!(uifont::read_target_weights("100 450", &weights).is_empty());
/// ```
//==============================================================================
pub fn read_target_weights(weight_descriptor: &str, weights: &[u16]) -> Vec<u16> {
	let weight_values: Vec<f64> = weight_descriptor.split_whitespace().filter_map(|weight_text| weight_text.parse::<f64>().ok()).collect();
	if weights.is_empty() {
		return Vec::new();
	}
	if weight_values.len() >= 2 {
		return weights.iter().copied().filter(|target_weight| (*target_weight as f64) >= weight_values[0] && (*target_weight as f64) <= weight_values[1]).collect();
	}
	let wanted_weight = weight_values.first().copied().unwrap_or(400.0);
	let mut nearest_weight = weights[0];
	for target_weight in weights {
		if (*target_weight as f64 - wanted_weight).abs() < (nearest_weight as f64 - wanted_weight).abs() {
			nearest_weight = *target_weight;
		}
	}
	vec![nearest_weight]
}

//==============================================================================
/// UI 글꼴 불러오기. (고른 언어와 라틴의 묶음을 올려 앞세우고, 그 언어의 글자를 미리 받는다. 원본 loadUiFont)
/// - 받는 동안은 update 를 프레임마다 부른다. 처음 글자가 준비됐는지는 is_ui_font_ready 로 본다. (돌려준 손잡이를 await 하면 준비될 때까지 기다린다)
///
/// # 인자
/// - `engine`: 애셋을 받고 글꼴 얼굴을 올릴 엔진.
/// - `font_group`: 고른 언어의 글꼴 묶음 이름. (색인 fonts.json 의 group)
/// - `text`: 미리 받을 글자. (그 언어의 화면 글자 모두 같은 것)
///
/// # 돌려줌
/// await 할 수 있는 불러오기 손잡이.
//==============================================================================
pub fn load_ui_font(engine: &mut Engine, font_group: &str, text: &str) -> UiFontLoadHandle {
	let primary_group = with_state(|state| {
		state.pending_initial_load = Some((font_group.to_string(), text.to_string()));
		state.initial_load_instant = Some(Instant::now());
		state.primary_group.clone()
	});
	load_font_index(engine);
	request_font_group(engine, font_group);
	request_font_group(engine, &primary_group);
	update(engine);
	UiFontLoadHandle
}

//==============================================================================
/// 글꼴 목록의 색인 받기 요청. (한 번만)
//==============================================================================
fn load_font_index(engine: &mut Engine) {
	let manifest_path = with_state(|state| {
		if state.font_index.is_some() || state.index_handle.is_some() {
			return None;
		}
		Some(state.manifest_path.clone())
	});
	if let Some(manifest_path) = manifest_path {
		let load_handle = engine.request_asset_bytes(&manifest_path);
		with_state(|state| {
			state.index_handle = Some(load_handle);
		});
		poll_font_index();
	}
}

//==============================================================================
/// 색인을 받았으면 든다.
//==============================================================================
fn poll_font_index() {
	let bytes = with_state(|state| {
		let load_handle = state.index_handle.as_ref()?;
		if !load_handle.is_finished() {
			return None;
		}
		let bytes = load_handle.get_bytes();
		state.index_handle = None;
		Some(bytes)
	});
	let bytes = match bytes {
		Some(bytes) => {
			bytes
		}
		None => {
			return;
		}
	};
	let manifest: Option<serde_json::Value> = bytes.as_deref().and_then(|bytes| serde_json::from_slice(bytes).ok());
	let font_list = manifest.as_ref().and_then(|manifest| manifest.get("fonts")).and_then(|fonts| fonts.as_array());
	match font_list {
		Some(font_list) => {
			let font_index: Vec<(String, String)> = font_list.iter().filter_map(|index_entry| Some((index_entry.get("group")?.as_str()?.to_string(), index_entry.get("manifest")?.as_str()?.to_string()))).collect();
			with_state(|state| {
				state.font_index = Some(font_index);
			});
		}
		None => {
			eprintln!("[uifont] 글꼴의 목록을 읽지 못했습니다.");
			with_state(|state| {
				state.font_index = Some(Vec::new());
				state.pending_initial_load = None;
			});
		}
	}
}

//==============================================================================
/// 글꼴 묶음 하나의 목록 받기 요청. (이미 있거나 받는 중이면 그대로, 색인을 아직 못 받았으면 받은 뒤 update 가 다시 청한다)
//==============================================================================
fn request_font_group(engine: &mut Engine, group: &str) {
	let request = with_state(|state| {
		let font_index = state.font_index.as_ref()?;
		if state.font_groups.iter().any(|font_group| font_group.group == group) || state.group_handle_list.iter().any(|(pending_group, _, _)| pending_group == group) {
			return None;
		}
		let order = font_index.iter().position(|(index_group, _)| index_group == group)?;
		Some((order, format!("{}{}", state.directory_path, font_index[order].1)))
	});
	if let Some((order, manifest_path)) = request {
		let load_handle = engine.request_asset_bytes(&manifest_path);
		with_state(|state| {
			state.group_handle_list.push((group.to_string(), order, load_handle));
		});
	}
}

//==============================================================================
/// 받은 묶음 목록 들기. (조각마다 굵기마다 얼굴을 만든다, 올리기는 apply_ui_font_group 이 차례를 맞춰 한다)
//==============================================================================
fn poll_font_groups() {
	let finished_list = with_state(|state| {
		let mut finished_list: Vec<(String, usize, Option<std::rc::Rc<Vec<u8>>>)> = Vec::new();
		state.group_handle_list.retain(|(group, order, load_handle)| {
			if !load_handle.is_finished() {
				return true;
			}
			finished_list.push((group.clone(), *order, load_handle.get_bytes()));
			false
		});
		finished_list
	});
	for (group, order, bytes) in finished_list {
		let manifest: Option<serde_json::Value> = bytes.as_deref().and_then(|bytes| serde_json::from_slice(bytes).ok());
		let face_list = match manifest.as_ref().and_then(|manifest| manifest.get("faces")).and_then(|faces| faces.as_array()) {
			Some(face_list) => {
				face_list.clone()
			}
			None => {
				eprintln!("[uifont] 글꼴 묶음의 목록을 읽지 못했습니다: {}", group);
				continue;
			}
		};
		with_state(|state| {
			if state.font_groups.iter().any(|font_group| font_group.group == group) {
				return;
			}
			let mut faces: Vec<UiFontFace> = Vec::new();
			for face in &face_list {
				let read_text = |name: &str| -> String {
					face.get(name).and_then(|field| field.as_str()).unwrap_or("").to_string()
				};
				let file = read_text("file");
				let unicode_range = read_text("unicodeRange");
				let ranges = parse_unicode_range(&unicode_range);
				for target_weight in read_target_weights(&read_text("weight"), &state.weights) {
					let mut descriptors = FontFaceDescriptors::create_from_css(&target_weight.to_string(), &read_text("style"), &unicode_range);
					descriptors.source_key = format!("{}#{}", file, target_weight);
					faces.push(UiFontFace {
						file: file.clone(),
						descriptors,
						weight: target_weight,
						ranges: ranges.clone(),
						load_handle: None,
					});
				}
			}
			state.font_groups.push(UiFontGroup {
				group: group.clone(),
				order,
				faces,
			});
		});
	}
}

//==============================================================================
/// 이어 가기. (받은 색인과 묶음 목록을 들고, 처음 불러오기를 마저 하고, 때가 되면 나머지 묶음을 받는다. 프레임마다 부른다)
/// - 이 글꼴로 재거나 그렸는데 받은 조각에 없던 글자의 조각도 여기서 받는다.
///
/// # 인자
/// - `engine`: 애셋을 받고 글꼴 얼굴을 올릴 엔진.
//==============================================================================
pub fn update(engine: &mut Engine) {
	poll_font_index();
	// 그린 글 가운데 받은 조각에 없던 글자의 조각을 받는다. (원본 브라우저가 그 글자의 범위에 든 FontFace 를 받기 시작하는 자리)
	let (family, is_ready_to_request) = with_state(|state| (state.family.clone(), !state.font_groups.is_empty()));
	textstringtexturecache::watch_unresolved_family_characters(&family);
	let unresolved_text = textstringtexturecache::take_unresolved_family_characters(&family);
	if is_ready_to_request && !unresolved_text.is_empty() {
		request_ui_font_text(engine, &unresolved_text);
	}
	// 색인을 받은 뒤 청하지 못한 처음 묶음을 청한다.
	let initial_groups = with_state(|state| {
		let (font_group, _) = state.pending_initial_load.as_ref()?;
		if state.font_index.is_none() {
			return None;
		}
		Some((font_group.clone(), state.primary_group.clone()))
	});
	if let Some((font_group, primary_group)) = initial_groups {
		request_font_group(engine, &font_group);
		request_font_group(engine, &primary_group);
	}
	poll_font_groups();

	// 처음 불러오기: 묶음을 다 받았으면 앞세우고 글자를 받는다.
	let initial_load = with_state(|state| {
		if state.font_index.is_none() || !state.group_handle_list.is_empty() {
			return None;
		}
		state.pending_initial_load.take()
	});
	if let Some((font_group, text)) = initial_load {
		let has_groups = with_state(|state| !state.font_groups.is_empty());
		if has_groups {
			apply_ui_font_group(engine, &font_group);
			let text_load_handle = load_ui_font_text(engine, &text);
			with_state(|state| {
				state.initial_text_handle_list = text_load_handle.font_load_handle_list;
				state.is_loaded = true;
				if !state.is_remaining_groups_requested {
					state.is_remaining_groups_requested = true;
					state.remaining_groups_due = Some(Instant::now() + std::time::Duration::from_secs_f64(state.remaining_delay_milliseconds / 1000.0));
				}
			});
		}
	}

	// 나머지 묶음: 때가 되면 모두 청하고, 다 받으면 차례를 맞춰 올리고 그동안 청한 글자의 조각을 받는다.
	let is_due = with_state(|state| match state.remaining_groups_due {
		Some(due_instant) => {
			Instant::now() >= due_instant
		}
		None => {
			false
		}
	});
	if is_due {
		let group_list: Vec<String> = with_state(|state| {
			state.remaining_groups_due = None;
			state.is_remaining_groups_loading = true;
			state.font_index.as_ref().map(|font_index| font_index.iter().map(|(group, _)| group.clone()).collect()).unwrap_or_default()
		});
		for group in group_list {
			request_font_group(engine, &group);
		}
		poll_font_groups();
	}
	let is_remaining_done = with_state(|state| state.is_remaining_groups_loading && state.group_handle_list.is_empty());
	if is_remaining_done {
		let (current_group, requested_text) = with_state(|state| {
			state.is_remaining_groups_loading = false;
			(state.current_group.clone(), state.requested_characters.iter().collect::<String>())
		});
		apply_ui_font_group(engine, &current_group);
		load_ui_font_text(engine, &requested_text);
	}
}

//==============================================================================
/// 고른 언어의 글꼴 묶음 앞세우기. (그 묶음을 라틴 바로 앞에 두고 모든 얼굴의 차례를 다시 정한다, 원본 applyUiFontGroup)
/// - 받은 묶음이 하나도 없으면 아무것도 하지 않는다. 차례를 바꾸면 엔진의 글자 캐시를 비운다.
///
/// # 인자
/// - `engine`: 얼굴 차례를 걸 엔진.
/// - `font_group`: 앞세울 묶음 이름. (언어를 바꿀 때 그 언어의 묶음)
//==============================================================================
pub fn apply_ui_font_group(engine: &mut Engine, font_group: &str) {
	let order_request = with_state(|state| {
		if state.font_groups.is_empty() {
			return None;
		}
		state.current_group = font_group.to_string();
		let primary_group = state.primary_group.clone();
		let mut font_groups = std::mem::take(&mut state.font_groups);
		// 나머지 묶음은 색인의 차례대로, 고른 묶음은 뒤에서 둘째, 라틴은 맨 뒤.
		font_groups.sort_by_key(|font_group_entry| {
			if font_group_entry.group == primary_group {
				(2, 0)
			}
			else if font_group_entry.group == font_group {
				(1, 0)
			}
			else {
				(0, font_group_entry.order)
			}
		});
		state.font_groups = font_groups;
		let source_key_list: Vec<String> = state.font_groups.iter().flat_map(|font_group_entry| font_group_entry.faces.iter().map(|face| face.descriptors.source_key.clone())).collect();
		Some((state.family.clone(), source_key_list))
	});
	if let Some((family, source_key_list)) = order_request {
		engine.set_font_face_order(&family, source_key_list);
		engine.clear_text_cache();
	}
}

//==============================================================================
/// 그 글자를 그 굵기로 그릴 얼굴의 자리. (나중에 놓인 묶음부터 보아 범위에 드는 첫 얼굴, 없으면 None. 원본 findDrawingFontFace)
//==============================================================================
fn find_drawing_face(state: &UiFontState, code_point: u32, target_weight: u16) -> Option<(usize, usize)> {
	for group_index in (0..state.font_groups.len()).rev() {
		for (face_index, face) in state.font_groups[group_index].faces.iter().enumerate() {
			if face.weight != target_weight {
				continue;
			}
			if face.ranges.is_empty() || face.ranges.iter().any(|(start, end)| code_point >= *start && code_point <= *end) {
				return Some((group_index, face_index));
			}
		}
	}
	None
}

//==============================================================================
/// 글자의 조각 받기. (그 글자를 그릴 얼굴만, 원본 loadUiFontText. 청한 얼굴을 모두 올리면 끝나는 손잡이를 돌려준다)
/// - 같은 파일을 여러 굵기로 올리면 바이트는 한 번 받고 굵기마다 올린다.
/// - 받은 묶음이 없거나 글이 비면 아무것도 청하지 않는다. 이미 받기를 청한 얼굴은 다시 청하지 않는다.
///
/// # 인자
/// - `engine`: 애셋을 받고 글꼴 얼굴을 올릴 엔진.
/// - `text`: 쓸 글자. (글자마다 굵기마다 그릴 얼굴을 찾는다)
///
/// # 돌려줌
/// 이번에 청한 얼굴들을 모두 올리면(또는 실패하면) 끝나는 손잡이. 청한 것이 없으면 바로 끝난 손잡이.
///
/// # 예제
/// 받은 묶음이 없으면 청할 것이 없다
/// ```rust
/// use vanilla::{uifont, Engine, EngineConfiguration};
///
/// let mut engine = Engine::new(EngineConfiguration::new());
/// let text_load_handle = uifont::load_ui_font_text(&mut engine, "Player One");
/// assert!(text_load_handle.get_font_load_handles().is_empty());
/// assert!(text_load_handle.is_finished());
/// ```
//==============================================================================
pub fn load_ui_font_text(engine: &mut Engine, text: &str) -> UiFontTextLoadHandle {
	let request_list = with_state(|state| {
		let mut request_list: Vec<(usize, usize)> = Vec::new();
		if state.font_groups.is_empty() || text.is_empty() {
			return request_list;
		}
		for character in text.chars() {
			state.requested_characters.insert(character);
			for target_weight in state.weights.clone() {
				if let Some((group_index, face_index)) = find_drawing_face(state, character as u32, target_weight) {
					if state.font_groups[group_index].faces[face_index].load_handle.is_none() && !request_list.contains(&(group_index, face_index)) {
						request_list.push((group_index, face_index));
					}
				}
			}
		}
		request_list
	});
	let mut font_load_handle_list: Vec<FontLoadHandle> = Vec::new();
	for (group_index, face_index) in request_list {
		let (family, directory_path, face, shared_handle) = with_state(|state| {
			let face = state.font_groups[group_index].faces[face_index].clone();
			// 같은 파일을 이미 받는 얼굴이 있으면 그 손잡이를 나눠 쓴다.
			let shared_handle = state.font_groups.iter().flat_map(|font_group| font_group.faces.iter()).find(|other_face| other_face.file == face.file && other_face.load_handle.is_some()).and_then(|other_face| other_face.load_handle.clone());
			(state.family.clone(), state.directory_path.clone(), face, shared_handle)
		});
		let load_handle = match shared_handle {
			Some(shared_handle) => {
				shared_handle
			}
			None => {
				engine.request_asset_bytes(&format!("{}{}", directory_path, face.file))
			}
		};
		let font_load_handle = engine.register_font_face_when_loaded(&family, face.descriptors.clone(), load_handle.clone());
		with_state(|state| {
			state.font_groups[group_index].faces[face_index].load_handle = Some(load_handle);
		});
		font_load_handle_list.push(font_load_handle);
	}
	UiFontTextLoadHandle {
		font_load_handle_list,
	}
}

//==============================================================================
/// 글자의 조각 받기 청하기. (사람 이름처럼 그때그때 나오는 글자, 처음 보는 글자만. 원본 requestUiFontText)
/// - 받은 묶음이 없으면 아무것도 하지 않는다. 손잡이는 돌려주지 않는다.
///
/// # 인자
/// - `engine`: 애셋을 받고 글꼴 얼굴을 올릴 엔진.
/// - `text`: 쓸 글자. (앞서 청한 글자는 뺀다)
//==============================================================================
pub fn request_ui_font_text(engine: &mut Engine, text: &str) {
	let new_text: String = with_state(|state| {
		if state.font_groups.is_empty() {
			return String::new();
		}
		text.chars().filter(|character| !state.requested_characters.contains(character)).collect()
	});
	if new_text.is_empty() {
		return;
	}
	load_ui_font_text(engine, &new_text);
}

//==============================================================================
/// 처음 불러오기를 기다리는 퓨처. (원본 await loadUiFont(묶음, 글자) 의 await 자리, load_ui_font 가 돌려준 손잡이를 await 하는 것과 같다)
/// - 기다리는 동안 프레임마다 update 를 부른다. 원본처럼 처음 글자를 다 올렸거나 wait_milliseconds 가 지나면 끝난다. (is_ui_font_ready)
///
/// # 돌려줌
/// is_ui_font_ready 가 참이 되면 끝나는 퓨처.
//==============================================================================
pub fn wait_for_load() -> UiFontLoadFuture {
	UiFontLoadFuture
}

//==============================================================================
/// UI 글꼴 불러오기 손잡이. (load_ui_font 가 돌려준다. 원본 loadUiFont 의 Promise 자리)
/// - await 하면 처음 글자를 다 올렸거나 wait_milliseconds 가 지날 때까지 기다린다.
///
/// # 예제
/// 글꼴 목록이 없으면 바로 끝나고 시스템 글꼴로 이어 간다
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{uifont, Engine, EngineConfiguration, UiFontOptions};
///
/// uifont::configure_ui_font(UiFontOptions {
/// 	manifest_path: Some(String::from("missing-fonts/fonts.json")),
/// 	..UiFontOptions::default()
/// });
/// let mut engine = Engine::new(EngineConfiguration::new());
/// let is_ready = Rc::new(Cell::new(false));
/// let is_ready_for_task = is_ready.clone();
/// engine.spawn_task(move |mut engine_context| async move {
/// 	engine_context.with(|engine| uifont::load_ui_font(engine, "korean", "Start")).await;
/// 	is_ready_for_task.set(true);
/// });
/// assert!(is_ready.get());
/// assert!(uifont::is_ui_font_ready());
/// assert_eq!(uifont::read_ui_font_face(), None);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct UiFontLoadHandle;

//==============================================================================
/// UI 글꼴 처음 불러오기를 기다리는 퓨처.
/// - UiFontLoadHandle 을 await 하거나 wait_for_load 가 돌려준다. 폴링할 때마다 update 를 부른다.
///
/// # 예제
/// 불러오기를 시작하지 않았으면 바로 끝난다
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{uifont, Engine, EngineConfiguration};
///
/// let mut engine = Engine::new(EngineConfiguration::new());
/// let is_finished = Rc::new(Cell::new(false));
/// let is_finished_for_task = is_finished.clone();
/// engine.spawn_task(move |_engine_context| async move {
/// 	uifont::wait_for_load().await;
/// 	is_finished_for_task.set(true);
/// });
/// assert!(is_finished.get());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct UiFontLoadFuture;

impl Future for UiFontLoadFuture {
	type Output = ();

	//==============================================================================
	/// update 를 부르고 준비됐는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut UiFontLoadFuture>, context: &mut Context<'_>) -> Poll<()> {
		task::with_polling_engine(update);
		if is_ui_font_ready() {
			return Poll::Ready(());
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

impl IntoFuture for UiFontLoadHandle {
	type Output = ();
	type IntoFuture = UiFontLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> UiFontLoadFuture {
		UiFontLoadFuture
	}
}

//==============================================================================
/// 글자 조각 받기 손잡이. (load_ui_font_text 가 돌려준다. 원본 loadUiFontText 의 Promise 자리, 청한 얼굴을 모두 올리면 끝난다)
///
/// # 예제
/// 끝났는지 묻거나 그대로 await 하기
/// ```rust
/// use vanilla::{uifont, Engine, EngineConfiguration};
///
/// let mut engine = Engine::new(EngineConfiguration::new());
/// let text_load_handle = uifont::load_ui_font_text(&mut engine, "Ready");
/// if !text_load_handle.is_finished() {
/// 	engine.spawn_task(move |_engine_context| async move {
/// 		text_load_handle.await;
/// 	});
/// }
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct UiFontTextLoadHandle {
	font_load_handle_list: Vec<FontLoadHandle>,
}

impl UiFontTextLoadHandle {
	//==============================================================================
	/// 청한 얼굴의 글꼴 받기 손잡이 목록. (씬에 맡길 때는 저마다의 get_load_handle)
	///
	/// # 돌려줌
	/// 청한 얼굴마다 하나인 글꼴 받기 손잡이 목록. 청한 것이 없으면 빈 목록.
	//==============================================================================
	pub fn get_font_load_handles(&self) -> &[FontLoadHandle] {
		&self.font_load_handle_list
	}

	//==============================================================================
	/// 다 끝났는지. (올렸거나 실패했거나, 청한 것이 없으면 참)
	///
	/// # 돌려줌
	/// 모든 손잡이가 끝났으면 true.
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.font_load_handle_list.iter().all(|font_load_handle| font_load_handle.is_finished())
	}
}

//==============================================================================
/// 글자 조각 받기를 기다리는 퓨처. (원본처럼 받지 못한 조각이 있어도 실패로 끝내지 않는다)
/// - UiFontTextLoadHandle 을 await 하면 이것이 된다.
///
/// # 예제
/// 청한 것이 없으면 첫 폴링에 끝난다
/// ```rust
/// use std::cell::Cell;
/// use std::future::IntoFuture;
/// use std::rc::Rc;
/// use vanilla::{uifont, Engine, EngineConfiguration, UiFontTextLoadFuture};
///
/// let mut engine = Engine::new(EngineConfiguration::new());
/// let text_load_future: UiFontTextLoadFuture = uifont::load_ui_font_text(&mut engine, "Ready").into_future();
/// let is_finished = Rc::new(Cell::new(false));
/// let is_finished_for_task = is_finished.clone();
/// engine.spawn_task(move |_engine_context| async move {
/// 	text_load_future.await;
/// 	is_finished_for_task.set(true);
/// });
/// assert!(is_finished.get());
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct UiFontTextLoadFuture {
	text_load_handle: UiFontTextLoadHandle,
}

impl Future for UiFontTextLoadFuture {
	type Output = ();

	//==============================================================================
	/// 다 끝났는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut UiFontTextLoadFuture>, context: &mut Context<'_>) -> Poll<()> {
		if self.text_load_handle.is_finished() {
			return Poll::Ready(());
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

impl IntoFuture for UiFontTextLoadHandle {
	type Output = ();
	type IntoFuture = UiFontTextLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> UiFontTextLoadFuture {
		UiFontTextLoadFuture {
			text_load_handle: self,
		}
	}
}

//==============================================================================
/// 처음 불러오기가 준비됐는지. (처음 글자의 조각을 다 받았거나 wait_milliseconds 가 지났으면 참, 원본 loadUiFont 의 기다림)
/// - 불러오기를 시작하지 않았거나 목록을 못 읽었으면 참이다. (시스템 글꼴로 이어 간다)
///
/// # 돌려줌
/// 처음 불러오기가 준비됐으면 true.
//==============================================================================
pub fn is_ui_font_ready() -> bool {
	with_state(|state| {
		let started_instant = match state.initial_load_instant {
			Some(started_instant) => {
				started_instant
			}
			None => {
				return true;
			}
		};
		if started_instant.elapsed().as_secs_f64() * 1000.0 >= state.wait_milliseconds {
			return true;
		}
		if state.pending_initial_load.is_some() {
			return false;
		}
		state.initial_text_handle_list.iter().all(|font_load_handle| font_load_handle.is_finished())
	})
}

//==============================================================================
/// 불러온 UI 글꼴의 이름. (엔진의 글자 컴포넌트에 set_font_family 로 준다, 못 불러왔으면 None. 원본 readUiFontFace)
///
/// # 돌려줌
/// 처음 묶음을 받아 올렸으면 글꼴 이름. 아직이거나 못 불러왔으면 None.
//==============================================================================
pub fn read_ui_font_face() -> Option<String> {
	with_state(|state| {
		if state.is_loaded {
			Some(state.family.clone())
		}
		else {
			None
		}
	})
}

//==============================================================================
/// UI 글꼴 문자열. (굵기, 크기. 원본 buildUiFontString)
///
/// # 인자
/// - `font_weight`: 글자 굵기. (500, 700 같은 CSS 굵기)
/// - `font_size`: 글자 크기. (픽셀)
///
/// # 돌려줌
/// Canvas2D font 꼴 글. (`500 16px "GameUI", sans-serif` 꼴)
//==============================================================================
pub fn build_ui_font_string(font_weight: u16, font_size: f64) -> String {
	let family = read_ui_font_family();
	format!("{} {}px \"{}\", sans-serif", font_weight, font_size, family)
}

//==============================================================================
/// UI 글꼴 설정. (불러오기 전에 부른다, None 인 칸은 그대로. 원본 configureUiFont)
/// - primary_group 을 주면 지금 고른 묶음도 그것으로 돌린다.
///
/// # 인자
/// - `options`: 바꿀 칸만 Some 으로 채운 설정.
//==============================================================================
pub fn configure_ui_font(options: UiFontOptions) {
	with_state(|state| {
		if let Some(family) = options.family {
			state.family = family;
		}
		if let Some(manifest_path) = options.manifest_path {
			state.manifest_path = manifest_path;
		}
		if let Some(directory_path) = options.directory_path {
			state.directory_path = directory_path;
		}
		if let Some(primary_group) = options.primary_group {
			state.current_group = primary_group.clone();
			state.primary_group = primary_group;
		}
		if let Some(weights) = options.weights {
			state.weights = weights;
		}
		if let Some(wait_milliseconds) = options.wait_milliseconds {
			state.wait_milliseconds = wait_milliseconds;
		}
		if let Some(remaining_delay_milliseconds) = options.remaining_delay_milliseconds {
			state.remaining_delay_milliseconds = remaining_delay_milliseconds;
		}
	});
}

//==============================================================================
/// UI 글꼴 이름 반환. (원본 readUiFontFamily)
///
/// # 돌려줌
/// 올릴 글꼴 이름. (처음은 "GameUI", 불러오지 않았어도 돌려준다)
//==============================================================================
pub fn read_ui_font_family() -> String {
	with_state(|state| state.family.clone())
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 조각의 굵기 → 올릴 굵기. (원본 readTargetWeights 와 같은 결과)
	//==============================================================================
	#[test]
	fn target_weights_like_original() {
		let weights = [500, 700];
		assert_eq!(read_target_weights("400 700", &weights), vec![500, 700]);
		assert_eq!(read_target_weights("400", &weights), vec![500]);
		assert_eq!(read_target_weights("700", &weights), vec![700]);
		assert_eq!(read_target_weights("100 450", &weights), Vec::<u16>::new());
		assert_eq!(build_ui_font_string(500, 16.0), "500 16px \"GameUI\", sans-serif");
	}
}

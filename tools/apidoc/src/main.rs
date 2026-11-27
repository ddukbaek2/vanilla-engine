//==============================================================================
//! API 문서 자료 뽑기. (vanilla.js 의 tools/apidoc.cjs 를 러스트 판에 맞게 옮긴 것)
//! - 엔진 크레이트(src/lib.rs)에서 pub mod 와 pub use 로 밖에서 닿는 공개 API(구조체, 트레이트, 꼴 별칭, 열거, 함수, 상수, 매크로)를
//!   syn 으로 읽어 docs/api/<판>.json 을 쓴다. 다시 내보낸 반사 매크로(vanilla::reflect)는 macros/src/lib.rs 에서 읽는다.
//! - 설명은 rustdoc 문서 주석(///, //!)에서 읽는다. 첫 절이 설명, `# 인자` 는 `` - `이름`: 설명 `` 목록, `# 돌려줌` 은 돌려주는 값,
//!   `# 예제` 는 코드 칸(바로 앞 글이 제목)이다. 문서 주석이 없으면 `//====` 머리 주석의 글을 쓴다. (다른 작업자가 고치는 중이라 아직 바꾸지 않은 파일)
//! - `# 예제` 의 코드 칸 바로 뒤 줄 `<!-- image: 파일 이름 -->` 은 그 예제의 image 칸이다. (HTML 주석이라 rustdoc 에서 보이지 않는다)
//!   그림은 docs/guide/<판>/images/<파일 이름> 에 두고 `--check` 가 있는지 본다.
//! - ko 는 문서 주석 글 그대로(긴 줄표와 가운뎃점만 쉼표로), en 은 docs/api/translation.en.json(ko 원문 → en) 표에서 온다. 한글이 없는 글은 그대로 en 이 된다.
//! - since 는 v0.3.2 태그(러스트 첫 판)의 소스에서 같은 타입에 같은 이름(밑줄과 대소문자를 빼고 비교, 0.3.2 는 camelCase 였다)이 있으면 "0.3.2", 아니면 만들려는 판.
//! - counterpart 는 docs/api/counterpart.<판>.json 의 짝(vanilla.js 의 같은 항목)이다. `--counterpart <vanilla.js 의 docs/api/<판>.json>` 으로 그 표를 다시 짓는다.
//! - 초안 모듈(이름이 experimental 이거나 experimental/ 로 시작하는 것)은 자료와 짝 표에 넣지 않는다. 초안을 가리키는 짝도 뺀다.
//!   밖에서 닿지 않는 모듈의 아이템을 초안 모듈에서만 다시 내보냈으면 그것도 초안이다. (split_draft_modules)
//!   `--check`(초안은 메모리에서 지어 따로 본다), `--gaps`, `--missing`, doctest 는 초안도 본다.
//! - 사용:
//!   ```text
//!   cargo run -p vanilla-engine-apidoc -- <판> [--date YYYY-MM-DD] [--counterpart <vanilla.js 자료>]   자료 쓰기 (가이드 doctest 장 목록 tools/apidoc/src/guide.rs 도)
//!   cargo run -p vanilla-engine-apidoc -- <판> --check                                         자료 꼴 검사와 수 세기, 가이드 ko 와 en 대조, 예제 그림과 가이드 그림 파일 검사
//!   cargo run -p vanilla-engine-apidoc -- <판> --missing [--only <경로 앞>]                     번역 표에 없는 ko 글 (JSON 배열)
//!   cargo run -p vanilla-engine-apidoc -- <판> --gaps [--only <경로 앞>] [--exclude <경로 앞>]   소스에서 문서가 빈 곳
//!   ```
//==============================================================================
use proc_macro2::LineColumn;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use syn::spanned::Spanned;
use syn::visit::Visit;

/// 자료의 엔진 이름.
const ENGINE_NAME: &str = "vanilla-engine";
/// 짝 엔진 이름.
const COUNTERPART_ENGINE_NAME: &str = "vanilla.js";
/// since 를 훑는 첫 판의 태그. (러스트 판의 첫 release)
const FIRST_VERSION_TAG: &str = "v0.3.2";
/// 첫 판.
const FIRST_VERSION: &str = "0.3.2";
/// 시그니처에 값을 그대로 보일 상수의 글자 수 상한. (원본 apidoc 과 같다)
const SHORT_VALUE_LENGTH: usize = 60;
/// 인자 절 머리.
const SECTION_PARAMETERS: &str = "# 인자";
/// 돌려줌 절 머리.
const SECTION_RETURNS: &str = "# 돌려줌";
/// 예제 절 머리.
const SECTION_EXAMPLES: &str = "# 예제";
/// 멤버 종류. (원본 자료와 같은 이름)
const MEMBER_KINDS: [&str; 5] = ["method", "property", "constructor", "static", "field"];
/// 원본의 여러 클래스를 합친 타입이 멤버의 짝을 더 찾아볼 원본 클래스. (ARCHITECTURE.md '원본과의 대응': 씬 매니저는 엔진과 씬에, 노드 셋은 NodeTree 와 WorldNode 에, 터치 레이캐스터는 씬에 합쳤다)
const COUNTERPART_CLASS_ALIASES: [(&str, &[&str]); 3] = [("Engine", &["SceneManager"]), ("SceneBase", &["SceneManager", "TouchRaycaster"]), ("NodeTree", &["WorldNode", "TransformNode", "ComponentNode", "Node"])];
/// 러스트 멤버 하나가 원본의 기본 인자 있는 멤버 하나와 짝이 되는 이름. (러스트 이름, 원본 이름)
const COUNTERPART_MEMBER_ALIASES: [(&str, &str); 1] = [("resolve_with_attachment_kept", "resolve")];
/// 클래스 자리에 두는 것의 종류. (러스트 struct, trait, 꼴 별칭)
const CLASS_KINDS: [&str; 3] = ["struct", "trait", "type"];
/// 초안 모듈 이름. (이 이름이거나 "이름/" 으로 시작하는 모듈은 자료에 넣지 않는다, 두 엔진 같음)
const DRAFT_MODULE_NAME: &str = "experimental";
/// 예제 그림 표시의 앞과 뒤. (`# 예제` 의 코드 칸 바로 뒤 줄 `<!-- image: blend-modes.webp -->`)
const IMAGE_MARKER_PREFIX: &str = "<!-- image:";
const IMAGE_MARKER_SUFFIX: &str = "-->";
/// 그림 파일의 확장자. (사이트와 정한 꼴, docs/guide/<판>/images/ 에 둔다)
const IMAGE_EXTENSIONS: [&str; 4] = [".webp", ".png", ".jpg", ".gif"];

/// 놓인 자리가 틀린 그림 표시. (예제 코드 칸 바로 뒤가 아님, 자료를 쓸 때 알린다)
static MISPLACED_IMAGE_MARKERS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

//==============================================================================
/// 예제 하나. (제목은 예제 칸 바로 앞 글, 코드는 rustdoc 이 숨기는 줄을 뺀 것, 그림은 코드 칸 바로 뒤의 그림 표시로 빈 글이면 없다)
//==============================================================================
#[derive(Clone, Default)]
struct Example {
	title: String,
	code: String,
	language: String,
	image: String,
}

//==============================================================================
/// 문서 주석을 절로 나눈 것.
//==============================================================================
#[derive(Default)]
struct DocumentText {
	description: String,
	parameters: Vec<(String, String)>,
	returns: String,
	examples: Vec<Example>,
}

//==============================================================================
/// 인자 하나.
//==============================================================================
struct Parameter {
	name: String,
	type_text: String,
	description: String,
}

//==============================================================================
/// 짝 엔진의 항목. (모듈, 클래스, 이름, 빈 글은 그 칸이 없다)
//==============================================================================
#[derive(Clone, PartialEq)]
struct CounterpartKey {
	module: String,
	class: String,
	name: String,
}

//==============================================================================
/// 멤버 하나. (메서드, 연관 함수, 연관 상수, 필드, 모듈 함수와 상수)
//==============================================================================
struct Member {
	kind: &'static str,
	name: String,
	signature: String,
	parameters: Vec<Parameter>,
	returns: Option<(String, String)>,
	description: String,
	since: String,
	source: String,
	line: usize,
	deprecated: Option<String>,
	examples: Vec<Example>,
	is_value: bool,
	documented_parameter_names: Vec<String>,
	counterpart: Option<CounterpartKey>,
}

//==============================================================================
/// 클래스 자리 하나. (struct, trait, 꼴 별칭)
//==============================================================================
struct ClassEntry {
	name: String,
	kind: &'static str,
	summary: String,
	signature: String,
	traits: Vec<String>,
	members: Vec<Member>,
	examples: Vec<Example>,
	source: String,
	line: usize,
	counterpart: Option<CounterpartKey>,
}

//==============================================================================
/// 열거 값 하나.
//==============================================================================
struct EnumValue {
	name: String,
	value: String,
	description: String,
	line: usize,
}

//==============================================================================
/// 열거 하나. (impl 블록의 공개 함수는 members 로 둔다)
//==============================================================================
struct EnumEntry {
	name: String,
	summary: String,
	values: Vec<EnumValue>,
	traits: Vec<String>,
	members: Vec<Member>,
	examples: Vec<Example>,
	source: String,
	line: usize,
	counterpart: Option<CounterpartKey>,
}

//==============================================================================
/// 모듈 하나. (파일 하나, 또는 파일 안의 모듈 블록)
//==============================================================================
struct ModuleEntry {
	name: String,
	source: String,
	summary: String,
	examples: Vec<Example>,
	classes: Vec<ClassEntry>,
	functions: Vec<Member>,
	enums: Vec<EnumEntry>,
	is_reachable: bool,
	exported_names: HashSet<String>,
	is_all_exported: bool,
	public_exported_names: HashSet<String>,
	is_all_exported_from_public: bool,
}

//==============================================================================
/// 아직 붙이지 않은 impl 블록. (타입이 다른 모듈에 있을 수 있어 다 읽은 뒤 붙인다)
//==============================================================================
struct PendingImplementation {
	module_index: usize,
	type_name: String,
	trait_text: String,
	members: Vec<Member>,
}

//==============================================================================
/// 첫 판(v0.3.2)에 있던 이름. (since 셈, 이름은 밑줄과 대소문자를 뺀 꼴)
//==============================================================================
#[derive(Default)]
struct FirstVersionNames {
	type_members: HashMap<String, HashSet<String>>,
	function_names: HashSet<String>,
}

//==============================================================================
/// 소스 읽기와 모으기.
//==============================================================================
struct Collector {
	repository_root: PathBuf,
	modules: Vec<ModuleEntry>,
	pending_implementations: Vec<PendingImplementation>,
	pending_exports: Vec<(String, String, String)>,
	source_texts: HashMap<String, Vec<String>>,
}

//==============================================================================
/// 명령줄 갈래.
//==============================================================================
struct Options {
	version: String,
	date: String,
	counterpart_source_path: String,
	is_check: bool,
	is_missing: bool,
	is_gaps: bool,
	only_prefixes: Vec<String>,
	exclude_prefixes: Vec<String>,
}

//==============================================================================
/// 이름을 since 와 짝 찾기에 쓰는 꼴로. (밑줄을 빼고 소문자, set_fill_color 와 setFillColor 가 같다)
//==============================================================================
fn normalize_name(name: &str) -> String {
	name.chars().filter(|character| *character != '_').flat_map(|character| character.to_lowercase()).collect()
}

//==============================================================================
/// snake_case 를 camelCase 로. (set_fill_color → setFillColor)
//==============================================================================
fn convert_snake_to_camel(name: &str) -> String {
	let mut result = String::new();
	let mut is_upper_next = false;
	for character in name.chars() {
		if character == '_' {
			is_upper_next = !result.is_empty();
			continue;
		}
		if is_upper_next {
			result.extend(character.to_uppercase());
			is_upper_next = false;
		}
		else {
			result.push(character);
		}
	}
	result
}

//==============================================================================
/// 글에 한글이 있는지.
//==============================================================================
fn has_hangul(text: &str) -> bool {
	text.chars().any(|character| matches!(character, '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' | '\u{AC00}'..='\u{D7A3}'))
}

//==============================================================================
/// 원문 글 다듬기. (긴 줄표 U+2014 와 가운뎃점 U+00B7 은 문서에 쓰지 않으므로 앞뒤 빈칸과 함께 쉼표로 바꾼다, 그 밖은 원문 그대로)
//==============================================================================
fn normalize_text(text: &str) -> String {
	let mut result = String::new();
	let mut characters = text.chars().peekable();
	while let Some(character) = characters.next() {
		if character == '\u{2014}' || character == '\u{00B7}' {
			while result.ends_with(' ') || result.ends_with('\t') {
				result.pop();
			}
			result.push_str(", ");
			while let Some(next) = characters.peek() {
				if *next == ' ' || *next == '\t' {
					characters.next();
				}
				else {
					break;
				}
			}
			continue;
		}
		result.push(character);
	}
	result
}

//==============================================================================
/// 공개 가시성인지. (pub 만, pub(crate) 같은 것은 밖에서 닿지 않는다)
//==============================================================================
fn is_public(visibility: &syn::Visibility) -> bool {
	matches!(visibility, syn::Visibility::Public(_))
}

//==============================================================================
/// cfg(test) 가 붙었는지.
//==============================================================================
fn has_test_configuration(attributes: &[syn::Attribute]) -> bool {
	attributes.iter().any(|attribute| {
		if !attribute.path().is_ident("cfg") {
			return false;
		}
		if let syn::Meta::List(list) = &attribute.meta {
			return list.tokens.to_string().trim() == "test";
		}
		false
	})
}

//==============================================================================
/// 문서 속성의 줄 목록. (/// 와 //! 모두 doc 속성이다, 없으면 None)
//==============================================================================
fn read_document_lines(attributes: &[syn::Attribute]) -> Option<Vec<String>> {
	let mut lines = Vec::new();
	let mut is_present = false;
	for attribute in attributes {
		if !attribute.path().is_ident("doc") {
			continue;
		}
		if let syn::Meta::NameValue(name_value) = &attribute.meta {
			if let syn::Expr::Lit(expression) = &name_value.value {
				if let syn::Lit::Str(literal) = &expression.lit {
					is_present = true;
					for line in literal.value().split('\n') {
						lines.push(line.to_string());
					}
				}
			}
		}
	}
	if is_present {
		Some(lines)
	}
	else {
		None
	}
}

//==============================================================================
/// 모든 줄에 같이 붙은 앞 빈칸 빼기. (rustdoc 과 같다)
//==============================================================================
fn remove_common_indent(lines: Vec<String>) -> Vec<String> {
	let indent = lines.iter().filter(|line| !line.trim().is_empty()).map(|line| line.chars().take_while(|character| *character == ' ' || *character == '\t').count()).min().unwrap_or(0);
	lines.into_iter().map(|line| {
		if line.trim().is_empty() {
			String::new()
		}
		else {
			line.chars().skip(indent).collect::<String>().trim_end().to_string()
		}
	}).collect()
}

//==============================================================================
/// 앞뒤 빈 줄을 빼고 잇기.
//==============================================================================
fn join_trimmed_lines(lines: &[String]) -> String {
	let start = lines.iter().position(|line| !line.trim().is_empty());
	let end = lines.iter().rposition(|line| !line.trim().is_empty());
	match (start, end) {
		(Some(start), Some(end)) => lines[start..=end].join("\n"),
		_ => String::new(),
	}
}

//==============================================================================
/// 초안 모듈인지. (experimental 이거나 experimental/ 로 시작, 두 엔진의 모듈 이름에 같이 쓴다)
//==============================================================================
fn is_draft_module_name(module_name: &str) -> bool {
	module_name == DRAFT_MODULE_NAME || module_name.strip_prefix(DRAFT_MODULE_NAME).map(|rest| rest.starts_with('/')).unwrap_or(false)
}

//==============================================================================
/// 모듈 목록을 자료에 넣을 것과 초안으로 가르기. ((넣을 것, 초안))
/// - 이름이 초안인 모듈은 통째로 초안이다.
/// - 밖에서 닿지 않는 모듈의 아이템은 초안 아닌 모듈이 다시 내보낸 것만 자료에 넣고, 초안 모듈에서만 다시 내보낸 것은 초안으로 친다.
///   (platform/audio/pannermath 처럼 experimental::spatialaudio 로만 내보낸 것)
//==============================================================================
fn split_draft_modules(modules: Vec<ModuleEntry>) -> (Vec<ModuleEntry>, Vec<ModuleEntry>) {
	let mut document_modules = Vec::new();
	let mut draft_modules = Vec::new();
	for mut module in modules {
		if is_draft_module_name(&module.name) {
			draft_modules.push(module);
			continue;
		}
		if module.is_reachable || module.is_all_exported_from_public {
			document_modules.push(module);
			continue;
		}
		let public_names = module.public_exported_names.clone();
		let (classes, draft_classes): (Vec<ClassEntry>, Vec<ClassEntry>) = std::mem::take(&mut module.classes).into_iter().partition(|class| public_names.contains(&class.name));
		let (enums, draft_enums): (Vec<EnumEntry>, Vec<EnumEntry>) = std::mem::take(&mut module.enums).into_iter().partition(|entry| public_names.contains(&entry.name));
		let (functions, draft_functions): (Vec<Member>, Vec<Member>) = std::mem::take(&mut module.functions).into_iter().partition(|function| public_names.contains(&function.name));
		if !draft_classes.is_empty() || !draft_enums.is_empty() || !draft_functions.is_empty() {
			draft_modules.push(ModuleEntry {
				name: module.name.clone(),
				source: module.source.clone(),
				summary: module.summary.clone(),
				examples: module.examples.clone(),
				classes: draft_classes,
				functions: draft_functions,
				enums: draft_enums,
				is_reachable: false,
				exported_names: module.exported_names.clone(),
				is_all_exported: module.is_all_exported,
				public_exported_names: HashSet::new(),
				is_all_exported_from_public: false,
			});
		}
		if !classes.is_empty() || !enums.is_empty() || !functions.is_empty() {
			module.classes = classes;
			module.enums = enums;
			module.functions = functions;
			document_modules.push(module);
		}
	}
	(document_modules, draft_modules)
}

//==============================================================================
/// 그림 표시 줄에서 파일 이름 읽기. (`<!-- image: blend-modes.webp -->` 면 Some("blend-modes.webp"), 표시가 아니면 None)
//==============================================================================
fn read_image_marker(trimmed_line: &str) -> Option<String> {
	let rest = trimmed_line.strip_prefix(IMAGE_MARKER_PREFIX)?;
	let name = rest.strip_suffix(IMAGE_MARKER_SUFFIX).unwrap_or(rest).trim();
	Some(name.to_string())
}

//==============================================================================
/// 그림 파일 이름이 꼴에 맞는지. (영문 소문자, 숫자, 점, 밑줄, 하이픈과 .webp, .png, .jpg, .gif)
//==============================================================================
fn is_image_name_valid(name: &str) -> bool {
	!name.is_empty() && name.chars().all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, '.' | '_' | '-')) && IMAGE_EXTENSIONS.iter().any(|extension| name.len() > extension.len() && name.ends_with(extension))
}

//==============================================================================
/// 예제 코드에서 rustdoc 이 숨기는 줄 빼기. ("# " 로 시작하거나 "#" 뿐인 줄, "##" 은 "#" 로)
//==============================================================================
fn remove_hidden_example_lines(lines: &[String]) -> String {
	let mut visible = Vec::new();
	for line in lines {
		let trimmed = line.trim_start();
		if trimmed == "#" || trimmed.starts_with("# ") {
			continue;
		}
		if trimmed.starts_with("##") {
			let indent = &line[..line.len() - trimmed.len()];
			visible.push(format!("{}{}", indent, &trimmed[1..]));
			continue;
		}
		visible.push(line.clone());
	}
	let indent = visible.iter().filter(|line| !line.trim().is_empty()).map(|line| line.chars().take_while(|character| *character == ' ' || *character == '\t').count()).min().unwrap_or(0);
	let stripped: Vec<String> = visible.iter().map(|line| line.chars().skip(indent).collect::<String>()).collect();
	join_trimmed_lines(&stripped)
}

//==============================================================================
/// 문서 주석을 절로 나누기.
/// - 설명은 첫 절과 모르는 절(# 인자, # 돌려줌, # 예제 밖의 # 머리)이고, 코드 칸 안의 # 는 머리로 보지 않는다.
//==============================================================================
fn parse_document(raw_lines: Vec<String>) -> DocumentText {
	#[derive(PartialEq)]
	enum Section {
		Description,
		Parameters,
		Returns,
		Examples,
	}
	let lines = remove_common_indent(raw_lines);
	let mut document = DocumentText::default();
	let mut description_lines: Vec<String> = Vec::new();
	let mut returns_lines: Vec<String> = Vec::new();
	let mut section = Section::Description;
	let mut is_in_fence = false;
	let mut example_title_lines: Vec<String> = Vec::new();
	let mut example_code_lines: Vec<String> = Vec::new();
	let mut example_language = String::new();
	let mut is_after_example = false;
	for line in &lines {
		let trimmed = line.trim();
		if !is_in_fence {
			if let Some(image_name) = read_image_marker(trimmed) {
				match document.examples.last_mut() {
					Some(example) if section == Section::Examples && is_after_example && example.image.is_empty() => example.image = image_name,
					_ => MISPLACED_IMAGE_MARKERS.lock().unwrap().push(image_name),
				}
				is_after_example = false;
				continue;
			}
		}
		if !is_in_fence && line.starts_with('#') {
			let heading = line.trim_end();
			if heading == SECTION_PARAMETERS {
				section = Section::Parameters;
				continue;
			}
			if heading == SECTION_RETURNS {
				section = Section::Returns;
				continue;
			}
			if heading == SECTION_EXAMPLES {
				section = Section::Examples;
				continue;
			}
			if heading.starts_with("# ") {
				section = Section::Description;
			}
		}
		let is_fence = trimmed.starts_with("```");
		match section {
			Section::Description => {
				if is_fence {
					is_in_fence = !is_in_fence;
				}
				description_lines.push(line.clone());
			}
			Section::Parameters => {
				if is_fence {
					is_in_fence = !is_in_fence;
				}
				if let Some(rest) = line.strip_prefix("- `") {
					if let Some(close_index) = rest.find('`') {
						let name = rest[..close_index].to_string();
						let after = rest[close_index + 1..].trim_start();
						let text = after.strip_prefix(':').unwrap_or(after).trim().to_string();
						document.parameters.push((name, text));
						continue;
					}
				}
				if !trimmed.is_empty() {
					if let Some(last) = document.parameters.last_mut() {
						if last.1.is_empty() {
							last.1 = trimmed.to_string();
						}
						else {
							last.1.push('\n');
							last.1.push_str(trimmed);
						}
					}
				}
			}
			Section::Returns => {
				if is_fence {
					is_in_fence = !is_in_fence;
				}
				returns_lines.push(line.clone());
			}
			Section::Examples => {
				if is_fence {
					if !is_in_fence {
						is_in_fence = true;
						let information = trimmed.trim_start_matches('`').trim();
						example_language = if information.is_empty() || information.split(',').any(|part| matches!(part.trim(), "rust" | "no_run" | "ignore" | "should_panic" | "compile_fail") || part.trim().starts_with("edition")) {
							String::from("rust")
						}
						else {
							information.split(',').next().unwrap_or("text").trim().to_string()
						};
						example_code_lines.clear();
					}
					else {
						is_in_fence = false;
						let code = if example_language == "rust" {
							remove_hidden_example_lines(&example_code_lines)
						}
						else {
							join_trimmed_lines(&example_code_lines)
						};
						document.examples.push(Example {
							title: join_trimmed_lines(&example_title_lines).replace('\n', " "),
							code: code,
							language: example_language.clone(),
							image: String::new(),
						});
						example_title_lines.clear();
						is_after_example = true;
					}
					continue;
				}
				if is_in_fence {
					example_code_lines.push(line.clone());
				}
				else if trimmed.is_empty() {
					example_title_lines.clear();
				}
				else {
					is_after_example = false;
					example_title_lines.push(trimmed.to_string());
				}
			}
		}
	}
	document.description = join_trimmed_lines(&description_lines);
	document.returns = join_trimmed_lines(&returns_lines);
	document
}

//==============================================================================
/// 줄에서 // 주석 빼기. (문자열 안의 // 는 둔다)
//==============================================================================
fn remove_line_comment(line: &str) -> String {
	let characters: Vec<char> = line.chars().collect();
	let mut is_in_string = false;
	let mut index = 0;
	while index < characters.len() {
		let character = characters[index];
		if is_in_string {
			if character == '\\' {
				index += 2;
				continue;
			}
			if character == '"' {
				is_in_string = false;
			}
		}
		else if character == '"' {
			is_in_string = true;
		}
		else if character == '/' && index + 1 < characters.len() && characters[index + 1] == '/' {
			return characters[..index].iter().collect();
		}
		index += 1;
	}
	line.to_string()
}

//==============================================================================
/// 여러 줄 글을 한 줄로. (빈칸을 하나로, 괄호 안쪽의 빈칸과 끝 쉼표를 뺀다)
//==============================================================================
fn collapse_whitespace(text: &str) -> String {
	let mut result = text.split_whitespace().collect::<Vec<&str>>().join(" ");
	for (from, to) in [("( ", "("), (" )", ")"), (",)", ")"), ("[ ", "["), (" ]", "]"), (",]", "]"), ("< ", "<"), (" >", ">"), (",>", ">")] {
		while result.contains(from) {
			result = result.replace(from, to);
		}
	}
	result
}

impl Collector {
	//==============================================================================
	/// 모으기 만들기.
	//==============================================================================
	fn new(repository_root: PathBuf) -> Collector {
		Collector {
			repository_root: repository_root,
			modules: Vec::new(),
			pending_implementations: Vec::new(),
			pending_exports: Vec::new(),
			source_texts: HashMap::new(),
		}
	}

	//==============================================================================
	/// 소스 글의 줄과 칸 범위를 글로. (줄은 1 부터, 칸은 0 부터의 글자 수, 줄 주석을 빼고 한 줄로)
	//==============================================================================
	fn slice_text(&self, source: &str, start: LineColumn, end: LineColumn) -> String {
		let lines = match self.source_texts.get(source) {
			Some(lines) => lines,
			None => {
				return String::new();
			}
		};
		let mut pieces = Vec::new();
		for line_number in start.line..=end.line {
			if line_number == 0 || line_number > lines.len() {
				continue;
			}
			let characters: Vec<char> = lines[line_number - 1].chars().collect();
			let from = if line_number == start.line {
				start.column.min(characters.len())
			}
			else {
				0
			};
			let to = if line_number == end.line {
				end.column.min(characters.len())
			}
			else {
				characters.len()
			};
			if from < to {
				pieces.push(remove_line_comment(&characters[from..to].iter().collect::<String>()));
			}
		}
		collapse_whitespace(&pieces.join(" "))
	}

	//==============================================================================
	/// 토큰 묶음의 소스 글.
	//==============================================================================
	fn read_node_text<T: Spanned>(&self, source: &str, node: &T) -> String {
		let span = node.span();
		self.slice_text(source, span.start(), span.end())
	}

	//==============================================================================
	/// 문서 주석이 없을 때 아이템 위의 `//====` 머리 주석 글. (아이템 첫 줄 바로 위부터 속성 줄을 건너뛰고 읽는다)
	//==============================================================================
	fn read_header_comment(&self, source: &str, item_line: usize) -> Option<(String, usize)> {
		let lines = self.source_texts.get(source)?;
		if item_line < 2 {
			return None;
		}
		let is_border = |line: &str| {
			let trimmed = line.trim();
			trimmed.starts_with("//==========")
		};
		let is_plain_comment = |line: &str| {
			let trimmed = line.trim();
			trimmed.starts_with("//") && !trimmed.starts_with("///") && !trimmed.starts_with("//!")
		};
		let mut index = item_line - 1;
		while index > 0 && lines[index - 1].trim().starts_with("#[") {
			index -= 1;
		}
		let mut collected: Vec<String> = Vec::new();
		while index > 0 && is_plain_comment(&lines[index - 1]) && !is_border(&lines[index - 1]) {
			collected.insert(0, lines[index - 1].clone());
			index -= 1;
		}
		if index > 0 && is_border(&lines[index - 1]) {
			index -= 1;
			let mut box_lines: Vec<String> = Vec::new();
			while index > 0 && is_plain_comment(&lines[index - 1]) && !is_border(&lines[index - 1]) {
				box_lines.insert(0, lines[index - 1].clone());
				index -= 1;
			}
			if index > 0 && is_border(&lines[index - 1]) {
				box_lines.extend(collected);
				collected = box_lines;
			}
		}
		if collected.is_empty() {
			return None;
		}
		let text_lines: Vec<String> = collected.iter().map(|line| {
			let trimmed = line.trim_start();
			let rest = &trimmed[2..];
			rest.strip_prefix(' ').unwrap_or(rest).to_string()
		}).collect();
		Some((join_trimmed_lines(&text_lines), index + 1))
	}

	//==============================================================================
	/// 아이템의 문서. (문서 주석, 없으면 머리 주석의 글을 설명으로)
	//==============================================================================
	fn read_document<T: Spanned>(&self, source: &str, attributes: &[syn::Attribute], node: &T) -> DocumentText {
		if let Some(lines) = read_document_lines(attributes) {
			return parse_document(lines);
		}
		let item_line = node.span().start().line;
		if let Some((text, _)) = self.read_header_comment(source, item_line) {
			return DocumentText {
				description: text,
				..DocumentText::default()
			};
		}
		DocumentText::default()
	}

	//==============================================================================
	/// 함수 시그니처로 멤버 짓기.
	//==============================================================================
	fn build_function_member(&self, source: &str, visibility: &syn::Visibility, signature: &syn::Signature, attributes: &[syn::Attribute], node_span: proc_macro2::Span, kind_override: Option<&'static str>) -> Member {
		let document = if let Some(lines) = read_document_lines(attributes) {
			parse_document(lines)
		}
		else if let Some((text, _)) = self.read_header_comment(source, node_span.start().line) {
			DocumentText {
				description: text,
				..DocumentText::default()
			}
		}
		else {
			DocumentText::default()
		};
		let name = signature.ident.to_string();
		let has_receiver = signature.receiver().is_some();
		let kind = match kind_override {
			Some(kind) => kind,
			None => {
				if has_receiver {
					"method"
				}
				else if name == "new" {
					"constructor"
				}
				else {
					"static"
				}
			}
		};
		let start = if matches!(visibility, syn::Visibility::Inherited) {
			signature.span().start()
		}
		else {
			visibility.span().start()
		};
		let signature_text = self.slice_text(source, start, signature.span().end());
		let mut parameters = Vec::new();
		for input in &signature.inputs {
			if let syn::FnArg::Typed(pattern_type) = input {
				let parameter_name = match &*pattern_type.pat {
					syn::Pat::Ident(pattern) => pattern.ident.to_string(),
					other => self.read_node_text(source, other),
				};
				let description = document.parameters.iter().find(|(documented_name, _)| *documented_name == parameter_name).map(|(_, text)| text.clone()).unwrap_or_default();
				parameters.push(Parameter {
					name: parameter_name,
					type_text: self.read_node_text(source, &*pattern_type.ty),
					description: description,
				});
			}
		}
		let returns = match &signature.output {
			syn::ReturnType::Default => None,
			syn::ReturnType::Type(_, return_type) => {
				let type_text = self.read_node_text(source, &**return_type);
				if type_text == "()" {
					None
				}
				else {
					Some((type_text, document.returns.clone()))
				}
			}
		};
		Member {
			kind: kind,
			name: name,
			signature: signature_text,
			parameters: parameters,
			returns: returns,
			description: document.description,
			since: String::new(),
			source: source.to_string(),
			line: signature.ident.span().start().line,
			deprecated: read_deprecated(attributes),
			examples: document.examples,
			is_value: false,
			documented_parameter_names: document.parameters.iter().map(|(name, _)| name.clone()).collect(),
			counterpart: None,
		}
	}

	//==============================================================================
	/// 값 멤버 짓기. (상수, static, 필드. returns 는 꼴만 담는다)
	//==============================================================================
	fn build_value_member(&self, source: &str, kind: &'static str, name: String, signature: String, type_text: String, attributes: &[syn::Attribute], node_span: proc_macro2::Span, line: usize) -> Member {
		let document = if let Some(lines) = read_document_lines(attributes) {
			parse_document(lines)
		}
		else if let Some((text, _)) = self.read_header_comment(source, node_span.start().line) {
			DocumentText {
				description: text,
				..DocumentText::default()
			}
		}
		else {
			DocumentText::default()
		};
		Member {
			kind: kind,
			name: name,
			signature: signature,
			parameters: Vec::new(),
			returns: Some((type_text, String::new())),
			description: document.description,
			since: String::new(),
			source: source.to_string(),
			line: line,
			deprecated: read_deprecated(attributes),
			examples: document.examples,
			is_value: true,
			documented_parameter_names: Vec::new(),
			counterpart: None,
		}
	}

	//==============================================================================
	/// 상수 시그니처. (값이 짧으면 값까지)
	//==============================================================================
	fn build_constant_signature(&self, source: &str, visibility: &syn::Visibility, keyword: &str, name: &str, type_node: &syn::Type, value: &syn::Expr) -> String {
		let type_text = self.read_node_text(source, type_node);
		let value_text = self.read_node_text(source, value);
		let visibility_text = if is_public(visibility) {
			"pub "
		}
		else {
			""
		};
		if value_text.chars().count() <= SHORT_VALUE_LENGTH {
			format!("{}{} {}: {} = {}", visibility_text, keyword, name, type_text, value_text)
		}
		else {
			format!("{}{} {}: {}", visibility_text, keyword, name, type_text)
		}
	}

	//==============================================================================
	/// 파일 하나를 모듈로 읽기.
	//==============================================================================
	fn load_module(&mut self, file_path: &Path, module_name: &str, child_directory: &Path, is_reachable: bool) {
		let source_text = match fs::read_to_string(file_path) {
			Ok(text) => text,
			Err(error) => {
				eprintln!("읽지 못함: {} ({})", file_path.display(), error);
				return;
			}
		};
		let source = file_path.strip_prefix(&self.repository_root).unwrap_or(file_path).to_string_lossy().replace('\\', "/");
		self.source_texts.insert(source.clone(), source_text.lines().map(|line| line.to_string()).collect());
		let file = match syn::parse_file(&source_text) {
			Ok(file) => file,
			Err(error) => {
				eprintln!("해석하지 못함: {} ({})", source, error);
				return;
			}
		};
		let document = if let Some(lines) = read_document_lines(&file.attrs) {
			parse_document(lines)
		}
		else {
			DocumentText::default()
		};
		let module_index = self.modules.len();
		self.modules.push(ModuleEntry {
			name: module_name.to_string(),
			source: source.clone(),
			summary: document.description,
			examples: document.examples,
			classes: Vec::new(),
			functions: Vec::new(),
			enums: Vec::new(),
			is_reachable: is_reachable,
			exported_names: HashSet::new(),
			is_all_exported: false,
			public_exported_names: HashSet::new(),
			is_all_exported_from_public: false,
		});
		self.process_items(&file.items, module_index, module_name, child_directory, is_reachable, &source);
	}

	//==============================================================================
	/// 모듈의 아이템 모으기.
	//==============================================================================
	fn process_items(&mut self, items: &[syn::Item], module_index: usize, module_name: &str, child_directory: &Path, is_reachable: bool, source: &str) {
		for item in items {
			match item {
				syn::Item::Mod(item_module) => {
					if has_test_configuration(&item_module.attrs) {
						continue;
					}
					let child_name = item_module.ident.to_string();
					let child_module_name = if module_name.is_empty() {
						child_name.clone()
					}
					else {
						format!("{}/{}", module_name, child_name)
					};
					let is_child_reachable = is_reachable && is_public(&item_module.vis);
					let next_directory = child_directory.join(&child_name);
					if let Some((_, inner_items)) = &item_module.content {
						let document = self.read_document(source, &item_module.attrs, item_module);
						let inner_index = self.modules.len();
						self.modules.push(ModuleEntry {
							name: child_module_name.clone(),
							source: source.to_string(),
							summary: document.description,
							examples: document.examples,
							classes: Vec::new(),
							functions: Vec::new(),
							enums: Vec::new(),
							is_reachable: is_child_reachable,
							exported_names: HashSet::new(),
							is_all_exported: false,
							public_exported_names: HashSet::new(),
							is_all_exported_from_public: false,
						});
						self.process_items(inner_items, inner_index, &child_module_name, &next_directory, is_child_reachable, source);
					}
					else {
						let file_path = child_directory.join(format!("{}.rs", child_name));
						let module_file_path = child_directory.join(&child_name).join("mod.rs");
						if file_path.exists() {
							self.load_module(&file_path, &child_module_name, &next_directory, is_child_reachable);
						}
						else if module_file_path.exists() {
							self.load_module(&module_file_path, &child_module_name, &next_directory, is_child_reachable);
						}
						else {
							eprintln!("모듈 파일 없음: {} ({})", child_module_name, file_path.display());
						}
					}
				}
				syn::Item::Struct(item_struct) => {
					if !is_public(&item_struct.vis) {
						continue;
					}
					let document = self.read_document(source, &item_struct.attrs, item_struct);
					let mut members = Vec::new();
					for (field_index, field) in item_struct.fields.iter().enumerate() {
						if !is_public(&field.vis) {
							continue;
						}
						let field_name = match &field.ident {
							Some(identifier) => identifier.to_string(),
							None => field_index.to_string(),
						};
						let type_text = self.read_node_text(source, &field.ty);
						let signature = format!("pub {}: {}", field_name, type_text);
						let line = field.span().start().line;
						members.push(self.build_value_member(source, "field", field_name, signature, type_text, &field.attrs, field.span(), line));
					}
					self.modules[module_index].classes.push(ClassEntry {
						name: item_struct.ident.to_string(),
						kind: "struct",
						summary: document.description,
						signature: String::new(),
						traits: read_derived_traits(&item_struct.attrs),
						members: members,
						examples: document.examples,
						source: source.to_string(),
						line: item_struct.ident.span().start().line,
						counterpart: None,
					});
				}
				syn::Item::Enum(item_enum) => {
					if !is_public(&item_enum.vis) {
						continue;
					}
					let document = self.read_document(source, &item_enum.attrs, item_enum);
					let mut values = Vec::new();
					for variant in &item_enum.variants {
						let variant_document = self.read_document(source, &variant.attrs, variant);
						let value = if let Some((_, discriminant)) = &variant.discriminant {
							self.read_node_text(source, discriminant)
						}
						else {
							match &variant.fields {
								syn::Fields::Unit => String::new(),
								fields => format!("{}{}", variant.ident, self.read_node_text(source, fields)),
							}
						};
						values.push(EnumValue {
							name: variant.ident.to_string(),
							value: value,
							description: variant_document.description,
							line: variant.ident.span().start().line,
						});
					}
					self.modules[module_index].enums.push(EnumEntry {
						name: item_enum.ident.to_string(),
						summary: document.description,
						values: values,
						traits: read_derived_traits(&item_enum.attrs),
						members: Vec::new(),
						examples: document.examples,
						source: source.to_string(),
						line: item_enum.ident.span().start().line,
						counterpart: None,
					});
				}
				syn::Item::Trait(item_trait) => {
					if !is_public(&item_trait.vis) {
						continue;
					}
					let document = self.read_document(source, &item_trait.attrs, item_trait);
					let mut members = Vec::new();
					for trait_item in &item_trait.items {
						match trait_item {
							syn::TraitItem::Fn(trait_function) => {
								members.push(self.build_function_member(source, &syn::Visibility::Inherited, &trait_function.sig, &trait_function.attrs, trait_function.span(), None));
							}
							syn::TraitItem::Const(trait_constant) => {
								let type_text = self.read_node_text(source, &trait_constant.ty);
								let signature = format!("const {}: {}", trait_constant.ident, type_text);
								let line = trait_constant.ident.span().start().line;
								members.push(self.build_value_member(source, "static", trait_constant.ident.to_string(), signature, type_text, &trait_constant.attrs, trait_constant.span(), line));
							}
							_ => {}
						}
					}
					let supertraits: Vec<String> = item_trait.supertraits.iter().map(|bound| self.read_node_text(source, bound)).collect();
					self.modules[module_index].classes.push(ClassEntry {
						name: item_trait.ident.to_string(),
						kind: "trait",
						summary: document.description,
						signature: String::new(),
						traits: supertraits,
						members: members,
						examples: document.examples,
						source: source.to_string(),
						line: item_trait.ident.span().start().line,
						counterpart: None,
					});
				}
				syn::Item::Type(item_type) => {
					if !is_public(&item_type.vis) {
						continue;
					}
					let document = self.read_document(source, &item_type.attrs, item_type);
					let start = item_type.vis.span().start();
					let end = item_type.ty.span().end();
					let signature = self.slice_text(source, start, end);
					self.modules[module_index].classes.push(ClassEntry {
						name: item_type.ident.to_string(),
						kind: "type",
						summary: document.description,
						signature: signature,
						traits: Vec::new(),
						members: Vec::new(),
						examples: document.examples,
						source: source.to_string(),
						line: item_type.ident.span().start().line,
						counterpart: None,
					});
				}
				syn::Item::Fn(item_function) => {
					if !is_public(&item_function.vis) {
						continue;
					}
					let member = self.build_function_member(source, &item_function.vis, &item_function.sig, &item_function.attrs, item_function.span(), Some("method"));
					self.modules[module_index].functions.push(member);
				}
				syn::Item::Const(item_constant) => {
					if !is_public(&item_constant.vis) {
						continue;
					}
					let name = item_constant.ident.to_string();
					let signature = self.build_constant_signature(source, &item_constant.vis, "const", &name, &item_constant.ty, &item_constant.expr);
					let type_text = self.read_node_text(source, &*item_constant.ty);
					let line = item_constant.ident.span().start().line;
					let member = self.build_value_member(source, "field", name, signature, type_text, &item_constant.attrs, item_constant.span(), line);
					self.modules[module_index].functions.push(member);
				}
				syn::Item::Static(item_static) => {
					if !is_public(&item_static.vis) {
						continue;
					}
					let name = item_static.ident.to_string();
					let signature = self.build_constant_signature(source, &item_static.vis, "static", &name, &item_static.ty, &item_static.expr);
					let type_text = self.read_node_text(source, &*item_static.ty);
					let line = item_static.ident.span().start().line;
					let member = self.build_value_member(source, "field", name, signature, type_text, &item_static.attrs, item_static.span(), line);
					self.modules[module_index].functions.push(member);
				}
				syn::Item::Macro(item_macro) => {
					let is_exported = item_macro.attrs.iter().any(|attribute| attribute.path().is_ident("macro_export"));
					if let (true, Some(identifier)) = (is_exported, &item_macro.ident) {
						let document = self.read_document(source, &item_macro.attrs, item_macro);
						self.modules[module_index].functions.push(Member {
							kind: "method",
							name: format!("{}!", identifier),
							signature: format!("macro_rules! {}", identifier),
							parameters: Vec::new(),
							returns: None,
							description: document.description,
							since: String::new(),
							source: source.to_string(),
							line: identifier.span().start().line,
							deprecated: read_deprecated(&item_macro.attrs),
							examples: document.examples,
							is_value: false,
							documented_parameter_names: Vec::new(),
							counterpart: None,
						});
					}
				}
				syn::Item::Impl(item_implementation) => {
					let type_name = match &*item_implementation.self_ty {
						syn::Type::Path(type_path) => match type_path.path.segments.last() {
							Some(segment) => segment.ident.to_string(),
							None => continue,
						},
						_ => continue,
					};
					let trait_text = match &item_implementation.trait_ {
						Some((_, path, _)) => self.read_node_text(source, path),
						None => String::new(),
					};
					let mut members = Vec::new();
					if trait_text.is_empty() {
						for implementation_item in &item_implementation.items {
							match implementation_item {
								syn::ImplItem::Fn(implementation_function) => {
									if !is_public(&implementation_function.vis) {
										continue;
									}
									members.push(self.build_function_member(source, &implementation_function.vis, &implementation_function.sig, &implementation_function.attrs, implementation_function.span(), None));
								}
								syn::ImplItem::Const(implementation_constant) => {
									if !is_public(&implementation_constant.vis) {
										continue;
									}
									let name = implementation_constant.ident.to_string();
									let signature = self.build_constant_signature(source, &implementation_constant.vis, "const", &name, &implementation_constant.ty, &implementation_constant.expr);
									let type_text = self.read_node_text(source, &implementation_constant.ty);
									let line = implementation_constant.ident.span().start().line;
									members.push(self.build_value_member(source, "static", name, signature, type_text, &implementation_constant.attrs, implementation_constant.span(), line));
								}
								_ => {}
							}
						}
					}
					self.pending_implementations.push(PendingImplementation {
						module_index: module_index,
						type_name: type_name,
						trait_text: trait_text,
						members: members,
					});
				}
				syn::Item::Use(item_use) => {
					if !is_public(&item_use.vis) {
						continue;
					}
					let mut paths = Vec::new();
					collect_use_paths(&item_use.tree, Vec::new(), &mut paths);
					for segments in paths {
						if let Some((module_path, item_name)) = resolve_use_path(module_name, &segments) {
							self.pending_exports.push((module_path, item_name, module_name.to_string()));
						}
					}
				}
				_ => {}
			}
		}
	}

	//==============================================================================
	/// 반사 매크로 크레이트 읽기. (엔진이 다시 내보낸 절차 매크로만, 모듈 이름은 macros)
	//==============================================================================
	fn load_macro_crate(&mut self, exported_names: &HashSet<String>) {
		let file_path = self.repository_root.join("macros/src/lib.rs");
		let source_text = match fs::read_to_string(&file_path) {
			Ok(text) => text,
			Err(_) => {
				return;
			}
		};
		let source = String::from("macros/src/lib.rs");
		self.source_texts.insert(source.clone(), source_text.lines().map(|line| line.to_string()).collect());
		let file = match syn::parse_file(&source_text) {
			Ok(file) => file,
			Err(error) => {
				eprintln!("해석하지 못함: {} ({})", source, error);
				return;
			}
		};
		let document = if let Some(lines) = read_document_lines(&file.attrs) {
			parse_document(lines)
		}
		else {
			DocumentText::default()
		};
		let mut functions = Vec::new();
		for item in &file.items {
			if let syn::Item::Fn(item_function) = item {
				let name = item_function.sig.ident.to_string();
				let is_procedural_macro = item_function.attrs.iter().any(|attribute| attribute.path().is_ident("proc_macro_attribute") || attribute.path().is_ident("proc_macro") || attribute.path().is_ident("proc_macro_derive"));
				if is_procedural_macro && exported_names.contains(&name) {
					let mut member = self.build_function_member(&source, &item_function.vis, &item_function.sig, &item_function.attrs, item_function.span(), Some("method"));
					member.signature = format!("#[{}]", name);
					member.parameters.clear();
					member.returns = None;
					functions.push(member);
				}
			}
		}
		if functions.is_empty() {
			return;
		}
		self.modules.push(ModuleEntry {
			name: String::from("macros"),
			source: source,
			summary: document.description,
			examples: document.examples,
			classes: Vec::new(),
			functions: functions,
			enums: Vec::new(),
			is_reachable: true,
			exported_names: HashSet::new(),
			is_all_exported: false,
			public_exported_names: HashSet::new(),
			is_all_exported_from_public: false,
		});
	}

	//==============================================================================
	/// 다 읽은 뒤 impl 블록과 다시 내보낸 이름 붙이기.
	//==============================================================================
	fn resolve(&mut self) {
		let pending = std::mem::take(&mut self.pending_implementations);
		for implementation in pending {
			let mut target: Option<(usize, bool, usize)> = None;
			let mut candidate_modules: Vec<usize> = vec![implementation.module_index];
			candidate_modules.extend((0..self.modules.len()).filter(|index| *index != implementation.module_index && self.modules[*index].is_reachable));
			candidate_modules.extend((0..self.modules.len()).filter(|index| *index != implementation.module_index && !self.modules[*index].is_reachable));
			for module_index in candidate_modules {
				let module = &self.modules[module_index];
				if let Some(class_index) = module.classes.iter().position(|class| class.name == implementation.type_name) {
					target = Some((module_index, true, class_index));
					break;
				}
				if let Some(enum_index) = module.enums.iter().position(|entry| entry.name == implementation.type_name) {
					target = Some((module_index, false, enum_index));
					break;
				}
			}
			let (module_index, is_class, entry_index) = match target {
				Some(found) => found,
				None => continue,
			};
			let trait_name = implementation.trait_text.clone();
			if is_class {
				let class = &mut self.modules[module_index].classes[entry_index];
				if !trait_name.is_empty() && !class.traits.contains(&trait_name) {
					class.traits.push(trait_name);
				}
				class.members.extend(implementation.members);
			}
			else {
				let entry = &mut self.modules[module_index].enums[entry_index];
				if !trait_name.is_empty() && !entry.traits.contains(&trait_name) {
					entry.traits.push(trait_name);
				}
				entry.members.extend(implementation.members);
			}
		}
		let exports = std::mem::take(&mut self.pending_exports);
		for (module_path, item_name, exporter_module_name) in exports {
			if let Some(module) = self.modules.iter_mut().find(|module| module.name == module_path) {
				let is_public_exporter = !is_draft_module_name(&exporter_module_name);
				if item_name == "*" {
					module.is_all_exported = true;
					module.is_all_exported_from_public |= is_public_exporter;
				}
				else {
					if is_public_exporter {
						module.public_exported_names.insert(item_name.clone());
					}
					module.exported_names.insert(item_name);
				}
			}
		}
	}

	//==============================================================================
	/// 밖에서 닿는 것만 남긴 모듈 목록. (닿는 모듈은 공개 아이템 모두, 닿지 않는 모듈은 다시 내보낸 이름만)
	/// - 빈 모듈은 뺀다. 다만 pub mod 사슬로 밖에서 닿는 묶음 모듈(아래 모듈을 둔 모듈)은 아이템이 없어도 모듈 설명이나 예제가 있으면 둔다.
	///   (vanilla::ui, vanilla::graphics3d::toon 같은 것)
	/// - 크레이트 뿌리(lib.rs)는 두지 않는다. (원본 자료도 진입점 import.js 를 모듈로 두지 않는다)
	//==============================================================================
	fn take_public_modules(&mut self) -> Vec<ModuleEntry> {
		let modules = std::mem::take(&mut self.modules);
		let parent_names: HashSet<String> = modules.iter().filter_map(|module| module.name.rfind('/').map(|index| module.name[..index].to_string())).collect();
		let mut result = Vec::new();
		for mut module in modules {
			if !module.is_reachable && !module.is_all_exported {
				let names = module.exported_names.clone();
				module.classes.retain(|class| names.contains(&class.name));
				module.enums.retain(|entry| names.contains(&entry.name));
				module.functions.retain(|function| names.contains(&function.name));
			}
			let has_document = !module.summary.trim().is_empty() || !module.examples.is_empty();
			let is_group = module.is_reachable && !module.name.is_empty() && parent_names.contains(&module.name);
			if module.classes.is_empty() && module.enums.is_empty() && module.functions.is_empty() && !(is_group && has_document) {
				continue;
			}
			result.push(module);
		}
		result
	}
}

//==============================================================================
/// deprecated 속성의 글. (note 가 없으면 "deprecated")
//==============================================================================
fn read_deprecated(attributes: &[syn::Attribute]) -> Option<String> {
	for attribute in attributes {
		if !attribute.path().is_ident("deprecated") {
			continue;
		}
		match &attribute.meta {
			syn::Meta::NameValue(name_value) => {
				if let syn::Expr::Lit(expression) = &name_value.value {
					if let syn::Lit::Str(literal) = &expression.lit {
						return Some(literal.value());
					}
				}
			}
			syn::Meta::List(_) => {
				let mut note = String::from("deprecated");
				let _ = attribute.parse_nested_meta(|meta| {
					if meta.path.is_ident("note") {
						let literal: syn::LitStr = meta.value()?.parse()?;
						note = literal.value();
					}
					else {
						let _ = meta.value().and_then(|value| value.parse::<syn::LitStr>());
					}
					Ok(())
				});
				return Some(note);
			}
			_ => {}
		}
		return Some(String::from("deprecated"));
	}
	None
}

//==============================================================================
/// derive 로 붙인 트레이트 이름 목록.
//==============================================================================
fn read_derived_traits(attributes: &[syn::Attribute]) -> Vec<String> {
	let mut names = Vec::new();
	for attribute in attributes {
		if !attribute.path().is_ident("derive") {
			continue;
		}
		let _ = attribute.parse_nested_meta(|meta| {
			if let Some(segment) = meta.path.segments.last() {
				names.push(segment.ident.to_string());
			}
			Ok(())
		});
	}
	names
}

//==============================================================================
/// use 나무를 경로 목록으로. (이름 바꾸기는 원래 이름, * 는 그대로)
//==============================================================================
fn collect_use_paths(tree: &syn::UseTree, prefix: Vec<String>, paths: &mut Vec<Vec<String>>) {
	match tree {
		syn::UseTree::Path(use_path) => {
			let mut next = prefix.clone();
			next.push(use_path.ident.to_string());
			collect_use_paths(&use_path.tree, next, paths);
		}
		syn::UseTree::Name(use_name) => {
			let mut next = prefix.clone();
			next.push(use_name.ident.to_string());
			paths.push(next);
		}
		syn::UseTree::Rename(use_rename) => {
			let mut next = prefix.clone();
			next.push(use_rename.ident.to_string());
			paths.push(next);
		}
		syn::UseTree::Glob(_) => {
			let mut next = prefix.clone();
			next.push(String::from("*"));
			paths.push(next);
		}
		syn::UseTree::Group(use_group) => {
			for item in &use_group.items {
				collect_use_paths(item, prefix.clone(), paths);
			}
		}
	}
}

//==============================================================================
/// use 경로를 (모듈 이름, 아이템 이름) 으로. (crate, self, super 와 하위 모듈 상대 경로, 밖의 크레이트는 macros 만)
//==============================================================================
fn resolve_use_path(module_name: &str, segments: &[String]) -> Option<(String, String)> {
	if segments.is_empty() {
		return None;
	}
	let mut module_segments: Vec<String> = if module_name.is_empty() {
		Vec::new()
	}
	else {
		module_name.split('/').map(|segment| segment.to_string()).collect()
	};
	let mut index = 0;
	match segments[0].as_str() {
		"crate" => {
			module_segments.clear();
			index = 1;
		}
		"self" => {
			index = 1;
		}
		"super" => {
			while index < segments.len() && segments[index] == "super" {
				module_segments.pop();
				index += 1;
			}
		}
		"vanilla_engine_macros" => {
			return Some((String::from("macros"), segments.last().unwrap().clone()));
		}
		"std" | "core" | "alloc" => {
			return None;
		}
		_ => {}
	}
	if index >= segments.len() {
		return None;
	}
	for segment in &segments[index..segments.len() - 1] {
		module_segments.push(segment.clone());
	}
	Some((module_segments.join("/"), segments.last().unwrap().clone()))
}

//==============================================================================
/// 첫 판(v0.3.2 태그)의 이름 모으기. (git 이 없거나 태그가 없으면 None)
//==============================================================================
fn load_first_version_names(repository_root: &Path) -> Option<FirstVersionNames> {
	let output = Command::new("git").arg("-C").arg(repository_root).arg("ls-tree").arg("-r").arg("--name-only").arg(FIRST_VERSION_TAG).arg("src").output().ok()?;
	if !output.status.success() {
		return None;
	}
	let mut names = FirstVersionNames::default();
	for file_path in String::from_utf8_lossy(&output.stdout).lines() {
		if !file_path.ends_with(".rs") {
			continue;
		}
		let show = Command::new("git").arg("-C").arg(repository_root).arg("show").arg(format!("{}:{}", FIRST_VERSION_TAG, file_path)).output().ok()?;
		if !show.status.success() {
			continue;
		}
		let text = String::from_utf8_lossy(&show.stdout).to_string();
		if let Ok(file) = syn::parse_file(&text) {
			collect_first_version_items(&file.items, &mut names);
		}
	}
	Some(names)
}

//==============================================================================
/// 첫 판 아이템의 이름 넣기.
//==============================================================================
fn collect_first_version_items(items: &[syn::Item], names: &mut FirstVersionNames) {
	for item in items {
		match item {
			syn::Item::Mod(item_module) => {
				if let Some((_, inner_items)) = &item_module.content {
					collect_first_version_items(inner_items, names);
				}
			}
			syn::Item::Struct(item_struct) => {
				let set = names.type_members.entry(item_struct.ident.to_string()).or_default();
				for field in item_struct.fields.iter() {
					if let Some(identifier) = &field.ident {
						set.insert(normalize_name(&identifier.to_string()));
					}
				}
			}
			syn::Item::Enum(item_enum) => {
				names.type_members.entry(item_enum.ident.to_string()).or_default();
			}
			syn::Item::Trait(item_trait) => {
				let set = names.type_members.entry(item_trait.ident.to_string()).or_default();
				for trait_item in &item_trait.items {
					match trait_item {
						syn::TraitItem::Fn(trait_function) => {
							set.insert(normalize_name(&trait_function.sig.ident.to_string()));
						}
						syn::TraitItem::Const(trait_constant) => {
							set.insert(normalize_name(&trait_constant.ident.to_string()));
						}
						_ => {}
					}
				}
			}
			syn::Item::Type(item_type) => {
				names.type_members.entry(item_type.ident.to_string()).or_default();
			}
			syn::Item::Fn(item_function) => {
				names.function_names.insert(normalize_name(&item_function.sig.ident.to_string()));
			}
			syn::Item::Const(item_constant) => {
				names.function_names.insert(normalize_name(&item_constant.ident.to_string()));
			}
			syn::Item::Static(item_static) => {
				names.function_names.insert(normalize_name(&item_static.ident.to_string()));
			}
			syn::Item::Macro(item_macro) => {
				if let Some(identifier) = &item_macro.ident {
					names.function_names.insert(normalize_name(&format!("{}!", identifier)));
				}
			}
			syn::Item::Impl(item_implementation) => {
				if item_implementation.trait_.is_some() {
					continue;
				}
				let type_name = match &*item_implementation.self_ty {
					syn::Type::Path(type_path) => match type_path.path.segments.last() {
						Some(segment) => segment.ident.to_string(),
						None => continue,
					},
					_ => continue,
				};
				let set = names.type_members.entry(type_name).or_default();
				for implementation_item in &item_implementation.items {
					match implementation_item {
						syn::ImplItem::Fn(implementation_function) => {
							set.insert(normalize_name(&implementation_function.sig.ident.to_string()));
						}
						syn::ImplItem::Const(implementation_constant) => {
							set.insert(normalize_name(&implementation_constant.ident.to_string()));
						}
						_ => {}
					}
				}
			}
			_ => {}
		}
	}
}

//==============================================================================
/// since 채우기. (첫 판에 그 타입의 그 이름이 있으면 첫 판)
//==============================================================================
fn assign_since(modules: &mut [ModuleEntry], names: &Option<FirstVersionNames>, version: &str) {
	let since_of_member = |type_name: &str, member_name: &str| -> String {
		if let Some(names) = names {
			if let Some(set) = names.type_members.get(type_name) {
				if set.contains(&normalize_name(member_name)) {
					return String::from(FIRST_VERSION);
				}
			}
		}
		version.to_string()
	};
	for module in modules.iter_mut() {
		for class in module.classes.iter_mut() {
			for member in class.members.iter_mut() {
				member.since = since_of_member(&class.name, &member.name);
			}
		}
		for entry in module.enums.iter_mut() {
			for member in entry.members.iter_mut() {
				member.since = since_of_member(&entry.name, &member.name);
			}
		}
		for function in module.functions.iter_mut() {
			let is_first = match names {
				Some(names) => names.function_names.contains(&normalize_name(&function.name)),
				None => false,
			};
			function.since = if is_first {
				String::from(FIRST_VERSION)
			}
			else {
				version.to_string()
			};
		}
	}
}

//==============================================================================
/// 짝 표 열쇠.
//==============================================================================
fn create_counterpart_lookup_key(module: &str, class: &str, name: &str) -> String {
	format!("{}\u{1f}{}\u{1f}{}", module, class, name)
}

//==============================================================================
/// 짝 표 읽기. (docs/api/counterpart.<판>.json 의 items, 어느 쪽이든 초안 모듈인 짝은 뺀다)
//==============================================================================
fn load_counterpart_table(path: &Path) -> HashMap<String, CounterpartKey> {
	let mut table = HashMap::new();
	let text = match fs::read_to_string(path) {
		Ok(text) => text,
		Err(_) => {
			return table;
		}
	};
	let value: Value = match serde_json::from_str(&text) {
		Ok(value) => value,
		Err(error) => {
			eprintln!("짝 표를 읽지 못함: {} ({})", path.display(), error);
			return table;
		}
	};
	let read_key = |object: &Value| -> CounterpartKey {
		CounterpartKey {
			module: object.get("module").and_then(Value::as_str).unwrap_or("").to_string(),
			class: object.get("class").and_then(Value::as_str).unwrap_or("").to_string(),
			name: object.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
		}
	};
	if let Some(items) = value.get("items").and_then(Value::as_array) {
		for item in items {
			let own = read_key(&item[ENGINE_NAME]);
			let other = read_key(&item[COUNTERPART_ENGINE_NAME]);
			if is_draft_module_name(&own.module) || is_draft_module_name(&other.module) {
				continue;
			}
			table.insert(create_counterpart_lookup_key(&own.module, &own.class, &own.name), other);
		}
	}
	table
}

//==============================================================================
/// 짝 엔진의 멤버 하나. (모듈, 클래스, 이름, 종류)
//==============================================================================
struct CounterpartMember {
	name: String,
	kind: String,
}

//==============================================================================
/// 짝 엔진 자료의 찾기 표.
//==============================================================================
struct CounterpartIndex {
	classes: HashMap<String, (String, Vec<CounterpartMember>)>,
	parents: HashMap<String, String>,
	enums: HashMap<String, (String, Vec<String>)>,
	functions: HashMap<String, Vec<(String, String, String)>>,
}

//==============================================================================
/// 짝 엔진 자료(vanilla.js docs/api/<판>.json)를 찾기 표로. (초안 모듈은 뺀다)
//==============================================================================
fn build_counterpart_index(document: &Value) -> CounterpartIndex {
	let mut index = CounterpartIndex {
		classes: HashMap::new(),
		parents: HashMap::new(),
		enums: HashMap::new(),
		functions: HashMap::new(),
	};
	for module in document["modules"].as_array().into_iter().flatten() {
		let module_name = module["name"].as_str().unwrap_or("").to_string();
		if is_draft_module_name(&module_name) {
			continue;
		}
		for class in module["classes"].as_array().into_iter().flatten() {
			let class_name = class["name"].as_str().unwrap_or("").to_string();
			let members = class["members"].as_array().into_iter().flatten().map(|member| CounterpartMember {
				name: member["name"].as_str().unwrap_or("").to_string(),
				kind: member["kind"].as_str().unwrap_or("").to_string(),
			}).collect();
			if let Some(parent) = class["extends"].as_str() {
				index.parents.entry(class_name.clone()).or_insert(parent.to_string());
			}
			index.classes.entry(class_name).or_insert((module_name.clone(), members));
		}
		for entry in module["enums"].as_array().into_iter().flatten() {
			let enum_name = entry["name"].as_str().unwrap_or("").to_string();
			let values = entry["values"].as_array().into_iter().flatten().map(|value| value["name"].as_str().unwrap_or("").to_string()).collect();
			index.enums.entry(enum_name).or_insert((module_name.clone(), values));
		}
		for function in module["functions"].as_array().into_iter().flatten() {
			let function_name = function["name"].as_str().unwrap_or("").to_string();
			let kind = function["kind"].as_str().unwrap_or("").to_string();
			index.functions.entry(normalize_name(&function_name)).or_default().push((module_name.clone(), function_name, kind));
		}
	}
	index
}

//==============================================================================
/// 모듈 이름의 마지막 조각. (base/rect → rect)
//==============================================================================
fn read_module_stem(module_name: &str) -> &str {
	module_name.rsplit('/').next().unwrap_or(module_name)
}

//==============================================================================
/// 멤버의 짝 찾기. (같은 이름의 camelCase, 밑줄과 대소문자를 뺀 이름, get_ / set_ / is_ / has_ 를 뗀 속성과 필드, new 와 constructor)
//==============================================================================
fn find_counterpart_member(member: &Member, candidates: &[CounterpartMember]) -> Option<String> {
	// 원본은 기본 인자 하나로 받는 것을 러스트가 이름 둘로 나눈 것. (원본 resolve(isAttachmentKept = false) 가 러스트 resolve 와 resolve_with_attachment_kept)
	for (own_name, counterpart_name) in COUNTERPART_MEMBER_ALIASES {
		if member.name == own_name {
			if let Some(found) = candidates.iter().find(|candidate| candidate.name == counterpart_name) {
				return Some(found.name.clone());
			}
		}
	}
	if let Some(found) = find_counterpart_member_by_name(member.kind, &member.name, candidates) {
		return Some(found);
	}
	for suffix in ["_mut", "_async"] {
		if let Some(stripped) = member.name.strip_suffix(suffix) {
			if let Some(found) = find_counterpart_member_by_name(member.kind, stripped, candidates) {
				return Some(found);
			}
		}
	}
	None
}

//==============================================================================
/// 이름 하나로 멤버의 짝 찾기.
//==============================================================================
fn find_counterpart_member_by_name(kind: &str, member_name: &str, candidates: &[CounterpartMember]) -> Option<String> {
	if kind == "constructor" {
		if let Some(found) = candidates.iter().find(|candidate| candidate.kind == "constructor") {
			return Some(found.name.clone());
		}
	}
	let camel = convert_snake_to_camel(member_name);
	if let Some(found) = candidates.iter().find(|candidate| candidate.name == camel) {
		return Some(found.name.clone());
	}
	let normalized = normalize_name(member_name);
	if let Some(found) = candidates.iter().find(|candidate| normalize_name(&candidate.name) == normalized) {
		return Some(found.name.clone());
	}
	for prefix in ["get_", "set_", "is_", "has_"] {
		if let Some(rest) = member_name.strip_prefix(prefix) {
			let normalized_rest = normalize_name(rest);
			if let Some(found) = candidates.iter().find(|candidate| (candidate.kind == "property" || candidate.kind == "field") && normalize_name(&candidate.name) == normalized_rest) {
				return Some(found.name.clone());
			}
		}
	}
	None
}

//==============================================================================
/// 짝 표 짓기. (짝 엔진 자료와 이름을 대어 모든 항목의 짝을 찾는다)
//==============================================================================
fn build_counterpart_items(modules: &[ModuleEntry], counterpart_document: &Value) -> (Vec<Value>, Vec<String>) {
	let index = build_counterpart_index(counterpart_document);
	let mut items = Vec::new();
	let mut unmatched = Vec::new();
	let create_key = |module: &str, class: &str, name: &str| -> Value {
		let mut object = Map::new();
		object.insert(String::from("module"), Value::from(module));
		if !class.is_empty() {
			object.insert(String::from("class"), Value::from(class));
		}
		if !name.is_empty() {
			object.insert(String::from("name"), Value::from(name));
		}
		Value::Object(object)
	};
	let mut push_pair = |own: Value, other: Value| {
		let mut object = Map::new();
		object.insert(String::from(ENGINE_NAME), own);
		object.insert(String::from(COUNTERPART_ENGINE_NAME), other);
		items.push(Value::Object(object));
	};
	for module in modules {
		for class in &module.classes {
			let base_name = class.name.strip_suffix("Base").unwrap_or(&class.name).to_string();
			let alias_primary = COUNTERPART_CLASS_ALIASES.iter().find(|(name, _)| *name == class.name).and_then(|(_, aliases)| aliases.first()).map(|alias| alias.to_string()).unwrap_or_default();
			let base_name = if index.classes.contains_key(&class.name) || index.classes.contains_key(&base_name) {
				base_name
			}
			else {
				alias_primary
			};
			if let Some((other_module, other_members)) = index.classes.get(&class.name).or_else(|| index.classes.get(&base_name)) {
				let other_class_name = if index.classes.contains_key(&class.name) {
					class.name.clone()
				}
				else {
					base_name.clone()
				};
				if other_class_name == class.name {
					push_pair(create_key(&module.name, &class.name, ""), create_key(other_module, &other_class_name, ""));
				}
				for member in &class.members {
					let mut found: Option<(String, String, String)> = find_counterpart_member(member, other_members).map(|other_name| (other_module.clone(), other_class_name.clone(), other_name));
					let mut parent_name = index.parents.get(&other_class_name).cloned();
					let mut depth = 0;
					while found.is_none() && depth < 8 {
						let parent = match parent_name {
							Some(parent) => parent,
							None => break,
						};
						if let Some((parent_module, parent_members)) = index.classes.get(&parent) {
							found = find_counterpart_member(member, parent_members).map(|other_name| (parent_module.clone(), parent.clone(), other_name));
						}
						parent_name = index.parents.get(&parent).cloned();
						depth += 1;
					}
					if found.is_none() {
						if let Some((_, aliases)) = COUNTERPART_CLASS_ALIASES.iter().find(|(name, _)| *name == class.name) {
							for alias in aliases.iter() {
								if let Some((alias_module, alias_members)) = index.classes.get(*alias) {
									found = find_counterpart_member(member, alias_members).map(|other_name| (alias_module.clone(), alias.to_string(), other_name));
								}
								if found.is_some() {
									break;
								}
							}
						}
					}
					match found {
						Some((found_module, found_class, other_name)) => push_pair(create_key(&module.name, &class.name, &member.name), create_key(&found_module, &found_class, &other_name)),
						None => unmatched.push(format!("{}::{}", class.name, member.name)),
					}
				}
			}
			else if let Some((other_module, values)) = index.enums.get(&class.name) {
				push_pair(create_key(&module.name, &class.name, ""), create_key(other_module, &class.name, ""));
				for member in &class.members {
					let normalized = normalize_name(&member.name);
					match values.iter().find(|value| normalize_name(value) == normalized) {
						Some(value) => push_pair(create_key(&module.name, &class.name, &member.name), create_key(other_module, &class.name, value)),
						None => unmatched.push(format!("{}::{}", class.name, member.name)),
					}
				}
			}
			else {
				unmatched.push(class.name.clone());
			}
		}
		for entry in &module.enums {
			if let Some((other_module, _)) = index.enums.get(&entry.name) {
				push_pair(create_key(&module.name, &entry.name, ""), create_key(other_module, &entry.name, ""));
			}
			else if let Some((other_module, _)) = index.classes.get(&entry.name) {
				push_pair(create_key(&module.name, &entry.name, ""), create_key(other_module, &entry.name, ""));
			}
			else {
				unmatched.push(entry.name.clone());
			}
		}
		for function in &module.functions {
			let candidates = match index.functions.get(&normalize_name(&function.name)) {
				Some(candidates) => candidates,
				None => {
					unmatched.push(format!("{}::{}", module.name, function.name));
					continue;
				}
			};
			let stem = read_module_stem(&module.name);
			let chosen = candidates.iter().find(|(other_module, _, _)| read_module_stem(other_module) == stem).unwrap_or(&candidates[0]);
			push_pair(create_key(&module.name, "", &function.name), create_key(&chosen.0, "", &chosen.1));
		}
	}
	(items, unmatched)
}

//==============================================================================
/// 짝 붙이기.
//==============================================================================
fn assign_counterparts(modules: &mut [ModuleEntry], table: &HashMap<String, CounterpartKey>) {
	for module in modules.iter_mut() {
		let module_name = module.name.clone();
		for class in module.classes.iter_mut() {
			class.counterpart = table.get(&create_counterpart_lookup_key(&module_name, &class.name, "")).cloned();
			for member in class.members.iter_mut() {
				member.counterpart = table.get(&create_counterpart_lookup_key(&module_name, &class.name, &member.name)).cloned();
			}
		}
		for entry in module.enums.iter_mut() {
			entry.counterpart = table.get(&create_counterpart_lookup_key(&module_name, &entry.name, "")).cloned();
			for member in entry.members.iter_mut() {
				member.counterpart = table.get(&create_counterpart_lookup_key(&module_name, &entry.name, &member.name)).cloned();
			}
		}
		for function in module.functions.iter_mut() {
			function.counterpart = table.get(&create_counterpart_lookup_key(&module_name, "", &function.name)).cloned();
		}
	}
}

//==============================================================================
/// 번역 표. (ko 원문 → en, 빠진 글을 센다)
//==============================================================================
struct Translator {
	table: Map<String, Value>,
	missing: Vec<String>,
}

impl Translator {
	//==============================================================================
	/// 설명 칸 짓기. ({ ko, en })
	//==============================================================================
	fn create_text(&mut self, korean: &str) -> Value {
		let korean = normalize_text(korean);
		let english = if korean.is_empty() {
			String::new()
		}
		else if let Some(found) = self.table.get(&korean).and_then(Value::as_str) {
			found.to_string()
		}
		else if !has_hangul(&korean) {
			korean.clone()
		}
		else {
			self.missing.push(korean.clone());
			String::new()
		};
		let mut object = Map::new();
		object.insert(String::from("ko"), Value::from(korean));
		object.insert(String::from("en"), Value::from(english));
		Value::Object(object)
	}
}

//==============================================================================
/// 짝 칸 짓기.
//==============================================================================
fn create_counterpart_value(key: &CounterpartKey) -> Value {
	let mut object = Map::new();
	object.insert(String::from("engine"), Value::from(COUNTERPART_ENGINE_NAME));
	object.insert(String::from("module"), Value::from(key.module.clone()));
	if !key.class.is_empty() {
		object.insert(String::from("class"), Value::from(key.class.clone()));
	}
	if !key.name.is_empty() {
		object.insert(String::from("name"), Value::from(key.name.clone()));
	}
	Value::Object(object)
}

//==============================================================================
/// 예제 칸 짓기.
//==============================================================================
fn create_examples_value(examples: &[Example], translator: &mut Translator) -> Value {
	Value::Array(examples.iter().map(|example| {
		let mut object = Map::new();
		object.insert(String::from("title"), translator.create_text(&example.title));
		object.insert(String::from("code"), Value::from(example.code.clone()));
		object.insert(String::from("language"), Value::from(example.language.clone()));
		if !example.image.is_empty() {
			object.insert(String::from("image"), Value::from(example.image.clone()));
		}
		Value::Object(object)
	}).collect())
}

//==============================================================================
/// 멤버 칸 짓기. (칸 차례: kind, name, signature, params, returns, description, since, source, counterpart, deprecated, examples)
//==============================================================================
fn create_member_value(member: &Member, translator: &mut Translator) -> Value {
	let mut object = Map::new();
	object.insert(String::from("kind"), Value::from(member.kind));
	object.insert(String::from("name"), Value::from(member.name.clone()));
	object.insert(String::from("signature"), Value::from(member.signature.clone()));
	let parameters: Vec<Value> = member.parameters.iter().map(|parameter| {
		let mut parameter_object = Map::new();
		parameter_object.insert(String::from("name"), Value::from(parameter.name.clone()));
		parameter_object.insert(String::from("type"), Value::from(parameter.type_text.clone()));
		parameter_object.insert(String::from("description"), translator.create_text(&parameter.description));
		Value::Object(parameter_object)
	}).collect();
	object.insert(String::from("params"), Value::Array(parameters));
	if let Some((type_text, description)) = &member.returns {
		let mut returns_object = Map::new();
		returns_object.insert(String::from("type"), Value::from(type_text.clone()));
		returns_object.insert(String::from("description"), translator.create_text(description));
		object.insert(String::from("returns"), Value::Object(returns_object));
	}
	object.insert(String::from("description"), translator.create_text(&member.description));
	object.insert(String::from("since"), Value::from(member.since.clone()));
	object.insert(String::from("source"), Value::from(member.source.clone()));
	if let Some(counterpart) = &member.counterpart {
		object.insert(String::from("counterpart"), create_counterpart_value(counterpart));
	}
	if let Some(deprecated) = &member.deprecated {
		object.insert(String::from("deprecated"), Value::from(normalize_text(deprecated)));
	}
	if !member.examples.is_empty() {
		object.insert(String::from("examples"), create_examples_value(&member.examples, translator));
	}
	Value::Object(object)
}

//==============================================================================
/// 자료 짓기.
//==============================================================================
fn create_document_value(modules: &[ModuleEntry], version: &str, date: &str, translator: &mut Translator) -> Value {
	let mut module_values = Vec::new();
	for module in modules {
		let mut module_object = Map::new();
		module_object.insert(String::from("name"), Value::from(module.name.clone()));
		module_object.insert(String::from("summary"), translator.create_text(&module.summary));
		if !module.examples.is_empty() {
			module_object.insert(String::from("examples"), create_examples_value(&module.examples, translator));
		}
		let mut class_values = Vec::new();
		for class in &module.classes {
			let mut class_object = Map::new();
			class_object.insert(String::from("name"), Value::from(class.name.clone()));
			class_object.insert(String::from("kind"), Value::from(class.kind));
			class_object.insert(String::from("summary"), translator.create_text(&class.summary));
			if !class.signature.is_empty() {
				class_object.insert(String::from("signature"), Value::from(class.signature.clone()));
			}
			if !class.traits.is_empty() {
				class_object.insert(String::from("traits"), Value::Array(class.traits.iter().map(|name| Value::from(name.clone())).collect()));
			}
			if let Some(counterpart) = &class.counterpart {
				class_object.insert(String::from("counterpart"), create_counterpart_value(counterpart));
			}
			class_object.insert(String::from("source"), Value::from(class.source.clone()));
			class_object.insert(String::from("members"), Value::Array(class.members.iter().map(|member| create_member_value(member, translator)).collect()));
			if !class.examples.is_empty() {
				class_object.insert(String::from("examples"), create_examples_value(&class.examples, translator));
			}
			class_values.push(Value::Object(class_object));
		}
		module_object.insert(String::from("classes"), Value::Array(class_values));
		module_object.insert(String::from("functions"), Value::Array(module.functions.iter().map(|function| create_member_value(function, translator)).collect()));
		let mut enum_values = Vec::new();
		for entry in &module.enums {
			let mut enum_object = Map::new();
			enum_object.insert(String::from("name"), Value::from(entry.name.clone()));
			enum_object.insert(String::from("summary"), translator.create_text(&entry.summary));
			if !entry.traits.is_empty() {
				enum_object.insert(String::from("traits"), Value::Array(entry.traits.iter().map(|name| Value::from(name.clone())).collect()));
			}
			if let Some(counterpart) = &entry.counterpart {
				enum_object.insert(String::from("counterpart"), create_counterpart_value(counterpart));
			}
			enum_object.insert(String::from("source"), Value::from(entry.source.clone()));
			let values: Vec<Value> = entry.values.iter().map(|value| {
				let mut value_object = Map::new();
				value_object.insert(String::from("name"), Value::from(value.name.clone()));
				if !value.value.is_empty() {
					value_object.insert(String::from("value"), Value::from(value.value.clone()));
				}
				value_object.insert(String::from("description"), translator.create_text(&value.description));
				Value::Object(value_object)
			}).collect();
			enum_object.insert(String::from("values"), Value::Array(values));
			if !entry.members.is_empty() {
				enum_object.insert(String::from("members"), Value::Array(entry.members.iter().map(|member| create_member_value(member, translator)).collect()));
			}
			if !entry.examples.is_empty() {
				enum_object.insert(String::from("examples"), create_examples_value(&entry.examples, translator));
			}
			enum_values.push(Value::Object(enum_object));
		}
		module_object.insert(String::from("enums"), Value::Array(enum_values));
		module_values.push(Value::Object(module_object));
	}
	let mut document = Map::new();
	document.insert(String::from("engine"), Value::from(ENGINE_NAME));
	document.insert(String::from("version"), Value::from(version));
	document.insert(String::from("date"), Value::from(date));
	document.insert(String::from("modules"), Value::Array(module_values));
	Value::Object(document)
}

//==============================================================================
/// JSON 글로. (탭 들여쓰기, 끝 줄바꿈, 원본 자료와 같은 꼴)
//==============================================================================
fn write_json_text(value: &Value) -> String {
	let mut text = String::new();
	write_json_value(value, 0, &mut text);
	text.push('\n');
	text
}

//==============================================================================
/// JSON 값 하나를 글에 잇기. (serde_json 의 예쁜 꼴과 같되 들여쓰기는 탭)
//==============================================================================
fn write_json_value(value: &Value, depth: usize, text: &mut String) {
	match value {
		Value::Array(items) => {
			if items.is_empty() {
				text.push_str("[]");
				return;
			}
			text.push_str("[\n");
			for (index, item) in items.iter().enumerate() {
				text.push_str(&"\t".repeat(depth + 1));
				write_json_value(item, depth + 1, text);
				if index + 1 < items.len() {
					text.push(',');
				}
				text.push('\n');
			}
			text.push_str(&"\t".repeat(depth));
			text.push(']');
		}
		Value::Object(object) => {
			if object.is_empty() {
				text.push_str("{}");
				return;
			}
			text.push_str("{\n");
			for (index, (key, item)) in object.iter().enumerate() {
				text.push_str(&"\t".repeat(depth + 1));
				text.push_str(&serde_json::to_string(key).unwrap());
				text.push_str(": ");
				write_json_value(item, depth + 1, text);
				if index + 1 < object.len() {
					text.push(',');
				}
				text.push('\n');
			}
			text.push_str(&"\t".repeat(depth));
			text.push('}');
		}
		other => text.push_str(&serde_json::to_string(other).unwrap()),
	}
}

//==============================================================================
/// 오늘 날짜. (한국 시각, YYYY-MM-DD)
//==============================================================================
fn read_today_korea() -> String {
	let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs() as i64).unwrap_or(0) + 9 * 3600;
	let days = seconds.div_euclid(86400);
	let shifted = days + 719468;
	let era = shifted.div_euclid(146097);
	let day_of_era = shifted - era * 146097;
	let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
	let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
	let month_index = (5 * day_of_year + 2) / 153;
	let day = day_of_year - (153 * month_index + 2) / 5 + 1;
	let month = if month_index < 10 {
		month_index + 3
	}
	else {
		month_index - 9
	};
	let year = year_of_era + era * 400 + if month <= 2 {
		1
	}
	else {
		0
	};
	format!("{:04}-{:02}-{:02}", year, month, day)
}

//==============================================================================
/// 자료 날짜. (--date, UPDATE.md 의 "# <판> (YYYY-MM-DD)", 이미 있는 자료의 날짜, 오늘 차례)
//==============================================================================
fn resolve_date(options: &Options, repository_root: &Path, output_path: &Path) -> String {
	if !options.date.is_empty() {
		return options.date.clone();
	}
	if let Ok(text) = fs::read_to_string(repository_root.join("UPDATE.md")) {
		let head = format!("# {} (", options.version);
		for line in text.lines() {
			if let Some(rest) = line.strip_prefix(&head) {
				if let Some(date) = rest.strip_suffix(')') {
					return date.to_string();
				}
			}
		}
	}
	if let Ok(text) = fs::read_to_string(output_path) {
		if let Ok(value) = serde_json::from_str::<Value>(&text) {
			if let Some(date) = value["date"].as_str() {
				return date.to_string();
			}
		}
	}
	read_today_korea()
}

//==============================================================================
/// 명령줄 읽기.
//==============================================================================
fn parse_options() -> Result<Options, String> {
	let arguments: Vec<String> = std::env::args().skip(1).collect();
	let mut options = Options {
		version: String::new(),
		date: String::new(),
		counterpart_source_path: String::new(),
		is_check: false,
		is_missing: false,
		is_gaps: false,
		only_prefixes: Vec::new(),
		exclude_prefixes: Vec::new(),
	};
	let mut index = 0;
	while index < arguments.len() {
		let argument = arguments[index].as_str();
		let read_value = |index: &mut usize| -> Result<String, String> {
			*index += 1;
			arguments.get(*index).cloned().ok_or_else(|| format!("{} 뒤에 값이 없다", argument))
		};
		match argument {
			"--check" => options.is_check = true,
			"--missing" => options.is_missing = true,
			"--gaps" => options.is_gaps = true,
			"--date" => options.date = read_value(&mut index)?,
			"--counterpart" => options.counterpart_source_path = read_value(&mut index)?,
			"--only" => {
				let value = read_value(&mut index)?;
				options.only_prefixes.push(value);
			}
			"--exclude" => {
				let value = read_value(&mut index)?;
				options.exclude_prefixes.push(value);
			}
			_ => {
				if argument.starts_with("--") {
					return Err(format!("모르는 갈래: {}", argument));
				}
				options.version = argument.to_string();
			}
		}
		index += 1;
	}
	if options.version.is_empty() {
		return Err(String::from("사용: cargo run -p vanilla-engine-apidoc -- <판> [--date YYYY-MM-DD] [--counterpart <vanilla.js 자료>] [--check] [--missing] [--gaps] [--only <경로 앞>] [--exclude <경로 앞>]"));
	}
	Ok(options)
}

//==============================================================================
/// 경로가 --only, --exclude 거르기를 지나는지.
//==============================================================================
fn is_path_selected(options: &Options, source: &str) -> bool {
	if !options.only_prefixes.is_empty() && !options.only_prefixes.iter().any(|prefix| source.starts_with(prefix.as_str())) {
		return false;
	}
	!options.exclude_prefixes.iter().any(|prefix| source.starts_with(prefix.as_str()))
}

//==============================================================================
/// 엔진 소스를 모두 읽어 공개 API 모으기.
//==============================================================================
fn collect_modules(repository_root: &Path, version: &str) -> Vec<ModuleEntry> {
	let mut collector = Collector::new(repository_root.to_path_buf());
	let source_directory = repository_root.join("src");
	collector.load_module(&source_directory.join("lib.rs"), "", &source_directory, true);
	let macro_names: HashSet<String> = collector.pending_exports.iter().filter(|(module, _, _)| module == "macros").map(|(_, name, _)| name.clone()).collect();
	collector.load_macro_crate(&macro_names);
	collector.resolve();
	let mut modules = collector.take_public_modules();
	let first_version_names = load_first_version_names(repository_root);
	if first_version_names.is_none() {
		eprintln!("{} 태그를 읽지 못해 since 를 모두 {} 로 둔다", FIRST_VERSION_TAG, version);
	}
	assign_since(&mut modules, &first_version_names, version);
	modules
}

//==============================================================================
/// 빈 곳 하나. (파일, 줄, 갈래, 이름)
//==============================================================================
struct Gap {
	source: String,
	line: usize,
	category: &'static str,
	name: String,
}

//==============================================================================
/// 머리 주석 없는 함수 찾기. (src 의 모든 파일, 모든 함수와 메서드)
//==============================================================================
struct HeaderVisitor<'a> {
	collector: &'a Collector,
	source: String,
	gaps: Vec<Gap>,
	scope: Vec<String>,
}

impl<'a> HeaderVisitor<'a> {
	//==============================================================================
	/// 함수 하나 보기. (문서 주석이나 머리 주석이 없으면 빈 곳)
	//==============================================================================
	fn check(&mut self, attributes: &[syn::Attribute], signature: &syn::Signature, span: proc_macro2::Span) {
		let has_document = read_document_lines(attributes).is_some();
		let has_header = self.collector.read_header_comment(&self.source, span.start().line).is_some();
		if !has_document && !has_header {
			let mut name = self.scope.join("::");
			if !name.is_empty() {
				name.push_str("::");
			}
			name.push_str(&signature.ident.to_string());
			self.gaps.push(Gap {
				source: self.source.clone(),
				line: signature.ident.span().start().line,
				category: "머리 주석 없는 함수",
				name: name,
			});
		}
	}
}

impl<'a, 'ast> Visit<'ast> for HeaderVisitor<'a> {
	//==============================================================================
	/// 함수 아이템.
	//==============================================================================
	fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
		self.check(&node.attrs, &node.sig, node.span());
		syn::visit::visit_item_fn(self, node);
	}

	//==============================================================================
	/// impl 블록. (멤버 이름 앞에 타입 이름을 붙인다)
	//==============================================================================
	fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
		let type_name = match &*node.self_ty {
			syn::Type::Path(type_path) => type_path.path.segments.last().map(|segment| segment.ident.to_string()).unwrap_or_default(),
			_ => String::new(),
		};
		self.scope.push(type_name);
		syn::visit::visit_item_impl(self, node);
		self.scope.pop();
	}

	//==============================================================================
	/// 트레이트. (멤버 이름 앞에 트레이트 이름을 붙인다)
	//==============================================================================
	fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
		self.scope.push(node.ident.to_string());
		syn::visit::visit_item_trait(self, node);
		self.scope.pop();
	}

	//==============================================================================
	/// impl 블록의 함수.
	//==============================================================================
	fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
		self.check(&node.attrs, &node.sig, node.span());
		syn::visit::visit_impl_item_fn(self, node);
	}

	//==============================================================================
	/// 트레이트의 함수.
	//==============================================================================
	fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
		self.check(&node.attrs, &node.sig, node.span());
		syn::visit::visit_trait_item_fn(self, node);
	}
}

//==============================================================================
/// 폴더 아래 .rs 파일 모두. (이름 차례)
//==============================================================================
fn list_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
	let mut entries: Vec<PathBuf> = match fs::read_dir(directory) {
		Ok(entries) => entries.flatten().map(|entry| entry.path()).collect(),
		Err(_) => {
			return;
		}
	};
	entries.sort();
	for path in entries {
		if path.is_dir() {
			list_rust_files(&path, files);
		}
		else if path.extension().map(|extension| extension == "rs").unwrap_or(false) {
			files.push(path);
		}
	}
}

//==============================================================================
/// 공개 API 의 빈 곳 모으기.
//==============================================================================
fn collect_api_gaps(modules: &[ModuleEntry]) -> Vec<Gap> {
	let mut gaps = Vec::new();
	let mut push = |source: &str, line: usize, category: &'static str, name: String| {
		gaps.push(Gap {
			source: source.to_string(),
			line: line,
			category: category,
			name: name,
		});
	};
	let check_member = |push: &mut dyn FnMut(&str, usize, &'static str, String), owner: &str, member: &Member| {
		let full_name = if owner.is_empty() {
			member.name.clone()
		}
		else {
			format!("{}::{}", owner, member.name)
		};
		if member.description.trim().is_empty() {
			push(&member.source, member.line, "멤버 설명", full_name.clone());
		}
		for parameter in &member.parameters {
			if parameter.description.trim().is_empty() {
				push(&member.source, member.line, "인자 설명", format!("{}({})", full_name, parameter.name));
			}
		}
		for documented_name in &member.documented_parameter_names {
			if !member.parameters.iter().any(|parameter| parameter.name == *documented_name) {
				push(&member.source, member.line, "없는 인자를 적음", format!("{}({})", full_name, documented_name));
			}
		}
		if let Some((_, description)) = &member.returns {
			if !member.is_value && description.trim().is_empty() {
				push(&member.source, member.line, "돌려줌 설명", full_name.clone());
			}
		}
	};
	for module in modules {
		if module.summary.trim().is_empty() {
			push(&module.source, 1, "모듈 요약", module.name.clone());
		}
		let has_class_example = module.classes.iter().any(|class| !class.examples.is_empty());
		let has_items = !module.classes.is_empty() || !module.enums.is_empty() || !module.functions.is_empty();
		if has_items && module.examples.is_empty() && !has_class_example {
			push(&module.source, 1, "예제 없는 모듈", module.name.clone());
		}
		for class in &module.classes {
			if class.summary.trim().is_empty() {
				push(&class.source, class.line, "클래스 요약", class.name.clone());
			}
			if class.kind != "type" && class.examples.is_empty() {
				push(&class.source, class.line, "예제 없는 구조체와 트레이트", class.name.clone());
			}
			for member in &class.members {
				check_member(&mut push, &class.name, member);
			}
		}
		for function in &module.functions {
			check_member(&mut push, "", function);
		}
		for entry in &module.enums {
			if entry.summary.trim().is_empty() {
				push(&entry.source, entry.line, "열거 요약", entry.name.clone());
			}
			for value in &entry.values {
				if value.description.trim().is_empty() {
					push(&entry.source, value.line, "열거 값 설명", format!("{}::{}", entry.name, value.name));
				}
			}
			for member in &entry.members {
				check_member(&mut push, &entry.name, member);
			}
		}
	}
	gaps
}

//==============================================================================
/// --gaps: 소스에서 문서가 빈 곳 찍기.
//==============================================================================
fn run_gaps(options: &Options, repository_root: &Path) {
	let modules = collect_modules(repository_root, &options.version);
	let mut gaps = collect_api_gaps(&modules);
	let mut collector = Collector::new(repository_root.to_path_buf());
	let mut files = Vec::new();
	list_rust_files(&repository_root.join("src"), &mut files);
	files.push(repository_root.join("macros/src/lib.rs"));
	for file_path in files {
		let source = file_path.strip_prefix(repository_root).unwrap_or(&file_path).to_string_lossy().replace('\\', "/");
		if !is_path_selected(options, &source) {
			continue;
		}
		let text = match fs::read_to_string(&file_path) {
			Ok(text) => text,
			Err(_) => continue,
		};
		collector.source_texts.insert(source.clone(), text.lines().map(|line| line.to_string()).collect());
		let file = match syn::parse_file(&text) {
			Ok(file) => file,
			Err(_) => continue,
		};
		let mut visitor = HeaderVisitor {
			collector: &collector,
			source: source.clone(),
			gaps: Vec::new(),
			scope: Vec::new(),
		};
		visitor.visit_file(&file);
		gaps.extend(visitor.gaps);
	}
	gaps.retain(|gap| is_path_selected(options, &gap.source));
	gaps.sort_by(|first, second| first.source.cmp(&second.source).then(first.line.cmp(&second.line)));
	let mut counts: Vec<(&'static str, usize)> = Vec::new();
	let mut current_source = String::new();
	for gap in &gaps {
		if gap.source != current_source {
			println!("{}", gap.source);
			current_source = gap.source.clone();
		}
		println!("\t{} {}:{}", gap.category, gap.name, gap.line);
		match counts.iter_mut().find(|(category, _)| *category == gap.category) {
			Some(entry) => entry.1 += 1,
			None => counts.push((gap.category, 1)),
		}
	}
	for (category, count) in &counts {
		println!("{} {} 개", category, count);
	}
	println!("빈 곳 {} 개", gaps.len());
}

//==============================================================================
/// 마크다운의 코드 칸 목록. ((언어 정보, 코드))
//==============================================================================
fn read_code_blocks(text: &str) -> Vec<(String, String)> {
	let mut blocks = Vec::new();
	let mut current: Option<(String, Vec<String>)> = None;
	for line in text.lines() {
		let trimmed = line.trim_start();
		if trimmed.starts_with("```") {
			match current.take() {
				Some((information, lines)) => blocks.push((information, lines.join("\n"))),
				None => current = Some((trimmed.trim_start_matches('`').trim().to_string(), Vec::new())),
			}
			continue;
		}
		if let Some((_, lines)) = current.as_mut() {
			lines.push(line.to_string());
		}
	}
	blocks
}

//==============================================================================
/// 쓰지 않는 글자(긴 줄표, 가운뎃점)가 있는 줄.
//==============================================================================
fn find_forbidden_characters(text: &str) -> Vec<usize> {
	text.lines().enumerate().filter(|(_, line)| line.contains('\u{2014}') || line.contains('\u{00B7}')).map(|(index, _)| index + 1).collect()
}

//==============================================================================
/// 파일 이름을 가이드 doctest 구조체 이름으로. ("08-drawing" → "Guide08Drawing")
//==============================================================================
fn create_guide_struct_name(file_stem: &str) -> String {
	let mut name = String::from("Guide");
	for part in file_stem.split(|character: char| !character.is_ascii_alphanumeric()) {
		let mut characters = part.chars();
		if let Some(first) = characters.next() {
			name.push(first.to_ascii_uppercase());
			name.push_str(characters.as_str());
		}
	}
	name
}

//==============================================================================
/// 가이드 doctest 장 목록 글. (tools/apidoc/src/guide.rs, ko 장마다 include_str 를 문서 속성으로 붙인 빈 구조체)
//==============================================================================
fn create_guide_test_source(repository_root: &Path, version: &str) -> String {
	let korean_directory = repository_root.join("docs/guide").join(version).join("ko");
	let mut names: Vec<String> = fs::read_dir(&korean_directory).map(|entries| entries.flatten().map(|entry| entry.file_name().to_string_lossy().to_string()).filter(|name| name.ends_with(".md")).collect()).unwrap_or_default();
	names.sort();
	let mut source = String::new();
	source.push_str("//==============================================================================\n");
	source.push_str("//! 가이드 doctest 장 목록. (`cargo run -p vanilla-engine-apidoc -- <판>` 이 docs/guide/<판>/ko 에서 다시 쓴다, 손으로 고치지 않는다)\n");
	source.push_str("//==============================================================================\n");
	for name in names {
		let file_stem = name.trim_end_matches(".md");
		source.push_str(&format!("#[doc = include_str!(\"../../../docs/guide/{}/ko/{}\")]\n", version, name));
		source.push_str(&format!("pub struct {};\n", create_guide_struct_name(file_stem)));
	}
	source
}

//==============================================================================
/// --check: 자료 꼴 검사와 수 세기, 가이드와 업데이트 문서 대조.
//==============================================================================
fn run_check(options: &Options, repository_root: &Path, output_path: &Path, is_draft_only: bool) -> bool {
	let mut errors: Vec<String> = Vec::new();
	let text = match fs::read_to_string(output_path) {
		Ok(text) => text,
		Err(error) => {
			eprintln!("자료를 읽지 못함: {} ({})", output_path.display(), error);
			return false;
		}
	};
	let document: Value = match serde_json::from_str(&text) {
		Ok(value) => value,
		Err(error) => {
			eprintln!("JSON 이 아님: {}", error);
			return false;
		}
	};
	for line in find_forbidden_characters(&text) {
		errors.push(format!("자료 {} 줄: 긴 줄표나 가운뎃점", line));
	}
	if document["engine"] != ENGINE_NAME {
		errors.push(String::from("engine"));
	}
	if document["version"] != options.version.as_str() {
		errors.push(String::from("version"));
	}
	let mut counts: Vec<(String, usize)> = Vec::new();
	let mut count = |name: &str, amount: usize| {
		match counts.iter_mut().find(|(existing, _)| existing == name) {
			Some(entry) => entry.1 += amount,
			None => counts.push((name.to_string(), amount)),
		}
	};
	let mut korean_empty: Vec<(String, usize)> = Vec::new();
	let mut english_empty = 0;
	let mut since_counts: Vec<(String, usize)> = Vec::new();
	let check_text = |value: &Value, place: &str, category: &str, is_counted: bool, errors: &mut Vec<String>, korean_empty: &mut Vec<(String, usize)>, english_empty: &mut usize| {
		let korean = value.get("ko").and_then(Value::as_str);
		let english = value.get("en").and_then(Value::as_str);
		match (korean, english) {
			(Some(korean), Some(english)) => {
				if korean.is_empty() {
					if is_counted {
						match korean_empty.iter_mut().find(|(existing, _)| existing == category) {
							Some(entry) => entry.1 += 1,
							None => korean_empty.push((category.to_string(), 1)),
						}
					}
				}
				else if english.is_empty() {
					*english_empty += 1;
				}
			}
			_ => errors.push(format!("{}: 설명 칸이 {{ ko, en }} 이 아님", place)),
		}
	};
	let check_examples = |value: &Value, place: &str, errors: &mut Vec<String>, korean_empty: &mut Vec<(String, usize)>, english_empty: &mut usize| -> usize {
		let examples = match value.get("examples") {
			Some(Value::Array(examples)) => examples,
			Some(_) => {
				errors.push(format!("{}: examples", place));
				return 0;
			}
			None => return 0,
		};
		for example in examples {
			check_text(&example["title"], place, "예제 제목", false, errors, korean_empty, english_empty);
			if !example["code"].is_string() || example["language"].as_str().is_none() {
				errors.push(format!("{}: 예제 칸", place));
			}
			if let Some(code) = example["code"].as_str() {
				if has_hangul(code) {
					errors.push(format!("{}: 예제 코드에 한글", place));
				}
			}
		}
		examples.len()
	};
	let check_member = |member: &Value, place: &str, errors: &mut Vec<String>, korean_empty: &mut Vec<(String, usize)>, english_empty: &mut usize, since_counts: &mut Vec<(String, usize)>| -> (usize, usize, usize) {
		let kind = member["kind"].as_str().unwrap_or("");
		if !MEMBER_KINDS.contains(&kind) {
			errors.push(format!("{}: kind {}", place, kind));
		}
		let member_place = format!("{}.{}", place, member["name"].as_str().unwrap_or("?"));
		let mut parameter_count = 0;
		for parameter in member["params"].as_array().into_iter().flatten() {
			parameter_count += 1;
			check_text(&parameter["description"], &member_place, "인자 설명", true, errors, korean_empty, english_empty);
		}
		if let Some(returns) = member.get("returns") {
			let is_value = kind == "field" || (kind == "static" && member["params"].as_array().map(|list| list.is_empty()).unwrap_or(true) && !member["signature"].as_str().unwrap_or("").contains("fn "));
			check_text(&returns["description"], &member_place, if is_value {
				"값 꼴 설명(세지 않음)"
			}
			else {
				"돌려줌 설명"
			}, !is_value, errors, korean_empty, english_empty);
		}
		check_text(&member["description"], &member_place, "멤버 설명", true, errors, korean_empty, english_empty);
		let since = member["since"].as_str().unwrap_or("");
		if since != FIRST_VERSION && since != options.version {
			errors.push(format!("{}: since {}", member_place, since));
		}
		match since_counts.iter_mut().find(|(existing, _)| existing == since) {
			Some(entry) => entry.1 += 1,
			None => since_counts.push((since.to_string(), 1)),
		}
		let has_counterpart = member.get("counterpart").is_some();
		let examples = check_examples(member, &member_place, errors, korean_empty, english_empty);
		(parameter_count, examples, has_counterpart as usize)
	};
	for module in document["modules"].as_array().into_iter().flatten() {
		let module_name = module["name"].as_str().unwrap_or("?").to_string();
		if !is_draft_only && is_draft_module_name(&module_name) {
			errors.push(format!("{}: 초안 모듈이 자료에 있음", module_name));
		}
		count("모듈", 1);
		check_text(&module["summary"], &module_name, "모듈 요약", true, &mut errors, &mut korean_empty, &mut english_empty);
		let module_examples = check_examples(module, &module_name, &mut errors, &mut korean_empty, &mut english_empty);
		count("모듈 예제", module_examples);
		for class in module["classes"].as_array().into_iter().flatten() {
			let class_place = format!("{}::{}", module_name, class["name"].as_str().unwrap_or("?"));
			let kind = class["kind"].as_str().unwrap_or("");
			if !CLASS_KINDS.contains(&kind) {
				errors.push(format!("{}: kind {}", class_place, kind));
			}
			count(&format!("클래스 ({})", kind), 1);
			check_text(&class["summary"], &class_place, "클래스 요약", true, &mut errors, &mut korean_empty, &mut english_empty);
			count("클래스 짝", class.get("counterpart").is_some() as usize);
			let class_examples = check_examples(class, &class_place, &mut errors, &mut korean_empty, &mut english_empty);
			count("클래스 예제", class_examples);
			for member in class["members"].as_array().into_iter().flatten() {
				count("멤버", 1);
				count(&format!("멤버 ({})", member["kind"].as_str().unwrap_or("")), 1);
				let (parameters, examples, counterparts) = check_member(member, &class_place, &mut errors, &mut korean_empty, &mut english_empty, &mut since_counts);
				count("인자", parameters);
				count("멤버 예제", examples);
				count("멤버 짝", counterparts);
			}
		}
		for function in module["functions"].as_array().into_iter().flatten() {
			count("함수와 상수", 1);
			let (parameters, examples, counterparts) = check_member(function, &module_name, &mut errors, &mut korean_empty, &mut english_empty, &mut since_counts);
			count("인자", parameters);
			count("함수 예제", examples);
			count("함수 짝", counterparts);
		}
		for entry in module["enums"].as_array().into_iter().flatten() {
			let enum_place = format!("{}::{}", module_name, entry["name"].as_str().unwrap_or("?"));
			count("열거", 1);
			check_text(&entry["summary"], &enum_place, "열거 요약", true, &mut errors, &mut korean_empty, &mut english_empty);
			count("열거 짝", entry.get("counterpart").is_some() as usize);
			for value in entry["values"].as_array().into_iter().flatten() {
				count("열거 값", 1);
				check_text(&value["description"], &enum_place, "열거 값 설명", true, &mut errors, &mut korean_empty, &mut english_empty);
			}
			for member in entry["members"].as_array().into_iter().flatten() {
				count("열거 멤버", 1);
				let (parameters, examples, counterparts) = check_member(member, &enum_place, &mut errors, &mut korean_empty, &mut english_empty, &mut since_counts);
				count("인자", parameters);
				count("멤버 예제", examples);
				count("멤버 짝", counterparts);
			}
			let enum_examples = check_examples(entry, &enum_place, &mut errors, &mut korean_empty, &mut english_empty);
			count("열거 예제", enum_examples);
		}
	}
	if is_draft_only {
		let korean_empty_count: usize = korean_empty.iter().map(|(_, amount)| amount).sum();
		println!("초안 모듈(자료에 넣지 않음) {}, ko 빈 칸 {}, en 빈 칸 {}", counts.iter().find(|(name, _)| name == "모듈").map(|(_, amount)| *amount).unwrap_or(0), korean_empty_count, english_empty);
		for error in &errors {
			println!("오류(초안): {}", error);
		}
		println!("초안 오류 {} 개", errors.len());
		return errors.is_empty();
	}
	let mut draft_counterpart_places: Vec<String> = Vec::new();
	collect_draft_counterparts(&document, "", &mut draft_counterpart_places);
	for place in &draft_counterpart_places {
		errors.push(format!("{}: 초안 모듈을 가리키는 짝", place));
	}
	let guide_directory = repository_root.join("docs/guide").join(&options.version);
	let mut guide_files: Vec<String> = Vec::new();
	let mut code_block_count = 0;
	for language in ["ko", "en"] {
		let mut names: Vec<String> = fs::read_dir(guide_directory.join(language)).map(|entries| entries.flatten().map(|entry| entry.file_name().to_string_lossy().to_string()).filter(|name| name.ends_with(".md")).collect()).unwrap_or_default();
		names.sort();
		if language == "ko" {
			guide_files = names;
		}
		else if names != guide_files {
			errors.push(format!("가이드 ko 와 en 의 파일이 다름: ko {:?}, en {:?}", guide_files, names));
		}
	}
	for name in &guide_files {
		let korean = fs::read_to_string(guide_directory.join("ko").join(name)).unwrap_or_default();
		let english = fs::read_to_string(guide_directory.join("en").join(name)).unwrap_or_default();
		for (language, text) in [("ko", &korean), ("en", &english)] {
			for line in find_forbidden_characters(text) {
				errors.push(format!("가이드 {}/{} {} 줄: 긴 줄표나 가운뎃점", language, name, line));
			}
		}
		let korean_blocks = read_code_blocks(&korean);
		let english_blocks = read_code_blocks(&english);
		code_block_count += korean_blocks.len();
		if korean_blocks != english_blocks {
			errors.push(format!("가이드 {}: ko 와 en 의 코드 칸이 다름", name));
		}
		for (information, code) in &korean_blocks {
			if information.starts_with("rust") && has_hangul(code) {
				errors.push(format!("가이드 {}: rust 코드 칸에 한글", name));
			}
		}
		if english.lines().filter(|line| !line.trim_start().starts_with("```")).any(has_hangul) && !read_code_blocks(&english).iter().any(|(_, code)| has_hangul(code)) {
			errors.push(format!("가이드 en/{}: 본문에 한글", name));
		}
	}
	let image_directory = guide_directory.join("images");
	let mut used_images: HashSet<String> = HashSet::new();
	let mut check_image = |name: &str, place: &str, errors: &mut Vec<String>| {
		if !is_image_name_valid(name) {
			errors.push(format!("{}: 그림 파일 이름 꼴 {:?} (영문 소문자, 숫자, 점, 밑줄, 하이픈과 .webp, .png, .jpg, .gif)", place, name));
		}
		else if !image_directory.join(name).is_file() {
			errors.push(format!("{}: 그림 파일 없음 docs/guide/{}/images/{}", place, options.version, name));
		}
		used_images.insert(name.to_string());
	};
	let mut example_images: Vec<(String, String)> = Vec::new();
	collect_example_images(&document, "", &mut example_images);
	for (place, name) in &example_images {
		check_image(name, place, &mut errors);
	}
	let mut guide_image_count = 0;
	for name in &guide_files {
		let korean_images = read_guide_images(&fs::read_to_string(guide_directory.join("ko").join(name)).unwrap_or_default());
		let english_images = read_guide_images(&fs::read_to_string(guide_directory.join("en").join(name)).unwrap_or_default());
		if korean_images != english_images {
			errors.push(format!("가이드 {}: ko 와 en 의 그림이 다름 (ko {:?}, en {:?})", name, korean_images, english_images));
		}
		for image in &korean_images {
			check_image(image, &format!("가이드 ko/{}", name), &mut errors);
		}
		guide_image_count += korean_images.len();
	}
	let mut source_files = Vec::new();
	list_rust_files(&repository_root.join("src"), &mut source_files);
	list_rust_files(&repository_root.join("macros/src"), &mut source_files);
	for source_file in &source_files {
		let source_text = fs::read_to_string(source_file).unwrap_or_default();
		for (line_index, line) in source_text.lines().enumerate() {
			let comment = line.trim_start().trim_start_matches("///").trim_start_matches("//!").trim();
			if let Some(image_name) = read_image_marker(comment) {
				if !example_images.iter().any(|(_, name)| *name == image_name) {
					errors.push(format!("{}:{}: 그림 표시 {} 가 자료에 없음 (# 예제 의 코드 칸 바로 뒤에 둔다)", source_file.strip_prefix(repository_root).unwrap_or(source_file).display(), line_index + 1, image_name));
				}
			}
		}
	}
	let unused_image_count = fs::read_dir(&image_directory).map(|entries| entries.flatten().filter(|entry| {
		let file_name = entry.file_name().to_string_lossy().to_string();
		!file_name.starts_with('.') && !used_images.contains(&file_name)
	}).count()).unwrap_or(0);
	let guide_test_path = repository_root.join("tools/apidoc/src/guide.rs");
	if fs::read_to_string(&guide_test_path).unwrap_or_default() != create_guide_test_source(repository_root, &options.version) {
		errors.push(String::from("가이드 doctest 장 목록(tools/apidoc/src/guide.rs)이 장 파일과 다름. (자료를 다시 뽑으면 다시 쓴다)"));
	}
	for language in ["ko", "en"] {
		let update_path = repository_root.join("docs/update").join(format!("{}.{}.md", options.version, language));
		match fs::read_to_string(&update_path) {
			Ok(update_text) => {
				for line in find_forbidden_characters(&update_text) {
					errors.push(format!("업데이트 {} {} 줄: 긴 줄표나 가운뎃점", language, line));
				}
			}
			Err(_) => errors.push(format!("업데이트 문서 없음: {}", update_path.display())),
		}
	}
	for (name, amount) in &counts {
		println!("{} {}", name, amount);
	}
	println!("since {}", since_counts.iter().map(|(since, amount)| format!("{} {}", since, amount)).collect::<Vec<String>>().join(", "));
	if korean_empty.is_empty() {
		println!("ko 빈 칸 0");
	}
	else {
		println!("ko 빈 칸 {}", korean_empty.iter().map(|(category, amount)| format!("{} {}", category, amount)).collect::<Vec<String>>().join(", "));
	}
	println!("en 빈 칸 {}", english_empty);
	println!("가이드 장 {}, 코드 칸 {}", guide_files.len(), code_block_count);
	println!("그림: 예제 {}, 가이드(ko) {}, 아무도 쓰지 않는 그림 파일 {} (docs/guide/{}/images)", example_images.len(), guide_image_count, unused_image_count, options.version);
	for error in &errors {
		println!("오류: {}", error);
	}
	println!("오류 {} 개", errors.len());
	errors.is_empty()
}

//==============================================================================
/// 자료에서 예제 그림 모으기. ([(자리, 파일 이름)], 모듈, 클래스, 멤버, 함수, 열거의 examples[].image)
//==============================================================================
fn collect_example_images(value: &Value, place: &str, images: &mut Vec<(String, String)>) {
	match value {
		Value::Object(object) => {
			let name = object.get("name").and_then(Value::as_str).unwrap_or("");
			let here = if name.is_empty() {
				place.to_string()
			}
			else if place.is_empty() {
				name.to_string()
			}
			else {
				format!("{}::{}", place, name)
			};
			if let Some(Value::Array(examples)) = object.get("examples") {
				for (index, example) in examples.iter().enumerate() {
					match example.get("image") {
						Some(Value::String(image)) => images.push((format!("{} 예제 {}", here, index + 1), image.clone())),
						Some(_) => images.push((format!("{} 예제 {}", here, index + 1), String::new())),
						None => {}
					}
				}
			}
			for (key, child) in object {
				if key != "examples" {
					collect_example_images(child, &here, images);
				}
			}
		}
		Value::Array(items) => {
			for item in items {
				collect_example_images(item, place, images);
			}
		}
		_ => {}
	}
}

//==============================================================================
/// 자료에서 초안 모듈을 가리키는 짝 모으기. (자리 목록)
//==============================================================================
fn collect_draft_counterparts(value: &Value, place: &str, places: &mut Vec<String>) {
	match value {
		Value::Object(object) => {
			let name = object.get("name").and_then(Value::as_str).unwrap_or("");
			let here = if name.is_empty() {
				place.to_string()
			}
			else if place.is_empty() {
				name.to_string()
			}
			else {
				format!("{}::{}", place, name)
			};
			if let Some(counterpart_module) = object.get("counterpart").and_then(|counterpart| counterpart.get("module")).and_then(Value::as_str) {
				if is_draft_module_name(counterpart_module) {
					places.push(here.clone());
				}
			}
			for (key, child) in object {
				if key != "counterpart" {
					collect_draft_counterparts(child, &here, places);
				}
			}
		}
		Value::Array(items) => {
			for item in items {
				collect_draft_counterparts(item, place, places);
			}
		}
		_ => {}
	}
}

//==============================================================================
/// 가이드 본문의 그림 파일 이름. (`![설명](../images/<이름>)`, 나온 차례, 코드 칸 안은 뺀다)
//==============================================================================
fn read_guide_images(text: &str) -> Vec<String> {
	let mut images = Vec::new();
	let mut is_in_fence = false;
	for line in text.lines() {
		if line.trim_start().starts_with("```") {
			is_in_fence = !is_in_fence;
			continue;
		}
		if is_in_fence {
			continue;
		}
		let mut rest = line;
		while let Some(start) = rest.find("](../images/") {
			let after = &rest[start + "](../images/".len()..];
			let end = after.find(|character: char| character == ')' || character.is_whitespace()).unwrap_or(after.len());
			if rest[..start].contains("![") {
				images.push(after[..end].to_string());
			}
			rest = &after[end..];
		}
	}
	images
}

//==============================================================================
/// 모듈 목록에서 모든 ko 글 모으기. (--missing, --only 로 거른 소스만)
//==============================================================================
fn collect_korean_texts(modules: &[ModuleEntry], options: &Options) -> Vec<String> {
	let mut texts = Vec::new();
	let push_member = |texts: &mut Vec<String>, member: &Member| {
		if !is_path_selected(options, &member.source) {
			return;
		}
		texts.push(member.description.clone());
		for parameter in &member.parameters {
			texts.push(parameter.description.clone());
		}
		if let Some((_, description)) = &member.returns {
			texts.push(description.clone());
		}
		for example in &member.examples {
			texts.push(example.title.clone());
		}
	};
	for module in modules {
		if is_path_selected(options, &module.source) {
			texts.push(module.summary.clone());
			for example in &module.examples {
				texts.push(example.title.clone());
			}
		}
		for class in &module.classes {
			if is_path_selected(options, &class.source) {
				texts.push(class.summary.clone());
				for example in &class.examples {
					texts.push(example.title.clone());
				}
			}
			for member in &class.members {
				push_member(&mut texts, member);
			}
		}
		for function in &module.functions {
			push_member(&mut texts, function);
		}
		for entry in &module.enums {
			if is_path_selected(options, &entry.source) {
				texts.push(entry.summary.clone());
				for value in &entry.values {
					texts.push(value.description.clone());
				}
				for example in &entry.examples {
					texts.push(example.title.clone());
				}
			}
			for member in &entry.members {
				push_member(&mut texts, member);
			}
		}
	}
	texts.into_iter().map(|text| normalize_text(&text)).collect()
}

//==============================================================================
/// 진입점.
//==============================================================================
fn main() {
	let options = match parse_options() {
		Ok(options) => options,
		Err(message) => {
			eprintln!("{}", message);
			std::process::exit(2);
		}
	};
	let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
	let api_directory = repository_root.join("docs/api");
	let output_path = api_directory.join(format!("{}.json", options.version));
	let translation_path = api_directory.join("translation.en.json");
	let counterpart_path = api_directory.join(format!("counterpart.{}.json", options.version));
	if options.is_check {
		let mut is_passed = run_check(&options, &repository_root, &output_path, false);
		let (_, draft_modules) = split_draft_modules(collect_modules(&repository_root, &options.version));
		if !draft_modules.is_empty() {
			let mut draft_translator = Translator {
				table: fs::read_to_string(&translation_path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default(),
				missing: Vec::new(),
			};
			let draft_document = create_document_value(&draft_modules, &options.version, "2000-01-01", &mut draft_translator);
			let draft_path = std::env::temp_dir().join(format!("vanilla-engine-apidoc-draft-{}.json", std::process::id()));
			fs::write(&draft_path, write_json_text(&draft_document)).unwrap();
			is_passed &= run_check(&options, &repository_root, &draft_path, true);
			let _ = fs::remove_file(&draft_path);
		}
		std::process::exit(if is_passed {
			0
		}
		else {
			1
		});
	}
	if options.is_gaps {
		run_gaps(&options, &repository_root);
		return;
	}
	let translation_table: Map<String, Value> = fs::read_to_string(&translation_path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
	let all_modules = collect_modules(&repository_root, &options.version);
	if options.is_missing {
		let mut seen = HashSet::new();
		let missing: Vec<Value> = collect_korean_texts(&all_modules, &options).into_iter().filter(|text| !text.is_empty() && has_hangul(text) && !translation_table.contains_key(text) && seen.insert(text.clone())).map(Value::from).collect();
		print!("{}", write_json_text(&Value::Array(missing)));
		return;
	}
	let (mut modules, draft_modules) = split_draft_modules(all_modules);
	if !options.counterpart_source_path.is_empty() {
		let counterpart_text = match fs::read_to_string(&options.counterpart_source_path) {
			Ok(text) => text,
			Err(error) => {
				eprintln!("짝 엔진 자료를 읽지 못함: {} ({})", options.counterpart_source_path, error);
				std::process::exit(1);
			}
		};
		let counterpart_document: Value = serde_json::from_str(&counterpart_text).unwrap_or(Value::Null);
		let (items, unmatched) = build_counterpart_items(&modules, &counterpart_document);
		let mut table = Map::new();
		table.insert(String::from("engine"), Value::from(ENGINE_NAME));
		table.insert(String::from("counterpartEngine"), Value::from(COUNTERPART_ENGINE_NAME));
		table.insert(String::from("version"), Value::from(options.version.clone()));
		table.insert(String::from("items"), Value::Array(items.clone()));
		fs::write(&counterpart_path, write_json_text(&Value::Object(table))).unwrap();
		println!("짝 {} 개, 못 찾은 것 {} 개 ({})", items.len(), unmatched.len(), counterpart_path.display());
		let unmatched_path = std::env::temp_dir().join("vanilla-engine-apidoc-unmatched.txt");
		let _ = fs::write(&unmatched_path, unmatched.join("\n"));
		println!("못 찾은 것 목록: {}", unmatched_path.display());
	}
	let counterpart_table = load_counterpart_table(&counterpart_path);
	assign_counterparts(&mut modules, &counterpart_table);
	let date = resolve_date(&options, &repository_root, &output_path);
	let mut translator = Translator {
		table: translation_table,
		missing: Vec::new(),
	};
	let document = create_document_value(&modules, &options.version, &date, &mut translator);
	fs::create_dir_all(&api_directory).unwrap();
	fs::write(&output_path, write_json_text(&document)).unwrap();
	fs::write(repository_root.join("tools/apidoc/src/guide.rs"), create_guide_test_source(&repository_root, &options.version)).unwrap();
	let class_count: usize = modules.iter().map(|module| module.classes.len()).sum();
	let member_count: usize = modules.iter().map(|module| module.classes.iter().map(|class| class.members.len()).sum::<usize>() + module.enums.iter().map(|entry| entry.members.len()).sum::<usize>()).sum();
	let function_count: usize = modules.iter().map(|module| module.functions.len()).sum();
	let enum_count: usize = modules.iter().map(|module| module.enums.len()).sum();
	let mut seen = HashSet::new();
	let missing_count = translator.missing.iter().filter(|text| seen.insert((*text).clone())).count();
	println!("{} 를 썼다. 모듈 {}, 클래스 {}, 멤버 {}, 함수와 상수 {}, 열거 {}, en 빈 칸 {} (서로 다른 글 {}), 넣지 않은 초안 모듈 {}", output_path.strip_prefix(&repository_root).unwrap_or(&output_path).display(), modules.len(), class_count, member_count, function_count, enum_count, translator.missing.len(), missing_count, draft_modules.len());
	let mut misplaced_images = MISPLACED_IMAGE_MARKERS.lock().unwrap().clone();
	misplaced_images.sort();
	misplaced_images.dedup();
	if !misplaced_images.is_empty() {
		println!("# 예제 의 코드 칸 바로 뒤가 아니라 그림을 붙이지 못한 그림 표시 {} 개: {}", misplaced_images.len(), misplaced_images.join(", "));
		std::process::exit(1);
	}
}

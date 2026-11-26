//==============================================================================
//! 제3자 라이선스 모음 짓기. (배포물에 넣는 THIRD_PARTY_LICENSES.md, 유니티의 Third Party Notices, 언리얼의 ThirdParty 라이선스 모음과 같은 몫)
//! - `cargo metadata --filter-platform` 으로 대상마다(데스크탑 세 OS 의 x86_64 와 aarch64, 웹 wasm32) 뿌리 패키지에서 닿는 크레이트를 모아 합친다.
//!   일반 의존과 빌드 의존(절차 매크로, 빌드 스크립트가 지은 코드도 실행 파일에 든다)을 따라가고 개발 의존은 뺀다. 뿌리 워크스페이스의 멤버는 뺀다. (그 저장소 자신의 것이라 LICENSE 가 맡는다)
//! - 크레이트마다 풀린 크레이트 폴더(cargo 레지스트리, git 체크아웃, 경로 의존)의 LICENSE, LICENCE, COPYING, COPYRIGHT, NOTICE, UNLICENSE 파일과 LICENSES 폴더, Cargo.toml 의 license-file 을 읽는다.
//! - 라이선스 식(SPDX, 옛 꼴의 "/" 는 OR)의 OR 는 하나를 고른다. 크레이트에 그 전문 파일이 있는 것을 먼저, 그 가운데 표기 의무가 가벼운 것(PREFERENCE 차례)을 고른다. AND 는 모두 싣고, WITH 는 예외 이름을 함께 적는다.
//! - 표기 의무가 없는 라이선스(CC0-1.0, Unlicense, 0BSD, MIT-0)를 고른 크레이트는 끝에 한 줄씩만 적는다.
//! - 전문은 같은 글끼리 한 번만 싣고 크레이트마다 가리킨다. NOTICE 와 COPYRIGHT 파일은 늘 싣는다. (Apache-2.0 4 절 (d)) 저작권 줄은 실은 파일에서 읽고, 없으면 Cargo.toml 의 authors 를 적는다.
//!   MPL-2.0 처럼 실행 파일로 내도 소스를 받을 곳을 알려야 하는 라이선스는 그 주소(crates.io 판, 저장소)를 적는다. (MPL-2.0 3.2 절 (a))
//! - 크레이트에 라이선스 파일이 없으면 SPDX 표준 전문(tools/licenses/texts)을 싣고, 저작권 줄은 Cargo.toml 의 authors 로 지은 것이라고 적는다.
//! - 크레이트가 아닌 실린 자료(소스에 옮겨 온 셈, 자료 파일)는 손으로 쓴다. 표시 줄(BUNDLED_BEGIN_MARKER, BUNDLED_END_MARKER) 사이에 쓰면 다시 지을 때 그대로 남는다.
//!   의존 패키지의 THIRD_PARTY_LICENSES.md 에 그 절이 있으면(게임이 쓰는 vanilla-engine 같은) 그 패키지 이름 아래 싣는다. 그래서 게임 파일에도 엔진이 실은 자료가 들어간다.
//! - 개인 경로(~/.cargo 같은)와 시각은 쓰지 않는다. 의존이 같으면 같은 파일이 나온다.
//! - 사용:
//!   ```text
//!   cargo run -p vanilla-engine-licenses                                   이 저장소의 THIRD_PARTY_LICENSES.md 쓰기
//!   cargo run -p vanilla-engine-licenses -- --check                        파일이 지금 의존과 같은지 보기 (다르면 끝 코드 1)
//!   cargo run -p vanilla-engine-licenses -- --manifest-path <Cargo.toml> --out <파일> [--all-features]
//!                                                                          그 패키지(게임)의 의존을 모은 파일 (.md 는 마크다운, 그 밖은 글)
//!   ```
//==============================================================================
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// 모으는 대상. (대상마다 붙는 크레이트가 달라 모두 합친다. windows-sys 의 대상 크레이트, 리눅스의 x11 과 웨이랜드, wasm 의 web-sys 같은)
const TARGETS: [&str; 7] = ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc", "x86_64-apple-darwin", "aarch64-apple-darwin", "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "wasm32-unknown-unknown"];
/// 짓는 파일 이름.
const OUTPUT_FILE_NAME: &str = "THIRD_PARTY_LICENSES.md";
/// 손으로 쓰는 실린 자료 절의 앞 표시 줄.
const BUNDLED_BEGIN_MARKER: &str = "<!-- vanilla-engine-licenses: bundled materials begin -->";
/// 손으로 쓰는 실린 자료 절의 뒤 표시 줄.
const BUNDLED_END_MARKER: &str = "<!-- vanilla-engine-licenses: bundled materials end -->";
/// 표기 의무가 없는 라이선스. (퍼블릭 도메인 바치기와 고지 조건이 없는 허용 라이선스)
const NO_ATTRIBUTION_LICENSES: [&str; 4] = ["CC0-1.0", "Unlicense", "0BSD", "MIT-0"];
/// OR 에서 고르는 차례. (앞일수록 표기 의무가 가볍다. 여기 없는 이름은 이 뒤, GPL 계열은 맨 뒤)
const PREFERENCE: [&str; 15] = ["CC0-1.0", "Unlicense", "0BSD", "MIT-0", "MIT", "ISC", "BSD-2-Clause", "BSD-3-Clause", "Zlib", "BSL-1.0", "Apache-2.0", "Unicode-3.0", "Unicode-DFS-2016", "CDLA-Permissive-2.0", "MPL-2.0"];
/// 크레이트에 라이선스 파일이 없을 때 싣는 SPDX 표준 전문. (이름, 전문, 맨 위에 저작권 줄을 붙이는 틀인지)
const STANDARD_TEXTS: [(&str, &str, bool); 7] = [
	("MIT", include_str!("../texts/MIT.txt"), true),
	("Apache-2.0", include_str!("../texts/Apache-2.0.txt"), false),
	("BSL-1.0", include_str!("../texts/BSL-1.0.txt"), false),
	("BSD-2-Clause", include_str!("../texts/BSD-2-Clause.txt"), true),
	("BSD-3-Clause", include_str!("../texts/BSD-3-Clause.txt"), true),
	("ISC", include_str!("../texts/ISC.txt"), true),
	("Zlib", include_str!("../texts/Zlib.txt"), true),
];
/// 글 꼴 줄의 가로줄. (80 칸)
const TEXT_RULE: &str = "--------------------------------------------------------------------------------";

//==============================================================================
/// 라이선스 식. (SPDX)
//==============================================================================
#[derive(Debug, Clone, PartialEq)]
enum Expression {
	/// 라이선스 하나. (WITH 예외가 있으면 그 이름)
	License { identifier: String, exception: Option<String> },
	/// 모두 따른다.
	And(Vec<Expression>),
	/// 하나를 고른다.
	Or(Vec<Expression>),
}

//==============================================================================
/// 고른 라이선스 하나.
//==============================================================================
#[derive(Debug, Clone, PartialEq)]
struct ChosenLicense {
	identifier: String,
	exception: Option<String>,
}

//==============================================================================
/// 라이선스 파일의 몫.
//==============================================================================
#[derive(Debug, Clone, Copy, PartialEq)]
enum FileRole {
	/// 라이선스 전문.
	License,
	/// NOTICE 파일. (Apache-2.0 4 절 (d), 늘 싣는다)
	Notice,
	/// COPYRIGHT 파일. (저작권과 라이선스 안내, 늘 싣는다)
	Copyright,
}

//==============================================================================
/// 크레이트 폴더의 라이선스 파일 하나.
//==============================================================================
struct LicenseFile {
	/// 크레이트 폴더 기준 이름. (LICENSES 폴더면 "LICENSES/이름")
	name: String,
	/// 글에서 읽은 라이선스 갈래. (license_family 의 이름, 모르면 비었다)
	families: Vec<String>,
	role: FileRole,
	text: String,
}

//==============================================================================
/// 실을 전문 하나. (제목과 글)
//==============================================================================
struct NoticeText {
	title: String,
	text: String,
}

//==============================================================================
/// 크레이트 하나의 고지.
//==============================================================================
struct CrateEntry {
	name: String,
	version: String,
	/// Cargo.toml 의 license 칸 그대로. (없으면 license-file 이나 NOASSERTION)
	expression_text: String,
	chosen: Vec<ChosenLicense>,
	/// 받은 곳. (crates.io, git 주소와 커밋, 경로 의존)
	source: String,
	repository: Option<String>,
	copyright_lines: Vec<String>,
	authors: Vec<String>,
	texts: Vec<NoticeText>,
	notes: Vec<String>,
	attribution_required: bool,
}

//==============================================================================
/// 실린 자료 절 하나. (머리와 마크다운 글)
//==============================================================================
struct BundledSection {
	heading: String,
	text: String,
}

//==============================================================================
/// 명령 줄 인자.
//==============================================================================
struct Arguments {
	manifest_path: Option<PathBuf>,
	output_path: Option<PathBuf>,
	check: bool,
	all_features: bool,
}

//==============================================================================
/// 명령 줄 인자를 읽는다.
//==============================================================================
fn parse_arguments() -> Result<Arguments, String> {
	let mut arguments = Arguments { manifest_path: None, output_path: None, check: false, all_features: false };
	let mut iterator = std::env::args().skip(1);
	while let Some(argument) = iterator.next() {
		match argument.as_str() {
			"--manifest-path" => {
				let value = iterator.next().ok_or("--manifest-path 뒤에 Cargo.toml 경로가 없다")?;
				arguments.manifest_path = Some(PathBuf::from(value));
			}
			"--out" => {
				let value = iterator.next().ok_or("--out 뒤에 파일 경로가 없다")?;
				arguments.output_path = Some(PathBuf::from(value));
			}
			"--check" => {
				arguments.check = true;
			}
			"--all-features" => {
				arguments.all_features = true;
			}
			_ => {
				return Err(format!("모르는 인자: {argument} (--manifest-path <Cargo.toml>, --out <파일>, --check, --all-features)"));
			}
		}
	}
	Ok(arguments)
}

//==============================================================================
/// 한 대상의 cargo metadata 를 읽는다.
//==============================================================================
fn read_metadata(arguments: &Arguments, target: &str) -> Result<Value, String> {
	let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
	let mut command = Command::new(cargo);
	command.args(["metadata", "--format-version", "1", "--filter-platform", target]);
	if let Some(manifest_path) = &arguments.manifest_path {
		command.arg("--manifest-path").arg(manifest_path);
	}
	if arguments.all_features {
		command.arg("--all-features");
	}
	let output = command.output().map_err(|error| format!("cargo metadata 를 부르지 못했다: {error}"))?;
	if !output.status.success() {
		return Err(format!("cargo metadata --filter-platform {target} 이 실패했다:\n{}", String::from_utf8_lossy(&output.stderr)));
	}
	serde_json::from_slice(&output.stdout).map_err(|error| format!("cargo metadata 의 JSON 을 읽지 못했다: {error}"))
}

//==============================================================================
/// 뿌리 패키지에서 일반 의존과 빌드 의존으로 닿는 패키지 id 를 모은다.
//==============================================================================
fn collect_reachable(metadata: &Value, reachable: &mut HashSet<String>) {
	let resolve = &metadata["resolve"];
	let mut node_table: HashMap<&str, &Value> = HashMap::new();
	for node in resolve["nodes"].as_array().into_iter().flatten() {
		if let Some(identifier) = node["id"].as_str() {
			node_table.insert(identifier, node);
		}
	}
	let mut pending: Vec<String> = Vec::new();
	if let Some(root) = resolve["root"].as_str() {
		pending.push(root.to_string());
	}
	else {
		for member in metadata["workspace_default_members"].as_array().into_iter().flatten() {
			if let Some(identifier) = member.as_str() {
				pending.push(identifier.to_string());
			}
		}
	}
	let mut visited: HashSet<String> = HashSet::new();
	while let Some(identifier) = pending.pop() {
		if !visited.insert(identifier.clone()) {
			continue;
		}
		let Some(node) = node_table.get(identifier.as_str()) else {
			continue;
		};
		for dependency in node["deps"].as_array().into_iter().flatten() {
			let shipped = dependency["dep_kinds"].as_array().into_iter().flatten().any(|kind| kind["kind"].is_null() || kind["kind"].as_str() == Some("build"));
			if shipped {
				if let Some(package) = dependency["pkg"].as_str() {
					pending.push(package.to_string());
				}
			}
		}
	}
	reachable.extend(visited);
}

//==============================================================================
/// SPDX 라이선스 식을 읽는다. (옛 꼴의 "/" 는 OR, 대소문자를 가리지 않는 AND, OR, WITH)
//==============================================================================
fn parse_expression(text: &str) -> Option<Expression> {
	let mut tokens: Vec<String> = Vec::new();
	let mut current = String::new();
	for character in text.chars() {
		if character == '(' || character == ')' || character == '/' || character.is_whitespace() {
			if !current.is_empty() {
				tokens.push(std::mem::take(&mut current));
			}
			if character == '/' {
				tokens.push("OR".to_string());
			}
			else if !character.is_whitespace() {
				tokens.push(character.to_string());
			}
		}
		else {
			current.push(character);
		}
	}
	if !current.is_empty() {
		tokens.push(current);
	}
	let mut position = 0;
	let expression = parse_or(&tokens, &mut position)?;
	if position != tokens.len() {
		return None;
	}
	Some(expression)
}

//==============================================================================
/// OR 로 이은 식을 읽는다.
//==============================================================================
fn parse_or(tokens: &[String], position: &mut usize) -> Option<Expression> {
	let mut list = vec![parse_and(tokens, position)?];
	while *position < tokens.len() && tokens[*position].eq_ignore_ascii_case("OR") {
		*position += 1;
		list.push(parse_and(tokens, position)?);
	}
	if list.len() == 1 {
		return list.pop();
	}
	Some(Expression::Or(list))
}

//==============================================================================
/// AND 로 이은 식을 읽는다.
//==============================================================================
fn parse_and(tokens: &[String], position: &mut usize) -> Option<Expression> {
	let mut list = vec![parse_with(tokens, position)?];
	while *position < tokens.len() && tokens[*position].eq_ignore_ascii_case("AND") {
		*position += 1;
		list.push(parse_with(tokens, position)?);
	}
	if list.len() == 1 {
		return list.pop();
	}
	Some(Expression::And(list))
}

//==============================================================================
/// 라이선스 하나(WITH 예외 포함)나 괄호 식을 읽는다.
//==============================================================================
fn parse_with(tokens: &[String], position: &mut usize) -> Option<Expression> {
	let token = tokens.get(*position)?;
	if token == "(" {
		*position += 1;
		let inner = parse_or(tokens, position)?;
		if tokens.get(*position).map(String::as_str) != Some(")") {
			return None;
		}
		*position += 1;
		return Some(inner);
	}
	if token == ")" || token.eq_ignore_ascii_case("AND") || token.eq_ignore_ascii_case("OR") || token.eq_ignore_ascii_case("WITH") {
		return None;
	}
	let identifier = token.clone();
	*position += 1;
	let mut exception = None;
	if *position < tokens.len() && tokens[*position].eq_ignore_ascii_case("WITH") {
		*position += 1;
		exception = Some(tokens.get(*position)?.clone());
		*position += 1;
	}
	Some(Expression::License { identifier, exception })
}

//==============================================================================
/// 식에 OR 가 있는지 본다.
//==============================================================================
fn has_or(expression: &Expression) -> bool {
	match expression {
		Expression::License { .. } => {
			false
		}
		Expression::And(list) => {
			list.iter().any(has_or)
		}
		Expression::Or(_) => {
			true
		}
	}
}

//==============================================================================
/// 라이선스 이름의 갈래. (파일 글에서 읽은 갈래와 맞춰 보는 이름, BSD 2 조와 3 조는 글로 가르기 어려워 하나로 본다)
//==============================================================================
fn license_family(identifier: &str) -> String {
	let base = identifier.trim_end_matches('+').trim_end_matches("-or-later").trim_end_matches("-only");
	if base.contains("LGPL") {
		return "LGPL".to_string();
	}
	if base.contains("GPL") {
		return "GPL".to_string();
	}
	if base == "0BSD" {
		return "0BSD".to_string();
	}
	if base.starts_with("BSD-") {
		return "BSD".to_string();
	}
	if base.starts_with("MPL-") {
		return "MPL-2.0".to_string();
	}
	if base.starts_with("Unicode-") {
		return "Unicode".to_string();
	}
	if base.starts_with("CDLA-") {
		return "CDLA".to_string();
	}
	base.to_string()
}

//==============================================================================
/// OR 에서 고르는 차례의 값. (작을수록 먼저 고른다)
//==============================================================================
fn preference_rank(identifier: &str) -> usize {
	if let Some(index) = PREFERENCE.iter().position(|name| *name == identifier) {
		return index;
	}
	if identifier.contains("GPL") {
		return PREFERENCE.len() + 1;
	}
	PREFERENCE.len()
}

//==============================================================================
/// 표기 의무가 있는 라이선스인지 본다.
//==============================================================================
fn requires_attribution(identifier: &str) -> bool {
	!NO_ATTRIBUTION_LICENSES.contains(&identifier)
}

//==============================================================================
/// 식에서 따를 라이선스를 고른다.
/// - OR 는 갈래마다 고른 결과를 견주어, 전문 파일이 없는 표기 의무 라이선스가 적은 것, 그다음 가장 무거운 라이선스의 차례가 앞선 것, 그다음 라이선스 수가 적은 것을 고른다.
//==============================================================================
fn choose_licenses(expression: &Expression, available_families: &[String]) -> Vec<ChosenLicense> {
	match expression {
		Expression::License { identifier, exception } => {
			vec![ChosenLicense { identifier: identifier.clone(), exception: exception.clone() }]
		}
		Expression::And(list) => {
			let mut chosen = Vec::new();
			for item in list {
				for license in choose_licenses(item, available_families) {
					if !chosen.contains(&license) {
						chosen.push(license);
					}
				}
			}
			chosen
		}
		Expression::Or(list) => {
			let mut best: Option<((usize, usize, usize), Vec<ChosenLicense>)> = None;
			for item in list {
				let candidate = choose_licenses(item, available_families);
				let missing = candidate.iter().filter(|license| requires_attribution(&license.identifier) && !available_families.contains(&license_family(&license.identifier))).count();
				let worst = candidate.iter().map(|license| preference_rank(&license.identifier)).max().unwrap_or(0);
				let cost = (missing, worst, candidate.len());
				let better = match &best {
					Some((best_cost, _)) => {
						cost < *best_cost
					}
					None => {
						true
					}
				};
				if better {
					best = Some((cost, candidate));
				}
			}
			best.map(|(_, chosen)| chosen).unwrap_or_default()
		}
	}
}

//==============================================================================
/// 파일 이름이 라이선스 파일 같은지 본다. (이 도구가 짓는 파일은 실린 자료 절로 따로 읽어 뺀다)
//==============================================================================
fn is_license_file_name(name: &str) -> bool {
	let upper = name.to_uppercase();
	if upper.ends_with(".RS") || name == OUTPUT_FILE_NAME {
		return false;
	}
	upper.contains("LICENSE") || upper.contains("LICENCE") || upper.starts_with("COPYING") || upper.starts_with("COPYRIGHT") || upper.starts_with("NOTICE")
}

//==============================================================================
/// 라이선스 파일의 몫을 이름으로 가른다.
//==============================================================================
fn file_role(name: &str) -> FileRole {
	let upper = name.rsplit('/').next().unwrap_or(name).to_uppercase();
	if upper.starts_with("NOTICE") {
		return FileRole::Notice;
	}
	if upper.starts_with("COPYRIGHT") {
		return FileRole::Copyright;
	}
	FileRole::License
}

//==============================================================================
/// 라이선스 파일의 갈래를 읽는다. (이름에 갈래가 적혔으면 이름으로, 아니면 글의 표준 문구로)
//==============================================================================
fn detect_families(name: &str, text: &str) -> Vec<String> {
	let mut families: Vec<String> = Vec::new();
	let upper = name.rsplit('/').next().unwrap_or(name).to_uppercase();
	let name_rules: [(&[&str], &str); 13] = [
		(&["APACHE"], "Apache-2.0"),
		(&["MIT-0", "MIT0"], "MIT-0"),
		(&["MIT"], "MIT"),
		(&["0BSD"], "0BSD"),
		(&["BSD"], "BSD"),
		(&["ZLIB"], "Zlib"),
		(&["ISC"], "ISC"),
		(&["UNICODE"], "Unicode"),
		(&["MPL"], "MPL-2.0"),
		(&["BOOST", "BSL"], "BSL-1.0"),
		(&["UNLICENSE"], "Unlicense"),
		(&["CC0"], "CC0-1.0"),
		(&["CDLA"], "CDLA"),
	];
	for (keywords, family) in name_rules {
		if keywords.iter().any(|keyword| upper.contains(keyword)) {
			let already = families.iter().any(|known| known == "MIT-0" && family == "MIT") || families.iter().any(|known| known == "0BSD" && family == "BSD");
			if !already {
				families.push(family.to_string());
			}
		}
	}
	if upper.contains("LGPL") {
		families.push("LGPL".to_string());
	}
	else if upper.contains("GPL") {
		families.push("GPL".to_string());
	}
	if !families.is_empty() {
		return families;
	}
	let uncommented: Vec<&str> = text.lines().map(|line| line.trim().trim_start_matches(['/', '#', '*', ';']).trim()).collect();
	let normalized = uncommented.join(" ").to_lowercase().replace(['\u{2018}', '\u{2019}'], "'").split_whitespace().collect::<Vec<_>>().join(" ");
	let opening: String = normalized.chars().take(200).collect();
	let unicode = normalized.contains("unicode, inc.") && normalized.contains("permission is hereby granted");
	if unicode {
		families.push("Unicode".to_string());
	}
	if normalized.contains("apache license") && normalized.contains("version 2.0") {
		families.push("Apache-2.0".to_string());
	}
	if normalized.contains("to deal in the software without restriction") {
		families.push("MIT".to_string());
	}
	if normalized.contains("mozilla public license") && normalized.contains("2.0") {
		families.push("MPL-2.0".to_string());
	}
	if normalized.contains("boost software license") {
		families.push("BSL-1.0".to_string());
	}
	if normalized.contains("this is free and unencumbered software released into the public domain") {
		families.push("Unlicense".to_string());
	}
	if normalized.contains("distribute this software for any purpose with or without fee") {
		if normalized.contains("provided that the above copyright notice") {
			families.push("ISC".to_string());
		}
		else {
			families.push("0BSD".to_string());
		}
	}
	if normalized.contains("redistribution and use in source and binary forms") {
		families.push("BSD".to_string());
	}
	if normalized.contains("this software is provided 'as-is'") {
		families.push("Zlib".to_string());
	}
	if normalized.contains("community data license agreement") {
		families.push("CDLA".to_string());
	}
	if normalized.contains("cc0 1.0 universal") {
		families.push("CC0-1.0".to_string());
	}
	// GPL 계열은 글이 그 라이선스로 시작할 때만. (MPL-2.0 본문도 GNU 라이선스 이름을 든다)
	if opening.contains("gnu lesser general public license") {
		families.push("LGPL".to_string());
	}
	else if opening.contains("gnu general public license") {
		families.push("GPL".to_string());
	}
	families
}

//==============================================================================
/// 글을 고지에 싣는 꼴로 고른다. (BOM 과 CR 을 빼고, 줄 끝 공백과 앞뒤 빈 줄을 뗀다)
//==============================================================================
fn tidy_text(text: &str) -> String {
	let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
	let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
	let first = lines.iter().position(|line| !line.is_empty()).unwrap_or(lines.len());
	let last = lines.iter().rposition(|line| !line.is_empty()).map(|index| index + 1).unwrap_or(first);
	lines[first..last].join("\n")
}

//==============================================================================
/// 크레이트 폴더의 라이선스 파일을 읽는다. (뿌리의 파일, LICENSES 폴더, license-file 칸, 이름 차례)
//==============================================================================
fn read_license_files(directory: &Path, license_file: Option<&str>) -> Vec<LicenseFile> {
	let mut name_list: Vec<String> = Vec::new();
	if let Ok(entries) = fs::read_dir(directory) {
		for entry in entries.flatten() {
			let name = entry.file_name().to_string_lossy().to_string();
			let path = entry.path();
			if path.is_file() && is_license_file_name(&name) {
				name_list.push(name);
			}
			else if path.is_dir() && name.eq_ignore_ascii_case("LICENSES") {
				if let Ok(inner_entries) = fs::read_dir(&path) {
					for inner in inner_entries.flatten() {
						if inner.path().is_file() {
							name_list.push(format!("{name}/{}", inner.file_name().to_string_lossy()));
						}
					}
				}
			}
		}
	}
	if let Some(license_file) = license_file {
		let name = license_file.replace('\\', "/").trim_start_matches("./").to_string();
		if !name_list.contains(&name) {
			name_list.push(name);
		}
	}
	name_list.sort();
	let mut file_list = Vec::new();
	for name in name_list {
		let Ok(bytes) = fs::read(directory.join(&name)) else {
			continue;
		};
		let text = tidy_text(&String::from_utf8_lossy(&bytes));
		if text.is_empty() {
			continue;
		}
		file_list.push(LicenseFile { families: detect_families(&name, &text), role: file_role(&name), name, text });
	}
	file_list
}

//==============================================================================
/// 글에서 저작권 줄을 찾는다. (Copyright, (c), © 로 시작하는 줄. 틀의 빈칸 줄과 라이선스 본문의 낱말은 뺀다)
//==============================================================================
fn find_copyright_lines(text: &str) -> Vec<String> {
	let mut lines: Vec<String> = Vec::new();
	for line in text.lines() {
		let trimmed = line.trim().trim_start_matches(['#', '*', '/', '-', ' ']).trim();
		let lower = trimmed.to_lowercase();
		let starts = lower.starts_with("copyright") || lower.starts_with("(c)") || trimmed.starts_with('©');
		if !starts {
			continue;
		}
		let placeholder = ["[yyyy]", "<year>", "{yyyy}", "[year]", "<yyyy>", "[name of", "<name of", "<owner>", "<copyright holders>"].iter().any(|word| lower.contains(word));
		if placeholder {
			continue;
		}
		if !lower.starts_with("copyright") && !trimmed.chars().any(|character| character.is_ascii_digit()) {
			continue;
		}
		if lower.starts_with("copyright") {
			let rest = trimmed["copyright".len()..].trim_start();
			let next = rest.chars().next();
			let looks_like_owner = match next {
				Some(character) => {
					character == '(' || character == '©' || character.is_ascii_digit() || character.is_uppercase()
				}
				None => {
					false
				}
			};
			let body_word = ["notice", "license", "owner", "holder", "statement", "act", "and ", "law"].iter().any(|word| rest.to_lowercase().starts_with(word));
			if !looks_like_owner || body_word {
				continue;
			}
		}
		let line = trimmed.trim_end_matches(" or").trim_end_matches(" and").to_string();
		if !lines.contains(&line) {
			lines.push(line);
		}
	}
	lines
}

//==============================================================================
/// Cargo.toml 의 authors 에서 이름만 뗀다. (꺾쇠 안의 메일 주소를 뺀다)
//==============================================================================
fn author_names(package: &Value) -> Vec<String> {
	let mut names = Vec::new();
	for author in package["authors"].as_array().into_iter().flatten() {
		if let Some(text) = author.as_str() {
			let name = match text.find('<') {
				Some(index) => {
					text[..index].trim()
				}
				None => {
					text.trim()
				}
			};
			if !name.is_empty() {
				names.push(name.to_string());
			}
		}
	}
	names
}

//==============================================================================
/// 받은 곳을 적는다. (crates.io, 다른 레지스트리, git 주소와 커밋, 경로 의존. 개인 경로는 쓰지 않는다)
//==============================================================================
fn describe_source(package: &Value) -> String {
	let Some(source) = package["source"].as_str() else {
		return "local path".to_string();
	};
	if source == "registry+https://github.com/rust-lang/crates.io-index" || source == "sparse+https://index.crates.io/" {
		return "crates.io".to_string();
	}
	if let Some(rest) = source.strip_prefix("git+") {
		let (address, commit) = match rest.split_once('#') {
			Some((address, commit)) => {
				(address, Some(commit))
			}
			None => {
				(rest, None)
			}
		};
		let address = address.split('?').next().unwrap_or(address);
		return match commit {
			Some(commit) => {
				format!("git {address} (commit {})", &commit[..commit.len().min(12)])
			}
			None => {
				format!("git {address}")
			}
		};
	}
	source.split_once('+').map(|(_, address)| address.to_string()).unwrap_or_else(|| source.to_string())
}

//==============================================================================
/// 고른 라이선스를 적는다. (WITH 예외 포함, " AND " 로 잇는다)
//==============================================================================
fn describe_chosen(chosen: &[ChosenLicense]) -> String {
	chosen.iter().map(|license| match &license.exception {
		Some(exception) => {
			format!("{} WITH {exception}", license.identifier)
		}
		None => {
			license.identifier.clone()
		}
	}).collect::<Vec<_>>().join(" AND ")
}

//==============================================================================
/// 파일의 전문 제목. (갈래나 몫, 모르면 파일 이름)
//==============================================================================
fn file_title(file: &LicenseFile) -> String {
	match file.role {
		FileRole::Notice => {
			"NOTICE".to_string()
		}
		FileRole::Copyright => {
			"COPYRIGHT".to_string()
		}
		FileRole::License => {
			if file.families.is_empty() {
				format!("License ({})", file.name)
			}
			else {
				file.families.join(", ")
			}
		}
	}
}

//==============================================================================
/// 패키지 하나의 고지를 짓는다.
//==============================================================================
fn build_crate_entry(package: &Value, warnings: &mut Vec<String>) -> CrateEntry {
	let name = package["name"].as_str().unwrap_or_default().to_string();
	let version = package["version"].as_str().unwrap_or_default().to_string();
	let directory = Path::new(package["manifest_path"].as_str().unwrap_or_default()).parent().map(Path::to_path_buf).unwrap_or_default();
	let license_file = package["license_file"].as_str();
	let file_list = read_license_files(&directory, license_file);
	let authors = author_names(package);
	let repository = package["repository"].as_str().map(str::to_string);
	let mut notes: Vec<String> = Vec::new();

	let expression_text = match (package["license"].as_str(), license_file) {
		(Some(license), _) => {
			license.to_string()
		}
		(None, Some(license_file)) => {
			format!("LicenseRef-file ({license_file})")
		}
		(None, None) => {
			"NOASSERTION".to_string()
		}
	};
	let expression = match package["license"].as_str() {
		Some(license) => {
			parse_expression(license)
		}
		None => {
			None
		}
	};
	let mut available_families: Vec<String> = Vec::new();
	for file in &file_list {
		if file.role == FileRole::License {
			available_families.extend(file.families.iter().cloned());
		}
	}
	let chosen = match &expression {
		Some(expression) => {
			choose_licenses(expression, &available_families)
		}
		None => {
			if package["license"].is_string() {
				warnings.push(format!("{name} {version}: 라이선스 식을 읽지 못했다 ({expression_text})"));
			}
			else if license_file.is_none() {
				warnings.push(format!("{name} {version}: Cargo.toml 에 license 도 license-file 도 없다"));
			}
			vec![ChosenLicense { identifier: expression_text.clone(), exception: None }]
		}
	};
	let attribution_required = chosen.iter().any(|license| requires_attribution(&license.identifier));
	if !attribution_required {
		return CrateEntry { name, version, expression_text, chosen, source: describe_source(package), repository, copyright_lines: Vec::new(), authors, texts: Vec::new(), notes, attribution_required };
	}

	// 실을 파일. (고른 라이선스의 전문, NOTICE, COPYRIGHT. 갈래를 모르는 파일은 OR 가 없는 식이거나 고른 라이선스의 전문을 못 찾았을 때 싣는다)
	let chosen_families: Vec<String> = chosen.iter().filter(|license| requires_attribution(&license.identifier)).map(|license| license_family(&license.identifier)).collect();
	let mut covered: Vec<String> = Vec::new();
	let mut included: Vec<&LicenseFile> = Vec::new();
	let mut unknown: Vec<&LicenseFile> = Vec::new();
	for file in &file_list {
		match file.role {
			FileRole::Notice | FileRole::Copyright => {
				included.push(file);
			}
			FileRole::License => {
				if file.families.is_empty() {
					unknown.push(file);
				}
				else if file.families.iter().any(|family| chosen_families.contains(family)) {
					covered.extend(file.families.iter().filter(|family| chosen_families.contains(family)).cloned());
					included.push(file);
				}
			}
		}
	}
	let all_covered = chosen_families.iter().all(|family| covered.contains(family));
	let use_unknown = !unknown.is_empty() && (expression.as_ref().is_none_or(|expression| !has_or(expression)) || !all_covered);
	if use_unknown {
		included.extend(unknown.iter().copied());
	}
	included.sort_by_key(|file| (file.role != FileRole::License, file.name.clone()));
	let mut texts: Vec<NoticeText> = included.iter().map(|file| {
		let matched: Vec<String> = chosen.iter().filter(|license| file.role == FileRole::License && file.families.contains(&license_family(&license.identifier))).map(|license| license.identifier.clone()).collect();
		let title = if matched.is_empty() {
			file_title(file)
		}
		else {
			matched.join(", ")
		};
		NoticeText { title, text: file.text.clone() }
	}).collect();
	let mut copyright_lines: Vec<String> = Vec::new();
	for file in &included {
		for line in find_copyright_lines(&file.text) {
			if !copyright_lines.contains(&line) {
				copyright_lines.push(line);
			}
		}
	}

	// 크레이트에 전문이 없는 라이선스는 SPDX 표준 전문. (틀이면 맨 위에 authors 로 지은 저작권 줄)
	if !use_unknown {
		for license in chosen.iter().filter(|license| requires_attribution(&license.identifier)) {
			let family = license_family(&license.identifier);
			if covered.contains(&family) {
				continue;
			}
			match STANDARD_TEXTS.iter().find(|(identifier, _, _)| *identifier == license.identifier) {
				Some((identifier, text, template)) => {
					let mut text = tidy_text(text);
					if *template {
						let holder = if authors.is_empty() {
							format!("the {name} authors")
						}
						else {
							authors.join(", ")
						};
						let line = format!("Copyright (c) {holder}");
						text = format!("{line}\n\n{text}");
						if !copyright_lines.contains(&line) {
							copyright_lines.push(line);
						}
					}
					notes.push(format!("The crate package has no {identifier} license file. The SPDX standard text is reproduced{}.", if *template { " with a copyright line made from the Cargo.toml authors" } else { "" }));
					texts.push(NoticeText { title: identifier.to_string(), text });
				}
				None => {
					notes.push(format!("The crate package has no {} license file. See https://spdx.org/licenses/{}.html for the license text.", license.identifier, license.identifier));
					warnings.push(format!("{name} {version}: {} 전문을 크레이트에서도 표준 전문(texts)에서도 찾지 못했다", license.identifier));
				}
			}
		}
	}

	// 소스 공개 조건. (MPL-2.0 3.2 절 (a) 같은)
	let disclosure = chosen.iter().any(|license| license.identifier.starts_with("MPL-") || license.identifier.contains("GPL") || license.identifier.starts_with("EPL-") || license.identifier.starts_with("CDDL-"));
	if disclosure {
		let mut place_list: Vec<String> = Vec::new();
		if describe_source(package) == "crates.io" {
			place_list.push(format!("https://crates.io/crates/{name}/{version}"));
		}
		if let Some(repository) = &repository {
			place_list.push(repository.clone());
		}
		notes.push(format!("Source Code Form: this crate is used unmodified, and its source code is available at {}.", place_list.join(" and ")));
	}

	CrateEntry { name, version, expression_text, chosen, source: describe_source(package), repository, copyright_lines, authors, texts, notes, attribution_required }
}

//==============================================================================
/// 판 글을 견준다. (점과 하이픈으로 나눈 조각을 수면 수로, 아니면 글로)
//==============================================================================
fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
	let left_parts: Vec<&str> = left.split(['.', '-', '+']).collect();
	let right_parts: Vec<&str> = right.split(['.', '-', '+']).collect();
	for (left_part, right_part) in left_parts.iter().zip(right_parts.iter()) {
		let ordering = match (left_part.parse::<u64>(), right_part.parse::<u64>()) {
			(Ok(left_number), Ok(right_number)) => {
				left_number.cmp(&right_number)
			}
			_ => {
				left_part.cmp(right_part)
			}
		};
		if ordering != std::cmp::Ordering::Equal {
			return ordering;
		}
	}
	left_parts.len().cmp(&right_parts.len())
}

//==============================================================================
/// THIRD_PARTY_LICENSES.md 에서 표시 줄 사이의 손으로 쓴 절을 읽는다. (없으면 None)
//==============================================================================
fn read_bundled_section(path: &Path) -> Option<String> {
	let text = fs::read_to_string(path).ok()?.replace("\r\n", "\n");
	let begin = text.find(BUNDLED_BEGIN_MARKER)? + BUNDLED_BEGIN_MARKER.len();
	let end = begin + text[begin..].find(BUNDLED_END_MARKER)?;
	Some(text[begin..end].trim_matches('\n').to_string())
}

//==============================================================================
/// 마크다운 목록 줄에 넣을 글. (꺾쇠가 HTML 꼬리표로 읽히지 않게 바꾼다)
//==============================================================================
fn markdown_inline(text: &str) -> String {
	text.replace('<', "&lt;")
}

//==============================================================================
/// 고지 파일 글을 짓는다.
//==============================================================================
fn render(markdown: bool, own_section: Option<&str>, bundled_list: &[BundledSection], crate_list: &[CrateEntry]) -> String {
	// 전문 묶기. (공백을 접은 글이 같으면 한 번만 싣는다)
	let mut text_key_table: HashMap<String, usize> = HashMap::new();
	let mut text_list: Vec<(&NoticeText, Vec<String>)> = Vec::new();
	let mut crate_text_numbers: Vec<Vec<usize>> = Vec::new();
	for entry in crate_list {
		let mut numbers = Vec::new();
		for notice in &entry.texts {
			let key = notice.text.split_whitespace().collect::<Vec<_>>().join(" ");
			let index = match text_key_table.get(&key) {
				Some(index) => {
					*index
				}
				None => {
					text_list.push((notice, Vec::new()));
					text_key_table.insert(key, text_list.len() - 1);
					text_list.len() - 1
				}
			};
			let user = format!("{} {}", entry.name, entry.version);
			if !text_list[index].1.contains(&user) {
				text_list[index].1.push(user);
			}
			if !numbers.contains(&(index + 1)) {
				numbers.push(index + 1);
			}
		}
		crate_text_numbers.push(numbers);
	}
	let attributed_count = crate_list.iter().filter(|entry| entry.attribution_required).count();
	let mut output = String::new();

	// 머리.
	if markdown {
		output.push_str("# Third-Party Licenses\n\n");
	}
	else {
		output.push_str("THIRD-PARTY LICENSES\n====================\n\n");
	}
	output.push_str("이 소프트웨어에 들어가는 제3자 크레이트와 자료의 라이선스 고지를 모은 파일입니다.\n");
	output.push_str("This file collects the license notices for the third-party crates and materials built into this software.\n\n");
	let bullet = if markdown { "- " } else { "* " };
	let code = |text: &str| -> String {
		if markdown {
			format!("`{text}`")
		}
		else {
			text.to_string()
		}
	};
	output.push_str(&format!("{bullet}Generated by {} from {} and the license files in each crate package. Only the bundled materials section between the markers is written by hand; the rest is regenerated.\n", code("vanilla-engine-licenses"), code("cargo metadata")));
	output.push_str(&format!("{bullet}Targets: {}. Normal and build dependencies are included; dev-dependencies are not.\n", TARGETS.iter().map(|target| code(target)).collect::<Vec<_>>().join(", ")));
	output.push_str(&format!("{bullet}Where a crate offers a choice of licenses, the license used is the one shown after \"used under\".\n"));
	output.push_str(&format!("{bullet}Crates: {} ({} with notices below, {} under licenses that require no notice). License texts: {}.\n\n", crate_list.len(), attributed_count, crate_list.len() - attributed_count, text_list.len()));

	// 실린 자료.
	if own_section.is_some() || !bundled_list.is_empty() {
		if markdown {
			output.push_str("## Bundled Materials\n\n");
		}
		else {
			output.push_str("BUNDLED MATERIALS\n=================\n\n");
		}
		if let Some(section) = own_section {
			if markdown {
				output.push_str(&format!("{BUNDLED_BEGIN_MARKER}\n{section}\n{BUNDLED_END_MARKER}\n\n"));
			}
			else {
				output.push_str(&format!("{section}\n\n"));
			}
		}
		for bundled in bundled_list {
			if markdown {
				output.push_str(&format!("### {}\n\n{}\n\n", bundled.heading, bundled.text));
			}
			else {
				output.push_str(&format!("{}\n{}\n\n{}\n\n", bundled.heading, "-".repeat(bundled.heading.chars().count()), bundled.text));
			}
		}
	}

	// 크레이트.
	if markdown {
		output.push_str("## Crates\n\n");
	}
	else {
		output.push_str("CRATES\n======\n\n");
	}
	for (entry, numbers) in crate_list.iter().zip(crate_text_numbers.iter()) {
		if !entry.attribution_required {
			continue;
		}
		let used = describe_chosen(&entry.chosen);
		let license_line = if used == entry.expression_text {
			code(&entry.expression_text)
		}
		else {
			format!("{}, used under {}", code(&entry.expression_text), code(&used))
		};
		let mut field_list: Vec<String> = Vec::new();
		field_list.push(format!("License: {license_line}"));
		let source = match &entry.repository {
			Some(repository) => {
				format!("{}, {}", entry.source, if markdown { format!("<{repository}>") } else { repository.clone() })
			}
			None => {
				entry.source.clone()
			}
		};
		field_list.push(format!("Source: {source}"));
		if !entry.copyright_lines.is_empty() {
			for line in &entry.copyright_lines {
				field_list.push(format!("Copyright: {}", if markdown { markdown_inline(line) } else { line.clone() }));
			}
		}
		else if !entry.authors.is_empty() {
			field_list.push(format!("Authors (Cargo.toml): {}", entry.authors.join(", ")));
		}
		for note in &entry.notes {
			field_list.push(format!("Note: {note}"));
		}
		if !numbers.is_empty() {
			let reference_list: Vec<String> = numbers.iter().map(|number| {
				let title = &text_list[number - 1].0.title;
				if markdown {
					format!("[T{number} {}](#t{number})", markdown_inline(title))
				}
				else {
					format!("T{number} ({title})")
				}
			}).collect();
			field_list.push(format!("License text: {}", reference_list.join(", ")));
		}
		if markdown {
			output.push_str(&format!("### {} {}\n\n", entry.name, entry.version));
			for field in field_list {
				output.push_str(&format!("- {field}\n"));
			}
			output.push('\n');
		}
		else {
			output.push_str(&format!("{} {}\n", entry.name, entry.version));
			for field in field_list {
				output.push_str(&format!("  {field}\n"));
			}
			output.push('\n');
		}
	}

	// 표기 의무가 없는 크레이트. (한 줄씩)
	if attributed_count < crate_list.len() {
		if markdown {
			output.push_str("## Crates Requiring No Notice\n\n");
		}
		else {
			output.push_str("CRATES REQUIRING NO NOTICE\n==========================\n\n");
		}
		output.push_str("These crates are used under a public domain dedication or a license without a notice condition. They are listed for reference only.\n\n");
		for entry in crate_list.iter().filter(|entry| !entry.attribution_required) {
			let used = describe_chosen(&entry.chosen);
			let license_line = if used == entry.expression_text {
				code(&entry.expression_text)
			}
			else {
				format!("{}, used under {}", code(&entry.expression_text), code(&used))
			};
			output.push_str(&format!("{bullet}{} {}: {license_line}\n", entry.name, entry.version));
		}
		output.push('\n');
	}

	// 전문.
	if markdown {
		output.push_str("## License Texts\n");
	}
	else {
		output.push_str("LICENSE TEXTS\n=============\n");
	}
	for (index, (notice, user_list)) in text_list.iter().enumerate() {
		let number = index + 1;
		if markdown {
			output.push_str(&format!("\n<a id=\"t{number}\"></a>\n\n### T{number}. {}\n\nUsed by: {}\n\n````text\n{}\n````\n", markdown_inline(&notice.title), user_list.join(", "), notice.text));
		}
		else {
			output.push_str(&format!("\n{TEXT_RULE}\nT{number}. {}\nUsed by: {}\n{TEXT_RULE}\n\n{}\n", notice.title, user_list.join(", "), notice.text));
		}
	}
	output
}

//==============================================================================
/// 고지 파일을 짓거나(--check 면) 견준다.
//==============================================================================
fn run(arguments: &Arguments) -> Result<bool, String> {
	let mut package_table: BTreeMap<String, Value> = BTreeMap::new();
	let mut reachable: HashSet<String> = HashSet::new();
	let mut workspace_members: HashSet<String> = HashSet::new();
	let mut workspace_root = PathBuf::new();
	for target in TARGETS {
		let metadata = read_metadata(arguments, target)?;
		workspace_root = PathBuf::from(metadata["workspace_root"].as_str().unwrap_or("."));
		for member in metadata["workspace_members"].as_array().into_iter().flatten() {
			if let Some(identifier) = member.as_str() {
				workspace_members.insert(identifier.to_string());
			}
		}
		collect_reachable(&metadata, &mut reachable);
		for package in metadata["packages"].as_array().into_iter().flatten() {
			if let Some(identifier) = package["id"].as_str() {
				package_table.entry(identifier.to_string()).or_insert_with(|| package.clone());
			}
		}
	}
	let output_path = arguments.output_path.clone().unwrap_or_else(|| workspace_root.join(OUTPUT_FILE_NAME));
	let markdown = output_path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("md"));

	// 크레이트 고지. (이름과 판 차례)
	let mut warnings: Vec<String> = Vec::new();
	let mut package_list: Vec<&Value> = reachable.iter().filter(|identifier| !workspace_members.contains(*identifier)).filter_map(|identifier| package_table.get(identifier)).collect();
	package_list.sort_by(|left, right| {
		let left_name = left["name"].as_str().unwrap_or_default();
		let right_name = right["name"].as_str().unwrap_or_default();
		left_name.cmp(right_name).then_with(|| compare_versions(left["version"].as_str().unwrap_or_default(), right["version"].as_str().unwrap_or_default())).then_with(|| left["id"].as_str().unwrap_or_default().cmp(right["id"].as_str().unwrap_or_default()))
	});
	let crate_list: Vec<CrateEntry> = package_list.iter().map(|package| build_crate_entry(package, &mut warnings)).collect();

	// 실린 자료. (이 저장소의 손으로 쓴 절은 짓는 파일이나 워크스페이스 뿌리의 THIRD_PARTY_LICENSES.md 에서, 의존 패키지의 절은 그 패키지 폴더에서)
	let mut own_source_list: Vec<PathBuf> = Vec::new();
	if markdown {
		own_source_list.push(output_path.clone());
	}
	own_source_list.push(workspace_root.join(OUTPUT_FILE_NAME));
	let own_section = own_source_list.iter().find_map(|path| read_bundled_section(path));
	let mut bundled_list: Vec<BundledSection> = Vec::new();
	for package in &package_list {
		let directory = Path::new(package["manifest_path"].as_str().unwrap_or_default()).parent().map(Path::to_path_buf).unwrap_or_default();
		if let Some(text) = read_bundled_section(&directory.join(OUTPUT_FILE_NAME)) {
			bundled_list.push(BundledSection { heading: format!("Materials bundled in {} {}", package["name"].as_str().unwrap_or_default(), package["version"].as_str().unwrap_or_default()), text });
		}
	}

	let text = render(markdown, own_section.as_deref(), &bundled_list, &crate_list);
	for warning in &warnings {
		eprintln!("알림: {warning}");
	}
	let attributed_count = crate_list.iter().filter(|entry| entry.attribution_required).count();
	let file_name = output_path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
	if arguments.check {
		let current = fs::read_to_string(&output_path).unwrap_or_default().replace("\r\n", "\n");
		if current == text {
			println!("{file_name}: 지금 의존과 같다. (크레이트 {}, 고지 {attributed_count}, 실린 자료 절 {})", crate_list.len(), usize::from(own_section.is_some()) + bundled_list.len());
			return Ok(true);
		}
		eprintln!("{file_name}: 지금 의존과 다르다. cargo run -p vanilla-engine-licenses 로 다시 짓는다.");
		return Ok(false);
	}
	if let Some(parent) = output_path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
		fs::create_dir_all(parent).map_err(|error| format!("{file_name} 의 폴더를 짓지 못했다: {error}"))?;
	}
	fs::write(&output_path, &text).map_err(|error| format!("{file_name} 을 쓰지 못했다: {error}"))?;
	println!("{file_name}: 크레이트 {} (고지 {attributed_count}, 표기 의무 없음 {}), 실린 자료 절 {}, {} 바이트", crate_list.len(), crate_list.len() - attributed_count, usize::from(own_section.is_some()) + bundled_list.len(), text.len());
	Ok(true)
}

//==============================================================================
/// 진입점.
//==============================================================================
fn main() -> ExitCode {
	let arguments = match parse_arguments() {
		Ok(arguments) => {
			arguments
		}
		Err(message) => {
			eprintln!("{message}");
			return ExitCode::from(2);
		}
	};
	match run(&arguments) {
		Ok(true) => {
			ExitCode::SUCCESS
		}
		Ok(false) => {
			ExitCode::from(1)
		}
		Err(message) => {
			eprintln!("{message}");
			ExitCode::from(2)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 식 하나에서 고른 라이선스 이름들.
	//==============================================================================
	fn chosen_names(text: &str, available: &[&str]) -> String {
		let available: Vec<String> = available.iter().map(|family| family.to_string()).collect();
		describe_chosen(&choose_licenses(&parse_expression(text).unwrap(), &available))
	}

	//==============================================================================
	/// OR 는 전문이 있는 것 가운데 표기 의무가 가벼운 것을, AND 는 모두 고른다.
	//==============================================================================
	#[test]
	fn chooses_licenses() {
		assert_eq!(chosen_names("MIT OR Apache-2.0", &["MIT", "Apache-2.0"]), "MIT");
		assert_eq!(chosen_names("Apache-2.0/MIT", &["MIT", "Apache-2.0"]), "MIT");
		assert_eq!(chosen_names("MIT OR Apache-2.0", &["Apache-2.0"]), "Apache-2.0");
		assert_eq!(chosen_names("Unlicense OR MIT", &["Unlicense", "MIT"]), "Unlicense");
		assert_eq!(chosen_names("0BSD OR MIT OR Apache-2.0", &[]), "0BSD");
		assert_eq!(chosen_names("(MIT OR Apache-2.0) AND Unicode-3.0", &["MIT", "Apache-2.0", "Unicode"]), "MIT AND Unicode-3.0");
		assert_eq!(chosen_names("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT", &["MIT", "Apache-2.0"]), "MIT");
		assert_eq!(chosen_names("Apache-2.0 OR GPL-2.0-only", &["Apache-2.0", "GPL"]), "Apache-2.0");
		assert_eq!(chosen_names("Apache-2.0 AND ISC", &[]), "Apache-2.0 AND ISC");
		assert!(parse_expression("MIT OR").is_none());
		assert!(parse_expression("(MIT").is_none());
	}

	//==============================================================================
	/// 저작권 줄은 소유자가 적힌 줄만 읽고 틀의 빈칸과 본문 낱말은 뺀다.
	//==============================================================================
	#[test]
	fn finds_copyright_lines() {
		let text = "Copyright (c) 2016 The Rust Project Developers\n   Copyright [yyyy] [name of copyright owner]\ncopyright notice that is included\nCopyright © 2015, Simonas Kazlauskas\n2. Grant of Copyright License.\nCopyright The Servo Project Developers\n(c) You must retain, in the Source form\nCOPYRIGHT AND PERMISSION NOTICE\n// Copyright 2015-2016 Brian Smith.\n  Copyright © 1993,2004 Sun Microsystems or";
		assert_eq!(find_copyright_lines(text), vec!["Copyright (c) 2016 The Rust Project Developers", "Copyright © 2015, Simonas Kazlauskas", "Copyright The Servo Project Developers", "Copyright 2015-2016 Brian Smith.", "Copyright © 1993,2004 Sun Microsystems"]);
	}

	//==============================================================================
	/// 파일 갈래는 이름에 적혔으면 이름으로, 아니면 표준 문구로 읽는다.
	//==============================================================================
	#[test]
	fn detects_families() {
		assert_eq!(detect_families("LICENSE-APACHE", ""), vec!["Apache-2.0"]);
		assert_eq!(detect_families("LICENSE-MIT.md", ""), vec!["MIT"]);
		assert_eq!(detect_families("LICENSE", "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software without restriction"), vec!["MIT"]);
		assert_eq!(detect_families("LICENSE", "This software is provided 'as-is', without any express or implied warranty."), vec!["Zlib"]);
		assert_eq!(detect_families("LICENSE", "Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice"), vec!["ISC"]);
		assert_eq!(detect_families("LICENSE.txt", "// Permission to use, copy, modify, and/or distribute this software for any\n// purpose with or without fee is hereby granted, provided that the above\n// copyright notice"), vec!["ISC"]);
		let mozilla = format!("Mozilla Public License Version 2.0\n{}\nmeans either the GNU General Public License, Version 2.0, the GNU Lesser General Public License, Version 2.1", "1. Definitions ".repeat(20));
		assert_eq!(detect_families("LICENSE", &mozilla), vec!["MPL-2.0"]);
		assert!(detect_families("COPYING", "This project is dual-licensed under the Unlicense and MIT licenses.").is_empty());
	}
}

//==============================================================================
//! 가이드 doctest 목록 짓기.
//! - docs/guide 아래 가장 높은 판의 ko 마크다운마다 `#[doc = include_str!(...)]` 를 붙인 빈 구조체를 OUT_DIR/guide.rs 에 쓴다. (src/lib.rs 가 cfg(doctest) 로 읽는다)
//! - 새 장을 더하면 다음 빌드에서 저절로 검사에 든다.
//==============================================================================
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

//==============================================================================
/// 판 글을 비교할 수의 묶음으로. ("0.4.0" → [0, 4, 0], 수가 아닌 조각은 0)
//==============================================================================
fn read_version_parts(version: &str) -> Vec<u64> {
	version.split('.').map(|part| part.parse::<u64>().unwrap_or(0)).collect()
}

//==============================================================================
/// 파일 이름을 구조체 이름으로. ("08-drawing" → "Guide08Drawing")
//==============================================================================
fn create_struct_name(file_stem: &str) -> String {
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
/// 빌드 스크립트 진입점.
//==============================================================================
fn main() {
	let manifest_directory = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
	let guide_directory = manifest_directory.join("../../docs/guide");
	println!("cargo:rerun-if-changed={}", guide_directory.display());
	let mut source = String::new();
	let mut newest: Option<(Vec<u64>, PathBuf)> = None;
	if let Ok(entries) = fs::read_dir(&guide_directory) {
		for entry in entries.flatten() {
			let path = entry.path();
			if !path.join("ko").is_dir() {
				continue;
			}
			let version_parts = read_version_parts(&entry.file_name().to_string_lossy());
			let is_newer = match &newest {
				Some((newest_parts, _)) => version_parts > *newest_parts,
				None => true,
			};
			if is_newer {
				newest = Some((version_parts, path));
			}
		}
	}
	if let Some((_, version_directory)) = newest {
		let korean_directory = version_directory.join("ko");
		let mut file_paths: Vec<PathBuf> = fs::read_dir(&korean_directory)
			.map(|entries| entries.flatten().map(|entry| entry.path()).filter(|path| path.extension().map(|extension| extension == "md").unwrap_or(false)).collect())
			.unwrap_or_default();
		file_paths.sort();
		for file_path in file_paths {
			let canonical_path = fs::canonicalize(&file_path).unwrap_or(file_path.clone());
			let file_stem = file_path.file_stem().unwrap().to_string_lossy().to_string();
			source.push_str(&format!("#[doc = include_str!({:?})]\npub struct {};\n", canonical_path.display().to_string(), create_struct_name(&file_stem)));
		}
	}
	let output_path = Path::new(&env::var("OUT_DIR").unwrap()).join("guide.rs");
	fs::write(output_path, source).unwrap();
}

//==============================================================================
//! 저장 글의 짝 없는 서로게이트 보존. (러스트 판에만 있다, LocalStorage 와 PersistedStore 가 쓴다)
//! - 원본 localStorage 와 JSON 은 JS 글(UTF-16)이라 짝 없는 서로게이트(짝을 잃은 U+D800 ~ U+DFFF)를 담는다.
//!   잘린 이모지처럼 사용자 입력에서 생기고, JSON.stringify 는 그것을 \ud83d 같은 이스케이프로 쓴다. (ES2019 의 잘 짜인 JSON.stringify)
//! - 러스트 String 은 UTF-8 이라 그런 글을 담지 못한다. 그래서 러스트 쪽에는 대체 문자(U+FFFD)로 보이고,
//!   고치지 않은 글을 다시 쓸 때는 원래 UTF-16 단위 그대로 쓴다. 원본 판이 저장한 값을 러스트 판이 읽고 다시 써도 잃지 않는다.
//! - PreservedText: 원래 UTF-16 단위와 대체 문자로 보인 글.
//! - parse_json, parse_json_code_units: JSON.parse 처럼 읽되 짝 없는 서로게이트(이스케이프와 날 단위 모두)를 대체 문자로 읽고,
//!   그런 글이 있던 자리(JSON 경로)와 원래 단위를 JsonSurrogateTable 에 남긴다. 수는 serde_json 이 읽은 것과 같다.
//! - stringify_json: serde_json 의 짧은 꼴(Value::to_string)과 같은 글을 쓰되, 표의 자리에 대체 문자로 보인 글이 그대로 있으면 원래 단위를,
//!   다른 자리라도 같은 글이면(원본에서 글을 옮겨 담은 것과 같다) 원래 단위를 JSON.stringify 처럼 \udXXX 로 쓴다.
//!   대체 문자로 보이는 글이 같은데 원래 단위가 서로 다른 글이 둘 이상이면 자리가 맞는 것만 원래 단위로 쓴다.
//==============================================================================
use serde_json::{Map, Number, Value};
use std::collections::HashMap;

//==============================================================================
/// 짝 없는 서로게이트가 있는지.
///
/// # 인자
/// - `code_units`: 볼 UTF-16 단위.
///
/// # 돌려줌
/// 짝 없는 서로게이트가 하나라도 있으면 true.
//==============================================================================
pub fn has_lone_surrogate(code_units: &[u16]) -> bool {
	char::decode_utf16(code_units.iter().copied()).any(|decode_result| decode_result.is_err())
}

//==============================================================================
/// 원래 UTF-16 단위와 대체 문자로 보인 글.
///
/// # 예제
/// 잘린 이모지의 앞 반쪽을 든 글
/// ```rust
/// use vanilla::misc::storagetext::PreservedText;
///
/// let preserved_text = PreservedText::from_code_units(&[0x61, 0xD83D]);
/// assert_eq!(preserved_text.get_lossy_text(), "a\u{fffd}");
/// assert_eq!(preserved_text.get_code_units(), &[0x61, 0xD83D]);
/// assert!(preserved_text.has_lone_surrogate());
/// assert!(!PreservedText::from_code_units(&[0xD83D, 0xDE00]).has_lone_surrogate());
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreservedText {
	lossy_text: String,
	code_units: Vec<u16>,
}

impl PreservedText {
	//==============================================================================
	/// UTF-16 단위로 만들기. (짝 없는 서로게이트는 대체 문자로 보인다)
	///
	/// # 인자
	/// - `code_units`: 원래 UTF-16 단위.
	///
	/// # 돌려줌
	/// 그 단위와 대체 문자로 보인 글을 든 PreservedText.
	//==============================================================================
	pub fn from_code_units(code_units: &[u16]) -> PreservedText {
		PreservedText {
			lossy_text: String::from_utf16_lossy(code_units),
			code_units: code_units.to_vec(),
		}
	}

	//==============================================================================
	/// 대체 문자로 보인 글.
	///
	/// # 돌려줌
	/// 짝 없는 서로게이트를 대체 문자(U+FFFD)로 바꾼 러스트 글.
	//==============================================================================
	pub fn get_lossy_text(&self) -> &str {
		&self.lossy_text
	}

	//==============================================================================
	/// 원래 UTF-16 단위.
	///
	/// # 돌려줌
	/// 만들 때 받은 원래 UTF-16 단위.
	//==============================================================================
	pub fn get_code_units(&self) -> &[u16] {
		&self.code_units
	}

	//==============================================================================
	/// 짝 없는 서로게이트가 있는지. (없으면 러스트 글과 같다)
	///
	/// # 돌려줌
	/// 짝 없는 서로게이트가 있으면 true.
	//==============================================================================
	pub fn has_lone_surrogate(&self) -> bool {
		has_lone_surrogate(&self.code_units)
	}
}

//==============================================================================
/// JSON.stringify 의 글 따옴표. (QuoteJSONString: " 와 \ 와 \b \f \n \r \t, 그 밖의 U+0000 ~ U+001F 와 짝 없는 서로게이트는 \u 소문자 넷 자리)
///
/// # 인자
/// - `code_units`: 따옴표로 감쌀 글의 UTF-16 단위.
///
/// # 돌려줌
/// JSON.stringify 가 쓰는 꼴의 JSON 글. (앞뒤 따옴표까지)
//==============================================================================
pub fn quote_json_code_units(code_units: &[u16]) -> String {
	let mut quoted_text = String::with_capacity(code_units.len() + 2);
	quoted_text.push('"');
	for decode_result in char::decode_utf16(code_units.iter().copied()) {
		match decode_result {
			Ok(character) => {
				match character {
					'"' => {
						quoted_text.push_str("\\\"");
					}
					'\\' => {
						quoted_text.push_str("\\\\");
					}
					'\u{8}' => {
						quoted_text.push_str("\\b");
					}
					'\u{c}' => {
						quoted_text.push_str("\\f");
					}
					'\n' => {
						quoted_text.push_str("\\n");
					}
					'\r' => {
						quoted_text.push_str("\\r");
					}
					'\t' => {
						quoted_text.push_str("\\t");
					}
					_ if (character as u32) < 0x20 => {
						quoted_text.push_str(&format!("\\u{:04x}", character as u32));
					}
					_ => {
						quoted_text.push(character);
					}
				}
			}
			Err(decode_error) => {
				quoted_text.push_str(&format!("\\u{:04x}", decode_error.unpaired_surrogate()));
			}
		}
	}
	quoted_text.push('"');
	quoted_text
}

//==============================================================================
/// JSON 경로 한 마디. (객체 키나 배열 차례)
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum JsonPathSegment {
	/// 객체 키.
	Key(String),
	/// 배열 차례. (0 부터)
	Index(usize),
}

//==============================================================================
/// 짝 없는 서로게이트가 든 글의 표. (parse_json 이 채우고 stringify_json 이 쓴다)
///
/// # 예제
/// 원본이 쓴 `\ud83d` 를 읽고 그대로 다시 쓰기
/// ```rust
/// use vanilla::misc::storagetext::{self, JsonPathSegment};
///
/// let stored_text = r#"{"name":"a\ud83d"}"#;
/// let (value, surrogate_table) = storagetext::parse_json(stored_text).unwrap();
/// assert_eq!(value["name"], "a\u{fffd}");
/// assert!(!surrogate_table.is_empty());
/// let name_path = [JsonPathSegment::Key(String::from("name"))];
/// assert_eq!(surrogate_table.find_value_code_units(&name_path, "a\u{fffd}"), Some(&[0x61u16, 0xD83D][..]));
/// assert_eq!(storagetext::stringify_json(&value, &surrogate_table), stored_text);
/// ```
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct JsonSurrogateTable {
	/// 자리 → 원래 글. (대체 문자가 든 값 글)
	path_map: HashMap<Vec<JsonPathSegment>, PreservedText>,
	/// 대체 문자로 보인 글 → 원래 단위. (대체 문자가 든 값과 키. 처음부터 대체 문자였던 글도 넣어,
	/// 원래 단위가 서로 다른 글이 같은 글로 보이면 None 으로 두고 자리로만 찾는다)
	text_map: HashMap<String, Option<Vec<u16>>>,
	/// 짝 없는 서로게이트가 든 글이 있었는지.
	has_lone_surrogate_text: bool,
}

impl JsonSurrogateTable {
	//==============================================================================
	/// 빈 표.
	///
	/// # 돌려줌
	/// 빈 표.
	//==============================================================================
	pub fn new() -> JsonSurrogateTable {
		JsonSurrogateTable::default()
	}

	//==============================================================================
	/// 비었는지. (짝 없는 서로게이트가 든 글이 없었다)
	///
	/// # 돌려줌
	/// 짝 없는 서로게이트가 든 글이 없었으면 true.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		!self.has_lone_surrogate_text
	}

	//==============================================================================
	/// 글 하나 남기기. (대체 문자가 든 글만, 자리가 있으면 값 글이고 없으면 키)
	//==============================================================================
	fn record(&mut self, path: Option<&[JsonPathSegment]>, preserved_text: PreservedText) {
		if !preserved_text.get_lossy_text().contains('\u{fffd}') {
			return;
		}
		if preserved_text.has_lone_surrogate() {
			self.has_lone_surrogate_text = true;
		}
		match self.text_map.get_mut(preserved_text.get_lossy_text()) {
			Some(code_units_entry) => {
				let is_same = code_units_entry.as_deref() == Some(preserved_text.get_code_units());
				if !is_same {
					*code_units_entry = None;
				}
			}
			None => {
				self.text_map.insert(preserved_text.get_lossy_text().to_string(), Some(preserved_text.get_code_units().to_vec()));
			}
		}
		if let Some(path) = path {
			self.path_map.insert(path.to_vec(), preserved_text);
		}
	}

	//==============================================================================
	/// 그 자리 값 글의 원래 단위. (자리가 맞고 글이 그대로면 그것, 아니면 같은 글로 보이는 원래 글이 하나뿐일 때 그것)
	///
	/// # 인자
	/// - `path`: 값의 자리. (맨 위 값에서부터의 JSON 경로)
	/// - `text`: 지금 그 자리에 든 글.
	///
	/// # 돌려줌
	/// 원래 UTF-16 단위. (자리가 맞고 글이 그대로면 그 자리의 것, 아니면 find_text_code_units 와 같다) 찾지 못하면 None.
	//==============================================================================
	pub fn find_value_code_units(&self, path: &[JsonPathSegment], text: &str) -> Option<&[u16]> {
		if let Some(preserved_text) = self.path_map.get(path) {
			if preserved_text.get_lossy_text() == text {
				return Some(preserved_text.get_code_units());
			}
		}
		self.find_text_code_units(text)
	}

	//==============================================================================
	/// 같은 글로 보이는 원래 글의 단위. (하나뿐일 때만)
	///
	/// # 인자
	/// - `text`: 대체 문자로 보인 글.
	///
	/// # 돌려줌
	/// 원래 UTF-16 단위. 그렇게 보이는 원래 글이 없거나, 원래 단위가 서로 다른 글이 둘 이상이면 None.
	//==============================================================================
	pub fn find_text_code_units(&self, text: &str) -> Option<&[u16]> {
		match self.text_map.get(text) {
			Some(Some(code_units)) => {
				Some(code_units)
			}
			_ => {
				None
			}
		}
	}
}

//==============================================================================
/// JSON 읽기. (JSON.parse 처럼, 짝 없는 서로게이트는 대체 문자로 읽고 표에 남긴다)
///
/// # 인자
/// - `text`: 읽을 JSON 글.
///
/// # 돌려줌
/// 값과 그 짝 없는 서로게이트 표. 틀린 JSON 이면 Err(까닭과 그 자리).
//==============================================================================
pub fn parse_json(text: &str) -> Result<(Value, JsonSurrogateTable), String> {
	let code_units: Vec<u16> = text.encode_utf16().collect();
	parse_json_code_units(&code_units)
}

//==============================================================================
/// UTF-16 단위의 JSON 읽기. (원본 JS 글 그대로, 날 짝 없는 서로게이트도 받는다)
///
/// # 인자
/// - `code_units`: 읽을 JSON 글의 UTF-16 단위. (날 짝 없는 서로게이트도 받는다)
///
/// # 돌려줌
/// 값과 그 짝 없는 서로게이트 표. 틀린 JSON 이면 Err(까닭과 그 자리).
//==============================================================================
pub fn parse_json_code_units(code_units: &[u16]) -> Result<(Value, JsonSurrogateTable), String> {
	let mut json_reader = JsonReader {
		code_units,
		position: 0,
		path: Vec::new(),
		surrogate_table: JsonSurrogateTable::new(),
	};
	json_reader.skip_whitespace();
	let value = json_reader.read_value()?;
	json_reader.skip_whitespace();
	if json_reader.position != code_units.len() {
		return Err(format!("JSON 끝 뒤에 남은 글 ({})", json_reader.position));
	}
	Ok((value, json_reader.surrogate_table))
}

//==============================================================================
/// JSON 객체의 칸 하나. (키는 원래 UTF-16 단위, 값의 표는 그 값을 바탕(빈 경로)으로 한 자리)
///
/// # 예제
/// 짝 없는 서로게이트로만 다른 두 키를 따로 읽기
/// ```rust
/// use vanilla::misc::storagetext;
///
/// let entry_list = storagetext::parse_json_object_entries(r#"{"k\ud800":"one","k\ud801":"two"}"#).unwrap();
/// assert_eq!(entry_list.len(), 2);
/// assert_eq!(entry_list[0].key.get_lossy_text(), "k\u{fffd}");
/// assert_eq!(entry_list[1].key.get_lossy_text(), "k\u{fffd}");
/// assert_eq!(entry_list[1].key.get_code_units(), &[0x6B, 0xD801]);
/// assert_eq!(entry_list[1].value, "two");
/// assert!(entry_list[1].surrogate_table.is_empty());
/// ```
//==============================================================================
pub struct JsonObjectEntry {
	/// 키. (원래 UTF-16 단위와 대체 문자로 보인 글)
	pub key: PreservedText,
	/// 값. (짝 없는 서로게이트는 대체 문자)
	pub value: Value,
	/// 값의 짝 없는 서로게이트 표. (값을 바탕(빈 경로)으로 한 자리)
	pub surrogate_table: JsonSurrogateTable,
}

//==============================================================================
/// 맨 위가 객체인 JSON 을 칸 목록으로 읽기. (맨 위가 객체가 아니면 빈 목록)
/// - 짝 없는 서로게이트로만 다른 두 키도 따로 담는다. (serde_json 맵에서는 대체 문자로 같은 키가 되어 하나만 남는다)
/// - 차례는 글에 적힌 차례고, 같은 키가 또 나와도 모두 담는다. (JSON.parse 처럼 뒤의 값이 이기게 하려면 담는 쪽이 덮어쓴다)
///
/// # 인자
/// - `text`: 읽을 JSON 글.
///
/// # 돌려줌
/// 칸 목록. (글에 적힌 차례, 맨 위가 객체가 아니면 빈 목록) 틀린 JSON 이면 Err(까닭과 그 자리).
//==============================================================================
pub fn parse_json_object_entries(text: &str) -> Result<Vec<JsonObjectEntry>, String> {
	let code_units: Vec<u16> = text.encode_utf16().collect();
	let mut json_reader = JsonReader {
		code_units: &code_units,
		position: 0,
		path: Vec::new(),
		surrogate_table: JsonSurrogateTable::new(),
	};
	json_reader.skip_whitespace();
	let entry_list = if json_reader.peek() == Some(0x7B) {
		json_reader.read_object_entries()?
	}
	else {
		json_reader.read_value()?;
		Vec::new()
	};
	json_reader.skip_whitespace();
	if json_reader.position != code_units.len() {
		return Err(format!("JSON 끝 뒤에 남은 글 ({})", json_reader.position));
	}
	Ok(entry_list)
}

//==============================================================================
/// JSON 쓰기. (serde_json 의 짧은 꼴과 같고, 표에 남은 글은 원래 단위로)
///
/// # 인자
/// - `value`: 쓸 값.
/// - `surrogate_table`: parse_json 이 남긴 표. (빈 표면 serde_json 의 짧은 꼴과 같다)
///
/// # 돌려줌
/// JSON 글.
//==============================================================================
pub fn stringify_json(value: &Value, surrogate_table: &JsonSurrogateTable) -> String {
	stringify_json_at(value, &[], surrogate_table)
}

//==============================================================================
/// JSON 쓰기. (값이 표의 base_path 자리에 있던 것일 때)
///
/// # 인자
/// - `value`: 쓸 값.
/// - `base_path`: 이 값이 표에서 있던 자리. (빈 경로면 stringify_json 과 같다)
/// - `surrogate_table`: parse_json 이 남긴 표.
///
/// # 돌려줌
/// JSON 글.
//==============================================================================
pub fn stringify_json_at(value: &Value, base_path: &[JsonPathSegment], surrogate_table: &JsonSurrogateTable) -> String {
	let mut json_text = String::new();
	let mut path = base_path.to_vec();
	write_json_value(&mut json_text, value, &mut path, surrogate_table);
	json_text
}

//==============================================================================
/// 값 하나 쓰기.
//==============================================================================
fn write_json_value(json_text: &mut String, value: &Value, path: &mut Vec<JsonPathSegment>, surrogate_table: &JsonSurrogateTable) {
	match value {
		Value::Null => {
			json_text.push_str("null");
		}
		Value::Bool(boolean_value) => {
			if *boolean_value {
				json_text.push_str("true");
			}
			else {
				json_text.push_str("false");
			}
		}
		Value::Number(number_value) => {
			json_text.push_str(&serde_json::to_string(number_value).unwrap_or_else(|_| String::from("null")));
		}
		Value::String(string_value) => {
			let code_units = if surrogate_table.is_empty() {
				None
			}
			else {
				surrogate_table.find_value_code_units(path, string_value)
			};
			write_json_string(json_text, string_value, code_units);
		}
		Value::Array(array_value) => {
			json_text.push('[');
			for (item_index, item_value) in array_value.iter().enumerate() {
				if item_index > 0 {
					json_text.push(',');
				}
				path.push(JsonPathSegment::Index(item_index));
				write_json_value(json_text, item_value, path, surrogate_table);
				path.pop();
			}
			json_text.push(']');
		}
		Value::Object(object_map) => {
			json_text.push('{');
			for (entry_index, (key, item_value)) in object_map.iter().enumerate() {
				if entry_index > 0 {
					json_text.push(',');
				}
				let key_code_units = if surrogate_table.is_empty() {
					None
				}
				else {
					surrogate_table.find_text_code_units(key)
				};
				write_json_string(json_text, key, key_code_units);
				json_text.push(':');
				path.push(JsonPathSegment::Key(key.clone()));
				write_json_value(json_text, item_value, path, surrogate_table);
				path.pop();
			}
			json_text.push('}');
		}
	}
}

//==============================================================================
/// 글 하나 쓰기. (원래 단위가 있으면 그것을 JSON.stringify 꼴로, 없으면 serde_json 꼴로. 짝 없는 서로게이트가 없으면 두 꼴이 같다)
//==============================================================================
fn write_json_string(json_text: &mut String, text: &str, code_units: Option<&[u16]>) {
	match code_units {
		Some(code_units) => {
			json_text.push_str(&quote_json_code_units(code_units));
		}
		None => {
			json_text.push_str(&serde_json::to_string(text).unwrap_or_else(|_| String::from("\"\"")));
		}
	}
}

//==============================================================================
/// UTF-16 단위의 JSON 읽개.
//==============================================================================
struct JsonReader<'a> {
	code_units: &'a [u16],
	position: usize,
	path: Vec<JsonPathSegment>,
	surrogate_table: JsonSurrogateTable,
}

impl JsonReader<'_> {
	//==============================================================================
	/// 지금 단위. (끝이면 None)
	//==============================================================================
	fn peek(&self) -> Option<u16> {
		self.code_units.get(self.position).copied()
	}

	//==============================================================================
	/// JSON 빈칸 건너뛰기. (빈칸, 탭, 줄바꿈, 캐리지 리턴. JSON.parse 와 같다)
	//==============================================================================
	fn skip_whitespace(&mut self) {
		while let Some(code_unit) = self.peek() {
			if code_unit == 0x20 || code_unit == 0x09 || code_unit == 0x0A || code_unit == 0x0D {
				self.position += 1;
			}
			else {
				break;
			}
		}
	}

	//==============================================================================
	/// 그 글자가 오는지 보고 넘긴다.
	//==============================================================================
	fn expect(&mut self, expected_character: char) -> Result<(), String> {
		if self.peek() == Some(expected_character as u16) {
			self.position += 1;
			return Ok(());
		}
		Err(format!("JSON 에 '{}' 가 와야 한다 ({})", expected_character, self.position))
	}

	//==============================================================================
	/// 낱말 하나. (true, false, null)
	//==============================================================================
	fn read_literal(&mut self, literal: &str, value: Value) -> Result<Value, String> {
		for literal_character in literal.chars() {
			self.expect(literal_character)?;
		}
		Ok(value)
	}

	//==============================================================================
	/// 값 하나.
	//==============================================================================
	fn read_value(&mut self) -> Result<Value, String> {
		match self.peek() {
			Some(0x7B) => {
				self.read_object()
			}
			Some(0x5B) => {
				self.read_array()
			}
			Some(0x22) => {
				let preserved_text = self.read_string()?;
				let text = preserved_text.get_lossy_text().to_string();
				let path = self.path.clone();
				self.surrogate_table.record(Some(&path), preserved_text);
				Ok(Value::String(text))
			}
			Some(0x74) => {
				self.read_literal("true", Value::Bool(true))
			}
			Some(0x66) => {
				self.read_literal("false", Value::Bool(false))
			}
			Some(0x6E) => {
				self.read_literal("null", Value::Null)
			}
			Some(code_unit) if code_unit == 0x2D || (0x30..=0x39).contains(&code_unit) => {
				self.read_number()
			}
			_ => {
				Err(format!("JSON 값이 와야 한다 ({})", self.position))
			}
		}
	}

	//==============================================================================
	/// 객체. (같은 키가 또 나오면 JSON.parse 처럼 뒤의 값이 이기고 자리는 처음 것)
	//==============================================================================
	fn read_object(&mut self) -> Result<Value, String> {
		self.expect('{')?;
		let mut object_map = Map::new();
		self.skip_whitespace();
		if self.peek() == Some(0x7D) {
			self.position += 1;
			return Ok(Value::Object(object_map));
		}
		loop {
			self.skip_whitespace();
			if self.peek() != Some(0x22) {
				return Err(format!("JSON 객체 키가 와야 한다 ({})", self.position));
			}
			let preserved_key = self.read_string()?;
			let key = preserved_key.get_lossy_text().to_string();
			self.surrogate_table.record(None, preserved_key);
			self.skip_whitespace();
			self.expect(':')?;
			self.skip_whitespace();
			self.path.push(JsonPathSegment::Key(key.clone()));
			let item_value = self.read_value()?;
			self.path.pop();
			object_map.insert(key, item_value);
			self.skip_whitespace();
			match self.peek() {
				Some(0x2C) => {
					self.position += 1;
				}
				Some(0x7D) => {
					self.position += 1;
					return Ok(Value::Object(object_map));
				}
				_ => {
					return Err(format!("JSON 객체에 ',' 나 '}}' 가 와야 한다 ({})", self.position));
				}
			}
		}
	}

	//==============================================================================
	/// 맨 위 객체의 칸 목록. (값마다 빈 경로와 새 표로 읽는다)
	//==============================================================================
	fn read_object_entries(&mut self) -> Result<Vec<JsonObjectEntry>, String> {
		self.expect('{')?;
		let mut entry_list: Vec<JsonObjectEntry> = Vec::new();
		self.skip_whitespace();
		if self.peek() == Some(0x7D) {
			self.position += 1;
			return Ok(entry_list);
		}
		loop {
			self.skip_whitespace();
			if self.peek() != Some(0x22) {
				return Err(format!("JSON 객체 키가 와야 한다 ({})", self.position));
			}
			let key = self.read_string()?;
			self.skip_whitespace();
			self.expect(':')?;
			self.skip_whitespace();
			self.path.clear();
			self.surrogate_table = JsonSurrogateTable::new();
			let value = self.read_value()?;
			let surrogate_table = std::mem::take(&mut self.surrogate_table);
			entry_list.push(JsonObjectEntry {
				key,
				value,
				surrogate_table,
			});
			self.skip_whitespace();
			match self.peek() {
				Some(0x2C) => {
					self.position += 1;
				}
				Some(0x7D) => {
					self.position += 1;
					return Ok(entry_list);
				}
				_ => {
					return Err(format!("JSON 객체에 ',' 나 '}}' 가 와야 한다 ({})", self.position));
				}
			}
		}
	}

	//==============================================================================
	/// 배열.
	//==============================================================================
	fn read_array(&mut self) -> Result<Value, String> {
		self.expect('[')?;
		let mut array_value: Vec<Value> = Vec::new();
		self.skip_whitespace();
		if self.peek() == Some(0x5D) {
			self.position += 1;
			return Ok(Value::Array(array_value));
		}
		loop {
			self.skip_whitespace();
			self.path.push(JsonPathSegment::Index(array_value.len()));
			let item_value = self.read_value()?;
			self.path.pop();
			array_value.push(item_value);
			self.skip_whitespace();
			match self.peek() {
				Some(0x2C) => {
					self.position += 1;
				}
				Some(0x5D) => {
					self.position += 1;
					return Ok(Value::Array(array_value));
				}
				_ => {
					return Err(format!("JSON 배열에 ',' 나 ']' 가 와야 한다 ({})", self.position));
				}
			}
		}
	}

	//==============================================================================
	/// 글. (따옴표 안, 날 제어 글자는 JSON.parse 처럼 틀린 것, \u 이스케이프는 단위 그대로)
	//==============================================================================
	fn read_string(&mut self) -> Result<PreservedText, String> {
		self.expect('"')?;
		let mut code_units: Vec<u16> = Vec::new();
		loop {
			let code_unit = match self.peek() {
				Some(code_unit) => {
					code_unit
				}
				None => {
					return Err(String::from("JSON 글이 닫히지 않았다"));
				}
			};
			self.position += 1;
			match code_unit {
				0x22 => {
					break;
				}
				0x5C => {
					let escape_unit = self.peek().ok_or_else(|| String::from("JSON 이스케이프가 끝났다"))?;
					self.position += 1;
					match escape_unit {
						0x22 | 0x5C | 0x2F => {
							code_units.push(escape_unit);
						}
						0x62 => {
							code_units.push(0x08);
						}
						0x66 => {
							code_units.push(0x0C);
						}
						0x6E => {
							code_units.push(0x0A);
						}
						0x72 => {
							code_units.push(0x0D);
						}
						0x74 => {
							code_units.push(0x09);
						}
						0x75 => {
							code_units.push(self.read_hex_code_unit()?);
						}
						_ => {
							return Err(format!("JSON 이스케이프가 틀렸다 ({})", self.position));
						}
					}
				}
				_ if code_unit < 0x20 => {
					return Err(format!("JSON 글에 날 제어 글자 ({})", self.position));
				}
				_ => {
					code_units.push(code_unit);
				}
			}
		}
		Ok(PreservedText::from_code_units(&code_units))
	}

	//==============================================================================
	/// \u 뒤 열여섯 진수 넷 자리.
	//==============================================================================
	fn read_hex_code_unit(&mut self) -> Result<u16, String> {
		let mut code_unit: u16 = 0;
		for _ in 0..4 {
			let hex_unit = self.peek().ok_or_else(|| String::from("JSON \\u 이스케이프가 끝났다"))?;
			let digit = char::from_u32(hex_unit as u32).and_then(|hex_character| hex_character.to_digit(16)).ok_or_else(|| format!("JSON \\u 이스케이프가 틀렸다 ({})", self.position))?;
			code_unit = code_unit * 16 + digit as u16;
			self.position += 1;
		}
		Ok(code_unit)
	}

	//==============================================================================
	/// 수. (JSON 수 문법으로 끊고 serde_json 이 읽는다. 앞의 0 과 끝의 점은 JSON.parse 처럼 틀린 것)
	//==============================================================================
	fn read_number(&mut self) -> Result<Value, String> {
		let start_position = self.position;
		if self.peek() == Some(0x2D) {
			self.position += 1;
		}
		match self.peek() {
			Some(0x30) => {
				self.position += 1;
			}
			Some(code_unit) if (0x31..=0x39).contains(&code_unit) => {
				self.skip_digits();
			}
			_ => {
				return Err(format!("JSON 수가 틀렸다 ({})", self.position));
			}
		}
		if self.peek() == Some(0x2E) {
			self.position += 1;
			if !self.skip_digits() {
				return Err(format!("JSON 수의 소수 자리가 없다 ({})", self.position));
			}
		}
		if self.peek() == Some(0x65) || self.peek() == Some(0x45) {
			self.position += 1;
			if self.peek() == Some(0x2B) || self.peek() == Some(0x2D) {
				self.position += 1;
			}
			if !self.skip_digits() {
				return Err(format!("JSON 수의 지수 자리가 없다 ({})", self.position));
			}
		}
		let number_text = String::from_utf16_lossy(&self.code_units[start_position..self.position]);
		let number_value: Number = serde_json::from_str(&number_text).map_err(|error| format!("JSON 수를 읽지 못했다 ({}): {}", number_text, error))?;
		Ok(Value::Number(number_value))
	}

	//==============================================================================
	/// 숫자 글자들을 넘긴다. (하나라도 넘겼는지)
	//==============================================================================
	fn skip_digits(&mut self) -> bool {
		let start_position = self.position;
		while let Some(code_unit) = self.peek() {
			if (0x30..=0x39).contains(&code_unit) {
				self.position += 1;
			}
			else {
				break;
			}
		}
		self.position > start_position
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 짝 없는 서로게이트가 없으면 serde_json 과 같은 값과 같은 글이다.
	//==============================================================================
	#[test]
	fn matches_serde_json_without_lone_surrogate() {
		let json_text = "{ \"name\" : \"가나 😀 \\\"q\\\" \\n\\t\\u0001/\\/\", \"list\": [1, -2.5, 3e2, 0.1, true, false, null, {\"a\": []}], \"empty\": {}, \"big\": 12345678901234567890 }";
		let (value, surrogate_table) = parse_json(json_text).expect("parse");
		let serde_value: Value = serde_json::from_str(json_text).expect("serde parse");
		assert_eq!(value, serde_value);
		assert!(surrogate_table.is_empty());
		assert_eq!(stringify_json(&value, &surrogate_table), serde_value.to_string());
		for broken_text in ["", "{", "[1,]", "{\"a\":1,}", "01", "1.", ".5", "\"a\nb\"", "\"\\x\"", "nul", "[1] 2", "\u{feff}{}", "+1"] {
			assert!(parse_json(broken_text).is_err(), "{:?} 는 틀린 JSON", broken_text);
			assert!(serde_json::from_str::<Value>(broken_text).is_err(), "{:?} 는 serde 도 틀린 JSON", broken_text);
		}
	}

	//==============================================================================
	/// 이스케이프된 짝 없는 서로게이트: 대체 문자로 보이고, 고치지 않고 쓰면 원본 JSON.stringify 와 같은 글이 된다.
	//==============================================================================
	#[test]
	fn keeps_escaped_lone_surrogate() {
		// 원본 JSON.stringify({ version: 1, state: { name: "a\ud83d", list: ["\ude00b", "ok"] } }) 의 글.
		let stored_text = "{\"version\":1,\"state\":{\"name\":\"a\\ud83d\",\"list\":[\"\\ude00b\",\"ok\"]}}";
		assert!(serde_json::from_str::<Value>(stored_text).is_err());
		let (value, surrogate_table) = parse_json(stored_text).expect("parse");
		assert_eq!(value["state"]["name"], Value::from("a\u{fffd}"));
		assert_eq!(value["state"]["list"][0], Value::from("\u{fffd}b"));
		assert_eq!(stringify_json(&value, &surrogate_table), stored_text);

		// 다른 칸을 고쳐도 남은 글은 그대로, 고친 글은 고친 대로.
		let mut changed_value = value.clone();
		changed_value["state"]["list"][1] = Value::from("new");
		changed_value["state"]["copy"] = changed_value["state"]["name"].clone();
		assert_eq!(stringify_json(&changed_value, &surrogate_table), "{\"version\":1,\"state\":{\"name\":\"a\\ud83d\",\"list\":[\"\\ude00b\",\"new\"],\"copy\":\"a\\ud83d\"}}");
		changed_value["state"]["name"] = Value::from("a\u{fffd}!");
		assert!(stringify_json(&changed_value, &surrogate_table).contains("\"name\":\"a\u{fffd}!\""));
	}

	//==============================================================================
	/// 날 짝 없는 서로게이트와 키, 같은 글로 보이는 서로 다른 원래 글.
	//==============================================================================
	#[test]
	fn keeps_raw_lone_surrogate_and_ambiguous_text() {
		let mut code_units: Vec<u16> = "{\"k".encode_utf16().collect();
		code_units.push(0xD800);
		code_units.extend("\":\"x".encode_utf16());
		code_units.push(0xDC01);
		code_units.extend("\",\"other\":\"\\udbff\"}".encode_utf16());
		let (value, surrogate_table) = parse_json_code_units(&code_units).expect("parse");
		assert_eq!(value["k\u{fffd}"], Value::from("x\u{fffd}"));
		assert_eq!(stringify_json(&value, &surrogate_table), "{\"k\\ud800\":\"x\\udc01\",\"other\":\"\\udbff\"}");

		let (ambiguous_value, ambiguous_table) = parse_json("[\"\\ud800\",\"\\udc00\"]").expect("parse");
		assert_eq!(stringify_json(&ambiguous_value, &ambiguous_table), "[\"\\ud800\",\"\\udc00\"]");
		let swapped_value = Value::Array(vec![Value::from("\u{fffd}"), Value::from("\u{fffd}"), Value::from("\u{fffd}")]);
		assert_eq!(stringify_json(&swapped_value, &ambiguous_table), "[\"\\ud800\",\"\\udc00\",\"\u{fffd}\"]");

		// 처음부터 대체 문자였던 글은 짝 없는 서로게이트로 바뀌지 않는다.
		let (genuine_value, genuine_table) = parse_json("{\"a\":\"\\ud800\",\"b\":\"\u{fffd}\"}").expect("parse");
		let mut moved_value = genuine_value.clone();
		moved_value["c"] = Value::from("\u{fffd}");
		assert_eq!(stringify_json(&moved_value, &genuine_table), "{\"a\":\"\\ud800\",\"b\":\"\u{fffd}\",\"c\":\"\u{fffd}\"}");
		let (only_genuine_value, only_genuine_table) = parse_json("[\"\u{fffd}\"]").expect("parse");
		assert!(only_genuine_table.is_empty());
		assert_eq!(stringify_json(&only_genuine_value, &only_genuine_table), "[\"\u{fffd}\"]");
	}

	//==============================================================================
	/// JSON.stringify 의 글 따옴표.
	//==============================================================================
	#[test]
	fn quotes_like_json_stringify() {
		let mut code_units: Vec<u16> = "\"\\/\u{8}\u{c}\n\r\t\u{1f}가😀".encode_utf16().collect();
		code_units.push(0xDFFF);
		assert_eq!(quote_json_code_units(&code_units), "\"\\\"\\\\/\\b\\f\\n\\r\\t\\u001f가😀\\udfff\"");
		assert!(has_lone_surrogate(&[0xD83D]));
		assert!(!has_lone_surrogate(&[0xD83D, 0xDE00]));
		let preserved_text = PreservedText::from_code_units(&[0x61, 0xDE00]);
		assert_eq!(preserved_text.get_lossy_text(), "a\u{fffd}");
		assert!(preserved_text.has_lone_surrogate());
	}
}

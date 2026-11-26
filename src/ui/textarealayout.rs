//==============================================================================
//! 여러 줄 입력 필드의 줄 배치. (크롬 textarea 의 white-space: pre-wrap 과 overflow-wrap: break-word 셈)
//! - 줄바꿈 문자로 문단을 나누고, 문단은 폭을 넘지 않게 줄 나눔 자리에서 나눈다. 줄 나눔 자리는 크롬이 쓰는 ICU 처럼
//!   유니코드 줄 나누기(UAX #14)에 블링크의 빠른 표(아스키, 라틴 1)를 더한 것이다. (ICU4X LineSegmenter 의 사전판, 데스크탑과 웹 캔버스 모드가 같다)
//! - pre-wrap: 공백은 그대로 두고, 줄 끝 공백은 폭을 넘어도 그 줄에 매달린다. (폭을 잴 때 빼고, 줄에는 남는다)
//! - break-word: 나눔 자리 없이 한 조각이 폭을 넘으면 글자 묶음 사이에서 나눈다.
//! - 탭은 다음 탭 자리(탭 크기 x 공백 폭의 배수)까지 민다. (CSS tab-size, 기본 8)
//! - 자리는 모두 문자(char) 단위다. 줄의 끝(end)은 그 줄의 마지막 글자 뒤이고, 줄바꿈 문자는 넣지 않는다.
//!   줄바꿈 문자로 끝난 줄은 다음 줄이 end + 1 에서, 폭으로 나뉜 줄은 다음 줄이 end 에서 시작한다.
//!   폭으로 나뉜 자리의 커서는 기본으로 다음 줄 앞에 보이고, is_upstream 이면 이 줄 끝에 보인다. (블링크의 선택 친화)
//==============================================================================
use crate::base::rect::Rect;
use unicode_segmentation::UnicodeSegmentation;

//==============================================================================
/// 보이는 줄 하나.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextAreaLine {
	/// 첫 글자 자리와 마지막 글자 뒤 자리. (문자 단위)
	pub start: usize,
	pub end: usize,
	/// 줄 위 y. (글 영역 기준)
	pub top: f64,
	/// 줄 안 글자 경계 x. (offsets[i] 는 start + i 자리의 x, 길이는 end - start + 1)
	pub offsets: Vec<f64>,
	/// 줄 안 글자 묶음 경계. (start 기준 문자 자리, 0 과 end - start 를 넣는다. 누른 자리는 이 경계에만 선다)
	pub grapheme_boundary_list: Vec<usize>,
	/// 줄바꿈 문자로 끝났는지. (아니면 폭으로 나뉘었거나 글 끝)
	pub is_hard_break: bool,
}

//==============================================================================
/// 줄 배치.
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TextAreaLayout {
	pub line_list: Vec<TextAreaLine>,
	pub line_height: f64,
}

//==============================================================================
/// 탭을 넣어 폭 재기. (start_x 에서 시작해 끝 x, 탭은 다음 탭 자리로)
//==============================================================================
pub(crate) fn measure_with_tabs(text: &str, start_x: f64, tab_width: f64, measure_width: &dyn Fn(&str) -> f64) -> f64 {
	let mut x = start_x;
	let mut is_first = true;
	for piece in text.split('\t') {
		if !is_first {
			x = advance_to_tab_stop(x, tab_width);
		}
		is_first = false;
		if !piece.is_empty() {
			x += measure_width(piece);
		}
	}
	x
}

//==============================================================================
/// 다음 탭 자리. (지금 x 보다 큰 첫 배수, 탭 폭이 0 이면 그대로)
//==============================================================================
fn advance_to_tab_stop(x: f64, tab_width: f64) -> f64 {
	if tab_width <= 0.0 {
		return x;
	}
	((x / tab_width).floor() + 1.0) * tab_width
}

//==============================================================================
/// 빠른 줄 나눔 표의 글자 범위. (블링크 character_property_data_generator.cc 의 LineBreakData kMinChar, kMaxChar)
//==============================================================================
const FAST_LINE_BREAK_MINIMUM_CHAR: u32 = 0x21;
const FAST_LINE_BREAK_MAXIMUM_CHAR: u32 = 0xFF;
/// 아스키 표의 글자 범위. (FillAscii 의 ALL_CHAR, '!' 부터 0x7F)
const ASCII_LINE_BREAK_MAXIMUM_CHAR: u32 = 0x7F;
const ASCII_LINE_BREAK_CHAR_COUNT: usize = (ASCII_LINE_BREAK_MAXIMUM_CHAR - FAST_LINE_BREAK_MINIMUM_CHAR + 1) as usize;

//==============================================================================
/// 블링크 아스키 줄 나눔 표. ([앞 글자][뒤 글자] 가 참이면 그 사이에서 나눈다)
/// - 블링크 LineBreakData::FillAscii 의 차례 그대로 칸을 덮어 짓는다. 여는 괄호 앞(앞 글자에 따라)과 '-', '?' 뒤에서 나누고, 그 밖의 아스키 짝은 나누지 않는다.
//==============================================================================
fn build_ascii_line_break_table() -> [[bool; ASCII_LINE_BREAK_CHAR_COUNT]; ASCII_LINE_BREAK_CHAR_COUNT] {
	let mut table = [[false; ASCII_LINE_BREAK_CHAR_COUNT]; ASCII_LINE_BREAK_CHAR_COUNT];
	let all_minimum = FAST_LINE_BREAK_MINIMUM_CHAR;
	let all_maximum = ASCII_LINE_BREAK_MAXIMUM_CHAR;
	let mut set_pair_value = |first_minimum: u32, first_maximum: u32, second_minimum: u32, second_maximum: u32, value: bool| {
		for first in first_minimum..=first_maximum {
			for second in second_minimum..=second_maximum {
				table[(first - all_minimum) as usize][(second - all_minimum) as usize] = value;
			}
		}
	};
	let ascii = |character: char| character as u32;
	set_pair_value(all_minimum, all_maximum, all_minimum, all_maximum, false);
	for opening in ['(', '<', '[', '{'] {
		set_pair_value(all_minimum, all_maximum, ascii(opening), ascii(opening), true);
	}
	set_pair_value(ascii('-'), ascii('-'), all_minimum, all_maximum, true);
	set_pair_value(ascii('?'), ascii('?'), all_minimum, all_maximum, true);
	set_pair_value(ascii('-'), ascii('-'), ascii('$'), ascii('$'), false);
	set_pair_value(all_minimum, all_maximum, ascii('!'), ascii('!'), false);
	set_pair_value(ascii('?'), ascii('?'), ascii('"'), ascii('"'), false);
	set_pair_value(ascii('?'), ascii('?'), ascii('\''), ascii('\''), false);
	set_pair_value(all_minimum, all_maximum, ascii(')'), ascii(')'), false);
	set_pair_value(all_minimum, all_maximum, ascii(','), ascii(','), false);
	set_pair_value(all_minimum, all_maximum, ascii('.'), ascii('.'), false);
	set_pair_value(all_minimum, all_maximum, ascii('/'), ascii('/'), false);
	set_pair_value(ascii('-'), ascii('-'), ascii('0'), ascii('9'), false);
	for closing in [':', ';', '?', ']', '}'] {
		set_pair_value(all_minimum, all_maximum, ascii(closing), ascii(closing), false);
	}
	for (first_minimum, first_maximum) in [('$', '$'), ('\'', '\''), ('(', '('), ('/', '/'), ('0', '9'), ('<', '<'), ('@', '@'), ('A', 'Z'), ('[', '['), ('^', '`'), ('a', 'z'), ('{', '{'), ('\u{7F}', '\u{7F}')] {
		set_pair_value(ascii(first_minimum), ascii(first_maximum), all_minimum, all_maximum, false);
	}
	table
}

//==============================================================================
/// 블링크 줄 나누기의 공백. (LazyLineBreakIterator::IsBreakableSpace: 공백, 탭, 줄바꿈)
//==============================================================================
fn is_breakable_space(character: char) -> bool {
	character == ' ' || character == '\t' || character == '\n'
}

//==============================================================================
/// 두 글자만 둔 글에서 ICU 가 그 사이를 나누는지. (블링크 LineBreakData::FillFromIcu 의 짝 값)
//==============================================================================
fn is_icu_pair_breakable(segmenter: &icu_segmenter::LineSegmenterBorrowed<'static>, first: char, second: char) -> bool {
	let mut pair_text = String::new();
	pair_text.push(first);
	pair_text.push(second);
	let first_length = first.len_utf8();
	segmenter.segment_str(&pair_text).any(|break_index| break_index == first_length)
}

//==============================================================================
/// 문단의 줄 나눔 자리. (바이트 자리, 0 과 끝은 빼고 차례대로. 그 자리에서 새 줄이 시작될 수 있다)
/// - 크롬 textarea 의 줄 나누기(블링크 LazyLineBreakIterator::NextBreakablePosition 의 normal, 공백 뭉치 뒤)와 같은 셈이다. 데스크탑과 웹 캔버스 모드가 같다.
///   - 공백(공백, 탭) 앞에서는 나누지 않고, 공백 뭉치 뒤에서 나눈다.
///   - 두 글자가 모두 '!' 부터 0xFF 면 블링크 빠른 표다. 둘 다 아스키면 FillAscii 의 표, 그 밖은 두 글자만 둔 ICU 짝 값이다.
///     '-' 뒤의 숫자는 '-' 앞 글자가 아스키 영숫자일 때만 나눈다. ('-' 뒤가 0x7F 넘는 글자면 ICU)
///   - 그 밖은 유니코드 줄 나누기(UAX #14 의 normal 엄격도, ICU4X LineSegmenter 사전판, 크롬 ICU4C 와 같은 규칙과 태국어 같은 글자의 사전)다.
/// - 블링크 코드를 옮기지 않고, 원문에서 읽은 규칙과 크롬 158 에서 잰 값으로 다시 짰다. (text_break_iterator.cc, character_property_data_generator.cc)
//==============================================================================
fn find_line_break_list(paragraph: &str) -> Vec<usize> {
	use std::sync::OnceLock;
	static ASCII_LINE_BREAK_TABLE: OnceLock<[[bool; ASCII_LINE_BREAK_CHAR_COUNT]; ASCII_LINE_BREAK_CHAR_COUNT]> = OnceLock::new();
	let ascii_table = ASCII_LINE_BREAK_TABLE.get_or_init(build_ascii_line_break_table);
	// 크롬의 ICU 줄 나누기는 lang 이 없으면 뿌리 규칙이고, 작은 가나와 장음 부호(UAX #14 의 CJ) 앞에서 나눈다. (CSS line-break: normal 과 같다, 크롬 158 에서 잼)
	let mut line_break_options = icu_segmenter::options::LineBreakOptions::default();
	line_break_options.strictness = Some(icu_segmenter::options::LineBreakStrictness::Normal);
	let segmenter = icu_segmenter::LineSegmenter::new_dictionary(line_break_options);
	let icu_break_list: Vec<usize> = segmenter.segment_str(paragraph).collect();
	let mut break_list: Vec<usize> = Vec::new();
	let mut last_last: Option<char> = None;
	let mut last: Option<char> = None;
	for (byte_index, current) in paragraph.char_indices() {
		if let Some(last_character) = last {
			if !is_breakable_space(current) {
				if is_breakable_space(last_character) {
					break_list.push(byte_index);
				}
				else {
					let last_code = last_character as u32;
					let current_code = current as u32;
					// 블링크 ShouldBreakFast: 나눔(Some(true)), 나누지 않음(Some(false)), ICU 에 맡김(None)
					let fast_result = if last_code < FAST_LINE_BREAK_MINIMUM_CHAR || current_code < FAST_LINE_BREAK_MINIMUM_CHAR {
						Some(false)
					}
					else if last_character == '-' && current_code > ASCII_LINE_BREAK_MAXIMUM_CHAR {
						None
					}
					else if last_character == '-' && current.is_ascii_digit() {
						Some(last_last.map(|character| character.is_ascii_alphanumeric()).unwrap_or(false))
					}
					else if last_code <= FAST_LINE_BREAK_MAXIMUM_CHAR && current_code <= FAST_LINE_BREAK_MAXIMUM_CHAR {
						if last_code <= ASCII_LINE_BREAK_MAXIMUM_CHAR && current_code <= ASCII_LINE_BREAK_MAXIMUM_CHAR {
							Some(ascii_table[(last_code - FAST_LINE_BREAK_MINIMUM_CHAR) as usize][(current_code - FAST_LINE_BREAK_MINIMUM_CHAR) as usize])
						}
						else {
							Some(is_icu_pair_breakable(&segmenter, last_character, current))
						}
					}
					else {
						None
					};
					let is_breakable = match fast_result {
						Some(is_breakable) => {
							is_breakable
						}
						None => {
							icu_break_list.binary_search(&byte_index).is_ok()
						}
					};
					if is_breakable {
						break_list.push(byte_index);
					}
				}
			}
		}
		last_last = last;
		last = Some(current);
	}
	break_list
}

//==============================================================================
/// 줄 끝 공백을 뺀 글. (pre-wrap 의 매달리는 공백)
//==============================================================================
fn trim_hanging_spaces(text: &str) -> &str {
	text.trim_end_matches([' ', '\u{3000}'])
}

//==============================================================================
/// 줄 배치 짓기. (폭이 0 이하면 줄바꿈 문자로만 나눈다)
//==============================================================================
pub(crate) fn build_text_area_layout(text: &str, wrap_width: f64, line_height: f64, tab_width: f64, measure_width: &dyn Fn(&str) -> f64) -> TextAreaLayout {
	let mut line_list: Vec<TextAreaLine> = Vec::new();
	let mut paragraph_start_character = 0;
	let paragraph_list: Vec<&str> = text.split('\n').collect();
	let paragraph_count = paragraph_list.len();
	for (paragraph_index, paragraph) in paragraph_list.into_iter().enumerate() {
		let is_last_paragraph = paragraph_index + 1 == paragraph_count;
		let mut piece_list: Vec<(usize, usize)> = Vec::new();
		if paragraph.is_empty() || wrap_width <= 0.0 {
			piece_list.push((0, paragraph.len()));
		}
		else {
			let break_list = find_line_break_list(paragraph);
			let mut line_start = 0;
			while line_start < paragraph.len() {
				// 넘지 않는 가장 먼 나눔 자리.
				let mut best_end: Option<usize> = None;
				for break_index in break_list.iter().copied().filter(|break_index| *break_index > line_start).chain(std::iter::once(paragraph.len())) {
					let candidate_width = measure_with_tabs(trim_hanging_spaces(&paragraph[line_start..break_index]), 0.0, tab_width, measure_width);
					if candidate_width <= wrap_width {
						best_end = Some(break_index);
					}
					else {
						break;
					}
				}
				let line_end = match best_end {
					Some(line_end) => {
						line_end
					}
					None => {
						// 첫 조각이 넘친다: 글자 묶음 사이에서 나눈다. (적어도 한 묶음)
						let mut grapheme_end = line_start;
						for (grapheme_offset, grapheme) in paragraph[line_start..].grapheme_indices(true) {
							let next_end = line_start + grapheme_offset + grapheme.len();
							if grapheme_end > line_start && measure_with_tabs(&paragraph[line_start..next_end], 0.0, tab_width, measure_width) > wrap_width {
								break;
							}
							grapheme_end = next_end;
						}
						grapheme_end
					}
				};
				piece_list.push((line_start, line_end));
				line_start = line_end;
			}
		}
		let piece_count = piece_list.len();
		for (piece_index, (piece_start, piece_end)) in piece_list.into_iter().enumerate() {
			let piece_text = &paragraph[piece_start..piece_end];
			let start = paragraph_start_character + paragraph[..piece_start].chars().count();
			let character_count = piece_text.chars().count();
			let mut offsets: Vec<f64> = Vec::with_capacity(character_count + 1);
			offsets.push(0.0);
			for (byte_index, character) in piece_text.char_indices() {
				let prefix_end = byte_index + character.len_utf8();
				offsets.push(measure_with_tabs(&piece_text[..prefix_end], 0.0, tab_width, measure_width));
			}
			let line_index = line_list.len();
			line_list.push(TextAreaLine {
				start,
				end: start + character_count,
				top: line_index as f64 * line_height,
				offsets,
				grapheme_boundary_list: crate::core::textinput::collect_grapheme_boundaries(piece_text),
				is_hard_break: piece_index + 1 == piece_count && !is_last_paragraph,
			});
		}
		paragraph_start_character += paragraph.chars().count() + 1;
	}
	TextAreaLayout {
		line_list,
		line_height,
	}
}

impl TextAreaLayout {
	//==============================================================================
	/// 글 높이. (줄 수 x 줄 높이)
	//==============================================================================
	pub(crate) fn get_content_height(&self) -> f64 {
		self.line_list.len() as f64 * self.line_height
	}

	//==============================================================================
	/// 자리가 있는 줄. (폭으로 나뉜 자리는 is_upstream 이면 앞 줄, 아니면 다음 줄)
	//==============================================================================
	pub(crate) fn find_line_index(&self, character_index: usize, is_upstream: bool) -> usize {
		if self.line_list.is_empty() {
			return 0;
		}
		for (line_index, line) in self.line_list.iter().enumerate() {
			if character_index < line.end {
				return line_index;
			}
			if character_index == line.end {
				let is_soft_boundary = !line.is_hard_break && line_index + 1 < self.line_list.len();
				if is_soft_boundary && !is_upstream {
					return line_index + 1;
				}
				return line_index;
			}
		}
		self.line_list.len() - 1
	}

	//==============================================================================
	/// 자리의 x. (그 줄의 글자 경계)
	//==============================================================================
	pub(crate) fn get_x_in_line(&self, line_index: usize, character_index: usize) -> f64 {
		match self.line_list.get(line_index) {
			Some(line) => {
				let local_index = character_index.clamp(line.start, line.end) - line.start;
				line.offsets.get(local_index).copied().unwrap_or(0.0)
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 커서 자리. ((x, 줄 위 y), 글 영역 기준)
	//==============================================================================
	pub(crate) fn get_caret_position(&self, character_index: usize, is_upstream: bool) -> (f64, f64) {
		let line_index = self.find_line_index(character_index, is_upstream);
		let top = self.line_list.get(line_index).map(|line| line.top).unwrap_or(0.0);
		(self.get_x_in_line(line_index, character_index), top)
	}

	//==============================================================================
	/// 줄 안 x 에 가까운 자리. ((자리, is_upstream), 줄 끝을 넘으면 줄 끝이고 폭으로 나뉜 줄이면 이 줄에 남게 is_upstream)
	//==============================================================================
	pub(crate) fn find_character_index_in_line(&self, line_index: usize, x: f64) -> (usize, bool) {
		let line = match self.line_list.get(line_index) {
			Some(line) => {
				line
			}
			None => {
				return (0, false);
			}
		};
		let local_index = crate::ui::uiinputfield::find_boundary_index_at_offset(&line.offsets, &line.grapheme_boundary_list, x);
		let character_index = line.start + local_index;
		let is_soft_end = character_index == line.end && !line.is_hard_break && line_index + 1 < self.line_list.len();
		(character_index, is_soft_end)
	}

	//==============================================================================
	/// 글 영역 (x, y) 에 가까운 자리. (위로 넘으면 첫 줄, 아래로 넘으면 마지막 줄)
	//==============================================================================
	pub(crate) fn find_character_index_at(&self, x: f64, y: f64) -> (usize, bool) {
		if self.line_list.is_empty() || self.line_height <= 0.0 {
			return (0, false);
		}
		let line_index = ((y / self.line_height).floor().max(0.0) as usize).min(self.line_list.len() - 1);
		self.find_character_index_in_line(line_index, x)
	}

	//==============================================================================
	/// 줄 위아래로 옮긴 자리. (line_delta 만큼, 가로 자리는 preferred_x. 첫 줄 위는 글 앞, 마지막 줄 아래는 글 끝)
	//==============================================================================
	pub(crate) fn move_vertically(&self, character_index: usize, is_upstream: bool, preferred_x: f64, line_delta: isize) -> (usize, bool) {
		if self.line_list.is_empty() {
			return (0, false);
		}
		let line_index = self.find_line_index(character_index, is_upstream) as isize;
		let target_index = line_index + line_delta;
		if target_index < 0 {
			return (0, false);
		}
		if target_index >= self.line_list.len() as isize {
			let last_line = &self.line_list[self.line_list.len() - 1];
			return (last_line.end, false);
		}
		self.find_character_index_in_line(target_index as usize, preferred_x)
	}

	//==============================================================================
	/// 줄의 앞 자리와 끝 자리. (끝은 폭으로 나뉜 줄이면 이 줄에 남게 is_upstream)
	//==============================================================================
	pub(crate) fn get_line_boundary(&self, line_index: usize, is_end: bool) -> (usize, bool) {
		match self.line_list.get(line_index) {
			Some(line) => {
				if is_end {
					let is_soft_end = !line.is_hard_break && line_index + 1 < self.line_list.len();
					(line.end, is_soft_end)
				}
				else {
					(line.start, false)
				}
			}
			None => {
				(0, false)
			}
		}
	}

	//==============================================================================
	/// 선택 사각형들. (줄마다 하나, 글 영역 기준)
	//==============================================================================
	pub(crate) fn build_selection_rect_list(&self, start_index: usize, end_index: usize) -> Vec<Rect> {
		let mut rect_list: Vec<Rect> = Vec::new();
		if start_index >= end_index {
			return rect_list;
		}
		for (line_index, line) in self.line_list.iter().enumerate() {
			let line_selection_start = start_index.max(line.start);
			let line_selection_end = end_index.min(line.end);
			let is_line_break_selected = line.is_hard_break && start_index <= line.end && end_index > line.end;
			if line_selection_start > line_selection_end || (line_selection_start == line_selection_end && !is_line_break_selected) {
				continue;
			}
			let start_x = self.get_x_in_line(line_index, line_selection_start);
			let end_x = self.get_x_in_line(line_index, line_selection_end);
			rect_list.push(Rect::create(start_x, line.top, (end_x - start_x).max(0.0), self.line_height));
		}
		rect_list
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 줄 나눔 자리가 크롬 textarea 와 같다. (크롬 158 맥, 1px 폭 pre-wrap 칸에서 줄이 바뀌는 첫 글자의 자리를 떠 왔다. 글자 단위)
	//==============================================================================
	#[test]
	fn line_break_list_matches_chrome_textarea() {
		let case_list: [(&str, &[usize]); 30] = [
			("hello world foo", &[6, 12]),
			("state-of-the-art e-mail", &[6, 9, 13, 17, 19]),
			("a/b http://x.com/a?b=c", &[4, 19]),
			("(abc) [def] {ghi}", &[6, 12]),
			("1,000.50 $30 30% 3.14", &[9, 13, 17]),
			("안녕하세요 반갑습니다", &[1, 2, 3, 4, 6, 7, 8, 9, 10]),
			("日本語の文章です。終わり", &[1, 2, 3, 4, 5, 6, 7, 9, 10, 11]),
			("中文句子，测试。", &[1, 2, 3, 5, 6]),
			("ภาษาไทยเป็นภาษาที่สวยงาม", &[4, 7, 11, 15, 18]),
			("foo—bar…baz", &[3, 4, 8]),
			("don't stop", &[6]),
			("a  b", &[3]),
			("가나다abc가나", &[1, 2, 3, 6, 7]),
			("x=1;y=2", &[]),
			("「こんにちは」と言った", &[2, 3, 4, 5, 7, 8, 9, 10]),
			("テスト・ケース", &[1, 2, 4, 5, 6]),
			("a(b)c x)(y z=[1]", &[6, 8, 11, 13]),
			("foo-123 a-1 -1 1-2", &[4, 8, 10, 12, 15, 17]),
			("«bonjour» déjà-vu ¿qué?", &[10, 15, 18]),
			("price: $5, ok! 50%off", &[7, 11, 15]),
			("tab\there", &[4]),
			("C++/C# a.b,c;d", &[7]),
			("über-straße", &[5]),
			("A&B|C~D", &[]),
			("привет мир", &[7]),
			("ABC-DEF?ghi", &[4, 8]),
			("あ〜い゠う", &[2, 4]),
			("ぁぃぅ々〻", &[1, 2]),
			("「中」〜〜", &[]),
			("a‐b–c—d", &[2, 4, 5, 6]),
		];
		let mismatch_list: Vec<String> = case_list.iter().filter_map(|(text, chrome_break_list)| {
			let char_break_list: Vec<usize> = find_line_break_list(text).into_iter().map(|byte_index| text[..byte_index].chars().count()).collect();
			if char_break_list == chrome_break_list.to_vec() {
				None
			}
			else {
				Some(format!("{}: 엔진 {:?}, 크롬 {:?}", text, char_break_list, chrome_break_list))
			}
		}).collect();
		assert!(mismatch_list.is_empty(), "{:#?}", mismatch_list);
	}

	// 글자마다 10 인 고정폭 재기.
	fn measure_fixed(text: &str) -> f64 {
		text.chars().count() as f64 * 10.0
	}

	//==============================================================================
	/// pre-wrap: 공백 뒤에서 나누고 줄 끝 공백은 매달리며, 줄바꿈 문자는 늘 나눈다. 긴 낱말은 글자 사이에서 나눈다.
	//==============================================================================
	#[test]
	fn wraps_like_pre_wrap_break_word() {
		let layout = build_text_area_layout("hello world foo\nabcdefghijkl", 80.0, 20.0, 80.0, &measure_fixed);
		let line_text_list: Vec<(usize, usize, bool)> = layout.line_list.iter().map(|line| (line.start, line.end, line.is_hard_break)).collect();
		// "hello " | "world " | "foo"(줄바꿈) | "abcdefgh" | "ijkl"
		assert_eq!(line_text_list, vec![(0, 6, false), (6, 12, false), (12, 15, true), (16, 24, false), (24, 28, false)]);
		assert_eq!(layout.line_list[1].top, 20.0);
		assert_eq!(layout.get_content_height(), 100.0);
		// 폭으로 나뉜 자리 6 은 기본으로 다음 줄, is_upstream 이면 앞 줄 끝.
		assert_eq!(layout.get_caret_position(6, false), (0.0, 20.0));
		assert_eq!(layout.get_caret_position(6, true), (60.0, 0.0));
		// 줄바꿈 문자 앞 자리 15 는 그 줄 끝.
		assert_eq!(layout.get_caret_position(15, false), (30.0, 40.0));
		assert_eq!(layout.get_caret_position(16, false), (0.0, 60.0));
	}

	//==============================================================================
	/// 위아래 이동은 가로 자리를 지키고, 첫 줄 위는 글 앞, 마지막 줄 아래는 글 끝. 누른 자리 찾기.
	//==============================================================================
	#[test]
	fn moves_vertically_and_hit_tests() {
		let layout = build_text_area_layout("abcdef\nab\nabcdef", 0.0, 20.0, 80.0, &measure_fixed);
		// 4 번(첫 줄 x 40)에서 아래로: 둘째 줄은 짧아 끝(9), 가로 40 을 지켜 셋째 줄 14.
		assert_eq!(layout.move_vertically(4, false, 40.0, 1), (9, false));
		assert_eq!(layout.move_vertically(9, false, 40.0, 1), (14, false));
		assert_eq!(layout.move_vertically(4, false, 40.0, -1), (0, false));
		assert_eq!(layout.move_vertically(14, false, 40.0, 1), (16, false));
		assert_eq!(layout.find_character_index_at(26.0, 25.0), (9, false));
		assert_eq!(layout.find_character_index_at(14.0, 45.0), (11, false));
		assert_eq!(layout.find_character_index_at(-5.0, -30.0), (0, false));
		// 선택: 첫 줄 2 부터 셋째 줄 12 까지면 줄마다 하나. (둘째 줄은 줄바꿈까지)
		let rect_list = layout.build_selection_rect_list(2, 12);
		assert_eq!(rect_list.len(), 3);
		assert_eq!(rect_list[0], Rect::create(20.0, 0.0, 40.0, 20.0));
		assert_eq!(rect_list[1], Rect::create(0.0, 20.0, 20.0, 20.0));
		assert_eq!(rect_list[2], Rect::create(0.0, 40.0, 20.0, 20.0));
	}

	//==============================================================================
	/// 탭은 탭 자리까지 민다.
	//==============================================================================
	#[test]
	fn tabs_advance_to_stops() {
		assert_eq!(measure_with_tabs("a\tb", 0.0, 40.0, &measure_fixed), 50.0);
		assert_eq!(measure_with_tabs("abcd\tb", 0.0, 40.0, &measure_fixed), 90.0);
		let layout = build_text_area_layout("\tx", 0.0, 20.0, 40.0, &measure_fixed);
		assert_eq!(layout.line_list[0].offsets, vec![0.0, 40.0, 50.0]);
	}

	//==============================================================================
	/// 빈 글과 줄바꿈으로 끝난 글은 빈 줄을 갖는다.
	//==============================================================================
	#[test]
	fn empty_lines() {
		let layout = build_text_area_layout("", 100.0, 20.0, 80.0, &measure_fixed);
		assert_eq!(layout.line_list.len(), 1);
		let layout = build_text_area_layout("a\n", 100.0, 20.0, 80.0, &measure_fixed);
		assert_eq!(layout.line_list.len(), 2);
		assert_eq!(layout.get_caret_position(2, false), (0.0, 20.0));
	}
}

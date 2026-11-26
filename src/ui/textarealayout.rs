//==============================================================================
//! 여러 줄 입력 필드의 줄 배치. (크롬 textarea 의 white-space: pre-wrap 과 overflow-wrap: break-word 셈)
//! - 줄바꿈 문자로 문단을 나누고, 문단은 폭을 넘지 않게 줄 나눔 자리에서 나눈다. 줄 나눔 자리는 크롬이 쓰는 ICU 처럼
//!   유니코드 줄 나누기(UAX #14)다. (데스크탑은 ICU4X LineSegmenter 의 사전판, 웹 캔버스 모드는 낱말 경계로 갈음한다)
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
	pub top: f32,
	/// 줄 안 글자 경계 x. (offsets[i] 는 start + i 자리의 x, 길이는 end - start + 1)
	pub offsets: Vec<f32>,
	/// 줄바꿈 문자로 끝났는지. (아니면 폭으로 나뉘었거나 글 끝)
	pub is_hard_break: bool,
}

//==============================================================================
/// 줄 배치.
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TextAreaLayout {
	pub line_list: Vec<TextAreaLine>,
	pub line_height: f32,
}

//==============================================================================
/// 탭을 넣어 폭 재기. (start_x 에서 시작해 끝 x, 탭은 다음 탭 자리로)
//==============================================================================
pub(crate) fn measure_with_tabs(text: &str, start_x: f32, tab_width: f32, measure_width: &dyn Fn(&str) -> f32) -> f32 {
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
fn advance_to_tab_stop(x: f32, tab_width: f32) -> f32 {
	if tab_width <= 0.0 {
		return x;
	}
	((x / tab_width).floor() + 1.0) * tab_width
}

//==============================================================================
/// 문단의 줄 나눔 자리. (바이트 자리, 0 과 끝은 빼고 차례대로. 그 자리에서 새 줄이 시작될 수 있다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn find_line_break_list(paragraph: &str) -> Vec<usize> {
	let segmenter = icu_segmenter::LineSegmenter::new_dictionary(Default::default());
	segmenter.segment_str(paragraph).filter(|break_index| *break_index > 0 && *break_index < paragraph.len()).collect()
}

//==============================================================================
/// 문단의 줄 나눔 자리. (웹 캔버스 모드: 공백 뒤와 낱말 경계로 갈음한다. 웹 DOM 입력의 줄은 브라우저 textarea 가 나눈다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn find_line_break_list(paragraph: &str) -> Vec<usize> {
	let mut break_list: Vec<usize> = Vec::new();
	let mut byte_index = 0;
	let mut is_previous_space = false;
	for (segment, is_word_like) in crate::core::component::text::segment_words(paragraph) {
		let is_space = !is_word_like && segment.chars().all(char::is_whitespace);
		if byte_index > 0 && (is_previous_space || (is_word_like && !is_space)) {
			break_list.push(byte_index);
		}
		is_previous_space = is_space;
		byte_index += segment.len();
	}
	break_list.retain(|break_index| *break_index < paragraph.len());
	break_list.dedup();
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
pub(crate) fn build_text_area_layout(text: &str, wrap_width: f32, line_height: f32, tab_width: f32, measure_width: &dyn Fn(&str) -> f32) -> TextAreaLayout {
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
			let mut offsets: Vec<f32> = Vec::with_capacity(character_count + 1);
			offsets.push(0.0);
			for (byte_index, character) in piece_text.char_indices() {
				let prefix_end = byte_index + character.len_utf8();
				offsets.push(measure_with_tabs(&piece_text[..prefix_end], 0.0, tab_width, measure_width));
			}
			let line_index = line_list.len();
			line_list.push(TextAreaLine {
				start,
				end: start + character_count,
				top: line_index as f32 * line_height,
				offsets,
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
	pub(crate) fn get_content_height(&self) -> f32 {
		self.line_list.len() as f32 * self.line_height
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
	pub(crate) fn get_x_in_line(&self, line_index: usize, character_index: usize) -> f32 {
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
	pub(crate) fn get_caret_position(&self, character_index: usize, is_upstream: bool) -> (f32, f32) {
		let line_index = self.find_line_index(character_index, is_upstream);
		let top = self.line_list.get(line_index).map(|line| line.top).unwrap_or(0.0);
		(self.get_x_in_line(line_index, character_index), top)
	}

	//==============================================================================
	/// 줄 안 x 에 가까운 자리. ((자리, is_upstream), 줄 끝을 넘으면 줄 끝이고 폭으로 나뉜 줄이면 이 줄에 남게 is_upstream)
	//==============================================================================
	pub(crate) fn find_character_index_in_line(&self, line_index: usize, x: f32) -> (usize, bool) {
		let line = match self.line_list.get(line_index) {
			Some(line) => {
				line
			}
			None => {
				return (0, false);
			}
		};
		let local_index = crate::ui::uiinputfield::find_character_index_at_offset(&line.offsets, x);
		let character_index = line.start + local_index;
		let is_soft_end = character_index == line.end && !line.is_hard_break && line_index + 1 < self.line_list.len();
		(character_index, is_soft_end)
	}

	//==============================================================================
	/// 글 영역 (x, y) 에 가까운 자리. (위로 넘으면 첫 줄, 아래로 넘으면 마지막 줄)
	//==============================================================================
	pub(crate) fn find_character_index_at(&self, x: f32, y: f32) -> (usize, bool) {
		if self.line_list.is_empty() || self.line_height <= 0.0 {
			return (0, false);
		}
		let line_index = ((y / self.line_height).floor().max(0.0) as usize).min(self.line_list.len() - 1);
		self.find_character_index_in_line(line_index, x)
	}

	//==============================================================================
	/// 줄 위아래로 옮긴 자리. (line_delta 만큼, 가로 자리는 preferred_x. 첫 줄 위는 글 앞, 마지막 줄 아래는 글 끝)
	//==============================================================================
	pub(crate) fn move_vertically(&self, character_index: usize, is_upstream: bool, preferred_x: f32, line_delta: isize) -> (usize, bool) {
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

	// 글자마다 10 인 고정폭 재기.
	fn measure_fixed(text: &str) -> f32 {
		text.chars().count() as f32 * 10.0
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

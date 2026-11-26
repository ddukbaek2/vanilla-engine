//==============================================================================
//! 화면 분할 패널. (원본 web/pane.js 의 PaneTheme, PaneStyle, Pane)
//! - 원본은 도구(파티클 에디터 같은)의 창 구성을 DOM 으로 짓는다. 가로나 세로로 나눈 패널, 고정, 비율, flex 크기, 최소와 최대, 끌어서 크기를 바꾸는 리사이저,
//!   숨기기, 끌어서 바꾼 크기 지우기, 배치 콜백, 그리고 같은 테마 색의 요소를 짓는 PaneStyle 프리셋(panel, toolbar, button, input, textarea, text)이다.
//! - 웹은 원본처럼 DOM 요소로 짓는다. (pane/dompane.rs)
//! - 데스크탑은 DOM 이 없어 엔진 노드로 같은 일을 한다. 컨테이너는 노드이고 내용은 그 자식 노드다. 리사이저는 끌 수 있는 노드이고 테마 색으로 그린다. (pane/nodepane.rs)
//! - 크기 셈(calculate_pane_layout)과 끌기 셈(calculate_resized_size)은 두 플랫폼이 같은 함수를 쓴다. (원본 updateLayout, onMouseMove)
//! - 원본과 다른 점:
//!   - PaneTheme 은 원본의 묶음(color, size, font)을 이름 앞머리로 편 연관 상수다. (PaneTheme.color.background → PaneTheme::COLOR_BACKGROUND,
//!     PaneTheme.size.resizer → PaneTheme::SIZE_RESIZER, PaneTheme.font.sizeSmall → PaneTheme::FONT_SIZE_SMALL)
//!     줄인 이름은 풀었다. (color.inputBg → COLOR_INPUT_BACKGROUND, font.mono → FONT_MONOSPACE) SIZE_RESIZER 는 픽셀 수(f64)다.
//!   - Pane 의 옵션은 PaneOptions 다. 크기는 PaneSize 다. (Flex, Fixed(픽셀), Ratio(0~1 비율))
//!     원본은 숫자 하나로 0 이나 "flex" 는 flex, 1 이하는 비율, 1 을 넘으면 픽셀로 읽는다. ("50%" 는 Ratio(0.5)) 그래서 get_initial_size 는 PaneSize 를 돌려준다.
//!   - PaneStyle::create 의 옵션은 PaneStyleOptions 다. 빈 글은 원본의 빈 값처럼 넣지 않는다. 원본 options.type 은 element_type 이다. (type 은 러스트 예약어)
//==============================================================================

// 패널 크기가 다시 놓일 때 부르는 콜백. (원본 setCallback)
pub type PaneResizeCallback = Box<dyn FnMut()>;

//==============================================================================
/// 테마 상수. (원본 PaneTheme)
//==============================================================================
pub struct PaneTheme;

impl PaneTheme {
	pub const COLOR_BACKGROUND: &'static str = "#1e1e1e";
	pub const COLOR_PANEL: &'static str = "#252526";
	pub const COLOR_TOOLBAR: &'static str = "#2d2d30";
	pub const COLOR_BORDER: &'static str = "#444";
	pub const COLOR_TEXT: &'static str = "#ccc";
	pub const COLOR_TEXT_DIM: &'static str = "#aaaaaa";
	pub const COLOR_ACCENT: &'static str = "#0e639c";
	pub const COLOR_ACCENT_HOVER: &'static str = "#1177bb";
	/// 주황색.
	pub const COLOR_WARNING: &'static str = "#d97706";
	pub const COLOR_WARNING_HOVER: &'static str = "#f59e0b";
	pub const COLOR_ERROR: &'static str = "#f44336";
	pub const COLOR_SUCCESS: &'static str = "#4CAF50";
	pub const COLOR_INPUT_BACKGROUND: &'static str = "#3c3c3c";
	pub const COLOR_RESIZER: &'static str = "#333";
	pub const COLOR_RESIZER_HOVER: &'static str = "#007acc";
	/// 리사이저 두께. (픽셀)
	pub const SIZE_RESIZER: f64 = 5.0;
	pub const FONT_FAMILY: &'static str = "sans-serif";
	pub const FONT_MONOSPACE: &'static str = "\"Consolas\", monospace";
	pub const FONT_SIZE: &'static str = "13px";
	pub const FONT_SIZE_SMALL: &'static str = "12px";
}

//==============================================================================
/// PaneStyle::create 옵션. (원본 options: { id, text, style, type, placeholder, accent, warning })
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct PaneStyleOptions<'a> {
	pub id: &'a str,
	pub text: &'a str,
	pub style: &'a [(&'a str, &'a str)],
	pub element_type: &'a str,
	pub placeholder: &'a str,
	pub accent: bool,
	pub warning: bool,
}

//==============================================================================
/// 나누는 방향. (원본 "horizontal" | "vertical")
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneDirection {
	/// 자식을 왼쪽에서 오른쪽으로 놓는다.
	#[default]
	Horizontal,
	/// 자식을 위에서 아래로 놓는다.
	Vertical,
}

//==============================================================================
/// 처음 크기. (부모의 나누는 방향 쪽 길이)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PaneSize {
	/// 고정 크기 형제들이 쓰고 남은 자리를 flex 형제끼리 똑같이 나눈다. (원본 0, "flex")
	#[default]
	Flex,
	/// 픽셀. (원본 1 을 넘는 숫자)
	Fixed(f64),
	/// 리사이저를 뺀 부모 길이에 곱하는 비율. (원본 1 이하 숫자, "50%")
	Ratio(f64),
}

//==============================================================================
/// 리사이저로 끌 수 있는 가장자리. (원본 resizableEdges)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneResizableEdges {
	pub top: bool,
	pub bottom: bool,
	pub left: bool,
	pub right: bool,
}

impl Default for PaneResizableEdges {
	//==============================================================================
	/// 기본값. (네 가장자리 모두 끌 수 있다)
	//==============================================================================
	fn default() -> PaneResizableEdges {
		PaneResizableEdges {
			top: true,
			bottom: true,
			left: true,
			right: true,
		}
	}
}

//==============================================================================
/// Pane 생성 옵션. (원본 options: { direction, size, minSize, maxSize, isResizable, resizableEdges })
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct PaneOptions {
	pub direction: PaneDirection,
	pub size: PaneSize,
	pub min_size: f64,
	pub max_size: f64,
	pub is_resizable: bool,
	pub resizable_edges: PaneResizableEdges,
}

impl Default for PaneOptions {
	//==============================================================================
	/// 기본값. (원본과 같이 가로, flex, 최소 50, 최대 제한 없음, 끌 수 있음)
	//==============================================================================
	fn default() -> PaneOptions {
		PaneOptions {
			direction: PaneDirection::Horizontal,
			size: PaneSize::Flex,
			min_size: 50.0,
			max_size: f64::MAX,
			is_resizable: true,
			resizable_edges: PaneResizableEdges::default(),
		}
	}
}

//==============================================================================
/// 배치 셈에 넣는 자식 하나.
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub(crate) struct PaneLayoutChild {
	pub is_visible: bool,
	pub initial_size: PaneSize,
	pub fixed_size: f64,
	pub min_size: f64,
	pub max_size: f64,
}

//==============================================================================
/// 배치 셈 결과. (자식과 리사이저의 [x, y, 폭, 높이], 숨겼으면 None. 자리는 패널 안쪽 기준)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaneLayoutResult {
	pub child_rect_list: Vec<Option<[f64; 4]>>,
	pub resizer_rect_list: Vec<Option<[f64; 4]>>,
}

//==============================================================================
/// 리사이저 두께. (끌 수 있으면 SIZE_RESIZER, 아니면 1 의 선. 원본 createResizer)
//==============================================================================
pub(crate) fn calculate_resizer_thickness(direction: PaneDirection, is_parent_resizable: bool, previous_is_resizable: bool, previous_edges: PaneResizableEdges, next_is_resizable: bool, next_edges: PaneResizableEdges) -> (bool, f64) {
	let mut can_resize = is_parent_resizable && previous_is_resizable && next_is_resizable;
	match direction {
		PaneDirection::Horizontal => {
			if !previous_edges.right || !next_edges.left {
				can_resize = false;
			}
		}
		PaneDirection::Vertical => {
			if !previous_edges.bottom || !next_edges.top {
				can_resize = false;
			}
		}
	}
	let thickness = if can_resize {
		PaneTheme::SIZE_RESIZER
	}
	else {
		1.0
	};
	(can_resize, thickness)
}

//==============================================================================
/// 배치 셈. (원본 updateLayout: 숨긴 자식과 맞닿은 리사이저는 빼고, 고정 크기 자식을 먼저 재고 남은 자리를 flex 자식끼리 나누고, 마지막 보이는 자식이 남은 끝까지 채운다)
//==============================================================================
pub(crate) fn calculate_pane_layout(direction: PaneDirection, width: f64, height: f64, child_list: &[PaneLayoutChild], resizer_thickness_list: &[f64]) -> PaneLayoutResult {
	let mut result = PaneLayoutResult {
		child_rect_list: vec![None; child_list.len()],
		resizer_rect_list: vec![None; resizer_thickness_list.len()],
	};
	if child_list.is_empty() {
		return result;
	}
	let mut total_resizer_thickness = 0.0;
	let mut resizer_visible_list: Vec<bool> = Vec::with_capacity(resizer_thickness_list.len());
	for (index, thickness) in resizer_thickness_list.iter().enumerate() {
		let is_resizer_visible = child_list.get(index).map(|child| child.is_visible).unwrap_or(false) && child_list.get(index + 1).map(|child| child.is_visible).unwrap_or(false);
		if is_resizer_visible {
			total_resizer_thickness += thickness;
		}
		resizer_visible_list.push(is_resizer_visible);
	}
	let main_size = match direction {
		PaneDirection::Horizontal => {
			width
		}
		PaneDirection::Vertical => {
			height
		}
	};
	let available_size = main_size - total_resizer_thickness;
	let mut used_size = 0.0;
	let mut flex_count = 0;
	let mut last_visible_index: Option<usize> = None;
	let mut size_list: Vec<f64> = vec![0.0; child_list.len()];
	for (index, child) in child_list.iter().enumerate() {
		if !child.is_visible {
			continue;
		}
		last_visible_index = Some(index);
		if child.initial_size == PaneSize::Flex {
			flex_count += 1;
		}
		else {
			let mut size = child.fixed_size;
			if size == 0.0 {
				size = match child.initial_size {
					PaneSize::Ratio(ratio) => {
						available_size * ratio
					}
					PaneSize::Fixed(pixel_size) => {
						pixel_size
					}
					PaneSize::Flex => {
						0.0
					}
				};
			}
			size = f64::max(child.min_size, f64::min(size, child.max_size));
			size_list[index] = size;
			used_size += size;
		}
	}
	let size_per_flex = if flex_count > 0 {
		f64::max(0.0, available_size - used_size) / flex_count as f64
	}
	else {
		0.0
	};
	for (index, child) in child_list.iter().enumerate() {
		if child.is_visible && child.initial_size == PaneSize::Flex {
			size_list[index] = size_per_flex;
		}
	}
	let mut offset = 0.0;
	for (index, child) in child_list.iter().enumerate() {
		if !child.is_visible {
			continue;
		}
		let child_size = if Some(index) == last_visible_index {
			main_size - offset
		}
		else {
			size_list[index]
		};
		result.child_rect_list[index] = Some(match direction {
			PaneDirection::Horizontal => {
				[offset, 0.0, child_size, height]
			}
			PaneDirection::Vertical => {
				[0.0, offset, width, child_size]
			}
		});
		offset += child_size;
		if index < resizer_thickness_list.len() && resizer_visible_list[index] {
			let thickness = resizer_thickness_list[index];
			result.resizer_rect_list[index] = Some(match direction {
				PaneDirection::Horizontal => {
					[offset, 0.0, thickness, height]
				}
				PaneDirection::Vertical => {
					[0.0, offset, width, thickness]
				}
			});
			offset += thickness;
		}
	}
	result
}

//==============================================================================
/// 끌기 셈. (원본 onMouseMove: 두 자식 사이의 쓸 수 있는 길이 안에서 앞 자식 크기를 정하고,
/// 앞이 flex 이고 뒤가 고정이면 뒤 자식의 고정 크기를, 아니면 앞 자식의 고정 크기를 바꾼다. (뒤 자식을 바꾸는지, 새 고정 크기))
/// - previous_start, next_end: 나누는 방향 쪽 앞 자식의 시작과 뒤 자식의 끝. pointer: 같은 좌표의 포인터 자리.
//==============================================================================
pub(crate) fn calculate_resized_size(previous_start: f64, next_end: f64, thickness: f64, pointer: f64, grab_offset: f64, previous_min_size: f64, next_min_size: f64, previous_is_flex: bool, next_is_flex: bool) -> (bool, f64) {
	let total_available = (next_end - previous_start) - thickness;
	let mut new_previous_size = (pointer - grab_offset) - previous_start;
	new_previous_size = f64::max(previous_min_size, f64::min(new_previous_size, total_available - next_min_size));
	if previous_is_flex && !next_is_flex {
		(true, total_available - new_previous_size)
	}
	else {
		(false, new_previous_size)
	}
}

#[cfg(target_arch = "wasm32")]
mod dompane;
#[cfg(target_arch = "wasm32")]
pub use dompane::{Pane, PaneStyle};
#[cfg(not(target_arch = "wasm32"))]
mod nodepane;
#[cfg(not(target_arch = "wasm32"))]
pub use nodepane::{Pane, PaneClickCallback, PaneElement, PaneStyle};

//==============================================================================
/// 시험. (두 플랫폼이 같이 쓰는 셈)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 고정, 비율, flex, 최소와 최대, 숨김, 마지막 자식이 끝까지.
	//==============================================================================
	#[test]
	fn layout_matches_original_rules() {
		let child = |initial_size: PaneSize, min_size: f64| -> PaneLayoutChild {
			PaneLayoutChild {
				is_visible: true,
				initial_size,
				fixed_size: match initial_size {
					PaneSize::Fixed(pixel_size) => {
						pixel_size
					}
					_ => {
						0.0
					}
				},
				min_size,
				max_size: f64::MAX,
			}
		};
		let child_list = [child(PaneSize::Fixed(240.0), 170.0), child(PaneSize::Flex, 200.0), child(PaneSize::Ratio(0.25), 50.0)];
		let result = calculate_pane_layout(PaneDirection::Horizontal, 1000.0, 600.0, &child_list, &[5.0, 5.0]);
		assert_eq!(result.child_rect_list[0], Some([0.0, 0.0, 240.0, 600.0]));
		assert_eq!(result.resizer_rect_list[0], Some([240.0, 0.0, 5.0, 600.0]));
		// 비율은 리사이저를 뺀 990 의 0.25, flex 는 남은 자리.
		assert_eq!(result.child_rect_list[1], Some([245.0, 0.0, 990.0 - 240.0 - 247.5, 600.0]));
		let third_start = 245.0 + 502.5 + 5.0;
		assert_eq!(result.child_rect_list[2], Some([third_start, 0.0, 1000.0 - third_start, 600.0]));

		// 가운데를 숨기면 맞닿은 리사이저 둘이 숨고, 마지막 보이는 자식이 끝까지 채운다.
		let mut hidden_list = child_list;
		hidden_list[1].is_visible = false;
		let hidden_result = calculate_pane_layout(PaneDirection::Horizontal, 1000.0, 600.0, &hidden_list, &[5.0, 5.0]);
		assert_eq!(hidden_result.child_rect_list[1], None);
		assert_eq!(hidden_result.resizer_rect_list, vec![None, None]);
		assert_eq!(hidden_result.child_rect_list[2], Some([240.0, 0.0, 760.0, 600.0]));

		// 최소 크기.
		let small_list = [child(PaneSize::Fixed(10.0), 50.0), child(PaneSize::Flex, 50.0)];
		let small_result = calculate_pane_layout(PaneDirection::Vertical, 300.0, 200.0, &small_list, &[1.0]);
		assert_eq!(small_result.child_rect_list[0], Some([0.0, 0.0, 300.0, 50.0]));
		assert_eq!(small_result.child_rect_list[1], Some([0.0, 51.0, 300.0, 149.0]));
	}

	//==============================================================================
	/// 끌기: 최소 크기 안으로 묶고, 앞이 flex 고 뒤가 고정이면 뒤를 바꾼다. 리사이저 두께.
	//==============================================================================
	#[test]
	fn resize_matches_original_rules() {
		assert_eq!(calculate_resized_size(0.0, 1000.0, 5.0, 300.0, 2.0, 170.0, 200.0, false, true), (false, 298.0));
		assert_eq!(calculate_resized_size(0.0, 1000.0, 5.0, 10.0, 0.0, 170.0, 200.0, false, true), (false, 170.0));
		assert_eq!(calculate_resized_size(0.0, 1000.0, 5.0, 990.0, 0.0, 170.0, 200.0, false, true), (false, 795.0));
		assert_eq!(calculate_resized_size(100.0, 1000.0, 5.0, 600.0, 0.0, 50.0, 50.0, true, false), (true, 395.0));
		let edges = PaneResizableEdges::default();
		assert_eq!(calculate_resizer_thickness(PaneDirection::Horizontal, true, true, edges, true, edges), (true, 5.0));
		let no_right_edges = PaneResizableEdges { right: false, ..edges };
		assert_eq!(calculate_resizer_thickness(PaneDirection::Horizontal, true, true, no_right_edges, true, edges), (false, 1.0));
		assert_eq!(calculate_resizer_thickness(PaneDirection::Vertical, true, false, edges, true, edges), (false, 1.0));
	}
}

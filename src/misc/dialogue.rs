//==============================================================================
//! 대사 진행기와 대본 파서.
//! - 원본 misc/dialogue.js 를 옮긴 것이다. (DialogueRunner, DialogueScriptParser)
//! - DialogueRunner: 대사, 선택지, 라벨, 점프로 된 대본을 차례대로 풀어 간다. 비주얼노벨 / RPG 대화 / 튜토리얼이 제각각 만들던 상태기계다.
//!   ```text
//!     runner.set_script(entries); runner.start(None);
//!     runner.get_current()   → DialogueCurrent::Line / choice / finished
//!     대사에서 runner.advance(), 선택지에서 runner.choose(번호).
//!   ```
//! - DialogueScriptParser: 한 줄 명령어 글을 대본으로 바꾼다.
//!   ```text
//!     /label 이름표
//!     /jump 이름표
//!     /speaker 화자이름              (이후 /say 의 기본 화자)
//!     /say 대사 내용
//!     /option 보기 글 -> 이름표       (이어 쓴 /option 은 선택지 하나로 묶인다)
//!     /end
//!   ```
//!   "/" 로 시작하지 않는 줄은 바로 앞 화자의 /say 로 본다. "//" 는 주석.
//! - 원본과 다른 점:
//!   대본 항목은 여러 필드를 가진 객체 대신 DialogueEntry 열거형이다. 한 항목이 라벨이면서 대사일 수는 없다.
//!   get_current 는 { kind, ... } 객체 대신 DialogueCurrent 열거형을 돌려주고, 선택지의 options 는 보기 글 목록(`Vec<String>`)이다.
//==============================================================================
use std::collections::HashMap;

//==============================================================================
/// 선택지의 보기 하나. (원본 { text, jump? })
///
/// # 예제
/// 대본을 항목으로 바로 짓고 점프 없는 보기 고르기
/// ```rust
/// use vanilla::{DialogueCurrent, DialogueEntry, DialogueOption, DialogueRunner};
///
/// let entry_list = vec![
/// 	DialogueEntry::Choice(vec![
/// 		DialogueOption { text: String::from("Yes"), jump: Some(String::from("yes")) },
/// 		DialogueOption { text: String::from("No"), jump: None },
/// 	]),
/// 	DialogueEntry::Line { speaker: String::from("Guide"), text: String::from("Maybe later.") },
/// 	DialogueEntry::End,
/// 	DialogueEntry::Label(String::from("yes")),
/// 	DialogueEntry::Line { speaker: String::from("Guide"), text: String::from("Great!") },
/// ];
/// let mut dialogue_runner = DialogueRunner::new();
/// dialogue_runner.set_script(entry_list);
/// dialogue_runner.start(None);
/// dialogue_runner.choose(1);
/// assert_eq!(dialogue_runner.get_current(), DialogueCurrent::Line { speaker: String::from("Guide"), text: String::from("Maybe later.") });
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogueOption {
	/// 보기 글.
	pub text: String,
	/// 고르면 옮겨 갈 라벨 이름. None 이거나 모르는 라벨이면 선택지 다음 항목으로 간다.
	pub jump: Option<String>,
}

//==============================================================================
/// 대본 항목. (원본 { speaker, text } / { label } / { jump } / { choice: `[...]` } / { end: true })
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogueEntry {
	/// 대사. (화자 이름과 대사 글, 화자가 없으면 빈 글)
	Line {
		speaker: String,
		text: String,
	},
	/// 라벨. (점프할 자리 이름, 진행할 때는 건너뛴다)
	Label(String),
	/// 점프. (그 이름의 라벨 다음으로 옮긴다, 모르는 라벨이면 다음 항목으로)
	Jump(String),
	/// 선택지. (보기 목록, choose 로 고를 때까지 머문다)
	Choice(Vec<DialogueOption>),
	/// 대본 끝. (여기 닿으면 끝난다)
	End,
}

//==============================================================================
/// 지금 보여 줄 것. (원본 { kind: "line" | "choice" | "finished", ... })
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogueCurrent {
	/// 대사를 보여 줄 차례. (advance 로 넘긴다)
	Line {
		speaker: String,
		text: String,
	},
	/// 선택지를 보여 줄 차례. (보기 글 목록, choose 로 고른다)
	Choice {
		options: Vec<String>,
	},
	/// 끝났거나 시작하지 않았다.
	Finished,
}

//==============================================================================
/// 대사 진행기.
///
/// # 예제
/// 대본을 풀어 가며 대사를 넘기고 보기를 고르기
/// ```rust
/// use vanilla::{DialogueCurrent, DialogueRunner, DialogueScriptParser};
///
/// let script_text = "
/// /speaker Clerk
/// /say Welcome.
/// /option Go on an adventure -> adventure
/// /option Go home
/// /label home
/// /say Goodbye.
/// /end
/// /label adventure
/// /say Let's go!
/// ";
/// let mut dialogue_runner = DialogueRunner::new();
/// dialogue_runner.set_script(DialogueScriptParser::parse(script_text));
/// dialogue_runner.start(None);
/// assert_eq!(dialogue_runner.get_current(), DialogueCurrent::Line { speaker: String::from("Clerk"), text: String::from("Welcome.") });
///
/// dialogue_runner.advance();
/// assert_eq!(dialogue_runner.get_current(), DialogueCurrent::Choice { options: vec![String::from("Go on an adventure"), String::from("Go home")] });
///
/// dialogue_runner.choose(0);
/// assert_eq!(dialogue_runner.get_current(), DialogueCurrent::Line { speaker: String::from("Clerk"), text: String::from("Let's go!") });
///
/// dialogue_runner.advance();
/// assert!(dialogue_runner.is_finished());
/// assert_eq!(dialogue_runner.get_current(), DialogueCurrent::Finished);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct DialogueRunner {
	entry_list: Vec<DialogueEntry>,
	label_table: HashMap<String, usize>,
	cursor: usize,
	is_finished: bool,
}

impl DialogueRunner {
	//==============================================================================
	/// 생성. (빈 대본, 끝난 상태)
	///
	/// # 돌려줌
	/// 대본이 없는 진행기. (set_script 와 start 로 시작한다)
	//==============================================================================
	pub fn new() -> DialogueRunner {
		DialogueRunner {
			entry_list: Vec::new(),
			label_table: HashMap::new(),
			cursor: 0,
			is_finished: true,
		}
	}

	//==============================================================================
	/// 대본 설정. (라벨 자리를 표로 만들고, 끝난 상태로 둔다. 같은 라벨이 여럿이면 뒤의 것)
	///
	/// # 인자
	/// - `entry_list`: 대본 항목 목록. (DialogueScriptParser::parse 의 결과나 손으로 지은 목록)
	//==============================================================================
	pub fn set_script(&mut self, entry_list: Vec<DialogueEntry>) {
		self.entry_list = entry_list;
		self.label_table.clear();
		for (entry_index, entry) in self.entry_list.iter().enumerate() {
			if let DialogueEntry::Label(label_name) = entry {
				self.label_table.insert(label_name.clone(), entry_index);
			}
		}
		self.cursor = 0;
		self.is_finished = true;
	}

	//==============================================================================
	/// 시작. (라벨을 넘기면 그 자리부터, 없는 라벨이면 처음부터)
	/// - 라벨과 점프를 건너 보여 줄 첫 대사나 선택지에 선다. 대본이 비었거나 그런 항목이 없으면 끝난 상태다.
	///
	/// # 인자
	/// - `label_name`: 시작할 라벨 이름. None 이면 처음부터.
	///
	/// # 예제
	/// 라벨에서 시작하기
	/// ```rust
	/// use vanilla::{DialogueCurrent, DialogueRunner, DialogueScriptParser};
	///
	/// let mut dialogue_runner = DialogueRunner::new();
	/// dialogue_runner.set_script(DialogueScriptParser::parse("/say First.\n/label second\n/say Second."));
	/// dialogue_runner.start(Some("second"));
	/// assert_eq!(dialogue_runner.get_current(), DialogueCurrent::Line { speaker: String::new(), text: String::from("Second.") });
	///
	/// dialogue_runner.start(Some("missing"));
	/// assert_eq!(dialogue_runner.get_current(), DialogueCurrent::Line { speaker: String::new(), text: String::from("First.") });
	/// ```
	//==============================================================================
	pub fn start(&mut self, label_name: Option<&str>) {
		self.is_finished = self.entry_list.is_empty();
		self.cursor = 0;
		if let Some(label_name) = label_name {
			if let Some(label_index) = self.label_table.get(label_name) {
				self.cursor = *label_index;
			}
		}
		self.skip_to_presentable();
	}

	//==============================================================================
	/// 지금 항목 반환.
	///
	/// # 돌려줌
	/// 지금 보여 줄 대사나 선택지(보기 글 목록, 복사본). 끝났으면 DialogueCurrent::Finished.
	//==============================================================================
	pub fn get_current(&self) -> DialogueCurrent {
		if self.is_finished {
			return DialogueCurrent::Finished;
		}
		match self.entry_list.get(self.cursor) {
			Some(DialogueEntry::Choice(option_list)) => {
				let options: Vec<String> = option_list.iter().map(|option| option.text.clone()).collect();
				DialogueCurrent::Choice {
					options,
				}
			}
			Some(DialogueEntry::Line { speaker, text }) => {
				DialogueCurrent::Line {
					speaker: speaker.clone(),
					text: text.clone(),
				}
			}
			_ => {
				DialogueCurrent::Finished
			}
		}
	}

	//==============================================================================
	/// 다음으로. (대사 항목에서 부른다. 선택지 위에서는 무시)
	//==============================================================================
	pub fn advance(&mut self) {
		if self.is_finished {
			return;
		}
		if let Some(DialogueEntry::Choice(_)) = self.entry_list.get(self.cursor) {
			return;
		}
		self.cursor += 1;
		self.skip_to_presentable();
	}

	//==============================================================================
	/// 선택. (선택지 항목에서 부른다. 보기의 점프 라벨이 있으면 거기로, 없거나 모르는 라벨이면 다음 항목으로)
	/// - 끝났거나 지금 항목이 선택지가 아니면 무시한다.
	///
	/// # 인자
	/// - `option_index`: 고를 보기 번호. (0 부터, 보기 수 이상이면 무시)
	//==============================================================================
	pub fn choose(&mut self, option_index: usize) {
		if self.is_finished {
			return;
		}
		let jump_label = match self.entry_list.get(self.cursor) {
			Some(DialogueEntry::Choice(option_list)) => {
				match option_list.get(option_index) {
					Some(option) => {
						option.jump.clone()
					}
					None => {
						return;
					}
				}
			}
			_ => {
				return;
			}
		};
		let label_index = jump_label.and_then(|jump_label| self.label_table.get(&jump_label).copied());
		match label_index {
			Some(label_index) => {
				self.cursor = label_index;
			}
			None => {
				self.cursor += 1;
			}
		}
		self.skip_to_presentable();
	}

	//==============================================================================
	/// 보여 줄 수 있는 항목까지 옮기기. (라벨은 건너뛰고, 점프는 라벨 다음으로, 끝이면 끝낸다)
	/// - 라벨 / 점프만 맴도는 잘못된 대본은 (항목 수 + 8) 번 돌고 끝낸다.
	//==============================================================================
	fn skip_to_presentable(&mut self) {
		let mut guard_count = self.entry_list.len() + 8;
		while guard_count > 0 {
			guard_count -= 1;
			let entry = match self.entry_list.get(self.cursor) {
				Some(entry) => {
					entry
				}
				None => {
					self.is_finished = true;
					return;
				}
			};
			match entry {
				DialogueEntry::End => {
					self.is_finished = true;
					return;
				}
				DialogueEntry::Label(_) => {
					self.cursor += 1;
					continue;
				}
				DialogueEntry::Jump(jump_label) => {
					match self.label_table.get(jump_label) {
						Some(label_index) => {
							self.cursor = *label_index + 1;
						}
						None => {
							self.cursor += 1;
						}
					}
					continue;
				}
				_ => {
					self.is_finished = false;
					return;
				}
			}
		}
		self.is_finished = true;
	}

	//==============================================================================
	/// 끝났는지.
	///
	/// # 돌려줌
	/// /end 나 대본 끝에 닿았거나 아직 시작하지 않았으면 true.
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.is_finished
	}
}

impl Default for DialogueRunner {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> DialogueRunner {
		DialogueRunner::new()
	}
}

//==============================================================================
/// 대본 파서. (정적 함수 묶음)
///
/// # 예제
/// 한 줄 명령어 글을 대본 항목으로
/// ```rust
/// use vanilla::{DialogueEntry, DialogueOption, DialogueScriptParser};
///
/// let entry_list = DialogueScriptParser::parse("/speaker Guide\nHello.\n// comment\n/option Next -> next\n/option Stay\n/label next\n/end");
/// assert_eq!(entry_list, vec![
/// 	DialogueEntry::Line { speaker: String::from("Guide"), text: String::from("Hello.") },
/// 	DialogueEntry::Choice(vec![
/// 		DialogueOption { text: String::from("Next"), jump: Some(String::from("next")) },
/// 		DialogueOption { text: String::from("Stay"), jump: None },
/// 	]),
/// 	DialogueEntry::Label(String::from("next")),
/// 	DialogueEntry::End,
/// ]);
/// ```
//==============================================================================
pub struct DialogueScriptParser;

impl DialogueScriptParser {
	//==============================================================================
	/// 해석. (줄마다 앞뒤 공백을 떼고 명령어를 본다. 모르는 명령어는 건너뛴다)
	/// - 빈 줄과 "//" 로 시작하는 줄은 건너뛴다. 이어 쓴 /option 줄은 다른 줄이 오거나 글이 끝날 때 선택지 하나로 묶는다.
	///
	/// # 인자
	/// - `script_text`: 대본 글. (줄은 `'\n'` 으로 나눈다)
	///
	/// # 돌려줌
	/// 대본 항목 목록. (/speaker 는 항목을 만들지 않고 뒤 대사의 화자만 바꾼다)
	//==============================================================================
	pub fn parse(script_text: &str) -> Vec<DialogueEntry> {
		let mut entry_list: Vec<DialogueEntry> = Vec::new();
		let mut current_speaker = String::new();
		let mut pending_choice: Option<Vec<DialogueOption>> = None;
		for raw_line in script_text.split('\n') {
			let line = raw_line.trim();
			if line.is_empty() || line.starts_with("//") {
				continue;
			}
			if let Some(body) = line.strip_prefix("/option ") {
				let option = match body.find("->") {
					Some(arrow_index) => {
						DialogueOption {
							text: body[..arrow_index].trim().to_string(),
							jump: Some(body[arrow_index + 2..].trim().to_string()),
						}
					}
					None => {
						DialogueOption {
							text: body.to_string(),
							jump: None,
						}
					}
				};
				pending_choice.get_or_insert_with(Vec::new).push(option);
				continue;
			}

			// 선택지가 아닌 줄이 오면 모아 둔 보기를 선택지 하나로 묶는다.
			if let Some(option_list) = pending_choice.take() {
				entry_list.push(DialogueEntry::Choice(option_list));
			}
			if let Some(label_name) = line.strip_prefix("/label ") {
				entry_list.push(DialogueEntry::Label(label_name.trim().to_string()));
			}
			else if let Some(jump_label) = line.strip_prefix("/jump ") {
				entry_list.push(DialogueEntry::Jump(jump_label.trim().to_string()));
			}
			else if let Some(speaker_name) = line.strip_prefix("/speaker ") {
				current_speaker = speaker_name.trim().to_string();
			}
			else if let Some(say_text) = line.strip_prefix("/say ") {
				entry_list.push(DialogueEntry::Line {
					speaker: current_speaker.clone(),
					text: say_text.to_string(),
				});
			}
			else if line == "/end" {
				entry_list.push(DialogueEntry::End);
			}
			else if line.starts_with('/') {
				// 모르는 명령어는 건너뛴다.
				continue;
			}
			else {
				entry_list.push(DialogueEntry::Line {
					speaker: current_speaker.clone(),
					text: line.to_string(),
				});
			}
		}
		if let Some(option_list) = pending_choice.take() {
			entry_list.push(DialogueEntry::Choice(option_list));
		}
		entry_list
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	/// 시험용 대본.
	const TEST_SCRIPT: &str = "
// 주석은 건너뛴다.
/speaker 점원
/say 안녕하세요.
어서 오세요.
/option 모험을 떠난다 -> adventure
/option 집에 간다
/unknown 무시
/label home
/say 잘 가요.
/end
/label adventure
/speaker 용사
/say 가자!
/jump home
";

	//==============================================================================
	/// 명령어 해석. (화자 이어 받기, 선택지 묶기, 모르는 명령어 무시)
	//==============================================================================
	#[test]
	fn parse_builds_entries() {
		let entry_list = DialogueScriptParser::parse(TEST_SCRIPT);
		assert_eq!(entry_list.len(), 9);
		assert_eq!(entry_list[0], DialogueEntry::Line {
			speaker: String::from("점원"),
			text: String::from("안녕하세요."),
		});
		assert_eq!(entry_list[1], DialogueEntry::Line {
			speaker: String::from("점원"),
			text: String::from("어서 오세요."),
		});
		assert_eq!(entry_list[2], DialogueEntry::Choice(vec![
			DialogueOption {
				text: String::from("모험을 떠난다"),
				jump: Some(String::from("adventure")),
			},
			DialogueOption {
				text: String::from("집에 간다"),
				jump: None,
			},
		]));
		assert_eq!(entry_list[3], DialogueEntry::Label(String::from("home")));
		assert_eq!(entry_list[5], DialogueEntry::End);
		assert_eq!(entry_list[8], DialogueEntry::Jump(String::from("home")));
	}

	//==============================================================================
	/// 진행: 대사 넘기기, 선택지에서 advance 무시, 점프, 끝.
	//==============================================================================
	#[test]
	fn runner_follows_choices_and_jumps() {
		let mut runner = DialogueRunner::new();
		runner.set_script(DialogueScriptParser::parse(TEST_SCRIPT));
		assert_eq!(runner.get_current(), DialogueCurrent::Finished);
		runner.start(None);
		assert_eq!(runner.get_current(), DialogueCurrent::Line {
			speaker: String::from("점원"),
			text: String::from("안녕하세요."),
		});
		runner.advance();
		runner.advance();
		assert_eq!(runner.get_current(), DialogueCurrent::Choice {
			options: vec![String::from("모험을 떠난다"), String::from("집에 간다")],
		});
		runner.advance();
		assert!(matches!(runner.get_current(), DialogueCurrent::Choice { .. }));
		runner.choose(5);
		assert!(matches!(runner.get_current(), DialogueCurrent::Choice { .. }));

		runner.choose(0);
		assert_eq!(runner.get_current(), DialogueCurrent::Line {
			speaker: String::from("용사"),
			text: String::from("가자!"),
		});
		runner.advance();
		assert_eq!(runner.get_current(), DialogueCurrent::Line {
			speaker: String::from("점원"),
			text: String::from("잘 가요."),
		});
		runner.advance();
		assert!(runner.is_finished());
		assert_eq!(runner.get_current(), DialogueCurrent::Finished);

		// 라벨부터 시작, 점프 없는 보기는 다음 항목.
		runner.start(Some("home"));
		assert_eq!(runner.get_current(), DialogueCurrent::Line {
			speaker: String::from("점원"),
			text: String::from("잘 가요."),
		});
		runner.start(None);
		runner.advance();
		runner.advance();
		runner.choose(1);
		assert_eq!(runner.get_current(), DialogueCurrent::Line {
			speaker: String::from("점원"),
			text: String::from("잘 가요."),
		});
	}

	//==============================================================================
	/// 점프만 맴도는 대본과 빈 대본은 끝난다.
	//==============================================================================
	#[test]
	fn looping_script_finishes() {
		let mut runner = DialogueRunner::new();
		runner.set_script(vec![DialogueEntry::Label(String::from("loop")), DialogueEntry::Jump(String::from("loop"))]);
		runner.start(None);
		assert!(runner.is_finished());
		runner.set_script(Vec::new());
		runner.start(None);
		assert!(runner.is_finished());
	}
}

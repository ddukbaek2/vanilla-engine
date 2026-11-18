//==============================================================================
// 대사 진행기와 대본 파서.
// - 원본 misc/dialogue.js 를 옮긴 것이다. (DialogueRunner, DialogueScriptParser)
// - DialogueRunner: 대사, 선택지, 라벨, 점프로 된 대본을 차례대로 풀어 간다. 비주얼노벨 / RPG 대화 / 튜토리얼이 제각각 만들던 상태기계다.
//     runner.setScript(entries); runner.start(None);
//     runner.getCurrent()   → DialogueCurrent::line / choice / finished
//     대사에서 runner.advance(), 선택지에서 runner.choose(번호).
// - DialogueScriptParser: 한 줄 명령어 글을 대본으로 바꾼다.
//     /label 이름표
//     /jump 이름표
//     /speaker 화자이름              (이후 /say 의 기본 화자)
//     /say 대사 내용
//     /option 보기 글 -> 이름표       (이어 쓴 /option 은 선택지 하나로 묶인다)
//     /end
//   "/" 로 시작하지 않는 줄은 바로 앞 화자의 /say 로 본다. "//" 는 주석.
// - 원본과 다른 점:
//   대본 항목은 여러 필드를 가진 객체 대신 DialogueEntry 열거형이다. 한 항목이 라벨이면서 대사일 수는 없다.
//   getCurrent 는 { kind, ... } 객체 대신 DialogueCurrent 열거형을 돌려주고, 선택지의 options 는 보기 글 목록(Vec<String>)이다.
//==============================================================================
use std::collections::HashMap;

//==============================================================================
// 선택지의 보기 하나. (원본 { text, jump? })
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogueOption {
	pub text: String,
	pub jump: Option<String>,
}

//==============================================================================
// 대본 항목. (원본 { speaker, text } / { label } / { jump } / { choice: [...] } / { end: true })
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogueEntry {
	line {
		speaker: String,
		text: String,
	},
	label(String),
	jump(String),
	choice(Vec<DialogueOption>),
	end,
}

//==============================================================================
// 지금 보여 줄 것. (원본 { kind: "line" | "choice" | "finished", ... })
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogueCurrent {
	line {
		speaker: String,
		text: String,
	},
	choice {
		options: Vec<String>,
	},
	finished,
}

//==============================================================================
// 대사 진행기.
//==============================================================================
#[derive(Clone, Debug)]
pub struct DialogueRunner {
	entryList: Vec<DialogueEntry>,
	labelTable: HashMap<String, usize>,
	cursor: usize,
	isFinished: bool,
}

impl DialogueRunner {
	//==============================================================================
	// 생성. (빈 대본, 끝난 상태)
	//==============================================================================
	pub fn new() -> DialogueRunner {
		DialogueRunner {
			entryList: Vec::new(),
			labelTable: HashMap::new(),
			cursor: 0,
			isFinished: true,
		}
	}

	//==============================================================================
	// 대본 설정. (라벨 자리를 표로 만들고, 끝난 상태로 둔다. 같은 라벨이 여럿이면 뒤의 것)
	//==============================================================================
	pub fn setScript(&mut self, entryList: Vec<DialogueEntry>) {
		self.entryList = entryList;
		self.labelTable.clear();
		for (entryIndex, entry) in self.entryList.iter().enumerate() {
			if let DialogueEntry::label(labelName) = entry {
				self.labelTable.insert(labelName.clone(), entryIndex);
			}
		}
		self.cursor = 0;
		self.isFinished = true;
	}

	//==============================================================================
	// 시작. (라벨을 넘기면 그 자리부터, 없는 라벨이면 처음부터)
	//==============================================================================
	pub fn start(&mut self, labelName: Option<&str>) {
		self.isFinished = self.entryList.is_empty();
		self.cursor = 0;
		if let Some(labelName) = labelName {
			if let Some(labelIndex) = self.labelTable.get(labelName) {
				self.cursor = *labelIndex;
			}
		}
		self.skipToPresentable();
	}

	//==============================================================================
	// 지금 항목 반환.
	//==============================================================================
	pub fn getCurrent(&self) -> DialogueCurrent {
		if self.isFinished {
			return DialogueCurrent::finished;
		}
		match self.entryList.get(self.cursor) {
			Some(DialogueEntry::choice(optionList)) => {
				let options: Vec<String> = optionList.iter().map(|option| option.text.clone()).collect();
				DialogueCurrent::choice {
					options,
				}
			}
			Some(DialogueEntry::line { speaker, text }) => {
				DialogueCurrent::line {
					speaker: speaker.clone(),
					text: text.clone(),
				}
			}
			_ => {
				DialogueCurrent::finished
			}
		}
	}

	//==============================================================================
	// 다음으로. (대사 항목에서 부른다. 선택지 위에서는 무시)
	//==============================================================================
	pub fn advance(&mut self) {
		if self.isFinished {
			return;
		}
		if let Some(DialogueEntry::choice(_)) = self.entryList.get(self.cursor) {
			return;
		}
		self.cursor += 1;
		self.skipToPresentable();
	}

	//==============================================================================
	// 선택. (선택지 항목에서 부른다. 보기의 점프 라벨이 있으면 거기로, 없거나 모르는 라벨이면 다음 항목으로)
	//==============================================================================
	pub fn choose(&mut self, optionIndex: usize) {
		if self.isFinished {
			return;
		}
		let jumpLabel = match self.entryList.get(self.cursor) {
			Some(DialogueEntry::choice(optionList)) => {
				match optionList.get(optionIndex) {
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
		let labelIndex = jumpLabel.and_then(|jumpLabel| self.labelTable.get(&jumpLabel).copied());
		match labelIndex {
			Some(labelIndex) => {
				self.cursor = labelIndex;
			}
			None => {
				self.cursor += 1;
			}
		}
		self.skipToPresentable();
	}

	//==============================================================================
	// 보여 줄 수 있는 항목까지 옮기기. (라벨은 건너뛰고, 점프는 라벨 다음으로, 끝이면 끝낸다)
	// - 라벨 / 점프만 맴도는 잘못된 대본은 (항목 수 + 8) 번 돌고 끝낸다.
	//==============================================================================
	fn skipToPresentable(&mut self) {
		let mut guardCount = self.entryList.len() + 8;
		while guardCount > 0 {
			guardCount -= 1;
			let entry = match self.entryList.get(self.cursor) {
				Some(entry) => {
					entry
				}
				None => {
					self.isFinished = true;
					return;
				}
			};
			match entry {
				DialogueEntry::end => {
					self.isFinished = true;
					return;
				}
				DialogueEntry::label(_) => {
					self.cursor += 1;
					continue;
				}
				DialogueEntry::jump(jumpLabel) => {
					match self.labelTable.get(jumpLabel) {
						Some(labelIndex) => {
							self.cursor = *labelIndex + 1;
						}
						None => {
							self.cursor += 1;
						}
					}
					continue;
				}
				_ => {
					self.isFinished = false;
					return;
				}
			}
		}
		self.isFinished = true;
	}

	//==============================================================================
	// 끝났는지.
	//==============================================================================
	pub fn isFinished(&self) -> bool {
		self.isFinished
	}
}

impl Default for DialogueRunner {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> DialogueRunner {
		DialogueRunner::new()
	}
}

//==============================================================================
// 대본 파서. (정적 함수 묶음)
//==============================================================================
pub struct DialogueScriptParser;

impl DialogueScriptParser {
	//==============================================================================
	// 해석. (줄마다 앞뒤 공백을 떼고 명령어를 본다. 모르는 명령어는 건너뛴다)
	//==============================================================================
	pub fn parse(scriptText: &str) -> Vec<DialogueEntry> {
		let mut entryList: Vec<DialogueEntry> = Vec::new();
		let mut currentSpeaker = String::new();
		let mut pendingChoice: Option<Vec<DialogueOption>> = None;
		for rawLine in scriptText.split('\n') {
			let line = rawLine.trim();
			if line.is_empty() || line.starts_with("//") {
				continue;
			}
			if let Some(body) = line.strip_prefix("/option ") {
				let option = match body.find("->") {
					Some(arrowIndex) => {
						DialogueOption {
							text: body[..arrowIndex].trim().to_string(),
							jump: Some(body[arrowIndex + 2..].trim().to_string()),
						}
					}
					None => {
						DialogueOption {
							text: body.to_string(),
							jump: None,
						}
					}
				};
				pendingChoice.get_or_insert_with(Vec::new).push(option);
				continue;
			}

			// 선택지가 아닌 줄이 오면 모아 둔 보기를 선택지 하나로 묶는다.
			if let Some(optionList) = pendingChoice.take() {
				entryList.push(DialogueEntry::choice(optionList));
			}
			if let Some(labelName) = line.strip_prefix("/label ") {
				entryList.push(DialogueEntry::label(labelName.trim().to_string()));
			}
			else if let Some(jumpLabel) = line.strip_prefix("/jump ") {
				entryList.push(DialogueEntry::jump(jumpLabel.trim().to_string()));
			}
			else if let Some(speakerName) = line.strip_prefix("/speaker ") {
				currentSpeaker = speakerName.trim().to_string();
			}
			else if let Some(sayText) = line.strip_prefix("/say ") {
				entryList.push(DialogueEntry::line {
					speaker: currentSpeaker.clone(),
					text: sayText.to_string(),
				});
			}
			else if line == "/end" {
				entryList.push(DialogueEntry::end);
			}
			else if line.starts_with('/') {
				// 모르는 명령어는 건너뛴다.
				continue;
			}
			else {
				entryList.push(DialogueEntry::line {
					speaker: currentSpeaker.clone(),
					text: line.to_string(),
				});
			}
		}
		if let Some(optionList) = pendingChoice.take() {
			entryList.push(DialogueEntry::choice(optionList));
		}
		entryList
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	// 시험용 대본.
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
	// 명령어 해석. (화자 이어 받기, 선택지 묶기, 모르는 명령어 무시)
	//==============================================================================
	#[test]
	fn parseBuildsEntries() {
		let entryList = DialogueScriptParser::parse(TEST_SCRIPT);
		assert_eq!(entryList.len(), 9);
		assert_eq!(entryList[0], DialogueEntry::line {
			speaker: String::from("점원"),
			text: String::from("안녕하세요."),
		});
		assert_eq!(entryList[1], DialogueEntry::line {
			speaker: String::from("점원"),
			text: String::from("어서 오세요."),
		});
		assert_eq!(entryList[2], DialogueEntry::choice(vec![
			DialogueOption {
				text: String::from("모험을 떠난다"),
				jump: Some(String::from("adventure")),
			},
			DialogueOption {
				text: String::from("집에 간다"),
				jump: None,
			},
		]));
		assert_eq!(entryList[3], DialogueEntry::label(String::from("home")));
		assert_eq!(entryList[5], DialogueEntry::end);
		assert_eq!(entryList[8], DialogueEntry::jump(String::from("home")));
	}

	//==============================================================================
	// 진행: 대사 넘기기, 선택지에서 advance 무시, 점프, 끝.
	//==============================================================================
	#[test]
	fn runnerFollowsChoicesAndJumps() {
		let mut runner = DialogueRunner::new();
		runner.setScript(DialogueScriptParser::parse(TEST_SCRIPT));
		assert_eq!(runner.getCurrent(), DialogueCurrent::finished);
		runner.start(None);
		assert_eq!(runner.getCurrent(), DialogueCurrent::line {
			speaker: String::from("점원"),
			text: String::from("안녕하세요."),
		});
		runner.advance();
		runner.advance();
		assert_eq!(runner.getCurrent(), DialogueCurrent::choice {
			options: vec![String::from("모험을 떠난다"), String::from("집에 간다")],
		});
		runner.advance();
		assert!(matches!(runner.getCurrent(), DialogueCurrent::choice { .. }));
		runner.choose(5);
		assert!(matches!(runner.getCurrent(), DialogueCurrent::choice { .. }));

		runner.choose(0);
		assert_eq!(runner.getCurrent(), DialogueCurrent::line {
			speaker: String::from("용사"),
			text: String::from("가자!"),
		});
		runner.advance();
		assert_eq!(runner.getCurrent(), DialogueCurrent::line {
			speaker: String::from("점원"),
			text: String::from("잘 가요."),
		});
		runner.advance();
		assert!(runner.isFinished());
		assert_eq!(runner.getCurrent(), DialogueCurrent::finished);

		// 라벨부터 시작, 점프 없는 보기는 다음 항목.
		runner.start(Some("home"));
		assert_eq!(runner.getCurrent(), DialogueCurrent::line {
			speaker: String::from("점원"),
			text: String::from("잘 가요."),
		});
		runner.start(None);
		runner.advance();
		runner.advance();
		runner.choose(1);
		assert_eq!(runner.getCurrent(), DialogueCurrent::line {
			speaker: String::from("점원"),
			text: String::from("잘 가요."),
		});
	}

	//==============================================================================
	// 점프만 맴도는 대본과 빈 대본은 끝난다.
	//==============================================================================
	#[test]
	fn loopingScriptFinishes() {
		let mut runner = DialogueRunner::new();
		runner.setScript(vec![DialogueEntry::label(String::from("loop")), DialogueEntry::jump(String::from("loop"))]);
		runner.start(None);
		assert!(runner.isFinished());
		runner.setScript(Vec::new());
		runner.start(None);
		assert!(runner.isFinished());
	}
}

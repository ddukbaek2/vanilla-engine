//==============================================================================
// 텍스트 컴포넌트. (노드 contentSize 영역 안에 정렬해 글자를 그린다)
// - 한 줄, 그리고 줄바꿈 문자나 wordWrapWidth 가 있으면 여러 줄로 그린다.
// - 원본에서 옮기지 않은 것: 밑줄 / 취소선, 오른쪽에서 왼쪽 방향, 기울임(italic 은 값만 든다).
// - 글자 수만큼 보이기(setVisibleCharacterCount, 타자기 연출)는 원본처럼 앞에서부터 자른다. 원본은 UTF-16 단위로 세고, 여기서는 글자(char) 단위로 센다.
//   여러 줄일 때 원본은 남은 수가 음수로 떨어지면 다음 줄을 통째로 그린다. 여기서는 0 에서 멈춰 다음 줄을 그리지 않는다.
//==============================================================================
use crate::base::color::Color;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::node::worldnode::NodeTransform;

// 폰트 이름을 정하지 않았을 때 쓰는 이름. (등록된 이름이 없으면 엔진 기본 폰트로 그린다)
const DEFAULT_FONT_FAMILY: &str = "default";

pub struct Text {
	componentBase: ComponentBase,
	fontFamily: String,
	text: String,
	fontSize: f32,
	textColor: Color,
	strokeColor: Color,
	strokeWidth: f32,
	isBold: bool,
	isItalic: bool,
	textAlign: TextAlign,
	textBaseline: TextBaseline,
	wordWrapWidth: f32,
	lineSpacing: f32,
	visibleCharacterCount: i32,
}

impl Text {
	//==============================================================================
	// 생성. (기본값은 원본과 같다: 32px, 흰색, 가운데 정렬, 가운데 기준선, 줄 간격 1.25)
	//==============================================================================
	pub fn new() -> Text {
		Text {
			componentBase: ComponentBase::new(),
			fontFamily: String::from(DEFAULT_FONT_FAMILY),
			text: String::new(),
			fontSize: 32.0,
			textColor: Color::white(),
			strokeColor: Color::white(),
			strokeWidth: 0.0,
			isBold: false,
			isItalic: false,
			textAlign: TextAlign::center,
			textBaseline: TextBaseline::middle,
			wordWrapWidth: 0.0,
			lineSpacing: 1.25,
			visibleCharacterCount: -1,
		}
	}

	//==============================================================================
	// 글과 크기로 생성.
	//==============================================================================
	pub fn createWithText(text: &str, fontSize: f32) -> Text {
		let mut textComponent = Text::new();
		textComponent.setText(text);
		textComponent.setFontSize(fontSize);
		textComponent
	}

	//==============================================================================
	// Canvas2D font 꼴 문자열 만들기.
	//==============================================================================
	pub fn buildFontString(&self) -> String {
		let mut styleParts: Vec<String> = Vec::new();
		if self.isItalic {
			styleParts.push(String::from("italic"));
		}
		if self.isBold {
			styleParts.push(String::from("bold"));
		}
		styleParts.push(format!("{}px", self.fontSize));
		styleParts.push(self.fontFamily.clone());
		styleParts.join(" ")
	}

	//==============================================================================
	// 폭에 맞춰 줄 나누기. (공백 단위로 채우고, 한 단어가 폭을 넘으면 글자 단위로 나눈다)
	//==============================================================================
	pub fn wrapTextToLines(&self, graphic: &Graphic, text: &str, maximumWidth: f32) -> Vec<String> {
		let mut lineList: Vec<String> = Vec::new();
		for paragraph in text.split('\n') {
			if maximumWidth <= 0.0 {
				lineList.push(paragraph.to_string());
				continue;
			}
			let mut currentLine = String::new();
			for word in paragraph.split(' ') {
				let candidateLine = if currentLine.is_empty() {
					word.to_string()
				}
				else {
					format!("{} {}", currentLine, word)
				};
				let candidateMetrics = graphic.measureText(&candidateLine);
				if candidateMetrics.width <= maximumWidth {
					currentLine = candidateLine;
					continue;
				}
				if !currentLine.is_empty() {
					lineList.push(currentLine.clone());
					currentLine.clear();
				}

				// 한 단어가 폭을 넘으면 글자 단위로 나눈다.
				let wordMetrics = graphic.measureText(word);
				if wordMetrics.width <= maximumWidth {
					currentLine = word.to_string();
					continue;
				}
				for character in word.chars() {
					let mut characterCandidateLine = currentLine.clone();
					characterCandidateLine.push(character);
					let characterCandidateMetrics = graphic.measureText(&characterCandidateLine);
					if characterCandidateMetrics.width > maximumWidth && !currentLine.is_empty() {
						lineList.push(currentLine.clone());
						currentLine = character.to_string();
					}
					else {
						currentLine = characterCandidateLine;
					}
				}
			}
			lineList.push(currentLine);
		}
		lineList
	}

	//==============================================================================
	// 정렬에 맞는 가로 위치.
	//==============================================================================
	fn calculateDrawX(&self, graphic: &Graphic, contentWidth: f32) -> (f32, TextAlign) {
		let textAlign = graphic.resolveTextAlign(self.textAlign);
		let drawX = match textAlign {
			TextAlign::left => {
				0.0
			}
			TextAlign::right => {
				contentWidth
			}
			_ => {
				contentWidth * 0.5
			}
		};
		(drawX, textAlign)
	}

	//==============================================================================
	// 한 줄 출력.
	//==============================================================================
	fn drawPlainText(&self, graphic: &mut Graphic, contentWidth: f32, contentHeight: f32, text: &str) {
		let (drawX, textAlign) = self.calculateDrawX(graphic, contentWidth);
		let drawY = match self.textBaseline {
			TextBaseline::top | TextBaseline::hanging => {
				0.0
			}
			TextBaseline::bottom | TextBaseline::ideographic | TextBaseline::alphabetic => {
				contentHeight
			}
			TextBaseline::middle => {
				contentHeight * 0.5
			}
		};

		let fontString = self.buildFontString();
		graphic.setFontString(&fontString);
		graphic.setTextAlign(textAlign);
		graphic.setTextBaseline(self.textBaseline);

		if self.strokeWidth > 0.0 {
			graphic.setStrokeColor(self.strokeColor);
			graphic.drawStrokeText(text, drawX, drawY, self.strokeWidth);
		}
		graphic.setFillColor(self.textColor);
		graphic.drawFillText(text, drawX, drawY);
	}

	//==============================================================================
	// 여러 줄 출력.
	//==============================================================================
	fn drawWrappedText(&self, graphic: &mut Graphic, contentWidth: f32, contentHeight: f32, fullText: &str) {
		let fontString = self.buildFontString();
		graphic.setFontString(&fontString);

		let wrapWidth = if self.wordWrapWidth > 0.0 {
			self.wordWrapWidth
		}
		else {
			contentWidth
		};
		let lineList = self.wrapTextToLines(graphic, fullText, wrapWidth);
		let lineHeight = self.fontSize * self.lineSpacing;
		let blockHeight = lineList.len() as f32 * lineHeight;
		let blockTop = match self.textBaseline {
			TextBaseline::top | TextBaseline::hanging => {
				0.0
			}
			TextBaseline::bottom | TextBaseline::ideographic | TextBaseline::alphabetic => {
				contentHeight - blockHeight
			}
			TextBaseline::middle => {
				(contentHeight - blockHeight) * 0.5
			}
		};

		let (drawX, textAlign) = self.calculateDrawX(graphic, contentWidth);
		graphic.setTextAlign(textAlign);
		graphic.setTextBaseline(TextBaseline::middle);

		// 보일 글자 수가 정해졌으면 줄마다 남은 수만큼만 그린다. (원본처럼 나눈 줄의 글자 수로 센다)
		let mut remainCount = self.visibleCharacterCount;
		for (lineIndex, fullLineText) in lineList.iter().enumerate() {
			let mut lineText = fullLineText.as_str();
			if remainCount >= 0 {
				if remainCount == 0 {
					break;
				}
				let lineCharacterCount = fullLineText.chars().count() as i32;
				if lineCharacterCount > remainCount {
					let cutIndex = fullLineText.char_indices().nth(remainCount as usize).map(|(byteIndex, _)| byteIndex).unwrap_or(fullLineText.len());
					lineText = &fullLineText[..cutIndex];
				}
				remainCount = (remainCount - lineCharacterCount).max(0);
			}
			if lineText.is_empty() {
				continue;
			}
			let lineY = blockTop + (lineIndex as f32 + 0.5) * lineHeight;
			if self.strokeWidth > 0.0 {
				graphic.setStrokeColor(self.strokeColor);
				graphic.drawStrokeText(lineText, drawX, lineY, self.strokeWidth);
			}
			graphic.setFillColor(self.textColor);
			graphic.drawFillText(lineText, drawX, lineY);
		}
	}

	//==============================================================================
	// 폰트 이름 설정. (Graphic 에 등록한 이름, 없으면 기본 폰트)
	//==============================================================================
	pub fn setFontFamily(&mut self, fontFamily: &str) {
		self.fontFamily = fontFamily.to_string();
	}

	//==============================================================================
	// 폰트 이름 반환.
	//==============================================================================
	pub fn getFontFamily(&self) -> &str {
		&self.fontFamily
	}

	//==============================================================================
	// 글 설정.
	//==============================================================================
	pub fn setText(&mut self, text: &str) {
		if self.text != text {
			self.text = text.to_string();
		}
	}

	//==============================================================================
	// 글 반환.
	//==============================================================================
	pub fn getText(&self) -> &str {
		&self.text
	}

	//==============================================================================
	// 글자 크기 설정. (픽셀)
	//==============================================================================
	pub fn setFontSize(&mut self, fontSize: f32) {
		self.fontSize = fontSize;
	}

	//==============================================================================
	// 글자 크기 반환.
	//==============================================================================
	pub fn getFontSize(&self) -> f32 {
		self.fontSize
	}

	//==============================================================================
	// 글자 색 설정.
	//==============================================================================
	pub fn setTextColor(&mut self, color: impl Into<Color>) {
		self.textColor = color.into();
	}

	//==============================================================================
	// 글자 색 반환.
	//==============================================================================
	pub fn getTextColor(&self) -> Color {
		self.textColor
	}

	//==============================================================================
	// 외곽선 색 설정.
	//==============================================================================
	pub fn setStrokeColor(&mut self, color: impl Into<Color>) {
		self.strokeColor = color.into();
	}

	//==============================================================================
	// 외곽선 색 반환.
	//==============================================================================
	pub fn getStrokeColor(&self) -> Color {
		self.strokeColor
	}

	//==============================================================================
	// 외곽선 두께 설정. (0 이면 외곽선 없음)
	//==============================================================================
	pub fn setStrokeWidth(&mut self, strokeWidth: f32) {
		self.strokeWidth = strokeWidth;
	}

	//==============================================================================
	// 외곽선 두께 반환.
	//==============================================================================
	pub fn getStrokeWidth(&self) -> f32 {
		self.strokeWidth
	}

	//==============================================================================
	// 굵게 설정.
	//==============================================================================
	pub fn setBold(&mut self, isBold: bool) {
		self.isBold = isBold;
	}

	//==============================================================================
	// 굵게 여부.
	//==============================================================================
	pub fn isBold(&self) -> bool {
		self.isBold
	}

	//==============================================================================
	// 기울임 설정. (값만 든다)
	//==============================================================================
	pub fn setItalic(&mut self, isItalic: bool) {
		self.isItalic = isItalic;
	}

	//==============================================================================
	// 기울임 여부.
	//==============================================================================
	pub fn isItalic(&self) -> bool {
		self.isItalic
	}

	//==============================================================================
	// 가로 정렬 설정.
	//==============================================================================
	pub fn setTextAlign(&mut self, textAlign: TextAlign) {
		self.textAlign = textAlign;
	}

	//==============================================================================
	// 가로 정렬 반환.
	//==============================================================================
	pub fn getTextAlign(&self) -> TextAlign {
		self.textAlign
	}

	//==============================================================================
	// 세로 기준선 설정.
	//==============================================================================
	pub fn setTextBaseline(&mut self, textBaseline: TextBaseline) {
		self.textBaseline = textBaseline;
	}

	//==============================================================================
	// 세로 기준선 반환.
	//==============================================================================
	pub fn getTextBaseline(&self) -> TextBaseline {
		self.textBaseline
	}

	//==============================================================================
	// 줄바꿈 폭 설정. (0 이면 줄바꿈 문자로만 나눈다)
	//==============================================================================
	pub fn setWordWrapWidth(&mut self, wordWrapWidth: f32) {
		self.wordWrapWidth = wordWrapWidth;
	}

	//==============================================================================
	// 줄바꿈 폭 반환.
	//==============================================================================
	pub fn getWordWrapWidth(&self) -> f32 {
		self.wordWrapWidth
	}

	//==============================================================================
	// 줄 간격 설정. (글자 크기의 배수)
	//==============================================================================
	pub fn setLineSpacing(&mut self, lineSpacing: f32) {
		self.lineSpacing = lineSpacing;
	}

	//==============================================================================
	// 줄 간격 반환.
	//==============================================================================
	pub fn getLineSpacing(&self) -> f32 {
		self.lineSpacing
	}

	//==============================================================================
	// 보일 글자 수 설정. (-1 이면 전체, 타자기 연출용. -1 아래는 -1 로 맞춘다)
	//==============================================================================
	pub fn setVisibleCharacterCount(&mut self, visibleCharacterCount: i32) {
		self.visibleCharacterCount = visibleCharacterCount.max(-1);
	}

	//==============================================================================
	// 보일 글자 수 반환.
	//==============================================================================
	pub fn getVisibleCharacterCount(&self) -> i32 {
		self.visibleCharacterCount
	}

	//==============================================================================
	// 보일 글자 수만큼 앞에서 자르기. (-1 이면 그대로)
	//==============================================================================
	fn applyVisibleCharacterCount<'a>(&self, text: &'a str) -> &'a str {
		if self.visibleCharacterCount < 0 {
			return text;
		}
		match text.char_indices().nth(self.visibleCharacterCount as usize) {
			Some((byteIndex, _)) => {
				&text[..byteIndex]
			}
			None => {
				text
			}
		}
	}
}

impl Default for Text {
	fn default() -> Text {
		Text::new()
	}
}

impl Component for Text {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"Text"
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.text.is_empty() {
			return;
		}
		let contentSize = transform.getContentSize();
		if self.wordWrapWidth > 0.0 || self.text.contains('\n') {
			self.drawWrappedText(graphic, contentSize.x, contentSize.y, &self.text);
			return;
		}
		let visibleText = self.applyVisibleCharacterCount(&self.text);
		if visibleText.is_empty() {
			return;
		}
		self.drawPlainText(graphic, contentSize.x, contentSize.y, visibleText);
	}
}

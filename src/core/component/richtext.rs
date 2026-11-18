//==============================================================================
// 리치텍스트 컴포넌트. (원본 core/component/richtext.js 의 RichText, parseMarkupToSegments)
// - 마크업(Unity TMP 와 같은 꼴)을 풀어 조각(segment)마다 폰트, 크기, 색, 굵게, 기울임, 밑줄, 취소선을 달리해 한 줄로 잇는다.
//   지원 태그: <b>, <i>, <u>, <s>, <color=#RRGGBB[AA]>, <color=이름>, <size=N>. 모르는 태그는 글 그대로 보인다.
// - 원본은 Text 를 상속한다. 러스트 판은 Text 를 품고(UILabel 이 Text 를 품는 것과 같은 꼴) 세터를 넘긴다.
//   마크업이 없으면 품은 Text 의 draw 로 그린다. (여러 줄, 줄바꿈 폭 그대로) 조각의 정하지 않은 속성은 Text 의 값을 쓴다.
// - 조각 배치(layoutSegments)는 Graphic 없이 재는 함수를 받아 한다. draw 는 글자 텍스처 캐시(textstringtexturecache)의 measureText 로 잰다.
// - 원본과 다른 것:
//   - 조각 속성 fontFace(FontFace / FontAsset) 는 fontFamily(등록한 폰트 이름) 다.
//   - Text 에 밑줄 / 취소선 / 기울임 그리기가 없어서, 전역(Text) 밑줄과 취소선은 없다. 조각의 <u>, <s> 는 원본처럼 선을 긋는다.
//     <i> 는 폰트 문자열에 italic 을 넣지만 글자 캐시가 기울여 그리지 않는다. (Text 와 같음)
//   - 잘못된 #색(16진수가 아닌 글자)은 흰색이다. (원본은 NaN 색)
//   - getIntrinsicContentSize 는 재는 함수를 받는 calculateIntrinsicContentSize 다.
//==============================================================================
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
// 마크업 색 이름 → HEX. (원본 NAMED_COLORS, Unity TMP 와 호환)
//==============================================================================
const NAMED_COLOR_LIST: [(&str, &str); 22] = [
	("red", "#ff0000"),
	("green", "#00ff00"),
	("blue", "#0000ff"),
	("white", "#ffffff"),
	("black", "#000000"),
	("yellow", "#ffff00"),
	("cyan", "#00ffff"),
	("magenta", "#ff00ff"),
	("gray", "#808080"),
	("grey", "#808080"),
	("orange", "#ffa500"),
	("purple", "#800080"),
	("brown", "#a52a2a"),
	("pink", "#ffc0cb"),
	("silver", "#c0c0c0"),
	("maroon", "#800000"),
	("olive", "#808000"),
	("navy", "#000080"),
	("teal", "#008080"),
	("lime", "#00ff00"),
	("aqua", "#00ffff"),
	("fuchsia", "#ff00ff"),
];

//==============================================================================
// 조각 속성. (None / false 면 Text 의 값을 쓴다)
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichTextAttributes {
	pub textColor: Option<Color>,
	pub fontSize: Option<f32>,
	pub fontFamily: Option<String>,
	pub strokeColor: Option<Color>,
	pub strokeWidth: Option<f32>,
	pub isBold: bool,
	pub isItalic: bool,
	pub isUnderline: bool,
	pub isStrikethrough: bool,
}

impl RichTextAttributes {
	//==============================================================================
	// 생성. (아무것도 정하지 않음)
	//==============================================================================
	pub fn new() -> RichTextAttributes {
		RichTextAttributes::default()
	}

	//==============================================================================
	// 안쪽 속성을 덮어쓰기. (정한 것만, 원본 mergeAttributeStack)
	//==============================================================================
	fn mergeFrom(&mut self, innerAttributes: &RichTextAttributes) {
		if innerAttributes.textColor.is_some() {
			self.textColor = innerAttributes.textColor;
		}
		if innerAttributes.fontSize.is_some() {
			self.fontSize = innerAttributes.fontSize;
		}
		if innerAttributes.fontFamily.is_some() {
			self.fontFamily = innerAttributes.fontFamily.clone();
		}
		if innerAttributes.strokeColor.is_some() {
			self.strokeColor = innerAttributes.strokeColor;
		}
		if innerAttributes.strokeWidth.is_some() {
			self.strokeWidth = innerAttributes.strokeWidth;
		}
		self.isBold = self.isBold || innerAttributes.isBold;
		self.isItalic = self.isItalic || innerAttributes.isItalic;
		self.isUnderline = self.isUnderline || innerAttributes.isUnderline;
		self.isStrikethrough = self.isStrikethrough || innerAttributes.isStrikethrough;
	}
}

//==============================================================================
// 조각. (글 + 속성)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct RichTextSegment {
	pub text: String,
	pub attributes: RichTextAttributes,
}

//==============================================================================
// 배치된 조각 하나. (그릴 자리와 모든 속성을 푼 값)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct RichTextRun {
	pub text: String,
	pub fontString: String,
	pub fontSize: f32,
	pub x: f32,
	pub width: f32,
	pub textColor: Color,
	pub strokeColor: Color,
	pub strokeWidth: f32,
	pub isUnderline: bool,
	pub isStrikethrough: bool,
}

//==============================================================================
// 조각 배치 결과.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct RichTextLayout {
	pub runList: Vec<RichTextRun>,
	pub totalWidth: f32,
	pub maximumFontSize: f32,
	pub baselineY: f32,
}

//==============================================================================
// 리치텍스트 컴포넌트.
//==============================================================================
pub struct RichText {
	text: Text,
	segments: Vec<RichTextSegment>,
}

impl RichText {
	//==============================================================================
	// 생성. (기본값은 Text 와 같다)
	//==============================================================================
	pub fn new() -> RichText {
		RichText {
			text: Text::new(),
			segments: Vec::new(),
		}
	}

	//==============================================================================
	// 글과 크기로 생성.
	//==============================================================================
	pub fn createWithText(text: &str, fontSize: f32) -> RichText {
		let mut richText = RichText::new();
		richText.setText(text);
		richText.setFontSize(fontSize);
		richText
	}

	//==============================================================================
	// 품은 글자 컴포넌트 반환. (전역 속성과 마크업이 없을 때의 그리기)
	//==============================================================================
	pub fn getTextComponent(&self) -> &Text {
		&self.text
	}

	//==============================================================================
	// 품은 글자 컴포넌트 반환. (변경용)
	//==============================================================================
	pub fn getTextComponentMut(&mut self) -> &mut Text {
		&mut self.text
	}

	//==============================================================================
	// 글 설정. (마크업을 풀어 조각으로 둔다. 알려진 태그가 없으면 조각을 비우고 일반 글로 그린다)
	//==============================================================================
	pub fn setText(&mut self, text: &str) {
		self.text.setText(text);
		self.segments = parseMarkupToSegments(text).unwrap_or_default();
	}

	//==============================================================================
	// 글 반환. (마크업이 든 원래 글)
	//==============================================================================
	pub fn getText(&self) -> &str {
		self.text.getText()
	}

	//==============================================================================
	// 조각 하나 더하기. (마크업 없이 코드로 조각을 쌓을 때)
	//==============================================================================
	pub fn appendSegment(&mut self, text: &str, attributes: RichTextAttributes) -> &mut RichText {
		self.segments.push(RichTextSegment {
			text: text.to_string(),
			attributes,
		});
		self
	}

	//==============================================================================
	// 조각 모두 지우기.
	//==============================================================================
	pub fn clearSegments(&mut self) {
		self.segments.clear();
	}

	//==============================================================================
	// 조각 목록.
	//==============================================================================
	pub fn getSegments(&self) -> &[RichTextSegment] {
		&self.segments
	}

	//==============================================================================
	// 글이 있는 조각이 하나라도 있는지.
	//==============================================================================
	pub fn hasSegments(&self) -> bool {
		self.segments.iter().any(|segment| !segment.text.is_empty())
	}

	//==============================================================================
	// 글자 크기 설정.
	//==============================================================================
	pub fn setFontSize(&mut self, fontSize: f32) {
		self.text.setFontSize(fontSize);
	}

	//==============================================================================
	// 글자 크기 반환.
	//==============================================================================
	pub fn getFontSize(&self) -> f32 {
		self.text.getFontSize()
	}

	//==============================================================================
	// 폰트 이름 설정.
	//==============================================================================
	pub fn setFontFamily(&mut self, fontFamily: &str) {
		self.text.setFontFamily(fontFamily);
	}

	//==============================================================================
	// 폰트 이름 반환.
	//==============================================================================
	pub fn getFontFamily(&self) -> &str {
		self.text.getFontFamily()
	}

	//==============================================================================
	// 글자 색 설정.
	//==============================================================================
	pub fn setTextColor(&mut self, color: impl Into<Color>) {
		self.text.setTextColor(color);
	}

	//==============================================================================
	// 글자 색 반환.
	//==============================================================================
	pub fn getTextColor(&self) -> Color {
		self.text.getTextColor()
	}

	//==============================================================================
	// 외곽선 색 설정.
	//==============================================================================
	pub fn setStrokeColor(&mut self, color: impl Into<Color>) {
		self.text.setStrokeColor(color);
	}

	//==============================================================================
	// 외곽선 색 반환.
	//==============================================================================
	pub fn getStrokeColor(&self) -> Color {
		self.text.getStrokeColor()
	}

	//==============================================================================
	// 외곽선 두께 설정.
	//==============================================================================
	pub fn setStrokeWidth(&mut self, strokeWidth: f32) {
		self.text.setStrokeWidth(strokeWidth);
	}

	//==============================================================================
	// 외곽선 두께 반환.
	//==============================================================================
	pub fn getStrokeWidth(&self) -> f32 {
		self.text.getStrokeWidth()
	}

	//==============================================================================
	// 굵게 설정.
	//==============================================================================
	pub fn setBold(&mut self, isBold: bool) {
		self.text.setBold(isBold);
	}

	//==============================================================================
	// 굵게 여부.
	//==============================================================================
	pub fn isBold(&self) -> bool {
		self.text.isBold()
	}

	//==============================================================================
	// 기울임 설정.
	//==============================================================================
	pub fn setItalic(&mut self, isItalic: bool) {
		self.text.setItalic(isItalic);
	}

	//==============================================================================
	// 기울임 여부.
	//==============================================================================
	pub fn isItalic(&self) -> bool {
		self.text.isItalic()
	}

	//==============================================================================
	// 가로 정렬 설정.
	//==============================================================================
	pub fn setTextAlign(&mut self, textAlign: TextAlign) {
		self.text.setTextAlign(textAlign);
	}

	//==============================================================================
	// 가로 정렬 반환.
	//==============================================================================
	pub fn getTextAlign(&self) -> TextAlign {
		self.text.getTextAlign()
	}

	//==============================================================================
	// 세로 기준선 설정.
	//==============================================================================
	pub fn setTextBaseline(&mut self, textBaseline: TextBaseline) {
		self.text.setTextBaseline(textBaseline);
	}

	//==============================================================================
	// 세로 기준선 반환.
	//==============================================================================
	pub fn getTextBaseline(&self) -> TextBaseline {
		self.text.getTextBaseline()
	}

	//==============================================================================
	// 줄바꿈 폭 설정. (마크업이 없을 때만 쓰인다. 조각은 원본처럼 한 줄)
	//==============================================================================
	pub fn setWordWrapWidth(&mut self, wordWrapWidth: f32) {
		self.text.setWordWrapWidth(wordWrapWidth);
	}

	//==============================================================================
	// 줄바꿈 폭 반환.
	//==============================================================================
	pub fn getWordWrapWidth(&self) -> f32 {
		self.text.getWordWrapWidth()
	}

	//==============================================================================
	// 줄 간격 설정.
	//==============================================================================
	pub fn setLineSpacing(&mut self, lineSpacing: f32) {
		self.text.setLineSpacing(lineSpacing);
	}

	//==============================================================================
	// 줄 간격 반환.
	//==============================================================================
	pub fn getLineSpacing(&self) -> f32 {
		self.text.getLineSpacing()
	}

	//==============================================================================
	// 조각의 폰트 문자열과 크기. (Canvas2D font 꼴)
	//==============================================================================
	fn resolveSegmentFont(&self, attributes: &RichTextAttributes) -> (String, f32) {
		let fontSize = attributes.fontSize.unwrap_or(self.text.getFontSize());
		let fontFamily = attributes.fontFamily.as_deref().unwrap_or(self.text.getFontFamily());
		let isBold = attributes.isBold || self.text.isBold();
		let isItalic = attributes.isItalic || self.text.isItalic();
		(buildFontString(fontFamily, fontSize, isBold, isItalic), fontSize)
	}

	//==============================================================================
	// 조각 배치. (합친 폭과 가장 큰 글자 크기로 정렬, 기준선은 Text 와 같은 규칙. 원본 drawSegments 의 자리 계산)
	// - measureWidth(폰트 문자열, 글) 은 글 폭을 돌려준다.
	//==============================================================================
	pub fn layoutSegments(&self, contentSize: Vector2, measureWidth: &mut dyn FnMut(&str, &str) -> f32) -> RichTextLayout {
		let mut runList: Vec<RichTextRun> = Vec::new();
		let mut totalWidth = 0.0;
		let mut maximumFontSize: f32 = 0.0;
		for segment in self.segments.iter() {
			if segment.text.is_empty() {
				continue;
			}
			let attributes = &segment.attributes;
			let (fontString, fontSize) = self.resolveSegmentFont(attributes);
			let width = measureWidth(&fontString, &segment.text);
			runList.push(RichTextRun {
				text: segment.text.clone(),
				fontString,
				fontSize,
				x: totalWidth,
				width,
				textColor: attributes.textColor.unwrap_or(self.text.getTextColor()),
				strokeColor: attributes.strokeColor.unwrap_or(self.text.getStrokeColor()),
				strokeWidth: attributes.strokeWidth.unwrap_or(self.text.getStrokeWidth()),
				isUnderline: attributes.isUnderline,
				isStrikethrough: attributes.isStrikethrough,
			});
			totalWidth += width;
			maximumFontSize = maximumFontSize.max(fontSize);
		}

		// 시작 x.
		let startX = match self.text.getTextAlign() {
			TextAlign::left | TextAlign::start => {
				0.0
			}
			TextAlign::right | TextAlign::end => {
				contentSize.x - totalWidth
			}
			TextAlign::center => {
				(contentSize.x - totalWidth) * 0.5
			}
		};
		for run in runList.iter_mut() {
			run.x += startX;
		}

		// 기준선 y.
		let baselineY = match self.text.getTextBaseline() {
			TextBaseline::top | TextBaseline::hanging => {
				0.0
			}
			TextBaseline::bottom | TextBaseline::ideographic | TextBaseline::alphabetic => {
				contentSize.y
			}
			TextBaseline::middle => {
				contentSize.y * 0.5
			}
		};
		RichTextLayout {
			runList,
			totalWidth,
			maximumFontSize,
			baselineY,
		}
	}

	//==============================================================================
	// 글의 자연 크기. (조각이 있으면 합친 폭 x 가장 큰 글자 크기, 없으면 일반 글 폭 x 글자 크기. 원본 getIntrinsicContentSize)
	//==============================================================================
	pub fn calculateIntrinsicContentSize(&self, measureWidth: &mut dyn FnMut(&str, &str) -> f32) -> Vector2 {
		if !self.hasSegments() {
			let fontString = self.text.buildFontString();
			let width = measureWidth(&fontString, self.text.getText());
			return Vector2::create(width, self.text.getFontSize());
		}
		let layout = self.layoutSegments(Vector2::zero(), measureWidth);
		let maximumFontSize = if layout.maximumFontSize > 0.0 {
			layout.maximumFontSize
		}
		else {
			self.text.getFontSize()
		};
		Vector2::create(layout.totalWidth, maximumFontSize)
	}

	//==============================================================================
	// 밑줄 y 오프셋. (원본 Text.computeUnderlineOffsetY)
	//==============================================================================
	fn calculateUnderlineOffsetY(&self, fontSize: f32) -> f32 {
		match self.text.getTextBaseline() {
			TextBaseline::top | TextBaseline::hanging => {
				fontSize + 2.0
			}
			TextBaseline::middle => {
				fontSize * 0.5 + 2.0
			}
			_ => {
				4.0
			}
		}
	}

	//==============================================================================
	// 취소선 y 오프셋. (원본 Text.computeStrikethroughOffsetY)
	//==============================================================================
	fn calculateStrikethroughOffsetY(&self, fontSize: f32) -> f32 {
		match self.text.getTextBaseline() {
			TextBaseline::top | TextBaseline::hanging => {
				fontSize * 0.55
			}
			TextBaseline::middle => {
				0.0
			}
			_ => {
				-fontSize * 0.35
			}
		}
	}

	//==============================================================================
	// 조각 그리기. (원본 drawSegments)
	//==============================================================================
	pub fn drawSegments(&self, graphic: &mut Graphic, contentSize: Vector2) {
		let layout = {
			let textStringTextureCache = graphic.getTextStringTextureCache();
			self.layoutSegments(contentSize, &mut |fontString: &str, text: &str| textStringTextureCache.measureText(fontString, text).width)
		};
		let baselineY = layout.baselineY;
		graphic.setTextAlign(TextAlign::left);
		graphic.setTextBaseline(self.text.getTextBaseline());
		for run in layout.runList.iter() {
			graphic.setFontString(&run.fontString);
			if run.strokeWidth > 0.0 {
				graphic.setStrokeColor(run.strokeColor);
				graphic.drawStrokeText(&run.text, run.x, baselineY, run.strokeWidth);
			}
			graphic.setFillColor(run.textColor);
			graphic.drawFillText(&run.text, run.x, baselineY);

			let decorationLineWidth = (run.fontSize / 16.0).max(1.0);
			if run.isUnderline {
				let underlineY = baselineY + self.calculateUnderlineOffsetY(run.fontSize);
				graphic.setStrokeColor(run.textColor);
				graphic.drawLine(&[Vector2::create(run.x, underlineY), Vector2::create(run.x + run.width, underlineY)], decorationLineWidth);
			}
			if run.isStrikethrough {
				let strikethroughY = baselineY + self.calculateStrikethroughOffsetY(run.fontSize);
				graphic.setStrokeColor(run.textColor);
				graphic.drawLine(&[Vector2::create(run.x, strikethroughY), Vector2::create(run.x + run.width, strikethroughY)], decorationLineWidth);
			}
		}
	}
}

impl Default for RichText {
	fn default() -> RichText {
		RichText::new()
	}
}

impl Component for RichText {
	fn getComponentBase(&self) -> &ComponentBase {
		self.text.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.text.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"RichText"
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if !self.hasSegments() {
			self.text.draw(transform, graphic);
			return;
		}
		self.drawSegments(graphic, transform.getContentSize());
	}
}

//==============================================================================
// Canvas2D font 꼴 문자열. (원본 text.js 의 buildFontString)
//==============================================================================
pub fn buildFontString(fontFamily: &str, fontSize: f32, isBold: bool, isItalic: bool) -> String {
	let mut styleParts: Vec<String> = Vec::new();
	if isItalic {
		styleParts.push(String::from("italic"));
	}
	if isBold {
		styleParts.push(String::from("bold"));
	}
	styleParts.push(format!("{}px", fontSize));
	styleParts.push(fontFamily.to_string());
	styleParts.join(" ")
}

//==============================================================================
// 색 값(#RRGGBB / #RRGGBBAA / 이름) 풀기. (원본 parseColorValue)
//==============================================================================
fn parseColorValue(value: Option<&str>) -> Option<Color> {
	let trimmedValue = value?.trim();
	if trimmedValue.is_empty() {
		return None;
	}
	if let Some(hexText) = trimmedValue.strip_prefix('#') {
		if !hexText.is_ascii() {
			return Some(Color::white());
		}
		return Some(Color::createFromHEX(trimmedValue));
	}
	let lowercaseValue = trimmedValue.to_lowercase();
	NAMED_COLOR_LIST.iter().find(|(colorName, _)| *colorName == lowercaseValue).map(|(_, hexText)| Color::createFromHEX(hexText))
}

//==============================================================================
// 자바스크립트 parseInt(텍스트, 10) 처럼 앞쪽 정수 읽기. (부호와 숫자까지, 숫자가 없으면 None)
//==============================================================================
fn parseLeadingInteger(text: &str) -> Option<i64> {
	let trimmedText = text.trim();
	let mut digitText = String::new();
	for (characterIndex, character) in trimmedText.chars().enumerate() {
		if characterIndex == 0 && (character == '+' || character == '-') {
			digitText.push(character);
			continue;
		}
		if character.is_ascii_digit() {
			digitText.push(character);
			continue;
		}
		break;
	}
	digitText.parse::<i64>().ok()
}

//==============================================================================
// 태그 하나 → 속성. (모르는 태그나 값이 틀리면 None. 원본 buildAttributesForTag)
//==============================================================================
fn buildAttributesForTag(tagName: &str, tagValue: Option<&str>) -> Option<RichTextAttributes> {
	let mut attributes = RichTextAttributes::new();
	match tagName {
		"color" => {
			attributes.textColor = Some(parseColorValue(tagValue)?);
		}
		"size" => {
			let fontSize = parseLeadingInteger(tagValue.unwrap_or(""))?;
			if fontSize <= 0 {
				return None;
			}
			attributes.fontSize = Some(fontSize as f32);
		}
		"b" => {
			attributes.isBold = true;
		}
		"i" => {
			attributes.isItalic = true;
		}
		"u" => {
			attributes.isUnderline = true;
		}
		"s" => {
			attributes.isStrikethrough = true;
		}
		_ => {
			return None;
		}
	}
	Some(attributes)
}

//==============================================================================
// 마크업 태그 하나 맞추기. (원본 정규식 /<(\/?)([a-zA-Z]+)(?:=([^>]+))?>/ 와 같다)
// - 맞으면 (끝 바이트 자리, 닫는 태그인지, 소문자 태그 이름, 값).
//==============================================================================
fn matchTagAt(input: &str, startByteIndex: usize) -> Option<(usize, bool, String, Option<String>)> {
	let bytes = input.as_bytes();
	let mut position = startByteIndex + 1;
	let isClose = bytes.get(position) == Some(&b'/');
	if isClose {
		position += 1;
	}
	let nameStart = position;
	while position < bytes.len() && bytes[position].is_ascii_alphabetic() {
		position += 1;
	}
	if position == nameStart {
		return None;
	}
	let tagName = input[nameStart..position].to_lowercase();
	match bytes.get(position) {
		Some(b'>') => {
			Some((position + 1, isClose, tagName, None))
		}
		Some(b'=') => {
			let valueStart = position + 1;
			let valueLength = input[valueStart..].find('>')?;
			if valueLength == 0 {
				return None;
			}
			let valueEnd = valueStart + valueLength;
			Some((valueEnd + 1, isClose, tagName, Some(input[valueStart..valueEnd].to_string())))
		}
		_ => {
			None
		}
	}
}

//==============================================================================
// 마크업 → 조각 목록. (알려진 태그가 하나도 없으면 None. 원본 parseMarkupToSegments)
// - 모르는 여는 태그는 글로 남긴다. 닫는 태그는 짝이 없어도 글에서 빠진다. (원본과 같음)
//==============================================================================
pub fn parseMarkupToSegments(input: &str) -> Option<Vec<RichTextSegment>> {
	if input.is_empty() || !input.contains('<') {
		return None;
	}
	let mut segments: Vec<RichTextSegment> = Vec::new();
	let mut stack: Vec<(String, RichTextAttributes)> = Vec::new();
	let mut lastIndex = 0;
	let mut searchIndex = 0;
	let mut hasFoundKnownTag = false;
	let mergeStack = |stack: &Vec<(String, RichTextAttributes)>| -> RichTextAttributes {
		let mut mergedAttributes = RichTextAttributes::new();
		for (_, attributes) in stack.iter() {
			mergedAttributes.mergeFrom(attributes);
		}
		mergedAttributes
	};
	while let Some(foundOffset) = input[searchIndex..].find('<') {
		let matchStart = searchIndex + foundOffset;
		let (matchEnd, isClose, tagName, tagValue) = match matchTagAt(input, matchStart) {
			Some(tagMatch) => {
				tagMatch
			}
			None => {
				searchIndex = matchStart + 1;
				continue;
			}
		};
		searchIndex = matchEnd;

		let precedingText = &input[lastIndex..matchStart];
		if !precedingText.is_empty() {
			segments.push(RichTextSegment {
				text: precedingText.to_string(),
				attributes: mergeStack(&stack),
			});
		}

		if isClose {
			if let Some(stackIndex) = stack.iter().rposition(|(stackTagName, _)| *stackTagName == tagName) {
				stack.remove(stackIndex);
				hasFoundKnownTag = true;
			}
			lastIndex = matchEnd;
			continue;
		}

		match buildAttributesForTag(&tagName, tagValue.as_deref()) {
			Some(tagAttributes) => {
				stack.push((tagName, tagAttributes));
				hasFoundKnownTag = true;
			}
			None => {
				segments.push(RichTextSegment {
					text: input[matchStart..matchEnd].to_string(),
					attributes: mergeStack(&stack),
				});
			}
		}
		lastIndex = matchEnd;
	}

	let trailingText = &input[lastIndex..];
	if !trailingText.is_empty() {
		segments.push(RichTextSegment {
			text: trailingText.to_string(),
			attributes: mergeStack(&stack),
		});
	}
	if !hasFoundKnownTag {
		return None;
	}
	Some(segments)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 시험용 재기. (글자 하나 = 크기의 절반 폭. 폰트 문자열의 "Npx" 를 읽는다)
	//==============================================================================
	fn measureByFontSize(fontString: &str, text: &str) -> f32 {
		let fontDescription = crate::core::textstringtexturecache::parseFontString(fontString);
		text.chars().count() as f32 * fontDescription.fontSize * 0.5
	}

	//==============================================================================
	// 조각 글 목록.
	//==============================================================================
	fn collectSegmentTexts(segments: &[RichTextSegment]) -> Vec<&str> {
		segments.iter().map(|segment| segment.text.as_str()).collect()
	}

	//==============================================================================
	// 태그 풀기: 굵게, 색(HEX, 이름), 크기, 겹침, 모르는 태그, 짝 없는 닫는 태그.
	//==============================================================================
	#[test]
	fn parseMarkup() {
		assert!(parseMarkupToSegments("plain text").is_none());
		assert!(parseMarkupToSegments("a < b and <unknown>tag</unknown>").is_none());
		assert!(parseMarkupToSegments("").is_none());

		let segments = parseMarkupToSegments("A<b>B<color=#ff000080>C</color></b><size=20px>D</size>E").expect("segments");
		assert_eq!(collectSegmentTexts(&segments), vec!["A", "B", "C", "D", "E"]);
		assert_eq!(segments[0].attributes, RichTextAttributes::new());
		assert!(segments[1].attributes.isBold);
		assert!(segments[2].attributes.isBold);
		assert_eq!(segments[2].attributes.textColor, Some(Color::createFromBytes(255, 0, 0, 128)));
		assert_eq!(segments[3].attributes.fontSize, Some(20.0));
		assert!(!segments[3].attributes.isBold);
		assert_eq!(segments[4].attributes, RichTextAttributes::new());

		// 안쪽 색이 이긴다. 이름 색은 대소문자를 가리지 않는다.
		let nested = parseMarkupToSegments("<color=red>R<COLOR=Navy>N</color>R</color>").expect("segments");
		assert_eq!(collectSegmentTexts(&nested), vec!["R", "N", "R"]);
		assert_eq!(nested[1].attributes.textColor, Some(Color::createFromHEX("#000080")));
		assert_eq!(nested[2].attributes.textColor, Some(Color::createFromHEX("#ff0000")));

		// 모르는 여는 태그와 틀린 값은 글로 남고, 짝 없는 닫는 태그는 빠진다.
		let unknown = parseMarkupToSegments("<u>x</u><foo>y</foo><size=0>z<color=zzz>w<b=>").expect("segments");
		assert_eq!(collectSegmentTexts(&unknown), vec!["x", "<foo>", "y", "<size=0>", "z", "<color=zzz>", "w<b=>"]);
		assert!(unknown[0].attributes.isUnderline);

		// 값에 '<' 가 들어가도 첫 '>' 까지가 값이다. 16진수가 아닌 # 색은 흰색.
		let strange = parseMarkupToSegments("<s>취소</s><color=#가나>한</color>").expect("segments");
		assert!(strange[0].attributes.isStrikethrough);
		assert_eq!(strange[1].attributes.textColor, Some(Color::white()));
	}

	//==============================================================================
	// 배치: 조각이 왼쪽부터 이어지고, 정렬은 합친 폭 기준, 기준선은 Text 규칙. 속성이 없으면 Text 값을 쓴다.
	//==============================================================================
	#[test]
	fn layoutRuns() {
		let mut richText = RichText::createWithText("ab<size=40>cd</size><b><color=blue>e</color></b>", 20.0);
		richText.setTextColor("#00ff00");
		richText.setTextAlign(TextAlign::center);
		richText.setTextBaseline(TextBaseline::middle);
		let layout = richText.layoutSegments(Vector2::create(200.0, 60.0), &mut measureByFontSize);
		assert_eq!(layout.runList.len(), 3);
		assert_eq!(layout.totalWidth, 20.0 + 40.0 + 10.0);
		assert_eq!(layout.maximumFontSize, 40.0);
		assert_eq!(layout.baselineY, 30.0);
		let startX = (200.0 - 70.0) * 0.5;
		assert_eq!(layout.runList[0].x, startX);
		assert_eq!(layout.runList[0].fontString, "20px default");
		assert_eq!(layout.runList[0].textColor, Color::createFromHEX("#00ff00"));
		assert_eq!(layout.runList[1].x, startX + 20.0);
		assert_eq!(layout.runList[1].fontSize, 40.0);
		assert_eq!(layout.runList[2].x, startX + 60.0);
		assert_eq!(layout.runList[2].fontString, "bold 20px default");
		assert_eq!(layout.runList[2].textColor, Color::createFromHEX("#0000ff"));

		richText.setTextAlign(TextAlign::right);
		richText.setTextBaseline(TextBaseline::top);
		let rightLayout = richText.layoutSegments(Vector2::create(200.0, 60.0), &mut measureByFontSize);
		assert_eq!(rightLayout.runList[0].x, 130.0);
		assert_eq!(rightLayout.baselineY, 0.0);
		assert_eq!(richText.calculateIntrinsicContentSize(&mut measureByFontSize), Vector2::create(70.0, 40.0));

		// 마크업이 없으면 조각이 없고 일반 글 크기를 잰다. 코드로 조각을 쌓을 수도 있다.
		richText.setText("plain");
		assert!(!richText.hasSegments());
		assert_eq!(richText.calculateIntrinsicContentSize(&mut measureByFontSize), Vector2::create(50.0, 20.0));
		let mut underlineAttributes = RichTextAttributes::new();
		underlineAttributes.isUnderline = true;
		underlineAttributes.fontFamily = Some(String::from("DOSGothic"));
		richText.appendSegment("x", underlineAttributes);
		assert!(richText.hasSegments());
		let appendedLayout = richText.layoutSegments(Vector2::create(100.0, 20.0), &mut measureByFontSize);
		assert_eq!(appendedLayout.runList[0].fontString, "20px DOSGothic");
		assert!(appendedLayout.runList[0].isUnderline);
		richText.clearSegments();
		assert!(richText.getSegments().is_empty());
	}
}

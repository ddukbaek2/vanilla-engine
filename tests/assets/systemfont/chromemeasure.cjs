//==============================================================================
// 크롬 Canvas2D measureText 로 폰트 문자열마다 글을 재 chrome_measure.json 으로 찍는다. (tests/desktopplatform.rs 의 크롬 견주기 기대값)
// - 실행: NODE_PATH=$(npm root -g) node tests/assets/systemfont/chromemeasure.cjs [저장할 json]
//   puppeteer-core 와 시스템 크롬을 헤드리스로 쓴다. (창을 띄우지 않는다)
// - 원본이 글을 굽고 재는 캔버스처럼 문서에 붙이지 않은 캔버스의 2D 컨텍스트로 잰다.
// - 깔린 글꼴과 크롬 판, 운영체제마다 값이 달라 저장소에 넣지 않는다. 엔진을 시험할 기계에서 찍는다. (CI 의 platforms 워크플로가 윈도우, 리눅스, 맥에서 찍는다)
// - 폰트 문자열은 원본 SYSTEM_FONT_STRING 사슬, 일반 이름, 운영체제마다 흔한 글꼴 이름, 없는 이름이고, 글은 라틴, 한글, 일본어, 이모지, 아랍어, 태국어다.
//==============================================================================
const fs = require("fs");
const path = require("path");
const puppeteer = require("puppeteer-core");

const outputPath = process.argv[2] ? path.resolve(process.argv[2]) : path.join(__dirname, "chrome_measure.json");
const chromePathList = [
	"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
	"C:/Program Files/Google/Chrome/Application/chrome.exe",
	"/usr/bin/google-chrome",
];
// 원본 base/platform.js 의 SYSTEM_FONT_STRING. (러스트 base/platform.rs 와 같은 글)
const SYSTEM_FONT_STRING = "-apple-system, \"Segoe UI\", Roboto, \"Apple SD Gothic Neo\", \"Malgun Gothic\", sans-serif";
const fontList = [
	`16px ${SYSTEM_FONT_STRING}`,
	`bold 16px ${SYSTEM_FONT_STRING}`,
	`12px ${SYSTEM_FONT_STRING}`,
	`24px ${SYSTEM_FONT_STRING}`,
	"16px sans-serif",
	"16px serif",
	"16px monospace",
	"16px system-ui",
	"16px cursive",
	"16px fantasy",
	"italic 16px sans-serif",
	"bold italic 20px serif",
	"16px Arial",
	"16px \"Times New Roman\"",
	"16px \"Courier New\"",
	"16px \"Segoe UI\"",
	"16px \"Malgun Gothic\"",
	"16px \"DejaVu Sans\"",
	"16px \"Liberation Sans\"",
	"16px \"Noto Sans CJK KR\"",
	"16px NoSuchFamily",
];
const textList = [
	"Hello World Wifi",
	"가나다라 한글",
	"日本語のテキスト",
	"\u{1F600}\u{1F44D}\u{1F3FD}",
	"مرحبا بالعالم",
	"ภาษาไทย",
	"Ünïcödé ñ 0123456789",
];

(async () => {
	const executablePath = chromePathList.find((chromePath) => fs.existsSync(chromePath));
	const browser = await puppeteer.launch({ executablePath, headless: true });
	const page = await browser.newPage();
	await page.goto("about:blank");
	const caseList = await page.evaluate((fontList, textList) => {
		const canvas = document.createElement("canvas");
		const context = canvas.getContext("2d");
		const resultList = [];
		for (const font of fontList) {
			context.font = font;
			for (const text of textList) {
				const textMetrics = context.measureText(text);
				resultList.push({
					font: font,
					text: text,
					width: textMetrics.width,
					fontBoundingBoxAscent: textMetrics.fontBoundingBoxAscent,
					fontBoundingBoxDescent: textMetrics.fontBoundingBoxDescent,
					actualBoundingBoxLeft: textMetrics.actualBoundingBoxLeft,
					actualBoundingBoxRight: textMetrics.actualBoundingBoxRight,
					actualBoundingBoxAscent: textMetrics.actualBoundingBoxAscent,
					actualBoundingBoxDescent: textMetrics.actualBoundingBoxDescent,
				});
			}
		}
		return resultList;
	}, fontList, textList);
	const languageList = await page.evaluate(() => navigator.languages);
	const result = { chrome: await browser.version(), platform: process.platform, architecture: process.arch, languages: languageList, cases: caseList };
	await browser.close();
	fs.writeFileSync(outputPath, JSON.stringify(result, null, "\t"));
	console.log(`${result.chrome} (${result.platform} ${result.architecture}): ${caseList.length} 글 → ${outputPath}`);
})();

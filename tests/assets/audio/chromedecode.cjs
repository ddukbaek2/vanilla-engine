//==============================================================================
// 크롬 decodeAudioData 의 결과를 chrome_decode.json 으로 찍는다. (데스크탑 소리 해석 시험의 기대값)
// - 실행: NODE_PATH=$(npm root -g) node tests/assets/audio/chromedecode.cjs
//   puppeteer-core 와 시스템 크롬을 헤드리스로 쓴다. (창을 띄우지 않는다)
// - 원본 resource/audioasset.js 와 같이 OfflineAudioContext(1, 1, 44100) 으로 푼다.
// - 파일마다 길이, 채널 수, 정한 자리의 표본 값, 채널마다 제곱합을 남긴다.
//==============================================================================
const fs = require("fs");
const http = require("http");
const path = require("path");
const puppeteer = require("puppeteer-core");

const assetDirectory = __dirname;
const outputPath = path.join(assetDirectory, "chrome_decode.json");
const audioExtensionList = [".wav", ".ogg", ".webm", ".mp3", ".flac", ".m4a", ".aac"];
const chromePathList = [
	"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
	"C:/Program Files/Google/Chrome/Application/chrome.exe",
	"/usr/bin/google-chrome",
];

const server = http.createServer((request, response) => {
	if (request.url === "/") {
		response.writeHead(200, { "Content-Type": "text/html" });
		response.end("<html><body></body></html>");
		return;
	}
	const filePath = path.join(assetDirectory, decodeURIComponent(request.url.split("?")[0]));
	fs.readFile(filePath, (error, data) => {
		if (error) {
			response.writeHead(404);
			response.end();
			return;
		}
		response.writeHead(200, { "Content-Type": "application/octet-stream" });
		response.end(data);
	});
});

server.listen(0, "127.0.0.1", async () => {
	const port = server.address().port;
	const executablePath = chromePathList.find((chromePath) => fs.existsSync(chromePath));
	const browser = await puppeteer.launch({ executablePath, headless: true });
	const page = await browser.newPage();
	await page.goto(`http://127.0.0.1:${port}/`);
	const fileList = fs.readdirSync(assetDirectory).filter((name) => audioExtensionList.includes(path.extname(name))).sort();
	const result = { chrome: await browser.version(), files: {} };
	for (const fileName of fileList) {
		result.files[fileName] = await page.evaluate(async (fileName) => {
			const response = await fetch("/" + fileName);
			const arrayBuffer = await response.arrayBuffer();
			const context = new OfflineAudioContext(1, 1, 44100);
			const buffer = await context.decodeAudioData(arrayBuffer);
			const indexList = [];
			for (let index = 0; index < 8; index++) {
				indexList.push(index);
			}
			for (let index = 16; index < buffer.length; index = Math.floor(index * 1.7) + 1) {
				indexList.push(index);
			}
			for (let index = Math.max(0, buffer.length - 4); index < buffer.length; index++) {
				if (!indexList.includes(index)) {
					indexList.push(index);
				}
			}
			const valueList = [];
			const energyList = [];
			for (let channelIndex = 0; channelIndex < buffer.numberOfChannels; channelIndex++) {
				const data = buffer.getChannelData(channelIndex);
				let sumOfSquares = 0;
				for (let index = 0; index < data.length; index++) {
					sumOfSquares += data[index] * data[index];
				}
				valueList.push(indexList.map((index) => data[index]));
				energyList.push(sumOfSquares);
			}
			return { length: buffer.length, channels: buffer.numberOfChannels, index: indexList, values: valueList, energy: energyList };
		}, fileName);
	}
	fs.writeFileSync(outputPath, JSON.stringify(result));
	await browser.close();
	server.close();
	console.log(`${result.chrome}: ${fileList.length} 파일 → ${outputPath}`);
});

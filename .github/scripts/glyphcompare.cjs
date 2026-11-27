//==============================================================================
// 글리프 래스터를 같은 기계의 크롬과 견준다. (CI 의 platforms 워크플로)
// - 실행: NODE_PATH=<puppeteer-core 와 pngjs 가 든 node_modules> node .github/scripts/glyphcompare.cjs <러스트 그림> <결과 폴더> <이름표>
//   러스트 그림은 captureexamples.cjs 가 찍은 <이름표>_glyphparity_20.png 이다.
// - 엔진과 vanilla.js 를 담은 윗 폴더를 뿌리로 정적 서버를 띄우고 tests/assets/glyphparity/index.html 을 960 x 720, 기기 배율 1 로 연다.
//   원본 엔진이 20 프레임을 그린 뒤 페이지 화면을 찍어 러스트 그림과 견준다.
// - 구역: 크기 줄(왼쪽), 그 밖의 줄(오른쪽)을 흰 바탕(위 절반)과 어두운 바탕(아래 절반)으로 나눠 RGB 채널의 가장 큰 차이를 화소마다 잰다.
//   구역마다 평균, 차이가 0 넘는 화소, 8 넘는 화소, 32 넘는 화소의 수와 가장 큰 차이를 <이름표>_glyph.json 에 쓰고 차이 그림(4 배)을 <이름표>_glyph_diff.png 로 쓴다.
// - 끝 코드는 찍지 못했을 때만 1 이다. (차이는 값으로 본다)
//==============================================================================
const fs = require("fs");
const http = require("http");
const path = require("path");
const puppeteer = require("puppeteer-core");
const { PNG } = require("pngjs");

// CHROME_PATH 가 있으면 그 크롬을 먼저 쓴다.
const chromePathList = [
	...(process.env.CHROME_PATH ? [process.env.CHROME_PATH] : []),
	"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
	"C:/Program Files/Google/Chrome/Application/chrome.exe",
	"/usr/bin/google-chrome",
];
// 견줄 구역. (이름, 왼쪽, 위, 오른쪽, 아래)
const REGION_LIST = [
	["sizes(white bg)", 0, 0, 540, 360],
	["lines(white bg)", 540, 0, 960, 360],
	["sizes(dark bg)", 0, 360, 540, 720],
	["lines(dark bg)", 540, 360, 960, 720],
];
const contentTypeTable = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8", ".map": "application/json" };

//==============================================================================
// 폴더를 뿌리로 정적 서버를 띄운다. (요청마다 응답을 찍는다)
//==============================================================================
function startStaticServer(rootDirectory) {
	const server = http.createServer((request, response) => {
		const filePath = path.join(rootDirectory, decodeURIComponent(request.url.split("?")[0]));
		fs.readFile(filePath, (error, data) => {
			if (error) {
				console.log(`서버: 404 ${request.url} (${error.code})`);
				response.writeHead(404);
				response.end();
				return;
			}
			const contentType = contentTypeTable[path.extname(filePath).toLowerCase()] || "application/octet-stream";
			console.log(`서버: 200 ${contentType} ${request.url} (${data.length} 바이트)`);
			response.writeHead(200, { "Content-Type": contentType, "Cache-Control": "no-store" });
			response.end(data);
		});
	});
	return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

//==============================================================================
// 두 그림의 구역별 차이를 잰다. (차이 그림도 쓴다)
//==============================================================================
function compareRegions(chromePng, rustPng, diffPath) {
	const width = Math.min(chromePng.width, rustPng.width);
	const height = Math.min(chromePng.height, rustPng.height);
	const diffPng = new PNG({ width, height });
	const differenceList = new Uint8Array(width * height);
	for (let positionY = 0; positionY < height; positionY++) {
		for (let positionX = 0; positionX < width; positionX++) {
			const chromeIndex = (positionY * chromePng.width + positionX) * 4;
			const rustIndex = (positionY * rustPng.width + positionX) * 4;
			let difference = 0;
			for (let channel = 0; channel < 3; channel++) {
				difference = Math.max(difference, Math.abs(chromePng.data[chromeIndex + channel] - rustPng.data[rustIndex + channel]));
			}
			differenceList[positionY * width + positionX] = difference;
			const diffIndex = (positionY * width + positionX) * 4;
			const value = Math.min(255, difference * 4);
			diffPng.data[diffIndex] = value;
			diffPng.data[diffIndex + 1] = value;
			diffPng.data[diffIndex + 2] = value;
			diffPng.data[diffIndex + 3] = 255;
		}
	}
	fs.writeFileSync(diffPath, PNG.sync.write(diffPng));
	const result = {};
	for (const [name, left, top, right, bottom] of REGION_LIST) {
		let sum = 0;
		let count = 0;
		let overZero = 0;
		let overEight = 0;
		let overThirtyTwo = 0;
		let maximum = 0;
		for (let positionY = top; positionY < Math.min(bottom, height); positionY++) {
			for (let positionX = left; positionX < Math.min(right, width); positionX++) {
				const difference = differenceList[positionY * width + positionX];
				sum += difference;
				count++;
				overZero += difference > 0 ? 1 : 0;
				overEight += difference > 8 ? 1 : 0;
				overThirtyTwo += difference > 32 ? 1 : 0;
				maximum = Math.max(maximum, difference);
			}
		}
		result[name] = { mean: count > 0 ? sum / count : 0, overZero, overEight, overThirtyTwo, maximum };
	}
	return result;
}

//==============================================================================
// 새 페이지로 장면을 연다. (본문 응답, 실패한 요청, 내려받기를 찍고, 열기가 실패하면 새 페이지로 세 번까지 다시 연다)
// - 윈도우의 크롬은 막 띄운 브라우저의 첫 탐색을 요청이 서버에 닿기 전에 취소할 때가 있다. (net::ERR_ABORTED, 다음 탐색부터는 된다, 원본 tests/chromepage.mjs 와 같은 까닭)
//==============================================================================
async function openPage(browser, url) {
	const ATTEMPT_COUNT = 3;
	for (let attempt = 1; attempt <= ATTEMPT_COUNT; attempt++) {
		const page = await browser.newPage();
		page.on("console", (message) => console.log("console: " + message.text()));
		page.on("pageerror", (error) => console.log("pageerror: " + error.message));
		page.on("requestfailed", (request) => console.log(`requestfailed: ${request.url()} ${request.failure() ? request.failure().errorText : ""}`));
		page.on("response", (response) => {
			if (response.request().isNavigationRequest()) {
				console.log(`본문 응답: ${response.status()} ${response.headers()["content-type"] || ""} ${response.url()}`);
			}
		});
		const session = await page.createCDPSession();
		session.on("Page.downloadWillBegin", (event) => console.log(`내려받기: ${event.url} ${event.suggestedFilename}`));
		await session.send("Page.enable");
		try {
			await page.setViewport({ width: 960, height: 720, deviceScaleFactor: 1 });
			await page.goto(url, { waitUntil: "load" });
			return page;
		}
		catch (error) {
			console.log(`열기 ${attempt}/${ATTEMPT_COUNT} 실패: ${error.message}`);
			await page.close().catch(() => {});
			if (attempt === ATTEMPT_COUNT) {
				throw error;
			}
			await new Promise((resolve) => setTimeout(resolve, 2000));
		}
	}
}

//==============================================================================
// 시작.
//==============================================================================
async function main() {
	const [rustPath, outputDirectory, tag] = process.argv.slice(2);
	const engineDirectory = path.resolve(__dirname, "..", "..");
	const rootDirectory = path.dirname(engineDirectory);
	const chromePath = chromePathList.find((candidate) => fs.existsSync(candidate));
	if (!chromePath || !fs.existsSync(rustPath)) {
		console.log(`크롬(${chromePath}) 이나 러스트 그림(${rustPath}) 이 없다`);
		process.exit(1);
	}
	const server = await startStaticServer(rootDirectory);
	const browser = await puppeteer.launch({ executablePath: chromePath, headless: true, args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--force-color-profile=srgb", "--hide-scrollbars", "--lang=en-US"] });
	try {
		const pagePath = path.relative(rootDirectory, path.join(engineDirectory, "tests", "assets", "glyphparity", "index.html")).split(path.sep).join("/");
		const page = await openPage(browser, `http://127.0.0.1:${server.address().port}/${pagePath}`);
		// 원본 엔진이 20 프레임을 그린 뒤(requestFrameCapture 가 끝남) 페이지를 찍는다. (SwiftShader 의 WebGL 되읽기는 검게 나와 chromecompare.cjs 처럼 화면을 찍는다)
		await page.waitForFunction(() => window.__captureDataUrl !== undefined, { timeout: 60000 });
		await new Promise((resolve) => setTimeout(resolve, 500));
		const chromePath = path.join(outputDirectory, `${tag}_glyph_chrome.png`);
		await page.screenshot({ path: chromePath, type: "png", clip: { x: 0, y: 0, width: 960, height: 720 } });
		const chromePng = PNG.sync.read(fs.readFileSync(chromePath));
		const rustPng = PNG.sync.read(fs.readFileSync(rustPath));
		console.log(`크롬 ${chromePng.width} x ${chromePng.height}, 러스트 ${rustPng.width} x ${rustPng.height}`);
		const result = compareRegions(chromePng, rustPng, path.join(outputDirectory, `${tag}_glyph_diff.png`));
		fs.writeFileSync(path.join(outputDirectory, `${tag}_glyph.json`), JSON.stringify(result, null, "\t"));
		for (const [name, value] of Object.entries(result)) {
			console.log(`${tag} ${name}: 평균 ${value.mean.toFixed(3)}, >0 ${value.overZero}, >8 ${value.overEight}, >32 ${value.overThirtyTwo}, 최대 ${value.maximum}`);
		}
	}
	finally {
		await browser.close();
		server.close();
	}
}

main().catch((error) => {
	console.log(error);
	process.exit(1);
});

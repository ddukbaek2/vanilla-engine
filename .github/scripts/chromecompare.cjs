//==============================================================================
// 원본 vanilla.js 예제를 같은 기계의 헤드리스 크롬으로 찍어 러스트 데스크탑 예제가 찍은 그림과 화소를 견준다. (CI 의 platforms 워크플로)
// - 실행: NODE_PATH=<puppeteer-core 와 pngjs 가 든 node_modules> node .github/scripts/chromecompare.cjs <vanilla.js 폴더> <그림 폴더> <이름표>
//   captureexamples.cjs 가 <그림 폴더>/<이름표>_<예제>_<프레임>.png 을 먼저 찍어 두어야 한다.
// - vanilla.js 폴더를 뿌리로 정적 서버를 띄우고, 예제 페이지를 러스트 예제와 같은 값(쿼리)과 같은 크기로 연다.
//   기기 배율은 러스트 그림 폭 / 페이지 폭이다. (러스트 창의 배율과 같게)
// - 크롬 그림 <이름표>_<예제>_chrome.png, 차이 그림 <이름표>_<예제>_diff.png(차이를 4 배), 크롬 콘솔 <이름표>_<예제>_chrome.log 을 쓴다.
// - 견준 값: RGB 의 평균 절대 차이(0 ~ 1), 가장 큰 차이(0 ~ 255), 차이가 2 / 255 넘는 화소와 16 / 255 넘는 화소의 몫. <이름표>_chrome.json 에 모은다.
// - 크롬의 WebGL 은 GPU 가 없는 기계라 SwiftShader 로, 러스트는 메사 llvmpipe 로 그려 래스터라이저가 다르다. 끝 코드는 찍지 못했을 때만 1 이다. (차이는 값으로 본다)
//==============================================================================
const fs = require("fs");
const http = require("http");
const path = require("path");
const puppeteer = require("puppeteer-core");
const { PNG } = require("pngjs");

// CHROME_PATH 가 있으면 그 크롬을 먼저 쓴다. (CI 는 browser-actions/setup-chrome 으로 판을 고정한 Chrome for Testing 을 넘긴다)
const chromePathList = [
	...(process.env.CHROME_PATH ? [process.env.CHROME_PATH] : []),
	"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
	"C:/Program Files/Google/Chrome/Application/chrome.exe",
	"C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
	"/usr/bin/google-chrome",
];
// 견줄 짝. (러스트 예제 이름과 찍은 프레임, 원본 페이지와 쿼리, 논리 크기. 쿼리는 captureexamples.cjs 의 환경 변수와 같은 값)
const PAIR_LIST = [
	{ name: "toonstyle", frame: "30", page: "examples/toonstyle/index.html", query: "", width: 1280, height: 720 },
	{ name: "toonmaterial", frame: "30", page: "examples/toonmaterial/index.html", query: "?frame=60", width: 1280, height: 720 },
	{ name: "skeleton2d", frame: "170", page: "examples/skeleton2d/index.html", query: "?frame=135", width: 960, height: 540 },
	{ name: "physics2d", frame: "60", page: "examples/physics2d/index.html", query: "?prestep=180", width: 1280, height: 720 },
	{ name: "tilemapshowcase", frame: "60", page: "examples/tilemapshowcase/index.html", query: "?time=1.25", width: 1280, height: 720 },
];
// 페이지가 다 그릴 때까지 기다릴 시간. (밀리초, 네트워크가 쉰 뒤)
const SETTLE_MILLISECONDS = 4000;

const contentTypeTable = {
	".html": "text/html; charset=utf-8",
	".js": "text/javascript; charset=utf-8",
	".mjs": "text/javascript; charset=utf-8",
	".json": "application/json; charset=utf-8",
	".css": "text/css; charset=utf-8",
	".wasm": "application/wasm",
	".png": "image/png",
	".jpg": "image/jpeg",
	".webp": "image/webp",
	".svg": "image/svg+xml",
	".ttf": "font/ttf",
	".otf": "font/otf",
	".woff": "font/woff",
	".woff2": "font/woff2",
	".glb": "model/gltf-binary",
	".fbx": "application/octet-stream",
	".ogg": "audio/ogg",
	".mp3": "audio/mpeg",
	".wav": "audio/wav",
};

//==============================================================================
// 폴더를 뿌리로 정적 서버를 띄운다. (빈 포트를 운영체제가 고른다)
//==============================================================================
function startStaticServer(rootDirectory) {
	return new Promise((resolve) => {
		const server = http.createServer((request, response) => {
			const requestPath = decodeURIComponent(request.url.split("?")[0]);
			const filePath = path.join(rootDirectory, requestPath);
			if (!filePath.startsWith(rootDirectory)) {
				response.writeHead(403);
				response.end();
				return;
			}
			fs.readFile(filePath, (error, data) => {
				if (error) {
					response.writeHead(404);
					response.end();
					return;
				}
				response.writeHead(200, { "Content-Type": contentTypeTable[path.extname(filePath).toLowerCase()] || "application/octet-stream", "Cache-Control": "no-store" });
				response.end(data);
			});
		});
		server.listen(0, "127.0.0.1", () => resolve(server));
	});
}

//==============================================================================
// 두 그림의 RGB 차이. (크기가 다르면 null)
//==============================================================================
function comparePng(firstPng, secondPng, diffPath) {
	if (firstPng.width !== secondPng.width || firstPng.height !== secondPng.height) {
		return null;
	}
	const diffPng = new PNG({ width: firstPng.width, height: firstPng.height });
	let differenceSum = 0;
	let maximumDifference = 0;
	let overTwoCount = 0;
	let overSixteenCount = 0;
	const pixelCount = firstPng.width * firstPng.height;
	for (let pixelIndex = 0; pixelIndex < pixelCount; pixelIndex++) {
		const offset = pixelIndex * 4;
		let pixelMaximum = 0;
		for (let channel = 0; channel < 3; channel++) {
			const difference = Math.abs(firstPng.data[offset + channel] - secondPng.data[offset + channel]);
			differenceSum += difference;
			pixelMaximum = Math.max(pixelMaximum, difference);
			diffPng.data[offset + channel] = Math.min(255, difference * 4);
		}
		diffPng.data[offset + 3] = 255;
		maximumDifference = Math.max(maximumDifference, pixelMaximum);
		if (pixelMaximum > 2) {
			overTwoCount++;
		}
		if (pixelMaximum > 16) {
			overSixteenCount++;
		}
	}
	fs.writeFileSync(diffPath, PNG.sync.write(diffPng));
	return {
		meanAbsoluteDifference: differenceSum / (pixelCount * 3 * 255),
		maximumDifference: maximumDifference,
		overTwoRatio: overTwoCount / pixelCount,
		overSixteenRatio: overSixteenCount / pixelCount,
	};
}

//==============================================================================
// 시작.
//==============================================================================
async function main() {
	const [originalDirectoryText, shotDirectoryText, tag] = process.argv.slice(2);
	if (!originalDirectoryText || !shotDirectoryText || !tag) {
		console.log("쓰는 법: node .github/scripts/chromecompare.cjs <vanilla.js 폴더> <그림 폴더> <이름표>");
		process.exit(2);
	}
	const originalDirectory = path.resolve(originalDirectoryText);
	const shotDirectory = path.resolve(shotDirectoryText);
	const executablePath = chromePathList.find((chromePath) => fs.existsSync(chromePath));
	if (!executablePath) {
		console.log("크롬을 찾지 못했다: " + chromePathList.join(", "));
		process.exit(1);
	}
	const server = await startStaticServer(originalDirectory);
	const port = server.address().port;
	const browser = await puppeteer.launch({
		executablePath,
		headless: true,
		args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--force-color-profile=srgb", "--hide-scrollbars", "--lang=en-US"],
	});
	console.log(`${await browser.version()} (${process.platform} ${process.arch})`);
	const resultList = [];
	let hasFailure = false;
	for (const pair of PAIR_LIST) {
		const rustPath = path.join(shotDirectory, `${tag}_${pair.name}_${pair.frame}.png`);
		const chromePath = path.join(shotDirectory, `${tag}_${pair.name}_chrome.png`);
		const result = { name: pair.name, page: pair.page + pair.query };
		resultList.push(result);
		if (!fs.existsSync(rustPath)) {
			result.error = "러스트 그림 없음";
			console.log(`${pair.name}: ${result.error}`);
			continue;
		}
		const rustPng = PNG.sync.read(fs.readFileSync(rustPath));
		const deviceScaleFactor = rustPng.width / pair.width;
		const page = await browser.newPage();
		const consoleLineList = [];
		page.on("console", (message) => consoleLineList.push(`[${message.type()}] ${message.text()}`));
		page.on("pageerror", (error) => consoleLineList.push(`[pageerror] ${error.message}`));
		page.on("requestfailed", (request) => consoleLineList.push(`[requestfailed] ${request.url()} ${request.failure() ? request.failure().errorText : ""}`));
		try {
			await page.setViewport({ width: pair.width, height: pair.height, deviceScaleFactor });
			// 윈도우의 크롬은 막 띄운 브라우저의 첫 탐색을 요청이 서버에 닿기 전에 취소할 때가 있어(net::ERR_ABORTED, 다음 탐색부터는 된다) 세 번까지 다시 연다.
			for (let attempt = 1; ; attempt++) {
				try {
					await page.goto(`http://127.0.0.1:${port}/${pair.page}${pair.query}`, { waitUntil: "networkidle0", timeout: 120000 });
					break;
				}
				catch (error) {
					if (attempt >= 3 || !String(error.message).includes("net::ERR_ABORTED")) {
						throw error;
					}
					console.log(`${pair.name}: 다시 열기 ${attempt}/3 (${error.message.split("\n")[0]})`);
				}
			}
			await new Promise((resolve) => setTimeout(resolve, SETTLE_MILLISECONDS));
			result.webGLRenderer = await page.evaluate(() => {
				const canvas = document.createElement("canvas");
				const context = canvas.getContext("webgl2");
				if (!context) {
					return "";
				}
				const extension = context.getExtension("WEBGL_debug_renderer_info");
				return extension ? context.getParameter(extension.UNMASKED_RENDERER_WEBGL) : context.getParameter(context.RENDERER);
			});
			await page.screenshot({ path: chromePath, type: "png", clip: { x: 0, y: 0, width: pair.width, height: pair.height } });
			const chromePng = PNG.sync.read(fs.readFileSync(chromePath));
			const comparison = comparePng(rustPng, chromePng, path.join(shotDirectory, `${tag}_${pair.name}_diff.png`));
			if (comparison) {
				Object.assign(result, comparison);
				console.log(`${pair.name}: 평균 ${comparison.meanAbsoluteDifference.toFixed(5)}, 가장 큰 ${comparison.maximumDifference}, 2 넘는 화소 ${(comparison.overTwoRatio * 100).toFixed(2)}%, 16 넘는 화소 ${(comparison.overSixteenRatio * 100).toFixed(2)}% (${rustPng.width} x ${rustPng.height}, 크롬 ${result.webGLRenderer})`);
			}
			else {
				result.error = `크기 다름: 러스트 ${rustPng.width} x ${rustPng.height}, 크롬 ${chromePng.width} x ${chromePng.height}`;
				console.log(`${pair.name}: ${result.error}`);
			}
		}
		catch (error) {
			result.error = error.message;
			hasFailure = true;
			console.log(`${pair.name}: 크롬 찍기 실패 ${error.message}`);
		}
		fs.writeFileSync(path.join(shotDirectory, `${tag}_${pair.name}_chrome.log`), consoleLineList.join("\n") + "\n");
		await page.close();
	}
	await browser.close();
	server.close();
	fs.writeFileSync(path.join(shotDirectory, `${tag}_chrome.json`), JSON.stringify(resultList, null, "\t"));
	process.exit(hasFailure ? 1 : 0);
}

main();

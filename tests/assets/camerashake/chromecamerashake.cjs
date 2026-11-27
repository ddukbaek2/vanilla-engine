//==============================================================================
// 크롬에서 원본 CameraShake, createOffsetViewMatrix, Camera3D.setViewOffset 을 돌려 chrome_camerashake.json 으로 찍는다. (러스트 camerashake.rs 의 두 엔진 견주기 시험의 기대값)
// - 실행: NODE_PATH=$(npm root -g) node tests/assets/camerashake/chromecamerashake.cjs <vanilla.js 저장소 자리>
//   puppeteer-core 와 시스템 크롬을 헤드리스로 쓴다. (창을 띄우지 않는다) 원본 저장소를 127.0.0.1 의 빈 자리에 잠깐 띄워 원본 모듈을 그대로 불러온다.
// - 원본 커밋 cb59ad0(jsgamereq) 에서 찍었다. 원본 셈을 바꾸면 다시 찍는다. 크롬 판은 chromeVersion 에 적는다.
//   node 가 아니라 크롬에서 찍는 까닭: 원본은 크롬 위에서 돈다. node 25(V8 14.1)의 Math.sin 은 끝자리가 크롬 158 과 갈리는 입력이 있다.
// - 경우마다 add 에 준 칸과 프레임 길이, 프레임마다 read() 의 [자리 x, y, z, 돌림 x, y, z] 와 isShaking() 을 둔다. (read 다음 update 차례)
// - 행렬은 getElements() 의 16 값(Float32Array 를 숫자로) 이다.
//==============================================================================
const fs = require("fs");
const http = require("http");
const path = require("path");
const puppeteer = require("puppeteer-core");

const repositoryPath = path.resolve(process.argv[2] || "../vanilla.js");
const outputPath = path.join(__dirname, "chrome_camerashake.json");
// CHROME_PATH 가 있으면 그 크롬을 먼저 쓴다.
const chromePathList = [
	...(process.env.CHROME_PATH ? [process.env.CHROME_PATH] : []),
	"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
	"C:/Program Files/Google/Chrome/Application/chrome.exe",
	"/usr/bin/google-chrome",
];
const contentTypes = { ".js": "text/javascript", ".mjs": "text/javascript", ".html": "text/html", ".glb": "application/octet-stream" };

(async () => {
	const server = http.createServer((request, response) => {
		const filePath = path.join(repositoryPath, decodeURIComponent(new URL(request.url, "http://127.0.0.1").pathname));
		if (!filePath.startsWith(repositoryPath) || !fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
			response.writeHead(request.url === "/" ? 200 : 404, { "Content-Type": "text/html" });
			response.end("");
			return;
		}
		response.writeHead(200, { "Content-Type": contentTypes[path.extname(filePath)] || "application/octet-stream" });
		response.end(fs.readFileSync(filePath));
	});
	await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
	const executablePath = chromePathList.find((chromePath) => fs.existsSync(chromePath));
	const browser = await puppeteer.launch({ executablePath, headless: true });
	try {
		const page = await browser.newPage();
		await page.goto("http://127.0.0.1:" + server.address().port + "/");
		const result = await page.evaluate(async () => {
			const { CameraShake, createOffsetViewMatrix } = await import("/src/graphics3d/camerashake.js");
			const { Camera3D } = await import("/src/graphics3d/camera3d.js");
			const { Matrix4 } = await import("/src/base/matrix4.js");
			const { Vector3 } = await import("/src/base/vector3.js");
			// 바탕 씨앗(null 이면 기본), 흔들림 칸 목록, 프레임 길이, 프레임 수.
			const caseList = [
				{ name: "example", baseSeed: 7, shakes: [{ amplitude: 0.09, rotationAmplitude: 0.01, duration: 0.25, positionAxes: [1, 0.5, 1] }, { amplitude: 0.04, duration: 0.6, age: 0.1 }], frameSeconds: 1 / 60, frameCount: 40 },
				{ name: "default", baseSeed: null, shakes: [{}, { amplitude: 0.2, rotationAmplitude: 0.05, frequency: 4, attack: 0.05, decayExponent: 1, rotationAxes: [0.5, 1, 0], age: -0.05, duration: 0.5 }], frameSeconds: 1 / 120, frameCount: 70 },
				{ name: "seeded", baseSeed: 42, shakes: [{ amplitude: 0.3, rotationAmplitude: 0.02, duration: 0.4, seed: 11 }, { amplitude: 0.1, rotationAmplitude: 0.05, duration: 0.8, frequency: 4, positionAxes: [1, 0.5, 0], seed: 22 }, { amplitude: 1, rotationAmplitude: 0.1, duration: 1, frequency: 12, decayExponent: 0, seed: 4294967295 }], frameSeconds: 1 / 30, frameCount: 32 },
			];
			const readValues = (shake) => {
				return [shake.position.x, shake.position.y, shake.position.z, shake.rotation.x, shake.rotation.y, shake.rotation.z];
			};
			const cases = caseList.map((shakeCase) => {
				const cameraShake = shakeCase.baseSeed === null ? new CameraShake() : new CameraShake({ seed: shakeCase.baseSeed });
				for (const shake of shakeCase.shakes) {
					cameraShake.add(shake);
				}
				const frames = [];
				for (let frameIndex = 0; frameIndex < shakeCase.frameCount; ++frameIndex) {
					frames.push({ values: readValues(cameraShake.read()), isShaking: cameraShake.isShaking() });
					cameraShake.update(shakeCase.frameSeconds);
				}
				return { ...shakeCase, frames };
			});
			const readElements = (matrix) => {
				return Array.from(matrix.getElements());
			};
			const viewMatrix = Matrix4.createLookAt(Vector3.create(3, 2, 6), Vector3.create(0, 1, 0), Vector3.create(0, 1, 0));
			const offsetPosition = Vector3.create(0.2, -0.1, 0.3);
			const offsetRotation = Vector3.create(0.3, -0.2, 0.4);
			const camera = new Camera3D();
			camera.lookAt(Vector3.create(0, 4, 10), Vector3.create(0, 0, 0));
			camera.setViewOffset(Vector3.create(0.1, 0.05, 0), Vector3.create(0.01, -0.02, 0.03));
			const cameraView = readElements(camera.getViewMatrix());
			const cameraViewProjection = readElements(camera.getViewProjectionMatrix());
			camera.setMatrices(viewMatrix, Matrix4.createPerspective(0.8, 1, 0.1, 100));
			camera.setViewOffset(offsetPosition);
			return {
				cases,
				offsetView: { view: readElements(viewMatrix), position: [0.2, -0.1, 0.3], rotation: [0.3, -0.2, 0.4], result: readElements(createOffsetViewMatrix(viewMatrix, offsetPosition, offsetRotation)) },
				camera: { eye: [0, 4, 10], position: [0.1, 0.05, 0], rotation: [0.01, -0.02, 0.03], view: cameraView, viewProjection: cameraViewProjection, rawView: readElements(camera.getViewMatrix()) },
			};
		});
		const chromeVersion = await browser.version();
		fs.writeFileSync(outputPath, JSON.stringify({ source: "vanilla.js cb59ad0 src/graphics3d/camerashake.js, camera3d.js", chromeVersion, ...result }) + "\n");
		console.log("찍음 " + outputPath + " (" + chromeVersion + ")");
	}
	finally {
		await browser.close();
		server.close();
	}
})();

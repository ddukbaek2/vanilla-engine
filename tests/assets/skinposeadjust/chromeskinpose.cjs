//==============================================================================
// 크롬에서 원본 writeSkinPose 의 뼈 고침(poseAdjust 표와 고리, adjustBones)을 돌려 스킨 행렬을 chrome_skinpose.json 으로 찍는다. (러스트 glbmeshreader.rs 의 두 엔진 견주기 시험의 기대값)
// - 실행: NODE_PATH=$(npm root -g) node tests/assets/skinposeadjust/chromeskinpose.cjs <vanilla.js 저장소 자리>
//   puppeteer-core 와 시스템 크롬을 헤드리스로 쓴다. (창을 띄우지 않는다) 원본 저장소를 127.0.0.1 의 빈 자리에 잠깐 띄워 원본 모듈과 fox.glb 를 그대로 불러온다.
// - 원본 커밋 cb59ad0(jsgamereq) 의 examples/ancientmountain/assets/fox.glb 로 찍었다. 원본 셈을 바꾸면 다시 찍는다. 크롬 판은 chromeVersion 에 적는다.
// - 경우마다 움직임, 시각, 섞기, 고침 표(mode 는 "add", "replace" 글자, 고리 경우는 hook 이름)와 본마다 16 씩의 스킨 행렬(Float32Array 를 숫자로)을 둔다.
// - 고리 "crouch": adjustBones 로 b_Spine01_02 를 x 축 0.6 라디안 돌린 뒤 b_Tail01_012 의 크기 y 를 1.2 배. (원본 adjustBones 문서 예제)
//==============================================================================
const fs = require("fs");
const http = require("http");
const path = require("path");
const puppeteer = require("puppeteer-core");

const repositoryPath = path.resolve(process.argv[2] || "../vanilla.js");
const outputPath = path.join(__dirname, "chrome_skinpose.json");
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
			const { readGlbArrays, writeSkinPose, adjustBones, BoneModifyMode } = await import("/src/graphics3d/toon/glbmeshreader.js");
			const foxBuffer = await (await fetch("/examples/ancientmountain/assets/fox.glb")).arrayBuffer();
			const skeleton = readGlbArrays(foxBuffer).skeleton;
			const jointCount = skeleton.joints.length;
			const toModifiers = (modifierList) => {
				return modifierList.map((modifier) => {
					const converted = { ...modifier };
					if (modifier.mode !== undefined) {
						converted.mode = modifier.mode === "replace" ? BoneModifyMode.replace : BoneModifyMode.add;
					}
					return converted;
				});
			};
			const half = 0.4;
			const caseList = [
				{ name: "plain", animationName: "Run", time: 0.3, blend: null, modifiers: null },
				{ name: "table", animationName: "Run", time: 0.3, blend: null, modifiers: [
					{ name: "b_Tail02_013", rotation: [Math.sin(half), 0, 0, Math.cos(half)], mode: "add" },
					{ name: "b_Head_05", rotation: [0.1, 0.2, 0, 0.9], mode: "replace", weight: 0.5 },
					{ name: "b_Hip_01", translation: [0, -2, 0.5], scale: [1.1, 0.9, 1], weight: 0.25 },
					{ nodeIndex: 3, translation: [1, 2, 3], rotation: [0, 0, 0, 2], scale: [2, 2, 2], mode: "replace" },
					{ name: "missing bone", translation: [1, 2, 3] },
				] },
				{ name: "blendTable", animationName: "Run", time: 0.3, blend: { animationName: "Survey", time: 1.1, weight: 0.4 }, modifiers: [
					{ name: "b_Spine01_02", rotation: [0.05, -0.1, 0.02, 1], weight: 0.7 },
					{ name: "b_Hip_01", translation: [0, 1, 0] },
					{ name: "b_Hip_01", translation: [0, 1, 0] },
				] },
				{ name: "crouchHook", animationName: "Survey", time: 0, blend: null, modifiers: null, hook: "crouch" },
			];
			const crouchHook = (locals, poseSkeleton) => {
				const crouchHalf = 0.6 / 2;
				adjustBones(locals, poseSkeleton, [{ name: "b_Spine01_02", rotation: [Math.sin(crouchHalf), 0, 0, Math.cos(crouchHalf)] }]);
				const tailIndex = poseSkeleton.nodes.findIndex((node) => node.name === "b_Tail01_012");
				locals[tailIndex].scale[1] *= 1.2;
			};
			const cases = caseList.map((poseCase) => {
				const target = new Float32Array(jointCount * 16);
				let poseAdjust = null;
				if (poseCase.hook === "crouch") {
					poseAdjust = crouchHook;
				}
				else if (poseCase.modifiers !== null) {
					poseAdjust = toModifiers(poseCase.modifiers);
				}
				writeSkinPose(skeleton, poseCase.animationName, poseCase.time, target, poseCase.blend, null, poseAdjust);
				return { ...poseCase, jointMatrices: Array.from(target) };
			});
			return { jointCount, cases };
		});
		const chromeVersion = await browser.version();
		fs.writeFileSync(outputPath, JSON.stringify({ source: "vanilla.js cb59ad0 src/graphics3d/toon/glbmeshreader.js writeSkinPose, adjustBones, fox.glb", chromeVersion, ...result }) + "\n");
		console.log("찍음 " + outputPath + " (" + chromeVersion + ")");
	}
	finally {
		await browser.close();
		server.close();
	}
})();

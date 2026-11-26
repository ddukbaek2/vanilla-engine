//==============================================================================
// 데스크탑 예제를 찍기 모드로 돌려 화면을 저장한다. (CI 의 platforms 워크플로, 빌드한 target/debug/examples 의 실행 파일을 바로 돈다)
// - 실행: node .github/scripts/captureexamples.cjs <저장 폴더> <이름표> [예제 이름,예제 이름,...]
//   이름표는 파일 이름 앞에 붙는다. (linux-x11, windows 같은 것) 예제 이름을 주면 그것만 돈다.
// - 예제마다 VANILLA_CAPTURE_PATH=<저장 폴더>/<이름표>_<예제>_{frame}.png, VANILLA_CAPTURE_FRAMES, VANILLA_CAPTURE_EXIT=1 과 예제의 환경 변수를 주고,
//   제한 시간 안에 끝나는지, 끝 코드가 0 인지, 청한 프레임의 그림이 모두 나왔는지 본다. 표준 출력과 오류는 <이름표>_<예제>.log 에 남긴다.
// - 원본 vanilla.js 페이지와 같은 장면을 그리는 예제는 원본 쿼리와 같은 값을 준다. (chromecompare.cjs 가 같은 값으로 원본을 찍는다)
// - 하나라도 실패하면 끝 코드 1. 결과는 <이름표>_capture.json 에도 쓴다.
// - 환경 변수 VANILLA_CI_CAPTURE_TIMEOUT 에 초를 주면 예제마다의 제한 시간을 그 값 아래로 줄인다. (그림이 나오지 않아 매달리는 기계에서 오래 기다리지 않게)
//==============================================================================
const childProcess = require("child_process");
const fs = require("fs");
const path = require("path");

const engineDirectory = path.resolve(__dirname, "..", "..");
const targetDirectory = process.env.CARGO_TARGET_DIR ? path.resolve(process.env.CARGO_TARGET_DIR) : path.join(engineDirectory, "target");
const executableSuffix = process.platform === "win32" ? ".exe" : "";
// 예제마다 찍을 프레임과 환경 변수, 제한 시간(초).
const EXAMPLE_LIST = [
	{ name: "quickstart", frames: "60" },
	{ name: "nodebasics", frames: "60" },
	{ name: "textshaping", frames: "60" },
	{ name: "textnodeextras", frames: "60" },
	{ name: "inputgallery", frames: "40,120" },
	{ name: "uigallery", frames: "120,200" },
	{ name: "uigallery2", frames: "90,180" },
	{ name: "uiscenelayout", frames: "60" },
	{ name: "engineextras", frames: "60" },
	{ name: "devtoolsshowcase", frames: "60" },
	{ name: "windowshowcase", frames: "60" },
	{ name: "panelayout", frames: "60" },
	{ name: "blendmodes", frames: "30,90" },
	{ name: "effectgallery", frames: "30,150" },
	{ name: "misctour", frames: "120" },
	{ name: "scenedocument", frames: "60" },
	{ name: "asyncawait", frames: "120" },
	{ name: "loadingshowcase", frames: "120" },
	{ name: "timelineshowcase", frames: "90,240" },
	{ name: "tilemapshowcase", frames: "60", environment: { TILEMAP_SHOWCASE_TIME: "1.25" } },
	{ name: "skeleton2d", frames: "170", environment: { SKELETON2D_FRAME: "135" } },
	{ name: "physics2d", frames: "60", environment: { PHYSICS2D_PRESTEP: "180" } },
	{ name: "toonstyle", frames: "30" },
	{ name: "toonmaterial", frames: "30", environment: { TOON_MATERIAL_FRAME: "60" } },
	{ name: "toonfx", frames: "60" },
	{ name: "toonpost", frames: "60" },
	{ name: "toonshowcase", frames: "60" },
	{ name: "toonview", frames: "60" },
	{ name: "modelviewer", frames: "60,180", timeoutSeconds: 900 },
	{ name: "oldface", frames: "61", timeoutSeconds: 900 },
];
const DEFAULT_TIMEOUT_SECONDS = 300;

//==============================================================================
// 예제 하나 돌리기. (결과 객체를 돌려준다)
//==============================================================================
function runExample(example, outputDirectory, tag) {
	const executablePath = path.join(targetDirectory, "debug", "examples", example.name + executableSuffix);
	const capturePath = path.join(outputDirectory, `${tag}_${example.name}_{frame}.png`);
	const frameList = example.frames.split(",").map((frameText) => frameText.trim());
	for (const frame of frameList) {
		fs.rmSync(capturePath.replace("{frame}", frame), { force: true });
	}
	const result = { name: example.name, frames: example.frames, environment: example.environment || {}, isPassed: false };
	if (!fs.existsSync(executablePath)) {
		result.error = "실행 파일 없음: " + executablePath;
		return result;
	}
	const environment = Object.assign({}, process.env, example.environment || {}, {
		VANILLA_CAPTURE_PATH: capturePath,
		VANILLA_CAPTURE_FRAMES: example.frames,
		VANILLA_CAPTURE_EXIT: "1",
	});
	const timeoutLimitSeconds = Number(process.env.VANILLA_CI_CAPTURE_TIMEOUT) || Infinity;
	const startTime = Date.now();
	const processResult = childProcess.spawnSync(executablePath, [], {
		cwd: engineDirectory,
		env: environment,
		timeout: Math.min(example.timeoutSeconds || DEFAULT_TIMEOUT_SECONDS, timeoutLimitSeconds) * 1000,
		maxBuffer: 64 * 1024 * 1024,
	});
	result.seconds = (Date.now() - startTime) / 1000;
	result.status = processResult.status;
	result.signal = processResult.signal;
	const logText = "# " + JSON.stringify(environment.VANILLA_CAPTURE_PATH) + "\n# 끝 코드 " + processResult.status + ", 신호 " + processResult.signal + ", " + result.seconds + " 초\n"
		+ (processResult.error ? "# 오류 " + processResult.error.message + "\n" : "")
		+ "## 표준 출력\n" + (processResult.stdout ? processResult.stdout.toString("utf8") : "")
		+ "\n## 표준 오류\n" + (processResult.stderr ? processResult.stderr.toString("utf8") : "");
	fs.writeFileSync(path.join(outputDirectory, `${tag}_${example.name}.log`), logText);
	result.missingFrames = frameList.filter((frame) => !fs.existsSync(capturePath.replace("{frame}", frame)));
	if (processResult.error) {
		result.error = processResult.error.code === "ETIMEDOUT" ? "제한 시간 넘김" : processResult.error.message;
	}
	else if (processResult.status !== 0) {
		result.error = "끝 코드 " + processResult.status + (processResult.signal ? " (" + processResult.signal + ")" : "");
	}
	else if (result.missingFrames.length > 0) {
		result.error = "그림 없음: " + result.missingFrames.join(",");
	}
	else {
		result.isPassed = true;
	}
	const stderrText = processResult.stderr ? processResult.stderr.toString("utf8") : "";
	result.stderrTail = stderrText.split("\n").filter((line) => line.trim() !== "").slice(-5);
	return result;
}

//==============================================================================
// 시작.
//==============================================================================
function main() {
	const [outputDirectoryText, tag, nameListText] = process.argv.slice(2);
	if (!outputDirectoryText || !tag) {
		console.log("쓰는 법: node .github/scripts/captureexamples.cjs <저장 폴더> <이름표> [예제 이름,...]");
		process.exit(2);
	}
	const outputDirectory = path.resolve(outputDirectoryText);
	fs.mkdirSync(outputDirectory, { recursive: true });
	const nameList = nameListText ? nameListText.split(",").map((name) => name.trim()).filter((name) => name !== "") : null;
	const exampleList = nameList ? EXAMPLE_LIST.filter((example) => nameList.includes(example.name)) : EXAMPLE_LIST;
	const resultList = [];
	for (const example of exampleList) {
		const result = runExample(example, outputDirectory, tag);
		resultList.push(result);
		console.log(`${result.isPassed ? "통과" : "실패"} ${tag} ${example.name} (${(result.seconds || 0).toFixed(1)} 초)${result.error ? ": " + result.error : ""}`);
		if (!result.isPassed) {
			for (const line of result.stderrTail || []) {
				console.log("    " + line);
			}
		}
	}
	fs.writeFileSync(path.join(outputDirectory, `${tag}_capture.json`), JSON.stringify(resultList, null, "\t"));
	const failedList = resultList.filter((result) => !result.isPassed);
	console.log(`${tag}: ${resultList.length - failedList.length} / ${resultList.length} 통과`);
	process.exit(failedList.length > 0 ? 1 : 0);
}

main();

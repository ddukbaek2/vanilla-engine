//==============================================================================
// 찍기 모드 결정성 시험. (빌드한 target/debug/examples 의 실행 파일을 숨긴 창 찍기 모드로 두 번 돌려 견준다)
// - 실행: node .github/scripts/capturedeterminism.cjs [저장 폴더] [예제 이름,예제 이름,...]
// - 같은 예제를 VANILLA_CAPTURE_PATH, VANILLA_CAPTURE_FRAMES, VANILLA_CAPTURE_EXIT=1 로 두 번 찍어, 같은 프레임 번호의 PNG 가 바이트까지 같은지 본다.
//   찍기 모드는 시간을 프레임마다 1/60 초씩 보내고, 씬을 불러온 뒤 로딩 화면 0.3 초도 엔진 시계로 센다. (원본 tests/core/capturedeterminismchrome.test.mjs 와 같은 시험)
//   찍는 프레임은 로딩 화면에서 씬으로 넘어가는 둘레(15 ~ 25)와 그 뒤(40, 90)다. 찍은 프레임에 서로 다른 그림이 있는지도 본다. (넘어가는 둘레를 봤는지)
// - 예제는 원본 시험과 같은 quickstart(Scene, 받는 애셋 없음)와 loadingshowcase(GameScene, 그림과 문서를 받는다)다.
// - 하나라도 다르면 끝 코드 1.
//==============================================================================
const childProcess = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

const engineDirectory = path.resolve(__dirname, "..", "..");
const targetDirectory = process.env.CARGO_TARGET_DIR ? path.resolve(process.env.CARGO_TARGET_DIR) : path.join(engineDirectory, "target");
const executableSuffix = process.platform === "win32" ? ".exe" : "";
const CAPTURE_FRAME_LIST = [15, 18, 19, 20, 21, 22, 25, 40, 90];
const RUN_COUNT = 2;
const TIMEOUT_MILLISECONDS = 120000;

const outputDirectory = process.argv[2] ? path.resolve(process.argv[2]) : fs.mkdtempSync(path.join(os.tmpdir(), "capturedeterminism-"));
const exampleNameList = process.argv[3] ? process.argv[3].split(",") : ["quickstart", "loadingshowcase"];
fs.mkdirSync(outputDirectory, { recursive: true });

//==============================================================================
// 예제 하나를 한 번 찍기. (프레임 번호 → PNG 바이트)
//==============================================================================
function captureExample(exampleName, runIndex) {
	const executablePath = path.join(targetDirectory, "debug", "examples", exampleName + executableSuffix);
	if (!fs.existsSync(executablePath)) {
		throw new Error("실행 파일이 없음 (cargo build --examples): " + executablePath);
	}
	const capturePath = path.join(outputDirectory, exampleName + "_run" + runIndex + "_{frame}.png");
	const environment = { ...process.env, VANILLA_CAPTURE_PATH: capturePath, VANILLA_CAPTURE_FRAMES: CAPTURE_FRAME_LIST.join(","), VANILLA_CAPTURE_EXIT: "1" };
	const result = childProcess.spawnSync(executablePath, [], { cwd: engineDirectory, env: environment, timeout: TIMEOUT_MILLISECONDS, encoding: "utf8" });
	if (result.status !== 0) {
		throw new Error(exampleName + " 끝 코드 " + result.status + " " + (result.error ? result.error.message : "") + "\n" + String(result.stderr).slice(-2000));
	}
	const pngTable = {};
	for (const frameNumber of CAPTURE_FRAME_LIST) {
		const framePath = capturePath.replace("{frame}", String(frameNumber));
		if (!fs.existsSync(framePath)) {
			throw new Error(exampleName + " " + frameNumber + " 프레임 그림이 없음: " + framePath);
		}
		pngTable[frameNumber] = fs.readFileSync(framePath);
	}
	return pngTable;
}

let failureCount = 0;
for (const exampleName of exampleNameList) {
	const runList = [];
	for (let runIndex = 0; runIndex < RUN_COUNT; ++runIndex) {
		runList.push(captureExample(exampleName, runIndex));
	}
	const differentFrameList = CAPTURE_FRAME_LIST.filter((frameNumber) => runList.some((pngTable) => !pngTable[frameNumber].equals(runList[0][frameNumber])));
	const distinctImageCount = new Set(CAPTURE_FRAME_LIST.map((frameNumber) => runList[0][frameNumber].toString("base64"))).size;
	const isSame = differentFrameList.length === 0;
	const hasTransition = distinctImageCount > 1;
	console.log((isSame && hasTransition ? "같음 " : "다름 ") + exampleName + ": 다른 프레임 [" + differentFrameList.join(",") + "], 서로 다른 그림 " + distinctImageCount + " 가지");
	if (!isSame || !hasTransition) {
		failureCount += 1;
	}
}
console.log("찍은 그림: " + outputDirectory);
process.exit(failureCount > 0 ? 1 : 0);

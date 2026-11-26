//==============================================================================
// 데스크탑 예제를 찍기 모드로 돌려 화면을 저장한다. (CI 의 platforms 워크플로, 빌드한 target/debug/examples 의 실행 파일을 바로 돈다)
// - 실행: node .github/scripts/captureexamples.cjs <저장 폴더> <이름표> [예제 이름,예제 이름,...]
//   이름표는 파일 이름 앞에 붙는다. (linux-x11, windows 같은 것) 예제 이름을 주면 그것만 돈다.
// - 예제마다 VANILLA_CAPTURE_PATH=<저장 폴더>/<이름표>_<예제>_{frame}.png, VANILLA_CAPTURE_FRAMES, VANILLA_CAPTURE_EXIT=1 과 예제의 환경 변수를 주고,
//   제한 시간 안에 끝나는지, 끝 코드가 0 인지, 청한 프레임의 그림이 모두 나왔는지 본다. 표준 출력과 오류는 <이름표>_<예제>.log 에 남긴다.
// - 원본 vanilla.js 페이지와 같은 장면을 그리는 예제는 원본 쿼리와 같은 값을 준다. (chromecompare.cjs 가 같은 값으로 원본을 찍는다)
// - 하나라도 실패하면 끝 코드 1. 결과는 <이름표>_capture.json 에도 쓴다.
// - 환경 변수 VANILLA_CI_CAPTURE_TIMEOUT 에 초를 주면 예제마다의 제한 시간을 그 값 아래로 줄인다. (그림이 나오지 않아 매달리는 기계에서 오래 기다리지 않게)
// - 멈춤과 느림을 가른다. 엔진은 찍기 모드에서 프레임마다 "Engine: 찍기 진행 프레임 N / 마지막, T 초" 를 표준 오류에 남긴다.
//   STALL_SECONDS 동안 진행 줄이 새로 나오지 않으면 멈춤으로 끊고, 진행하는 채로 예제의 제한 시간을 넘기면 느림으로 끊는다.
//   어느 쪽이든 마지막 진행 줄과 GL 렌더러(엔진이 처음에 남기는 "Engine: 찍기 GL 렌더러")를 결과에 적는다.
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
	{ name: "glyphparity", frames: "20" },
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
	{ name: "oldface", frames: "61", timeoutSeconds: 2700 },
];
// 예제 하나의 제한 시간(초). 멈춤은 STALL_SECONDS 가 잡고, 이것은 느린 기계에서 작업 시간(180 분)을 지키는 상한이다.
// - 근거(2026-11-26): 인텔 맥 CI(macos-26-intel)에서 toonmaterial 이 30 프레임에 262 초(프레임당 8.7 초), toonstyle, toonfx 는 300 초, oldface 는 900 초를 넘겼다.
//   같은 x86_64 디버그 빌드를 애플 실리콘의 로제타와 하드웨어 GL 로 돌리면 toonstyle 5.6 초, toonfx 6.2 초, oldface 7.7 초라 시간은 엔진이 아니라 러너의 GL 에서 든다.
//   (리눅스, 윈도우의 메사 llvmpipe 는 toonstyle 4, 7 초, oldface 67, 96 초) 그래서 300 초의 세 배를 기본, oldface 는 900 초의 세 배를 둔다.
const DEFAULT_TIMEOUT_SECONDS = 900;
// 진행 줄 사이를 기다리는 가장 긴 시간(초). 첫 진행 줄 앞의 준비(인텔 맥 디버그 빌드의 oldface 30 초)와 가장 느린 프레임보다 넉넉히.
const STALL_SECONDS = 180;
const PROGRESS_PREFIX = "Engine: 찍기 진행 ";
const RENDERER_PREFIX = "Engine: 찍기 GL 렌더러 ";

//==============================================================================
// 예제 하나 돌리기. (결과 객체를 돌려준다)
//==============================================================================
async function runExample(example, outputDirectory, tag) {
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
	const timeoutSeconds = Math.min(example.timeoutSeconds || DEFAULT_TIMEOUT_SECONDS, timeoutLimitSeconds);
	const processResult = await runProcess(executablePath, environment, timeoutSeconds);
	result.seconds = processResult.seconds;
	result.status = processResult.status;
	result.signal = processResult.signal;
	result.lastProgress = processResult.lastProgress;
	result.renderer = processResult.renderer;
	const logText = "# " + JSON.stringify(environment.VANILLA_CAPTURE_PATH) + "\n# 끝 코드 " + processResult.status + ", 신호 " + processResult.signal + ", " + result.seconds + " 초\n"
		+ (processResult.error ? "# 오류 " + processResult.error + "\n" : "")
		+ "## 표준 출력\n" + processResult.stdout
		+ "\n## 표준 오류\n" + processResult.stderr;
	fs.writeFileSync(path.join(outputDirectory, `${tag}_${example.name}.log`), logText);
	result.missingFrames = frameList.filter((frame) => !fs.existsSync(capturePath.replace("{frame}", frame)));
	if (processResult.error) {
		result.error = processResult.error + " (마지막 진행: " + (processResult.lastProgress || "없음") + ")";
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
	result.stderrTail = processResult.stderr.split("\n").filter((line) => line.trim() !== "" && !line.startsWith(PROGRESS_PREFIX)).slice(-5);
	return result;
}

//==============================================================================
// 실행 파일 하나를 돌려 끝나기를 기다린다. (진행 줄이 STALL_SECONDS 동안 없으면 멈춤, 제한 시간을 넘기면 느림으로 끊는다)
//==============================================================================
function runProcess(executablePath, environment, timeoutSeconds) {
	return new Promise((resolve) => {
		const startTime = Date.now();
		const stdoutChunkList = [];
		const stderrChunkList = [];
		let stderrLineRest = "";
		let lastProgressTime = startTime;
		let lastProgress = "";
		let renderer = "";
		let error = "";
		const child = childProcess.spawn(executablePath, [], { cwd: engineDirectory, env: environment });
		const stop = (reason) => {
			if (error === "") {
				error = reason;
				child.kill("SIGTERM");
				setTimeout(() => child.kill("SIGKILL"), 10000).unref();
			}
		};
		child.stdout.on("data", (chunk) => stdoutChunkList.push(chunk));
		child.stderr.on("data", (chunk) => {
			stderrChunkList.push(chunk);
			const lineList = (stderrLineRest + chunk.toString("utf8")).split("\n");
			stderrLineRest = lineList.pop();
			for (const line of lineList) {
				if (line.startsWith(PROGRESS_PREFIX)) {
					lastProgressTime = Date.now();
					lastProgress = line.slice(PROGRESS_PREFIX.length).trim() + ", 걸린 " + ((lastProgressTime - startTime) / 1000).toFixed(1) + " 초";
				}
				else if (line.startsWith(RENDERER_PREFIX)) {
					renderer = line.slice(RENDERER_PREFIX.length).trim();
				}
			}
		});
		const timer = setInterval(() => {
			const now = Date.now();
			if (now - startTime > timeoutSeconds * 1000) {
				stop("제한 시간 넘김(느림, " + timeoutSeconds + " 초)");
			}
			else if (now - lastProgressTime > STALL_SECONDS * 1000) {
				stop("멈춤(" + STALL_SECONDS + " 초 동안 진행 없음)");
			}
		}, 1000);
		child.on("error", (spawnError) => stop(spawnError.message));
		child.on("close", (status, signal) => {
			clearInterval(timer);
			resolve({
				seconds: (Date.now() - startTime) / 1000,
				status,
				signal,
				error,
				lastProgress,
				renderer,
				stdout: Buffer.concat(stdoutChunkList).toString("utf8"),
				stderr: Buffer.concat(stderrChunkList).toString("utf8"),
			});
		});
	});
}

//==============================================================================
// 시작.
//==============================================================================
async function main() {
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
		const result = await runExample(example, outputDirectory, tag);
		resultList.push(result);
		if (result.renderer && !resultList.some((previous) => previous !== result && previous.renderer)) {
			console.log(`${tag} GL 렌더러: ${result.renderer}`);
		}
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

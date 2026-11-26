//==============================================================================
// 크롬 WebAudio 로 BeepPlayer 의 톤을 그려 chrome_beep.json 으로 찍는다. (데스크탑 믹서 비프 시험의 기대값)
// - 실행: NODE_PATH=$(npm root -g) node tests/assets/mixer/chromebeep.cjs
//   puppeteer-core 와 시스템 크롬을 헤드리스로 쓴다. (창을 띄우지 않는다)
// - OfflineAudioContext 에 원본 misc/beepplayer.js 의 playTone 이 짓는 그래프를 그대로 짓는다.
//   (OscillatorNode(type, frequency.setValueAtTime) → GainNode(setValueAtTime, linearRamp, setValueAtTime, exponentialRamp) → 출력, start, stop(끝 + 0.01))
//   OfflineAudioContext 는 그리기 전에 state 가 running 이 아니라서, 원본의 음소거, 컨텍스트 확인 줄만 뺐다.
// - 그리는 도중 부르기는 suspend 로 멈춘 그 시각(currentTime)에 부른다.
// - 표본은 왼쪽 채널 전체를 Float32Array 바이트(리틀 엔디안)의 base64 로 둔다. (모노 그래프라 오른쪽은 왼쪽과 같은지 여기서 본다)
// - 크롬의 오실레이터 셈은 CPU 마다 다른 커널(arm64 NEON, x86 SSE2)을 쓴다. 어느 CPU 의 크롬인지 architecture 에 적는다. (node 의 process.arch)
//==============================================================================
const fs = require("fs");
const path = require("path");
const puppeteer = require("puppeteer-core");

const outputPath = path.join(__dirname, "chrome_beep.json");
// CHROME_PATH 가 있으면 그 크롬을 먼저 쓴다. (CI 는 browser-actions/setup-chrome 으로 판을 고정한 Chrome for Testing 을 넘긴다)
const chromePathList = [
	...(process.env.CHROME_PATH ? [process.env.CHROME_PATH] : []),
	"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
	"C:/Program Files/Google/Chrome/Application/chrome.exe",
	"/usr/bin/google-chrome",
];

(async () => {
	const executablePath = chromePathList.find((chromePath) => fs.existsSync(chromePath));
	const browser = await puppeteer.launch({ executablePath, headless: true });
	const page = await browser.newPage();
	await page.goto("about:blank");
	const result = await page.evaluate(async () => {
		// 원본 BeepPlayer.playTone 의 그래프. (음소거, 컨텍스트 확인 줄을 뺀 나머지 그대로)
		function playTone(audioContext, volume, frequency, durationSeconds, waveform, delaySeconds, volumeScale) {
			const startTime = audioContext.currentTime + delaySeconds;
			const endTime = startTime + durationSeconds;
			const attackSeconds = Math.min(0.005, durationSeconds * 0.25);
			const releaseSeconds = Math.min(0.02, durationSeconds * 0.5);
			const peakVolume = Math.max(0.0001, volume * volumeScale);

			const oscillatorNode = audioContext.createOscillator();
			oscillatorNode.type = waveform;
			oscillatorNode.frequency.setValueAtTime(frequency, startTime);

			const gainNode = audioContext.createGain();
			gainNode.gain.setValueAtTime(0.0001, startTime);
			gainNode.gain.linearRampToValueAtTime(peakVolume, startTime + attackSeconds);
			gainNode.gain.setValueAtTime(peakVolume, Math.max(startTime + attackSeconds, endTime - releaseSeconds));
			gainNode.gain.exponentialRampToValueAtTime(0.0001, endTime);

			oscillatorNode.connect(gainNode);
			gainNode.connect(audioContext.destination);
			oscillatorNode.start(startTime);
			oscillatorNode.stop(endTime + 0.01);
		}
		// 원본 playSequence 처럼 지연을 길이만큼 쌓는다. (톤 하나: { frequency, duration, waveform?, volumeScale? })
		function createSequence(toneList) {
			let delaySeconds = 0;
			return toneList.map((tone) => {
				const sequenceTone = { frequency: tone.frequency, duration: tone.duration, waveform: tone.waveform ? tone.waveform : "sine", delay: delaySeconds, volumeScale: (tone.volumeScale !== undefined) ? tone.volumeScale : 1 };
				delaySeconds += tone.duration;
				return sequenceTone;
			});
		}
		function createTone(frequency, duration, waveform, delay, volumeScale) {
			return { frequency, duration, waveform, delay, volumeScale };
		}
		// 부르기 하나: { time(0 이면 그리기 전, 아니면 그 시각에 멈춰서), toneList }
		const caseList = [
			// 프리셋. (원본 playPreset 의 톤들)
			{ name: "click", contextRate: 44100, volume: 0.35, callList: [{ time: 0, toneList: [createTone(880, 0.045, "sine", 0, 1)] }] },
			{ name: "error_square", contextRate: 48000, volume: 0.35, callList: [{ time: 0, toneList: createSequence([{ frequency: 220, duration: 0.1, waveform: "square", volumeScale: 0.7 }, { frequency: 180, duration: 0.14, waveform: "square", volumeScale: 0.7 }]) }] },
			{ name: "warning_triangle", contextRate: 44100, volume: 0.35, callList: [{ time: 0, toneList: createSequence([{ frequency: 740, duration: 0.08, waveform: "triangle" }, { frequency: 740, duration: 0.08, waveform: "triangle" }]) }] },
			// 표본율 22050 은 표 2048 칸.
			{ name: "notification_22050", contextRate: 22050, volume: 0.35, callList: [{ time: 0, toneList: createSequence([{ frequency: 1047, duration: 0.06 }, { frequency: 1319, duration: 0.1 }]) }] },
			// 표본 사이에서 시작하는 톱니파. (지연)
			{ name: "sawtooth_delay", contextRate: 48000, volume: 0.35, callList: [{ time: 0, toneList: [createTone(300, 0.05, "sawtooth", 0.0123, 1)] }] },
			// 그리는 도중 부르기: 지금 시각에 바로 시작하는 confirm 과 겹쳐 트는 click.
			{ name: "confirm_mid_render", contextRate: 48000, volume: 0.5, callList: [{ time: 0.032, toneList: createSequence([{ frequency: 660, duration: 0.06 }, { frequency: 990, duration: 0.09 }]) }, { time: 0.064, toneList: [createTone(880, 0.045, "sine", 0, 1)] }] },
			// 표본율 96000 은 표 16384 칸. 아주 짧은 톤은 attack 이 길이의 1/4, release 가 1/2.
			{ name: "short_square_96000", contextRate: 96000, volume: 0.35, callList: [{ time: 0, toneList: [createTone(3000, 0.01, "square", 0.001, 1), createTone(523, 0.02, "triangle", 0.004, 0.5)] }] },
			// 낮은 주파수는 3 점, 5 점 보간. (읽기 걸음이 0.3, 0.16 아래)
			{ name: "low_frequency", contextRate: 44100, volume: 0.35, callList: [{ time: 0, toneList: [createTone(1.5, 0.1, "sine", 0, 1), createTone(3, 0.1, "triangle", 0.003, 1)] }] },
			// 나이퀴스트 가까이. (배음을 가장 많이 덜어 낸 표)
			{ name: "near_nyquist", contextRate: 44100, volume: 0.35, callList: [{ time: 0, toneList: [createTone(15000, 0.02, "square", 0, 1), createTone(21000, 0.02, "sawtooth", 0.005, 1)] }] },
		];
		const output = { chrome: navigator.userAgent, cases: [] };
		for (const testCase of caseList) {
			let lastEndTime = 0;
			for (const call of testCase.callList) {
				for (const tone of call.toneList) {
					lastEndTime = Math.max(lastEndTime, call.time + tone.delay + tone.duration + 0.01);
				}
			}
			const renderLength = Math.ceil((lastEndTime + 0.005) * testCase.contextRate / 128) * 128;
			const context = new OfflineAudioContext(2, renderLength, testCase.contextRate);
			const callTimeList = [];
			testCase.callList.forEach((call, callIndex) => {
				const play = () => {
					callTimeList[callIndex] = context.currentTime;
					for (const tone of call.toneList) {
						playTone(context, testCase.volume, tone.frequency, tone.duration, tone.waveform, tone.delay, tone.volumeScale);
					}
				};
				if (call.time === 0) {
					play();
				}
				else {
					context.suspend(call.time).then(() => {
						play();
						context.resume();
					});
				}
			});
			const rendered = await context.startRendering();
			const left = rendered.getChannelData(0);
			const right = rendered.getChannelData(1);
			for (let index = 0; index < renderLength; index++) {
				if (left[index] !== right[index]) {
					throw new Error(`${testCase.name}: 왼쪽과 오른쪽이 ${index} 에서 다르다`);
				}
			}
			const bytes = new Uint8Array(left.buffer, left.byteOffset, left.byteLength);
			let binary = "";
			for (let index = 0; index < bytes.length; index++) {
				binary += String.fromCharCode(bytes[index]);
			}
			output.cases.push({
				name: testCase.name,
				contextRate: testCase.contextRate,
				renderLength,
				volume: testCase.volume,
				callList: testCase.callList.map((call, callIndex) => ({ ...call, callTime: callTimeList[callIndex] })),
				samples: btoa(binary),
			});
		}
		return output;
	});
	result.chrome = await browser.version();
	// 크롬은 node 와 같은 CPU 로 돈다고 본다. (arm64 맥의 크롬은 arm64 로 돈다)
	result.architecture = process.arch;
	fs.writeFileSync(outputPath, JSON.stringify(result));
	await browser.close();
	console.log(`${result.chrome} (${result.architecture}): ${result.cases.length} 경우 → ${outputPath}`);
})();

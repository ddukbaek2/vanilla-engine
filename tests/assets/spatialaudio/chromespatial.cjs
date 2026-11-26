//==============================================================================
// 크롬 WebAudio PannerNode 그래프의 결과를 chrome_spatial.json 으로 찍는다. (데스크탑 믹서 PannerNode 시험의 기대값, 초안)
// - 실행: NODE_PATH=$(npm root -g) node tests/assets/spatialaudio/chromespatial.cjs
//   puppeteer-core 와 시스템 크롬을 헤드리스로 쓴다. (창을 띄우지 않는다)
// - OfflineAudioContext 로 SpatialAudioPlayer 가 짓는 그래프를 그린다. (AudioBufferSourceNode → GainNode → PannerNode → [GainNode(버스)] → 출력)
// - 등력 좌우, 거리 모델 셋, 원뿔, 리스너 자리와 방향, 자리와 리스너의 자동화(표본마다 셈), 그리는 도중 멈춰 값 대입과 설정 바꾸기(바뀐 때만 다시 셈),
//   GainNode 가 0 인 동안 흐르는 자리 자동화(침묵 넘기기)를 한다.
// - 소리는 산수로만 짓는다. (tests/assets/mixer/chromemixer.cjs 와 같은 버퍼)
//==============================================================================
const fs = require("fs");
const path = require("path");
const puppeteer = require("puppeteer-core");

const outputPath = path.join(__dirname, "chrome_spatial.json");
const chromePathList = [
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
		const BUFFER_SAMPLE_RATE = 44100;
		const BUFFER_LENGTH = 3000;
		const RENDER_LENGTH = 4096;
		function createChannel(multiplier, modulus, offset, divisor) {
			const channel = new Float32Array(BUFFER_LENGTH);
			for (let index = 0; index < BUFFER_LENGTH; index++) {
				channel[index] = (((index * multiplier) % modulus) - offset) / divisor;
			}
			return channel;
		}
		function createBuffer(context, channelList) {
			const buffer = context.createBuffer(channelList.length, BUFFER_LENGTH, BUFFER_SAMPLE_RATE);
			channelList.forEach((channel, channelIndex) => buffer.copyToChannel(channel, channelIndex));
			return buffer;
		}
		const stereoChannelList = [createChannel(37, 200, 100, 250), createChannel(53, 160, 80, 200)];
		const monoChannelList = [createChannel(29, 180, 90, 225)];
		const defaultPanner = { distanceModel: "inverse", refDistance: 1, maxDistance: 10000, rolloffFactor: 1, coneInnerAngle: 360, coneOuterAngle: 360, coneOuterGain: 0, position: [0, 0, 0], orientation: [1, 0, 0], automation: [] };
		const defaultListener = { position: [0, 0, 0], forward: [0, 0, -1], up: [0, 1, 0], automation: [] };
		const defaultPlayer = { channels: "mono", gain: 1, gainAutomation: [], busGain: null, rate: 1, loop: false, offset: 0, stop: null };
		// 경우 하나: { name, contextRate, isStatic(자동화와 멈춤이 없고 표본율이 같아 식으로 다시 셀 수 있음), player, panner, listener, suspendList }
		// 자동화 하나: { param, type: "setValueAtTime" | "linearRamp" | "exponentialRamp" | "setTarget", value, time 이나 frame, timeConstant }
		// 멈춤 하나: { frame, actions: [{ kind: "pannerValue" | "listenerValue", param, value } | { kind: "settings", settings }] }
		const createCase = (name, contextRate, isStatic, player, panner, listener, suspendList) => ({
			name,
			contextRate,
			isStatic,
			player: { ...defaultPlayer, ...player },
			panner: { ...defaultPanner, ...panner },
			listener: { ...defaultListener, ...listener },
			suspendList,
		});
		const caseList = [
			createCase("static_mono_inverse", 44100, true, { gain: 0.8 }, { position: [3, 0, -2] }, {}, []),
			createCase("static_stereo_linear_cone", 44100, true, { channels: "stereo", gain: 0.9 }, { distanceModel: "linear", refDistance: 2, maxDistance: 20, rolloffFactor: 0.8, coneInnerAngle: 60, coneOuterAngle: 180, coneOuterGain: 0.2, position: [-5, 1, 3], orientation: [1, 0, 0.5] }, {}, []),
			createCase("static_exponential_listener_turned", 44100, true, { gain: 0.7 }, { distanceModel: "exponential", refDistance: 1.5, rolloffFactor: 1.5, position: [0.5, 4, -1] }, { position: [1, 0, 1], forward: [1, 0, -1], up: [0, 1, 0] }, []),
			createCase("static_same_position", 44100, true, { gain: 0.5 }, { coneInnerAngle: 30, coneOuterAngle: 90, coneOuterGain: 0.1, orientation: [0, 0, 0] }, {}, []),
			createCase("static_linear_equal_distances_bus", 44100, true, { channels: "stereo", gain: 0.6, busGain: 0.6 }, { distanceModel: "linear", refDistance: 5, maxDistance: 5, rolloffFactor: 0.25, position: [2, 0, 2] }, {}, []),
			createCase("automated_panner_ramp", 48000, false, { gain: 0.9, rate: 1.2, loop: true }, { position: [-10, 0, -1], automation: [
				{ param: "positionX", type: "setValueAtTime", value: -10, time: 0 },
				{ param: "positionX", type: "linearRamp", value: 10, time: 0.08 },
				{ param: "positionY", type: "setTarget", value: 2, time: 0.01, timeConstant: 0.02 },
			] }, {}, []),
			createCase("automated_listener", 44100, false, { channels: "stereo", gain: 0.8 }, { refDistance: 2, position: [1, 0, -3], coneInnerAngle: 90, coneOuterAngle: 270, coneOuterGain: 0.3, orientation: [0, 0, 1] }, { automation: [
				{ param: "forwardX", type: "setValueAtTime", value: 0, time: 0 },
				{ param: "forwardX", type: "linearRamp", value: 1, time: 0.05 },
				{ param: "positionZ", type: "setValueAtTime", value: 0.5, time: 0.02 },
				{ param: "positionZ", type: "exponentialRamp", value: 3, time: 0.07 },
			] }, []),
			createCase("suspend_value_changes", 48000, false, { gain: 0.75 }, { position: [-2, 0, -2], coneInnerAngle: 120, coneOuterAngle: 200, coneOuterGain: 0.4, orientation: [1, 0, 0] }, {}, [
				{ frame: 1024, actions: [{ kind: "pannerValue", param: "positionX", value: 4 }, { kind: "pannerValue", param: "positionZ", value: 1 }] },
				{ frame: 1536, actions: [{ kind: "listenerValue", param: "positionX", value: 1.5 }] },
				{ frame: 2048, actions: [{ kind: "settings", settings: { refDistance: 3, coneOuterGain: 0.1 } }] },
				{ frame: 2560, actions: [{ kind: "settings", settings: { distanceModel: "linear" } }] },
				{ frame: 3072, actions: [{ kind: "pannerValue", param: "orientationX", value: -1 }] },
			]),
			createCase("silent_gain_with_panner_automation", 44100, false, { gain: 1, gainAutomation: [
				{ type: "setValueAtTime", value: 0, time: 0 },
				{ type: "setValueAtTime", value: 0.7, frame: 1280 },
			] }, { position: [0, 0, -1], automation: [
				{ param: "positionX", type: "setValueAtTime", value: -6, time: 0 },
				{ param: "positionX", type: "linearRamp", value: 6, time: 0.06 },
			] }, {}, []),
		];
		const indexList = [];
		for (let index = 0; index < RENDER_LENGTH; index += 13) {
			indexList.push(index);
		}
		// 경계 시각은 frame 으로 적는다. (러스트 시험의 JSON 읽기가 실수 끝자리를 어긋내지 않게, 시각은 frame / 표본율)
		const applyAutomation = (param, automation, contextRate) => {
			const time = automation.frame !== undefined ? automation.frame / contextRate : automation.time;
			if (automation.type === "setValueAtTime") {
				param.setValueAtTime(automation.value, time);
			}
			else if (automation.type === "linearRamp") {
				param.linearRampToValueAtTime(automation.value, time);
			}
			else if (automation.type === "exponentialRamp") {
				param.exponentialRampToValueAtTime(automation.value, time);
			}
			else if (automation.type === "setTarget") {
				param.setTargetAtTime(automation.value, time, automation.timeConstant);
			}
		};
		const output = { bufferSampleRate: BUFFER_SAMPLE_RATE, bufferLength: BUFFER_LENGTH, renderLength: RENDER_LENGTH, index: indexList, cases: [] };
		for (const testCase of caseList) {
			const context = new OfflineAudioContext(2, RENDER_LENGTH, testCase.contextRate);
			const listener = context.listener;
			const listenerSetting = testCase.listener;
			const listenerParamList = ["positionX", "positionY", "positionZ", "forwardX", "forwardY", "forwardZ", "upX", "upY", "upZ"];
			const listenerValueList = [...listenerSetting.position, ...listenerSetting.forward, ...listenerSetting.up];
			listenerParamList.forEach((paramName, paramIndex) => {
				listener[paramName].value = listenerValueList[paramIndex];
			});
			for (const automation of listenerSetting.automation) {
				applyAutomation(listener[automation.param], automation, testCase.contextRate);
			}
			const player = testCase.player;
			let outputNode = context.destination;
			if (player.busGain !== null) {
				const busGainNode = context.createGain();
				busGainNode.gain.value = player.busGain;
				busGainNode.connect(context.destination);
				outputNode = busGainNode;
			}
			const pannerSetting = testCase.panner;
			const pannerNode = context.createPanner();
			pannerNode.panningModel = "equalpower";
			pannerNode.distanceModel = pannerSetting.distanceModel;
			pannerNode.refDistance = pannerSetting.refDistance;
			pannerNode.maxDistance = pannerSetting.maxDistance;
			pannerNode.rolloffFactor = pannerSetting.rolloffFactor;
			pannerNode.coneInnerAngle = pannerSetting.coneInnerAngle;
			pannerNode.coneOuterAngle = pannerSetting.coneOuterAngle;
			pannerNode.coneOuterGain = pannerSetting.coneOuterGain;
			const pannerParamList = ["positionX", "positionY", "positionZ", "orientationX", "orientationY", "orientationZ"];
			const pannerValueList = [...pannerSetting.position, ...pannerSetting.orientation];
			pannerParamList.forEach((paramName, paramIndex) => {
				pannerNode[paramName].value = pannerValueList[paramIndex];
			});
			for (const automation of pannerSetting.automation) {
				applyAutomation(pannerNode[automation.param], automation, testCase.contextRate);
			}
			pannerNode.connect(outputNode);
			const gainNode = context.createGain();
			gainNode.gain.value = player.gain;
			for (const automation of player.gainAutomation) {
				applyAutomation(gainNode.gain, automation, testCase.contextRate);
			}
			gainNode.connect(pannerNode);
			const source = context.createBufferSource();
			source.buffer = createBuffer(context, player.channels === "stereo" ? stereoChannelList : monoChannelList);
			source.loop = player.loop;
			source.playbackRate.value = player.rate;
			source.connect(gainNode);
			source.start(0, player.offset);
			if (player.stop !== null) {
				source.stop(player.stop);
			}
			for (const suspend of testCase.suspendList) {
				context.suspend(suspend.frame / testCase.contextRate).then(() => {
					for (const action of suspend.actions) {
						if (action.kind === "pannerValue") {
							pannerNode[action.param].value = action.value;
						}
						else if (action.kind === "listenerValue") {
							listener[action.param].value = action.value;
						}
						else if (action.kind === "settings") {
							for (const [settingName, settingValue] of Object.entries(action.settings)) {
								pannerNode[settingName] = settingValue;
							}
						}
					}
					context.resume();
				});
			}
			const rendered = await context.startRendering();
			const valueList = [];
			const energyList = [];
			for (let channelIndex = 0; channelIndex < 2; channelIndex++) {
				const data = rendered.getChannelData(channelIndex);
				valueList.push(indexList.map((index) => data[index]));
				let sumOfSquares = 0;
				for (let index = 0; index < data.length; index++) {
					sumOfSquares += data[index] * data[index];
				}
				energyList.push(sumOfSquares);
			}
			output.cases.push({ ...testCase, values: valueList, energy: energyList });
		}
		return output;
	});
	result.chrome = await browser.version();
	fs.writeFileSync(outputPath, JSON.stringify(result));
	await browser.close();
	console.log(`${result.chrome}: ${result.cases.length} 그래프 → ${outputPath}`);
})();

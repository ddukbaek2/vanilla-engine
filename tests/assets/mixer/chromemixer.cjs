//==============================================================================
// 크롬 WebAudio 노드 그래프의 결과를 chrome_mixer.json 으로 찍는다. (데스크탑 믹서 시험의 기대값)
// - 실행: NODE_PATH=$(npm root -g) node tests/assets/mixer/chromemixer.cjs
//   puppeteer-core 와 시스템 크롬을 헤드리스로 쓴다. (창을 띄우지 않는다)
// - OfflineAudioContext 로 원본 AudioPlayer, AudioBus, SoundSystem.playEffect 가 짓는 그래프를 그린다.
//   (AudioBufferSourceNode → GainNode → [StereoPannerNode] → GainNode(버스) 또는 출력)
// - 원본 SoundSystem 의 자동화(value 대입, setValueAtTime, linearRampToValueAtTime, exponentialRampToValueAtTime, setTargetAtTime,
//   cancelScheduledValues, stop(when))와, 그리는 도중 suspend 로 멈춰 지금 gain.value 에서 줄여 끄기(fadeOutEffects, fadeOutMusic)와 새 음악 올리기를 한다.
// - 소리는 산수로만 짓는다. (사인 같은 초월 함수는 플랫폼마다 끝자리가 다를 수 있어 쓰지 않는다)
// - 크롬의 자동화(exponentialRamp, setTarget)는 운영체제의 exp, pow 를 불러 기계마다 끝자리가 다르다. 어느 운영체제와 CPU 의 크롬인지 platform, architecture 에 적는다. (node 의 process.platform, process.arch)
//==============================================================================
const fs = require("fs");
const path = require("path");
const puppeteer = require("puppeteer-core");

const outputPath = path.join(__dirname, "chrome_mixer.json");
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
		// 플레이어 하나: { channels: "stereo" | "mono", gain(value 대입, 없으면 null), automation, pan(없으면 null), bus(버스 번호나 null), rate, loop, offset, stop(시각이나 null), startTime }
		// 자동화 하나: { type: "setValueAtTime" | "linearRamp" | "exponentialRamp" | "setTarget" | "cancel", value, time, timeConstant }
		// 멈춤 하나: { time, actions: [{ kind: "fadeOut", player, fade, margin } | { kind: "startPlayer", player }] } (startTime 이 그 시각인 플레이어를 그때 만든다)
		const createPlayer = (player, extra) => ({ gain: null, automation: [], pan: null, bus: null, rate: 1, loop: false, offset: 0, stop: null, startTime: 0, ...player, ...extra });
		const caseList = [
			{ name: "stereo_gain_same_rate", contextRate: 44100, busList: [], suspendList: [], playerList: [createPlayer({ channels: "stereo", gain: 0.8 })] },
			{ name: "stereo_gain_resampled", contextRate: 48000, busList: [], suspendList: [], playerList: [createPlayer({ channels: "stereo", gain: 0.8 })] },
			{ name: "mono_pan_bus", contextRate: 48000, busList: [{ gain: 0.7, automation: [] }], suspendList: [], playerList: [createPlayer({ channels: "mono", gain: 0.5, pan: 0.3, bus: 0 })] },
			{ name: "stereo_rate_loop_pan", contextRate: 48000, busList: [{ gain: 0.9, automation: [] }], suspendList: [], playerList: [createPlayer({ channels: "stereo", gain: 1, pan: -0.6, bus: 0, rate: 1.37, loop: true, offset: 0.01 })] },
			{ name: "mixed_bus_and_direct", contextRate: 48000, busList: [{ gain: 0.6, automation: [] }], suspendList: [], playerList: [
				createPlayer({ channels: "mono", gain: 0.9, bus: 0 }),
				createPlayer({ channels: "stereo", gain: 0.4, pan: 0, bus: 0, rate: 0.75, offset: 0.002 }),
				createPlayer({ channels: "mono", gain: 0.3, pan: 0.8, loop: true, offset: 0.05 }),
			] },
			// SoundSystem.playEffect 의 앞만 틀고 줄여 끄기와 applyVolumes 의 버스 setTargetAtTime.
			{ name: "effect_play_seconds", contextRate: 48000, busList: [{ gain: null, automation: [{ type: "setTarget", value: 0.7, time: 0, timeConstant: 0.05 }] }], suspendList: [], playerList: [
				createPlayer({ channels: "mono", gain: 0.8, pan: 0.4, bus: 0, loop: true, automation: [{ type: "setValueAtTime", value: 0.8, time: 0.02 }, { type: "linearRamp", value: 0, time: 0.05 }], stop: 0.07 }),
			] },
			// SoundSystem.fadeOutEffects: 그리는 도중 지금 gain.value 에서 0 으로.
			{ name: "fade_out_mid_render", contextRate: 48000, busList: [{ gain: null, automation: [{ type: "setTarget", value: 0.5, time: 0, timeConstant: 0.05 }] }], suspendList: [{ time: 0.032, actions: [{ kind: "fadeOut", player: 0, fade: 0.02, margin: 0.02 }] }], playerList: [
				createPlayer({ channels: "stereo", gain: 0.9, bus: 0, loop: true, automation: [{ type: "setValueAtTime", value: 0.9, time: 0.01 }, { type: "linearRamp", value: 0.3, time: 0.06 }] }),
			] },
			// SoundSystem.updateMusic, fadeOutMusic: 앞 음악을 지금 크기에서 줄이고 새 음악을 0 에서 올린다.
			{ name: "music_crossfade", contextRate: 48000, busList: [{ gain: null, automation: [{ type: "setTarget", value: 0.6, time: 0, timeConstant: 0.05 }] }], suspendList: [{ time: 0.032, actions: [{ kind: "fadeOut", player: 0, fade: 0.04, margin: 0.05 }, { kind: "startPlayer", player: 1 }] }], playerList: [
				createPlayer({ channels: "stereo", gain: 0.9, bus: 0, loop: true }),
				createPlayer({ channels: "mono", gain: null, bus: 0, loop: true, offset: 0.01, startTime: 0.032, automation: [{ type: "setValueAtTime", value: 0, time: 0.032 }, { type: "linearRamp", value: 0.9, time: 0.072 }] }),
			] },
			// 지수 램프, 목표 따라가기 뒤 램프, 지운 뒤 다시.
			{ name: "exponential_target_cancel", contextRate: 48000, busList: [], suspendList: [], playerList: [
				createPlayer({ channels: "mono", gain: null, loop: true, automation: [
					{ type: "setValueAtTime", value: 0.2, time: 0 },
					{ type: "exponentialRamp", value: 0.9, time: 0.02 },
					{ type: "setTarget", value: 0.1, time: 0.025, timeConstant: 0.008 },
					{ type: "linearRamp", value: 0.6, time: 0.05 },
					{ type: "setValueAtTime", value: 0.4, time: 0.055 },
					{ type: "linearRamp", value: 0.05, time: 0.08 },
					{ type: "cancel", time: 0.07 },
				] }),
			] },
		];
		const indexList = [];
		for (let index = 0; index < 16; index++) {
			indexList.push(index);
		}
		for (let index = 16; index < RENDER_LENGTH; index += 37) {
			indexList.push(index);
		}
		const output = { chrome: navigator.userAgent, bufferSampleRate: BUFFER_SAMPLE_RATE, bufferLength: BUFFER_LENGTH, renderLength: RENDER_LENGTH, index: indexList, cases: [] };
		for (const testCase of caseList) {
			const context = new OfflineAudioContext(2, RENDER_LENGTH, testCase.contextRate);
			const applyAutomation = (param, automationList) => {
				for (const automation of automationList) {
					if (automation.type === "setValueAtTime") {
						param.setValueAtTime(automation.value, automation.time);
					}
					else if (automation.type === "linearRamp") {
						param.linearRampToValueAtTime(automation.value, automation.time);
					}
					else if (automation.type === "exponentialRamp") {
						param.exponentialRampToValueAtTime(automation.value, automation.time);
					}
					else if (automation.type === "setTarget") {
						param.setTargetAtTime(automation.value, automation.time, automation.timeConstant);
					}
					else if (automation.type === "cancel") {
						param.cancelScheduledValues(automation.time);
					}
				}
			};
			const busNodeList = testCase.busList.map((bus) => {
				const busGain = context.createGain();
				if (bus.gain !== null) {
					busGain.gain.value = bus.gain;
				}
				applyAutomation(busGain.gain, bus.automation);
				busGain.connect(context.destination);
				return busGain;
			});
			const playerNodeList = [];
			const startPlayer = (player, playerIndex) => {
				const gainNode = context.createGain();
				if (player.gain !== null) {
					gainNode.gain.value = player.gain;
				}
				applyAutomation(gainNode.gain, player.automation);
				const outputNode = player.bus === null ? context.destination : busNodeList[player.bus];
				if (player.pan !== null) {
					const pannerNode = context.createStereoPanner();
					pannerNode.pan.value = player.pan;
					gainNode.connect(pannerNode);
					pannerNode.connect(outputNode);
				}
				else {
					gainNode.connect(outputNode);
				}
				const source = context.createBufferSource();
				source.buffer = createBuffer(context, player.channels === "stereo" ? stereoChannelList : monoChannelList);
				source.loop = player.loop;
				source.playbackRate.value = player.rate;
				source.connect(gainNode);
				source.start(0, player.offset);
				if (player.stop !== null) {
					source.stop(player.stop);
				}
				playerNodeList[playerIndex] = { gainNode, source };
			};
			testCase.playerList.forEach((player, playerIndex) => {
				if (player.startTime === 0) {
					startPlayer(player, playerIndex);
				}
			});
			for (const suspend of testCase.suspendList) {
				context.suspend(suspend.time).then(() => {
					const now = context.currentTime;
					for (const action of suspend.actions) {
						if (action.kind === "fadeOut") {
							const { gainNode, source } = playerNodeList[action.player];
							const currentGain = gainNode.gain.value;
							gainNode.gain.cancelScheduledValues(now);
							gainNode.gain.setValueAtTime(currentGain, now);
							gainNode.gain.linearRampToValueAtTime(0, now + action.fade);
							source.stop(now + action.fade + action.margin);
						}
						else if (action.kind === "startPlayer") {
							startPlayer(testCase.playerList[action.player], action.player);
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
	result.platform = process.platform;
	result.architecture = process.arch;
	fs.writeFileSync(outputPath, JSON.stringify(result));
	await browser.close();
	console.log(`${result.chrome} (${result.platform} ${result.architecture}): ${result.cases.length} 그래프 → ${outputPath}`);
})();

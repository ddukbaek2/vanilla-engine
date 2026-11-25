//==============================================================================
// 크롬 WebAudio 노드 그래프의 결과를 chrome_mixer.json 으로 찍는다. (데스크탑 믹서 시험의 기대값)
// - 실행: NODE_PATH=$(npm root -g) node tests/assets/mixer/chromemixer.cjs
//   puppeteer-core 와 시스템 크롬을 헤드리스로 쓴다. (창을 띄우지 않는다)
// - OfflineAudioContext 로 원본 AudioPlayer, AudioBus, SoundSystem.playEffect 가 짓는 그래프를 그린다.
//   (AudioBufferSourceNode → GainNode → [StereoPannerNode] → GainNode(버스) 또는 출력)
// - 소리는 산수로만 짓는다. (사인 같은 초월 함수는 플랫폼마다 끝자리가 다를 수 있어 쓰지 않는다)
//==============================================================================
const fs = require("fs");
const path = require("path");
const puppeteer = require("puppeteer-core");

const outputPath = path.join(__dirname, "chrome_mixer.json");
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
		// 플레이어 하나: { channels: "stereo" | "mono", gain, pan(없으면 null), bus(버스 번호나 null), rate, loop, offset }
		const caseList = [
			{ name: "stereo_gain_same_rate", contextRate: 44100, busGainList: [], playerList: [{ channels: "stereo", gain: 0.8, pan: null, bus: null, rate: 1, loop: false, offset: 0 }] },
			{ name: "stereo_gain_resampled", contextRate: 48000, busGainList: [], playerList: [{ channels: "stereo", gain: 0.8, pan: null, bus: null, rate: 1, loop: false, offset: 0 }] },
			{ name: "mono_pan_bus", contextRate: 48000, busGainList: [0.7], playerList: [{ channels: "mono", gain: 0.5, pan: 0.3, bus: 0, rate: 1, loop: false, offset: 0 }] },
			{ name: "stereo_rate_loop_pan", contextRate: 48000, busGainList: [0.9], playerList: [{ channels: "stereo", gain: 1, pan: -0.6, bus: 0, rate: 1.37, loop: true, offset: 0.01 }] },
			{ name: "mixed_bus_and_direct", contextRate: 48000, busGainList: [0.6], playerList: [
				{ channels: "mono", gain: 0.9, pan: null, bus: 0, rate: 1, loop: false, offset: 0 },
				{ channels: "stereo", gain: 0.4, pan: 0, bus: 0, rate: 0.75, loop: false, offset: 0.002 },
				{ channels: "mono", gain: 0.3, pan: 0.8, bus: null, rate: 1, loop: true, offset: 0.05 },
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
			const busList = testCase.busGainList.map((gain) => {
				const busGain = context.createGain();
				busGain.gain.value = gain;
				busGain.connect(context.destination);
				return busGain;
			});
			for (const player of testCase.playerList) {
				const gainNode = context.createGain();
				gainNode.gain.value = player.gain;
				const outputNode = player.bus === null ? context.destination : busList[player.bus];
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

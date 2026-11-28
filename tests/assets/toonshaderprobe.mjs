// 원본(vanilla.js) 저장소의 툰 셰이더 글을 줄마다 `이름<탭>글(탭, 줄바꿈, 역슬래시는 \t, \n, \\)` 로 찍는다.
// 러스트 시험(src/graphics3d/toon/tooninstancebuffertests.rs)이 node 로 불러 글자까지 견준다.
// 쓰는 법: node toonshaderprobe.mjs <원본 저장소 경로>
const root = process.argv[2];
const vanilla = await import(root + "/import.js");
const { composeToonProgramSources, ToonInstanceLayout, ToonScreenOutline, JOINT_BLOCK_GLSL, MORPH_GLSL } = vanilla;
const escape = (text) => text.replace(/\\/g, "\\\\").replace(/\t/g, "\\t").replace(/\n/g, "\\n");
const layout = new ToonInstanceLayout([{ name: "instanceMatrix", size: 16 }, { name: "instanceTint", size: 4 }, { name: "instanceLineId", size: 1 }, { name: "instanceShape", size: 2 }]);
const sources = composeToonProgramSources(null, layout, false, "gamma");
const lineSources = ToonScreenOutline.readProgramSources("gamma");
const output = [
	["solid", sources.solidVertexShader],
	["outline", sources.outlineVertexShader],
	["depth", sources.depthVertexShader],
	["line", lineSources[0].vertexShader],
	["jointblock", JOINT_BLOCK_GLSL],
	["morph", MORPH_GLSL],
];
for (const [name, text] of output) {
	console.log(name + "\t" + escape(text));
}

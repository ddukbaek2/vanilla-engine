//==============================================================================
// tests/glbquantization.rs 의 시험 자료 짓기. (원본 tests/graphics3d/meshquantization.test.mjs 의 GLB 짓기 함수를 그대로 돌려 GLB 와 원본 읽기가 낸 값을 뽑는다)
// - 실행: node tests/assets/glbquantization/make.mjs <vanilla.js 저장소> tests/assets/glbquantization
// - 원본 시험 파일에서 짓기 함수만 떼어 임시 폴더에 모듈로 쓰고, 원본 src 의 readGlbArrays 들과 SkinnedModel 로 읽은 값을 expected.json 에 담는다.
//==============================================================================
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
const [, , jsRoot, outDir] = process.argv;
const testText = fs.readFileSync(path.join(jsRoot, "tests/graphics3d/meshquantization.test.mjs"), "utf8");
const head = testText.slice(0, testText.indexOf("const floatArrays"));
const tailMarker = "//==============================================================================\n// json 을 그대로 두고";
const tail = testText.slice(testText.indexOf(tailMarker)).replace('finish("meshquantization");', "");
let body = (head + tail).replaceAll("../../src/", "file://" + jsRoot + "/src/").replace(/import \{ check[^\n]*\n/, "");
body += `
export { createFloatModel, createQuantizedModelA, createQuantizedModelB, createSkinnedModel, readChunks, rebuildGlb, rebuildGlbKeepingBuffers };
export { readGlbArrays, readGlbNodeArrays, readGlbNodeInfos, writeSkinPose, SkinnedModel };
`;
const modulePath = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "glbquantization-")), "builders.mjs");
fs.writeFileSync(modulePath, body);
const m = await import("file://" + modulePath);
fs.mkdirSync(outDir, { recursive: true });
const save = (name, arrayBuffer) => fs.writeFileSync(path.join(outDir, name), new Uint8Array(arrayBuffer));
const list = (value) => (value ? Array.from(value) : null);
const expected = {};
const models = { float: m.createFloatModel(), quantized_a: m.createQuantizedModelA().arrayBuffer, quantized_b: m.createQuantizedModelB().arrayBuffer };
for (const [name, arrayBuffer] of Object.entries(models)) {
	save(name + ".glb", arrayBuffer);
	const arrays = m.readGlbArrays(arrayBuffer);
	const parts = m.readGlbNodeArrays(arrayBuffer, { width: 0.02, addedParts: ["hull"] });
	const infos = m.readGlbNodeInfos(arrayBuffer);
	const meshDescription = m.SkinnedModel.createWithoutShaderProgram().interpretContent(...m.readChunks(arrayBuffer)).meshDescriptionList[0];
	expected[name] = {
		vertexData: list(arrays.vertexData), uvData: list(arrays.uvData), nodeName: arrays.materialRanges[0].nodeName,
		partNames: Object.keys(parts), partVertexData: list(parts.hull.arrays !== undefined ? parts.hull.arrays.vertexData : parts.hull.vertexData),
		partOutlineVertexData: list(parts.hull.outlineVertexData), partExtras: parts.hull.extras, center: infos.hull.center,
		pbrPositions: list(meshDescription.positions), pbrNormals: list(meshDescription.normals), pbrTextureCoordinates: list(meshDescription.textureCoordinates),
	};
}
for (const isQuantized of [false, true]) {
	const name = isQuantized ? "skinned_quantized" : "skinned_float";
	const arrayBuffer = m.createSkinnedModel(isQuantized).arrayBuffer;
	save(name + ".glb", arrayBuffer);
	const arrays = m.readGlbArrays(arrayBuffer);
	const jointMatrices = new Float32Array(16);
	m.writeSkinPose(arrays.skeleton, "Turn", 0.5, jointMatrices);
	const outlined = m.readGlbArrays(arrayBuffer, { width: 0.05, addedParts: ["skin"] });
	const model = m.SkinnedModel.createWithoutShaderProgram();
	model.interpretContent(...m.readChunks(arrayBuffer));
	expected[name] = {
		vertexData: list(arrays.vertexData), skinData: list(arrays.skinData), jointMatrices: list(jointMatrices),
		outlineVertexData: list(outlined.outlineVertexData), toonRotationValues: list(arrays.skeleton.animations.Turn.channels[0].values),
		inverseBindMatrices: list(arrays.skeleton.inverseBindMatrices), pbrRotationValues: list(model.getAnimationList()[0].channels[0].values),
	};
}
const messages = {};
for (const extensionName of ["EXT_meshopt_compression", "KHR_draco_mesh_compression", "VENDOR_unknown_thing"]) {
	const sourceChunks = m.readChunks(m.createQuantizedModelA().arrayBuffer);
	const json = sourceChunks[0];
	json.extensionsRequired.push(extensionName);
	const arrayBuffer = m.rebuildGlb(json, sourceChunks[1]);
	save("required_" + extensionName + ".glb", arrayBuffer);
	messages[extensionName] = {};
	for (const [readerName, readEvent] of [["toon", () => m.readGlbArrays(arrayBuffer)], ["pbr", () => m.SkinnedModel.createWithoutShaderProgram().interpretContent(...m.readChunks(arrayBuffer))]]) {
		try { readEvent(); messages[extensionName][readerName] = null; } catch (error) { messages[extensionName][readerName] = String(error.message); }
	}
}
{
	const sourceChunks = m.readChunks(m.createQuantizedModelA().arrayBuffer);
	const json = sourceChunks[0];
	json.extensionsUsed.push("EXT_meshopt_compression");
	const arrayBuffer = m.rebuildGlb(json, sourceChunks[1]);
	const fallbackJson = m.readChunks(arrayBuffer)[0];
	fallbackJson.buffers.push({ byteLength: fallbackJson.buffers[0].byteLength, extensions: { EXT_meshopt_compression: { fallback: true } } });
	fallbackJson.bufferViews[0].buffer = 1;
	const fallbackArrayBuffer = m.rebuildGlbKeepingBuffers(fallbackJson, m.readChunks(arrayBuffer)[1]);
	save("fallback_buffer.glb", fallbackArrayBuffer);
	messages.fallback = {};
	for (const [readerName, readEvent] of [["toon", () => m.readGlbArrays(fallbackArrayBuffer)], ["pbr", () => m.SkinnedModel.createWithoutShaderProgram().interpretContent(...m.readChunks(fallbackArrayBuffer))]]) {
		try { readEvent(); messages.fallback[readerName] = null; } catch (error) { messages.fallback[readerName] = String(error.message); }
	}
}
expected.messages = messages;
fs.writeFileSync(path.join(outDir, "expected.json"), JSON.stringify(expected, null, "\t") + "\n");
console.log(Object.keys(expected).join(","), JSON.stringify(messages, null, 1));

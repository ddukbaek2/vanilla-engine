//==============================================================================
// 네온 호라이즌 예제. (데스크탑, 원본 examples/neonhorizon 과 같은 장면, 같은 UI, 같은 동작)
// - 초콜릿 하늘, 줄무늬 없는 바닐라 태양, 별, 유성, 네온 그리드 지형, 부스트 빛살, 먼지 파티클, 함대(전투순양함 1척 + 호위함 3척, 선미 글로우와 기함 주포 광선).
//   블룸, 색수차, 필름 그레인, 필믹 톤맵, 비네트, 레터박스 합성과 우측 상단 옵션 패널(토글 일곱, 슬라이더 둘)도 같다.
// - 조작: 마우스 이동은 시선 시차, 누르고 있으면 부스트, 누른 채 좌우로 끌면 좌우 이동. 패널 머리를 누르면 펴지고 접힌다.
// - 모델(battlecruiser.glb, escort.glb)은 원본처럼 코드로 만든 것을 examples/assets/neonhorizon 에 둔다. (이 저장소 MIT)
// - 원본은 하늘, 지형, 합성을 자체 WebGL 로 그린다. 여기서도 같은 셰이더를 엔진의 ShaderProgram, FullscreenPass, RenderTarget 으로 돌린다.
// - 갱신은 화면 저장 중에는 프레임마다 1/60초 고정이고, 평소에는 실제 시간이다.
// - 실행: cargo run --release --example neonhorizon (20초 뒤 스스로 끝난다)
//   화면 저장: VANILLA_CAPTURE_PATH=shots/neon_{frame}.png VANILLA_CAPTURE_FRAMES=240 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::path::PathBuf;
use vanilla::core::fullscreenpass::FullscreenPass;
use vanilla::core::graphic::GlTexture;
use vanilla::core::shaderprogram::ShaderProgram;
use vanilla::{Color, Engine, EngineConfiguration, GlContext, Graphic, RenderTarget, Rect, Scene, SceneBase, SkinnedModel, SkinnedModelLoadHandle, SkinnedModelRenderer, TextAlign, TextBaseline, Vector2};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 예제 애셋 폴더.
const EXAMPLE_ASSET_DIRECTORY: &str = "examples/assets/neonhorizon";

// 스스로 끝내는 시각. (초)
const EXIT_SECONDS: f64 = 20.0;

// 화면 저장 중 한 걸음. (초)
const FIXED_TIME_STEP: f64 = 1.0 / 60.0;

// 지형 그리드. (x: -40..40, z: 0..48, 셀 크기 1)
const GRID_HALF_WIDTH: i32 = 40;
const GRID_DEPTH: i32 = 48;
const GRID_CELL_SIZE: f64 = 1.0;

// 빛살과 파티클 수.
const STREAK_COUNT: usize = 300;
const STREAK_FLOATS_PER_VERTEX: usize = 5;
const PARTICLE_COUNT: usize = 500;

// 플레이어 좌우 이동 한계.
const PLAYER_MOVE_LIMIT: f64 = 14.0;

// 옵션 패널 꼴. (CSS 픽셀, 우측 상단)
const PANEL_WIDTH: f64 = 236.0;
const PANEL_HEADER_HEIGHT: f64 = 40.0;
const PANEL_ROW_HEIGHT: f64 = 30.0;
const PANEL_MARGIN_RIGHT: f64 = 18.0;
const SLIDER_TRACK_WIDTH: f64 = 90.0;

// 모델 로컬 선미(엔진 글로우 앵커)와 프로우(기함 주포 앵커).
const SHIP_STERN_LOCAL: [f64; 3] = [0.0, 0.0, -0.56];
const SHIP_PROW_LOCAL: [f64; 3] = [0.0, 0.0, 0.47];

// 배경. (초콜릿 하늘 + 바닐라 태양 + 별 + 유성 + 지평선 글로우 + 함대 글로우와 주포)
const BACKGROUND_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentCoordinate;
uniform float elapsedTime;
uniform float aspectRatio;
uniform float shipStrength;
uniform vec4 shipAnchorList[4];
uniform vec4 beamAnchor;
out vec4 outputColor;

float hash21(vec2 point) {
	point = fract(point * vec2(234.34, 435.345));
	point += dot(point, point + 34.23);
	return fract(point.x * point.y);
}

float segmentDistance(vec2 point, vec2 segmentStart, vec2 segmentEnd) {
	vec2 startToPoint = point - segmentStart;
	vec2 startToEnd = segmentEnd - segmentStart;
	float projection = clamp(dot(startToPoint, startToEnd) / dot(startToEnd, startToEnd), 0.0, 1.0);
	return length(startToPoint - startToEnd * projection);
}

void main() {
	vec2 screenPoint = vec2(fragmentCoordinate.x * aspectRatio, fragmentCoordinate.y);
	float horizonY = -0.08;

	vec3 skyTopColor = vec3(0.045, 0.028, 0.014);
	vec3 skyHorizonColor = vec3(0.46, 0.26, 0.1);
	float skyGradient = smoothstep(1.1, horizonY, fragmentCoordinate.y);
	vec3 color = mix(skyTopColor, skyHorizonColor, skyGradient * skyGradient);

	if (fragmentCoordinate.y > horizonY) {
		vec2 starGrid = screenPoint * 60.0;
		vec2 starCell = floor(starGrid);
		float starRandom = hash21(starCell);
		if (starRandom > 0.93) {
			vec2 starLocalPoint = fract(starGrid) - vec2(fract(starRandom * 13.7), fract(starRandom * 71.3));
			float starDistance = length(starLocalPoint);
			float starSize = 0.04 + fract(starRandom * 93.1) * 0.08;
			float starIntensity = smoothstep(starSize, 0.0, starDistance);
			float twinkle = 0.45 + 0.55 * sin(elapsedTime * (2.0 + starRandom * 6.0) + starRandom * 40.0);
			float starFade = smoothstep(horizonY, horizonY + 0.35, fragmentCoordinate.y);
			color += vec3(1.0, 0.95, 0.84) * starIntensity * twinkle * starFade * 0.8;
		}
	}

	if (shipStrength > 0.001) {
		for (int shipIndex = 0; shipIndex < 4; ++shipIndex) {
			vec4 shipAnchor = shipAnchorList[shipIndex];
			if (shipAnchor.w > 0.001) {
				float enginePulse = 0.75 + 0.25 * sin(elapsedTime * 3.1 + float(shipIndex) * 1.7);
				float engineDistance = length(screenPoint - shipAnchor.xy) / max(shipAnchor.z, 0.02);
				color += vec3(1.0, 0.72, 0.32) * exp(-engineDistance * 95.0) * enginePulse * 0.9 * shipAnchor.w;
				color += vec3(1.0, 0.6, 0.25) * exp(-engineDistance * 22.0) * enginePulse * 0.25 * shipAnchor.w;
			}
		}

		float fireCycle = 22.0;
		float firePhase = fract(elapsedTime / fireCycle);
		vec2 beamOrigin = beamAnchor.xy;
		vec2 beamDirection = normalize(beamAnchor.zw);

		float chargeAmount = smoothstep(0.05, 0.1, firePhase) * (1.0 - smoothstep(0.1, 0.115, firePhase)) * shipStrength;
		float chargeDistance = length(screenPoint - beamOrigin);
		color += vec3(1.0, 0.85, 0.5) * exp(-chargeDistance * 90.0) * chargeAmount * 2.2;
		color += vec3(1.0, 0.7, 0.35) * exp(-chargeDistance * 22.0) * chargeAmount * 0.5;

		float beamStrength = smoothstep(0.1, 0.108, firePhase) * (1.0 - smoothstep(0.15, 0.17, firePhase)) * shipStrength;
		if (beamStrength > 0.001) {
			float beamFlicker = 0.85 + 0.15 * sin(elapsedTime * 47.0);
			float beamDistance = segmentDistance(screenPoint, beamOrigin, beamOrigin + beamDirection * 5.0);
			color += vec3(1.0, 0.97, 0.85) * exp(-beamDistance * 220.0) * beamStrength * beamFlicker * 3.0;
			color += vec3(1.0, 0.8, 0.45) * exp(-beamDistance * 40.0) * beamStrength * beamFlicker * 0.9;
			color += vec3(1.0, 0.6, 0.3) * exp(-beamDistance * 9.0) * beamStrength * 0.25;
			color += vec3(1.0, 0.95, 0.8) * exp(-chargeDistance * 30.0) * beamStrength * 1.2;
		}
	}

	{
		float shootingCycle = 6.0;
		float shootingSeed = floor(elapsedTime / shootingCycle);
		float shootingPhase = fract(elapsedTime / shootingCycle);
		float seedRandom = hash21(vec2(shootingSeed, 17.0));
		if (shootingPhase < 0.16) {
			float travel = shootingPhase / 0.16;
			vec2 startPoint = vec2(-aspectRatio + seedRandom * aspectRatio * 2.0, 0.95 - seedRandom * 0.3);
			vec2 velocity = vec2(1.6 + seedRandom * 0.8, -0.55);
			vec2 headPoint = startPoint + velocity * travel;
			vec2 tailPoint = headPoint - velocity * 0.16;
			float streakDistance = segmentDistance(screenPoint, tailPoint, headPoint);
			float streakFade = (1.0 - travel) * smoothstep(horizonY + 0.1, horizonY + 0.3, fragmentCoordinate.y);
			color += vec3(1.0, 0.94, 0.8) * exp(-streakDistance * 220.0) * streakFade * 1.6;
			color += vec3(0.9, 0.68, 0.38) * exp(-streakDistance * 40.0) * streakFade * 0.35;
		}
	}

	vec2 sunCenter = vec2(0.0, horizonY + 0.18);
	float sunDistance = length(vec2(screenPoint.x, (fragmentCoordinate.y - sunCenter.y) * 1.0) - vec2(sunCenter.x, 0.0));
	float sunRadius = 0.34;
	float sunDisc = smoothstep(sunRadius, sunRadius - 0.012, sunDistance);
	float sunLocalY = (fragmentCoordinate.y - (sunCenter.y - sunRadius)) / (sunRadius * 2.0);
	sunDisc *= smoothstep(-0.08, 0.3, sunLocalY);
	vec3 sunColor = mix(vec3(0.95, 0.42, 0.12), vec3(1.0, 0.93, 0.66), clamp(sunLocalY, 0.0, 1.0));
	color = mix(color, sunColor, sunDisc);

	float sunGlow = exp(-sunDistance * 4.2) * 0.38;
	color += vec3(1.0, 0.62, 0.24) * sunGlow;
	float horizonGlow = exp(-abs(fragmentCoordinate.y - horizonY) * 9.0) * 0.5;
	color += vec3(0.95, 0.55, 0.2) * horizonGlow;

	outputColor = vec4(color, 1.0);
}";

// 지형. (정점에서 높이 변위 + 등장 페이드, 프래그먼트에서 네온 그리드)
const TERRAIN_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec2 vertexGridPosition;
uniform mat4 projectionMatrix;
uniform mat4 viewMatrix;
uniform float scrollOffset;
out vec3 fragmentWorldPosition;
out float fragmentViewDistance;
out float fragmentHeight;

float latticeRandom(int cellX, int cellZ) {
	uint hashValue = uint(cellX) * 374761393u + uint(cellZ) * 668265263u;
	hashValue = (hashValue ^ (hashValue >> 13u)) * 1274126177u;
	hashValue = hashValue ^ (hashValue >> 16u);
	return float(hashValue & 65535u) / 65535.0;
}

float valueNoise(vec2 point) {
	vec2 cellOrigin = floor(point);
	vec2 cellFraction = point - cellOrigin;
	vec2 weight = cellFraction * cellFraction * (3.0 - 2.0 * cellFraction);
	int cellX = int(cellOrigin.x);
	int cellZ = int(cellOrigin.y);
	float value00 = latticeRandom(cellX, cellZ);
	float value10 = latticeRandom(cellX + 1, cellZ);
	float value01 = latticeRandom(cellX, cellZ + 1);
	float value11 = latticeRandom(cellX + 1, cellZ + 1);
	float valueX0 = mix(value00, value10, weight.x);
	float valueX1 = mix(value01, value11, weight.x);
	return mix(valueX0, valueX1, weight.y);
}

float terrainHeight(vec2 worldXZ) {
	float sideFactor = smoothstep(2.2, 9.0, abs(worldXZ.x));
	float noiseLarge = valueNoise(worldXZ * 0.06);
	float noiseMedium = valueNoise(worldXZ * 0.15 + 37.0);
	float noiseSmall = valueNoise(worldXZ * 0.37 + 91.0);
	float combined = noiseLarge * 1.4 + noiseMedium * 0.65 + noiseSmall * 0.25;
	return pow(max(combined - 0.7, 0.0), 1.5) * 4.6 * sideFactor;
}

void main() {
	float snappedOffset = floor(scrollOffset);
	float residualOffset = scrollOffset - snappedOffset;
	vec2 worldXZ = vec2(vertexGridPosition.x, vertexGridPosition.y + snappedOffset);
	float height = terrainHeight(worldXZ);
	float viewDistance = vertexGridPosition.y - residualOffset;

	float emergenceFade = 1.0 - smoothstep(34.0, 44.0, viewDistance);
	height *= emergenceFade;

	vec3 renderPosition = vec3(vertexGridPosition.x, height, -viewDistance);
	fragmentWorldPosition = vec3(worldXZ.x, height, worldXZ.y);
	fragmentViewDistance = viewDistance;
	fragmentHeight = height;
	gl_Position = projectionMatrix * viewMatrix * vec4(renderPosition, 1.0);
}";
const TERRAIN_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec3 fragmentWorldPosition;
in float fragmentViewDistance;
in float fragmentHeight;
uniform float elapsedTime;
uniform float scrollOffset;
uniform vec3 cameraPosition;
out vec4 outputColor;

void main() {
	vec2 gridCoordinate = fragmentWorldPosition.xz * 0.5;
	vec2 gridDistance = abs(fract(gridCoordinate - 0.5) - 0.5) / fwidth(gridCoordinate);
	float lineDistance = min(gridDistance.x, gridDistance.y);
	float lineIntensity = 1.0 - smoothstep(0.0, 1.6, lineDistance);

	float heightRatio = clamp(fragmentHeight * 0.22, 0.0, 1.0);
	vec3 lineColor = mix(vec3(0.95, 0.58, 0.14), vec3(1.0, 0.95, 0.75), heightRatio);

	vec3 surfacePosition = vec3(fragmentWorldPosition.x, fragmentHeight, fragmentViewDistance);
	vec3 surfaceNormal = normalize(cross(dFdx(surfacePosition), dFdy(surfacePosition)));
	if (surfaceNormal.y < 0.0) {
		surfaceNormal = -surfaceNormal;
	}
	vec3 sunDirection = normalize(vec3(-fragmentWorldPosition.x * 0.02, 0.25, 1.0));
	float sunLight = pow(max(dot(surfaceNormal, sunDirection), 0.0), 2.0);
	float skyLight = max(surfaceNormal.y, 0.0);

	vec3 floorColor = vec3(0.05, 0.032, 0.016);
	floorColor += vec3(0.42, 0.24, 0.09) * sunLight * smoothstep(0.2, 2.5, fragmentHeight);
	floorColor += vec3(0.12, 0.1, 0.07) * skyLight * 0.35;

	float lineGlow = 1.0 - smoothstep(0.0, 6.0, lineDistance);
	vec3 color = floorColor + lineColor * lineIntensity * 1.35 + lineColor * lineGlow * 0.22;

	float lateralGlow = exp(-abs(fragmentWorldPosition.x) * 1.6);
	float coreGlow = exp(-abs(fragmentWorldPosition.x) * 5.0);
	float flowShimmer = sin(fragmentViewDistance * 0.55 + scrollOffset * 1.4) * 0.5
	                  + sin(fragmentViewDistance * 0.23 + scrollOffset * 0.8 + 1.7) * 0.35
	                  + sin(fragmentViewDistance * 0.91 + scrollOffset * 2.3 + 3.9) * 0.15;
	float streamFlow = 0.55 + 0.3 * flowShimmer;
	color += vec3(1.0, 0.88, 0.55) * (lateralGlow * 0.3 + coreGlow * 0.45) * streamFlow;

	color *= 0.94 + 0.06 * sin(elapsedTime * 2.2 + fragmentViewDistance * 0.35);

	float lateralDistance = max(abs(fragmentWorldPosition.x - cameraPosition.x) - 12.0, 0.0);
	float fogFactor = smoothstep(18.0, 46.0, fragmentViewDistance + lateralDistance * 1.6);
	vec3 fogColor = vec3(0.42, 0.24, 0.1);
	color = mix(color, fogColor, fogFactor);

	outputColor = vec4(color, 1.0);
}";

// 부스트 빛살. (꾹 누르면 가속하며 카메라를 스쳐 지나가는 먼지빛살)
const STREAK_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec4 streakData;
in float streakEndFlag;
uniform mat4 projectionMatrix;
uniform mat4 viewMatrix;
uniform float streakScroll;
uniform float boostFactor;
uniform float heightOffset;
uniform float lateralOffset;
out float fragmentSeed;
out float fragmentTailRatio;

void main() {
	float seed = streakData.w;
	float speedFactor = 0.7 + seed * 0.6;
	float loopZ = 46.0 - mod(streakData.z + streakScroll * speedFactor, 46.0);
	float streakLength = (0.6 + seed * 1.0) * (0.35 + boostFactor * 2.2);
	vec3 worldPosition = vec3(streakData.x + lateralOffset, streakData.y + heightOffset, loopZ + streakEndFlag * streakLength);
	vec3 renderPosition = vec3(worldPosition.x, worldPosition.y, -worldPosition.z);
	gl_Position = projectionMatrix * viewMatrix * vec4(renderPosition, 1.0);
	fragmentSeed = seed;
	fragmentTailRatio = streakEndFlag;
}";
const STREAK_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in float fragmentSeed;
in float fragmentTailRatio;
uniform float boostFactor;
uniform float fadeFactor;
out vec4 outputColor;

void main() {
	vec3 goldColor = vec3(1.0, 0.72, 0.35);
	vec3 creamColor = vec3(0.97, 0.9, 0.68);
	vec3 color = mix(goldColor, creamColor, fract(fragmentSeed * 3.77));
	float headFade = 1.0 - fragmentTailRatio;
	float boostAlpha = 0.04 + boostFactor * 0.9;
	outputColor = vec4(color * headFade * boostAlpha * fadeFactor, 0.0);
}";

// 부유 먼지 파티클. (가산 합성 포인트 스프라이트, 지형 깊이에 가려짐)
const PARTICLE_VERTEXSHADER_SOURCE: &str = "#version 300 es
in vec4 vertexParticle;
uniform mat4 projectionMatrix;
uniform mat4 viewMatrix;
uniform float elapsedTime;
uniform float heightOffset;
uniform float lateralOffset;
out float fragmentSeed;
out float fragmentProximityFade;

void main() {
	float seed = vertexParticle.w;
	float floatY = mod(vertexParticle.y + elapsedTime * (0.12 + seed * 0.25), 7.0);
	float swayX = vertexParticle.x + sin(elapsedTime * (0.4 + seed) + seed * 40.0) * 0.4;
	float loopZ = mod(vertexParticle.z + elapsedTime * 1.2, 38.0) + 2.0;
	vec3 renderPosition = vec3(swayX + lateralOffset, floatY + 0.2 + heightOffset, -loopZ);
	vec4 viewPosition = viewMatrix * vec4(renderPosition, 1.0);
	gl_Position = projectionMatrix * viewPosition;
	float viewDepth = max(-viewPosition.z, 0.1);
	gl_PointSize = clamp(70.0 / viewDepth, 1.5, 14.0);
	fragmentProximityFade = smoothstep(1.2, 5.0, viewDepth);
	fragmentSeed = seed;
}";
const PARTICLE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in float fragmentSeed;
in float fragmentProximityFade;
uniform float elapsedTime;
uniform float fadeFactor;
out vec4 outputColor;

void main() {
	vec2 pointOffset = gl_PointCoord - 0.5;
	float pointDistance = length(pointOffset);
	float alpha = smoothstep(0.5, 0.05, pointDistance);
	vec3 goldColor = vec3(1.0, 0.72, 0.35);
	vec3 creamColor = vec3(0.97, 0.9, 0.68);
	vec3 color = mix(goldColor, creamColor, fract(fragmentSeed * 7.31));
	float twinkle = 0.55 + 0.45 * sin(elapsedTime * (1.5 + fragmentSeed * 3.0) + fragmentSeed * 50.0);
	outputColor = vec4(color * alpha * twinkle * fragmentProximityFade * fadeFactor * 0.45, 0.0);
}";

// 블룸 1단계: 밝은 영역 추출. (씬 → 하프 해상도)
const BRIGHTPASS_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
out vec4 outputColor;

void main() {
	vec3 sceneColor = texture(sceneTexture, fragmentTextureCoordinate).rgb;
	float brightness = max(max(sceneColor.r, sceneColor.g), sceneColor.b);
	float extractFactor = smoothstep(0.5, 0.95, brightness);
	outputColor = vec4(sceneColor * extractFactor, 1.0);
}";

// 블룸 2단계: 9탭 가우시안 블러. (가로, 세로 분리)
const BLUR_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sourceTexture;
uniform vec2 blurDirection;
out vec4 outputColor;

void main() {
	float weights[5] = float[5](0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
	vec3 color = texture(sourceTexture, fragmentTextureCoordinate).rgb * weights[0];
	for (int tapIndex = 1; tapIndex < 5; ++tapIndex) {
		vec2 tapOffset = blurDirection * float(tapIndex) * 1.6;
		color += texture(sourceTexture, fragmentTextureCoordinate + tapOffset).rgb * weights[tapIndex];
		color += texture(sourceTexture, fragmentTextureCoordinate - tapOffset).rgb * weights[tapIndex];
	}
	outputColor = vec4(color, 1.0);
}";

// 최종 합성: 씬 + 블룸 + 색수차 + 필믹 톤맵 + 비네트 + 레터박스.
const COMPOSITE_FRAGMENTSHADER_SOURCE: &str = "#version 300 es
precision highp float;
in vec2 fragmentCoordinate;
in vec2 fragmentTextureCoordinate;
uniform sampler2D sceneTexture;
uniform sampler2D bloomTexture;
uniform float elapsedTime;
uniform float boostFactor;
uniform vec2 resolution;
uniform float bloomStrength;
uniform float aberrationStrength;
uniform float filmStrength;
uniform float letterboxStrength;
out vec4 outputColor;

float hash21(vec2 point) {
	point = fract(point * vec2(234.34, 435.345));
	point += dot(point, point + 34.23);
	return fract(point.x * point.y);
}

void main() {
	vec2 centerOffset = fragmentTextureCoordinate - 0.5;
	float aberrationAmount = dot(centerOffset, centerOffset) * (0.012 + boostFactor * 0.02) * aberrationStrength;
	vec2 aberrationOffset = centerOffset * aberrationAmount;
	float channelRed = texture(sceneTexture, fragmentTextureCoordinate + aberrationOffset).r;
	float channelGreen = texture(sceneTexture, fragmentTextureCoordinate).g;
	float channelBlue = texture(sceneTexture, fragmentTextureCoordinate - aberrationOffset).b;
	vec3 color = vec3(channelRed, channelGreen, channelBlue);

	vec3 bloomColor = texture(bloomTexture, fragmentTextureCoordinate).rgb;
	color += bloomColor * 0.95 * bloomStrength;

	float scanline = sin(fragmentTextureCoordinate.y * resolution.y * 1.6) * 0.5 + 0.5;
	color *= mix(1.0, 0.97 + scanline * 0.03, filmStrength);

	float grain = hash21(fragmentTextureCoordinate * resolution.xy + fract(elapsedTime) * 371.0);
	color += (grain - 0.5) * 0.012 * filmStrength;

	float vignette = 1.0 - dot(fragmentCoordinate * 0.5, fragmentCoordinate * 0.5);
	color *= clamp(vignette, 0.0, 1.0);

	color = clamp((color * (2.51 * color + 0.03)) / (color * (2.43 * color + 0.59) + 0.14), 0.0, 1.0);

	float letterboxCurve = smoothstep(0.0, 0.01, fragmentTextureCoordinate.y - 0.045)
	                     * smoothstep(0.0, 0.01, 0.955 - fragmentTextureCoordinate.y);
	color *= mix(1.0, letterboxCurve, letterboxStrength);

	float introFade = smoothstep(0.2, 2.6, elapsedTime);
	color *= introFade;

	outputColor = vec4(color, 1.0);
}";

//==============================================================================
// 옵션 정의. (토글 일곱, 슬라이더 둘)
//==============================================================================
struct OptionDefinition {
	name: &'static str,
	label: &'static str,
	is_slider: bool,
	minimum_value: f64,
	maximum_value: f64,
}

const OPTION_DEFINITION_LIST: [OptionDefinition; 9] = [
	OptionDefinition { name: "bloom", label: "BLOOM", is_slider: false, minimum_value: 0.0, maximum_value: 0.0 },
	OptionDefinition { name: "aberration", label: "CHROMATIC", is_slider: false, minimum_value: 0.0, maximum_value: 0.0 },
	OptionDefinition { name: "film", label: "FILM GRAIN", is_slider: false, minimum_value: 0.0, maximum_value: 0.0 },
	OptionDefinition { name: "letterbox", label: "LETTERBOX", is_slider: false, minimum_value: 0.0, maximum_value: 0.0 },
	OptionDefinition { name: "particles", label: "PARTICLES", is_slider: false, minimum_value: 0.0, maximum_value: 0.0 },
	OptionDefinition { name: "streaks", label: "STREAKS", is_slider: false, minimum_value: 0.0, maximum_value: 0.0 },
	OptionDefinition { name: "ships", label: "SHIPS", is_slider: false, minimum_value: 0.0, maximum_value: 0.0 },
	OptionDefinition { name: "glow", label: "GLOW", is_slider: true, minimum_value: 0.0, maximum_value: 1.5 },
	OptionDefinition { name: "speed", label: "SPEED", is_slider: true, minimum_value: 0.4, maximum_value: 2.0 },
];

// 토글 번호. (OPTION_DEFINITION_LIST 의 앞 일곱)
const TOGGLE_BLOOM: usize = 0;
const TOGGLE_ABERRATION: usize = 1;
const TOGGLE_FILM: usize = 2;
const TOGGLE_LETTERBOX: usize = 3;
const TOGGLE_PARTICLES: usize = 4;
const TOGGLE_STREAKS: usize = 5;
const TOGGLE_SHIPS: usize = 6;
const TOGGLE_COUNT: usize = 7;

//==============================================================================
// 함선 배치. (월드 좌표, 카메라는 -z 를 바라본다. 기함 1척 + 호위함 3척이 원거리 상공에 떠 있다)
//==============================================================================
struct ShipDefinition {
	is_flagship: bool,
	base_position: [f64; 3],
	yaw_radian: f64,
	pitch_radian: f64,
	scale: f64,
	drift_seed: f64,
	fade: f64,
}

const SHIP_DEFINITION_LIST: [ShipDefinition; 4] = [
	ShipDefinition { is_flagship: true, base_position: [-34.0, 21.0, -78.0], yaw_radian: 1.35, pitch_radian: -0.04, scale: 34.0, drift_seed: 0.0, fade: 0.95 },
	ShipDefinition { is_flagship: false, base_position: [34.0, 25.0, -105.0], yaw_radian: -1.35, pitch_radian: 0.03, scale: 20.0, drift_seed: 13.0, fade: 0.8 },
	ShipDefinition { is_flagship: false, base_position: [14.0, 32.0, -135.0], yaw_radian: -1.2, pitch_radian: 0.05, scale: 13.0, drift_seed: 31.0, fade: 0.62 },
	ShipDefinition { is_flagship: false, base_position: [55.0, 13.0, -150.0], yaw_radian: -1.5, pitch_radian: -0.02, scale: 10.0, drift_seed: 47.0, fade: 0.5 },
];

//==============================================================================
// 행렬 수학. (열우선, 원본과 같은 식)
//==============================================================================
fn create_perspective_matrix(field_of_view_radian: f64, aspect_ratio: f64, near_distance: f64, far_distance: f64) -> [f32; 16] {
	let focal_length = 1.0 / (field_of_view_radian / 2.0).tan();
	let range_inverse = 1.0 / (near_distance - far_distance);
	[
		(focal_length / aspect_ratio) as f32, 0.0, 0.0, 0.0,
		0.0, focal_length as f32, 0.0, 0.0,
		0.0, 0.0, ((near_distance + far_distance) * range_inverse) as f32, -1.0,
		0.0, 0.0, (near_distance * far_distance * range_inverse * 2.0) as f32, 0.0,
	]
}

fn create_view_matrix(camera_position: [f64; 3], pitch_radian: f64, yaw_radian: f64) -> [f32; 16] {
	let (sin_pitch, cos_pitch) = pitch_radian.sin_cos();
	let (sin_yaw, cos_yaw) = yaw_radian.sin_cos();
	let right_axis = [cos_yaw, 0.0, sin_yaw];
	let up_axis = [-sin_pitch * sin_yaw, cos_pitch, sin_pitch * cos_yaw];
	let back_axis = [-sin_yaw * cos_pitch, -sin_pitch, cos_yaw * cos_pitch];
	let dot = |axis: &[f64; 3]| -> f32 {
		-(axis[0] * camera_position[0] + axis[1] * camera_position[1] + axis[2] * camera_position[2]) as f32
	};
	[
		right_axis[0] as f32, up_axis[0] as f32, back_axis[0] as f32, 0.0,
		right_axis[1] as f32, up_axis[1] as f32, back_axis[1] as f32, 0.0,
		right_axis[2] as f32, up_axis[2] as f32, back_axis[2] as f32, 0.0,
		dot(&right_axis), dot(&up_axis), dot(&back_axis), 1.0,
	]
}

fn multiply_matrix4(left_matrix: &[f32; 16], right_matrix: &[f32; 16]) -> [f32; 16] {
	let mut result = [0.0f32; 16];
	for column in 0..4 {
		for row in 0..4 {
			let mut sum = 0.0f32;
			for term in 0..4 {
				sum += left_matrix[term * 4 + row] * right_matrix[column * 4 + term];
			}
			result[column * 4 + row] = sum;
		}
	}
	result
}

fn create_ship_model_matrix(position: [f64; 3], yaw_radian: f64, pitch_radian: f64, scale: f64) -> [f32; 16] {
	let (sin_yaw, cos_yaw) = yaw_radian.sin_cos();
	let (sin_pitch, cos_pitch) = pitch_radian.sin_cos();
	[
		(cos_yaw * scale) as f32, 0.0, (-sin_yaw * scale) as f32, 0.0,
		(sin_yaw * sin_pitch * scale) as f32, (cos_pitch * scale) as f32, (cos_yaw * sin_pitch * scale) as f32, 0.0,
		(sin_yaw * cos_pitch * scale) as f32, (-sin_pitch * scale) as f32, (cos_yaw * cos_pitch * scale) as f32, 0.0,
		position[0] as f32, position[1] as f32, position[2] as f32, 1.0,
	]
}

// 모델 로컬 점을 월드로.
fn transform_point(model_matrix: &[f32; 16], local: &[f64; 3]) -> [f64; 3] {
	let component = |row: usize| -> f64 {
		model_matrix[row] as f64 * local[0] + model_matrix[4 + row] as f64 * local[1] + model_matrix[8 + row] as f64 * local[2] + model_matrix[12 + row] as f64
	};
	[component(0), component(1), component(2)]
}

// 점을 화면 좌표로 투영. (배경 셰이더의 screenPoint 좌표계)
fn project_point_to_screen(view_projection_matrix: &[f32; 16], point: &[f64; 3], aspect_ratio: f64) -> Option<[f64; 2]> {
	let component = |row: usize| -> f64 {
		view_projection_matrix[row] as f64 * point[0] + view_projection_matrix[4 + row] as f64 * point[1] + view_projection_matrix[8 + row] as f64 * point[2] + view_projection_matrix[12 + row] as f64
	};
	let clip_w = component(3);
	if clip_w <= 0.0001 {
		return None;
	}
	Some([(component(0) / clip_w) * aspect_ratio, component(1) / clip_w])
}

//==============================================================================
// 지형 높이. (셰이더의 terrainHeight 와 같은 식, 플레이어 발밑 계산용)
//==============================================================================
fn smoothstep_value(edge_start: f64, edge_end: f64, value: f64) -> f64 {
	let normalized = ((value - edge_start) / (edge_end - edge_start)).clamp(0.0, 1.0);
	normalized * normalized * (3.0 - 2.0 * normalized)
}

fn lattice_random_at(cell_x: i32, cell_z: i32) -> f64 {
	let mut hash_value = (cell_x as u32).wrapping_mul(374761393).wrapping_add((cell_z as u32).wrapping_mul(668265263));
	hash_value = (hash_value ^ (hash_value >> 13)).wrapping_mul(1274126177);
	hash_value ^= hash_value >> 16;
	(hash_value & 65535) as f64 / 65535.0
}

fn value_noise_at(point_x: f64, point_z: f64) -> f64 {
	let cell_origin_x = point_x.floor();
	let cell_origin_z = point_z.floor();
	let fraction_x = point_x - cell_origin_x;
	let fraction_z = point_z - cell_origin_z;
	let weight_x = fraction_x * fraction_x * (3.0 - 2.0 * fraction_x);
	let weight_z = fraction_z * fraction_z * (3.0 - 2.0 * fraction_z);
	let cell_x = cell_origin_x as i32;
	let cell_z = cell_origin_z as i32;
	let value_00 = lattice_random_at(cell_x, cell_z);
	let value_10 = lattice_random_at(cell_x + 1, cell_z);
	let value_01 = lattice_random_at(cell_x, cell_z + 1);
	let value_11 = lattice_random_at(cell_x + 1, cell_z + 1);
	let value_x0 = value_00 + (value_10 - value_00) * weight_x;
	let value_x1 = value_01 + (value_11 - value_01) * weight_x;
	value_x0 + (value_x1 - value_x0) * weight_z
}

fn terrain_height_at(world_x: f64, world_z: f64) -> f64 {
	let side_factor = smoothstep_value(2.2, 9.0, world_x.abs());
	let noise_large = value_noise_at(world_x * 0.06, world_z * 0.06);
	let noise_medium = value_noise_at(world_x * 0.15 + 37.0, world_z * 0.15 + 37.0);
	let noise_small = value_noise_at(world_x * 0.37 + 91.0, world_z * 0.37 + 91.0);
	let combined = noise_large * 1.4 + noise_medium * 0.65 + noise_small * 0.25;
	(combined - 0.7).max(0.0).powf(1.5) * 4.6 * side_factor
}

// 지형 표면 높이. (메시와 같게 셀 코너 높이를 쌍선형 보간, 표면 관통 방지)
fn terrain_surface_height_at(world_x: f64, world_z: f64) -> f64 {
	let cell_start_x = (world_x / GRID_CELL_SIZE).floor() * GRID_CELL_SIZE;
	let cell_start_z = (world_z / GRID_CELL_SIZE).floor() * GRID_CELL_SIZE;
	let normalized_x = (world_x - cell_start_x) / GRID_CELL_SIZE;
	let normalized_z = (world_z - cell_start_z) / GRID_CELL_SIZE;
	let corner_height_00 = terrain_height_at(cell_start_x, cell_start_z);
	let corner_height_10 = terrain_height_at(cell_start_x + GRID_CELL_SIZE, cell_start_z);
	let corner_height_01 = terrain_height_at(cell_start_x, cell_start_z + GRID_CELL_SIZE);
	let corner_height_11 = terrain_height_at(cell_start_x + GRID_CELL_SIZE, cell_start_z + GRID_CELL_SIZE);
	let near_height = corner_height_00 + (corner_height_10 - corner_height_00) * normalized_x;
	let far_height = corner_height_01 + (corner_height_11 - corner_height_01) * normalized_x;
	near_height + (far_height - near_height) * normalized_z
}

//==============================================================================
// 작은 난수. (예제 하나에서만 쓰는 xorshift, 시작 때 빛살과 파티클 자리를 뿌린다)
//==============================================================================
struct SmallRandom {
	state: u32,
}

impl SmallRandom {
	fn next_value(&mut self) -> f64 {
		self.state ^= self.state << 13;
		self.state ^= self.state >> 17;
		self.state ^= self.state << 5;
		(self.state >> 8) as f64 / 16777216.0
	}
}

fn float_bytes(value_list: &[f32]) -> Vec<u8> {
	value_list.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn rgba(red: f64, green: f64, blue: f64, alpha: f64) -> Color {
	Color::create(red / 255.0, green / 255.0, blue / 255.0, alpha)
}

//==============================================================================
// initialize 에서 만드는 GL 리소스.
//==============================================================================
struct NeonResources {
	background_pass: FullscreenPass,
	brightpass_pass: FullscreenPass,
	blur_pass: FullscreenPass,
	composite_pass: FullscreenPass,
	terrain_program: ShaderProgram,
	terrain_vertex_array: vanilla::core::graphic::GlVertexArray,
	terrain_index_count: i32,
	streak_program: ShaderProgram,
	streak_vertex_array: vanilla::core::graphic::GlVertexArray,
	particle_program: ShaderProgram,
	particle_vertex_array: vanilla::core::graphic::GlVertexArray,
	scene_render_target: RenderTarget,
	bloom_render_target_a: RenderTarget,
	bloom_render_target_b: RenderTarget,
	ship_renderer: Option<SkinnedModelRenderer>,
	flagship_model: Option<SkinnedModel>,
	escort_model: Option<SkinnedModel>,
}

//==============================================================================
// 네온 호라이즌 씬.
//==============================================================================
struct NeonHorizonScene {
	scene_base: SceneBase,
	flagship_load_handle: Option<SkinnedModelLoadHandle>,
	escort_load_handle: Option<SkinnedModelLoadHandle>,
	resources: Option<NeonResources>,
	load_error_message: Option<String>,
	elapsed_time: f64,
	// 입력.
	pointer_target_x: f64,
	pointer_target_y: f64,
	pointer_current_x: f64,
	pointer_current_y: f64,
	last_pointer_position: Vector2,
	is_boost_pressed: bool,
	drag_last_x: Option<f64>,
	player_target_x: f64,
	player_current_x: f64,
	// 상태.
	boost_factor: f64,
	scroll_distance: f64,
	streak_scroll: f64,
	run_bob_phase: f64,
	option_state_list: [bool; TOGGLE_COUNT],
	option_blend_list: [f64; TOGGLE_COUNT],
	glow_value: f64,
	speed_value: f64,
	active_slider_index: Option<usize>,
	is_panel_collapsed: bool,
	panel_collapse_blend: f64,
	delta_time: f64,
	is_first_frame_checked: bool,
}

impl NeonHorizonScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> NeonHorizonScene {
		NeonHorizonScene {
			scene_base: SceneBase::new(),
			flagship_load_handle: None,
			escort_load_handle: None,
			resources: None,
			load_error_message: None,
			elapsed_time: 0.0,
			pointer_target_x: 0.0,
			pointer_target_y: 0.0,
			pointer_current_x: 0.0,
			pointer_current_y: 0.0,
			last_pointer_position: Vector2::create(-1.0, -1.0),
			is_boost_pressed: false,
			drag_last_x: None,
			player_target_x: 0.0,
			player_current_x: 0.0,
			boost_factor: 0.0,
			scroll_distance: 0.0,
			streak_scroll: 0.0,
			run_bob_phase: 0.0,
			option_state_list: [true; TOGGLE_COUNT],
			option_blend_list: [1.0; TOGGLE_COUNT],
			glow_value: 1.0,
			speed_value: 1.0,
			active_slider_index: None,
			is_panel_collapsed: true,
			panel_collapse_blend: 0.0,
			delta_time: 0.0,
			is_first_frame_checked: false,
		}
	}

	//==============================================================================
	// GL 리소스 만들기. (프로그램, 그리드 메시, 빛살과 파티클 버퍼, 렌더 타겟, 함선 모델)
	//==============================================================================
	fn create_resources(&self, gl: &GlContext) -> Result<NeonResources, String> {
		let background_pass = FullscreenPass::new(gl, BACKGROUND_FRAGMENTSHADER_SOURCE)?;
		let brightpass_pass = FullscreenPass::new(gl, BRIGHTPASS_FRAGMENTSHADER_SOURCE)?;
		let blur_pass = FullscreenPass::new(gl, BLUR_FRAGMENTSHADER_SOURCE)?;
		let composite_pass = FullscreenPass::new(gl, COMPOSITE_FRAGMENTSHADER_SOURCE)?;
		let terrain_program = ShaderProgram::create_with_attribute_locations(gl, TERRAIN_VERTEXSHADER_SOURCE.trim(), TERRAIN_FRAGMENTSHADER_SOURCE.trim(), &[("vertexGridPosition", 0)])?;
		let streak_program = ShaderProgram::create_with_attribute_locations(gl, STREAK_VERTEXSHADER_SOURCE.trim(), STREAK_FRAGMENTSHADER_SOURCE.trim(), &[("streakData", 0), ("streakEndFlag", 1)])?;
		let particle_program = ShaderProgram::create_with_attribute_locations(gl, PARTICLE_VERTEXSHADER_SOURCE.trim(), PARTICLE_FRAGMENTSHADER_SOURCE.trim(), &[("vertexParticle", 0)])?;

		// 그리드 정점과 인덱스.
		let grid_column_count = ((GRID_HALF_WIDTH * 2) as f64 / GRID_CELL_SIZE) as usize;
		let grid_row_count = (GRID_DEPTH as f64 / GRID_CELL_SIZE) as usize;
		let mut terrain_position_list: Vec<f32> = Vec::new();
		for row_index in 0..=grid_row_count {
			for column_index in 0..=grid_column_count {
				terrain_position_list.push((-GRID_HALF_WIDTH as f64 + column_index as f64 * GRID_CELL_SIZE) as f32);
				terrain_position_list.push((row_index as f64 * GRID_CELL_SIZE) as f32);
			}
		}
		let grid_vertex_stride = (grid_column_count + 1) as u16;
		let mut terrain_index_list: Vec<u16> = Vec::new();
		for row_index in 0..grid_row_count as u16 {
			for column_index in 0..grid_column_count as u16 {
				let top_left = row_index * grid_vertex_stride + column_index;
				let top_right = top_left + 1;
				let bottom_left = top_left + grid_vertex_stride;
				let bottom_right = bottom_left + 1;
				terrain_index_list.extend_from_slice(&[top_left, bottom_left, top_right, top_right, bottom_left, bottom_right]);
			}
		}
		let terrain_index_bytes: Vec<u8> = terrain_index_list.iter().flat_map(|value| value.to_le_bytes()).collect();

		// 빛살과 파티클 자리. (빛살당 머리, 꼬리 두 정점)
		let mut random = SmallRandom { state: 0x2545f491 };
		let mut streak_value_list: Vec<f32> = Vec::with_capacity(STREAK_COUNT * 2 * STREAK_FLOATS_PER_VERTEX);
		for streak_index in 0..STREAK_COUNT {
			let side_sign = if streak_index % 2 == 0 { 1.0 } else { -1.0 };
			let streak_x = side_sign * (1.5 + random.next_value() * 16.0);
			let streak_y = 0.3 + random.next_value() * 6.2;
			let streak_z = random.next_value() * 46.0;
			let streak_seed = random.next_value();
			for end_index in 0..2 {
				streak_value_list.extend_from_slice(&[streak_x as f32, streak_y as f32, streak_z as f32, streak_seed as f32, end_index as f32]);
			}
		}
		let mut particle_value_list: Vec<f32> = Vec::with_capacity(PARTICLE_COUNT * 4);
		for _ in 0..PARTICLE_COUNT {
			particle_value_list.push(((random.next_value() - 0.5) * 34.0) as f32);
			particle_value_list.push((random.next_value() * 7.0) as f32);
			particle_value_list.push((random.next_value() * 38.0) as f32);
			particle_value_list.push(random.next_value() as f32);
		}

		let (terrain_vertex_array, streak_vertex_array, particle_vertex_array) = unsafe {
			let terrain_vertex_array = gl.create_vertex_array()?;
			gl.bind_vertex_array(Some(terrain_vertex_array));
			let terrain_vertex_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(terrain_vertex_buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &float_bytes(&terrain_position_list), glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(0);
			gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
			let terrain_index_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(terrain_index_buffer));
			gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, &terrain_index_bytes, glow::STATIC_DRAW);

			let streak_vertex_array = gl.create_vertex_array()?;
			gl.bind_vertex_array(Some(streak_vertex_array));
			let streak_vertex_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(streak_vertex_buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &float_bytes(&streak_value_list), glow::STATIC_DRAW);
			let streak_stride_bytes = (STREAK_FLOATS_PER_VERTEX * 4) as i32;
			gl.enable_vertex_attrib_array(0);
			gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, streak_stride_bytes, 0);
			gl.enable_vertex_attrib_array(1);
			gl.vertex_attrib_pointer_f32(1, 1, glow::FLOAT, false, streak_stride_bytes, 16);

			let particle_vertex_array = gl.create_vertex_array()?;
			gl.bind_vertex_array(Some(particle_vertex_array));
			let particle_vertex_buffer = gl.create_buffer()?;
			gl.bind_buffer(glow::ARRAY_BUFFER, Some(particle_vertex_buffer));
			gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &float_bytes(&particle_value_list), glow::STATIC_DRAW);
			gl.enable_vertex_attrib_array(0);
			gl.vertex_attrib_pointer_f32(0, 4, glow::FLOAT, false, 0, 0);
			gl.bind_vertex_array(None);
			(terrain_vertex_array, streak_vertex_array, particle_vertex_array)
		};

		// 렌더 타겟. (크기는 그릴 때 맞춘다)
		let scene_render_target = RenderTarget::new(gl, 2, 2, true, None)?;
		let bloom_render_target_a = RenderTarget::new(gl, 2, 2, false, None)?;
		let bloom_render_target_b = RenderTarget::new(gl, 2, 2, false, None)?;

		// 함선. (전투순양함 하나, 호위함 하나를 네 척에 돌려 쓴다)
		let mut ship_renderer = None;
		let mut flagship_model = None;
		let mut escort_model = None;
		if let Ok(mut renderer) = SkinnedModelRenderer::new(gl, None, None) {
			renderer.set_sun_direction(0.32, 0.38, 0.87);
			renderer.set_fog_color(0.24, 0.14, 0.06);
			renderer.set_fog_range(60.0, 190.0);
			if let Some(Ok(model)) = self.flagship_load_handle.as_ref().and_then(|load_handle| load_handle.create_model(gl, &renderer)) {
				flagship_model = Some(model);
			}
			if let Some(Ok(model)) = self.escort_load_handle.as_ref().and_then(|load_handle| load_handle.create_model(gl, &renderer)) {
				escort_model = Some(model);
			}
			ship_renderer = Some(renderer);
		}

		Ok(NeonResources {
			background_pass,
			brightpass_pass,
			blur_pass,
			composite_pass,
			terrain_program,
			terrain_vertex_array,
			terrain_index_count: terrain_index_list.len() as i32,
			streak_program,
			streak_vertex_array,
			particle_program,
			particle_vertex_array,
			scene_render_target,
			bloom_render_target_a,
			bloom_render_target_b,
			ship_renderer,
			flagship_model,
			escort_model,
		})
	}

	//==============================================================================
	// 패널 배치. (panel_x, panel_y, panel_height)
	//==============================================================================
	fn calculate_panel_layout(&self, window_width: f64) -> (f64, f64, f64) {
		let panel_x = window_width - PANEL_MARGIN_RIGHT - PANEL_WIDTH;
		let list_height = (OPTION_DEFINITION_LIST.len() as f64 * PANEL_ROW_HEIGHT + 10.0) * self.panel_collapse_blend;
		(panel_x, 18.0, PANEL_HEADER_HEIGHT + list_height)
	}

	fn slider_track_x(panel_x: f64) -> f64 {
		panel_x + PANEL_WIDTH - 16.0 - SLIDER_TRACK_WIDTH
	}

	fn get_slider_value(&self, option_index: usize) -> f64 {
		if OPTION_DEFINITION_LIST[option_index].name == "glow" {
			self.glow_value
		}
		else {
			self.speed_value
		}
	}

	fn update_slider_from_x(&mut self, client_x: f64, window_width: f64) {
		if let Some(option_index) = self.active_slider_index {
			let (panel_x, _, _) = self.calculate_panel_layout(window_width);
			let normalized = ((client_x - NeonHorizonScene::slider_track_x(panel_x)) / SLIDER_TRACK_WIDTH).clamp(0.0, 1.0);
			let option_definition = &OPTION_DEFINITION_LIST[option_index];
			let slider_value = option_definition.minimum_value + (option_definition.maximum_value - option_definition.minimum_value) * normalized;
			if option_definition.name == "glow" {
				self.glow_value = slider_value;
			}
			else {
				self.speed_value = slider_value;
			}
		}
	}

	//==============================================================================
	// 패널 누르기. (true 면 부스트, 드래그 입력으로 넘기지 않는다)
	//==============================================================================
	fn handle_panel_press(&mut self, client_x: f64, client_y: f64, window_width: f64) -> bool {
		let (panel_x, panel_y, panel_height) = self.calculate_panel_layout(window_width);
		if client_x < panel_x || client_x > panel_x + PANEL_WIDTH || client_y < panel_y || client_y > panel_y + panel_height {
			return false;
		}
		if client_y < panel_y + PANEL_HEADER_HEIGHT {
			self.is_panel_collapsed = !self.is_panel_collapsed;
			return true;
		}
		if self.panel_collapse_blend > 0.9 {
			let row_index = ((client_y - panel_y - PANEL_HEADER_HEIGHT - 4.0) / PANEL_ROW_HEIGHT).floor();
			if row_index >= 0.0 && (row_index as usize) < OPTION_DEFINITION_LIST.len() {
				let row_index = row_index as usize;
				if OPTION_DEFINITION_LIST[row_index].is_slider {
					self.active_slider_index = Some(row_index);
					self.update_slider_from_x(client_x, window_width);
				}
				else {
					self.option_state_list[row_index] = !self.option_state_list[row_index];
				}
			}
		}
		true
	}

	//==============================================================================
	// 3D 그리기. (배경 → 지형 → 함대 → 빛살 → 파티클 → 블룸 → 합성)
	//==============================================================================
	fn render_scene(&mut self, gl: &GlContext, width: i32, height: i32) -> Result<(), String> {
		let elapsed_time = self.elapsed_time;
		let delta_time = self.delta_time;
		let boost_factor = self.boost_factor;
		let aspect_ratio = width as f64 / height.max(1) as f64;

		// 카메라. (지면 + 눈높이 + 보행 바운스 + 포인터 시차 + 부스트 시 화각 확장)
		let player_world_z = self.scroll_distance - 0.5;
		let surface_height = terrain_surface_height_at(self.player_current_x, player_world_z);
		let run_bob_y = (self.run_bob_phase * 2.0).sin() * 0.05 * (1.0 + boost_factor * 0.6);
		let run_sway_x = self.run_bob_phase.sin() * 0.05;
		let camera_position = [
			self.player_current_x + run_sway_x + self.pointer_current_x * 0.4,
			surface_height + 1.9 + run_bob_y + self.pointer_current_y * -0.3,
			0.5,
		];
		let camera_pitch = -0.06 + self.pointer_current_y * 0.08;
		let camera_yaw = self.pointer_current_x * 0.12;
		let field_of_view_radian = (std::f64::consts::PI / 3.0) * (1.0 + boost_factor * 0.1);
		let projection_matrix = create_perspective_matrix(field_of_view_radian, aspect_ratio, 0.1, 200.0);
		let view_matrix = create_view_matrix(camera_position, camera_pitch, camera_yaw);
		let view_projection_matrix = multiply_matrix4(&projection_matrix, &view_matrix);

		let option_blend_list = self.option_blend_list;
		let glow_value = self.glow_value;
		let player_current_x = self.player_current_x;
		let scroll_distance = self.scroll_distance;
		let streak_scroll = self.streak_scroll;

		let resources = match &mut self.resources {
			Some(resources) => {
				resources
			}
			None => {
				return Ok(());
			}
		};
		resources.scene_render_target.resize(gl, width, height)?;
		let bloom_width = (width >> 1).max(1);
		let bloom_height = (height >> 1).max(1);
		resources.bloom_render_target_a.resize(gl, bloom_width, bloom_height)?;
		resources.bloom_render_target_b.resize(gl, bloom_width, bloom_height)?;

		// 함대 변환과 화면 앵커. (배경 셰이더의 엔진 글로우와 주포가 이 좌표를 쓴다)
		let ship_strength = option_blend_list[TOGGLE_SHIPS];
		let mut ship_model_matrix_list: Vec<[f32; 16]> = Vec::new();
		let mut ship_anchor_values = [0.0f32; 16];
		let mut beam_anchor_values = [0.0f32, 0.0, 1.0, 0.0];
		if resources.flagship_model.is_some() && ship_strength > 0.001 {
			for (ship_index, ship_definition) in SHIP_DEFINITION_LIST.iter().enumerate() {
				let drift_time = elapsed_time + ship_definition.drift_seed;
				let ship_position = [
					ship_definition.base_position[0] + (drift_time * 0.05).sin() * 1.4,
					ship_definition.base_position[1] + (drift_time * 0.08).sin() * 0.8,
					ship_definition.base_position[2],
				];
				let ship_scale = ship_definition.scale * (0.001 + 0.999 * ship_strength);
				let ship_model_matrix = create_ship_model_matrix(ship_position, ship_definition.yaw_radian, ship_definition.pitch_radian, ship_scale);
				ship_model_matrix_list.push(ship_model_matrix);
				let stern_world = transform_point(&ship_model_matrix, &SHIP_STERN_LOCAL);
				let stern_screen = project_point_to_screen(&view_projection_matrix, &stern_world, aspect_ratio);
				if let Some(stern_screen) = stern_screen {
					ship_anchor_values[ship_index * 4] = stern_screen[0] as f32;
					ship_anchor_values[ship_index * 4 + 1] = stern_screen[1] as f32;
					ship_anchor_values[ship_index * 4 + 2] = (ship_scale * 2.4 / -ship_position[2]) as f32;
					ship_anchor_values[ship_index * 4 + 3] = (ship_definition.fade * ship_strength) as f32;
				}
				if ship_index == 0 {
					let prow_world = transform_point(&ship_model_matrix, &SHIP_PROW_LOCAL);
					let prow_screen = project_point_to_screen(&view_projection_matrix, &prow_world, aspect_ratio);
					if let (Some(prow_screen), Some(stern_screen)) = (prow_screen, stern_screen) {
						beam_anchor_values = [prow_screen[0] as f32, prow_screen[1] as f32, (prow_screen[0] - stern_screen[0]) as f32, (prow_screen[1] - stern_screen[1]) as f32];
					}
				}
			}
		}

		unsafe {
			// 프로그램 포인트 크기 켜기. (데스크탑 GL 에서 gl_PointSize 를 쓴다)
			gl.enable(0x8642);

			// 1. 씬 → 오프스크린 렌더 타겟.
			resources.scene_render_target.bind(gl);
			gl.clear_color(0.04, 0.025, 0.012, 1.0);
			gl.depth_mask(true);
			gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

			// 1-1. 배경.
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
			gl.disable(glow::CULL_FACE);
			resources.background_pass.use_program(gl);
			gl.uniform_1_f32(resources.background_pass.get_uniform_location(gl, "elapsedTime").as_ref(), elapsed_time as f32);
			gl.uniform_1_f32(resources.background_pass.get_uniform_location(gl, "aspectRatio").as_ref(), aspect_ratio as f32);
			gl.uniform_1_f32(resources.background_pass.get_uniform_location(gl, "shipStrength").as_ref(), ship_strength as f32);
			let ship_anchor_location = resources.background_pass.get_uniform_location(gl, "shipAnchorList[0]").or_else(|| resources.background_pass.get_uniform_location(gl, "shipAnchorList"));
			gl.uniform_4_f32_slice(ship_anchor_location.as_ref(), &ship_anchor_values);
			gl.uniform_4_f32_slice(resources.background_pass.get_uniform_location(gl, "beamAnchor").as_ref(), &beam_anchor_values);
			resources.background_pass.draw(gl);

			// 1-2. 지형.
			gl.enable(glow::DEPTH_TEST);
			resources.terrain_program.use_program(gl);
			gl.bind_vertex_array(Some(resources.terrain_vertex_array));
			gl.uniform_matrix_4_f32_slice(resources.terrain_program.get_uniform_location(gl, "projectionMatrix").as_ref(), false, &projection_matrix);
			gl.uniform_matrix_4_f32_slice(resources.terrain_program.get_uniform_location(gl, "viewMatrix").as_ref(), false, &view_matrix);
			gl.uniform_1_f32(resources.terrain_program.get_uniform_location(gl, "scrollOffset").as_ref(), scroll_distance as f32);
			gl.uniform_1_f32(resources.terrain_program.get_uniform_location(gl, "elapsedTime").as_ref(), elapsed_time as f32);
			gl.uniform_3_f32(resources.terrain_program.get_uniform_location(gl, "cameraPosition").as_ref(), camera_position[0] as f32, camera_position[1] as f32, camera_position[2] as f32);
			gl.draw_elements(glow::TRIANGLES, resources.terrain_index_count, glow::UNSIGNED_SHORT, 0);
			gl.bind_vertex_array(None);

			// 1-2b. 함대. (엔진 SkinnedModelRenderer)
			if !ship_model_matrix_list.is_empty() {
				if let (Some(ship_renderer), Some(flagship_model)) = (&resources.ship_renderer, &mut resources.flagship_model) {
					flagship_model.update(delta_time);
					if let Some(escort_model) = &mut resources.escort_model {
						escort_model.update(delta_time);
					}
					// 이전 프레임 후처리가 유닛1에 걸어 둔 씬 텍스처를 풀어 준다. (씬 타깃에 그리는 중이라 피드백 금지)
					gl.active_texture(glow::TEXTURE1);
					gl.bind_texture(glow::TEXTURE_2D, None);
					gl.active_texture(glow::TEXTURE0);
					for (ship_index, ship_model_matrix) in ship_model_matrix_list.iter().enumerate() {
						let use_flagship = SHIP_DEFINITION_LIST[ship_index].is_flagship || resources.escort_model.is_none();
						if use_flagship {
							ship_renderer.draw(gl, flagship_model, &view_projection_matrix, ship_model_matrix, camera_position[0], camera_position[1], camera_position[2]);
						}
						else if let Some(escort_model) = &mut resources.escort_model {
							ship_renderer.draw(gl, escort_model, &view_projection_matrix, ship_model_matrix, camera_position[0], camera_position[1], camera_position[2]);
						}
					}
				}
			}

			// 1-3. 부스트 빛살. (가산 합성, 깊이 기록 없이 테스트만, 옵션 페이드)
			gl.enable(glow::BLEND);
			gl.blend_func(glow::ONE, glow::ONE);
			gl.depth_mask(false);
			if option_blend_list[TOGGLE_STREAKS] > 0.01 {
				resources.streak_program.use_program(gl);
				gl.bind_vertex_array(Some(resources.streak_vertex_array));
				gl.uniform_matrix_4_f32_slice(resources.streak_program.get_uniform_location(gl, "projectionMatrix").as_ref(), false, &projection_matrix);
				gl.uniform_matrix_4_f32_slice(resources.streak_program.get_uniform_location(gl, "viewMatrix").as_ref(), false, &view_matrix);
				gl.uniform_1_f32(resources.streak_program.get_uniform_location(gl, "streakScroll").as_ref(), streak_scroll as f32);
				gl.uniform_1_f32(resources.streak_program.get_uniform_location(gl, "boostFactor").as_ref(), boost_factor as f32);
				gl.uniform_1_f32(resources.streak_program.get_uniform_location(gl, "heightOffset").as_ref(), (camera_position[1] - 2.1) as f32);
				gl.uniform_1_f32(resources.streak_program.get_uniform_location(gl, "lateralOffset").as_ref(), player_current_x as f32);
				gl.uniform_1_f32(resources.streak_program.get_uniform_location(gl, "fadeFactor").as_ref(), option_blend_list[TOGGLE_STREAKS] as f32);
				gl.draw_arrays(glow::LINES, 0, (STREAK_COUNT * 2) as i32);
			}

			// 1-5. 파티클. (가산 합성, 깊이 테스트로 지형에 가려짐, 옵션 페이드)
			if option_blend_list[TOGGLE_PARTICLES] > 0.01 {
				resources.particle_program.use_program(gl);
				gl.bind_vertex_array(Some(resources.particle_vertex_array));
				gl.uniform_matrix_4_f32_slice(resources.particle_program.get_uniform_location(gl, "projectionMatrix").as_ref(), false, &projection_matrix);
				gl.uniform_matrix_4_f32_slice(resources.particle_program.get_uniform_location(gl, "viewMatrix").as_ref(), false, &view_matrix);
				gl.uniform_1_f32(resources.particle_program.get_uniform_location(gl, "elapsedTime").as_ref(), elapsed_time as f32);
				gl.uniform_1_f32(resources.particle_program.get_uniform_location(gl, "fadeFactor").as_ref(), option_blend_list[TOGGLE_PARTICLES] as f32);
				gl.uniform_1_f32(resources.particle_program.get_uniform_location(gl, "heightOffset").as_ref(), (camera_position[1] - 2.1) as f32);
				gl.uniform_1_f32(resources.particle_program.get_uniform_location(gl, "lateralOffset").as_ref(), player_current_x as f32);
				gl.draw_arrays(glow::POINTS, 0, PARTICLE_COUNT as i32);
			}
			gl.bind_vertex_array(None);
			gl.depth_mask(true);
			gl.disable(glow::DEPTH_TEST);
			gl.disable(glow::BLEND);
			gl.active_texture(glow::TEXTURE0);

			let scene_texture: Option<GlTexture> = resources.scene_render_target.get_color_texture(0);

			// 2. 블룸: 밝은 영역 추출. (하프 해상도)
			resources.bloom_render_target_a.bind(gl);
			resources.brightpass_pass.use_program(gl);
			gl.uniform_1_i32(resources.brightpass_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
			gl.bind_texture(glow::TEXTURE_2D, scene_texture);
			resources.brightpass_pass.draw(gl);

			// 3. 블룸: 가로, 세로 가우시안 블러 핑퐁.
			resources.blur_pass.use_program(gl);
			gl.uniform_1_i32(resources.blur_pass.get_uniform_location(gl, "sourceTexture").as_ref(), 0);
			let blur_direction_location = resources.blur_pass.get_uniform_location(gl, "blurDirection");
			for _ in 0..2 {
				resources.bloom_render_target_b.bind(gl);
				gl.uniform_2_f32(blur_direction_location.as_ref(), 1.0 / bloom_width as f32, 0.0);
				gl.bind_texture(glow::TEXTURE_2D, resources.bloom_render_target_a.get_color_texture(0));
				resources.blur_pass.draw(gl);
				resources.bloom_render_target_a.bind(gl);
				gl.uniform_2_f32(blur_direction_location.as_ref(), 0.0, 1.0 / bloom_height as f32);
				gl.bind_texture(glow::TEXTURE_2D, resources.bloom_render_target_b.get_color_texture(0));
				resources.blur_pass.draw(gl);
			}

			// 4. 최종 합성 → 화면.
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);
			gl.viewport(0, 0, width, height);
			resources.composite_pass.use_program(gl);
			gl.uniform_1_i32(resources.composite_pass.get_uniform_location(gl, "sceneTexture").as_ref(), 0);
			gl.uniform_1_i32(resources.composite_pass.get_uniform_location(gl, "bloomTexture").as_ref(), 1);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "elapsedTime").as_ref(), elapsed_time as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "boostFactor").as_ref(), boost_factor as f32);
			gl.uniform_2_f32(resources.composite_pass.get_uniform_location(gl, "resolution").as_ref(), width as f32, height as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "bloomStrength").as_ref(), (option_blend_list[TOGGLE_BLOOM] * glow_value) as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "aberrationStrength").as_ref(), option_blend_list[TOGGLE_ABERRATION] as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "filmStrength").as_ref(), option_blend_list[TOGGLE_FILM] as f32);
			gl.uniform_1_f32(resources.composite_pass.get_uniform_location(gl, "letterboxStrength").as_ref(), option_blend_list[TOGGLE_LETTERBOX] as f32);
			gl.active_texture(glow::TEXTURE0);
			gl.bind_texture(glow::TEXTURE_2D, scene_texture);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, resources.bloom_render_target_a.get_color_texture(0));
			resources.composite_pass.draw(gl);
			gl.active_texture(glow::TEXTURE1);
			gl.bind_texture(glow::TEXTURE_2D, None);
			gl.active_texture(glow::TEXTURE0);

			if !self.is_first_frame_checked {
				self.is_first_frame_checked = true;
				println!("neonhorizon: 첫 3D 프레임 GL 오류 코드 0x{:X}", gl.get_error());
			}
		}
		Ok(())
	}

	//==============================================================================
	// 글자를 자간을 벌려 가운데 맞춤으로 그린다. (원본 CSS 의 letter-spacing + text-indent 와 같은 모양)
	//==============================================================================
	fn draw_spaced_text(graphic: &mut Graphic, text: &str, center_x: f64, middle_y: f64, spacing: f64) {
		let width_list: Vec<f64> = text.chars().map(|character| graphic.measure_text(&character.to_string()).width).collect();
		let total_width: f64 = width_list.iter().sum::<f64>() + spacing * (width_list.len().saturating_sub(1)) as f64;
		let mut character_x = center_x - total_width * 0.5;
		graphic.set_text_align(TextAlign::Left);
		for (character, character_width) in text.chars().zip(width_list.iter()) {
			graphic.draw_fill_text(&character.to_string(), character_x, middle_y);
			character_x += character_width + spacing;
		}
	}
}

impl Scene for NeonHorizonScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (함선 모델 받기 부탁)
	//==============================================================================
	fn load(&mut self, engine: &Engine) {
		let flagship_load_handle = SkinnedModel::load_from_url(engine, "battlecruiser.glb");
		let escort_load_handle = SkinnedModel::load_from_url(engine, "escort.glb");
		self.scene_base.track_load_handle(flagship_load_handle.get_load_handle());
		self.scene_base.track_load_handle(escort_load_handle.get_load_handle());
		self.flagship_load_handle = Some(flagship_load_handle);
		self.escort_load_handle = Some(escort_load_handle);
	}

	//==============================================================================
	// 초기화. (모델을 다 받은 뒤 GL 리소스를 만든다)
	//==============================================================================
	fn initialize(&mut self, engine: &Engine) {
		let graphic = match engine.get_graphic() {
			Some(graphic) => {
				graphic
			}
			None => {
				return;
			}
		};
		match self.create_resources(graphic.get_gl()) {
			Ok(resources) => {
				self.resources = Some(resources);
			}
			Err(error) => {
				eprintln!("neonhorizon: {}", error);
				self.load_error_message = Some(error);
			}
		}
	}

	//==============================================================================
	// 갱신. (입력, 부스트, 옵션 보간, 전진 거리)
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		let engine = Engine::get_engine().unwrap();
		let is_capture = std::env::var("VANILLA_CAPTURE_PATH").is_ok();
		let delta_time = if is_capture {
			FIXED_TIME_STEP
		}
		else {
			time_delta.min(0.1)
		};
		self.delta_time = delta_time;
		self.elapsed_time += delta_time;
		if self.elapsed_time >= EXIT_SECONDS {
			engine.exit_application();
		}

		// 입력. (좌표는 CSS 픽셀)
		let window_size = engine.get_view_manager().get_canvas_native_size();
		let (pointer_position, is_pressed, is_down, is_released) = {
			let input_manager = engine.get_input_manager();
			(input_manager.get_canvas_native_input_position(), input_manager.is_touch_pressed(), input_manager.is_touch_down(), input_manager.is_touch_released())
		};
		if window_size.x > 0.0 && window_size.y > 0.0 && (pointer_position.x != self.last_pointer_position.x || pointer_position.y != self.last_pointer_position.y) {
			self.last_pointer_position = pointer_position;
			self.pointer_target_x = (pointer_position.x / window_size.x) * 2.0 - 1.0;
			self.pointer_target_y = (pointer_position.y / window_size.y) * 2.0 - 1.0;
		}
		if is_pressed {
			if !self.handle_panel_press(pointer_position.x, pointer_position.y, window_size.x) {
				self.is_boost_pressed = true;
				self.drag_last_x = Some(pointer_position.x);
			}
		}
		else if is_down {
			if self.active_slider_index.is_some() {
				self.update_slider_from_x(pointer_position.x, window_size.x);
			}
			else if self.is_boost_pressed {
				if let Some(drag_last_x) = self.drag_last_x {
					self.player_target_x = (self.player_target_x + (pointer_position.x - drag_last_x) * (26.0 / window_size.x)).clamp(-PLAYER_MOVE_LIMIT, PLAYER_MOVE_LIMIT);
				}
				self.drag_last_x = Some(pointer_position.x);
			}
		}
		if is_released || (!is_down && !is_pressed) {
			self.is_boost_pressed = false;
			self.drag_last_x = None;
			self.active_slider_index = None;
		}

		// 부스트 가감속. (누르면 서서히 가속, 떼면 서서히 감속)
		let boost_target = if self.is_boost_pressed { 1.0 } else { 0.0 };
		let boost_blend = if self.is_boost_pressed { 2.2 } else { 1.2 };
		self.boost_factor += (boost_target - self.boost_factor) * (boost_blend * delta_time).min(1.0);

		// 옵션 토글, 패널 접힘 보간.
		for toggle_index in 0..TOGGLE_COUNT {
			let option_target = if self.option_state_list[toggle_index] { 1.0 } else { 0.0 };
			self.option_blend_list[toggle_index] += (option_target - self.option_blend_list[toggle_index]) * (7.0 * delta_time).min(1.0);
		}
		let collapse_target = if self.is_panel_collapsed { 0.0 } else { 1.0 };
		self.panel_collapse_blend += (collapse_target - self.panel_collapse_blend) * (8.0 * delta_time).min(1.0);

		// 전진 거리 적분. (SPEED 슬라이더 반영)
		self.scroll_distance += (4.2 + self.boost_factor * 16.0) * self.speed_value * delta_time;
		self.streak_scroll += (10.0 + self.boost_factor * 34.0) * self.speed_value * delta_time;

		// 포인터 지연 추적, 플레이어 좌우 이동, 보행 바운스 위상. (원본은 프레임당 0.04 고정)
		self.pointer_current_x += (self.pointer_target_x - self.pointer_current_x) * 0.04;
		self.pointer_current_y += (self.pointer_target_y - self.pointer_current_y) * 0.04;
		self.player_current_x += (self.player_target_x - self.player_current_x) * (5.0 * delta_time).min(1.0);
		self.run_bob_phase += (4.2 + self.boost_factor * 16.0) * self.speed_value * delta_time * 0.55;
	}

	//==============================================================================
	// 3D 출력. (2D 묶음 내보내기 → 3D 패스 → 2D 상태 복구)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		graphic.flush_batch();
		let canvas_pixel_size = engine.get_view_manager().get_canvas_pixel_size();
		if let Err(error) = self.render_scene(graphic.get_gl(), canvas_pixel_size.x as i32, canvas_pixel_size.y as i32) {
			eprintln!("neonhorizon: {}", error);
		}
		graphic.restore_render_state();
	}

	//==============================================================================
	// 2D 출력. (제목, 아래쪽 사이트 링크, 옵션 패널)
	//==============================================================================
	fn draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		engine.get_view_manager().apply_canvas_native_rect(graphic);
		let window_size = engine.get_view_manager().get_canvas_native_size();
		if self.resources.is_none() {
			graphic.set_fill_color(Color::white());
			graphic.set_font_string("16px default");
			graphic.set_text_align(TextAlign::Left);
			graphic.set_text_baseline(TextBaseline::Middle);
			graphic.draw_fill_text(&self.load_error_message.clone().unwrap_or_else(|| String::from("loading...")), 24.0, 32.0);
			return;
		}
		let elapsed_time = self.elapsed_time;
		graphic.set_text_baseline(TextBaseline::Middle);

		// 제목. (원본 titleEntrance 3.2초, 위에서 14px 내려오며 나타남)
		let entrance_progress = (elapsed_time / 3.2).clamp(0.0, 1.0);
		let entrance_ease = 1.0 - (1.0 - entrance_progress).powi(5);
		let title_alpha = entrance_ease;
		let title_offset = -14.0 * (1.0 - entrance_ease);
		let title_font_size = (window_size.x * 0.056).clamp(28.0, 68.0);
		let sub_font_size = (window_size.x * 0.012).clamp(9.0, 12.0);
		let title_top = window_size.y * 0.13 + title_offset;
		let center_x = window_size.x * 0.5;
		graphic.set_global_alpha(title_alpha);
		graphic.set_font_string(&format!("{}px default", title_font_size.round()));
		graphic.set_fill_color(rgba(241.0, 233.0, 216.0, 1.0));
		let full_title = "NEON HORIZON";
		let full_width: f64 = full_title.chars().map(|character| graphic.measure_text(&character.to_string()).width + title_font_size * 0.5).sum();
		let line_pitch = title_font_size * 1.18;
		let line_count;
		if full_width > window_size.x * 0.5 {
			line_count = 2.0;
			NeonHorizonScene::draw_spaced_text(graphic, "NEON", center_x, title_top + title_font_size * 0.53, title_font_size * 0.5);
			NeonHorizonScene::draw_spaced_text(graphic, "HORIZON", center_x, title_top + title_font_size * 0.53 + line_pitch, title_font_size * 0.5);
		}
		else {
			line_count = 1.0;
			NeonHorizonScene::draw_spaced_text(graphic, full_title, center_x, title_top + title_font_size * 0.53, title_font_size * 0.5);
		}
		let divider_width = 240.0f64.min(window_size.x * 0.4);
		let divider_y = title_top + line_count * line_pitch + 16.0;
		for step_index in 0..40 {
			let step_ratio = step_index as f64 / 39.0;
			let step_alpha = 0.7 * (1.0 - (step_ratio * 2.0 - 1.0).abs());
			graphic.set_fill_color(rgba(243.0, 229.0, 171.0, step_alpha));
			graphic.draw_rect(Rect::create(center_x - divider_width * 0.5 + divider_width * step_index as f64 / 40.0, divider_y, divider_width / 40.0 + 0.5, 1.0));
		}
		graphic.set_font_string(&format!("{}px default", sub_font_size.round()));
		graphic.set_fill_color(rgba(209.0, 193.0, 178.0, 0.75));
		NeonHorizonScene::draw_spaced_text(graphic, "VANILLA ENGINE", center_x, divider_y + 14.0 + sub_font_size * 0.67, sub_font_size * 0.85);

		// 아래쪽 사이트 링크. (알약 모양 테두리)
		let link_width = 245.0;
		let link_height = 36.0;
		let link_rect = Rect::create(center_x - link_width * 0.5, window_size.y * 0.935 - link_height, link_width, link_height);
		graphic.set_fill_color(rgba(13.0, 7.0, 4.0, 0.45));
		graphic.draw_round_rect(link_rect, link_height * 0.5);
		graphic.set_stroke_color(rgba(243.0, 229.0, 171.0, 0.3));
		graphic.draw_stroke_round_rect(link_rect, link_height * 0.5, 1.0);
		graphic.set_fill_color(rgba(241.0, 233.0, 216.0, 0.75));
		NeonHorizonScene::draw_spaced_text(graphic, "VANILLAPUNCH.COM", center_x, link_rect.position.y + link_height * 0.5 + 1.0, sub_font_size * 0.45);

		// 옵션 패널.
		let intro_fade = ((elapsed_time - 0.2) / 2.4).clamp(0.0, 1.0);
		let (panel_x, panel_y, panel_height) = self.calculate_panel_layout(window_size.x);
		graphic.set_global_alpha(intro_fade);
		let panel_rect = Rect::create(panel_x, panel_y, PANEL_WIDTH, panel_height);
		graphic.set_fill_color(rgba(13.0, 7.0, 4.0, 0.6));
		graphic.draw_round_rect(panel_rect, 14.0);
		graphic.set_stroke_color(rgba(243.0, 229.0, 171.0, 0.25));
		graphic.draw_stroke_round_rect(panel_rect, 14.0, 1.0);
		graphic.set_font_string("11px default");
		graphic.set_text_align(TextAlign::Left);
		graphic.set_fill_color(rgba(243.0, 229.0, 171.0, 0.9));
		graphic.draw_fill_text("O P T I O N S", panel_x + 16.0, panel_y + PANEL_HEADER_HEIGHT * 0.5 + 1.0);
		graphic.set_text_align(TextAlign::Right);
		graphic.set_fill_color(rgba(243.0, 229.0, 171.0, 0.6));
		graphic.draw_fill_text(if self.is_panel_collapsed { ">" } else { "v" }, panel_x + PANEL_WIDTH - 16.0, panel_y + PANEL_HEADER_HEIGHT * 0.5 + 1.0);
		if self.panel_collapse_blend > 0.05 {
			let list_alpha = intro_fade * self.panel_collapse_blend;
			for (row_index, option_definition) in OPTION_DEFINITION_LIST.iter().enumerate() {
				let row_y = panel_y + PANEL_HEADER_HEIGHT + 4.0 + row_index as f64 * PANEL_ROW_HEIGHT;
				if row_y + PANEL_ROW_HEIGHT > panel_y + panel_height {
					break;
				}
				let row_middle_y = row_y + PANEL_ROW_HEIGHT * 0.5;
				graphic.set_global_alpha(list_alpha);
				graphic.set_text_align(TextAlign::Left);
				graphic.set_font_string("11px default");
				graphic.set_fill_color(rgba(241.0, 233.0, 216.0, 0.8));
				graphic.draw_fill_text(option_definition.label, panel_x + 16.0, row_middle_y + 1.0);
				if option_definition.is_slider {
					let slider_normalized = (self.get_slider_value(row_index) - option_definition.minimum_value) / (option_definition.maximum_value - option_definition.minimum_value);
					let track_x = NeonHorizonScene::slider_track_x(panel_x);
					graphic.set_fill_color(rgba(241.0, 233.0, 216.0, 0.18));
					graphic.draw_round_rect(Rect::create(track_x, row_middle_y - 2.0, SLIDER_TRACK_WIDTH, 4.0), 2.0);
					graphic.set_fill_color(rgba(232.0, 162.0, 60.0, 0.9));
					graphic.draw_round_rect(Rect::create(track_x, row_middle_y - 2.0, SLIDER_TRACK_WIDTH * slider_normalized, 4.0), 2.0);
					graphic.set_fill_color(rgba(255.0, 250.0, 240.0, 1.0));
					graphic.draw_circle(Vector2::create(track_x + SLIDER_TRACK_WIDTH * slider_normalized, row_middle_y), 6.0);
				}
				else {
					let toggle_blend = self.option_blend_list[row_index];
					let switch_x = panel_x + PANEL_WIDTH - 16.0 - 36.0;
					let track_rect = Rect::create(switch_x, row_middle_y - 9.5, 36.0, 19.0);
					let track_red = 0.16 + (0.85 - 0.16) * toggle_blend;
					let track_green = 0.14 + (0.72 - 0.14) * toggle_blend;
					let track_blue = 0.1 + (0.35 - 0.1) * toggle_blend;
					graphic.set_fill_color(Color::create(track_red, track_green, track_blue, 0.85));
					graphic.draw_round_rect(track_rect, 9.5);
					graphic.set_stroke_color(rgba(243.0, 229.0, 171.0, 0.25 + toggle_blend * 0.45));
					graphic.draw_stroke_round_rect(track_rect, 9.5, 1.0);
					if toggle_blend > 0.5 {
						graphic.set_fill_color(rgba(255.0, 250.0, 240.0, 1.0));
					}
					else {
						graphic.set_fill_color(rgba(241.0, 233.0, 216.0, 0.75));
					}
					graphic.draw_circle(Vector2::create(switch_x + 9.5 + toggle_blend * 17.0, row_middle_y), 6.5);
				}
			}
		}
		graphic.set_global_alpha(1.0);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("바닐라엔진 neonhorizon");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.window_size = vanilla::Vector2::create(1280.0, 720.0);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(NeonHorizonScene::new())) {
		eprintln!("{}", error);
	}
}

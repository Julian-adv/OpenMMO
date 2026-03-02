import * as THREE from 'three'

const vertexShader = /* glsl */ `
uniform float uTime;
uniform vec4 waveA;
uniform vec4 waveB;
uniform vec4 waveC;

varying vec2 vUv;
varying vec3 vWorldPos;
varying vec3 vOrigWorldPos;
varying vec4 vClipPos;

#define PI 3.14159265359

vec3 GerstnerWave(vec4 wave, vec3 p) {
  float steepness = wave.z;
  float wavelength = wave.w;
  float k = 2.0 * PI / wavelength;
  float c = sqrt(9.8 / k);
  vec2 d = normalize(wave.xy);
  float f = k * (dot(d, p.xz) - c * uTime * 0.1);
  float a = steepness / k;
  return vec3(d.x * a * cos(f), a * sin(f), d.y * a * cos(f));
}

void main() {
  vUv = uv;

  vec4 worldPos = modelMatrix * vec4(position, 1.0);
  vOrigWorldPos = worldPos.xyz;

  vec3 p = worldPos.xyz;
  vec3 offset = GerstnerWave(waveA, p);
  offset += GerstnerWave(waveB, p);
  offset += GerstnerWave(waveC, p);
  worldPos.xyz += offset;

  vWorldPos = worldPos.xyz;

  vClipPos = projectionMatrix * viewMatrix * worldPos;
  gl_Position = vClipPos;
}
`

const fragmentShader = /* glsl */ `
uniform float uTime;
uniform sampler2D uHeightmap;
uniform vec3 uShallowColor;
uniform vec3 uDeepColor;
uniform float uMaxDepth;
uniform vec3 uSunDirection;
uniform vec3 uSunColor;
uniform sampler2D uNormalMap;
uniform sampler2D uFoamMap;
uniform sampler2D uSurfaceMap;
uniform vec3 uCameraDirection;
uniform sampler2D uRefractionMap;
uniform float uRefractionStrength;

varying vec2 vUv;
varying vec3 vWorldPos;
varying vec3 vOrigWorldPos;
varying vec4 vClipPos;

// 4-sample normal map blending (technique from Three.js Water.js)
// Uses prime-ratio divisors to break up repetition
vec4 getNoise(vec2 uv) {
  float t = uTime * 0.06;
  vec2 uv0 = (uv / 79.0) + vec2(t / 17.0, t / 29.0);
  vec2 uv1 = uv / 263.0 - vec2(t / -19.0, t / 31.0);
  vec2 uv2 = uv / vec2(8907.0, 9803.0) + vec2(t / 101.0, t / 97.0);
  vec2 uv3 = uv / vec2(1091.0, 1027.0) - vec2(t / 109.0, t / -113.0);
  vec4 noise = texture2D(uNormalMap, uv0) +
    texture2D(uNormalMap, uv1) +
    texture2D(uNormalMap, uv2) +
    texture2D(uNormalMap, uv3);
  return noise * 0.5 - 1.0;
}

void main() {
  // 1. Depth calculation (use undisplaced Y to avoid Gerstner artifacts in foam)
  float terrainHeight = texture2D(uHeightmap, vUv).r;
  float depth = max(0.0, vOrigWorldPos.y - terrainHeight);
  float depthFactor = clamp(depth / uMaxDepth, 0.0, 1.0);

  // 2. Depth-based color (smoothstep for gradual shallow-to-deep)
  float smoothDepth = smoothstep(0.0, 1.0, depthFactor);
  vec3 waterColor = mix(uShallowColor, uDeepColor, smoothDepth);

  // 3. Surface normal from 4-sample noise (Water.js technique)
  vec4 noise = getNoise(vOrigWorldPos.xz);
  vec3 surfaceNormal = normalize(noise.xzy * vec3(1.5, 1.0, 1.5));

  // View direction (from surface toward camera) — constant for orthographic
  vec3 viewDir = normalize(-uCameraDirection);

  // 4. Refraction: sample the scene rendered without water
  // Orthographic: w=1, so clip.xy / 1 maps directly to NDC
  vec2 screenUV = vClipPos.xy * 0.5 + 0.5;
  vec2 refractionUV = screenUV + surfaceNormal.xz * uRefractionStrength;
  refractionUV = clamp(refractionUV, 0.0, 1.0);
  vec3 refractionColor = texture2D(uRefractionMap, refractionUV).rgb;

  // Blend refraction with depth tint: shallow shows terrain, deep fades to deep color
  float refractionMix = 1.0 - smoothstep(0.0, 0.7, smoothDepth);
  waterColor = mix(waterColor, refractionColor, refractionMix * 0.7);

  // Specular: broad sun reflection
  vec3 halfDir = normalize(uSunDirection + viewDir);
  float NdotH = max(dot(surfaceNormal, halfDir), 0.0);
  float specBroad = pow(NdotH, 64.0) * 0.35;
  vec3 specular = uSunColor * specBroad;

  // Diffuse lighting
  float diffuse = max(dot(surfaceNormal, uSunDirection), 0.0) * 0.1;

  // Smoothed normal for reflection (avoids normal-map grid showing through)
  vec3 reflNormal = normalize(mix(vec3(0.0, 1.0, 0.0), surfaceNormal, 0.3));

  // Fresnel reflection
  float cosTheta = max(dot(viewDir, reflNormal), 0.0);
  float fresnel = 0.1 + 0.9 * pow(1.0 - cosTheta, 2.0);

  // Procedural sky reflection — 3-band gradient (ground / horizon haze / zenith)
  vec3 reflectDir = reflect(-viewDir, reflNormal);
  float skyY = clamp(reflectDir.y * 0.5 + 0.5, 0.0, 1.0);
  float skyBrightness = smoothstep(-0.1, 0.3, uSunDirection.y);

  vec3 groundColor = vec3(0.08, 0.12, 0.15) * skyBrightness;
  vec3 hazeColor = vec3(0.55, 0.65, 0.75) * skyBrightness;
  vec3 zenithColor = vec3(0.12, 0.25, 0.50) * skyBrightness;

  float sunsetFactor = 1.0 - smoothstep(0.0, 0.5, uSunDirection.y);
  hazeColor = mix(hazeColor, uSunColor * 0.5, sunsetFactor * 0.3);

  // Blend ground->haze->zenith
  vec3 skyReflection = mix(groundColor, hazeColor, smoothstep(0.0, 0.35, skyY));
  skyReflection = mix(skyReflection, zenithColor, smoothstep(0.35, 0.7, skyY));

  float sunDot = max(dot(reflectDir, uSunDirection), 0.0);
  skyReflection += uSunColor * pow(sunDot, 8.0) * 0.25;

  // 5. Shore foam — animated waves approaching the shoreline
  float noisePerturb = noise.x * 0.07 + noise.z * 0.04;
  float foamNoise = noise.x * 0.5 + 0.5;
  float noisyD = depth + noisePerturb;

  // Wave approach parameters
  float waveSpeed = 0.0175;
  float spawnDepth = 1.0;
  float bandHalfMax = 0.03;

  // Two wave cycles offset by half-period
  float cycle1 = fract(uTime * waveSpeed);
  float cycle2 = fract(uTime * waveSpeed + 0.5);

  // Movement stops at 70% of cycle, band lingers at shore for remaining 30%
  float movePhase1 = min(cycle1 / 0.7, 1.0);
  float movePhase2 = min(cycle2 / 0.7, 1.0);
  float minDepth = 0.25;
  float center1 = mix(spawnDepth, minDepth, movePhase1);
  float center2 = mix(spawnDepth, minDepth, movePhase2);

  // Fade in at start, fade out at end of cycle before next band spawns
  float fadeIn1 = smoothstep(0.0, 0.15, cycle1);
  float fadeIn2 = smoothstep(0.0, 0.15, cycle2);
  float fadeOut1 = 1.0 - smoothstep(0.85, 1.0, cycle1);
  float fadeOut2 = 1.0 - smoothstep(0.85, 1.0, cycle2);

  // Band widens and brightens as it approaches shore (like real waves breaking)
  float proximity1 = clamp(center1 / spawnDepth, 0.0, 1.0);
  float proximity2 = clamp(center2 / spawnDepth, 0.0, 1.0);
  // Per-position thickness variation using noise
  float thickVar1 = 0.7 + 0.6 * sin(vOrigWorldPos.x * 2.1 + vOrigWorldPos.z * 1.7 + center1 * 4.0);
  float thickVar2 = 0.7 + 0.6 * sin(vOrigWorldPos.x * 1.8 + vOrigWorldPos.z * 2.3 + center2 * 4.0);
  float bh1 = bandHalfMax * (0.15 + 0.85 * (1.0 - proximity1)) * thickVar1;
  float bh2 = bandHalfMax * (0.15 + 0.85 * (1.0 - proximity2)) * thickVar2;
  float bright1 = (1.0 + 0.6 * (1.0 - proximity1)) * fadeIn1 * fadeOut1;
  float bright2 = (1.0 + 0.6 * (1.0 - proximity2)) * fadeIn2 * fadeOut2;

  // Soft bands around each center
  float band1 = smoothstep(center1 - bh1 - 0.06, center1 - bh1, noisyD)
              * (1.0 - smoothstep(center1 + bh1, center1 + bh1 + 0.06, noisyD));
  float band2 = smoothstep(center2 - bh2 - 0.06, center2 - bh2, noisyD)
              * (1.0 - smoothstep(center2 + bh2, center2 + bh2 + 0.06, noisyD));

  // Density variation from noise, modulated by brightness
  band1 *= smoothstep(0.2, 0.55, foamNoise) * 0.25 * bright1;
  band2 *= smoothstep(0.25, 0.6, foamNoise) * 0.2 * bright2;

  // Subtle brightening near shore
  float foamGlow = (1.0 - smoothstep(0.0, 0.35, depth)) * 0.06;

  // Sample water surface texture (two layers scrolling slowly)
  float st = uTime * 0.008;
  vec2 surfUV0 = vWorldPos.xz * 0.12 + vec2(st, st * 0.7);
  vec2 surfUV1 = vWorldPos.xz * 0.08 - vec2(st * 0.6, st * 0.9);
  vec3 surfTex = (texture2D(uSurfaceMap, surfUV0).rgb + texture2D(uSurfaceMap, surfUV1).rgb) * 0.5;

  // Blend water with sky reflection via Fresnel, then add specular
  vec3 litWater = mix(waterColor, surfTex, 0.3) + diffuse;
  vec3 surfaceColor = mix(litWater, skyReflection, fresnel);
  surfaceColor += specular;

  // Sample foam texture moving with each band (UV offset tied to cycle)
  vec2 foamUV1 = vOrigWorldPos.xz * 0.4 + cycle1 * 0.3;
  vec2 foamUV2 = vOrigWorldPos.xz * 0.4 + cycle2 * 0.3;
  float foamTex1 = texture2D(uFoamMap, foamUV1).r;
  float foamTex2 = texture2D(uFoamMap, foamUV2).r;

  // Blend foam bands with texture
  float foamWithTex = clamp(max(max(band1 * foamTex1, band2 * foamTex2), foamGlow), 0.0, 1.0);
  vec3 foamColor = mix(vec3(0.85, 0.92, 0.95), vec3(1.0), foamWithTex);
  vec3 finalColor = mix(surfaceColor, foamColor, foamWithTex * 0.9);

  // 6. Alpha: deeper water is more opaque, foam adds opacity
  float alpha = mix(0.65, 0.95, smoothDepth);
  alpha = min(1.0, alpha + foamWithTex * 0.5);

  // 7. Shore edge softening: fade alpha near depth=0 with noise perturbation
  //    but preserve opacity where foam is visible
  float shoreFade = smoothstep(0.0, 0.25, depth + noise.y * 0.12 + noise.x * 0.06);
  alpha *= max(shoreFade, foamWithTex * 0.85);

  gl_FragColor = vec4(finalColor, alpha);
}
`

export interface WaterMaterialOptions {
  heightmapTexture: THREE.DataTexture
  normalMap: THREE.Texture
  foamMap: THREE.Texture
  surfaceMap: THREE.Texture
  refractionMap?: THREE.Texture | null
}

export function createWaterMaterial(
  options: WaterMaterialOptions
): THREE.ShaderMaterial {
  // Use a 1x1 white pixel as fallback when no refraction map is provided yet
  const fallbackTex = new THREE.DataTexture(
    new Uint8Array([128, 128, 128, 255]),
    1,
    1,
    THREE.RGBAFormat
  )
  fallbackTex.needsUpdate = true

  // Default Gerstner wave parameters: direction.xy, steepness, wavelength
  const degToDir = (deg: number) => {
    const rad = (deg * Math.PI) / 180
    return [Math.sin(rad), Math.cos(rad)]
  }
  const [ax, az] = degToDir(0)
  const [bx, bz] = degToDir(30)
  const [cx, cz] = degToDir(60)

  return new THREE.ShaderMaterial({
    uniforms: {
      uTime: { value: 0.0 },
      uHeightmap: { value: options.heightmapTexture },
      uShallowColor: { value: new THREE.Color(0.15, 0.45, 0.52) },
      uDeepColor: { value: new THREE.Color(0.02, 0.05, 0.18) },
      uMaxDepth: { value: 1.8 },
      uSunDirection: {
        value: new THREE.Vector3(0.5, 0.8, 0.3).normalize(),
      },
      uSunColor: { value: new THREE.Color(1.0, 0.95, 0.8) },
      uNormalMap: { value: options.normalMap },
      uFoamMap: { value: options.foamMap },
      uSurfaceMap: { value: options.surfaceMap },
      uCameraDirection: { value: new THREE.Vector3(0, -1, 0) },
      uRefractionMap: { value: options.refractionMap ?? fallbackTex },
      uRefractionStrength: { value: 0.02 },
      waveA: { value: new THREE.Vector4(ax, az, 0.03, 20) },
      waveB: { value: new THREE.Vector4(bx, bz, 0.02, 15) },
      waveC: { value: new THREE.Vector4(cx, cz, 0.015, 10) },
    },
    vertexShader,
    fragmentShader,
    transparent: true,
    depthWrite: false,
    side: THREE.FrontSide,
  })
}

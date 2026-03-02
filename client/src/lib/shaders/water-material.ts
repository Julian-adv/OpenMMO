import * as THREE from 'three'

const vertexShader = /* glsl */ `
uniform float uTime;

varying vec2 vUv;
varying vec3 vWorldPos;

void main() {
  vUv = uv;

  vec3 pos = position;

  // Two sine waves for gentle surface displacement (Y axis)
  float wave1 = sin(pos.x * 0.8 + uTime * 0.6) * cos(pos.z * 0.6 + uTime * 0.4) * 0.08;
  float wave2 = sin(pos.x * 1.5 + pos.z * 1.2 + uTime * 1.0) * 0.04;
  pos.y += wave1 + wave2;

  vec4 worldPos = modelMatrix * vec4(pos, 1.0);
  vWorldPos = worldPos.xyz;

  gl_Position = projectionMatrix * viewMatrix * worldPos;
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

varying vec2 vUv;
varying vec3 vWorldPos;

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
  // 1. Depth calculation
  float terrainHeight = texture2D(uHeightmap, vUv).r;
  float depth = max(0.0, vWorldPos.y - terrainHeight);
  float depthFactor = clamp(depth / uMaxDepth, 0.0, 1.0);

  // 2. Depth-based color (smoothstep for gradual shallow-to-deep)
  float smoothDepth = smoothstep(0.0, 1.0, depthFactor);
  vec3 waterColor = mix(uShallowColor, uDeepColor, smoothDepth);

  // 3. Surface normal from 4-sample noise (Water.js technique)
  vec4 noise = getNoise(vWorldPos.xz);
  vec3 surfaceNormal = normalize(noise.xzy * vec3(1.5, 1.0, 1.5));

  // 4. Specular: broad sun reflection + cell-based point sparkles
  vec3 viewDir = vec3(0.0, 1.0, 0.0);
  vec3 halfDir = normalize(uSunDirection + viewDir);
  float NdotH = max(dot(surfaceNormal, halfDir), 0.0);

  float specBroad = pow(NdotH, 64.0) * 0.35;

  vec3 specular = uSunColor * specBroad;

  // Diffuse lighting
  float diffuse = max(dot(surfaceNormal, uSunDirection), 0.0) * 0.1;

  // 5. Shore foam
  float foamEdge = 1.0 - smoothstep(0.0, 0.7, depth);
  float foamScroll = sin(vWorldPos.x * 0.7 + uTime * 0.8) * sin(vWorldPos.z * 0.9 + uTime * 0.6);
  float foam = foamEdge * max(noise.x + foamScroll * 0.2, 0.0) * 0.6;

  // Combine
  vec3 finalColor = waterColor + diffuse + specular + vec3(foam);

  // 6. Alpha
  float alpha = mix(0.65, 0.95, smoothDepth);
  alpha = min(1.0, alpha + foam * 0.4);

  // 7. Shore edge softening: fade alpha near depth=0 with noise perturbation
  float shoreFade = smoothstep(0.0, 0.3, depth + noise.y * 0.08);
  alpha *= shoreFade;

  gl_FragColor = vec4(finalColor, alpha);
}
`

export interface WaterMaterialOptions {
  heightmapTexture: THREE.DataTexture
  normalMap: THREE.Texture
}

export function createWaterMaterial(
  options: WaterMaterialOptions
): THREE.ShaderMaterial {
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
    },
    vertexShader,
    fragmentShader,
    transparent: true,
    depthWrite: false,
    side: THREE.FrontSide,
  })
}

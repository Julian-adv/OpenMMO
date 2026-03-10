import * as THREE from 'three'
import { NodeMaterial } from 'three/webgpu'
import {
  Fn,
  uniform,
  texture,
  uv,
  vec2,
  vec3,
  vec4,
  float,
  sin,
  cos,
  sqrt,
  dot,
  normalize,
  smoothstep,
  mix,
  clamp,
  pow,
  fract,
  floor,
  max,
  reflect,
  varying,
  positionLocal,
  modelWorldMatrix,
  cameraProjectionMatrix,
  cameraViewMatrix,
} from 'three/tsl'

// ─── Uniforms ────────────────────────────────────────────
const PI = float(Math.PI)

// ─── Gerstner Wave (TSL Fn) ─────────────────────────────
const gerstnerWave = /* #__PURE__ */ Fn(
  ([wave_immutable, p_immutable, time_immutable]: [
    ReturnType<typeof vec4>,
    ReturnType<typeof vec3>,
    ReturnType<typeof float>,
  ]) => {
    const wave = vec4(wave_immutable)
    const p = vec3(p_immutable)
    const time = float(time_immutable)

    const steepness = wave.z
    const wavelength = wave.w
    const k = PI.mul(2).div(wavelength)
    const c = sqrt(float(9.8).div(k))
    const d = normalize(wave.xy)
    const f = k.mul(dot(d, p.xz).sub(c.mul(time).mul(0.1)))
    const a = steepness.div(k)
    // Only vertical (Y) displacement to avoid tile boundary tearing
    return vec3(0, a.mul(sin(f)), 0)
  }
)

// ─── Gerstner Wave Normal contribution ──────────────────
// Returns the tangent/bitangent partial derivatives for a single wave
// so we can analytically compute the surface normal from summed waves.
const gerstnerNormal = /* #__PURE__ */ Fn(
  ([wave_immutable, p_immutable, time_immutable]: [
    ReturnType<typeof vec4>,
    ReturnType<typeof vec3>,
    ReturnType<typeof float>,
  ]) => {
    const wave = vec4(wave_immutable)
    const p = vec3(p_immutable)
    const time = float(time_immutable)

    const steepness = wave.z
    const wavelength = wave.w
    const k = PI.mul(2).div(wavelength)
    const c = sqrt(float(9.8).div(k))
    const d = normalize(wave.xy)
    const f = k.mul(dot(d, p.xz).sub(c.mul(time).mul(0.1)))

    // Partial derivatives: dP/dx and dP/dz contributions
    // tangent_x += -d.x * d.x * steepness * sin(f)
    // tangent_y +=  d.x * steepness * cos(f)
    // bitangent_z += -d.y * d.y * steepness * sin(f)
    // bitangent_y +=  d.y * steepness * cos(f)
    const sf = sin(f)
    const cf = cos(f)
    // Return vec4(tx, ty, bz, by) for accumulation
    return vec4(
      d.x.mul(d.x).mul(steepness).mul(sf).negate(),
      d.x.mul(steepness).mul(cf),
      d.y.mul(d.y).mul(steepness).mul(sf).negate(),
      d.y.mul(steepness).mul(cf)
    )
  }
)

// ─── Hash-based value noise ─────────────────────────────
const hash = /* #__PURE__ */ Fn(([p_immutable]: [ReturnType<typeof vec2>]) => {
  const p = vec2(p_immutable)
  return fract(sin(dot(p, vec2(127.1, 311.7))).mul(43758.5453))
})

const valueNoise = /* #__PURE__ */ Fn(
  ([p_immutable]: [ReturnType<typeof vec2>]) => {
    const p = vec2(p_immutable)
    const i = floor(p)
    const fv = fract(p)
    const f = fv.mul(fv).mul(float(3).sub(fv.mul(2))) // smoothstep interpolation

    const a = hash(i)
    const b = hash(i.add(vec2(1.0, 0.0)))
    const c = hash(i.add(vec2(0.0, 1.0)))
    const d = hash(i.add(vec2(1.0, 1.0)))

    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y)
  }
)

// ─── 4-sample normal noise ──────────────────────────────
const getNoise = /* #__PURE__ */ Fn(
  ([
    worldXZ_immutable,
    normalMapTex,
    time_immutable,
    waveA_immutable,
    waveB_immutable,
    waveC_immutable,
  ]: [
    ReturnType<typeof vec2>,
    ReturnType<typeof texture>,
    ReturnType<typeof float>,
    ReturnType<typeof vec4>,
    ReturnType<typeof vec4>,
    ReturnType<typeof vec4>,
  ]) => {
    const worldXZ = vec2(worldXZ_immutable)
    const time = float(time_immutable)
    const t = time.mul(0.06)

    // Wave directions and phase speeds from Gerstner uniforms
    const dirA = normalize(vec4(waveA_immutable).xy)
    const dirB = normalize(vec4(waveB_immutable).xy)
    const dirC = normalize(vec4(waveC_immutable).xy)
    const kA = PI.mul(2).div(vec4(waveA_immutable).w)
    const kB = PI.mul(2).div(vec4(waveB_immutable).w)
    const kC = PI.mul(2).div(vec4(waveC_immutable).w)
    const cA = sqrt(float(9.8).div(kA)).mul(0.1)
    const cB = sqrt(float(9.8).div(kB)).mul(0.1)
    const cC = sqrt(float(9.8).div(kC)).mul(0.1)

    // UV offset follows wave direction * phase speed * time
    const wlA = vec4(waveA_immutable).w
    const wlB = vec4(waveB_immutable).w
    const wlC = vec4(waveC_immutable).w
    const uv0 = worldXZ.div(wlA.mul(0.5)).add(dirA.mul(cA.mul(t).mul(0.3)))
    const uv1 = worldXZ.div(wlB.mul(0.5)).add(dirB.mul(cB.mul(t).mul(0.2)))
    const uv2 = worldXZ.div(wlC.mul(0.5)).add(dirC.mul(cC.mul(t).mul(0.1)))

    const noise = normalMapTex
      .sample(uv0)
      .add(normalMapTex.sample(uv1))
      .add(normalMapTex.sample(uv2))

    return noise.mul(0.5).sub(1.0)
  }
)

// ─── Export interface ────────────────────────────────────
export interface WaterMaterialOptions {
  heightmapTexture: THREE.DataTexture
  normalMap: THREE.Texture
  foamMap: THREE.Texture
  causticsMap: THREE.Texture
  refractionMap?: THREE.Texture | null
  reflectionMap?: THREE.Texture | null
}

export interface WaterMaterialUniforms {
  uTime: { value: number }
  uSunDirection: { value: THREE.Vector3 }
  uSunColor: { value: THREE.Color }
  uCameraDirection: { value: THREE.Vector3 }
  uMoonBrightness: { value: number }
  uRefractionMap: { value: THREE.Texture }
  uReflectionMap: { value: THREE.Texture }
  uHeightmapTexture: { value: THREE.Texture }
}

// Module-level wave configs shared across all tiles to ensure matching heights at boundaries
const waveConfigs = [
  {
    angle: Math.random() * Math.PI * 2,
    speed: 0.0013,
    steepness: 0.06,
    wavelength: 20,
  },
  {
    angle: Math.random() * Math.PI * 2,
    speed: 0.0021,
    steepness: 0.04,
    wavelength: 14,
  },
  {
    angle: Math.random() * Math.PI * 2,
    speed: 0.0009,
    steepness: 0.03,
    wavelength: 9,
  },
]

export interface WaterMaterialResult {
  material: NodeMaterial
  updateWaveDirections: (elapsed: number) => void
  uniforms: WaterMaterialUniforms
}

export function createWaterMaterial(
  options: WaterMaterialOptions
): WaterMaterialResult {
  const fallbackTex = new THREE.DataTexture(
    new Uint8Array([128, 128, 128, 255]),
    1,
    1,
    THREE.RGBAFormat
  )
  fallbackTex.needsUpdate = true

  // Scalar/vector uniforms
  const uTime = uniform(0)
  const uWaveA = uniform(
    new THREE.Vector4(0, 1, waveConfigs[0].steepness, waveConfigs[0].wavelength)
  )
  const uWaveB = uniform(
    new THREE.Vector4(0, 1, waveConfigs[1].steepness, waveConfigs[1].wavelength)
  )
  const uWaveC = uniform(
    new THREE.Vector4(0, 1, waveConfigs[2].steepness, waveConfigs[2].wavelength)
  )
  const waveUniforms = [uWaveA, uWaveB, uWaveC]
  // 4-stop water color gradient (hex references from target image)
  // 1. Very shallow: nearly transparent mint
  const uVeryShallowColor = uniform(new THREE.Color(0.75, 0.88, 0.78))
  // 2. Shallow: turquoise-green (바다2)
  const uShallowColor = uniform(new THREE.Color(0.2, 0.58, 0.42))
  // 3. Mid: darker turquoise-navy (바다3)
  const uMidColor = uniform(new THREE.Color(0.02, 0.34, 0.32))
  // 4. Deep: deep navy (바다4)
  const uDeepColor = uniform(new THREE.Color(0.002, 0.06, 0.18))
  const uMaxDepth = uniform(2.5)
  const uSunDirection = uniform(new THREE.Vector3(0.5, 0.8, 0.3).normalize())
  const uSunColor = uniform(new THREE.Color(1.0, 0.95, 0.8))
  const uCameraDirection = uniform(new THREE.Vector3(0, -1, 0))
  const uMoonBrightness = uniform(0)
  const uRefractionStrength = uniform(0.1)

  // Texture nodes (use texture() directly — update via .value)
  const heightmapTex = texture(options.heightmapTexture)
  const normalMapTex = texture(options.normalMap)
  const foamMapTex = texture(options.foamMap)
  const causticsTex = texture(options.causticsMap)
  const refractionTex = texture(options.refractionMap ?? fallbackTex)
  const reflectionTex = texture(options.reflectionMap ?? fallbackTex)

  // ─── Vertex: Gerstner wave displacement ────────────
  const vOrigWorldPos = varying(vec3(0), 'v_origWorldPos')
  const vWorldPos = varying(vec3(0), 'v_worldPos')
  const vWaveHeight = varying(float(0), 'v_waveHeight')
  const vClipPos = varying(vec4(0), 'v_clipPos')
  const vUv = varying(vec2(0), 'v_uv')

  const positionNode = Fn(() => {
    const localPos = positionLocal.toVar()
    vUv.assign(uv())

    const worldPos = modelWorldMatrix.mul(vec4(localPos, 1.0)).toVar()
    vOrigWorldPos.assign(worldPos.xyz)

    const p = worldPos.xyz
    // Sample heightmap in vertex to get approximate depth for wave damping
    const vtxHeightUV = vUv.mul(64.0 / 65.0).add(0.5 / 65.0)
    const vtxTerrainH = heightmapTex.sample(vtxHeightUV).r
    const vtxDepth = max(float(0), p.y.sub(vtxTerrainH))
    // Dampen Gerstner waves in shallow water (fade out over depth 0~1.5)
    const waveDamping = smoothstep(float(0.0), float(1.5), vtxDepth)

    const offset = gerstnerWave(uWaveA, p, uTime)
      .add(gerstnerWave(uWaveB, p, uTime))
      .add(gerstnerWave(uWaveC, p, uTime))
      .mul(waveDamping)
      .toVar()

    worldPos.xyz.addAssign(offset)
    vWaveHeight.assign(offset.y)

    vWorldPos.assign(worldPos.xyz)

    const clipPos = cameraProjectionMatrix.mul(cameraViewMatrix).mul(worldPos)
    vClipPos.assign(clipPos)

    return clipPos
  })()

  // ─── Fragment: full water shading ──────────────────
  const fragmentNode = Fn(() => {
    // 1. Depth calculation
    // Remap UV to align 65 vertices with 65×65 texel centers:
    // vertex k has UV = k/64, texel center k is at (k+0.5)/65
    const heightmapUV = vUv.mul(64.0 / 65.0).add(0.5 / 65.0)
    const terrainHeight = heightmapTex.sample(heightmapUV).r
    const depth = max(float(0), vOrigWorldPos.y.sub(terrainHeight))
    const depthFactor = clamp(depth.div(uMaxDepth), 0.0, 1.0)

    // 2. Depth-based color (4-stop gradient)
    // Very shallow → Shallow (0.0 ~ 0.08)
    const c1 = mix(
      uVeryShallowColor,
      uShallowColor,
      smoothstep(float(0.0), float(0.08), depthFactor)
    )
    // Shallow → Mid (0.08 ~ 0.25)
    const c2 = mix(
      c1,
      uMidColor,
      smoothstep(float(0.08), float(0.25), depthFactor)
    )
    // Mid → Deep (0.25 ~ 0.7)
    const waterColor = mix(
      c2,
      uDeepColor,
      smoothstep(float(0.25), float(0.7), depthFactor)
    ).toVar()

    // 3a. Gerstner wave analytical normal (large swells)
    const waveP = vOrigWorldPos
    const gnA = gerstnerNormal(uWaveA, waveP, uTime)
    const gnB = gerstnerNormal(uWaveB, waveP, uTime)
    const gnC = gerstnerNormal(uWaveC, waveP, uTime)
    const tx = float(1.0).add(gnA.x).add(gnB.x).add(gnC.x)
    const ty = gnA.y.add(gnB.y).add(gnC.y)
    const bz = float(1.0).add(gnA.z).add(gnB.z).add(gnC.z)
    const by = gnA.w.add(gnB.w).add(gnC.w)
    const gerstnerN = normalize(vec3(ty.negate(), tx.mul(bz), by.negate()))

    // 3b. Normal map sampling for small ripples
    const rippleNoise = getNoise(
      vOrigWorldPos.xz,
      normalMapTex,
      uTime,
      uWaveA,
      uWaveB,
      uWaveC
    )
    const rippleN = rippleNoise.xzy.mul(vec3(1.5, 0.0, 1.5))

    // 3c. Combine: perturb Gerstner normal with ripple detail
    const surfaceNormal = normalize(gerstnerN.add(rippleN))

    // View direction
    const viewDir = normalize(vec3(uCameraDirection).negate())

    // 4. Refraction — distort UV by surface normals for underwater ripple
    const screenUV = vClipPos.xy.mul(0.5).add(0.5)
    // Y-flip for WebGPU render target coordinate convention
    const refractionBaseUV = vec2(screenUV.x, float(1.0).sub(screenUV.y))
    // Sample normal map directly for stronger refraction distortion
    const refrUV1 = vOrigWorldPos.xz.mul(0.08).add(uTime.mul(0.015))
    const refrUV2 = vOrigWorldPos.xz.mul(0.06).sub(uTime.mul(0.01))
    const refrNoise = normalMapTex
      .sample(refrUV1)
      .rg.add(normalMapTex.sample(refrUV2).rg)
      .sub(1.0) // center around 0 (each sample 0~1, sum 0~2, -1 → -1~1)
    const refractionDistort = refrNoise.mul(uRefractionStrength)
    const refractionUV = clamp(
      refractionBaseUV.add(refractionDistort),
      0.0,
      1.0
    )
    const refractionColor = refractionTex.sample(refractionUV).rgb

    // Darken water color at night before mixing (prevents emerald tint on dark refraction)
    const waterNightFactor = smoothstep(
      float(-0.05),
      float(0.1),
      uSunDirection.y
    )
      .mul(0.85)
      .add(0.15)
    waterColor.mulAssign(waterNightFactor)

    // Refraction — visible in 바다1 and 바다2 (depthFactor 0 ~ 0.25)
    // Stay strong (0.95) up to depthFactor 0.15, then drop sharply to 0 by 0.35
    const refractionMix = float(1)
      .sub(smoothstep(float(0.05), float(0.35), depthFactor))
      .mul(0.95)
    waterColor.assign(mix(waterColor, refractionColor, refractionMix))

    // Underwater caustics — light pattern on the seafloor, seen through refraction
    const cUV1 = vOrigWorldPos.xz
      .mul(0.1)
      .add(vec2(uTime.mul(0.015), uTime.mul(0.01)))
    const cUV2 = vOrigWorldPos.xz
      .mul(0.095)
      .sub(vec2(uTime.mul(0.008), uTime.mul(0.01)))
    const causticsLayer1 = causticsTex.sample(cUV1).r
    const causticsLayer2 = causticsTex.sample(cUV2).r
    const rawCaustics = causticsLayer1.min(causticsLayer2)
    const causticsDetail = foamMapTex.sample(
      vOrigWorldPos.xz.mul(0.3).add(uTime.mul(0.01))
    ).r
    const causticsPattern = rawCaustics
      .min(float(0.5))
      .div(float(0.5))
      .mul(causticsDetail)
    // Shimmer: high-frequency flicker based on position + time
    const shimmer = sin(
      vOrigWorldPos.x.mul(0.4).add(vOrigWorldPos.z.mul(0.6)).add(uTime.mul(0.5))
    )
      .mul(0.4)
      .add(0.8) // oscillate between 0.4 and 1.2
    const causticsShimmer = causticsPattern.mul(shimmer)
    const causticsStrength = float(1).sub(
      smoothstep(float(0), float(0.5), depthFactor)
    )
    // Brighten the refraction (seafloor) where caustics lines are, then blend into water
    // At night use dim blue-grey tint instead of sunColor
    const causticsNightFactor = smoothstep(
      float(-0.05),
      float(0.1),
      uSunDirection.y
    )
    const causticsLightColor = mix(
      vec3(0.08, 0.1, 0.15),
      vec3(uSunColor),
      causticsNightFactor
    )
    // Additive caustics on water surface — only where caustics pattern exists
    const causticsDepthGate = smoothstep(float(0.05), float(0.25), depthFactor)
    waterColor.addAssign(
      causticsLightColor
        .mul(causticsShimmer.mul(1.2))
        .mul(causticsStrength)
        .mul(causticsDepthGate)
    )

    // Specular: use gentle normal to avoid cloud-like patches
    const specNormal = normalize(mix(vec3(0, 1, 0), surfaceNormal, 0.3))
    const halfDir = normalize(vec3(uSunDirection).add(viewDir))
    const NdotH = max(dot(specNormal, halfDir), 0.0)
    const specBroad = pow(NdotH, float(128)).mul(0.3)
    const specular = vec3(uSunColor).mul(specBroad).toVar()

    // Sun sparkles – use displaced world pos so sparkles ride the waves
    const spT = uTime.mul(0.04)
    const spUV1 = vWorldPos.xz.mul(0.5).add(vec2(spT, spT.mul(0.7)))
    const spUV2 = vWorldPos.xz.mul(0.8).sub(vec2(spT.mul(0.6), spT))
    const sp1 = normalMapTex.sample(spUV1).r
    const sp2 = normalMapTex.sample(spUV2).g
    // Boost sparkles on wave crests, dim in troughs
    const waveCrestFactor = smoothstep(float(-0.05), float(0.1), vWaveHeight)
      .mul(0.8)
      .add(0.2)
    const sunSparkleStrength = smoothstep(
      float(0),
      float(0.15),
      uSunDirection.y
    ).mul(float(0.3).add(float(0.7).mul(uSunDirection.y)))
    // At night, keep a faint moonlit sparkle (only when a moon is actually visible)
    const moonSparkleStrength = float(1)
      .sub(smoothstep(float(-0.05), float(0.05), uSunDirection.y))
      .mul(0.15)
      .mul(smoothstep(float(0), float(0.1), uMoonBrightness))
    const sparkle = smoothstep(float(1.3), float(1.45), sp1.add(sp2))
      .mul(8.0)
      .mul(waveCrestFactor)
      .mul(max(sunSparkleStrength, moonSparkleStrength))
    specular.addAssign(vec3(uSunColor).mul(sparkle))

    // Smoothed normal for reflection
    const reflNormal = normalize(mix(vec3(0, 1, 0), surfaceNormal, 0.3))

    // Procedural sky reflection — time-of-day color palette
    const reflectDir = reflect(viewDir.negate(), reflNormal)
    const skyY = clamp(reflectDir.y.mul(0.5).add(0.5), 0.0, 1.0)
    const sunY = uSunDirection.y

    // Blend factors: night → twilight → day
    const nightFactor = float(1).sub(
      smoothstep(float(-0.15), float(0.05), sunY)
    )
    const twilightFactor = smoothstep(float(-0.15), float(0.0), sunY).mul(
      float(1).sub(smoothstep(float(0.05), float(0.3), sunY))
    )
    const dayFactor = smoothstep(float(0.05), float(0.3), sunY)

    // Night palette — dark blues
    const nightGround = vec3(0.02, 0.03, 0.06)
    const nightHaze = vec3(0.04, 0.06, 0.12)
    const nightZenith = vec3(0.02, 0.04, 0.1)

    // Twilight palette — warm oranges/purples
    const twiGround = vec3(0.12, 0.06, 0.04)
    const twiHaze = vec3(0.7, 0.35, 0.15)
    const twiZenith = vec3(0.15, 0.1, 0.25)

    // Day palette
    const dayGround = vec3(0.08, 0.12, 0.15)
    const dayHaze = vec3(0.55, 0.65, 0.75)
    const dayZenith = vec3(0.12, 0.25, 0.5)

    // Blend palettes by time of day
    const groundColor = nightGround
      .mul(nightFactor)
      .add(twiGround.mul(twilightFactor))
      .add(dayGround.mul(dayFactor))
    const hazeColorBase = nightHaze
      .mul(nightFactor)
      .add(twiHaze.mul(twilightFactor))
      .add(dayHaze.mul(dayFactor))
    const zenithColor = nightZenith
      .mul(nightFactor)
      .add(twiZenith.mul(twilightFactor))
      .add(dayZenith.mul(dayFactor))

    // Tint haze with sun color during sunset/sunrise
    const sunsetFactor = smoothstep(float(-0.05), float(0.0), sunY).mul(
      float(1).sub(smoothstep(float(0.0), float(0.3), sunY))
    )
    const hazeColor = mix(
      hazeColorBase,
      vec3(uSunColor).mul(0.6),
      sunsetFactor.mul(0.5)
    )

    const skyReflection = mix(
      mix(groundColor, hazeColor, smoothstep(float(0), float(0.35), skyY)),
      zenithColor,
      smoothstep(float(0.35), float(0.7), skyY)
    ).toVar()

    // Sun highlight on water
    const sunDot = max(dot(reflectDir, vec3(uSunDirection)), 0.0)
    skyReflection.addAssign(
      vec3(uSunColor).mul(pow(sunDot, float(8)).mul(0.25))
    )

    // Entity reflection (planar reflection pass)
    // Y-flip to convert from clip-space UV to render-target texture UV
    const reflUV = vec2(screenUV.x, float(1.0).sub(screenUV.y))
    const reflectionSample = reflectionTex.sample(
      clamp(reflUV.add(surfaceNormal.xz.mul(0.01)), 0.0, 1.0)
    )
    // Where alpha > 0, use entity reflection instead of sky
    skyReflection.assign(
      mix(skyReflection, reflectionSample.rgb, reflectionSample.a.mul(0.5))
    )

    // Wave speed & cycles (shared by shore drawback + foam bands)
    const waveSpeed = float(0.012)
    const cycle1 = fract(uTime.mul(waveSpeed))
    const cycle2 = fract(uTime.mul(waveSpeed).add(0.5))
    const move1 = smoothstep(float(0), float(0.7), cycle1)
    const move2 = smoothstep(float(0), float(0.7), cycle2)

    // Shore drawback — smooth sine oscillation, 2x frequency to match two foam bands
    const shorePhase = uTime
      .mul(waveSpeed)
      .mul(PI.mul(4))
      .sub(PI.mul(1.0 / 2.0))
    const shoreRecede = sin(shorePhase).mul(0.5).add(0.5)
    const shoreDepthOffset = shoreRecede.mul(0.35)
    const shoreAdjustedDepth = max(float(0), depth.sub(shoreDepthOffset))
    const shoreZone = float(1).sub(
      smoothstep(float(0), float(0.45), shoreAdjustedDepth)
    )
    const sn1 = valueNoise(vOrigWorldPos.xz.mul(0.2).add(uTime.mul(0.07)))
    const sn2 = valueNoise(vOrigWorldPos.xz.mul(0.4).add(uTime.mul(0.04)))
    const sn3 = valueNoise(vOrigWorldPos.xz.mul(0.08).add(uTime.mul(0.1)))
    const holeMask = sn1.mul(0.5).add(sn2.mul(0.3)).add(sn3.mul(0.2))
    const edgeCutoff = smoothstep(float(0), float(0.01), depth)
    const holeThreshold = shoreZone.mul(0.9)
    const holeAlpha = smoothstep(
      holeThreshold.sub(0.05),
      holeThreshold.add(0.05),
      holeMask
    ).mul(edgeCutoff)
    // Hole edge foam — straddles the threshold so foam bleeds slightly into hole side
    const distFromHole = holeMask.sub(holeThreshold)
    // Foam ramps up from slightly inside hole (-0.03) and fades out into water (0.5)
    const holeEdge = smoothstep(float(-0.03), float(0.01), distFromHole)
      .mul(float(1).sub(smoothstep(float(0.01), float(0.5), distFromHole)))
      .mul(shoreZone)
    // Keep alpha visible in the hole-side foam fringe
    const holeFoamFringe = smoothstep(
      float(-0.03),
      float(0.0),
      distFromHole
    ).mul(shoreZone)

    // 5. Shore foam — wide breaking waves

    // Noise-perturbed depth for irregular edges
    const foamNoise2 = valueNoise(vOrigWorldPos.xz.mul(0.3))
    const foamNoise3 = valueNoise(vOrigWorldPos.xz.mul(0.15))
    const noisyD = depth
      .add(foamNoise2.mul(0.15))
      .add(foamNoise3.mul(0.1))
      .add(valueNoise(vOrigWorldPos.xz.mul(0.2)).mul(0.3))

    // Waves move from deeper water toward shore
    const spawnDepth = float(1.5)
    const shoreDepth = float(0.15)
    const center1 = mix(spawnDepth, shoreDepth, move1)
    const center2 = mix(spawnDepth, shoreDepth, move2)

    // Fade in/out
    const fade1 = smoothstep(float(0), float(0.1), cycle1).mul(
      float(1).sub(smoothstep(float(0.9), float(1), cycle1))
    )
    const fade2 = smoothstep(float(0), float(0.1), cycle2).mul(
      float(1).sub(smoothstep(float(0.9), float(1), cycle2))
    )

    // Bands widen near shore (wave shoaling)
    const bandWidth1 = float(0.04).add(float(0.1).mul(move1))
    const bandWidth2 = float(0.04).add(float(0.1).mul(move2))

    // Soft band shape
    const band1 = smoothstep(center1.sub(bandWidth1), center1, noisyD)
      .mul(float(1).sub(smoothstep(center1, center1.add(bandWidth1), noisyD)))
      .mul(fade1)
      .toVar()
    const band2 = smoothstep(center2.sub(bandWidth2), center2, noisyD)
      .mul(float(1).sub(smoothstep(center2, center2.add(bandWidth2), noisyD)))
      .mul(fade2)
      .toVar()

    // Break up with large-scale noise for organic edges
    const bn1 = valueNoise(vOrigWorldPos.xz.mul(0.15).add(center1.mul(1.5)))
    const bn2 = valueNoise(vOrigWorldPos.xz.mul(0.15).add(center2.mul(1.5)))
    band1.mulAssign(smoothstep(float(0.2), float(0.5), bn1))
    band2.mulAssign(smoothstep(float(0.2), float(0.5), bn2))

    // Shore foam at hole edges — foam fringe at water boundary
    // Dim shore foam at night so it matches wave foam brightness
    const shoreDayNight = smoothstep(float(-0.05), float(0.1), sunY)
    const shoreBase = holeEdge.mul(mix(float(0.5), float(1.4), shoreDayNight))

    // Brightening near shore
    const foamGlow = float(1)
      .sub(smoothstep(float(0), float(0.4), depth))
      .mul(0.15)

    // Blend water with sky reflection via Fresnel, then add specular
    // Dampen fresnel and specular in shallows to keep refracted sand clean
    // Normal-based ripple shading — directly modulate water brightness
    const fresnelViewDir = normalize(vec3(viewDir.x, float(0.15), viewDir.z))
    const NdotV = max(dot(surfaceNormal, fresnelViewDir), 0.0)
    // Ripple brightness: normals facing away get brighter, facing toward get darker
    const rippleBright = mix(
      float(0.75),
      float(1.25),
      pow(float(1).sub(NdotV), float(1.5))
    )
    waterColor.mulAssign(rippleBright)

    // Tint sky reflection toward turquoise so it doesn't wash out the water color
    const tintedSkyReflection = mix(
      skyReflection,
      vec3(uMidColor).mul(1.3),
      float(0.7)
    )
    // Less reflection in shallows to preserve transparent look
    const shallowDamp = smoothstep(float(0.1), float(0.4), depthFactor)
    const fresnel = pow(float(1).sub(NdotV), float(2)).mul(0.08)
    const surfaceColor = mix(
      waterColor,
      tintedSkyReflection,
      mix(float(0.005), float(0.06), shallowDamp).add(fresnel)
    )
      .add(specular.mul(shallowDamp))
      .toVar()

    // Foam texture with band movement
    const foamUV1 = vOrigWorldPos.xz.mul(0.4).add(cycle1.mul(0.3))
    const foamUV2 = vOrigWorldPos.xz.mul(0.4).add(cycle2.mul(0.3))
    const foamTex1 = foamMapTex.sample(foamUV1).r
    const foamTex2 = foamMapTex.sample(foamUV2).r

    // Shore foam texture (two layers at different scales, slowly moving)
    const shoreFoamUV1 = vOrigWorldPos.xz
      .mul(0.5)
      .add(vec2(uTime.mul(0.006), uTime.mul(0.004)))
    const shoreFoamUV2 = vOrigWorldPos.xz
      .mul(0.35)
      .sub(vec2(uTime.mul(0.003), uTime.mul(0.005)))
    const shoreFoamTex = max(
      foamMapTex.sample(shoreFoamUV1).r,
      foamMapTex.sample(shoreFoamUV2).r
    )
    const shoreBaseTex = shoreBase.mul(shoreFoamTex)

    // Blend foam — combine wave bands, persistent shore foam, and glow
    const waveFoam = max(band1.mul(foamTex1), band2.mul(foamTex2))
    const foamWithTex = clamp(
      max(max(waveFoam, shoreBaseTex), foamGlow),
      0.0,
      1.0
    )
    // Foam: purely additive — no color replacement, just add white on top
    const foamDayNight = smoothstep(float(-0.05), float(0.1), sunY)
    const foamDepthMask = float(1)
      .sub(smoothstep(float(0.3), float(0.7), depthFactor))
      .mul(0.7)
      .add(0.3)
    // Daytime: bright white additive. Night: dimmer additive.
    const foamAddStrength = mix(float(0.06), float(0.7), foamDayNight)
    const foamAdd = vec3(1, 1, 1).mul(
      foamWithTex.mul(foamAddStrength).mul(foamDepthMask)
    )
    const finalColorBeforeRefl = surfaceColor.toVar()

    // Overlay entity reflection
    finalColorBeforeRefl.assign(
      mix(
        finalColorBeforeRefl,
        reflectionSample.rgb,
        reflectionSample.a.mul(0.3)
      )
    )
    // Caustics glow — DISABLED for testing
    // const glowNightFactor = smoothstep(float(-0.05), float(0.1), sunY)
    // const glowColor = mix(
    //   vec3(0.08, 0.1, 0.15),
    //   vec3(uSunColor),
    //   glowNightFactor
    // )
    // const causticsGlow = glowColor
    //   .mul(pow(causticsShimmer, float(2.0)).mul(causticsStrength).mul(1.5))
    //   .mul(causticsDepthGate)
    // finalColorBeforeRefl.addAssign(causticsGlow)

    // Darken water surface at night (match scene ambient)
    const nightDarken = smoothstep(float(-0.05), float(0.1), sunY)
      .mul(0.75)
      .add(0.25)
    // Extra darkening for mid-depth water at night (emerald zone)
    const midDepthWeight = smoothstep(
      float(0.15),
      float(0.35),
      depthFactor
    ).mul(float(1).sub(smoothstep(float(0.5), float(0.8), depthFactor)))
    const nightExtra = float(1).sub(
      float(1).sub(nightDarken).mul(midDepthWeight).mul(0.35)
    )
    const nightDarkenFull = nightDarken.mul(nightExtra)
    finalColorBeforeRefl.mulAssign(nightDarkenFull)

    // Additive foam AFTER night darkening — so foam white isn't darkened with the base
    finalColorBeforeRefl.addAssign(foamAdd)
    finalColorBeforeRefl.addAssign(vec3(1, 1, 1).mul(shoreBaseTex.mul(0.4)))

    const finalColor = finalColorBeforeRefl

    // 6. Alpha — very transparent at shore, gradually opaque
    // Very shallow: 0.05 (sand clearly visible), shallow: 0.35, mid+: 0.97
    const a1 = mix(
      float(0.05),
      float(0.35),
      smoothstep(float(0.0), float(0.08), depthFactor)
    )
    const baseAlpha = mix(
      a1,
      float(0.97),
      smoothstep(float(0.08), float(0.35), depthFactor)
    )
    // Boost alpha where refraction is active so the undistorted terrain beneath
    // doesn't bleed through and cancel the refraction distortion effect
    const refrAlphaBoost = refractionMix.mul(0.9)
    const alpha = max(baseAlpha, refrAlphaBoost)
      .add(foamWithTex.mul(0.9))
      .add(sparkle)
      .min(1.0)
      .toVar()

    // 7. Shore edge — reuse hole variables computed earlier
    alpha.mulAssign(max(holeAlpha, holeFoamFringe))

    // 7b. Wet sand — darken refraction (terrain) color near hole boundary
    // Uses actual terrain appearance instead of flat color for natural look
    const wetMask = smoothstep(float(-0.55), float(-0.05), distFromHole)
      .mul(float(1).sub(smoothstep(float(0.0), float(0.02), distFromHole)))
      .mul(edgeCutoff)
      .mul(
        mix(
          smoothstep(float(0.85), float(0.9), shoreRecede), // 밀물: 빨리 사라짐
          smoothstep(float(0.05), float(0.15), shoreRecede), // 썰물: 빨리 나타남
          smoothstep(float(0.0), float(0.1), cos(shorePhase)) // 방향 감지
        )
      )
    // Sample refraction (terrain color) and darken it to simulate wetness
    const wetTerrainColor = refractionColor.mul(0.5)
    finalColorBeforeRefl.assign(
      mix(finalColorBeforeRefl, wetTerrainColor, wetMask)
    )
    // Restore alpha in wet zone so darkened terrain is visible through holes
    alpha.assign(max(alpha, wetMask.mul(0.85)))

    // At night, only make very shallow water (바다1) more transparent
    const veryShallowWeight = float(1).sub(
      smoothstep(float(0.0), float(0.08), depthFactor)
    )
    const nightAlphaReduce = float(1).sub(
      float(1).sub(nightDarken).mul(veryShallowWeight).mul(0.5)
    )
    alpha.mulAssign(nightAlphaReduce)

    return vec4(finalColor, alpha)
  })()

  // ─── Build material ────────────────────────────────
  const material = new NodeMaterial()
  material.transparent = true
  material.depthWrite = false
  material.side = THREE.FrontSide
  material.vertexNode = positionNode
  material.fragmentNode = fragmentNode

  // Update wave directions based on elapsed time
  const updateWaveDirections = (elapsed: number) => {
    for (let i = 0; i < waveConfigs.length; i++) {
      const cfg = waveConfigs[i]
      const angle = cfg.angle + elapsed * cfg.speed
      const v = waveUniforms[i].value
      v.x = Math.sin(angle)
      v.y = Math.cos(angle)
    }
  }

  return {
    material,
    updateWaveDirections,
    uniforms: {
      uTime,
      uSunDirection,
      uSunColor,
      uCameraDirection,
      uMoonBrightness,
      uRefractionMap: refractionTex,
      uReflectionMap: reflectionTex,
      uHeightmapTexture: heightmapTex,
    },
  }
}

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
  smoothstep,
  mix,
  clamp,
  pow,
  fract,
  max,
  reflect,
  varying,
  normalize,
  dot,
  positionLocal,
  modelWorldMatrix,
  cameraProjectionMatrix,
  cameraViewMatrix,
} from 'three/tsl'
import { PI, gerstnerWave, gerstnerNormal } from './gerstner'
import { valueNoise, sampleNormalNoise } from './tsl-noise'
import {
  type WaterMaterialOptions,
  type WaterMaterialResult,
  waterFallbackTex,
  waterWetnessFallbackTex,
  waveConfigs,
} from './water-types'

// Re-export public API from water-types
export {
  waterFallbackTex,
  waterWetnessFallbackTex,
  waterHeightFallbackTex,
} from './water-types'
export type {
  WaterMaterialOptions,
  WaterMaterialUniforms,
  WaterMaterialResult,
} from './water-types'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any // TSL node — broad type for internal helper params

// ─── Create Water Material ─────────────────────────────

export function createWaterMaterial(
  options: WaterMaterialOptions
): WaterMaterialResult {
  // ── Uniforms ──
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

  const uVeryShallowColor = uniform(new THREE.Color(0.75, 0.88, 0.78))
  const uShallowColor = uniform(new THREE.Color(0.2, 0.58, 0.42))
  const uMidColor = uniform(new THREE.Color(0.02, 0.34, 0.32))
  const uDeepColor = uniform(new THREE.Color(0.002, 0.06, 0.18))
  const uMaxDepth = uniform(2.5)
  const uSunDirection = uniform(new THREE.Vector3(0.5, 0.8, 0.3).normalize())
  const uSunColor = uniform(new THREE.Color(1.0, 0.95, 0.8))
  const uCameraDirection = uniform(new THREE.Vector3(0, -1, 0))
  const uMoonBrightness = uniform(0)
  const uRefractionStrength = uniform(0.1)

  // ── Texture Nodes ──
  const heightmapTex = texture(options.heightmapTexture)
  const normalMapTex = texture(options.normalMap)
  const foamMapTex = texture(options.foamMap)
  const causticsTex = texture(options.causticsMap)
  const refractionTex = texture(options.refractionMap ?? waterFallbackTex)
  const reflectionTex = texture(options.reflectionMap ?? waterFallbackTex)
  const wetnessMapTex = texture(options.wetnessMap ?? waterWetnessFallbackTex)
  const uCaptureMode = uniform(0)

  // ── Varyings ──
  const vOrigWorldPos = varying(vec3(0), 'v_origWorldPos')
  const vWorldPos = varying(vec3(0), 'v_worldPos')
  const vWaveHeight = varying(float(0), 'v_waveHeight')
  const vClipPos = varying(vec4(0), 'v_clipPos')
  const vUv = varying(vec2(0), 'v_uv')

  // Aligns 65 vertices with 65×65 texel centers
  const toHeightmapUV = (uvCoord: N) => uvCoord.mul(64.0 / 65.0).add(0.5 / 65.0)

  // ── Fragment Helpers ──────────────────────────────────
  // Plain JS functions that build TSL node sub-graphs.
  // Called from within the fragment Fn(), they compose into the shader.

  function buildDepthColor(depthFactor: N) {
    const c1 = mix(
      uVeryShallowColor,
      uShallowColor,
      smoothstep(float(0.0), float(0.08), depthFactor)
    )
    const c2 = mix(
      c1,
      uMidColor,
      smoothstep(float(0.08), float(0.25), depthFactor)
    )
    return mix(c2, uDeepColor, smoothstep(float(0.25), float(0.7), depthFactor))
  }

  function buildSurfaceNormal(worldPos: N) {
    const gnA = gerstnerNormal(uWaveA, worldPos, uTime)
    const gnB = gerstnerNormal(uWaveB, worldPos, uTime)
    const gnC = gerstnerNormal(uWaveC, worldPos, uTime)
    const tx = float(1.0).add(gnA.x).add(gnB.x).add(gnC.x)
    const ty = gnA.y.add(gnB.y).add(gnC.y)
    const bz = float(1.0).add(gnA.z).add(gnB.z).add(gnC.z)
    const by = gnA.w.add(gnB.w).add(gnC.w)
    const gerstnerN = normalize(vec3(ty.negate(), tx.mul(bz), by.negate()))

    const rippleNoise = sampleNormalNoise(
      worldPos.xz,
      normalMapTex,
      uTime,
      uWaveA,
      uWaveB,
      uWaveC
    )
    const rippleN = rippleNoise.xzy.mul(vec3(1.5, 0.0, 1.5))
    return normalize(gerstnerN.add(rippleN))
  }

  function buildRefraction(screenUV: N, worldPos: N, depthFactor: N) {
    const baseUV = vec2(screenUV.x, float(1.0).sub(screenUV.y))
    const refrUV1 = worldPos.xz.mul(0.08).add(uTime.mul(0.015))
    const refrUV2 = worldPos.xz.mul(0.06).sub(uTime.mul(0.01))
    const refrNoise = normalMapTex
      .sample(refrUV1)
      .rg.add(normalMapTex.sample(refrUV2).rg)
      .sub(1.0)
    const distort = refrNoise.mul(uRefractionStrength)
    const finalUV = clamp(baseUV.add(distort), 0.0, 1.0)
    const color = refractionTex.sample(finalUV).rgb
    const mixFactor = float(1)
      .sub(smoothstep(float(0.05), float(0.35), depthFactor))
      .mul(0.95)
    return { color, mixFactor }
  }

  function buildCaustics(worldPos: N, depthFactor: N) {
    const sunY = uSunDirection.y
    const cUV1 = worldPos.xz
      .mul(0.1)
      .add(vec2(uTime.mul(0.015), uTime.mul(0.01)))
    const cUV2 = worldPos.xz
      .mul(0.095)
      .sub(vec2(uTime.mul(0.008), uTime.mul(0.01)))
    const rawCaustics = causticsTex
      .sample(cUV1)
      .r.min(causticsTex.sample(cUV2).r)
    const causticsDetail = foamMapTex.sample(
      worldPos.xz.mul(0.3).add(uTime.mul(0.01))
    ).r
    const causticsPattern = rawCaustics
      .min(float(0.5))
      .div(float(0.5))
      .mul(causticsDetail)

    const shimmer = sin(
      worldPos.x.mul(0.4).add(worldPos.z.mul(0.6)).add(uTime.mul(0.5))
    )
      .mul(0.4)
      .add(0.8)
    const causticsShimmer = causticsPattern.mul(shimmer)
    const causticsStrength = float(1).sub(
      smoothstep(float(0), float(0.5), depthFactor)
    )

    const nightFactor = smoothstep(float(-0.05), float(0.1), sunY)
    const lightColor = mix(vec3(0.08, 0.1, 0.15), vec3(uSunColor), nightFactor)
    const depthGate = smoothstep(float(0.05), float(0.25), depthFactor)
    return lightColor
      .mul(causticsShimmer.mul(1.2))
      .mul(causticsStrength)
      .mul(depthGate)
  }

  function buildSpecular(surfaceNormal: N, viewDir: N, displacedWorldPos: N) {
    const specNormal = normalize(mix(vec3(0, 1, 0), surfaceNormal, 0.3))
    const halfDir = normalize(vec3(uSunDirection).add(viewDir))
    const NdotH = max(dot(specNormal, halfDir), 0.0)
    const specular = vec3(uSunColor)
      .mul(pow(NdotH, float(128)).mul(0.3))
      .toVar()

    // Sun sparkles — ride the displaced wave surface
    const spT = uTime.mul(0.04)
    const sp1 = normalMapTex.sample(
      displacedWorldPos.xz.mul(0.5).add(vec2(spT, spT.mul(0.7)))
    ).r
    const sp2 = normalMapTex.sample(
      displacedWorldPos.xz.mul(0.8).sub(vec2(spT.mul(0.6), spT))
    ).g
    const waveCrestFactor = smoothstep(float(-0.05), float(0.1), vWaveHeight)
      .mul(0.8)
      .add(0.2)
    const sunSparkleStrength = smoothstep(
      float(0),
      float(0.15),
      uSunDirection.y
    ).mul(float(0.3).add(float(0.7).mul(uSunDirection.y)))
    const moonSparkleStrength = float(1)
      .sub(smoothstep(float(-0.05), float(0.05), uSunDirection.y))
      .mul(0.15)
      .mul(smoothstep(float(0), float(0.1), uMoonBrightness))
    const sparkle = smoothstep(float(1.3), float(1.45), sp1.add(sp2))
      .mul(8.0)
      .mul(waveCrestFactor)
      .mul(max(sunSparkleStrength, moonSparkleStrength))
    specular.addAssign(vec3(uSunColor).mul(sparkle))

    return { specular, sparkle }
  }

  function buildSkyReflection(surfaceNormal: N, viewDir: N, screenUV: N) {
    const reflNormal = normalize(mix(vec3(0, 1, 0), surfaceNormal, 0.3))
    const reflectDir = reflect(viewDir.negate(), reflNormal)
    const skyY = clamp(reflectDir.y.mul(0.5).add(0.5), 0.0, 1.0)
    const sunY = uSunDirection.y

    // Time-of-day blend factors
    const nightFactor = float(1).sub(
      smoothstep(float(-0.15), float(0.05), sunY)
    )
    const twilightFactor = smoothstep(float(-0.15), float(0.0), sunY).mul(
      float(1).sub(smoothstep(float(0.05), float(0.3), sunY))
    )
    const dayFactor = smoothstep(float(0.05), float(0.3), sunY)

    // Sky palettes blended by time of day
    const groundColor = vec3(0.02, 0.03, 0.06)
      .mul(nightFactor)
      .add(vec3(0.12, 0.06, 0.04).mul(twilightFactor))
      .add(vec3(0.08, 0.12, 0.15).mul(dayFactor))
    const hazeColorBase = vec3(0.04, 0.06, 0.12)
      .mul(nightFactor)
      .add(vec3(0.7, 0.35, 0.15).mul(twilightFactor))
      .add(vec3(0.55, 0.65, 0.75).mul(dayFactor))
    const zenithColor = vec3(0.02, 0.04, 0.1)
      .mul(nightFactor)
      .add(vec3(0.15, 0.1, 0.25).mul(twilightFactor))
      .add(vec3(0.12, 0.25, 0.5).mul(dayFactor))

    // Sunset tint
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
    const reflUV = vec2(screenUV.x, float(1.0).sub(screenUV.y))
    const reflectionSample = reflectionTex.sample(
      clamp(reflUV.add(surfaceNormal.xz.mul(0.01)), 0.0, 1.0)
    )
    skyReflection.assign(
      mix(skyReflection, reflectionSample.rgb, reflectionSample.a.mul(0.5))
    )

    return { skyReflection, reflectionSample }
  }

  function buildShoreMask(depth: N, worldPos: N, waveSpeed: N) {
    const shorePhase = uTime
      .mul(waveSpeed)
      .mul(PI.mul(4))
      .sub(PI.mul(1.0 / 2.0))
    const shoreRecede = sin(shorePhase).mul(0.5).add(0.5)
    const shoreAdjustedDepth = max(float(0), depth.sub(shoreRecede.mul(0.35)))
    const shoreZone = float(1).sub(
      smoothstep(float(0), float(0.45), shoreAdjustedDepth)
    )

    const sn1 = valueNoise(worldPos.xz.mul(0.2).add(uTime.mul(0.07)))
    const sn2 = valueNoise(worldPos.xz.mul(0.4).add(uTime.mul(0.04)))
    const sn3 = valueNoise(worldPos.xz.mul(0.08).add(uTime.mul(0.1)))
    const holeMask = sn1.mul(0.5).add(sn2.mul(0.3)).add(sn3.mul(0.2))

    const edgeCutoff = smoothstep(float(0), float(0.01), depth)
    const holeThreshold = shoreZone.mul(0.9)
    const holeAlpha = smoothstep(
      holeThreshold.sub(0.05),
      holeThreshold.add(0.05),
      holeMask
    ).mul(edgeCutoff)

    const distFromHole = holeMask.sub(holeThreshold)
    const holeEdge = smoothstep(float(-0.03), float(0.01), distFromHole)
      .mul(float(1).sub(smoothstep(float(0.01), float(0.5), distFromHole)))
      .mul(shoreZone)
    const holeFoamFringe = smoothstep(
      float(-0.03),
      float(0.0),
      distFromHole
    ).mul(shoreZone)

    return { holeAlpha, holeFoamFringe, holeEdge, shoreZone }
  }

  function buildFoam(
    depth: N,
    depthFactor: N,
    worldPos: N,
    move1: N,
    move2: N,
    cycle1: N,
    cycle2: N,
    holeEdge: N,
    sunY: N
  ) {
    // Noise-perturbed depth for irregular edges
    const noisyD = depth
      .add(valueNoise(worldPos.xz.mul(0.3)).mul(0.15))
      .add(valueNoise(worldPos.xz.mul(0.15)).mul(0.1))
      .add(valueNoise(worldPos.xz.mul(0.2)).mul(0.3))

    // Wave bands move from deeper water toward shore
    const spawnDepth = float(1.5)
    const shoreDepth = float(0.15)
    const center1 = mix(spawnDepth, shoreDepth, move1)
    const center2 = mix(spawnDepth, shoreDepth, move2)

    const fade1 = smoothstep(float(0), float(0.1), cycle1).mul(
      float(1).sub(smoothstep(float(0.9), float(1), cycle1))
    )
    const fade2 = smoothstep(float(0), float(0.1), cycle2).mul(
      float(1).sub(smoothstep(float(0.9), float(1), cycle2))
    )

    // Bands widen near shore (wave shoaling)
    const bw1 = float(0.04).add(float(0.1).mul(move1))
    const bw2 = float(0.04).add(float(0.1).mul(move2))

    const band1 = smoothstep(center1.sub(bw1), center1, noisyD)
      .mul(float(1).sub(smoothstep(center1, center1.add(bw1), noisyD)))
      .mul(fade1)
      .toVar()
    const band2 = smoothstep(center2.sub(bw2), center2, noisyD)
      .mul(float(1).sub(smoothstep(center2, center2.add(bw2), noisyD)))
      .mul(fade2)
      .toVar()

    // Break up with large-scale noise for organic edges
    band1.mulAssign(
      smoothstep(
        float(0.2),
        float(0.5),
        valueNoise(worldPos.xz.mul(0.15).add(center1.mul(1.5)))
      )
    )
    band2.mulAssign(
      smoothstep(
        float(0.2),
        float(0.5),
        valueNoise(worldPos.xz.mul(0.15).add(center2.mul(1.5)))
      )
    )

    // Shore foam at hole edges
    const shoreDayNight = smoothstep(float(-0.05), float(0.1), sunY)
    const shoreBase = holeEdge.mul(mix(float(0.5), float(1.4), shoreDayNight))
    const foamGlow = float(1)
      .sub(smoothstep(float(0), float(0.4), depth))
      .mul(0.15)

    // Foam texture sampling
    const foamTex1 = foamMapTex.sample(
      worldPos.xz.mul(0.4).add(cycle1.mul(0.3))
    ).r
    const foamTex2 = foamMapTex.sample(
      worldPos.xz.mul(0.4).add(cycle2.mul(0.3))
    ).r

    // Shore foam texture (two layers)
    const shoreFoamTex = max(
      foamMapTex.sample(
        worldPos.xz.mul(0.5).add(vec2(uTime.mul(0.006), uTime.mul(0.004)))
      ).r,
      foamMapTex.sample(
        worldPos.xz.mul(0.35).sub(vec2(uTime.mul(0.003), uTime.mul(0.005)))
      ).r
    )
    const shoreBaseTex = shoreBase.mul(shoreFoamTex)

    // Combine foam
    const waveFoam = max(band1.mul(foamTex1), band2.mul(foamTex2))
    const foamWithTex = clamp(
      max(max(waveFoam, shoreBaseTex), foamGlow),
      0.0,
      1.0
    )

    // Day/night foam strength
    const foamDayNight = smoothstep(float(-0.05), float(0.1), sunY)
    const foamDepthMask = float(1)
      .sub(smoothstep(float(0.3), float(0.7), depthFactor))
      .mul(0.7)
      .add(0.3)
    const foamAddStrength = mix(float(0.06), float(0.7), foamDayNight)
    const foamAdd = vec3(1, 1, 1).mul(
      foamWithTex.mul(foamAddStrength).mul(foamDepthMask)
    )

    return { foamWithTex, shoreBaseTex, foamAdd }
  }

  function buildWetSand(refractionColor: N, holeAlpha: N, holeFoamFringe: N) {
    const texelSize = float(1.0 / 256) // WETNESS_SIZE
    const rawWetness = wetnessMapTex
      .sample(vUv.add(vec2(texelSize, 0)))
      .r.add(wetnessMapTex.sample(vUv.add(vec2(texelSize.negate(), 0))).r)
      .add(wetnessMapTex.sample(vUv.add(vec2(0, texelSize))).r)
      .add(wetnessMapTex.sample(vUv.add(vec2(0, texelSize.negate()))).r)
      .mul(0.25)
    const wetness = smoothstep(float(0.2), float(0.7), rawWetness)
    const inHoleFactor = float(1).sub(holeAlpha)
    const wetMask = wetness.mul(inHoleFactor)
    const wetBlend = smoothstep(float(0.05), float(0.3), wetMask)
    const wetDarken = mix(float(0.85), float(0.35), wetness)
    const wetTerrainColor = refractionColor.mul(wetDarken)
    const colorWetBlend = wetBlend.mul(float(1).sub(holeFoamFringe))
    return { wetBlend, wetTerrainColor, colorWetBlend }
  }

  // ── Vertex Shader ─────────────────────────────────────

  const positionNode = Fn(() => {
    const localPos = positionLocal.toVar()
    vUv.assign(uv())

    const worldPos = modelWorldMatrix.mul(vec4(localPos, 1.0)).toVar()
    vOrigWorldPos.assign(worldPos.xyz)

    const p = worldPos.xyz
    const vtxTerrainH = heightmapTex.sample(toHeightmapUV(vUv)).r
    const vtxDepth = max(float(0), p.y.sub(vtxTerrainH))
    const waveDamping = smoothstep(float(0.0), float(1.5), vtxDepth)

    const gerstnerOffset = gerstnerWave(uWaveA, p, uTime)
      .add(gerstnerWave(uWaveB, p, uTime))
      .add(gerstnerWave(uWaveC, p, uTime))
      .mul(waveDamping)
      .toVar()

    // Skip Gerstner displacement in capture mode so UV↔screen mapping stays exact
    const offset = mix(gerstnerOffset, vec3(0), uCaptureMode).toVar()

    worldPos.xyz.addAssign(offset)
    vWaveHeight.assign(offset.y)
    vWorldPos.assign(worldPos.xyz)

    const clipPos = cameraProjectionMatrix.mul(cameraViewMatrix).mul(worldPos)
    vClipPos.assign(clipPos)

    return clipPos
  })()

  // ── Fragment Shader ───────────────────────────────────

  const fragmentNode = Fn(() => {
    // Depth
    const terrainHeight = heightmapTex.sample(toHeightmapUV(vUv)).r
    const depth = max(float(0), vOrigWorldPos.y.sub(terrainHeight))
    const depthFactor = clamp(depth.div(uMaxDepth), 0.0, 1.0)
    const sunY = uSunDirection.y

    // Base water color (4-stop depth gradient)
    const waterColor = buildDepthColor(depthFactor).toVar()

    // Surface normal (Gerstner analytical + ripple detail)
    const surfaceNormal = buildSurfaceNormal(vOrigWorldPos)

    // View direction
    const viewDir = normalize(vec3(uCameraDirection).negate())

    // Screen UV from clip position
    const screenUV = vClipPos.xy.mul(0.5).add(0.5)

    // Refraction
    const refraction = buildRefraction(screenUV, vOrigWorldPos, depthFactor)

    // Darken water color at night before mixing
    const waterNightFactor = smoothstep(float(-0.05), float(0.1), sunY)
      .mul(0.85)
      .add(0.15)
    waterColor.mulAssign(waterNightFactor)

    // Blend refraction into water color
    waterColor.assign(mix(waterColor, refraction.color, refraction.mixFactor))

    // Underwater caustics
    waterColor.addAssign(buildCaustics(vOrigWorldPos, depthFactor))

    // Specular highlights + sun sparkles
    const { specular, sparkle } = buildSpecular(
      surfaceNormal,
      viewDir,
      vWorldPos
    )

    // Sky + entity reflection
    const { skyReflection, reflectionSample } = buildSkyReflection(
      surfaceNormal,
      viewDir,
      screenUV
    )

    // Wave timing (shared by shore mask + foam)
    const waveSpeed = float(0.012)
    const cycle1 = fract(uTime.mul(waveSpeed))
    const cycle2 = fract(uTime.mul(waveSpeed).add(0.5))
    const move1 = smoothstep(float(0), float(0.7), cycle1)
    const move2 = smoothstep(float(0), float(0.7), cycle2)

    // Shore mask (hole alpha, foam fringe)
    const shore = buildShoreMask(depth, vOrigWorldPos, waveSpeed)

    // Foam
    const foam = buildFoam(
      depth,
      depthFactor,
      vOrigWorldPos,
      move1,
      move2,
      cycle1,
      cycle2,
      shore.holeEdge,
      sunY
    )

    // Fresnel + ripple brightness
    const fresnelViewDir = normalize(vec3(viewDir.x, float(0.15), viewDir.z))
    const NdotV = max(dot(surfaceNormal, fresnelViewDir), 0.0)
    const rippleBright = mix(
      float(0.75),
      float(1.25),
      pow(float(1).sub(NdotV), float(1.5))
    )
    waterColor.mulAssign(rippleBright)

    // Tinted sky reflection + Fresnel composition
    const tintedSkyReflection = mix(
      skyReflection,
      vec3(uMidColor).mul(1.3),
      float(0.7)
    )
    const shallowDamp = smoothstep(float(0.1), float(0.4), depthFactor)
    const fresnel = pow(float(1).sub(NdotV), float(2)).mul(0.08)
    const color = mix(
      waterColor,
      tintedSkyReflection,
      mix(float(0.005), float(0.06), shallowDamp).add(fresnel)
    )
      .add(specular.mul(shallowDamp))
      .toVar()

    // Entity reflection overlay
    color.assign(mix(color, reflectionSample.rgb, reflectionSample.a.mul(0.3)))

    // Night darkening
    const nightDarken = smoothstep(float(-0.05), float(0.1), sunY)
      .mul(0.75)
      .add(0.25)
    const midDepthWeight = smoothstep(
      float(0.15),
      float(0.35),
      depthFactor
    ).mul(float(1).sub(smoothstep(float(0.5), float(0.8), depthFactor)))
    const nightExtra = float(1).sub(
      float(1).sub(nightDarken).mul(midDepthWeight).mul(0.35)
    )
    color.mulAssign(nightDarken.mul(nightExtra))

    // Additive foam (after night darkening so foam white isn't darkened)
    color.addAssign(foam.foamAdd)
    color.addAssign(vec3(1, 1, 1).mul(foam.shoreBaseTex.mul(0.4)))

    // Alpha
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
    const refrAlphaBoost = refraction.mixFactor.mul(0.9)
    const alpha = max(baseAlpha, refrAlphaBoost)
      .add(foam.foamWithTex.mul(0.9))
      .add(sparkle)
      .min(1.0)
      .toVar()

    // Shore edge
    alpha.mulAssign(max(shore.holeAlpha, shore.holeFoamFringe))

    // Wet sand
    const wet = buildWetSand(
      refraction.color,
      shore.holeAlpha,
      shore.holeFoamFringe
    )
    color.assign(mix(color, wet.wetTerrainColor, wet.colorWetBlend))
    alpha.assign(max(alpha, wet.wetBlend.mul(0.6)))

    // Night alpha reduction for very shallow water
    const veryShallowWeight = float(1).sub(
      smoothstep(float(0.0), float(0.08), depthFactor)
    )
    const nightAlphaReduce = float(1).sub(
      float(1).sub(nightDarken).mul(veryShallowWeight).mul(0.5)
    )
    alpha.mulAssign(nightAlphaReduce)

    // Capture mode outputs only holeAlpha
    const outputAlpha = mix(alpha, shore.holeAlpha, uCaptureMode)
    return vec4(color, outputAlpha)
  })()

  // ── Build Material ────────────────────────────────────

  const material = new NodeMaterial()
  material.transparent = true
  material.depthWrite = false
  material.side = THREE.FrontSide
  material.vertexNode = positionNode
  material.fragmentNode = fragmentNode

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
      uNormalMap: normalMapTex,
      uFoamMap: foamMapTex,
      uCausticsMap: causticsTex,
      uWetnessMap: wetnessMapTex,
      uCaptureMode,
      uWaveA,
      uWaveB,
      uWaveC,
    },
  }
}

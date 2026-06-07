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
  smoothstep,
  mix,
  clamp,
  pow,
  max,
  length,
  reflect,
  varying,
  normalize,
  dot,
  positionLocal,
  modelWorldMatrix,
  cameraProjectionMatrix,
  cameraViewMatrix,
  fract,
  abs,
} from 'three/tsl'
import {
  waterFallbackTex,
  getCloudTexture,
  sampleCloudPhoto,
  toHeightmapUV,
} from './water-types'
import { SEA_LEVEL } from '../components/game-scene/terrain-utils'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any // TSL node — broad type for internal helper params

/**
 * Per-tile river shader: 65×65 quad with vertex Y pre-baked to the
 * river surface, fragment alpha + color derived from `depth =
 * vWorldPos.y − heightmapBed`, ripple normal scrolled along flow
 * vector sampled from the river-field texture.
 */

export interface RiverFieldMaterialOptions {
  normalMap: THREE.Texture
  heightmapTexture: THREE.Texture
  riverField: THREE.Texture
  reflectionMap?: THREE.Texture | null
  refractionMap?: THREE.Texture | null
}

export interface RiverFieldMaterialUniforms {
  uTime: { value: number }
  uSunDirection: { value: THREE.Vector3 }
  uSunColor: { value: THREE.Color }
  uCameraDirection: { value: THREE.Vector3 }
  uMoonBrightness: { value: number }
  uTorchPos: { value: THREE.Vector3 }
  uTorchColor: { value: THREE.Color }
  uTorchIntensity: { value: number }
  uTorchDistance: { value: number }
  uReflectionMap: { value: THREE.Texture }
  uRefractionMap: { value: THREE.Texture }
  uNormalMap: { value: THREE.Texture }
  uHeightmapTexture: { value: THREE.Texture }
  uRiverField: { value: THREE.Texture }
  uSeaFadeBottom: { value: number }
  uSeaFadeTop: { value: number }
}

export interface RiverFieldMaterialResult {
  material: NodeMaterial
  uniforms: RiverFieldMaterialUniforms
}

export function createRiverFieldMaterial(
  options: RiverFieldMaterialOptions
): RiverFieldMaterialResult {
  // ── Uniforms ──
  const uTime = uniform(0)
  const uSunDirection = uniform(new THREE.Vector3(0.5, 0.8, 0.3).normalize())
  const uSunColor = uniform(new THREE.Color(1.0, 0.95, 0.8))
  const uCameraDirection = uniform(new THREE.Vector3(0, -1, 0))
  const uMoonBrightness = uniform(0)
  const uTorchPos = uniform(new THREE.Vector3(0, -1000, 0))
  const uTorchColor = uniform(new THREE.Color(1.0, 0.8, 0.4))
  const uTorchIntensity = uniform(0)
  const uTorchDistance = uniform(50)

  const uShallowColor = uniform(new THREE.Color(0.18, 0.32, 0.32))
  const uMidColor = uniform(new THREE.Color(0.04, 0.12, 0.18))
  const uDeepColor = uniform(new THREE.Color(0.02, 0.05, 0.12))

  /** Surface depth at full opacity (m). Past this, alpha plateaus at the
   *  body opacity. Matches the bake's `RIVER_DEPTH_OFFSET_M = 0.5` so the
   *  channel center hits full body just before the offset cap. */
  const uMaxDepth = uniform(0.5)
  const uRefractionStrength = uniform(0.04)

  /** Fade band on the local bed height. The bake pins `surfaceY` to
   *  `bed_at_proj + 0.5` at the polyline endpoint so it stays ~0.5 m even
   *  beyond the coast, hence the gate must read `bedHeight`. Defaults
   *  keep the river opaque to −0.6 m (river carve floor is 0 m) and fully
   *  transparent below −1.5 m. */
  const uSeaFadeTop = uniform(SEA_LEVEL - 0.6)
  const uSeaFadeBottom = uniform(SEA_LEVEL - 1.5)

  // ── Textures ──
  const heightmapTex = texture(options.heightmapTexture)
  const riverFieldTex = texture(options.riverField)
  const normalMapTex = texture(options.normalMap)
  const reflectionTex = texture(options.reflectionMap ?? waterFallbackTex)
  const refractionTex = texture(options.refractionMap ?? waterFallbackTex)
  const reflectionMixScale = float(options.reflectionMap ? 1 : 0)
  const refractionMixScale = float(options.refractionMap ? 1 : 0)
  const cloudTex = texture(getCloudTexture())

  // ── Varyings ──
  const vWorldPos = varying(vec3(0), 'rf_worldPos')
  const vClipPos = varying(vec4(0), 'rf_clipPos')

  // ── Vertex ──
  const positionNode = Fn(() => {
    const localPos = vec4(positionLocal, 1.0)
    const worldPos = modelWorldMatrix.mul(localPos).toVar()
    vWorldPos.assign(worldPos.xyz)
    const clipPos = cameraProjectionMatrix.mul(cameraViewMatrix).mul(worldPos)
    vClipPos.assign(clipPos)
    return clipPos
  })()

  // ── Fragment ──
  const fragmentNode = Fn(() => {
    const sunY = uSunDirection.y
    // Rec. 601 luma weights, reused for every desaturation below.
    const LUMA_REC601 = vec3(0.299, 0.587, 0.114)

    // The quad is a tile-sized PlaneGeometry → its built-in vUv already
    // covers [0,1] across the heightmap-aligned 65×65 textures. Half-
    // texel inset lands samples on texel centers (matches sea shader).
    const sampleUV = clamp(toHeightmapUV(uv()), 0.0, 1.0)

    const bedHeight = heightmapTex.sample(sampleUV).r
    const surfaceY = vWorldPos.y
    const depth = max(float(0), surfaceY.sub(bedHeight)).toVar()
    const depthFactor = clamp(depth.div(uMaxDepth), 0.0, 1.0)

    // ── Base color: 3-stop depth gradient (sea-style). ──
    const c1 = mix(
      uShallowColor,
      uMidColor,
      smoothstep(float(0.0), float(0.4), depthFactor)
    )
    const waterColor = mix(
      c1,
      uDeepColor,
      smoothstep(float(0.4), float(0.85), depthFactor)
    ).toVar()

    // GB = downstream flow direction (unit) from the bake; bilinear
    // filtering blends at confluences without a per-fragment branch.
    // Magnitude is scaled below by bed proximity to sea so the river
    // decelerates into the mouth — scaling the vector instead of the
    // time phase keeps neighbouring fragments phase-coherent.
    const flowSpeed = mix(
      float(0.3),
      float(1.0),
      smoothstep(float(SEA_LEVEL), float(SEA_LEVEL + 1.5), bedHeight)
    )
    const flow = riverFieldTex.sample(sampleUV).gb.mul(flowSpeed)

    // Ripple normal: world-XZ aligned UVs scrolled along flow.
    // Two-phase flowmap: `flow × uTime` would grow unboundedly, causing
    // adjacent pixels with slightly different flow (Voronoi boundaries,
    // confluences) to decorrelate in texture space and develop a vortex
    // artifact. Wrap each phase in [0, 1] and crossfade two half-period-
    // offset phases so the wrap is invisible.
    const NORMAL_SCALE = float(0.18)
    const buildWrappedDrift = (rate: N, flow: N) => {
      const phase = uTime.mul(rate)
      const pA = fract(phase)
      const pB = fract(phase.add(0.5))
      const mixW = abs(pA.sub(0.5)).mul(2.0)
      return { driftA: flow.mul(pA), driftB: flow.mul(pB), mixW }
    }
    const {
      driftA: flowOffA,
      driftB: flowOffB,
      mixW: rippleMix,
    } = buildWrappedDrift(float(0.4), flow)
    const nBase1 = vWorldPos.xz.mul(NORMAL_SCALE)
    const nBase2 = vWorldPos.xz.mul(NORMAL_SCALE.mul(0.6)).add(vec2(0.3, 0))
    // `flowScale2` attenuates flow drift on the finer-scale second sample
    // so the two scales don't move in lockstep.
    const buildRippleN = (
      a: N,
      b: N,
      offA: N,
      offB: N,
      flowScale2: N,
      mixW: N
    ): N => {
      const sA = normalMapTex
        .sample(a.sub(offA))
        .add(normalMapTex.sample(b.sub(offA.mul(flowScale2))))
        .mul(0.5)
        .sub(1.0)
      const sB = normalMapTex
        .sample(a.sub(offB))
        .add(normalMapTex.sample(b.sub(offB.mul(flowScale2))))
        .mul(0.5)
        .sub(1.0)
      const s = mix(sA, sB, mixW)
      return normalize(vec3(s.r.mul(1.2), float(1.0), s.g.mul(1.2)))
    }
    const rippleN = buildRippleN(
      nBase1,
      nBase2,
      flowOffA,
      flowOffB,
      float(0.7),
      rippleMix
    )

    // ── View / screen ──
    const viewDir = normalize(vec3(uCameraDirection).negate())
    const screenUV = vClipPos.xy.mul(0.5).add(0.5)
    const screenUVFlipped = vec2(screenUV.x, float(1.0).sub(screenUV.y))

    // ── Refraction (shallow water shows the bed) ──
    const refrDistort = rippleN.xz.mul(uRefractionStrength)
    const refrUV = clamp(screenUVFlipped.add(refrDistort), 0.0, 1.0)
    const tintedRefr = refractionTex.sample(refrUV).rgb
    // Time-of-day weight reused for sky/cloud/body grading further down.
    const nightFactor = float(1)
      .sub(smoothstep(float(-0.15), float(0.05), sunY))
      .toVar()
    const refrLuma = dot(tintedRefr, LUMA_REC601)
    const nightRefr = mix(tintedRefr, vec3(refrLuma), nightFactor.mul(0.9)).mul(
      float(1).sub(nightFactor.mul(0.88))
    )
    const refrColor = mix(tintedRefr, nightRefr, nightFactor)
    // Shallow-water mix peaks where depth is low (near banks), fades
    // off in deep water so the body color dominates mid-channel.
    const refrShallow = float(1)
      .sub(smoothstep(float(0.05), float(0.5), depthFactor))
      .toVar()
    const refrMix = refrShallow.mul(0.85).mul(refractionMixScale).toVar()
    waterColor.assign(mix(waterColor, refrColor, refrMix))

    // ── Sky reflection (condensed sea pattern) ──
    // `reflT` is a uniform vertical scroll (no per-pixel flow term)
    // so it stays as `uTime × rate` without flowmap wrapping. The flow-
    // aligned drift reuses `buildWrappedDrift` to keep its phase bounded.
    const WOBBLE_SHAKE_RATE = float(0.05)
    const WOBBLE_DRIFT_RATE = float(0.05)
    const reflT = uTime.mul(WOBBLE_SHAKE_RATE)
    const {
      driftA: reflDriftA,
      driftB: reflDriftB,
      mixW: reflMix,
    } = buildWrappedDrift(WOBBLE_DRIFT_RATE, flow)
    const reflBase1 = vWorldPos.xz.mul(NORMAL_SCALE).sub(vec2(0, reflT))
    const reflBase2 = vWorldPos.xz
      .mul(NORMAL_SCALE.mul(0.7))
      .add(vec2(0.4, 0))
      .add(vec2(0, reflT.mul(0.9)))
    const reflRippleN = buildRippleN(
      reflBase1,
      reflBase2,
      reflDriftA,
      reflDriftB,
      float(1.0),
      reflMix
    )
    const reflNormal = normalize(mix(vec3(0, 1, 0), reflRippleN, 0.05))
    const reflectDir = reflect(viewDir.negate(), reflNormal)
    const skyY = clamp(reflectDir.y.mul(0.5).add(0.5), 0.0, 1.0)

    const twilightFactor = smoothstep(float(-0.15), float(0.0), sunY).mul(
      float(1).sub(smoothstep(float(0.05), float(0.3), sunY))
    )
    const dayFactor = smoothstep(float(0.05), float(0.3), sunY)

    const hazeColorBase = vec3(0.021, 0.026, 0.035)
      .mul(nightFactor)
      .add(vec3(0.7, 0.35, 0.15).mul(twilightFactor))
      .add(vec3(0.45, 0.62, 0.82).mul(dayFactor))
    const zenithColor = vec3(0.015, 0.019, 0.03)
      .mul(nightFactor)
      .add(vec3(0.15, 0.1, 0.25).mul(twilightFactor))
      .add(vec3(0.12, 0.35, 0.8).mul(dayFactor))
    const groundColor = vec3(0.012, 0.015, 0.022)
      .mul(nightFactor)
      .add(vec3(0.12, 0.06, 0.04).mul(twilightFactor))
      .add(vec3(0.08, 0.12, 0.15).mul(dayFactor))

    const sunsetFactor = smoothstep(float(-0.05), float(0.0), sunY).mul(
      float(1).sub(smoothstep(float(0.0), float(0.3), sunY))
    )
    const hazeColor = mix(
      hazeColorBase,
      uSunColor.rgb.mul(0.6),
      sunsetFactor.mul(0.5)
    )
    const skyReflection = mix(
      mix(groundColor, hazeColor, smoothstep(float(0), float(0.35), skyY)),
      zenithColor,
      smoothstep(float(0.35), float(0.7), skyY)
    ).toVar()

    const sunDot = max(dot(reflectDir, vec3(uSunDirection)), 0.0)
    skyReflection.addAssign(uSunColor.rgb.mul(pow(sunDot, float(8)).mul(0.25)))

    const { cloudColor, cloudWeight } = sampleCloudPhoto(
      reflectDir,
      vWorldPos.xz,
      uTime,
      dayFactor,
      cloudTex
    )
    skyReflection.assign(
      mix(
        skyReflection,
        cloudColor,
        cloudWeight.mul(mix(float(0.42), float(0.95), reflectionMixScale))
      )
    )

    const cloudLuma = dot(cloudColor, LUMA_REC601)
    const cloudHorizonWeight = smoothstep(float(0.15), float(0.45), skyY)
    const sunsetCloudColor = mix(cloudColor, vec3(cloudLuma), 0.52)
      .mul(vec3(0.62, 0.18, 0.075))
      .mul(0.12)
      .add(vec3(0.006, 0.003, 0.0015).mul(sunsetFactor))
    const sunsetCloudWeight = cloudHorizonWeight.mul(twilightFactor).mul(0.85)
    skyReflection.assign(
      mix(skyReflection, sunsetCloudColor, sunsetCloudWeight)
    )

    const nightCloudColor = mix(cloudColor, vec3(cloudLuma), 0.7)
      .mul(0.08)
      .add(vec3(0.004, 0.006, 0.01))
    const nightCloudWeight = cloudHorizonWeight.mul(nightFactor).mul(0.85)
    skyReflection.assign(mix(skyReflection, nightCloudColor, nightCloudWeight))

    const reflectionSample = reflectionTex.sample(
      clamp(screenUVFlipped.add(rippleN.xz.mul(0.01)), 0.0, 1.0)
    )
    skyReflection.assign(
      mix(
        skyReflection,
        reflectionSample.rgb,
        reflectionSample.a.mul(0.5).mul(reflectionMixScale)
      )
    )

    // Torch remains reflection-only: its flickering PointLight intensity
    // creates a warm glint without changing river body color, refraction,
    // or alpha. Time-of-day still lives in the sky/cloud reflection above.
    const torchVec = uTorchPos.sub(vWorldPos)
    const torchLen = length(torchVec)
    const torchAtten = pow(
      max(float(0), float(1).sub(torchLen.div(uTorchDistance))),
      float(2)
    )
    const torchDir = torchVec.div(max(torchLen, float(0.001)))
    const torchSpecNormal = normalize(mix(vec3(0, 1, 0), rippleN, 0.75))
    const torchHalfDir = normalize(torchDir.add(viewDir))
    const torchNdotH = max(dot(torchSpecNormal, torchHalfDir), 0.0)
    const torchTightAtten = torchAtten.mul(torchAtten)
    const torchReflection = uTorchColor.rgb
      .mul(pow(torchNdotH, float(48)))
      .mul(torchTightAtten)
      .mul(uTorchIntensity)
      .mul(0.004)
      .mul(depthFactor)
      .toVar()

    // ── Specular ──
    const specNormal = normalize(mix(vec3(0, 1, 0), rippleN, 0.3))
    const halfDir = normalize(vec3(uSunDirection).add(viewDir))
    const NdotH = max(dot(specNormal, halfDir), 0.0)
    const specular = uSunColor.rgb.mul(pow(NdotH, float(128)).mul(0.35)).toVar()

    // Surface sparkle uses the same daylight path regardless of time of day;
    // only the sky/cloud reflection above carries the day/night/twilight tint.
    const sparkleT = uTime.mul(0.05)
    const sparkleUV1 = vWorldPos.xz
      .mul(NORMAL_SCALE.mul(2.5))
      .sub(flow.mul(sparkleT))
    const sparkleUV2 = vWorldPos.xz
      .mul(NORMAL_SCALE.mul(4.0))
      .add(flow.mul(sparkleT.mul(0.6)))
    const sp1 = normalMapTex.sample(sparkleUV1).r
    const sp2 = normalMapTex.sample(sparkleUV2).g
    const sparkle = smoothstep(float(1.35), float(1.5), sp1.add(sp2))
      .mul(3.0)
      .mul(depthFactor)
    specular.addAssign(uSunColor.rgb.mul(sparkle))

    // ── Fresnel + final composite ──
    const fresnelViewDir = normalize(vec3(viewDir.x, float(0.15), viewDir.z))
    const NdotV = max(dot(rippleN, fresnelViewDir), 0.0)
    const rippleBright = mix(
      float(0.85),
      float(1.2),
      pow(float(1).sub(NdotV), float(1.5))
    )
    waterColor.mulAssign(rippleBright)

    const fresnel = pow(float(1).sub(NdotV), float(2)).mul(0.5)
    // Sky reflection eases off where refraction takes over so the
    // tinted bed reads through shallow water without being washed by
    // sky tint (matches the sea's weighting at shore).
    const reflectionBase = mix(float(0.35), float(0.05), refrShallow.mul(0.9))
    // Deep-water reflection gets lifted at dusk/night; both share this ramp.
    const depthReflLift = smoothstep(
      float(0.04),
      float(0.35),
      depthFactor
    ).toVar()
    const twilightReflectionLift = depthReflLift.mul(twilightFactor).mul(0.18)
    const nightReflectionLift = depthReflLift.mul(nightFactor).mul(0.28)
    const reflectionMix = clamp(
      reflectionBase
        .add(fresnel)
        .add(twilightReflectionLift)
        .add(nightReflectionLift)
        .mul(mix(float(0.58), float(1.0), reflectionMixScale)),
      0.0,
      0.9
    )
    const color = mix(waterColor, skyReflection, reflectionMix)
      .add(specular.mul(depthFactor))
      .toVar()

    color.assign(
      mix(
        color,
        reflectionSample.rgb,
        reflectionSample.a.mul(0.3).mul(reflectionMixScale)
      )
    )

    // Night grade only mutes the final river body; torch reflection is added
    // afterward so flicker stays warm and visible.
    const nightLuma = dot(color, LUMA_REC601)
    const nightMutedColor = mix(
      color,
      vec3(nightLuma),
      nightFactor.mul(0.28)
    ).mul(float(1).sub(nightFactor.mul(0.14)))
    color.assign(mix(color, nightMutedColor, nightFactor))
    color.addAssign(torchReflection)

    // 5 cm hard edge anchors the visible bank exactly at the carve
    // boundary; body alpha ramps to 0.95 over the next `uMaxDepth − 0.05 m`.
    const depthEdgeCut = smoothstep(float(0), float(0.05), depth)
    const bodyAlpha = mix(
      float(0.005),
      float(0.95),
      smoothstep(float(0.05), uMaxDepth, depth)
    ).toVar()
    // Refraction (the in-shader bed blend) is off in deep water, so the
    // bed that shows there at night comes from the body alpha never quite
    // reaching 1 (~0.95) — a torch-lit bed bleeds through the last few
    // percent of framebuffer transparency. Push deep water to fully
    // opaque at night/twilight (`1 − dayFactor`), scaled by depth so
    // shallow banks (low alpha, where the soft edge lives) are untouched.
    const bedHideByNight = float(1).sub(dayFactor)
    bodyAlpha.assign(
      mix(bodyAlpha, float(1.0), bedHideByNight.mul(depthFactor))
    )
    const seaFade = smoothstep(uSeaFadeBottom, uSeaFadeTop, bedHeight)
    const alpha = float(0.95).mul(depthEdgeCut).mul(bodyAlpha).mul(seaFade)

    return vec4(color, alpha)
  })()

  // ── Build ──
  const material = new NodeMaterial()
  material.transparent = true
  material.depthWrite = false
  material.side = THREE.DoubleSide
  material.vertexNode = positionNode
  material.fragmentNode = fragmentNode

  return {
    material,
    uniforms: {
      uTime,
      uSunDirection,
      uSunColor,
      uCameraDirection,
      uMoonBrightness,
      uTorchPos,
      uTorchColor,
      uTorchIntensity,
      uTorchDistance,
      uReflectionMap: reflectionTex,
      uRefractionMap: refractionTex,
      uNormalMap: normalMapTex,
      uHeightmapTexture: heightmapTex,
      uRiverField: riverFieldTex,
      uSeaFadeBottom,
      uSeaFadeTop,
    },
  }
}

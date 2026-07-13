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
  floor,
  fract,
  max,
  min,
  abs,
  length,
  exp,
  reflect,
  varying,
  normalize,
  dot,
  positionLocal,
  modelWorldMatrix,
  cameraProjectionMatrix,
  cameraViewMatrix,
  cameraNear,
  cameraFar,
  viewportLinearDepth,
} from 'three/tsl'
import { PI, gerstnerWave, gerstnerNormal } from './gerstner'
import {
  SHORE_WAVE_SPEED,
  WASH_RUNUP_START,
  WASH_RUNUP_END,
  BORE_FADE_END,
  WASH_FLUSH_START,
  WASH_FLUSH_END,
  MOVE_END,
  BRK_START_MOVE,
  BRK_END_MOVE,
  RECEDE_START,
  RECEDE_END,
} from './shore-wave-timing'
import { sampleNormalNoise } from './tsl-noise'
import {
  waterFallbackTex,
  waterWetnessFallbackTex,
  waterHeightFallbackTex,
  waveConfigs,
  getCloudTexture,
  sampleCloudPhoto,
  toHeightmapUV,
} from './water-types'
import { waterFieldFallbackTex } from '../utils/water-quad-geometry'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any // TSL node — broad type for internal helper params

/**
 * Unified water shader: one mesh + one material per tile renders the sea,
 * rivers, and the estuary between them. The mesh's vertex Y carries the
 * baked water-field surface (flat `SEA_LEVEL` for sea-only tiles); the
 * field texture's `riverness` channel cross-fades sea behavior (Gerstner
 * waves, foam bands, caustics, wet sand) against river behavior (flowmap
 * ripples, river palette, bank alpha ramp).
 *
 * Shoreline (with `pixelDepth`): instead of fading shallow water to
 * near-transparent (the old sea shader's approach — it made the water
 * plane read as a floating sheet when the player stood in it), the
 * intersection with terrain/entities is resolved per-pixel against the
 * opaque depth buffer: a ~6 cm soft blend at the contact line, a foam
 * edge band above it, and Beer-Lambert absorption of the refracted bed.
 * Entities standing in water get the same treatment for free — they're
 * in the opaque depth. Without `pixelDepth` (MSAA presets, where the
 * depth copy is unavailable) a compressed heightmap-depth alpha ramp
 * stands in.
 */

export interface WaterFieldMaterialOptions {
  heightmapTexture: THREE.Texture
  waterField: THREE.Texture
  normalMap: THREE.Texture
  foamMap: THREE.Texture
  causticsMap: THREE.Texture
  refractionMap?: THREE.Texture | null
  reflectionMap?: THREE.Texture | null
  wetnessMap?: THREE.Texture | null
  /** Per-pixel depth-buffer shoreline. Must be false when the canvas is
   *  multisampled (antialias preset) — the framebuffer depth copy the
   *  `viewportDepthTexture` node performs cannot source an MSAA buffer. */
  pixelDepth?: boolean
}

export interface WaterFieldMaterialUniforms {
  uTime: { value: number }
  uSunDirection: { value: THREE.Vector3 }
  uSunColor: { value: THREE.Color }
  uCameraDirection: { value: THREE.Vector3 }
  uMoonBrightness: { value: number }
  uTorchPos: { value: THREE.Vector3 }
  uTorchColor: { value: THREE.Color }
  uTorchIntensity: { value: number }
  uTorchDistance: { value: number }
  uRefractionMap: { value: THREE.Texture }
  uReflectionMap: { value: THREE.Texture }
  uHeightmapTexture: { value: THREE.Texture }
  uWaterField: { value: THREE.Texture }
  uNormalMap: { value: THREE.Texture }
  uFoamMap: { value: THREE.Texture }
  uCausticsMap: { value: THREE.Texture }
  uWetnessMap: { value: THREE.Texture }
  uWaveA: { value: THREE.Vector4 }
  uWaveB: { value: THREE.Vector4 }
  uWaveC: { value: THREE.Vector4 }
}

export interface WaterFieldMaterialResult {
  material: NodeMaterial
  uniforms: WaterFieldMaterialUniforms
}

// Pre-baked tileable value noise texture (512×512, 64 periods with
// smoothstep interpolation baked in). A texture beats procedural
// hash+valueNoise Fn()s here: inlining them ballooned the WGSL and
// pipeline compile time.
let _noiseTex: THREE.Texture | null = null
function getNoiseTexture(): THREE.Texture {
  if (!_noiseTex) {
    const loader = new THREE.TextureLoader()
    _noiseTex = loader.load('/textures/value-noise.jpg')
    _noiseTex.wrapS = _noiseTex.wrapT = THREE.RepeatWrapping
    _noiseTex.minFilter = THREE.LinearMipMapLinearFilter
    _noiseTex.magFilter = THREE.LinearFilter
  }
  return _noiseTex
}
const NOISE_PERIODS = 64

/** Shore-swell shape: crests spawn at `SWELL_SPAWN_DEPTH` (m) and run up
 *  to `SWELL_SHORE_DEPTH` (≈ the waterline). The pulse starts low, broad,
 *  and symmetric; shoaling raises and narrows it, with the shoreward face
 *  steepening fastest. Breaking collapses it into a low, broad bore. Widths
 *  are in noisy depth-space meters. */
const SWELL_SPAWN_DEPTH = 1.5
const SWELL_SHORE_DEPTH = 0.03
const SWELL_SPAWN_AMP = 0.18
const SWELL_BREAK_AMP = 0.42
const SWELL_SPAWN_W = 0.55
const SWELL_BREAK_FRONT_W = 0.08
const SWELL_BREAK_BACK_W = 0.22
const SWELL_BORE_FRONT_W = 0.45
const SWELL_BORE_BACK_W = 0.75

/** Whitewash front geometry (noisyD depth units): the front sits just
 *  landward of the swell peak (PEAK_OFFSET), overshoots past the
 *  waterline analytically at full run-up, then receives a shallow-water
 *  render offset because the dry-land part is depth-occluded. It retreats
 *  seaward by FLUSH_RETREAT during the flush. Seaward the foam is hard-capped
 *  at WASH_BACK_DEPTH — where the wave broke (= the crest depth at break
 *  onset) — so the open sea never whitens. */
const WASH_PEAK_OFFSET = 0.08
const WASH_OVERSHOOT = -0.15
/** Visual compensation for the water-plane depth clip: the analytic run-up
 *  travels onto dry land, but that part of the sea-level mesh is occluded by
 *  terrain. Shift its foam footprint back into visible shallow water. */
const WASH_RUNUP_SEAWARD_SHIFT = 0.22
/** Smaller visibility compensation for the low post-break bore. It ramps in
 *  only after the whitecap has finished breaking so foam does not detach from
 *  the tall geometric crest. */
const BORE_FOAM_SEAWARD_SHIFT = 0.12
const WASH_FLUSH_RETREAT = 0.6
const WASH_BACK_DEPTH =
  SWELL_SPAWN_DEPTH + (SWELL_SHORE_DEPTH - SWELL_SPAWN_DEPTH) * BRK_START_MOVE

/** Swash-water sheet (noisyD depth units): the drained edge sits at
 *  SWASH_MAX_DEPTH between waves; the run-up sweeps the edge landward
 *  past the waterline to SWASH_RUNUP_OVERSHOOT so the strip floods with
 *  the whitewash. */
const SWASH_MAX_DEPTH = 0.7
const SWASH_RUNUP_OVERSHOOT = -0.45

/** DEBUG: paint the broken-wave residue red (instead of the normal white)
 *  so its lifetime/extent can be judged in isolation. */
const RESIDUE_DEBUG = false

/** DEBUG: paint the traveling shore-swell profile by face: red on the
 *  shoreward/front slope, blue on the seaward/back slope. */
const SWELL_PROFILE_DEBUG = false

/** Depth + per-wave world-space noise — the coordinate the shore bands and
 *  swell live in. The seed stays constant for one wave, then offsets the
 *  noise field when that phase starts its next cycle, giving every wave a
 *  different but stable ragged front. Must stay identical between vertex
 *  (swell displacement) and fragment (foam) or the whitecap slides off the
 *  geometric crest. */
function buildNoisyDepth(
  depth: N,
  worldXZ: N,
  waveSeed: N,
  sampleNoise: (c: N) => N
): N {
  const waveOffset = vec2(waveSeed.mul(17.13), waveSeed.mul(29.71))
  return depth
    .add(sampleNoise(worldXZ.mul(0.3).add(waveOffset)).mul(0.15))
    .add(
      sampleNoise(
        worldXZ.mul(0.15).add(vec2(waveSeed.mul(31.37), waveSeed.mul(11.79)))
      ).mul(0.1)
    )
    .add(
      sampleNoise(
        worldXZ.mul(0.2).add(vec2(waveSeed.mul(7.43), waveSeed.mul(23.17)))
      ).mul(0.3)
    )
    .toVar()
}

/** One traveling shore-wave phase. `move` runs the crest from spawn depth
 *  to the waterline over the first MOVE_END of the cycle; `brk` is
 *  breaking progress — the swell collapses and the whitecap blooms as
 *  the crest passes BRK_START_MOVE of its travel; the broken bore then
 *  rides the rest of the way in under its foam front.
 *  `fade` kills the swell and its foam right after landfall, so the
 *  backwash is bare water — only the whitewash sheet (see
 *  `buildPhaseFoam`) and the anchored residue stay behind. `seed` is a
 *  per-wave constant that decorrelates each wave's residue pattern. Two
 *  half-offset phases give a continuous wave train. */
function buildShoreWavePhase(uTime: N, offset: number) {
  const cycle = fract(uTime.mul(SHORE_WAVE_SPEED).add(offset)).toVar()
  const move = smoothstep(float(0), float(MOVE_END), cycle).toVar()
  const center = mix(
    float(SWELL_SPAWN_DEPTH),
    float(SWELL_SHORE_DEPTH),
    move
  ).toVar()
  // Cross-fade the low bore during the opening of the longer run-up. Keeping
  // this fade short avoids stretching the geometric bore across the whole
  // run-up while its whitewash continues moving landward.
  const fade = smoothstep(float(0), float(0.1), cycle)
    .mul(
      float(1).sub(
        smoothstep(float(WASH_RUNUP_START), float(BORE_FADE_END), cycle)
      )
    )
    .toVar()
  const brk = smoothstep(
    float(BRK_START_MOVE),
    float(BRK_END_MOVE),
    move
  ).toVar()
  const seed = fract(
    floor(uTime.mul(SHORE_WAVE_SPEED).add(offset))
      .mul(0.618034)
      .add(offset * 0.754877666)
  ).toVar()
  // Backwash progress: 0 until landfall, → 1 as the sheet finishes
  // draining. Drives the swash-sheet edge, the residue's seaward drag
  // and dimming, and the receding-edge foam line.
  const receded = smoothstep(
    float(RECEDE_START),
    float(RECEDE_END),
    cycle
  ).toVar()
  // Whitewash sheet progress: `runup` pushes the broken foam front from
  // the break point up to the waterline; `flush` is the backwash pulling
  // it out — the sheet fades and slides seaward as flush → 1.
  const runup = smoothstep(
    float(WASH_RUNUP_START),
    float(WASH_RUNUP_END),
    cycle
  ).toVar()
  const flush = smoothstep(
    float(WASH_FLUSH_START),
    float(WASH_FLUSH_END),
    cycle
  ).toVar()
  return { cycle, move, center, fade, brk, seed, receded, runup, flush }
}

/** Swell displacement of one phase at `noisyD`: the asymmetric hump grows
 *  approaching the shore (shoaling) and collapses once it breaks. `prof`
 *  is the raw 0–1 shape, reused by the fragment as the whitecap mask. */
function buildSwell(
  noisyD: N,
  depth: N,
  phase: ReturnType<typeof buildShoreWavePhase>
) {
  const s = noisyD.sub(phase.center)
  // Finish shoaling just as breaking starts. The front reaches its minimum
  // safe width first, reading as nearly vertical without becoming narrower
  // than the water mesh can resolve reliably.
  const shoal = smoothstep(float(0), float(BRK_START_MOVE), phase.move)
  const approachFrontW = mix(
    float(SWELL_SPAWN_W),
    float(SWELL_BREAK_FRONT_W),
    shoal
  )
  const approachBackW = mix(
    float(SWELL_SPAWN_W),
    float(SWELL_BREAK_BACK_W),
    shoal
  )
  // Let the cap form briefly at maximum steepness, then rapidly spread the
  // collapsing pulse into a low bore that continues toward the shoreline.
  const collapse = smoothstep(float(0.35), float(1), phase.brk)
  const frontW = mix(approachFrontW, float(SWELL_BORE_FRONT_W), collapse)
  const backW = mix(approachBackW, float(SWELL_BORE_BACK_W), collapse)
  // Near-triangular profile: linear faces keep a sharp crest kink right
  // on the foam line (smoothstep faces rounded it into a soft mound);
  // the mild pow tightens the toes without softening the peak.
  const prof = pow(
    clamp(min(s.div(frontW).add(1), float(1).sub(s.div(backW))), 0.0, 1.0),
    float(1.3)
  ).toVar()
  const shoalingAmp = mix(float(SWELL_SPAWN_AMP), float(SWELL_BREAK_AMP), shoal)
  // 75% collapse (not full): a low bore hump keeps traveling under the
  // foam front after the break, like a real spilling breaker.
  const amp = shoalingAmp.mul(phase.fade).mul(float(1).sub(collapse.mul(0.75)))
  // Feather at the true waterline so the crest never lifts dry sand.
  const height = prof.mul(amp).mul(smoothstep(float(0.03), float(0.18), depth))
  return { height, prof }
}

/** Animated receding-wave shore mask (shared with the wetness capture
 *  material). `holeAlpha` ≈ 1 in the water body, dipping toward 0 inside
 *  the wave-synced noise holes near the waterline; `holeEdge` /
 *  `holeFoamFringe` outline those holes for foam. */
function buildShoreMaskNodes(
  depth: N,
  worldPos: N,
  uTime: N,
  sampleNoise: (c: N) => N
) {
  const shorePhase = uTime
    .mul(SHORE_WAVE_SPEED)
    .mul(PI.mul(4))
    .sub(PI.mul(1.0 / 2.0))
  const shoreRecede = sin(shorePhase).mul(0.5).add(0.5)
  const shoreAdjustedDepth = max(float(0), depth.sub(shoreRecede.mul(0.35)))
  const shoreZone = float(1).sub(
    smoothstep(float(0), float(0.45), shoreAdjustedDepth)
  )

  const sn1 = sampleNoise(worldPos.xz.mul(0.2).add(uTime.mul(0.07)))
  const sn2 = sampleNoise(worldPos.xz.mul(0.4).add(uTime.mul(0.04)))
  const sn3 = sampleNoise(worldPos.xz.mul(0.08).add(uTime.mul(0.1)))
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
  const holeFoamFringe = smoothstep(float(-0.03), float(0.0), distFromHole).mul(
    shoreZone
  )

  return { holeAlpha, holeFoamFringe, holeEdge, shoreZone }
}

export function createWaterFieldMaterial(
  options: WaterFieldMaterialOptions
): WaterFieldMaterialResult {
  const pixelDepth = options.pixelDepth ?? false

  // ── Uniforms ──
  const uTime = uniform(0)
  const makeWaveVec = (cfg: (typeof waveConfigs)[number]) =>
    new THREE.Vector4(
      Math.sin(cfg.angle),
      Math.cos(cfg.angle),
      cfg.steepness,
      cfg.wavelength
    )
  const uWaveA = uniform(makeWaveVec(waveConfigs[0]))
  const uWaveB = uniform(makeWaveVec(waveConfigs[1]))
  const uWaveC = uniform(makeWaveVec(waveConfigs[2]))

  // Sea palette (4-stop) + river palette (3-stop) — blended by riverness.
  const uVeryShallowColor = uniform(new THREE.Color(0.75, 0.88, 0.78))
  const uShallowColor = uniform(new THREE.Color(0.2, 0.58, 0.42))
  const uMidColor = uniform(new THREE.Color(0.02, 0.34, 0.32))
  const uDeepColor = uniform(new THREE.Color(0.002, 0.06, 0.18))
  const uRiverShallowColor = uniform(new THREE.Color(0.1, 0.18, 0.3))
  const uRiverMidColor = uniform(new THREE.Color(0.03, 0.09, 0.2))
  const uRiverDeepColor = uniform(new THREE.Color(0.015, 0.035, 0.11))

  /** Sea scatter-gradient depth normalization (m). */
  const uMaxDepth = uniform(2.5)
  /** River gradient/alpha depth plateau (m) — matches the bake's
   *  `RIVER_DEPTH_OFFSET_M = 0.5` channel depth. */
  const uRiverMaxDepth = uniform(0.5)
  /** Beer-Lambert absorption per meter of view distance through water
   *  (pixelDepth only). Red absorbs fastest, as in real water. */
  const uAbsorption = uniform(new THREE.Vector3(0.45, 0.15, 0.08))

  const uSunDirection = uniform(new THREE.Vector3(0.5, 0.8, 0.3).normalize())
  const uSunColor = uniform(new THREE.Color(1.0, 0.95, 0.8))
  const uCameraDirection = uniform(new THREE.Vector3(0, -1, 0))
  const uMoonBrightness = uniform(0)
  const uRefractionStrength = uniform(0.1)
  const uTorchPos = uniform(new THREE.Vector3(0, -1000, 0))
  const uTorchColor = uniform(new THREE.Color(1.0, 0.8, 0.4))
  const uTorchIntensity = uniform(0)
  const uTorchDistance = uniform(50)

  // ── Texture Nodes ──
  const heightmapTex = texture(options.heightmapTexture)
  const waterFieldTex = texture(options.waterField)
  const normalMapTex = texture(options.normalMap)
  const foamMapTex = texture(options.foamMap)
  const causticsTex = texture(options.causticsMap)
  const refractionTex = texture(options.refractionMap ?? waterFallbackTex)
  const reflectionTex = texture(options.reflectionMap ?? waterFallbackTex)
  const refractionMixScale = float(options.refractionMap ? 1 : 0)
  const reflectionMixScale = float(options.reflectionMap ? 1 : 0)
  const wetnessMapTex = texture(options.wetnessMap ?? waterWetnessFallbackTex)
  const noiseTex = texture(getNoiseTexture())
  const cloudTex = texture(getCloudTexture())

  // ── Varyings ──
  const vOrigWorldPos = varying(vec3(0), 'v_origWorldPos')
  const vWorldPos = varying(vec3(0), 'v_worldPos')
  const vWaveHeight = varying(float(0), 'v_waveHeight')
  const vClipPos = varying(vec4(0), 'v_clipPos')
  const vUv = varying(vec2(0), 'v_uv')
  const vBedGrad = varying(vec2(0), 'v_bedGrad')

  const sampleNoise = (noiseCoord: N) =>
    noiseTex.sample(noiseCoord.div(NOISE_PERIODS)).r

  // ── Vertex Shader ─────────────────────────────────────
  // Vertex Y already carries the baked water surface. Gerstner waves are
  // added on top, damped both by shallow water (as the old sea shader
  // did) and by riverness so river ribbons stay flow-rippled, not wavy.

  const positionNode = Fn(() => {
    const localPos = positionLocal.toVar()
    vUv.assign(uv())

    const worldPos = modelWorldMatrix.mul(vec4(localPos, 1.0)).toVar()
    vOrigWorldPos.assign(worldPos.xyz)

    const p = worldPos.xyz
    const hUV = toHeightmapUV(vUv)
    // Bed gradient for the fragment swell normal. The heightmap is static
    // (65 texels over 64 m ⇒ ~1 m per texel), so central differences are
    // per-vertex work, not per-fragment fetches. +v is −z in world
    // (PlaneGeometry uv under rotateX(-π/2)).
    const HM_TEXEL = float(1.0 / 65.0)
    vBedGrad.assign(
      vec2(
        heightmapTex
          .sample(hUV.add(vec2(HM_TEXEL, 0)))
          .r.sub(heightmapTex.sample(hUV.sub(vec2(HM_TEXEL, 0))).r),
        heightmapTex
          .sample(hUV.sub(vec2(0, HM_TEXEL)))
          .r.sub(heightmapTex.sample(hUV.add(vec2(0, HM_TEXEL))).r)
      ).mul(0.5)
    )
    const vtxTerrainH = heightmapTex.sample(hUV).r
    const vtxField = waterFieldTex.sample(hUV)
    const vtxRiverness = vtxField.a
    const vtxDepth = max(float(0), p.y.sub(vtxTerrainH))
    // Cubic ease keeps endpoints fixed but compresses the shallow half so
    // the emerald band stays calm — a plain smoothstep read as choppy
    // once the cloud reflection amplified the perceived wave motion.
    const waveDamping = pow(
      smoothstep(float(0.0), float(1.5), vtxDepth),
      float(3.0)
    ).mul(float(1).sub(vtxRiverness))

    const offset = gerstnerWave(uWaveA, p, uTime)
      .add(gerstnerWave(uWaveB, p, uTime))
      .add(gerstnerWave(uWaveC, p, uTime))
      .mul(waveDamping)
      .toVar()

    // Shore swell: traveling breaker humps in the shallow band the
    // Gerstner damping just flattened — same phases/coordinate as the
    // fragment foam so the whitecap sits on the geometric crest. Gated
    // like seaFxGate: no swell inside rivers or across the estuary.
    const vtxSeaness = float(1).sub(vtxRiverness)
    const vtxSwellGate = vtxSeaness
      .mul(vtxSeaness)
      .mul(
        float(1).sub(smoothstep(float(0.02), float(0.2), length(vtxField.gb)))
      )
    const vtxPhaseA = buildShoreWavePhase(uTime, 0)
    const vtxPhaseB = buildShoreWavePhase(uTime, 0.5)
    const vtxNoisyDA = buildNoisyDepth(
      vtxDepth,
      p.xz,
      vtxPhaseA.seed,
      sampleNoise
    )
    const vtxNoisyDB = buildNoisyDepth(
      vtxDepth,
      p.xz,
      vtxPhaseB.seed,
      sampleNoise
    )
    const swellY = buildSwell(vtxNoisyDA, vtxDepth, vtxPhaseA)
      .height.add(buildSwell(vtxNoisyDB, vtxDepth, vtxPhaseB).height)
      .mul(vtxSwellGate)
    offset.addAssign(vec3(0, swellY, 0))

    worldPos.xyz.addAssign(offset)
    vWaveHeight.assign(offset.y)
    vWorldPos.assign(worldPos.xyz)

    const clipPos = cameraProjectionMatrix.mul(cameraViewMatrix).mul(worldPos)
    vClipPos.assign(clipPos)

    return clipPos
  })()

  // ── Fragment Shader ───────────────────────────────────

  const fragmentNode = Fn(() => {
    const sunY = uSunDirection.y
    const LUMA_REC601 = vec3(0.299, 0.587, 0.114)
    const sampleUV = clamp(toHeightmapUV(vUv), 0.0, 1.0)

    // ── Field + analytic (heightmap) depth ──
    const field = waterFieldTex.sample(sampleUV)
    const riverness = field.a.toVar()
    const seaness = float(1).sub(riverness).toVar()
    const flow = field.gb.toVar() // magnitude = baked estuary-decayed speed
    // (field.r carries the baked turbulence — consumed CPU-side by
    // river-rock placement and its drifting wake-foam particles, not
    // sampled here.)
    // Geographic river influence (successor of the old splatmap-G byte).
    // Baked flow magnitude is ≥ 0.3 × radial-envelope inside the channel
    // and exactly 0 outside, so it marks "near a river" even at the mouth
    // where the estuary gate has already zeroed riverness — without it,
    // sea foam bands / cloud highlights / shore wash would paint right
    // across the estuary ribbon.
    const flowProx = smoothstep(float(0.02), float(0.2), length(flow)).toVar()
    const riverProxGate = float(1).sub(flowProx).toVar()
    // Sea surface-effect gate: off inside inland channels (riverness) AND
    // near mouths (flow proximity).
    const seaFxGate = seaness.mul(seaness).mul(riverProxGate).toVar()
    // River-styling weight for palette/alpha: riverness alone drops to 0
    // at the mouth (estuary gate) while the water there is still visually
    // the river — flow proximity keeps the river palette and bank alpha
    // ramp all the way to the sea. Zero in open water (flow = 0).
    const riverStyleW = max(riverness, flowProx).toVar()
    const bedHeight = heightmapTex.sample(sampleUV).r
    const depth = max(float(0), vOrigWorldPos.y.sub(bedHeight)).toVar()
    const depthFactorSea = clamp(depth.div(uMaxDepth), 0.0, 1.0)
    const depthFactorRiver = clamp(depth.div(uRiverMaxDepth), 0.0, 1.0)
    const depthFactor = mix(depthFactorSea, depthFactorRiver, riverness)

    // ── Shore swell (fragment recompute of the vertex displacement) ──
    // Same phases as the foam bands below so the whitecap sits on the
    // geometric crest. The depth-space slope (finite difference) times
    // the bed gradient gives the world-space slope for the normal.
    const phaseA = buildShoreWavePhase(uTime, 0)
    const phaseB = buildShoreWavePhase(uTime, 0.5)
    const noisyDA = buildNoisyDepth(
      depth,
      vOrigWorldPos.xz,
      phaseA.seed,
      sampleNoise
    )
    const noisyDB = buildNoisyDepth(
      depth,
      vOrigWorldPos.xz,
      phaseB.seed,
      sampleNoise
    )
    const swellA = buildSwell(noisyDA, depth, phaseA)
    const swellB = buildSwell(noisyDB, depth, phaseB)
    const buildSwellDebugFaces = (
      noisyD: N,
      phase: ReturnType<typeof buildShoreWavePhase>,
      swell: ReturnType<typeof buildSwell>
    ) => {
      const s = noisyD.sub(phase.center)
      const active = swell.prof
        .mul(phase.fade)
        .mul(smoothstep(float(0.03), float(0.18), depth))
      // Depth decreases toward shore, so s < 0 is the incoming/front
      // face and s > 0 is the trailing, seaward/back face.
      const backWeight = smoothstep(float(-0.015), float(0.015), s)
      return {
        front: active.mul(float(1).sub(backWeight)),
        back: active.mul(backWeight),
      }
    }
    const debugFacesA = buildSwellDebugFaces(noisyDA, phaseA, swellA)
    const debugFacesB = buildSwellDebugFaces(noisyDB, phaseB, swellB)
    const swellDebugFront = max(debugFacesA.front, debugFacesB.front)
    const swellDebugBack = max(debugFacesA.back, debugFacesB.back)
    const SWELL_EPS = 0.08
    const swellSlopeD = buildSwell(noisyDA.add(SWELL_EPS), depth, phaseA)
      .height.add(buildSwell(noisyDB.add(SWELL_EPS), depth, phaseB).height)
      .sub(swellA.height.add(swellB.height))
      .div(SWELL_EPS)
    // Depth grows where the bed drops (d·depth/dx = −gradBed.x), so the
    // heightfield normal (−dh/dx, 1, −dh/dz) reduces to +slope × gradBed
    // (bed gradient interpolated from the vertex stage).
    const swellN = vec3(
      swellSlopeD.mul(vBedGrad.x),
      float(0),
      swellSlopeD.mul(vBedGrad.y)
    )

    // ── Swash transparency ──
    // The swash strip only holds water while a wave is in. Per phase the
    // sheet's edge is a depth threshold: on the run-in it hugs the
    // incoming crest (no standing water ahead of the wave — like the
    // drained strip in front of a breaker), then the run-up sweeps it
    // landward past the noise floor so the water boundary visibly rushes
    // up the strip with the whitewash, and the backwash pulls it back
    // out to SWASH_MAX_DEPTH. min() of the two phases: either phase's
    // water covers the strip.
    const swashThreshold = (phase: ReturnType<typeof buildShoreWavePhase>) => {
      const inThr = min(phase.center, float(SWASH_MAX_DEPTH))
      const runupThr = mix(inThr, float(SWASH_RUNUP_OVERSHOOT), phase.runup)
      return max(runupThr, phase.receded.mul(SWASH_MAX_DEPTH))
    }
    const swashThrA = swashThreshold(phaseA)
    const swashThrB = swashThreshold(phaseB)
    const swashWaterGate = mix(
      float(1),
      max(
        smoothstep(swashThrA, swashThrA.add(0.08), noisyDA),
        smoothstep(swashThrB, swashThrB.add(0.08), noisyDB)
      ),
      seaFxGate
    ).toVar()
    // Unit landward direction (the bed gradient points uphill). Sampling
    // the foam pattern landward of the fragment makes the pattern appear
    // to drift seaward — the backwash drag on the swash-zone foam.
    const landwardDir = vBedGrad.div(max(length(vBedGrad), float(0.02))).toVar()

    // ── Per-pixel view distance through water (depth buffer) ──
    // Orthographic camera: NDC z is already linear in [near, far] and the
    // depth attachment stores it raw, so the water column the view ray
    // crosses is just Δdepth01 × (far − near). `vertDepth` projects that
    // onto the vertical axis — the water thickness above whatever opaque
    // surface (terrain, player, prop) the depth buffer saw.
    // `viewportLinearDepth` = linearDepth(viewportDepthTexture()): for an
    // orthographic camera that's the raw depth-attachment value, linear01
    // in [near, far]. Own-fragment depth comes from vClipPos (NOT the
    // built-in linearDepth — that derives from positionView, which never
    // sees the custom vertexNode's Gerstner displacement).
    const fragDepth01 = vClipPos.z.div(vClipPos.w)
    const viewDist = pixelDepth
      ? max(float(0), viewportLinearDepth.sub(fragDepth01))
          .mul(cameraFar.sub(cameraNear))
          .toVar()
      : depth.toVar()
    const vertDepth = pixelDepth
      ? viewDist.mul(abs(uCameraDirection.y)).toVar()
      : depth

    // ── Surface normal: Gerstner+ripple (sea) ⇄ flowmap ripple (river) ──
    const gnA = gerstnerNormal(uWaveA, vOrigWorldPos, uTime)
    const gnB = gerstnerNormal(uWaveB, vOrigWorldPos, uTime)
    const gnC = gerstnerNormal(uWaveC, vOrigWorldPos, uTime)
    const tx = float(1.0).add(gnA.x).add(gnB.x).add(gnC.x)
    const ty = gnA.y.add(gnB.y).add(gnC.y)
    const bz = float(1.0).add(gnA.z).add(gnB.z).add(gnC.z)
    const by = gnA.w.add(gnB.w).add(gnC.w)
    const gerstnerN = normalize(vec3(ty.negate(), tx.mul(bz), by.negate()))
    const rippleNoise = sampleNormalNoise(
      vOrigWorldPos.xz,
      normalMapTex,
      uTime,
      uWaveA,
      uWaveB,
      uWaveC
    )
    const seaN = normalize(
      gerstnerN
        .add(rippleNoise.xzy.mul(vec3(1.5, 0.0, 1.5)))
        .add(swellN.mul(float(1.8).mul(seaFxGate)))
    )

    // River ripples: two-phase wrapped flowmap. Unbounded `flow × time`
    // UV drift decorrelates neighbouring fragments at Voronoi seams and
    // confluences into a vortex artifact; wrapping each phase in [0, 1]
    // and crossfading two half-period-offset phases hides the wrap.
    // `waternormals` is conventionally encoded around 0.5. Decode the
    // averaged samples back to a signed vector before building the normal;
    // subtracting 1 after the average left an almost constant diagonal tilt
    // that hid the time-varying downstream ripple.
    const NORMAL_SCALE = float(0.18)
    // Wrap frequency of the two-phase crossfade. Must stay BOTH low and
    // spatially uniform:
    //  * above ~1 Hz the phases cross-dissolve faster than the eye can
    //    track, and (the blend's mean offset being a constant 0.5·drift)
    //    the pattern time-averages into a near-static shimmer — raising
    //    the rate made the river look SLOWER, which is how the old
    //    riverness-blended 4.0 inland rate silently did nothing;
    //  * a riverness-dependent rate lets fract(uTime·rate) diverge
    //    without bound between neighbouring fragments, shredding the
    //    estuary blend band into unrelated phases as uTime grows.
    // Perceived current is therefore tuned with the drift LENGTH per
    // cycle below, never with this rate:
    //   speed ≈ |flow| · driftLen · RIPPLE_WRAP_RATE / NORMAL_SCALE  [m/s]
    const RIPPLE_WRAP_RATE = float(0.35)
    // Inland: |flow| ≈ 1 mid-channel ⇒ ~1.9 m/s — a brisk, clearly legible
    // current (the baked flow map supplies direction and bank falloff).
    // NOTE: with the coast-distance estuary gate this now applies to the
    // whole inland course (plains included), not just mountain reaches.
    const RIVER_DRIFT_LEN = float(1.0)
    // Mouth: |flow| has decayed to 0.3 there ⇒ ~0.1 m/s, matching the
    // deliberately gentle drift the estuary already had.
    const ESTUARY_DRIFT_LEN = float(0.17)
    const buildWrappedDrift = (driftLen: N, flowVec: N) => {
      const phase = uTime.mul(RIPPLE_WRAP_RATE)
      const pA = fract(phase)
      const pB = fract(phase.add(0.5))
      const mixW = abs(pA.sub(0.5)).mul(2.0)
      const drift = flowVec.mul(driftLen)
      return { driftA: drift.mul(pA), driftB: drift.mul(pB), mixW }
    }
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
        .sub(0.5)
        .mul(2.0)
      const sB = normalMapTex
        .sample(a.sub(offB))
        .add(normalMapTex.sample(b.sub(offB.mul(flowScale2))))
        .mul(0.5)
        .sub(0.5)
        .mul(2.0)
      const s = mix(sA, sB, mixW)
      return normalize(vec3(s.r.mul(1.2), float(1.0), s.g.mul(1.2)))
    }
    const {
      driftA: flowOffA,
      driftB: flowOffB,
      mixW: rippleMix,
    } = buildWrappedDrift(
      // Keep the very slow mouth intact, but recover the inland current soon
      // after leaving it. This concentrates the deceleration at the sea end
      // instead of making the whole lower reach feel sluggish.
      mix(
        ESTUARY_DRIFT_LEN,
        RIVER_DRIFT_LEN,
        smoothstep(float(0.3), float(0.85), riverness)
      ),
      flow
    )
    const nBase1 = vOrigWorldPos.xz.mul(NORMAL_SCALE)
    const nBase2 = vOrigWorldPos.xz.mul(NORMAL_SCALE.mul(0.6)).add(vec2(0.3, 0))
    const riverN = buildRippleN(
      nBase1,
      nBase2,
      flowOffA,
      flowOffB,
      float(0.7),
      rippleMix
    )

    // Flow remains valid through the estuary even after `riverness` reaches
    // zero, so use the same styling weight that drives its colour/alpha.
    const surfaceNormal = normalize(mix(seaN, riverN, riverStyleW))

    // ── Body color ──
    const seaC1 = mix(
      uVeryShallowColor,
      uShallowColor,
      smoothstep(float(0.0), float(0.08), depthFactorSea)
    )
    const seaC2 = mix(
      seaC1,
      uMidColor,
      smoothstep(float(0.08), float(0.25), depthFactorSea)
    )
    const seaBody = mix(
      seaC2,
      uDeepColor,
      smoothstep(float(0.25), float(0.7), depthFactorSea)
    )
    const riverC1 = mix(
      uRiverShallowColor,
      uRiverMidColor,
      smoothstep(float(0.0), float(0.4), depthFactorRiver)
    )
    const riverBody = mix(
      riverC1,
      uRiverDeepColor,
      smoothstep(float(0.4), float(0.85), depthFactorRiver)
    )
    const waterColor = mix(seaBody, riverBody, riverStyleW).toVar()

    // ── View / screen ──
    const viewDir = normalize(vec3(uCameraDirection).negate())
    const screenUV = vClipPos.xy.mul(0.5).add(0.5)
    const screenUVFlipped = vec2(screenUV.x, float(1.0).sub(screenUV.y))

    // ── Time-of-day factors (shared) ──
    const nightFactor = float(1)
      .sub(smoothstep(float(-0.15), float(0.05), sunY))
      .toVar()
    const twilightFactor = smoothstep(float(-0.15), float(0.0), sunY).mul(
      float(1).sub(smoothstep(float(0.05), float(0.3), sunY))
    )
    const dayFactor = smoothstep(float(0.05), float(0.3), sunY)

    // ── Refraction ──
    const distort = surfaceNormal.xz.mul(
      mix(uRefractionStrength, float(0.04), riverStyleW)
    )
    const refrUV = clamp(screenUVFlipped.add(distort), 0.0, 1.0)
    const refrRaw = refractionTex.sample(refrUV).rgb.toVar()
    // Beer-Lambert: the bed seen through more water loses red first.
    if (pixelDepth) {
      const uAbs = vec3(uAbsorption)
      refrRaw.mulAssign(
        vec3(
          exp(viewDist.mul(uAbs.x).negate()),
          exp(viewDist.mul(uAbs.y).negate()),
          exp(viewDist.mul(uAbs.z).negate())
        )
      )
    }
    // Wet-sand darkening rides the refracted bed — where the wetness map
    // is high the bed reads as soaked sand (replaces the old transparent
    // "hole" reveal, which is gone with the opaque shoreline).
    const texelSize = float(1.0 / 256) // WETNESS_SIZE
    const rawWetness = wetnessMapTex
      .sample(vUv.add(vec2(texelSize, 0)))
      .r.add(wetnessMapTex.sample(vUv.add(vec2(texelSize.negate(), 0))).r)
      .add(wetnessMapTex.sample(vUv.add(vec2(0, texelSize))).r)
      .add(wetnessMapTex.sample(vUv.add(vec2(0, texelSize.negate()))).r)
      .mul(0.25)
    const wetness = smoothstep(float(0.2), float(0.7), rawWetness)
    const wetDarken = mix(float(1.0), float(0.55), wetness.mul(seaFxGate))
    refrRaw.mulAssign(wetDarken)

    // River-side night grading of the refracted bed.
    const refrLuma = dot(refrRaw, LUMA_REC601)
    const nightRefr = mix(refrRaw, vec3(refrLuma), nightFactor.mul(0.9)).mul(
      float(1).sub(nightFactor.mul(0.88))
    )
    const refrColor = mix(refrRaw, nightRefr, nightFactor.mul(riverness))

    const seaRefrMix = float(1)
      .sub(smoothstep(float(0.05), float(0.35), depthFactorSea))
      .mul(0.95)
    const riverRefrShallow = float(1)
      .sub(smoothstep(float(0.05), float(0.5), depthFactorRiver))
      .toVar()
    const refrMix = mix(seaRefrMix, riverRefrShallow.mul(0.85), riverness)
      .mul(refractionMixScale)
      .toVar()

    // Night-darken the sea body before mixing (river handles night via
    // the luma mute in its composite path).
    const waterNightFactor = smoothstep(float(-0.05), float(0.1), sunY)
      .mul(0.85)
      .add(0.15)
    waterColor.mulAssign(mix(waterNightFactor, float(1.0), riverness))
    waterColor.assign(mix(waterColor, refrColor, refrMix))

    // ── Caustics (sea only — the flow ripple field would fight it) ──
    const causticsGate = seaFxGate
    const cUV1 = vOrigWorldPos.xz
      .mul(0.1)
      .add(vec2(uTime.mul(0.015), uTime.mul(0.01)))
    const cUV2 = vOrigWorldPos.xz
      .mul(0.095)
      .sub(vec2(uTime.mul(0.008), uTime.mul(0.01)))
    const rawCaustics = causticsTex
      .sample(cUV1)
      .r.min(causticsTex.sample(cUV2).r)
    const causticsDetail = foamMapTex.sample(
      vOrigWorldPos.xz.mul(0.3).add(uTime.mul(0.01))
    ).r
    const causticsPattern = rawCaustics
      .min(float(0.5))
      .div(float(0.5))
      .mul(causticsDetail)
    const shimmer = sin(
      vOrigWorldPos.x.mul(0.4).add(vOrigWorldPos.z.mul(0.6)).add(uTime.mul(0.5))
    )
      .mul(0.4)
      .add(0.8)
    const causticsStrength = float(1).sub(
      smoothstep(float(0), float(0.5), depthFactorSea)
    )
    const causticsNight = smoothstep(float(-0.05), float(0.1), sunY)
    const causticsLight = mix(
      vec3(0.08, 0.1, 0.15),
      uSunColor.rgb,
      causticsNight
    )
    const causticsDepthGate = smoothstep(
      float(0.05),
      float(0.25),
      depthFactorSea
    )
    waterColor.addAssign(
      causticsLight
        .mul(causticsPattern.mul(shimmer).mul(1.2))
        .mul(causticsStrength)
        .mul(causticsDepthGate)
        .mul(causticsGate)
    )

    // ── Specular + sparkles (blended normal, blended magnitudes) ──
    const specNormal = normalize(mix(vec3(0, 1, 0), surfaceNormal, 0.3))
    const halfDir = normalize(vec3(uSunDirection).add(viewDir))
    const NdotH = max(dot(specNormal, halfDir), 0.0)
    const specular = uSunColor.rgb
      .mul(pow(NdotH, float(128)).mul(mix(float(0.3), float(0.35), riverness)))
      .toVar()

    const spT = uTime.mul(0.04)
    const drift1 = vec2(spT, spT.mul(0.7))
    const drift2 = vec2(spT.mul(0.6), spT)
    const sp1 = normalMapTex.sample(vWorldPos.xz.mul(0.5).add(drift1)).r
    const sp2 = normalMapTex.sample(vWorldPos.xz.mul(0.8).sub(drift2)).g
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
    const seaSparkle = smoothstep(float(1.3), float(1.45), sp1.add(sp2))
      .mul(8.0)
      .mul(waveCrestFactor)
      .mul(max(sunSparkleStrength, moonSparkleStrength))
    // River motion comes solely from its flow-advected normal map. Keep the
    // sea's glitter out of the river/estuary instead of adding a second,
    // more conspicuous motion layer.
    const sparkle = seaSparkle.mul(float(1).sub(riverStyleW))
    specular.addAssign(uSunColor.rgb.mul(sparkle))

    // ── Sky reflection (shared structure, riverness-blended palettes) ──
    const reflNormal = normalize(
      mix(vec3(0, 1, 0), surfaceNormal, mix(float(0.3), float(0.05), riverness))
    )
    const reflectDir = reflect(viewDir.negate(), reflNormal)
    const skyY = clamp(reflectDir.y.mul(0.5).add(0.5), 0.0, 1.0)

    const groundColor = mix(
      vec3(0.02, 0.03, 0.06),
      vec3(0.012, 0.015, 0.022),
      riverness
    )
      .mul(nightFactor)
      .add(vec3(0.12, 0.06, 0.04).mul(twilightFactor))
      .add(vec3(0.08, 0.12, 0.15).mul(dayFactor))
    const hazeColorBase = mix(
      vec3(0.04, 0.06, 0.12),
      vec3(0.021, 0.026, 0.035),
      riverness
    )
      .mul(nightFactor)
      .add(vec3(0.7, 0.35, 0.15).mul(twilightFactor))
      .add(
        mix(vec3(0.55, 0.65, 0.75), vec3(0.28, 0.42, 0.68), riverness).mul(
          dayFactor
        )
      )
    const zenithColor = mix(
      vec3(0.02, 0.04, 0.1),
      vec3(0.015, 0.019, 0.03),
      riverness
    )
      .mul(nightFactor)
      .add(vec3(0.15, 0.1, 0.25).mul(twilightFactor))
      .add(
        mix(vec3(0.12, 0.25, 0.5), vec3(0.08, 0.22, 0.55), riverness).mul(
          dayFactor
        )
      )

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

    // ── Cloud photo (one sample, applied river-style + sea-style) ──
    const cloudReflNormal = normalize(mix(vec3(0, 1, 0), surfaceNormal, 0.05))
    const cloudReflectDir = reflect(viewDir.negate(), cloudReflNormal)
    const { cloudColor, cloudWeight } = sampleCloudPhoto(
      cloudReflectDir,
      vWorldPos.xz,
      uTime,
      dayFactor,
      cloudTex
    )
    // River: clouds fold into the sky reflection…
    skyReflection.assign(
      mix(
        skyReflection,
        cloudColor,
        cloudWeight
          .mul(mix(float(0.26), float(0.6), reflectionMixScale))
          .mul(riverStyleW)
      )
    )
    // …with sunset/night grading so night rivers don't mirror a day photo.
    const cloudLuma = dot(cloudColor, LUMA_REC601)
    const cloudHorizonWeight = smoothstep(float(0.15), float(0.45), skyY)
    const sunsetCloudColor = mix(cloudColor, vec3(cloudLuma), 0.52)
      .mul(vec3(0.62, 0.18, 0.075))
      .mul(0.12)
      .add(vec3(0.006, 0.003, 0.0015).mul(sunsetFactor))
    skyReflection.assign(
      mix(
        skyReflection,
        sunsetCloudColor,
        cloudHorizonWeight.mul(twilightFactor).mul(0.85).mul(riverStyleW)
      )
    )
    const nightCloudColor = mix(cloudColor, vec3(cloudLuma), 0.7)
      .mul(0.08)
      .add(vec3(0.004, 0.006, 0.01))
    skyReflection.assign(
      mix(
        skyReflection,
        nightCloudColor,
        cloudHorizonWeight.mul(nightFactor).mul(0.85).mul(riverStyleW)
      )
    )

    // ── Entity reflection (planar pass) ──
    const reflectionSample = reflectionTex.sample(
      clamp(screenUVFlipped.add(surfaceNormal.xz.mul(0.01)), 0.0, 1.0)
    )
    skyReflection.assign(
      mix(
        skyReflection,
        reflectionSample.rgb,
        reflectionSample.a.mul(0.5).mul(reflectionMixScale)
      )
    )

    // ── Shore wash + foam bands (sea side) ──
    const shore = buildShoreMaskNodes(depth, vOrigWorldPos, uTime, sampleNoise)
    // One phase's wave foam, following the real surf lifecycle:
    //  1. approach — a moderate foam line rides the traveling crest,
    //  2. breaking — at BRK_START_MOVE of the run-in the whitecap
    //     blooms as `brk` → 1 and the swell collapses into a low bore,
    //  3. bore + run-up — the whitewash front rides the bore the rest
    //     of the way in (foam only landward of the break point), then
    //     surges past the waterline over the swash strip,
    //  4. backwash — the flush fades the sheet fast while dragging it
    //     seaward; what stays is the residue: foam anchored to the
    //     ground (static per-wave UV, not drifting with the water) over
    //     the swash zone, dissolving into patches as a rising threshold
    //     eats the foam texture.
    const buildPhaseFoam = (
      phase: ReturnType<typeof buildShoreWavePhase>,
      noisyD: N
    ) => {
      const bw = float(0.04).add(float(0.1).mul(phase.move))
      const breakSpread = smoothstep(float(0), float(0.65), phase.brk)
      // Breaking completes near cycle 0.30. Delay the render-space shift
      // until afterwards so the cap stays attached to the steep crest, then
      // move the low bore foam into depth-visible shallow water.
      const boreSeawardShift = smoothstep(
        float(0.32),
        float(0.46),
        phase.cycle
      ).mul(BORE_FOAM_SEAWARD_SHIFT)
      const foamCenter = phase.center.add(boreSeawardShift)
      const foamFrontW = bw.mul(1.6).mul(mix(float(1), float(4), breakSpread))
      // Keep the foam footprint at a 70:30 front/back width ratio through
      // both the narrow approach and the breaking expansion.
      const foamBackW = foamFrontW.mul(3 / 7)
      const frontBiasedMask = float(1).sub(
        smoothstep(foamCenter, foamCenter.add(foamBackW), noisyD)
      )
      const foamProfile = smoothstep(
        foamCenter.sub(foamFrontW),
        foamCenter,
        noisyD
      ).mul(frontBiasedMask)
      const band = foamProfile
        .mul(phase.fade)
        .mul(
          smoothstep(
            float(0.2),
            float(0.5),
            sampleNoise(vOrigWorldPos.xz.mul(0.15).add(phase.center.mul(1.5)))
          )
        )
      // Breaking whitecap: rides the crest top; as the break completes
      // (right before the run-up) the cap densifies — its mask widens
      // down the swell faces (falling pow) and its texture becomes denser,
      // while the gain below keeps the widened bore from reaching solid
      // white across its whole footprint.
      const crest = pow(foamProfile, mix(float(2.0), float(1.3), phase.brk))
        .mul(phase.brk)
        .mul(phase.fade)
      const foamTex = foamMapTex.sample(
        vOrigWorldPos.xz.mul(0.4).add(phase.cycle.mul(0.3))
      ).r
      const crestTex = mix(foamTex, foamTex.mul(0.45).add(0.55), phase.brk)
      // The widened breaker/bore covers much more area than the approaching
      // band, so taper its peak intensity as it spreads instead of letting
      // that whole footprint reach solid white.
      const breakFoamGain = mix(float(1), float(0.6), phase.brk)
      const liveFoam = max(
        band.mul(mix(float(0.3), float(1.0), phase.brk)).mul(foamTex),
        crest.mul(crestTex).mul(float(1).add(phase.brk.mul(0.2)))
      ).mul(breakFoamGain)

      const resSpatial = float(1).sub(
        smoothstep(float(0.3), float(0.6), noisyD)
      )
      // Residue must be gone by ~0.85: the OTHER phase (offset 0.5) has
      // visibly re-entered the swash zone by cycle ≈ 0.87, and a fresh
      // wave washing over the previous wave's leftovers reads wrong.
      const resTime = smoothstep(float(0.52), float(0.6), phase.cycle).mul(
        float(1).sub(smoothstep(float(0.72), float(0.86), phase.cycle))
      )
      const dissolve = smoothstep(float(0.6), float(0.84), phase.cycle)
      // Backwash drag: the pattern is sampled landward of the fragment
      // by a growing offset, so the leftover foam visibly slides seaward
      // with the receding sheet…
      const resTex = foamMapTex.sample(
        vOrigWorldPos.xz
          .add(landwardDir.mul(phase.receded.mul(2.0)))
          .mul(0.4)
          .add(vec2(phase.seed.mul(7.3), phase.seed.mul(4.1)))
      ).r
      // Residue is anchored to the shore after the crest has passed, so it
      // must not be clipped by the moving crest's front/back mask. A slightly
      // lower dissolve threshold keeps sparse patches present without adding
      // a differently shaped fallback band.
      const resThr = mix(float(0.28), float(0.72), dissolve)
      const residue = resSpatial
        .mul(resTime)
        .mul(smoothstep(resThr, resThr.add(0.2), resTex))
        .mul(0.55)
      // …and dims as it goes (color only — `residue` keeps the alpha).
      const residueLit = residue.mul(mix(float(1), float(0.35), phase.receded))

      // Whitewash sheet: the front rides the broken crest the rest of
      // the way in, sweeps past the waterline at full run-up, and the
      // flush pulls it back out seaward. Lives in the SAME noisyD
      // coordinate as the swell so the band snakes along the crest
      // wiggle for wiggle, sitting just landward of the peak — the foam
      // forms on the breaker's front face, never behind it. A NARROW
      // band riding the front, widening a little as the run-up spreads
      // it over the strip, hard-capped seaward at the break point.
      // Position and texture advection use linear travel progress: the eased
      // phase nodes intentionally slow at their endpoints, which made the
      // foam appear parked from late RUN-UP through early FLUSH.
      const runupTravel = clamp(
        phase.cycle
          .sub(WASH_RUNUP_START)
          .div(WASH_FLUSH_START - WASH_RUNUP_START),
        0.0,
        1.0
      )
      const flushTravel = clamp(
        phase.cycle
          .sub(WASH_FLUSH_START)
          .div(WASH_FLUSH_END - WASH_FLUSH_START),
        0.0,
        1.0
      )
      const washFront = mix(
        phase.center.sub(WASH_PEAK_OFFSET),
        float(WASH_OVERSHOOT),
        runupTravel
      )
        .add(
          runupTravel
            .mul(float(1).sub(flushTravel))
            .mul(WASH_RUNUP_SEAWARD_SHIFT)
        )
        .add(flushTravel.mul(WASH_FLUSH_RETREAT))
        .toVar()
      const washBack = min(
        washFront.add(
          float(0.12).add(phase.brk.mul(0.1)).add(phase.runup.mul(0.2))
        ),
        float(WASH_BACK_DEPTH)
      )
      const washBand = smoothstep(
        washFront.sub(0.02),
        washFront.add(0.05),
        noisyD
      ).mul(float(1).sub(smoothstep(washBack, washBack.add(0.12), noisyD)))
      // Use the same linear progress for the texture so its internal pattern
      // stays locked to the moving band instead of pausing at the handoff.
      const washTex = foamMapTex.sample(
        vOrigWorldPos.xz
          .add(landwardDir.mul(flushTravel.mul(2.2).sub(runupTravel.mul(1.2))))
          .mul(0.35)
          .add(vec2(phase.seed.mul(5.7), phase.seed.mul(3.9)))
      ).r
      // Preserve texture holes from the start of run-up: a moderate threshold
      // rejects dark texels, while the small solid floor prevents the sheet
      // from becoming a flat white plate. FLUSH still raises the threshold to
      // shred the remaining foam into streaks.
      const runupWashThr = mix(float(0.3), float(0.42), runupTravel)
      const washThr = mix(runupWashThr, float(0.75), phase.flush)
      const washFade = float(1).sub(phase.flush)
      const washSolid = washFade.mul(mix(float(0.12), float(0.05), runupTravel))
      const wash = washBand
        .mul(phase.brk)
        .mul(washFade.mul(washFade))
        .mul(
          smoothstep(washThr, washThr.add(0.22), washTex)
            .mul(float(1).sub(washSolid))
            .add(washSolid)
        )
        .mul(frontBiasedMask)
        .mul(breakFoamGain)

      return { live: liveFoam, residue, residueLit, wash }
    }
    const shoreDayNight = smoothstep(float(-0.05), float(0.1), sunY)
    const foamA = buildPhaseFoam(phaseA, noisyDA)
    const foamB = buildPhaseFoam(phaseB, noisyDB)
    const residueFoam = max(foamA.residue, foamB.residue).toVar()
    const residueLitFoam = max(foamA.residueLit, foamB.residueLit).toVar()
    // Whitewash composites like the residue — an independent layer with
    // its own alpha — so the run-up sheet can ride ahead of the swash
    // water and the flushing sheet stays visible on the draining strip.
    const washFoam = max(foamA.wash, foamB.wash).toVar()
    // Residue is NOT folded into the water foam: it composites as an
    // independent layer after the alpha, so it stays visible on the bare
    // sand once the swash water has drained out from under it.
    const waveFoam = max(foamA.live, foamB.live)

    // Receding-waterline foam: a dim ragged line riding the sheet's
    // seaward-moving edge through the backwash, gone once the strip has
    // fully drained. Its texture shares the backwash drag so the streaks
    // pull seaward with the edge.
    const recedeFadeOut = (phase: ReturnType<typeof buildShoreWavePhase>) =>
      phase.receded.mul(
        float(1).sub(
          smoothstep(
            float(RECEDE_END - 0.04),
            float(RECEDE_END + 0.03),
            phase.cycle
          )
        )
      )
    const buildRecedeEdgeFoam = (
      phase: ReturnType<typeof buildShoreWavePhase>,
      noisyD: N,
      swashThr: N
    ) =>
      smoothstep(swashThr.sub(0.06), swashThr, noisyD)
        .mul(
          float(1).sub(
            smoothstep(swashThr.add(0.03), swashThr.add(0.16), noisyD)
          )
        )
        .mul(recedeFadeOut(phase))
        .mul(
          foamMapTex.sample(
            vOrigWorldPos.xz
              .add(landwardDir.mul(phase.receded.mul(2.0)))
              .mul(0.45)
              .add(vec2(phase.seed.mul(4.7), phase.seed.mul(8.3)))
          ).r
        )
    const recedeEdgeFoam = max(
      buildRecedeEdgeFoam(phaseA, noisyDA, swashThrA),
      buildRecedeEdgeFoam(phaseB, noisyDB, swashThrB)
    )
      .mul(seaFxGate)
      .toVar()
    const foamWithTex = clamp(waveFoam, 0.0, 1.0)
    const foamDepthMask = float(1)
      .sub(smoothstep(float(0.3), float(0.7), depthFactorSea))
      .mul(0.7)
      .add(0.3)
    const foamAddStrength = mix(float(0.06), float(0.7), shoreDayNight)
    const foamAdd = vec3(1, 1, 1).mul(
      foamWithTex.mul(foamAddStrength).mul(foamDepthMask)
    )

    // ── Pixel-depth foam edge: hugs every water↔opaque contact line —
    // the beach, rocks, and players' legs alike. ──
    const foamEdgeMask = pixelDepth
      ? float(1)
          .sub(smoothstep(float(0.02), float(0.35), vertDepth))
          .mul(smoothstep(float(0.0), float(0.02), vertDepth))
      : float(0)
    const edgeFoamTex = foamMapTex.sample(
      vOrigWorldPos.xz.mul(0.45).add(vec2(uTime.mul(0.008), uTime.mul(0.005)))
    ).r
    const edgeNoise = sampleNoise(
      vOrigWorldPos.xz.mul(0.35).add(uTime.mul(0.05))
    )
    // No standing contact foam on the beach waterline: the shore's
    // boundary foam is owned entirely by the wave cycle (whitecap →
    // residue → receding edge line). Contact lines in deeper water —
    // rocks, players' legs — keep it.
    const swashEdgeSuppress = float(1).sub(
      float(1)
        .sub(
          max(
            smoothstep(float(0.45), float(0.6), noisyDA),
            smoothstep(float(0.45), float(0.6), noisyDB)
          )
        )
        .mul(seaFxGate)
    )
    const foamEdge = foamEdgeMask
      .mul(edgeFoamTex)
      .mul(smoothstep(float(0.25), float(0.7), edgeNoise).mul(0.7).add(0.3))
      .mul(mix(float(0.3), float(1.0), shoreDayNight))
      .mul(mix(float(0.25), float(1.0), seaFxGate))
      .mul(swashWaterGate)
      .mul(swashEdgeSuppress)
      .toVar()

    // ── Torch reflection (was river-only; depth-gated so it stays subtle) ──
    const torchVec = uTorchPos.sub(vWorldPos)
    const torchLen = length(torchVec)
    const torchAtten = pow(
      max(float(0), float(1).sub(torchLen.div(uTorchDistance))),
      float(2)
    )
    const torchDir = torchVec.div(max(torchLen, float(0.001)))
    const torchSpecNormal = normalize(mix(vec3(0, 1, 0), surfaceNormal, 0.75))
    const torchHalfDir = normalize(torchDir.add(viewDir))
    const torchNdotH = max(dot(torchSpecNormal, torchHalfDir), 0.0)
    const torchReflection = uTorchColor.rgb
      .mul(pow(torchNdotH, float(48)))
      .mul(torchAtten.mul(torchAtten))
      .mul(uTorchIntensity)
      .mul(0.004)
      .mul(depthFactor)

    // ── Fresnel base ──
    const fresnelViewDir = normalize(vec3(viewDir.x, float(0.15), viewDir.z))
    const NdotV = max(dot(surfaceNormal, fresnelViewDir), 0.0)
    const rippleBright = mix(
      mix(float(0.75), float(0.85), riverness),
      mix(float(1.25), float(1.2), riverness),
      pow(float(1).sub(NdotV), float(1.5))
    )
    waterColor.mulAssign(rippleBright)

    // ── Sea composite ──
    const tintedSkyReflection = mix(
      skyReflection,
      uMidColor.rgb.mul(1.3),
      float(0.7)
    )
    const shallowDamp = smoothstep(float(0.1), float(0.4), depthFactorSea)
    const seaFresnel = pow(float(1).sub(NdotV), float(2)).mul(0.08)
    const colorSea = mix(
      waterColor,
      tintedSkyReflection,
      mix(float(0.005), float(0.06), shallowDamp).add(seaFresnel)
    )
      .add(specular.mul(shallowDamp))
      .toVar()
    // Cloud overlay on the deep body, gated OUT of the emerald shallows
    // where the photo's blue-sky pedestal would push the coastal green
    // toward blue; the complement window below re-adds only the BRIGHT
    // cloud shapes there as a pure-white lift so the green survives.
    const cloudDepthGate = smoothstep(float(0.25), float(0.55), depthFactorSea)
    colorSea.assign(
      mix(
        colorSea,
        cloudColor,
        cloudWeight.mul(
          cloudDepthGate.mul(mix(float(0.18), float(0.65), reflectionMixScale))
        )
      )
    )
    const cloudLumMax = max(cloudColor.r, max(cloudColor.g, cloudColor.b))
    const cloudBrightMask = smoothstep(float(0.4), float(0.95), cloudLumMax)
    const highlightBandGate = smoothstep(
      float(0.05),
      float(0.15),
      depthFactorSea
    )
      .mul(float(1).sub(cloudDepthGate))
      .mul(riverProxGate)
    colorSea.assign(
      mix(
        colorSea,
        vec3(1, 1, 1),
        cloudWeight
          .mul(
            highlightBandGate.mul(
              mix(float(0.08), float(0.25), reflectionMixScale)
            )
          )
          .mul(cloudBrightMask)
      )
    )
    colorSea.assign(
      mix(
        colorSea,
        reflectionSample.rgb,
        reflectionSample.a.mul(0.3).mul(reflectionMixScale)
      )
    )
    const nightDarken = smoothstep(float(-0.05), float(0.1), sunY)
      .mul(0.75)
      .add(0.25)
    const midDepthWeight = smoothstep(
      float(0.15),
      float(0.35),
      depthFactorSea
    ).mul(float(1).sub(smoothstep(float(0.5), float(0.8), depthFactorSea)))
    const nightExtra = float(1).sub(
      float(1).sub(nightDarken).mul(midDepthWeight).mul(0.35)
    )
    colorSea.mulAssign(nightDarken.mul(nightExtra))
    colorSea.addAssign(foamAdd.mul(seaFxGate))

    // ── River composite ──
    const riverFresnel = pow(float(1).sub(NdotV), float(2)).mul(0.5)
    const reflectionBase = mix(
      float(0.24),
      float(0.05),
      riverRefrShallow.mul(0.9)
    )
    const depthReflLift = smoothstep(float(0.04), float(0.35), depthFactorRiver)
    const reflectionMixRiver = clamp(
      reflectionBase
        .add(riverFresnel)
        .add(depthReflLift.mul(twilightFactor).mul(0.18))
        .add(depthReflLift.mul(nightFactor).mul(0.28))
        .mul(mix(float(0.58), float(1.0), reflectionMixScale)),
      0.0,
      0.9
    )
    const colorRiver = mix(waterColor, skyReflection, reflectionMixRiver)
      .add(specular.mul(depthFactorRiver))
      .toVar()
    colorRiver.assign(
      mix(
        colorRiver,
        reflectionSample.rgb,
        reflectionSample.a.mul(0.3).mul(reflectionMixScale)
      )
    )
    const riverNightLuma = dot(colorRiver, LUMA_REC601)
    const riverNightMuted = mix(
      colorRiver,
      vec3(riverNightLuma),
      nightFactor.mul(0.28)
    ).mul(float(1).sub(nightFactor.mul(0.14)))
    colorRiver.assign(mix(colorRiver, riverNightMuted, nightFactor))

    // ── Blend + global additions ──
    const color = mix(colorSea, colorRiver, riverStyleW).toVar()
    color.addAssign(torchReflection)
    color.addAssign(vec3(1, 1, 1).mul(foamEdge.mul(0.8)))
    const residueTint = RESIDUE_DEBUG
      ? vec3(1.5, 0, 0)
      : vec3(1, 1, 1).mul(foamAddStrength)
    color.addAssign(
      residueTint
        .mul(RESIDUE_DEBUG ? residueFoam : residueLitFoam)
        .mul(seaFxGate)
    )
    color.addAssign(
      vec3(1, 1, 1).mul(foamAddStrength).mul(washFoam).mul(seaFxGate)
    )
    // Receding edge line: darker than the live foam — thin dirty water,
    // not fresh whitewash.
    color.addAssign(
      vec3(0.5, 0.52, 0.55)
        .mul(recedeEdgeFoam)
        .mul(mix(float(0.15), float(0.6), shoreDayNight))
    )
    if (SWELL_PROFILE_DEBUG) {
      const debugTotal = swellDebugFront.add(swellDebugBack)
      const debugMask = clamp(debugTotal, 0.0, 1.0).mul(seaFxGate)
      const debugColor = vec3(swellDebugFront, 0, swellDebugBack).div(
        max(debugTotal, float(0.0001))
      )
      color.assign(mix(color, debugColor, debugMask))
    }

    // ── Alpha ──
    // Sea: high floor (the bed shows through the refraction color, not
    // through framebuffer transparency — that's what killed the old
    // floating-sheet look), mild receding-wave modulation for the wash.
    const seaAlpha = max(
      mix(
        float(0.55),
        float(0.92),
        smoothstep(float(0.0), float(0.25), depthFactorSea)
      ),
      seaRefrMix.mul(refractionMixScale).mul(0.9)
    )
      .add(foamWithTex.mul(seaFxGate).mul(0.9))
      .add(sparkle)
      .min(1.0)
      .mul(
        mix(
          float(1.0),
          mix(float(0.75), float(1.0), shore.holeAlpha),
          seaFxGate
        )
      )
      .toVar()
    // Night alpha reduction for very shallow sea (as before, gentler).
    const veryShallowWeight = float(1).sub(
      smoothstep(float(0.0), float(0.08), depthFactorSea)
    )
    seaAlpha.mulAssign(
      float(1).sub(float(1).sub(nightDarken).mul(veryShallowWeight).mul(0.35))
    )
    // Fully drain the swash strip between waves (bare wet sand shows).
    seaAlpha.mulAssign(swashWaterGate)

    // River: bank-to-body ramp; deep water goes opaque at night so a
    // torch-lit bed doesn't bleed through the last few percent.
    // Near-zero floor with a 5 cm dead zone keeps water off the bank slope.
    // Fade it in over just 10 cm so a character does not reveal a long,
    // semi-transparent strip before reaching the actual channel depth.
    const riverAlpha = mix(
      float(0.02),
      float(0.95),
      smoothstep(float(0.05), float(0.15), depth)
    ).toVar()
    riverAlpha.assign(
      mix(riverAlpha, float(1.0), float(1).sub(dayFactor).mul(depthFactorRiver))
    )

    // River ramp by riverStyleW, not riverness: at the mouth the estuary
    // gate zeroes riverness while the bank-taper sheet still hugs the
    // grassy banks, and the sea path's 0.55 floor (with swash drained off
    // by seaFxGate there) would render that sheet as standing floodwater.
    const alpha = mix(seaAlpha, riverAlpha, riverStyleW)
      .add(foamEdge.mul(0.6))
      .add(residueFoam.mul(seaFxGate))
      .add(washFoam.mul(seaFxGate))
      .add(recedeEdgeFoam.mul(0.5))
      .min(1.0)
      .toVar()

    // ── Shoreline edge cut ──
    // pixelDepth: ~6 cm soft blend against the opaque depth buffer —
    // pixel-exact around terrain AND entities. The analytic heightmap
    // edge is multiplied in as well: where water and terrain are
    // near-coplanar (sandbar tips), the depth-test clip line zigzags
    // per-triangle, and only the bilinear heightmap contour is smooth —
    // fading alpha on it hides the jagged geometric cut. Entities aren't
    // in the heightmap, so their contact stays purely pixel-depth.
    // Fallback: analytic contact only — coarser, but never floats.
    const edgeBlend = pixelDepth
      ? smoothstep(float(0.0), float(0.06), vertDepth).mul(
          smoothstep(float(0.0), float(0.04), depth)
        )
      : smoothstep(float(0.0), float(0.02), depth)
    alpha.mulAssign(edgeBlend)

    return vec4(color, alpha)
  })()

  // ── Build Material ────────────────────────────────────

  const material = new NodeMaterial()
  material.transparent = true
  material.depthWrite = false
  // DoubleSide: river reaches slope downstream; under the tilted ortho
  // camera a FrontSide-only surface can open holes on steep drops.
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
      uRefractionMap: refractionTex,
      uReflectionMap: reflectionTex,
      uHeightmapTexture: heightmapTex,
      uWaterField: waterFieldTex,
      uNormalMap: normalMapTex,
      uFoamMap: foamMapTex,
      uCausticsMap: causticsTex,
      uWetnessMap: wetnessMapTex,
      uWaveA,
      uWaveB,
      uWaveC,
    },
  }
}

// ── Wetness capture material ─────────────────────────────
// The wetness pre-pass needs the shore mask's `holeAlpha` rendered from
// above into a per-tile RT. The old system reused the sea material with a
// `uCaptureMode` switch + a wave-steepness save/restore dance; a dedicated
// material is cheaper (one tiny pipeline, compiled once, shared across all
// tiles) and keeps depth-buffer nodes out of the capture RT, which has no
// depth texture to copy. Per-tile textures are swapped in via `.value`
// before each capture render.

export interface WaterCaptureMaterialResult {
  material: NodeMaterial
  uniforms: {
    uTime: { value: number }
    uHeightmapTexture: { value: THREE.Texture }
    uWaterField: { value: THREE.Texture }
  }
}

let _captureMaterial: WaterCaptureMaterialResult | null = null

export function getWaterCaptureMaterial(): WaterCaptureMaterialResult {
  if (_captureMaterial) return _captureMaterial

  const uTime = uniform(0)
  const heightmapTex = texture(waterHeightFallbackTex)
  const waterFieldTex = texture(waterFieldFallbackTex)
  const noiseTex = texture(getNoiseTexture())

  const vWorldPos = varying(vec3(0), 'wc_worldPos')
  const vUv = varying(vec2(0), 'wc_uv')

  const positionNode = Fn(() => {
    vUv.assign(uv())
    const worldPos = modelWorldMatrix.mul(vec4(positionLocal, 1.0))
    vWorldPos.assign(worldPos.xyz)
    return cameraProjectionMatrix.mul(cameraViewMatrix).mul(worldPos)
  })()

  const fragmentNode = Fn(() => {
    const sampleUV = clamp(toHeightmapUV(vUv), 0.0, 1.0)
    const bedHeight = heightmapTex.sample(sampleUV).r
    const depth = max(float(0), vWorldPos.y.sub(bedHeight))
    const sampleNoise = (noiseCoord: N) =>
      noiseTex.sample(noiseCoord.div(NOISE_PERIODS)).r
    const shore = buildShoreMaskNodes(depth, vWorldPos, uTime, sampleNoise)
    // Rivers don't wet the sand (their banks have no wash animation), so
    // the captured alpha is gated out near river influence — riverness
    // for inland channels, flow magnitude for mouths (same pair of gates
    // as the main material's seaFxGate).
    const fieldSample = waterFieldTex.sample(sampleUV)
    const proxGate = float(1).sub(
      smoothstep(float(0.02), float(0.2), length(fieldSample.gb))
    )
    return vec4(
      0,
      0,
      0,
      shore.holeAlpha.mul(float(1).sub(fieldSample.a)).mul(proxGate)
    )
  })()

  const material = new NodeMaterial()
  material.transparent = true
  material.blending = THREE.NoBlending
  material.depthWrite = false
  material.side = THREE.DoubleSide
  material.vertexNode = positionNode
  material.fragmentNode = fragmentNode

  _captureMaterial = {
    material,
    uniforms: {
      uTime,
      uHeightmapTexture: heightmapTex,
      uWaterField: waterFieldTex,
    },
  }
  return _captureMaterial
}

import * as THREE from 'three'
import { MeshStandardNodeMaterial } from 'three/webgpu'
import {
  uniform,
  vec2,
  vec3,
  float,
  sin,
  cos,
  mix,
  smoothstep,
  positionLocal,
  instanceIndex,
  hash,
  attribute,
  texture,
  floor,
} from 'three/tsl'
import { loadGLB } from '../utils/gltfCache'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any // TSL node -- broad type for shader node expressions

// ── Grass billboard geometry from GLB ─────────────────────
// Loads grassLODs.glb and extracts the named LOD mesh geometry.
// UV y is flipped so that y=0 at base, y=1 at tip (matching our shader convention).

async function loadLODGeometry(
  lodName: string,
  url = '/models/grassLODs.glb',
  scale: number | [number, number, number] = 5
): Promise<THREE.BufferGeometry> {
  const gltf = await loadGLB(url)
  let found: THREE.BufferGeometry | null = null
  gltf.scene.traverse((child) => {
    if (child instanceof THREE.Mesh && child.name.includes(lodName)) {
      found = child.geometry
    }
  })
  if (!found) {
    throw new Error(`${lodName} mesh not found in ${url}`)
  }
  // Clone to avoid mutating the cached GLTF geometry
  const geometry = (found as THREE.BufferGeometry).clone()
  const [sx, sy, sz] = Array.isArray(scale) ? scale : [scale, scale, scale]
  geometry.scale(sx, sy, sz)

  // Flip UV y: GLB has y=1 at base, y=0 at tip → our convention y=0 base, y=1 tip
  const uvAttr = geometry.getAttribute('uv')
  if (uvAttr) {
    for (let i = 0; i < uvAttr.count; i++) {
      uvAttr.setY(i, 1 - uvAttr.getY(i))
    }
    uvAttr.needsUpdate = true
  }

  return geometry
}

export function loadGrassBillboardGeometry(
  url = '/models/grassLODs.glb',
  scale = 5
): Promise<THREE.BufferGeometry> {
  return loadLODGeometry('LOD01', url, scale)
}

export function loadFlowerBillboardGeometry(
  url = '/models/grassLODs.glb',
  scale: number | [number, number, number] = [3, 5.5, 3]
): Promise<THREE.BufferGeometry> {
  return loadLODGeometry('LOD02', url, scale)
}

const textureLoader = new THREE.TextureLoader()

export function loadAlphaTexture(url: string): Promise<THREE.Texture> {
  return textureLoader.loadAsync(url)
}

export const loadGrassAlphaTexture = () =>
  loadAlphaTexture('/textures/grass1.jpeg')
export const loadFlowerColorTexture = () =>
  loadAlphaTexture('/textures/flowerx4.png')

// ── Splatmap R-channel vegetation subtype ranges ─────────
export const SHORT_GRASS_R_MIN = 230
export const SHORT_GRASS_R_MAX = 239
export const TALL_GRASS_R_MIN = 240
export const TALL_GRASS_R_MAX = 249

// ── Wind state snapshot (shared with particle systems) ───
export interface WindState {
  windDirX: number
  windDirZ: number
  /** Wind strength multiplier (0.3 .. 1.0) */
  windStrength: number
  time: number
}

// ── TSL grass material ───────────────────────────────────

export const GRASS_TRAIL_COUNT = 5

export const GUST_WAVE_COUNT = 3

export interface GrassMaterialUniforms {
  uTime: { value: number }
  uWindStrength: { value: number }
  uWindFrequency: { value: number }
  /** Normalized wind direction (x, z) */
  uWindDir: { value: THREE.Vector2 }
  /** Gust envelope (0 = calm, 1 = full gust), controlled by JS state machine */
  uGustStrength: { value: number }
  /** Per-wave direction angle (radians) */
  uWaveAngles: { value: number }[]
  /** Per-wave amplitude envelope (0 = silent, 1 = full) */
  uWaveAmps: { value: number }[]
  /** Per-wave params: vec4(freq, speed, amp, Q) */
  uWaveParams: { value: THREE.Vector4 }[]
  /** vec3(worldX, worldZ, strength) per trail point */
  uTrail: { value: THREE.Vector3 }[]
  uInteractionRadius: { value: number }
  uInteractionStrength: { value: number }
}

export interface GrassMaterialConfig {
  baseColor?: [number, number, number]
  tipColor?: [number, number, number]
  windStrength?: number
  windFrequency?: number
  widthScaleMin?: number
  widthScaleExtent?: number
  heightScaleMin?: number
  heightScaleExtent?: number
  interactionRadius?: number
  interactionStrength?: number
  alphaMap?: THREE.Texture
  /** Color texture for the billboard. When set, the texture color is used
   *  directly and alpha is derived from the texture. */
  colorMap?: THREE.Texture
  /** Atlas grid size (e.g. 2 for a 2×2 atlas). Each instance randomly picks
   *  one sub-tile by offsetting UVs. Only used with colorMap. */
  atlasGrid?: number
}

export const TALL_GRASS_CONFIG: GrassMaterialConfig = {
  baseColor: [0.012, 0.035, 0.01],
  tipColor: [0.04, 0.09, 0.02],
  windStrength: 0.12,
  widthScaleMin: 0.6,
  widthScaleExtent: 0.6,
  heightScaleMin: 0.7,
  heightScaleExtent: 0.6,
  interactionRadius: 2.0,
  interactionStrength: 0.35,
}

export const FLOWER_CONFIG: GrassMaterialConfig = {
  baseColor: [0.02, 0.06, 0.015],
  tipColor: [0.06, 0.12, 0.03],
  windStrength: 0.04,
  widthScaleMin: 0.8,
  widthScaleExtent: 0.5,
  heightScaleMin: 0.6,
  heightScaleExtent: 0.5,
  interactionRadius: 1.5,
  interactionStrength: 0.12,
  atlasGrid: 2,
}

/**
 * Per-instance world position attribute name.
 * Each InstancedMesh must have an InstancedBufferAttribute with this name
 * containing vec2 (worldX, worldZ) per instance.
 */
export const GRASS_INSTANCE_POS_ATTR = 'aInstanceWorldXZ'
export const GRASS_INSTANCE_ROT_ATTR = 'aInstanceRotation'

export function createGrassMaterial(cfg?: GrassMaterialConfig): {
  material: MeshStandardNodeMaterial
  uniforms: GrassMaterialUniforms
} {
  const bc = cfg?.baseColor ?? [0.015, 0.04, 0.008]
  const tc = cfg?.tipColor ?? [0.06, 0.14, 0.03]
  const ws = cfg?.windStrength ?? 0.06
  const wf = cfg?.windFrequency ?? 2.0
  const wsMin = cfg?.widthScaleMin ?? 0.7
  const wsExt = cfg?.widthScaleExtent ?? 0.7
  const hsMin = cfg?.heightScaleMin ?? 0.8
  const hsExt = cfg?.heightScaleExtent ?? 0.4
  const ir = cfg?.interactionRadius ?? 1.5
  const is = cfg?.interactionStrength ?? 0.15

  const uTime = uniform(0)
  const uWindStrength = uniform(ws)
  const uWindFrequency = uniform(wf)
  const uWindDir = uniform(new THREE.Vector2(1, 0))
  const uGustStrength = uniform(0)
  const uWaveAngles = [uniform(0), uniform(0.4), uniform(-0.3)]
  const uWaveAmps = [uniform(1), uniform(1), uniform(1)]
  const uWaveParams = [
    uniform(new THREE.Vector4(0.35, 0.7, 1.5, 0.75)),
    uniform(new THREE.Vector4(0.31, 0.8, 1.6, 0.87)),
    uniform(new THREE.Vector4(0.39, 1.5, 1.7, 0.95)),
  ]
  const uInteractionRadius = uniform(ir)
  const uInteractionStrength = uniform(is)

  // 5 individual trail point uniforms: vec3(worldX, worldZ, strength)
  const uTrail = Array.from({ length: GRASS_TRAIL_COUNT }, () =>
    uniform(new THREE.Vector3(0, 0, 0))
  )

  const mat = new MeshStandardNodeMaterial()
  mat.side = THREE.DoubleSide
  mat.roughness = 0.8
  mat.metalness = 0.0

  // ── Atlas UV: pick a random sub-tile per instance for atlas textures ──
  let texUV: N | undefined
  if (cfg?.colorMap && cfg.atlasGrid && cfg.atlasGrid > 1) {
    const grid = cfg.atlasGrid
    const totalCells = float(grid * grid)
    const atlasHash = hash(vec2(instanceIndex.toFloat().mul(0.19), float(7.3)))
    const idx = floor(atlasHash.mul(totalCells).min(totalCells.sub(0.001)))
    const invGrid = float(1 / grid)
    const col = idx.sub(floor(idx.mul(invGrid)).mul(float(grid)))
    const row = floor(idx.mul(invGrid))
    const origUV = attribute('uv')
    texUV = vec2(
      origUV.x.mul(invGrid).add(col.mul(invGrid)),
      origUV.y.mul(invGrid).add(row.mul(invGrid))
    )
  }

  // ── Alpha map (billboard texture) ──
  const colorTexNode = cfg?.colorMap
    ? texUV
      ? texture(cfg.colorMap, texUV)
      : texture(cfg.colorMap)
    : undefined

  if (colorTexNode) {
    mat.transparent = true
    mat.alphaTest = 0.1
    mat.opacityNode = colorTexNode.a
  } else if (cfg?.alphaMap) {
    mat.transparent = true
    mat.alphaTest = 0.1
    const alphaTexNode = texture(cfg.alphaMap)
    mat.opacityNode = alphaTexNode.r
  }

  // ── Per-instance attributes ──
  const instanceWorldXZ = attribute(GRASS_INSTANCE_POS_ATTR, 'vec2')
  const instanceRotation = attribute(GRASS_INSTANCE_ROT_ATTR, 'float')

  // ── Color: base → tip gradient with per-instance variation ──
  const baseColor = vec3(bc[0], bc[1], bc[2])
  const tipColor = vec3(tc[0], tc[1], tc[2])
  const uvY = attribute('uv').y
  const gradientColor = mix(
    baseColor,
    tipColor,
    smoothstep(float(0), float(0.8), uvY)
  )

  // Root darkening (AO): darken the bottom of each blade
  const rootAO = mix(
    float(0.45),
    float(1.0),
    smoothstep(float(0), float(0.35), uvY)
  )

  // Per-instance hue/brightness variation via hashes of instanceIndex
  const brightnessHash = hash(
    vec2(instanceIndex.toFloat().mul(0.37), float(1.7))
  )
  const brightness = float(0.85).add(brightnessHash.mul(0.3)) // 0.85 ~ 1.15

  let finalColor: N
  if (colorTexNode) {
    // Use color texture directly, with per-instance brightness variation
    finalColor = colorTexNode.rgb.mul(brightness).mul(rootAO)
  } else {
    // Slight yellow-green ↔ blue-green hue shift per instance
    const hueHash = hash(vec2(instanceIndex.toFloat().mul(0.73), float(3.1)))
    const hueShift = vec3(
      float(1.0).add(hueHash.sub(0.5).mul(0.15)),
      float(1.0),
      float(1.0).add(hueHash.sub(0.5).mul(-0.1))
    )
    finalColor = gradientColor.mul(brightness).mul(hueShift).mul(rootAO)
  }
  mat.colorNode = finalColor

  // Do NOT set normalNode — the geometry normals (0,1,0) will be
  // automatically transformed to view-space by the default pipeline.
  // Setting normalNode directly treats it as view-space which breaks lighting.

  // ── Per-instance shape variation: width & height ──
  const shapeHash1 = hash(vec2(instanceIndex.toFloat().mul(0.53), float(2.3)))
  const shapeHash2 = hash(vec2(instanceIndex.toFloat().mul(0.91), float(4.7)))
  const widthScale = float(wsMin).add(shapeHash1.mul(wsExt))
  const heightScale = float(hsMin).add(shapeHash2.mul(hsExt))

  // ── Vertex displacement ──
  const rawPos = positionLocal.toVar()
  // Apply per-instance shape variation (width x, height y)
  const localPosX = rawPos.x.mul(widthScale)
  const localPosY = rawPos.y.mul(heightScale)
  const localPosZ = rawPos.z.mul(widthScale)

  const instanceHash = hash(vec2(instanceIndex.toFloat().mul(0.1), float(0.5)))
  const phaseOffset = instanceHash.mul(6.283)

  const heightFactor = uvY.mul(uvY)

  // ── Directional wind: inverse-rotate into blade local space ──
  const cosR = cos(instanceRotation)
  const sinR = sin(instanceRotation)
  const localWindX = uWindDir.x.mul(cosR).sub(uWindDir.y.mul(sinR))
  const localWindZ = uWindDir.x.mul(sinR).add(uWindDir.y.mul(cosR))

  // ── Gerstner wave gusts ──────────────────────────────────
  // Per-wave params (freq, speed, amp, Q) and direction are all uniform-driven.
  // Gerstner phase distortion (phase + Q*sin(phase)) creates sharp crests (fast gust onset)
  // and broad troughs (slow recovery) — naturally asymmetric.
  let gust: N = float(0)
  for (let wi = 0; wi < GUST_WAVE_COUNT; wi++) {
    const wp = uWaveParams[wi] // vec4(freq, speed, amp, Q)
    const wFreq = wp.x
    const wSpeed = wp.y
    const wAmp = wp.z
    const wQ = wp.w

    // Per-wave direction from uniform angle
    const wAngle = uWaveAngles[wi]
    const cOff = cos(wAngle)
    const sOff = sin(wAngle)
    const wDirX = uWindDir.x.mul(cOff).sub(uWindDir.y.mul(sOff))
    const wDirZ = uWindDir.x.mul(sOff).add(uWindDir.y.mul(cOff))

    // Spatial phase along wave direction
    const spatial = instanceWorldXZ.x
      .mul(wDirX)
      .add(instanceWorldXZ.y.mul(wDirZ))

    // Perpendicular coordinate for wavefront warping
    const perp = instanceWorldXZ.x
      .mul(wDirZ.negate())
      .add(instanceWorldXZ.y.mul(wDirX))
    const warp = sin(perp.mul(0.15)).mul(2.5)

    const phase = spatial.mul(wFreq).add(warp).sub(uTime.mul(wSpeed))

    // Gerstner phase distortion: bunches crests, spreads troughs
    const gerstnerPhase = phase.add(wQ.mul(sin(phase)))

    // Wave value mapped to [0, 1], scaled by per-wave amplitude envelope
    const waveVal = cos(gerstnerPhase).add(1).mul(0.5)
    gust = gust.add(waveVal.mul(wAmp).mul(uWaveAmps[wi]))
  }

  // Always-active baseline ripple + JS-controlled gust boost
  gust = gust.mul(float(0.15).add(uGustStrength.mul(0.85)))

  // ── Circular bending: combine wind + idle sway into a bend angle ──
  // Wind bend angle (base lean + gust modulation)
  const windBendAngle = uWindStrength.mul(5.0).mul(float(1.0).add(gust))
  const bendFromWindX = localWindX.mul(windBendAngle)
  const bendFromWindZ = localWindZ.mul(windBendAngle)

  // Idle sway: per-instance random direction, gentle in-place oscillation
  const idleSwayAngle = sin(uTime.mul(uWindFrequency).add(phaseOffset)).mul(
    uWindStrength
  )
  const idleDirAngle = phaseOffset // random direction per instance
  const idleBendX = cos(idleDirAngle).mul(idleSwayAngle)
  const idleBendZ = sin(idleDirAngle).mul(idleSwayAngle)

  // Combined bend angle (local-space X and Z components)
  const totalBendX = bendFromWindX.add(idleBendX)
  const totalBendZ = bendFromWindZ.add(idleBendZ)

  // Bend magnitude and normalized direction
  const bendMag = totalBendX
    .mul(totalBendX)
    .add(totalBendZ.mul(totalBendZ))
    .sqrt()
    .add(float(0.0001))
  const bendDirX = totalBendX.div(bendMag)
  const bendDirZ = totalBendZ.div(bendMag)

  // Per-vertex bend angle (quadratic profile: stiff at base, flexible at tip)
  const maxBend = float(1.22) // ~70°
  const vertexAngle = bendMag.mul(heightFactor).min(maxBend)
  const bendSin = sin(vertexAngle)
  const bendCos = cos(vertexAngle)

  // ── Player interaction: additive trail push (pure functional, no assign) ──
  let totalPushX: N = float(0)
  let totalPushZ: N = float(0)
  let totalStr: N = float(0)

  for (const tp of uTrail) {
    const dx = instanceWorldXZ.x.sub(tp.x)
    const dz = instanceWorldXZ.y.sub(tp.y) // vec2.y = worldZ
    const d = dx.mul(dx).add(dz.mul(dz)).sqrt().add(float(0.001))
    const prox = float(1.0).sub(smoothstep(float(0), uInteractionRadius, d))
    const str = prox.mul(prox).mul(tp.z) // tp.z = strength
    totalPushX = totalPushX.add(dx.div(d).mul(str))
    totalPushZ = totalPushZ.add(dz.div(d).mul(str))
    totalStr = totalStr.add(str)
  }

  // Clamp total strength to 1
  const clampedStr = totalStr.min(float(1.0))
  const pushStrength = clampedStr.mul(uInteractionStrength)
  // uvY=0→0, uvY=0.4(mid)→0.19, uvY=1(tip)→1.2 (tip > mid but less extreme)
  const bendProfile = uvY.mul(uvY).mul(float(1.2))
  const pushFactor = pushStrength.mul(bendProfile)
  // Normalize accumulated direction
  const totalLen = totalPushX
    .mul(totalPushX)
    .add(totalPushZ.mul(totalPushZ))
    .sqrt()
    .add(float(0.001))
  const pushX = totalPushX.div(totalLen).mul(pushFactor)
  const pushZ = totalPushZ.div(totalLen).mul(pushFactor)
  const pushY = pushStrength.mul(heightFactor).mul(-0.15)

  // Spine rotation (circular bend) + lateral offset + push interaction
  mat.positionNode = vec3(
    localPosX.add(bendDirX.mul(bendSin).mul(localPosY)).add(pushX),
    bendCos.mul(localPosY).add(pushY),
    localPosZ.add(bendDirZ.mul(bendSin).mul(localPosY)).add(pushZ)
  )

  return {
    material: mat,
    uniforms: {
      uTime: uTime as unknown as { value: number },
      uWindStrength: uWindStrength as unknown as { value: number },
      uWindFrequency: uWindFrequency as unknown as { value: number },
      uWindDir: uWindDir as unknown as { value: THREE.Vector2 },
      uGustStrength: uGustStrength as unknown as { value: number },
      uWaveAngles: uWaveAngles as unknown as { value: number }[],
      uWaveAmps: uWaveAmps as unknown as { value: number }[],
      uWaveParams: uWaveParams as unknown as { value: THREE.Vector4 }[],
      uTrail: uTrail as unknown as { value: THREE.Vector3 }[],
      uInteractionRadius: uInteractionRadius as unknown as { value: number },
      uInteractionStrength: uInteractionStrength as unknown as {
        value: number
      },
    },
  }
}

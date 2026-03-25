import * as THREE from 'three'
import { MeshStandardNodeMaterial } from 'three/webgpu'
import {
  Fn,
  uniform,
  vec2,
  vec3,
  vec4,
  float,
  sin,
  cos,
  mix,
  smoothstep,
  sqrt,
  select,
  positionLocal,
  normalLocal,
  instanceIndex,
  hash,
  attribute,
  instancedArray,
  deltaTime,
  cameraViewMatrix,
} from 'three/tsl'
import { GUST_WAVE_COUNT, type GrassMaterialConfig } from './grass-material'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any // TSL node -- broad type for shader node expressions

// ── Per-instance attribute names (used by both blade & flower) ───
export const BLADE_INSTANCE_POS_ATTR = 'aInstanceWorldXZ'
export const BLADE_INSTANCE_ROT_ATTR = 'aInstanceRotation'

// ── Compute context: per-sub-chunk GPU storage + dispatch ────────

export interface GrassComputeContext {
  /** vec4 per blade: (worldX, worldZ, worldY, rotation) — written from CPU */
  bladeData: N
  /** float per blade: scale — written from CPU */
  bladeScale: N
  /** vec4 per blade: (windBendX, windBendZ, pushBendX, pushBendZ) — GPU state */
  bendState: N
  /** Compute node to dispatch each frame */
  computeUpdate: N
  /** Actual blade count (set when writing data) */
  count: number
}

export interface GrassComputeUniforms {
  uTime: { value: number }
  uDeltaTime: { value: number }
  uWindStrength: { value: number }
  uWindFrequency: { value: number }
  uWindDir: { value: THREE.Vector2 }
  uGustStrength: { value: number }
  uWaveAngles: { value: number }[]
  uWaveAmps: { value: number }[]
  uWaveParams: { value: THREE.Vector4 }[]
  /** Player current position: vec3(worldX, worldZ, strength). Always active.
   *  Asymmetric lerp (fast push / slow recovery) creates natural trail effect. */
  uPlayerPos: { value: THREE.Vector3 }
  uInteractionRadius: { value: number }
  uInteractionStrength: { value: number }
}

/**
 * Create shared uniforms for a grass type (short or tall).
 * All sub-chunk compute contexts of the same type share these uniforms,
 * so updating them once per frame affects all dispatches.
 */
export function createSharedComputeUniforms(
  cfg?: GrassMaterialConfig
): GrassComputeUniforms {
  const ws = cfg?.windStrength ?? 0.06
  const wf = cfg?.windFrequency ?? 2.0
  const ir = cfg?.interactionRadius ?? 1.5
  const is_ = cfg?.interactionStrength ?? 0.15

  return {
    uTime: uniform(0) as unknown as { value: number },
    uDeltaTime: uniform(0) as unknown as { value: number },
    uWindStrength: uniform(ws) as unknown as { value: number },
    uWindFrequency: uniform(wf) as unknown as { value: number },
    uWindDir: uniform(new THREE.Vector2(1, 0)) as unknown as {
      value: THREE.Vector2
    },
    uGustStrength: uniform(0) as unknown as { value: number },
    uWaveAngles: [uniform(0), uniform(0.4), uniform(-0.3)] as unknown as {
      value: number
    }[],
    uWaveAmps: [uniform(1), uniform(1), uniform(1)] as unknown as {
      value: number
    }[],
    uWaveParams: [
      uniform(new THREE.Vector4(0.35, 0.7, 1.5, 0.75)),
      uniform(new THREE.Vector4(0.31, 0.8, 1.6, 0.87)),
      uniform(new THREE.Vector4(0.39, 1.5, 1.7, 0.95)),
    ] as unknown as { value: THREE.Vector4 }[],
    uPlayerPos: uniform(new THREE.Vector3(99999, 99999, 0)) as unknown as {
      value: THREE.Vector3
    },
    uInteractionRadius: uniform(ir) as unknown as { value: number },
    uInteractionStrength: uniform(is_) as unknown as { value: number },
  }
}

/**
 * Create a compute context for one sub-chunk.
 * Each context has its own instancedArray buffers but references shared uniforms.
 */
export function createGrassComputeContext(
  capacity: number,
  sharedUniforms: GrassComputeUniforms
): GrassComputeContext {
  const bladeData = instancedArray(capacity, 'vec4')
  const bladeScale = instancedArray(capacity, 'float')
  const bendState = instancedArray(capacity, 'vec4')

  // Cast shared uniforms back to TSL nodes for use in Fn()
  const uTime = sharedUniforms.uTime as unknown as N
  const uWindStrength = sharedUniforms.uWindStrength as unknown as N
  const uWindFrequency = sharedUniforms.uWindFrequency as unknown as N
  const uWindDir = sharedUniforms.uWindDir as unknown as N
  const uGustStrength = sharedUniforms.uGustStrength as unknown as N
  const uWaveAngles = sharedUniforms.uWaveAngles as unknown as N[]
  const uWaveAmps = sharedUniforms.uWaveAmps as unknown as N[]
  const uWaveParams = sharedUniforms.uWaveParams as unknown as N[]

  const uPlayerPos = sharedUniforms.uPlayerPos as unknown as N
  const uInteractionRadius = sharedUniforms.uInteractionRadius as unknown as N
  const uInteractionStrength =
    sharedUniforms.uInteractionStrength as unknown as N

  const computeUpdate = Fn(() => {
    const blade = bladeData.element(instanceIndex)
    const bend = bendState.element(instanceIndex)

    const bx = blade.x // worldX
    const bz = blade.y // worldZ

    // ── Gerstner wave gusts ──
    let gust: N = float(0)
    for (let wi = 0; wi < GUST_WAVE_COUNT; wi++) {
      const wp = uWaveParams[wi]
      const wFreq = wp.x
      const wSpeed = wp.y
      const wAmp = wp.z
      const wQ = wp.w

      const wAngle = uWaveAngles[wi]
      const cOff = cos(wAngle)
      const sOff = sin(wAngle)
      const wDirX = uWindDir.x.mul(cOff).sub(uWindDir.y.mul(sOff))
      const wDirZ = uWindDir.x.mul(sOff).add(uWindDir.y.mul(cOff))

      const spatial = bx.mul(wDirX).add(bz.mul(wDirZ))
      const perp = bx.mul(wDirZ.negate()).add(bz.mul(wDirX))
      const warp = sin(perp.mul(0.15)).mul(2.5)

      const phase = spatial.mul(wFreq).add(warp).sub(uTime.mul(wSpeed))
      const gerstnerPhase = phase.add(wQ.mul(sin(phase)))
      const waveVal = cos(gerstnerPhase).add(1).mul(0.5)
      gust = gust.add(waveVal.mul(wAmp).mul(uWaveAmps[wi]))
    }
    gust = gust.mul(float(0.15).add(uGustStrength.mul(0.85)))

    // ── Wind bend target (world space) ──
    const windBendAngle = uWindStrength.mul(5.0).mul(float(1.0).add(gust))
    const windTargetX = uWindDir.x.mul(windBendAngle)
    const windTargetZ = uWindDir.y.mul(windBendAngle)

    // ── Idle sway ──
    const instanceHash = hash(
      vec2(instanceIndex.toFloat().mul(0.1), float(0.5))
    )
    const phaseOffset = instanceHash.mul(6.283)
    const idleSwayAngle = sin(uTime.mul(uWindFrequency).add(phaseOffset)).mul(
      uWindStrength
    )
    const idleDirAngle = phaseOffset
    const idleX = cos(idleDirAngle).mul(idleSwayAngle)
    const idleZ = sin(idleDirAngle).mul(idleSwayAngle)

    // ── Static lean ──
    const leanHash1 = hash(vec2(instanceIndex.toFloat().mul(0.31), float(5.5)))
    const leanHash2 = hash(vec2(instanceIndex.toFloat().mul(0.67), float(6.1)))
    const staticLeanX = leanHash1.sub(0.5).mul(0.15)
    const staticLeanZ = leanHash2.sub(0.5).mul(0.15)

    // Combined wind target (with idle + static lean)
    const totalWindX = windTargetX.add(idleX).add(staticLeanX)
    const totalWindZ = windTargetZ.add(idleZ).add(staticLeanZ)

    // Lerp wind bend state (smooth following)
    const lw = deltaTime.mul(4.0).saturate()
    bend.x.assign(mix(bend.x, totalWindX, lw))
    bend.y.assign(mix(bend.y, totalWindZ, lw))

    // ── Player interaction (single push point + asymmetric lerp) ──
    const pdx = bx.sub(uPlayerPos.x)
    const pdz = bz.sub(uPlayerPos.y) // .y = worldZ
    const pd = sqrt(pdx.mul(pdx).add(pdz.mul(pdz))).add(float(0.001))
    const pProx = float(1.0).sub(smoothstep(float(0), uInteractionRadius, pd))
    const pStr = pProx.mul(pProx).mul(uPlayerPos.z) // .z = strength
    const pushDirX = pdx.div(pd).mul(pStr)
    const pushDirZ = pdz.div(pd).mul(pStr)

    const pushTargetX = pushDirX.mul(uInteractionStrength)
    const pushTargetZ = pushDirZ.mul(uInteractionStrength)

    // Asymmetric lerp: fast push (dt*12), slow recovery (dt*1)
    const targetMag = sqrt(
      pushTargetX.mul(pushTargetX).add(pushTargetZ.mul(pushTargetZ))
    )
    const currentMag = sqrt(bend.z.mul(bend.z).add(bend.w.mul(bend.w)))
    const lm = select(
      targetMag.greaterThan(currentMag),
      deltaTime.mul(12.0),
      deltaTime.mul(1.0)
    ).saturate()
    bend.z.assign(mix(bend.z, pushTargetX, lm))
    bend.w.assign(mix(bend.w, pushTargetZ, lm))
  })().compute(capacity)

  return { bladeData, bladeScale, bendState, computeUpdate, count: 0 }
}

/**
 * Write blade placement data into the compute context's buffers.
 * Call this when a sub-chunk is loaded/assigned.
 *
 * bladeData layout: vec4(worldX, worldZ, worldY, rotation)
 * bladeScale layout: float(scale)
 */
export function writeBladeData(
  ctx: GrassComputeContext,
  worldXZ: Float32Array,
  worldY: Float32Array,
  rotations: Float32Array,
  scales: Float32Array,
  count: number
): void {
  const arr = ctx.bladeData.value.array as Float32Array
  const scaleArr = ctx.bladeScale.value.array as Float32Array
  for (let i = 0; i < count; i++) {
    const base = i * 4
    arr[base] = worldXZ[i * 2] // worldX
    arr[base + 1] = worldXZ[i * 2 + 1] // worldZ
    arr[base + 2] = worldY[i] // worldY
    arr[base + 3] = rotations[i] // rotation
    scaleArr[i] = scales[i]
  }
  // Zero out remaining slots (scale = 0 → invisible)
  for (let i = count; i < scaleArr.length; i++) {
    scaleArr[i] = 0
  }

  // Signal GPU re-upload
  ctx.bladeData.value.needsUpdate = true
  ctx.bladeScale.value.needsUpdate = true

  ctx.count = count
}

// ── Blade material (reads from compute buffers) ──────────────────

/**
 * Create a MeshStandardNodeMaterial for blade grass that reads bend state
 * from the compute shader's instancedArray buffers.
 *
 * Each sub-chunk mesh needs its own material instance (since each references
 * different instancedArray buffers), but the shader source is identical so
 * Three.js deduplicates pipeline compilation.
 */
export function createBladeMaterial(
  ctx: GrassComputeContext,
  cfg?: GrassMaterialConfig
): MeshStandardNodeMaterial {
  const bc = cfg?.baseColor ?? [0.015, 0.04, 0.008]
  const tc = cfg?.tipColor ?? [0.06, 0.14, 0.03]
  const wsMin = cfg?.widthScaleMin ?? 0.7
  const wsExt = cfg?.widthScaleExtent ?? 0.7
  // Height scale is fully handled by placement data (instanceScale).
  // No additional shader-side height variation needed.

  const mat = new MeshStandardNodeMaterial()
  mat.side = THREE.DoubleSide
  mat.roughness = 0.55
  mat.metalness = 0.0
  // Fade bottom edge to avoid hard cutoff at ground level
  mat.transparent = true
  mat.opacityNode = smoothstep(float(0.0), float(0.08), attribute('uv').y)

  // ── Read from compute buffers ──────────────────────────
  const blade = ctx.bladeData.element(instanceIndex)
  const bend = ctx.bendState.element(instanceIndex)
  const instanceScale = ctx.bladeScale.element(instanceIndex)

  const instanceWorldX = blade.x
  const instanceWorldZ = blade.y
  const instanceWorldY = blade.z
  const instanceRotation = blade.w

  // ── Color: base → tip gradient ─────────────────────────
  const baseColor = vec3(bc[0], bc[1], bc[2])
  const tipColor = vec3(tc[0], tc[1], tc[2])
  const uvY = attribute('uv').y

  const gradientColor = mix(
    baseColor,
    tipColor,
    smoothstep(float(0), float(0.8), uvY)
  )

  // Root darkening (AO)
  const rootAO = mix(
    float(0.45),
    float(1.0),
    smoothstep(float(0), float(0.35), uvY)
  )

  // Per-instance brightness + hue variation
  const brightnessHash = hash(
    vec2(instanceIndex.toFloat().mul(0.37), float(1.7))
  )
  const brightness = float(0.85).add(brightnessHash.mul(0.3))

  const hueHash = hash(vec2(instanceIndex.toFloat().mul(0.73), float(3.1)))
  const hueShift = vec3(
    float(1.0).add(hueHash.sub(0.5).mul(0.15)),
    float(1.0),
    float(1.0).add(hueHash.sub(0.5).mul(-0.1))
  )

  mat.colorNode = gradientColor.mul(brightness).mul(hueShift).mul(rootAO)

  // ── Per-instance shape variation ───────────────────────
  const shapeHash1 = hash(vec2(instanceIndex.toFloat().mul(0.53), float(2.3)))
  const widthScale = float(wsMin).add(shapeHash1.mul(wsExt))
  const heightScale = instanceScale

  // ── Vertex displacement ────────────────────────────────
  const rawPos = positionLocal.toVar()
  const localPosX = rawPos.x.mul(widthScale).mul(instanceScale)
  const localPosY = rawPos.y.mul(heightScale)
  const localPosZ = rawPos.z.mul(widthScale).mul(instanceScale)

  const heightFactor = uvY.mul(uvY)

  // ── Read bend state from compute shader ────────────────
  // bend.xy = wind bend, bend.zw = interaction push
  const windBendX = bend.x
  const windBendZ = bend.y
  const pushBendX = bend.z
  const pushBendZ = bend.w

  // Combined bend
  const totalBendX = windBendX.add(pushBendX)
  const totalBendZ = windBendZ.add(pushBendZ)

  const bendMag = sqrt(
    totalBendX.mul(totalBendX).add(totalBendZ.mul(totalBendZ))
  ).add(float(0.0001))
  const bendDirX = totalBendX.div(bendMag)
  const bendDirZ = totalBendZ.div(bendMag)

  // Per-vertex bend angle (quadratic profile: stiff at base, flexible at tip)
  const maxBend = float(0.87) // ~50°
  const vertexAngle = bendMag.mul(heightFactor).min(maxBend)
  const bendSin = sin(vertexAngle)
  const bendCos = cos(vertexAngle)

  // Push also adds per-vertex displacement for tip deflection
  const pushProfile = uvY.mul(uvY).mul(float(1.2))
  const pushX = pushBendX.mul(pushProfile)
  const pushZ = pushBendZ.mul(pushProfile)
  const pushY = sqrt(
    pushBendX.mul(pushBendX).add(pushBendZ.mul(pushBendZ))
  ).mul(heightFactor.mul(-0.15))

  // ── Rotate local position by instance rotation ─────────
  const cosR = cos(instanceRotation)
  const sinR = sin(instanceRotation)
  const rotX = localPosX.mul(cosR).sub(localPosZ.mul(sinR))
  const rotZ = localPosX.mul(sinR).add(localPosZ.mul(cosR))

  // ── Normal: rotate geometry normal by instance rotation, then to view space ──
  // Geometry normals are (0, 0, 1). After Y-axis rotation by instanceRotation:
  const worldNormalX = normalLocal.z.mul(sinR).negate()
  const worldNormalY = normalLocal.y
  const worldNormalZ = normalLocal.z.mul(cosR)
  const worldNormal = vec3(worldNormalX, worldNormalY, worldNormalZ).normalize()
  // normalNode must return view-space normals (per memory note)
  mat.normalNode = cameraViewMatrix.mul(vec4(worldNormal, 0.0)).xyz.normalize()

  // ── Final: world position + spine bend + push ──────────
  mat.positionNode = vec3(
    instanceWorldX
      .add(rotX)
      .add(bendDirX.mul(bendSin).mul(localPosY))
      .add(pushX),
    instanceWorldY.add(bendCos.mul(localPosY)).add(pushY),
    instanceWorldZ
      .add(rotZ)
      .add(bendDirZ.mul(bendSin).mul(localPosY))
      .add(pushZ)
  )

  return mat
}

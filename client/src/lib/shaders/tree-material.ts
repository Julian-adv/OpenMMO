/**
 * TSL tree material + instance buffer helpers.
 *
 * Follows the grass-layer pattern exactly:
 *   - instancedArray buffers for per-instance data
 *   - positionNode overrides instanceMatrix (which stays zeroed)
 *   - normalNode computed manually in view space
 *
 * This avoids the WebGPU InstancedMesh pipeline issues that cause 1 FPS
 * with the standard instanceMatrix path.
 */

import * as THREE from 'three'
import { MeshStandardNodeMaterial } from 'three/webgpu'
import {
  instancedArray,
  instanceIndex,
  texture,
  uv,
  attribute,
  float,
  vec3,
  vec4,
  cos,
  sin,
  positionLocal,
  normalLocal,
  cameraViewMatrix,
} from 'three/tsl'

// ── Instance buffer context ──────────────────────────────

export interface TreeInstanceContext {
  /** vec4(worldX, worldZ, worldY, rotation) per instance */
  posData: ReturnType<typeof instancedArray>
  /** float(scale) per instance */
  scaleData: ReturnType<typeof instancedArray>
  count: number
}

export function createTreeInstanceContext(
  capacity: number
): TreeInstanceContext {
  return {
    posData: instancedArray(capacity, 'vec4'),
    scaleData: instancedArray(capacity, 'float'),
    count: 0,
  }
}

/**
 * Write tree placement data into the instance buffers.
 * Layout matches grass: posData = vec4(worldX, worldZ, worldY, rotation).
 */
export function writeTreeInstanceData(
  ctx: TreeInstanceContext,
  instances: Float32Array[], // arrays of [x, y, z, rotation, scale] × N
  maxInstances: number
): void {
  const posArr = ctx.posData.value.array as Float32Array
  const scaleArr = ctx.scaleData.value.array as Float32Array

  let idx = 0
  for (const raw of instances) {
    const count = raw.length / 5
    for (let i = 0; i < count && idx < maxInstances; i++) {
      const base = i * 5
      const p = idx * 4
      posArr[p] = raw[base] // worldX
      posArr[p + 1] = raw[base + 2] // worldZ
      posArr[p + 2] = raw[base + 1] // worldY
      posArr[p + 3] = raw[base + 3] // rotation
      scaleArr[idx] = raw[base + 4] // scale
      idx++
    }
  }

  ctx.count = idx

  // Zero remaining scales → invisible (degenerate)
  for (let i = idx; i < scaleArr.length; i++) {
    scaleArr[i] = 0
  }

  ctx.posData.value.needsUpdate = true
  ctx.scaleData.value.needsUpdate = true
}

// ── TSL material ─────────────────────────────────────────

/**
 * Create a MeshStandardNodeMaterial for trees that reads instance data
 * from the given context's instancedArray buffers.
 *
 * Each sub-mesh needs its own material (different buffer references),
 * but the shader structure is identical → pipeline dedup.
 */
export function createTreeMaterial(
  ctx: TreeInstanceContext,
  src: THREE.MeshStandardMaterial
): MeshStandardNodeMaterial {
  const mat = new MeshStandardNodeMaterial()
  mat.side = src.side
  mat.alphaTest = src.alphaTest
  mat.transparent = src.transparent
  mat.depthWrite = src.depthWrite
  mat.metalness = src.metalness
  mat.roughness = src.roughness

  const uvCoord = uv()

  // ── Read per-instance data ──
  const data = ctx.posData.element(instanceIndex)
  const instanceX = data.x
  const instanceZ = data.y
  const instanceY = data.z
  const instanceRot = data.w
  const instanceScale = ctx.scaleData.element(instanceIndex)

  const cosR = cos(instanceRot)
  const sinR = sin(instanceRot)

  // ── Position: scale → rotate Y → translate ──
  const lp = positionLocal.mul(instanceScale)
  const rotX = lp.x.mul(cosR).sub(lp.z.mul(sinR))
  const rotZ = lp.x.mul(sinR).add(lp.z.mul(cosR))

  mat.positionNode = vec3(
    instanceX.add(rotX),
    instanceY.add(lp.y),
    instanceZ.add(rotZ)
  )

  // ── Normal: rotate by Y, then to view space ──
  const n = normalLocal
  const nRotX = n.x.mul(cosR).sub(n.z.mul(sinR))
  const nRotZ = n.x.mul(sinR).add(n.z.mul(cosR))
  const worldNormal = vec3(nRotX, n.y, nRotZ).normalize()
  mat.normalNode = cameraViewMatrix.mul(vec4(worldNormal, 0.0)).xyz.normalize()

  // ── Color + alpha ──
  if (src.map) {
    const baseColor = texture(src.map, uvCoord)
    if (src.vertexColors) {
      mat.colorNode = baseColor.rgb.mul(attribute('color').rgb)
    } else {
      mat.colorNode = baseColor.rgb
    }
    if (src.alphaTest > 0) {
      mat.opacityNode = baseColor.a
    }
  }

  // ── Roughness / metalness from texture ──
  if (src.roughnessMap) {
    const mrSample = texture(src.roughnessMap, uvCoord)
    mat.roughnessNode = mrSample.g.mul(float(src.roughness))
    mat.metalnessNode = mrSample.b.mul(float(src.metalness))
  }

  return mat
}

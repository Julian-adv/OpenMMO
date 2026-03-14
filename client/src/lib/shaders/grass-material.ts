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
} from 'three/tsl'

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any // TSL node -- broad type for shader node expressions

// ── Grass blade geometry ─────────────────────────────────
// 5 vertices forming a tapered blade (2 triangles bottom + 1 triangle tip)
//
//        4 (tip)
//       / \
//      /   \
//    2 ───── 3  (mid, narrower)
//    |       |
//    0 ───── 1  (base, full width)

export function createGrassBladeGeometry(
  width = 0.04,
  height = 0.2,
  midFrac = 0.4,
  midWidthFrac = 0.5
): THREE.BufferGeometry {
  const hw = width / 2
  const mh = height * midFrac
  const mw = hw * midWidthFrac

  // prettier-ignore
  const positions = new Float32Array([
    -hw, 0,      0,   // 0: base-left
     hw, 0,      0,   // 1: base-right
    -mw, mh,     0,   // 2: mid-left
     mw, mh,     0,   // 3: mid-right
     0,  height, 0,   // 4: tip
  ])

  // prettier-ignore
  const normals = new Float32Array([
    0, 0, 1,
    0, 0, 1,
    0, 0, 1,
    0, 0, 1,
    0, 0, 1,
  ])

  // UV: u=horizontal (0-1), v=vertical (0=base, 1=tip)
  // prettier-ignore
  const uvs = new Float32Array([
    0, 0,
    1, 0,
    0, midFrac,
    1, midFrac,
    0.5, 1,
  ])

  const indices = [0, 1, 2, 1, 3, 2, 2, 3, 4]

  const geo = new THREE.BufferGeometry()
  geo.setAttribute('position', new THREE.BufferAttribute(positions, 3))
  geo.setAttribute('normal', new THREE.BufferAttribute(normals, 3))
  geo.setAttribute('uv', new THREE.BufferAttribute(uvs, 2))
  geo.setIndex(indices)
  return geo
}

// ── TSL grass material ───────────────────────────────────

export const GRASS_TRAIL_COUNT = 5

export interface GrassMaterialUniforms {
  uTime: { value: number }
  uWindStrength: { value: number }
  uWindFrequency: { value: number }
  /** vec3(worldX, worldZ, strength) per trail point */
  uTrail: { value: THREE.Vector3 }[]
  uInteractionRadius: { value: number }
  uInteractionStrength: { value: number }
}

/**
 * Per-instance world position attribute name.
 * Each InstancedMesh must have an InstancedBufferAttribute with this name
 * containing vec2 (worldX, worldZ) per instance.
 */
export const GRASS_INSTANCE_POS_ATTR = 'aInstanceWorldXZ'

export function createGrassMaterial(): {
  material: MeshStandardNodeMaterial
  uniforms: GrassMaterialUniforms
} {
  const uTime = uniform(0)
  const uWindStrength = uniform(0.06)
  const uWindFrequency = uniform(2.0)
  const uInteractionRadius = uniform(1.5)
  const uInteractionStrength = uniform(0.15)

  // 5 individual trail point uniforms: vec3(worldX, worldZ, strength)
  const uTrail = Array.from({ length: GRASS_TRAIL_COUNT }, () =>
    uniform(new THREE.Vector3(0, 0, 0))
  )

  const mat = new MeshStandardNodeMaterial()
  mat.side = THREE.DoubleSide
  mat.roughness = 0.8
  mat.metalness = 0.0

  // ── Per-instance world position (vec2: x, z) ──
  const instanceWorldXZ = attribute(GRASS_INSTANCE_POS_ATTR, 'vec2')

  // ── Color: base → tip gradient with per-instance variation ──
  const baseColor = vec3(0.015, 0.04, 0.008) // dark root
  const tipColor = vec3(0.06, 0.14, 0.03) // bright tip
  const uvY = attribute('uv').y
  const gradientColor = mix(
    baseColor,
    tipColor,
    smoothstep(float(0), float(0.8), uvY)
  )

  // Per-instance hue/brightness variation via hashes of instanceIndex
  const brightnessHash = hash(
    vec2(instanceIndex.toFloat().mul(0.37), float(1.7))
  )
  const hueHash = hash(vec2(instanceIndex.toFloat().mul(0.73), float(3.1)))
  const brightness = float(0.85).add(brightnessHash.mul(0.3)) // 0.85 ~ 1.15
  // Slight yellow-green ↔ blue-green hue shift per instance
  const hueShift = vec3(
    float(1.0).add(hueHash.sub(0.5).mul(0.15)),
    float(1.0),
    float(1.0).add(hueHash.sub(0.5).mul(-0.1))
  )
  mat.colorNode = gradientColor.mul(brightness).mul(hueShift)

  // Do NOT set normalNode — the geometry normals (0,1,0) will be
  // automatically transformed to view-space by the default pipeline.
  // Setting normalNode directly treats it as view-space which breaks lighting.

  // ── Per-instance shape variation: width & height ──
  const shapeHash1 = hash(vec2(instanceIndex.toFloat().mul(0.53), float(2.3)))
  const shapeHash2 = hash(vec2(instanceIndex.toFloat().mul(0.91), float(4.7)))
  // Width: 0.7x ~ 1.4x, Height: 0.8x ~ 1.2x
  const widthScale = float(0.7).add(shapeHash1.mul(0.7))
  const heightScale = float(0.8).add(shapeHash2.mul(0.4))

  // ── Wind: displace upper vertices ──
  const rawPos = positionLocal.toVar()
  // Apply per-instance shape variation (width x, height y)
  const localPosX = rawPos.x.mul(widthScale)
  const localPosY = rawPos.y.mul(heightScale)
  const localPosZ = rawPos.z.mul(widthScale)
  const windPhase = uTime.mul(uWindFrequency)

  const instanceHash = hash(vec2(instanceIndex.toFloat().mul(0.1), float(0.5)))
  const phaseOffset = instanceHash.mul(6.283)

  const heightFactor = uvY.mul(uvY)
  const windAmount = heightFactor.mul(uWindStrength)
  const windX = sin(windPhase.add(phaseOffset)).mul(windAmount)
  const windZ = cos(windPhase.mul(0.7).add(phaseOffset.mul(1.3))).mul(
    windAmount.mul(0.5)
  )

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

  mat.positionNode = vec3(
    localPosX.add(windX).add(pushX),
    localPosY.add(pushY),
    localPosZ.add(windZ).add(pushZ)
  )

  return {
    material: mat,
    uniforms: {
      uTime: uTime as unknown as { value: number },
      uWindStrength: uWindStrength as unknown as { value: number },
      uWindFrequency: uWindFrequency as unknown as { value: number },
      uTrail: uTrail as unknown as { value: THREE.Vector3 }[],
      uInteractionRadius: uInteractionRadius as unknown as { value: number },
      uInteractionStrength: uInteractionStrength as unknown as {
        value: number
      },
    },
  }
}

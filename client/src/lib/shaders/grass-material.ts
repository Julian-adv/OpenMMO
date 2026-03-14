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

  // Normals point up (0,1,0) so blades receive light from above
  // prettier-ignore
  const normals = new Float32Array([
    0, 1, 0,
    0, 1, 0,
    0, 1, 0,
    0, 1, 0,
    0, 1, 0,
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

export interface GrassMaterialUniforms {
  uTime: { value: number }
  uWindStrength: { value: number }
  uWindFrequency: { value: number }
}

export function createGrassMaterial(): {
  material: MeshStandardNodeMaterial
  uniforms: GrassMaterialUniforms
} {
  const uTime = uniform(0)
  const uWindStrength = uniform(0.1)
  const uWindFrequency = uniform(2.0)

  const mat = new MeshStandardNodeMaterial()
  mat.side = THREE.DoubleSide
  mat.roughness = 0.8
  mat.metalness = 0.0

  // ── Color: base → tip gradient with per-instance variation ──
  const baseColor = vec3(0.03, 0.08, 0.015)
  const tipColor = vec3(0.08, 0.17, 0.04)
  const uvY = attribute('uv').y
  const gradientColor = mix(
    baseColor,
    tipColor,
    smoothstep(float(0), float(1), uvY)
  )

  // Per-instance brightness variation via hash of instanceIndex
  const brightnessHash = hash(
    vec2(instanceIndex.toFloat().mul(0.37), float(1.7))
  )
  const brightness = float(0.9).add(brightnessHash.mul(0.2)) // 0.9 ~ 1.1
  mat.colorNode = gradientColor.mul(brightness)

  // Do NOT set normalNode — the geometry normals (0,1,0) will be
  // automatically transformed to view-space by the default pipeline.
  // Setting normalNode directly treats it as view-space which breaks lighting.

  // ── Wind: displace upper vertices ──
  const localPos = positionLocal.toVar()
  const windPhase = uTime.mul(uWindFrequency)

  const instanceHash = hash(vec2(instanceIndex.toFloat().mul(0.1), float(0.5)))
  const phaseOffset = instanceHash.mul(6.283)

  // Wind displacement increases with height squared
  const windAmount = uvY.mul(uvY).mul(uWindStrength)
  const windX = sin(windPhase.add(phaseOffset)).mul(windAmount)
  const windZ = cos(windPhase.mul(0.7).add(phaseOffset.mul(1.3))).mul(
    windAmount.mul(0.5)
  )

  mat.positionNode = vec3(
    localPos.x.add(windX),
    localPos.y,
    localPos.z.add(windZ)
  )

  return {
    material: mat,
    uniforms: {
      uTime: uTime as unknown as { value: number },
      uWindStrength: uWindStrength as unknown as { value: number },
      uWindFrequency: uWindFrequency as unknown as { value: number },
    },
  }
}

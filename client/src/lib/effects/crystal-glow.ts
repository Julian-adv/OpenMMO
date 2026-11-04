import * as THREE from 'three'
import { MeshStandardNodeMaterial } from 'three/webgpu'
import {
  color,
  max,
  positionLocal,
  positionWorld,
  sin,
  smoothstep,
  texture,
  time,
} from 'three/tsl'

const GLOW_COLOR = '#3d6bff'
const GLOW_STRENGTH = 1.4
export const CRYSTAL_GLOW_SPEED = 1.3
const PHASE_X = 0.9
const PHASE_Z = 1.3

/** Per-cluster pulse phase, shared with the crystal light pool. */
export function crystalGlowPhase(p: { x: number; z: number }) {
  return p.x * PHASE_X + p.z * PHASE_Z
}

const glowing = new WeakSet<THREE.Object3D>()

/** Lights the blue cores of a crystal model with a slow upward shimmer, once
 *  per template; clones share the result. */
export function applyCrystalGlow(template: THREE.Object3D) {
  if (glowing.has(template)) return
  glowing.add(template)
  template.traverse((o) => {
    if (!(o instanceof THREE.Mesh)) return
    const src = o.material as THREE.MeshStandardMaterial
    if (!src.map) return
    const base = texture(src.map)
    const core = smoothstep(0.01, 0.12, base.b.sub(max(base.r, base.g)))
    const phase = positionWorld.x.mul(PHASE_X).add(positionWorld.z.mul(PHASE_Z))
    const wave = sin(
      positionLocal.y.mul(4).sub(time.mul(CRYSTAL_GLOW_SPEED)).add(phase)
    )
    const material = new MeshStandardNodeMaterial({
      map: src.map,
      normalMap: src.normalMap,
      roughnessMap: src.roughnessMap,
      metalnessMap: src.metalnessMap,
      roughness: src.roughness,
      metalness: src.metalness,
      side: src.side,
    })
    material.emissiveNode = color(GLOW_COLOR)
      .mul(core)
      .mul(wave.mul(0.35).add(0.65))
      .mul(GLOW_STRENGTH)
    o.material = material
  })
}

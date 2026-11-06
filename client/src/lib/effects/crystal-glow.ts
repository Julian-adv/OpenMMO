import * as THREE from 'three'
import { MeshStandardNodeMaterial } from 'three/webgpu'
import {
  color,
  max,
  mx_noise_float,
  positionGeometry,
  positionLocal,
  positionWorld,
  sin,
  smoothstep,
  texture,
  time,
  vec3,
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

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type N = any

/** The clusters' blue glow on `core`, pulsing up `height` at `speed`. */
function shimmer(core: N, height: N, speed: number) {
  const phase = positionWorld.x.mul(PHASE_X).add(positionWorld.z.mul(PHASE_Z))
  const wave = sin(height.mul(4).sub(time.mul(speed)).add(phase))
  return color(GLOW_COLOR)
    .mul(core)
    .mul(wave.mul(0.35).add(0.65))
    .mul(GLOW_STRENGTH)
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
    const material = new MeshStandardNodeMaterial({
      map: src.map,
      normalMap: src.normalMap,
      roughnessMap: src.roughnessMap,
      metalnessMap: src.metalnessMap,
      roughness: src.roughness,
      metalness: src.metalness,
      side: src.side,
    })
    material.emissiveNode = shimmer(core, positionLocal.y, CRYSTAL_GLOW_SPEED)
    o.material = material
  })
}

const SHARD_COLOR = '#2b2340'

/** Black-violet crystal shards with glowing blue cores, the clusters' slow
 *  upward shimmer running through them. */
export function crystalShardMaterial() {
  const material = new MeshStandardNodeMaterial({
    color: SHARD_COLOR,
    roughness: 0.3,
    metalness: 0.15,
    flatShading: true,
  })
  const core = smoothstep(
    0.05,
    0.45,
    mx_noise_float(positionGeometry.mul(vec3(3, 1.2, 3)))
  )
  material.emissiveNode = shimmer(
    core.mul(0.8).add(0.2),
    positionWorld.y,
    CRYSTAL_GLOW_SPEED * 3
  )
  return material
}

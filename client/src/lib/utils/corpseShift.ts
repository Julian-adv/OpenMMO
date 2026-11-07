import * as THREE from 'three'
import { forEachPosedVertex } from './characterAnimationUtils'

export interface PointXZ {
  x: number
  z: number
}

const DIRECTIONS = Array.from({ length: 16 }, (_, i) => {
  const a = (i / 16) * Math.PI * 2
  return { x: Math.cos(a), z: Math.sin(a) }
})
const MARGIN = 0.3
const MAX_SHIFT = 2
const STEP = 0.25

/**
 * Ground outline of `model`'s current skinned pose, model-local: the farthest
 * vertex in each of 16 directions. Call with the death clip on its last frame.
 */
export function computeCorpseOutline(model: THREE.Object3D): THREE.Vector3[] {
  const reach = DIRECTIONS.map(() => -Infinity)
  const points = DIRECTIONS.map(() => new THREE.Vector3())
  forEachPosedVertex(model, 3, (v) => {
    for (let d = 0; d < DIRECTIONS.length; d++) {
      const r = v.x * DIRECTIONS[d].x + v.z * DIRECTIONS[d].z
      if (r > reach[d]) {
        reach[d] = r
        points[d].copy(v)
      }
    }
  })
  return points.filter((_, d) => reach[d] > -Infinity)
}

/**
 * The smallest move of a corpse at `center` that keeps its world `outline`,
 * padded by a margin, clear of walls; `blocked` tests the line between two
 * points. Null when it already fits or nothing within reach does.
 */
export function findCorpseShift(
  center: PointXZ,
  outline: PointXZ[],
  blocked: (x0: number, z0: number, x1: number, z1: number) => boolean
): PointXZ | null {
  const padded = outline.map((p) => {
    const dx = p.x - center.x
    const dz = p.z - center.z
    const len = Math.hypot(dx, dz) || 1
    return { x: p.x + (dx / len) * MARGIN, z: p.z + (dz / len) * MARGIN }
  })
  const clear = (p: PointXZ, dx: number, dz: number) =>
    !blocked(center.x + dx, center.z + dz, p.x + dx, p.z + dz)
  const stuck = padded.filter((p) => !clear(p, 0, 0))
  if (!stuck.length) return null
  // Only move away from the points lying through a wall.
  const away = stuck.reduce(
    (a, p) => ({ x: a.x + center.x - p.x, z: a.z + center.z - p.z }),
    { x: 0, z: 0 }
  )
  const dirs = DIRECTIONS.filter((d) => d.x * away.x + d.z * away.z >= 0)
  for (let r = STEP; r <= MAX_SHIFT + 1e-6; r += STEP) {
    for (const d of dirs) {
      const dx = d.x * r
      const dz = d.z * r
      if (
        !blocked(center.x, center.z, center.x + dx, center.z + dz) &&
        padded.every((p) => clear(p, dx, dz))
      ) {
        return { x: dx, z: dz }
      }
    }
  }
  return null
}

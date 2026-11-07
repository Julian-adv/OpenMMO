import type * as THREE from 'three'

/**
 * Isometric SW-camera ray–AABB occlusion test.
 *
 * The scene camera looks down ~35° with XZ forward (1,0,−1)/√2. A ray from the
 * player toward the camera, R(s) = (px−s, py+s, pz+s) for s ≥ 0, passes through
 * `box` iff the run [sMin, sMax] inside it exceeds `minDepth` — i.e. the box
 * occludes the player by at least that much. `margin` widens the box in XZ.
 * Shared by the housing/tree/dungeon occluders so the camera model lives in
 * exactly one place.
 */
export function isoCameraOccludesPlayer(
  box: THREE.Box3,
  px: number,
  py: number,
  pz: number,
  minDepth: number,
  margin = 0
): boolean {
  const sHigh = box.max.y - py
  if (sHigh <= 0) return false
  const sLow = Math.max(box.min.y - py, 0)
  const sMin = Math.max(px - box.max.x - margin, box.min.z - margin - pz, sLow)
  const sMax = Math.min(px - box.min.x + margin, box.max.z + margin - pz, sHigh)
  return sMax - sMin > minDepth
}

/**
 * `isoCameraOccludesPlayer` with about `margin` metres of screen slack, so walls
 * that clip the figure's edge count too. Boxes north or east of the player sit behind them and never count.
 */
export function isoCameraOccludesNearPlayer(
  box: THREE.Box3,
  px: number,
  py: number,
  pz: number,
  minDepth: number,
  margin: number
): boolean {
  if (box.max.z <= pz || box.min.x >= px) return false
  return isoCameraOccludesPlayer(
    box,
    px,
    py,
    pz,
    minDepth,
    margin * Math.SQRT1_2
  )
}

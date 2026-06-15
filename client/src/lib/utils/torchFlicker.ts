import type * as THREE from 'three'

export const TORCH_BASE_INTENSITY = 50
export const TORCH_BASE_DISTANCE = 50
export const TORCH_BASE_DECAY = 1.2
/** Shadow-camera far plane for the torch point light. Smaller than the light's
 *  illumination range (TORCH_BASE_DISTANCE): with decay 1.2 the light is only
 *  ~7% as bright at 25m as up close, so shadows past here are imperceptible.
 *  Keeping the far plane tight (vs the full 50m) tightens perspective depth
 *  precision, which lets the shadow bias stay small and avoids peter-panning
 *  (shadows floating off the floor). */
export const TORCH_SHADOW_FAR = 25
/** Per-face resolution of the torch point light's cube shadow map. 1024 keeps
 *  the far-plane texel (~2·far/res) small enough that contact shadows don't drop
 *  out at range; raising it costs ~4× the per-frame depth pass (6 cube faces). */
export const TORCH_SHADOW_MAP_SIZE = 1024
/** Depth bias for the torch shadow. Kept tiny because the tight TORCH_SHADOW_FAR
 *  and the dungeon geometry's own thickness already suppress acne; a larger
 *  magnitude reintroduces peter-panning. */
export const TORCH_SHADOW_BIAS = -0.0001
export const TORCH_BASE_POSITION = { x: -0.5, y: 3.0, z: 0.3 } as const

export interface TorchFlickerState {
  time: number
}

/**
 * Compute flicker offsets for a given time.
 */
function computeFlicker(t: number) {
  return {
    intensity: Math.sin(t * 3.1) * 1.5 + Math.sin(t * 5.7) * 1.0,
    dx: Math.sin(t * 2.3) * 0.015,
    dy: Math.sin(t * 3.1) * 0.02,
  }
}

/**
 * Apply flickering to a torch light in local space (child of player group).
 * Sets position relative to TORCH_BASE_POSITION.
 */
export function applyTorchFlicker(
  light: THREE.PointLight,
  flickerTime: number,
  deltaTime: number
): number {
  const t = flickerTime + deltaTime
  const f = computeFlicker(t)
  light.intensity = TORCH_BASE_INTENSITY + f.intensity
  light.position.x = TORCH_BASE_POSITION.x + f.dx
  light.position.y = TORCH_BASE_POSITION.y + f.dy
  return t
}

/**
 * Apply flickering to a torch light in world space.
 * Adds flicker offsets to the provided world base position.
 */
export function applyTorchFlickerWorld(
  light: THREE.PointLight,
  flickerTime: number,
  deltaTime: number,
  baseX: number,
  baseY: number,
  baseZ: number
): number {
  const t = flickerTime + deltaTime
  const f = computeFlicker(t)
  light.intensity = TORCH_BASE_INTENSITY + f.intensity
  light.position.x = baseX + f.dx
  light.position.y = baseY + f.dy
  light.position.z = baseZ
  return t
}

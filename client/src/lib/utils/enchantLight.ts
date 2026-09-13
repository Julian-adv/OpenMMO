import * as THREE from 'three'
import { TORCH_BASE_DECAY, TORCH_BASE_DISTANCE } from './torchFlicker'

export const ENCHANT_LIGHT_INTENSITY = 70
export const ENCHANT_ARMOR_LIGHT_INTENSITY = 4
export const ENCHANT_LIGHT_RANGE = 14

export interface EnchantLight {
  playerId: number
  position: THREE.Vector3
  intensity: number
}

export function selectEnchantLight(
  lights: readonly EnchantLight[],
  playerId: number,
  position: { x: number; y: number; z: number }
): EnchantLight | null {
  let selected: EnchantLight | null = null
  let distance = ENCHANT_LIGHT_RANGE ** 2
  for (const light of lights) {
    if (light.intensity <= 0) continue
    if (light.playerId === playerId) return light
    const next =
      (light.position.x - position.x) ** 2 +
      (light.position.y - position.y) ** 2 +
      (light.position.z - position.z) ** 2
    if (next < distance) {
      distance = next
      selected = light
    }
  }
  return selected
}

export function applyEnchantLight(
  light: THREE.PointLight,
  source: EnchantLight | null
) {
  const active = source !== null && source.intensity > 0
  light.color.set(active ? '#ffe3a0' : '#ffcc66')
  light.distance = active ? ENCHANT_LIGHT_RANGE : TORCH_BASE_DISTANCE
  light.decay = active ? 1.5 : TORCH_BASE_DECAY
  const near = active ? 0.1 : 1.5
  if (light.shadow.camera.near !== near) {
    light.shadow.camera.near = near
    light.shadow.camera.updateProjectionMatrix()
    light.shadow.needsUpdate = true
  }
  if (active) {
    light.position.copy(source.position)
    light.intensity = source.intensity
  }
  return active
}

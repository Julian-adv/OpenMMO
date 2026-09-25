import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { applyEnchantLight, selectEnchantLight } from './enchantLight'
import { TORCH_BASE_DISTANCE, TORCH_BASE_DECAY } from './torchFlicker'

describe('enchant shadow light priority', () => {
  it('prefers the local enchantment then the nearest visible active enchantment', () => {
    const origin = new THREE.Vector3()
    const distant = {
      playerId: 3,
      position: new THREE.Vector3(30, 0, 0),
      intensity: 70,
    }
    const near = {
      playerId: 2,
      position: new THREE.Vector3(1, 1, 0),
      intensity: 70,
    }
    const local = {
      playerId: 1,
      position: new THREE.Vector3(0, 3, 0),
      intensity: 70,
    }
    expect(selectEnchantLight([distant, near, local], 1, origin)).toBe(local)
    local.intensity = 0
    expect(selectEnchantLight([distant, near, local], 1, origin)).toBe(near)
    near.intensity = 0
    expect(selectEnchantLight([distant, near, local], 1, origin)).toBeNull()
  })

  it('takes over the existing shadow light and restores its torch settings', () => {
    const light = new THREE.PointLight()
    light.castShadow = true
    const source = {
      playerId: 1,
      position: new THREE.Vector3(1, 2, 3),
      intensity: 70,
    }
    expect(applyEnchantLight(light, source)).toBe(true)
    expect(light.position.equals(source.position)).toBe(true)
    expect(light.intensity).toBe(70)
    expect(light.castShadow).toBe(true)
    expect(light.shadow.camera.near).toBe(0.1)
    expect(applyEnchantLight(light, null)).toBe(false)
    expect(light.color.getHexString()).toBe('ffcc66')
    expect(light.distance).toBe(TORCH_BASE_DISTANCE)
    expect(light.decay).toBe(TORCH_BASE_DECAY)
    expect(light.shadow.camera.near).toBe(1.5)
    light.dispose()
  })
})

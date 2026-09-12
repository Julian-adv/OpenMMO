import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { createWeaponGlowGeometry } from './weaponGlowGeometry'

describe('weapon glow geometry', () => {
  it('matches the transformed weapon silhouette without modifying its source geometry', () => {
    const weapon = new THREE.Group()
    weapon.position.set(10, 3, 4)
    weapon.rotation.z = 0.6
    const blade = new THREE.Mesh(new THREE.BoxGeometry(0.2, 2, 0.1))
    blade.position.y = 0.6
    weapon.add(blade)
    const original = blade.geometry.getAttribute('position').array.slice()
    const geometry = createWeaponGlowGeometry(weapon)
    geometry.computeBoundingBox()
    expect(geometry.boundingBox!.min.y).toBeCloseTo(-0.4)
    expect(geometry.boundingBox!.max.y).toBeCloseTo(1.6)
    expect(geometry.boundingBox!.max.x).toBeCloseTo(0.1)
    expect(geometry.index).toBeNull()
    expect(geometry.getAttribute('normal').count).toBe(36)
    const uv = geometry.getAttribute('uv')
    for (let i = 0; i < uv.count; i++) {
      expect(uv.getY(i)).toBeGreaterThanOrEqual(-1e-6)
      expect(uv.getY(i)).toBeLessThanOrEqual(1 + 1e-6)
    }
    expect(blade.geometry.getAttribute('position').array).toEqual(original)
    geometry.dispose()
    blade.geometry.dispose()
  })
})

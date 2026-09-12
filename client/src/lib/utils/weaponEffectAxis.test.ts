import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { getWeaponEffectAxis } from './weaponEffectAxis'

describe('weapon effect axis', () => {
  it('finds the centerline in weapon space through nested transforms', () => {
    const weapon = new THREE.Group()
    weapon.position.set(9, 3, -7)
    weapon.rotation.y = 0.7
    const nested = new THREE.Group()
    nested.rotation.z = Math.PI / 2
    nested.scale.set(2, 1, 0.5)
    const blade = new THREE.Mesh(new THREE.BoxGeometry(0.1, 2, 0.1))
    blade.position.y = 1
    nested.add(blade)
    weapon.add(nested)
    const original = blade.geometry.getAttribute('position').array.slice()
    const axis = getWeaponEffectAxis(weapon)
    expect(axis.center.distanceTo(new THREE.Vector3(-1, 0, 0))).toBeLessThan(
      1e-5
    )
    expect(axis.direction.toArray()).toEqual([-1, 0, 0])
    expect(axis.length).toBeCloseTo(2)
    expect(blade.geometry.getAttribute('position').array).toEqual(original)
    expect(getWeaponEffectAxis(weapon)).toBe(axis)
    blade.geometry.dispose()
  })

  it('rejects empty or hidden geometry', () => {
    const weapon = new THREE.Group()
    const hidden = new THREE.Mesh(new THREE.BoxGeometry())
    hidden.visible = false
    weapon.add(hidden)
    expect(getWeaponEffectAxis(weapon).length).toBe(0)
    hidden.geometry.dispose()
  })
})

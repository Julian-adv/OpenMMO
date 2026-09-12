import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { EnchantWeaponGrip } from './enchantAnimation'
import { getWeaponEffectAxis } from './weaponEffectAxis'

describe('enchantment weapon grip', () => {
  it('fits the weapon to the baked pose without rotating character bones', () => {
    const hand = new THREE.Bone()
    hand.rotation.set(0.8, -0.5, 0.3)
    const handRotation = hand.quaternion.clone()
    const weapon = new THREE.Mesh(
      new THREE.BoxGeometry(2, 0.1, 0.15).translate(-0.7, 0, 0)
    )
    weapon.rotation.set(-1.9, -0.47, 0.14)
    hand.add(weapon)
    const gripRotation = weapon.quaternion.clone()
    const grip = new EnchantWeaponGrip(weapon)
    grip.update(1)
    weapon.updateWorldMatrix(true, false)
    expect(
      getWeaponEffectAxis(weapon)
        .direction.clone()
        .transformDirection(weapon.matrixWorld).y
    ).toBeCloseTo(1)
    expect(hand.quaternion.angleTo(handRotation)).toBeLessThan(1e-6)
    grip.update(0)
    expect(weapon.quaternion.angleTo(gripRotation)).toBeLessThan(1e-6)
    weapon.geometry.dispose()
  })
})

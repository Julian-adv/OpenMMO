import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import {
  EnchantWeaponGrip,
  getArmorEnchantHold,
  isArmorEnchantHold,
} from './enchantAnimation'
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
    for (const lowered of [false, true]) {
      grip.update(1, lowered)
      weapon.updateWorldMatrix(true, false)
      expect(
        getWeaponEffectAxis(weapon)
          .direction.clone()
          .transformDirection(weapon.matrixWorld).y
      ).toBeCloseTo(lowered ? -1 : 1)
    }
    expect(hand.quaternion.angleTo(handRotation)).toBeLessThan(1e-6)
    grip.update(0)
    expect(weapon.quaternion.angleTo(gripRotation)).toBeLessThan(1e-6)
    weapon.geometry.dispose()
  })
})

describe('armor enchantment hold', () => {
  it('holds the first idle pose through repeated loops without changing the source clip', () => {
    const head = new THREE.Object3D()
    head.name = 'Head'
    const idle = new THREE.AnimationClip('look-around', 2, [
      new THREE.NumberKeyframeTrack(
        'Head.rotation[y]',
        [0, 1, 2],
        [0.1, 0.8, -0.5]
      ),
    ])
    const mixer = new THREE.AnimationMixer(head)
    expect(isArmorEnchantHold(undefined, idle)).toBe(false)
    expect(isArmorEnchantHold(idle, idle)).toBe(false)
    const hold = getArmorEnchantHold(idle)
    expect(isArmorEnchantHold(hold, idle)).toBe(true)
    mixer.clipAction(hold).play()
    for (let frame = 0; frame < 60; frame++) {
      mixer.update(0.1)
      expect(head.rotation.y).toBeCloseTo(0.1)
    }
    expect(idle.tracks[0].times).toHaveLength(3)
    expect(getArmorEnchantHold(idle)).toBe(hold)
    mixer.stopAllAction()
    mixer.clipAction(idle).play()
    mixer.update(1)
    expect(head.rotation.y).toBeCloseTo(0.8)
  })

  it('crossfades out of the hold into movement without pausing the mixer', () => {
    const root = new THREE.Object3D()
    const idle = new THREE.AnimationClip('idle', 2, [
      new THREE.NumberKeyframeTrack('.position[x]', [0, 2], [0, 1]),
    ])
    const walk = new THREE.AnimationClip('walk', 2, [
      new THREE.NumberKeyframeTrack('.position[x]', [0, 2], [0, 4]),
    ])
    const mixer = new THREE.AnimationMixer(root)
    const held = mixer.clipAction(getArmorEnchantHold(idle)).play()
    mixer.update(1)
    const moving = mixer.clipAction(walk).play().crossFadeFrom(held, 0.3, false)
    mixer.update(0.5)
    expect(root.position.x).toBeCloseTo(1)
    expect(moving.time).toBeCloseTo(0.5)
    expect(held.getEffectiveWeight()).toBe(0)
  })
})

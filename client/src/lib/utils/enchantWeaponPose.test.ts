import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { EnchantWeaponPose } from './enchantWeaponPose'
import { getWeaponEffectAxis } from './weaponEffectAxis'

function rig() {
  const root = new THREE.Group()
  root.rotation.y = 0.7
  const head = new THREE.Bone()
  head.name = 'Head'
  head.position.y = 1.7
  root.add(head)
  const bones = [head]
  const hands: THREE.Bone[] = []
  for (const side of ['Right', 'Left']) {
    const sign = side === 'Right' ? 1 : -1
    const upper = new THREE.Bone()
    upper.name = `${side}Arm`
    upper.position.set(sign * 0.18, 1.4, 0)
    const lower = new THREE.Bone()
    lower.name = `${side}ForeArm`
    lower.position.set(sign * 0.3, 0, 0)
    const hand = new THREE.Bone()
    hand.name = `${side}Hand`
    hand.position.set(sign * 0.28, 0, 0)
    root.add(upper)
    upper.add(lower)
    lower.add(hand)
    bones.push(upper, lower, hand)
    hands.push(hand)
  }
  const skin = new THREE.SkinnedMesh()
  skin.bind(new THREE.Skeleton(bones))
  root.add(skin)
  const weapon = new THREE.Mesh(
    new THREE.BoxGeometry(1.8, 0.08, 0.12).translate(0.5, 0, 0)
  )
  hands[0].add(weapon)
  return { root, weapon, bones }
}

describe('enchantment weapon pose', () => {
  it('holds the weapon vertically, suppresses idle sway and restores the underlying animation', () => {
    const { root, weapon, bones } = rig()
    const positions = bones.map((bone) => bone.position.clone())
    const rotations = bones.map((bone) => bone.quaternion.clone())
    const pose = new EnchantWeaponPose(root, weapon, 'RightHand', true)
    pose.apply(0.4, true)
    root.updateWorldMatrix(true, true)
    const axis = getWeaponEffectAxis(weapon).direction
    expect(
      axis
        .clone()
        .transformDirection(weapon.matrixWorld)
        .dot(new THREE.Vector3(0, 1, 0))
    ).toBeCloseTo(1)
    const held = weapon.getWorldPosition(new THREE.Vector3())
    pose.restore()
    for (let i = 0; i < bones.length; i++) {
      expect(bones[i].position.distanceTo(positions[i])).toBeLessThan(1e-6)
      expect(bones[i].quaternion.angleTo(rotations[i])).toBeLessThan(1e-6)
    }
    bones[1].rotation.z = 0.25
    pose.apply(0.1, true)
    expect(
      weapon.getWorldPosition(new THREE.Vector3()).distanceTo(held)
    ).toBeLessThan(1e-6)
    pose.restore()
    expect(bones[1].rotation.z).toBeCloseTo(0.25)
    pose.apply(0.3, false)
    expect(pose.weight).toBe(0)
    expect(bones[1].rotation.z).toBeCloseTo(0.25)
    weapon.geometry.dispose()
  })
})

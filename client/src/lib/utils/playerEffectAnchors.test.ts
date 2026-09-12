import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { PlayerEffectAnchors } from './playerEffectAnchors'

function rig(torsoName = 'Spine1') {
  const player = new THREE.Group()
  const rider = new THREE.Group()
  const torso = new THREE.Bone()
  torso.name = torsoName
  torso.position.y = 1.1
  const right = new THREE.Bone()
  right.name = 'RightHand'
  right.position.set(0.35, 0.1, 0)
  const left = new THREE.Bone()
  left.name = 'LeftHand'
  left.position.set(-0.35, 0.1, 0)
  torso.add(right, left)
  const mesh = new THREE.SkinnedMesh()
  mesh.bind(new THREE.Skeleton([torso, right, left]))
  rider.add(torso, mesh)
  player.add(rider)
  return { player, rider, torso, right, left }
}

describe('player effect anchors', () => {
  it('follows mounting, dismounting, turning and animated bones without a render', () => {
    const { player, rider, torso, right } = rig()
    const anchors = new PlayerEffectAnchors(rider)
    const target = new THREE.Vector3()
    for (const seatHeight of [0, 0.9, 1.0, 0]) {
      player.position.set(15, 7, -20)
      player.rotation.y += 0.3
      rider.position.y = seatHeight
      torso.rotation.z += 0.1
      right.position.y += 0.05
      for (const weapon of [false, true]) {
        expect(anchors.getWorldPosition(weapon, 'RightHand', target)).toBe(true)
        const bone = weapon ? right : torso
        expect(
          target.distanceTo(bone.getWorldPosition(new THREE.Vector3()))
        ).toBeLessThan(1e-6)
        if (!weapon) expect(target.y).toBeCloseTo(8.1 + seatHeight)
      }
    }
  })

  it('uses the wielding hand for bows and swords', () => {
    const { rider, left, right } = rig()
    const anchors = new PlayerEffectAnchors(rider)
    const target = new THREE.Vector3()
    for (const hand of ['LeftHand', 'RightHand'] as const) {
      expect(anchors.getWorldPosition(true, hand, target)).toBe(true)
      const bone = hand === 'LeftHand' ? left : right
      expect(target.equals(bone.getWorldPosition(new THREE.Vector3()))).toBe(
        true
      )
    }
  })

  it('supports a spine fallback and rejects models without an anchor', () => {
    const { rider, torso } = rig('Spine')
    const target = new THREE.Vector3()
    expect(
      new PlayerEffectAnchors(rider).getWorldPosition(
        false,
        'RightHand',
        target
      )
    ).toBe(true)
    expect(target.equals(torso.getWorldPosition(new THREE.Vector3()))).toBe(
      true
    )
    expect(
      new PlayerEffectAnchors(new THREE.Group()).getWorldPosition(
        false,
        'RightHand',
        target
      )
    ).toBe(false)
  })
})

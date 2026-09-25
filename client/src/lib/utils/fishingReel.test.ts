import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { FishingReel } from './fishingReel'
import type { FishingCatch } from '../stores/fishingStore'
import { WORLD_WIDTH_X } from '../terrain/world-wrap'

function rig() {
  const root = new THREE.Group()
  root.position.set(40, 3, -20)
  root.rotation.y = 0.6
  root.scale.setScalar(1.2)
  const bones: THREE.Bone[] = []
  const hands = ['Left', 'Right'].map((side, i) => {
    const sign = i === 0 ? 1 : -1
    const arm = new THREE.Bone()
    arm.name = `${side}Arm`
    arm.position.set(sign * 0.12, 1.5, 0)
    const elbow = new THREE.Bone()
    elbow.name = `${side}ForeArm`
    elbow.position.set(sign * 0.05, -0.25, 0.1)
    const hand = new THREE.Bone()
    hand.name = `${side}Hand`
    hand.position.set(-sign * 0.05, -0.12, 0.23)
    root.add(arm)
    arm.add(elbow)
    elbow.add(hand)
    bones.push(arm, elbow, hand)
    return hand
  })
  const skin = new THREE.SkinnedMesh(new THREE.BufferGeometry())
  root.add(skin)
  skin.bind(new THREE.Skeleton(bones))
  const prop = new THREE.Group()
  hands[1].add(prop)
  const anchor = (name: string, position: number[], parent = prop) => {
    const node = new THREE.Group()
    node.name = name
    node.position.fromArray(position)
    parent.add(node)
    return node
  }
  const grip = anchor('rod_grip', [0, 0.075, 0])
  const tip = anchor('rod_tip', [0, 0.075, 2])
  const rotor = anchor('reel_rotor', [0.05, 0, 0])
  anchor('reel_axis', [0, 1, 0], rotor)
  const handle = anchor('reel_handle', [0.04, 0.01, 0], rotor)
  root.updateMatrixWorld(true)
  return { root, hands, bones, prop, grip, tip, rotor, handle }
}

const point = (obj: THREE.Object3D) => obj.getWorldPosition(new THREE.Vector3())
const palm = (hand: THREE.Object3D) =>
  hand.localToWorld(new THREE.Vector3(0, 0.075, 0))

describe('fishing reel motion', () => {
  it('lifts the rod, takes the line in the right hand, and hangs the catch from its mouth', () => {
    const { root, hands, prop, tip } = rig()
    const motion = new FishingReel(root, prop)
    for (let i = 0; i < 90; i++) motion.update(1 / 60, 'hold', true)
    const before = point(tip)
    const caught: FishingCatch = {
      startedAt: 0,
      fish: { item_def_id: 'raw_trout', size_cm: 42, trophy: false },
      waterPosition: { x: 40 + WORLD_WIDTH_X, y: 3, z: -16 },
    }
    motion.update(1 / 60, null, true, caught, 0)
    expect(point(root.getObjectByName('landed_fish')!).x).toBeCloseTo(40)
    for (let i = 0; i <= 120; i++)
      motion.update(1 / 60, null, true, caught, (i * 1000) / 60)
    expect(point(tip).y - before.y).toBeGreaterThan(0.5)
    const fish = root.getObjectByName('landed_fish')!
    const rightPalm = palm(hands[1])
    expect(
      point(fish).distanceTo(
        rightPalm.clone().add(new THREE.Vector3(0, -0.3, 0))
      )
    ).toBeLessThan(1e-5)
    const line = root
      .getObjectByName('fishing_catch')!
      .children.find((obj) => obj instanceof THREE.Line) as THREE.Line
    const positions = line.geometry.getAttribute('position')
    expect(
      new THREE.Vector3()
        .fromBufferAttribute(positions, 1)
        .distanceTo(rightPalm)
    ).toBeLessThan(1e-5)
    expect(
      new THREE.Vector3()
        .fromBufferAttribute(positions, 2)
        .distanceTo(point(fish))
    ).toBeLessThan(1e-5)
    const forearm = point(hands[1]).sub(point(hands[1].parent!)).normalize()
    expect(
      forearm.dot(rightPalm.clone().sub(point(hands[1])).normalize())
    ).toBeGreaterThan(0.9999)
    motion.update(1 / 60, null, false, caught, 2050)
    expect(root.getObjectByName('fishing_catch')).toBeUndefined()
    motion.dispose()
  })

  it('keeps both hands on their targets while the crank turns independently of the rod', () => {
    const { root, hands, prop, grip, tip, rotor, handle } = rig()
    const motion = new FishingReel(root, prop)
    for (let i = 0; i < 180; i++) motion.update(1 / 60, 'reel', true)
    const tipBefore = point(tip)
    const handleBefore = point(handle)
    const rotorBefore = rotor.quaternion.clone()
    for (let i = 0; i < 30; i++) {
      motion.update(1 / 60, 'reel', true)
      expect(palm(hands[0]).distanceTo(point(grip))).toBeLessThan(1e-5)
      expect(palm(hands[1]).distanceTo(point(handle))).toBeLessThan(1e-5)
      expect(point(tip).distanceTo(tipBefore)).toBeLessThan(1e-5)
    }
    expect(rotor.quaternion.angleTo(rotorBefore)).toBeGreaterThan(1)
    expect(point(handle).distanceTo(handleBefore)).toBeGreaterThan(0.03)
  })

  it('reverses when giving line and stops on hold', () => {
    const { root, prop, rotor } = rig()
    const motion = new FishingReel(root, prop)
    const turn = (stance: 'reel' | 'giveline' | 'hold') => {
      for (let i = 0; i < 120; i++) motion.update(1 / 60, stance, true)
      const before = rotor.quaternion.clone()
      motion.update(1 / 60, stance, true)
      return before.invert().multiply(rotor.quaternion)
    }
    expect(turn('reel').y).toBeLessThan(-0.01)
    expect(turn('giveline').y).toBeGreaterThan(0.01)
    expect(turn('hold').angleTo(new THREE.Quaternion())).toBeLessThan(1e-6)
  })

  it('keeps the crank wrist straight through full turns in both directions', () => {
    const { root, hands, prop, handle } = rig()
    const motion = new FishingReel(root, prop)
    const hand = hands[1]
    for (const stance of ['reel', 'giveline'] as const) {
      for (let i = 0; i < 90; i++) motion.update(1 / 60, stance, true)
      for (let i = 0; i < 120; i++) {
        motion.update(1 / 60, stance, true)
        const forearm = point(hand).sub(point(hand.parent!)).normalize()
        const handDirection = palm(hand).sub(point(hand)).normalize()
        expect(forearm.dot(handDirection)).toBeGreaterThan(0.9999)
        expect(palm(hand).distanceTo(point(handle))).toBeLessThan(1e-5)
      }
    }
  })

  it('restores the authored pose on cast, movement, and weapon detach without accumulating drift', () => {
    const { root, bones, prop, tip } = rig()
    const poses = bones.map((bone) => bone.quaternion.clone())
    const originalTip = point(tip)
    const motion = new FishingReel(root, prop)
    for (let cycle = 0; cycle < 4; cycle++) {
      for (let i = 0; i < 120; i++) motion.update(1 / 60, 'reel', true)
      motion.update(1 / 60, null, false)
      expect(point(tip).distanceTo(originalTip)).toBeLessThan(1e-6)
      bones.forEach((bone, i) =>
        expect(bone.quaternion.angleTo(poses[i])).toBeLessThan(1e-6)
      )
    }
    motion.update(1 / 60, 'reel', true)
    motion.restore()
    expect(point(tip).distanceTo(originalTip)).toBeLessThan(1e-6)
  })

  it('leaves older unrigged rods alone', () => {
    const { root, prop, rotor, tip } = rig()
    rotor.removeFromParent()
    const before = point(tip)
    new FishingReel(root, prop).update(1 / 60, 'reel', true)
    expect(point(tip).distanceTo(before)).toBe(0)
  })
})

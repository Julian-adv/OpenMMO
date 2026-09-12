import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { HorseMount } from './horseMount'

function mount(animations: THREE.AnimationClip[] = [], withHead = true) {
  const scene = new THREE.Group()
  if (withHead) {
    const neck = new THREE.Bone()
    neck.name = 'Neck'
    const head = new THREE.Bone()
    head.name = 'Head'
    neck.add(head)
    scene.add(neck)
  }
  return new HorseMount({ scene, animations } as unknown as GLTF)
}

function headClip(name: string, yaw: number) {
  const values = [0, yaw, 0].flatMap((angle) =>
    new THREE.Quaternion()
      .setFromEuler(new THREE.Euler(0.5, angle, 0, 'YXZ'))
      .toArray()
  )
  return new THREE.AnimationClip(name, 1, [
    new THREE.QuaternionKeyframeTrack('Head.quaternion', [0, 0.5, 1], values),
  ])
}

describe('rider facing follows the animated horse head', () => {
  it.each([-0.8, 0, 0.8])(
    'follows turn yaw %s including neck rotation, pitch and a transformed parent',
    (yaw) => {
      const horse = mount()
      const parent = new THREE.Group()
      parent.position.set(1200, 50, 2300)
      parent.rotation.y = 2.9
      parent.add(horse.root)
      horse.root.getObjectByName('Neck')!.rotation.y = yaw / 2
      horse.root.getObjectByName('Head')!.rotation.set(0.7, yaw / 2, 0.2, 'YXZ')
      horse.update(0, 0, 0)
      for (let i = 1; i <= 90; i++) horse.update(1 / 60, 1, -i * 0.04)
      expect(horse.riderFacingYaw).toBeCloseTo(yaw, 6)
      horse.dispose()
    }
  )

  it.each([-1, 1])(
    'smooths the animated turn and its return for side %s',
    (side) => {
      const horse = mount([
        headClip('idle', 0),
        headClip(`turn_${side < 0 ? 'right' : 'left'}_90`, side * 0.8),
      ])
      horse.update(0, 0, 0)
      for (let i = 1; i <= 18; i++) horse.update(1 / 60, 1, side * i * 0.04)
      const peak = Math.abs(horse.riderFacingYaw)
      expect(peak).toBeGreaterThan(0.3)
      expect(peak).toBeLessThan(0.8)
      horse.update(1 / 60, 0, side * 18 * 0.04)
      expect(Math.abs(horse.riderFacingYaw)).toBeGreaterThan(peak * 0.9)
      for (let i = 0; i < 90; i++) horse.update(1 / 60, 0, side * 18 * 0.04)
      expect(horse.riderFacingYaw).toBeCloseTo(0, 3)
      horse.dispose()
    }
  )

  it.each([-1, 1])(
    'limits idle head turns to 30 degrees for side %s',
    (side) => {
      const horse = mount()
      const head = horse.root.getObjectByName('Head')!
      head.rotation.y = side * 0.2
      for (let i = 0; i < 120; i++) horse.update(1 / 60, 0, 0)
      expect(horse.riderFacingYaw).toBeCloseTo(side * 0.2, 5)
      head.rotation.y = side * (Math.PI - 0.01)
      for (let i = 0; i < 240; i++) {
        horse.update(1 / 60, 0, 0)
        expect(Math.abs(horse.riderFacingYaw)).toBeLessThanOrEqual(Math.PI / 6)
      }
      expect(horse.riderFacingYaw).toBeCloseTo((side * Math.PI) / 6, 5)
      head.rotation.y = -side * (Math.PI - 0.01)
      const before = horse.riderFacingYaw
      horse.update(1 / 60, 0, 0)
      expect(Math.abs(horse.riderFacingYaw - before)).toBeLessThan(0.11)
      for (let i = 0; i < 240; i++) horse.update(1 / 60, 0, 0)
      expect(horse.riderFacingYaw).toBeCloseTo((-side * Math.PI) / 6, 5)
      horse.dispose()
    }
  )

  it('eases back into the idle limit without snapping at the end of a turn', () => {
    const horse = mount()
    const head = horse.root.getObjectByName('Head')!
    head.rotation.y = 1
    horse.update(0, 0, 0)
    for (let i = 1; i <= 60; i++) horse.update(1 / 60, 1, i * 0.04)
    const before = horse.riderFacingYaw
    horse.update(1 / 60, 0, 2.4)
    expect(horse.riderFacingYaw).toBeGreaterThan(Math.PI / 6)
    expect(before - horse.riderFacingYaw).toBeLessThan(0.05)
    for (let i = 0; i < 120; i++) horse.update(1 / 60, 0, 2.4)
    expect(horse.riderFacingYaw).toBeCloseTo(Math.PI / 6, 5)
    horse.dispose()
  })

  it('returns smoothly at the same rate across frame rates and pauses', () => {
    const simulate = (fps: number) => {
      const horse = mount()
      horse.riderFacingYaw = 0.5
      horse.update(0, 0, 0)
      expect(horse.riderFacingYaw).toBe(0.5)
      for (let i = 0; i < fps / 2; i++) horse.update(1 / fps, 0, 0)
      const result = horse.riderFacingYaw
      expect(result).toBeGreaterThan(0)
      expect(result).toBeLessThan(0.03)
      horse.dispose()
      return result
    }
    expect(simulate(30)).toBeCloseTo(simulate(120), 8)
  })

  it('leaves the rider facing forward when the head is missing', () => {
    const horse = mount([], false)
    horse.update(1 / 60, 0, 0)
    horse.update(1 / 60, 1, 0.05)
    expect(horse.riderFacingYaw).toBe(0)
    horse.dispose()
  })
})

describe('mounted backward walking', () => {
  it('plays the walk cycle backward while keeping the body facing unchanged', () => {
    const clip = new THREE.AnimationClip('walk', 1, [
      new THREE.NumberKeyframeTrack('Head.position[x]', [0, 1], [0, 1]),
    ])
    const horse = mount([clip])
    const head = horse.root.getObjectByName('Head')!
    horse.update(0, 0, 0, { x: 0, z: 0 })
    horse.update(0.1, 2, 0, { x: 0, z: -0.2 })
    expect(head.position.x).toBeCloseTo(0.9)
    horse.update(0.1, 2, 0, { x: 0, z: -0.4 })
    expect(head.position.x).toBeCloseTo(0.8)
    expect(horse.root.rotation.y).toBe(0)
    horse.update(0.1, 2, 0, { x: 0, z: -0.2 })
    expect(head.position.x).toBeCloseTo(0.9)
    horse.dispose()
  })

  it('does not interpret a teleport as backward movement', () => {
    const horse = mount([
      new THREE.AnimationClip('walk', 1, [
        new THREE.NumberKeyframeTrack('Head.position[x]', [0, 1], [0, 1]),
      ]),
    ])
    horse.update(0, 0, 0, { x: 0, z: 0 })
    horse.update(0.1, 2, 0, { x: 0, z: -20 })
    expect(horse.root.getObjectByName('Head')!.position.x).toBeCloseTo(0.1)
    horse.dispose()
  })
})

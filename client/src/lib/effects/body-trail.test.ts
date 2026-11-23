import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import { BodyTrail } from './body-trail'

function serpent() {
  const group = new THREE.Group()
  const chain = (names: string[], step: number) => {
    let parent: THREE.Object3D = group
    names.forEach((name, i) => {
      const bone = new THREE.Bone()
      bone.name = name
      bone.position.set(0, 0, i === 0 ? step / 2 : step)
      parent.add(bone)
      parent = bone
    })
  }
  chain(['Neck1', 'Neck2', 'Head'], 0.25)
  chain(
    Array.from({ length: 10 }, (_, i) => `Tail${i + 1}`),
    -0.3
  )
  return group
}

function heading(group: THREE.Group, name: string) {
  const bone = group.getObjectByName(name)!
  const forward = new THREE.Vector3(0, 0, 1).applyQuaternion(
    bone.getWorldQuaternion(new THREE.Quaternion())
  )
  return Math.atan2(forward.x, forward.z)
}

function frame(
  group: THREE.Group,
  trail: BodyTrail,
  x: number,
  z: number,
  facing: number,
  now = 0,
  steering?: Parameters<BodyTrail['track']>[4]
) {
  group.traverse((o) => {
    if (o instanceof THREE.Bone) o.quaternion.identity()
  })
  const yaw = trail.track(x, z, facing, now, steering)
  group.position.set(x, 0, z)
  group.rotation.y = yaw
  trail.pose(yaw, facing)
}

describe('BodyTrail', () => {
  it('keeps the body in place while only the head turns to face', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    frame(group, trail, 0, 0, 0)
    const facing = THREE.MathUtils.degToRad(80)
    for (let t = 16; t <= 480; t += 16) frame(group, trail, 0, 0, facing, t)
    expect(heading(group, 'Tail5')).toBeCloseTo(0, 5)
    expect(heading(group, 'Head')).toBeCloseTo(facing, 5)
  })

  it('swings round slowly at rest when the target is behind', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    frame(group, trail, 0, 0, 0, 0)
    for (let t = 16; t <= 3000; t += 16) frame(group, trail, 0, 0, Math.PI, t)
    expect(Math.abs(heading(group, 'Tail5'))).toBeGreaterThan(Math.PI / 2)
  })

  it('lays the tail along the path the neck crawled', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    let t = 0
    frame(group, trail, 0, 0, Math.PI / 2, t)
    for (let x = 0.05; x <= 3; x += 0.05)
      frame(group, trail, x, 0, Math.PI / 2, (t += 25))
    for (let z = 0.05; z <= 1.2; z += 0.05)
      frame(group, trail, 3, z, 0, (t += 25))
    for (let i = 0; i < 40; i++) frame(group, trail, 3, 1.2, 0, (t += 25))
    // The front of the body has turned the corner; the back still lies along X.
    expect(Math.abs(heading(group, 'Tail1'))).toBeLessThan(0.3)
    expect(Math.abs(heading(group, 'Tail10') - Math.PI / 2)).toBeLessThan(0.3)
  })

  it('glides past a snap in the neck instead of kinking', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    let t = 0
    frame(group, trail, 0, 0, 0, t)
    frame(group, trail, 1.5, 0, 0, (t += 16))
    expect(heading(group, 'Tail10')).toBeCloseTo(0, 5)
  })

  it('limits frame-to-frame steering through a chase corner and target reversal', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    frame(group, trail, 0, 0, 0)
    const bones = [
      'Head',
      ...Array.from({ length: 10 }, (_, i) => `Tail${i + 1}`),
    ]
    let previous = bones.map((bone) => heading(group, bone))
    for (let i = 1; i <= 90; i++) {
      const x = Math.min(Math.max(i - 15, 0), 20) * 0.1
      const z = Math.min(i, 15) * 0.1
      const facing = i < 16 ? 0 : i < 36 ? Math.PI / 2 : -Math.PI / 2
      frame(group, trail, x, z, facing, (i * 1000) / 60)
      const current = bones.map((bone) => heading(group, bone))
      current.forEach((yaw, k) => {
        const delta = yaw - previous[k]
        const step = Math.abs(Math.atan2(Math.sin(delta), Math.cos(delta)))
        expect(step).toBeLessThanOrEqual(
          ((k === 0 ? 2.5 : 1) * Math.PI) / 60 + 1e-6
        )
      })
      previous = current
    }
  })

  it('ignores a neck that wobbles in place', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    frame(group, trail, 0, 0, 0)
    for (let i = 0; i < 60; i++) {
      const wobble = i % 2 ? 0.2 : -0.2
      frame(group, trail, wobble, wobble * 0.5, 0, (i + 1) * 16)
    }
    expect(heading(group, 'Tail10')).toBeCloseTo(0, 5)
  })

  it('starts a fresh trail and facing on teleport', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    frame(group, trail, 0, 0, 0)
    frame(group, trail, 0.1, 0, 1, 16)
    frame(group, trail, 10, 10, -1, 32)
    for (const name of ['Head', 'Tail1', 'Tail10'])
      expect(heading(group, name)).toBeCloseTo(-1, 5)
  })

  it('keeps a travelling boss on course through brief facing corrections', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    const course = 0.9
    for (let i = 0; i < 90; i++) {
      const d = (i * 4.6) / 30
      const facing = i % 12 === 6 ? course - Math.PI : course
      frame(
        group,
        trail,
        d * Math.sin(course),
        d * Math.cos(course),
        facing,
        (i * 1000) / 30,
        'travel'
      )
      for (const bone of ['Head', 'Tail1', 'Tail10'])
        expect(heading(group, bone)).toBeCloseTo(course, 5)
    }
  })

  it('releases a curved trail for a rear strike and resumes from the new heading', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    let t = 0
    frame(group, trail, 0, 0, 0, t, 'travel')
    for (let i = 1; i <= 30; i++)
      frame(group, trail, 0, i * 0.1, 0, (t += 1000 / 60), 'travel')
    for (let i = 1; i <= 24; i++)
      frame(group, trail, i * 0.1, 3, Math.PI / 2, (t += 1000 / 60), 'travel')
    expect(
      Math.abs(heading(group, 'Tail1') - heading(group, 'Tail10'))
    ).toBeGreaterThan(0.4)
    const facing = -Math.PI / 2
    for (let i = 0; i < 96; i++) {
      const prev = group.rotation.y
      frame(group, trail, 2.4, 3, facing, (t += 1000 / 60), 'face')
      const delta = group.rotation.y - prev
      expect(
        Math.abs(Math.atan2(Math.sin(delta), Math.cos(delta)))
      ).toBeLessThanOrEqual(Math.PI / 20 + 1e-6)
    }
    for (const bone of ['Head', 'Tail1', 'Tail10'])
      expect(heading(group, bone)).toBeCloseTo(facing, 5)
    frame(group, trail, 2.3, 3, facing, (t += 1000 / 60), 'travel')
    expect(heading(group, 'Tail10')).toBeCloseTo(facing, 5)
  })

  it('does not stack turns on bones no clip rewrote', () => {
    const group = serpent()
    const trail = new BodyTrail(group)
    frame(group, trail, 0, 0, 0)
    // No reset between frames, as when an action has stopped.
    for (let i = 1; i <= 30; i++) {
      const yaw = trail.track(0, 0, 1, i * 16)
      group.rotation.y = yaw
      trail.pose(yaw, 1)
    }
    expect(heading(group, 'Head')).toBeCloseTo(1, 5)
  })
})

import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { TwoHandedGrip } from './twoHandedGrip'

function rig() {
  const root = new THREE.Group()
  root.position.set(40, 12, -18)
  root.rotation.set(0.2, 1.1, -0.1)
  root.scale.setScalar(1.4)
  const right = new THREE.Object3D()
  right.position.set(0.2, 1.2, 0.1)
  right.rotation.set(-0.4, 0.3, 0.2)
  const left = new THREE.Object3D()
  left.position.set(0, 1.1, 0.1)
  root.add(right, left)
  const index = new THREE.Object3D()
  const thumb = new THREE.Object3D()
  index.position.set(0, 0.09, 0.01)
  thumb.position.set(0, 0.07, -0.01)
  left.add(index, thumb)
  const prop = new THREE.Object3D()
  prop.position.set(0, 0.08, 0)
  prop.rotation.set(-1.907, -0.475, 0.139)
  right.add(prop)
  return { root, right, left, index, thumb, prop }
}

function expectOnHandle(prop: THREE.Object3D, point: THREE.Vector3) {
  prop.worldToLocal(point)
  expect(point.x).toBeLessThan(0)
  expect(Math.hypot(point.y, point.z)).toBeLessThan(1e-6)
}

describe('two-handed grip', () => {
  it('keeps both grips on the handle as the hands and player move', () => {
    const { root, right, left, index, thumb, prop } = rig()
    const grip = new TwoHandedGrip(prop, left, 0.3, [index, thumb])
    const originalRightRotation = right.quaternion.clone()
    for (let frame = 0; frame < 20; frame++) {
      root.position.x += 2
      root.rotation.y += 0.1
      left.position.y += 0.003
      left.rotation.z += 0.02
      const rightGrip = prop.getWorldPosition(new THREE.Vector3())
      grip.update(true)
      const leftGrip = index
        .getWorldPosition(new THREE.Vector3())
        .add(thumb.getWorldPosition(new THREE.Vector3()))
        .multiplyScalar(0.5)
      expectOnHandle(prop, leftGrip)
      expect(
        prop.getWorldPosition(new THREE.Vector3()).distanceTo(rightGrip)
      ).toBeLessThan(1e-6)
      expect(right.quaternion.equals(originalRightRotation)).toBe(true)
    }
  })

  it('restores the authored grip outside two-handed animations', () => {
    const { left, index, thumb, prop } = rig()
    const original = prop.quaternion.clone()
    const grip = new TwoHandedGrip(prop, left, 0.3, [index, thumb])
    grip.update(true)
    expect(prop.quaternion.angleTo(original)).toBeGreaterThan(0.1)
    grip.update(false)
    expect(prop.quaternion.equals(original)).toBe(true)
  })

  it('uses the palm when the rig has no finger bones', () => {
    const { left, prop } = rig()
    const grip = new TwoHandedGrip(prop, left, 0.3)
    grip.update(true)
    expectOnHandle(prop, left.localToWorld(new THREE.Vector3(0, 0.08, 0)))
  })

  it('keeps a finite authored rotation when both grips coincide', () => {
    const { right, prop } = rig()
    const point = new THREE.Object3D()
    point.position.copy(prop.position)
    right.add(point)
    const original = prop.quaternion.clone()
    new TwoHandedGrip(prop, point, 0.3, [point]).update(true)
    expect(prop.quaternion.equals(original)).toBe(true)
  })

  it('lets the off hand release without pulling the sword away', () => {
    const { right, prop } = rig()
    const finger = new THREE.Object3D()
    right.add(finger)
    const original = prop.quaternion.clone()
    const grip = new TwoHandedGrip(prop, finger, 0.3, [finger])
    const rotationAt = (distance: number) => {
      finger.position.copy(prop.position).add(new THREE.Vector3(0, 0, distance))
      grip.update(true)
      return prop.quaternion.angleTo(original)
    }
    const held = rotationAt(0.3)
    const releasing = rotationAt(0.375)
    expect(releasing).toBeGreaterThan(0)
    expect(releasing).toBeLessThan(held)
    expect(rotationAt(0.46)).toBeLessThan(1e-7)
  })
})

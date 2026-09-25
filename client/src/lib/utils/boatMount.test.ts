import { readFileSync } from 'node:fs'
import { beforeAll, describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { GLTFLoader, type GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { BoatMount } from './boatMount'

let gltf: GLTF
beforeAll(async () => {
  const file = readFileSync(
    new URL('../../../public/models/mounts/rowboat.glb', import.meta.url)
  )
  gltf = await new GLTFLoader()
    .register(() => ({
      name: 'headless-materials',
      loadMaterial: async () => new THREE.MeshBasicMaterial(),
    }))
    .parseAsync(
      file.buffer.slice(file.byteOffset, file.byteOffset + file.byteLength),
      ''
    )
})

function advance(boat: BoatMount, seconds: number, speed: number, fps = 60) {
  for (let i = 0; i < seconds * fps; i++) boat.update(1 / fps, speed)
}

describe('rowboat strokes', () => {
  it('seats the rower over the middle thwart facing the stern', () => {
    const boat = new BoatMount(gltf)
    const middle = boat.root.getObjectByName('ThwartMid')!
    expect(boat.seat.position.x).toBeCloseTo(middle.position.x)
    expect(boat.seat.position.z).toBeCloseTo(middle.position.z)
    expect(boat.seat.position.y - middle.position.y).toBeCloseTo(0.02)
    const forward = new THREE.Vector3(0, 0, 1).applyQuaternion(
      boat.seat.quaternion
    )
    expect(forward.distanceTo(new THREE.Vector3(0, 0, -1))).toBeLessThan(1e-6)
    expect(boat.grips.map((grip) => grip.parent?.name)).toEqual([
      'OarPort',
      'OarStarboard',
    ])
  })

  it('pulls the handles toward the rower while submerged blades push water astern', () => {
    const boat = new BoatMount(gltf)
    advance(boat, 3, 3)
    const oar = boat.root.getObjectByName('OarPort')!
    const blade = () =>
      boat.root.worldToLocal(
        oar.localToWorld(
          new THREE.Vector3(
            -Math.sin(Math.PI / 30),
            0,
            Math.cos(Math.PI / 30)
          ).multiplyScalar(0.924)
        )
      )
    const grip = () =>
      boat.root.worldToLocal(
        boat.grips[0].getWorldPosition(new THREE.Vector3())
      )
    const forward = new THREE.Vector3(0, 0, 1).applyQuaternion(
      boat.seat.quaternion
    )
    let pullingFrames = 0
    for (let i = 0; i < 114; i++) {
      const previousBlade = blade()
      const previousGrip = grip()
      const previousLean = boat.riderLean
      boat.update(1 / 60, 3)
      const currentBlade = blade()
      if (previousBlade.y < -0.01 && currentBlade.y < -0.01) {
        expect(currentBlade.z).toBeLessThan(previousBlade.z)
        expect(grip().sub(previousGrip).dot(forward)).toBeLessThan(0)
        expect(boat.riderLean).toBeLessThan(previousLean)
        pullingFrames++
      }
    }
    expect(pullingFrames).toBeGreaterThan(20)
  })

  it('pivots both real oars at the gunwales and lifts the blades during recovery', () => {
    const boat = new BoatMount(gltf)
    advance(boat, 3, 3)
    const parent = new THREE.Group()
    parent.position.set(1200, 50, 2300)
    parent.rotation.y = 1.2
    parent.add(boat.root)
    const heights: number[] = []
    const normalHeights: number[] = []
    for (let i = 0; i < 114; i++) {
      boat.update(1 / 60, 3)
      for (const [name, side] of [
        ['OarStarboard', 1],
        ['OarPort', -1],
      ] as const) {
        const oar = boat.root.getObjectByName(name)!
        const axis = new THREE.Vector3(
          side * Math.sin(Math.PI / 30),
          0,
          Math.cos(Math.PI / 30)
        )
        const pivot = boat.root.worldToLocal(
          oar.localToWorld(axis.clone().multiplyScalar(-0.55))
        )
        expect(
          pivot.distanceTo(new THREE.Vector3(side * 0.57, 0.44, -0.2))
        ).toBeLessThan(1e-6)
        const blade = boat.root.worldToLocal(
          oar.localToWorld(axis.multiplyScalar(0.924))
        )
        expect(blade.x * side).toBeGreaterThan(1.5)
        if (side === 1) {
          heights.push(blade.y)
          normalHeights.push(
            Math.abs(
              new THREE.Vector3(0, 1, 0).applyQuaternion(oar.quaternion).y
            )
          )
        }
      }
    }
    expect(Math.min(...heights)).toBeLessThan(-0.1)
    expect(Math.max(...heights)).toBeGreaterThan(0.14)
    expect(normalHeights[heights.indexOf(Math.min(...heights))]).toBeLessThan(
      0.05
    )
    expect(Math.max(...normalHeights)).toBeGreaterThan(0.95)
  })

  it('holds the oars still and spread before stowing without changing other boats', () => {
    const boat = new BoatMount(gltf)
    const other = new BoatMount(gltf)
    const source = gltf.scene.getObjectByName('OarPort')!
    const rest = source.position.clone()
    advance(boat, 2, 3)
    const oar = boat.root.getObjectByName('OarPort')!
    const before = oar.position.clone()
    boat.update(1 / 60, 0)
    expect(oar.position.distanceTo(before)).toBeLessThan(0.15)
    expect(boat.rowingWeight).toBeGreaterThan(0.8)
    advance(boat, 2, 0)
    const heldPosition = oar.position.clone()
    const heldRotation = oar.quaternion.clone()
    advance(boat, 2, 0)
    expect(boat.rowingWeight).toBeGreaterThan(0.99)
    expect(Math.abs(boat.riderLean)).toBeLessThan(1e-6)
    expect(oar.position.distanceTo(rest)).toBeGreaterThan(0.5)
    expect(oar.position.distanceTo(heldPosition)).toBeLessThan(1e-6)
    expect(oar.quaternion.angleTo(heldRotation)).toBeLessThan(1e-6)
    advance(boat, 5, 0)
    expect(boat.rowingWeight).toBeLessThan(0.001)
    expect(oar.position.distanceTo(rest)).toBeLessThan(1e-6)
    expect(oar.quaternion.angleTo(source.quaternion)).toBeLessThan(1e-6)
    expect(source.position.equals(rest)).toBe(true)
    expect(other.root.getObjectByName('OarPort')!.position.equals(rest)).toBe(
      true
    )
    expect(source.children).toHaveLength(0)
  })

  it('leaves the oars stowed until the boat first moves', () => {
    const boat = new BoatMount(gltf)
    const oar = boat.root.getObjectByName('OarPort')!
    const rest = oar.position.clone()
    advance(boat, 10, 0)
    expect(boat.rowingWeight).toBe(0)
    expect(oar.position.equals(rest)).toBe(true)
  })

  it('resumes from the spread pose and renews the hold after another stop', () => {
    const boat = new BoatMount(gltf)
    advance(boat, 2, 3)
    advance(boat, 4, 0)
    const oar = boat.root.getObjectByName('OarPort')!
    const held = oar.position.clone()
    boat.update(1 / 60, 3)
    expect(boat.rowingWeight).toBeGreaterThan(0.99)
    expect(oar.position.distanceTo(held)).toBeLessThan(0.05)
    advance(boat, 0.5, 3)
    expect(oar.position.distanceTo(held)).toBeGreaterThan(0.1)
    advance(boat, 4, 0)
    expect(boat.rowingWeight).toBeGreaterThan(0.99)
    advance(boat, 2, 0)
    expect(boat.rowingWeight).toBeLessThan(0.03)
  })

  it('cancels the hold when another action needs the hands', () => {
    const boat = new BoatMount(gltf)
    advance(boat, 2, 3)
    boat.update(1 / 60, 0, false)
    advance(boat, 2, 0)
    expect(boat.rowingWeight).toBeLessThan(0.001)
  })

  it('keeps timing consistent across frame rates, reverse speed and paused frames', () => {
    const slow = new BoatMount(gltf)
    const fast = new BoatMount(gltf)
    advance(slow, 2, 3, 30)
    advance(fast, 2, -3, 120)
    slow.update(0, 0)
    expect(slow.rowingWeight).toBeCloseTo(fast.rowingWeight, 8)
    expect(slow.riderLean).toBeCloseTo(fast.riderLean, 8)
    const a = slow.root.getObjectByName('OarPort')!
    const b = fast.root.getObjectByName('OarPort')!
    expect(a.position.distanceTo(b.position)).toBeLessThan(1e-8)
    expect(a.quaternion.angleTo(b.quaternion)).toBeLessThan(1e-6)
    advance(slow, 6, 0, 30)
    advance(fast, 6, 0, 120)
    const held = slow.rowingWeight
    slow.update(0, 0)
    expect(slow.rowingWeight).toBe(held)
    expect(slow.rowingWeight).toBeCloseTo(fast.rowingWeight, 8)
    expect(a.position.distanceTo(b.position)).toBeLessThan(1e-8)
    expect(a.quaternion.angleTo(b.quaternion)).toBeLessThan(1e-6)
  })
})

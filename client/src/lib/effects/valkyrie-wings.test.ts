import { existsSync } from 'node:fs'
import * as THREE from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { describe, expect, it } from 'vitest'
import { bindModularPart } from '../utils/modularCharacter'
import { loadHeadlessGlb } from '../utils/headless-glb.fixture'
import {
  attachValkyrieWings,
  disposeValkyrieWings,
  updateValkyrieWings,
} from './valkyrie-wings'

const directory = 'models/characters/modular_female'
const hasAssets = existsSync(`public/${directory}/valkyrie_wings.glb`)
const identity = new THREE.Quaternion()

async function wearer() {
  const [body, wings] = await Promise.all([
    loadHeadlessGlb(`${directory}/base.glb`),
    loadHeadlessGlb(`${directory}/valkyrie_wings.glb`),
  ])
  const root = clone(body.scene)
  const skinned = bindModularPart(root, wings.scene)
  return { root, skinned }
}

function pivots(root: THREE.Object3D) {
  const values: THREE.Object3D[] = []
  root.traverse((node) => {
    if (node.userData.valkyrie_wing) values.push(node)
  })
  return values
}

function actionFor(root: THREE.Object3D, name: string) {
  const mixer = new THREE.AnimationMixer(root)
  const action = mixer.clipAction(new THREE.AnimationClip(name, 1, []))
  return { action, mixer }
}

describe.skipIf(!hasAssets)('Valkyrie wing hinges', () => {
  it('preserves the skinned pose and follows a rotated torso without changing geometry', async () => {
    const { root, skinned } = await wearer()
    root.position.set(4, 2, -3)
    root.rotation.y = 0.7
    root.getObjectByName('Spine2')!.rotation.x += 0.23
    root.updateMatrixWorld(true)
    const samples = skinned.map((mesh) => {
      const values: [number, THREE.Vector3][] = []
      for (let i = 0; i < mesh.geometry.attributes.position.count; i += 37)
        values.push([
          i,
          mesh.localToWorld(mesh.getVertexPosition(i, new THREE.Vector3())),
        ])
      return values
    })
    const wings = attachValkyrieWings(root, skinned)
    root.updateMatrixWorld(true)
    for (const [side, wing] of wings.entries()) {
      expect(wing.geometry).toBe(skinned[side].geometry)
      expect(wing.material).toBe(skinned[side].material)
      for (const [index, expected] of samples[side])
        expect(
          wing
            .localToWorld(wing.getVertexPosition(index, new THREE.Vector3()))
            .distanceTo(expected)
        ).toBeLessThan(1e-6)
    }
    const { action } = actionFor(root, 'walk')
    action.time = 0.25
    updateValkyrieWings(root, 0, action)
    root.updateMatrixWorld(true)
    const before = wings[0].localToWorld(
      wings[0].getVertexPosition(100, new THREE.Vector3())
    )
    root.position.x += 3
    root.updateMatrixWorld(true)
    const after = wings[0].localToWorld(
      wings[0].getVertexPosition(100, new THREE.Vector3())
    )
    expect(
      after.sub(before).distanceTo(new THREE.Vector3(3, 0, 0))
    ).toBeLessThan(1e-6)
    disposeValkyrieWings(root)
  })

  it('breathes gently at idle and preserves both rigid wing shapes', async () => {
    const { root, skinned } = await wearer()
    const wings = attachValkyrieWings(root, skinned)
    const { action } = actionFor(root, 'idle1')
    const pivot = pivots(root)[0]
    const spans = wings.map((wing) =>
      wing
        .getVertexPosition(0, new THREE.Vector3())
        .distanceTo(wing.getVertexPosition(100, new THREE.Vector3()))
    )
    const angles: number[] = []
    const a = new THREE.Vector3(),
      b = new THREE.Vector3()
    for (let frame = 0; frame < 540; frame++) {
      updateValkyrieWings(root, 1 / 60, action)
      root.updateMatrixWorld(true)
      if (frame >= 270) angles.push(pivot.quaternion.angleTo(identity))
      for (const [i, wing] of wings.entries()) {
        wing.localToWorld(wing.getVertexPosition(0, a))
        wing.localToWorld(wing.getVertexPosition(100, b))
        expect(a.distanceTo(b)).toBeCloseTo(spans[i], 6)
      }
    }
    expect(Math.max(...angles) - Math.min(...angles)).toBeGreaterThan(0.08)
    expect(Math.max(...angles)).toBeLessThanOrEqual(
      THREE.MathUtils.degToRad(10)
    )
    const paused = pivot.quaternion.clone()
    for (let i = 0; i < 10; i++) updateValkyrieWings(root, 0, action)
    expect(pivot.quaternion.angleTo(paused)).toBeLessThan(1e-7)
    disposeValkyrieWings(root)
  })

  it('uses the walking clip phase for two mirrored folds per stride and smooth transitions', async () => {
    const { root, skinned } = await wearer()
    attachValkyrieWings(root, skinned)
    const { action } = actionFor(root, 'torch_sword_walk')
    const [left, right] = pivots(root)
    const angles: number[] = []
    for (const phase of [0, 0.125, 0.25, 0.5, 0.75, 1]) {
      action.time = phase
      updateValkyrieWings(root, 0, action)
      angles.push(left.quaternion.angleTo(identity))
      expect(left.quaternion.x).toBeCloseTo(-right.quaternion.x, 7)
      expect(left.quaternion.y).toBeCloseTo(-right.quaternion.y, 7)
      expect(left.quaternion.z).toBeCloseTo(-right.quaternion.z, 7)
      expect(left.quaternion.w).toBeCloseTo(right.quaternion.w, 7)
    }
    expect(angles[0]).toBeCloseTo(angles[3], 7)
    expect(angles[3]).toBeCloseTo(angles[5], 7)
    expect(angles[2]).toBeCloseTo(angles[4], 7)
    expect(angles[2] - angles[0]).toBeCloseTo(THREE.MathUtils.degToRad(32), 7)
    const { action: idle } = actionFor(root, 'idle_armed')
    const previous = left.quaternion.clone()
    updateValkyrieWings(root, 1 / 60, idle)
    expect(previous.angleTo(left.quaternion)).toBeLessThan(0.02)
    disposeValkyrieWings(root)
  })

  it.each([
    { name: 'walk', speed: 1, fps: 60 },
    { name: 'run', speed: 1.5, fps: 30 },
  ])(
    'keeps $name folds visible during smoothed playback at $fps Hz',
    async ({ name, speed, fps }) => {
      const { root, skinned } = await wearer()
      attachValkyrieWings(root, skinned)
      const { action, mixer } = actionFor(root, name)
      action.timeScale = speed
      action.play()
      const pivot = pivots(root)[0]
      const angles: number[] = []
      for (let frame = 0; frame < fps * 4; frame++) {
        mixer.update(1 / fps)
        updateValkyrieWings(root, 1 / fps, action)
        if (frame >= fps * 2) angles.push(pivot.quaternion.angleTo(identity))
      }
      const min = Math.min(...angles)
      const max = Math.max(...angles)
      expect(min).toBeGreaterThanOrEqual(THREE.MathUtils.degToRad(8))
      expect(max).toBeLessThanOrEqual(THREE.MathUtils.degToRad(40))
      expect(max - min).toBeGreaterThan(THREE.MathUtils.degToRad(20))
      disposeValkyrieWings(root)
    }
  )

  it('shares static buffers between wearers while keeping pivots and playback independent', async () => {
    const { root, skinned } = await wearer()
    const original = skinned.map((mesh) => mesh.geometry)
    const wings = attachValkyrieWings(root, skinned)
    const second = clone(root)
    const [firstPivot] = pivots(root)
    const [secondPivot] = pivots(second)
    const versions = wings.flatMap((wing) =>
      Object.values(wing.geometry.attributes).map((attribute) => {
        const buffer =
          attribute instanceof THREE.InterleavedBufferAttribute
            ? attribute.data
            : attribute
        return { buffer, version: buffer.version }
      })
    )
    const { action } = actionFor(root, 'walk')
    action.time = 0.25
    for (let i = 0; i < 60; i++) updateValkyrieWings(root, 1 / 60, action)
    expect(firstPivot.quaternion.angleTo(identity)).toBeGreaterThan(0.1)
    expect(secondPivot.quaternion.angleTo(identity)).toBe(0)
    for (const [i, wing] of wings.entries()) {
      expect(wing.geometry).toBe(original[i])
      expect(second.getObjectByName(wing.name)).toBeInstanceOf(THREE.Mesh)
      expect((second.getObjectByName(wing.name) as THREE.Mesh).geometry).toBe(
        original[i]
      )
    }
    for (const { buffer, version } of versions)
      expect(buffer.version).toBe(version)
    disposeValkyrieWings(root)
    disposeValkyrieWings(second)
  })

  it('initializes wings attached after an empty preview update', async () => {
    const { root, skinned } = await wearer()
    const { action } = actionFor(root, 'walk')
    updateValkyrieWings(root, 1 / 60, action)
    attachValkyrieWings(root, skinned)
    action.time = 0.25
    updateValkyrieWings(root, 0, action)
    expect(pivots(root)).toHaveLength(2)
    expect(pivots(root)[0].quaternion.angleTo(identity)).toBeGreaterThan(0.1)
    disposeValkyrieWings(root)
  })
})

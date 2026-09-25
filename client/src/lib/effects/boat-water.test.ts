import { readFileSync } from 'node:fs'
import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest'
import * as THREE from 'three'
import { GLTFLoader, type GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { BoatMount } from '../utils/boatMount'
import { BoatWaterEffects } from './boat-water'

let gltf: GLTF
beforeEach(() => {
  let seed = 17
  vi.spyOn(Math, 'random').mockImplementation(() => {
    seed = (seed * 1664525 + 1013904223) >>> 0
    return seed / 2 ** 32
  })
})
afterEach(() => vi.restoreAllMocks())
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

function setup(surfaceAt: (x: number, z: number) => number | null = () => 0) {
  const boat = new BoatMount(gltf)
  const vessel = new THREE.Group()
  vessel.add(boat.root)
  const texture = new THREE.Texture()
  const effect = new BoatWaterEffects(boat, texture, surfaceAt)
  const camera = new THREE.PerspectiveCamera()
  const foam = effect.group.getObjectByName(
    'boatWakeFoam'
  ) as THREE.InstancedMesh
  const spray = effect.group.getObjectByName(
    'boatOarSpray'
  ) as THREE.InstancedMesh
  const step = (speed: number, rowing = speed !== 0, animateOars = true) => {
    vessel.position.z += speed / 60
    boat.update(1 / 60, animateOars && rowing ? 3 : 0)
    effect.update(1 / 60, camera, rowing)
  }
  return { boat, vessel, texture, effect, camera, foam, spray, step }
}

function positions(mesh: THREE.InstancedMesh) {
  const matrix = new THREE.Matrix4()
  return Array.from({ length: mesh.count }, (_, i) => {
    mesh.getMatrixAt(i, matrix)
    return new THREE.Vector3().setFromMatrixPosition(matrix)
  })
}

describe('boat water effects', () => {
  it('leaves a spreading wake in world space and fades it after stopping', () => {
    const { vessel, effect, foam, spray, step } = setup()
    for (let i = 0; i < 120; i++) step(3, true, false)
    const wake = positions(foam)
    expect(wake.length).toBeGreaterThan(50)
    expect(wake.some((p) => p.x < -1)).toBe(true)
    expect(wake.some((p) => p.x > 1)).toBe(true)
    expect(wake.some((p) => p.z < vessel.position.z - 2)).toBe(true)
    expect(spray.count).toBe(0)
    const before = positions(foam)
    const matrices = Array.from(foam.instanceMatrix.array).slice(
      0,
      foam.count * 16
    )
    vessel.rotation.y = Math.PI / 2
    for (let i = 0; i < 30; i++) step(0, false, false)
    const after = positions(foam)
    expect(after.length).toBeLessThan(before.length)
    for (let i = 0; i < after.length; i++) {
      const original = before.findIndex((point) => point.equals(after[i]))
      expect(original).toBeGreaterThanOrEqual(0)
      expect(
        Array.from(foam.instanceMatrix.array).slice(i * 16, (i + 1) * 16)
      ).toEqual(matrices.slice(original * 16, (original + 1) * 16))
    }
    for (let i = 0; i < 60; i++) step(0, false, false)
    expect(foam.count).toBeGreaterThan(0)
    expect(foam.count).toBeLessThan(before.length)
    for (const point of positions(foam))
      expect(before.some((original) => original.equals(point))).toBe(true)
    for (let i = 0; i < 180; i++) step(0)
    expect(foam.count).toBe(0)
    expect(spray.count).toBe(0)
    effect.dispose()
  })

  it('leaves stopped foam in place when the boat starts moving again', () => {
    const { effect, foam, step } = setup()
    for (let i = 0; i < 60; i++) step(3, true, false)
    step(0, false, false)
    const stopped = positions(foam)
    for (let i = 0; i < 30; i++) step(3, true, false)
    expect(foam.count).toBeGreaterThan(stopped.length)
    const lastZ = Math.max(...stopped.map((point) => point.z))
    const remaining = positions(foam).filter((point) => point.z <= lastZ)
    expect(remaining.length).toBeGreaterThan(0)
    for (const point of remaining)
      expect(stopped.some((original) => original.equals(point))).toBe(true)
    effect.dispose()
  })

  it('emits splashes at actual blade entries and stops while the oars are held', () => {
    const { boat, effect, spray, step } = setup()
    let entries = 0
    let previousCount = 0
    for (let i = 0; i < 360; i++) {
      step(3)
      if (spray.count > previousCount) {
        entries++
        const droplets = positions(spray)
        const fresh = droplets[droplets.length - 1]
        const tips = boat.bladeTips.map((tip) =>
          tip.getWorldPosition(new THREE.Vector3()).setY(0)
        )
        expect(
          Math.min(
            ...tips.map((tip) => Math.hypot(tip.x - fresh.x, tip.z - fresh.z))
          )
        ).toBeLessThan(0.2)
        expect(fresh.y).toBeGreaterThan(0)
        expect(fresh.y).toBeLessThan(0.1)
      }
      previousCount = spray.count
    }
    expect(entries).toBeGreaterThanOrEqual(4)
    expect(entries).toBeLessThanOrEqual(8)
    for (let i = 0; i < 60; i++) step(0)
    expect(boat.rowingWeight).toBeGreaterThan(0.99)
    for (let i = 0; i < 180; i++) {
      step(0)
      expect(spray.count).toBe(0)
    }
    effect.dispose()
  })

  it('samples river heights for each foam patch and clears dry or hidden boats', () => {
    const surfaceAt = (_x: number, z: number) => (z > 20 ? null : 5 + z * 0.04)
    const { vessel, effect, foam, step } = setup(surfaceAt)
    for (let i = 0; i < 90; i++) {
      vessel.position.y = surfaceAt(0, vessel.position.z + 3 / 60)!
      step(3, true, false)
    }
    expect(foam.count).toBeGreaterThan(20)
    for (const point of positions(foam))
      expect(point.y).toBeCloseTo(surfaceAt(point.x, point.z)! + 0.035, 5)
    vessel.position.y = -10000
    step(0)
    expect(foam.count).toBe(0)
    vessel.position.set(0, 5, 30)
    step(3)
    expect(foam.count).toBe(0)
    effect.dispose()
  })

  it('does not paint wakes for standing, reversing or teleporting boats', () => {
    const { vessel, effect, foam, spray, step } = setup()
    for (let i = 0; i < 60; i++) step(0, false, false)
    for (let i = 0; i < 60; i++) step(-3, true, false)
    expect(foam.count).toBe(0)
    for (let i = 0; i < 60; i++) step(3, true, false)
    expect(foam.count).toBeGreaterThan(0)
    vessel.position.z += 100
    step(3)
    expect(foam.count).toBe(0)
    expect(spray.count).toBe(0)
    effect.dispose()
  })

  it('keeps the particle budget bounded and releases its own resources', () => {
    const { effect, foam, spray, texture, step } = setup()
    for (let i = 0; i < 900; i++) step(6)
    expect(foam.count).toBeLessThanOrEqual(192)
    expect(spray.count).toBeLessThanOrEqual(64)
    const geometryDispose = vi.spyOn(foam.geometry, 'dispose')
    const textureDispose = vi.spyOn(texture, 'dispose')
    effect.dispose()
    expect(geometryDispose).toHaveBeenCalledOnce()
    expect(textureDispose).not.toHaveBeenCalled()
    expect(effect.group.children).toHaveLength(0)
    texture.dispose()
  })
})

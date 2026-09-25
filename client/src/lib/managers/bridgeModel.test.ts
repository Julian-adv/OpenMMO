import { readFileSync } from 'node:fs'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { createGLTFLoader } from '../utils/gltfCache'
import { bridgeManager } from './bridgeManager'
import type { ObjectDef, ObjectPlacement } from '../stores/editorStore'

const modelId = 'bridge_wood_long'
const catalog: ObjectDef[] = JSON.parse(
  readFileSync(
    new URL('../../../public/models/objects/catalog.json', import.meta.url),
    'utf8'
  )
)
const definition = catalog.find((entry) => entry.id === modelId)!
let scene: THREE.Group

beforeAll(async () => {
  const file = readFileSync(
    new URL(
      '../../../public/models/objects/bridge_wood_long.glb',
      import.meta.url
    )
  )
  const gltf = await createGLTFLoader()
    .register(() => ({
      name: 'headless-materials',
      loadMaterial: async () => new THREE.MeshBasicMaterial(),
    }))
    .parseAsync(
      file.buffer.slice(file.byteOffset, file.byteOffset + file.byteLength),
      ''
    )
  scene = gltf.scene
  bridgeManager.reset()
  bridgeManager.registerBridgeMesh(modelId, scene, definition.bridge!)
  bridgeManager.syncRegion(
    0,
    0,
    [
      {
        id: 1,
        type: modelId,
        x: 0,
        y: 0,
        z: 0,
        rotation: 0,
        floorLevel: 0,
      } as ObjectPlacement,
    ],
    new Map([[modelId, definition]])
  )
})

afterAll(() => bridgeManager.reset())

describe('long wooden bridge asset', () => {
  it('loads the full bridge at its authored size', () => {
    const bounds = new THREE.Box3().setFromObject(scene)
    const size = bounds.getSize(new THREE.Vector3())
    expect(size.x).toBeCloseTo(3.5233, 3)
    expect(size.y).toBeCloseTo(2.5511, 3)
    expect(size.z).toBeCloseTo(20.3254, 3)
    let triangles = 0
    scene.traverse((object) => {
      if (object instanceof THREE.Mesh)
        triangles += object.geometry.index!.count / 3
    })
    expect(triangles).toBe(53666)
  })

  it('keeps a continuous walkable deck across the span', () => {
    for (const x of [-1, 0, 1]) {
      for (let step = 0; step <= 202; step++) {
        const z = -10.1 + step / 10
        const y = bridgeManager.findDeckYAt(x, z, 0.1)
        expect(y, `deck at ${x}, ${z}`).not.toBeNull()
        expect(y!).toBeGreaterThanOrEqual(0)
        expect(y!).toBeLessThan(0.36)
        expect(bridgeManager.isMovementBlocked(x, z, x, z + 0.1, y!)).toBe(
          false
        )
      }
    }
  })

  it('allows the ends and blocks crossing the sides', () => {
    expect(bridgeManager.isMovementBlocked(0, -10.3, 0, -10, 0.1)).toBe(false)
    expect(bridgeManager.isMovementBlocked(0, 10, 0, 10.3, 0.1)).toBe(false)
    expect(bridgeManager.isMovementBlocked(1, 0, 1.9, 0, 0.1)).toBe(true)
    expect(bridgeManager.isMovementBlocked(-1, 0, -1.9, 0, 0.1)).toBe(true)
  })
})

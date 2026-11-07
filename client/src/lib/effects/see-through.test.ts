import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import { SeeThrough, seeThroughSource } from './see-through'

describe('SeeThrough', () => {
  it('reuses one ghost per source when another system swaps the material back', () => {
    const base = new THREE.MeshStandardMaterial()
    const mesh = new THREE.Mesh<THREE.BufferGeometry, THREE.Material>(
      new THREE.BufferGeometry(),
      base
    )
    const root = new THREE.Group().add(mesh)
    const seeThrough = new SeeThrough()

    seeThrough.update(root, true)
    const ghost = mesh.material
    expect(ghost).not.toBe(base)
    expect(seeThroughSource(ghost)).toBe(base)

    for (let i = 0; i < 5; i++) {
      mesh.material = seeThroughSource(mesh.material)!
      seeThrough.update(root, true)
      expect(mesh.material).toBe(ghost)
    }
    expect(ghost.opacity).toBeCloseTo(0.3)

    seeThrough.update(root, false)
    expect(mesh.material).toBe(base)
  })
})

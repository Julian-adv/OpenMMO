import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { femaleOutfitMask } from './female-outfit-mask'

const earFit = {
  mesh_index: 0,
  primitive_index: 0,
  primitive_triangles: 2,
  hide_triangle_indices: [],
  vertex_adjustments: [
    { vertex: 0, position: [0.2, 0.3, 0], normal: [0, 0, 1] },
  ],
}

describe('femaleOutfitMask vertex fitting', () => {
  it('splits the skin at the cut, blends skin weights, and restores on removal', () => {
    const geometry = new THREE.PlaneGeometry()
    geometry.setAttribute(
      'skinIndex',
      new THREE.Uint16BufferAttribute(
        [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
        4
      )
    )
    geometry.setAttribute(
      'skinWeight',
      new THREE.Float32BufferAttribute(
        [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
        4
      )
    )
    const mesh = new THREE.Mesh(geometry)
    const masks = femaleOutfitMask()
    masks.register(
      'boots',
      {
        mesh_index: 0,
        primitive_index: 0,
        primitive_triangles: 2,
        hide_triangle_indices: [],
        clip_below_y: { height: 0, triangle_indices: [0, 1] },
      },
      mesh
    )
    masks.setEquipped('boots', true)
    const clipped = mesh.geometry
    expect(clipped).not.toBe(geometry)
    const indices = clipped.index!
    const positions = clipped.attributes.position
    for (let i = 0; i < indices.count; i++)
      expect(positions.getY(indices.getX(i))).toBeGreaterThanOrEqual(-1e-6)
    expect(positions.count).toBeGreaterThan(geometry.attributes.position.count)
    for (let i = geometry.attributes.position.count; i < positions.count; i++) {
      const weights = clipped.attributes.skinWeight
      expect(
        weights.getX(i) + weights.getY(i) + weights.getZ(i) + weights.getW(i)
      ).toBeCloseTo(1)
    }
    expect(
      Array.from(
        { length: positions.count - geometry.attributes.position.count },
        (_, i) =>
          clipped.attributes.skinWeight.getY(
            geometry.attributes.position.count + i
          )
      ).some((value) => value > 0)
    ).toBe(true)
    masks.register(
      'cover',
      {
        mesh_index: 0,
        primitive_index: 0,
        primitive_triangles: 2,
        hide_triangle_indices: [0, 1],
      },
      mesh
    )
    masks.setEquipped('cover', true)
    expect(mesh.geometry.index!.count).toBe(0)
    masks.setEquipped('cover', false)
    expect(mesh.geometry.index!.count).toBeGreaterThan(0)
    masks.setEquipped('boots', false)
    expect(mesh.geometry).toBe(geometry)
    masks.dispose()
    geometry.dispose()
  })

  it('combines fitting with clothing masks and restores each independently', () => {
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry())
    const original = mesh.geometry
    const positions = original.attributes.position.array.slice()
    const indices = original.index!.array.slice()
    const masks = femaleOutfitMask()
    masks.register('helmet', earFit, mesh)
    masks.register(
      'top',
      { ...earFit, hide_triangle_indices: [1], vertex_adjustments: [] },
      mesh
    )
    masks.setEquipped('helmet', true)
    expect(mesh.geometry.index!.array).toEqual(indices)
    expect(mesh.geometry.attributes.position.getX(0)).toBeCloseTo(0.2)
    expect(original.attributes.position.array).toEqual(positions)
    masks.setEquipped('top', true)
    expect(mesh.geometry.index!.count).toBe(3)
    expect(mesh.geometry.attributes.position.getX(0)).toBeCloseTo(0.2)
    masks.setEquipped('helmet', false)
    expect(mesh.geometry.index!.count).toBe(3)
    expect(mesh.geometry.attributes.position.array).toEqual(positions)
    masks.setEquipped('top', false)
    expect(mesh.geometry).toBe(original)
    masks.dispose()
    original.dispose()
  })

  it('keeps shared geometry intact for other wearers and restores on disposal', () => {
    const original = new THREE.PlaneGeometry()
    const first = new THREE.Mesh(original)
    const second = new THREE.Mesh(original)
    const masks = femaleOutfitMask()
    masks.register('helmet', earFit, first)
    masks.setEquipped('helmet', true)
    expect(second.geometry).toBe(original)
    expect(first.geometry).not.toBe(second.geometry)
    masks.dispose()
    expect(first.geometry).toBe(original)
    original.dispose()
  })

  it.each([
    { vertex: 4, position: [0, 0, 0], normal: [0, 0, 1] },
    { vertex: 0, position: [Number.NaN, 0, 0], normal: [0, 0, 1] },
    { vertex: 0, position: [0, 0, 0], normal: [0, 1] },
  ])('rejects incompatible vertex adjustments %#', (adjustment) => {
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry())
    const masks = femaleOutfitMask()
    expect(() =>
      masks.register(
        'helmet',
        { ...earFit, vertex_adjustments: [adjustment] },
        mesh
      )
    ).toThrow()
    masks.dispose()
    mesh.geometry.dispose()
  })
})

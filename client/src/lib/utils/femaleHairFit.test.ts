import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'

vi.mock('../data/femaleHairFits.json', () => ({
  default: {
    default: {},
    custom: {
      bob: {
        'move-x': 1,
        'move-y': -2,
        'move-z': 0.5,
        scale: 100,
        'scale-x': 110,
        'scale-y': 90,
        'scale-z': 100,
        'rotate-x': 0,
        'rotate-y': 15,
        'rotate-z': 0,
      },
    },
  },
}))

import {
  HAIR_FIT_DEFAULT,
  applyFemaleHairFit,
  femaleHairFitMatrix,
  femaleHairPivot,
  fitFemaleHair,
} from './femaleHairFit'

function hair(
  geometry = new THREE.BoxGeometry(0.1, 0.15, 0.12).translate(0, 1.7, 0)
) {
  const mesh = new THREE.SkinnedMesh(geometry, new THREE.MeshBasicMaterial())
  const head = new THREE.Bone()
  head.name = 'Head'
  head.position.y = 1.65
  mesh.add(head)
  mesh.updateMatrixWorld(true)
  mesh.bind(new THREE.Skeleton([head]))
  return mesh
}

describe('female hair fits', () => {
  it('moves and scales around the rest head joint, independently of animation', () => {
    const mesh = hair()
    const pivot = femaleHairPivot(mesh)
    expect(pivot.y).toBeCloseTo(1.65)
    mesh.skeleton.bones[0].position.y += 0.2
    mesh.skeleton.bones[0].rotation.y = 1
    mesh.updateMatrixWorld(true)
    expect(femaleHairPivot(mesh).toArray()).toEqual(pivot.toArray())
    const matrix = femaleHairFitMatrix(
      { ...HAIR_FIT_DEFAULT, 'move-y': 1, scale: 110 },
      pivot
    )
    expect(pivot.clone().applyMatrix4(matrix).y).toBeCloseTo(1.66)
    expect(
      pivot
        .clone()
        .add(new THREE.Vector3(0.1, 0, 0))
        .applyMatrix4(matrix).x
    ).toBeCloseTo(0.11)
  })

  it('keeps source geometry intact, isolates faces and shares fitted geometry across players', () => {
    const first = hair()
    const source = first.geometry
    const positions = Array.from(source.attributes.position.array)
    const uv = Array.from(source.attributes.uv.array)
    const indices = Array.from(source.index!.array)
    fitFemaleHair(first, 'custom', 'hair')
    expect(first.geometry).not.toBe(source)
    expect(Array.from(source.attributes.position.array)).toEqual(positions)
    expect(Array.from(first.geometry.attributes.position.array)).not.toEqual(
      positions
    )
    expect(Array.from(first.geometry.attributes.uv.array)).toEqual(uv)
    expect(Array.from(first.geometry.index!.array)).toEqual(indices)
    const second = hair(source)
    fitFemaleHair(second, 'custom', 'hair')
    expect(second.geometry).toBe(first.geometry)
    const originalFace = hair(source)
    fitFemaleHair(originalFace, 'default', 'hair')
    expect(originalFace.geometry).toBe(source)
    const longHair = hair(source)
    fitFemaleHair(longHair, 'custom', 'valkyrie_hair')
    expect(longHair.geometry).toBe(source)
    const trimmed = hair(source)
    fitFemaleHair(trimmed, 'custom', 'barbarian_hair')
    expect(trimmed.geometry).toBe(first.geometry)
  })

  it('fits long hair at the head and leaves it on the body below the shoulders', () => {
    const source = new THREE.BufferGeometry().setAttribute(
      'position',
      new THREE.Float32BufferAttribute([0, 1.7, 0, 0, 1.53, 0, 0, 1.3, 0], 3)
    )
    const fit = { ...HAIR_FIT_DEFAULT, 'move-z': 2 }
    const pivot = new THREE.Vector3(0, 1.62, 0)
    const z = (style: 'long' | 'bob') =>
      Array.from(
        applyFemaleHairFit(source, fit, pivot, style).attributes.position.array
      ).filter((_, i) => i % 3 === 2)
    expect(z('bob')).toEqual([0.02, 0.02, 0.02].map(Math.fround))
    const [head, neck, chest] = z('long')
    expect(head).toBeCloseTo(0.02)
    expect(neck).toBeCloseTo(0.01)
    expect(chest).toBe(0)
  })
})

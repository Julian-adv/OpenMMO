import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import {
  partFitMatrix,
  sleeveEndDeformer,
  type PartFit,
} from './female-part-fit'

const fit: PartFit = {
  'move-x': 2,
  'move-y': -1.5,
  'move-z': 3,
  scale: 110,
  'scale-x': 95,
  'scale-y': 105,
  'scale-z': 120,
  'rotate-x': 10,
  'rotate-y': -20,
  'rotate-z': 30,
}

describe('partFitMatrix', () => {
  // Expected values come from tools/outfits/female_outfit_adjust.py, which bakes the same transform.
  it.each([1, -1] as const)('matches the Blender bake for side %i', (side) => {
    const point = new THREE.Vector3(0.45 * side, 1, 0.05).applyMatrix4(
      partFitMatrix(fit, new THREE.Vector3(0.4 * side, 1.1, 0.02), side)
    )
    expect(point.x).toBeCloseTo(0.503244 * side, 5)
    expect(point.y).toBeCloseTo(0.999643, 5)
    expect(point.z).toBeCloseTo(0.108507, 5)
  })
})

describe('sleeveEndDeformer', () => {
  // Expected values come from sleeve_end in tools/outfits/female_outfit_adjust.py.
  it('matches the Blender bake on both sleeves', () => {
    const points = [
      [0.2, 1.4, -0.05],
      [0.27, 1.32, -0.03],
      [0.31, 1.27, -0.06],
      [0.33, 1.25, -0.04],
      [0.1, 1.3, 0],
      [-0.27, 1.32, -0.03],
      [-0.31, 1.27, -0.06],
    ]
    const deform = sleeveEndDeformer([new Float32Array(points.flat())], {
      1: [
        new THREE.Vector3(0.16, 1.458, -0.047),
        new THREE.Vector3(0.32, 1.255, -0.047),
      ],
      [-1]: [
        new THREE.Vector3(-0.16, 1.458, -0.047),
        new THREE.Vector3(-0.32, 1.255, -0.047),
      ],
    })
    const ends = {
      1: { forward: 1.5, out: -2 },
      [-1]: { forward: -1, out: 0.5 },
    }
    const expected = [
      [0.2, 1.4, -0.05],
      [0.261697, 1.313456, -0.022071],
      [0.295549, 1.25861, -0.0462],
      [0.312852, 1.236484, -0.023625],
      [0.1, 1.3, 0],
      [-0.272506, 1.321975, -0.036382],
      [-0.314362, 1.273438, -0.071107],
    ]
    for (const [i, p] of points.entries()) {
      const point = new THREE.Vector3(...p)
      deform(point, (side) => ends[side])
      expect(point.x).toBeCloseTo(expected[i][0], 5)
      expect(point.y).toBeCloseTo(expected[i][1], 5)
      expect(point.z).toBeCloseTo(expected[i][2], 5)
    }
  })
})

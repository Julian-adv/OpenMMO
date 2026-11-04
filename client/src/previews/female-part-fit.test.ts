import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import { partFitMatrix, type PartFit } from './female-part-fit'

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
  // Expected values come from tools/outfits/female_priest_adjust.py, which bakes the same transform.
  it.each([1, -1] as const)('matches the Blender bake for side %i', (side) => {
    const point = new THREE.Vector3(0.45 * side, 1, 0.05).applyMatrix4(
      partFitMatrix(fit, new THREE.Vector3(0.4 * side, 1.1, 0.02), side)
    )
    expect(point.x).toBeCloseTo(0.503244 * side, 5)
    expect(point.y).toBeCloseTo(0.999643, 5)
    expect(point.z).toBeCloseTo(0.108507, 5)
  })
})

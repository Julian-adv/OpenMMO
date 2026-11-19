import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { neckPlaneMask } from './femaleNeckPlane'

const geometry = (points: number[][], index: number[]) => {
  const result = new THREE.BufferGeometry()
  result.setAttribute(
    'position',
    new THREE.Float32BufferAttribute(points.flat(), 3)
  )
  result.setAttribute(
    'skinIndex',
    new THREE.Uint16BufferAttribute(new Array(points.length * 4).fill(0), 4)
  )
  result.setIndex(index)
  return result
}

describe('neckPlaneMask', () => {
  const plane = {
    point: [0, 1.5, 0] as [number, number, number],
    normal: [0, 1, 0] as [number, number, number],
    radius: 0.1,
  }

  it('hides triangles under the plane, cuts those across it and keeps the rest', () => {
    const mask = neckPlaneMask(
      geometry(
        [
          [0, 1.45, 0],
          [0.02, 1.46, 0],
          [0, 1.47, 0.02],
          [0, 1.52, 0],
          [0.02, 1.55, 0],
          [0, 1.6, 0.02],
          // Under the plane but outside the collar's radius: a shoulder.
          [0.3, 1.45, 0],
          [0.32, 1.46, 0],
          [0.3, 1.47, 0.02],
        ],
        [0, 1, 2, 2, 3, 4, 3, 4, 5, 6, 7, 8]
      ),
      plane
    )
    expect(mask?.primitive_triangles).toBe(4)
    expect(mask?.hide_triangle_indices).toEqual([0])
    expect(mask?.clip_planes[0].triangle_indices).toEqual([1])
  })

  it('returns null for a mesh wholly above the plane', () => {
    expect(
      neckPlaneMask(
        geometry(
          [
            [0, 1.6, 0],
            [0.02, 1.62, 0],
            [0, 1.64, 0.02],
          ],
          [0, 1, 2]
        ),
        plane
      )
    ).toBeNull()
  })
})

import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import { applySkinBrush, skinBrush } from './female-skin-brush'

// Triangle 0 is a 10 cm sliver; triangle 1 is a small triangle 5 cm away from where the sliver is clicked.
const position = new THREE.BufferAttribute(
  new Float32Array([
    0, 0, 0, 0.1, 0, 0, 0.1, 0.002, 0, 0.05, 0.05, 0, 0.051, 0.05, 0, 0.05,
    0.051, 0,
  ]),
  3
)
const index = [0, 1, 2, 3, 4, 5]

describe('skinBrush', () => {
  it('picks the clicked sliver even when its centre and corners are outside the radius', () => {
    const pick = skinBrush(position, index)
    const picked = pick(
      { a: 0, b: 1, c: 2 },
      new THREE.Vector3(0.45, 0.45, 0.1),
      0.003
    )
    expect(picked).toEqual([0])
  })

  it('picks a triangle whose corner is inside the radius', () => {
    const pick = skinBrush(position, index)
    const picked = pick(
      { a: 3, b: 4, c: 5 },
      new THREE.Vector3(1, 0, 0),
      0.0005
    )
    expect(picked).toContain(1)
  })
})

describe('applySkinBrush', () => {
  it('adds hidden and removes shown triangles', () => {
    expect(
      applySkinBrush([1, 2], { hide: new Set([3]), show: new Set([1]) })
    ).toEqual([2, 3])
  })
})

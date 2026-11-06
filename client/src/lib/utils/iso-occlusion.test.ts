import { describe, expect, it } from 'vitest'
import { Box3, Vector3 } from 'three'
import {
  isoCameraOccludesNearPlayer,
  isoCameraOccludesPlayer,
} from './iso-occlusion'

const wall = (x0: number, x1: number, z0: number, z1: number) =>
  new Box3(new Vector3(x0, 0, z0), new Vector3(x1, 3, z1))

const near = (box: Box3, px = 0, pz = 0) =>
  isoCameraOccludesNearPlayer(box, px, 0, pz, 0.05, 1)

describe('isoCameraOccludesNearPlayer', () => {
  it('counts a front wall whose end clips the figure edge', () => {
    const box = wall(-5, -0.8, 0.5, 0.6)
    expect(isoCameraOccludesPlayer(box, 0, 0, 0, 0.05)).toBe(false)
    expect(near(box)).toBe(true)
  })

  it('counts a room corner hiding the figure from just past its end', () => {
    expect(near(wall(-5, 0.05, -0.05, 0.05), 1.7, -1.44)).toBe(true)
  })

  it('ignores a front wall ending beyond the margin', () => {
    expect(near(wall(-5, -2.5, 0.5, 0.6))).toBe(false)
  })

  it('ignores a far front wall', () => {
    expect(near(wall(-5, 5, 4, 4.1))).toBe(false)
  })

  it('ignores walls behind or beside the player', () => {
    expect(near(wall(-5, 5, -0.6, -0.5))).toBe(false)
    expect(near(wall(0.5, 0.6, -5, 5))).toBe(false)
  })
})

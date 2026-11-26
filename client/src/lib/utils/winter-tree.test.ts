import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import {
  buildWinterTreeGlow,
  WINTER_TREE_LIGHTS,
  WINTER_TREE_STAR,
} from './winter-tree'

describe('winter tree glow', () => {
  it('keeps every bulb on the boughs of the 6.2 m tree, under the star', () => {
    expect(WINTER_TREE_LIGHTS.length).toBeGreaterThan(40)
    const [starX, starY, starZ] = WINTER_TREE_STAR
    expect(Math.hypot(starX, starZ)).toBeLessThan(0.2)
    expect(starY).toBeGreaterThan(5.4)
    expect(starY).toBeLessThan(6.2)
    for (const [x, y, z] of WINTER_TREE_LIGHTS) {
      expect(y).toBeGreaterThan(0.6)
      expect(y).toBeLessThan(starY)
      expect(Math.hypot(x, z)).toBeLessThan(1.9)
    }
  })

  it('draws one halo per bulb, a star, glints and drifting dust', () => {
    const glow = buildWinterTreeGlow()
    const bulbs = glow.getObjectByName(
      'winter-tree-bulbs'
    ) as THREE.InstancedMesh
    expect(bulbs.count).toBe(WINTER_TREE_LIGHTS.length)
    expect(glow.getObjectByName('winter-tree-star')?.position.y).toBe(
      WINTER_TREE_STAR[1]
    )
    for (const name of ['winter-tree-glints', 'winter-tree-motes']) {
      const field = glow.getObjectByName(name) as THREE.InstancedMesh
      expect(field.count).toBeGreaterThan(0)
      // Placed in the shader, so the bounds must be set by hand.
      expect(field.clone().boundingSphere?.radius).toBeGreaterThan(3)
    }
  })
})

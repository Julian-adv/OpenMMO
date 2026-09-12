import { describe, expect, it } from 'vitest'
import * as THREE from 'three'
import { aboveWaterGroups } from './render-passes'

const layer = (group: THREE.Group) => ({ getGroup: () => group })

describe('aboveWaterGroups', () => {
  it('hides the rain layer from the refraction and reflection passes', () => {
    const rain = new THREE.Group()
    const wind = new THREE.Group()
    const groups = aboveWaterGroups({
      entityClipGroup: undefined,
      grassLayerRef: undefined,
      treeLayerRef: undefined,
      windParticlesRef: layer(wind) as never,
      rainLayerRef: layer(rain) as never,
      objectOverlayRef: undefined,
      riverRocksRef: undefined,
      shoreSprayRef: undefined,
    })
    expect(groups).toContain(rain)
    expect(groups).toContain(wind)
  })
})

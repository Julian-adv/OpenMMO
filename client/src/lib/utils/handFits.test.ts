import * as THREE from 'three'
import { describe, expect, it } from 'vitest'
import { poseMainHandProp, poseOffHandProp, type HandFits } from './handProps'
import {
  MODULAR_FEMALE_MODEL_PATH,
  MODULAR_MALE_MODEL_PATH,
} from './modelPaths'

const fits: HandFits = {
  all: {
    'hand:main': { position: [0, 0.08, 0], rotation: [0, 0, 0] },
    'hand:off': { position: [0, 0.08, 0], rotation: [0, Math.PI, 0] },
    'type:pickaxe': { rotation: [-1, 0, 0] },
    'model:weapons/miner_pickaxe.glb': { position: [0, 0.2, 0] },
  },
  modular_female: {
    'model:weapons/torch.glb': { position: [0.01, 0.09, 0.02] },
    'type:pickaxe': { position: [0.1, 0.1, 0.1], rotation: [0, 0, 1] },
  },
}
const seat = (pose: (prop: THREE.Object3D) => void) => {
  const prop = new THREE.Object3D()
  pose(prop)
  return [...prop.position.toArray(), ...prop.rotation.toArray().slice(0, 3)]
}

describe('hand fits', () => {
  it('take each field from the most specific fit, the rig over all rigs', () => {
    // model sets position, type still sets rotation.
    expect(
      seat((p) =>
        poseMainHandProp(
          p,
          'miner_pickaxe',
          undefined,
          MODULAR_MALE_MODEL_PATH,
          fits
        )
      )
    ).toEqual([0, 0.2, 0, -1, 0, 0])
    // A female variant shares the female fits, which override all rigs.
    expect(
      seat((p) =>
        poseMainHandProp(
          p,
          'miner_pickaxe',
          undefined,
          `${MODULAR_FEMALE_MODEL_PATH}#ranger`,
          fits
        )
      )
    ).toEqual([0.1, 0.1, 0.1, 0, 0, 1])
  })

  it('keep the fields a fit leaves out', () => {
    expect(
      seat((p) =>
        poseOffHandProp(p, 'weapons/torch.glb', MODULAR_FEMALE_MODEL_PATH, fits)
      )
    ).toEqual([0.01, 0.09, 0.02, 0, Math.PI, 0])
    expect(
      seat((p) => poseOffHandProp(p, 'weapons/torch.glb', undefined, fits))
    ).toEqual([0, 0.08, 0, 0, Math.PI, 0])
  })
})

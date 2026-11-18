import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'
import {
  loadCharacterAnimationPack,
  loadCharacterModel,
} from './characterModel'
import { createCharacterModelRoot } from './characterAnimationUtils'
import { poseMainHandProp } from './handProps'
import { loadGLB } from './gltfCache'
import {
  MODULAR_FEMALE_MODEL_PATH,
  MODULAR_MALE_MODEL_PATH,
} from './modelPaths'
import {
  SNOW_DIG_CLIP,
  SNOW_DIG_PACK,
  SNOW_SHOVEL_ITEM,
} from '../stores/roadSnowStore'

vi.mock('./gltfCache', async (importOriginal) => {
  const original = await importOriginal<typeof import('./gltfCache')>()
  return {
    ...original,
    loadGLB: vi.fn(async (path: string) => {
      const { loadHeadlessGlb } = await import('./headless-glb.fixture')
      return loadHeadlessGlb(path.slice(1))
    }),
  }
})

const rigs = [
  ['male', MODULAR_MALE_MODEL_PATH],
  ['female', MODULAR_FEMALE_MODEL_PATH],
] as const

describe('snow shovel in the dig clip', () => {
  for (const [name, modelPath] of rigs) {
    it(`runs the shaft through the left hand and brings the blade to the ground on the ${name} rig`, async () => {
      const source = await loadCharacterModel(modelPath)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      const { animations } = await loadCharacterAnimationPack(
        modelPath,
        `/models/${SNOW_DIG_PACK}`
      )
      const clip = animations.find((clip) => clip.name === SNOW_DIG_CLIP)!
      const shovel = (
        await loadGLB('/models/weapons/snow_shovel.glb')
      ).scene.clone()
      const box = new THREE.Box3().setFromObject(shovel)
      modelRoot.getObjectByName('RightHand')!.add(shovel)
      poseMainHandProp(shovel, SNOW_SHOVEL_ITEM, undefined, modelPath)
      const leftHand = modelRoot.getObjectByName('LeftHand')!
      const mixer = new THREE.AnimationMixer(modelRoot)
      mixer.clipAction(clip).play()
      const grip = new THREE.Vector3()
      const blade = new THREE.Vector3()
      const left = new THREE.Vector3()
      let lowest = Infinity
      let farthest = 0
      for (let i = 0; i < 24; i++) {
        mixer.setTime((clip.duration * i) / 24)
        modelRoot.updateMatrixWorld(true)
        shovel.localToWorld(grip.set(0, 0, 0))
        shovel.localToWorld(blade.set(box.max.x, 0, 0))
        leftHand.getWorldPosition(left)
        const shaft = new THREE.Line3(grip.clone(), blade.clone())
        const nearest = shaft.closestPointToPoint(
          left,
          true,
          new THREE.Vector3()
        )
        farthest = Math.max(farthest, nearest.distanceTo(left))
        lowest = Math.min(lowest, blade.y)
      }
      expect(farthest).toBeLessThan(0.3)
      expect(lowest).toBeLessThan(0.25)
      expect(lowest).toBeGreaterThan(-0.35)
    }, 30000)
  }
})

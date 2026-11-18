import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'
import {
  loadCharacterAnimationPack,
  loadCharacterModel,
} from './characterModel'
import { createCharacterModelRoot } from './characterAnimationUtils'
import { getWeaponAnimation } from '../data/weaponAnimationDefs'
import { poseMainHandProp } from './handProps'
import { loadGLB } from './gltfCache'
import {
  MODULAR_FEMALE_MODEL_PATH,
  MODULAR_MALE_MODEL_PATH,
} from './modelPaths'

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

describe('modular spear rest pose', () => {
  for (const [name, modelPath] of rigs) {
    it(`stands the spear upright with its butt on the ground on the ${name} rig`, async () => {
      const profile = getWeaponAnimation('spear')!
      const source = await loadCharacterModel(modelPath)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      const { animations } = await loadCharacterAnimationPack(
        modelPath,
        `/models/${profile.pack}`
      )
      const idle = animations.find((clip) => clip.name === profile.idle)!
      const spear = (await loadGLB('/models/weapons/spear.glb')).scene.clone()
      const box = new THREE.Box3().setFromObject(spear)
      const hand = modelRoot.getObjectByName('RightHand')!
      hand.add(spear)
      poseMainHandProp(spear, 'spear', undefined, modelPath)
      const mixer = new THREE.AnimationMixer(modelRoot)
      mixer.clipAction(idle).play()
      const butt = new THREE.Vector3()
      const tip = new THREE.Vector3()
      for (const time of [0, idle.duration / 3, (idle.duration * 2) / 3]) {
        mixer.setTime(time)
        modelRoot.updateMatrixWorld(true)
        spear.localToWorld(butt.set(box.min.x, 0, 0))
        spear.localToWorld(tip.set(box.max.x, 0, 0))
        expect(tip.clone().sub(butt).normalize().y).toBeGreaterThan(0.99)
        expect(Math.abs(butt.y)).toBeLessThan(0.005)
      }
    }, 30000)
  }
})

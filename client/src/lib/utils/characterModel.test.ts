import { existsSync } from 'node:fs'
import { resolve } from 'node:path'
import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'
import {
  loadCharacterModel,
  loadCharacterAnimationPack,
  MODULAR_SWORD_ATTACHMENT,
  modularSwordAttachment,
} from './characterModel'
import {
  CHARACTER_ANIMATION_PACK_PATHS,
  getAvailableGenders,
  getCharacterModelPath,
  getNpcModelPath,
  MODULAR_MALE_MODEL_PATH,
  PLAYER_CLASSES,
} from './modelPaths'
import {
  createCharacterModelRoot,
  computeSoleGroundOffset,
} from './characterAnimationUtils'
import { skinnedParts } from './modularCharacter'

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

describe('male player model selection', () => {
  it('uses the modular body for every available male player class and keeps NPC models', () => {
    for (const cls of PLAYER_CLASSES) {
      if (getAvailableGenders(cls).includes('male')) {
        expect(getCharacterModelPath(cls, 'male')).toBe(MODULAR_MALE_MODEL_PATH)
        expect(getCharacterModelPath(cls, 'male', true)).not.toBe(
          MODULAR_MALE_MODEL_PATH
        )
      }
      if (getAvailableGenders(cls).includes('female'))
        expect(getCharacterModelPath(cls, 'female')).not.toBe(
          MODULAR_MALE_MODEL_PATH
        )
    }
    expect(getCharacterModelPath('guard', 'male')).toBe(
      '/models/characters/guard.glb'
    )
    expect(getNpcModelPath('Tobin')).toBe('/models/characters/tobin.glb')
  })
})

describe.skipIf(
  !existsSync(resolve('public', MODULAR_MALE_MODEL_PATH.slice(1)))
)('modular player assets', () => {
  it('assembles clothes before exposure and isolates skeletons between players', async () => {
    const source = await loadCharacterModel(MODULAR_MALE_MODEL_PATH)
    expect(await loadCharacterModel(MODULAR_MALE_MODEL_PATH)).toBe(source)
    const { modelRoot: first } = createCharacterModelRoot(source.scene)
    const { modelRoot: second } = createCharacterModelRoot(source.scene)
    const a = skinnedParts(first)
    const b = skinnedParts(second)
    expect(new Set(a.map((mesh) => mesh.skeleton)).size).toBe(1)
    expect(a[0].skeleton).not.toBe(b[0].skeleton)
    expect(a[0].skeleton.bones[0]).not.toBe(b[0].skeleton.bones[0])
    expect(
      a.some(
        (mesh) =>
          (mesh.userData.region ?? mesh.parent?.userData.region) === 'torso' &&
          !mesh.visible
      )
    ).toBe(true)
    const hips = first.getObjectByName('Hips')!
    const rest = second.getObjectByName('Hips')!.position.clone()
    hips.position.y += 0.5
    expect(second.getObjectByName('Hips')!.position).toEqual(rest)
    expect(modularSwordAttachment(first, 'weapons/sword.glb')?.name).toBe(
      MODULAR_SWORD_ATTACHMENT
    )
    expect(modularSwordAttachment(first, 'weapons/bow.glb')).toBeUndefined()
  })

  it('keeps the corrected movement and sword grip through crossfades', async () => {
    const [source, pack] = await Promise.all([
      loadCharacterModel(MODULAR_MALE_MODEL_PATH),
      loadCharacterAnimationPack(
        MODULAR_MALE_MODEL_PATH,
        CHARACTER_ANIMATION_PACK_PATHS.locomotion
      ),
    ])
    const { modelRoot } = createCharacterModelRoot(source.scene)
    modelRoot.position.y = computeSoleGroundOffset(modelRoot)
    const mixer = new THREE.AnimationMixer(modelRoot)
    const idle = pack.animations.find((clip) => clip.name === 'idle1')!
    const walk = pack.animations.find((clip) => clip.name === 'walk')!
    const idleAction = mixer.clipAction(idle).play()
    mixer.update(0.1)
    const socket = modelRoot.getObjectByName(MODULAR_SWORD_ATTACHMENT)!
    expect(socket.position.length()).toBeGreaterThan(0)
    const before = socket.position.clone()
    mixer.clipAction(walk).reset().play().crossFadeFrom(idleAction, 0.3, false)
    mixer.update(0.15)
    expect(socket.position.distanceTo(before)).toBeLessThan(0.1)
    expect(socket.quaternion.length()).toBeCloseTo(1)
    const idleArmValues = idle.tracks.find(
      (track) => track.name === 'RightArm.quaternion'
    )!.values
    for (const clip of pack.animations.filter((clip) =>
      /^idle[2-5]$/.test(clip.name)
    )) {
      const armValues = clip.tracks.find(
        (track) => track.name === 'RightArm.quaternion'
      )!.values
      expect(armValues, clip.name).toEqual(idleArmValues)
    }
    mixer.stopAllAction()
    mixer.uncacheRoot(modelRoot)
  })

  it('loads all gameplay packs against the same rig with valid tracks', async () => {
    const source = await loadCharacterModel(MODULAR_MALE_MODEL_PATH)
    const { modelRoot } = createCharacterModelRoot(source.scene)
    const paths = [
      ...Object.values(CHARACTER_ANIMATION_PACK_PATHS),
      '/models/animations/great_sword.glb',
      '/models/animations/riding.glb',
    ]
    for (const path of paths) {
      const pack = await loadCharacterAnimationPack(
        MODULAR_MALE_MODEL_PATH,
        path
      )
      expect(pack.animations.length).toBeGreaterThan(0)
      for (const clip of pack.animations) {
        expect(clip.validate()).toBe(true)
        for (const track of clip.tracks)
          expect(
            modelRoot.getObjectByName(track.name.split('.')[0]),
            track.name
          ).toBeDefined()
      }
    }
  })
})

import { existsSync } from 'node:fs'
import { resolve } from 'node:path'
import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'
import {
  loadCharacterModel,
  applyCharacterArmor,
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
import type { ArmorEquipment } from '../network/networkTypes'

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
  const visible = (root: THREE.Object3D, id: string) =>
    skinnedParts(root).some(
      (mesh) => mesh.userData.part_id === id && mesh.visible
    )
  const bodyRegion = (meshes: THREE.SkinnedMesh[], region: string) =>
    meshes.filter(
      (mesh) =>
        (mesh.userData.part_id ?? mesh.parent?.userData.part_id) === 'base' &&
        (mesh.userData.region ?? mesh.parent?.userData.region) === region
    )

  it('equips the current rogue set, removes each slot and switches mixed equipment', async () => {
    const source = await loadCharacterModel(MODULAR_MALE_MODEL_PATH)
    const { modelRoot: rogue } = createCharacterModelRoot(source.scene)
    const { modelRoot: other } = createCharacterModelRoot(source.scene)
    const armor = {
      chest: 'worn_rogue_top',
      pants: 'worn_rogue_pants',
      hands: 'worn_rogue_gloves',
      boots: 'worn_rogue_boots',
    }
    const parts = {
      chest: 'top_rogue',
      pants: 'pants_rogue',
      hands: 'gloves_rogue',
      boots: 'boots_rogue',
    } as const
    applyCharacterArmor(rogue, armor)
    for (const part of Object.values(parts)) {
      expect(visible(rogue, part)).toBe(true)
      expect(visible(other, part)).toBe(false)
      expect(visible(source.scene, part)).toBe(false)
    }
    expect(visible(rogue, 'hair_crop')).toBe(true)
    expect(visible(rogue, 'top_linen')).toBe(false)
    expect(visible(rogue, 'pants_cloth')).toBe(false)
    const meshes = skinnedParts(rogue)
    for (const region of ['hands', 'forearms'])
      expect(bodyRegion(meshes, region).every((mesh) => mesh.visible)).toBe(
        true
      )
    const gloves = meshes.filter(
      (mesh) => mesh.userData.part_id === 'gloves_rogue'
    )
    expect(gloves.length).toBeGreaterThan(0)
    const handJoints = new Set<string>()
    for (const mesh of gloves) {
      const joints = mesh.geometry.getAttribute('skinIndex')
      const weights = mesh.geometry.getAttribute('skinWeight')
      for (let i = 0; i < joints.count; i++)
        for (let j = 0; j < 4; j++)
          if (weights.getComponent(i, j) > 0)
            handJoints.add(mesh.skeleton.bones[joints.getComponent(i, j)].name)
    }
    expect(handJoints.has('LeftHand')).toBe(true)
    expect(handJoints.has('RightHand')).toBe(true)
    for (const slot of Object.keys(parts) as (keyof typeof parts)[]) {
      applyCharacterArmor(rogue, { ...armor, [slot]: null })
      for (const [otherSlot, part] of Object.entries(parts))
        expect(visible(rogue, part)).toBe(otherSlot !== slot)
    }
    applyCharacterArmor(rogue, {
      ...armor,
      chest: 'worn_breastplate',
      head: 'worn_plate_helmet',
    })
    expect(visible(rogue, 'top_plate')).toBe(true)
    expect(visible(rogue, 'top_rogue')).toBe(false)
    expect(visible(rogue, 'hair_crop')).toBe(false)
    applyCharacterArmor(rogue, armor)
    expect(visible(rogue, 'top_rogue')).toBe(true)
    expect(visible(rogue, 'hair_crop')).toBe(true)
    applyCharacterArmor(rogue, {})
    for (const part of Object.values(parts))
      expect(visible(rogue, part)).toBe(false)
    for (const region of ['torso', 'legs', 'hands', 'feet'])
      expect(bodyRegion(meshes, region).every((mesh) => mesh.visible)).toBe(
        true
      )
  })

  it('equips and removes barbarian pieces independently without affecting other players', async () => {
    const source = await loadCharacterModel(MODULAR_MALE_MODEL_PATH)
    const { modelRoot: barbarian } = createCharacterModelRoot(source.scene)
    const { modelRoot: other } = createCharacterModelRoot(source.scene)
    const armor = {
      chest: 'worn_barbarian_armor',
      pants: 'worn_barbarian_pants',
      boots: 'worn_barbarian_boots',
      hands: 'worn_barbarian_bracers',
      head: 'worn_barbarian_helmet',
    }
    const parts = {
      chest: 'top',
      pants: 'pants',
      boots: 'boots',
      hands: 'gloves',
      head: 'helmet',
    } as const
    applyCharacterArmor(barbarian, armor)
    for (const part of Object.values(parts)) {
      expect(visible(barbarian, `${part}_barbarian`)).toBe(true)
      expect(visible(other, `${part}_barbarian`)).toBe(false)
      expect(visible(source.scene, `${part}_barbarian`)).toBe(false)
    }
    expect(visible(barbarian, 'hair_crop')).toBe(false)
    expect(visible(barbarian, 'top_linen')).toBe(false)
    expect(visible(barbarian, 'pants_cloth')).toBe(false)
    const meshes = skinnedParts(barbarian)
    for (const region of ['torso', 'upper_arms', 'legs', 'hands', 'feet']) {
      const body = bodyRegion(meshes, region)
      expect(body.length).toBeGreaterThan(0)
      expect(body.every((mesh) => mesh.visible)).toBe(true)
    }
    expect(
      meshes.some((mesh) => mesh.visible && mesh.userData.pelt_physics)
    ).toBe(true)
    for (const slot of Object.keys(parts) as (keyof typeof parts)[]) {
      applyCharacterArmor(barbarian, { ...armor, [slot]: null })
      for (const [otherSlot, part] of Object.entries(parts))
        expect(visible(barbarian, `${part}_barbarian`)).toBe(otherSlot !== slot)
      expect(visible(barbarian, 'hair_crop')).toBe(slot === 'head')
    }
    const mixed: ArmorEquipment = { ...armor, chest: 'worn_breastplate' }
    applyCharacterArmor(barbarian, mixed)
    expect(visible(barbarian, 'top_plate')).toBe(true)
    expect(visible(barbarian, 'top_barbarian')).toBe(false)
    expect(visible(barbarian, 'gloves_barbarian')).toBe(true)
    applyCharacterArmor(barbarian, {})
    for (const part of Object.values(parts))
      expect(visible(barbarian, `${part}_barbarian`)).toBe(false)
    expect(visible(other, 'top_linen')).toBe(true)
  })

  it('changes each armor slot from equipped items and keeps other players independent', async () => {
    const source = await loadCharacterModel(MODULAR_MALE_MODEL_PATH)
    const { modelRoot: knight } = createCharacterModelRoot(source.scene)
    const { modelRoot: other } = createCharacterModelRoot(source.scene)
    const armor = {
      chest: 'worn_breastplate',
      pants: 'worn_plate_greaves',
      boots: 'worn_plate_boots',
      hands: 'worn_plate_gauntlets',
      head: 'worn_plate_helmet',
    }
    applyCharacterArmor(knight, armor)
    for (const part of ['top', 'pants', 'boots', 'gloves', 'helmet']) {
      expect(visible(knight, part + '_plate')).toBe(true)
      expect(visible(other, part + '_plate')).toBe(false)
    }
    expect(visible(knight, 'hair_crop')).toBe(false)
    expect(visible(other, 'hair_crop')).toBe(true)
    const meshes = skinnedParts(knight)
    expect(new Set(meshes.map((mesh) => mesh.skeleton)).size).toBe(1)
    expect(meshes[0].skeleton.bones).toHaveLength(65)
    const legs = bodyRegion(meshes, 'legs')
    const bootAnkles = bodyRegion(meshes, 'boot_ankles')
    const bareAnkles = bodyRegion(meshes, 'ankles')
    expect(legs.length).toBeGreaterThan(0)
    expect(bootAnkles.length).toBeGreaterThan(0)
    expect(bareAnkles.length).toBeGreaterThan(0)
    expect(legs.every((mesh) => !mesh.visible)).toBe(true)
    expect(bootAnkles.every((mesh) => !mesh.visible)).toBe(true)

    applyCharacterArmor(knight, { ...armor, head: null })
    expect(visible(knight, 'helmet_plate')).toBe(false)
    expect(visible(knight, 'hair_crop')).toBe(true)
    expect(visible(knight, 'top_plate')).toBe(true)

    applyCharacterArmor(knight, { ...armor, pants: null })
    expect(visible(knight, 'pants_plate')).toBe(false)
    expect(visible(knight, 'pants_cloth')).toBe(false)
    expect(legs.every((mesh) => mesh.visible)).toBe(true)
    expect(bootAnkles.every((mesh) => mesh.visible)).toBe(true)
    expect(bareAnkles.every((mesh) => !mesh.visible)).toBe(true)
    expect(visible(knight, 'top_plate')).toBe(true)
    expect(visible(knight, 'boots_plate')).toBe(true)
    expect(visible(other, 'pants_cloth')).toBe(true)

    applyCharacterArmor(knight, { ...armor, chest: null })
    expect(visible(knight, 'top_plate')).toBe(false)
    expect(visible(knight, 'top_linen')).toBe(false)
    expect(visible(knight, 'pants_plate')).toBe(true)
    expect(legs.every((mesh) => !mesh.visible)).toBe(true)
    expect(bootAnkles.every((mesh) => !mesh.visible)).toBe(true)
    for (const region of ['torso', 'upper_arms', 'forearms', 'neck']) {
      const body = bodyRegion(meshes, region)
      expect(body.length).toBeGreaterThan(0)
      expect(body.every((mesh) => mesh.visible)).toBe(true)
    }
    expect(visible(other, 'top_linen')).toBe(true)

    applyCharacterArmor(knight, armor)
    expect(visible(knight, 'top_plate')).toBe(true)
    expect(visible(knight, 'top_linen')).toBe(false)

    applyCharacterArmor(knight, { ...armor, chest: 'leather_armor' })
    expect(visible(knight, 'top_plate')).toBe(false)
    expect(visible(knight, 'top_linen')).toBe(true)
    expect(visible(knight, 'pants_plate')).toBe(true)

    applyCharacterArmor(knight, { ...armor, pants: 'leather_pants' })
    expect(visible(knight, 'pants_plate')).toBe(false)
    expect(visible(knight, 'pants_cloth')).toBe(true)
    expect(legs.every((mesh) => !mesh.visible)).toBe(true)

    applyCharacterArmor(knight, {})
    for (const part of ['top', 'pants', 'boots', 'gloves', 'helmet'])
      expect(visible(knight, part + '_plate')).toBe(false)
    expect(visible(knight, 'top_linen')).toBe(false)
    expect(visible(knight, 'pants_cloth')).toBe(false)
    expect(legs.every((mesh) => mesh.visible)).toBe(true)
    expect(visible(knight, 'hair_crop')).toBe(true)
    expect(
      meshes.some((mesh) => mesh.userData.region === 'hands' && mesh.visible)
    ).toBe(true)

    applyCharacterArmor(other, { chest: 'breastplate', head: 'plate_helmet' })
    expect(visible(other, 'top_plate')).toBe(true)
    expect(visible(other, 'helmet_plate')).toBe(true)
    expect(visible(knight, 'top_plate')).toBe(false)
    expect(visible(source.scene, 'top_plate')).toBe(false)
  })

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

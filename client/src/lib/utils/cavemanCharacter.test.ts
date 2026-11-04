import { existsSync } from 'node:fs'
import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'
import {
  applyCharacterArmor,
  disposeCharacterSkeletons,
  loadCharacterAnimationPack,
  loadCharacterModel,
} from './characterModel'
import { createCharacterModelRoot } from './characterAnimationUtils'
import {
  CHARACTER_ANIMATION_PACK_PATHS,
  MODULAR_FEMALE_DIRECTORY,
  MODULAR_FEMALE_MODEL_PATH,
} from './modelPaths'
import { skinnedParts } from './modularCharacter'
import { loadGLB } from './gltfCache'

vi.mock('./gltfCache', async (importOriginal) => {
  const original = await importOriginal<typeof import('./gltfCache')>()
  const sources = new Map<string, ReturnType<typeof original.loadGLB>>()
  return {
    ...original,
    loadGLB: vi.fn(async (path: string) => {
      const { loadHeadlessGlb } = await import('./headless-glb.fixture')
      let source = sources.get(path)
      if (!source) {
        source = loadHeadlessGlb(path.slice(1))
        sources.set(path, source)
      }
      return source
    }),
  }
})

const armor = {
  chest: 'worn_caveman_top',
  pants: 'worn_caveman_pants',
  hands: 'worn_caveman_bracers',
  boots: 'worn_caveman_boots',
}
const names = ['top', 'bottom', 'gloves', 'boots']
const parts = (root: THREE.Object3D, name: string) =>
  skinnedParts(root).filter((mesh) => mesh.userData.female_part === name)
const shown = (root: THREE.Object3D, name: string) =>
  parts(root, name).some((mesh) => mesh.visible)
const body = (root: THREE.Object3D) =>
  skinnedParts(root).find((mesh) => mesh.userData.female_body)!
const hasAssets = existsSync(
  `public${MODULAR_FEMALE_DIRECTORY}/caveman_top.glb`
)

describe.skipIf(!hasAssets)('female caveman runtime outfit', () => {
  it('loads equipment on demand, shares meshes and restores independent body masks', async () => {
    const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
    const { modelRoot: first } = createCharacterModelRoot(source.scene)
    const { modelRoot: second } = createCharacterModelRoot(source.scene)
    const original = body(first).geometry
    for (const name of names)
      expect(parts(first, `caveman_${name}`)).toHaveLength(0)
    await applyCharacterArmor(first, armor)
    for (const name of names) expect(shown(first, `caveman_${name}`)).toBe(true)
    expect(shown(first, 'hair')).toBe(true)
    expect(shown(first, 'helmet')).toBe(false)
    expect(body(first).geometry.index!.count).toBeLessThan(
      original.index!.count
    )
    expect(body(second).geometry).toBe(original)
    await applyCharacterArmor(second, armor)
    for (const name of names)
      expect(parts(first, `caveman_${name}`)[0].geometry).toBe(
        parts(second, `caveman_${name}`)[0].geometry
      )
    for (const [slot, name] of [
      ['chest', 'top'],
      ['pants', 'bottom'],
      ['hands', 'gloves'],
      ['boots', 'boots'],
    ]) {
      await applyCharacterArmor(first, { ...armor, [slot]: null })
      for (const other of names)
        expect(shown(first, `caveman_${other}`)).toBe(other !== name)
      expect(shown(second, `caveman_${name}`)).toBe(true)
    }
    await applyCharacterArmor(first, {
      ...armor,
      chest: 'worn_breastplate',
      hands: 'worn_rogue_gloves',
    })
    expect(shown(first, 'top')).toBe(true)
    expect(shown(first, 'rogue_gloves')).toBe(true)
    expect(shown(first, 'caveman_top')).toBe(false)
    expect(shown(first, 'caveman_bottom')).toBe(true)
    await applyCharacterArmor(first, {})
    expect(body(first).geometry.index!.count).toBe(original.index!.count)
    for (const name of names)
      expect(shown(first, `caveman_${name}`)).toBe(false)
    disposeCharacterSkeletons(first)
    expect(body(first).geometry).toBe(original)
    disposeCharacterSkeletons(second)
  })

  it('keeps skin visible on a failed load, retries and ignores an outdated selection', async () => {
    const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
    const { modelRoot } = createCharacterModelRoot(source.scene)
    const triangles = body(modelRoot).geometry.index!.count
    const error = vi.spyOn(console, 'error').mockImplementation(() => {})
    vi.mocked(loadGLB).mockRejectedValueOnce(new Error('unavailable'))
    try {
      await applyCharacterArmor(modelRoot, { chest: armor.chest })
      expect(shown(modelRoot, 'caveman_top')).toBe(false)
      expect(body(modelRoot).geometry.index!.count).toBe(triangles)
      await applyCharacterArmor(modelRoot, { chest: armor.chest })
      expect(shown(modelRoot, 'caveman_top')).toBe(true)
      const old = applyCharacterArmor(modelRoot, armor)
      const latest = applyCharacterArmor(modelRoot, { chest: 'worn_rogue_top' })
      await Promise.all([old, latest])
      expect(shown(modelRoot, 'rogue_top')).toBe(true)
      for (const name of names)
        expect(shown(modelRoot, `caveman_${name}`)).toBe(false)
    } finally {
      error.mockRestore()
      disposeCharacterSkeletons(modelRoot)
    }
  })

  it('plays every game pack with finite fitted geometry and switches heels without advancing time', async () => {
    const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
    const { modelRoot } = createCharacterModelRoot(source.scene)
    await applyCharacterArmor(modelRoot, armor)
    const mixer = new THREE.AnimationMixer(modelRoot)
    const point = new THREE.Vector3()
    const boots = parts(modelRoot, 'caveman_boots')
    for (const path of [
      ...Object.values(CHARACTER_ANIMATION_PACK_PATHS),
      ...['great_sword', 'riding', 'dagger_preview'].map(
        (name) => `/models/animations/${name}.glb`
      ),
    ]) {
      const pack = await loadCharacterAnimationPack(
        MODULAR_FEMALE_MODEL_PATH,
        path
      )
      for (const clip of pack.animations) {
        mixer.stopAllAction()
        mixer.clipAction(clip).play()
        for (const fraction of [0, 0.25, 0.5, 0.75, 0.99]) {
          mixer.setTime(clip.duration * fraction)
          modelRoot.updateMatrixWorld(true)
          for (const mesh of skinnedParts(modelRoot).filter(
            (mesh) => mesh.visible
          )) {
            for (
              let i = 0;
              i < mesh.geometry.attributes.position.count;
              i += 43
            ) {
              mesh.getVertexPosition(i, point)
              expect(
                point.toArray().every(Number.isFinite),
                `${clip.name}:${mesh.name}`
              ).toBe(true)
            }
          }
          for (const side of ['Left', 'Right'])
            expect(
              modelRoot.getObjectByName(`${side}Foot`)!.quaternion.length()
            ).toBeCloseTo(1, 5)
        }
        if (clip.name === 'idle1') {
          let floor = Infinity
          for (const boot of boots)
            for (let i = 0; i < boot.geometry.attributes.position.count; i++)
              floor = Math.min(floor, boot.getVertexPosition(i, point).y)
          expect(Math.abs(floor)).toBeLessThan(0.02)
          const foot = modelRoot.getObjectByName('LeftFoot')!
          const heeled = foot.quaternion.clone()
          await applyCharacterArmor(modelRoot, { ...armor, boots: null })
          expect(foot.quaternion.angleTo(heeled)).toBeGreaterThan(0.1)
          await applyCharacterArmor(modelRoot, armor)
          expect(foot.quaternion.angleTo(heeled)).toBeLessThan(1e-6)
        }
      }
    }
    mixer.stopAllAction()
    disposeCharacterSkeletons(modelRoot)
  }, 30000)
})

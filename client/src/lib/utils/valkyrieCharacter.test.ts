import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
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
  isModularModel,
  MODULAR_FEMALE_MODEL_PATH,
  MODULAR_VALKYRIE_MODEL_PATH,
} from './modelPaths'
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

const armor = {
  head: 'worn_valkyrie_helmet',
  chest: 'worn_valkyrie_top',
  pants: 'worn_valkyrie_pants',
  hands: 'worn_valkyrie_gloves',
  boots: 'worn_valkyrie_boots',
}
const hasAssets = existsSync(`public${MODULAR_FEMALE_MODEL_PATH}`)
const parts = (root: THREE.Object3D, name: string) => {
  const meshes: THREE.Mesh[] = []
  root.traverse((node) => {
    if (node instanceof THREE.Mesh && node.userData.female_part === name)
      meshes.push(node)
  })
  return meshes
}
const shown = (root: THREE.Object3D, name: string) =>
  parts(root, name).some((mesh) => mesh.visible)

describe('Valkyrie base appearance', () => {
  it('pins every required Valkyrie model in the asset lock', () => {
    const lock = new Map(
      readFileSync('../assets.lock', 'utf8')
        .split('\n')
        .filter((line) => line.startsWith('file '))
        .map((line) => {
          const [, hash, path] = line.split(' ')
          return [path, hash]
        })
    )
    const paths = [
      'characters/modular_female/base.glb',
      ...['hair', 'wings', 'top', 'bottom', 'gloves', 'boots', 'helmet'].map(
        (part) => `characters/modular_female/valkyrie_${part}.glb`
      ),
      ...['helmet', 'top', 'pants', 'gloves', 'boots'].map(
        (part) => `armor/valkyrie_${part}.glb`
      ),
    ]
    for (const path of paths) {
      const key = `client/public/models/${path}`
      expect(lock.get(key), key).toMatch(/^[0-9a-f]{64}$/)
      const file = `public/models/${path}`
      if (existsSync(file))
        expect(
          createHash('sha256').update(readFileSync(file)).digest('hex')
        ).toBe(lock.get(key))
    }
  })

  it.skipIf(!hasAssets)(
    'keeps hair and wings through unequipping, recoloring and armor changes',
    async () => {
      const source = await loadCharacterModel(MODULAR_VALKYRIE_MODEL_PATH)
      expect(isModularModel(MODULAR_VALKYRIE_MODEL_PATH)).toBe(true)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      const { modelRoot: other } = createCharacterModelRoot(source.scene)
      const body = skinnedParts(modelRoot).find(
        (mesh) => mesh.userData.female_body
      )!
      const original = body.geometry
      await applyCharacterArmor(modelRoot, armor)
      for (const name of ['helmet', 'top', 'bottom', 'gloves', 'boots'])
        expect(shown(modelRoot, `valkyrie_${name}`)).toBe(true)
      expect(
        parts(modelRoot, 'valkyrie_gloves').reduce(
          (sum, mesh) => sum + mesh.geometry.index!.count / 3,
          0
        )
      ).toBe(1702)
      expect(
        parts(modelRoot, 'valkyrie_helmet')[0].geometry.index!.count / 3
      ).toBe(1343)
      expect(body.geometry.index!.count).toBeLessThan(original.index!.count)
      expect(
        skinnedParts(other).find((mesh) => mesh.userData.female_body)!.geometry
      ).toBe(original)
      for (const next of [
        {},
        { head: 'worn_plate_helmet', chest: 'worn_breastplate' },
        armor,
      ]) {
        await applyCharacterArmor(modelRoot, next, {
          face: 'default',
          hair: 'none',
          hair_color: '#000000',
          eye_color: '#000000',
        })
        expect(shown(modelRoot, 'valkyrie_hair')).toBe(true)
        expect(shown(modelRoot, 'valkyrie_wings')).toBe(true)
        expect(shown(modelRoot, 'hair')).toBe(false)
        expect(shown(modelRoot, 'barbarian_hair')).toBe(false)
      }
      await applyCharacterArmor(modelRoot, {})
      // Bare, the body still gives up the base head the face part replaces.
      expect(body.geometry.index!.count).toBe(
        original.index!.count -
          source.scene.userData.female_face_part.hide_triangle_indices.length *
            3
      )
      const normal = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      expect(parts(normal.scene, 'valkyrie_wings')).toHaveLength(0)
      expect(parts(normal.scene, 'valkyrie_hair')).toHaveLength(0)
      disposeCharacterSkeletons(modelRoot)
      expect(body.geometry).toBe(original)
      disposeCharacterSkeletons(other)
    }
  )

  it.skipIf(!hasAssets)(
    'discards an outdated lazy equipment selection',
    async () => {
      const source = await loadCharacterModel(MODULAR_VALKYRIE_MODEL_PATH)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      const old = applyCharacterArmor(modelRoot, armor)
      const latest = applyCharacterArmor(modelRoot, { chest: 'worn_rogue_top' })
      await Promise.all([old, latest])
      expect(shown(modelRoot, 'rogue_top')).toBe(true)
      expect(shown(modelRoot, 'valkyrie_top')).toBe(false)
      expect(shown(modelRoot, 'valkyrie_helmet')).toBe(false)
      expect(shown(modelRoot, 'valkyrie_hair')).toBe(true)
      expect(shown(modelRoot, 'valkyrie_wings')).toBe(true)
      disposeCharacterSkeletons(modelRoot)
    }
  )

  it.skipIf(!hasAssets)(
    'plays shared game packs with baked Valkyrie heels and fixed wing geometry',
    async () => {
      const source = await loadCharacterModel(MODULAR_VALKYRIE_MODEL_PATH)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      await applyCharacterArmor(modelRoot, armor)
      const mixer = new THREE.AnimationMixer(modelRoot)
      const wing = parts(modelRoot, 'valkyrie_wings')[0]
      const point = new THREE.Vector3(),
        other = new THREE.Vector3()
      let span: number | undefined
      for (const path of Object.values(CHARACTER_ANIMATION_PACK_PATHS)) {
        const pack = await loadCharacterAnimationPack(
          MODULAR_VALKYRIE_MODEL_PATH,
          path
        )
        expect(pack.animations.length).toBeGreaterThan(0)
        for (const clip of pack.animations) {
          mixer.stopAllAction()
          mixer.clipAction(clip).play()
          for (const time of [0, clip.duration / 2]) {
            mixer.setTime(time)
            modelRoot.updateMatrixWorld(true)
            wing.localToWorld(wing.getVertexPosition(0, point))
            wing.localToWorld(wing.getVertexPosition(20, other))
            expect(point.toArray().every(Number.isFinite)).toBe(true)
            span ??= point.distanceTo(other)
            expect(point.distanceTo(other)).toBeCloseTo(span, 5)
            expect(
              modelRoot.getObjectByName('LeftFoot')!.quaternion.length()
            ).toBeCloseTo(1, 5)
            expect(
              modelRoot
                .getObjectByName('Hips')!
                .position.toArray()
                .every(Number.isFinite)
            ).toBe(true)
          }
        }
      }
      mixer.stopAllAction()
      disposeCharacterSkeletons(modelRoot)
    },
    30000
  )
})

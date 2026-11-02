import { existsSync } from 'node:fs'
import { resolve } from 'node:path'
import * as THREE from 'three'
import { describe, expect, it, vi } from 'vitest'
import {
  loadCharacterModel,
  applyCharacterArmor,
  disposeCharacterSkeletons,
  loadCharacterAnimationPack,
} from './characterModel'
import { createCharacterModelRoot } from './characterAnimationUtils'
import {
  MODULAR_FEMALE_DIRECTORY,
  MODULAR_FEMALE_MODEL_PATH,
  MODULAR_VALKYRIE_MODEL_PATH,
  getCharacterModelPath,
  CHARACTER_ANIMATION_PACK_PATHS,
} from './modelPaths'
import { skinnedParts } from './modularCharacter'
import { FEMALE_BOOTS_PARTS, FEMALE_BOOTS_PROPERTY } from './femaleCharacter'
import { poseMainHandProp } from './handProps'
import { updatePeltPhysics } from '../effects/pelt-rig'
import { attachCapeFit, fitCapeToSkeleton } from '../effects/cape-rig'
import { robeBaseSkeleton } from '../effects/robe-rig'
import itemDefs from '../data/itemDefs'
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
const armor = {
  head: 'worn_plate_helmet',
  chest: 'worn_breastplate',
  pants: 'worn_plate_greaves',
  boots: 'worn_plate_boots',
  hands: 'worn_plate_gauntlets',
}
const barbarian = {
  head: 'worn_barbarian_helmet',
  chest: 'worn_barbarian_armor',
  pants: 'worn_barbarian_pants',
  boots: 'worn_barbarian_boots',
  hands: 'worn_barbarian_bracers',
}
const count = (root: THREE.Object3D, part?: string) =>
  skinnedParts(root)
    .filter(
      (m) =>
        m.visible && (part === undefined || m.userData.female_part === part)
    )
    .reduce((sum, m) => sum + m.geometry.index!.count / 3, 0)

const hasAssets = existsSync(
  resolve('public', MODULAR_FEMALE_MODEL_PATH.slice(1))
)

describe('female modular starter outfits', () => {
  it.skipIf(!hasAssets)(
    'loads and plays the female social animation pack',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      const pack = await loadCharacterAnimationPack(
        MODULAR_FEMALE_MODEL_PATH,
        CHARACTER_ANIMATION_PACK_PATHS.social
      )
      const clip = pack.animations.find(
        (animation) => animation.name === 'sit_idle'
      )
      expect(clip).toBeDefined()
      expect(clip!.duration).toBeGreaterThan(0)
      const hips = modelRoot.getObjectByName('Hips')!
      const mixer = new THREE.AnimationMixer(modelRoot)
      mixer.clipAction(clip!).play()
      mixer.setTime(clip!.duration / 2)
      expect(hips.position.toArray().every(Number.isFinite)).toBe(true)
      expect(hips.quaternion.toArray().every(Number.isFinite)).toBe(true)
      mixer.stopAllAction()
      disposeCharacterSkeletons(modelRoot)
    }
  )
  it.skipIf(!hasAssets)(
    'stores game dimensions in the rig and keeps attached props at unit scale',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      modelRoot.traverse((node) => {
        for (const value of node.scale.toArray())
          expect(value).toBeCloseTo(1, 6)
      })
      const body = skinnedParts(modelRoot).find((m) => m.userData.female_body)!
      body.geometry.computeBoundingBox()
      const height = body.geometry.boundingBox!.getSize(new THREE.Vector3()).y
      expect(height).toBeGreaterThan(1.7)
      expect(height).toBeLessThan(1.9)
      const prop = new THREE.Group()
      poseMainHandProp(
        prop,
        'worn_iron_sword',
        undefined,
        MODULAR_FEMALE_MODEL_PATH
      )
      modelRoot.getObjectByName('RightHand')!.add(prop)
      const scale = prop.getWorldScale(new THREE.Vector3())
      for (const value of scale.toArray()) expect(value).toBeCloseTo(1, 6)
      disposeCharacterSkeletons(modelRoot)
    }
  )
  it('uses the modular female player while keeping NPCs and other classes unchanged', () => {
    expect(getCharacterModelPath('knight', 'female')).toBe(
      MODULAR_FEMALE_MODEL_PATH
    )
    expect(getCharacterModelPath('barbarian', 'female')).toBe(
      MODULAR_FEMALE_MODEL_PATH
    )
    expect(getCharacterModelPath('knight', 'female', true)).toBe(
      '/models/characters/female_knight.glb'
    )
    expect(getCharacterModelPath('rogue', 'female')).toBe(
      MODULAR_FEMALE_MODEL_PATH
    )
    expect(getCharacterModelPath('rogue', 'female', true)).toBe(
      '/models/characters/female_rogue.glb'
    )
    expect(getCharacterModelPath('valkyrie', 'female')).toBe(
      MODULAR_VALKYRIE_MODEL_PATH
    )
    expect(getCharacterModelPath('valkyrie', 'female', true)).toBe(
      '/models/characters/valkyrie.glb'
    )
    expect(getCharacterModelPath('priest', 'female')).toBe(
      '/models/characters/female_priest.glb'
    )
  })
  it.skipIf(!hasAssets)(
    'keeps iron and steel sword handles inside the curled female hand',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      await applyCharacterArmor(modelRoot, { hands: 'worn_rogue_gloves' })
      const pack = await loadCharacterAnimationPack(
        MODULAR_FEMALE_MODEL_PATH,
        CHARACTER_ANIMATION_PACK_PATHS.locomotion
      )
      const { loadHeadlessGlb } = await import('./headless-glb.fixture')
      const hand = modelRoot.getObjectByName('RightHand')!
      const mixer = new THREE.AnimationMixer(modelRoot)
      for (const [item, file] of [
        ['worn_iron_sword', 'sword'],
        ['steel_longsword', 'steel_longsword'],
      ]) {
        const { scene: sword } = await loadHeadlessGlb(
          `models/weapons/${file}.glb`
        )
        sword.updateMatrixWorld(true)
        const handle = new THREE.Box3()
        const point = new THREE.Vector3()
        sword.traverse((node) => {
          if (!(node instanceof THREE.Mesh)) return
          const positions = node.geometry.attributes.position
          for (let i = 0; i < positions.count; i++) {
            point
              .fromBufferAttribute(positions, i)
              .applyMatrix4(node.matrixWorld)
            if (point.x >= -0.1 && point.x <= -0.06) handle.expandByPoint(point)
          }
        })
        expect(handle.isEmpty()).toBe(false)
        const center = handle.getCenter(new THREE.Vector3())
        poseMainHandProp(sword, item, undefined, MODULAR_FEMALE_MODEL_PATH)
        hand.add(sword)
        for (const name of ['idle1', 'walk', 'run']) {
          const clip = pack.animations.find((clip) => clip.name === name)!
          mixer.clipAction(clip).reset().play()
          mixer.setTime(clip.duration / 3)
          modelRoot.updateMatrixWorld(true)
          const grip = new THREE.Line3(
            sword.localToWorld(center.clone().setX(-0.11)),
            sword.localToWorld(center.clone().setX(0.01))
          )
          for (const finger of ['Index', 'Middle', 'Ring', 'Pinky']) {
            const tip = modelRoot
              .getObjectByName(`RightHand${finger}3`)!
              .getWorldPosition(new THREE.Vector3())
            const distance = grip
              .closestPointToPoint(tip, true, point)
              .distanceTo(tip)
            expect(distance, `${item}:${name}:${finger}`).toBeLessThan(0.035)
          }
          mixer.stopAllAction()
        }
        sword.removeFromParent()
      }
      disposeCharacterSkeletons(modelRoot)
    }
  )
  it.skipIf(!hasAssets)(
    'equips all five parts, restores hair and sleeves, and isolates character masks',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { modelRoot: first } = createCharacterModelRoot(source.scene)
      const { modelRoot: second } = createCharacterModelRoot(source.scene)
      const original = count(second)
      await applyCharacterArmor(first, armor)
      expect(count(first, 'helmet')).toBe(2166)
      expect(count(first, 'hair')).toBe(0)
      expect(count(first)).toBe(51538)
      expect(count(second)).toBe(original)
      const covered = count(first, 'top')
      await applyCharacterArmor(first, { ...armor, head: null, hands: null })
      expect(count(first, 'hair')).toBeGreaterThan(0)
      expect(count(first, 'helmet')).toBe(0)
      expect(count(first, 'gloves')).toBe(0)
      expect(count(first, 'top')).toBe(covered + 720)
      await applyCharacterArmor(first, {})
      expect(count(first)).toBe(original)
      disposeCharacterSkeletons(first)
      disposeCharacterSkeletons(second)
    }
  )
  it.skipIf(!hasAssets)(
    'uses rogue parts for rogue gear and preserves substitute armor styles',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      const parts = {
        head: 'helmet',
        chest: 'top',
        pants: 'bottom',
        boots: 'boots',
        hands: 'gloves',
      } as const
      for (const [id, item] of Object.entries(itemDefs)) {
        const slot = item.equipSlot as keyof typeof parts
        if (!(slot in parts)) continue
        const part = parts[slot]
        const barbarianStyle =
          item.material === 'leather' || id.startsWith('worn_barbarian_')
        const brilliance = id === 'helm_of_brilliance'
        const valkyrie = id.startsWith('worn_valkyrie_')
        const rogueStyle =
          item.worldModel?.startsWith('armor/rogue_') ||
          ['leather_armor', 'leather_gloves', 'leather_boots'].includes(id)
        const [shown, hidden] = valkyrie
          ? [`valkyrie_${part}`, part]
          : rogueStyle
            ? [`rogue_${part}`, part]
            : brilliance
              ? ['brilliance_helmet', part]
              : barbarianStyle
                ? [`barbarian_${part}`, part]
                : [part, `barbarian_${part}`]
        await applyCharacterArmor(modelRoot, { [slot]: id } as ArmorEquipment)
        expect(count(modelRoot, shown), id).toBeGreaterThan(0)
        expect(count(modelRoot, hidden), id).toBe(0)
        if (brilliance) expect(count(modelRoot, 'barbarian_helmet'), id).toBe(0)
        if (slot === 'head') {
          expect(count(modelRoot, 'barbarian_hair') > 0, id).toBe(
            barbarianStyle
          )
          expect(count(modelRoot, 'hair') > 0, id).toBe(valkyrie)
        }
      }
      await applyCharacterArmor(modelRoot, {})
      for (const part of Object.values(parts)) {
        expect(count(modelRoot, part)).toBe(0)
        expect(count(modelRoot, `barbarian_${part}`)).toBe(0)
        expect(count(modelRoot, `rogue_${part}`)).toBe(0)
      }
      expect(count(modelRoot, 'hair')).toBeGreaterThan(0)
      disposeCharacterSkeletons(modelRoot)
    }
  )
  it.skipIf(!hasAssets)(
    'equips the rogue creation outfit, gates garters and restores isolated body masks',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { modelRoot: first } = createCharacterModelRoot(source.scene)
      const { modelRoot: second } = createCharacterModelRoot(source.scene)
      const body = (root: THREE.Object3D) =>
        skinnedParts(root).find((mesh) => mesh.userData.female_body)!
      const originalGeometry = body(first).geometry
      const rogue = {
        chest: 'worn_rogue_top',
        pants: 'worn_rogue_pants',
        hands: 'worn_rogue_gloves',
        boots: 'worn_rogue_boots',
      }
      await applyCharacterArmor(first, rogue)
      for (const part of ['top', 'bottom', 'gloves', 'boots', 'garters'])
        expect(count(first, `rogue_${part}`)).toBeGreaterThan(0)
      expect(count(first, 'rogue_garters')).toBe(768)
      expect(count(first, 'hair')).toBeGreaterThan(0)
      expect(body(first).geometry.index!.count).toBeLessThan(
        originalGeometry.index!.count
      )
      expect(body(second).geometry).toBe(originalGeometry)
      expect(
        body(second).geometry.getAttribute('rogueUnderwear')
      ).toBeUndefined()
      await applyCharacterArmor(first, { ...rogue, boots: 'worn_plate_boots' })
      expect(count(first, 'rogue_garters')).toBe(0)
      expect(count(first, 'boots')).toBeGreaterThan(0)
      await applyCharacterArmor(first, { ...rogue, pants: null })
      expect(count(first, 'rogue_garters')).toBe(0)
      await applyCharacterArmor(first, rogue)
      expect(count(first, 'rogue_garters')).toBe(768)
      await applyCharacterArmor(first, {})
      expect(body(first).geometry.index!.count).toBe(
        originalGeometry.index!.count
      )
      for (const part of ['top', 'bottom', 'gloves', 'boots', 'garters'])
        expect(count(first, `rogue_${part}`)).toBe(0)
      disposeCharacterSkeletons(first)
      expect(body(first).geometry).toBe(originalGeometry)
      disposeCharacterSkeletons(second)
    }
  )
  it.skipIf(!hasAssets)(
    'loads the brilliance helmet while a cape hangs off the spine',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { clonedScene, modelRoot } = createCharacterModelRoot(source.scene)
      modelRoot.updateMatrixWorld(true)
      const cape = attachCapeFit(fitCapeToSkeleton(clonedScene)!)
      await applyCharacterArmor(modelRoot, { head: 'helm_of_brilliance' })
      expect(count(modelRoot, 'brilliance_helmet')).toBeGreaterThan(0)
      expect(count(modelRoot, 'hair')).toBe(0)
      cape.dispose()
      disposeCharacterSkeletons(modelRoot)
    }
  )
  it.skipIf(!hasAssets)(
    'equips the barbarian outfit with helmet hair and mixes it with plate',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      const original = count(modelRoot)
      await applyCharacterArmor(modelRoot, barbarian)
      for (const part of ['top', 'bottom', 'boots', 'gloves', 'helmet']) {
        expect(count(modelRoot, `barbarian_${part}`)).toBeGreaterThan(0)
        expect(count(modelRoot, part)).toBe(0)
      }
      expect(count(modelRoot, 'hair')).toBe(0)
      expect(count(modelRoot, 'barbarian_hair')).toBeGreaterThan(0)
      const body = skinnedParts(modelRoot).find((m) => m.userData.female_body)!
      const covered = body.geometry.index!.count
      await applyCharacterArmor(modelRoot, { ...barbarian, head: null })
      expect(count(modelRoot, 'hair')).toBeGreaterThan(0)
      expect(count(modelRoot, 'barbarian_hair')).toBe(0)
      await applyCharacterArmor(modelRoot, { ...armor, chest: barbarian.chest })
      expect(count(modelRoot, 'barbarian_top')).toBeGreaterThan(0)
      expect(count(modelRoot, 'top')).toBe(0)
      expect(count(modelRoot, 'helmet')).toBeGreaterThan(0)
      expect(count(modelRoot, 'barbarian_hair')).toBe(0)
      expect(body.geometry.index!.count).not.toBe(covered)
      await applyCharacterArmor(modelRoot, {})
      expect(count(modelRoot)).toBe(original)
      disposeCharacterSkeletons(modelRoot)
    }
  )
  it.skipIf(!hasAssets)(
    'keeps the barbarian fur physics in meters at its tuned scale',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const panels = skinnedParts(source.scene).filter(
        (m) => m.userData.pelt_physics
      )
      expect(panels).toHaveLength(4)
      for (const mesh of panels) {
        const { pelt_physics } = mesh.userData
        expect(pelt_physics.unit).toBeCloseTo(1.8, 6)
        expect(pelt_physics.pivot[1]).toBeGreaterThan(0.95)
        expect(pelt_physics.pivot[1]).toBeLessThan(1.15)
      }
    }
  )
  it.skipIf(!hasAssets)(
    'shares static skirt buffers and one hem skeleton per wearer across ten wearers',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const characters = []
      for (let i = 0; i < 10; i++) {
        const { modelRoot } = createCharacterModelRoot(source.scene)
        await applyCharacterArmor(modelRoot, barbarian)
        const panels = skinnedParts(modelRoot).filter(
          (mesh) => mesh.userData.pelt_physics
        )
        const originals = panels.map((mesh) => mesh.geometry)
        const base = panels[0].skeleton
        modelRoot.position.x = i * 0.7
        updatePeltPhysics(modelRoot, 0)
        expect(panels).toHaveLength(4)
        for (const panel of panels) {
          expect(panel.userData.robe_physics).toBeDefined()
          expect(panel.skeleton).toBe(panels[0].skeleton)
          expect(robeBaseSkeleton(panel)).toBe(base)
        }
        expect(panels[0].skeleton.bones).toHaveLength(base.bones.length + 6)
        characters.push({ modelRoot, panels, originals })
      }
      const first = characters[0]
      for (const { panels } of characters.slice(1))
        for (let i = 0; i < panels.length; i++) {
          expect(panels[i].geometry).toBe(first.panels[i].geometry)
          expect(panels[i].skeleton).not.toBe(first.panels[i].skeleton)
        }
      const attributes = first.panels.flatMap((panel) =>
        Object.values(panel.geometry.attributes).map((attribute) => {
          const buffer =
            attribute instanceof THREE.InterleavedBufferAttribute
              ? attribute.data
              : attribute
          return { attribute: buffer, version: buffer.version }
        })
      )
      const reference = new THREE.SkinnedMesh()
      const actual = new THREE.Vector3(),
        expected = new THREE.Vector3()
      let movement = 0
      const hems = first.panels.map((panel, i) => {
        const position = first.originals[i].getAttribute('position')
        let lower = 0
        for (let k = 1; k < position.count; k++)
          if (position.getY(k) < position.getY(lower)) lower = k
        return {
          panel,
          lower,
          initial: panel.getVertexPosition(lower, new THREE.Vector3()),
        }
      })
      for (let frame = 0; frame < 60; frame++) {
        first.modelRoot.getObjectByName('LeftUpLeg')!.rotation.x =
          -Math.sin(frame / 15) * 0.8
        first.modelRoot.getObjectByName('RightUpLeg')!.rotation.x =
          Math.sin(frame / 15) * 0.8
        for (const { modelRoot } of characters)
          updatePeltPhysics(modelRoot, 1 / 60)
        for (const { panel, lower, initial } of hems)
          movement = Math.max(
            movement,
            panel.getVertexPosition(lower, actual).distanceTo(initial)
          )
        for (const [i, panel] of first.panels.entries()) {
          reference.geometry = first.originals[i]
          reference.bind(robeBaseSkeleton(panel), panel.bindMatrix)
          for (const vertex of panel.userData.pelt_physics.attachment_vertices)
            expect(
              panel
                .getVertexPosition(vertex, actual)
                .distanceTo(reference.getVertexPosition(vertex, expected))
            ).toBeLessThan(1e-6)
        }
      }
      expect(movement).toBeGreaterThan(0.01)
      for (const { attribute, version } of attributes)
        expect(attribute.version).toBe(version)
      for (const { modelRoot, panels, originals } of characters) {
        disposeCharacterSkeletons(modelRoot)
        for (const [i, panel] of panels.entries()) {
          expect(panel.geometry).toBe(originals[i])
          expect(panel.skeleton.bones).toHaveLength(65)
        }
      }
    }
  )

  it.skipIf(!hasAssets)(
    'plays baked runtime motions with a rigid helmet and finite boot correction',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      await applyCharacterArmor(modelRoot, armor)
      const mixer = new THREE.AnimationMixer(modelRoot)
      const helmet = skinnedParts(modelRoot).find(
        (m) => m.userData.female_part === 'helmet'
      )!
      const hips = modelRoot.getObjectByName('Hips')!
      const a = new THREE.Vector3(),
        b = new THREE.Vector3()
      let length: number | undefined
      for (const path of Object.values(CHARACTER_ANIMATION_PACK_PATHS)) {
        const pack = await loadCharacterAnimationPack(
          MODULAR_FEMALE_MODEL_PATH,
          path
        )
        expect(pack.animations.length).toBeGreaterThan(0)
        for (const clip of pack.animations) {
          mixer.stopAllAction()
          mixer.clipAction(clip).play()
          const lift = clip.tracks.find(
            (track) => track.name === `Hips.${FEMALE_BOOTS_PROPERTY}`
          )!
          for (const time of [0, clip.duration / 2, clip.duration * 0.99]) {
            mixer.setTime(time)
            modelRoot.updateMatrixWorld(true)
            const handScale = modelRoot
              .getObjectByName('RightHand')!
              .getWorldScale(new THREE.Vector3())
            for (const value of handScale.toArray())
              expect(value).toBeCloseTo(1, 5)
            helmet.getVertexPosition(0, a)
            helmet.getVertexPosition(20, b)
            expect(a.toArray().every(Number.isFinite)).toBe(true)
            length ??= a.distanceTo(b)
            expect(a.distanceTo(b)).toBeCloseTo(length, 5)
            expect(hips.quaternion.length()).toBeCloseTo(1, 5)
          }
          const width = (FEMALE_BOOTS_PARTS.length + 1) * 3
          for (let i = 0; i < lift.values.length; i += width)
            for (let v = 1; v <= FEMALE_BOOTS_PARTS.length; v++) {
              const boots = lift.values[i + v * 3 + 1] - lift.values[i + 1]
              expect(boots).toBeGreaterThanOrEqual(-1e-6)
              expect(boots).toBeLessThan(0.2)
            }
        }
      }
      mixer.stopAllAction()
      disposeCharacterSkeletons(modelRoot)
    },
    30000
  )
  it.skipIf(!hasAssets)(
    'bakes heel pose, ground lift and sword grip into clips, switched by boots',
    async () => {
      const source = await loadCharacterModel(MODULAR_FEMALE_MODEL_PATH)
      const { modelRoot } = createCharacterModelRoot(source.scene)
      await applyCharacterArmor(modelRoot, armor)
      const { loadHeadlessGlb } = await import('./headless-glb.fixture')
      const offsets = await Promise.all(
        FEMALE_BOOTS_PARTS.map(async (name) => {
          const gltf = await loadHeadlessGlb(
            `${MODULAR_FEMALE_DIRECTORY.slice(1)}/${name}.glb`
          )
          return new THREE.Quaternion().fromArray(
            gltf.scene.userData.heel_pose.local_quaternion_offsets.LeftFoot
          )
        })
      )
      const pack = await loadCharacterAnimationPack(
        MODULAR_FEMALE_MODEL_PATH,
        CHARACTER_ANIMATION_PACK_PATHS.locomotion
      )
      const raw = (
        await loadHeadlessGlb(
          `${MODULAR_FEMALE_DIRECTORY.slice(1)}/animations/locomotion.glb`
        )
      ).animations.find((clip) => clip.name === 'walk')!
      const walk = pack.animations.find((clip) => clip.name === 'walk')!
      const track = (clip: THREE.AnimationClip, name: string) =>
        clip.tracks.find((track) => track.name === name)!
      const foot = track(walk, `LeftFoot.${FEMALE_BOOTS_PROPERTY}`).values
      const bare = track(raw, 'LeftFoot.quaternion').values
      const q = new THREE.Quaternion()
      for (const i of [0, 10, 40]) {
        const expected = q.fromArray(bare, i * 4)
        const angle = (variant: number) =>
          new THREE.Quaternion()
            .fromArray(
              foot,
              (i * (FEMALE_BOOTS_PARTS.length + 1) + variant) * 4
            )
            .angleTo(
              variant
                ? expected.clone().multiply(offsets[variant - 1])
                : expected
            )
        for (let v = 0; v <= FEMALE_BOOTS_PARTS.length; v++)
          expect(angle(v)).toBeLessThan(1e-3)
      }
      for (const [bone, quaternion] of Object.entries(
        source.scene.userData.female_grip as Record<string, number[]>
      ))
        expect(Array.from(track(walk, `${bone}.quaternion`).values)).toEqual([
          ...quaternion,
          ...quaternion,
        ])

      const mixer = new THREE.AnimationMixer(modelRoot)
      mixer.clipAction(walk).play()
      const scene = modelRoot.children[0]
      const body = skinnedParts(modelRoot).find((m) => m.userData.female_body)!
      const soles = (part: string) =>
        skinnedParts(modelRoot).filter((m) => m.userData.female_part === part)
      const lowest = (meshes: THREE.SkinnedMesh[]) => {
        let y = Infinity
        const point = new THREE.Vector3()
        for (const mesh of meshes)
          for (let i = 0; i < mesh.geometry.attributes.position.count; i++)
            y = Math.min(y, mesh.getVertexPosition(i, point).y)
        return y
      }
      const rawFoot = track(raw, 'LeftFoot.quaternion')
      expect(rawFoot.getInterpolation()).toBe(THREE.InterpolateLinear)
      for (const time of [0, walk.duration / 3, (walk.duration * 2) / 3]) {
        await applyCharacterArmor(modelRoot, { ...armor, boots: null })
        mixer.setTime(time)
        modelRoot.updateMatrixWorld(true)
        const floor = lowest([body])
        const expected = new THREE.Quaternion().fromArray(
          rawFoot.InterpolantFactoryMethodLinear().evaluate(time)
        )
        expect(
          modelRoot.getObjectByName('LeftFoot')!.quaternion.angleTo(expected)
        ).toBeLessThan(1e-3)
        for (const [boots, part] of [
          [armor.boots, 'boots'],
          [barbarian.boots, 'barbarian_boots'],
          ['worn_valkyrie_boots', 'valkyrie_boots'],
        ]) {
          await applyCharacterArmor(modelRoot, { ...armor, boots })
          mixer.setTime(time)
          modelRoot.updateMatrixWorld(true)
          expect(Math.abs(lowest(soles(part)) - floor)).toBeLessThan(0.01)
        }
        expect(scene.position.y).toBe(0)
      }
      mixer.stopAllAction()
      disposeCharacterSkeletons(modelRoot)
    },
    30000
  )
})

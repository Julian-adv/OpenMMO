import * as THREE from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import type { ArmorEquipment } from '../network/networkTypes'
import { getItemDef } from '../data/itemDefs'
import { loadGLB } from './gltfCache'
import {
  MODULAR_FEMALE_DIRECTORY,
  MODULAR_FEMALE_MODEL_PATH,
  MODULAR_VALKYRIE_MODEL_PATH,
  MODULAR_FEMALE_RANGER_MODEL_PATH,
} from './modelPaths'
import { bindModularPart, skinnedParts } from './modularCharacter'
import { femaleUnderwearColor } from './femaleUnderwearColor'
import { femaleOutfitMask } from './female-outfit-mask'
import { modularOutfitForArmor } from './modularEquipment'
import { attachValkyrieWings } from '../effects/valkyrie-wings'

const slots = ['top', 'bottom', 'boots', 'gloves', 'helmet'] as const
type Slot = (typeof slots)[number]
type Style =
  | 'plate'
  | 'barbarian'
  | 'rogue'
  | 'valkyrie'
  | 'caveman'
  | 'priest'
  | 'ranger'
const prefixes: Record<Style, string> = {
  plate: '',
  barbarian: 'barbarian_',
  rogue: 'rogue_',
  valkyrie: 'valkyrie_',
  caveman: 'caveman_',
  priest: 'priest_',
  ranger: 'ranger_',
}
const styles = Object.keys(prefixes) as Style[]
type Helmet = Exclude<Style, 'rogue' | 'caveman' | 'ranger'> | 'brilliance'
const LAZY_HELMET = 'brilliance_helmet'
// Model-path variants of the shared body: parts always loaded, and the hair worn without a helmet
// (null: the variant's fixed hair is always shown).
type Variant = { fixed: string[]; hair: Style | null }
const VARIANTS: Record<string, Variant> = {
  [MODULAR_VALKYRIE_MODEL_PATH]: {
    fixed: ['valkyrie_hair', 'valkyrie_wings'],
    hair: null,
  },
  [MODULAR_FEMALE_RANGER_MODEL_PATH]: {
    fixed: ['ranger_hair'],
    hair: 'ranger',
  },
}
const variantOf = (names: string[]) =>
  Object.values(VARIANTS).find((variant) =>
    variant.fixed.every((name) => names.includes(name))
  )
// Styles loaded on first equip rather than with the character, and the slots each one has.
const LAZY_STYLES: Partial<Record<Style, readonly Slot[]>> = {
  valkyrie: slots,
  caveman: slots.filter((slot) => slot !== 'helmet'),
  priest: slots,
  ranger: slots.filter((slot) => slot !== 'helmet'),
}
const partName = (style: Style, slot: Slot | 'hair') =>
  `${prefixes[style]}${slot}`
const lazyParts = new Set([
  LAZY_HELMET,
  ...Object.entries(LAZY_STYLES).flatMap(([style, styleSlots]) =>
    styleSlots.map((slot) => partName(style as Style, slot))
  ),
])
export const FEMALE_PARTS = styles
  .filter((style) => !(style in LAZY_STYLES))
  .flatMap((style) =>
    (['hair', ...slots] as const)
      .filter(
        (slot) => style !== 'rogue' || (slot !== 'hair' && slot !== 'helmet')
      )
      .map((slot) => partName(style, slot))
  )
  .concat('rogue_garters')
/** Boots parts in the order their baked variants follow the bare value. */
export const FEMALE_BOOTS_PARTS = styles.map((style) =>
  partName(style, 'boots')
)
// Baked tracks carry [bare, ...boots] values; this property picks one per character.
export const FEMALE_BOOTS_PROPERTY = 'femaleBoots'
// glTF allows one channel per node and path, so boots values ship as companion animations.
export const femaleBootsClipName = (clip: string, boots: string) =>
  `${clip}|${FEMALE_BOOTS_PROPERTY}:${boots}`
const models = new Map<string, Promise<GLTF>>()
const packs = new Map<string, Promise<GLTF>>()
const pairedBaseClips = new WeakMap<GLTF, THREE.AnimationClip[]>()

function baseClips(base: GLTF): THREE.AnimationClip[] {
  let clips = pairedBaseClips.get(base)
  if (!clips) {
    clips = pairFemaleClips(base.scene, base.animations)
    pairedBaseClips.set(base, clips)
  }
  return clips
}

export function assembleFemaleCharacter(
  base: GLTF,
  sources: GLTF[],
  names = FEMALE_PARTS
): THREE.Group {
  const scene = clone(base.scene) as THREE.Group
  const variant = variantOf(names)
  scene.userData.female_hair = variant ? variant.hair : 'plate'
  base.scene.traverse((node) => {
    const association = base.parser.associations.get(node)
    if (
      node instanceof THREE.SkinnedMesh &&
      association?.meshes === 0 &&
      association.primitives === 0
    )
      scene.getObjectByName(node.name)!.userData.female_body = true
  })
  scene.userData.modular_character = true
  scene.userData.female_parts = {}
  for (const [i, name] of names.entries()) {
    scene.userData.female_parts[name] = sources[i].scene.userData
    const meshes = bindModularPart(scene, sources[i].scene)
    for (const mesh of meshes) {
      mesh.userData.female_part = name
      mesh.visible = variant ? variant.fixed.includes(name) : name === 'hair'
    }
    if (name === 'valkyrie_wings') attachValkyrieWings(scene, meshes)
  }
  for (const mesh of skinnedParts(scene)) mesh.frustumCulled = false
  scene.userData.female_grip = Object.fromEntries(
    base.animations
      .find((clip) => clip.name === 'idle_armed')!
      .tracks.filter((track) =>
        /^RightHand(Thumb|Index|Middle|Ring|Pinky)[123]\.quaternion$/.test(
          track.name
        )
      )
      .map((track) => [
        track.name.split('.')[0],
        Array.from(track.values.slice(0, 4)),
      ])
  )
  return scene
}

export function loadFemaleCharacter(
  path = MODULAR_FEMALE_MODEL_PATH
): Promise<GLTF> {
  let pending = models.get(path)
  if (!pending) {
    const names = [...FEMALE_PARTS, ...(VARIANTS[path]?.fixed ?? [])]
    pending = Promise.all([
      loadGLB(MODULAR_FEMALE_MODEL_PATH),
      ...names.map((name) =>
        loadGLB(`${MODULAR_FEMALE_DIRECTORY}/${name}.glb`)
      ),
    ])
      .then(([base, ...sources]) => {
        const scene = assembleFemaleCharacter(base, sources, names)
        return { ...base, scene, scenes: [scene], animations: baseClips(base) }
      })
      .catch((error) => {
        models.delete(path)
        throw error
      })
    models.set(path, pending)
  }
  return pending
}

export function loadFemaleAnimationPack(path: string): Promise<GLTF> {
  let pending = packs.get(path)
  if (!pending) {
    pending = Promise.all([loadGLB(MODULAR_FEMALE_MODEL_PATH), loadGLB(path)])
      .then(([base, pack]) => ({
        ...pack,
        animations: pairFemaleClips(base.scene, pack.animations),
      }))
      .catch((error) => {
        packs.delete(path)
        throw error
      })
    packs.set(path, pending)
  }
  return pending
}

function canonical(values: Float32Array, offset: number, rest: number[]) {
  let dot = 0
  for (let i = 0; i < 4; i++) dot += values[offset + i] * rest[i]
  if (dot < 0) for (let i = 0; i < 4; i++) values[offset + i] *= -1
}

/** Zips each clip's companion boots animations into `<node>.femaleBoots` [bare, ...boots] tracks. */
function pairFemaleClips(
  scene: THREE.Object3D,
  clips: THREE.AnimationClip[]
): THREE.AnimationClip[] {
  const byName = new Map(clips.map((clip) => [clip.name, clip]))
  const companions = new Set(
    clips.flatMap((clip) =>
      FEMALE_BOOTS_PARTS.map((boots) => femaleBootsClipName(clip.name, boots))
    )
  )
  const width = FEMALE_BOOTS_PARTS.length + 1
  return clips
    .filter((clip) => !companions.has(clip.name))
    .map((clip) => {
      const variants = FEMALE_BOOTS_PARTS.map((boots) => {
        const variant = byName.get(femaleBootsClipName(clip.name, boots))
        if (!variant) throw new Error(`부츠 보정 동작이 없습니다: ${clip.name}`)
        return variant
      })
      const tracks = clip.tracks.slice()
      for (const { name } of variants[0].tracks) {
        const index = tracks.findIndex((track) => track.name === name)
        const bare = tracks[index]
        const sources = variants.map((variant) =>
          variant.tracks.find((track) => track.name === name)
        )
        if (
          !bare ||
          sources.some((track) => track?.times.length !== bare.times.length)
        )
          throw new Error(`부츠 보정 트랙이 맞지 않습니다: ${name}`)
        const [node] = name.split('.')
        const size = bare.getValueSize()
        const rest =
          size === 4 ? scene.getObjectByName(node)!.quaternion.toArray() : []
        const values = new Float32Array(bare.values.length * width)
        for (let i = 0; i < bare.times.length; i++) {
          for (const [v, track] of [bare, ...sources].entries()) {
            const offset = (i * width + v) * size
            for (let j = 0; j < size; j++)
              values[offset + j] = track!.values[i * size + j]
            if (size === 4) canonical(values, offset, rest)
          }
        }
        tracks[index] = new THREE.KeyframeTrack(
          `${node}.${FEMALE_BOOTS_PROPERTY}`,
          bare.times,
          values
        )
      }
      return new THREE.AnimationClip(
        clip.name,
        clip.duration,
        tracks,
        clip.blendMode
      )
    })
}

type Outfit = Record<Exclude<Slot, 'helmet'>, Style | null> & {
  helmet: Helmet | null
}

interface FemaleState {
  mask: ReturnType<typeof femaleOutfitMask>
  refreshBoots: () => void
  underwearColor: ReturnType<typeof femaleUnderwearColor>
  equipped?: Outfit
}

const states = new WeakMap<THREE.Object3D, FemaleState>()
const partLoads = new WeakMap<THREE.Object3D, Map<string, Promise<void>>>()
const latestArmor = new WeakMap<THREE.Object3D, ArmorEquipment | undefined>()

function femaleScene(root: THREE.Object3D) {
  return root.userData.female_parts
    ? root
    : root.children.find((child) => child.userData.female_parts)
}

// PropertyMixer skips unchanged values, so a boots toggle re-applies the last values.
function installBootsSwitch(scene: THREE.Object3D, state: FemaleState) {
  const offsets = scene.userData.female_parts.boots.heel_pose
    .local_quaternion_offsets as Record<string, number[]>
  const targets: [THREE.Object3D, THREE.Vector3 | THREE.Quaternion, number][] =
    Object.keys(offsets).map((name) => {
      const bone = scene.getObjectByName(name)!
      return [bone, bone.quaternion, 4]
    })
  const hips = scene.getObjectByName('Hips')!
  targets.push([hips, hips.position, 3])
  const width = FEMALE_BOOTS_PARTS.length + 1
  const refreshes = targets.map(([node, target, size]) => {
    const values = new Float32Array(size * width)
    let ready = false
    const select = () => {
      if (!ready) return
      const boots = state.equipped?.boots
      const variant = boots ? styles.indexOf(boots) + 1 : 0
      target.fromArray(values, variant * size)
      if (target instanceof THREE.Quaternion) target.normalize()
    }
    Object.assign(node, {
      [FEMALE_BOOTS_PROPERTY]: {
        fromArray(array: ArrayLike<number>, offset: number) {
          for (let i = 0; i < values.length; i++) values[i] = array[offset + i]
          ready = true
          select()
        },
        toArray(array: number[], offset: number) {
          for (let v = 0; v < width; v++)
            target.toArray(array, offset + v * size)
          return array
        },
      },
    })
    return select
  })
  return () => refreshes.forEach((refresh) => refresh())
}

const ROGUE_LEATHER_MODELS = new Set([
  'armor/leather_armor.glb',
  'armor/leather_gloves.glb',
  'armor/leather_boots.glb',
])

const femaleStyle = (itemId: string | null | undefined): Style | null => {
  if (!itemId) return null
  const item = getItemDef(itemId)
  const model = item?.worldModel ?? ''
  if (model.startsWith('armor/valkyrie_')) return 'valkyrie'
  if (model.startsWith('armor/caveman_')) return 'caveman'
  if (model.startsWith('armor/priest_')) return 'priest'
  if (model.startsWith('armor/ranger_')) return 'ranger'
  if (model.startsWith('armor/rogue_') || ROGUE_LEATHER_MODELS.has(model))
    return 'rogue'
  return item?.material === 'leather' || model.startsWith('armor/barbarian_')
    ? 'barbarian'
    : 'plate'
}

const femaleHelmet = (itemId: string | null | undefined): Helmet | null => {
  if (modularOutfitForArmor({ head: itemId }).helmet === 'brilliance')
    return 'brilliance'
  const style = femaleStyle(itemId)
  return style === 'rogue' || style === 'caveman' || style === 'ranger'
    ? 'barbarian'
    : style
}

function bindLazyFemalePart(
  scene: THREE.Object3D,
  name: string
): Promise<void> {
  let loads = partLoads.get(scene)
  if (!loads) partLoads.set(scene, (loads = new Map()))
  let load = loads.get(name)
  if (!load) {
    const current = loads
    load = loadGLB(`${MODULAR_FEMALE_DIRECTORY}/${name}.glb`).then(
      (gltf) => {
        if (partLoads.get(scene) !== current) return
        scene.userData.female_parts[name] = gltf.scene.userData
        for (const mesh of bindModularPart(scene, gltf.scene)) {
          mesh.userData.female_part = name
          mesh.visible = false
          mesh.frustumCulled = false
        }
        const state = states.get(scene)
        if (state) registerLazyPart(scene, state, name)
      },
      (error) => {
        current.delete(name)
        throw error
      }
    )
    loads.set(name, load)
  }
  return load
}

function registerLazyPart(
  scene: THREE.Object3D,
  state: FemaleState,
  name: string
) {
  const body = skinnedParts(scene).find((mesh) => mesh.userData.female_body)!
  state.mask.register(name, scene.userData.female_parts[name].body_mask, body)
}

function femaleOutfit(armor?: ArmorEquipment): Outfit {
  return {
    top: femaleStyle(armor?.chest),
    bottom: femaleStyle(armor?.pants),
    boots: femaleStyle(armor?.boots),
    gloves: femaleStyle(armor?.hands),
    helmet: femaleHelmet(armor?.head),
  }
}

export async function applyFemaleArmorAsync(
  root: THREE.Object3D,
  armor?: ArmorEquipment
): Promise<boolean> {
  const scene = femaleScene(root)
  if (!scene) return false
  latestArmor.set(scene, armor)
  const outfit = femaleOutfit(armor)
  const missing = slots.flatMap((slot) => {
    const style = outfit[slot]
    const name =
      style === 'brilliance' ? LAZY_HELMET : style && partName(style, slot)
    return name && lazyParts.has(name) && !scene.userData.female_parts[name]
      ? [name]
      : []
  })
  if (missing.length) {
    await Promise.all(
      missing.map((name) =>
        bindLazyFemalePart(scene, name).catch((error) => {
          console.error(`Failed to load female part: ${name}`, error)
        })
      )
    )
    if (!latestArmor.has(scene)) return true
  }
  return applyFemaleArmor(root, latestArmor.get(scene))
}

export function applyFemaleArmor(
  root: THREE.Object3D,
  armor?: ArmorEquipment
): boolean {
  const scene = femaleScene(root)
  if (!scene) return false
  const equipped = femaleOutfit(armor)
  for (const slot of slots) {
    const style = equipped[slot]
    const name =
      style === 'brilliance' ? LAZY_HELMET : style && partName(style, slot)
    if (name && !scene.userData.female_parts[name]) equipped[slot] = null
  }
  let state = states.get(scene)
  if (
    state?.equipped &&
    slots.every((slot) => state!.equipped![slot] === equipped[slot])
  )
    return true
  const meshes = skinnedParts(scene)
  const part = (name: string) =>
    meshes.filter((mesh) => mesh.userData.female_part === name)
  if (!state) {
    const { female_parts } = scene.userData
    const body = meshes.find((mesh) => mesh.userData.female_body)!
    const underwearColor = femaleUnderwearColor(body)
    const mask = femaleOutfitMask()
    for (const style of styles)
      for (const slot of slots) {
        const name = partName(style, slot)
        if (female_parts[name])
          mask.register(name, female_parts[name].body_mask, body)
      }
    mask.register('sleeves', female_parts.gloves.sleeve_mask, part('top')[0])
    state = { mask, underwearColor, refreshBoots: () => {} }
    if (female_parts[LAZY_HELMET]) registerLazyPart(scene, state, LAZY_HELMET)
    state.refreshBoots = installBootsSwitch(scene, state)
    states.set(scene, state)
  }
  const { helmet } = equipped
  const bare: Style | null = scene.userData.female_hair
  const hair =
    bare === null || helmet === 'plate' || helmet === 'brilliance'
      ? null
      : partName(
          helmet === 'valkyrie' || helmet === 'priest'
            ? 'plate'
            : (helmet ?? bare),
          'hair'
        )
  for (const style of styles) {
    if (style === 'valkyrie') continue
    const name = partName(style, 'hair')
    for (const mesh of part(name)) mesh.visible = name === hair
  }
  const shown: Record<string, boolean> = {
    sleeves: equipped.top === 'plate' && equipped.gloves === 'plate',
  }
  for (const style of styles)
    for (const slot of slots) {
      const name = partName(style, slot)
      if (!scene.userData.female_parts[name]) continue
      shown[name] = equipped[slot] === style
      for (const mesh of part(name)) mesh.visible = shown[name]
    }
  if (scene.userData.female_parts[LAZY_HELMET]) {
    shown[LAZY_HELMET] = helmet === 'brilliance'
    for (const mesh of part(LAZY_HELMET)) mesh.visible = shown[LAZY_HELMET]
  }
  for (const mesh of part('rogue_garters'))
    mesh.visible = equipped.bottom === 'rogue' && equipped.boots === 'rogue'
  state.mask.setEquippedAll(shown)
  state.underwearColor(
    equipped.bottom === 'rogue',
    equipped.bottom === 'caveman'
  )
  const bootsChanged = state.equipped?.boots !== equipped.boots
  state.equipped = equipped
  if (bootsChanged) state.refreshBoots()
  return true
}

export function disposeFemaleCharacter(root: THREE.Object3D) {
  const scene = femaleScene(root)
  if (!scene) return
  const state = states.get(scene)
  state?.mask.dispose()
  state?.underwearColor.dispose()
  states.delete(scene)
  partLoads.delete(scene)
  latestArmor.delete(scene)
}

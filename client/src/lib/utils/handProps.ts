import * as THREE from 'three'
import { getItemDef, isRangedWeapon } from '../data/itemDefs'
import handFits from '../data/handFits.json'
import {
  MODULAR_MALE_MODEL_PATH,
  MODULAR_FEMALE_MODEL_PATH,
} from './modelPaths'

type Fit = { position?: number[]; rotation?: number[] }
/** Seats by rig ("all", then the rig's own), each keyed `hand:main|off`,
 *  `item:<id>`, `model:<worldModel>`, `type:<weaponType>` or `category:<c>`. */
export type HandFits = Record<string, Record<string, Fit>>

/** Rigs with their own layer in handFits.json, by name. */
export const HAND_FIT_RIGS: Record<string, string> = {
  modular_male: MODULAR_MALE_MODEL_PATH,
  modular_female: MODULAR_FEMALE_MODEL_PATH,
}

/** The handFits rig for a character model; female variants (#fragment) share
 *  the female grips. */
export const handFitRig = (modelPath?: string) =>
  Object.keys(HAND_FIT_RIGS).find(
    (rig) => HAND_FIT_RIGS[rig] === modelPath?.split('#')[0]
  )

/** Keys a held item answers to, most specific first. */
export function handFitKeys(itemDefId?: string, worldModel?: string) {
  const def = itemDefId ? getItemDef(itemDefId) : undefined
  const model = worldModel ?? def?.worldModel
  return [
    itemDefId && `item:${itemDefId}`,
    model && `model:${model}`,
    def?.weaponType && `type:${def.weaponType}`,
    def?.category && `category:${def.category}`,
  ].filter((key): key is string => !!key)
}

/** Every fit in `layer` the keys match, least specific first, so each field
 *  comes from the most specific fit that sets it. True if any matched. */
function applyFit(
  prop: THREE.Object3D,
  layer: Record<string, Fit> | undefined,
  keys: string[]
) {
  let matched = false
  for (const key of [...keys].reverse()) {
    const fit = layer?.[key]
    if (!fit) continue
    matched = true
    if (fit.position) prop.position.fromArray(fit.position)
    if (fit.rotation)
      prop.rotation.fromArray(fit.rotation as [number, number, number])
  }
  return matched
}

// Fitted in tools/rig-importer against bow_shoot.
const BOW_POSITION = new THREE.Vector3(0.01, 0.06, 0.04)
const BOW_ROTATION = new THREE.Euler((-13 * Math.PI) / 180, 0, 0)

// Scale grip reach by forearm length; bracers distort hand mesh bounds.
const KNIGHT_FOREARM_METERS = 0.267
const BOW_FOREARM_RATIO = BOW_POSITION.y / KNIGHT_FOREARM_METERS

export const MANDOLIN_ITEM_DEF_ID = 'mandolin'

// Fitted with tools/fit-hand-prop.mjs --tilt 15 --push 0.06 --lift 0.04.
const MANDOLIN_ROTATION = new THREE.Euler(-2.413, -0.409, -0.353)
const MANDOLIN_POSITION = new THREE.Vector3(-0.03, 0.103, 0.126)
/** Pose the mandolin for the `/play_music` strumming clip. */
export function poseMusicProp(prop: THREE.Object3D) {
  prop.position.copy(MANDOLIN_POSITION)
  prop.rotation.copy(MANDOLIN_ROTATION)
}

// Bows sit in the left hand; other weapons lead with the right.
export function mainHandBoneFor(
  itemDefId: string | null | undefined
): 'RightHand' | 'LeftHand' {
  return isRangedWeapon(itemDefId) ? 'LeftHand' : 'RightHand'
}

// Scale the bow's offset to this rig's forearm when available.
export function poseMainHandProp(
  prop: THREE.Object3D,
  itemDefId: string,
  forearm?: number,
  modelPath?: string,
  fits: HandFits = handFits
) {
  const keys = handFitKeys(itemDefId)
  applyFit(prop, fits.all, ['hand:main'])
  if (!applyFit(prop, fits.all, keys) && isRangedWeapon(itemDefId)) {
    prop.position.copy(BOW_POSITION)
    if (forearm && forearm > 0) prop.position.y = forearm * BOW_FOREARM_RATIO
    prop.rotation.copy(BOW_ROTATION)
  }
  const rig = handFitRig(modelPath)
  if (rig) applyFit(prop, fits[rig], keys)
}

const forearmCache = new Map<string, number>()

// Cache the bind-pose forearm length in metres; missing bones return zero.
export function forearmLength(
  root: THREE.Object3D,
  handBone: 'RightHand' | 'LeftHand',
  key: string
): number {
  const cached = forearmCache.get(key)
  if (cached !== undefined) return cached

  const foreArmBone = handBone === 'LeftHand' ? 'LeftForeArm' : 'RightForeArm'
  let length = 0
  root.traverse((child) => {
    const mesh = child as THREE.SkinnedMesh
    if (length > 0 || !mesh.isSkinnedMesh) return
    const at = (name: string) => {
      const index = mesh.skeleton.bones.findIndex((b) => b.name === name)
      if (index < 0) return null
      // The bind matrix is the inverse of what the skeleton stores.
      return new THREE.Vector3().setFromMatrixPosition(
        mesh.skeleton.boneInverses[index].clone().invert()
      )
    }
    const hand = at(handBone)
    const foreArm = at(foreArmBone)
    if (hand && foreArm) length = hand.distanceTo(foreArm)
  })

  forearmCache.set(key, length)
  return length
}
/** Pose a left-hand prop — shields and torches face the other way. */
export function poseOffHandProp(
  prop: THREE.Object3D,
  worldModel?: string,
  modelPath?: string,
  fits: HandFits = handFits
) {
  const keys = handFitKeys(undefined, worldModel)
  applyFit(prop, fits.all, ['hand:off'])
  applyFit(prop, fits.all, keys)
  const rig = handFitRig(modelPath)
  if (rig) applyFit(prop, fits[rig], keys)
}

/** Where the flame sits on a torch GLB that names no `torch_tip` empty. */
export const FALLBACK_TORCH_TIP_LOCAL_OFFSET = new THREE.Vector3(0.6, 0, 0)

// Use the named tip, or attach a fallback at its local offset.
export function resolveTipNode(
  prop: THREE.Object3D,
  name: string,
  fallbackOffset: THREE.Vector3
): THREE.Object3D {
  const found = prop.getObjectByName(name)
  if (found) return found
  const node = new THREE.Object3D()
  node.position.copy(fallbackOffset)
  prop.add(node)
  return node
}

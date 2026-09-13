import * as THREE from 'three'
import { loadGLB } from './gltfCache'
import { CHARACTER_ANIMATION_PACK_PATHS } from './modelPaths'
import {
  groundRetargetedClips,
  retargetAnimationsForCharacterModel,
} from './characterAnimationUtils'
import { getWeaponEffectAxis } from './weaponEffectAxis'

export const ENCHANT_WEAPON_ANIMATION = 'enchant_weapon'
export const ENCHANT_LEFT_WEAPON_ANIMATION = 'enchant_weapon_left'
export const ENCHANT_ARMOR_ANIMATION = 'enchant_armor'
export const ENCHANT_LEFT_ARMOR_ANIMATION = 'enchant_armor_left'
const clipsByModel = new Map<
  string,
  Promise<Map<string, THREE.AnimationClip>>
>()
const armorHolds = new WeakMap<THREE.AnimationClip, THREE.AnimationClip>()

export function getArmorEnchantHold(idle: THREE.AnimationClip) {
  let hold = armorHolds.get(idle)
  if (!hold) {
    const tracks = idle.tracks.map((track) => {
      const held = track.clone()
      held.times = new Float32Array([0])
      held.values = track.values.slice(0, track.getValueSize())
      return held
    })
    hold = new THREE.AnimationClip(`enchant_armor:${idle.name}`, 1, tracks)
    armorHolds.set(idle, hold)
  }
  return hold
}

export function isArmorEnchantHold(
  clip: THREE.AnimationClip | undefined,
  idle: THREE.AnimationClip
) {
  return clip !== undefined && armorHolds.get(idle) === clip
}

export function loadEnchantAnimations(modelPath: string, root: THREE.Object3D) {
  let pending = clipsByModel.get(modelPath)
  if (!pending) {
    pending = Promise.all(
      [
        CHARACTER_ANIMATION_PACK_PATHS.social,
        CHARACTER_ANIMATION_PACK_PATHS.enchantArmor,
      ].map(async (path) => {
        const gltf = await loadGLB(path)
        const clips = gltf.animations.filter((clip) =>
          [
            ENCHANT_WEAPON_ANIMATION,
            ENCHANT_LEFT_WEAPON_ANIMATION,
            ENCHANT_ARMOR_ANIMATION,
            ENCHANT_LEFT_ARMOR_ANIMATION,
          ].includes(clip.name)
        )
        return retargetAnimationsForCharacterModel(root, gltf.scene, clips)
      })
    )
      .then((packs) => groundRetargetedClips(root, packs.flat()))
      .then((clips) => new Map(clips.map((clip) => [clip.name, clip])))
      .catch((error) => {
        clipsByModel.delete(modelPath)
        throw error
      })
    clipsByModel.set(modelPath, pending)
  }
  return pending
}

export class EnchantWeaponGrip {
  private readonly rotation: THREE.Quaternion
  private readonly inverseHand = new THREE.Quaternion()
  private readonly correction = new THREE.Quaternion()
  private readonly direction = new THREE.Vector3()
  private readonly up = new THREE.Vector3()

  constructor(private readonly weapon: THREE.Object3D) {
    this.rotation = weapon.quaternion.clone()
  }

  update(weight: number, lowered = false) {
    this.weapon.quaternion.copy(this.rotation)
    if (weight <= 0 || !this.weapon.parent) return
    this.weapon.parent.getWorldQuaternion(this.inverseHand).invert()
    this.up.set(0, lowered ? -1 : 1, 0).applyQuaternion(this.inverseHand)
    this.direction
      .copy(getWeaponEffectAxis(this.weapon).direction)
      .applyQuaternion(this.rotation)
    this.correction.setFromUnitVectors(this.direction, this.up)
    this.correction.multiply(this.rotation)
    this.weapon.quaternion.slerp(this.correction, weight)
  }
}

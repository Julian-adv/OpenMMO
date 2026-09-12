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
const clipsByModel = new Map<
  string,
  Promise<Map<string, THREE.AnimationClip>>
>()

export function loadEnchantAnimations(modelPath: string, root: THREE.Object3D) {
  let pending = clipsByModel.get(modelPath)
  if (!pending) {
    pending = loadGLB(CHARACTER_ANIMATION_PACK_PATHS.social)
      .then(async (gltf) => {
        const clips = gltf.animations.filter(
          (clip) =>
            clip.name === ENCHANT_WEAPON_ANIMATION ||
            clip.name === ENCHANT_LEFT_WEAPON_ANIMATION
        )
        const retargeted = await retargetAnimationsForCharacterModel(
          root,
          gltf.scene,
          clips
        )
        const grounded = await groundRetargetedClips(root, retargeted)
        return new Map(grounded.map((clip) => [clip.name, clip]))
      })
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

  update(weight: number) {
    this.weapon.quaternion.copy(this.rotation)
    if (weight <= 0 || !this.weapon.parent) return
    this.weapon.parent.getWorldQuaternion(this.inverseHand).invert()
    this.up.set(0, 1, 0).applyQuaternion(this.inverseHand)
    this.direction
      .copy(getWeaponEffectAxis(this.weapon).direction)
      .applyQuaternion(this.rotation)
    this.correction.setFromUnitVectors(this.direction, this.up)
    this.correction.multiply(this.rotation)
    this.weapon.quaternion.slerp(this.correction, weight)
  }
}

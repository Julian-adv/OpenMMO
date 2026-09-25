import type * as THREE from 'three'
import { findBoneByName } from './characterAnimationUtils'

export interface EnchantEffectAnchor {
  position: THREE.Vector3
  weapon: THREE.Object3D | null
}

export class PlayerEffectAnchors {
  private rightHand: THREE.Bone | undefined
  private leftHand: THREE.Bone | undefined
  private torso: THREE.Bone | undefined

  constructor(root: THREE.Object3D) {
    this.rightHand = findBoneByName(root, 'RightHand')
    this.leftHand = findBoneByName(root, 'LeftHand')
    this.torso = findBoneByName(root, 'Spine1') ?? findBoneByName(root, 'Spine')
  }

  getWorldPosition(
    weapon: boolean,
    hand: 'RightHand' | 'LeftHand',
    target: THREE.Vector3
  ): boolean {
    const bone = weapon
      ? hand === 'LeftHand'
        ? this.leftHand
        : this.rightHand
      : this.torso
    if (!bone) return false
    bone.getWorldPosition(target)
    return true
  }
}

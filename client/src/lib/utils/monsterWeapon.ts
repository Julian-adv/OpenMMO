import type * as THREE from 'three'
import {
  parseWeaponRotation,
  type MonsterDefinition,
} from '../data/monsterDefs'

type WeaponGrip = Pick<
  MonsterDefinition,
  'weaponOffset' | 'weaponOffsetX' | 'weaponOffsetZ' | 'weaponRotation'
>

/**
 * Seat a monster's hand weapon on `bone` with the csv grip: weaponOffset slides
 * it from the wrist to the palm, x/z and the rotation settle it into the fist.
 * A baked `WeaponSocket` that skill clips turn takes the grip instead of the
 * weapon. Returns the node the weapon goes under.
 */
export function seatMonsterWeapon(
  bone: THREE.Object3D,
  weapon: THREE.Object3D,
  def: WeaponGrip
): THREE.Object3D {
  const socket = bone.getObjectByName('WeaponSocket')
  const grip = socket ?? weapon
  grip.position.set(
    def.weaponOffsetX ?? 0,
    def.weaponOffset ?? 0,
    def.weaponOffsetZ ?? 0
  )
  grip.rotation.set(...parseWeaponRotation(def.weaponRotation))
  return socket ?? bone
}

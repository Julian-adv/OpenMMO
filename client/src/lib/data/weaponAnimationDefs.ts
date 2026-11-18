import definitions from '../../../../data/weapon_animations.json'
import { getItemDef, type ItemDefinition, type WeaponType } from './itemDefs'

export interface WeaponAnimationDefinition {
  id: WeaponType
  pack: string
  idle?: string
  walk?: string
  run?: string
  attack?: string
  attackImpactMs?: number
  offHandGripReach?: number
  /** The off hand holds the shaft ahead of the main hand, toward the point. */
  offHandAhead?: boolean
  /** The idle clip is a one-handed rest pose: the stance between swings is
   *  the shared combat idle and the off hand does not hold the weapon in it. */
  restIdle?: boolean
}

const profiles = definitions as Partial<
  Record<WeaponType, WeaponAnimationDefinition>
>

export function weaponAnimationFor(
  def: Pick<ItemDefinition, 'weaponType'> | undefined
) {
  return def?.weaponType ? profiles[def.weaponType] : undefined
}

export function getWeaponAnimation(itemId: string | null | undefined) {
  return weaponAnimationFor(itemId ? getItemDef(itemId) : undefined)
}

export function weaponAnimationClipName(
  profile: WeaponAnimationDefinition | undefined,
  state: string,
  movementMode: string | undefined
): string | undefined {
  if (state === 'attack') return profile?.attack
  if (state === 'idle') return profile?.idle
  if (state === 'moving')
    return movementMode === 'run' ? profile?.run : profile?.walk
  return undefined
}

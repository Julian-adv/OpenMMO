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
  gripRotationRadians?: string
  offHandGripReach?: number
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

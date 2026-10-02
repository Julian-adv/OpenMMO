import { getItemDef } from '../data/itemDefs'
import type { ArmorEquipment } from '../network/networkTypes'
import { DEFAULT_MODULAR_OUTFIT, type ModularOutfit } from './modularCharacter'

const ARMOR_STYLES = {
  chest: {
    'armor/rogue_top.glb': 'rogue',
    'armor/plate_armor.glb': 'plate',
    'armor/barbarian_armor.glb': 'barbarian',
  },
  pants: {
    'armor/rogue_pants.glb': 'rogue',
    'armor/iron_leggings.glb': 'plate',
    'armor/barbarian_pants.glb': 'barbarian',
  },
  boots: {
    'armor/rogue_boots.glb': 'rogue',
    'armor/plate_greaves.glb': 'plate',
    'armor/barbarian_boots.glb': 'barbarian',
  },
  hands: {
    'armor/rogue_gloves.glb': 'rogue',
    'armor/plate_gauntlets.glb': 'plate',
    'armor/barbarian_bracers.glb': 'barbarian',
  },
  head: {
    'armor/plate_helmet.glb': 'plate',
    'armor/barbarian_helmet.glb': 'barbarian',
  },
} as const

export function modularOutfitForArmor(
  armor: ArmorEquipment = {}
): ModularOutfit {
  const style = <Style extends string>(
    models: Record<string, Style>,
    id: string | null | undefined
  ) => models[getItemDef(id ?? '')?.worldModel ?? '']
  return {
    ...DEFAULT_MODULAR_OUTFIT,
    top: armor.chest
      ? (style(ARMOR_STYLES.chest, armor.chest) ?? DEFAULT_MODULAR_OUTFIT.top)
      : 'none',
    pants: armor.pants
      ? (style(ARMOR_STYLES.pants, armor.pants) ?? 'cloth')
      : 'none',
    boots: armor.boots
      ? (style(ARMOR_STYLES.boots, armor.boots) ?? DEFAULT_MODULAR_OUTFIT.boots)
      : 'none',
    gloves: style(ARMOR_STYLES.hands, armor.hands) ?? 'none',
    helmet: style(ARMOR_STYLES.head, armor.head) ?? 'none',
  }
}

import { getItemDef } from '../data/itemDefs'
import type { ArmorEquipment } from '../network/networkTypes'
import { DEFAULT_MODULAR_OUTFIT, type ModularOutfit } from './modularCharacter'

const ARMOR_STYLES = {
  chest: {
    'armor/plate_armor.glb': 'plate',
    'armor/barbarian_armor.glb': 'barbarian',
  },
  pants: {
    'armor/iron_leggings.glb': 'plate',
    'armor/barbarian_pants.glb': 'barbarian',
  },
  boots: {
    'armor/plate_greaves.glb': 'plate',
    'armor/barbarian_boots.glb': 'barbarian',
  },
  hands: {
    'armor/plate_gauntlets.glb': 'plate',
    'armor/barbarian_bracers.glb': 'barbarian',
  },
  head: {
    'armor/plate_helmet.glb': 'plate',
    'armor/barbarian_helmet.glb': 'barbarian',
  },
} satisfies Record<string, Record<string, 'plate' | 'barbarian'>>

export function modularOutfitForArmor(
  armor: ArmorEquipment = {}
): ModularOutfit {
  const style = (slot: keyof typeof ARMOR_STYLES) => {
    const models: Record<string, 'plate' | 'barbarian'> = ARMOR_STYLES[slot]
    return models[getItemDef(armor[slot] ?? '')?.worldModel ?? '']
  }
  return {
    ...DEFAULT_MODULAR_OUTFIT,
    top: armor.chest ? (style('chest') ?? DEFAULT_MODULAR_OUTFIT.top) : 'none',
    pants: armor.pants ? (style('pants') ?? 'cloth') : 'none',
    boots: armor.boots
      ? (style('boots') ?? DEFAULT_MODULAR_OUTFIT.boots)
      : 'none',
    gloves: style('hands') ?? 'none',
    helmet: style('head') ?? 'none',
  }
}

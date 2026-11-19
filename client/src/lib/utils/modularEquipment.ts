import { getItemDef } from '../data/itemDefs'
import type { ArmorEquipment } from '../network/networkTypes'
import { DEFAULT_MODULAR_OUTFIT, type ModularOutfit } from './modularCharacter'

const ARMOR_STYLES = {
  chest: {
    'armor/guard_top.glb': 'guard',
    'armor/tuxedo_top.glb': 'tuxedo',
    'armor/bard_top.glb': 'bard',
    'armor/priest_top.glb': 'priest',
    'armor/ranger_top.glb': 'ranger',
    'armor/caveman_top.glb': 'caveman',
    'armor/rogue_top.glb': 'rogue',
    'armor/plate_armor.glb': 'plate',
    'armor/barbarian_armor.glb': 'barbarian',
  },
  pants: {
    'armor/guard_pants.glb': 'guard',
    'armor/tuxedo_pants.glb': 'tuxedo',
    'armor/bard_pants.glb': 'bard',
    'armor/priest_pants.glb': 'priest',
    'armor/ranger_pants.glb': 'ranger',
    'armor/caveman_pants.glb': 'caveman',
    'armor/rogue_pants.glb': 'rogue',
    'armor/iron_leggings.glb': 'plate',
    'armor/barbarian_pants.glb': 'barbarian',
  },
  boots: {
    'armor/guard_boots.glb': 'guard',
    'armor/tuxedo_boots.glb': 'tuxedo',
    'armor/bard_boots.glb': 'bard',
    'armor/priest_boots.glb': 'priest',
    'armor/ranger_boots.glb': 'ranger',
    'armor/caveman_boots.glb': 'caveman',
    'armor/rogue_boots.glb': 'rogue',
    'armor/plate_greaves.glb': 'plate',
    'armor/barbarian_boots.glb': 'barbarian',
  },
  hands: {
    'armor/guard_gloves.glb': 'guard',
    'armor/tuxedo_gloves.glb': 'tuxedo',
    'armor/ranger_gloves.glb': 'ranger',
    'armor/caveman_bracers.glb': 'caveman',
    'armor/rogue_gloves.glb': 'rogue',
    'armor/plate_gauntlets.glb': 'plate',
    'armor/barbarian_bracers.glb': 'barbarian',
  },
  head: {
    'armor/guard_helmet.glb': 'guard',
    'armor/bard_helmet.glb': 'bard',
    'armor/priest_helmet.glb': 'priest',
    'armor/plate_helmet.glb': 'plate',
    'armor/barbarian_helmet.glb': 'barbarian',
    'armor/helm_of_brilliance.glb': 'brilliance',
  },
} as const

/** Female-only clothes show the default outfit on the male body. */
const FEMALE_ONLY_MODEL = /^armor\/(ceremony|shopkeeper|maid|pink_maid)_/

const MATERIAL_STYLES = { metal: 'plate', leather: 'rogue' } as const
const HELMET_MATERIAL_STYLES = { metal: 'plate', leather: 'plate' } as const

export function modularOutfitForArmor(
  armor: ArmorEquipment = {}
): ModularOutfit {
  const style = <Style extends string>(
    models: Record<string, Style>,
    id: string | null | undefined,
    materials: Record<string, Style>
  ) => {
    const item = getItemDef(id ?? '')
    if (FEMALE_ONLY_MODEL.test(item?.worldModel ?? '')) return undefined
    return models[item?.worldModel ?? ''] ?? materials[item?.material ?? '']
  }
  return {
    ...DEFAULT_MODULAR_OUTFIT,
    top: armor.chest
      ? (style(ARMOR_STYLES.chest, armor.chest, MATERIAL_STYLES) ??
        DEFAULT_MODULAR_OUTFIT.top)
      : 'none',
    pants: armor.pants
      ? (style(ARMOR_STYLES.pants, armor.pants, MATERIAL_STYLES) ?? 'cloth')
      : 'none',
    boots: armor.boots
      ? (style(ARMOR_STYLES.boots, armor.boots, MATERIAL_STYLES) ??
        DEFAULT_MODULAR_OUTFIT.boots)
      : 'none',
    gloves: style(ARMOR_STYLES.hands, armor.hands, MATERIAL_STYLES) ?? 'none',
    helmet:
      style(ARMOR_STYLES.head, armor.head, HELMET_MATERIAL_STYLES) ?? 'none',
  }
}

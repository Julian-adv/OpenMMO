import { getItemDef } from '../data/itemDefs'
import type { ArmorEquipment } from '../network/networkTypes'
import { DEFAULT_MODULAR_OUTFIT, type ModularOutfit } from './modularCharacter'

export function modularOutfitForArmor(
  armor: ArmorEquipment = {}
): ModularOutfit {
  const model = (slot: keyof ArmorEquipment) =>
    getItemDef(armor[slot] ?? '')?.worldModel
  return {
    ...DEFAULT_MODULAR_OUTFIT,
    top:
      model('chest') === 'armor/plate_armor.glb'
        ? 'plate'
        : DEFAULT_MODULAR_OUTFIT.top,
    pants: model('pants') === 'armor/iron_leggings.glb' ? 'plate' : 'cloth',
    boots:
      model('boots') === 'armor/plate_greaves.glb'
        ? 'plate'
        : DEFAULT_MODULAR_OUTFIT.boots,
    gloves: model('hands') === 'armor/plate_gauntlets.glb' ? 'plate' : 'none',
    helmet: model('head') === 'armor/plate_helmet.glb' ? 'plate' : 'none',
  }
}

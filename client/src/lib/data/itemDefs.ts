import itemsJson from '../../../../data/items.json'
import type { EquipSlot } from '../network/networkTypes'

export interface ItemDefinition {
  id: string
  name: string
  description: string
  weight: number
  equipSlot: EquipSlot | null
  stackable: boolean
  icon: string
  worldModel?: string
}

const itemDefs = itemsJson as Record<string, ItemDefinition>

export function getItemDef(itemDefId: string): ItemDefinition | undefined {
  return itemDefs[itemDefId]
}

export default itemDefs

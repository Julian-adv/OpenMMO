import { get, writable } from 'svelte/store'
import { AUSCULTATION, abilityEquipmentAllowed } from '../data/abilities'
import type { EquipSlot, PlayerInventory } from '../network/networkTypes'
import { canBeginAbility, beginAbility } from './abilityStore'

export type InspectionTarget =
  | { kind: 'player'; player_id: number }
  | { kind: 'monster'; monster_id: string }

export type InspectionResult = {
  target: InspectionTarget
  name: string
  level: number
  health: number
  max_health: number
  guard: number
  equipment: { slot: EquipSlot; item_def_id: string; enchant: number }[]
}

export const inspectionTargeting = writable(false)
export const inspectionResult = writable<InspectionResult | null>(null)

export function cancelInspection() {
  inspectionTargeting.set(false)
}

export function queueInspection(
  equipped: PlayerInventory['equipped'],
  now = Date.now()
) {
  if (get(inspectionTargeting)) {
    cancelInspection()
    return false
  }
  if (
    !abilityEquipmentAllowed(AUSCULTATION.id, equipped) ||
    !canBeginAbility(AUSCULTATION.id, now)
  )
    return false
  inspectionResult.set(null)
  inspectionTargeting.set(true)
  return true
}

export function takeInspectionTarget(
  target: InspectionTarget | null,
  equipped: PlayerInventory['equipped'],
  now = Date.now()
) {
  if (!get(inspectionTargeting)) return null
  if (!abilityEquipmentAllowed(AUSCULTATION.id, equipped)) {
    cancelInspection()
    return null
  }
  if (!target || !beginAbility(AUSCULTATION.id, now)) return null
  cancelInspection()
  return target
}

export function resetInspection() {
  cancelInspection()
  inspectionResult.set(null)
}

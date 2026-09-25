import { beforeEach, expect, it } from 'vitest'
import { get } from 'svelte/store'
import {
  AUSCULTATION,
  abilityEquipmentAllowed,
  isAbilityAvailable,
} from '../data/abilities'
import type { PlayerInventory } from '../network/networkTypes'
import {
  abilityCooldowns,
  abilityPending,
  resetAbilities,
} from './abilityStore'
import {
  cancelInspection,
  inspectionResult,
  inspectionTargeting,
  queueInspection,
  resetInspection,
  takeInspectionTarget,
} from './inspectionStore'
import { quickslots, assignQuickslot } from './quickslotStore'

const equipped: PlayerInventory['equipped'] = {
  neck: {
    instance_id: 1,
    item_def_id: 'stethoscope',
    quantity: 1,
    enchant: 0,
    locked: false,
  },
}

beforeEach(() => {
  resetAbilities()
  resetInspection()
})

it('is available to every class and requires the stethoscope in the neck slot', () => {
  for (const characterClass of [
    'knight',
    'rogue',
    'merchant',
    'priest',
  ] as const) {
    expect(isAbilityAvailable(AUSCULTATION.id, characterClass)).toBe(true)
  }
  expect(abilityEquipmentAllowed(AUSCULTATION.id, equipped)).toBe(true)
  expect(abilityEquipmentAllowed(AUSCULTATION.id, {})).toBe(false)
  expect(
    abilityEquipmentAllowed(AUSCULTATION.id, { main_hand: equipped.neck })
  ).toBe(false)
  expect(
    abilityEquipmentAllowed(AUSCULTATION.id, {
      neck: { ...equipped.neck!, item_def_id: 'silver_necklace' },
    })
  ).toBe(false)
})

it('waits for a left-click target without starting a request and ignores empty ground', () => {
  expect(queueInspection(equipped, 1000)).toBe(true)
  expect(get(abilityPending)).toEqual({})
  expect(takeInspectionTarget(null, equipped, 2000)).toBeNull()
  expect(get(inspectionTargeting)).toBe(true)
  const target = { kind: 'player', player_id: 42 } as const
  expect(takeInspectionTarget(target, equipped, 2000)).toEqual(target)
  expect(get(inspectionTargeting)).toBe(false)
  expect(get(abilityPending).auscultation).toBe(5000)
  expect(takeInspectionTarget(target, equipped, 2001)).toBeNull()
})

it('supports monsters and refuses requests after the stethoscope is removed', () => {
  const target = { kind: 'monster', monster_id: 'goblin-1' } as const
  queueInspection(equipped, 1000)
  expect(takeInspectionTarget(target, {}, 1000)).toBeNull()
  expect(get(inspectionTargeting)).toBe(false)
  expect(get(abilityPending)).toEqual({})
  queueInspection(equipped, 1000)
  expect(takeInspectionTarget(target, equipped, 1000)).toEqual(target)
})

it('keeps the quickslot binding through cancellation and removal', () => {
  assignQuickslot(0, { skill: AUSCULTATION.id })
  queueInspection(equipped)
  cancelInspection()
  expect(get(inspectionTargeting)).toBe(false)
  expect(queueInspection({})).toBe(false)
  expect(get(quickslots)[0]).toEqual({ skill: AUSCULTATION.id })
  expect(queueInspection(equipped)).toBe(true)
  expect(queueInspection(equipped)).toBe(false)
  expect(get(inspectionTargeting)).toBe(false)
})

it('blocks a pending request and server cooldown, and clears results on logout', () => {
  abilityPending.set({ auscultation: 2000 })
  expect(queueInspection(equipped, 1999)).toBe(false)
  abilityPending.set({})
  abilityCooldowns.set({ auscultation: 2800 })
  expect(queueInspection(equipped, 2799)).toBe(false)
  expect(queueInspection(equipped, 2800)).toBe(true)
  inspectionResult.set({
    target: { kind: 'player', player_id: 2 },
    name: 'Target',
    level: 1,
    health: 10,
    max_health: 10,
    guard: 10,
    equipment: [],
  })
  resetInspection()
  expect(get(inspectionTargeting)).toBe(false)
  expect(get(inspectionResult)).toBeNull()
})

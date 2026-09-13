import { beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import {
  getAbility,
  guardianWardEquipment,
  abilityEquipmentAllowed,
} from '../data/abilities'
import {
  abilityCooldowns,
  abilityPending,
  activeBuffs,
  bowMark,
  updateBowMark,
  beginAbility,
  queueAbilityEffect,
  resetAbilities,
  takeAbilityEffects,
  timerSnapshot,
  visibleAbilityBuffs,
} from './abilityStore'
import type { ItemInstance } from '../network/networkTypes'

const item = (item_def_id: string): ItemInstance => ({
  instance_id: 1,
  item_def_id,
  enchant: 9,
  quantity: 1,
  locked: false,
})

beforeEach(() => {
  resetAbilities()
  vi.useRealTimers()
})

it('requires a Bow for True Aim and keeps its private mark separate from buff timers', () => {
  expect(abilityEquipmentAllowed('bow_mark', { main_hand: item('bow') })).toBe(
    true
  )
  for (const weapon of ['dagger', 'iron_sword', 'morningstar', 'torch'])
    expect(
      abilityEquipmentAllowed('bow_mark', { main_hand: item(weapon) })
    ).toBe(false)
  expect(abilityEquipmentAllowed('bow_mark', {})).toBe(false)
  updateBowMark('target', 5000, 1000)
  expect(get(bowMark)).toEqual({
    monsterId: 'target',
    startedAt: 1000,
    until: 6000,
  })
  expect(get(activeBuffs)).toEqual({})
  updateBowMark('target', 4000, 2000)
  expect(get(bowMark)?.startedAt).toBe(1000)
  updateBowMark(null, 0, 3000)
  expect(get(bowMark)?.until).toBe(3000)
  updateBowMark(null, 0, 4000)
  expect(get(bowMark)?.until).toBe(3000)
  resetAbilities()
  expect(get(bowMark)).toBeNull()
})

it('ends True Aim at five seconds and keeps its ten-second cooldown', () => {
  vi.useFakeTimers()
  vi.setSystemTime(1000)
  activeBuffs.set(timerSnapshot([{ ability: 'bow_mark', remaining_ms: 5000 }]))
  abilityCooldowns.set(
    timerSnapshot([{ ability: 'bow_mark', remaining_ms: 10000 }])
  )
  const unsubscribe = visibleAbilityBuffs.subscribe(() => {})
  expect(get(visibleAbilityBuffs).map((buff) => buff.id)).toEqual(['bow_mark'])
  vi.advanceTimersByTime(5000)
  expect(get(visibleAbilityBuffs)).toEqual([])
  expect(beginAbility('bow_mark')).toBe(false)
  vi.advanceTimersByTime(5000)
  expect(beginAbility('bow_mark')).toBe(true)
  unsubscribe()
})

describe('Guardian Ward', () => {
  it('uses actual equipped weapon and armor classifications', () => {
    for (const weapon of [
      'iron_sword',
      'goblin_sword',
      'small_sword',
      'morningstar',
    ]) {
      expect(
        guardianWardEquipment({
          main_hand: item(weapon),
          off_hand: item('raven_shield'),
        })
      ).toBe(true)
    }
    for (const weapon of ['dagger', 'great_sword', 'torch']) {
      expect(
        guardianWardEquipment({
          main_hand: item(weapon),
          off_hand: item('wooden_shield'),
        })
      ).toBe(false)
    }
    expect(
      guardianWardEquipment({
        main_hand: item('iron_sword'),
        off_hand: item('torch'),
      })
    ).toBe(false)
    expect(guardianWardEquipment({ main_hand: item('iron_sword') })).toBe(false)
  })

  it('waits for server cooldown and releases a lost acknowledgement after three seconds', () => {
    expect(beginAbility('guardian_ward', 1000)).toBe(true)
    expect(beginAbility('guardian_ward', 3999)).toBe(false)
    expect(beginAbility('guardian_ward', 4000)).toBe(true)
    abilityCooldowns.set(
      timerSnapshot([{ ability: 'guardian_ward', remaining_ms: 45000 }], 5000)
    )
    abilityPending.set({})
    expect(beginAbility('guardian_ward', 49999)).toBe(false)
    expect(beginAbility('guardian_ward', 50000)).toBe(true)
  })

  it('replaces buff timers and clears all session state on logout', () => {
    activeBuffs.set(
      timerSnapshot([{ ability: 'guardian_ward', remaining_ms: 60000 }], 1000)
    )
    activeBuffs.set(
      timerSnapshot([{ ability: 'guardian_ward', remaining_ms: 60000 }], 46000)
    )
    expect(get(activeBuffs).guardian_ward).toBe(106000)
    queueAbilityEffect({
      ability: 'guardian_ward',
      player_id: 1,
      position: { x: 0, y: 0, z: 0 },
      floor_level: 0,
      targets: [1],
    })
    resetAbilities()
    expect(get(activeBuffs)).toEqual({})
    expect(get(abilityCooldowns)).toEqual({})
    expect(takeAbilityEffects()).toEqual([])
  })
})

it('resolves both abilities for saved quickslots and shared tooltips', () => {
  expect(getAbility('guardian_ward')?.name).toBe('Guardian Ward')
  expect(getAbility('dagger_double_slash')?.name).toBe('Double Slash')
  expect(getAbility('radiance')?.name).toBe('Radiance')
  expect(getAbility('bow_mark')?.name).toBe('True Aim')
  expect(getAbility('unknown')).toBeUndefined()
})

it('allows Radiance with empty hands and every weapon while keeping Ward restrictions', () => {
  for (const equipped of [
    {},
    ...[
      'dagger',
      'iron_sword',
      'morningstar',
      'spear',
      'great_sword',
      'bow',
    ].map((id) => ({ main_hand: item(id) })),
  ]) {
    expect(abilityEquipmentAllowed('radiance', equipped)).toBe(true)
    expect(abilityEquipmentAllowed('guardian_ward', equipped)).toBe(false)
  }
})

it('uses the server Radiance cooldown without blocking another ability', () => {
  abilityCooldowns.set(
    timerSnapshot([{ ability: 'radiance', remaining_ms: 800 }], 1000)
  )
  expect(beginAbility('radiance', 1799)).toBe(false)
  expect(beginAbility('guardian_ward', 1799)).toBe(true)
  expect(beginAbility('radiance', 1800)).toBe(true)
})

it('shows both buff timers and removes only the one that ends', () => {
  vi.useFakeTimers()
  vi.setSystemTime(1000)
  activeBuffs.set(
    timerSnapshot([
      { ability: 'radiance', remaining_ms: 120000 },
      { ability: 'guardian_ward', remaining_ms: 60000 },
    ])
  )
  const unsubscribe = visibleAbilityBuffs.subscribe(() => {})
  expect(get(visibleAbilityBuffs).map((buff) => buff.id)).toEqual([
    'guardian_ward',
    'radiance',
  ])
  vi.advanceTimersByTime(60000)
  expect(get(visibleAbilityBuffs).map((buff) => buff.id)).toEqual(['radiance'])
  activeBuffs.set({})
  expect(get(visibleAbilityBuffs)).toEqual([])
  unsubscribe()
  vi.useRealTimers()
})

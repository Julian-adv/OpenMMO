import { beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'
import { getAbility, guardianWardEquipment } from '../data/abilities'
import {
  abilityCooldowns,
  abilityPending,
  activeBuffs,
  beginAbility,
  queueAbilityEffect,
  resetAbilities,
  takeAbilityEffects,
  timerSnapshot,
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
  expect(getAbility('unknown')).toBeUndefined()
})

import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { readFileSync } from 'node:fs'
import { initSync } from '../wasm/onlinerpg_shared'
import { get } from 'svelte/store'
import {
  getAbility,
  guardianWardEquipment,
  abilityEquipmentAllowed,
  GUARDIAN_WARD,
  DOUBLE_SLASH,
} from '../data/abilities'
import {
  abilityCooldowns,
  abilityClock,
  applyAbilityCooldowns,
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
import {
  acknowledgeDaggerSkill,
  consumeDaggerSkill,
  daggerSkillClock,
  daggerSkillState,
  queueDaggerSkill,
  resetDaggerSkill,
} from './daggerSkillStore'

const item = (item_def_id: string): ItemInstance => ({
  instance_id: 1,
  item_def_id,
  enchant: 9,
  quantity: 1,
  locked: false,
})

beforeAll(() => {
  initSync({
    module: readFileSync(
      new URL('../wasm/onlinerpg_shared_bg.wasm', import.meta.url)
    ),
  })
})

it('uses the server mana cost in the Ward tooltip and keeps Double Slash free', () => {
  expect(GUARDIAN_WARD.manaCost).toBe(2)
  expect(GUARDIAN_WARD.stats.find((stat) => stat.label === 'Cost')?.value).toBe(
    '2 MP'
  )
  expect(DOUBLE_SLASH.manaCost).toBe(0)
})

beforeEach(() => {
  resetAbilities()
  resetDaggerSkill()
  vi.useRealTimers()
})

it('restores the Double Slash countdown on reconnect and blocks queuing until expiry', () => {
  vi.useFakeTimers()
  vi.setSystemTime(1_000)
  acknowledgeDaggerSkill(10_000)
  resetDaggerSkill()
  vi.setSystemTime(4_000)
  const unsubscribe = daggerSkillClock.subscribe(() => {})
  applyAbilityCooldowns([
    { ability: 'dagger_double_slash', remaining_ms: 7_000 },
  ])
  expect(get(daggerSkillState).cooldownUntil - get(daggerSkillClock)).toBe(
    7_000
  )
  expect(queueDaggerSkill()).toBe(false)
  vi.advanceTimersByTime(6_999)
  expect(queueDaggerSkill()).toBe(false)
  vi.advanceTimersByTime(1)
  expect(queueDaggerSkill()).toBe(true)
  unsubscribe()
})

it('keeps a queued or pending Double Slash when another ability sends a snapshot', () => {
  queueDaggerSkill(100)
  applyAbilityCooldowns(
    [{ ability: 'dagger_double_slash', remaining_ms: 0 }],
    150
  )
  expect(consumeDaggerSkill(200)).toBe(true)
  applyAbilityCooldowns(
    [{ ability: 'dagger_double_slash', remaining_ms: 9_000 }],
    250
  )
  expect(get(daggerSkillState).pending).toBe(true)
  expect(queueDaggerSkill(300)).toBe(false)
  acknowledgeDaggerSkill(8_900, 350)
  expect(get(daggerSkillState).pending).toBe(false)
})

it('ticks through the longest cooldown, buff or pending deadline and stops when cleared', () => {
  vi.useFakeTimers()
  vi.setSystemTime(1_000)
  abilityCooldowns.set({ guardian_ward: 1_300 })
  activeBuffs.set({ radiance: 1_200 })
  abilityPending.set({ bow_mark: 1_400 })
  const unsubscribe = abilityClock.subscribe(() => {})
  vi.advanceTimersByTime(300)
  expect(get(abilityClock)).toBe(1_300)
  expect(vi.getTimerCount()).toBe(1)
  vi.advanceTimersByTime(100)
  expect(get(abilityClock)).toBe(1_400)
  expect(vi.getTimerCount()).toBe(0)
  abilityCooldowns.set({ guardian_ward: 1_600 })
  expect(vi.getTimerCount()).toBe(1)
  resetAbilities()
  expect(vi.getTimerCount()).toBe(0)
  unsubscribe()
})

it('requires a dagger for Double Slash through the shared equipment check', () => {
  expect(abilityEquipmentAllowed('dagger_double_slash', {})).toBe(false)
  expect(
    abilityEquipmentAllowed('dagger_double_slash', {
      main_hand: item('dagger'),
    })
  ).toBe(true)
  for (const weapon of ['bow', 'iron_sword', 'morningstar', 'torch']) {
    expect(
      abilityEquipmentAllowed('dagger_double_slash', {
        main_hand: item(weapon),
      })
    ).toBe(false)
  }
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

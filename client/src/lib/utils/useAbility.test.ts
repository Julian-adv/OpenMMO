import { readFileSync } from 'node:fs'
import { get, writable, type Writable } from 'svelte/store'
import { afterEach, beforeAll, beforeEach, expect, it, vi } from 'vitest'
import { initSharedWasm, testItem } from './ability.fixture'
import { networkManager } from '../network/socket'
import { combatController } from '../managers/combatController'
import { abilityCooldowns, resetAbilities } from '../stores/abilityStore'
import {
  acknowledgeDaggerSkill,
  daggerSkillState,
  resetDaggerSkill,
} from '../stores/daggerSkillStore'
import {
  gameStore,
  reportSkillFailure,
  type LocalPlayer,
} from '../stores/gameStore'
import { inspectionTargeting, resetInspection } from '../stores/inspectionStore'
import { inventoryStore } from '../stores/inventoryStore'
import { manaState } from '../stores/manaStore'
import {
  assignQuickslot,
  quickslots,
  resetQuickslots,
} from '../stores/quickslotStore'
import { useAbility } from './useAbility'
import { chargeTargeting } from '../stores/chargeStrikeStore'
import { FISHING, getAbility, isAbilityAvailable } from '../data/abilities'
import {
  fishingTargeting,
  myFishing,
  resetFishingStore,
} from '../stores/fishingStore'
import { skillsStore, resetSkillsStore } from '../stores/skillsStore'
import { hoveredPartyMemberId, partyRoster } from '../stores/partyStore'
import { monsterManager } from '../managers/monsterManager'
import {
  hoveredMonsterId as hoveredMonster,
  hoveredPlayerId as hoveredPlayer,
  type RemotePlayer,
} from '../stores/gameStore'
import { currentDungeonDepth } from '../stores/dungeonStore'
import { playerVisualFloorLevel } from '../stores/housingStore'

const hoveredMonsterId = hoveredMonster as unknown as Writable<string | null>
const hoveredPlayerId = hoveredPlayer as unknown as Writable<number | null>

vi.mock('../stores/gameStore', () => ({
  gameStore: writable({ currentPlayer: null, otherPlayers: new Map() }),
  hoveredMonsterId: writable(null),
  hoveredPlayerId: writable(null),
  reportSkillFailure: vi.fn(),
}))
vi.mock('../network/socket', () => ({
  networkManager: { sendUseAbility: vi.fn() },
}))
vi.mock('../managers/combatController', () => ({
  combatController: { getAbilityTarget: vi.fn() },
}))
vi.mock('../managers/monsterManager', () => ({
  monsterManager: { monsters: new Map() },
}))

function updatePlayer(patch: Partial<LocalPlayer>) {
  gameStore.update((state) => ({
    ...state,
    currentPlayer: { ...state.currentPlayer!, ...patch },
  }))
}

beforeAll(initSharedWasm)

beforeEach(() => {
  vi.clearAllMocks()
  chargeTargeting.set(false)
  vi.useFakeTimers()
  vi.setSystemTime(1000)
  resetAbilities()
  resetDaggerSkill()
  resetInspection()
  resetQuickslots()
  resetFishingStore()
  resetSkillsStore()
  currentDungeonDepth.set(0)
  playerVisualFloorLevel.set(0)
  updatePlayer({ characterClass: 'knight', health: 10, mount: null })
  inventoryStore.set({
    bag: [],
    equipped: {
      main_hand: testItem('iron_sword'),
      off_hand: testItem('wooden_shield'),
      neck: testItem('stethoscope'),
    },
  })
  manaState.set({ mana: 10, max_mana: 10 })
})

afterEach(() => vi.useRealTimers())

it('casts without a quickslot and suppresses duplicate requests while pending', () => {
  useAbility('guardian_ward')
  useAbility('guardian_ward')
  expect(networkManager.sendUseAbility).toHaveBeenCalledExactlyOnceWith(
    'guardian_ward',
    null
  )
  expect(get(quickslots).every((slot) => slot === null)).toBe(true)
  expect(reportSkillFailure).not.toHaveBeenCalled()
})

it.each([
  [{ health: 0 }, 'You cannot use skills while dead.'],
  [{ mount: 'horse' }, 'You cannot use skills while mounted.'],
] as const)('rejects unavailable player states: %j', (patch, message) => {
  updatePlayer(patch)
  useAbility('guardian_ward')
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  expect(reportSkillFailure).toHaveBeenCalledWith(message)
})

it('keeps equipment errors for missing skill items', () => {
  inventoryStore.set({ bag: [], equipped: {} })
  useAbility('auscultation')
  expect(reportSkillFailure).toHaveBeenCalledWith(
    'Equip a stethoscope to use Auscultation.'
  )
  expect(get(inspectionTargeting)).toBe(false)
  useAbility('guardian_ward')
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    'Cannot use Guardian Ward.'
  )
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
})

it('rejects insufficient mana and honors the server cooldown', () => {
  manaState.set({ mana: 1, max_mana: 10 })
  useAbility('guardian_ward')
  expect(reportSkillFailure).toHaveBeenLastCalledWith('Not enough mana.')
  manaState.set({ mana: 2, max_mana: 10 })
  abilityCooldowns.set({ guardian_ward: 2000 })
  useAbility('guardian_ward')
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    'Guardian Ward is not ready yet.'
  )
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  vi.setSystemTime(2000)
  useAbility('guardian_ward')
  expect(networkManager.sendUseAbility).toHaveBeenCalledOnce()
})

it('queues and cancels inspection, and cancels it when using another skill', () => {
  useAbility('auscultation')
  expect(get(inspectionTargeting)).toBe(true)
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  useAbility('auscultation')
  expect(get(inspectionTargeting)).toBe(false)
  useAbility('auscultation')
  useAbility('guardian_ward')
  expect(get(inspectionTargeting)).toBe(false)
  expect(networkManager.sendUseAbility).toHaveBeenCalledOnce()
})

it.each(['dagger', 'worn_dagger'])(
  'queues Double Slash with %s, toggles it off, and honors its cooldown',
  (itemId) => {
    updatePlayer({ characterClass: 'rogue' })
    inventoryStore.set({ bag: [], equipped: { main_hand: testItem(itemId) } })
    manaState.set({ mana: 0, max_mana: 10 })
    useAbility('dagger_double_slash')
    expect(get(daggerSkillState).queued).toBe(true)
    useAbility('dagger_double_slash')
    expect(get(daggerSkillState).queued).toBe(false)
    acknowledgeDaggerSkill(10000)
    useAbility('dagger_double_slash')
    expect(get(daggerSkillState).queued).toBe(false)
    expect(reportSkillFailure).toHaveBeenCalledWith(
      'Double Slash is not ready yet.'
    )
    expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  }
)

it('ignores unknown, hidden, and other-class skills', () => {
  for (const id of ['unknown', 'radiance', 'bow_mark', 'dagger_double_slash'])
    useAbility(id)
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  expect(get(daggerSkillState).queued).toBe(false)
  expect(reportSkillFailure).not.toHaveBeenCalled()
})

it('lists Fishing for every class only after learning it, with an icon and instructions', () => {
  for (const characterClass of [
    'knight',
    'rogue',
    'merchant',
    'priest',
  ] as const) {
    expect(isAbilityAvailable(FISHING.id, characterClass)).toBe(false)
    expect(isAbilityAvailable(FISHING.id, characterClass, [FISHING.id])).toBe(
      true
    )
  }
  const fishing = getAbility('fishing')!
  expect(
    readFileSync(new URL(`../../../public${fishing.icon}`, import.meta.url))
      .length
  ).toBeGreaterThan(0)
  expect(fishing.description).toContain('water')
  expect(fishing.manaCost).toBe(0)
})

it('uses a Fishing quickslot to select water without sending a combat ability', () => {
  inventoryStore.set({
    bag: [],
    equipped: { main_hand: testItem('fishing_rod') },
  })
  useAbility(FISHING.id)
  expect(get(fishingTargeting)).toBe(false)
  skillsStore.set({ learned: ['fishing'] })
  manaState.set({ mana: 0, max_mana: 10 })
  assignQuickslot(3, { skill: FISHING.id })
  const entry = get(quickslots)[3]!
  if (!('skill' in entry)) throw new Error('Expected a skill binding')
  useAbility(entry.skill)
  expect(get(fishingTargeting)).toBe(true)
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  useAbility(entry.skill)
  expect(get(fishingTargeting)).toBe(false)
  expect(get(quickslots)[3]).toEqual({ skill: FISHING.id })
  expect(reportSkillFailure).not.toHaveBeenCalled()
})

it('requires a rod and open water, supports rowboats, and blocks a second session', () => {
  skillsStore.set({ learned: ['fishing'] })
  useAbility(FISHING.id)
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    'Equip a fishing rod to use Fishing.'
  )
  expect(get(fishingTargeting)).toBe(false)
  inventoryStore.set({
    bag: [],
    equipped: { main_hand: testItem('fishing_rod') },
  })
  currentDungeonDepth.set(1)
  useAbility(FISHING.id)
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    'You can only fish outdoors.'
  )
  currentDungeonDepth.set(0)
  playerVisualFloorLevel.set(1)
  useAbility(FISHING.id)
  expect(get(fishingTargeting)).toBe(false)
  playerVisualFloorLevel.set(0)
  updatePlayer({ mount: 'horse' })
  useAbility(FISHING.id)
  expect(get(fishingTargeting)).toBe(false)
  updatePlayer({ mount: 'rowboat' })
  useAbility(FISHING.id)
  expect(get(fishingTargeting)).toBe(true)
  resetFishingStore()
  myFishing.set({ phase: 'casting' })
  useAbility(FISHING.id)
  expect(get(fishingTargeting)).toBe(false)
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    'You are already fishing.'
  )
})

it('switches between fishing and inspection targeting and resets on logout', () => {
  skillsStore.set({ learned: ['fishing'] })
  inventoryStore.update((inventory) => ({
    ...inventory,
    equipped: { ...inventory.equipped, main_hand: testItem('fishing_rod') },
  }))
  useAbility('auscultation')
  useAbility(FISHING.id)
  expect(get(inspectionTargeting)).toBe(false)
  expect(get(fishingTargeting)).toBe(true)
  useAbility('auscultation')
  expect(get(fishingTargeting)).toBe(false)
  expect(get(inspectionTargeting)).toBe(true)
  useAbility(FISHING.id)
  resetFishingStore()
  expect(get(fishingTargeting)).toBe(false)
})

it('sends Valhalla Charge at the targeted monster with a one-handed sword or spear', () => {
  updatePlayer({ characterClass: 'valkyrie' })
  const target = vi.mocked(combatController.getAbilityTarget)
  target.mockReturnValue(null)
  for (const [weapon, allowed] of [
    ['great_sword', false],
    ['mace', false],
    ['spear', true],
  ] as const) {
    inventoryStore.set({ bag: [], equipped: { main_hand: testItem(weapon) } })
    useAbility('valhalla_charge')
    if (!allowed)
      expect(reportSkillFailure).toHaveBeenLastCalledWith(
        'Equip a one-handed sword or spear to use Valhalla Charge.'
      )
    expect(get(chargeTargeting)).toBe(allowed)
  }
  useAbility('valhalla_charge')
  expect(get(chargeTargeting)).toBe(false)
  useAbility('valhalla_charge')
  useAbility('auscultation')
  expect(get(chargeTargeting)).toBe(false)
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  target.mockReturnValue('m21')
  useAbility('valhalla_charge')
  expect(get(chargeTargeting)).toBe(false)
  expect(networkManager.sendUseAbility).toHaveBeenCalledExactlyOnceWith(
    'valhalla_charge',
    'm21'
  )
  updatePlayer({ characterClass: 'knight' })
  expect(isAbilityAvailable('valhalla_charge', 'knight')).toBe(false)
  expect(isAbilityAvailable('valhalla_charge', 'valkyrie')).toBe(true)
  const icon = getAbility('valhalla_charge')!.icon
  expect(
    readFileSync(new URL(`../../../public${icon}`, import.meta.url)).length
  ).toBeGreaterThan(0)
})

it('aims Healing Prayer at a hovered party row, player or undead, else yourself', () => {
  updatePlayer({ characterClass: 'priest', health: 10, maxHealth: 10 })
  useAbility('healing_prayer')
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    "The target's HP is already full."
  )
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()

  const cast = (expected: [string | null, number | null]) => {
    resetAbilities()
    useAbility('healing_prayer')
    expect(networkManager.sendUseAbility).toHaveBeenLastCalledWith(
      'healing_prayer',
      ...expected
    )
  }
  updatePlayer({ health: 4 })
  cast([null, null])
  const monsters = monsterManager.monsters as Map<string, unknown>
  monsters.set('m1', { id: 'm1', type: 'goblin', state: 'idle' })
  monsters.set('m2', { id: 'm2', type: 'skeleton', state: 'idle' })
  hoveredMonsterId.set('m1')
  cast([null, null])
  hoveredMonsterId.set('m2')
  cast(['m2', null])

  gameStore.update((state) => ({
    ...state,
    otherPlayers: new Map([
      [7, { id: 7, health: 1, maxHealth: 0 } as RemotePlayer],
    ]),
  }))
  hoveredPlayerId.set(7)
  cast([null, 7])
  partyRoster.set({
    leaderId: 8,
    lootShared: false,
    members: [{ id: 8, name: 'Ann', class: 'knight', hp: 30, max_hp: 30 }],
  })
  hoveredPartyMemberId.set(8)
  vi.mocked(networkManager.sendUseAbility).mockClear()
  resetAbilities()
  useAbility('healing_prayer')
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    "The target's HP is already full."
  )
  expect(networkManager.sendUseAbility).not.toHaveBeenCalled()
  partyRoster.set({
    leaderId: 8,
    lootShared: false,
    members: [{ id: 8, name: 'Ann', class: 'knight', hp: 12, max_hp: 30 }],
  })
  cast([null, 8])
  hoveredPartyMemberId.set(null)
  hoveredPlayerId.set(null)
  hoveredMonsterId.set(null)
  partyRoster.set(null)
  monsters.clear()
  for (const id of ['healing_prayer', 'radiance'])
    expect(
      readFileSync(
        new URL(`../../../public${getAbility(id)!.icon}`, import.meta.url)
      ).length
    ).toBeGreaterThan(0)
})

it('plays Battle Hymn from a bag instrument once there is mana for it', () => {
  updatePlayer({ characterClass: 'bard' })
  inventoryStore.set({ bag: [], equipped: {} })
  useAbility('battle_hymn')
  expect(reportSkillFailure).toHaveBeenLastCalledWith(
    'You need an instrument to play the Battle Hymn.'
  )
  inventoryStore.set({ bag: [testItem('worn_mandolin')], equipped: {} })
  manaState.set({ mana: 4, max_mana: 10 })
  useAbility('battle_hymn')
  expect(reportSkillFailure).toHaveBeenLastCalledWith('Not enough mana.')
  manaState.set({ mana: 5, max_mana: 10 })
  useAbility('battle_hymn')
  expect(networkManager.sendUseAbility).toHaveBeenCalledExactlyOnceWith(
    'battle_hymn',
    null
  )
  expect(
    readFileSync(
      new URL(
        `../../../public${getAbility('battle_hymn')!.icon}`,
        import.meta.url
      )
    ).length
  ).toBeGreaterThan(0)
})

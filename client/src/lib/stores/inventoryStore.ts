import { derived, get, writable } from 'svelte/store'
import type {
  CharacterAttributes,
  ArmorEquipment,
  EquipSlot,
  ItemInstance,
  PlayerInventory,
} from '../network/networkTypes'
import {
  getItemDef,
  givesLight,
  isRangedWeapon,
  type ItemDefinition,
} from '../data/itemDefs'
import { activeDebuffs } from './debuffStore'
import { calendarVisible } from './debugStore'
import { armorWeightMult } from '../data/debuffPresentation'
import { hungerState } from './hungerStore'

export type { EquipSlot, ItemInstance, PlayerInventory }

const initialState: PlayerInventory = {
  bag: [],
  equipped: {},
}

export const inventoryStore = writable<PlayerInventory>({ ...initialState })
export const equippedArmor = derived(
  inventoryStore,
  ({ equipped }): ArmorEquipment => ({
    head: equipped.head?.item_def_id,
    chest: equipped.chest?.item_def_id,
    pants: equipped.pants?.item_def_id,
    boots: equipped.boots?.item_def_id,
    hands: equipped.hands?.item_def_id,
  })
)
export const itemLockMode = writable(false)

/** Units of `itemDefId` in the bag. */
export const bagItemCount = (itemDefId: string) =>
  derived(inventoryStore, (inv) =>
    inv.bag
      .filter((item) => item.item_def_id === itemDefId)
      .reduce((sum, item) => sum + item.quantity, 0)
  )

/** Mirrors the server's equip-target rule (`EquipSlot::alternate`). */
const ALTERNATE_SLOT: Partial<Record<EquipSlot, EquipSlot>> = {
  ring: 'ring_left',
  ring_left: 'ring',
}

/** Where an item of `defId` is worn, checking the def's slot and its alternate. */
export function wornOfDef<T extends Pick<ItemInstance, 'item_def_id'>>(
  defId: string,
  slot: EquipSlot | null | undefined,
  equipped: Partial<Record<EquipSlot, T>>
): { slot: EquipSlot; item: T } | undefined {
  if (!slot) return
  for (const s of [slot, ALTERNATE_SLOT[slot]]) {
    const item = s && equipped[s]
    if (s && item?.item_def_id === defId) return { slot: s, item }
  }
}

/** The equipped item that equipping `def` from the bag would send back to
 *  the bag, or undefined when nothing is displaced (empty slot, free
 *  alternate slot, or `item` is already equipped). */
export function displacedByEquip(
  def: ItemDefinition,
  item?: ItemInstance
): ItemInstance | undefined {
  const slot = def.equipSlot
  if (!slot) return
  const equipped = get(inventoryStore).equipped
  if (
    item &&
    Object.values(equipped).some((e) => e.instance_id === item.instance_id)
  )
    return
  const alt = ALTERNATE_SLOT[slot]
  if (alt && !equipped[alt]) return
  return equipped[slot]
}

/** The local player's gold in the smallest currency unit (copper). */
export const playerGold = writable(0)

/** Server-computed stats, updated on join and equipment changes. */
export type EffectiveStatKey = 'guard' | 'cha' | 'int'
export const playerEffectiveStats = writable<
  | (Pick<CharacterAttributes, EffectiveStatKey> & {
      maxHpBonus: number
      criticalChancePercent: number
    })
  | null
>(null)

/** Item defs that act as a carried light source (mirrors shared TORCH_ITEM_IDS). */
const TORCH_ITEM_IDS = ['torch', 'worn_torch']

/** The bag stack displayed as the character's quiver. */
export function wornAmmoStack(
  inv: Pick<PlayerInventory, 'bag' | 'equipped' | 'active_ammo'>
): ItemInstance | undefined {
  if (!inv.active_ammo) return undefined
  if (!isRangedWeapon(inv.equipped.main_hand?.item_def_id)) return undefined
  return inv.bag.find((item) => item.item_def_id === inv.active_ammo)
}

/** Mirrors the server's ammo gate (`loaded_ammo`): a weapon that fires ammo
 *  needs a round of its kind somewhere in the bag. */
export function outOfAmmo(
  inv: Pick<PlayerInventory, 'bag' | 'equipped'>
): boolean {
  const kind = getItemDef(inv.equipped.main_hand?.item_def_id ?? '')?.ammoKind
  if (!kind) return false
  return !inv.bag.some((item) => {
    const def = getItemDef(item.item_def_id)
    return def?.category === 'ammo' && def.ammoKind === kind
  })
}

export function isTorchItemDefId(id: string | null | undefined): boolean {
  return id != null && TORCH_ITEM_IDS.includes(id)
}

/** True when the local player has a torch equipped in the off-hand slot. */
export const localTorchEquipped = derived(inventoryStore, (inv) => {
  const id = inv.equipped.off_hand?.item_def_id
  return isTorchItemDefId(id)
})

export const localHeadLight = derived(inventoryStore, (inv) =>
  givesLight(inv.equipped.head?.item_def_id)
)

export function carriedItemWeight(
  item: Pick<ItemInstance, 'item_def_id' | 'quantity'>,
  armorMult: number
): number {
  const def = getItemDef(item.item_def_id)
  const mult = def?.category === 'armor' ? armorMult : 1
  return (def?.weight ?? 1) * mult * item.quantity
}

export const liveArmorMult = (debuffs: { id: string; until: number }[]) => {
  const now = Date.now()
  return armorWeightMult(debuffs.filter((d) => d.until > now).map((d) => d.id))
}

/** Mirrors the server's `calc_total_weight`: soaked armour drags, worn and
 *  packed alike (doc/DEBUFF.md). Per-item tooltips keep the dry weight. */
export const carryWeight = derived(
  [inventoryStore, activeDebuffs],
  ([inv, debuffs]) => {
    const armorMult = liveArmorMult(debuffs)
    let total = 0
    for (const item of inv.bag) total += carriedItemWeight(item, armorMult)
    for (const item of Object.values(inv.equipped)) {
      if (item) total += carriedItemWeight(item, armorMult)
    }
    return total
  }
)

/** The selected character's STR, the base of the carry limit. */
export const localStr = writable(10)

/** Mirrors the server's max_carry_weight: STR × 15 scaled by hunger. */
export const carryLimit = derived(
  [localStr, hungerState],
  ([str, hunger]) => str * 15 * (hunger?.carryMult ?? 1)
)

/** Mirrors the server's `pickup_item_with_range`, which takes a partial pile
 *  and refuses only when not even one unit fits. */
export function fitsOneUnit(current: number, max: number, unitWeight: number) {
  return unitWeight <= 0 || Math.floor((max - current) / unitWeight + 1e-3) >= 1
}

export function canCarryOne(itemDefId: string): boolean {
  const unit = carriedItemWeight(
    { item_def_id: itemDefId, quantity: 1 },
    liveArmorMult(get(activeDebuffs))
  )
  return fitsOneUnit(get(carryWeight), get(carryLimit), unit)
}

export const formatKg = (weight: number) => (weight / 10).toFixed(1)

/** The local player's first revive item (phoenix talisman), offered on the
 *  death dialog together with its def. */
export const reviveItem = derived(inventoryStore, (inv) => {
  for (const item of inv.bag) {
    const def = getItemDef(item.item_def_id)
    if (def?.reviveHpPercent != null) return { item, def }
  }
  return null
})

export function setInventory(inventory: PlayerInventory) {
  inventoryStore.set(inventory)
}

export function resetInventoryStore() {
  inventoryStore.set({ bag: [], equipped: {} })
  itemLockMode.set(false)
  playerGold.set(0)
  playerEffectiveStats.set(null)
}

const hasTimekeeper = derived(inventoryStore, (inv) =>
  inv.bag.some(
    (item) => getItemDef(item.item_def_id)?.category === 'timekeeper'
  )
)

/** Hour and date beside the sky widget: debug CAL toggle or a carried timekeeper. */
export const calendarShown = derived(
  [calendarVisible, hasTimekeeper],
  ([v, t]) => v || t
)

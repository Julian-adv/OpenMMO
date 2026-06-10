import { derived, writable } from 'svelte/store'
import type {
  EquipSlot,
  ItemInstance,
  PlayerInventory,
} from '../network/networkTypes'

export type { EquipSlot, ItemInstance, PlayerInventory }

const initialState: PlayerInventory = {
  bag: [],
  equipped: {},
}

export const inventoryStore = writable<PlayerInventory>({ ...initialState })

/** The local player's gold in the smallest currency unit (copper). */
export const playerGold = writable(0)

/** True when the local player has a torch equipped in the off-hand slot. */
export const localTorchEquipped = derived(
  inventoryStore,
  (inv) => inv.equipped.off_hand?.item_def_id === 'torch'
)

export function setInventory(inventory: PlayerInventory) {
  inventoryStore.set(inventory)
}

export function resetInventoryStore() {
  inventoryStore.set({ bag: [], equipped: {} })
  playerGold.set(0)
}

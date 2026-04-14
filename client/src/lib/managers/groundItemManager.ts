import { SvelteMap } from 'svelte/reactivity'
import { hmrSingleton } from '../utils/hmr'
import type { ServerGroundItem } from '../network/networkTypes'

export interface GroundItemData {
  instanceId: number
  itemDefId: string
  position: { x: number; y: number; z: number }
  floorLevel: number
  inHand?: boolean
}

class GroundItemManager {
  items = new SvelteMap<number, GroundItemData>()
  private pickupInProgress = new Set<number>()
  private pendingRemoval = new Set<number>()

  spawn(item: ServerGroundItem) {
    this.items.set(item.instance_id, {
      instanceId: item.instance_id,
      itemDefId: item.item_def_id,
      position: { ...item.position },
      floorLevel: item.floor_level,
    })
  }

  beginPickup(instanceId: number) {
    if (!this.items.has(instanceId)) return
    this.pickupInProgress.add(instanceId)
  }

  setInHand(instanceId: number) {
    const item = this.items.get(instanceId)
    if (!item) return
    this.items.set(instanceId, { ...item, inHand: true })
  }

  finishPickup(instanceId: number) {
    this.pickupInProgress.delete(instanceId)
    if (this.pendingRemoval.has(instanceId)) {
      this.pendingRemoval.delete(instanceId)
      this.items.delete(instanceId)
      return
    }
    // Pickup not confirmed by server (e.g., inventory full) — item returns to ground.
    const item = this.items.get(instanceId)
    if (item?.inHand) {
      this.items.set(instanceId, { ...item, inHand: false })
    }
  }

  remove(instanceId: number) {
    if (this.pickupInProgress.has(instanceId)) {
      this.pendingRemoval.add(instanceId)
      return
    }
    this.items.delete(instanceId)
  }

  reset() {
    this.items.clear()
    this.pickupInProgress.clear()
    this.pendingRemoval.clear()
  }
}

export const groundItemManager = hmrSingleton(
  'groundItemManager',
  () => new GroundItemManager()
)

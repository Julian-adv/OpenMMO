import { SvelteMap } from 'svelte/reactivity'
import { hmrSingleton } from '../utils/hmr'
import type { ServerGroundItem } from '../network/networkTypes'

export interface GroundItemData {
  instanceId: number
  itemDefId: string
  position: { x: number; y: number; z: number }
  floorLevel: number
}

class GroundItemManager {
  items = new SvelteMap<number, GroundItemData>()

  spawn(item: ServerGroundItem) {
    this.items.set(item.instance_id, {
      instanceId: item.instance_id,
      itemDefId: item.item_def_id,
      position: { ...item.position },
      floorLevel: item.floor_level,
    })
  }

  remove(instanceId: number) {
    this.items.delete(instanceId)
  }

  reset() {
    this.items.clear()
  }
}

export const groundItemManager = hmrSingleton(
  'groundItemManager',
  () => new GroundItemManager()
)

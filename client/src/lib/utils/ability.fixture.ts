import { readFileSync } from 'node:fs'
import type { ItemInstance } from '../network/networkTypes'
import { initSync } from '../wasm/onlinerpg_shared'

export function initSharedWasm() {
  initSync({
    module: readFileSync(
      new URL('../wasm/onlinerpg_shared_bg.wasm', import.meta.url)
    ),
  })
}

export const testItem = (item_def_id: string): ItemInstance => ({
  instance_id: 1,
  item_def_id,
  enchant: 0,
  quantity: 1,
})

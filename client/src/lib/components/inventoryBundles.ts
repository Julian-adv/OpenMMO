import type { ItemInstance } from '../network/networkTypes'
import { getItemDef } from '../data/itemDefs'

export type InventoryBundle = 'keys' | 'scrolls'

export interface BundleCell {
  kind: 'bundle'
  bundle: InventoryBundle
  items: ItemInstance[]
}
export type InventoryCell = { kind: 'item'; item: ItemInstance } | BundleCell

export const BUNDLE_ICONS: Record<InventoryBundle, string> = {
  keys: 'objects/key_ring.png',
  scrolls: 'objects/scroll_bundle.png',
}

const BUNDLE_BY_CATEGORY: Record<string, InventoryBundle> = {
  dungeon_key: 'keys',
  return_scroll: 'scrolls',
  teleport_scroll: 'scrolls',
  estate_return_scroll: 'scrolls',
  party_summon_scroll: 'scrolls',
  enchant_scroll: 'scrolls',
  enchant_armor_scroll: 'scrolls',
  house_scroll: 'scrolls',
}

function bundleOf(itemDefId: string): InventoryBundle | undefined {
  return BUNDLE_BY_CATEGORY[getItemDef(itemDefId)?.category ?? '']
}

/** Folds each bundle's items into one cell at its first member's place. */
export function foldBundles(sorted: readonly ItemInstance[]): InventoryCell[] {
  const cells: InventoryCell[] = []
  const bundles = new Map<InventoryBundle, BundleCell>()
  for (const item of sorted) {
    const bundle = bundleOf(item.item_def_id)
    if (!bundle) {
      cells.push({ kind: 'item', item })
      continue
    }
    const cell = bundles.get(bundle)
    if (cell) {
      cell.items.push(item)
    } else {
      const created: BundleCell = { kind: 'bundle', bundle, items: [item] }
      bundles.set(bundle, created)
      cells.push(created)
    }
  }
  return cells.map((cell) =>
    cell.kind === 'bundle' && cell.items.length < 2
      ? { kind: 'item', item: cell.items[0] }
      : cell
  )
}

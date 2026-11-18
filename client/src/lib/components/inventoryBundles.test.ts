import { describe, expect, it } from 'vitest'
import type { ItemInstance } from '../network/networkTypes'
import { foldBundles } from './inventoryBundles'

function item(instanceId: number, itemDefId: string): ItemInstance {
  return {
    instance_id: instanceId,
    item_def_id: itemDefId,
    quantity: 1,
    enchant: 0,
  }
}

describe('foldBundles', () => {
  it('folds the keys into one cell where the first key sat', () => {
    const cells = foldBundles([
      item(1, 'bread'),
      item(2, 'crypt_key_5'),
      item(3, 'scroll_of_return'),
      item(4, 'orc_key_5'),
      item(5, 'orc_key_10'),
    ])
    expect(
      cells.map((cell) =>
        cell.kind === 'bundle'
          ? `[${cell.bundle}:${cell.items.map((i) => i.item_def_id)}]`
          : cell.item.item_def_id
      )
    ).toEqual([
      'bread',
      '[keys:crypt_key_5,orc_key_5,orc_key_10]',
      'scroll_of_return',
    ])
  })

  it('leaves a single kind unfolded', () => {
    expect(foldBundles([item(1, 'crypt_key_5')])).toEqual([
      { kind: 'item', item: item(1, 'crypt_key_5') },
    ])
  })
})

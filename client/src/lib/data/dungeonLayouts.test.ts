import { describe, expect, it } from 'vitest'
import hashes from '../../../../shared/src/dungeon/layout_hashes.fixture.json'
import {
  dungeon_layout_hash,
  dungeon_set_layout_variants,
} from '../wasm/onlinerpg_shared'
import { initSharedWasm } from '../utils/ability.fixture'

/** The wasm generator must build every dungeon exactly as the native server
 *  does (`layouts_match_the_shared_fixture` checks the native side). */
describe('dungeon layouts', () => {
  it('match the native fixture in the wasm build', () => {
    initSharedWasm()
    for (const [id, hash] of Object.entries(hashes)) {
      expect(dungeon_layout_hash(id), id).toBe(hash)
    }
  })

  it('redraw a dungeon when the server names a variant', () => {
    initSharedWasm()
    const fixture = hashes as Record<string, string>
    const set = (variants: [string, number][]) =>
      dungeon_set_layout_variants(
        variants.map(([id]) => id),
        Uint32Array.from(variants, ([, variant]) => variant)
      )
    expect(set([['old_crypt', 7]])).toEqual(['old_crypt'])
    const redrawn = dungeon_layout_hash('old_crypt')
    expect(redrawn).not.toBe(fixture.old_crypt)
    expect(dungeon_layout_hash('orc_warrens')).toBe(fixture.orc_warrens)
    expect(set([['old_crypt', 7]])).toEqual([])
    expect(dungeon_layout_hash('old_crypt')).toBe(redrawn)
    expect(set([])).toEqual(['old_crypt'])
    expect(dungeon_layout_hash('old_crypt')).toBe(fixture.old_crypt)
  })
})

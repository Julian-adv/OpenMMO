import { describe, expect, it } from 'vitest'
import hashes from '../../../../shared/src/dungeon/layout_hashes.fixture.json'
import { dungeon_layout_hash } from '../wasm/onlinerpg_shared'
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
})

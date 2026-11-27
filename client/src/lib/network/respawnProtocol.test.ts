import { beforeAll, expect, it } from 'vitest'
import { initSharedWasm } from '../utils/ability.fixture'
import { serialize_client_message } from '../wasm/onlinerpg_shared'
import type { ClientMessage } from './networkTypes'

beforeAll(initSharedWasm)

it('serializes respawn places and shelter settings with the shared protocol', () => {
  const messages: ClientMessage[] = [
    { RequestRespawn: { place: 'Town' } },
    { RequestRespawn: { place: 'Estate' } },
    { RequestRespawn: { place: 'FamilySeat' } },
    { RequestRespawn: { place: { Nearby: { estate_id: 7 } } } },
    'RequestRespawnOptions',
    { SetShelterAccess: { furniture_id: 3, access: 'Public' } },
  ]
  for (const message of messages) {
    expect(serialize_client_message(message).byteLength).toBeGreaterThan(0)
  }
})

it('rejects the old bare respawn request', () => {
  expect(() => serialize_client_message('RequestRespawn')).toThrow()
})

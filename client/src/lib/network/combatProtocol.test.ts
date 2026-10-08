import { beforeAll, expect, it } from 'vitest'
import { initSharedWasm } from '../utils/ability.fixture'
import { serialize_client_message } from '../wasm/onlinerpg_shared'
import type { ClientMessage } from './networkTypes'

beforeAll(initSharedWasm)

it('serializes attack controls with the current shared protocol', () => {
  const messages: ClientMessage[] = [
    {
      StartPlayerAttack: {
        monster_id: 'm1',
        request_id: 1,
        dagger_skill: false,
      },
    },
    { StopPlayerAttack: { request_id: 1 } },
    { SetPlayerAttackSkill: { request_id: 1, dagger_skill: true } },
  ]
  for (const message of messages) {
    expect(serialize_client_message(message).byteLength).toBeGreaterThan(0)
  }
})

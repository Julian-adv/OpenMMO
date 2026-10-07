import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../wasm/onlinerpg_shared', () => ({
  default: vi.fn(async () => {}),
  serialize_client_message: (message: unknown) =>
    new TextEncoder().encode(JSON.stringify(message)),
  deserialize_server_message: vi.fn(),
  protocol_version: () => 117,
  stamp_layout_version: (version: string) => version,
  close_code_protocol_mismatch: () => 4001,
  close_code_client_desync: () => 4004,
}))
vi.mock('./messageHandlers', () => ({
  handleServerMessage: vi.fn(),
  resetTerrainDownloads: vi.fn(),
}))
vi.mock('../managers/monsterManager', () => ({
  monsterManager: { reset: vi.fn() },
}))
vi.mock('../managers/remotePlayerManager', () => ({
  remotePlayerManager: { reset: vi.fn() },
}))

import { networkManager } from './socket'

class TestWebSocket {
  static OPEN = 1
  static CONNECTING = 0
  static CLOSING = 2
  static CLOSED = 3
  static current: TestWebSocket
  readyState = TestWebSocket.OPEN
  binaryType = ''
  onopen: (() => void) | null = null
  onclose: ((event: { code: number; reason: string }) => void) | null = null
  onerror: (() => void) | null = null
  onmessage: (() => void) | null = null
  send = vi.fn()

  constructor() {
    TestWebSocket.current = this
  }

  close() {
    this.readyState = TestWebSocket.CLOSED
  }
}

const authenticate = () =>
  networkManager.authSuccess.emit({ accountName: 'account', characters: [] })

async function expectBlocked(cancel = false) {
  expect(await networkManager.requestDeleteCharacter(42, cancel)).toEqual({
    ok: false,
    message: 'Authenticate first',
  })
  expect(TestWebSocket.current.send).not.toHaveBeenCalled()
}

describe('character deletion authentication', () => {
  beforeEach(async () => {
    vi.stubGlobal('WebSocket', TestWebSocket)
    await networkManager.ensureWasm()
    networkManager.connect('ws://test')
  })

  afterEach(() => {
    networkManager.disconnect()
    vi.unstubAllGlobals()
  })

  it.each([false, true])(
    'blocks deletion (cancel=%s) until the open socket is authenticated',
    async (cancel) => {
      await expectBlocked(cancel)

      authenticate()
      const result = networkManager.requestDeleteCharacter(42, cancel)
      await Promise.resolve()
      const sent = TestWebSocket.current.send.mock.calls.map(([bytes]) =>
        JSON.parse(new TextDecoder().decode(bytes))
      )
      expect(sent).toEqual([
        { ClientInfo: expect.any(Object) },
        cancel
          ? { CancelCharacterDeletion: { character_id: 42 } }
          : { DeleteCharacter: { character_id: 42 } },
      ])
      networkManager.characterDeletionChanged.emit({
        characterId: 42,
        deletionDueAt: cancel ? null : 12345,
      })
      expect(await result).toEqual({
        ok: true,
        deletionDueAt: cancel ? null : 12345,
      })
    }
  )

  it('requires fresh authentication after a connection closes and reopens', async () => {
    authenticate()
    TestWebSocket.current.close()
    TestWebSocket.current.onclose?.({ code: 1000, reason: '' })
    networkManager.connect()
    await expectBlocked()
  })

  it('clears authentication when the session is cleared', async () => {
    authenticate()
    networkManager.clearSession()
    await expectBlocked(true)
  })
})

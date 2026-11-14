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
import { localTeleportActive } from '../stores/teleportEffectStore'
import { setApiAuthToken } from '../utils/networkUtils'

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

describe('movement during teleport', () => {
  beforeEach(async () => {
    vi.stubGlobal('WebSocket', TestWebSocket)
    await networkManager.ensureWasm()
    networkManager.connect('ws://test')
  })

  afterEach(() => {
    networkManager.disconnect()
    vi.unstubAllGlobals()
  })

  it('drops goals and keyboard movement while allowing stop and fresh movement after arrival', () => {
    const goal = { request_id: 1, x: 10, z: 20, sprinting: false }
    const direction = {
      request_id: 2,
      rotation: 0,
      forward: 1,
      turn: 0,
      sprinting: false,
    }
    localTeleportActive.set(true)
    networkManager.sendMoveGoal(goal, [1, 2])
    networkManager.sendMoveDirection(direction)
    expect(TestWebSocket.current.send).not.toHaveBeenCalled()

    networkManager.sendMoveStop(3)
    localTeleportActive.set(false)
    networkManager.sendMoveGoal(
      { ...goal, request_id: 4, x: 500, z: 600 },
      [490, 590]
    )
    networkManager.sendMoveDirection({ ...direction, request_id: 5 })

    const sent = TestWebSocket.current.send.mock.calls.map(([bytes]) =>
      JSON.parse(new TextDecoder().decode(bytes))
    )
    expect(sent).toEqual([
      { ClientInfo: expect.any(Object) },
      { PlayerMoveStop: { request_id: 3 } },
      {
        PlayerMoveGoal: {
          ...goal,
          request_id: 4,
          x: 500,
          z: 600,
          client_position: [490, 590],
        },
      },
      { PlayerMoveDirection: { ...direction, request_id: 5 } },
    ])
  })
})

describe('automatic rejoin', () => {
  beforeEach(async () => {
    vi.useFakeTimers()
    vi.stubGlobal('WebSocket', TestWebSocket)
    await networkManager.ensureWasm()
    networkManager.connect('ws://test')
    setApiAuthToken('token')
  })

  afterEach(() => {
    networkManager.disconnect()
    networkManager.clearSession()
    vi.unstubAllGlobals()
    vi.useRealTimers()
  })

  const sent = () =>
    TestWebSocket.current.send.mock.calls.map(([bytes]) =>
      new TextDecoder().decode(bytes as Uint8Array)
    )

  it('reports the world ready once the server readmits the character', async () => {
    authenticate()
    const entry = networkManager.requestEnterGame(7)
    await vi.waitFor(() =>
      expect(sent()).toContain(
        JSON.stringify({ EnterGame: { character_id: 7 } })
      )
    )
    networkManager.joinSuccess.emit()
    await entry

    TestWebSocket.current.onclose?.({ code: 1006, reason: '' })
    await vi.runOnlyPendingTimersAsync()
    authenticate()
    await vi.waitFor(() =>
      expect(sent()).toContain(
        JSON.stringify({ EnterGame: { character_id: 7 } })
      )
    )
    expect(sent()).not.toContain(JSON.stringify('WorldReady'))

    networkManager.joinSuccess.emit()
    await vi.waitFor(() =>
      expect(sent()).toContain(JSON.stringify('WorldReady'))
    )
  })

  it('does not rejoin a character whose entry was refused', async () => {
    authenticate()
    const entry = networkManager.requestEnterGame(9)
    await vi.waitFor(() =>
      expect(sent()).toContain(
        JSON.stringify({ EnterGame: { character_id: 9 } })
      )
    )
    networkManager.characterError.emit('paused')
    expect(await entry).toEqual({ ok: false, message: 'paused' })
    TestWebSocket.current.send.mockClear()

    TestWebSocket.current.onclose?.({ code: 1006, reason: '' })
    await vi.runOnlyPendingTimersAsync()
    authenticate()
    await vi.runOnlyPendingTimersAsync()
    expect(sent()).toContainEqual(expect.stringContaining('Authenticate'))
    expect(sent()).not.toContainEqual(expect.stringContaining('EnterGame'))
  })
})

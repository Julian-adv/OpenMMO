import { gameStore, resetGameStore } from '../stores/gameStore'
import { remotePlayerManager } from '../managers/remotePlayerManager'
import { monsterManager } from '../managers/monsterManager'
import { getDefaultServerUrl } from '../utils/networkUtils'
import { simplePasswordHash } from '../utils/authUtils'
import { clearServerGameTime } from '../stores/timeStore'
import initWasm, {
  serialize_client_message,
  deserialize_server_message,
} from '../wasm/onlinerpg_shared'
import { createEvent } from './networkEvents'
import { handleServerMessage } from './messageHandlers'
import type {
  AccountCharacter,
  CharacterClass,
  CharacterRollResult,
  ClientMessage,
  RollCharacterStatsResult,
} from './networkTypes'

export type {
  AccountCharacter,
  CharacterAttributes,
  CharacterClass,
  CharacterRollResult,
  RollCharacterStatsResult,
} from './networkTypes'

class NetworkManager {
  private socket: WebSocket | null = null
  private reconnectAttempts = 0
  private maxReconnectAttempts = 5
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private lastServerUrl: string = ''
  private lastAccountName: string = ''
  private lastPasswordHash: string = ''
  private lastCreateAccount = false
  private lastCharacterId: number | null = null
  private wasmReady = false

  // Events
  readonly respawnRequested = createEvent<() => void>()
  readonly playerRespawned = createEvent<(playerId: string) => void>()
  readonly authSuccess =
    createEvent<
      (payload: { accountName: string; characters: AccountCharacter[] }) => void
    >()
  readonly authError = createEvent<(message: string) => void>()
  readonly joinSuccess = createEvent<() => void>()
  readonly characterCreated =
    createEvent<(character: AccountCharacter) => void>()
  readonly characterStatsRolled =
    createEvent<(result: CharacterRollResult) => void>()
  readonly characterDeleted = createEvent<(characterId: number) => void>()
  readonly characterError = createEvent<(message: string) => void>()
  readonly kicked = createEvent<(reason: string) => void>()

  constructor() {
    this.joinSuccess.on(() => {
      this.lastCreateAccount = false
    })
  }

  private get messageEvents() {
    return {
      authSuccess: this.authSuccess,
      authError: this.authError,
      joinSuccess: this.joinSuccess,
      characterCreated: this.characterCreated,
      characterStatsRolled: this.characterStatsRolled,
      characterDeleted: this.characterDeleted,
      characterError: this.characterError,
      kicked: this.kicked,
      playerRespawned: this.playerRespawned,
    }
  }

  async ensureWasm() {
    if (!this.wasmReady) {
      await initWasm()
      this.wasmReady = true
    }
  }

  connect(serverUrl?: string) {
    if (serverUrl) {
      this.lastServerUrl = serverUrl
    } else if (!this.lastServerUrl) {
      this.lastServerUrl = getDefaultServerUrl()
    }

    const targetUrl = this.lastServerUrl

    if (this.socket?.readyState === WebSocket.OPEN) {
      console.log('Already connected, skipping connection attempt')
      return
    }

    if (this.socket?.readyState === WebSocket.CONNECTING) {
      console.log('Connection in progress, skipping connection attempt')
      return
    }

    console.log('Attempting to connect to:', targetUrl)
    this.socket = new WebSocket(targetUrl)
    this.socket.binaryType = 'arraybuffer'

    this.socket.onopen = () => {
      console.log('Connected to server')
      gameStore.update((state) => ({ ...state, isConnected: true }))
      clearServerGameTime()
      this.reconnectAttempts = 0
      if (this.reconnectTimer) {
        clearTimeout(this.reconnectTimer)
        this.reconnectTimer = null
      }
    }

    this.socket.onclose = (event) => {
      console.log('Disconnected from server', event.code, event.reason)
      gameStore.update((state) => ({ ...state, isConnected: false }))

      if (event.code !== 1000) {
        this.handleReconnect()
      }
    }

    this.socket.onerror = (error) => {
      console.error('WebSocket error:', error)
      this.handleReconnect()
    }

    this.socket.onmessage = (event) => {
      try {
        const bytes = new Uint8Array(event.data as ArrayBuffer)
        const message = deserialize_server_message(bytes)
        handleServerMessage(message, this.messageEvents, () =>
          this.disconnect()
        )
      } catch (error) {
        console.error('Error deserializing server message:', error)
      }
    }
  }

  private handleReconnect() {
    if (
      this.reconnectAttempts < this.maxReconnectAttempts &&
      !this.reconnectTimer
    ) {
      this.reconnectAttempts++
      console.log(
        `Reconnection attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts}`
      )
      this.reconnectTimer = setTimeout(async () => {
        this.reconnectTimer = null
        this.connect()
        if (
          this.lastAccountName &&
          this.lastPasswordHash &&
          this.lastCharacterId
        ) {
          const opened = await this.waitForSocketOpen(5000)
          if (opened) {
            this.authenticateWithHash(
              this.lastAccountName,
              this.lastPasswordHash,
              this.lastCreateAccount
            )
            const unsub = this.authSuccess.on(() => {
              unsub()
              if (this.lastCharacterId) {
                this.sendAndSerialize({
                  EnterGame: { character_id: this.lastCharacterId },
                })
              }
            })
          }
        }
      }, 2000 * this.reconnectAttempts)
    }
  }

  private sendMessage(msg: ClientMessage) {
    if (this.socket?.readyState === WebSocket.OPEN && this.wasmReady) {
      const bytes = serialize_client_message(msg)
      this.socket.send(bytes)
    }
  }

  private isConnected(): boolean {
    return this.socket?.readyState === WebSocket.OPEN && this.wasmReady
  }

  private sendAndSerialize(msg: ClientMessage): boolean {
    if (!this.isConnected()) return false
    const bytes = serialize_client_message(msg)
    this.socket!.send(bytes)
    return true
  }

  private requestWithTimeout<T>(
    timeoutMs: number,
    timeoutMessage: string,
    setup: (
      settle: (result: T) => void,
      onCleanup: (unsub: () => void) => void
    ) => {
      send: () => boolean
      notSentResult: T
    }
  ): Promise<T> {
    return new Promise((resolve) => {
      let settled = false
      const cleanups: (() => void)[] = []

      const settle = (result: T) => {
        if (settled) return
        settled = true
        clearTimeout(timeout)
        cleanups.forEach((fn) => fn())
        resolve(result)
      }

      const onCleanup = (unsub: () => void) => cleanups.push(unsub)

      const timeout = setTimeout(
        () => settle({ ok: false, message: timeoutMessage } as T),
        timeoutMs
      )

      const { send, notSentResult } = setup(settle, onCleanup)

      const sent = send()
      if (!sent && !settled) {
        settle(notSentResult)
      }
    })
  }

  private waitForSocketOpen(timeoutMs: number): Promise<boolean> {
    if (this.socket?.readyState === WebSocket.OPEN) {
      return Promise.resolve(true)
    }

    return new Promise((resolve) => {
      const start = Date.now()
      const interval = setInterval(() => {
        const socket = this.socket
        if (socket?.readyState === WebSocket.OPEN) {
          clearInterval(interval)
          resolve(true)
          return
        }

        const isClosed =
          !socket ||
          socket.readyState === WebSocket.CLOSING ||
          socket.readyState === WebSocket.CLOSED
        if (isClosed || Date.now() - start >= timeoutMs) {
          clearInterval(interval)
          resolve(false)
        }
      }, 50)
    })
  }

  // --- Public send methods ---

  sendPlayerAttack(monsterId: string) {
    this.sendMessage({ PlayerAttack: { monster_id: monsterId } })
  }

  sendMonsterAttack(monsterId: string, targetPlayerId: string) {
    this.sendMessage({
      MonsterAttack: {
        monster_id: monsterId,
        target_player_id: targetPlayerId,
      },
    })
  }

  requestRespawn() {
    if (this.isConnected()) {
      const bytes = serialize_client_message('RequestRespawn')
      this.socket!.send(bytes)
      this.respawnRequested.emit()
    }
  }

  sendPlayerMove(
    position: { x: number; y: number; z: number },
    rotation: number
  ) {
    this.sendMessage({ PlayerMove: { position, rotation } })
  }

  sendMonsterMove(
    monsterId: string,
    position: { x: number; y: number; z: number },
    rotation: number,
    state: string,
    targetPosition: { x: number; y: number; z: number }
  ) {
    this.sendMessage({
      MonsterMove: {
        monster_id: monsterId,
        position,
        rotation,
        state,
        target_position: targetPosition,
      },
    })
  }

  sendChatMessage(message: string) {
    this.sendMessage({ ChatMessage: { message } })
  }

  requestSpawnMonster(
    type: string,
    position: { x: number; y: number; z: number },
    rotation: number
  ) {
    this.sendMessage({
      RequestSpawnMonster: { monster_type: type, position, rotation },
    })
  }

  // --- Auth & character request methods ---

  authenticate(accountName: string, password: string, createAccount: boolean) {
    const passwordHash = simplePasswordHash(password)
    return this.authenticateWithHash(accountName, passwordHash, createAccount)
  }

  private authenticateWithHash(
    accountName: string,
    passwordHash: string,
    createAccount: boolean
  ): boolean {
    this.lastAccountName = accountName
    this.lastPasswordHash = passwordHash
    this.lastCreateAccount = createAccount

    return this.sendAndSerialize({
      Authenticate: {
        account_name: accountName,
        password_hash: passwordHash,
        create_account: createAccount,
      },
    })
  }

  async requestAuthentication(
    serverUrl: string,
    accountName: string,
    password: string,
    createAccount: boolean
  ): Promise<{
    ok: boolean
    message?: string
    accountName?: string
    characters?: AccountCharacter[]
  }> {
    await this.ensureWasm()
    this.connect(serverUrl)
    const opened = await this.waitForSocketOpen(5000)
    if (!opened) {
      return { ok: false, message: 'Failed to connect to server' }
    }

    return this.requestWithTimeout(
      8000,
      'Authentication timed out',
      (settle, onCleanup) => {
        onCleanup(
          this.authSuccess.on((payload) => {
            settle({
              ok: true,
              accountName: payload.accountName,
              characters: payload.characters,
            })
          })
        )
        onCleanup(
          this.authError.on((message) => {
            settle({ ok: false, message })
          })
        )
        return {
          send: () => this.authenticate(accountName, password, createAccount),
          notSentResult: { ok: false, message: 'Socket is not connected' },
        }
      }
    )
  }

  async requestCreateCharacter(
    characterName: string,
    characterClass: CharacterClass
  ): Promise<{ ok: boolean; message?: string; character?: AccountCharacter }> {
    await this.ensureWasm()
    if (!this.isConnected()) {
      return { ok: false, message: 'Socket is not connected' }
    }

    return this.requestWithTimeout(
      8000,
      'Character creation timed out',
      (settle, onCleanup) => {
        onCleanup(
          this.characterCreated.on((character) => {
            settle({ ok: true, character })
          })
        )
        onCleanup(
          this.characterError.on((message) => {
            settle({ ok: false, message })
          })
        )
        onCleanup(
          this.authError.on((message) => {
            settle({ ok: false, message })
          })
        )
        return {
          send: () =>
            this.sendAndSerialize({
              CreateCharacter: {
                character_name: characterName,
                character_class: characterClass,
              },
            }),
          notSentResult: { ok: false, message: 'Socket is not connected' },
        }
      }
    )
  }

  async requestDeleteCharacter(
    characterId: number
  ): Promise<{ ok: boolean; message?: string }> {
    await this.ensureWasm()
    if (!this.isConnected()) {
      return { ok: false, message: 'Socket is not connected' }
    }

    return this.requestWithTimeout(
      8000,
      'Character deletion timed out',
      (settle, onCleanup) => {
        onCleanup(
          this.characterDeleted.on(() => {
            settle({ ok: true })
          })
        )
        onCleanup(
          this.characterError.on((message) => {
            settle({ ok: false, message })
          })
        )
        onCleanup(
          this.authError.on((message) => {
            settle({ ok: false, message })
          })
        )
        return {
          send: () =>
            this.sendAndSerialize({
              DeleteCharacter: { character_id: characterId },
            }),
          notSentResult: { ok: false, message: 'Socket is not connected' },
        }
      }
    )
  }

  async requestRollCharacterStats(): Promise<RollCharacterStatsResult> {
    await this.ensureWasm()
    if (!this.isConnected()) {
      return { ok: false, message: 'Socket is not connected' }
    }

    return this.requestWithTimeout(
      8000,
      'Stat roll timed out',
      (settle, onCleanup) => {
        onCleanup(
          this.characterStatsRolled.on((result) => {
            settle({
              ok: true,
              attributes: result.attributes,
              maxHp: result.maxHp,
            })
          })
        )
        onCleanup(
          this.characterError.on((message) => {
            settle({ ok: false, message })
          })
        )
        onCleanup(
          this.authError.on((message) => {
            settle({ ok: false, message })
          })
        )
        return {
          send: () => this.sendAndSerialize('RollCharacterStats'),
          notSentResult: { ok: false, message: 'Socket is not connected' },
        }
      }
    )
  }

  async requestEnterGame(
    characterId: number
  ): Promise<{ ok: boolean; message?: string }> {
    await this.ensureWasm()
    if (!this.isConnected()) {
      return { ok: false, message: 'Socket is not connected' }
    }

    this.lastCharacterId = characterId
    return this.requestWithTimeout(
      8000,
      'Game entry timed out',
      (settle, onCleanup) => {
        onCleanup(
          this.joinSuccess.on(() => {
            settle({ ok: true })
          })
        )
        onCleanup(
          this.characterError.on((message) => {
            settle({ ok: false, message })
          })
        )
        onCleanup(
          this.authError.on((message) => {
            settle({ ok: false, message })
          })
        )
        return {
          send: () =>
            this.sendAndSerialize({
              EnterGame: { character_id: characterId },
            }),
          notSentResult: { ok: false, message: 'Socket is not connected' },
        }
      }
    )
  }

  // --- Connection management ---

  disconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    clearServerGameTime()
    if (this.socket) {
      this.socket.onopen = null
      this.socket.onclose = null
      this.socket.onerror = null
      this.socket.onmessage = null
      this.socket.close()
      this.socket = null
    }
  }

  private resetAllState() {
    this.disconnect()
    resetGameStore()
    monsterManager.reset()
    remotePlayerManager.reset()
  }

  async reconnect() {
    console.log('Manual reconnection requested. Resetting state...')
    this.resetAllState()

    const serverUrl = this.lastServerUrl
    const accountName = this.lastAccountName
    const passwordHash = this.lastPasswordHash
    const createAccount = this.lastCreateAccount
    const characterId = this.lastCharacterId
    if (!serverUrl || !accountName || !passwordHash || !characterId) {
      console.warn('Reconnect skipped: missing account or character context')
      return
    }

    await this.ensureWasm()
    this.connect(serverUrl)
    const opened = await this.waitForSocketOpen(5000)
    if (!opened) {
      console.warn('Reconnect failed: socket open timeout')
      return
    }

    this.requestWithTimeout<{ ok: boolean; message?: string }>(
      8000,
      'Reconnect auth timed out',
      (settle, onCleanup) => {
        onCleanup(
          this.authSuccess.on(() => {
            settle({ ok: true })
            this.sendAndSerialize({
              EnterGame: { character_id: characterId },
            })
          })
        )
        onCleanup(
          this.authError.on((message) => {
            settle({ ok: false, message })
            console.warn('Reconnect auth failed:', message)
          })
        )
        return {
          send: () =>
            this.authenticateWithHash(accountName, passwordHash, createAccount),
          notSentResult: { ok: false, message: 'Socket is not connected' },
        }
      }
    )
  }

  async requestReauthenticate(): Promise<{
    ok: boolean
    message?: string
    accountName?: string
    characters?: AccountCharacter[]
  }> {
    this.resetAllState()

    const serverUrl = this.lastServerUrl
    const accountName = this.lastAccountName
    const passwordHash = this.lastPasswordHash
    const createAccount = this.lastCreateAccount
    if (!serverUrl || !accountName || !passwordHash) {
      return { ok: false, message: 'Missing account context' }
    }

    await this.ensureWasm()
    this.connect(serverUrl)
    const opened = await this.waitForSocketOpen(5000)
    if (!opened) {
      return { ok: false, message: 'Failed to connect to server' }
    }

    return this.requestWithTimeout(
      8000,
      'Re-authentication timed out',
      (settle, onCleanup) => {
        onCleanup(
          this.authSuccess.on((payload) => {
            settle({
              ok: true,
              accountName: payload.accountName,
              characters: payload.characters,
            })
          })
        )
        onCleanup(
          this.authError.on((message) => {
            settle({ ok: false, message })
          })
        )
        return {
          send: () =>
            this.authenticateWithHash(accountName, passwordHash, createAccount),
          notSentResult: { ok: false, message: 'Socket is not connected' },
        }
      }
    )
  }
}

export const networkManager: NetworkManager =
  import.meta.hot?.data?.networkManager ?? new NetworkManager()

if (import.meta.hot) {
  import.meta.hot.dispose((data) => {
    data.networkManager = networkManager
  })
}

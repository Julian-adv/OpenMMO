import { get } from 'svelte/store'
import {
  gameStore,
  updatePlayer,
  addChatMessage,
  addChatBubble,
  resetGameStore,
} from '../stores/gameStore'
import type { LocalPlayer, RemotePlayer } from '../stores/gameStore'
import { Vector3 } from 'three'
import { remotePlayerManager } from '../managers/remotePlayerManager'
import { monsterManager } from '../managers/monsterManager'
import type { MonsterData } from '../types/Monster'
import { getDefaultServerUrl } from '../utils/networkUtils'
import { requestCameraReset } from '../stores/cameraStore'
import { simplePasswordHash } from '../utils/authUtils'
import { clearServerGameTime, setServerGameTime } from '../stores/timeStore'
import initWasm, {
  serialize_client_message,
  deserialize_server_message,
} from '../wasm/onlinerpg_shared'

type Position = {
  x: number
  y: number
  z: number
}

type ServerPlayer = {
  id: string
  name: string
  position: Position
  rotation: number
  level: number
  health: number
  max_health: number
}

type ServerMonster = {
  id: string
  monster_type: string
  position: Position
  rotation: number
  state: string
  owner_id?: string
  health: number
  max_health: number
}

export type AccountCharacter = {
  id: number
  name: string
  created_at: number
  level: number
  max_hp: number
  attributes: CharacterAttributes
}

export type CharacterAttributes = {
  str: number
  dex: number
  con: number
  int: number
  wis: number
  cha: number
}

export type CharacterRollResult = {
  attributes: CharacterAttributes
  maxHp: number
}

export type RollCharacterStatsResult =
  | {
      ok: true
      attributes: CharacterAttributes
      maxHp: number
    }
  | {
      ok: false
      message: string
    }

// Serde externally tagged enum shapes
type ClientMessage =
  | {
      Authenticate: {
        account_name: string
        password_hash: string
        create_account: boolean
      }
    }
  | { CreateCharacter: { character_name: string } }
  | { DeleteCharacter: { character_id: number } }
  | 'RollCharacterStats'
  | { EnterGame: { character_id: number } }
  | { PlayerMove: { position: Position; rotation: number } }
  | { ChatMessage: { message: string } }
  | {
      RequestSpawnMonster: {
        monster_type: string
        position: Position
        rotation: number
      }
    }
  | {
      MonsterMove: {
        monster_id: string
        position: Position
        rotation: number
        state: string
        target_position: Position
      }
    }
  | { PlayerAttack: { monster_id: string } }
  | { MonsterAttack: { monster_id: string; target_player_id: string } }
  | 'RequestRespawn'

type RespawnRequestedHandler = () => void
type PlayerRespawnedHandler = (playerId: string) => void
type AuthSuccessPayload = {
  accountName: string
  characters: AccountCharacter[]
}
type AuthSuccessHandler = (payload: AuthSuccessPayload) => void
type AuthErrorHandler = (message: string) => void
type JoinSuccessHandler = () => void
type CharacterCreatedHandler = (character: AccountCharacter) => void
type CharacterStatsRolledHandler = (result: CharacterRollResult) => void
type CharacterDeletedHandler = (characterId: number) => void
type CharacterErrorHandler = (message: string) => void
type KickedHandler = (reason: string) => void

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
  private respawnRequestedHandlers = new Set<RespawnRequestedHandler>()
  private playerRespawnedHandlers = new Set<PlayerRespawnedHandler>()
  private authSuccessHandlers = new Set<AuthSuccessHandler>()
  private authErrorHandlers = new Set<AuthErrorHandler>()
  private joinSuccessHandlers = new Set<JoinSuccessHandler>()
  private characterCreatedHandlers = new Set<CharacterCreatedHandler>()
  private characterStatsRolledHandlers = new Set<CharacterStatsRolledHandler>()
  private characterDeletedHandlers = new Set<CharacterDeletedHandler>()
  private characterErrorHandlers = new Set<CharacterErrorHandler>()
  private kickedHandlers = new Set<KickedHandler>()

  async ensureWasm() {
    if (!this.wasmReady) {
      await initWasm()
      this.wasmReady = true
    }
  }

  onRespawnRequested(handler: RespawnRequestedHandler) {
    this.respawnRequestedHandlers.add(handler)
    return () => this.respawnRequestedHandlers.delete(handler)
  }

  onPlayerRespawned(handler: PlayerRespawnedHandler) {
    this.playerRespawnedHandlers.add(handler)
    return () => this.playerRespawnedHandlers.delete(handler)
  }

  onAuthSuccess(handler: AuthSuccessHandler) {
    this.authSuccessHandlers.add(handler)
    return () => this.authSuccessHandlers.delete(handler)
  }

  onAuthError(handler: AuthErrorHandler) {
    this.authErrorHandlers.add(handler)
    return () => this.authErrorHandlers.delete(handler)
  }

  onJoinSuccess(handler: JoinSuccessHandler) {
    this.joinSuccessHandlers.add(handler)
    return () => this.joinSuccessHandlers.delete(handler)
  }

  onCharacterCreated(handler: CharacterCreatedHandler) {
    this.characterCreatedHandlers.add(handler)
    return () => this.characterCreatedHandlers.delete(handler)
  }

  onCharacterStatsRolled(handler: CharacterStatsRolledHandler) {
    this.characterStatsRolledHandlers.add(handler)
    return () => this.characterStatsRolledHandlers.delete(handler)
  }

  onCharacterDeleted(handler: CharacterDeletedHandler) {
    this.characterDeletedHandlers.add(handler)
    return () => this.characterDeletedHandlers.delete(handler)
  }

  onCharacterError(handler: CharacterErrorHandler) {
    this.characterErrorHandlers.add(handler)
    return () => this.characterErrorHandlers.delete(handler)
  }

  onKicked(handler: KickedHandler) {
    this.kickedHandlers.add(handler)
    return () => this.kickedHandlers.delete(handler)
  }

  private emitRespawnRequested() {
    this.respawnRequestedHandlers.forEach((handler) => handler())
  }

  private emitPlayerRespawned(playerId: string) {
    this.playerRespawnedHandlers.forEach((handler) => handler(playerId))
  }

  private emitAuthSuccess(payload: AuthSuccessPayload) {
    this.authSuccessHandlers.forEach((handler) => handler(payload))
  }

  private emitAuthError(message: string) {
    this.authErrorHandlers.forEach((handler) => handler(message))
  }

  private emitJoinSuccess() {
    this.joinSuccessHandlers.forEach((handler) => handler())
  }

  private emitCharacterCreated(character: AccountCharacter) {
    this.characterCreatedHandlers.forEach((handler) => handler(character))
  }

  private emitCharacterStatsRolled(result: CharacterRollResult) {
    this.characterStatsRolledHandlers.forEach((handler) => handler(result))
  }

  private emitCharacterDeleted(characterId: number) {
    this.characterDeletedHandlers.forEach((handler) => handler(characterId))
  }

  private emitCharacterError(message: string) {
    this.characterErrorHandlers.forEach((handler) => handler(message))
  }

  private emitKicked(reason: string) {
    this.kickedHandlers.forEach((handler) => handler(reason))
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

      // Only reconnect if it wasn't a manual disconnect
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
        this.handleServerMessage(message)
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
      this.reconnectTimer = setTimeout(() => {
        this.reconnectTimer = null
        this.connect()
      }, 2000 * this.reconnectAttempts)
    }
  }

  private sendMessage(msg: ClientMessage) {
    if (this.socket?.readyState === WebSocket.OPEN && this.wasmReady) {
      const bytes = serialize_client_message(msg)
      this.socket.send(bytes)
    }
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private handleServerMessage(raw: any) {
    // Serde externally tagged: unit variants are strings,
    // struct variants are { VariantName: { ...data } }
    if (typeof raw === 'string') {
      return
    }

    const type = Object.keys(raw)[0]
    const data = raw[type]

    switch (type) {
      case 'AuthSuccess': {
        const characters = (data.characters as AccountCharacter[]) ?? []
        this.emitAuthSuccess({
          accountName: data.account_name,
          characters,
        })
        break
      }

      case 'AuthError': {
        console.warn('Authentication error:', data.message)
        this.emitAuthError(data.message)
        break
      }

      case 'JoinSuccess': {
        const serverPlayer: ServerPlayer = data.player
        console.log('Join successful, received player data:', serverPlayer)
        this.lastCreateAccount = false
        const playerPosition = new Vector3(
          serverPlayer.position.x,
          serverPlayer.position.y,
          serverPlayer.position.z
        )
        const player: LocalPlayer = {
          ...serverPlayer,
          position: playerPosition,
          maxHealth: serverPlayer.max_health,
        }
        gameStore.update((state) => ({
          ...state,
          currentPlayer: player,
        }))
        this.emitJoinSuccess()
        break
      }

      case 'CharacterCreated': {
        const character: AccountCharacter = data.character
        this.emitCharacterCreated(character)
        break
      }

      case 'CharacterStatsRolled': {
        const attributes: CharacterAttributes = data.attributes
        this.emitCharacterStatsRolled({
          attributes,
          maxHp: data.max_hp,
        })
        break
      }

      case 'CharacterDeleted': {
        this.emitCharacterDeleted(data.character_id)
        break
      }

      case 'CharacterError': {
        this.emitCharacterError(data.message)
        break
      }

      case 'PlayerJoined': {
        const serverPlayer: ServerPlayer = data.player
        const playerPosition = new Vector3(
          serverPlayer.position.x,
          serverPlayer.position.y,
          serverPlayer.position.z
        )
        const player: LocalPlayer = {
          ...serverPlayer,
          position: playerPosition,
          maxHealth: serverPlayer.max_health,
        }
        const remotePlayer: RemotePlayer = {
          id: serverPlayer.id,
          name: serverPlayer.name,
          level: serverPlayer.level,
          health: serverPlayer.health,
          maxHealth: serverPlayer.max_health,
        }
        gameStore.update((state) => {
          if (!state.currentPlayer) {
            console.log('Setting current player from PlayerJoined:', player)
            return { ...state, currentPlayer: player }
          } else if (serverPlayer.id !== state.currentPlayer.id) {
            remotePlayerManager.initPlayer(
              serverPlayer.id,
              serverPlayer.position,
              serverPlayer.rotation
            )
            state.otherPlayers.set(serverPlayer.id, remotePlayer)
            addChatMessage(`${serverPlayer.name} joined the game`)
          }
          return state
        })
        break
      }

      case 'PlayerLeft':
        gameStore.update((state) => {
          const player = state.otherPlayers.get(data.player_id)
          remotePlayerManager.removePlayer(data.player_id)
          if (player) {
            state.otherPlayers.delete(data.player_id)
            addChatMessage(`${player.name} left the game`)
          }
          return state
        })
        break

      case 'PlayerMoved': {
        const state = get(gameStore)
        if (state.currentPlayer?.id === data.player_id) {
          break
        }
        remotePlayerManager.setTargetPosition(data.player_id, {
          x: data.position.x,
          y: data.position.y,
          z: data.position.z,
        })
        break
      }

      case 'ChatMessage': {
        const state = get(gameStore)
        const playerName =
          state.currentPlayer?.id === data.player_id
            ? state.currentPlayer?.name
            : (state.otherPlayers.get(data.player_id)?.name ?? 'Unknown')
        addChatMessage(`${playerName}: ${data.message}`)
        addChatBubble(data.player_id, data.message)
        break
      }

      case 'GameState':
        gameStore.update((state) => {
          state.otherPlayers.clear()
          remotePlayerManager.reset()
          Object.values(data.players as Record<string, ServerPlayer>).forEach(
            (serverPlayer) => {
              if (serverPlayer.id !== state.currentPlayer?.id) {
                const player: RemotePlayer = {
                  id: serverPlayer.id,
                  name: serverPlayer.name,
                  level: serverPlayer.level,
                  health: serverPlayer.health,
                  maxHealth: serverPlayer.max_health,
                }
                remotePlayerManager.initPlayer(
                  serverPlayer.id,
                  serverPlayer.position,
                  serverPlayer.rotation
                )
                state.otherPlayers.set(serverPlayer.id, player)
              }
            }
          )
          return state
        })

        monsterManager.reset()
        if (data.monsters) {
          Object.values(data.monsters as Record<string, ServerMonster>).forEach(
            (monster) => {
              monsterManager.spawnWithId(
                monster.id,
                monster.monster_type as MonsterData['type'],
                monster.position,
                monster.owner_id,
                monster.health,
                monster.max_health
              )
            }
          )
        }
        break

      case 'GameTimeSync': {
        setServerGameTime({
          year: data.datetime.year,
          month: data.datetime.month,
          day: data.datetime.day,
          hour: data.datetime.hour,
          minute: data.datetime.minute,
          isNight: data.is_night,
        })
        break
      }

      case 'MonsterSpawned': {
        const monster: ServerMonster = data.monster
        monsterManager.spawnWithId(
          monster.id,
          monster.monster_type as MonsterData['type'],
          monster.position,
          monster.owner_id,
          monster.health,
          monster.max_health
        )
        break
      }

      case 'MonsterMoved':
        monsterManager.updateMonsterFromNetwork(
          data.monster_id,
          data.position,
          data.rotation,
          data.state,
          data.target_position
        )
        break

      case 'MonsterRemoved':
        monsterManager.remove(data.monster_id)
        break

      case 'MonsterDead':
        monsterManager.handleMonsterDead(data.monster_id)
        break

      case 'PlayerAttacked': {
        remotePlayerManager.handleAttack(data.player_id)

        const gameState = get(gameStore)
        const attackerName =
          gameState.currentPlayer?.id === data.player_id
            ? 'You'
            : gameState.otherPlayers.get(data.player_id)?.name || 'Unknown'

        if (data.hit) {
          addChatMessage(
            `${attackerName} rolled ${data.roll}: HIT for ${data.damage} damage!`
          )
        } else {
          addChatMessage(`${attackerName} rolled ${data.roll}: MISSED!`)
        }

        monsterManager.handleMonsterAttacked(
          data.monster_id,
          data.player_id,
          data.hit,
          data.damage
        )
        break
      }

      case 'MonsterAttackedPlayer': {
        const gameState2 = get(gameStore)
        const isCurrentPlayer = gameState2.currentPlayer?.id === data.player_id

        let damageInfo = undefined
        if (isCurrentPlayer) {
          const prevTrigger =
            gameState2.currentPlayer?.lastDamageInfo?.trigger ?? 0
          damageInfo = {
            damage: data.damage,
            hit: data.hit,
            trigger: prevTrigger + 1,
          }
        }

        if (data.hit) {
          updatePlayer(data.player_id, {
            health: isCurrentPlayer
              ? Math.max(
                  0,
                  (gameState2.currentPlayer?.health ?? 0) - data.damage
                )
              : Math.max(
                  0,
                  (gameState2.otherPlayers.get(data.player_id)?.health ?? 0) -
                    data.damage
                ),
            ...(isCurrentPlayer ? { lastDamageInfo: damageInfo } : {}),
          })

          const targetName = isCurrentPlayer
            ? 'You'
            : (gameState2.otherPlayers.get(data.player_id)?.name ?? 'Unknown')
          addChatMessage(
            `Monster rolled ${data.roll}: HIT ${targetName} for ${data.damage} damage!`
          )
        } else {
          if (isCurrentPlayer) {
            updatePlayer(data.player_id, {
              lastDamageInfo: damageInfo,
            })
          }
          addChatMessage(`Monster rolled ${data.roll}: MISSED!`)
        }
        break
      }

      case 'PlayerDead': {
        console.log('Player dead:', data.player_id)
        const gameState3 = get(gameStore)
        const isDeadCurrentPlayer =
          gameState3.currentPlayer?.id === data.player_id
        const deadPlayerName = isDeadCurrentPlayer
          ? 'You'
          : (gameState3.otherPlayers.get(data.player_id)?.name ?? 'Unknown')
        addChatMessage(
          `${deadPlayerName === 'You' ? 'You have' : deadPlayerName + ' has'} been slain!`
        )

        if (!isDeadCurrentPlayer) {
          remotePlayerManager.handleDead(data.player_id)
        }
        break
      }

      case 'Kicked': {
        console.warn('Kicked from server:', data.reason)
        this.emitKicked(data.reason)
        resetGameStore()
        monsterManager.reset()
        remotePlayerManager.reset()
        this.disconnect()
        break
      }

      case 'PlayerRespawned': {
        const serverPlayer: ServerPlayer = data.player
        console.log('Player respawned:', serverPlayer.id)
        const gameState4 = get(gameStore)
        const isCurrentPlayerRespawned =
          gameState4.currentPlayer?.id === serverPlayer.id

        if (isCurrentPlayerRespawned) {
          const respawnPosition = new Vector3(
            serverPlayer.position.x,
            serverPlayer.position.y,
            serverPlayer.position.z
          )
          updatePlayer(serverPlayer.id, {
            position: respawnPosition,
            health: serverPlayer.health,
            maxHealth: serverPlayer.max_health,
          })
          requestCameraReset()
          addChatMessage('You have respawned.')
        } else {
          updatePlayer(serverPlayer.id, {
            health: serverPlayer.health,
            maxHealth: serverPlayer.max_health,
          })
          addChatMessage(`${serverPlayer.name} has respawned.`)
          remotePlayerManager.handleRespawn(
            serverPlayer.id,
            serverPlayer.position,
            serverPlayer.rotation
          )
        }
        this.emitPlayerRespawned(serverPlayer.id)
        break
      }
    }
  }

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
    if (this.socket?.readyState === WebSocket.OPEN && this.wasmReady) {
      const bytes = serialize_client_message('RequestRespawn')
      this.socket.send(bytes)
      this.emitRespawnRequested()
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

  authenticate(accountName: string, password: string, createAccount: boolean) {
    const passwordHash = simplePasswordHash(password)
    return this.authenticateWithHash(accountName, passwordHash, createAccount)
  }

  private authenticateWithHash(
    accountName: string,
    passwordHash: string,
    createAccount: boolean
  ) {
    this.lastAccountName = accountName
    this.lastPasswordHash = passwordHash
    this.lastCreateAccount = createAccount

    if (this.socket?.readyState === WebSocket.OPEN && this.wasmReady) {
      console.log('Sending authentication request for account:', accountName)
      const msg: ClientMessage = {
        Authenticate: {
          account_name: accountName,
          password_hash: passwordHash,
          create_account: createAccount,
        },
      }
      const bytes = serialize_client_message(msg)
      this.socket.send(bytes)
      return true
    }

    return false
  }

  private sendCreateCharacter(characterName: string) {
    if (this.socket?.readyState === WebSocket.OPEN && this.wasmReady) {
      const msg: ClientMessage = {
        CreateCharacter: {
          character_name: characterName,
        },
      }
      const bytes = serialize_client_message(msg)
      this.socket.send(bytes)
      return true
    }

    return false
  }

  private sendDeleteCharacter(characterId: number) {
    if (this.socket?.readyState === WebSocket.OPEN && this.wasmReady) {
      const msg: ClientMessage = {
        DeleteCharacter: {
          character_id: characterId,
        },
      }
      const bytes = serialize_client_message(msg)
      this.socket.send(bytes)
      return true
    }

    return false
  }

  private sendRollCharacterStats() {
    if (this.socket?.readyState === WebSocket.OPEN && this.wasmReady) {
      const bytes = serialize_client_message('RollCharacterStats')
      this.socket.send(bytes)
      return true
    }

    return false
  }

  private sendEnterGame(characterId: number) {
    if (this.socket?.readyState === WebSocket.OPEN && this.wasmReady) {
      this.lastCharacterId = characterId
      const msg: ClientMessage = {
        EnterGame: {
          character_id: characterId,
        },
      }
      const bytes = serialize_client_message(msg)
      this.socket.send(bytes)
      return true
    }

    return false
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

    return new Promise((resolve) => {
      let settled = false
      let timeout: ReturnType<typeof setTimeout> | null = null
      let offSuccess: () => void = () => {}
      let offError: () => void = () => {}

      const cleanup = () => {
        if (timeout) {
          clearTimeout(timeout)
          timeout = null
        }
        offSuccess()
        offError()
      }

      timeout = setTimeout(() => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Authentication timed out' })
      }, 8000)

      offSuccess = this.onAuthSuccess((payload) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({
          ok: true,
          accountName: payload.accountName,
          characters: payload.characters,
        })
      })

      offError = this.onAuthError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      const sent = this.authenticate(accountName, password, createAccount)
      if (!sent && !settled) {
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Socket is not connected' })
      }
    })
  }

  async requestCreateCharacter(
    characterName: string
  ): Promise<{ ok: boolean; message?: string; character?: AccountCharacter }> {
    await this.ensureWasm()
    if (this.socket?.readyState !== WebSocket.OPEN) {
      return { ok: false, message: 'Socket is not connected' }
    }

    return new Promise((resolve) => {
      let settled = false
      let timeout: ReturnType<typeof setTimeout> | null = null
      let offCreated: () => void = () => {}
      let offCharacterError: () => void = () => {}
      let offAuthError: () => void = () => {}

      const cleanup = () => {
        if (timeout) {
          clearTimeout(timeout)
          timeout = null
        }
        offCreated()
        offCharacterError()
        offAuthError()
      }

      timeout = setTimeout(() => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Character creation timed out' })
      }, 8000)

      offCreated = this.onCharacterCreated((character) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: true, character })
      })

      offCharacterError = this.onCharacterError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      offAuthError = this.onAuthError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      const sent = this.sendCreateCharacter(characterName)
      if (!sent && !settled) {
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Socket is not connected' })
      }
    })
  }

  async requestDeleteCharacter(
    characterId: number
  ): Promise<{ ok: boolean; message?: string }> {
    await this.ensureWasm()
    if (this.socket?.readyState !== WebSocket.OPEN) {
      return { ok: false, message: 'Socket is not connected' }
    }

    return new Promise((resolve) => {
      let settled = false
      let timeout: ReturnType<typeof setTimeout> | null = null
      let offDeleted: () => void = () => {}
      let offCharacterError: () => void = () => {}
      let offAuthError: () => void = () => {}

      const cleanup = () => {
        if (timeout) {
          clearTimeout(timeout)
          timeout = null
        }
        offDeleted()
        offCharacterError()
        offAuthError()
      }

      timeout = setTimeout(() => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Character deletion timed out' })
      }, 8000)

      offDeleted = this.onCharacterDeleted(() => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: true })
      })

      offCharacterError = this.onCharacterError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      offAuthError = this.onAuthError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      const sent = this.sendDeleteCharacter(characterId)
      if (!sent && !settled) {
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Socket is not connected' })
      }
    })
  }

  async requestRollCharacterStats(): Promise<RollCharacterStatsResult> {
    await this.ensureWasm()
    if (this.socket?.readyState !== WebSocket.OPEN) {
      return { ok: false, message: 'Socket is not connected' }
    }

    return new Promise((resolve) => {
      let settled = false
      let timeout: ReturnType<typeof setTimeout> | null = null
      let offRolled: () => void = () => {}
      let offCharacterError: () => void = () => {}
      let offAuthError: () => void = () => {}

      const cleanup = () => {
        if (timeout) {
          clearTimeout(timeout)
          timeout = null
        }
        offRolled()
        offCharacterError()
        offAuthError()
      }

      timeout = setTimeout(() => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Stat roll timed out' })
      }, 8000)

      offRolled = this.onCharacterStatsRolled((result) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({
          ok: true,
          attributes: result.attributes,
          maxHp: result.maxHp,
        })
      })

      offCharacterError = this.onCharacterError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      offAuthError = this.onAuthError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      const sent = this.sendRollCharacterStats()
      if (!sent && !settled) {
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Socket is not connected' })
      }
    })
  }

  async requestEnterGame(
    characterId: number
  ): Promise<{ ok: boolean; message?: string }> {
    await this.ensureWasm()
    if (this.socket?.readyState !== WebSocket.OPEN) {
      return { ok: false, message: 'Socket is not connected' }
    }

    return new Promise((resolve) => {
      let settled = false
      let timeout: ReturnType<typeof setTimeout> | null = null
      let offJoin: () => void = () => {}
      let offCharacterError: () => void = () => {}
      let offAuthError: () => void = () => {}

      const cleanup = () => {
        if (timeout) {
          clearTimeout(timeout)
          timeout = null
        }
        offJoin()
        offCharacterError()
        offAuthError()
      }

      timeout = setTimeout(() => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Game entry timed out' })
      }, 8000)

      offJoin = this.onJoinSuccess(() => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: true })
      })

      offCharacterError = this.onCharacterError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      offAuthError = this.onAuthError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      const sent = this.sendEnterGame(characterId)
      if (!sent && !settled) {
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Socket is not connected' })
      }
    })
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

  disconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    clearServerGameTime()
    if (this.socket) {
      this.socket.close()
      this.socket = null
    }
  }

  async reconnect() {
    console.log('Manual reconnection requested. Resetting state...')
    this.disconnect()
    resetGameStore()
    monsterManager.reset()
    remotePlayerManager.reset()

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

    let settled = false
    let timeout: ReturnType<typeof setTimeout> | null = null
    let offSuccess: () => void = () => {}
    let offError: () => void = () => {}

    const cleanup = () => {
      if (timeout) {
        clearTimeout(timeout)
        timeout = null
      }
      offSuccess()
      offError()
    }

    timeout = setTimeout(() => {
      if (settled) return
      settled = true
      cleanup()
      console.warn('Reconnect auth timed out')
    }, 8000)

    offSuccess = this.onAuthSuccess(() => {
      if (settled) return
      settled = true
      cleanup()
      this.sendEnterGame(characterId)
    })

    offError = this.onAuthError((message) => {
      if (settled) return
      settled = true
      cleanup()
      console.warn('Reconnect auth failed:', message)
    })

    const sent = this.authenticateWithHash(
      accountName,
      passwordHash,
      createAccount
    )
    if (!sent && !settled) {
      settled = true
      cleanup()
      console.warn('Reconnect failed: socket is not connected')
    }
  }

  async requestReauthenticate(): Promise<{
    ok: boolean
    message?: string
    accountName?: string
    characters?: AccountCharacter[]
  }> {
    this.disconnect()
    resetGameStore()
    monsterManager.reset()
    remotePlayerManager.reset()

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

    return new Promise((resolve) => {
      let settled = false
      let timeout: ReturnType<typeof setTimeout> | null = null
      let offSuccess: () => void = () => {}
      let offError: () => void = () => {}

      const cleanup = () => {
        if (timeout) {
          clearTimeout(timeout)
          timeout = null
        }
        offSuccess()
        offError()
      }

      timeout = setTimeout(() => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Re-authentication timed out' })
      }, 8000)

      offSuccess = this.onAuthSuccess((payload) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({
          ok: true,
          accountName: payload.accountName,
          characters: payload.characters,
        })
      })

      offError = this.onAuthError((message) => {
        if (settled) return
        settled = true
        cleanup()
        resolve({ ok: false, message })
      })

      const sent = this.authenticateWithHash(
        accountName,
        passwordHash,
        createAccount
      )
      if (!sent && !settled) {
        settled = true
        cleanup()
        resolve({ ok: false, message: 'Socket is not connected' })
      }
    })
  }
}

export const networkManager = new NetworkManager()

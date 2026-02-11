import { get } from 'svelte/store'
import {
  gameStore,
  updatePlayer,
  addChatMessage,
  addChatBubble,
  resetGameStore,
} from '../stores/gameStore'
import type { Player } from '../stores/gameStore'
import { Vector3 } from 'three'
import { remotePlayerManager } from '../managers/remotePlayerManager'
import { monsterManager } from '../managers/monsterManager'
import type { MonsterData } from '../types/Monster'
import { getDefaultServerUrl } from '../utils/networkUtils'
import { requestCameraReset } from '../stores/cameraStore'

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

type ClientMessage =
  | { type: 'join'; player_name: string }
  | { type: 'player_move'; position: Position; rotation: number }
  | { type: 'chat_message'; message: string }
  | {
      type: 'request_spawn_monster'
      monster_type: string
      position: Position
      rotation: number
    }
  | {
      type: 'monster_move'
      monster_id: string
      position: Position
      rotation: number
      state: string
      target_position: Position
    }
  | { type: 'player_attack'; monster_id: string }
  | { type: 'monster_attack'; monster_id: string; target_player_id: string }
  | { type: 'request_respawn' }

type ServerMessage =
  | { type: 'player_joined'; player: ServerPlayer }
  | { type: 'player_left'; player_id: string }
  | {
      type: 'player_moved'
      player_id: string
      position: Position
      rotation: number
    }
  | {
      type: 'player_attacked'
      player_id: string
      monster_id: string
      hit: boolean
      roll: number
      damage: number
    }
  | { type: 'chat_message'; player_id: string; message: string }
  | {
      type: 'game_state'
      players: Record<string, ServerPlayer>
      monsters?: Record<string, ServerMonster>
    }
  | { type: 'join_success'; player: ServerPlayer }
  | { type: 'monster_spawned'; monster: ServerMonster }
  | {
      type: 'monster_moved'
      monster_id: string
      position: Position
      rotation: number
      state: string
      target_position: Position
      owner_id?: string
    }
  | { type: 'monster_removed'; monster_id: string }
  | { type: 'monster_dead'; monster_id: string }
  | {
      type: 'monster_attacked_player'
      monster_id: string
      player_id: string
      hit: boolean
      roll: number
      damage: number
    }
  | { type: 'player_dead'; player_id: string }
  | { type: 'player_respawned'; player: ServerPlayer }

type RespawnRequestedHandler = () => void
type PlayerRespawnedHandler = (playerId: string) => void

class NetworkManager {
  private socket: WebSocket | null = null
  private reconnectAttempts = 0
  private maxReconnectAttempts = 5
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private lastServerUrl: string = ''
  private lastPlayerName: string = ''
  private respawnRequestedHandlers = new Set<RespawnRequestedHandler>()
  private playerRespawnedHandlers = new Set<PlayerRespawnedHandler>()

  onRespawnRequested(handler: RespawnRequestedHandler) {
    this.respawnRequestedHandlers.add(handler)
    return () => this.respawnRequestedHandlers.delete(handler)
  }

  onPlayerRespawned(handler: PlayerRespawnedHandler) {
    this.playerRespawnedHandlers.add(handler)
    return () => this.playerRespawnedHandlers.delete(handler)
  }

  private emitRespawnRequested() {
    this.respawnRequestedHandlers.forEach((handler) => handler())
  }

  private emitPlayerRespawned(playerId: string) {
    this.playerRespawnedHandlers.forEach((handler) => handler(playerId))
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

    this.socket.onopen = () => {
      console.log('Connected to server')
      gameStore.update((state) => ({ ...state, isConnected: true }))
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
        const message: ServerMessage = JSON.parse(event.data)
        this.handleServerMessage(message)
      } catch (error) {
        console.error('Error parsing server message:', error)
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

  private handleServerMessage(message: ServerMessage) {
    switch (message.type) {
      case 'join_success': {
        console.log('Join successful, received player data:', message.player)
        const playerPosition = new Vector3(
          message.player.position.x,
          message.player.position.y,
          message.player.position.z
        )
        const player: Player = {
          ...message.player,
          position: playerPosition,
          maxHealth: message.player.max_health,
        }
        gameStore.update((state) => ({
          ...state,
          currentPlayer: player,
        }))
        break
      }

      case 'player_joined': {
        const playerPosition = new Vector3(
          message.player.position.x,
          message.player.position.y,
          message.player.position.z
        )
        const player: Player = {
          ...message.player,
          position: playerPosition,
          targetPosition: playerPosition.clone(),
          maxHealth: message.player.max_health,
        }
        gameStore.update((state) => {
          // If we don't have a current player yet, this might be us
          if (!state.currentPlayer) {
            console.log('Setting current player from player_joined:', player)
            return { ...state, currentPlayer: player }
          } else if (message.player.id !== state.currentPlayer.id) {
            // This is another player - initialize with rotation
            remotePlayerManager.initPlayer(
              message.player.id,
              message.player.position,
              message.player.rotation
            )
            state.otherPlayers.set(message.player.id, player)
            addChatMessage(`${message.player.name} joined the game`)
          }
          return state
        })
        break
      }

      case 'player_left':
        gameStore.update((state) => {
          const player = state.otherPlayers.get(message.player_id)
          if (player) {
            state.otherPlayers.delete(message.player_id)
            remotePlayerManager.removePlayer(message.player_id) // Add cleanup here
            addChatMessage(`${player.name} left the game`)
          }
          return state
        })
        break

      case 'player_moved': {
        const targetPosition = new Vector3(
          message.position.x,
          message.position.y,
          message.position.z
        )
        // Set targetPosition for interpolation instead of directly setting position
        updatePlayer(message.player_id, { targetPosition })
        break
      }

      case 'chat_message': {
        const state = get(gameStore)
        const playerName =
          state.currentPlayer?.id === message.player_id
            ? state.currentPlayer.name
            : (state.otherPlayers.get(message.player_id)?.name ?? 'Unknown')
        addChatMessage(`${playerName}: ${message.message}`)
        addChatBubble(message.player_id, message.message)
        break
      }

      case 'game_state':
        gameStore.update((state) => {
          state.otherPlayers.clear()
          remotePlayerManager.reset() // Clear all remote player states
          Object.values(message.players).forEach((serverPlayer) => {
            if (serverPlayer.id !== state.currentPlayer?.id) {
              const playerPos = new Vector3(
                serverPlayer.position.x,
                serverPlayer.position.y,
                serverPlayer.position.z
              )
              const player: Player = {
                ...serverPlayer,
                position: playerPos,
                targetPosition: playerPos.clone(),
                maxHealth: serverPlayer.max_health,
              }
              // Initialize remote player with rotation from server
              remotePlayerManager.initPlayer(
                serverPlayer.id,
                serverPlayer.position,
                serverPlayer.rotation
              )
              state.otherPlayers.set(serverPlayer.id, player)
            }
          })
          return state
        })

        // Sync monsters from server - clear and rebuild to avoid ghost entities
        monsterManager.reset()
        if (message.monsters) {
          Object.values(message.monsters).forEach((monster: ServerMonster) => {
            monsterManager.spawnWithId(
              monster.id,
              monster.monster_type as MonsterData['type'],
              monster.position,
              monster.owner_id,
              monster.health,
              monster.max_health
            )
          })
        }
        break

      case 'monster_spawned':
        console.log('Monster spawned from server:', message.monster)
        monsterManager.spawnWithId(
          message.monster.id,
          message.monster.monster_type as MonsterData['type'],
          message.monster.position,
          message.monster.owner_id,
          message.monster.health,
          message.monster.max_health
        )
        break

      case 'monster_moved':
        monsterManager.updateMonsterFromNetwork(
          message.monster_id,
          message.position,
          message.rotation,
          message.state,
          message.target_position
        )
        break

      case 'monster_removed':
        console.log('Monster removed from server:', message.monster_id)
        monsterManager.remove(message.monster_id)
        break

      case 'monster_dead':
        console.log('Monster dead from server:', message.monster_id)
        monsterManager.handleMonsterDead(message.monster_id)
        break

      case 'player_attacked': {
        console.log('Player attacked:', message.player_id, 'Hit:', message.hit)
        remotePlayerManager.handleAttack(message.player_id)

        // Show hit/miss in chat for debugging
        const gameState = get(gameStore)
        const attackerName =
          gameState.currentPlayer?.id === message.player_id
            ? 'You'
            : gameState.otherPlayers.get(message.player_id)?.name || 'Unknown'

        if (message.hit) {
          addChatMessage(
            `${attackerName} rolled ${message.roll}: HIT for ${message.damage} damage!`
          )
        } else {
          addChatMessage(`${attackerName} rolled ${message.roll}: MISSED!`)
        }

        // Notify monsterManager that monster_id was attacked by player_id
        monsterManager.handleMonsterAttacked(
          message.monster_id,
          message.player_id,
          message.hit,
          message.damage
        )
        break
      }

      case 'monster_attacked_player': {
        console.log(
          'Monster',
          message.monster_id,
          'attacked player',
          message.player_id,
          'Hit:',
          message.hit
        )

        const gameState2 = get(gameStore)
        const isCurrentPlayer =
          gameState2.currentPlayer?.id === message.player_id

        // Only build damage info for current player (floating text)
        let damageInfo = undefined
        if (isCurrentPlayer) {
          const prevTrigger =
            gameState2.currentPlayer?.lastDamageInfo?.trigger ?? 0
          damageInfo = {
            damage: message.damage,
            hit: message.hit,
            trigger: prevTrigger + 1,
          }
        }

        if (message.hit) {
          // Update player HP and optionally damage info
          updatePlayer(message.player_id, {
            health: isCurrentPlayer
              ? Math.max(
                  0,
                  (gameState2.currentPlayer?.health ?? 0) - message.damage
                )
              : Math.max(
                  0,
                  (gameState2.otherPlayers.get(message.player_id)?.health ??
                    0) - message.damage
                ),
            ...(isCurrentPlayer ? { lastDamageInfo: damageInfo } : {}),
          })

          const targetName = isCurrentPlayer
            ? 'You'
            : (gameState2.otherPlayers.get(message.player_id)?.name ??
              'Unknown')
          addChatMessage(
            `Monster rolled ${message.roll}: HIT ${targetName} for ${message.damage} damage!`
          )
        } else {
          // Only update damage info for current player to show "Miss"
          if (isCurrentPlayer) {
            updatePlayer(message.player_id, {
              lastDamageInfo: damageInfo,
            })
          }
          addChatMessage(`Monster rolled ${message.roll}: MISSED!`)
        }
        break
      }

      case 'player_dead': {
        console.log('Player dead:', message.player_id)
        const gameState3 = get(gameStore)
        const isDeadCurrentPlayer =
          gameState3.currentPlayer?.id === message.player_id
        const deadPlayerName = isDeadCurrentPlayer
          ? 'You'
          : (gameState3.otherPlayers.get(message.player_id)?.name ?? 'Unknown')
        addChatMessage(
          `${deadPlayerName === 'You' ? 'You have' : deadPlayerName + ' has'} been slain!`
        )

        if (!isDeadCurrentPlayer) {
          remotePlayerManager.handleDead(message.player_id)
        }
        break
      }

      case 'player_respawned': {
        console.log('Player respawned:', message.player.id)
        const respawnPosition = new Vector3(
          message.player.position.x,
          message.player.position.y,
          message.player.position.z
        )
        const gameState4 = get(gameStore)
        const isCurrentPlayerRespawned =
          gameState4.currentPlayer?.id === message.player.id

        updatePlayer(message.player.id, {
          position: respawnPosition,
          targetPosition: respawnPosition.clone(),
          health: message.player.health,
          maxHealth: message.player.max_health,
        })

        if (isCurrentPlayerRespawned) {
          requestCameraReset()
          addChatMessage('You have respawned.')
        } else {
          addChatMessage(`${message.player.name} has respawned.`)
          remotePlayerManager.handleRespawn(
            message.player.id,
            message.player.position,
            message.player.rotation
          )
        }
        this.emitPlayerRespawned(message.player.id)
        break
      }
    }
  }

  sendPlayerAttack(monsterId: string) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      const message: ClientMessage = {
        type: 'player_attack',
        monster_id: monsterId,
      }
      this.socket.send(JSON.stringify(message))
    }
  }

  sendMonsterAttack(monsterId: string, targetPlayerId: string) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      const message: ClientMessage = {
        type: 'monster_attack',
        monster_id: monsterId,
        target_player_id: targetPlayerId,
      }
      this.socket.send(JSON.stringify(message))
    }
  }

  requestRespawn() {
    if (this.socket?.readyState === WebSocket.OPEN) {
      const message: ClientMessage = {
        type: 'request_respawn',
      }
      this.socket.send(JSON.stringify(message))
      this.emitRespawnRequested()
    }
  }

  sendPlayerMove(
    position: { x: number; y: number; z: number },
    rotation: number
  ) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      const message: ClientMessage = {
        type: 'player_move',
        position,
        rotation,
      }
      this.socket.send(JSON.stringify(message))
    }
  }

  sendMonsterMove(
    monsterId: string,
    position: { x: number; y: number; z: number },
    rotation: number,
    state: string,
    targetPosition: { x: number; y: number; z: number }
  ) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      const message: ClientMessage = {
        type: 'monster_move',
        monster_id: monsterId,
        position,
        rotation,
        state,
        target_position: targetPosition,
      }
      this.socket.send(JSON.stringify(message))
    }
  }

  sendChatMessage(message: string) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      const clientMessage: ClientMessage = {
        type: 'chat_message',
        message,
      }
      this.socket.send(JSON.stringify(clientMessage))
    }
  }

  joinGame(playerName: string) {
    this.lastPlayerName = playerName
    if (this.socket?.readyState === WebSocket.OPEN) {
      console.log('Sending join request for player:', playerName)
      const message: ClientMessage = {
        type: 'join',
        player_name: playerName,
      }
      this.socket.send(JSON.stringify(message))
      // Don't create currentPlayer here, wait for server response
    }
  }

  requestSpawnMonster(
    type: string,
    position: { x: number; y: number; z: number },
    rotation: number
  ) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      const message: ClientMessage = {
        type: 'request_spawn_monster',
        monster_type: type,
        position,
        rotation,
      }
      this.socket.send(JSON.stringify(message))
    }
  }

  disconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    if (this.socket) {
      this.socket.close()
      this.socket = null
    }
  }

  reconnect() {
    console.log('Manual reconnection requested. Resetting state...')
    this.disconnect()
    resetGameStore()
    monsterManager.reset()
    remotePlayerManager.reset()

    // Save references to rejoin after connection
    const serverUrl = this.lastServerUrl
    const playerName = this.lastPlayerName

    this.connect(serverUrl)

    // Wait for socket to open then join
    const checkInterval = setInterval(() => {
      if (this.socket?.readyState === WebSocket.OPEN) {
        clearInterval(checkInterval)
        this.joinGame(playerName)
      }
    }, 100)

    // Safety timeout
    setTimeout(() => clearInterval(checkInterval), 5000)
  }
}

export const networkManager = new NetworkManager()

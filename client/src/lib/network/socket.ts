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

class NetworkManager {
  private socket: WebSocket | null = null
  private reconnectAttempts = 0
  private maxReconnectAttempts = 5
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private lastServerUrl: string = ''
  private lastPlayerName: string = ''

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

        // Sync monsters if provided
        if (message.monsters) {
          // Ideally we should sync full state, but for now let's just spawn them if they don't exist
          // Or we can clear and respawn?
          // For simplicity, let's just make sure they are spawned
          Object.values(message.monsters).forEach((monster: ServerMonster) => {
            if (!monsterManager.monsters.has(monster.id)) {
              monsterManager.spawnWithId(
                monster.id,
                monster.monster_type as MonsterData['type'],
                monster.position,
                monster.owner_id
              )
            }
          })
        }
        break

      case 'monster_spawned':
        console.log('Monster spawned from server:', message.monster)
        monsterManager.spawnWithId(
          message.monster.id,
          message.monster.monster_type as MonsterData['type'],
          message.monster.position,
          message.monster.owner_id
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

      case 'player_attacked': {
        console.log('Player attacked:', message.player_id, 'Hit:', message.hit)
        remotePlayerManager.handleAttack(message.player_id)

        // Show hit/miss in chat for debugging
        const gameState = get(gameStore)
        const attackerName =
          gameState.currentPlayer?.id === message.player_id
            ? 'You'
            : gameState.otherPlayers.get(message.player_id)?.name || 'Unknown'
        addChatMessage(
          `${attackerName} rolled ${message.roll}: ${message.hit ? 'HIT' : 'MISSED'}!`
        )

        // Notify monsterManager that monster_id was attacked by player_id
        monsterManager.handleMonsterAttacked(
          message.monster_id,
          message.player_id,
          message.hit
        )
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

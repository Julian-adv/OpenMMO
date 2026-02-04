import { get } from 'svelte/store'
import {
  gameStore,
  updatePlayer,
  addChatMessage,
  addChatBubble,
} from '../stores/gameStore'
import type { Player } from '../stores/gameStore'
import { Vector3 } from 'three'
import { remotePlayerManager } from '../managers/remotePlayerManager'

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

type ClientMessage =
  | { type: 'join'; player_name: string }
  | { type: 'player_move'; position: Position; rotation: number }
  | { type: 'chat_message'; message: string }

type ServerMessage =
  | { type: 'player_joined'; player: ServerPlayer }
  | { type: 'player_left'; player_id: string }
  | {
      type: 'player_moved'
      player_id: string
      position: Position
      rotation: number
    }
  | { type: 'chat_message'; player_id: string; message: string }
  | { type: 'game_state'; players: Record<string, ServerPlayer> }
  | { type: 'join_success'; player: ServerPlayer }

class NetworkManager {
  private socket: WebSocket | null = null
  private reconnectAttempts = 0
  private maxReconnectAttempts = 5
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null

  connect(serverUrl: string = 'ws://192.168.0.17:8080') {
    if (this.socket?.readyState === WebSocket.OPEN) {
      console.log('Already connected, skipping connection attempt')
      return
    }

    if (this.socket?.readyState === WebSocket.CONNECTING) {
      console.log('Connection in progress, skipping connection attempt')
      return
    }

    console.log('Attempting to connect to:', serverUrl)
    this.socket = new WebSocket(serverUrl)

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
            const newOtherPlayers = new Map(state.otherPlayers)
            newOtherPlayers.set(message.player.id, player)
            addChatMessage(`${message.player.name} joined the game`)
            return { ...state, otherPlayers: newOtherPlayers }
          }
          return state
        })
        break
      }

      case 'player_left':
        gameStore.update((state) => {
          const player = state.otherPlayers.get(message.player_id)
          if (player) {
            const newOtherPlayers = new Map(state.otherPlayers)
            newOtherPlayers.delete(message.player_id)
            addChatMessage(`${player.name} left the game`)
            return { ...state, otherPlayers: newOtherPlayers }
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
          const newOtherPlayers = new Map<string, Player>()
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
              newOtherPlayers.set(serverPlayer.id, player)
            }
          })
          return { ...state, otherPlayers: newOtherPlayers }
        })
        break
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
}

export const networkManager = new NetworkManager()

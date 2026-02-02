import { writable } from 'svelte/store'
import type { Vector3 } from 'three'

export interface Player {
  id: string
  name: string
  position: Vector3
  targetPosition?: Vector3 // For remote player interpolation
  level: number
  health: number
  maxHealth: number
}

export interface GameState {
  isConnected: boolean
  currentPlayer: Player | null
  otherPlayers: Map<string, Player>
  chatMessages: string[]
}

const initialGameState: GameState = {
  isConnected: false,
  currentPlayer: null,
  otherPlayers: new Map(),
  chatMessages: [],
}

export const gameStore = writable<GameState>(initialGameState)

export const updatePlayer = (playerId: string, playerData: Partial<Player>) => {
  gameStore.update((state) => {
    if (state.currentPlayer && state.currentPlayer.id === playerId) {
      return {
        ...state,
        currentPlayer: { ...state.currentPlayer, ...playerData },
      }
    } else {
      const existingPlayer = state.otherPlayers.get(playerId)
      if (existingPlayer) {
        const newOtherPlayers = new Map(state.otherPlayers)
        newOtherPlayers.set(playerId, { ...existingPlayer, ...playerData })
        return { ...state, otherPlayers: newOtherPlayers }
      }
    }
    return state
  })
}

export const addChatMessage = (message: string) => {
  gameStore.update((state) => {
    const newMessages = [...state.chatMessages, message]
    return {
      ...state,
      chatMessages:
        newMessages.length > 100 ? newMessages.slice(-100) : newMessages,
    }
  })
}

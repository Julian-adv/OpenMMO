import { writable } from 'svelte/store'
import { SvelteMap } from 'svelte/reactivity'
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

export interface ChatBubble {
  playerId: string
  message: string
  timestamp: number
  duration: number
}

export interface GameState {
  isConnected: boolean
  currentPlayer: Player | null
  otherPlayers: Map<string, Player>
  chatMessages: string[]
  chatBubbles: Map<string, ChatBubble> // playerId -> ChatBubble
}

const initialGameState: GameState = {
  isConnected: false,
  currentPlayer: null,
  otherPlayers: new SvelteMap(),
  chatMessages: [],
  chatBubbles: new Map(),
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
        state.otherPlayers.set(playerId, { ...existingPlayer, ...playerData })
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

const MIN_BUBBLE_DURATION = 5000
const MAX_BUBBLE_DURATION = 10000

export const addChatBubble = (playerId: string, message: string) => {
  gameStore.update((state) => {
    const newChatBubbles = new Map(state.chatBubbles)
    const duration = Math.min(
      MAX_BUBBLE_DURATION,
      Math.max(MIN_BUBBLE_DURATION, MIN_BUBBLE_DURATION + message.length * 50)
    )
    newChatBubbles.set(playerId, {
      playerId,
      message,
      timestamp: Date.now(),
      duration,
    })
    return { ...state, chatBubbles: newChatBubbles }
  })
}

export const removeChatBubble = (playerId: string) => {
  gameStore.update((state) => {
    const newChatBubbles = new Map(state.chatBubbles)
    newChatBubbles.delete(playerId)
    return { ...state, chatBubbles: newChatBubbles }
  })
}

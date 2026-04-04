import { writable } from 'svelte/store'
import { SvelteMap } from 'svelte/reactivity'
import type { Vector3 } from 'three'
import type { CharacterClass, Gender } from '../network/networkTypes'

export interface PlayerDamageInfo {
  damage: number
  hit: boolean
  trigger: number
}

interface PlayerBase {
  id: string
  name: string
  level: number
  totalXp?: number
  health: number
  maxHealth: number
  characterClass: CharacterClass
  gender: Gender
  torchOn?: boolean
  lastDamageInfo?: PlayerDamageInfo
  lastRegenInfo?: PlayerDamageInfo
}

export interface LocalPlayer extends PlayerBase {
  position: Vector3
  rotation: number
}

export interface RemotePlayer extends PlayerBase {
  floorLevel: number
}

export interface ChatBubble {
  playerId: string
  message: string
  timestamp: number
  duration: number
}

export type ChatSender = 'local' | 'remote' | 'system'

export interface ChatEntry {
  text: string
  sender: ChatSender
  name?: string
  hit?: boolean
}

export interface GameState {
  isConnected: boolean
  currentPlayer: LocalPlayer | null
  otherPlayers: Map<string, RemotePlayer>
  chatMessages: ChatEntry[]
  combatMessages: ChatEntry[]
  chatBubbles: Map<string, ChatBubble> // playerId -> ChatBubble
}

const initialGameState: GameState = {
  isConnected: false,
  currentPlayer: null,
  otherPlayers: new SvelteMap(),
  chatMessages: [],
  combatMessages: [],
  chatBubbles: new Map(),
}

export const gameStore = writable<GameState>(initialGameState)

export const resetGameStore = () => {
  gameStore.set({
    ...initialGameState,
    otherPlayers: new SvelteMap(),
    chatBubbles: new Map(),
  })
}

const MAX_MESSAGES = 100

export const updatePlayer = (
  playerId: string,
  playerData: Partial<LocalPlayer> | Partial<RemotePlayer>
) => {
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

const addMessageTo = (
  field: 'chatMessages' | 'combatMessages',
  entry: ChatEntry
) => {
  gameStore.update((state) => {
    const newMessages = [...state[field], entry]
    return {
      ...state,
      [field]:
        newMessages.length > MAX_MESSAGES
          ? newMessages.slice(-MAX_MESSAGES)
          : newMessages,
    }
  })
}

export const addChatMessage = (entry: ChatEntry) =>
  addMessageTo('chatMessages', entry)

export const addCombatMessage = (entry: ChatEntry) =>
  addMessageTo('combatMessages', entry)

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

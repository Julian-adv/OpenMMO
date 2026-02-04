import {
  gameStore,
  removeChatBubble,
  type ChatBubble,
} from '../stores/gameStore'

export const CHAT_BUBBLE_DURATION = 5000 // 5 seconds

let chatBubbles: Map<string, ChatBubble> = new Map()
let checkInterval: ReturnType<typeof setInterval> | null = null

// Subscribe to gameStore to keep chatBubbles in sync
const unsubscribe = gameStore.subscribe((state) => {
  chatBubbles = state.chatBubbles
})

function checkExpiredChatBubbles() {
  const now = Date.now()
  for (const [playerId, bubble] of chatBubbles) {
    if (now - bubble.timestamp > CHAT_BUBBLE_DURATION) {
      removeChatBubble(playerId)
    }
  }
}

export function startChatBubbleChecker() {
  if (checkInterval) return

  checkInterval = setInterval(checkExpiredChatBubbles, 1000)
}

export function stopChatBubbleChecker() {
  if (checkInterval) {
    clearInterval(checkInterval)
    checkInterval = null
  }
}

export function cleanup() {
  stopChatBubbleChecker()
  unsubscribe()
}

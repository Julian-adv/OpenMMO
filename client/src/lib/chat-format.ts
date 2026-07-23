import type { ChatEntry } from './stores/gameStore'

/** The sender gets the same whisper echoed back; direction decides the label. */
export function whisperChatEntry(
  from: string,
  to: string,
  message: string,
  ownName: string | undefined
): ChatEntry {
  const outgoing = from === ownName
  return {
    text: message,
    sender: 'whisper',
    name: outgoing ? `To ${to}` : `From ${from}`,
  }
}

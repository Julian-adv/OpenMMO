import type { ChatEntry, StoredChatEntry } from './stores/gameStore'
import { translate, translateServerMessage, type Locale } from './i18n'

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
    name: outgoing ? to : from,
    whisperDirection: outgoing ? 'outgoing' : 'incoming',
  }
}

export function chatEntryName(entry: ChatEntry, language: Locale): string {
  const name = entry.name ?? ''
  if (!entry.whisperDirection) return name
  return translate(
    entry.whisperDirection === 'outgoing'
      ? 'chat.whisperTo'
      : 'chat.whisperFrom',
    { name },
    language
  )
}

export function chatEntryText(entry: ChatEntry, language: Locale): string {
  return translateServerMessage(
    { message: entry.text, localization: entry.localization },
    language
  )
}

export function shouldTranslateChatEntry(entry: ChatEntry): boolean {
  return (
    !entry.localization &&
    (entry.sender !== 'system' || entry.autoTranslate === true)
  )
}

/** Party lines carry the sender's name as-is; the panel adds the [Party] tag. */
export function partyChatEntry(from: string, message: string): ChatEntry {
  return { text: message, sender: 'party', name: from }
}

/** Count unread party lines by stable id, even after older rows are evicted. */
export function unreadPartyCount(
  entries: Pick<StoredChatEntry, 'sender' | 'name' | 'id'>[],
  seenId: number,
  ownName: string | undefined
): number {
  return entries.filter(
    (e) => e.sender === 'party' && e.id > seenId && e.name !== ownName
  ).length
}

export function unreadChatCount(
  entries: Pick<
    StoredChatEntry,
    'sender' | 'name' | 'id' | 'whisperDirection'
  >[],
  seenId: number,
  ownName: string | undefined
): number {
  return entries.filter(
    (entry) =>
      entry.id > seenId &&
      entry.sender !== 'local' &&
      entry.name !== ownName &&
      entry.whisperDirection !== 'outgoing'
  ).length
}

/** Keep legacy server notices alongside structured party messages. */
export function isPartyTabLine(
  entry: Pick<ChatEntry, 'sender' | 'text' | 'localization'>
): boolean {
  return (
    entry.sender === 'party' ||
    (entry.sender === 'system' &&
      (entry.localization?.code.startsWith('server.party') ||
        entry.localization?.code.startsWith('server.summon') ||
        entry.text.startsWith('Party:') ||
        entry.text.startsWith('Summon:')))
  )
}

import type { ChatEntry, StoredChatEntry } from './stores/gameStore'
import { translate, translateServerMessage, type Locale } from './i18n'

/** The sender gets the same whisper echoed back; direction decides the label.
 *  The line names the other party, so it carries their family. */
export function whisperChatEntry(
  from: string,
  to: string,
  message: string,
  ownName: string | undefined,
  fromFamily: string | null = null,
  toFamily: string | null = null
): ChatEntry {
  const outgoing = from === ownName
  return {
    text: message,
    sender: 'whisper',
    name: outgoing ? to : from,
    family: (outgoing ? toFamily : fromFamily) ?? null,
    whisperDirection: outgoing ? 'outgoing' : 'incoming',
  }
}

const NAME_MARK = '\uE000'

/** The label around the name, so the family can sit right after the name
 *  inside "To {name}" or "{name}에게". */
export function chatEntryNameParts(
  entry: ChatEntry,
  language: Locale
): { before: string; name: string; after: string } {
  const name = entry.name ?? ''
  if (!entry.whisperDirection) return { before: '', name, after: '' }
  const key =
    entry.whisperDirection === 'outgoing'
      ? 'chat.whisperTo'
      : 'chat.whisperFrom'
  const full = translate(key, { name }, language)
  const [before, after, ...rest] = translate(
    key,
    { name: NAME_MARK },
    language
  ).split(NAME_MARK)
  if (after === undefined || rest.length > 0 || before + name + after !== full)
    return { before: '', name: full, after: '' }
  return { before, name, after }
}

export function chatEntryName(entry: ChatEntry, language: Locale): string {
  const { before, name, after } = chatEntryNameParts(entry, language)
  return before + name + after
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
export function partyChatEntry(
  from: string,
  message: string,
  family: string | null = null
): ChatEntry {
  return { text: message, sender: 'party', name: from, family }
}

export function familyChatEntry(from: string, message: string): ChatEntry {
  return { text: message, sender: 'family', name: from }
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

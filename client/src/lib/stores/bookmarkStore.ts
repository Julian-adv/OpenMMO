import { get, writable } from 'svelte/store'
import { translate } from '../i18n'
import { npcNameById } from '../data/traderDefs'
import { addChatMessage } from './gameStore'
import { bagItemCount } from './inventoryStore'

/** The registry NPC `BLESSER` on the server. */
export const BLESSER = npcNameById('maelis')
/** Plain teleport scrolls the blesser will take: locked stacks are left alone. */
export const blessableScrolls = bagItemCount('scroll_of_teleportation', true)

export interface Bookmark {
  id: number
  name: string
  x: number
  z: number
}

export type BookmarkError =
  | 'name'
  | 'duplicate'
  | 'full'
  | 'not_surface'
  | 'no_landing'
  | 'owned'
  | 'combat'
  | 'unknown'
  | 'no_control'
  | 'failed'

export const bookmarks = writable<Bookmark[]>([])

/** Open bookmark dialog; `scroll` is the teleport scroll being aimed, if any. */
export const bookmarkDialog = writable<{
  scroll: number | null
  error: BookmarkError | null
} | null>(null)

export function openBookmarks(scroll: number | null = null) {
  bookmarkDialog.set({ scroll, error: null })
}

export function applyBookmarks(list: Bookmark[]) {
  bookmarks.set(list)
  bookmarkDialog.update((dialog) => dialog && { ...dialog, error: null })
}

/** A refusal shows in the open dialog, or in chat once it has closed. */
export function refuseBookmark(error: BookmarkError) {
  if (get(bookmarkDialog)) {
    bookmarkDialog.update((dialog) => dialog && { ...dialog, error })
  } else {
    addChatMessage({
      text: translate(`bookmark.error.${error}`),
      sender: 'system',
    })
  }
}

export type BlessError =
  | 'too_far'
  | 'asleep'
  | 'off_duty'
  | 'no_scrolls'
  | 'trading'
  | 'not_enough_gold'
  | 'failed'

export interface BlessSession {
  npcPlayerId: number
  pending: boolean
  error: BlessError | null
  /** Scrolls blessed by the last request of this visit. */
  blessed: number | null
}

export const blessSession = writable<BlessSession | null>(null)

export function openBlessing(npcPlayerId: number) {
  blessSession.set({ npcPlayerId, pending: false, error: null, blessed: null })
}

/** A refusal with no window open (asking through the menu) goes to chat. */
export function applyBlessResult(data: {
  blessed: number
  error?: BlessError | null
}) {
  if (!get(blessSession) && data.error) {
    addChatMessage({
      text: translate(`bless.error.${data.error}`),
      sender: 'system',
    })
    return
  }
  blessSession.update(
    (session) =>
      session && {
        ...session,
        pending: false,
        error: data.error ?? null,
        blessed: data.error ? null : data.blessed,
      }
  )
}

export function resetBookmarks() {
  bookmarks.set([])
  bookmarkDialog.set(null)
  blessSession.set(null)
}

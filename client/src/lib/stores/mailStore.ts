import { get, writable } from 'svelte/store'
import { translate, type MessageKey } from '../i18n'
import { addChatMessage } from './gameStore'
import { bagItemCount } from './inventoryStore'
import { showAnnouncement } from './announcementStore'
import type { GameDateTime } from './noticeBoardStore'

/** Mirrors shared/src/mail.rs. */
const LETTER_ITEM = 'letter'
export const LETTER_BODY_MAX_CHARS = 1000

export type MailboxRef =
  | { Village: { object_id: number } }
  | { Estate: { furniture_id: number } }

export interface LetterSummary {
  id: number
  sender: string
  excerpt: string
  written_at: GameDateTime
  read: boolean
}

export interface Letter {
  id: number
  sender: string
  body: string
  written_at: GameDateTime
}

export type MailRefusal =
  | 'out_of_reach'
  | 'empty'
  | 'too_long'
  | 'too_fast'
  | 'no_letter_paper'
  | 'no_such_recipient'
  | 'recipient_full'
  | 'letter_missing'
  | 'server_error'

export type MailView =
  | { kind: 'list' }
  | { kind: 'letter'; letter: Letter }
  | { kind: 'write' }

export interface MailSession {
  mailbox: MailboxRef
  view: MailView
  letters: LetterSummary[]
  /** Banner above the view: why a request was refused, or whom a letter went to. */
  notice: { refused: MailRefusal } | { sentTo: string } | null
  /** Set while a request is in flight; cleared by any reply. */
  pending: boolean
}

export const mailSession = writable<MailSession | null>(null)

export const letterPaperCount = bagItemCount(LETTER_ITEM)

const sameMailbox = (a: MailboxRef, b: MailboxRef) =>
  JSON.stringify(a) === JSON.stringify(b)

export function closeMailbox() {
  mailSession.set(null)
}

export function handleMailbox(data: {
  mailbox: MailboxRef
  letters: LetterSummary[]
}) {
  mailSession.set({
    mailbox: data.mailbox,
    view: { kind: 'list' },
    letters: data.letters,
    notice: null,
    pending: false,
  })
}

export function handleMailLetter(data: {
  mailbox: MailboxRef
  letter: Letter
}) {
  mailSession.update((session) => {
    if (!session || !sameMailbox(session.mailbox, data.mailbox)) return session
    return {
      ...session,
      view: { kind: 'letter', letter: data.letter },
      letters: session.letters.map((l) =>
        l.id === data.letter.id ? { ...l, read: true } : l
      ),
      notice: null,
      pending: false,
    }
  })
}

export function handleMailSent(recipient: string) {
  mailSession.update(
    (session) =>
      session && {
        ...session,
        view: { kind: 'list' },
        notice: { sentTo: recipient },
        pending: false,
      }
  )
}

/** Shown in the open mailbox; in chat when it closes the mailbox or none is open. */
export function handleMailRefused(reason: MailRefusal) {
  mailSession.update((session) => {
    if (session && reason !== 'out_of_reach') {
      return { ...session, notice: { refused: reason }, pending: false }
    }
    addChatMessage({
      text: translate(`mail.refused.${reason}` as MessageKey),
      sender: 'system',
    })
    return null
  })
}

/** Unread count waiting to be shown once the world has loaded. */
export const pendingMailNotice = writable<number | null>(null)

export function handleMailNotice(unread: number) {
  pendingMailNotice.set(unread)
}

export function showPendingMailNotice() {
  const unread = get(pendingMailNotice)
  if (unread === null) return
  pendingMailNotice.set(null)
  showAnnouncement(
    translate('mail.unread', { count: unread }),
    translate('mail.title')
  )
}

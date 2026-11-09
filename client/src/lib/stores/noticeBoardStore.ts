import { writable } from 'svelte/store'
import { translate, type MessageKey } from '../i18n'
import { addChatMessage } from './gameStore'
import type { ServerGameTime } from './timeStore'

/** Mirrors shared/src/notice_board.rs. */
export const NOTICE_TITLE_MAX_CHARS = 60
export const NOTICE_BODY_MAX_CHARS = 2000
export const NOTICE_COMMENT_MAX_CHARS = 300

/** `GameDateTime` in shared/src/world.rs. */
export type GameDateTime = Omit<ServerGameTime, 'isNight'>

export interface NoticePostSummary {
  id: number
  title: string
  author: string
  written_at: GameDateTime
  comment_count: number
}

export interface NoticePost {
  id: number
  title: string
  body: string
  author: string
  written_at: GameDateTime
}

export interface NoticeComment {
  id: number
  author: string
  body: string
  written_at: GameDateTime
}

export type NoticeBoardRefusal =
  | 'out_of_reach'
  | 'empty'
  | 'too_long'
  | 'too_fast'
  | 'post_missing'
  | 'comments_full'
  | 'server_error'

export type NoticeBoardView =
  | { kind: 'list' }
  | { kind: 'post'; post: NoticePost; comments: NoticeComment[] }
  | { kind: 'write' }

export interface NoticeBoardSession {
  objectId: number
  view: NoticeBoardView
  posts: NoticePostSummary[]
  refusal: NoticeBoardRefusal | null
  /** Set while a request is in flight; cleared by any reply. */
  pending: boolean
}

export const noticeBoard = writable<NoticeBoardSession | null>(null)

export function closeNoticeBoard() {
  noticeBoard.set(null)
}

export function handleNoticeBoard(data: {
  object_id: number
  posts: NoticePostSummary[]
}) {
  noticeBoard.set({
    objectId: data.object_id,
    view: { kind: 'list' },
    posts: data.posts,
    refusal: null,
    pending: false,
  })
}

export function handleNoticePostView(data: {
  object_id: number
  post: NoticePost
  comments: NoticeComment[]
}) {
  noticeBoard.update((session) => {
    if (!session || session.objectId !== data.object_id) return session
    const { id, title, author, written_at } = data.post
    const summary = {
      id,
      title,
      author,
      written_at,
      comment_count: data.comments.length,
    }
    const posts = [...session.posts]
    const index = posts.findIndex((p) => p.id === id)
    if (index >= 0) posts[index] = summary
    else posts.unshift(summary)
    return {
      ...session,
      view: { kind: 'post', post: data.post, comments: data.comments },
      posts,
      refusal: null,
      pending: false,
    }
  })
}

/** Shown in the open board; in chat when it closes the board or none is open. */
export function handleNoticeBoardRefused(reason: NoticeBoardRefusal) {
  noticeBoard.update((session) => {
    if (session && reason !== 'out_of_reach') {
      return { ...session, refusal: reason, pending: false }
    }
    addChatMessage({
      text: translate(`noticeBoard.refused.${reason}` as MessageKey),
      sender: 'system',
    })
    return null
  })
}

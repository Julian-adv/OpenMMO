import { derived, writable } from 'svelte/store'
import { translate, type MessageKey } from '../i18n'
import { addChatMessage } from './gameStore'
import { inventoryStore } from './inventoryStore'
import type { ServerGameTime } from './timeStore'

/** Mirrors shared/src/notice_board.rs. */
export const NOTICE_TITLE_MAX_CHARS = 60
export const NOTICE_BODY_MAX_CHARS = 2000
export const NOTICE_COMMENT_MAX_CHARS = 300
export const NOTICE_PAPER_ITEM = 'notice_paper'
export const NOTICE_EXCERPT_CHARS = 80

export const noticeExcerpt = (body: string) =>
  [...body].slice(0, NOTICE_EXCERPT_CHARS).join('')
export const BOARD_FACE_W = 1.66
export const BOARD_FACE_H = 1.4
export const PAPER_SCALE = 1000
/** Paper size in metres. */
export const PAPER_W = 0.3
export const PAPER_H = 0.36

/** `clamp_paper_position`: keeps the whole paper on the face. */
export function clampPaperPosition(x: number, y: number): [number, number] {
  const halfX = Math.ceil((PAPER_W / 2 / BOARD_FACE_W) * PAPER_SCALE)
  const halfY = Math.ceil((PAPER_H / 2 / BOARD_FACE_H) * PAPER_SCALE)
  const clamp = (v: number, lo: number) =>
    Math.min(PAPER_SCALE - lo, Math.max(lo, Math.round(v)))
  return [clamp(x, halfX), clamp(y, halfY)]
}

export const NOTICE_MAX_COVER = 0.2
export const NOTICE_POSTS_PER_WRITER = 3

/** `cover_fraction`: share of a paper at `b` hidden by one at `a`. */
export function coverFraction(
  a: [number, number],
  b: [number, number]
): number {
  const dx = (Math.abs(a[0] - b[0]) / PAPER_SCALE) * BOARD_FACE_W
  const dy = (Math.abs(a[1] - b[1]) / PAPER_SCALE) * BOARD_FACE_H
  return (
    (Math.max(0, PAPER_W - dx) * Math.max(0, PAPER_H - dy)) /
    (PAPER_W * PAPER_H)
  )
}

/** `spot_allowed`: leaves each protected paper mostly visible. */
export function spotAllowed(
  spot: [number, number],
  protectedSpots: [number, number][]
): boolean {
  return protectedSpots.every((p) => coverFraction(spot, p) <= NOTICE_MAX_COVER)
}

/** `free_paper_spot`: the allowed spot least covering any paper, or null. */
export function freePaperSpot(
  protectedSpots: [number, number][],
  all: [number, number][]
): [number, number] | null {
  let best: [number, number] | null = null
  let bestCover = Infinity
  for (let row = 0; row <= 8; row++) {
    for (let col = 0; col <= 10; col++) {
      const spot = clampPaperPosition(col * 100, row * 125)
      if (!spotAllowed(spot, protectedSpots)) continue
      const cover = all.reduce((sum, p) => sum + coverFraction(spot, p), 0)
      if (cover < bestCover) {
        best = spot
        bestCover = cover
      }
    }
  }
  return best
}

/** `paper_tilt`: small rotation in radians, stable per post. */
export function paperTilt(id: number): number {
  const mixed = BigInt.asUintN(64, BigInt(id) * 2654435761n)
  const h = Number((mixed >> 8n) % 1000n)
  return (h / 999 - 0.5) * 0.14
}

export const noticePaperCount = derived(inventoryStore, (inv) =>
  inv.bag
    .filter((item) => item.item_def_id === NOTICE_PAPER_ITEM)
    .reduce((sum, item) => sum + item.quantity, 0)
)

/** `GameDateTime` in shared/src/world.rs. */
export type GameDateTime = Omit<ServerGameTime, 'isNight'>

export interface NoticePostSummary {
  id: number
  title: string
  excerpt: string
  author: string
  written_at: GameDateTime
  comment_count: number
  x: number
  y: number
  image?: string | null
  expired: boolean
  mine: boolean
}

export interface NoticePost {
  id: number
  title: string
  body: string
  author: string
  written_at: GameDateTime
  x: number
  y: number
  image?: string | null
  expired: boolean
}

/** One paper on the board model. */
export interface NoticePaper {
  id: number
  x: number
  y: number
  poster: boolean
  expired: boolean
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
  | 'no_paper'
  | 'bad_image'
  | 'not_registrar'
  | 'nothing_expired'
  | 'covers_others'
  | 'board_full'
  | 'too_many_posts'
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

/** Papers per board object id, for the board model. */
export const noticeBoardPapers = writable<Map<number, NoticePaper[]>>(new Map())

const samePaper = (a: NoticePaper, b: NoticePaper) =>
  a.id === b.id &&
  a.x === b.x &&
  a.y === b.y &&
  a.poster === b.poster &&
  a.expired === b.expired

/** Keeps the old list when nothing changed, so the board model is not rebuilt. */
function setPapers(objectId: number, papers: NoticePaper[]) {
  noticeBoardPapers.update((all) => {
    const old = all.get(objectId)
    if (
      old?.length === papers.length &&
      old.every((p, i) => samePaper(p, papers[i]))
    )
      return all
    return new Map(all).set(objectId, papers)
  })
}

function papersOf(posts: NoticePostSummary[]): NoticePaper[] {
  return [...posts]
    .sort((a, b) => a.id - b.id)
    .map(({ id, x, y, image, expired }) => ({
      id,
      x,
      y,
      poster: !!image,
      expired,
    }))
}

export const handleNoticeBoardPapers = (data: {
  object_id: number
  papers: NoticePaper[]
}) => setPapers(data.object_id, data.papers)

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
  setPapers(data.object_id, papersOf(data.posts))
}

export function handleNoticePostView(data: {
  object_id: number
  post: NoticePost
  comments: NoticeComment[]
}) {
  noticeBoard.update((session) => {
    if (!session || session.objectId !== data.object_id) return session
    const { id, title, author, written_at, x, y, image, expired } = data.post
    const summary: NoticePostSummary = {
      id,
      title,
      excerpt: noticeExcerpt(data.post.body),
      author,
      written_at,
      comment_count: data.comments.length,
      x,
      y,
      image,
      expired,
      mine:
        session.view.kind === 'write' ||
        session.posts.some((p) => p.id === id && p.mine),
    }
    const posts = [...session.posts]
    const index = posts.findIndex((p) => p.id === id)
    if (index >= 0) posts[index] = summary
    else posts.unshift(summary)
    setPapers(session.objectId, papersOf(posts))
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

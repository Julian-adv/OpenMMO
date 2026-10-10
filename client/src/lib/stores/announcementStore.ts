import { writable } from 'svelte/store'

export const ANNOUNCEMENT_DURATION_MS = 30_000
const MAX_VISIBLE_ANNOUNCEMENTS = 3

type Announcement = {
  id: number
  text: string
  timer: ReturnType<typeof setTimeout>
}

export const announcements = writable<Announcement[]>([])
let nextId = 0

export function dismissAnnouncement(id: number) {
  announcements.update((entries) => {
    clearTimeout(entries.find((entry) => entry.id === id)?.timer)
    return entries.filter((entry) => entry.id !== id)
  })
}

export function clearAnnouncements() {
  announcements.update((entries) => {
    for (const entry of entries) clearTimeout(entry.timer)
    return []
  })
}

export function showAnnouncement(text: string) {
  const id = ++nextId
  const timer = setTimeout(
    () => dismissAnnouncement(id),
    ANNOUNCEMENT_DURATION_MS
  )
  announcements.update((entries) => {
    const updated = [...entries, { id, text, timer }]
    for (const evicted of updated.slice(0, -MAX_VISIBLE_ANNOUNCEMENTS)) {
      clearTimeout(evicted.timer)
    }
    return updated.slice(-MAX_VISIBLE_ANNOUNCEMENTS)
  })
}

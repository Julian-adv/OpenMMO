import { get, writable } from 'svelte/store'
import { translate } from '../i18n'
import { addChatMessage } from './gameStore'

/** The character the player walks after until they move by hand or lose them. */
export const followTarget = writable<{ id: number; name: string } | null>(null)

export function stopFollowing(messageKey?: 'follow.stopped' | 'follow.lost') {
  const target = get(followTarget)
  if (!target) return
  followTarget.set(null)
  if (messageKey)
    addChatMessage({
      text: translate(messageKey, { name: target.name }),
      sender: 'system',
    })
}

export function toggleFollow(id: number, name: string) {
  if (get(followTarget)?.id === id) return stopFollowing('follow.stopped')
  followTarget.set({ id, name })
  addChatMessage({
    text: translate('follow.started', { name }),
    sender: 'system',
  })
}

import { get, writable } from 'svelte/store'
import { translate } from '../i18n'
import { networkManager } from '../network/socket'
import { addChatMessage } from './gameStore'

/** The character the player walks after until they move by hand or lose them.
 *  `admin` marks an admin `/follow`, which only the server ends. */
export const followTarget = writable<{
  id: number
  name: string
  admin?: boolean
} | null>(null)

export function stopFollowing(messageKey?: 'follow.stopped' | 'follow.lost') {
  const target = get(followTarget)
  if (!target || target.admin) return
  followTarget.set(null)
  if (messageKey)
    addChatMessage({
      text: translate(messageKey, { name: target.name }),
      sender: 'system',
    })
}

export function toggleFollow(id: number, name: string) {
  const current = get(followTarget)
  if (current?.admin) {
    networkManager.sendChatMessage(
      current.id === id ? '/unfollow' : `/follow ${name}`
    )
    return
  }
  if (current?.id === id) return stopFollowing('follow.stopped')
  followTarget.set({ id, name })
  addChatMessage({
    text: translate('follow.started', { name }),
    sender: 'system',
  })
}

export function setAdminFollow(target: { id: number; name: string } | null) {
  if (target) followTarget.set({ ...target, admin: true })
  else if (get(followTarget)?.admin) followTarget.set(null)
}

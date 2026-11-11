import { writable } from 'svelte/store'

/** Where plain input lines go: local say, the party or the family channel.
 *  Sticky: `/p` or `/g` (or the input-bar menu) holds until `/s` switches
 *  back; losing the group reverts it (see ChatPanel). */
export type ChatChannel = 'say' | 'party' | 'family'

export const chatChannel = writable<ChatChannel>('say')

/** Losing the channel's group reverts the input to say, but only once the
 *  draft is empty. A pending line keeps the private channel, so it can only
 *  fall into the server's private refusal, never into public chat. */
export function shouldRevertToSay(
  channelOpen: boolean,
  channel: ChatChannel,
  draft: string
): boolean {
  return !channelOpen && channel !== 'say' && draft.trim().length === 0
}

export function shouldBlockNpcTalkForPrivateDraft(
  channel: ChatChannel,
  draft: string
): boolean {
  return channel !== 'say' && draft.trim().length > 0
}

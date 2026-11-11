import { describe, it, expect } from 'vitest'
import {
  shouldBlockNpcTalkForPrivateDraft,
  shouldRevertToSay,
} from './chatChannelStore'

describe('shouldRevertToSay', () => {
  it('reverts an empty draft when the party is gone', () => {
    expect(shouldRevertToSay(false, 'party', '')).toBe(true)
    expect(shouldRevertToSay(false, 'party', '   ')).toBe(true)
  })

  it('keeps a pending party draft off the public channel', () => {
    // The draft holds the party channel, so sending it draws the server's
    // private not-in-a-party refusal instead of local chat.
    expect(shouldRevertToSay(false, 'party', 'meet at the west gate')).toBe(
      false
    )
  })

  it('never reverts while the party is alive or already on say', () => {
    expect(shouldRevertToSay(true, 'party', '')).toBe(false)
    expect(shouldRevertToSay(false, 'say', '')).toBe(false)
  })

  it('reverts the family channel the same way', () => {
    expect(shouldRevertToSay(false, 'family', '')).toBe(true)
    expect(shouldRevertToSay(false, 'family', 'see you at home')).toBe(false)
  })
})

describe('shouldBlockNpcTalkForPrivateDraft', () => {
  it('blocks retargeting a party draft to NPC talk', () => {
    expect(
      shouldBlockNpcTalkForPrivateDraft('party', 'keep this in the party')
    ).toBe(true)
  })

  it('allows NPC talk when no party draft would be exposed', () => {
    expect(shouldBlockNpcTalkForPrivateDraft('party', '   ')).toBe(false)
    expect(shouldBlockNpcTalkForPrivateDraft('say', 'hello')).toBe(false)
    expect(shouldBlockNpcTalkForPrivateDraft('family', 'hello')).toBe(true)
  })
})

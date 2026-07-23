import { describe, it, expect } from 'vitest'
import { whisperChatEntry } from './chat-format'

describe('whisperChatEntry', () => {
  it('labels the echo of an own whisper as outgoing', () => {
    expect(whisperChatEntry('Miru', 'Rica', 'psst', 'Miru')).toEqual({
      text: 'psst',
      sender: 'whisper',
      name: 'To Rica',
    })
  })

  it('labels a received whisper with the sender', () => {
    expect(whisperChatEntry('Miru', 'Rica', 'psst', 'Rica')).toEqual({
      text: 'psst',
      sender: 'whisper',
      name: 'From Miru',
    })
  })

  it('treats an unknown own name as incoming', () => {
    expect(whisperChatEntry('Miru', 'Rica', 'psst', undefined).name).toBe(
      'From Miru'
    )
  })
})

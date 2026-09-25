import { describe, expect, it } from 'vitest'
import { isChatAtBottom } from './chat-scroll'

describe('isChatAtBottom', () => {
  it('follows messages when the transcript is at the bottom', () => {
    expect(
      isChatAtBottom({ scrollHeight: 500, scrollTop: 300, clientHeight: 200 })
    ).toBe(true)
  })

  it('allows small fractional layout differences at the bottom', () => {
    expect(
      isChatAtBottom({ scrollHeight: 500, scrollTop: 292, clientHeight: 200 })
    ).toBe(true)
  })

  it('does not follow messages while reading earlier lines', () => {
    expect(
      isChatAtBottom({ scrollHeight: 500, scrollTop: 250, clientHeight: 200 })
    ).toBe(false)
  })
})

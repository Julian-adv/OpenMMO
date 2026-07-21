import { describe, expect, it } from 'vitest'
import { chatInputKeyIntent } from './chat-input-keys'

function key(
  key: string,
  opts: { isComposing?: boolean; keyCode?: number } = {}
) {
  return {
    key,
    isComposing: opts.isComposing ?? false,
    keyCode: opts.keyCode ?? 0,
  }
}

describe('chatInputKeyIntent', () => {
  it('sends on plain Enter', () => {
    expect(chatInputKeyIntent(key('Enter', { keyCode: 13 }))).toBe('send')
  })

  it('does not send on Enter during IME composition', () => {
    expect(
      chatInputKeyIntent(key('Enter', { isComposing: true, keyCode: 13 }))
    ).toBe('none')
  })

  it('does not send on IME keydown reported as keyCode 229', () => {
    expect(chatInputKeyIntent(key('Enter', { keyCode: 229 }))).toBe('none')
  })

  it('sends on the Enter that follows a committed composition', () => {
    const composing = key('Enter', { isComposing: true, keyCode: 13 })
    const committed = key('Enter', { keyCode: 13 })
    expect(chatInputKeyIntent(composing)).toBe('none')
    expect(chatInputKeyIntent(committed)).toBe('send')
  })

  it('completes commands on Tab', () => {
    expect(chatInputKeyIntent(key('Tab', { keyCode: 9 }))).toBe(
      'complete-command'
    )
  })

  it('ignores other keys', () => {
    expect(chatInputKeyIntent(key('a', { keyCode: 65 }))).toBe('none')
    expect(chatInputKeyIntent(key('Escape', { keyCode: 27 }))).toBe('none')
  })
})

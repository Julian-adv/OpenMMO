export type ChatKeyIntent =
  | 'complete-command'
  | 'send'
  | 'blur'
  | 'history-prev'
  | 'history-next'
  | 'none'

/** Complete partial command names without extending an exact match. */
export function commandCompletions(input: string, names: string[]): string[] {
  if (!input.startsWith('/') || input.includes(' ')) return []
  if (names.includes(input)) return []
  return names.filter((name) => name.startsWith(input))
}

/** Focus chat on Enter unless the channel menu owns the key. */
export function shouldFocusChatOnEnter(
  event: { key: string; isComposing: boolean; keyCode: number },
  channelMenuOpen: boolean
): boolean {
  if (event.isComposing || event.keyCode === 229) return false
  return event.key === 'Enter' && !channelMenuOpen
}

/** Ignore IME keydowns, including browsers that report only keyCode 229. */
export function chatInputKeyIntent(event: {
  key: string
  isComposing: boolean
  keyCode: number
}): ChatKeyIntent {
  if (event.isComposing || event.keyCode === 229) return 'none'
  if (event.key === 'Escape') return 'blur'
  if (event.key === 'Tab') return 'complete-command'
  if (event.key === 'ArrowUp') return 'history-prev'
  if (event.key === 'ArrowDown') return 'history-next'
  if (event.key !== 'Enter') return 'none'
  return 'send'
}

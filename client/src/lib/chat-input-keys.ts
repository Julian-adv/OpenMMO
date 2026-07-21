export type ChatKeyIntent = 'complete-command' | 'send' | 'none'

/** Enter pressed while an IME composition is active (e.g. Korean) only commits
 *  the composing syllable; sending then would clear the input mid-composition
 *  and leave the committed syllable behind as a stray follow-up message.
 *  keyCode 229 covers browsers that fire IME keydowns without isComposing. */
export function chatInputKeyIntent(event: {
  key: string
  isComposing: boolean
  keyCode: number
}): ChatKeyIntent {
  if (event.key === 'Tab') return 'complete-command'
  if (event.key !== 'Enter') return 'none'
  if (event.isComposing || event.keyCode === 229) return 'none'
  return 'send'
}

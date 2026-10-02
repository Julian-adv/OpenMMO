import { persistedString } from './persisted'

const PICKUP_KEY_LABELS = {
  Comma: ',',
  Period: '.',
  Semicolon: ';',
  Quote: "'",
  Slash: '/',
  Backslash: '\\',
  BracketLeft: '[',
  BracketRight: ']',
  Minus: '-',
  Equal: '=',
  Backquote: '`',
  KeyQ: 'Q',
  KeyR: 'R',
  KeyT: 'T',
  KeyY: 'Y',
  KeyU: 'U',
  KeyO: 'O',
  KeyP: 'P',
  KeyH: 'H',
  KeyJ: 'J',
  KeyK: 'K',
  KeyL: 'L',
  KeyZ: 'Z',
  KeyX: 'X',
  KeyV: 'V',
  KeyB: 'B',
  KeyN: 'N',
} as const

export type PickupKeyCode = keyof typeof PICKUP_KEY_LABELS
export const DEFAULT_PICKUP_KEY: PickupKeyCode = 'Comma'

export function isPickupKeyCode(code: string): code is PickupKeyCode {
  return Object.hasOwn(PICKUP_KEY_LABELS, code)
}

export function pickupKeyLabel(code: PickupKeyCode): string {
  return PICKUP_KEY_LABELS[code]
}

export const pickupKeyCode = persistedString<PickupKeyCode>(
  'onlinerpg_pickupKeyCode',
  DEFAULT_PICKUP_KEY,
  isPickupKeyCode
)

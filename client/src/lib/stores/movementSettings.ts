import { persistedBoolean, persistedString } from './persisted'

export const alwaysRun = persistedBoolean('onlinerpg_alwaysRun', true)

export type KeyboardMovementMode = 'world' | 'character'

export const keyboardMovementMode = persistedString<KeyboardMovementMode>(
  'onlinerpg_keyboardMovementMode',
  'world',
  (value): value is KeyboardMovementMode =>
    value === 'world' || value === 'character'
)

// Cache the preference for per-frame reads.
let alwaysRunNow = false
alwaysRun.subscribe((v) => (alwaysRunNow = v))

// Shift inverts the preference.
export function sprintRequested(shiftHeld: boolean): boolean {
  return alwaysRunNow ? !shiftHeld : shiftHeld
}

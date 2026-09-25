import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'

describe('keyboard movement preference', () => {
  const storage = new Map<string, string>()
  const key = 'onlinerpg_keyboardMovementMode'

  beforeEach(() => {
    vi.resetModules()
    storage.clear()
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
    })
  })

  afterEach(() => vi.unstubAllGlobals())

  it('defaults to fixed directions for existing and new players', async () => {
    storage.set('onlinerpg_alwaysRun', 'false')
    const { keyboardMovementMode } = await import('./movementSettings')
    expect(get(keyboardMovementMode)).toBe('world')
  })

  it.each(['world', 'character'] as const)(
    'restores saved %s controls',
    async (mode) => {
      storage.set(key, mode)
      const { keyboardMovementMode } = await import('./movementSettings')
      expect(get(keyboardMovementMode)).toBe(mode)
    }
  )

  it('persists changes across reloads', async () => {
    const { keyboardMovementMode } = await import('./movementSettings')
    keyboardMovementMode.set('character')
    vi.resetModules()
    expect(get((await import('./movementSettings')).keyboardMovementMode)).toBe(
      'character'
    )
    keyboardMovementMode.set('world')
    vi.resetModules()
    expect(get((await import('./movementSettings')).keyboardMovementMode)).toBe(
      'world'
    )
  })

  it('falls back to fixed directions for an invalid saved mode', async () => {
    storage.set(key, 'invalid')
    const { keyboardMovementMode } = await import('./movementSettings')
    expect(get(keyboardMovementMode)).toBe('world')
  })
})

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { get } from 'svelte/store'

describe('pickup key preference', () => {
  const storage = new Map<string, string>()
  const storageKey = 'onlinerpg_pickupKeyCode'

  beforeEach(() => {
    vi.resetModules()
    storage.clear()
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
    })
  })

  afterEach(() => vi.unstubAllGlobals())

  it('defaults to comma', async () => {
    const { pickupKeyCode, pickupKeyLabel } = await import('./pickupSettings')
    expect(get(pickupKeyCode)).toBe('Comma')
    expect(pickupKeyLabel(get(pickupKeyCode))).toBe(',')
  })

  it('persists rebinding and reset across reloads', async () => {
    const { pickupKeyCode, DEFAULT_PICKUP_KEY } =
      await import('./pickupSettings')
    pickupKeyCode.set('KeyZ')
    vi.resetModules()
    const reloaded = await import('./pickupSettings')
    expect(get(reloaded.pickupKeyCode)).toBe('KeyZ')
    expect(reloaded.pickupKeyLabel(get(reloaded.pickupKeyCode))).toBe('Z')
    reloaded.pickupKeyCode.set(DEFAULT_PICKUP_KEY)
    vi.resetModules()
    expect(get((await import('./pickupSettings')).pickupKeyCode)).toBe('Comma')
  })

  it.each(['Period', 'KeyQ', 'KeyZ'])(
    'restores the saved %s binding',
    async (code) => {
      storage.set(storageKey, code)
      expect(get((await import('./pickupSettings')).pickupKeyCode)).toBe(code)
    }
  )

  it.each([
    'invalid',
    'KeyW',
    'KeyE',
    'KeyM',
    'KeyF',
    'Digit1',
    'Escape',
    'Enter',
    'Space',
    'constructor',
    'toString',
  ])('rejects reserved or invalid saved key %s', async (code) => {
    storage.set(storageKey, code)
    const { pickupKeyCode, isPickupKeyCode } = await import('./pickupSettings')
    expect(isPickupKeyCode(code)).toBe(false)
    expect(get(pickupKeyCode)).toBe('Comma')
  })
})

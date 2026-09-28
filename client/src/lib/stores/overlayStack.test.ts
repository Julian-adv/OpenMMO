import { beforeEach, describe, expect, it } from 'vitest'
import { get } from 'svelte/store'
import { characterPanelVisible, inventoryVisible } from './debugStore'
import { emotePanelVisible } from './emoteStore'
import { shopSession, type ShopSession } from './tradeStore'
import {
  closeTopOverlay,
  mountOverlay,
  openOverlays,
  topOverlay,
} from './overlayStack'

const SESSION: ShopSession = {
  merchantPlayerId: 1,
  merchantName: 'Rica',
  catalog: [],
  sellRatePercent: 50,
  priceIndexPercent: 100,
  wishlist: [],
  stock: [],
  buyback: [],
}

beforeEach(() => {
  characterPanelVisible.set(false)
  inventoryVisible.set(false)
  emotePanelVisible.set(false)
  shopSession.set(null)
})

describe('topOverlay', () => {
  it('breaks a same-layer tie with the most recently opened', () => {
    expect(topOverlay(['inventory', 'character'])).toBe('character')
    expect(topOverlay(['character', 'inventory'])).toBe('inventory')
  })

  it('prefers the higher layer over the later open', () => {
    expect(topOverlay(['settings', 'inventory'])).toBe('settings')
    expect(topOverlay(['trade', 'character'])).toBe('trade')
    expect(topOverlay(['worldMap', 'inventory'])).toBe('worldMap')
  })

  it('paints the world map over the trade window despite its lower z-index', () => {
    expect(topOverlay(['worldMap', 'trade'])).toBe('worldMap')
    expect(topOverlay(['trade', 'worldMap'])).toBe('worldMap')
  })

  it('ranks the fullscreen dialogs by their paint order', () => {
    expect(topOverlay(['loading', 'respawn'])).toBe('respawn')
    expect(topOverlay(['respawn', 'worldMap'])).toBe('worldMap')
    expect(topOverlay(['respawn', 'settings'])).toBe('settings')
  })

  it('has no top when nothing is open', () => {
    expect(topOverlay([])).toBeUndefined()
  })
})

describe('openOverlays', () => {
  it('follows the order the panels were opened in', () => {
    inventoryVisible.set(true)
    characterPanelVisible.set(true)
    expect(get(openOverlays)).toEqual(['inventory', 'character'])
  })

  it('drops a panel closed by its own button', () => {
    inventoryVisible.set(true)
    characterPanelVisible.set(true)
    inventoryVisible.set(false)
    expect(get(openOverlays)).toEqual(['character'])
  })

  it('tracks the trade window through its shop session', () => {
    shopSession.set(SESSION)
    expect(get(openOverlays)).toEqual(['trade'])
    shopSession.set(null)
    expect(get(openOverlays)).toEqual([])
  })

  it('moves the trade window back to the top when the merchant changes', () => {
    shopSession.set(SESSION)
    characterPanelVisible.set(true)
    shopSession.set({ ...SESSION, merchantPlayerId: 2 })
    expect(get(openOverlays)).toEqual(['character', 'trade'])
  })
})

describe('closeTopOverlay', () => {
  it('closes one panel per press, newest first', () => {
    inventoryVisible.set(true)
    characterPanelVisible.set(true)

    expect(closeTopOverlay()).toBe('closed')
    expect(get(characterPanelVisible)).toBe(false)
    expect(get(inventoryVisible)).toBe(true)

    expect(closeTopOverlay()).toBe('closed')
    expect(get(inventoryVisible)).toBe(false)
    expect(get(openOverlays)).toEqual([])
  })

  it.each([
    'worldMap',
    'settings',
    'respawn',
    'chatChannelMenu',
    'instrument',
  ] as const)('closes %s ahead of a panel opened before it', (id) => {
    inventoryVisible.set(true)
    let closes = 0
    const unmount = mountOverlay(id, () => closes++)

    expect(closeTopOverlay()).toBe('closed')
    expect(closes).toBe(1)
    expect(get(inventoryVisible)).toBe(true)

    unmount()
    expect(closeTopOverlay()).toBe('closed')
    expect(get(inventoryVisible)).toBe(false)
  })

  it.each(['worldMap', 'settings'] as const)(
    'still closes %s first when a panel was opened over it',
    (id) => {
      let closes = 0
      const unmount = mountOverlay(id, () => closes++)
      inventoryVisible.set(true)

      expect(closeTopOverlay()).toBe('closed')
      expect(closes).toBe(1)
      expect(get(inventoryVisible)).toBe(true)
      unmount()
    }
  )

  it('closes the trade window before a character sheet under it', () => {
    characterPanelVisible.set(true)
    shopSession.set(SESSION)

    expect(closeTopOverlay()).toBe('closed')
    expect(get(shopSession)).toBeNull()
    expect(get(characterPanelVisible)).toBe(true)

    expect(closeTopOverlay()).toBe('closed')
    expect(get(characterPanelVisible)).toBe(false)
  })

  it('never closes a panel hidden behind the loading dialog', () => {
    characterPanelVisible.set(true)
    const unmount = mountOverlay('loading')

    expect(closeTopOverlay()).toBe('blocked')
    expect(get(characterPanelVisible)).toBe(true)

    unmount()
    expect(closeTopOverlay()).toBe('closed')
    expect(get(characterPanelVisible)).toBe(false)
  })

  it('still closes the world map painted above the loading dialog', () => {
    const unmountLoading = mountOverlay('loading')
    let mapCloses = 0
    const unmountMap = mountOverlay('worldMap', () => mapCloses++)

    expect(closeTopOverlay()).toBe('closed')
    expect(mapCloses).toBe(1)

    unmountMap()
    expect(closeTopOverlay()).toBe('blocked')
    unmountLoading()
  })

  it('reports nothing to close when no overlay is open', () => {
    expect(closeTopOverlay()).toBe('none')
  })

  it('closes the emote panel like any other side panel', () => {
    emotePanelVisible.set(true)
    expect(get(openOverlays)).toEqual(['emotes'])

    expect(closeTopOverlay()).toBe('closed')
    expect(get(emotePanelVisible)).toBe(false)
    expect(get(openOverlays)).toEqual([])
  })

  it('closes the social menu before the emote panel behind it', () => {
    emotePanelVisible.set(true)
    let menuCloses = 0
    const unmount = mountOverlay('socialMenu', () => menuCloses++)

    expect(closeTopOverlay()).toBe('closed')
    expect(menuCloses).toBe(1)
    expect(get(emotePanelVisible)).toBe(true)

    unmount()
    expect(closeTopOverlay()).toBe('closed')
    expect(get(emotePanelVisible)).toBe(false)
  })

  it('closes the chat channel menu ahead of the settings dialog', () => {
    let settingsCloses = 0
    const unmountSettings = mountOverlay('settings', () => settingsCloses++)
    let menuCloses = 0
    const unmountMenu = mountOverlay('chatChannelMenu', () => menuCloses++)

    expect(closeTopOverlay()).toBe('closed')
    expect(menuCloses).toBe(1)
    expect(settingsCloses).toBe(0)
    unmountMenu()
    unmountSettings()
  })
})

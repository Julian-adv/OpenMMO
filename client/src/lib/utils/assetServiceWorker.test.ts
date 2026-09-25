import { afterEach, describe, expect, it, vi } from 'vitest'

afterEach(() => {
  vi.resetModules()
  vi.unstubAllGlobals()
})

describe('asset service worker', () => {
  it('sends unique hashed GLB and BGM URLs from the asset manifest', async () => {
    vi.stubGlobal('window', {
      __ASSET_MANIFEST__: {
        '/models/characters/knight.glb':
          '/models/characters/knight.92dadd6c.glb',
        '/models/duplicate.glb': '/models/characters/knight.92dadd6c.glb',
        '/models/objects/catalog.json': '/models/objects/catalog.12345678.json',
        '/textures/stone.png': '/textures/stone.12345678.png',
        '/models/unhashed.glb': '/models/unhashed.glb',
        '/textures/stone.glb': '/textures/stone.12345678.glb',
        '/bgm/song.mp3': '/bgm/song.12345678.mp3',
        '/bgm/song.m4a': '/bgm/song.12345678.m4a',
        '/bgm/song.ogg': '/bgm/song.12345678.ogg',
        '/bgm/unhashed.mp3': '/bgm/unhashed.mp3',
        '/sounds/hit.ogg': '/sounds/hit.12345678.ogg',
      },
    })

    const postMessage = vi.fn()
    const registration = {
      active: { postMessage },
      waiting: null,
      installing: null,
    }
    const serviceWorker = {
      register: vi.fn(async () => registration),
      addEventListener: vi.fn(),
      ready: Promise.resolve(registration),
      controller: registration.active,
    }
    vi.stubGlobal('navigator', { serviceWorker })

    const { registerAssetServiceWorker } = await import('./assetServiceWorker')
    await registerAssetServiceWorker()
    await serviceWorker.ready

    expect(serviceWorker.register).toHaveBeenCalledWith(
      '/model-service-worker.js',
      { scope: '/', updateViaCache: 'none' }
    )
    expect(postMessage).toHaveBeenCalledTimes(1)
    expect(postMessage).toHaveBeenCalledWith({
      type: 'openmmo:asset-manifest',
      urls: [
        '/bgm/song.12345678.m4a',
        '/bgm/song.12345678.mp3',
        '/bgm/song.12345678.ogg',
        '/models/characters/knight.92dadd6c.glb',
        '/textures/stone.12345678.glb',
      ],
    })
  })

  it('does nothing when service workers are unavailable', async () => {
    vi.stubGlobal('window', {})
    vi.stubGlobal('navigator', {})

    const { registerAssetServiceWorker } = await import('./assetServiceWorker')
    await expect(registerAssetServiceWorker()).resolves.toBeUndefined()
  })
})

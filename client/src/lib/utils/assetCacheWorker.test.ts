import { createHash, webcrypto } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { runInNewContext } from 'node:vm'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const origin = 'https://game.example'
const cacheName = 'openmmo-models-v1'
const model = '/models/knight.12345678.glb'
const music = '/bgm/Fields%20of%20Gold.12345678.mp3'
const source = readFileSync(
  new URL('../../../public/model-service-worker.js', import.meta.url),
  'utf8'
)

class MemoryCache {
  entries = new Map<string, Response>()
  key(request: Request | string) {
    return new URL(typeof request === 'string' ? request : request.url, origin)
      .href
  }
  async match(request: Request | string) {
    return this.entries.get(this.key(request))?.clone()
  }
  async put(request: Request | string, response: Response) {
    this.entries.set(this.key(request), response.clone())
  }
  async keys() {
    return [...this.entries.keys()].map((url) => new Request(url))
  }
  async delete(request: Request | string) {
    return this.entries.delete(this.key(request))
  }
}

type WorkerEvent = {
  request?: Request
  data?: { type: string; urls: string[] }
  waitUntil: (promise: Promise<unknown>) => void
  respondWith: (promise: Promise<Response>) => void
}

let stores: Map<string, MemoryCache>
let caches: {
  open: ReturnType<typeof vi.fn<(name: string) => Promise<MemoryCache>>>
  keys: () => Promise<string[]>
  delete: (name: string) => Promise<boolean>
}
let network: ReturnType<typeof vi.fn<(request: Request) => Promise<Response>>>
let clock: number

function networkResponse(body = '0123456789', status = 200) {
  const response = new Response(body, {
    status,
    headers: { 'Content-Type': 'audio/mpeg' },
  })
  Object.defineProperty(response, 'type', { value: 'basic' })
  return response
}

function worker(maxBytes = 500_000_000, terrainBytes = 128_000_000) {
  const handlers = new Map<string, (event: WorkerEvent) => void>()
  runInNewContext(
    source
      .replace('500_000_000', String(maxBytes))
      .replace('128_000_000', String(terrainBytes)),
    {
      self: {
        location: { origin },
        clients: { claim: async () => {} },
        skipWaiting: async () => {},
        addEventListener: (
          name: string,
          handler: (event: WorkerEvent) => void
        ) => handlers.set(name, handler),
      },
      caches,
      crypto: webcrypto,
      fetch: network,
      URL,
      Request,
      Response,
      Headers,
      Date: { now: () => ++clock },
    }
  )
  function dispatch(name: string, event: Partial<WorkerEvent> = {}) {
    const tasks: Promise<unknown>[] = []
    let response: Promise<Response> | undefined
    handlers.get(name)!({
      ...event,
      waitUntil: (promise) => tasks.push(promise),
      respondWith: (promise) => {
        response = promise
      },
    })
    return { response, done: Promise.all(tasks) }
  }
  return {
    dispatch,
    async get(path: string, headers?: HeadersInit) {
      const event = dispatch('fetch', {
        request: new Request(new URL(path, origin), { headers }),
      })
      const [response] = await Promise.all([event.response, event.done])
      return response
    },
    async manifest(urls: string[], type = 'openmmo:asset-manifest') {
      await dispatch('message', { data: { type, urls } }).done
    },
  }
}

beforeEach(() => {
  clock = 0
  stores = new Map()
  caches = {
    open: vi.fn(async (name: string) => {
      if (!stores.has(name)) stores.set(name, new MemoryCache())
      return stores.get(name)!
    }),
    keys: async () => [...stores.keys()],
    delete: async (name: string) => stores.delete(name),
  }
  network = vi.fn(async () => networkResponse())
})

describe('asset cache capacity', () => {
  const third = '/textures/stone.12345678.glb'

  it('keeps files at the limit and evicts the least recently used file on overflow', async () => {
    const current = worker(20)
    await current.get(model)
    await current.get(music)
    const cache = await caches.open(cacheName)
    expect(await cache.keys()).toHaveLength(2)
    await current.get(model)
    await current.get(third)
    expect(await cache.match(model)).toBeDefined()
    expect(await cache.match(music)).toBeUndefined()
    expect(await cache.match(third)).toBeDefined()
    expect(network).toHaveBeenCalledTimes(3)
  })

  it('retains recency across worker restarts, including range playback', async () => {
    const current = worker(20)
    await current.get(model)
    await current.get(music)
    await current.get(model, { Range: 'bytes=0-1' })
    await worker(20).get(third)
    const cache = await caches.open(cacheName)
    expect(await cache.match(model)).toBeDefined()
    expect(await cache.match(music)).toBeUndefined()
  })

  it('measures and trims preexisting caches without size metadata on activation', async () => {
    const cache = await caches.open(cacheName)
    await cache.put(model, new Response('0123456789'))
    await cache.put(music, new Response('0123456789'))
    await worker(15).dispatch('activate').done
    expect(await cache.match(model)).toBeUndefined()
    expect(await cache.match(music)).toBeDefined()
    expect(network).not.toHaveBeenCalled()
  })

  it('serializes simultaneous stores to stay within the combined budget', async () => {
    const current = worker(20)
    await Promise.all([
      current.get(model),
      current.get(music),
      current.get(third),
    ])
    const cache = await caches.open(cacheName)
    expect(await cache.keys()).toHaveLength(2)
  })

  it('plays an oversized file without evicting or caching other files', async () => {
    const current = worker(20)
    await current.get(model)
    network.mockImplementationOnce(async () => networkResponse('x'.repeat(21)))
    expect(await (await current.get(music))?.text()).toBe('x'.repeat(21))
    const cache = await caches.open(cacheName)
    expect(await cache.match(model)).toBeDefined()
    expect(await cache.match(music)).toBeUndefined()
  })

  it('accounts for manifest deletion before subsequent downloads', async () => {
    const current = worker(20)
    await current.get(model)
    await current.get(music)
    await current.manifest([music, third])
    await current.get(third)
    const cache = await caches.open(cacheName)
    expect(await cache.match(music)).toBeDefined()
    expect(await cache.match(third)).toBeDefined()
  })

  it('stores a requested file again if it was removed outside the worker', async () => {
    const current = worker(20)
    await current.get(model)
    const cache = await caches.open(cacheName)
    await cache.delete(model)
    await current.get(model)
    expect(await cache.match(model)).toBeDefined()
    expect(network).toHaveBeenCalledTimes(2)
  })
})

describe('shared asset worker', () => {
  it.each([
    model,
    music,
    '/bgm/song.12345678.m4a',
    '/bgm/song.12345678.ogg',
    '/textures/stone.12345678.glb',
  ])('persists %s across worker restarts', async (path) => {
    expect(await (await worker().get(path))?.text()).toBe('0123456789')
    network.mockRejectedValue(new Error('offline'))
    expect(await (await worker().get(path))?.text()).toBe('0123456789')
    expect(network).toHaveBeenCalledTimes(1)
  })

  it('reuses the deployed model cache without fetching again', async () => {
    await (await caches.open(cacheName)).put(model, new Response('old model'))
    const current = worker()
    await current.dispatch('activate').done
    expect(await (await current.get(model))?.text()).toBe('old model')
    expect(network).not.toHaveBeenCalled()
  })

  it('shares a simultaneous full-file download', async () => {
    let finish!: (response: Response) => void
    network.mockImplementationOnce(
      () => new Promise((resolve) => (finish = resolve))
    )
    const current = worker()
    const a = current.get(music)
    const b = current.get(music)
    await vi.waitFor(() => expect(network).toHaveBeenCalledTimes(1))
    finish(networkResponse())
    const responses = await Promise.all([a, b])
    expect(await responses[0]?.text()).toBe('0123456789')
    expect(await responses[1]?.text()).toBe('0123456789')
    expect(network).toHaveBeenCalledTimes(1)
  })

  it.each(['open', 'read', 'write'])(
    'plays network data when cache %s fails',
    async (operation) => {
      const cache = await caches.open(cacheName)
      if (operation === 'open')
        caches.open.mockRejectedValue(new Error('denied'))
      if (operation === 'read')
        vi.spyOn(cache, 'match').mockRejectedValue(new Error('unavailable'))
      if (operation === 'write')
        vi.spyOn(cache, 'put').mockRejectedValue(new Error('quota exceeded'))
      expect(await (await worker().get(music))?.text()).toBe('0123456789')
    }
  )

  it.each([404, 206])(
    'does not persist an HTTP %s response',
    async (status) => {
      network.mockImplementation(async () => networkResponse('partial', status))
      const current = worker()
      await current.get(music)
      await current.get(music)
      expect(network).toHaveBeenCalledTimes(2)
      expect(await (await caches.open(cacheName)).keys()).toHaveLength(0)
    }
  )

  it('retries after a network failure', async () => {
    network.mockRejectedValueOnce(new Error('offline'))
    const current = worker()
    await expect(current.get(music)).rejects.toThrow('offline')
    expect(await (await current.get(music))?.text()).toBe('0123456789')
    expect(network).toHaveBeenCalledTimes(2)
  })

  it.each([
    '/models/knight.glb',
    '/bgm/song.mp3',
    '/api/terrain/height',
    '/textures/stone.12345678.png',
    'https://elsewhere.example/models/knight.12345678.glb',
  ])('leaves unrelated or mutable requests untouched: %s', async (path) => {
    expect(await worker().get(path)).toBeUndefined()
    expect(network).not.toHaveBeenCalled()
  })

  it('retains current and previous manifests across serialized updates', async () => {
    const oldest = '/models/knight.aaaaaaaa.glb'
    const previous = '/models/knight.bbbbbbbb.glb'
    const cache = await caches.open(cacheName)
    for (const url of [oldest, previous, model, music])
      await cache.put(url, new Response(url))
    const current = worker()
    await current.manifest([oldest])
    await cache.put(previous, new Response(previous))
    await cache.put(model, new Response(model))
    await cache.put(music, new Response(music))
    await Promise.all([
      current.manifest([previous, music]),
      current.manifest([model, music]),
    ])
    expect(await cache.match(oldest)).toBeUndefined()
    expect(await cache.match(previous)).toBeDefined()
    const metadata = await caches.open('openmmo-model-manifests-v1')
    const response = await metadata.match('/__openmmo_model_manifest__')
    const state = await response!.json()
    expect(state.current).toEqual([music, model].map((url) => origin + url))
    expect(state.previous).toEqual([music, previous].map((url) => origin + url))
  })

  it('ignores old model-only messages so older tabs cannot evict music', async () => {
    const current = worker()
    await current.manifest([model, music])
    await current.get(music)
    await current.manifest([model], 'openmmo:model-manifest')
    await current.manifest(
      ['/models/other.aaaaaaaa.glb'],
      'openmmo:model-manifest'
    )
    expect(await (await caches.open(cacheName)).match(music)).toBeDefined()
  })
})

describe('cached music seeking', () => {
  it.each([
    ['bytes=2-5', '2345', 'bytes 2-5/10'],
    ['bytes=7-', '789', 'bytes 7-9/10'],
    ['bytes=-3', '789', 'bytes 7-9/10'],
    ['bytes=8-99', '89', 'bytes 8-9/10'],
  ])(
    'serves %s from a complete cached track',
    async (range, body, contentRange) => {
      const current = worker()
      await current.get(music)
      const response = await current.get(music, { Range: range })
      expect(response?.status).toBe(206)
      expect(response?.headers.get('Content-Range')).toBe(contentRange)
      expect(response?.headers.get('Content-Length')).toBe(String(body.length))
      expect(response?.headers.get('Content-Type')).toBe('audio/mpeg')
      expect(await response?.text()).toBe(body)
      expect(network).toHaveBeenCalledTimes(1)
    }
  )

  it('returns 416 for an unsatisfiable cached range', async () => {
    const current = worker()
    await current.get(music)
    const response = await current.get(music, { Range: 'bytes=10-' })
    expect(response?.status).toBe(416)
    expect(response?.headers.get('Content-Range')).toBe('bytes */10')
    expect(network).toHaveBeenCalledTimes(1)
  })

  it('streams an uncached range and stores a separate complete response', async () => {
    network.mockImplementation(async (request) =>
      request.headers.has('Range')
        ? networkResponse('01', 206)
        : networkResponse()
    )
    const current = worker()
    const response = await current.get(music, { Range: 'bytes=0-1' })
    expect(response?.status).toBe(206)
    expect(await response?.text()).toBe('01')
    expect(network.mock.calls[0][0].headers.get('Range')).toBe('bytes=0-1')
    expect(network.mock.calls[1][0].headers.has('Range')).toBe(false)
    const cached = await (await caches.open(cacheName)).match(music)
    expect(cached?.status).toBe(200)
    expect(await cached?.text()).toBe('0123456789')
    expect(
      await (await current.get(music, { Range: 'bytes=7-' }))?.text()
    ).toBe('789')
    expect(network).toHaveBeenCalledTimes(2)
  })

  it('serves ranges immediately while sharing a full download with other requests', async () => {
    let finish!: (response: Response) => void
    network.mockImplementation((request) =>
      request.headers.has('Range')
        ? Promise.resolve(networkResponse('01', 206))
        : new Promise((resolve) => (finish = resolve))
    )
    const current = worker()
    const first = current.dispatch('fetch', {
      request: new Request(origin + music, { headers: { Range: 'bytes=0-1' } }),
    })
    expect(await (await first.response)?.text()).toBe('01')
    const second = current.dispatch('fetch', {
      request: new Request(origin + music, { headers: { Range: 'bytes=0-1' } }),
    })
    expect(await (await second.response)?.text()).toBe('01')
    const whole = current.get(music)
    await Promise.resolve()
    expect(
      network.mock.calls.filter(([request]) => !request.headers.has('Range'))
    ).toHaveLength(1)
    finish(networkResponse())
    await Promise.all([first.done, second.done])
    expect(await (await whole)?.text()).toBe('0123456789')
    expect(network).toHaveBeenCalledTimes(3)
  })

  it.each(['network', 'http', 'partial'])(
    'keeps range playback working and retries a failed full download: %s',
    async (failure) => {
      let fullRequests = 0
      network.mockImplementation(async (request) => {
        if (request.headers.has('Range')) return networkResponse('01', 206)
        if (++fullRequests === 1) {
          if (failure === 'network') throw new Error('offline')
          return networkResponse('failed', failure === 'http' ? 500 : 206)
        }
        return networkResponse()
      })
      const current = worker()
      expect((await current.get(music, { Range: 'bytes=0-1' }))?.status).toBe(
        206
      )
      expect(await (await caches.open(cacheName)).match(music)).toBeUndefined()
      await current.get(music, { Range: 'bytes=0-1' })
      expect((await (await caches.open(cacheName)).match(music))?.status).toBe(
        200
      )
      expect(fullRequests).toBe(2)
    }
  )

  it('removes range and conditional headers only from the full download', async () => {
    network.mockImplementation(async (request) =>
      request.headers.has('Range')
        ? networkResponse('01', 206)
        : networkResponse()
    )
    await worker().get(music, {
      Range: 'bytes=0-1',
      'If-Range': '"old"',
      'If-None-Match': '"etag"',
      'If-Modified-Since': 'Wed, 09 Sep 2026 00:00:00 GMT',
    })
    expect(network.mock.calls[0][0].headers.get('If-Range')).toBe('"old"')
    for (const header of [
      'Range',
      'If-Range',
      'If-None-Match',
      'If-Modified-Since',
    ])
      expect(network.mock.calls[1][0].headers.has(header)).toBe(false)
  })

  it('does not fetch a full file when Cache Storage is unavailable', async () => {
    caches.open.mockRejectedValue(new Error('denied'))
    network.mockImplementation(async () => networkResponse('01', 206))
    expect((await worker().get(music, { Range: 'bytes=0-1' }))?.status).toBe(
      206
    )
    expect(network).toHaveBeenCalledTimes(1)
  })

  it.each([
    new Headers({ Range: 'bytes=0-1,4-5' }),
    new Headers({ Range: 'bytes=0-1', 'If-Range': '"old-etag"' }),
  ])(
    'forwards complex or conditional ranges to the server',
    async (headers) => {
      const current = worker()
      await current.get(music)
      await current.get(music, headers)
      expect(network).toHaveBeenCalledTimes(2)
    }
  )
})

describe('terrain Cache Storage', () => {
  const terrainCache = 'openmmo-terrain-files-v1'
  const hash = (body: string) => createHash('sha256').update(body).digest('hex')
  const url = (body: string, x = '0000') =>
    `/api/terrain/files/grass/r+00_+00/g_+${x}_+0000.bin?hash=${hash(body)}`
  const key = (body: string) => `/__openmmo_terrain_file__/${hash(body)}`

  it('reuses verified raw bytes after worker restarts and asset manifest cleanup', async () => {
    const current = worker()
    await current.get(url('0123456789'))
    await current.manifest([model])
    await current.manifest([music])
    const restarted = worker()
    expect(await (await restarted.get(url('0123456789', '0001')))!.text()).toBe(
      '0123456789'
    )
    expect(network).toHaveBeenCalledTimes(1)
    expect(network.mock.calls[0][0].cache).toBe('no-store')
    expect(await (await caches.open(terrainCache)).keys()).toHaveLength(1)
  })

  it('never stores a response with the wrong hash and fetches the latest version', async () => {
    const current = worker()
    expect((await current.get(url('old')))?.status).toBe(409)
    expect(await (await caches.open(terrainCache)).keys()).toHaveLength(0)
    expect((await current.get(url('0123456789')))?.status).toBe(200)
    expect(network).toHaveBeenCalledTimes(2)
  })

  it('redownloads corrupted stored content and works when storage is unavailable', async () => {
    const cache = await caches.open(terrainCache)
    await cache.put(key('0123456789'), new Response('corrupt'))
    expect((await worker().get(url('0123456789')))?.status).toBe(200)
    expect(await (await cache.match(key('0123456789')))!.text()).toBe(
      '0123456789'
    )
    caches.open.mockRejectedValue(new Error('unavailable'))
    expect((await worker().get(url('0123456789')))?.status).toBe(200)
    expect(network).toHaveBeenCalledTimes(2)
  })

  it('keeps terrain capacity separate and persists least recently used eviction', async () => {
    const current = worker(20, 20)
    for (const body of ['aaaaaaaaaa', 'bbbbbbbbbb']) {
      network.mockImplementationOnce(async () => networkResponse(body))
      await current.get(url(body))
    }
    await current.get(url('aaaaaaaaaa'))
    await current.get(model)
    network.mockImplementationOnce(async () => networkResponse('cccccccccc'))
    await worker(20, 20).get(url('cccccccccc'))
    const cache = await caches.open(terrainCache)
    expect(await cache.match(key('aaaaaaaaaa'))).toBeDefined()
    expect(await cache.match(key('bbbbbbbbbb'))).toBeUndefined()
    expect(await cache.match(key('cccccccccc'))).toBeDefined()
    expect(await (await caches.open(cacheName)).match(model)).toBeDefined()
  })

  it('deduplicates simultaneous downloads by content hash across tile paths', async () => {
    const current = worker()
    const responses = await Promise.all([
      current.get(url('0123456789')),
      current.get(url('0123456789', '0001')),
    ])
    expect(responses.map((response) => response?.status)).toEqual([200, 200])
    expect(network).toHaveBeenCalledTimes(1)
  })
})

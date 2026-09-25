const ASSET_CACHE = 'openmmo-models-v1'
const MANIFEST_CACHE = 'openmmo-model-manifests-v1'
const MANIFEST_KEY = '/__openmmo_model_manifest__'
const MAX_CACHE_BYTES = 500_000_000
const MANIFEST_MESSAGE = 'openmmo:asset-manifest'
const OWNED_CACHE_PREFIX = 'openmmo-model'
const HASHED_ASSET_URL =
  /^\/(?:(?:models|textures)\/.+\.[0-9a-f]{8}\.glb|bgm\/.+\.[0-9a-f]{8}\.(?:mp3|m4a|ogg))$/i
const TERRAIN_URL =
  /^\/api\/terrain\/files\/(height|splat|trees|grass|landscaping)\/r[+-]\d+_[+-]\d+\/[hstgl]_[+-]\d+_[+-]\d+\.bin$/
const assetStore = {
  name: ASSET_CACHE,
  limit: MAX_CACHE_BYTES,
  indexKey: '/__openmmo_asset_cache_index__',
}
const terrainStore = {
  name: 'openmmo-terrain-files-v1',
  limit: 128_000_000,
  indexKey: '/__openmmo_terrain_cache_index__',
}
const inFlight = new Map()
let cacheUpdates = Promise.resolve()

function updateCache(operation) {
  cacheUpdates = cacheUpdates.then(operation).catch(() => {})
  return cacheUpdates
}

self.addEventListener('install', () => self.skipWaiting())
self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            .filter(
              (name) =>
                name.startsWith(OWNED_CACHE_PREFIX) &&
                name !== ASSET_CACHE &&
                name !== MANIFEST_CACHE
            )
            .map((name) => caches.delete(name))
        )
      )
      .then(() =>
        updateCache(async () => {
          for (const store of [assetStore, terrainStore]) {
            const cache = await caches.open(store.name)
            await loadCacheIndex(cache, store)
            await trimCache(cache, store.limit, store)
            await saveCacheIndex(store)
          }
        })
      )
      .then(() => self.clients.claim())
  )
})

self.addEventListener('fetch', (event) => {
  const request = event.request
  const url = new URL(request.url)
  if (request.method !== 'GET') return
  const terrain =
    TERRAIN_URL.test(url.pathname) &&
    /^[0-9a-f]{64}$/.test(url.searchParams.get('hash') ?? '') &&
    !request.headers.has('range')
  if (
    !terrain &&
    (url.origin !== self.location.origin ||
      !HASHED_ASSET_URL.test(url.pathname))
  )
    return
  const operation = resolveAsset(request, terrain ? terrainStore : assetStore)
  event.respondWith(operation.then(({ response }) => response))
  event.waitUntil(operation.then(({ cacheWrite }) => cacheWrite))
})

self.addEventListener('message', (event) => {
  if (event.data?.type !== MANIFEST_MESSAGE) return
  event.waitUntil(updateCache(() => updateManifest(event.data.urls)))
})

function cacheKey(request, store) {
  if (store !== terrainStore) return request
  const url = new URL(request.url)
  return new Request(
    new URL(
      `/__openmmo_terrain_file__/${url.searchParams.get('hash')}`,
      url.origin
    )
  )
}

async function verifyTerrain(response, request) {
  const digest = await crypto.subtle.digest(
    'SHA-256',
    await response.clone().arrayBuffer()
  )
  const hash = Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, '0')
  ).join('')
  return hash === new URL(request.url).searchParams.get('hash')
}

async function resolveAsset(request, store) {
  let cache
  let hasCachedFile = false
  const key = cacheKey(request, store)
  try {
    cache = await caches.open(store.name)
    let cached = await cache.match(key)
    if (
      cached &&
      store === terrainStore &&
      !(await verifyTerrain(cached, request))
    ) {
      await cache.delete(key)
      cached = null
    }
    if (cached) {
      hasCachedFile = true
      const response = request.headers.has('range')
        ? await readRange(request, cached)
        : cached
      if (response) {
        const usedAt = Date.now()
        const cacheWrite = updateCache(async () => {
          await loadCacheIndex(cache, store)
          const entry = store.index.get(key.url)
          if (entry) entry.lastUsed = Math.max(entry.lastUsed, usedAt)
          await trimCache(cache, store.limit, store)
          await saveCacheIndex(store)
        })
        return { response, cacheWrite }
      }
    }
  } catch {
    cache = null
  }

  if (request.headers.has('range')) {
    const streaming = fetch(request)
    let cacheWrite = Promise.resolve()
    if (cache && !hasCachedFile) {
      const headers = new Headers(request.headers)
      for (const name of [
        'range',
        'if-range',
        'if-none-match',
        'if-modified-since',
        'if-match',
        'if-unmodified-since',
      ])
        headers.delete(name)
      const fullRequest = new Request(request, { headers, signal: null })
      cacheWrite = downloadAsset(fullRequest, cache, store)
        .then(({ cacheWrite }) => cacheWrite)
        .catch(() => {})
    }
    return { response: await streaming, cacheWrite }
  }
  const { response, cacheWrite } = await downloadAsset(request, cache, store)
  return { response: response.clone(), cacheWrite }
}

function downloadAsset(request, cache, store) {
  const key = cacheKey(request, store).url
  let network = inFlight.get(key)
  if (!network) {
    network = fetchAndCache(request, cache, store)
    inFlight.set(key, network)
    void network
      .then(({ cacheWrite }) => cacheWrite)
      .catch(() => {})
      .finally(() => inFlight.delete(key))
  }
  return network
}

async function readRange(request, cached) {
  if (request.headers.has('if-range')) return null
  const match = /^bytes=(\d*)-(\d*)$/i.exec(request.headers.get('range'))
  if (!match || (!match[1] && !match[2])) return null
  const blob = await cached.blob()
  const start = match[1]
    ? Number(match[1])
    : Math.max(0, blob.size - Number(match[2]))
  const end = match[1] && match[2] ? Number(match[2]) : blob.size - 1
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end)) return null
  if (start >= blob.size || end < start) {
    return new Response(null, {
      status: 416,
      headers: { 'Content-Range': `bytes */${blob.size}` },
    })
  }
  const last = Math.min(end, blob.size - 1)
  const headers = new Headers(cached.headers)
  headers.delete('Content-Encoding')
  headers.set('Content-Range', `bytes ${start}-${last}/${blob.size}`)
  headers.set('Content-Length', String(last - start + 1))
  headers.set('Accept-Ranges', 'bytes')
  return new Response(blob.slice(start, last + 1), { status: 206, headers })
}

async function fetchAndCache(request, cache, store) {
  const response = await fetch(
    store === terrainStore
      ? new Request(request, { cache: 'no-store' })
      : request
  )
  let cacheWrite = Promise.resolve()
  if (
    store === terrainStore &&
    response.status === 200 &&
    !(await verifyTerrain(response, request))
  ) {
    return {
      response: new Response(null, {
        status: 409,
        headers: { 'Cache-Control': 'no-store' },
      }),
      cacheWrite,
    }
  }
  if (
    cache &&
    response.status === 200 &&
    (response.type === 'basic' ||
      (store === terrainStore && response.type === 'cors'))
  ) {
    cacheWrite = storeAsset(
      cache,
      cacheKey(request, store),
      response.clone(),
      store
    ).catch(() => {})
  }
  return { response, cacheWrite }
}

async function loadCacheIndex(cache, store) {
  if (store.index) return
  let saved
  try {
    const metadata = await caches.open(MANIFEST_CACHE)
    saved = new Map(await (await metadata.match(store.indexKey)).json())
  } catch {
    saved = new Map()
  }
  const entries = new Map()
  for (const request of await cache.keys()) {
    const entry = saved.get(request.url)
    if (
      Number.isSafeInteger(entry?.size) &&
      entry.size >= 0 &&
      Number.isFinite(entry?.lastUsed) &&
      entry.lastUsed >= 0
    ) {
      entries.set(request.url, entry)
    } else {
      const response = await cache.match(request)
      if (response)
        entries.set(request.url, {
          size: (await response.blob()).size,
          lastUsed: 0,
        })
    }
  }
  store.index = entries
}

async function saveCacheIndex(store) {
  const metadata = await caches.open(MANIFEST_CACHE)
  await metadata.put(
    store.indexKey,
    new Response(JSON.stringify([...store.index]))
  )
}

async function trimCache(cache, limit, store) {
  let total = [...store.index.values()].reduce(
    (sum, entry) => sum + entry.size,
    0
  )
  if (total <= limit) return
  const oldest = [...store.index].sort((a, b) => a[1].lastUsed - b[1].lastUsed)
  for (const [url, entry] of oldest) {
    if (total <= limit) break
    await cache.delete(url)
    store.index.delete(url)
    total -= entry.size
  }
}

async function storeAsset(cache, request, response, store) {
  const size = (await response.clone().blob()).size
  if (size > store.limit) return
  const usedAt = Date.now()
  await updateCache(async () => {
    await loadCacheIndex(cache, store)
    const previous = store.index.get(request.url)
    if (previous && (await cache.match(request))) {
      previous.lastUsed = Math.max(previous.lastUsed, usedAt)
    } else {
      store.index.delete(request.url)
      await trimCache(cache, store.limit - size, store)
      try {
        await cache.put(request, response)
      } catch (error) {
        await saveCacheIndex(store)
        throw error
      }
      store.index.set(request.url, { size, lastUsed: usedAt })
    }
    await saveCacheIndex(store)
  })
}

async function updateManifest(rawUrls) {
  const urls = normalizeManifest(rawUrls)
  if (urls.length === 0) return
  const manifestCache = await caches.open(MANIFEST_CACHE)
  const previousState = await readManifestState(manifestCache)
  const known =
    sameManifest(urls, previousState.current) ||
    sameManifest(urls, previousState.previous)
  const state = known
    ? previousState
    : { current: urls, previous: previousState.current }
  if (!known) {
    await manifestCache.put(
      MANIFEST_KEY,
      new Response(JSON.stringify(state), {
        headers: { 'Content-Type': 'application/json' },
      })
    )
  }
  const keep = new Set([...state.current, ...state.previous])
  const cache = await caches.open(ASSET_CACHE)
  await loadCacheIndex(cache, assetStore)
  const requests = await cache.keys()
  await Promise.all(
    requests
      .filter((request) => !keep.has(request.url))
      .map(async (request) => {
        await cache.delete(request)
        assetStore.index.delete(request.url)
      })
  )
  await trimCache(cache, assetStore.limit, assetStore)
  await saveCacheIndex(assetStore)
}
function normalizeManifest(rawUrls) {
  if (!Array.isArray(rawUrls)) return []

  const urls = rawUrls.flatMap((rawUrl) => {
    if (typeof rawUrl !== 'string') return []
    try {
      const url = new URL(rawUrl, self.location.origin)
      if (
        url.origin !== self.location.origin ||
        !HASHED_ASSET_URL.test(url.pathname)
      ) {
        return []
      }
      return [url.href]
    } catch {
      return []
    }
  })

  return [...new Set(urls)].sort()
}

async function readManifestState(cache) {
  try {
    const response = await cache.match(MANIFEST_KEY)
    if (!response) return { current: [], previous: [] }
    const state = await response.json()
    return {
      current: normalizeManifest(state.current),
      previous: normalizeManifest(state.previous),
    }
  } catch {
    return { current: [], previous: [] }
  }
}

function sameManifest(left, right) {
  return (
    left.length === right.length &&
    left.every((url, index) => url === right[index])
  )
}

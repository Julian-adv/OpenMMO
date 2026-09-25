import { DefaultLoadingManager } from 'three'

// Production embeds content hashes; development uses the original paths.
declare global {
  interface Window {
    __ASSET_MANIFEST__?: Record<string, string>
  }
}

const manifest = new Map(
  Object.entries(
    (typeof window !== 'undefined' && window.__ASSET_MANIFEST__) || {}
  )
)

const HASHED_ASSET_URL =
  /^\/(?:(?:models|textures)\/.+\.[0-9a-f]{8}\.glb|bgm\/.+\.[0-9a-f]{8}\.(?:mp3|m4a|ogg))$/i

export function assetUrl(path: string): string {
  return manifest.get(path) ?? path
}

export function getCachedAssetUrls(): string[] {
  return [...new Set(manifest.values())]
    .filter((url) => HASHED_ASSET_URL.test(url))
    .sort()
}

if (manifest.size > 0) DefaultLoadingManager.setURLModifier(assetUrl)

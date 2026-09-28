import { createHash } from 'node:crypto'
import { vi } from 'vitest'
import type { TerrainFile } from './terrainFiles'

export function stubSha256Digest() {
  vi.stubGlobal('crypto', {
    subtle: {
      digest: async (_algorithm: string, bytes: ArrayBuffer) => {
        const digest = createHash('sha256')
          .update(new Uint8Array(bytes))
          .digest()
        return digest.buffer.slice(
          digest.byteOffset,
          digest.byteOffset + digest.byteLength
        )
      },
    },
  })
}

export const terrainFile = (path: string, bytes: Uint8Array): TerrainFile => ({
  path,
  hash: createHash('sha256').update(bytes).digest('hex'),
})

export function landscapeBytes() {
  const landscape = new Uint8Array(4 + 16384 + 512)
  landscape.set([76, 78, 68, 49]) // "LND1"
  landscape[4] = 5
  landscape[4 + 16384] = 128
  return landscape
}

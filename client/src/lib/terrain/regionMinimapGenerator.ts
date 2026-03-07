import { getTerrainApiUrl } from '../utils/networkUtils'
import { DEEP_WATER_THRESHOLD } from './terrainGenerator'
import type { TerrainMetaManager } from '../managers/terrainMetaManager'

const REGION_SIZE = 16
const TILE_DIM = 64
const VERTS_PER_SIDE = TILE_DIM + 1
const MAP_PX = REGION_SIZE * TILE_DIM // 1024

/** Texture name → minimap RGB color */
const TEXTURE_COLORS: Record<string, [number, number, number]> = {
  rocky_terrain_02_1k: [80, 140, 50], // green
  sandy_gravel_02_1k: [194, 178, 128], // sand
  snow_02_1k: [240, 240, 245], // white
  gravel_floor_1k: [160, 150, 130], // gray-brown
  red_laterite_soil_stones_1k: [180, 100, 60], // reddish-brown
  gravel_road_1k: [140, 135, 125], // gray
}

const COLOR_SHALLOW_WATER: [number, number, number] = [100, 160, 220]
const COLOR_DEEP_WATER: [number, number, number] = [30, 60, 150]
const COLOR_FALLBACK: [number, number, number] = [120, 120, 100]

function decodeHeight(value: number): number {
  return value * 0.05 - 500.0
}

export async function generateRegionMinimap(
  rx: number,
  rz: number,
  metaManager: TerrainMetaManager,
  onProgress?: (pct: number, label: string) => void
): Promise<Blob> {
  const apiUrl = getTerrainApiUrl()
  const meta = await metaManager.fetchMeta(rx, rz)

  // Build per-channel color from region meta
  const channelColors: [number, number, number][] = meta.layers.map(
    (layer) => TEXTURE_COLORS[layer.texture] ?? COLOR_FALLBACK
  )

  // Fetch all tiles' height + splat data
  const heightmaps = new Map<string, Uint16Array>()
  const splatmaps = new Map<string, Uint8Array>()

  const BATCH_SIZE = 16
  const allCoords: { tx: number; tz: number }[] = []
  for (let lz = 0; lz < REGION_SIZE; lz++) {
    for (let lx = 0; lx < REGION_SIZE; lx++) {
      allCoords.push({ tx: rx * REGION_SIZE + lx, tz: rz * REGION_SIZE + lz })
    }
  }

  for (let i = 0; i < allCoords.length; i += BATCH_SIZE) {
    const batch = allCoords.slice(i, i + BATCH_SIZE)
    await Promise.all(
      batch.flatMap(({ tx, tz }) => {
        const key = `${tx},${tz}`
        return [
          fetch(`${apiUrl}/api/terrain/height/${tx}/${tz}`)
            .then((r) => r.arrayBuffer())
            .then((buf) => heightmaps.set(key, new Uint16Array(buf)))
            .catch(() => {}),
          fetch(`${apiUrl}/api/terrain/splat/${tx}/${tz}`)
            .then((r) => r.arrayBuffer())
            .then((buf) => splatmaps.set(key, new Uint8Array(buf)))
            .catch(() => {}),
        ]
      })
    )
    const pct = Math.round(((i + batch.length) / allCoords.length) * 80)
    onProgress?.(
      pct,
      `Loading tiles... ${i + batch.length}/${allCoords.length}`
    )
  }

  onProgress?.(80, 'Rendering minimap...')
  await new Promise((r) => requestAnimationFrame(r))

  // Generate pixel data
  const canvas = document.createElement('canvas')
  canvas.width = MAP_PX
  canvas.height = MAP_PX
  const ctx = canvas.getContext('2d')!
  const imageData = ctx.createImageData(MAP_PX, MAP_PX)
  const pixels = imageData.data

  for (let lz = 0; lz < REGION_SIZE; lz++) {
    for (let lx = 0; lx < REGION_SIZE; lx++) {
      const tx = rx * REGION_SIZE + lx
      const tz = rz * REGION_SIZE + lz
      const key = `${tx},${tz}`
      const heightData = heightmaps.get(key)
      const splatData = splatmaps.get(key)

      for (let cz = 0; cz < TILE_DIM; cz++) {
        for (let cx = 0; cx < TILE_DIM; cx++) {
          const pixX = lx * TILE_DIM + cx
          const pixY = lz * TILE_DIM + cz
          const pixIdx = (pixY * MAP_PX + pixX) * 4

          // Height check
          let height = 0
          if (heightData) {
            height = decodeHeight(heightData[cz * VERTS_PER_SIDE + cx])
          }

          let r: number, g: number, b: number

          if (height < DEEP_WATER_THRESHOLD) {
            ;[r, g, b] = COLOR_DEEP_WATER
          } else if (height < -0.4) {
            ;[r, g, b] = COLOR_SHALLOW_WATER
          } else if (splatData) {
            // Find dominant splat channel
            const splatIdx = (cz * TILE_DIM + cx) * 4
            let maxVal = -1
            let maxCh = 0
            for (let ch = 0; ch < 4; ch++) {
              if (splatData[splatIdx + ch] > maxVal) {
                maxVal = splatData[splatIdx + ch]
                maxCh = ch
              }
            }
            ;[r, g, b] = channelColors[maxCh]
          } else {
            ;[r, g, b] = COLOR_FALLBACK
          }

          pixels[pixIdx] = r
          pixels[pixIdx + 1] = g
          pixels[pixIdx + 2] = b
          pixels[pixIdx + 3] = 255
        }
      }
    }
  }

  ctx.putImageData(imageData, 0, 0)

  onProgress?.(90, 'Encoding PNG...')

  const blob = await new Promise<Blob>((resolve, reject) => {
    canvas.toBlob((b) => {
      if (b) resolve(b)
      else reject(new Error('Failed to encode PNG'))
    }, 'image/png')
  })

  onProgress?.(95, 'Uploading to server...')

  await fetch(`${apiUrl}/api/terrain/minimap/${rx}/${rz}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'image/png' },
    body: blob,
  })

  return blob
}

/** Build the server URL for a region minimap (HTTP-cacheable). */
export function regionMinimapServerUrl(rx: number, rz: number): string {
  return `${getTerrainApiUrl()}/api/terrain/minimap/${rx}/${rz}`
}

import { getTerrainApiUrl } from '../utils/networkUtils'
import {
  DEEP_WATER_THRESHOLD,
  REGION_SIZE,
  TILE_DIM,
  VERTS_PER_SIDE,
} from './terrain-constants'
import { MAX_PALETTE, unpackPrimary, unpackSecondary } from './splat-encoding'
import { GLOBAL_PALETTE } from '../utils/splatLayerLoader'
import type { HouseData } from '../types/housing'

/** Height above which shore holes in water shader reveal sand underneath (~0.2-0.25m depth) */
const VISIBLE_SAND_THRESHOLD = -0.25

const MAP_PX = REGION_SIZE * TILE_DIM // 1024

/** Texture name → minimap RGB color */
const TEXTURE_COLORS: Record<string, [number, number, number]> = {
  rocky_terrain_02_1k: [80, 140, 50], // green (grass)
  sandy_gravel_02_1k: [210, 185, 110], // sand
  snow_02_1k: [240, 240, 245], // white
  gravel_floor_1k: [160, 150, 130], // gray-brown
  red_laterite_soil_stones_1k: [180, 100, 60], // reddish-brown
  gravel_road_1k: [140, 135, 125], // gray
  patterned_paving_02_1k: [235, 225, 205], // pale beige (road)
  marble_cliff_01_1k: [200, 195, 185], // light gray marble (cliff face)
  ganges_river_pebbles_1k: [110, 100, 85], // wet river pebbles
}

const COLOR_SHALLOW_WATER: [number, number, number] = [100, 160, 220]
const COLOR_DEEP_WATER: [number, number, number] = [30, 60, 150]
const COLOR_FALLBACK: [number, number, number] = [120, 120, 100]
const COLOR_BUILDING: [number, number, number] = [220, 140, 40]

function decodeHeight(value: number): number {
  return value * 0.05 - 500.0
}

export async function generateRegionMinimap(
  rx: number,
  rz: number,
  onProgress?: (pct: number, label: string) => void
): Promise<Blob> {
  const apiUrl = getTerrainApiUrl()

  // Global palette is identical for every region, so the per-slot color table
  // is a one-time derivation. Pre-pad to MAX_PALETTE so the hot pixel loop can
  // skip bounds / fallback checks.
  const channelColors: [number, number, number][] = new Array(MAX_PALETTE)
    .fill(null)
    .map(() => COLOR_FALLBACK)
  GLOBAL_PALETTE.forEach((layer, i) => {
    channelColors[i] = TEXTURE_COLORS[layer.texture] ?? COLOR_FALLBACK
  })

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
          } else if (height < VISIBLE_SAND_THRESHOLD) {
            ;[r, g, b] = COLOR_SHALLOW_WATER
          } else if (splatData) {
            const splatIdx = (cz * TILE_DIM + cx) * 4
            const packed = splatData[splatIdx]
            const blend = splatData[splatIdx + 2] / 255
            const cP = channelColors[unpackPrimary(packed)]
            const cS = channelColors[unpackSecondary(packed)]
            r = Math.round(cP[0] * (1 - blend) + cS[0] * blend)
            g = Math.round(cP[1] * (1 - blend) + cS[1] * blend)
            b = Math.round(cP[2] * (1 - blend) + cS[2] * blend)
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

  // Overlay building footprints
  // Terrain tiles are centered: tile tx covers [tx*64-32, tx*64+32).
  // The first tile in the region (lx=0, tx=rx*16) starts at rx*1024-32.
  const houses = await fetchHousesInRegion(rx, rz, apiUrl)
  const regionWorldX = rx * REGION_SIZE * TILE_DIM - TILE_DIM / 2
  const regionWorldZ = rz * REGION_SIZE * TILE_DIM - TILE_DIM / 2

  for (const house of houses) {
    for (const room of house.rooms) {
      if (room.floorLevel !== 0) continue
      const roomWorldX = house.origin.x + room.localX - regionWorldX
      const roomWorldZ = house.origin.z + room.localZ - regionWorldZ
      const minPx = Math.max(0, Math.floor(roomWorldX))
      const minPz = Math.max(0, Math.floor(roomWorldZ))
      const maxPx = Math.min(MAP_PX, Math.ceil(roomWorldX + room.sizeX))
      const maxPz = Math.min(MAP_PX, Math.ceil(roomWorldZ + room.sizeZ))

      for (let pz = minPz; pz < maxPz; pz++) {
        for (let px = minPx; px < maxPx; px++) {
          const idx = (pz * MAP_PX + px) * 4
          pixels[idx] = COLOR_BUILDING[0]
          pixels[idx + 1] = COLOR_BUILDING[1]
          pixels[idx + 2] = COLOR_BUILDING[2]
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

/** Fetch all houses in a region, batched and deduplicated. */
export async function fetchHousesInRegion(
  rx: number,
  rz: number,
  apiUrl: string
): Promise<HouseData[]> {
  const BATCH_SIZE = 16
  const chunkCoords: [number, number][] = []
  for (let tz = 0; tz < REGION_SIZE; tz++) {
    for (let tx = 0; tx < REGION_SIZE; tx++) {
      chunkCoords.push([rx * REGION_SIZE + tx, rz * REGION_SIZE + tz])
    }
  }

  const houses: HouseData[] = []
  for (let i = 0; i < chunkCoords.length; i += BATCH_SIZE) {
    const batch = chunkCoords.slice(i, i + BATCH_SIZE)
    const results = await Promise.all(
      batch.map(([cx, cz]) =>
        fetch(`${apiUrl}/api/housing/area/${cx}/${cz}`)
          .then((r) => (r.ok ? (r.json() as Promise<HouseData[]>) : []))
          .catch(() => [] as HouseData[])
      )
    )
    houses.push(...results.flat())
  }

  // A house may span multiple chunks
  const seen = new Set<string>()
  return houses.filter((h) => {
    if (seen.has(h.id)) return false
    seen.add(h.id)
    return true
  })
}

/** Build the server URL for a region minimap (HTTP-cacheable). */
export function regionMinimapServerUrl(rx: number, rz: number): string {
  return `${getTerrainApiUrl()}/api/terrain/minimap/${rx}/${rz}`
}

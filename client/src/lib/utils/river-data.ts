/**
 * River segment data: decoder for the offline-baked `rivers/*.bin` files.
 *
 * Format (see `shared/src/worldgen/tile_bake.rs` `bake_rivers_binary`):
 *
 *   header (16 bytes):
 *     bytes 0..4    ASCII "RIV1"
 *     bytes 4..6    u16 version (must be 1)
 *     bytes 6..8    u16 segment_count
 *     bytes 8..16   2× f32 reserved (0.0)
 *
 *   per-segment (32 bytes, 8× f32):
 *     ax, az, bx, bz
 *     width_a, width_b
 *     flow_norm_a, flow_norm_b
 *
 * The baker assigns each segment to exactly one tile (midpoint ownership),
 * so adjacent polyline segments may live in different tile files. Endpoints
 * are shared across the split, so ribbons in neighbor tiles still meet.
 */

const MAGIC = 0x31564952 // "RIV1" little-endian
const HEADER_BYTES = 16
const BYTES_PER_SEGMENT = 32
const SUPPORTED_VERSION = 1

export interface RiverSegment {
  /** World-space endpoint A (meters). */
  ax: number
  az: number
  /** World-space endpoint B (meters). */
  bx: number
  bz: number
  /** Surface width at each endpoint (meters). */
  widthA: number
  widthB: number
  /** Normalized flow accumulation at each endpoint (0..1). Drives color /
   *  scroll-speed variation in the river shader. */
  flowNormA: number
  flowNormB: number
}

export interface RiverTileData {
  /** Decoded segments belonging to this tile (midpoint-owned). May be empty
   *  if the tile file existed but contained zero segments — unusual but
   *  legal. */
  segments: RiverSegment[]
}

/** Decode a `rivers/*.bin` tile file. Throws on corrupt data (wrong magic,
 *  version mismatch, truncated payload) so callers see the problem early
 *  rather than rendering garbage. */
export function decodeRiverData(buffer: ArrayBuffer): RiverTileData {
  if (buffer.byteLength < HEADER_BYTES) {
    throw new Error(`river data too small: ${buffer.byteLength} bytes`)
  }
  const view = new DataView(buffer)
  const magic = view.getUint32(0, true)
  if (magic !== MAGIC) {
    throw new Error(
      `river data magic mismatch: got 0x${magic.toString(16)}, expected 0x${MAGIC.toString(16)}`
    )
  }
  const version = view.getUint16(4, true)
  if (version !== SUPPORTED_VERSION) {
    throw new Error(
      `river data version ${version} unsupported (expected ${SUPPORTED_VERSION})`
    )
  }
  const segmentCount = view.getUint16(6, true)
  const expectedSize = HEADER_BYTES + segmentCount * BYTES_PER_SEGMENT
  if (buffer.byteLength !== expectedSize) {
    throw new Error(
      `river data size ${buffer.byteLength} does not match header (${expectedSize} for ${segmentCount} segs)`
    )
  }

  const segments: RiverSegment[] = new Array(segmentCount)
  for (let i = 0; i < segmentCount; i++) {
    const off = HEADER_BYTES + i * BYTES_PER_SEGMENT
    segments[i] = {
      ax: view.getFloat32(off, true),
      az: view.getFloat32(off + 4, true),
      bx: view.getFloat32(off + 8, true),
      bz: view.getFloat32(off + 12, true),
      widthA: view.getFloat32(off + 16, true),
      widthB: view.getFloat32(off + 20, true),
      flowNormA: view.getFloat32(off + 24, true),
      flowNormB: view.getFloat32(off + 28, true),
    }
  }

  return { segments }
}

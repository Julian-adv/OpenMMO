export interface ReferenceImageData {
  width: number
  height: number
  pixels: Uint8ClampedArray // RGBA flat array
}

export interface BiomeWeights {
  sea: number
  plains: number
  mountain: number
  highland: number
  river: number
}

// Reference colors for each biome (RGB)
const BIOME_COLORS = [
  { r: 1, g: 42, b: 254 }, // Sea (blue)
  { r: 7, g: 247, b: 46 }, // Plains (green)
  { r: 127, g: 79, b: 39 }, // Mountain (brown)
  { r: 253, g: 255, b: 254 }, // Highland/Snow (white)
  { r: 60, g: 232, b: 194 }, // River (sky blue)
] as const

const PIXELS_PER_METER = 1 / 32 // 1 pixel = 32m

export async function loadReferenceImage(
  file: File
): Promise<ReferenceImageData> {
  const bitmap = await createImageBitmap(file)
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height)
  const ctx = canvas.getContext('2d')!
  ctx.drawImage(bitmap, 0, 0)
  const imageData = ctx.getImageData(0, 0, bitmap.width, bitmap.height)
  bitmap.close()
  return {
    width: imageData.width,
    height: imageData.height,
    pixels: imageData.data,
  }
}

function samplePixelBilinear(
  img: ReferenceImageData,
  px: number,
  pz: number
): [number, number, number] {
  const x0 = Math.floor(px)
  const z0 = Math.floor(pz)
  const x1 = Math.min(x0 + 1, img.width - 1)
  const z1 = Math.min(z0 + 1, img.height - 1)
  const fx = px - x0
  const fz = pz - z0

  const cx0 = Math.max(0, Math.min(x0, img.width - 1))
  const cz0 = Math.max(0, Math.min(z0, img.height - 1))

  const i00 = (cz0 * img.width + cx0) * 4
  const i10 = (cz0 * img.width + x1) * 4
  const i01 = (z1 * img.width + cx0) * 4
  const i11 = (z1 * img.width + x1) * 4

  const w00 = (1 - fx) * (1 - fz)
  const w10 = fx * (1 - fz)
  const w01 = (1 - fx) * fz
  const w11 = fx * fz

  const r =
    img.pixels[i00] * w00 +
    img.pixels[i10] * w10 +
    img.pixels[i01] * w01 +
    img.pixels[i11] * w11
  const g =
    img.pixels[i00 + 1] * w00 +
    img.pixels[i10 + 1] * w10 +
    img.pixels[i01 + 1] * w01 +
    img.pixels[i11 + 1] * w11
  const b =
    img.pixels[i00 + 2] * w00 +
    img.pixels[i10 + 2] * w10 +
    img.pixels[i01 + 2] * w01 +
    img.pixels[i11 + 2] * w11

  return [r, g, b]
}

function colorToBiomeWeights(r: number, g: number, b: number): BiomeWeights {
  const EPSILON = 1.0
  const weights: number[] = []
  let totalWeight = 0

  for (const ref of BIOME_COLORS) {
    const dr = r - ref.r
    const dg = g - ref.g
    const db = b - ref.b
    const dist = Math.sqrt(dr * dr + dg * dg + db * db)
    const w = 1 / ((dist + EPSILON) * (dist + EPSILON))
    weights.push(w)
    totalWeight += w
  }

  return {
    sea: weights[0] / totalWeight,
    plains: weights[1] / totalWeight,
    mountain: weights[2] / totalWeight,
    highland: weights[3] / totalWeight,
    river: weights[4] / totalWeight,
  }
}

export function sampleBiomeWeights(
  img: ReferenceImageData,
  worldX: number,
  worldZ: number
): BiomeWeights | null {
  // Image center = world origin (0, 0)
  const px = worldX * PIXELS_PER_METER + img.width / 2
  const pz = worldZ * PIXELS_PER_METER + img.height / 2

  // Outside image bounds
  if (px < 0 || px >= img.width || pz < 0 || pz >= img.height) return null

  const [r, g, b] = samplePixelBilinear(img, px, pz)
  return colorToBiomeWeights(r, g, b)
}

// Sea color reference for land detection
const SEA_REF = BIOME_COLORS[0]
const SEA_DIST_THRESHOLD = 80 // color distance — below this = sea

function isLandPixel(img: ReferenceImageData, ix: number, iz: number): boolean {
  const idx = (iz * img.width + ix) * 4
  const dr = img.pixels[idx] - SEA_REF.r
  const dg = img.pixels[idx + 1] - SEA_REF.g
  const db = img.pixels[idx + 2] - SEA_REF.b
  return Math.sqrt(dr * dr + dg * dg + db * db) > SEA_DIST_THRESHOLD
}

/**
 * Compute land density at a world position by sampling a radius of pixels
 * in the reference image. Returns 0 (open ocean) to 1 (surrounded by land).
 * Uses the reference image's full extent, so it captures large-scale curvature.
 */
export function sampleLandDensity(
  img: ReferenceImageData,
  worldX: number,
  worldZ: number,
  radiusPixels: number
): number {
  const px = worldX * PIXELS_PER_METER + img.width / 2
  const pz = worldZ * PIXELS_PER_METER + img.height / 2

  const cx = Math.round(px)
  const cz = Math.round(pz)
  const r = radiusPixels

  let landCount = 0
  let totalCount = 0

  for (let dz = -r; dz <= r; dz++) {
    const iz = cz + dz
    if (iz < 0 || iz >= img.height) continue
    for (let dx = -r; dx <= r; dx++) {
      const ix = cx + dx
      if (ix < 0 || ix >= img.width) continue
      totalCount++
      if (isLandPixel(img, ix, iz)) landCount++
    }
  }

  return totalCount > 0 ? landCount / totalCount : 0
}

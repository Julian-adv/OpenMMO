/**
 * Bakes a tileable grayscale cloud density texture used by the river shader
 * (client/src/lib/shaders/river-material.ts) for sky reflections.
 *
 * Strategy: periodic 2D Perlin noise, 5-octave FBM, domain-warped once to
 * break up the grid-aligned feel, then contrast-shaped so output is mostly
 * clear (0) with scattered cloud blobs (0.5-1.0). The Perlin gradient lookup
 * wraps at the configured period (PX, PY), so the output PNG tiles exactly.
 *
 * Usage: node client/scripts/bake-sky-clouds.mjs
 * Output: client/public/textures/sky-clouds.png (1024×512, 8-bit grayscale)
 */

import { writeFileSync } from 'node:fs'
import { deflateSync, crc32 } from 'node:zlib'
import { Buffer } from 'node:buffer'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const OUT_PATH = resolve(__dirname, '../public/textures/sky-clouds.png')

const WIDTH = 1024
const HEIGHT = 512

// Base noise periods across the full image. More periods = smaller cloud
// features; 4 across / 2 down gives ~2:1 stretched blobs that look like
// cumulus clusters rather than a uniform pattern.
const PERIOD_X = 4
const PERIOD_Y = 2

const OCTAVES = 4
const PERSISTENCE = 0.55
const LACUNARITY = 2

// Shaping — aim for puffy cumulus blobs, not wispy smoke.
// Smoothstep range maps normalized above-threshold to [0,1]: the narrow
// range plus a moderate threshold gives clean bright interiors with a
// gentle falloff at the blob fringes.
const COVERAGE_THRESHOLD = 0.48
const EDGE_LOW = 0.05
const EDGE_HIGH = 0.45
const WARP_STRENGTH = 0.3

// ─── Seeded Perlin noise, periodic in (PX, PY) ──────────

function makePerlin(seed) {
  const perm = new Uint8Array(512)
  const base = new Uint8Array(256)
  for (let i = 0; i < 256; i++) base[i] = i

  // LCG so the output is reproducible without a dependency.
  let s = (seed | 0) || 1
  const rand = () => {
    s = (s * 1664525 + 1013904223) >>> 0
    return s / 0x100000000
  }
  for (let i = 255; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1))
    const tmp = base[i]
    base[i] = base[j]
    base[j] = tmp
  }
  for (let i = 0; i < 512; i++) perm[i] = base[i & 255]

  const fade = (t) => t * t * t * (t * (t * 6 - 15) + 10)
  const lerp = (a, b, t) => a + t * (b - a)
  const grad = (h, x, y) => {
    const u = (h & 1) === 0 ? x : -x
    const v = (h & 2) === 0 ? y : -y
    return u + v
  }

  // Gradient indices wrap at (px, py) so sampling at (x+px, y) yields the
  // same value as sampling at (x, y) — that's what makes the output tile.
  return function perlin(x, y, px, py) {
    const xi = Math.floor(x)
    const yi = Math.floor(y)
    const xf = x - xi
    const yf = y - yi
    const u = fade(xf)
    const v = fade(yf)

    const X = ((xi % px) + px) % px
    const Y = ((yi % py) + py) % py
    const X1 = (X + 1) % px
    const Y1 = (Y + 1) % py

    const a = perm[(perm[X] + Y) & 255]
    const b = perm[(perm[X1] + Y) & 255]
    const c = perm[(perm[X] + Y1) & 255]
    const d = perm[(perm[X1] + Y1) & 255]

    const n00 = grad(a, xf, yf)
    const n10 = grad(b, xf - 1, yf)
    const n01 = grad(c, xf, yf - 1)
    const n11 = grad(d, xf - 1, yf - 1)

    return lerp(lerp(n00, n10, u), lerp(n01, n11, u), v)
  }
}

const perlin = makePerlin(424242)

function fbm(x, y, px, py) {
  let amp = 1
  let freq = 1
  let sum = 0
  let norm = 0
  for (let o = 0; o < OCTAVES; o++) {
    sum += amp * perlin(x * freq, y * freq, px * freq, py * freq)
    norm += amp
    amp *= PERSISTENCE
    freq *= LACUNARITY
  }
  return sum / norm
}

// ─── Generate pixels ────────────────────────────────────

console.log(`baking ${WIDTH}×${HEIGHT} sky cloud texture...`)
const t0 = Date.now()

const pixels = new Uint8Array(WIDTH * HEIGHT)
for (let py = 0; py < HEIGHT; py++) {
  for (let px = 0; px < WIDTH; px++) {
    const u = px / WIDTH
    const v = py / HEIGHT
    const x = u * PERIOD_X
    const y = v * PERIOD_Y

    // Domain warp keeps tileability because fbm itself wraps at (PX, PY).
    const warpX = fbm(x + 5.1, y + 2.3, PERIOD_X, PERIOD_Y) * WARP_STRENGTH
    const warpY = fbm(x + 1.7, y + 9.8, PERIOD_X, PERIOD_Y) * WARP_STRENGTH

    // Main cloud field, mapped from [-1,1] to [0,1].
    let n = fbm(x + warpX, y + warpY, PERIOD_X, PERIOD_Y)
    n = n * 0.5 + 0.5

    // Shape: subtract threshold so most of the sky is 0 (clear). Smoothstep
    // then pulls the "above threshold" range onto a clean S-curve, giving
    // bright cloud interiors and soft fringes without a uniform-gray haze.
    const above = Math.max(0, n - COVERAGE_THRESHOLD) / (1 - COVERAGE_THRESHOLD)
    const t = Math.min(1, Math.max(0, (above - EDGE_LOW) / (EDGE_HIGH - EDGE_LOW)))
    const cloud = t * t * (3 - 2 * t)

    pixels[py * WIDTH + px] = Math.min(255, Math.round(cloud * 255))
  }
}

// ─── PNG encoding (8-bit grayscale, color type 0) ───────

function writeChunk(type, payload) {
  const len = Buffer.alloc(4)
  len.writeUInt32BE(payload.length, 0)
  const typeBuf = Buffer.from(type, 'ascii')
  const crcInput = Buffer.concat([typeBuf, payload])
  const crcBuf = Buffer.alloc(4)
  crcBuf.writeUInt32BE(crc32(crcInput) >>> 0, 0)
  return Buffer.concat([len, typeBuf, payload, crcBuf])
}

function encodePNG(data, width, height) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])

  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(width, 0)
  ihdr.writeUInt32BE(height, 4)
  ihdr[8] = 8 // bit depth
  ihdr[9] = 0 // color type: grayscale
  ihdr[10] = 0 // compression
  ihdr[11] = 0 // filter
  ihdr[12] = 0 // interlace

  // Each scanline gets a filter byte (0 = None) prepended.
  const raw = Buffer.alloc((width + 1) * height)
  for (let y = 0; y < height; y++) {
    raw[y * (width + 1)] = 0
    for (let x = 0; x < width; x++) {
      raw[y * (width + 1) + 1 + x] = data[y * width + x]
    }
  }
  const idatPayload = deflateSync(raw)

  return Buffer.concat([
    signature,
    writeChunk('IHDR', ihdr),
    writeChunk('IDAT', idatPayload),
    writeChunk('IEND', Buffer.alloc(0)),
  ])
}

const png = encodePNG(pixels, WIDTH, HEIGHT)
writeFileSync(OUT_PATH, png)
console.log(`wrote ${OUT_PATH} (${png.length} bytes) in ${Date.now() - t0}ms`)

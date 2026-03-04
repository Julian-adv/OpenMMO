/**
 * Self-contained 2D simplex noise implementation.
 * Based on Stefan Gustavson's simplex noise algorithm.
 */

// Gradient vectors for 2D simplex noise
const GRAD2 = [
  [1, 1],
  [-1, 1],
  [1, -1],
  [-1, -1],
  [1, 0],
  [-1, 0],
  [0, 1],
  [0, -1],
] as const

const F2 = 0.5 * (Math.sqrt(3) - 1) // skew factor for 2D
const G2 = (3 - Math.sqrt(3)) / 6 // unskew factor for 2D

/** Create a seeded permutation table */
function buildPerm(seed: number): Uint8Array {
  const perm = new Uint8Array(512)
  const p = new Uint8Array(256)
  for (let i = 0; i < 256; i++) p[i] = i

  // Seed-based Fisher-Yates shuffle
  let s = seed >>> 0
  for (let i = 255; i > 0; i--) {
    s = (s * 1664525 + 1013904223) >>> 0
    const j = s % (i + 1)
    const tmp = p[i]
    p[i] = p[j]
    p[j] = tmp
  }

  for (let i = 0; i < 256; i++) {
    perm[i] = p[i]
    perm[i + 256] = p[i]
  }
  return perm
}

/**
 * Create a 2D simplex noise function with a given seed.
 * Returns values in [-1, 1].
 */
export function createNoise2D(seed: number): (x: number, y: number) => number {
  const perm = buildPerm(seed)

  return function noise2D(x: number, y: number): number {
    // Skew input to determine which simplex cell we're in
    const s = (x + y) * F2
    const i = Math.floor(x + s)
    const j = Math.floor(y + s)

    const t = (i + j) * G2
    const X0 = i - t // unskewed cell origin
    const Y0 = j - t
    const x0 = x - X0 // distance from cell origin
    const y0 = y - Y0

    // Determine which simplex we're in
    const i1 = x0 > y0 ? 1 : 0
    const j1 = x0 > y0 ? 0 : 1

    const x1 = x0 - i1 + G2
    const y1 = y0 - j1 + G2
    const x2 = x0 - 1 + 2 * G2
    const y2 = y0 - 1 + 2 * G2

    const ii = i & 255
    const jj = j & 255

    // Gradient indices
    const gi0 = perm[ii + perm[jj]] % 8
    const gi1 = perm[ii + i1 + perm[jj + j1]] % 8
    const gi2 = perm[ii + 1 + perm[jj + 1]] % 8

    // Corner contributions
    let n0 = 0,
      n1 = 0,
      n2 = 0

    let t0 = 0.5 - x0 * x0 - y0 * y0
    if (t0 > 0) {
      t0 *= t0
      n0 = t0 * t0 * (GRAD2[gi0][0] * x0 + GRAD2[gi0][1] * y0)
    }

    let t1 = 0.5 - x1 * x1 - y1 * y1
    if (t1 > 0) {
      t1 *= t1
      n1 = t1 * t1 * (GRAD2[gi1][0] * x1 + GRAD2[gi1][1] * y1)
    }

    let t2 = 0.5 - x2 * x2 - y2 * y2
    if (t2 > 0) {
      t2 *= t2
      n2 = t2 * t2 * (GRAD2[gi2][0] * x2 + GRAD2[gi2][1] * y2)
    }

    // Scale to [-1, 1]
    return 70 * (n0 + n1 + n2)
  }
}

/**
 * Fractional Brownian motion using 2D noise.
 * Returns a value roughly in [-1, 1].
 */
export function fbm2D(
  noise: (x: number, y: number) => number,
  x: number,
  y: number,
  octaves: number,
  lacunarity: number,
  persistence: number
): number {
  let value = 0
  let amplitude = 1
  let frequency = 1
  let maxAmplitude = 0

  for (let i = 0; i < octaves; i++) {
    value += noise(x * frequency, y * frequency) * amplitude
    maxAmplitude += amplitude
    amplitude *= persistence
    frequency *= lacunarity
  }

  return value / maxAmplitude
}

/**
 * Seeded pseudo-random number generator (mulberry32).
 * Returns a function that produces values in [0, 1).
 */
export function createRng(seed: number): () => number {
  let s = seed >>> 0
  return function () {
    s = (s + 0x6d2b79f5) | 0
    let t = Math.imul(s ^ (s >>> 15), 1 | s)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

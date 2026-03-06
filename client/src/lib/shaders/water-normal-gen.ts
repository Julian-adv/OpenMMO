import * as THREE from 'three'

/**
 * Generate a tileable perlin-noise normal map (256x256) using Canvas2D.
 * No directional bias — produces uniform, isotropic wave detail
 * suitable for surface normals and sparkle sampling.
 */
export function generateWaterNormalMap(): THREE.CanvasTexture {
  const size = 256
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const ctx = canvas.getContext('2d')!
  const imageData = ctx.createImageData(size, size)
  const data = imageData.data

  // Generate tileable height field using layered value noise
  const heights = new Float32Array(size * size)
  const octaves = [
    { freq: 8, amp: 0.4 },
    { freq: 16, amp: 0.3 },
    { freq: 32, amp: 0.2 },
    { freq: 64, amp: 0.1 },
  ]

  const rng = mulberry32(137)
  // Generate gradient tables for each octave for tileable noise
  for (const oct of octaves) {
    const g = oct.freq
    const grid = new Float32Array((g + 1) * (g + 1))
    for (let i = 0; i < g * g; i++) {
      grid[i] = rng()
    }
    // Wrap edges for tiling
    for (let i = 0; i <= g; i++) {
      grid[i * (g + 1) + g] = grid[i * (g + 1)] // right = left
      grid[g * (g + 1) + i] = grid[i] // bottom = top
    }
    grid[g * (g + 1) + g] = grid[0] // corner

    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        const fx = (x / size) * g
        const fy = (y / size) * g
        const ix = Math.floor(fx)
        const iy = Math.floor(fy)
        const tx = fx - ix
        const ty = fy - iy
        // Smoothstep interpolation
        const sx = tx * tx * (3 - 2 * tx)
        const sy = ty * ty * (3 - 2 * ty)

        const stride = g + 1
        const v00 = grid[iy * stride + ix]
        const v10 = grid[iy * stride + ix + 1]
        const v01 = grid[(iy + 1) * stride + ix]
        const v11 = grid[(iy + 1) * stride + ix + 1]

        const v =
          (v00 * (1 - sx) + v10 * sx) * (1 - sy) +
          (v01 * (1 - sx) + v11 * sx) * sy

        heights[y * size + x] += v * oct.amp
      }
    }
  }

  // Derive normals from height field (central differences)
  const scale = 8.0 // normal strength — needs enough variation for sparkle threshold
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const xl = heights[y * size + ((x - 1 + size) % size)]
      const xr = heights[y * size + ((x + 1) % size)]
      const yl = heights[((y - 1 + size) % size) * size + x]
      const yr = heights[((y + 1) % size) * size + x]

      const dx = (xl - xr) * scale
      const dy = (yl - yr) * scale

      // Normal in tangent space: (dx, dy, 1) normalized
      const len = Math.sqrt(dx * dx + dy * dy + 1)
      const nx = dx / len
      const ny = dy / len
      const nz = 1 / len

      const idx = (y * size + x) * 4
      data[idx] = Math.floor((nx * 0.5 + 0.5) * 255)
      data[idx + 1] = Math.floor((ny * 0.5 + 0.5) * 255)
      data[idx + 2] = Math.floor((nz * 0.5 + 0.5) * 255)
      data[idx + 3] = 255
    }
  }

  ctx.putImageData(imageData, 0, 0)

  const texture = new THREE.CanvasTexture(canvas)
  texture.wrapS = THREE.RepeatWrapping
  texture.wrapT = THREE.RepeatWrapping
  texture.minFilter = THREE.LinearMipMapLinearFilter
  texture.magFilter = THREE.LinearFilter
  texture.needsUpdate = true
  return texture
}

/** Simple deterministic PRNG (Mulberry32) */
function mulberry32(seed: number) {
  let s = seed | 0
  return () => {
    s = (s + 0x6d2b79f5) | 0
    let t = Math.imul(s ^ (s >>> 15), 1 | s)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

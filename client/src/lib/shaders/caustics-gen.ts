import * as THREE from 'three'

/**
 * Generate a tileable Voronoi-based caustics texture (256x256) using Canvas2D.
 * The min(layer1, layer2) trick in the terrain shader creates a convincing
 * light-focusing pattern from this single texture.
 */
export function generateCausticsTexture(): THREE.CanvasTexture {
  const size = 256
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const ctx = canvas.getContext('2d')!
  const imageData = ctx.createImageData(size, size)
  const data = imageData.data

  // Generate random Voronoi cell centers, replicated for tiling
  const cellCount = 24
  const rng = mulberry32(42) // deterministic seed
  const points: { x: number; y: number }[] = []
  for (let i = 0; i < cellCount; i++) {
    points.push({ x: rng() * size, y: rng() * size })
  }

  // Replicate points in 3x3 grid for seamless tiling
  const tiled: { x: number; y: number }[] = []
  for (let ox = -1; ox <= 1; ox++) {
    for (let oz = -1; oz <= 1; oz++) {
      for (const p of points) {
        tiled.push({ x: p.x + ox * size, y: p.y + oz * size })
      }
    }
  }

  // For each pixel, compute distance to nearest and second-nearest cell center
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let d1 = Infinity
      let d2 = Infinity
      for (const p of tiled) {
        const dx = x - p.x
        const dy = y - p.y
        const d = Math.sqrt(dx * dx + dy * dy)
        if (d < d1) {
          d2 = d1
          d1 = d
        } else if (d < d2) {
          d2 = d
        }
      }

      // Edge detection: bright lines where d2 - d1 is small (cell boundaries)
      const edge = d2 - d1
      // Normalize and invert: thin bright lines on dark background
      const maxEdge = 18
      const v = Math.pow(1 - Math.min(edge / maxEdge, 1), 3)
      const byte = Math.floor(v * 255)

      const idx = (y * size + x) * 4
      data[idx] = byte
      data[idx + 1] = byte
      data[idx + 2] = byte
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

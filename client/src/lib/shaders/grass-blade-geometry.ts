import * as THREE from 'three'

/**
 * Create a procedural grass blade strip geometry.
 *
 * The blade is a flat strip with `segments` quads, tapered from base to tip.
 * UV convention: y=0 at base, y=1 at tip (matching existing grass shader).
 *
 * @param segments Number of vertical segments (default 5)
 * @param halfWidth Base half-width in local units (default 0.04)
 * @param height Blade height in local units (default 1.0)
 * @param taper Fraction of width remaining at tip (0 = point, 1 = no taper, default 0.1)
 */
export function createBladeGeometry(
  segments = 5,
  halfWidth = 0.04,
  height = 1.0,
  taper = 0.1
): THREE.BufferGeometry {
  const vertCount = (segments + 1) * 2
  const positions = new Float32Array(vertCount * 3)
  const normals = new Float32Array(vertCount * 3)
  const uvs = new Float32Array(vertCount * 2)

  for (let i = 0; i <= segments; i++) {
    const t = i / segments // 0 at base, 1 at tip
    const y = t * height
    const hw = halfWidth * (1.0 - t * (1.0 - taper))

    const base = i * 2

    // Left vertex
    positions[base * 3] = -hw
    positions[base * 3 + 1] = y
    positions[base * 3 + 2] = 0

    // Right vertex
    positions[(base + 1) * 3] = hw
    positions[(base + 1) * 3 + 1] = y
    positions[(base + 1) * 3 + 2] = 0

    // Normals: all face +Z (DoubleSide handles backface)
    normals[base * 3 + 2] = 1
    normals[(base + 1) * 3 + 2] = 1

    // UVs
    uvs[base * 2] = 0
    uvs[base * 2 + 1] = t
    uvs[(base + 1) * 2] = 1
    uvs[(base + 1) * 2 + 1] = t
  }

  // Triangle indices
  const indexCount = segments * 6
  const indices = new Uint16Array(indexCount)
  for (let i = 0; i < segments; i++) {
    const b = i * 2
    const o = i * 6
    indices[o] = b
    indices[o + 1] = b + 1
    indices[o + 2] = b + 2
    indices[o + 3] = b + 1
    indices[o + 4] = b + 3
    indices[o + 5] = b + 2
  }

  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3))
  geometry.setAttribute('normal', new THREE.BufferAttribute(normals, 3))
  geometry.setAttribute('uv', new THREE.BufferAttribute(uvs, 2))
  geometry.setIndex(new THREE.BufferAttribute(indices, 1))

  return geometry
}

import * as THREE from 'three'

/**
 * Create a star-billboard geometry: one or more blade strips intersecting at
 * equal angles around the Y axis.
 *
 * Each strip is a tapered quad strip with `segments` quads.
 * UV convention: y=0 at base, y=1 at tip (matching grass shader).
 *
 * @param strips Number of intersecting strips (1 = single blade, 2 = cross/90°, 3 = star/60°)
 */
export function createStarGeometry(
  segments = 5,
  halfWidth = 0.15,
  height = 1.0,
  taper = 0.1,
  strips = 3
): THREE.BufferGeometry {
  const stripVertCount = (segments + 1) * 2
  const stripIndexCount = segments * 6
  const totalVerts = stripVertCount * strips
  const totalIndices = stripIndexCount * strips

  const positions = new Float32Array(totalVerts * 3)
  const normals = new Float32Array(totalVerts * 3)
  const uvs = new Float32Array(totalVerts * 2)
  const indices = new Uint16Array(totalIndices)

  for (let strip = 0; strip < strips; strip++) {
    const angle = (strip * Math.PI) / strips
    const dirX = Math.cos(angle)
    const dirZ = Math.sin(angle)
    const nrmX = -dirZ
    const nrmZ = dirX

    const vOff = strip * stripVertCount
    const iOff = strip * stripIndexCount

    for (let i = 0; i <= segments; i++) {
      const t = i / segments
      const y = t * height
      const hw = halfWidth * (1.0 - t * (1.0 - taper))

      const base = vOff + i * 2

      positions[base * 3] = -hw * dirX
      positions[base * 3 + 1] = y
      positions[base * 3 + 2] = -hw * dirZ
      positions[(base + 1) * 3] = hw * dirX
      positions[(base + 1) * 3 + 1] = y
      positions[(base + 1) * 3 + 2] = hw * dirZ

      normals[base * 3] = nrmX
      normals[base * 3 + 2] = nrmZ
      normals[(base + 1) * 3] = nrmX
      normals[(base + 1) * 3 + 2] = nrmZ

      uvs[base * 2] = 0
      uvs[base * 2 + 1] = t
      uvs[(base + 1) * 2] = 1
      uvs[(base + 1) * 2 + 1] = t
    }

    for (let i = 0; i < segments; i++) {
      const b = vOff + i * 2
      const o = iOff + i * 6
      indices[o] = b
      indices[o + 1] = b + 1
      indices[o + 2] = b + 2
      indices[o + 3] = b + 1
      indices[o + 4] = b + 3
      indices[o + 5] = b + 2
    }
  }

  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3))
  geometry.setAttribute('normal', new THREE.BufferAttribute(normals, 3))
  geometry.setAttribute('uv', new THREE.BufferAttribute(uvs, 2))
  geometry.setIndex(new THREE.BufferAttribute(indices, 1))

  return geometry
}

/** Single blade strip — convenience wrapper for `createStarGeometry` with 1 strip. */
export function createBladeGeometry(
  segments = 5,
  halfWidth = 0.04,
  height = 1.0,
  taper = 0.1
): THREE.BufferGeometry {
  return createStarGeometry(segments, halfWidth, height, taper, 1)
}

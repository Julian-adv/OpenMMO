// Read animation durations from GLB accessor metadata.
import { readFileSync } from 'fs'
import { readGlb } from './glb.mjs'

export function parseGlbJson(filePath) {
  return readGlb(readFileSync(filePath)).json
}

/** Clip name -> duration in seconds, rounded to millisecond precision. */
export function extractDurations(filePath) {
  const gltf = parseGlbJson(filePath)
  const result = {}
  if (!gltf.animations) return result

  for (const anim of gltf.animations) {
    const name = anim.name
    if (!name) continue

    let maxTime = 0
    for (const sampler of anim.samplers || []) {
      const accessorIdx = sampler.input
      if (accessorIdx == null || !gltf.accessors) continue
      const accessor = gltf.accessors[accessorIdx]
      if (accessor && accessor.max) {
        const t = Array.isArray(accessor.max) ? accessor.max[0] : accessor.max
        if (t > maxTime) maxTime = t
      }
    }

    result[name] = Math.round(maxTime * 1000) / 1000
  }

  return result
}

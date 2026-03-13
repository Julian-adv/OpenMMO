#!/usr/bin/env node
/**
 * Extract animation durations from GLB files and write to a shared JSON.
 *
 * GLB format (glTF 2.0 binary):
 *   - 12-byte header: magic(4) + version(4) + length(4)
 *   - Chunks: length(4) + type(4) + data(length)
 *     - Type 0x4E4F534A = JSON chunk
 *
 * The JSON chunk contains `animations[].name` and `animations[].samplers`
 * with `input` accessor indices. The actual duration is `max` of the input
 * accessor (time keyframes).
 *
 * Usage: node tools/extract-animation-durations.mjs
 */

import { readFileSync, writeFileSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(__dirname, '..')

// GLB files to scan (relative to client/public)
const GLB_FILES = [
  'client/public/models/animations/combat_melee.glb',
  'client/public/models/animations/locomotion.glb',
  'client/public/models/female_knight.glb',
]

function parseGlbJson(filePath) {
  const buf = readFileSync(filePath)

  // Validate GLB header
  const magic = buf.readUInt32LE(0)
  if (magic !== 0x46546c67) {
    // 'glTF'
    throw new Error(`Not a GLB file: ${filePath}`)
  }

  // First chunk should be JSON
  const chunkLength = buf.readUInt32LE(12)
  const chunkType = buf.readUInt32LE(16)
  if (chunkType !== 0x4e4f534a) {
    // 'JSON'
    throw new Error(`First chunk is not JSON in ${filePath}`)
  }

  const jsonStr = buf.toString('utf8', 20, 20 + chunkLength)
  return JSON.parse(jsonStr)
}

function extractDurations(filePath) {
  const gltf = parseGlbJson(filePath)
  const result = {}

  if (!gltf.animations) return result

  for (const anim of gltf.animations) {
    const name = anim.name
    if (!name) continue

    // Find the maximum time across all samplers' input accessors
    let maxTime = 0
    for (const sampler of anim.samplers || []) {
      const accessorIdx = sampler.input
      if (accessorIdx == null || !gltf.accessors) continue
      const accessor = gltf.accessors[accessorIdx]
      if (accessor && accessor.max) {
        // accessor.max is [maxTime] for scalar time accessors
        const t = Array.isArray(accessor.max) ? accessor.max[0] : accessor.max
        if (t > maxTime) maxTime = t
      }
    }

    result[name] = Math.round(maxTime * 1000) / 1000 // Round to ms precision
  }

  return result
}

// Main
const allDurations = {}

for (const relPath of GLB_FILES) {
  const fullPath = resolve(ROOT, relPath)
  try {
    const durations = extractDurations(fullPath)
    const source = relPath.replace(/.*\//, '').replace('.glb', '')
    allDurations[source] = durations
    console.log(`✓ ${relPath}: ${Object.keys(durations).length} animations`)
  } catch (e) {
    console.warn(`⚠ Skipping ${relPath}: ${e.message}`)
  }
}

const json = JSON.stringify(allDurations, null, 2) + '\n'
const outPath = resolve(ROOT, 'agent-client/data/animation_durations.json')
writeFileSync(outPath, json)
console.log(`\nWrote ${outPath}`)

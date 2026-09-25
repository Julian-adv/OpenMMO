// Sample bridge deck heights into the catalog; --dry-run only prints samples.
import { NodeIO } from '@gltf-transform/core'
import { EXTMeshoptCompression, KHRMaterialsUnlit, KHRMeshQuantization } from '@gltf-transform/extensions'
import { MeshoptDecoder } from '../client/node_modules/three/examples/jsm/libs/meshopt_decoder.module.js'
import { readFile, writeFile } from 'fs/promises'

const SAMPLES = 11 // t=0, 0.1, …, 1.0
const CATALOG_PATH = 'client/public/models/objects/catalog.json'
const MODELS_DIR = 'client/public/models'

const io = new NodeIO()
  .registerExtensions([EXTMeshoptCompression, KHRMaterialsUnlit, KHRMeshQuantization])
  .registerDependencies({ 'meshopt.decoder': MeshoptDecoder })

function multiply(a, b) {
  const out = new Array(16)
  for (let i = 0; i < 4; i++)
    for (let j = 0; j < 4; j++) {
      let s = 0
      for (let k = 0; k < 4; k++) s += a[k * 4 + j] * b[i * 4 + k]
      out[i * 4 + j] = s
    }
  return out
}

function applyTransform(accessor, mat) {
  const out = new Float32Array(accessor.getCount() * 3)
  const element = []
  for (let i = 0; i < out.length; i += 3) {
    const [x, y, z] = accessor.getElement(i / 3, element)
    out[i]     = mat[0] * x + mat[4] * y + mat[8]  * z + mat[12]
    out[i + 1] = mat[1] * x + mat[5] * y + mat[9]  * z + mat[13]
    out[i + 2] = mat[2] * x + mat[6] * y + mat[10] * z + mat[14]
  }
  return out
}

function nodeWorld(node) {
  const m = node.getMatrix()
  const parent = node.getParentNode()
  return parent ? multiply(nodeWorld(parent), m) : m
}

// Möller–Trumbore ray-triangle intersection.
function rayTri(ox, oy, oz, dx, dy, dz, ax, ay, az, bx, by, bz, cx, cy, cz) {
  const ex1 = bx - ax, ey1 = by - ay, ez1 = bz - az
  const ex2 = cx - ax, ey2 = cy - ay, ez2 = cz - az
  const px = dy * ez2 - dz * ey2
  const py = dz * ex2 - dx * ez2
  const pz = dx * ey2 - dy * ex2
  const det = ex1 * px + ey1 * py + ez1 * pz
  if (Math.abs(det) < 1e-9) return null
  const inv = 1 / det
  const tx = ox - ax, ty = oy - ay, tz = oz - az
  const u = (tx * px + ty * py + tz * pz) * inv
  if (u < 0 || u > 1) return null
  const qx = ty * ez1 - tz * ey1
  const qy = tz * ex1 - tx * ez1
  const qz = tx * ey1 - ty * ex1
  const v = (dx * qx + dy * qy + dz * qz) * inv
  if (v < 0 || u + v > 1) return null
  return (ex2 * qx + ey2 * qy + ez2 * qz) * inv
}

async function loadTriangles(path) {
  const doc = await io.read(path)
  const root = doc.getRoot()
  const tris = []
  for (const node of root.listNodes()) {
    const mesh = node.getMesh()
    if (!mesh) continue
    const world = nodeWorld(node)
    for (const prim of mesh.listPrimitives()) {
      const pos = prim.getAttribute('POSITION')
      if (!pos) continue
      const idx = prim.getIndices()
      const positions = applyTransform(pos, world)
      const indexArr = idx ? idx.getArray() : null
      const count = indexArr ? indexArr.length : positions.length / 3
      for (let i = 0; i < count; i += 3) {
        const a = (indexArr ? indexArr[i] : i) * 3
        const b = (indexArr ? indexArr[i + 1] : i + 1) * 3
        const c = (indexArr ? indexArr[i + 2] : i + 2) * 3
        tris.push([
          positions[a], positions[a + 1], positions[a + 2],
          positions[b], positions[b + 1], positions[b + 2],
          positions[c], positions[c + 1], positions[c + 2],
        ])
      }
    }
  }
  return tris
}

function sampleDeckY(tris, deckAxis, halfLen, crownY) {
  const samples = new Array(SAMPLES)
  const rayY = crownY + 5 // safely above crown
  for (let i = 0; i < SAMPLES; i++) {
    const t = i / (SAMPLES - 1)
    const along = t * halfLen
    const ox = deckAxis === 'x' ? along : 0
    const oz = deckAxis === 'z' ? along : 0
    let bestT = Infinity
    for (const tri of tris) {
      const hit = rayTri(ox, rayY, oz, 0, -1, 0, ...tri)
      if (hit !== null && hit > 0 && hit < bestT) bestT = hit
    }
    samples[i] = isFinite(bestT) ? +(rayY - bestT).toFixed(4) : null
  }
  return samples
}

const catalogText = await readFile(CATALOG_PATH, 'utf8')
const catalog = JSON.parse(catalogText)

for (const def of catalog) {
  if (def.kind !== 'bridge' || !def.bridge) continue
  const m = def.bridge
  const halfLen =
    m.deckAxis === 'z'
      ? Math.max(Math.abs(m.deckMinZ), Math.abs(m.deckMaxZ))
      : Math.max(Math.abs(m.deckMinX), Math.abs(m.deckMaxX))
  const tris = await loadTriangles(`${MODELS_DIR}/${def.model}`)
  const samples = sampleDeckY(tris, m.deckAxis, halfLen, m.deckCrownY)
  const missing = samples.findIndex((v) => v === null)
  if (missing !== -1) {
    console.warn(`  ${def.id}: no hit at t=${(missing / (SAMPLES - 1)).toFixed(2)} — leaving prior samples`)
    continue
  }
  m.deckYSamples = samples
  console.log(`  ${def.id}: ${samples.map((v) => v.toFixed(2)).join(' ')}`)
}

if (!process.argv.includes('--dry-run')) {
  await writeFile(CATALOG_PATH, JSON.stringify(catalog, null, 2) + '\n', 'utf8')
  console.log(`wrote ${CATALOG_PATH}`)
}

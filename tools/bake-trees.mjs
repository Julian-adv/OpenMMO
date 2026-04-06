/**
 * Bake node transforms into vertex data, normalize height, and convert
 * alpha BLEND → MASK in tree GLB files.
 *
 * Usage: node tools/bake-trees.mjs
 */
import { NodeIO } from '@gltf-transform/core'
import { EXTTextureWebP } from '@gltf-transform/extensions'

const TARGET_HEIGHT = 6
const ALPHA_CUTOFF = 0.5

const io = new NodeIO().registerExtensions([EXTTextureWebP])

/** Multiply a vec3 position by a 4x4 column-major matrix. */
function transformPoint(m, v) {
  return [
    m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12],
    m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13],
    m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14],
  ]
}

/** Multiply a vec3 normal by the upper-left 3x3 of a 4x4 column-major matrix. */
function transformNormal(m, n) {
  const x = m[0] * n[0] + m[4] * n[1] + m[8] * n[2]
  const y = m[1] * n[0] + m[5] * n[1] + m[9] * n[2]
  const z = m[2] * n[0] + m[6] * n[1] + m[10] * n[2]
  const len = Math.sqrt(x * x + y * y + z * z)
  return len > 0 ? [x / len, y / len, z / len] : [0, 1, 0]
}

/** Build column-major 4x4 from TRS. */
function trsToMat4(t, r, s) {
  // Default values
  t = t || [0, 0, 0]
  r = r || [0, 0, 0, 1]
  s = s || [1, 1, 1]

  const [qx, qy, qz, qw] = r
  const [sx, sy, sz] = s

  const x2 = qx + qx, y2 = qy + qy, z2 = qz + qz
  const xx = qx * x2, xy = qx * y2, xz = qx * z2
  const yy = qy * y2, yz = qy * z2, zz = qz * z2
  const wx = qw * x2, wy = qw * y2, wz = qw * z2

  return [
    (1 - (yy + zz)) * sx, (xy + wz) * sx, (xz - wy) * sx, 0,
    (xy - wz) * sy, (1 - (xx + zz)) * sy, (yz + wx) * sy, 0,
    (xz + wy) * sz, (yz - wx) * sz, (1 - (xx + yy)) * sz, 0,
    t[0], t[1], t[2], 1,
  ]
}

/** Multiply two 4x4 column-major matrices. */
function mat4Mul(a, b) {
  const out = new Array(16)
  for (let i = 0; i < 4; i++) {
    for (let j = 0; j < 4; j++) {
      out[j * 4 + i] =
        a[i] * b[j * 4] +
        a[4 + i] * b[j * 4 + 1] +
        a[8 + i] * b[j * 4 + 2] +
        a[12 + i] * b[j * 4 + 3]
    }
  }
  return out
}

/** Recursively compute world matrix for a node, traversing parents. */
function getWorldMatrix(node) {
  const local = trsToMat4(
    node.getTranslation(),
    node.getRotation(),
    node.getScale()
  )
  const parent = node.getParentNode()
  if (!parent) return local
  return mat4Mul(getWorldMatrix(parent), local)
}

for (const file of [
  'client/public/models/tree.glb',
  'client/public/models/tree2.glb',
]) {
  const doc = await io.read(file)
  const root = doc.getRoot()

  // 1) Bake each node's world transform into its mesh vertex data
  for (const node of root.listNodes()) {
    const mesh = node.getMesh()
    if (!mesh) continue

    const worldMat = getWorldMatrix(node)

    for (const prim of mesh.listPrimitives()) {
      const pos = prim.getAttribute('POSITION')
      if (pos) {
        const v = [0, 0, 0]
        for (let i = 0; i < pos.getCount(); i++) {
          pos.getElement(i, v)
          const tv = transformPoint(worldMat, v)
          pos.setElement(i, tv)
        }
      }

      const norm = prim.getAttribute('NORMAL')
      if (norm) {
        const n = [0, 0, 0]
        for (let i = 0; i < norm.getCount(); i++) {
          norm.getElement(i, n)
          const tn = transformNormal(worldMat, n)
          norm.setElement(i, tn)
        }
      }
    }

    // Clear node transform
    node.setTranslation([0, 0, 0])
    node.setRotation([0, 0, 0, 1])
    node.setScale([1, 1, 1])
  }

  // 2) Compute bounding box from baked positions
  let minX = Infinity, minY = Infinity, minZ = Infinity
  let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity

  for (const mesh of root.listMeshes()) {
    for (const prim of mesh.listPrimitives()) {
      const pos = prim.getAttribute('POSITION')
      if (!pos) continue
      const v = [0, 0, 0]
      for (let i = 0; i < pos.getCount(); i++) {
        pos.getElement(i, v)
        minX = Math.min(minX, v[0])
        minY = Math.min(minY, v[1])
        minZ = Math.min(minZ, v[2])
        maxX = Math.max(maxX, v[0])
        maxY = Math.max(maxY, v[1])
        maxZ = Math.max(maxZ, v[2])
      }
    }
  }

  const sizeY = maxY - minY
  const centerX = (minX + maxX) / 2
  const centerZ = (minZ + maxZ) / 2
  const scale = TARGET_HEIGHT / sizeY

  console.log(`${file}: world bounds Y=[${minY.toFixed(2)}, ${maxY.toFixed(2)}] height=${sizeY.toFixed(2)} → scale=${scale.toFixed(4)}`)

  // 3) Normalize: center XZ, bottom Y=0, scale to TARGET_HEIGHT
  for (const mesh of root.listMeshes()) {
    for (const prim of mesh.listPrimitives()) {
      const pos = prim.getAttribute('POSITION')
      if (!pos) continue
      const v = [0, 0, 0]
      for (let i = 0; i < pos.getCount(); i++) {
        pos.getElement(i, v)
        v[0] = (v[0] - centerX) * scale
        v[1] = (v[1] - minY) * scale
        v[2] = (v[2] - centerZ) * scale
        pos.setElement(i, v)
      }
    }
  }

  // 4) Convert alpha BLEND → MASK
  for (const mat of root.listMaterials()) {
    if (mat.getAlphaMode() === 'BLEND') {
      mat.setAlphaMode('MASK')
      mat.setAlphaCutoff(ALPHA_CUTOFF)
      console.log(`  ${mat.getName()}: BLEND → MASK (cutoff=${ALPHA_CUTOFF})`)
    }
  }

  await io.write(file, doc)
  console.log(`  saved: ${file}`)
}

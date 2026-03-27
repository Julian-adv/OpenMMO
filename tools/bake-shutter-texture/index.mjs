/**
 * bake-shutter-texture — Pre-bake composite shutter texture (wood frame + linen center)
 *
 * Reads wood_shutter_1k.glb and rough_linen_1k.glb, composites them into a
 * shutter panel texture (diffuse, normal, ORM), and writes shutter_panel_1k.glb.
 *
 * Usage:
 *   cd tools/bake-shutter-texture && npm install && npm run bake
 */
import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import { createCanvas, loadImage } from 'canvas'
import { Document, NodeIO } from '@gltf-transform/core'
import { ALL_EXTENSIONS } from '@gltf-transform/extensions'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const TEXTURES_DIR = path.join(__dirname, '../../client/public/textures/housing')
const OUTPUT_PATH = path.join(TEXTURES_DIR, 'shutter_panel_1k.glb')

// Shutter geometry constants (must match house-geo-walls.ts)
const SHUTTER_BAR = 0.03
const SHUTTER_BORDER = 0.045
const WINDOW_WIDTH = 0.8
const DEFAULT_WALL_HEIGHT = 3
const FRAME_BEAM_Y_FRAC = 0.4
const FRAME_BEAM_FRAC = 0.05
const WINDOW_BOTTOM = 1.2
const WINDOW_HEIGHT = 1.0
const FRAME_DIAG_THICKNESS = 0.06

const halfW = WINDOW_WIDTH / 2
const beamTop =
  DEFAULT_WALL_HEIGHT * FRAME_BEAM_Y_FRAC +
  (DEFAULT_WALL_HEIGHT * FRAME_BEAM_FRAC) / 2
const headerBot = WINDOW_BOTTOM + WINDOW_HEIGHT - FRAME_DIAG_THICKNESS / 2
const panelH = headerBot - beamTop

const BORDER_FRAC_U = SHUTTER_BORDER / halfW
const BORDER_FRAC_V = SHUTTER_BORDER / panelH
const CROSS_FRAC_U = SHUTTER_BAR / halfW
const CROSS_FRAC_V = SHUTTER_BAR / panelH

const SIZE = 256

// ---------------------------------------------------------------------------
// GLB texture extraction
// ---------------------------------------------------------------------------

async function extractTexturesFromGLB(glbPath) {
  const io = new NodeIO().registerExtensions(ALL_EXTENSIONS)
  const doc = await io.read(glbPath)
  const root = doc.getRoot()
  const materials = root.listMaterials()
  if (materials.length === 0) throw new Error(`No materials in ${glbPath}`)

  const mat = materials[0]
  const result = {}

  const baseColorTex = mat.getBaseColorTexture()
  if (baseColorTex) {
    result.diffuse = await loadImage(Buffer.from(baseColorTex.getImage()))
  }

  const normalTex = mat.getNormalTexture()
  if (normalTex) {
    result.normal = await loadImage(Buffer.from(normalTex.getImage()))
  }

  // metallicRoughness texture (G=roughness, B=metallic)
  const mrTex = mat.getMetallicRoughnessTexture()
  if (mrTex) {
    result.mr = await loadImage(Buffer.from(mrTex.getImage()))
  }

  // AO texture
  const aoTex = mat.getOcclusionTexture()
  if (aoTex) {
    result.ao = await loadImage(Buffer.from(aoTex.getImage()))
  }

  return result
}

// ---------------------------------------------------------------------------
// Canvas compositing helpers
// ---------------------------------------------------------------------------

function clipBarRegion(ctx, bdU, bdV, crU, crV, size) {
  ctx.beginPath()
  ctx.rect(0, 0, bdU, size) // left border
  ctx.rect(size - bdU, 0, bdU, size) // right border
  ctx.rect(0, 0, size, bdV) // bottom border
  ctx.rect(0, size - bdV, size, bdV) // top border
  ctx.rect(bdU, (size - crV) / 2, size - bdU * 2, crV) // h-cross (inside borders)
  ctx.rect((size - crU) / 2, bdV, crU, size - bdV * 2) // v-cross (inside borders)
  ctx.clip()
}

function compositeBarCanvas(woodImg, linenImg, fallbackWood, fallbackLinen) {
  const canvas = createCanvas(SIZE, SIZE)
  const ctx = canvas.getContext('2d')

  const bdU = Math.round(BORDER_FRAC_U * SIZE)
  const bdV = Math.round(BORDER_FRAC_V * SIZE)
  const crU = Math.round(CROSS_FRAC_U * SIZE)
  const crV = Math.round(CROSS_FRAC_V * SIZE)

  // Linen background
  if (linenImg) ctx.drawImage(linenImg, 0, 0, SIZE, SIZE)
  else {
    ctx.fillStyle = fallbackLinen
    ctx.fillRect(0, 0, SIZE, SIZE)
  }

  // Clip to bar regions and draw wood
  ctx.save()
  clipBarRegion(ctx, bdU, bdV, crU, crV, SIZE)
  if (woodImg) ctx.drawImage(woodImg, 0, 0, SIZE, SIZE)
  else {
    ctx.fillStyle = fallbackWood
    ctx.fillRect(0, 0, SIZE, SIZE)
  }
  ctx.restore()

  return canvas
}

function addEdgeAO(canvas) {
  const ctx = canvas.getContext('2d')
  const bdU = Math.round(BORDER_FRAC_U * SIZE)
  const bdV = Math.round(BORDER_FRAC_V * SIZE)
  const crU = Math.round(CROSS_FRAC_U * SIZE)
  const crV = Math.round(CROSS_FRAC_V * SIZE)

  const scale = SIZE / 256
  ctx.strokeStyle = 'rgba(0, 0, 0, 0.25)'
  ctx.lineWidth = 4 * scale
  const edges = [
    // Border inner edges (full span)
    [bdU, 0, bdU, SIZE],
    [SIZE - bdU, 0, SIZE - bdU, SIZE],
    [0, bdV, SIZE, bdV],
    [0, SIZE - bdV, SIZE, SIZE - bdV],
    // Cross edges (only between borders)
    [bdU, (SIZE - crV) / 2, SIZE - bdU, (SIZE - crV) / 2],
    [bdU, (SIZE + crV) / 2, SIZE - bdU, (SIZE + crV) / 2],
    [(SIZE - crU) / 2, bdV, (SIZE - crU) / 2, SIZE - bdV],
    [(SIZE + crU) / 2, bdV, (SIZE + crU) / 2, SIZE - bdV],
  ]
  for (const [x1, y1, x2, y2] of edges) {
    ctx.beginPath()
    ctx.moveTo(x1, y1)
    ctx.lineTo(x2, y2)
    ctx.stroke()
  }
}

function addBarEdgeNormals(canvas) {
  const ctx = canvas.getContext('2d')
  const bdU = Math.round(BORDER_FRAC_U * SIZE)
  const bdV = Math.round(BORDER_FRAC_V * SIZE)
  const crU = Math.round(CROSS_FRAC_U * SIZE)
  const crV = Math.round(CROSS_FRAC_V * SIZE)

  // 85° from surface
  const lo = 2
  const hi = 254
  const flat = 128
  const z = 139 // cos(85°) * 127 + 128

  const scale = SIZE / 256
  const offset = Math.round(1 * scale)
  ctx.lineWidth = 2 * scale

  // Vertical edges: [x, r_channel, y_start, y_end]
  const vEdges = [
    // Border edges (full height)
    [bdU - offset, hi, 0, SIZE],
    [SIZE - bdU + offset, lo, 0, SIZE],
    // Cross edges (only between top/bottom borders)
    [(SIZE - crU) / 2 + offset, lo, bdV, SIZE - bdV],
    [(SIZE + crU) / 2 - offset, hi, bdV, SIZE - bdV],
  ]
  for (const [x, r, y0, y1] of vEdges) {
    ctx.strokeStyle = `rgb(${r},${flat},${z})`
    ctx.beginPath()
    ctx.moveTo(x, y0)
    ctx.lineTo(x, y1)
    ctx.stroke()
  }

  // Horizontal edges: [y, g_channel, x_start, x_end]
  const hEdges = [
    // Border edges (full width)
    [bdV - offset, hi, 0, SIZE],
    [SIZE - bdV + offset, lo, 0, SIZE],
    // Cross edges (only between left/right borders)
    [(SIZE - crV) / 2 + offset, lo, bdU, SIZE - bdU],
    [(SIZE + crV) / 2 - offset, hi, bdU, SIZE - bdU],
  ]
  for (const [y, g, x0, x1] of hEdges) {
    ctx.strokeStyle = `rgb(${flat},${g},${z})`
    ctx.beginPath()
    ctx.moveTo(x0, y)
    ctx.lineTo(x1, y)
    ctx.stroke()
  }
}

function packORM(aoImg, mrImg) {
  const canvas = createCanvas(SIZE, SIZE)
  const ctx = canvas.getContext('2d')

  // Default: R=255(full AO), G=220(rough), B=0(non-metallic)
  ctx.fillStyle = 'rgb(255,220,0)'
  ctx.fillRect(0, 0, SIZE, SIZE)

  if (mrImg) {
    const mrc = createCanvas(SIZE, SIZE)
    const mctx = mrc.getContext('2d')
    mctx.drawImage(mrImg, 0, 0, SIZE, SIZE)
    const mrData = mctx.getImageData(0, 0, SIZE, SIZE).data

    const imgData = ctx.getImageData(0, 0, SIZE, SIZE)
    const data = imgData.data
    for (let i = 0; i < data.length; i += 4) {
      data[i + 1] = mrData[i + 1] // G = roughness
      data[i + 2] = mrData[i + 2] // B = metallic
    }
    ctx.putImageData(imgData, 0, 0)
  }

  if (aoImg) {
    const aoc = createCanvas(SIZE, SIZE)
    const actx = aoc.getContext('2d')
    actx.drawImage(aoImg, 0, 0, SIZE, SIZE)
    const aoData = actx.getImageData(0, 0, SIZE, SIZE).data

    const imgData = ctx.getImageData(0, 0, SIZE, SIZE)
    const data = imgData.data
    for (let i = 0; i < data.length; i += 4) {
      data[i] = aoData[i] // R = AO
    }
    ctx.putImageData(imgData, 0, 0)
  }

  return canvas
}

// ---------------------------------------------------------------------------
// GLB output
// ---------------------------------------------------------------------------

function canvasToPNG(canvas) {
  const buf = canvas.toBuffer('image/png')
  return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength)
}

async function writeGLB(diffuseCanvas, normalCanvas, ormCanvas) {
  const doc = new Document()
  const buffer = doc.createBuffer('main')

  const diffuseTex = doc
    .createTexture('diffuse')
    .setImage(canvasToPNG(diffuseCanvas))
    .setMimeType('image/png')

  const normalTex = doc
    .createTexture('normal')
    .setImage(canvasToPNG(normalCanvas))
    .setMimeType('image/png')

  const ormTex = doc
    .createTexture('orm')
    .setImage(canvasToPNG(ormCanvas))
    .setMimeType('image/png')

  const mat = doc
    .createMaterial('shutter_panel')
    .setBaseColorTexture(diffuseTex)
    .setNormalTexture(normalTex)
    .setMetallicRoughnessTexture(ormTex)
    .setOcclusionTexture(ormTex)
    .setRoughnessFactor(0.85)
    .setMetallicFactor(0.0)

  // GLB requires at least one mesh with vertex data; create a minimal triangle
  const posAccessor = doc
    .createAccessor('position')
    .setType('VEC3')
    .setArray(new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]))
    .setBuffer(buffer)
  const prim = doc.createPrimitive().setMaterial(mat).setAttribute('POSITION', posAccessor)
  const mesh = doc.createMesh('shutter').addPrimitive(prim)
  const node = doc.createNode('shutter').setMesh(mesh)
  doc.createScene('Scene').addChild(node)

  const io = new NodeIO().registerExtensions(ALL_EXTENSIONS)
  const glb = await io.writeBinary(doc)
  return glb
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  console.log('Loading source textures...')

  const woodTextures = await extractTexturesFromGLB(
    path.join(TEXTURES_DIR, 'wood_shutter_1k.glb')
  )
  const linenTextures = await extractTexturesFromGLB(
    path.join(TEXTURES_DIR, 'rough_linen_1k.glb')
  )

  console.log(
    `  wood: diffuse=${!!woodTextures.diffuse} normal=${!!woodTextures.normal} mr=${!!woodTextures.mr} ao=${!!woodTextures.ao}`
  )
  console.log(
    `  linen: diffuse=${!!linenTextures.diffuse} normal=${!!linenTextures.normal} mr=${!!linenTextures.mr} ao=${!!linenTextures.ao}`
  )
  console.log(
    `  border frac: U=${BORDER_FRAC_U.toFixed(4)} V=${BORDER_FRAC_V.toFixed(4)}`
  )
  console.log(
    `  cross frac:  U=${CROSS_FRAC_U.toFixed(4)} V=${CROSS_FRAC_V.toFixed(4)}`
  )

  // Diffuse
  console.log('Compositing diffuse...')
  const diffuseCanvas = compositeBarCanvas(
    woodTextures.diffuse,
    linenTextures.diffuse,
    '#6b5a3e',
    '#c8b898'
  )
  addEdgeAO(diffuseCanvas)

  // Normal
  console.log('Compositing normal...')
  const normalCanvas = compositeBarCanvas(
    woodTextures.normal,
    linenTextures.normal,
    'rgb(128,128,255)',
    'rgb(128,128,255)'
  )
  addBarEdgeNormals(normalCanvas)

  // ORM: composite AO and MR channels separately, then pack
  console.log('Compositing ORM...')
  const woodORM = packORM(woodTextures.ao, woodTextures.mr)
  const linenORM = packORM(linenTextures.ao, linenTextures.mr)
  const ormCanvas = compositeBarCanvas(woodORM, linenORM, 'rgb(255,220,0)', 'rgb(255,220,0)')

  // Write GLB
  console.log('Writing GLB...')
  const glb = await writeGLB(diffuseCanvas, normalCanvas, ormCanvas)
  fs.writeFileSync(OUTPUT_PATH, Buffer.from(glb))
  console.log(`Done! Written to ${OUTPUT_PATH} (${(glb.byteLength / 1024).toFixed(1)} KB)`)
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})

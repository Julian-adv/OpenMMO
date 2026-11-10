import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile, writeFile } from 'node:fs/promises'
import sharp from 'sharp'
import { NodeIO, PropertyType } from '@gltf-transform/core'
import { ALL_EXTENSIONS, EXTMeshoptCompression } from '@gltf-transform/extensions'
import { prune } from '@gltf-transform/functions'
import { MeshoptDecoder, MeshoptEncoder } from 'meshoptimizer'

const directory = 'client/public/models/characters/modular_female'
// `--face ranger|chic` builds a bald-head face part; the default is the older custom face.
const face = process.argv.includes('--face') ? process.argv[process.argv.indexOf('--face') + 1] : 'custom'
const dates = { custom: '2026-11-08', ranger: '2026-11-10', chic: '2026-11-11' }
assert.ok(Object.hasOwn(dates, face))
const bald = face !== 'custom'
const folders = { chic: 'tripo_chic_v2' }
const source = bald
  ? `assets/modular_human_female_01/faces/${folders[face] ?? `tripo_${face}_v1`}/base_${face}.glb`
  : 'assets/modular_human_female_01/faces/tripo_custom_v1/base_custom_preview.glb'
// `--size 512` builds the part the game view loads; 1024 is the close-up part.
const size = process.argv.includes('--size') ? Number(process.argv[process.argv.indexOf('--size') + 1]) : 1024
assert.ok(size === 1024 || (bald && size === 512))
const tier = size === 1024 ? '' : `_${size}`
const output = `${directory}/face_${face}${tier}.glb`
await Promise.all([MeshoptDecoder.ready, MeshoptEncoder.ready])
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({
  'meshopt.decoder': MeshoptDecoder, 'meshopt.encoder': MeshoptEncoder,
})
const [base, custom] = await Promise.all([io.read(`${directory}/base.glb`), io.read(source)])
const original = base.getRoot().listMeshes()[0].listPrimitives()[0]
const body = custom.getRoot().listMeshes()[0].listPrimitives()[0]
const point = (primitive, id) => primitive.getAttribute('POSITION').getElement(id, [])
const uv = (primitive, id) => primitive.getAttribute('TEXCOORD_0').getElement(id, [])
const uvKey = values => values.map(value => Math.round(value * 1e6)).join(',')
const triangleKey = ids => ids.slice().sort((a, b) => a - b).join(',')
const originals = new Map()
for (let id = 0; id < original.getAttribute('POSITION').getCount(); id++) {
  const key = uvKey(uv(original, id))
  if (!originals.has(key)) originals.set(key, [])
  originals.get(key).push(id)
}
const vertices = Array.from({ length: body.getAttribute('POSITION').getCount() }, (_, id) => {
  const candidates = originals.get(uvKey(uv(body, id))) ?? []
  const p = point(body, id)
  candidates.sort((a, b) => distance(point(original, a), p) - distance(point(original, b), p))
  const sourceId = candidates[0] ?? -1
  if (p[1] < 1.502) assert.ok(sourceId >= 0 && distance(point(original, sourceId), p) < 1e-10)
  return sourceId
})
function distance(a, b) { return a.reduce((sum, value, i) => sum + (value - b[i]) ** 2, 0) }
const sourceIndices = original.getIndices().getArray()
const sourceTriangles = new Map()
const uvTriangles = new Map()
const neck = []
for (let i = 0; i < sourceIndices.length; i += 3) {
  const ids = Array.from(sourceIndices.slice(i, i + 3))
  sourceTriangles.set(triangleKey(ids), i / 3)
  uvTriangles.set(ids.map(id => uvKey(uv(original, id))).sort().join(';'), i / 3)
  if (ids.some(id => point(original, id)[1] >= 1.502) && ids.some(id => point(original, id)[1] <= 1.552))
    neck.push({ index: i / 3, ids, uv: ids.map(id => uv(original, id)) })
}
function contains(p, triangle) {
  const [a, b, c] = triangle
  const d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
  if (Math.abs(d) < 1e-15) return false
  const u = ((b[1] - c[1]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[1])) / d
  const v = ((c[1] - a[1]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[1])) / d
  return u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001
}
const indices = body.getIndices().getArray()
const triangles = []
for (let i = 0; i < indices.length; i += 3) {
  const ids = Array.from(indices.slice(i, i + 3))
  const parentIds = ids.map(id => vertices[id])
  let sourceId = sourceTriangles.get(triangleKey(parentIds)) ?? uvTriangles.get(ids.map(id => uvKey(uv(body, id))).sort().join(';'))
  if (sourceId === undefined) {
    assert.ok(ids.every(id => point(body, id)[1] >= 1.502))
    const center = [0, 1].map(axis => ids.reduce((sum, id) => sum + uv(body, id)[axis] / 3, 0))
    const matches = neck.filter(triangle => contains(center, triangle.uv))
    matches.sort((a, b) => parentIds.filter(id => b.ids.includes(id)).length - parentIds.filter(id => a.ids.includes(id)).length)
    assert.ok(matches.length, `Unmapped neck triangle ${i / 3}`)
    sourceId = matches[0].index
  }
  triangles.push(sourceId)
}
const inputs = {}
const record = async path => { inputs[path] = createHash('sha256').update(await readFile(path)).digest('hex') }
await Promise.all([source, `${directory}/base.glb`].map(record))

// Triangles reaching the neck fit (y >= 1.502 m) ship in the part; the rest match base.glb exactly.
const stripTriangles = []
for (let t = 0; t < indices.length / 3; t++)
  if ([0, 1, 2].some(k => point(body, indices[t * 3 + k])[1] >= 1.502)) stripTriangles.push(t)
const kept = new Set()
for (let t = 0; t < indices.length / 3; t++) if (!stripTriangles.includes(t)) kept.add(triangles[t])
assert.equal(kept.size, indices.length / 3 - stripTriangles.length)
const hideTriangles = []
for (let t = 0; t < sourceIndices.length / 3; t++) if (!kept.has(t)) hideTriangles.push(t)
for (let t = 0; t < sourceIndices.length / 3; t++)
  assert.equal(!kept.has(t), [0, 1, 2].some(k => point(original, sourceIndices[t * 3 + k])[1] >= 1.502))
const stripSources = stripTriangles.map(t => triangles[t])
assert.ok(stripSources.every(t => !kept.has(t)))

const stripVertices = [...new Set(stripTriangles.flatMap(t => [0, 1, 2].map(k => indices[t * 3 + k])))].sort((a, b) => a - b)
const remap = new Map(stripVertices.map((id, i) => [id, i]))
for (const id of stripVertices) {
  if (point(body, id)[1] >= 1.502) continue
  const s = vertices[id]
  for (const semantic of ['POSITION', 'TEXCOORD_0', 'WEIGHTS_0'])
    assert.deepEqual(body.getAttribute(semantic).getElement(id, []), original.getAttribute(semantic).getElement(s, []), semantic)
  const w = body.getAttribute('WEIGHTS_0').getElement(id, [])
  const a = body.getAttribute('JOINTS_0').getElement(id, []), b = original.getAttribute('JOINTS_0').getElement(s, [])
  assert.ok(a.every((joint, k) => joint === b[k] || w[k] === 0))
}
const strip = custom.createPrimitive().setMaterial(body.getMaterial().clone().setName('Custom face neck strip'))
for (const semantic of body.listSemantics()) {
  const from = body.getAttribute(semantic)
  const array = new (from.getArray().constructor)(stripVertices.length * from.getElementSize())
  for (const [i, id] of stripVertices.entries()) array.set(from.getElement(id, []), i * from.getElementSize())
  strip.setAttribute(semantic, custom.createAccessor().setType(from.getType()).setArray(array).setNormalized(from.getNormalized()).setBuffer(from.getBuffer()))
}
strip.setIndices(custom.createAccessor().setType('SCALAR').setBuffer(body.getIndices().getBuffer())
  .setArray(Uint16Array.from(stripTriangles.flatMap(t => [0, 1, 2].map(k => remap.get(indices[t * 3 + k]))))))

// Keep the atlas layout and flatten every texel the strip never samples, so the textures compress to little.
async function stripTexture(image, quality) {
  const { data, info } = await sharp(image).removeAlpha().raw().toBuffer({ resolveWithObject: true })
  const { width, height } = info
  let mask = new Uint8Array(width * height)
  const uvs = strip.getAttribute('TEXCOORD_0'), list = strip.getIndices().getArray()
  for (let i = 0; i < list.length; i += 3) {
    const p = [0, 1, 2].map(k => uvs.getElement(list[i + k], []).map((v, axis) => v * (axis ? height : width)))
    const [x0, x1] = [Math.floor(Math.min(...p.map(q => q[0]))), Math.ceil(Math.max(...p.map(q => q[0])))]
    const [y0, y1] = [Math.floor(Math.min(...p.map(q => q[1]))), Math.ceil(Math.max(...p.map(q => q[1])))]
    for (let y = Math.max(0, y0); y <= Math.min(height - 1, y1); y++)
      for (let x = Math.max(0, x0); x <= Math.min(width - 1, x1); x++) mask[y * width + x] = 1
  }
  const pad = Math.round(width / 128)
  for (let pass = 0; pass < 2; pass++) {
    const next = new Uint8Array(mask)
    for (let y = 0; y < height; y++)
      for (let x = 0; x < width; x++) {
        if (!mask[y * width + x]) continue
        for (let d = 1; d <= pad; d++) {
          const [nx, ny] = pass ? [x, y + d] : [x + d, y]
          const [mx, my] = pass ? [x, y - d] : [x - d, y]
          if (nx < width && ny < height) next[ny * width + nx] = 1
          if (mx >= 0 && my >= 0) next[my * width + mx] = 1
        }
      }
    mask = next
  }
  const sum = [0, 0, 0]; let count = 0
  for (let i = 0; i < mask.length; i++) if (mask[i]) { count++; for (let c = 0; c < 3; c++) sum[c] += data[i * 3 + c] }
  for (let i = 0; i < mask.length; i++) if (!mask[i]) for (let c = 0; c < 3; c++) data[i * 3 + c] = Math.round(sum[c] / count)
  const flattened = sharp(data, { raw: { width, height, channels: 3 } })
  // The bald-head fits work on 4096 maps; the strip ships at the body's 1024, or at the game part's size.
  return (bald && width > size ? flattened.resize(size, size, { kernel: 'lanczos3' }) : flattened).webp(quality).toBuffer()
}
const baseMaterial = original.getMaterial()
const stripMaterial = strip.getMaterial()
const texture = (image, name) => custom.createTexture(name).setImage(new Uint8Array(image)).setMimeType('image/webp')
stripMaterial.setBaseColorTexture(texture(await stripTexture(body.getMaterial().getBaseColorTexture().getImage(), { quality: 94, effort: 5 }), 'Neck strip color'))
stripMaterial.setNormalTexture(texture(await stripTexture(baseMaterial.getNormalTexture().getImage(), { quality: 90, smartSubsample: true, effort: 5 }), 'Neck strip normal'))
// The bald-head fits blend the body's roughness toward the head's near the seam.
const stripOrm = bald ? body.getMaterial() : baseMaterial
stripMaterial.setMetallicRoughnessTexture(texture(await stripTexture(stripOrm.getMetallicRoughnessTexture().getImage(), { quality: 90, smartSubsample: true, effort: 5 }), 'Neck strip ORM'))
for (const slot of ['getBaseColorTextureInfo', 'getNormalTextureInfo', 'getMetallicRoughnessTextureInfo'])
  assert.equal(stripMaterial[slot]().getTexCoord(), 0)

const bodyMesh = custom.getRoot().listMeshes()[0]
bodyMesh.setName(`neck_female_${face}`)
body.dispose()
bodyMesh.addPrimitive(strip)
for (const node of custom.getRoot().listNodes()) {
  if (!node.getMesh()) continue
  const name = node.getMesh().getName()
  node.setExtras({ ...node.getExtras(), part_id: name, female_face: name === `face_female_${face}`, female_neck: name === `neck_female_${face}` })
}
for (const material of custom.getRoot().listMaterials())
  if (material.getAlphaMode() !== 'OPAQUE') material.setExtras({ ...material.getExtras(), appearance_color_fixed: true })
if (bald) {
  // The flat-coloured lashes take no tint. Head maps ship at 1024 for close-ups and 512 for the game view (user decision 2026-11-10).
  for (const mesh of custom.getRoot().listMeshes()) {
    if (mesh === bodyMesh) continue
    const material = mesh.listPrimitives()[0].getMaterial()
    if (mesh.getName().startsWith('eyelashes')) material.setExtras({ ...material.getExtras(), appearance_color_fixed: true })
    for (const map of [material.getBaseColorTexture(), material.getNormalTexture(), material.getMetallicRoughnessTexture()]) {
      if (!map) continue
      map.setImage(new Uint8Array(await sharp(map.getImage()).resize(size, size, { kernel: 'lanczos3' })
        .webp({ quality: 92, smartSubsample: true, effort: 5 }).toBuffer())).setMimeType('image/webp')
    }
  }
}
const scene = custom.getRoot().listScenes()[0]
scene.setExtras({ ...scene.getExtras(), female_part: `face_${face}`, female_face_part: {
  body_triangles: sourceIndices.length / 3, hide_triangle_indices: hideTriangles, strip_source_triangles: stripSources,
} })
for (const animation of custom.getRoot().listAnimations()) animation.dispose()
await custom.transform(prune({ propertyTypes: [PropertyType.ACCESSOR, PropertyType.TEXTURE, PropertyType.MATERIAL], keepAttributes: true, keepIndices: true }))
custom.createExtension(EXTMeshoptCompression).setRequired(true).setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE })
await writeFile(output, await io.writeBinary(custom))
const packed = await io.read(output)
for (const [i, mesh] of custom.getRoot().listMeshes().entries()) {
  const before = mesh.listPrimitives()[0]
  const after = packed.getRoot().listMeshes()[i].listPrimitives()[0]
  for (const semantic of before.listSemantics())
    assert.deepEqual(after.getAttribute(semantic).getArray(), before.getAttribute(semantic).getArray())
  const a = before.getIndices().getArray(), b = after.getIndices().getArray()
  assert.equal(a.length, b.length)
  for (let j = 0; j < a.length; j += 3)
    assert.ok([0, 1, 2].some(shift => [0, 1, 2].every(corner => a[j + corner] === b[j + (corner + shift) % 3])))
}
for (const [i, texture] of custom.getRoot().listTextures().entries())
  assert.deepEqual(packed.getRoot().listTextures()[i].getImage(), texture.getImage())
assert.deepEqual(custom.getRoot().listSkins()[0].getInverseBindMatrices().getArray(), base.getRoot().listSkins()[0].getInverseBindMatrices().getArray())
const thumbnails = {}
for (const name of bald ? [] : ['default', 'custom']) {
  const path = `client/src/assets/character-appearance/female-face-${name}.png`
  thumbnails[path] = createHash('sha256').update(await readFile(path)).digest('hex')
}
const report = {
  date: dates[face], source, output, inputs,
  output_sha256: createHash('sha256').update(await readFile(output)).digest('hex'),
  output_bytes: (await readFile(output)).length,
  base_body_triangles: sourceIndices.length / 3, hidden_base_triangles: hideTriangles.length,
  neck_strip_triangles: stripTriangles.length, neck_strip_vertices: stripVertices.length,
  head_and_eyelash_geometry_and_textures_preserved: !bald,
  head_textures: bald ? `${size} WebP q92` : 'source images',
  neck_strip: 'Custom body triangles at or above 1.502 m with their fitted positions; the neck-matched color and base.glb normal and ORM, with untouched texels flattened. Seam vertices below 1.502 m match base.glb in position, UV and weights.',
  outfit_masks: 'base.glb body masks apply unchanged; strip triangles follow the base triangle each one replaces',
  inverse_bind_matrices_match_canonical_body: true,
  thumbnails: { generator: 'Blender 5.2 EEVEE; tools/blender-scripts/render_female_hair_thumbnail.py; 512px render downsampled to 128px', files: thumbnails },
  animations: 'Reuse canonical female base clips, boots companions and animation packs',
  runtime: 'Bound onto base.glb as a part; base head, neck and default eyelashes hidden',
  appearance: 'Accepted fitted body, head and eyelash geometry and compressed PBR textures preserved',
}
await writeFile(`doc/assets/modular-female-${face}-face-runtime${tier.replace('_', '-')}.json`, JSON.stringify(report, null, 2) + '\n')
console.log(JSON.stringify(report))

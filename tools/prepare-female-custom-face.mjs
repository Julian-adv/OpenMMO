import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readdir, readFile, writeFile } from 'node:fs/promises'
import { NodeIO, PropertyType } from '@gltf-transform/core'
import { ALL_EXTENSIONS, EXTMeshoptCompression } from '@gltf-transform/extensions'
import { prune } from '@gltf-transform/functions'
import { MeshoptDecoder, MeshoptEncoder } from 'meshoptimizer'

const directory = 'client/public/models/characters/modular_female'
const source = 'assets/modular_human_female_01/faces/tripo_custom_v1/base_custom_preview.glb'
const output = `${directory}/base_custom.glb`
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
const masks = {}
for (const file of (await readdir(directory)).sort()) {
  if (!file.endsWith('.glb') || file === 'base_custom.glb') continue
  const path = `${directory}/${file}`
  const document = await io.read(path)
  const mask = document.getRoot().listScenes()[0].getExtras().body_mask
  if (!mask) continue
  await record(path)
  assert.equal(mask.primitive_triangles * 3, sourceIndices.length)
  const hidden = new Set(mask.hide_triangle_indices)
  const adjustment = new Map((mask.vertex_adjustments ?? []).map(item => [item.vertex, item]))
  const remapped = {
    ...mask,
    primitive_triangles: triangles.length,
    hide_triangle_indices: triangles.flatMap((id, i) => hidden.has(id) ? [i] : []),
  }
  if (mask.vertex_adjustments) remapped.vertex_adjustments = vertices.flatMap((id, i) => adjustment.has(id) ? [{ ...adjustment.get(id), vertex: i }] : [])
  if (mask.clip_below_y) {
    const clipped = new Set(mask.clip_below_y.triangle_indices)
    remapped.clip_below_y = { ...mask.clip_below_y, triangle_indices: triangles.flatMap((id, i) => clipped.has(id) ? [i] : []) }
  }
  masks[file.slice(0, -4)] = remapped
}
for (const node of custom.getRoot().listNodes()) {
  if (!node.getMesh()) continue
  const name = node.getMesh().getName()
  node.setExtras({ ...node.getExtras(), female_body: name === 'female_body', female_face: name === 'face_female_custom' })
}
for (const material of custom.getRoot().listMaterials())
  if (material.getAlphaMode() !== 'OPAQUE') material.setExtras({ ...material.getExtras(), appearance_color_fixed: true })
const scene = custom.getRoot().listScenes()[0]
scene.setExtras({ ...scene.getExtras(), female_face_body_masks: masks })
for (const animation of custom.getRoot().listAnimations()) animation.dispose()
await custom.transform(prune({ propertyTypes: [PropertyType.ACCESSOR], keepAttributes: true, keepIndices: true }))
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
for (const face of ['default', 'custom']) {
  const path = `client/src/assets/character-appearance/female-face-${face}.png`
  thumbnails[path] = createHash('sha256').update(await readFile(path)).digest('hex')
}
const report = {
  date: '2026-11-08', source, output, inputs,
  output_sha256: createHash('sha256').update(await readFile(output)).digest('hex'),
  output_bytes: (await readFile(output)).length, body_triangles: triangles.length,
  remapped_outfit_masks: Object.keys(masks),
  geometry_and_texture_bytes_preserved: true,
  inverse_bind_matrices_match_canonical_body: true,
  thumbnails: { generator: 'Blender 5.2 EEVEE; tools/blender-scripts/render_female_hair_thumbnail.py; 512px render downsampled to 128px', files: thumbnails },
  animations: 'Reuse canonical female base clips, boots companions and animation packs',
  appearance: 'Accepted fitted body, head and eyelash geometry and compressed PBR textures preserved',
}
await writeFile('doc/assets/modular-female-custom-face-runtime.json', JSON.stringify(report, null, 2) + '\n')
console.log(JSON.stringify(report))

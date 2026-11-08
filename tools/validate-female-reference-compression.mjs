import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile, writeFile } from 'node:fs/promises'
import { NodeIO } from '@gltf-transform/core'
import { ALL_EXTENSIONS } from '@gltf-transform/extensions'
import { MeshoptDecoder } from 'meshoptimizer'
import sharp from 'sharp'

const directory = new URL('../assets/modular_human_female_01/faces/tripo_custom_v1/', import.meta.url)
const fitting = JSON.parse(await readFile(new URL('fitting.json', directory), 'utf8'))
const textureLimit = fitting.preview_texture_limit ?? 2576
await MeshoptDecoder.ready
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({ 'meshopt.decoder': MeshoptDecoder })
const inputs = await Promise.all(['base_custom.glb', 'base_custom_preview.glb'].map(name => readFile(new URL(name, directory))))
const documents = await Promise.all(inputs.map(input => io.readBinary(input)))
const [source, compressed] = documents.map(document => document.getRoot())
const equalArrays = (a, b, label) => {
  assert.equal(a.length, b.length, label)
  assert.ok(a.every((value, i) => value === b[i]), label)
}
let triangles = 0
assert.equal(compressed.listMeshes().length, source.listMeshes().length)
assert.equal(compressed.listMaterials().length, source.listMaterials().length)
for (const [i, mesh] of source.listMeshes().entries()) {
  const a = mesh.listPrimitives()[0]
  const b = compressed.listMeshes()[i].listPrimitives()[0]
  assert.deepEqual(a.listSemantics(), b.listSemantics())
  for (const key of a.listSemantics()) equalArrays(a.getAttribute(key).getArray(), b.getAttribute(key).getArray(), `${mesh.getName()}: ${key}`)
  const left = a.getIndices().getArray(), right = b.getIndices().getArray()
  assert.equal(left.length, right.length)
  triangles += left.length / 3
  for (let j = 0; j < left.length; j += 3) {
    const matches = [0, 1, 2].some(shift => [0, 1, 2].every(corner => left[j + corner] === right[j + (corner + shift) % 3]))
    assert.ok(matches, `${mesh.getName()}: triangle ${j / 3} winding`)
  }
}
for (const [i, animation] of source.listAnimations().entries()) {
  const other = compressed.listAnimations()[i]
  assert.equal(animation.getName(), other.getName())
  for (const [j, sampler] of animation.listSamplers().entries()) {
    const target = other.listSamplers()[j]
    assert.equal(sampler.getInterpolation(), target.getInterpolation())
    for (const key of ['getInput', 'getOutput']) equalArrays(sampler[key]().getArray(), target[key]().getArray(), `animation ${i}: ${key}`)
  }
}
const textureSizes = []
for (const [i, material] of source.listMaterials().entries()) {
  const other = compressed.listMaterials()[i]
  assert.equal(material.getAlphaMode(), other.getAlphaMode())
  const a = material.getBaseColorTexture(), b = other.getBaseColorTexture()
  const originalSize = a.getSize()
  const scale = Math.min(1, textureLimit / Math.max(...originalSize))
  assert.deepEqual(b.getSize(), originalSize.map(value => Math.round(value * scale)))
  textureSizes.push({ material: material.getName(), source_size: originalSize, size: b.getSize(), mime: b.getMimeType(), alpha: other.getAlphaMode() })
  if (material.getAlphaMode() !== 'OPAQUE') {
    const [left, right] = await Promise.all([a, b].map(texture => sharp(texture.getImage()).ensureAlpha().raw().toBuffer()))
    assert.ok(left.equals(right), 'Lossless eyelash RGBA')
  }
}
const report = {
  date: fitting.iris_highlight_cleanup?.date ?? '2026-11-07', sources: inputs.map((input, i) => ({ path: new URL(i ? 'base_custom_preview.glb' : 'base_custom.glb', directory).pathname, sha256: createHash('sha256').update(input).digest('hex') })),
  source_bytes: inputs[0].length, output_bytes: inputs[1].length, triangles,
  all_vertex_attributes_exact: true, triangle_order_and_winding_preserved: true,
  animations_exact: source.listAnimations().length,
  compressed_eyelash_rgba_exact: source.listMaterials().some(material => material.getAlphaMode() !== 'OPAQUE') ? true : null,
  neck_fit_only: Boolean(fitting.neck_fit_only),
  meshes: compressed.listMeshes().length, materials: compressed.listMaterials().length, texture_sizes: textureSizes,
  compression: `Meshopt lossless attributes; WebP base color quality 94; lossless PNG/WebP for RGBA; JPEG normal/ORM; maximum dimension ${textureLimit}`,
}
await writeFile(new URL('../doc/assets/modular-female-custom-face-compressed-v1.json', import.meta.url), JSON.stringify(report, null, 2) + '\n')
console.log(JSON.stringify(report))

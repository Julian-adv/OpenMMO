import assert from 'node:assert/strict'
import { mkdir, readFile, writeFile, rename } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { gzipSync } from 'node:zlib'
import { NodeIO } from '@gltf-transform/core'
import { ALL_EXTENSIONS, EXTTextureWebP } from '@gltf-transform/extensions'
import sharp from 'sharp'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree } from './lib/headless-three.mjs'
import { root, directory, sources, dependencies, signature, output, bodySkin } from './lib/female-preview.mjs'

const names = process.argv.slice(2)
if (!names.length) names.push(...Object.keys(sources))
assert.ok(names.every(name => Object.hasOwn(sources, name)), 'Unknown preview asset')
await mkdir(directory, { recursive: true })
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
sharp.cache(false)
sharp.concurrency(2)

for (const name of names) {
  const inputSignature = await signature(name)
  const source = await readFile(new URL(sources[name], root))
  let bytes, details
  if (name === 'animations') {
    const { server, load } = await headlessThree()
    try {
      const { femaleTorchClips, levelGaze } = await server.ssrLoadModule('/src/previews/female-torch.ts')
      const { scene, animations } = await load(sources.base)
      const pack = await load('client/public/models/characters/modular_male/animations/offhand.glb')
      const find = name => {
        const clip = animations.find(clip => clip.name === name)
        assert.ok(clip, name)
        return clip
      }
      const walk = find('walk'), combat = find('combat_idle')
      levelGaze(scene, combat)
      levelGaze(scene, walk)
      const clips = [walk, combat, ...await femaleTorchClips(scene, pack, find('idle_armed'), walk, combat)]
      const serialized = clips.map(clip => THREE.AnimationClip.toJSON(clip))
      for (let i = 0; i < clips.length; i++) {
        const restored = THREE.AnimationClip.parse(serialized[i])
        assert.equal(restored.duration, clips[i].duration)
        for (let t = 0; t < clips[i].tracks.length; t++) {
          assert.deepEqual(restored.tracks[t].times, clips[i].tracks[t].times)
          assert.deepEqual(restored.tracks[t].values, clips[i].tracks[t].values)
        }
      }
      bytes = Buffer.from(JSON.stringify({ clips: serialized }) + '\n')
      details = { clips: clips.map(clip => clip.name), exact_track_roundtrip: true }
    } finally { await server.close() }
  } else {
    const io = new NodeIO().registerExtensions(ALL_EXTENSIONS)
    const doc = await io.readBinary(source)
    if (name === 'base') {
      const skin = doc.getRoot().listMeshes()[0].listPrimitives()[0].getMaterial()
      skin.getBaseColorTexture().setImage(await readFile(new URL(bodySkin.color, root))).setMimeType('image/png')
      skin.getMetallicRoughnessTexture().setImage(await readFile(new URL(bodySkin.orm, root))).setMimeType('image/png')
    }
    const colors = new Set(doc.getRoot().listMaterials().map(m => m.getBaseColorTexture()))
    const textures = []
    for (const texture of doc.getRoot().listTextures()) {
      const original = texture.getImage()
      const before = await sharp(original).metadata()
      if (/eyelash/i.test(texture.getName())) {
        textures.push({ name: texture.getName(), preserved: true, size: [before.width, before.height] })
        continue
      }
      const size = colors.has(texture) ? 2048 : 1024
      const encoded = await sharp(original).resize({ width: size, height: size, fit: 'inside', withoutEnlargement: true })
        .webp(colors.has(texture) ? { quality: 94, effort: 5 } : { lossless: true, effort: 5 }).toBuffer()
      texture.setImage(encoded).setMimeType('image/webp')
      const after = await sharp(encoded).metadata()
      textures.push({ name: texture.getName(), before: [before.width, before.height], after: [after.width, after.height], bytes: encoded.length })
    }
    doc.createExtension(EXTTextureWebP).setRequired(true)
    bytes = Buffer.from(await io.writeBinary(doc))
    const restored = await io.readBinary(bytes)
    const originalAccessors = doc.getRoot().listAccessors()
    const restoredAccessors = restored.getRoot().listAccessors()
    assert.equal(originalAccessors.length, restoredAccessors.length)
    const fingerprints = accessors => accessors.map(accessor => {
      const array = accessor.getArray()
      return `${accessor.getType()}:${accessor.getComponentType()}:${hash(Buffer.from(array.buffer, array.byteOffset, array.byteLength))}`
    }).sort()
    assert.deepEqual(fingerprints(restoredAccessors), fingerprints(originalAccessors))
    details = { textures, exact_geometry_skin_animation_accessors: true }
  }
  assert.equal(await signature(name), inputSignature, 'Source changed during preview preparation; retry')
  const target = output(name)
  const temporary = new URL(target.href + `.${process.pid}.tmp`)
  await writeFile(temporary, bytes)
  await rename(temporary, target)
  if (name === 'animations') await writeFile(new URL(target.href + '.gz'), gzipSync(bytes))
  const provenance = await Promise.all(dependencies(name).map(async path => ({ path, sha256: hash(await readFile(new URL(path, root))) })))
  const report = { signature: inputSignature, source_bytes: source.length, output_bytes: bytes.length, sha256: hash(bytes), sources: provenance, ...details }
  await writeFile(new URL(`${name}.manifest.json`, directory), JSON.stringify(report, null, 2) + '\n')
  console.log(`${name}: ${source.length} -> ${bytes.length} bytes`)
}

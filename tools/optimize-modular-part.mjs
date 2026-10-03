import { readFile, writeFile } from 'node:fs/promises'
import { gzipSync } from 'node:zlib'
import { NodeIO } from '@gltf-transform/core'
import { ALL_EXTENSIONS, EXTMeshoptCompression } from '@gltf-transform/extensions'
import { prune } from '@gltf-transform/functions'
import { MeshoptDecoder, MeshoptEncoder } from 'meshoptimizer'
import sharp from 'sharp'

const [source, output] = process.argv.slice(2)
if (!source || !output || process.argv.length !== 4) {
  console.error('Usage: node tools/optimize-modular-part.mjs SOURCE.glb OUTPUT.glb')
  process.exit(1)
}

await Promise.all([MeshoptEncoder.ready, MeshoptDecoder.ready])
const io = new NodeIO()
  .registerExtensions(ALL_EXTENSIONS)
  .registerDependencies({
    'meshopt.encoder': MeshoptEncoder,
    'meshopt.decoder': MeshoptDecoder,
  })
const original = await readFile(source)
const document = await io.readBinary(original)
if (document.hasExtension('EXT_meshopt_compression')) {
  throw new Error('Use the uncompressed source to avoid repeated texture compression.')
}
await document.transform(prune({ keepLeaves: true, keepAttributes: true, keepIndices: true }))
for (const texture of document.getRoot().listTextures()) {
  const image = texture.getImage()
  if (!image) throw new Error('An embedded part texture is required.')
  const input = sharp(image)
  if ((await input.metadata()).hasAlpha)
    throw new Error('Only opaque textures are supported.')
  const resized = await input
    .resize({ width: 512, height: 512, fit: 'inside', withoutEnlargement: true })
    .jpeg({ quality: 90, chromaSubsampling: '4:4:4' })
    .toBuffer()
  texture.setImage(resized).setMimeType('image/jpeg')
}
document.createExtension(EXTMeshoptCompression)
  .setRequired(true)
  .setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE })
const compressed = await io.writeBinary(document)
await writeFile(output, compressed)
console.log(`${original.length} → ${compressed.length} bytes`)
console.log(`gzip level 6: ${gzipSync(original, { level: 6 }).length} → ${gzipSync(compressed, { level: 6 }).length} bytes`)

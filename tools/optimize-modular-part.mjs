import { readFile, writeFile } from 'node:fs/promises'
import { gzipSync } from 'node:zlib'
import { NodeIO } from '@gltf-transform/core'
import { ALL_EXTENSIONS, EXTMeshoptCompression, EXTTextureWebP } from '@gltf-transform/extensions'
import { prune } from '@gltf-transform/functions'
import { MeshoptDecoder, MeshoptEncoder } from 'meshoptimizer'
import sharp from 'sharp'

const [source, output] = process.argv.slice(2)
const body = process.argv[4] === '--body'
if (!source || !output || process.argv.length !== (body ? 5 : 4)) {
  console.error('Usage: node tools/optimize-modular-part.mjs SOURCE.glb OUTPUT.glb [--body]')
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
const materials = document.getRoot().listMaterials()
const webp = new Set(body ? materials.map(material => material.getBaseColorTexture()) : [])
const normals = new Set(body ? materials.map(material => material.getNormalTexture()) : [])
const size = body ? 1024 : 512
for (const texture of document.getRoot().listTextures()) {
  const image = texture.getImage()
  if (!image) throw new Error('An embedded part texture is required.')
  const input = sharp(image)
  if ((await input.metadata()).hasAlpha)
    throw new Error('Only opaque textures are supported.')
  const [encoded, mimeType] = webp.has(texture)
    ? [input.webp({ quality: 95, effort: 6 }), 'image/webp']
    : [input
        .resize({ width: size, height: size, fit: 'inside', withoutEnlargement: true })
        .jpeg({ quality: normals.has(texture) ? 92 : 90, chromaSubsampling: '4:4:4' }), 'image/jpeg']
  texture.setImage(await encoded.toBuffer()).setMimeType(mimeType)
}
if (body) document.createExtension(EXTTextureWebP).setRequired(true)
document.createExtension(EXTMeshoptCompression)
  .setRequired(true)
  .setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE })
const compressed = await io.writeBinary(document)
await writeFile(output, compressed)
console.log(`${original.length} → ${compressed.length} bytes`)
console.log(`gzip level 6: ${gzipSync(original, { level: 6 }).length} → ${gzipSync(compressed, { level: 6 }).length} bytes`)

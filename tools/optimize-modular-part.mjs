import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { execFileSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { gzipSync } from 'node:zlib'
import { NodeIO, PropertyType } from '@gltf-transform/core'
import { ALL_EXTENSIONS, EXTMeshoptCompression, EXTTextureWebP, KHRTextureBasisu } from '@gltf-transform/extensions'
import { dedup, prune } from '@gltf-transform/functions'
import { MeshoptDecoder, MeshoptEncoder } from 'meshoptimizer'
import sharp from 'sharp'

const [source, output] = process.argv.slice(2)
const flags = process.argv.slice(4)
const body = flags.includes('--body')
const ktx2 = flags.includes('--ktx2')
const sizeFlag = flags.find(flag => flag.startsWith('--size='))
const size = sizeFlag ? Number(sizeFlag.slice(7)) : body ? 1024 : 512
const qualityFlag = flags.find(flag => flag.startsWith('--quality='))
const quality = qualityFlag ? Number(qualityFlag.slice(10)) : 90
if (
  !source || !output ||
  !Number.isSafeInteger(size) || size <= 0 ||
  !Number.isSafeInteger(quality) || quality < 1 || quality > 100 ||
  flags.some(flag => !['--body', '--ktx2', sizeFlag, qualityFlag].includes(flag))
) {
  console.error('Usage: node tools/optimize-modular-part.mjs SOURCE.glb OUTPUT.glb [--body] [--ktx2] [--size=PIXELS] [--quality=1..100]')
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
if (document.getRoot().listExtensionsUsed().some(extension => extension.extensionName === 'EXT_meshopt_compression')) {
  throw new Error('Use the uncompressed source to avoid repeated texture compression.')
}
await document.transform(
  prune({ keepLeaves: true, keepAttributes: true, keepIndices: true }),
  dedup({ propertyTypes: [PropertyType.TEXTURE] })
)
const materials = document.getRoot().listMaterials()
const colors = new Set(materials.flatMap(material => [material.getBaseColorTexture(), material.getEmissiveTexture()]))
const webp = new Set(body ? materials.map(material => material.getBaseColorTexture()) : [])
const normals = new Set(body ? materials.map(material => material.getNormalTexture()) : [])
const temporary = ktx2 ? await mkdtemp(join(tmpdir(), 'modular-ktx2-')) : undefined
async function toKtx2(input, srgb) {
  const png = join(temporary, 'texture.png')
  const encoded = join(temporary, 'texture.ktx2')
  await input.png().toFile(png)
  execFileSync(process.env.TOKTX ?? 'toktx', [
    '--t2', '--encode', 'uastc', '--uastc_quality', '3',
    '--uastc_rdo_l', '0.5', '--uastc_rdo_m', '--zcmp', '18',
    '--genmipmap', '--assign_oetf', srgb ? 'srgb' : 'linear',
    encoded, png,
  ], { stdio: 'inherit' })
  return readFile(encoded)
}
try {
  for (const texture of document.getRoot().listTextures()) {
    const image = texture.getImage()
    if (!image) throw new Error('An embedded part texture is required.')
    const input = sharp(image)
    const hasAlpha = (await input.metadata()).hasAlpha
    if (body || !hasAlpha)
      input.resize({ width: size, height: size, fit: 'inside', withoutEnlargement: true })
    if (ktx2) {
      texture.setImage(await toKtx2(input, colors.has(texture))).setMimeType('image/ktx2')
      continue
    }
    const [encoded, mimeType] = hasAlpha
      ? [input.webp({ lossless: true, effort: 6 }), 'image/webp']
      : webp.has(texture)
      ? [input.webp({ quality, effort: 6 }), 'image/webp']
      : [input.jpeg({ quality: normals.has(texture) ? Math.max(92, quality) : quality, chromaSubsampling: '4:4:4' }), 'image/jpeg']
    texture.setImage(await encoded.toBuffer()).setMimeType(mimeType)
  }
} finally {
  if (temporary) await rm(temporary, { recursive: true, force: true })
}
if (document.getRoot().listTextures().some(texture => texture.getMimeType() === 'image/webp'))
  document.createExtension(EXTTextureWebP).setRequired(true)
if (ktx2) document.createExtension(KHRTextureBasisu).setRequired(true)
document.createExtension(EXTMeshoptCompression)
  .setRequired(true)
  .setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE })
const compressed = await io.writeBinary(document)
await writeFile(output, compressed)
console.log(`${original.length} → ${compressed.length} bytes`)
console.log(`gzip level 6: ${gzipSync(original, { level: 6 }).length} → ${gzipSync(compressed, { level: 6 }).length} bytes`)

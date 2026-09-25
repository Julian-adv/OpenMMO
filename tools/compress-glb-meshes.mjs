import { readFile, writeFile } from 'node:fs/promises'
import { gzipSync } from 'node:zlib'
import { NodeIO, VertexLayout } from '@gltf-transform/core'
import { ALL_EXTENSIONS } from '@gltf-transform/extensions'
import { dedup, meshopt, prune, weld } from '@gltf-transform/functions'
import { MeshoptDecoder, MeshoptEncoder } from 'meshoptimizer'

const [source, output] = process.argv.slice(2)
if (!source || !output || process.argv.length !== 4) {
  console.error('Usage: node tools/compress-glb-meshes.mjs SOURCE.glb OUTPUT.glb')
  process.exit(1)
}

await Promise.all([MeshoptEncoder.ready, MeshoptDecoder.ready])
const io = new NodeIO()
  .registerExtensions(ALL_EXTENSIONS)
  .registerDependencies({
    'meshopt.encoder': MeshoptEncoder,
    'meshopt.decoder': MeshoptDecoder,
  })
  .setVertexLayout(VertexLayout.SEPARATE)
const original = await readFile(source)
const document = await io.readBinary(original)
if (document.getRoot().listExtensionsUsed().some(e => e.extensionName === 'EXT_meshopt_compression')) {
  throw new Error('Use the uncompressed source to avoid repeated quantization.')
}
await document.transform(
  dedup(),
  weld(),
  prune(),
  meshopt({ encoder: MeshoptEncoder, level: 'medium' })
)
const compressed = await io.writeBinary(document)
await writeFile(output, compressed)
console.log(`${original.length} → ${compressed.length} bytes`)
console.log(`gzip level 6: ${gzipSync(original, { level: 6 }).length} → ${gzipSync(compressed, { level: 6 }).length} bytes`)

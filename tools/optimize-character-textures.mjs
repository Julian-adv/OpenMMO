import { readFile, writeFile } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { NodeIO } from "@gltf-transform/core";
import {
  ALL_EXTENSIONS,
  EXTMeshoptCompression,
  EXTTextureWebP,
} from "@gltf-transform/extensions";
import { MeshoptDecoder, MeshoptEncoder } from "meshoptimizer";
import sharp from "sharp";

const [source, output, sizeArg] = process.argv.slice(2);
const size = Number(sizeArg);
if (
  !source ||
  !output ||
  process.argv.length !== 5 ||
  !Number.isSafeInteger(size) ||
  size <= 0
) {
  console.error(
    "Usage: node tools/optimize-character-textures.mjs SOURCE.glb OUTPUT.glb BASE_MAX_SIZE",
  );
  process.exit(1);
}

await Promise.all([MeshoptEncoder.ready, MeshoptDecoder.ready]);
const io = new NodeIO()
  .registerExtensions(ALL_EXTENSIONS)
  .registerDependencies({
    "meshopt.encoder": MeshoptEncoder,
    "meshopt.decoder": MeshoptDecoder,
  });
const original = await readFile(source);
const document = await io.readBinary(original);
if (document.hasExtension("EXT_meshopt_compression")) {
  throw new Error(
    "Use the uncompressed source to avoid repeated texture compression.",
  );
}
const textures = new Set(
  document
    .getRoot()
    .listMaterials()
    .map((material) => material.getBaseColorTexture())
    .filter(Boolean),
);
for (const texture of textures) {
  const image = texture.getImage();
  if (!image) throw new Error("An embedded base color texture is required.");
  const input = sharp(image);
  const { hasAlpha } = await input.metadata();
  input.resize({
    width: size,
    height: size,
    fit: "inside",
    withoutEnlargement: true,
  });
  texture
    .setImage(
      await input
        .webp(
          hasAlpha ? { lossless: true, effort: 6 } : { quality: 90, effort: 6 },
        )
        .toBuffer(),
    )
    .setMimeType("image/webp");
}
if (textures.size) document.createExtension(EXTTextureWebP).setRequired(true);
document
  .createExtension(EXTMeshoptCompression)
  .setRequired(true)
  .setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE });
const compressed = await io.writeBinary(document);
await writeFile(output, compressed);
console.log(`${original.length} → ${compressed.length} bytes`);
console.log(
  `gzip level 6: ${gzipSync(original, { level: 6 }).length} → ${gzipSync(compressed, { level: 6 }).length} bytes`,
);

import { readFile, writeFile } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { NodeIO, PropertyType } from "@gltf-transform/core";
import { dedup, prune, resample } from "@gltf-transform/functions";
import {
  ALL_EXTENSIONS,
  EXTMeshoptCompression,
  EXTTextureWebP,
} from "@gltf-transform/extensions";
import { MeshoptDecoder, MeshoptEncoder } from "meshoptimizer";
import sharp from "sharp";

const [source, output, sizeArg, ...flags] = process.argv.slice(2);
const size = Number(sizeArg);
const resizeAll = flags.includes("--all-textures");
const lossyMaps = flags.includes("--lossy-maps");
const animationFlag = flags.find((flag) =>
  flag.startsWith("--keep-animations="),
);
const animationNames = animationFlag
  ?.slice("--keep-animations=".length)
  .split(",");
if (
  !source ||
  !output ||
  flags.some(
    (flag) =>
      flag !== "--lossless-png" &&
      flag !== "--all-textures" &&
      flag !== "--lossy-maps" &&
      flag !== "--resample-animations" &&
      flag !== animationFlag,
  ) ||
  new Set(flags).size !== flags.length ||
  animationNames?.some((name) => !name) ||
  !Number.isSafeInteger(size) ||
  size <= 0
) {
  console.error(
    "Usage: node tools/optimize-character-textures.mjs SOURCE.glb OUTPUT.glb BASE_MAX_SIZE [--lossless-png] [--all-textures] [--lossy-maps] [--resample-animations] [--keep-animations=NAME,...]",
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
if (
  document
    .getRoot()
    .listExtensionsUsed()
    .some((extension) => extension.extensionName === "EXT_meshopt_compression")
) {
  throw new Error(
    "Use the uncompressed source to avoid repeated texture compression.",
  );
}
if (animationNames) {
  const animations = document.getRoot().listAnimations();
  for (const name of animationNames) {
    if (!animations.some((animation) => animation.getName() === name)) {
      throw new Error(`Missing requested animation: ${name}`);
    }
  }
  for (const animation of animations) {
    if (animationNames.includes(animation.getName())) continue;
    for (const channel of animation.listChannels()) channel.dispose();
    for (const sampler of animation.listSamplers()) sampler.dispose();
    animation.dispose();
  }
}
// Unused UVs and tangents each cost a vertex buffer; WebGPU allows 8.
await document.transform(
  prune({ propertyTypes: [PropertyType.ACCESSOR], keepAttributes: false }),
);
if (flags.includes("--resample-animations")) {
  await document.transform(resample({ tolerance: 1e-5 }), dedup({ propertyTypes: [PropertyType.ACCESSOR] }));
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
if (flags.includes("--lossless-png") || resizeAll) {
  for (const texture of document.getRoot().listTextures()) {
    if (
      textures.has(texture) ||
      (!resizeAll && texture.getMimeType() !== "image/png")
    )
      continue;
    const image = texture.getImage();
    if (!image) throw new Error("An embedded texture is required.");
    const input = sharp(image);
    if (resizeAll) {
      input.resize({
        width: size,
        height: size,
        fit: "inside",
        withoutEnlargement: true,
      });
    }
    const compressed = await input
      .webp(lossyMaps ? { quality: 90, effort: 6 } : { lossless: true, effort: 6 })
      .toBuffer();
    if (resizeAll || compressed.length < image.length) {
      texture.setImage(compressed).setMimeType("image/webp");
    }
  }
}
if (
  document
    .getRoot()
    .listTextures()
    .some((texture) => texture.getMimeType() === "image/webp")
) {
  document.createExtension(EXTTextureWebP).setRequired(true);
}
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

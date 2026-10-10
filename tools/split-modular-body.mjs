import { createHash } from "node:crypto";
import { NodeIO } from "@gltf-transform/core";
import { ALL_EXTENSIONS } from "@gltf-transform/extensions";
import { prune } from "@gltf-transform/functions";

const io = new NodeIO().registerExtensions(ALL_EXTENSIONS);
const faceRegions = new Set(["head", "neck", "face_neck_bridge"]);
const textureSlots = [
  ["BaseColor", ["map"]],
  ["Normal", ["normalMap"]],
  ["MetallicRoughness", ["metalnessMap", "roughnessMap"]],
];
const digest = (texture) =>
  texture && createHash("sha256").update(texture.getImage()).digest("hex");

export async function splitModularBody(source, commonSource, output, name) {
  const document = await io.read(source);
  const face = name !== "base";
  for (const node of document.getRoot().listNodes()) {
    if (!node.getMesh()) continue;
    if (faceRegions.has(node.getExtras().region) !== face) node.dispose();
  }
  if (face) {
    const common = await io.read(commonSource);
    const skin = common
      .getRoot()
      .listNodes()
      .find((node) => node.getExtras().region === "torso")
      .getMesh()
      .listPrimitives()[0]
      .getMaterial();
    for (const material of document.getRoot().listMaterials()) {
      const shared = [];
      for (const [slot, properties] of textureSlots) {
        const texture = material[`get${slot}Texture`]();
        if (!texture || digest(texture) !== digest(skin[`get${slot}Texture`]()))
          continue;
        material[`set${slot}Texture`](null);
        shared.push(...properties);
      }
      if (shared.length)
        material.setExtras({
          ...material.getExtras(),
          modular_body_textures: shared,
        });
    }
  }
  await document.transform(
    prune({ keepLeaves: true, keepAttributes: true, keepIndices: true }),
  );
  await io.write(output, document);
}

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import * as THREE from "../client/node_modules/three/build/three.module.js";
import { GLTFLoader } from "../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js";
import { GLTFExporter } from "../client/node_modules/three/examples/jsm/exporters/GLTFExporter.js";
import { createServer } from "../client/node_modules/vite/dist/node/index.js";

const root = fileURLToPath(new URL("../", import.meta.url));
const fitted = "assets/modular_human_male_01/parts/fitted";
const tuned = "assets/modular_human_male_01/rigged_hand_tuned";
const output = "client/public/models/characters/modular_male";
const parts = [
  "base",
  "hair_crop",
  "top_linen",
  "pants_cloth",
  "boots_leather",
  "top_plate",
  "pants_plate",
  "gloves_plate",
  "boots_plate",
  "helmet_plate",
  "top_barbarian",
  "pants_barbarian",
  "gloves_barbarian",
  "boots_barbarian",
  "helmet_barbarian",
];
const rogue = JSON.parse(
  readFileSync(
    resolve(root, "doc/assets/modular-rogue-source-selection.json"),
    "utf8",
  ),
).fitting_candidate;
const rogueParts = ["top_rogue", "pants_rogue", "gloves_rogue", "boots_rogue"];
parts.push(...rogueParts);
const meshoptParts = new Set(["gloves_rogue", "boots_barbarian"]);
const packs = [
  "locomotion",
  "combat_melee",
  "social",
  "fishing",
  "combat_ranged",
  "offhand",
  "enchant_armor",
  "great_sword",
  "riding",
];
const python = resolve(
  root,
  existsSync(resolve(root, ".venv/Scripts/python.exe"))
    ? ".venv/Scripts/python.exe"
    : ".venv/bin/python",
);
const hash = (path) =>
  createHash("sha256")
    .update(readFileSync(resolve(root, path)))
    .digest("hex");
const report = {
  source:
    "Modular male body, knight, barbarian and rogue equipment and user-provided Mixamo rig; source tiers and licenses in doc/assets/characters.md",
  inputs: {},
  outputs: {},
};
mkdirSync(resolve(root, output, "animations"), { recursive: true });
function option(flag, allowed) {
  const index = process.argv.indexOf(flag);
  if (index === -1) return undefined;
  const value = process.argv[index + 1];
  if (!allowed.includes(value))
    throw new Error(`${flag} requires one of: ${allowed.join(", ")}`);
  return value;
}
function writeManifest(merge) {
  const manifest = resolve(root, output, "manifest.json");
  if (merge && existsSync(manifest)) {
    const previous = JSON.parse(readFileSync(manifest, "utf8"));
    report.inputs = { ...previous.inputs, ...report.inputs };
    report.outputs = { ...previous.outputs, ...report.outputs };
  }
  writeFileSync(manifest, JSON.stringify(report, null, 2) + "\n");
}
const selectedPart = option("--part", parts);
const selectedPack = option("--pack", packs);
let selectedParts = process.argv.includes("--rogue-only") ? rogueParts : parts;
if (selectedPart) selectedParts = [selectedPart];
if (selectedPack) selectedParts = [];
for (const name of selectedParts) {
  const source = rogueParts.includes(name)
    ? (rogue.part_overrides[name] ?? `${rogue.directory}/${name}.glb`)
    : `${fitted}/${name}.glb`;
  report.inputs[source] = hash(source);
  const [command, args] = meshoptParts.has(name)
    ? [
        process.execPath,
        [
          resolve(root, "tools/optimize-modular-part.mjs"),
          resolve(root, source),
          resolve(root, output, `${name}.glb`),
        ],
      ]
    : [
        python,
        [
          resolve(root, "tools/repack-glb-textures.py"),
          resolve(root, source),
          "--out",
          resolve(root, output),
        ],
      ];
  execFileSync(command, args, { stdio: "inherit" });
  report.outputs[`${name}.glb`] = hash(`${output}/${name}.glb`);
}

if (
  selectedPart ||
  process.argv.includes("--parts-only") ||
  process.argv.includes("--rogue-only")
) {
  writeManifest(true);
  process.exit(0);
}

globalThis.self = globalThis;
globalThis.FileReader = class {
  readAsArrayBuffer(blob) {
    blob.arrayBuffer().then((result) => {
      this.result = result;
      this.onloadend?.();
    });
  }
};
const loader = new GLTFLoader();
loader.register(() => ({
  name: "HeadlessTextures",
  loadTexture: () => Promise.resolve(null),
}));
async function load(path) {
  const bytes = readFileSync(resolve(root, path));
  return loader.parseAsync(
    bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
    "",
  );
}
const server = await createServer({
  configFile: false,
  root: resolve(root, "client"),
  server: { middlewareMode: true, watch: null },
  appType: "custom",
  optimizeDeps: { noDiscovery: true, include: [] },
});
try {
  const runtime = await server.ssrLoadModule(
    "/src/lib/utils/characterAnimationUtils.ts",
  );
  const modular = await server.ssrLoadModule(
    "/src/lib/utils/modularCharacter.ts",
  );
  const base = await load(`${fitted}/base.glb`);
  const baked = await load(`${tuned}/animations.glb`);
  const corrected = new Map(
    modular
      .modularAnimationClips(base.scene, baked, "corrected")
      .map((clip) => [clip.name, clip]),
  );
  const profile = modular.parseModularHandProfile(
    JSON.parse(readFileSync(resolve(root, tuned, "hand-grips.json"), "utf8")),
  );
  const rigId = modular.modularRigId(base.scene);
  for (const name of ["idle2", "idle3", "idle4", "idle5"]) {
    const clip = corrected.get("idle1").clone();
    clip.name = name;
    corrected.set(name, clip);
    profile.iron_sword_by_clip[name] = profile.iron_sword_by_clip.idle1;
  }
  report.inputs[`${tuned}/animations.glb`] = hash(`${tuned}/animations.glb`);
  report.inputs[`${tuned}/hand-grips.json`] = hash(`${tuned}/hand-grips.json`);
  for (const name of selectedPack ? [selectedPack] : packs) {
    const path = `client/public/models/animations/${name}.glb`;
    report.inputs[path] = hash(path);
    const pack = await load(path);
    const missing = pack.animations.filter((clip) => !corrected.has(clip.name));
    let clips = await runtime.retargetAnimationsForCharacterModel(
      base.scene,
      pack.scene,
      missing,
    );
    clips = clips.map((clip) =>
      modular.applyModularFingerPose(clip, profile, rigId),
    );
    if (name !== "riding")
      clips = await runtime.groundRetargetedClips(
        base.scene,
        clips,
        name === "offhand"
          ? {
              plantedClips: ["torch_idle1", "torch_idle2", "torch_walk"],
              baselineClips: ["torch_run"],
              soleClearance: 0.002,
            }
          : {},
      );
    const byName = new Map(clips.map((clip) => [clip.name, clip]));
    const animations = pack.animations.map(
      (clip) => corrected.get(clip.name) ?? byName.get(clip.name),
    );
    const scene = new THREE.Scene();
    scene.userData = {
      rig_id: rigId,
      animation_stage: "modular-baked-v1",
      variant: "corrected",
      hand_profile: profile,
    };
    const rig = base.scene.clone(true);
    const meshes = [];
    rig.traverse((node) => {
      if (node.isMesh) meshes.push(node);
    });
    for (const mesh of meshes) mesh.removeFromParent();
    scene.add(rig);
    const bytes = await new GLTFExporter().parseAsync(scene, {
      binary: true,
      animations,
    });
    const roundtrip = await loader.parseAsync(bytes, "");
    modular.modularAnimationClips(base.scene, roundtrip, "corrected");
    writeFileSync(
      resolve(root, output, "animations", `${name}.glb`),
      Buffer.from(bytes),
    );
    report.outputs[`animations/${name}.glb`] = {
      sha256: hash(`${output}/animations/${name}.glb`),
      clips: animations.map((clip) => clip.name),
    };
    console.log(
      `${name}: ${animations.length} clips, ${bytes.byteLength} bytes`,
    );
  }
  writeManifest(Boolean(selectedPack));
} finally {
  await server.close();
}

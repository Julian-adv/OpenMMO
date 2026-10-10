import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import * as THREE from "../client/node_modules/three/build/three.module.js";
import { GLTFLoader } from "../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js";
import { GLTFExporter } from "../client/node_modules/three/examples/jsm/exporters/GLTFExporter.js";
import { createServer } from "../client/node_modules/vite/dist/node/index.js";
import { splitModularBody } from "./split-modular-body.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const fitted = "assets/modular_human_male_01/fitted";
const tuned = "assets/modular_human_male_01/animations";
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
const cavemanSources = {
  top_caveman:
    "assets/modular_human_male_01/caveman/tripo_top_v1/top_caveman.glb",
  pants_caveman:
    "assets/modular_human_male_01/caveman/tripo_pants_v1/pants_caveman.glb",
  boots_caveman:
    "assets/modular_human_male_01/caveman/tripo_boots_v1/boots_caveman.glb",
  gloves_caveman:
    "assets/modular_human_male_01/caveman/tripo_bracer_v1/gloves_caveman.glb",
};
const cavemanParts = Object.keys(cavemanSources);
parts.push(...cavemanParts);
const rangerSources = {
  top_ranger: "assets/modular_human_male_01/ranger/tripo_top_v4/top_ranger.glb",
  pants_ranger:
    "assets/modular_human_male_01/ranger/tripo_pants_v1/pants_ranger.glb",
  gloves_ranger:
    "assets/modular_human_male_01/ranger/tripo_gloves_v1/gloves_ranger.glb",
  boots_ranger:
    "assets/modular_human_male_01/ranger/tripo_boots_v1/boots_ranger.glb",
};
const rangerParts = Object.keys(rangerSources);
parts.push(...rangerParts);
const priestSources = {
  helmet_priest:
    "assets/modular_human_male_01/priest/tripo_helmet_v1/helmet_priest.glb",
  top_priest: "assets/modular_human_male_01/priest/tripo_top_v1/top_priest.glb",
  pants_priest:
    "assets/modular_human_male_01/priest/tripo_pants_v1/pants_priest.glb",
  boots_priest:
    "assets/modular_human_male_01/priest/tripo_boots_v1/boots_priest.glb",
};
const priestParts = Object.keys(priestSources);
parts.push(...priestParts);
const appearanceSources = {
  face_default: `${fitted}/base.glb`,
  face_ranger:
    "assets/modular_human_male_01/faces/tripo_ranger_v1/base_ranger.glb",
  face_rugged:
    "assets/modular_human_male_01/faces/tripo_rugged_v1/base_rugged.glb",
  hair_wavy_bone:
    "assets/modular_human_male_01/hair/tripo_wavy_v1/hair_wavy_bone.glb",
  hair_ranger:
    "assets/modular_human_male_01/hair/tripo_ranger_v1/hair_ranger.glb",
};
const appearanceParts = Object.keys(appearanceSources);
parts.push(...appearanceParts);
const bodyParts = ["base", "face_default", "face_rugged", "face_ranger"];
const outfitParts = parts.filter(
  (name) => !bodyParts.includes(name) && !name.startsWith("hair_"),
);
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
const hash = (path) =>
  createHash("sha256")
    .update(readFileSync(resolve(root, path)))
    .digest("hex");
const report = {
  source:
    "Modular male body, knight, barbarian, rogue, caveman, ranger and priest equipment and user-provided Mixamo rig; source tiers and licenses in doc/assets/characters.md",
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
  delete report.outputs["base_rugged.glb"];
  delete report.outputs["base_ranger.glb"];
  writeFileSync(manifest, JSON.stringify(report, null, 2) + "\n");
  for (const name of ["base_rugged", "base_ranger"])
    rmSync(resolve(root, output, `${name}.glb`), { force: true });
}
const selectedPart = option("--part", parts);
const selectedPack = option("--pack", packs);
const partSets = {
  "--sole-offsets": [],
  "--body-only": bodyParts,
  "--appearance-only": appearanceParts,
  "--priest-only": priestParts,
  "--ranger-only": rangerParts,
  "--caveman-only": cavemanParts,
  "--outfits-only": outfitParts,
  "--rogue-only": rogueParts,
};
const partSetFlag = Object.keys(partSets).find((flag) =>
  process.argv.includes(flag),
);
let selectedParts = partSetFlag ? partSets[partSetFlag] : parts;
if (selectedPart) selectedParts = [selectedPart];
if (selectedPack) selectedParts = [];
for (const name of selectedParts) {
  const source =
    appearanceSources[name] ??
    cavemanSources[name] ??
    rangerSources[name] ??
    priestSources[name] ??
    (rogueParts.includes(name)
      ? (rogue.part_overrides[name] ?? `${rogue.directory}/${name}.glb`)
      : `${fitted}/${name}.glb`);
  report.inputs[source] = hash(source);
  const temporary = bodyParts.includes(name)
    ? mkdtempSync(resolve(tmpdir(), "modular-body-"))
    : undefined;
  try {
    const input = temporary
      ? resolve(temporary, `${name}.glb`)
      : resolve(root, source);
    if (temporary) {
      report.inputs[`${fitted}/base.glb`] = hash(`${fitted}/base.glb`);
      await splitModularBody(
        resolve(root, source),
        resolve(root, fitted, "base.glb"),
        input,
        name,
      );
    }
    execFileSync(
      process.execPath,
      [
        resolve(root, "tools/optimize-modular-part.mjs"),
        input,
        resolve(root, output, `${name}.glb`),
        ...(outfitParts.includes(name) ? ["--ktx2"] : []),
        ...(bodyParts.includes(name) ? ["--body"] : []),
        ...(name === "face_ranger" ? ["--size=2048", "--quality=95"] : []),
      ],
      { stdio: "inherit" },
    );
  } finally {
    if (temporary) rmSync(temporary, { recursive: true, force: true });
  }
  report.outputs[`${name}.glb`] = hash(`${output}/${name}.glb`);
}

const partsOnly =
  partSetFlag !== undefined || process.argv.includes("--parts-only");

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
  if (!selectedPack)
    await writeSoleOffsets(
      runtime,
      modular,
      await server.ssrLoadModule("/src/lib/utils/headless-glb.fixture.ts"),
    );
  if (selectedPart || partsOnly) {
    if (selectedParts.length) writeManifest(true);
  } else await preparePacks(runtime, modular);
} finally {
  await server.close();
}

// Measured once per boot style here so clients don't skin every boots GLB at load.
async function writeSoleOffsets(runtime, modular, headless) {
  const loadOutput = async (name) =>
    (
      await headless.loadHeadlessGlb(
        `${output.replace("client/public/", "")}/${name}.glb`,
      )
    ).scene;
  const ids = new Set(
    modular.modularOutfitParts(modular.DEFAULT_MODULAR_OUTFIT),
  );
  for (const style of modular.MODULAR_BOOTS)
    if (style !== "none") ids.add(`boots_${style}`);
  const sources = new Map(
    await Promise.all([...ids].map(async (id) => [id, await loadOutput(id)])),
  );
  const faces = ["default", "rugged", "ranger"];
  const scenes = await Promise.all(
    faces.map(async (face) => {
      const scene = await loadOutput("base");
      modular.bindModularFace(scene, await loadOutput(`face_${face}`));
      return scene;
    }),
  );
  let offsets;
  for (const [i, scene] of scenes.entries()) {
    const face = faces[i];
    const body = modular.skinnedParts(scene);
    const parts = new Map(
      [...sources].map(([id, source]) => [
        id,
        modular.bindModularPart(scene, source),
      ]),
    );
    const measured = {};
    for (const style of modular.MODULAR_BOOTS) {
      modular.showModularOutfit(body, parts, {
        ...modular.DEFAULT_MODULAR_OUTFIT,
        boots: style,
      });
      measured[style] = runtime.computeSoleGroundOffset(scene);
    }
    if (offsets) {
      for (const style of modular.MODULAR_BOOTS)
        if (Math.abs(offsets[style] - measured[style]) > 1e-6)
          throw new Error(`${face} sole offset differs for ${style} boots`);
    } else offsets = measured;
  }
  writeFileSync(
    resolve(root, "client/src/lib/utils/modularSoleOffsets.json"),
    JSON.stringify(offsets, null, 2) + "\n",
  );
  console.log("sole offsets:", offsets);
}

async function preparePacks(runtime, modular) {
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
    if (animations.some((clip) => /^idle[1-5]$/.test(clip.name))) {
      scene.userData.idle_shoulder_retraction_degrees =
        baked.scene.userData.idle_shoulder_retraction_degrees ?? 0;
      scene.userData.idle_upper_body_symmetry_revision =
        baked.scene.userData.idle_upper_body_symmetry_revision;
    }
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
}

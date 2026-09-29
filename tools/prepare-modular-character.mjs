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
];
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
    "Modular male body, knight plate parts and user-provided Mixamo rig; source tiers and licenses in doc/assets/characters.md",
  inputs: {},
  outputs: {},
};
mkdirSync(resolve(root, output, "animations"), { recursive: true });
for (const name of parts) {
  const source = `${fitted}/${name}.glb`;
  report.inputs[source] = hash(source);
  execFileSync(
    python,
    [
      resolve(root, "tools/repack-glb-textures.py"),
      resolve(root, source),
      "--out",
      resolve(root, output),
    ],
    { stdio: "inherit" },
  );
  report.outputs[`${name}.glb`] = hash(`${output}/${name}.glb`);
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
  for (const name of packs) {
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
      clips = await runtime.groundRetargetedClips(base.scene, clips);
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
  writeFileSync(
    resolve(root, output, "manifest.json"),
    JSON.stringify(report, null, 2) + "\n",
  );
} finally {
  await server.close();
}

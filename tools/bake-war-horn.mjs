// Bakes the war-horn blow (client/src/lib/utils/warHornPose.ts) into the shared
// animation pack. After the per-rig packs are built from it, `--rigs` re-solves
// each rig's arms onto the horn (retargeting leaves the hands ~9 cm off) and
// records where each rig holds the horn.
// Run: node tools/bake-war-horn.mjs
//      node tools/prepare-modular-character.mjs --pack war_horn
//      node tools/prepare-female-knight.mjs --pack war_horn
//      node tools/bake-war-horn.mjs --rigs
import assert from "node:assert/strict";
import { readFileSync, renameSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { NodeIO, VertexLayout } from "@gltf-transform/core";
import {
  ALL_EXTENSIONS,
  EXTMeshoptCompression,
} from "@gltf-transform/extensions";
import { dedup } from "@gltf-transform/functions";
import { MeshoptDecoder, MeshoptEncoder } from "meshoptimizer";
import * as THREE from "../client/node_modules/three/build/three.module.js";
import { GLTFExporter } from "../client/node_modules/three/examples/jsm/exporters/GLTFExporter.js";
import { headlessThree, root } from "./lib/headless-three.mjs";

const SOURCE = "client/public/models/animations/locomotion.glb";
const OUTPUT = "client/public/models/animations/war_horn.glb";
const HORN = "client/public/models/objects/war_horn.glb";
const FITS = "client/src/lib/data/handFits.json";
const FPS = 30;
const ARM_BONES = ["Left", "Right"].flatMap((side) =>
  ["Arm", "ForeArm", "Hand"].map((bone) => side + bone),
);
const RIGS = [
  [
    "modular_male",
    "client/public/models/characters/modular_male",
    "manifest.json",
  ],
  [
    "modular_female",
    "client/public/models/characters/modular_female",
    "../../../../../doc/assets/female-knight-runtime-sources.json",
  ],
];

globalThis.FileReader = class {
  readAsArrayBuffer(blob) {
    blob.arrayBuffer().then((result) => {
      this.result = result;
      this.onloadend?.();
    });
  }
};
const { server, load } = await headlessThree();
try {
  const horn = await server.ssrLoadModule("/src/lib/utils/warHornPose.ts");
  const hornScene = (await load(HORN)).scene;
  if (process.argv.includes("--rigs")) await rigs(horn, hornScene);
  else await bake(horn, hornScene);
} finally {
  await server.close();
}

async function bake(
  { WarHornPose, WAR_HORN_CLIP, WAR_HORN_SECONDS },
  hornScene,
) {
  const pack = await load(SOURCE);
  const scene = pack.scene;
  const idle = pack.animations.find((clip) => clip.name === "idle1");
  assert.ok(idle);
  const pose = WarHornPose.create(scene, hornScene, false);
  assert.ok(pose);
  const mixer = new THREE.AnimationMixer(scene);
  mixer.clipAction(idle).play();
  const bones = [];
  scene.traverse((node) => node.isBone && bones.push(node));
  const frames = Math.round(WAR_HORN_SECONDS * FPS) + 1;
  const times = [];
  const rotations = bones.map(() => []);
  const hips = [];
  for (let frame = 0; frame < frames; frame++) {
    const time = Math.min(frame / FPS, WAR_HORN_SECONDS);
    pose.restore();
    mixer.setTime(time);
    scene.updateMatrixWorld(true);
    pose.update(Math.min(time, WAR_HORN_SECONDS - 1e-4));
    scene.updateMatrixWorld(true);
    times.push(time);
    bones.forEach((bone, i) => rotations[i].push(...bone.quaternion.toArray()));
    hips.push(...bones.find((bone) => bone.name === "Hips").position.toArray());
  }
  pose.dispose();
  mixer.stopAllAction();
  const tracks = bones.map(
    (bone, i) =>
      new THREE.QuaternionKeyframeTrack(
        `${bone.name}.quaternion`,
        times,
        rotations[i],
      ),
  );
  tracks.push(new THREE.VectorKeyframeTrack("Hips.position", times, hips));
  const clip = new THREE.AnimationClip(WAR_HORN_CLIP, WAR_HORN_SECONDS, tracks);
  const meshes = [];
  scene.traverse((node) => node.isMesh && meshes.push(node));
  for (const mesh of meshes) mesh.removeFromParent();
  const bytes = await new GLTFExporter().parseAsync(scene, {
    binary: true,
    animations: [clip],
  });
  writeFileSync(new URL(OUTPUT, root), Buffer.from(bytes));
  console.log(`${OUTPUT}: ${frames} frames, ${tracks.length} tracks`);
}

// Each rig solves its own arms onto the horn, then holds it in the right hand
// where the horn sits at the lips mid-blow.
async function rigs(horn, hornScene) {
  const {
    WarHornPose,
    WAR_HORN_CLIP,
    WAR_HORN_HOLD_SECONDS,
    warHornHoldMatrix,
  } = horn;
  const { createCharacterModelRoot } = await server.ssrLoadModule(
    "/src/lib/utils/characterAnimationUtils.ts",
  );
  await Promise.all([MeshoptEncoder.ready, MeshoptDecoder.ready]);
  const io = new NodeIO()
    .registerExtensions(ALL_EXTENSIONS)
    .registerDependencies({
      "meshopt.encoder": MeshoptEncoder,
      "meshopt.decoder": MeshoptDecoder,
    });
  const fits = JSON.parse(readFileSync(new URL(FITS, root)));
  for (const [rig, directory, report] of RIGS) {
    const female = rig === "modular_female";
    const packPath = `${directory}/animations/war_horn.glb`;
    const { modelRoot } = createCharacterModelRoot(
      (await load(`${directory}/base.glb`)).scene,
    );
    const clip = (await load(packPath)).animations.find(
      (c) => c.name === WAR_HORN_CLIP,
    );
    assert.ok(clip, rig);
    const times = Array.from(clip.tracks[0].times);
    const pose = WarHornPose.create(modelRoot, hornScene, female);
    const mixer = new THREE.AnimationMixer(modelRoot);
    mixer.clipAction(clip).play();
    const bones = ARM_BONES.map((name) => modelRoot.getObjectByName(name));
    const solved = bones.map(() => []);
    for (const time of times) {
      pose.restore();
      mixer.setTime(time);
      modelRoot.updateMatrixWorld(true);
      pose.update(Math.min(time, clip.duration - 1e-4));
      modelRoot.updateMatrixWorld(true);
      bones.forEach((bone, i) => solved[i].push(...bone.quaternion.toArray()));
    }
    pose.dispose();

    const doc = await io.read(new URL(packPath, root).pathname);
    const animation = doc
      .getRoot()
      .listAnimations()
      .find((a) => a.getName() === WAR_HORN_CLIP);
    for (const [i, name] of ARM_BONES.entries()) {
      const channel = animation
        .listChannels()
        .find(
          (c) =>
            c.getTargetNode().getName() === name &&
            c.getTargetPath() === "rotation",
        );
      assert.ok(channel, name);
      const sampler = channel.getSampler();
      assert.equal(sampler.getInput().getCount(), times.length, name);
      const output = doc
        .createAccessor()
        .setType("VEC4")
        .setArray(new Float32Array(solved[i]))
        .setBuffer(doc.getRoot().listBuffers()[0]);
      sampler.setOutput(output);
    }
    await doc.transform(dedup());
    doc
      .createExtension(EXTMeshoptCompression)
      .setRequired(true)
      .setEncoderOptions({
        method: EXTMeshoptCompression.EncoderMethod.QUANTIZE,
      });
    io.setVertexLayout(VertexLayout.SEPARATE);
    const path = new URL(packPath, root).pathname;
    writeFileSync(`${path}.tmp`, await io.writeBinary(doc));
    renameSync(`${path}.tmp`, path);
    recordOutput(directory, report, packPath);

    const { modelRoot: held } = createCharacterModelRoot(
      (await load(`${directory}/base.glb`)).scene,
    );
    const baked = (await load(packPath)).animations.find(
      (c) => c.name === WAR_HORN_CLIP,
    );
    const replay = new THREE.AnimationMixer(held);
    replay.clipAction(baked).play();
    replay.setTime(WAR_HORN_HOLD_SECONDS);
    held.updateMatrixWorld(true);
    const holdAt = warHornHoldMatrix(held, female);
    const inHand = new THREE.Matrix4()
      .copy(held.getObjectByName("RightHand").matrixWorld)
      .invert()
      .multiply(holdAt);
    const position = new THREE.Vector3();
    const quaternion = new THREE.Quaternion();
    inHand.decompose(position, quaternion, new THREE.Vector3());
    const rotation = new THREE.Euler().setFromQuaternion(quaternion);
    const round = (values) => values.map((v) => Math.round(v * 1000) / 1000);
    fits[rig]["model:objects/war_horn.glb"] = {
      position: round(position.toArray()),
      rotation: round([rotation.x, rotation.y, rotation.z]),
    };
    console.log(rig, fits[rig]["model:objects/war_horn.glb"]);
  }
  writeFileSync(new URL(FITS, root), JSON.stringify(fits, null, 2) + "\n");
}

// Keeps the build report's hash of the rewritten pack current.
function recordOutput(directory, report, packPath) {
  const path = new URL(`${directory}/${report}`, root);
  const json = JSON.parse(readFileSync(path));
  const sha256 = createHash("sha256")
    .update(readFileSync(new URL(packPath, root)))
    .digest("hex");
  const key = report === "manifest.json" ? "animations/war_horn.glb" : packPath;
  if (typeof json.outputs[key] === "object") json.outputs[key].sha256 = sha256;
  else json.outputs[key] = sha256;
  writeFileSync(path, JSON.stringify(json, null, 2) + "\n");
}

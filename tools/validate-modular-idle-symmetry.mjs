import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import * as THREE from "../client/node_modules/three/build/three.module.js";
import { headlessThree, clipSampler } from "./lib/headless-three.mjs";

const { server, load, sources } = await headlessThree();
const root = new URL("../", import.meta.url);
try {
  const mod = await server.ssrLoadModule("/src/lib/utils/modularCharacter.ts");
  const body = (
    await load("assets/modular_human_male_01/fitted/base.glb")
  ).scene;
  const bones = mod.skinnedParts(body)[0].skeleton.bones;
  const sampler = clipSampler(body, { samples: 501, restore: bones });
  const packs = [
    "assets/modular_human_male_01/animations/animations.glb",
    "client/public/models/characters/modular_male/animations/locomotion.glb",
  ];
  const beforeIndex = process.argv.indexOf("--before-dir");
  const beforeDir = beforeIndex < 0 ? null : process.argv[beforeIndex + 1];
  const modified = new Set([
    "Spine2.quaternion",
    ...["Shoulder", "Arm", "ForeArm", "Hand"].flatMap((name) =>
      ["Left", "Right"].map((side) => `${side}${name}.quaternion`),
    ),
  ]);
  const checks = [];
  const beforeSources = [];
  for (const path of packs) {
    const pack = await load(path);
    const clips = mod.modularAnimationClips(body, pack, "corrected");
    assert.equal(
      pack.scene.userData.idle_upper_body_symmetry_revision,
      "2026-10-09-v1",
    );
    let unchangedTracks = 0;
    if (beforeDir) {
      const name = path.endsWith("locomotion.glb")
        ? "locomotion.glb"
        : "animations.glb";
      const before = await load(`${beforeDir}/${name}`);
      const lock = readFileSync(new URL("assets.lock", root), "utf8").split(
        "\n",
      );
      const source = sources.at(-1);
      assert.ok(
        lock.includes(`file ${source.sha256} ${path}`),
        "Baseline must match the pinned asset",
      );
      beforeSources.push({
        path,
        sha256: source.sha256,
        repository: lock[0].split(" ")[1],
        revision: lock[1].split(" ")[1],
      });
      assert.deepEqual(
        clips.map((c) => c.name),
        before.animations.map((c) => c.name),
      );
      for (const clip of clips) {
        const old = before.animations.find((c) => c.name === clip.name);
        assert.equal(clip.duration, old.duration);
        assert.equal(clip.tracks.length, old.tracks.length);
        for (const track of clip.tracks) {
          const oldTrack = old.tracks.find((t) => t.name === track.name);
          assert.ok(oldTrack);
          assert.deepEqual(track.times, oldTrack.times);
          if (/^idle[1-5]$/.test(clip.name) && modified.has(track.name))
            continue;
          assert.deepEqual(
            track.values,
            oldTrack.values,
            `${clip.name}: ${track.name}`,
          );
          unchangedTracks++;
        }
      }
    }
    for (const clip of clips.filter((c) => /^idle[1-5]$/.test(c.name))) {
      const stats = {
        clip: clip.name,
        samples: 0,
        maximumShoulderHeightDifferenceMm: 0,
        maximumMirroredArmJointErrorMm: 0,
        maximumTorsoYawRollDegrees: 0,
        armAbductionDegrees: [Infinity, -Infinity],
      };
      for (const _ of sampler(clip)) {
        stats.samples++;
        const spine = body.getObjectByName("Spine2");
        const origin = spine.getWorldPosition(new THREE.Vector3());
        const orientation = spine.getWorldQuaternion(new THREE.Quaternion());
        const euler = new THREE.Euler().setFromQuaternion(orientation, "YXZ");
        stats.maximumTorsoYawRollDegrees = Math.max(
          stats.maximumTorsoYawRollDegrees,
          (Math.abs(euler.y) * 180) / Math.PI,
          (Math.abs(euler.z) * 180) / Math.PI,
        );
        const point = (name) =>
          body
            .getObjectByName(name)
            .getWorldPosition(new THREE.Vector3())
            .sub(origin);
        for (const suffix of ["Shoulder", "Arm", "ForeArm", "Hand"]) {
          const left = point("Left" + suffix),
            right = point("Right" + suffix);
          if (suffix === "Arm")
            stats.maximumShoulderHeightDifferenceMm = Math.max(
              stats.maximumShoulderHeightDifferenceMm,
              Math.abs(left.y - right.y) * 1000,
            );
          left.x *= -1;
          stats.maximumMirroredArmJointErrorMm = Math.max(
            stats.maximumMirroredArmJointErrorMm,
            left.distanceTo(right) * 1000,
          );
        }
        const inverse = spine.matrixWorld.clone().invert();
        for (const side of ["Left", "Right"]) {
          const arm = body
            .getObjectByName(side + "Arm")
            .getWorldPosition(new THREE.Vector3())
            .applyMatrix4(inverse);
          const elbow = body
            .getObjectByName(side + "ForeArm")
            .getWorldPosition(new THREE.Vector3())
            .applyMatrix4(inverse);
          const vector = elbow.sub(arm);
          const angle =
            (Math.atan2(Math.hypot(vector.x, vector.z), -vector.y) * 180) /
            Math.PI;
          stats.armAbductionDegrees[0] = Math.min(
            stats.armAbductionDegrees[0],
            angle,
          );
          stats.armAbductionDegrees[1] = Math.max(
            stats.armAbductionDegrees[1],
            angle,
          );
        }
      }
      assert.ok(
        stats.maximumShoulderHeightDifferenceMm < 0.1,
        JSON.stringify(stats),
      );
      assert.ok(
        stats.maximumMirroredArmJointErrorMm < 0.8,
        JSON.stringify(stats),
      );
      assert.ok(stats.maximumTorsoYawRollDegrees < 0.01, JSON.stringify(stats));
      assert.ok(
        stats.armAbductionDegrees[0] > 10 && stats.armAbductionDegrees[1] < 16,
      );
      checks.push({ path, unchangedTracks, ...stats });
    }
  }
  const report = {
    date: "2026-10-09",
    sources: sources.filter((s) => !beforeDir || !s.path.startsWith(beforeDir)),
    before: beforeSources,
    checks,
    scope:
      "Exported canonical/runtime idle clips; chest-relative mirrored arm joints, world shoulder heights, torso yaw/roll, and unchanged other animation tracks.",
  };
  writeFileSync(
    new URL("doc/assets/modular-idle-symmetry-v1.json", root),
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report));
} finally {
  await server.close();
}

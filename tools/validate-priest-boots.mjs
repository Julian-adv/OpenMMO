import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";
import * as THREE from "../client/node_modules/three/build/three.module.js";
import {
  headlessThree,
  loadClips,
  clipSampler,
} from "./lib/headless-three.mjs";
const root = new URL("../", import.meta.url);
const { server, load, sources } = await headlessThree();
try {
  const mod = await server.ssrLoadModule("/src/lib/utils/modularCharacter.ts");
  const { priestBootRim } = await server.ssrLoadModule(
    "/src/lib/utils/priestBootCuff.ts",
  );
  const body = (
    await load("assets/modular_human_male_01/parts/fitted/base.glb")
  ).scene;
  const skin = mod.skinnedParts(body),
    parts = new Map();
  for (const [id, path] of Object.entries({
    hair_crop: "fitted/hair_crop.glb",
    top_priest: "priest_tripo_top_v1/top_priest.glb",
    pants_priest: "priest_tripo_pants_v1/pants_priest.glb",
    boots_priest: "priest_tripo_boots_v1/boots_priest.glb",
  }))
    parts.set(
      id,
      mod.bindModularPart(
        body,
        (await load("assets/modular_human_male_01/parts/" + path)).scene,
      ),
    );
  const tris = (ms) => ms.reduce((n, m) => n + m.geometry.index.count / 3, 0);
  const preserved = tris(skin) + tris([...parts.values()].flat());
  const pants = parts.get("pants_priest")[0],
    original = pants.geometry;
  const selected = mod.showModularOutfit(
    skin,
    parts,
    mod.PRIEST_MODULAR_OUTFIT,
  );
  assert.ok(selected.has("boots_priest"));
  const fitted = pants.geometry;
  assert.notEqual(fitted, original);
  let maxDeficit = -Infinity,
    hemVertices = 0;
  const point = new THREE.Vector3();
  for (let i = 0; i < fitted.attributes.position.count; i++) {
    point.fromBufferAttribute(fitted.attributes.position, i);
    const rim = priestBootRim(point),
      height = rim.height - 0.008;
    if (point.y < height + 1e-5) {
      hemVertices++;
      maxDeficit = Math.max(maxDeficit, rim.radius - rim.innerRadius);
      const joint = point.x > 0 ? "LeftLeg" : "RightLeg";
      let legWeight = 0,
        sum = 0;
      for (let j = 0; j < 4; j++) {
        const w = fitted.attributes.skinWeight.getComponent(i, j);
        sum += w;
        if (
          pants.skeleton.bones[fitted.attributes.skinIndex.getComponent(i, j)]
            .name === joint
        )
          legWeight += w;
      }
      assert.ok(Math.abs(sum - 1) < 1e-6);
      assert.ok(legWeight > 0.9999);
      assert.ok(rim.radius <= rim.innerRadius + 1e-5);
    }
  }
  assert.ok(hemVertices > 10);
  const clips = await loadClips(body, load, mod.modularAnimationClips);
  const sampler = clipSampler(body, {
    samples: 13,
    restore: pants.skeleton.bones,
  });
  for (const clip of clips)
    for (const _ of sampler(clip))
      for (const mesh of [pants, ...parts.get("boots_priest")])
        for (let i = 0; i < mesh.geometry.attributes.position.count; i++) {
          mesh.getVertexPosition(i, point);
          assert.ok(point.toArray().every(Number.isFinite));
        }
  mod.showModularOutfit(skin, parts, {
    ...mod.PRIEST_MODULAR_OUTFIT,
    boots: "none",
  });
  assert.equal(pants.geometry, original);
  assert.ok(
    skin.filter((m) => mod.region(m) === "feet").every((m) => m.visible),
  );
  mod.showModularOutfit(skin, parts, mod.PRIEST_MODULAR_OUTFIT);
  assert.equal(pants.geometry, fitted);
  const boots = parts.get("boots_priest");
  parts.delete("boots_priest");
  mod.showModularOutfit(skin, parts, mod.PRIEST_MODULAR_OUTFIT);
  assert.equal(pants.geometry, original);
  assert.ok(
    skin.filter((m) => mod.region(m) === "feet").every((m) => m.visible),
  );
  parts.set("boots_priest", boots);
  mod.showModularOutfit(skin, parts, mod.PRIEST_MODULAR_OUTFIT);
  const equipped = [
    ...skin,
    ...[...parts]
      .filter(([id]) => selected.has(id))
      .flatMap(([, meshes]) => meshes),
  ];
  const report = {
    date: "2026-10-09",
    sources,
    hem_vertices: hemVertices,
    maximum_hem_radial_deficit_m: maxDeficit,
    hem_rigid_to_own_leg: true,
    motion_samples: 91,
    clips: clips.map((c) => c.name),
    restoration: [
      "Boot removal restores complete trousers and bare feet",
      "Repeated selection reuses trimmed geometry",
      "Missing boot asset restores complete trousers and bare feet",
    ],
    budget: {
      scope:
        "Canonical body, crop hair, priest top, chainmail pants and boots; no gloves, helmet, weapon or cape",
      preserved_glb_triangles_including_hidden: preserved,
      runtime_triangles_including_hidden: tris(equipped),
      visible_triangles: tris(equipped.filter((m) => m.visible)),
      face_triangles: 1505,
    },
    scope:
      "Actual GLBs, rig binding, finite skinned vertices and tucked hem radius; not a full triangle collision test.",
  };
  writeFileSync(
    new URL("doc/assets/modular-priest-tripo-boots-assembly-v1.json", root),
    JSON.stringify(report, null, 2) + "\n",
  );
  console.log(JSON.stringify(report));
} finally {
  await server.close();
}

import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { clone } from '../client/node_modules/three/examples/jsm/utils/SkeletonUtils.js'
import { headlessThree, loadClips, clipSampler, hash, root } from './lib/headless-three.mjs'

const { server, load, sources } = await headlessThree()
try {
  const { bindModularPart, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { updatePeltPhysics, resetPeltPhysics, disposePeltPhysics } = await server.ssrLoadModule('/src/lib/effects/pelt-rig.ts')
  const base = await load('assets/modular_human_male_01/parts/fitted/base.glb')
  const robe = await load('assets/modular_human_male_01/parts/priest_tripo_top_v1/top_priest.glb')
  const body = clone(base.scene)
  const [mesh] = bindModularPart(body, robe.scene)
  const originalGeometry = mesh.geometry, originalSkeleton = mesh.skeleton
  const clips = await loadClips(body, load, modularAnimationClips, [
    ['client/public/models/characters/modular_male/animations/locomotion.glb', ['idle1', 'walk', 'run', 'jump']],
    ['client/public/models/characters/modular_male/animations/combat_melee.glb', ['slash1', 'combat_idle']],
    ['client/public/models/characters/modular_male/animations/social.glb', ['sit_idle']],
  ])
  const sampler = clipSampler(body, { samples: 13, restore: [...originalSkeleton.bones] })
  const report = { sources, simulation: { controls: 8, maximum_hz: 30, added_render_vertices: 0, exported_bones: 65, runtime_bones: 73 }, clips: [] }
  const duplicates = new Map()
  const rest = originalGeometry.attributes.position
  for (let i = 0; i < rest.count; i++) {
    const key = [rest.getX(i), rest.getY(i), rest.getZ(i)].map(v => v.toFixed(6)).join(',')
    if (!duplicates.has(key)) duplicates.set(key, [])
    duplicates.get(key).push(i)
  }
  const seams = [...duplicates.values()].filter(ids => ids.length > 1)
  for (const clip of clips) {
    let maximumSeamGap = 0
    for (const _sample of sampler(clip)) {
      resetPeltPhysics(body)
      updatePeltPhysics(body, 1 / 30)
      assert.equal(mesh.skeleton.bones.length, 73)
      const points = Array.from({ length: rest.count }, (_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
      for (const p of points) assert.ok(p.toArray().every(Number.isFinite))
      for (const ids of seams) for (const id of ids.slice(1)) maximumSeamGap = Math.max(maximumSeamGap, points[id].distanceTo(points[ids[0]]))
    }
    assert.ok(maximumSeamGap < 1e-5)
    report.clips.push({ name: clip.name, samples: 13, all_vertices_finite: true, maximum_seam_gap_m: maximumSeamGap })
  }
  assert.deepEqual(mesh.geometry.attributes.position.array, originalGeometry.attributes.position.array)
  const hair = await load('assets/modular_human_male_01/parts/fitted/hair_crop.glb')
  bindModularPart(body, hair.scene)
  assert.equal(mesh.skeleton.bones.length, 73)
  report.late_part_bind_preserves_robe = true
  disposePeltPhysics(body)
  assert.equal(mesh.geometry, originalGeometry)
  assert.equal(mesh.skeleton, originalSkeleton)
  report.disposal_restores_canonical_geometry_and_skeleton = true

  const actors = Array.from({ length: 100 }, () => {
    const body = clone(base.scene)
    const [mesh] = bindModularPart(body, robe.scene)
    updatePeltPhysics(body, 1 / 30)
    return { body, mesh }
  })
  assert.equal(new Set(actors.map(actor => actor.mesh.geometry)).size, 1)
  assert.equal(new Set(actors.map(actor => actor.mesh.skeleton)).size, actors.length)
  for (let frame = 0; frame < 30; frame++) for (const actor of actors) updatePeltPhysics(actor.body, 1 / 60)
  const elapsed = []
  for (let frame = 0; frame < 240; frame++) {
    const start = performance.now()
    for (const actor of actors) updatePeltPhysics(actor.body, 1 / 60)
    elapsed.push(performance.now() - start)
  }
  elapsed.sort((a, b) => a - b)
  report.benchmark = {
    environment: `Node ${process.version}; headless CPU-only; no rendering or animation mixers`,
    actors: actors.length, measured_frames: elapsed.length, frame_hz: 60, simulation_hz: 30,
    average_ms_per_100_actor_frame: elapsed.reduce((a, b) => a + b, 0) / elapsed.length,
    p95_ms_per_100_actor_frame: elapsed[Math.floor(elapsed.length * .95)],
    shared_robe_geometries: 1, independent_robe_skeletons: actors.length,
  }
  for (const actor of actors) disposePeltPhysics(actor.body)
  report.code = ['client/src/lib/effects/robe-rig.ts', 'client/src/lib/effects/pelt-rig.ts', 'client/src/lib/utils/modularCharacter.ts'].map(path => ({ path, sha256: hash(path) }))
  report.limits = 'Approximate thigh-direction clearance; no cloth mesh contacts, self-collision or wrinkle solver. Extreme raised-leg poses and mixed equipment need further visual acceptance. CPU timings exclude rendering.'
  writeFileSync(new URL('doc/assets/modular-priest-tripo-top-runtime-v1.json', root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report, null, 2))
} finally {
  await server.close()
}

import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { clipSampler, headlessThree, loadClips, root } from './lib/headless-three.mjs'

const { server, sources, load } = await headlessThree()
try {
  const { bindModularPart, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const body = (await load('assets/modular_human_male_01/fitted/base.glb')).scene
  const meshes = bindModularPart(body, (await load('assets/modular_human_male_01/caveman/tripo_bracer_v1/gloves_caveman.glb')).scene)
  assert.equal(meshes.length, 2)
  const records = meshes.map(mesh => {
    const geometry = mesh.geometry
    const side = mesh.name.endsWith('left') ? 'Left' : 'Right'
    const rest = Array.from({ length: geometry.attributes.position.count }, (_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
    const edges = new Map()
    let rigidBracer = 0
    for (let i = 0; i < rest.length; i++) {
      let sum = 0
      for (let j = 0; j < 4; j++) {
        const weight = geometry.attributes.skinWeight.array[i * 4 + j]
        sum += weight
        if (weight > 1e-6) assert.equal(mesh.skeleton.bones[geometry.attributes.skinIndex.array[i * 4 + j]].name, side + 'ForeArm')
      }
      assert.ok(Math.abs(sum - 1) < 1e-6)
      assert.equal(geometry.attributes.skinWeight.getX(i), 1)
      rigidBracer++
    }
    assert.ok(rigidBracer > 100)
    for (let i = 0; i < geometry.index.count; i += 3) for (let j = 0; j < 3; j++) {
      const a = geometry.index.getX(i + j), b = geometry.index.getX(i + (j + 1) % 3)
      const length = rest[a].distanceTo(rest[b])
      if (length > .001) edges.set([a, b].sort((a, b) => a - b).join(','), { a, b, length })
    }
    return { mesh, rest, edges: [...edges.values()], rigidBracer, extension: 0, rigidError: 0, motion: 0 }
  })
  const poses = clipSampler(body)
  const clips = []
  for (const clip of await loadClips(body, load, modularAnimationClips)) {
    const name = clip.name
    let extension = 0, rigidError = 0
    for (const _ of poses(clip)) {
      for (const record of records) {
        const { mesh, rest, edges } = record
        const points = rest.map((_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
        for (let i = 0; i < points.length; i++) {
          assert.ok(points[i].toArray().every(Number.isFinite), `${name}: nonfinite vertex`)
          record.motion = Math.max(record.motion, points[i].distanceTo(rest[i]))
        }
        for (const { a, b, length } of edges) {
          const ratio = points[a].distanceTo(points[b]) / length
          const error = Math.abs(ratio - 1)
          assert.ok(error < 1e-4, `${name}: rigid bracer changed length`)
          rigidError = Math.max(rigidError, error)
          record.rigidError = Math.max(record.rigidError, error)
          extension = Math.max(extension, ratio - 1)
          record.extension = Math.max(record.extension, ratio - 1)
        }
      }
    }
    clips.push({ clip: name, samples: 25, maximum_edge_extension_relative: extension, maximum_rigid_bracer_length_error_relative: rigidError })
  }
  assert.ok(records.every(record => record.motion > .1), 'Bracers did not follow animation')
  const report = {
    date: '2026-10-04',
    method: 'Actual canonical animation clips and modular binding; 25 evenly spaced skinned geometry samples per clip',
    sources,
    triangles: meshes.reduce((sum, mesh) => sum + mesh.geometry.index.count / 3, 0),
    exact_rig_binding: true,
    normalized_weights: true,
    opposite_arm_influences: false,
    clips,
    meshes: records.map(r => ({
      name: r.mesh.name,
      rigid_bracer_vertices: r.rigidBracer,
      maximum_edge_extension_relative: r.extension,
      maximum_rigid_bracer_length_error_relative: r.rigidError,
      maximum_animated_motion_m: r.motion,
    })),
    scope: 'Both bracers are rigid to their own ForeArm bone. Wrist and finger motion is independent; no cloth physics. Numeric checks do not certify every body intersection or equipment combination.',
  }
  writeFileSync(new URL('doc/assets/modular-caveman-tripo-bracer-animation-v1.json', root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report.clips))
} finally {
  await server.close()
}

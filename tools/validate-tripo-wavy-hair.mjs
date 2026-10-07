import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { CANONICAL_CLIPS_WITH_DYING, clipSampler, headlessThree, loadClips, root } from './lib/headless-three.mjs'

const ranger = process.argv.includes('--ranger')
const style = ranger ? 'ranger' : 'wavy'
const part = ranger ? 'hair_ranger' : 'hair_wavy_bone'
const output = `assets/modular_human_male_01/parts/hair_tripo_${style}_v1/`
const { server, sources, load } = await headlessThree()
try {
  const { bindModularPart, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const body = (await load('assets/modular_human_male_01/parts/fitted/base.glb')).scene
  const meshes = bindModularPart(body, (await load(output + part + '.glb')).scene)
  assert.equal(meshes.length, 1)
  const mesh = meshes[0]
  const geometry = mesh.geometry
  const positions = geometry.attributes.position
  const rest = Array.from({ length: positions.count }, (_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
  for (let i = 0; i < positions.count; i++) {
    assert.equal(geometry.attributes.skinWeight.getX(i), 1)
    assert.equal(mesh.skeleton.bones[geometry.attributes.skinIndex.getX(i)].name, 'Head')
  }
  const edges = []
  for (let i = 0; i < geometry.index.count; i += 3)
    for (let j = 0; j < 3; j++) {
      const a = geometry.index.getX(i + j), b = geometry.index.getX(i + (j + 1) % 3)
      const length = rest[a].distanceTo(rest[b])
      if (length > .001) edges.push({ a, b, length })
    }
  const head = mesh.skeleton.bones.find(b => b.name === 'Head')
  body.updateMatrixWorld(true)
  const inverseHead = head.matrixWorld.clone().invert()
  const headLocal = rest.map(p => p.clone().applyMatrix4(inverseHead))
  const animations = await loadClips(body, load, modularAnimationClips, CANONICAL_CLIPS_WITH_DYING)
  const poses = clipSampler(body)
  const clips = [], snapshots = []
  const bones = mesh.skeleton.bones
  const restMatrices = bones.map(b => b.matrixWorld.clone())
  let maximumMotion = 0
  for (const clip of animations) {
    const name = clip.name
    let headError = 0, edgeError = 0
    for (const { sample: frame, time } of poses(clip)) {
      const points = rest.map((_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
      for (let i = 0; i < points.length; i++) {
        assert.ok(points[i].toArray().every(Number.isFinite))
        const expected = headLocal[i].clone().applyMatrix4(head.matrixWorld)
        headError = Math.max(headError, points[i].distanceTo(expected))
        maximumMotion = Math.max(maximumMotion, points[i].distanceTo(rest[i]))
      }
      for (const { a, b, length } of edges)
        edgeError = Math.max(edgeError, Math.abs(points[a].distanceTo(points[b]) / length - 1))
      if (frame === (name === 'dying' ? 20 : 12))
        snapshots.push({ clip: name, time,
          bone_deformation_matrices: Object.fromEntries(bones.map((b, i) => [b.name, b.matrixWorld.clone().multiply(restMatrices[i].clone().invert()).elements])),
        })
    }
    assert.ok(headError < 1e-5, `${name}: Head binding error ${headError}`)
    assert.ok(edgeError < 1e-4, `${name}: hair distortion ${edgeError}`)
    clips.push({ clip: name, samples: 25, maximum_head_binding_error_m: headError, maximum_edge_length_error_relative: edgeError })
  }
  assert.ok(maximumMotion > .1)
  const report = { date: ranger ? '2026-10-08' : '2026-10-04', sources, rig_id: 'human_male_01_mixamo_candidate_v2',
    triangles: geometry.index.count / 3, bones: bones.length, normalized_weights: true,
    exact_head_following: true, maximum_animated_motion_m: maximumMotion, clips,
    scope: 'Canonical modular binding and 200 sampled poses. Head-fixed hair; no secondary hair physics. Intersections and equipment compatibility require separate review.',
  }
  const reportPath = ranger ? 'doc/assets/modular-ranger-tripo-hair-animation-v1.json' : 'doc/assets/modular-wavy-hair-animation-v1.json'
  writeFileSync(new URL(reportPath, root), JSON.stringify(report, null, 2) + '\n')
  if (!ranger)
    writeFileSync(new URL(output + 'animation-snapshots.json', root), JSON.stringify(snapshots, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally {
  await server.close()
}

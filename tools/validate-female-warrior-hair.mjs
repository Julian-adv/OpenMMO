import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'

const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const hairPath = 'assets/modular_human_female_01/hair/tripo_warrior_v1/fitted_v1/hair_female_warrior.glb'
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { scene: body, animations } = await load(basePath)
  const hair = bindModularPart(body, (await load(hairPath)).scene)
  assert.equal(hair.length, 1)
  body.updateMatrixWorld(true)
  const bones = hair[0].skeleton.bones
  assert.equal(bones.length, 65)
  const head = bones.find(bone => bone.name === 'Head')
  const inverseHead = head.matrixWorld.clone().invert()
  const inverseRest = bones.map(bone => bone.matrixWorld.clone().invert())
  const initial = hair.map(mesh => {
    const position = mesh.geometry.attributes.position
    const weights = mesh.geometry.attributes.skinWeight
    const joints = mesh.geometry.attributes.skinIndex
    const points = [], roots = [], edges = []
    for (let i = 0; i < position.count; i++) {
      const point = mesh.getVertexPosition(i, new THREE.Vector3())
      assert.ok(point.distanceTo(new THREE.Vector3().fromBufferAttribute(position, i)) < 1.8e-6)
      points.push(point)
      assert.ok(Math.abs(weights.getX(i) + weights.getY(i) + weights.getZ(i) + weights.getW(i) - 1) < 1e-6)
      if (position.getY(i) >= 1.611) {
        assert.equal(weights.getX(i), 1)
        assert.equal(bones[joints.getX(i)].name, 'Head')
        roots.push({ i, local: point.clone().applyMatrix4(inverseHead) })
      }
    }
    const index = mesh.geometry.index
    for (let i = 0; i < index.count; i += 3)
      for (let j = 0; j < 3; j++) {
        const a = index.getX(i + j), b = index.getX(i + (j + 1) % 3)
        const length = points[a].distanceTo(points[b])
        if (length > .0036) edges.push({ a, b, length })
      }
    return { points, roots, edges }
  })
  const sample = clipSampler(body, { samples: 61, restore: bones })
  const clips = [], snapshots = []
  for (const clip of animations) {
    let rootError = 0, maximumStretch = 0, maximumMotion = 0
    for (const { sample: frame, time } of sample(clip)) {
      hair.forEach((mesh, m) => {
        const { points: rest, roots, edges } = initial[m]
        const points = rest.map((p, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
        for (let i = 0; i < points.length; i++) {
          assert.ok(points[i].toArray().every(Number.isFinite))
          maximumMotion = Math.max(maximumMotion, points[i].distanceTo(rest[i]))
        }
        for (const { i, local } of roots)
          rootError = Math.max(rootError, points[i].distanceTo(local.clone().applyMatrix4(head.matrixWorld)))
        for (const { a, b, length } of edges)
          maximumStretch = Math.max(maximumStretch, points[a].distanceTo(points[b]) / length)
      })
      if (frame === 24)
        snapshots.push({ clip: clip.name, time, matrices: Object.fromEntries(bones.map((bone, i) => [bone.name, bone.matrixWorld.clone().multiply(inverseRest[i]).toArray()])) })
    }
    assert.ok(rootError < 1.8e-5, `${clip.name}: detached hair root`)
    assert.ok(maximumStretch < 3, `${clip.name}: excessive stretching ${maximumStretch}`)
    clips.push({ name: clip.name, samples: 61, maximum_root_error: rootError, maximum_edge_stretch: maximumStretch, stretch_review_required: maximumStretch >= 2, maximum_motion: maximumMotion })
  }
  const triangles = hair.reduce((sum, mesh) => sum + mesh.geometry.index.count / 3, 0)
  assert.equal(triangles, 1871)
  const report = { date: '2026-11-02', base: { path: basePath, sha256: hash(basePath) }, hair: { path: hairPath, sha256: hash(hairPath) },
    triangles, bones: bones.length, part_count: hair.length, exact_bind_compatibility: true, normalized_weights: true,
    unchanged_rest_shape: true, crown_follows_head: true, clips,
    scope: 'Rest binding, finite deformation, root attachment and stretching. Body collision and appearance reviewed separately; no secondary hair physics.' }
  writeFileSync(new URL('../doc/assets/modular-female-warrior-hair-animation-v1.json', import.meta.url), JSON.stringify(report, null, 2) + '\n')
  writeFileSync(new URL('../assets/modular_human_female_01/hair/tripo_warrior_v1/fitted_v1/animation-snapshots.json', import.meta.url), JSON.stringify(snapshots) + '\n')
  console.log(JSON.stringify(report))
} finally {
  await server.close()
}

import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'

const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const topPath = 'assets/items/female_knight_top/tripo_v1/fitted_v4/top_female_knight.glb'
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { scene: body, animations } = await load(basePath)
  const parts = bindModularPart(body, (await load(topPath)).scene)
  assert.equal(parts.length, 1)
  const [top] = parts
  body.updateMatrixWorld(true)
  const bones = top.skeleton.bones
  let bodyMesh
  body.traverse(object => { if (object.isSkinnedMesh && !bodyMesh) bodyMesh = object })
  for (let i = 0; i < bones.length; i++) {
    assert.equal(bones[i], bodyMesh.skeleton.bones[i])
    const actual = top.skeleton.boneInverses[i].elements
    const expected = bodyMesh.skeleton.boneInverses[i].elements
    assert.ok(actual.every((v, k) => Math.abs(v - expected[k]) < 1e-7))
  }
  assert.equal(bones.length, 65)
  const position = top.geometry.attributes.position
  const weights = top.geometry.attributes.skinWeight
  const joints = top.geometry.attributes.skinIndex
  const rest = []
  for (let i = 0; i < position.count; i++) {
    const point = top.getVertexPosition(i, new THREE.Vector3())
    assert.ok(point.distanceTo(new THREE.Vector3().fromBufferAttribute(position, i)) < 1.8e-6)
    rest.push(point)
    assert.ok(Math.abs(weights.getX(i) + weights.getY(i) + weights.getZ(i) + weights.getW(i) - 1) < 1e-6)
    for (let k = 0; k < 4; k++) {
      if (!weights.getComponent(i, k) || Math.abs(rest[i].x) < 1.8e-4) continue
      const name = bones[joints.getComponent(i, k)].name
      const side = rest[i].x > 0 ? 'Right' : 'Left'
      assert.ok(!(name.startsWith(side) && /Shoulder|Arm/.test(name)), `opposite-side ${name}`)
    }
  }
  const sample = clipSampler(body, { samples: 121, restore: bones })
  const index = top.geometry.index
  const spine1 = bones.findIndex(b => b.name === 'Spine1')
  const core = i => {
    let first = 0
    for (let k = 0; k < 4; k++) {
      const joint = joints.getComponent(i, k)
      if (joint === spine1) first += weights.getComponent(i, k)
    }
    return Math.abs(first - 1) < 1e-6
  }
  const plateBones = new Set(['LeftArm', 'RightArm', 'Spine1'])
  const plateBone = i => weights.getX(i) === 1 && plateBones.has(bones[joints.getX(i)].name)
    ? joints.getX(i) : -1
  const plateEdges = [], edges = [], allEdges = []
  for (let i = 0; i < index.count; i += 3)
    for (let k = 0; k < 3; k++) {
      const a = index.getX(i + k), b = index.getX(i + (k + 1) % 3)
      const length = rest[a].distanceTo(rest[b])
      if (length > .0018 && plateBone(a) >= 0 && plateBone(a) === plateBone(b))
        plateEdges.push([a, b, length])
      if (length > .0054) allEdges.push([a, b, length])
      if (length > .0018 && core(a) && core(b)) edges.push([a, b, length])
    }
  assert.ok(edges.length > 100)
  const clips = []
  for (const clip of animations) {
    let plateStretch = 0
    let maximumMotion = 0, maximumThoraxStretch = 0, maximumEdgeRatio = 0
    const posed = []
    for (const _ of sample(clip)) {
      for (let i = 0; i < position.count; i++) {
        const point = top.getVertexPosition(i, new THREE.Vector3())
        assert.ok(point.toArray().every(Number.isFinite))
        posed[i] = point
        maximumMotion = Math.max(maximumMotion, point.distanceTo(rest[i]))
      }
      for (const [a, b, length] of allEdges)
        maximumEdgeRatio = Math.max(maximumEdgeRatio, posed[a].distanceTo(posed[b]) / length)
      for (const [a, b, length] of plateEdges)
        plateStretch = Math.max(plateStretch, Math.abs(posed[a].distanceTo(posed[b]) / length - 1))
      for (const [a, b, length] of edges)
        maximumThoraxStretch = Math.max(maximumThoraxStretch, Math.abs(posed[a].distanceTo(posed[b]) / length - 1))
    }
    assert.ok(maximumEdgeRatio < 6, `${clip.name}: garment edge stretched ${maximumEdgeRatio} times`)
    assert.ok(maximumThoraxStretch < .02, `${clip.name}: rigid thorax distorted by ${maximumThoraxStretch}`)
    assert.ok(plateEdges.length > 6000)
    assert.ok(plateStretch < .0001, `${clip.name}: plate edge distortion ${plateStretch}`)
    clips.push({ name: clip.name, maximum_plate_edge_length_change: plateStretch, samples: 121, maximum_motion: maximumMotion, maximum_thorax_edge_length_change: maximumThoraxStretch, maximum_edge_length_ratio: maximumEdgeRatio })
  }
  const report = { date: '2026-11-20', base: { path: basePath, sha256: hash(basePath) }, top: { path: topPath, sha256: hash(topPath) },
    triangles: top.geometry.index.count / 3, bones: bones.length, exact_bind_compatibility: true, normalized_weights: true,
    unchanged_rest_shape: true, no_opposite_arm_influence: true, thorax_rigidity_checked_edges: edges.length, clips,
    rigid_plate_edges_checked: plateEdges.length,
    scope: 'Runtime binding and sampled edge lengths of the cuirass and both pauldrons.' }
  writeFileSync(new URL(`../doc/assets/modular-female-knight-top-animation-v4.json`, import.meta.url), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify({ ...report, clips: clips.map(c => c.name) }))
} finally {
  await server.close()
}

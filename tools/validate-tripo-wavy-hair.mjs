import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { fileURLToPath } from 'node:url'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'
import { createServer } from '../client/node_modules/vite/dist/node/index.js'

const root = new URL('../', import.meta.url)
const output = 'assets/modular_human_male_01/parts/hair_tripo_wavy_v1/'
const server = await createServer({
  root: fileURLToPath(new URL('client/', root)), configFile: false,
  optimizeDeps: { noDiscovery: true, include: [] },
  server: { middlewareMode: true, watch: null }, appType: 'custom',
})
globalThis.self = globalThis
const loader = new GLTFLoader().register(() => ({
  name: 'headless-materials', loadMaterial: async () => new THREE.MeshBasicMaterial(),
}))
const sources = []
async function load(path) {
  const data = readFileSync(new URL(path, root))
  sources.push({ path, sha256: createHash('sha256').update(data).digest('hex') })
  return loader.parseAsync(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), '')
}
try {
  const { bindModularPart, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const body = (await load('assets/modular_human_male_01/parts/fitted/base.glb')).scene
  const meshes = bindModularPart(body, (await load(output + 'hair_wavy_bone.glb')).scene)
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
  const corrected = modularAnimationClips(body, await load('assets/modular_human_male_01/rigged_hand_tuned/animations.glb'), 'corrected')
  const social = modularAnimationClips(body, await load('client/public/models/characters/modular_male/animations/social.glb'), 'corrected')
  const names = ['idle1', 'walk', 'run', 'jump', 'combat_idle', 'slash1', 'dying', 'sit_idle']
  const mixer = new THREE.AnimationMixer(body)
  const clips = [], snapshots = []
  const bones = mesh.skeleton.bones
  const restMatrices = bones.map(b => b.matrixWorld.clone())
  let maximumMotion = 0
  for (const name of names) {
    mixer.stopAllAction()
    const clip = [...corrected, ...social].find(c => c.name === name)
    assert.ok(clip, name)
    const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1)
    action.clampWhenFinished = true
    action.play()
    let headError = 0, edgeError = 0
    for (let frame = 0; frame <= 24; frame++) {
      mixer.setTime(clip.duration * frame / 24)
      body.updateMatrixWorld(true)
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
        snapshots.push({ clip: name, time: clip.duration * frame / 24,
          bone_deformation_matrices: Object.fromEntries(bones.map((b, i) => [b.name, b.matrixWorld.clone().multiply(restMatrices[i].clone().invert()).elements])),
        })
    }
    assert.ok(headError < 1e-5, `${name}: Head binding error ${headError}`)
    assert.ok(edgeError < 1e-4, `${name}: hair distortion ${edgeError}`)
    clips.push({ clip: name, samples: 25, maximum_head_binding_error_m: headError, maximum_edge_length_error_relative: edgeError })
  }
  assert.ok(maximumMotion > .1)
  const report = { date: '2026-10-04', sources, rig_id: 'human_male_01_mixamo_candidate_v2',
    triangles: geometry.index.count / 3, bones: bones.length, normalized_weights: true,
    exact_head_following: true, maximum_animated_motion_m: maximumMotion, clips,
    scope: 'Canonical modular binding and 200 sampled poses. Head-fixed hair; no secondary hair physics. Intersections and equipment compatibility require separate review.',
  }
  writeFileSync(new URL('doc/assets/modular-wavy-hair-animation-v1.json', root), JSON.stringify(report, null, 2) + '\n')
  writeFileSync(new URL(output + 'animation-snapshots.json', root), JSON.stringify(snapshots, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally {
  await server.close()
}

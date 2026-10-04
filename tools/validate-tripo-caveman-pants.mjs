import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { fileURLToPath } from 'node:url'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'
import { createServer } from '../client/node_modules/vite/dist/node/index.js'

const root = new URL('../', import.meta.url)
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
  const { updatePeltPhysics, resetPeltPhysics, disposePeltPhysics } = await server.ssrLoadModule('/src/lib/effects/pelt-rig.ts')
  const body = (await load('assets/modular_human_male_01/parts/fitted/base.glb')).scene
  const meshes = bindModularPart(body, (await load('assets/modular_human_male_01/parts/caveman_tripo_pants_v1/pants_caveman.glb')).scene)
  assert.equal(meshes.length, 6)
  const records = meshes.map(mesh => {
    const geometry = mesh.geometry
    const rest = Array.from({ length: geometry.attributes.position.count }, (_, i) => new THREE.Vector3().fromBufferAttribute(geometry.attributes.position, i))
    const edges = new Map()
    for (let i = 0; i < geometry.index.count; i += 3) for (let j = 0; j < 3; j++) {
      const a = geometry.index.getX(i + j), b = geometry.index.getX(i + (j + 1) % 3)
      const length = rest[a].distanceTo(rest[b])
      if (length > .001) edges.set([a, b].sort((a, b) => a - b).join(','), { a, b, length })
    }
    for (let i = 0; i < rest.length; i++) for (let j = 0; j < 4; j++) {
      if (geometry.attributes.skinWeight.array[i * 4 + j] > 1e-6)
        assert.equal(mesh.skeleton.bones[geometry.attributes.skinIndex.array[i * 4 + j]].name, 'Hips')
    }
    const physics = mesh.userData.pelt_physics
    let landmarks
    if (physics?.bend?.joint) {
      const pivot = new THREE.Vector3(...physics.pivot)
      const axis = new THREE.Vector3().crossVectors(new THREE.Vector3(...physics.outward), new THREE.Vector3(0, 1, 0))
      landmarks = [.15, .5, .9].map(depth => {
        let best = 0, score = Infinity
        for (let i = 0; i < rest.length; i++) {
          const offset = rest[i].clone().sub(pivot)
          const distance = (offset.y + depth * physics.length) ** 2 + offset.dot(axis) ** 2
          if (distance < score) { best = i; score = distance }
        }
        return best
      })
    }
    return { mesh, rest, edges: [...edges.values()], extension: 0, motion: 0, landmarks, fold: 0 }
  })
  const mixer = new THREE.AnimationMixer(body)
  const clips = []
  const packs = []
  for (const [path, names] of [
    ['assets/modular_human_male_01/rigged_hand_tuned/animations.glb', ['idle1', 'walk', 'run', 'jump', 'combat_idle', 'slash1']],
    ['client/public/models/characters/modular_male/animations/social.glb', ['sit_idle']],
  ]) {
    const animations = modularAnimationClips(body, await load(path), 'corrected')
    packs.push({ animations, names })
  }
  for (const { animations, names } of packs) {
    for (const name of names) {
      mixer.stopAllAction()
      const clip = animations.find(c => c.name === name)
      assert.ok(clip, name)
      const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1)
      action.clampWhenFinished = true
      action.play()
      mixer.update(0)
      body.updateMatrixWorld(true)
      updatePeltPhysics(body, 0)
      resetPeltPhysics(body)
      const frames = Math.ceil(clip.duration * 60)
      let extension = 0
      for (let frame = 0; frame <= frames; frame++) {
        if (frame) {
          mixer.update(1 / 60)
          body.updateMatrixWorld(true)
          updatePeltPhysics(body, 1 / 60)
        }
        if (frame % Math.max(1, Math.floor(frames / 12)) && frame !== frames) continue
        for (const record of records) {
          const { mesh, rest, edges } = record
          const position = mesh.geometry.attributes.position
          const points = rest.map((_, i) => new THREE.Vector3().fromBufferAttribute(position, i))
          if (record.landmarks) {
            const [a, b, c] = record.landmarks
            const outward = new THREE.Vector3(...mesh.userData.pelt_physics.outward)
            const tilt = (points, a, b) => {
              const direction = points[b].clone().sub(points[a])
              return Math.atan2(direction.dot(outward), -direction.y)
            }
            const change = tilt(points, b, c) - tilt(rest, b, c) - tilt(points, a, b) + tilt(rest, a, b)
            record.fold = Math.max(record.fold, Math.abs(Math.atan2(Math.sin(change), Math.cos(change))))
          }
          for (let i = 0; i < points.length; i++) {
            assert.ok(points[i].toArray().every(Number.isFinite), `${name}: nonfinite point`)
            record.motion = Math.max(record.motion, points[i].distanceTo(rest[i]))
          }
          for (const { a, b, length } of edges) {
            const error = Math.max(0, points[a].distanceTo(points[b]) / length - 1)
            assert.ok(error < .0212, `${name}: ${mesh.name} extension ${error}`)
            extension = Math.max(extension, error)
            record.extension = Math.max(record.extension, error)
          }
        }
      }
      clips.push({ clip: name, frames, maximum_edge_extension_relative: extension })
    }
  }
  const panels = records.filter(r => r.mesh.userData.pelt_physics)
  assert.equal(panels.length, 4)
  assert.ok(panels.every(r => r.motion > .01))
  assert.ok(panels.filter(r => r.landmarks).every(r => r.fold > .08), 'Side midpoint did not bend')
  disposePeltPhysics(body)
  for (const { mesh, rest } of records) for (let i = 0; i < rest.length; i++)
    assert.ok(new THREE.Vector3().fromBufferAttribute(mesh.geometry.attributes.position, i).equals(rest[i]))
  const report = { date: '2026-10-04', method: 'Actual runtime physics at 60 Hz, about 13 geometry samples per clip; extension measured against original GLB positions, not an already simulated pose', sources,
    mesh_count: meshes.length, panel_count: panels.length, all_skin_influences: 'Hips only', clips,
    maximum_allowed_extension: '2% plus 1 micrometer numerical allowance; edges shorter than 1mm excluded from relative-error reporting',
    cached_geometry_restored: true,
    meshes: records.map(r => ({ name: r.mesh.name, triangles: r.mesh.geometry.index.count / 3, maximum_edge_extension_relative: r.extension, maximum_local_motion_m: r.motion, maximum_midpoint_fold_radians: r.landmarks ? r.fold : undefined })) }
  writeFileSync(new URL('doc/assets/modular-caveman-tripo-pants-animation-v1.json', root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report.clips))
} finally {
  await server.close()
}

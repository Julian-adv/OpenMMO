import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'
import { createServer } from '../client/node_modules/vite/dist/node/index.js'

const { values: { part } } = parseArgs({ options: { part: { type: 'string', default: 'caveman' } } })
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
  const body = (await load('assets/modular_human_male_01/parts/fitted/base.glb')).scene
  const meshes = bindModularPart(body, (await load(`assets/modular_human_male_01/parts/${part}_tripo_boots_v1/boots_${part}.glb`)).scene)
  assert.equal(meshes.length, 2)
  const records = meshes.map(mesh => {
    const geometry = mesh.geometry
    const side = mesh.name.endsWith('left') ? 'Left' : 'Right'
    const rest = Array.from({ length: geometry.attributes.position.count }, (_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
    const edges = new Map()
    let rigidCalf = 0
    for (let i = 0; i < rest.length; i++) {
      let sum = 0
      for (let j = 0; j < 4; j++) {
        const weight = geometry.attributes.skinWeight.array[i * 4 + j]
        sum += weight
        if (weight > 1e-6) assert.ok(['Leg', 'Foot', 'ToeBase'].some(bone => mesh.skeleton.bones[geometry.attributes.skinIndex.array[i * 4 + j]].name === side + bone))
      }
      assert.ok(Math.abs(sum - 1) < 1e-6)
      if (rest[i].y < .115 && rest[i].z > .085 && rest[i].z < .145) {
        assert.equal(geometry.attributes.skinWeight.getY(i), 1)
        assert.equal(mesh.skeleton.bones[geometry.attributes.skinIndex.getY(i)].name, side + 'Foot')
      }
      if (rest[i].y > .18) {
        assert.equal(geometry.attributes.skinWeight.getX(i), 1)
        assert.equal(mesh.skeleton.bones[geometry.attributes.skinIndex.getX(i)].name, side + 'Leg')
        rigidCalf++
      }
    }
    assert.ok(rigidCalf > 100)
    for (let i = 0; i < geometry.index.count; i += 3) for (let j = 0; j < 3; j++) {
      const a = geometry.index.getX(i + j), b = geometry.index.getX(i + (j + 1) % 3)
      const length = rest[a].distanceTo(rest[b])
      if (length > .001) edges.set([a, b].sort((a, b) => a - b).join(','), { a, b, length, calf: rest[a].y > .18 && rest[b].y > .18 })
    }
    return { mesh, rest, edges: [...edges.values()], rigidCalf, extension: 0, calfError: 0, motion: 0 }
  })
  const packs = []
  for (const [path, names] of [
    ['assets/modular_human_male_01/rigged_hand_tuned/animations.glb', ['idle1', 'walk', 'run', 'jump', 'combat_idle', 'slash1']],
    ['client/public/models/characters/modular_male/animations/social.glb', ['sit_idle']],
  ]) packs.push({ animations: modularAnimationClips(body, await load(path), 'corrected'), names })
  const mixer = new THREE.AnimationMixer(body)
  const clips = []
  for (const { animations, names } of packs) for (const name of names) {
    mixer.stopAllAction()
    const clip = animations.find(c => c.name === name)
    assert.ok(clip, name)
    const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1)
    action.clampWhenFinished = true
    action.play()
    let extension = 0, calfError = 0
    for (let frame = 0; frame <= 24; frame++) {
      mixer.setTime(clip.duration * frame / 24)
      body.updateMatrixWorld(true)
      for (const record of records) {
        const { mesh, rest, edges } = record
        const points = rest.map((_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
        for (let i = 0; i < points.length; i++) {
          assert.ok(points[i].toArray().every(Number.isFinite), `${name}: nonfinite vertex`)
          record.motion = Math.max(record.motion, points[i].distanceTo(rest[i]))
        }
        for (const { a, b, length, calf } of edges) {
          const ratio = points[a].distanceTo(points[b]) / length
          if (calf) {
            const error = Math.abs(ratio - 1)
            assert.ok(error < 1e-4, `${name}: rigid calf changed length`)
            calfError = Math.max(calfError, error)
            record.calfError = Math.max(record.calfError, error)
          }
          extension = Math.max(extension, ratio - 1)
          record.extension = Math.max(record.extension, ratio - 1)
        }
      }
    }
    clips.push({ clip: name, samples: 25, maximum_edge_extension_relative: extension, maximum_rigid_calf_length_error_relative: calfError })
  }
  assert.ok(records.every(record => record.motion > .1), 'Boots did not follow animation')
  const report = {
    date: new Date().toISOString().slice(0, 10), method: 'Actual canonical animation clips and modular binding; 25 evenly spaced skinned geometry samples per clip', sources,
    triangles: meshes.reduce((sum, mesh) => sum + mesh.geometry.index.count / 3, 0), exact_rig_binding: true, normalized_weights: true, opposite_leg_influences: false,
    clips, meshes: records.map(r => ({ name: r.mesh.name, rigid_calf_vertices: r.rigidCalf, maximum_edge_extension_relative: r.extension,
      maximum_rigid_calf_length_error_relative: r.calfError, maximum_animated_motion_m: r.motion })),
    scope: 'Calf shape is rigid to its own Leg bone. Ankle and toe are deliberately deformable leather, with no cloth physics. Numeric checks do not certify every body intersection or equipment combination.',
  }
  writeFileSync(new URL(`doc/assets/modular-${part}-tripo-boots-animation-v1.json`, root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report.clips))
} finally {
  await server.close()
}

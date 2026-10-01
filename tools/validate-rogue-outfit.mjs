import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { createHash } from 'node:crypto'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'
import { measureAnkleConnections } from './outfits/ankle-section.mjs'
import { createServer } from '../client/node_modules/vite/dist/node/index.js'

const root = new URL('../', import.meta.url)
const selection = JSON.parse(readFileSync(new URL('doc/assets/modular-rogue-source-selection.json', root), 'utf8'))
const candidate = selection.fitting_candidate
const directory = candidate.directory + '/'
const fitting = JSON.parse(readFileSync(new URL(candidate.report, root), 'utf8'))
const server = await createServer({
  root: fileURLToPath(new URL('client/', root)),
  configFile: false,
  optimizeDeps: { noDiscovery: true, include: [] },
  server: { middlewareMode: true, watch: null },
  appType: 'custom',
})
const hash = (path) => createHash('sha256').update(readFileSync(new URL(path, root))).digest('hex')
globalThis.self = globalThis
const loader = new GLTFLoader().register(() => ({
  name: 'headless-materials',
  loadMaterial: async () => new THREE.MeshBasicMaterial(),
}))
async function load(path) {
  const data = readFileSync(new URL(path, root))
  return loader.parseAsync(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), '')
}
function checkAnkles(ankles, label) {
  for (const ankle of ankles) for (const plane of ankle.planes) {
    assert.equal(plane.missing_rays, 0, `${label} ${ankle.side}: open ankle section at ${plane.height_m}`)
    assert.ok(plane.minimum_clearance_m >= .002, `${label} ${ankle.side}: ankle clearance ${plane.minimum_clearance_m}`)
  }
}
try {
  const { bindModularPart, modularAnimationClips, skinnedParts } =
    await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const bodyPath = 'client/public/models/characters/modular_male/base.glb'
  const body = (await load(bodyPath)).scene
  assert.ok(skinnedParts(body).length > 0)
  const meshes = []
  const sources = [{ path: bodyPath, sha256: hash(bodyPath) }]
  for (const name of ['top_rogue', 'pants_rogue', 'gloves_rogue', 'boots_rogue']) {
    const path = directory + name + '.glb'
    const part = await load(path)
    meshes.push(...bindModularPart(body, part.scene))
    sources.push({ path, sha256: hash(path) })
  }
  body.updateMatrixWorld(true)
  const skeleton = meshes[0].skeleton
  for (const mesh of meshes) assert.equal(mesh.skeleton, skeleton)
  const original = skeleton.bones.map((bone) => ({ bone, p: bone.position.clone(), q: bone.quaternion.clone(), s: bone.scale.clone() }))
  const vector = new THREE.Vector3()
  const mixer = new THREE.AnimationMixer(body)
  const report = {
    status: 'Runtime bind and numeric skinning checks; not a visual gameplay acceptance',
    sources,
    bind_modular_part_passed: true,
    samples_per_clip: 13,
    clips: [],
  }
  const snapshots = []
  const rest = meshes.map((mesh) => ({ mesh, positions: Array.from({ length: mesh.geometry.attributes.position.count }, (_, i) => mesh.getVertexPosition(i, new THREE.Vector3()).applyMatrix4(mesh.matrixWorld)) }))
  report.rest_ankles = measureAnkleConnections(rest, fitting.ankle_connections, skeleton)
  checkAnkles(report.rest_ankles, 'rest')
  for (const [pack, names] of [
    ['locomotion', ['idle1', 'walk', 'run', 'jump']],
    ['combat_melee', ['slash1', 'combat_idle']],
    ['social', ['sit_idle']],
  ]) {
    const path = `client/public/models/characters/modular_male/animations/${pack}.glb`
    const gltf = await load(path)
    const clips = modularAnimationClips(body, gltf, 'corrected')
    report.sources.push({ path, sha256: hash(path) })
    for (const name of names) {
      const clip = clips.find((clip) => clip.name === name)
      assert.ok(clip, `${pack}/${name} missing`)
      const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1)
      action.clampWhenFinished = true
      action.play()
      const bounds = new THREE.Box3()
      const meshBounds = meshes.map(() => new THREE.Box3())
      const ankleSamples = []
      for (let sample = 0; sample < 13; sample++) {
        const time = clip.duration * sample / 12
        mixer.setTime(time)
        body.updateMatrixWorld(true)
        skeleton.update()
        const posed = []
        for (const [mi, mesh] of meshes.entries()) {
          const positions = mesh.geometry.attributes.position
          const vertices = []
          for (let i = 0; i < positions.count; i++) {
            mesh.getVertexPosition(i, vector).applyMatrix4(mesh.matrixWorld)
            assert.ok(vector.toArray().every(Number.isFinite), `${name}: nonfinite skinning`)
            bounds.expandByPoint(vector)
            meshBounds[mi].expandByPoint(vector)
            vertices.push(vector.clone())
          }
          posed.push({ mesh, positions: vertices })
        }
        const ankles = measureAnkleConnections(posed, fitting.ankle_connections, skeleton)
        checkAnkles(ankles, `${name} ${time}`)
        ankleSamples.push({ time, ankles })
        if (sample === 5 && name !== 'combat_idle') {
          snapshots.push({
            clip: name,
            time,
            bone_deformation_matrices: Object.fromEntries(skeleton.bones.map((bone, i) => [bone.name, new THREE.Matrix4().multiplyMatrices(bone.matrixWorld, skeleton.boneInverses[i]).toArray()])),
          })
        }
      }
      assert.ok(bounds.getSize(vector).length() < 4, `${name}: exploding geometry`)
      report.clips.push({
        name, duration: clip.duration,
        all_vertices_finite: true,
        ankle_samples: ankleSamples,
        bounds: { min: bounds.min.toArray(), max: bounds.max.toArray() },
        parts: meshes.map((mesh, i) => ({ name: mesh.name, min: meshBounds[i].min.toArray(), max: meshBounds[i].max.toArray() })),
      })
      console.log(`${name}: 13 poses, ${meshes.length} meshes finite, both ankle connections passed`)
      action.stop()
      mixer.uncacheClip(clip)
      for (const { bone, p, q, s } of original) {
        bone.position.copy(p)
        bone.quaternion.copy(q)
        bone.scale.copy(s)
      }
      body.updateMatrixWorld(true)
    }
  }
  const planes = [report.rest_ankles, ...report.clips.flatMap((clip) => clip.ankle_samples.map((sample) => sample.ankles))].flatMap((ankles) => ankles.flatMap((ankle) => ankle.planes))
  report.ankle_summary = {
    planes_tested: planes.length,
    radial_samples: planes.length * 144,
    missing_rays: planes.reduce((sum, plane) => sum + plane.missing_rays, 0),
    minimum_clearance_m: Math.min(...planes.map((plane) => plane.minimum_clearance_m)),
    required_clearance_m: .002,
    method: '144 radial rays at 7 planes per ankle in the inverse shared-weight frame; rest plus 13 poses per clip',
  }
  writeFileSync(new URL(candidate.animation_report, root), JSON.stringify(report, null, 2) + '\n')
  writeFileSync(new URL(directory + 'animation-snapshots.json', root), JSON.stringify(snapshots) + '\n')
} finally {
  await server.close()
}

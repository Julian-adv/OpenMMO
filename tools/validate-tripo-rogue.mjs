import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { createHash } from 'node:crypto'
import { parseArgs } from 'node:util'
import { measureAnkleConnections } from './outfits/ankle-section.mjs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'
import { createServer } from '../client/node_modules/vite/dist/node/index.js'

const root = new URL('../', import.meta.url)
const { values } = parseArgs({ options: {
  directory: { type: 'string', default: 'assets/modular_human_male_01/parts/rogue_tripo_v1' },
  part: { type: 'string', default: 'top_rogue' },
  report: { type: 'string', default: 'doc/assets/modular-rogue-tripo-animation-v1.json' },
  boots: { type: 'string' },
  fitting: { type: 'string' },
} })
const directory = values.directory.replace(/\/$/, '') + '/'
const hash = (path) => createHash('sha256').update(readFileSync(new URL(path, root))).digest('hex')
const server = await createServer({
  root: fileURLToPath(new URL('client/', root)), configFile: false,
  optimizeDeps: { noDiscovery: true, include: [] },
  server: { middlewareMode: true, watch: null }, appType: 'custom',
})
globalThis.self = globalThis
const loader = new GLTFLoader().register(() => ({
  name: 'headless-materials', loadMaterial: async () => new THREE.MeshBasicMaterial(),
}))
async function load(path) {
  const data = readFileSync(new URL(path, root))
  return loader.parseAsync(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), '')
}
try {
  const { bindModularPart, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const bodyPath = 'assets/modular_human_male_01/parts/fitted/base.glb'
  const body = (await load(bodyPath)).scene
  const partPath = directory + values.part + '.glb'
  const meshes = bindModularPart(body, (await load(partPath)).scene)
  assert.equal(meshes.length, 1)
  const mesh = meshes[0]
  const skeleton = mesh.skeleton
  let boots = [], references
  if (values.boots) {
    assert.ok(values.fitting, '--fitting is required with --boots')
    boots = bindModularPart(body, (await load(values.boots)).scene)
    references = JSON.parse(readFileSync(new URL(values.fitting, root), 'utf8')).ankle_connections
    for (const boot of boots) assert.equal(boot.skeleton, skeleton)
  }
  body.updateMatrixWorld(true)
  skeleton.update()
  const restBones = skeleton.bones.map((bone) => ({ bone, p: bone.position.clone(), q: bone.quaternion.clone(), s: bone.scale.clone() }))
  const positions = (part = mesh) => Array.from({ length: part.geometry.attributes.position.count }, (_, i) => part.getVertexPosition(i, new THREE.Vector3()).applyMatrix4(part.matrixWorld))
  const rest = positions()
  const leatherCore = rest.map((p) => values.part === 'pants_rogue'
    ? Math.abs(p.x) > .075 && p.y > .49 && p.y < .66 && p.z > .015
    : Math.abs(p.x) < .14 && p.y > 1.18 && p.y < 1.42)
  const indices = mesh.geometry.index.array
  const edges = new Map()
  for (let i = 0; i < indices.length; i += 3) for (const [a, b] of [[indices[i], indices[i + 1]], [indices[i + 1], indices[i + 2]], [indices[i + 2], indices[i]]]) {
    const length = rest[a].distanceTo(rest[b])
    if (length > .001) edges.set([a, b].sort((x, y) => x - y).join(','), { a, b, length })
  }
  const duplicates = new Map()
  for (const [i, p] of rest.entries()) {
    const key = p.toArray().map((x) => x.toFixed(6)).join(',')
    if (!duplicates.has(key)) duplicates.set(key, [])
    duplicates.get(key).push(i)
  }
  const seams = [...duplicates.values()].filter((group) => group.length > 1)
  const report = {
    status: 'Numeric runtime bind and skinning review; visual review is separate',
    sources: [bodyPath, partPath].map((path) => ({ path, sha256: hash(path) })),
    bind_modular_part_passed: true, samples_per_clip: 13, clips: [],
    strain_region: values.part === 'pants_rogue' ? 'Front knee region, including leather patches' : 'Central torso leather',
  }
  function checkAnkles(points) {
    const posed = [{ mesh, positions: points }, ...boots.map((part) => ({ mesh: part, positions: positions(part) }))]
    const result = measureAnkleConnections(posed, references, skeleton)
    for (const ankle of result) for (const plane of ankle.planes) {
      assert.equal(plane.missing_rays, 0, `${ankle.side}: open ankle section`)
      assert.ok(plane.minimum_clearance_m >= .002, `${ankle.side}: ankle clearance ${plane.minimum_clearance_m}`)
    }
    return result
  }
  if (references) {
    report.sources.push({ path: values.boots, sha256: hash(values.boots) },
      { path: values.fitting, sha256: hash(values.fitting) })
    report.rest_ankles = checkAnkles(rest)
  }
  const snapshots = []
  const poses = []
  const mixer = new THREE.AnimationMixer(body)
  for (const [pack, names] of [['locomotion', ['idle1', 'walk', 'run', 'jump']], ['combat_melee', ['slash1', 'combat_idle']], ['social', ['sit_idle']]]) {
    const path = `client/public/models/characters/modular_male/animations/${pack}.glb`
    const clips = modularAnimationClips(body, await load(path), 'corrected')
    report.sources.push({ path, sha256: hash(path) })
    for (const name of names) {
      const clip = clips.find((clip) => clip.name === name)
      assert.ok(clip, name)
      const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1)
      action.clampWhenFinished = true
      action.play()
      let maximumStretch = 1, maximumSeamGap = 0, worstTime = 0, worstEdge
      const bounds = new THREE.Box3()
      const stretches = []
      const leatherStrains = []
      const ankleSamples = []
      for (let sample = 0; sample < 13; sample++) {
        const time = clip.duration * sample / 12
        mixer.setTime(time)
        body.updateMatrixWorld(true)
        skeleton.update()
        const points = positions()
        if (references) ankleSamples.push({ time, ankles: checkAnkles(points) })
        for (const p of points) {
          assert.ok(p.toArray().every(Number.isFinite), `${name}: nonfinite vertex`)
          bounds.expandByPoint(p)
        }
        for (const { a, b, length } of edges.values()) {
          const ratio = points[a].distanceTo(points[b]) / length
          stretches.push(ratio)
          if (leatherCore[a] && leatherCore[b]) leatherStrains.push(Math.abs(ratio - 1))
          if (ratio > maximumStretch) { maximumStretch = ratio; worstTime = time; worstEdge = { a, b, length, rest: [rest[a].toArray(), rest[b].toArray()] } }
        }
        for (const group of seams) for (const i of group.slice(1)) maximumSeamGap = Math.max(maximumSeamGap, points[group[0]].distanceTo(points[i]))
        const matrices = skeleton.bones.map((bone, i) => new THREE.Matrix4().multiplyMatrices(bone.matrixWorld, skeleton.boneInverses[i]).toArray())
        poses.push({ clip: name, time, matrices })
        if (sample === 5) snapshots.push({ clip: name, time, bone_deformation_matrices: Object.fromEntries(skeleton.bones.map((bone, i) => [bone.name, matrices[i]])) })
      }
      assert.ok(bounds.getSize(new THREE.Vector3()).length() < 3, `${name}: exploding mesh`)
      assert.ok(maximumSeamGap < 1e-5, `${name}: open UV seam`)
      stretches.sort((a, b) => a - b)
      leatherStrains.sort((a, b) => a - b)
      report.clips.push({ name, maximum_edge_stretch_ratio: maximumStretch, stretch_p99: stretches[Math.floor(stretches.length * .99)], leather_core_strain_p95: leatherStrains[Math.floor(leatherStrains.length * .95)], worst_time: worstTime, worst_edge: worstEdge, maximum_seam_gap_m: maximumSeamGap, all_vertices_finite: true })
      if (references) report.clips.at(-1).ankle_samples = ankleSamples
      console.log(name, report.clips.at(-1))
      action.stop()
      mixer.uncacheClip(clip)
      for (const { bone, p, q, s } of restBones) { bone.position.copy(p); bone.quaternion.copy(q); bone.scale.copy(s) }
      body.updateMatrixWorld(true)
    }
  }
  writeFileSync(new URL(values.report, root), JSON.stringify(report, null, 2) + '\n')
  writeFileSync(new URL(directory + 'animation-snapshots.json', root), JSON.stringify(snapshots) + '\n')
  writeFileSync(new URL(directory + 'validation-poses.json', root), JSON.stringify(poses) + '\n')
} finally { await server.close() }

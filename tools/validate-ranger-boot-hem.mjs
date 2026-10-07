import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { fileURLToPath } from 'node:url'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'
import { createServer } from '../client/node_modules/vite/dist/node/index.js'

const root = new URL('../', import.meta.url)
const style = process.argv.includes('--rogue') ? 'rogue' : 'ranger'
const outfit = { pants: style, top: 'none' }
const pantsPath = `assets/modular_human_male_01/parts/${style}_tripo_pants_v1/pants_${style}.glb`
const cuff = JSON.parse(readFileSync(new URL('client/src/lib/data/rangerBootCuff.json', root)))
assert.equal(createHash('sha256').update(readFileSync(new URL(cuff.source, root))).digest('hex'), cuff.sha256)
const server = await createServer({ root: fileURLToPath(new URL('client/', root)), configFile: false,
  optimizeDeps: { noDiscovery: true, include: [] }, server: { middlewareMode: true, watch: null }, appType: 'custom' })
globalThis.self = globalThis
const loader = new GLTFLoader().register(() => ({ name: 'headless-materials', loadMaterial: async () => new THREE.MeshBasicMaterial({ side: THREE.DoubleSide }) }))
async function load(path) {
  const data = readFileSync(new URL(path, root))
  return loader.parseAsync(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), '')
}
try {
  const { bindModularPart, showModularOutfit, RANGER_MODULAR_OUTFIT, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { rangerBootRim } = await server.ssrLoadModule('/src/lib/utils/rangerBootCuff.ts')
  const body = (await load('assets/modular_human_male_01/parts/fitted/base.glb')).scene
  const pants = bindModularPart(body, (await load(pantsPath)).scene)
  const boots = bindModularPart(body, (await load(cuff.source)).scene)
  const parts = new Map([[`pants_${style}`, pants], ['boots_ranger', boots]])
  const original = pants[0].geometry
  const originalUV = Array.from(original.attributes.uv.array)
  showModularOutfit([], parts, { ...RANGER_MODULAR_OUTFIT, ...outfit })
  const trimmed = pants[0].geometry
  const hems = []
  const raycaster = new THREE.Raycaster()
  let minimumClearance = Infinity
  body.updateMatrixWorld(true)
  for (let i = 0; i < trimmed.attributes.position.count; i++) {
    const point = new THREE.Vector3().fromBufferAttribute(trimmed.attributes.position, i)
    const rim = rangerBootRim(point)
    if (Math.abs(point.y - (rim.height - cuff.overlap)) > 1e-6) continue
    const side = point.x > 0 ? 'Left' : 'Right'
    const leg = pants[0].skeleton.bones.find(b => b.name === side + 'Leg')
    assert.equal(trimmed.attributes.skinWeight.getX(i), 1)
    assert.equal(pants[0].skeleton.bones[trimmed.attributes.skinIndex.getX(i)], leg)
    assert.ok(rim.radius <= rim.innerRadius + 1e-6)
    const origin = new THREE.Vector3((point.x > 0 ? 1 : -1) * cuff.origin[0], point.y, cuff.origin[1])
    raycaster.set(origin, point.clone().sub(origin).normalize())
    const hits = raycaster.intersectObjects(boots, false)
    assert.ok(hits.length, `No cuff wall at hem ${i}`)
    const clearance = hits[0].distance - point.distanceTo(origin)
    assert.ok(clearance > .0005, `Hem ${i} crosses cuff: ${clearance}`)
    minimumClearance = Math.min(minimumClearance, clearance)
    const local = pants[0].getVertexPosition(i, new THREE.Vector3()).applyMatrix4(leg.matrixWorld.clone().invert())
    hems.push({ i, leg, local })
  }
  assert.ok(hems.length > 30)
  const snapshot = JSON.parse(execFileSync(fileURLToPath(new URL('.venv/bin/python', root)), ['tools/export-ranger-boot-pants.py', style], {
    cwd: fileURLToPath(root), input: JSON.stringify(pants.map(mesh => Object.fromEntries([
      ...Object.entries(mesh.geometry.attributes).map(([name, attribute]) => [name, Array.from(attribute.array)]),
      ['indices', Array.from(mesh.geometry.index.array)],
    ]))), encoding: 'utf8', maxBuffer: 8 * 1024 * 1024,
  }))
  const clips = []
  const packs = []
  for (const [path, names] of [
    ['assets/modular_human_male_01/rigged_hand_tuned/animations.glb', ['idle1', 'walk', 'run', 'jump', 'combat_idle', 'slash1']],
    ['client/public/models/characters/modular_male/animations/social.glb', ['sit_idle']],
  ]) {
    packs.push({ animations: modularAnimationClips(body, await load(path), 'corrected'), names })
  }
  const mixer = new THREE.AnimationMixer(body)
  let maximumHemMotionError = 0
  for (const { animations, names } of packs) {
    for (const name of names) {
      mixer.stopAllAction()
      const clip = animations.find(c => c.name === name)
      assert.ok(clip)
      const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1)
      action.clampWhenFinished = true
      action.play()
      for (let frame = 0; frame <= 24; frame++) {
        mixer.setTime(clip.duration * frame / 24)
        body.updateMatrixWorld(true)
        for (const { i, leg, local } of hems) {
          const posed = pants[0].getVertexPosition(i, new THREE.Vector3()).applyMatrix4(leg.matrixWorld.clone().invert())
          const error = posed.distanceTo(local)
          assert.ok(error < 1e-5)
          maximumHemMotionError = Math.max(maximumHemMotionError, error)
        }
      }
      clips.push({ name, samples: 25 })
    }
  }
  showModularOutfit([], parts, { ...RANGER_MODULAR_OUTFIT, ...outfit, top: 'plate' })
  const plate = pants[0].geometry.attributes.position
  for (let i = 0; i < plate.count; i++) assert.ok(plate.getY(i) <= (style === 'rogue' ? 1.105001 : 1.060001))
  showModularOutfit([], parts, { ...RANGER_MODULAR_OUTFIT, ...outfit, boots: 'none' })
  assert.equal(pants[0].geometry, original)
  assert.deepEqual(Array.from(original.attributes.uv.array), originalUV)
  const report = { date: '2026-10-07', pants_source: pantsPath, pants_sha256: createHash('sha256').update(readFileSync(new URL(pantsPath, root))).digest('hex'), cuff_source: cuff.source, cuff_sha256: cuff.sha256,
    rim_height_range_m: [Math.min(...cuff.profile.map(p => p[0])), Math.max(...cuff.profile.map(p => p[0]))],
    overlap_m: cuff.overlap, inset_m: cuff.inset, hem_vertices: hems.length, minimum_actual_inner_wall_clearance_m: minimumClearance,
    maximum_hem_relative_to_leg_motion_error_m: maximumHemMotionError, clips, snapshot,
    plate_waist_and_restore_checks: true, scope: 'Actual boot wall ray intersections at each runtime hem vertex; hem and cuff both rigid to own Leg across 175 posed samples. Does not certify all trouser triangles or other equipment combinations.' }
  writeFileSync(new URL(style === 'rogue' ? 'doc/assets/modular-ranger-tripo-boots-rogue-hem-v1.json' : 'doc/assets/modular-ranger-tripo-boots-hem-v2.json', root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally {
  await server.close()
}

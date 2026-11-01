import assert from 'node:assert/strict'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, root } from './lib/headless-three.mjs'
import { monsterRow, packClipNames, gripRotations } from './lib/humanoid-grip.mjs'
import { readGlb, glbWriter } from './lib/glb.mjs'

const id = process.argv[2]
assert.match(id ?? '', /^[a-z0-9_]+$/)
const row = monsterRow(id), source = `assets/monsters/${id}/rig_v1/${id}.glb`
const fit = JSON.parse(readFileSync(new URL(`assets/monsters/${id}/grip_v1/fit.json`, root)))
const tmp = `/tmp/${id}-bake`; mkdirSync(tmp, { recursive: true })
const { server, load } = await headlessThree()
try {
  const runtime = await server.ssrLoadModule('/src/lib/utils/characterAnimationUtils.ts')
  const { splitClipNames } = await server.ssrLoadModule('/src/lib/data/monsterDefs.ts')
  const { CHARACTER_ANIMATION_PACK_PATHS } = await server.ssrLoadModule('/src/lib/utils/modelPaths.ts')
  const target = await load(source)
  const packs = await Promise.all([CHARACTER_ANIMATION_PACK_PATHS.locomotion, CHARACTER_ANIMATION_PACK_PATHS.combatMelee].map(p => load(`client/public${p}`)))
  const requested = packClipNames(row, packs, splitClipNames)
  for (const key of ['animIdle', 'animWalk', 'animRun', 'animAttack', 'animAttackIdle', 'animDie']) assert.ok(splitClipNames(row[key]).some(name => requested.includes(name)), `No player-pack clip for ${key}`)
  const clips = []
  for (const pack of packs) clips.push(...await runtime.retargetAnimationsForCharacterModel(target.scene, pack.scene, pack.animations.filter(c => requested.includes(c.name))))
  assert.deepEqual(clips.map(c => c.name).sort(), [...requested].sort())
  const mesh = runtime.findPrimarySkinnedMesh(target.scene); assert.ok(mesh)
  const grip = gripRotations(mesh, fit, await load(fit.reference_model))
  for (const clip of clips) {
    clip.tracks = clip.tracks.filter(t => !grip.has(t.name.split('.')[0]))
    for (const [name, q] of grip) clip.tracks.push(new THREE.QuaternionKeyframeTrack(`${name}.quaternion`, [0, clip.duration], [...q.toArray(), ...q.toArray()]))
  }
  const output = await runtime.groundRetargetedClips(target.scene, clips, {
    plantedClips: ['animIdle', 'animWalk', 'animAttack', 'animAttackIdle'].flatMap(key => splitClipNames(row[key])),
    baselineClips: splitClipNames(row.animRun), restClip: splitClipNames(row.animDie)[0], restOffset: row.corpseGroundOffset ?? 0,
  })
  for (const clip of output) {
    clip.tracks = clip.tracks.map(track => {
      const size = track.getValueSize(), first = Array.from(track.values.slice(0, size))
      if (track.times.length <= 2 || !track.values.every((v, i) => Math.abs(v - first[i % size]) < 1e-7)) return track
      return new track.constructor(track.name, [0, clip.duration], [...first, ...first])
    })
  }
  const { json, bin } = readGlb(readFileSync(new URL(source, root)))
  const writer = glbWriter(json, bin), timeAccessors = new Map()
  json.animations = output.map(clip => {
    const channels = [], samplers = []
    for (const track of clip.tracks) {
      const [name, property] = track.name.split('.'), node = json.nodes.findIndex(n => n.name === name); assert.ok(node >= 0, name)
      const times = Array.from(track.times), key = Buffer.from(new Float32Array(times).buffer).toString('base64')
      if (!timeAccessors.has(key)) timeAccessors.set(key, writer.addAccessor(times, 'SCALAR', { min: [times[0]], max: [times.at(-1)] }))
      samplers.push({ input: timeAccessors.get(key), output: writer.addAccessor(track.values, track.getValueSize() === 4 ? 'VEC4' : 'VEC3'), interpolation: 'LINEAR' })
      channels.push({ sampler: samplers.length - 1, target: { node, path: property === 'quaternion' ? 'rotation' : 'translation' } })
    }
    return { name: clip.name, channels, samplers }
  })
  const intermediate = `${tmp}/${id}.glb`; writeFileSync(intermediate, writer.finish())
  execFileSync(process.execPath, ['tools/optimize-character-textures.mjs', intermediate, `client/public/models/monsters/${id}.glb`, '1024', '--all-textures', '--lossy-maps', '--resample-animations'], { cwd: new URL(root), stdio: 'inherit' })
  console.log(JSON.stringify(output.map(c => ({ name: c.name, duration: c.duration }))))
} finally { await server.close() }

import assert from 'node:assert/strict'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, root } from './lib/headless-three.mjs'
import { monsterRow, monsterPacks, packClipNames, gripRotations } from './lib/humanoid-grip.mjs'
import { readGlb, glbWriter } from './lib/glb.mjs'

const STANCE_BLEND_S = 0.25
const THRUST_CLIP = 'dagger_stab'
const THRUST_AIM_M = 1.5
const THRUST_EASE = 2.5

const id = process.argv[2]
assert.match(id ?? '', /^[a-z0-9_]+$/)
const row = monsterRow(id), source = `assets/monsters/${id}/rig_v1/${id}.glb`
const fit = JSON.parse(readFileSync(new URL(`assets/monsters/${id}/grip_v1/fit.json`, root)))

// A weapon pack's attack only plays until the next swing restarts it, so cut it
// at the weapon's interval and ease the tail into the first combat-stance pose.
function trimIntoStance(clip, end, stance) {
  const start = end - STANCE_BLEND_S
  clip.tracks = clip.tracks.map(track => {
    const size = track.getValueSize(), sample = track.createInterpolant()
    const target = stance.tracks.find(t => t.name === track.name)?.values.slice(0, size)
    const times = [...Array.from(track.times).filter(t => t < end), end], values = []
    for (const t of times) {
      let value = Array.from(sample.evaluate(t))
      if (target && t > start) {
        const w = THREE.MathUtils.smoothstep((t - start) / STANCE_BLEND_S, 0, 1)
        value = size === 4
          ? new THREE.Quaternion().fromArray(value).slerp(new THREE.Quaternion().fromArray(target), w).toArray()
          : value.map((v, i) => THREE.MathUtils.lerp(v, target[i], w))
      }
      values.push(...value)
    }
    return new track.constructor(track.name, times, values)
  })
  clip.resetDuration()
}

function landAt(clip, impact, landing) {
  for (const track of clip.tracks) track.times = track.times.map(t => t < impact ? t * landing / impact : t - impact + landing)
  clip.resetDuration()
}

// Poses the scene at clip time t and returns how far the right hand is ahead of the shoulder.
function reachSampler(clip, scene) {
  const mixer = new THREE.AnimationMixer(scene); mixer.clipAction(clip).play()
  const hand = scene.getObjectByName('RightHand'), shoulder = scene.getObjectByName('RightArm')
  const reach = t => { mixer.setTime(Math.min(t, clip.duration - 1e-5)); scene.updateMatrixWorld(true); return hand.getWorldPosition(new THREE.Vector3()).z - shoulder.getWorldPosition(new THREE.Vector3()).z }
  return Object.assign(reach, { release: () => { mixer.stopAllAction(); mixer.uncacheRoot(scene) } })
}

// Retime the thrust by reach so the arm speeds up from the cocked pose and stops dead at full extension.
function snapThrust(clip, scene) {
  const reach = reachSampler(clip, scene), step = 1 / 240
  const times = Array.from({ length: Math.floor(clip.duration / step) }, (_, i) => i * step), reaches = times.map(reach)
  reach.release()
  const full = Math.max(...reaches), peak = reaches.findIndex(r => r > full - 0.005)
  let start = peak; while (start > 0 && reaches[start - 1] < reaches[start]) start--
  const t0 = times[start], t1 = times[peak]
  const progress = reaches.slice(start, peak + 1).map(r => (r - reaches[start]) / (reaches[peak] - reaches[start]))
  const sourceTime = s => {
    const p = s ** THRUST_EASE, j = Math.max(1, progress.findIndex(v => v >= p)), a = progress[j - 1], b = progress[j]
    return times[start + j - 1] + step * (b > a ? (p - a) / (b - a) : 0)
  }
  const keys = Math.ceil((t1 - t0) * 120)
  clip.tracks = clip.tracks.map(track => {
    const size = track.getValueSize(), sample = track.createInterpolant(), keyTimes = [], values = []
    const keep = i => { keyTimes.push(track.times[i]); values.push(...track.values.slice(i * size, (i + 1) * size)) }
    track.times.forEach((t, i) => t < t0 && keep(i))
    for (let k = 0; k <= keys; k++) { keyTimes.push(t0 + (t1 - t0) * k / keys); values.push(...sample.evaluate(sourceTime(k / keys))) }
    track.times.forEach((t, i) => t > t1 && keep(i))
    return new track.constructor(track.name, keyTimes, values)
  })
}

// The pack was posed for the player's grip, so turn this grip's hand-space blade level at the target while the arm is thrust out.
function aimBlade(clip, scene, blade) {
  const reach = reachSampler(clip, scene), hand = scene.getObjectByName('RightHand')
  const track = clip.tracks.find(t => t.name === 'RightHand.quaternion'); assert.ok(track.times.length > clip.duration * 20)
  const poses = Array.from(track.times, t => ({ reach: reach(t), handQ: hand.getWorldQuaternion(new THREE.Quaternion()), at: hand.getWorldPosition(new THREE.Vector3()), parentQ: hand.parent.getWorldQuaternion(new THREE.Quaternion()) }))
  reach.release()
  const rest = poses[0].reach, full = Math.max(...poses.map(p => p.reach)), peak = poses.findIndex(p => p.reach === full)
  track.values = Float32Array.from(poses.flatMap(({ reach: r, handQ, at, parentQ }, i) => {
    const d = blade.clone().applyQuaternion(handQ), aim = new THREE.Vector3(-at.x, 0, THRUST_AIM_M - at.z).normalize()
    // Released early on the way back, where the pulled-in wrist would fold into the grip.
    const w = THREE.MathUtils.smoothstep((r - rest) / (full - rest), ...(i > peak ? [0.85, 0.97] : [0.5, 0.95]))
    const turn = new THREE.Quaternion().slerp(new THREE.Quaternion().setFromUnitVectors(d, aim), w)
    return parentQ.invert().multiply(turn.multiply(handQ)).toArray()
  }))
}
const tmp = `/tmp/${id}-bake`; mkdirSync(tmp, { recursive: true })
const { server, load } = await headlessThree()
try {
  const runtime = await server.ssrLoadModule('/src/lib/utils/characterAnimationUtils.ts')
  const { splitClipNames } = await server.ssrLoadModule('/src/lib/data/monsterDefs.ts')
  const { CHARACTER_ANIMATION_PACK_PATHS } = await server.ssrLoadModule('/src/lib/utils/modelPaths.ts')
  const { seatMonsterWeapon } = await server.ssrLoadModule('/src/lib/utils/monsterWeapon.ts')
  const { getWeaponEffectAxis } = await server.ssrLoadModule('/src/lib/utils/weaponEffectAxis.ts')
  const target = await load(source)
  const { paths, attack } = monsterPacks(row, CHARACTER_ANIMATION_PACK_PATHS)
  const packs = await Promise.all(paths.map(p => load(`client/public${p}`)))
  const requested = packClipNames(row, packs, splitClipNames)
  for (const key of ['animIdle', 'animWalk', 'animRun', 'animAttack', 'animAttackIdle', 'animDie']) assert.ok(splitClipNames(row[key]).some(name => requested.includes(name)), `No player-pack clip for ${key}`)
  const clips = []
  for (const pack of packs) clips.push(...await runtime.retargetAnimationsForCharacterModel(target.scene, pack.scene, pack.animations.filter(c => requested.includes(c.name))))
  assert.deepEqual(clips.map(c => c.name).sort(), [...requested].sort())
  const weaponAttack = clips.find(c => c.name === attack?.clip)
  if (weaponAttack) {
    const stance = clips.find(c => c.name === splitClipNames(row.animAttackIdle)[0]); assert.ok(stance)
    trimIntoStance(weaponAttack, attack.end, stance)
    landAt(weaponAttack, attack.impact, attack.landing)
    if (weaponAttack.name === THRUST_CLIP) {
      snapThrust(weaponAttack, target.scene)
      const seat = new THREE.Object3D(); seatMonsterWeapon(new THREE.Object3D(), seat, row)
      const blade = getWeaponEffectAxis((await load(`client/public/models/${attack.model}`)).scene).direction
      aimBlade(weaponAttack, target.scene, blade.clone().applyQuaternion(seat.quaternion))
    }
  }
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

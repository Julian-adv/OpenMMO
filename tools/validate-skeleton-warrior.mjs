import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'

const root = new URL('../', import.meta.url)
const readJson = (path) => JSON.parse(readFileSync(new URL(path, root), 'utf8'))
const definition = readJson('data/monsters.json').skeleton_warrior
assert.ok(definition, 'Generate the monster data before validating')
globalThis.self = globalThis
const loader = new GLTFLoader()
loader.register(() => ({
  name: 'GeometryValidation',
  loadTexture: () => Promise.resolve(null),
}))

async function load(path) {
  const bytes = readFileSync(new URL(path, root))
  return loader.parseAsync(
    bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
    ''
  )
}

const gltf = await load(`client/public/models/${definition.model}`)
const mesh = gltf.scene.getObjectByProperty('isSkinnedMesh', true)
assert.ok(mesh, 'Skeleton Warrior must retain its skinned mesh')
const triangles = mesh.geometry.index.count / 3
assert.equal(triangles, 10000, 'Import must preserve the supplied mesh')
assert.deepEqual(gltf.animations.map((clip) => clip.name).sort(), [
  'Attack',
  'Death',
  'Idle',
  'Run',
  'Walk',
])

const mixer = new THREE.AnimationMixer(gltf.scene)
const clips = new Map(gltf.animations.map((clip) => [clip.name, clip]))
for (const [name, value] of Object.entries(definition)) {
  if (!name.startsWith('anim') || !value) continue
  for (const clip of value.split('|')) {
    assert.ok(clips.has(clip), `Missing ${name}: ${clip}`)
  }
}

function sample(name, time) {
  mixer.stopAllAction()
  const action = mixer.clipAction(clips.get(name))
  action.reset().setLoop(THREE.LoopOnce, 1)
  action.clampWhenFinished = true
  action.play()
  mixer.setTime(time)
  gltf.scene.updateMatrixWorld(true)
  mesh.skeleton.update()
  const bounds = new THREE.Box3()
  const vertex = new THREE.Vector3()
  for (
    let index = 0;
    index < mesh.geometry.attributes.position.count;
    index++
  ) {
    mesh.getVertexPosition(index, vertex).applyMatrix4(mesh.matrixWorld)
    assert.ok(
      vertex.toArray().every(Number.isFinite),
      `${name}: invalid skinning`
    )
    bounds.expandByPoint(vertex)
  }
  const bone = (boneName) =>
    gltf.scene.getObjectByName(boneName).getWorldPosition(new THREE.Vector3())
  return {
    bounds,
    right: bone('R_Hand'),
    hip: bone('Hip'),
  }
}

for (const [name, clip] of clips) {
  for (let frame = 0; frame <= Math.ceil(clip.duration * 30); frame++) {
    const pose = sample(name, Math.min(frame / 30, clip.duration))
    assert.ok(
      pose.bounds.min.y > -0.015 &&
        pose.bounds.min.y < (name === 'Run' ? 0.5 : 0.015),
      `${name}: feet/corpse leave the ground`
    )
    assert.ok(
      pose.bounds.getSize(new THREE.Vector3()).length() < 4,
      `${name}: stretched mesh`
    )
  }
}

for (const name of ['Walk', 'Run']) {
  const start = sample(name, 0).hip
  for (let index = 1; index <= 20; index++) {
    const hip = sample(name, (clips.get(name).duration * index) / 20).hip
    assert.ok(
      Math.hypot(hip.x - start.x, hip.z - start.z) < 0.15,
      `${name}: root motion was not removed`
    )
  }
}

assert.equal(definition.weapon, 'morningstar')
assert.equal(definition.weaponBone, 'R_Hand')
assert.equal(definition.weaponOffset, 0.103)
assert.equal(definition.weaponOffsetX, 0.02)
assert.equal(definition.weaponOffsetZ, -0.004)
const handBone = gltf.scene.getObjectByName(definition.weaponBone)
assert.ok(handBone, 'Weapon attachment bone must exist')
const weapon = await load('client/public/models/weapons/morningstar.glb')
weapon.scene.position.set(
  definition.weaponOffsetX,
  definition.weaponOffset,
  definition.weaponOffsetZ
)
handBone.add(weapon.scene)

for (const [name, clip] of clips) {
  for (let frame = 0; frame <= Math.ceil(clip.duration * 120); frame++) {
    sample(name, Math.min(frame / 120, clip.duration))
    const bounds = new THREE.Box3().setFromObject(weapon.scene, true)
    assert.ok(bounds.min.y >= -0.015, `${name}: morningstar penetrates the ground`)
  }
}

const attack = definition.animAttack
const ready = sample(attack, 0)
const impact = sample(attack, definition.attackImpactDelay / 1000)
assert.ok(
  impact.right.z > ready.right.z + 0.4,
  'The striking hand must reach forward at impact'
)
const weaponBounds = new THREE.Box3().setFromObject(weapon.scene)
assert.ok(
  weaponBounds.getSize(new THREE.Vector3()).length() > 0.9,
  'The full-size morningstar must remain attached at impact'
)
const durationMs = Math.round(clips.get(attack).duration * 1000)
assert.equal(
  readJson('data/monster_attack_clips.json').skeleton_warrior,
  durationMs
)
assert.ok(
  definition.attackImpactDelay > 0 && definition.attackImpactDelay < durationMs
)
assert.ok(definition.attackCooldown >= durationMs)
const corpse = sample(
  definition.animDie,
  clips.get(definition.animDie).duration
)
assert.ok(corpse.bounds.max.y < 0.6, 'The death clip must finish lying down')
console.log(
  `Skeleton Warrior validated: ${triangles} triangles, ${clips.size} clips, ${durationMs}ms slash, ${definition.attackImpactDelay}ms impact`
)
console.log(
  fileURLToPath(new URL(`client/public/models/${definition.model}`, root))
)

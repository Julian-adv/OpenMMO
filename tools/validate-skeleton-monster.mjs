import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'

const root = new URL('../', import.meta.url)
const readJson = (path) => JSON.parse(readFileSync(new URL(path, root), 'utf8'))
const definition = readJson('data/monsters.json').skeleton
assert.ok(definition, 'Generate the monster data before validating')
globalThis.self = globalThis
const loader = new GLTFLoader()
loader.register(() => ({
  name: 'GeometryValidation',
  loadTexture: () => Promise.resolve(null),
}))
const bytes = readFileSync(new URL(`client/public/models/${definition.model}`, root))
const gltf = await loader.parseAsync(
  bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  ''
)
const mesh = gltf.scene.getObjectByProperty('isSkinnedMesh', true)
assert.ok(mesh, 'Skeleton must retain its skinned mesh')
const triangles = mesh.geometry.index.count / 3
assert.equal(triangles, 8375, 'Import must preserve the supplied mesh')
assert.deepEqual(gltf.animations.map(clip => clip.name).sort(), ['Attack', 'Death', 'Idle', 'Run', 'Walk'])
const mixer = new THREE.AnimationMixer(gltf.scene)
const clips = new Map(gltf.animations.map((clip) => [clip.name, clip]))
for (const [name, value] of Object.entries(definition)) {
  if (name.startsWith('anim') && value) {
    for (const clip of value.split('|')) assert.ok(clips.has(clip), `Missing ${name}: ${clip}`)
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
  for (let i = 0; i < mesh.geometry.attributes.position.count; i++) {
    mesh.getVertexPosition(i, vertex).applyMatrix4(mesh.matrixWorld)
    assert.ok(vertex.toArray().every(Number.isFinite), `${name}: invalid skinning`)
    bounds.expandByPoint(vertex)
  }
  const bone = (name) => gltf.scene.getObjectByName(name).getWorldPosition(new THREE.Vector3())
  return {
    bounds,
    right: bone('R_Hand'),
    left: bone('L_Hand'),
    hip: bone('Hip'),
    feet: [bone('R_Foot'), bone('L_Foot')],
  }
}

for (const [name, clip] of clips) {
  for (let frame = 0; frame <= Math.ceil(clip.duration * 30); frame++) {
    const pose = sample(name, Math.min(frame / 30, clip.duration))
    assert.ok(pose.bounds.min.y > -.015 && pose.bounds.min.y < (name === 'Run' ? .5 : .015), `${name}: feet/corpse leave the ground`)
    assert.ok(pose.bounds.getSize(new THREE.Vector3()).length() < 3.5, `${name}: stretched mesh`)
  }
}

for (const name of ['Walk', 'Run']) {
  const start = sample(name, 0).hip
  for (let i = 1; i <= 20; i++) {
    const hip = sample(name, clips.get(name).duration * i / 20).hip
    assert.ok(Math.hypot(hip.x - start.x, hip.z - start.z) < .08, `${name}: root motion was not removed`)
  }
}

const attack = definition.animAttack
const ready = sample(attack, 0)
const impact = sample(attack, definition.attackImpactDelay / 1000)
assert.ok(impact.right.z > ready.right.z + .15, 'The striking hand must reach forward at impact')
const durationMs = Math.round(clips.get(attack).duration * 1000)
assert.equal(readJson('data/monster_attack_clips.json').skeleton, durationMs)
assert.ok(definition.attackImpactDelay > 0 && definition.attackImpactDelay < durationMs)
assert.ok(definition.attackCooldown >= durationMs)
const corpse = sample(definition.animDie, clips.get(definition.animDie).duration)
assert.ok(corpse.bounds.max.y < .55, 'The death clip must finish lying down')
console.log(`Skeleton validated: ${triangles} triangles, ${clips.size} clips, ${durationMs}ms swipe, ${definition.attackImpactDelay}ms impact`)
console.log(fileURLToPath(new URL(`client/public/models/${definition.model}`, root)))

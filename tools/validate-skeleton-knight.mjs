import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'

const root = new URL('../', import.meta.url)
const readJson = (path) => JSON.parse(readFileSync(new URL(path, root), 'utf8'))
const definition = readJson('data/monsters.json').skeleton_knight
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
assert.ok(mesh, 'Skeleton Knight must retain its skinned mesh')
const triangles = mesh.geometry.index.count / 3
assert.equal(triangles, 10002, 'Import must preserve the supplied mesh')
assert.deepEqual(gltf.animations.map((clip) => clip.name).sort(), [
  'Attack',
  'Death',
  'Idle',
  'Run',
  'Walk',
])

const mixer = new THREE.AnimationMixer(gltf.scene)
const clips = new Map(gltf.animations.map((clip) => [clip.name, clip]))
gltf.scene.updateMatrixWorld(true)
const armSegments = ['L_Forearm', 'L_Hand', 'R_Forearm', 'R_Hand'].map((name) => {
  const bone = gltf.scene.getObjectByName(name)
  assert.ok(bone, `Missing arm bone: ${name}`)
  return {
    bone,
    length: bone.getWorldPosition(new THREE.Vector3()).distanceTo(
      bone.parent.getWorldPosition(new THREE.Vector3())
    ),
  }
})
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
  for (const { bone, length } of armSegments) {
    const currentLength = bone.getWorldPosition(new THREE.Vector3()).distanceTo(
      bone.parent.getWorldPosition(new THREE.Vector3())
    )
    assert.ok(
      Math.abs(currentLength - length) < 0.001,
      `${name} ${time}: ${bone.name} changes arm length (${currentLength} vs ${length})`
    )
  }
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
      pose.bounds.min.y > (name === 'Death' ? -0.27 : -0.015) &&
        pose.bounds.min.y < (name === 'Run' ? 0.5 : name === 'Death' ? .06 : 0.015),
      `${name} ${frame/30}: feet/corpse leave the ground (${pose.bounds.min.y})`
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

assert.equal(definition.weapon, 'weapons/skeleton_greatsword.glb')
assert.equal(definition.weaponBone, 'R_Hand')
assert.equal(definition.weaponOffset, 0.11)
assert.equal(definition.weaponOffsetX, 0)
assert.equal(definition.weaponOffsetZ, 0)
const handBone = gltf.scene.getObjectByName(definition.weaponBone)
assert.ok(handBone, 'Weapon attachment bone must exist')
const weapon = await load('client/public/models/weapons/skeleton_greatsword.glb')
weapon.scene.position.set(
  definition.weaponOffsetX,
  definition.weaponOffset,
  definition.weaponOffsetZ
)
handBone.add(weapon.scene)

const attack = definition.animAttack
const ready = sample(attack, 0)
const impact = sample(attack, definition.attackImpactDelay / 1000)
assert.ok(
  impact.right.z > ready.right.z + 0.3,
  'The striking hand must reach forward at impact'
)
const weaponBounds = new THREE.Box3().setFromObject(weapon.scene)
assert.ok(
  weaponBounds.getSize(new THREE.Vector3()).length() > 0.9,
  'The full-size skeleton_greatsword must remain attached at impact'
)
const durationMs = Math.round(clips.get(attack).duration * 1000)
assert.equal(
  readJson('data/monster_attack_clips.json').skeleton_knight,
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
assert.ok(corpse.hip.y < 0.35 && corpse.bounds.max.y < 1, 'The death clip must finish lying down with bent legs')
assert.ok(gltf.scene.getObjectByName('Head').getWorldPosition(new THREE.Vector3()).y < .4)
const point = new THREE.Vector3()
function bladeBounds() {
  const box = new THREE.Box3()
  weapon.scene.traverse((object) => {
    if (!object.isMesh) return
    const positions = object.geometry.attributes.position
    for (let index = 0; index < positions.count; index++) {
      point.fromBufferAttribute(positions, index).applyMatrix4(object.matrixWorld)
      box.expandByPoint(point)
    }
  })
  return box
}
for (const [name, clip] of clips) {
  let previousRotation
  let settledPosition
  for (let frame = 0; frame <= Math.floor(clip.duration * 60); frame++) {
    sample(name, frame / 60)
    const blade = bladeBounds()
    if (name === 'Death') {
      const handGrip = handBone.localToWorld(new THREE.Vector3(0, .11, 0))
      assert.ok(weapon.scene.getWorldPosition(new THREE.Vector3()).distanceTo(handGrip) < 1e-5, 'Death must retain the sword in the right hand')
      const rotation = weapon.scene.getWorldQuaternion(new THREE.Quaternion())
      if (previousRotation) assert.ok(previousRotation.angleTo(rotation) < .10, `Death ${frame/60}: sword rotation jumps`)
      previousRotation = rotation
      if (frame/60 >= 2.25) {
        if (settledPosition) assert.ok(handGrip.distanceTo(settledPosition) < .005, 'Sword must remain still after landing')
        settledPosition ??= handGrip
      }
    }
    assert.ok(blade.min.y > -.035, `${name} ${frame/60}: blade penetrates floor (${blade.min.y})`)
    if (['Idle', 'Walk', 'Run'].includes(name)) {
      assert.ok(blade.min.y < .065, `${name}: dragging blade lifts off ground`)
      const tip = weapon.scene.localToWorld(new THREE.Vector3(1.48, 0, 0))
      assert.ok(tip.z < -.7, `${name}: sword must trail behind the knight`)
    }
    if (name === 'Attack' && frame/60 >= 10/24 && frame/60 <= 40/24) {
      const left = gltf.scene.getObjectByName('L_Hand').localToWorld(new THREE.Vector3(0, .11, 0))
      const grip = weapon.scene.localToWorld(new THREE.Vector3(-.20, 0, 0))
      assert.ok(left.distanceTo(grip) < .06, `Attack ${frame/60}: left hand leaves hilt`)
    }
  }
}
console.log(
  `Skeleton Knight validated: ${triangles} triangles, ${clips.size} clips, ${durationMs}ms slash, ${definition.attackImpactDelay}ms impact`
)
console.log(
  fileURLToPath(new URL(`client/public/models/${definition.model}`, root))
)

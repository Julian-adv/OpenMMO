import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'
await ensureFemalePreview('animations')

const directory = 'assets/items/valkyrie_helmet/fitted_v2'
const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const partPath = `${directory}/helmet_female_valkyrie.glb`
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const base = await load(basePath)
  for (const data of JSON.parse(readFileSync('assets/modular_human_female_01/preview/animations.json')).clips) {
    const clip = THREE.AnimationClip.parse(data)
    const index = base.animations.findIndex(entry => entry.name === clip.name)
    if (index < 0) base.animations.push(clip)
    else base.animations[index] = clip
  }
  let body
  base.scene.traverse(node => { if (node.isSkinnedMesh && !body) body = node })
  const [helmet, ...others] = bindModularPart(base.scene, (await load(partPath)).scene)
  assert.equal(others.length, 0)
  assert.equal(helmet.geometry.index.count / 3, 1343)
  base.scene.updateMatrixWorld(true)
  const { bones, boneInverses } = helmet.skeleton
  assert.equal(bones.length, 65)
  bones.forEach((bone, i) => {
    assert.equal(bone, body.skeleton.bones[i])
    assert.deepEqual(boneInverses[i].elements, body.skeleton.boneInverses[i].elements)
  })
  const head = bones.find(bone => bone.name === 'Head')
  const inverseHead = head.matrixWorld.clone().invert()
  const { position, skinIndex, skinWeight } = helmet.geometry.attributes
  const rest = [], local = [], edges = []
  for (let i = 0; i < position.count; i++) {
    const point = helmet.getVertexPosition(i, new THREE.Vector3())
    assert.ok(point.distanceTo(new THREE.Vector3().fromBufferAttribute(position, i)) < 1.8e-6)
    assert.equal(bones[skinIndex.getX(i)].name, 'Head')
    assert.deepEqual([skinWeight.getX(i),skinWeight.getY(i),skinWeight.getZ(i),skinWeight.getW(i)], [1,0,0,0])
    rest.push(point)
    local.push(point.clone().applyMatrix4(inverseHead))
  }
  const index = helmet.geometry.index
  for (let i = 0; i < index.count; i += 3)
    for (let j = 0; j < 3; j++) {
      const a = index.getX(i+j), b = index.getX(i+(j+1)%3)
      const length = rest[a].distanceTo(rest[b])
      if (length > 1.8e-5) edges.push([a,b,length])
    }
  const clips = []
  const sample = clipSampler(base.scene, {samples:61, restore:bones})
  for (const clip of base.animations) {
    let attachmentError = 0, rigidError = 0
    for (const _ of sample(clip)) {
      const points = rest.map((_, i) => helmet.getVertexPosition(i, new THREE.Vector3()))
      for (let i = 0; i < points.length; i++) {
        assert.ok(points[i].toArray().every(Number.isFinite))
        attachmentError = Math.max(attachmentError,points[i].distanceTo(local[i].clone().applyMatrix4(head.matrixWorld)))
      }
      for (const [a,b,length] of edges) rigidError = Math.max(rigidError,Math.abs(points[a].distanceTo(points[b])/length-1))
    }
    assert.ok(attachmentError < 1.8e-5,`${clip.name}: detached helmet`)
    assert.ok(rigidError < 1e-4,`${clip.name}: deformed plate`)
    clips.push({name:clip.name,samples:61,maximum_attachment_error:attachmentError,maximum_rigid_error:rigidError})
  }
  const previous = await load('assets/items/valkyrie_helmet/fitted_v1/helmet_female_valkyrie.glb')
  let previousMesh
  previous.scene.traverse(node => { if (node.isSkinnedMesh) previousMesh = node })
  const oldPosition = previousMesh.geometry.attributes.position
  assert.equal(position.count, oldPosition.count)
  assert.deepEqual(helmet.geometry.index.array, previousMesh.geometry.index.array)
  assert.deepEqual(helmet.geometry.attributes.uv.array, previousMesh.geometry.attributes.uv.array)
  let moved = 0, stationary = 0, maximumTranslationError = 0
  for (let i = 0; i < position.count; i++) {
    assert.equal(position.getX(i), oldPosition.getX(i))
    assert.equal(position.getY(i), oldPosition.getY(i))
    const dz = position.getZ(i) - oldPosition.getZ(i)
    if (Math.abs(dz) < 1e-7) stationary++
    else {
      moved++
      maximumTranslationError = Math.max(maximumTranslationError, Math.abs(dz + 0.012))
    }
  }
  assert.ok(moved > 0 && stationary > 0)
  assert.ok(maximumTranslationError < 1e-7)
  const pendant = {moved_vertices: moved, stationary_vertices: stationary, translation_m:[0,0,-0.012],maximum_translation_error_m:maximumTranslationError,curve_preserved:true}
  const report = {pendant, base_sha256:hash(basePath),helmet_sha256:hash(partPath),triangles:1343,bones:65,bind_compatibility:true,head_weight:1,clips,scope:'Rigid Head attachment and finite motion; full hair and face clearance is not certified.'}
  writeFileSync(`${directory}/animation-validation.json`,JSON.stringify(report,null,2)+'\n')
  console.log(`Valkyrie helmet: ${report.triangles} triangles, ${report.clips.length} clips, 61 samples each; Head attachment passed`)
} finally { await server.close() }

import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'

await ensureFemalePreview('animations')
const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const directory = process.argv[2] ?? 'assets/items/valkyrie_starter/tripo_bottom_v1/fitted_v8'
const partPath = `${directory}/bottom_female_valkyrie.glb`
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { scene: body, animations: clips } = await load(basePath)
  for (const data of JSON.parse(readFileSync('assets/modular_human_female_01/preview/animations.json')).clips) {
    const clip = THREE.AnimationClip.parse(data)
    const index = clips.findIndex(entry => entry.name === clip.name)
    if (index < 0) clips.push(clip)
    else clips[index] = clip
  }
  let bodyMesh
  body.traverse(node => { if (node.isSkinnedMesh && !bodyMesh) bodyMesh = node })
  const parts = bindModularPart(body, (await load(partPath)).scene)
  assert.equal(parts.length, 1)
  const mesh = parts[0]
  assert.equal(mesh.skeleton, bodyMesh.skeleton)
  body.updateMatrixWorld(true)
  const { position, skinIndex, skinWeight } = mesh.geometry.attributes
  let maxRestError = 0
  for (let i = 0; i < position.count; i++) {
    let total = 0
    for (let j = 0; j < 4; j++) {
      const weight = skinWeight.getComponent(i, j)
      total += weight
      if (weight) assert.ok(['Hips', 'LeftUpLeg', 'RightUpLeg'].includes(mesh.skeleton.bones[skinIndex.getComponent(i, j)].name))
    }
    assert.ok(Math.abs(total - 1) < 1e-5)
    const original = new THREE.Vector3().fromBufferAttribute(position, i)
    maxRestError = Math.max(maxRestError, original.distanceTo(mesh.getVertexPosition(i, new THREE.Vector3())))
  }
  assert.ok(maxRestError < 1e-5)
  const sample = clipSampler(body, { samples: 13, restore: mesh.skeleton.bones })
  const results = []
  for (const name of ['idle1', 'walk', 'run']) {
    const clip = clips.find(c => c.name === name)
    assert.ok(clip, name)
    let maximumDisplacement = 0
    for (const _ of sample(clip)) {
      for (let i = 0; i < position.count; i++) {
        const posed = mesh.getVertexPosition(i, new THREE.Vector3())
        assert.ok(posed.toArray().every(Number.isFinite))
        maximumDisplacement = Math.max(maximumDisplacement, posed.distanceTo(new THREE.Vector3().fromBufferAttribute(position, i)))
      }
    }
    results.push({ name, samples: 13, maximum_displacement_m: maximumDisplacement })
  }
  const report = { body_sha256: hash(basePath), bottom_sha256: hash(partPath), triangles: mesh.geometry.index.count / 3,
    vertices: position.count, shared_skeleton: true, normalized_weights: true, maximum_rest_error_m: maxRestError, clips: results,
    scope: 'Numeric binding and finite skinning checks; body, armor and hand collisions are not measured.' }
  writeFileSync(`${directory}/rig-validation.json`, JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally {
  await server.close()
}

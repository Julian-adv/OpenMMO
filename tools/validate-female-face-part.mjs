import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler } from './lib/headless-three.mjs'

const directory = 'client/public/models/characters/modular_female/'
const { server, load, sources } = await headlessThree()
try {
  const { bindModularPart, skinnedParts } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const base = await load(directory + 'base.glb')
  const part = await load(directory + 'face_custom.glb')
  const scene = base.scene
  const meshes = bindModularPart(scene, part.scene)
  scene.updateMatrixWorld(true)
  const body = skinnedParts(scene).find(mesh => {
    const association = base.parser.associations.get(mesh)
    return association?.meshes === 0 && association.primitives === 0
  })
  const neck = meshes.find(mesh => mesh.userData.female_neck)
  const head = meshes.find(mesh => mesh.userData.female_face)
  const face = part.scene.userData.female_face_part
  assert.equal(body.geometry.index.count / 3, face.body_triangles)
  assert.equal(neck.geometry.index.count / 3, face.strip_source_triangles.length)

  const key = (mesh, i) => new THREE.Vector3().fromBufferAttribute(mesh.geometry.attributes.position, i)
    .toArray().map(value => Math.round(value * 1e6)).join(',')
  const hidden = new Set(face.hide_triangle_indices)
  const visibleBody = new Map()
  const bodyIndex = body.geometry.index
  for (let t = 0; t < face.body_triangles; t++) {
    if (hidden.has(t)) continue
    for (let k = 0; k < 3; k++) {
      const i = bodyIndex.getX(t * 3 + k)
      visibleBody.set(key(body, i), i)
    }
  }
  const headVertices = new Map()
  for (let i = 0; i < head.geometry.attributes.position.count; i++) headVertices.set(key(head, i), i)
  const lower = [], upper = []
  for (let i = 0; i < neck.geometry.attributes.position.count; i++) {
    const k = key(neck, i)
    if (visibleBody.has(k)) lower.push([i, visibleBody.get(k)])
    if (headVertices.has(k)) upper.push([i, headVertices.get(k)])
  }
  assert.ok(lower.length > 80, `body seam ${lower.length}`)
  assert.ok(upper.length > 80, `head seam ${upper.length}`)

  const bones = body.skeleton.bones
  const sampler = clipSampler(scene, { samples: 25, restore: bones })
  const clips = []
  const a = new THREE.Vector3(), b = new THREE.Vector3()
  for (const clip of base.animations.filter(clip => !clip.name.includes('|'))) {
    let bodySeam = 0, headSeam = 0
    for (const _ of sampler(clip)) {
      for (const [i, j] of lower) bodySeam = Math.max(bodySeam, neck.getVertexPosition(i, a).distanceTo(body.getVertexPosition(j, b)))
      for (const [i, j] of upper) headSeam = Math.max(headSeam, neck.getVertexPosition(i, a).distanceTo(head.getVertexPosition(j, b)))
    }
    assert.ok(bodySeam < 3e-6, `${clip.name}: body seam ${bodySeam}`)
    assert.ok(headSeam < 3e-6, `${clip.name}: head seam ${headSeam}`)
    clips.push({ name: clip.name, samples: 25, maximum_body_seam_error_m: bodySeam, maximum_head_seam_error_m: headSeam })
  }
  const report = {
    date: '2026-11-08', sources, body_seam_vertices: lower.length, head_seam_vertices: upper.length, clips,
    scope: 'base.glb with face_custom.glb bound as a part: neck strip to visible body and to head, across the canonical clips.',
  }
  writeFileSync(new URL('../doc/assets/modular-female-face-part-seams.json', import.meta.url), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify({ ...report, sources: undefined }))
} finally {
  await server.close()
}

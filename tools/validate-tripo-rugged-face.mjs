import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { CANONICAL_CLIPS_WITH_DYING, clipSampler, headlessThree, loadClips, root } from './lib/headless-three.mjs'

const ranger = process.argv.includes('--ranger')
const faceName = ranger ? 'face_ranger' : 'face_rugged'
const output = `assets/modular_human_male_01/faces/tripo_${ranger ? 'ranger' : 'rugged'}_v1/`
const { server, sources, load } = await headlessThree()
const vertex = (mesh, i) => mesh.getVertexPosition(i, new THREE.Vector3())
function edgeMatches(mesh, positions) {
  const indices = mesh.geometry.index
  return positions.map(point => {
    let best
    const target = new THREE.Vector3().fromArray(point)
    for (let i = 0; i < indices.count; i += 3) {
      const ids = [0, 1, 2].map(j => indices.getX(i + j))
      const triangle = new THREE.Triangle(...ids.map(j => vertex(mesh, j)))
      const closest = triangle.closestPointToPoint(target, new THREE.Vector3())
      const distance = closest.distanceTo(target)
      if (!best || distance < best.distance)
        best = { ids, distance, bary: triangle.getBarycoord(closest, new THREE.Vector3()).toArray() }
    }
    assert.ok(best.distance < 2e-6, `Rest seam ${best.distance}`)
    return best
  })
}
try {
  const { bindModularPart, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const body = (await load('assets/modular_human_male_01/fitted/base.glb')).scene
  const faceMeshes = bindModularPart(body, (await load(output + faceName + '.glb')).scene)
  const hair = bindModularPart(body, (await load(ranger
    ? 'assets/modular_human_male_01/hair/tripo_ranger_v1/hair_ranger.glb'
    : 'assets/modular_human_male_01/hair/tripo_wavy_v1/hair_wavy_bone.glb')).scene)[0]
  assert.equal(faceMeshes.length, 3)
  const face = faceMeshes.find(m => m.name === faceName)
  const bridge = faceMeshes.find(m => m.name === 'face_neck_bridge')
  const lowerNeck = faceMeshes.find(m => m.name === 'face_neck_lower')
  let neck
  body.traverse(n => { if (n.isSkinnedMesh && n.userData.region === 'neck') neck = n })
  assert.ok(neck)
  body.updateMatrixWorld(true)
  const seam = JSON.parse(readFileSync(new URL(output + 'neck-seam-validation.json', root)))
  const lower = edgeMatches(lowerNeck, seam.lower)
  const bodySeam = edgeMatches(neck, seam.body_seam)
  const bodySeamVertices = seam.body_seam.map(p => {
    const target = new THREE.Vector3().fromArray(p)
    const ids = Array.from({ length: lowerNeck.geometry.attributes.position.count }, (_, i) => i)
    return ids.reduce((a, b) => vertex(lowerNeck, a).distanceTo(target) < vertex(lowerNeck, b).distanceTo(target) ? a : b)
  })
  const lastRow = seam.upper_start
  const upper = edgeMatches(face, seam.upper)
  const head = face.skeleton.bones.find(b => b.name === 'Head')
  const inverseHead = head.matrixWorld.clone().invert()
  const rigid = []
  for (const mesh of [...faceMeshes, hair]) {
    const g = mesh.geometry
    for (let i = 0; i < g.attributes.position.count; i++) {
      const weight = [0, 1, 2, 3].map(j => g.attributes.skinWeight.getComponent(i, j))
      assert.ok(Math.abs(weight.reduce((a, b) => a + b, 0) - 1) < 1e-6)
      const p = vertex(mesh, i)
      if (mesh === hair || (mesh === face && (p.y > 1.720001 || (p.z > .045001 && p.y > 1.666001)))) {
        assert.equal(weight[0], 1)
        assert.equal(mesh.skeleton.bones[g.attributes.skinIndex.getX(i)].name, 'Head')
        rigid.push({ mesh, index: i, local: p.clone().applyMatrix4(inverseHead) })
      }
    }
  }
  const animations = await loadClips(body, load, modularAnimationClips, CANONICAL_CLIPS_WITH_DYING)
  const poses = clipSampler(body)
  const bones = face.skeleton.bones
  const restMatrices = bones.map(b => b.matrixWorld.clone())
  const clips = [], snapshots = []
  for (const clip of animations) {
    const name = clip.name
    let lowerError = 0, upperError = 0, rigidError = 0, bodyError = 0
    for (const { sample: frame, time } of poses(clip)) {
      for (const mesh of [...faceMeshes, hair])
        for (let i = 0; i < mesh.geometry.attributes.position.count; i++)
          assert.ok(vertex(mesh, i).toArray().every(Number.isFinite))
      for (const { mesh, index, local } of rigid)
        rigidError = Math.max(rigidError, vertex(mesh, index).distanceTo(local.clone().applyMatrix4(head.matrixWorld)))
      for (const [reference, matches, offset, which] of [[lowerNeck, lower, 0, 'lower'], [face, upper, lastRow, 'upper']]) {
        for (let i = 0; i < matches.length; i++) {
          const expected = new THREE.Vector3()
          matches[i].ids.forEach((id, j) => expected.addScaledVector(vertex(reference, id), matches[i].bary[j]))
          const error = vertex(bridge, offset + i).distanceTo(expected)
          if (which === 'lower') lowerError = Math.max(lowerError, error)
          else upperError = Math.max(upperError, error)
        }
      }
      for (let i = 0; i < bodySeam.length; i++) {
        const expected = new THREE.Vector3()
        bodySeam[i].ids.forEach((id, j) => expected.addScaledVector(vertex(neck, id), bodySeam[i].bary[j]))
        bodyError = Math.max(bodyError, vertex(lowerNeck, bodySeamVertices[i]).distanceTo(expected))
      }
      if (!ranger && frame === (name === 'dying' ? 20 : 12))
        snapshots.push({ clip: name, time,
          bone_deformation_matrices: Object.fromEntries(bones.map((b, i) => [b.name, b.matrixWorld.clone().multiply(restMatrices[i].clone().invert()).elements])) })
    }
    assert.ok(bodyError < 1e-6, `${name}: retained neck seam ${bodyError}`)
    assert.ok(rigidError < 1e-5, `${name}: rigid face/hair ${rigidError}`)
    assert.ok(lowerError < 1e-5, `${name}: body seam ${lowerError}`)
    assert.ok(upperError < 1e-6, `${name}: face seam ${upperError}`)
    clips.push({ clip: name, samples: 25, body_neck_seam_error_m: bodyError, lower_seam_error_m: lowerError, upper_seam_error_m: upperError, rigid_head_error_m: rigidError })
  }
  const report = { date: ranger ? '2026-10-08' : '2026-10-05', sources, bones: bones.length, normalized_weights: true,
    face_triangles: face.geometry.index.count / 3, retained_neck_triangles: lowerNeck.geometry.index.count / 3, neck_bridge_triangles: bridge.geometry.index.count / 3,
    hair_triangles: hair.geometry.index.count / 3, clips,
    scope: '200 actual game poses; finite deformation, rigid face and hair, interpolated neck and head seams. Facial expressions and all equipment intersections are outside this validation.' }
  writeFileSync(new URL(`doc/assets/modular-${ranger ? 'ranger-tripo' : 'rugged'}-face-animation-v1.json`, root), JSON.stringify(report, null, 2) + '\n')
  if (!ranger) writeFileSync(new URL(output + 'animation-snapshots.json', root), JSON.stringify(snapshots, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally {
  await server.close()
}

import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, clipSampler, hash } from './lib/headless-three.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'
await ensureFemalePreview('animations')

const directory = 'assets/items/female_caveman_bracers/tripo_v1/fitted_v1'
const basePath = 'assets/modular_human_female_01/rig_v1/female-base.glb'
const partPath = `${directory}/gloves_female_caveman.glb`
const TOLERANCE = 0.00054
const RAYS = [
  new THREE.Vector3(0.31, 0.52, 0.79).normalize(),
  new THREE.Vector3(-0.67, 0.2, -0.71).normalize(),
]

function solid(mesh) {
  const position = mesh.geometry.attributes.position
  const index = mesh.geometry.index
  const triangles = []
  for (let i = 0; i < index.count; i += 3)
    triangles.push(
      [0, 1, 2].map((k) =>
        new THREE.Vector3().fromBufferAttribute(position, index.getX(i + k))
      )
    )
  const box = new THREE.Box3().setFromBufferAttribute(position)
  const crossings = (origin, direction) => {
    let hits = 0
    const e1 = new THREE.Vector3(),
      e2 = new THREE.Vector3(),
      h = new THREE.Vector3(),
      s = new THREE.Vector3(),
      q = new THREE.Vector3()
    for (const [a, b, c] of triangles) {
      e1.subVectors(b, a)
      e2.subVectors(c, a)
      h.crossVectors(direction, e2)
      const det = e1.dot(h)
      if (Math.abs(det) < 1e-12) continue
      s.subVectors(origin, a)
      const u = s.dot(h) / det
      if (u < 0 || u > 1) continue
      q.crossVectors(s, e1)
      const v = direction.dot(q) / det
      if (v < 0 || u + v > 1) continue
      if (e2.dot(q) / det > 0) hits++
    }
    return hits
  }
  const closest = (point) => {
    let best = Infinity
    const triangle = new THREE.Triangle(),
      target = new THREE.Vector3()
    for (const [a, b, c] of triangles) {
      triangle.set(a, b, c).closestPointToPoint(point, target)
      best = Math.min(best, target.distanceTo(point))
    }
    return best
  }
  return (point) => {
    if (!box.containsPoint(point)) return 0
    if (!RAYS.every((ray) => crossings(point, ray) % 2)) return 0
    const depth = closest(point)
    return depth > TOLERANCE ? depth : 0
  }
}

const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule(
    '/src/lib/utils/modularCharacter.ts'
  )
  const { femaleOutfitMask } = await server.ssrLoadModule(
    '/src/lib/utils/female-outfit-mask.ts'
  )
  const base = await load(basePath)
  for (const data of JSON.parse(
    readFileSync('assets/modular_human_female_01/preview/animations.json')
  ).clips) {
    const clip = THREE.AnimationClip.parse(data)
    const index = base.animations.findIndex((entry) => entry.name === clip.name)
    if (index < 0) base.animations.push(clip)
    else base.animations[index] = clip
  }
  let body
  base.scene.traverse((node) => {
    if (node.isSkinnedMesh && base.parser.associations.get(node)?.primitives === 0)
      body = node
  })
  const skeleton = body.skeleton
  const bracers = bindModularPart(base.scene, (await load(partPath)).scene)
  assert.equal(bracers.length, 2)
  const sides = bracers.map((mesh) => {
    assert.equal(mesh.skeleton.bones.length, 65)
    for (let i = 0; i < 65; i++) {
      assert.equal(mesh.skeleton.bones[i], skeleton.bones[i])
      assert.deepEqual(
        mesh.skeleton.boneInverses[i].elements,
        skeleton.boneInverses[i].elements
      )
    }
    const side = mesh.name.endsWith('left') ? 'Left' : 'Right'
    const { skinIndex, skinWeight } = mesh.geometry.attributes
    const bone = skeleton.bones.findIndex((b) => b.name === side + 'ForeArm')
    for (let i = 0; i < skinIndex.count; i++) {
      assert.equal(skinIndex.getX(i), bone)
      assert.equal(skinWeight.getX(i), 1)
    }
    return { side, bone, inside: solid(mesh) }
  })
  const coverage = JSON.parse(readFileSync(`${directory}/body-occlusion.json`))
  const mask = femaleOutfitMask(base)
  const original = body.geometry
  mask.register('gloves', coverage)
  mask.setEquipped('gloves', true)
  assert.equal(
    body.geometry.index.count,
    original.index.count - coverage.hide_triangle_indices.length * 3
  )
  const visible = new Set(body.geometry.index.array)
  mask.setEquipped('gloves', false)
  assert.equal(body.geometry, original)
  mask.dispose()

  const probe = [...visible]
  const region = (i) => {
    let best = 0,
      name = ''
    for (let k = 0; k < 4; k++)
      if (original.attributes.skinWeight.getComponent(i, k) > best) {
        best = original.attributes.skinWeight.getComponent(i, k)
        name = skeleton.bones[original.attributes.skinIndex.getComponent(i, k)].name
      }
    return name
  }
  const sample = clipSampler(base.scene, { samples: 33, restore: skeleton.bones })
  const clips = []
  const point = new THREE.Vector3(),
    inverse = new THREE.Matrix4(),
    skinning = new THREE.Matrix4()
  const measure = (name) => {
    base.scene.updateMatrixWorld(true)
    skeleton.update()
    const local = sides.map(({ bone }) =>
      inverse.copy(
        skinning.multiplyMatrices(
          skeleton.bones[bone].matrixWorld,
          skeleton.boneInverses[bone]
        )
      ).invert().clone()
    )
    const hits = []
    for (const i of probe) {
      body.getVertexPosition(i, point)
      sides.forEach(({ side, inside }, s) => {
        const depth = inside(point.clone().applyMatrix4(local[s]))
        if (depth) hits.push({ vertex: i, bracer: side, bone: region(i), depth_m: depth })
      })
    }
    return hits
  }
  const summarize = (name, samples) => {
    const own = samples.flatMap(({ time, hits }) =>
      hits.filter((h) => h.bone.startsWith(h.bracer) && /Arm|Hand/.test(h.bone)).map((h) => ({ ...h, time }))
    )
    const other = samples.flatMap(({ time, hits }) =>
      hits.filter((h) => !(h.bone.startsWith(h.bracer) && /Arm|Hand/.test(h.bone))).map((h) => ({ ...h, time }))
    )
    const worst = (list) => list.reduce((a, b) => (b.depth_m > (a?.depth_m ?? 0) ? b : a), null)
    return {
      name,
      samples: samples.length,
      own_arm_penetrating_samples: new Set(own.map((h) => h.time)).size,
      own_arm_worst: worst(own),
      other_body_penetrating_samples: new Set(other.map((h) => h.time)).size,
      other_body_regions: [...new Set(other.map((h) => h.bone))],
      other_body_worst: worst(other),
    }
  }
  clips.push(summarize('rest', [{ time: 0, hits: measure('rest') }]))
  for (const clip of base.animations) {
    const samples = []
    for (const { time } of sample(clip)) samples.push({ time, hits: measure(clip.name) })
    clips.push(summarize(clip.name, samples))
  }
  const report = {
    base_sha256: hash(basePath),
    bracers_sha256: hash(partPath),
    occlusion_sha256: hash(`${directory}/body-occlusion.json`),
    probe_vertices: probe.length,
    tolerance_m: TOLERANCE,
    method:
      'Bind with the runtime bindModularPart, apply the body mask, then sample visible body vertices using two-ray parity in ForeArm rest space over 33 samples per preview clip. The source mesh has open boundaries; parity is diagnostic and cannot prove absence of intersections.',
    clips,
  }
  writeFileSync(`${directory}/animation-validation.json`, JSON.stringify(report, null, 2) + '\n')
  for (const clip of clips)
    console.log(
      clip.name,
      clip.own_arm_penetrating_samples,
      clip.own_arm_worst?.depth_m?.toFixed(4) ?? '-',
      clip.other_body_penetrating_samples,
      clip.other_body_regions.join(','),
      clip.other_body_worst?.depth_m?.toFixed(4) ?? '-'
    )
} finally {
  await server.close()
}

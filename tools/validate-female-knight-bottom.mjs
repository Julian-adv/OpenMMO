import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { AnimationClip, Vector3 } from '../client/node_modules/three/build/three.module.js'
import { clipSampler, hash, headlessThree } from './lib/headless-three.mjs'

const directory = 'assets/items/female_knight_bottom/reference_v1/fitted_v1'
const runtime = 'client/public/models/characters/modular_female'
const read = path => JSON.parse(readFileSync(path))
const { panels, harness_triangles } = read(`${directory}/fitting.json`)
const { server, load } = await headlessThree()
try {
  const { bindModularPart } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { femaleBootsClipName } = await server.ssrLoadModule('/src/lib/utils/femaleCharacter.ts')
  const { scene: body } = await load(`${runtime}/base.glb`)
  const { scene: part } = await load(`${runtime}/bottom.glb`)
  assert.deepEqual(part.userData.body_mask, read(`${directory}/body-occlusion.json`))
  const [mesh, ...other] = bindModularPart(body, part)
  assert.equal(other.length, 0)
  body.updateMatrixWorld(true)
  const { geometry, skeleton } = mesh
  const { skinWeight, skinIndex, position } = geometry.attributes
  assert.equal(skeleton.bones.length, 65)
  const indices = geometry.index
  const points = new Map(), edges = []
  let triangles = harness_triangles
  for (const { first_triangle, triangles: count, bone } of Object.values(panels)) {
    assert.equal(first_triangle, triangles)
    triangles += count
    for (let i = first_triangle * 3; i < triangles * 3; i += 3) {
      const corners = [0, 1, 2].map(k => indices.getX(i + k))
      for (const id of corners) {
        assert.equal(skinWeight.getX(id), 1)
        assert.equal(skeleton.bones[skinIndex.getX(id)].name, bone)
        if (!points.has(id)) {
          const rest = mesh.getVertexPosition(id, new Vector3())
          assert.ok(rest.distanceTo(new Vector3().fromBufferAttribute(position, id)) < 2e-6)
          points.set(id, { rest, posed: new Vector3() })
        }
      }
      for (let k = 0; k < 3; k++) {
        const a = points.get(corners[k]), b = points.get(corners[(k + 1) % 3])
        edges.push({ a, b, length: a.rest.distanceTo(b.rest) })
      }
    }
  }
  assert.equal(triangles * 3, indices.count)
  const sample = clipSampler(body, { samples: 61, restore: skeleton.bones })
  const clips = {}
  for (const [pack, names] of Object.entries({
    locomotion: ['idle1', 'walk', 'jog', 'run', 'jump'],
    social: ['stand_to_sit', 'sit_idle', 'sit_to_stand'],
    combat_melee: ['combat_idle', 'slash1', 'dying'],
  })) {
    const { animations } = await load(`${runtime}/animations/${pack}.glb`)
    for (const name of names) {
      const base = animations.find(c => c.name === name)
      const boots = animations.find(c => c.name === femaleBootsClipName(name, 'boots'))
      const overrides = new Map(boots.tracks.map(track => [track.name, track]))
      const clip = new AnimationClip(name, base.duration, base.tracks.map(track => overrides.get(track.name) ?? track))
      let maxError = 0, maxMotion = 0
      for (const _ of sample(clip)) {
        for (const [id, point] of points) {
          mesh.getVertexPosition(id, point.posed)
          maxMotion = Math.max(maxMotion, point.posed.distanceTo(point.rest))
        }
        for (const { a, b, length } of edges)
          maxError = Math.max(maxError, Math.abs(a.posed.distanceTo(b.posed) - length))
      }
      assert.ok(maxMotion > .001, name)
      assert.ok(maxError < 2e-6, `${name} plate edge error ${maxError} m`)
      clips[name] = maxError
    }
  }
  const report = { runtime_sha256: hash(`${runtime}/bottom.glb`), triangles, panels,
    samples_per_clip: 61, maximum_animated_edge_error_m: clips }
  writeFileSync(`${directory}/runtime-validation.json`, JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally {
  await server.close()
}

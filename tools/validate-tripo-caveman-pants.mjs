import assert from 'node:assert/strict'
import { writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree, loadClips, root } from './lib/headless-three.mjs'

const runtime = process.argv.includes('--runtime')
const { server, sources, load } = await headlessThree({ meshopt: true })
try {
  const { bindModularPart, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { updatePeltPhysics, resetPeltPhysics, disposePeltPhysics } = await server.ssrLoadModule('/src/lib/effects/pelt-rig.ts')
  const body = (await load(runtime ? 'client/public/models/characters/modular_male/base.glb' : 'assets/modular_human_male_01/fitted/base.glb')).scene
  const meshes = bindModularPart(body, (await load(runtime ? 'client/public/models/characters/modular_male/pants_caveman.glb' : 'assets/modular_human_male_01/caveman/tripo_pants_v1/pants_caveman.glb')).scene)
  assert.equal(meshes.length, 6)
  const records = meshes.map(mesh => {
    const geometry = mesh.geometry
    const rest = Array.from({ length: geometry.attributes.position.count }, (_, i) => new THREE.Vector3().fromBufferAttribute(geometry.attributes.position, i))
    const edges = new Map()
    for (let i = 0; i < geometry.index.count; i += 3) for (let j = 0; j < 3; j++) {
      const a = geometry.index.getX(i + j), b = geometry.index.getX(i + (j + 1) % 3)
      const length = rest[a].distanceTo(rest[b])
      if (length > .001) edges.set([a, b].sort((a, b) => a - b).join(','), { a, b, length })
    }
    for (let i = 0; i < rest.length; i++) for (let j = 0; j < 4; j++) {
      if (geometry.attributes.skinWeight.array[i * 4 + j] > 1e-6)
        assert.equal(mesh.skeleton.bones[geometry.attributes.skinIndex.array[i * 4 + j]].name, 'Hips')
    }
    const physics = mesh.userData.pelt_physics
    assert.ok(!physics?.bend, `${mesh.name}: expensive bend solver must not be used`)
    assert.equal(geometry.attributes.uv.count, rest.length)
    if (physics?.cloth) {
      assert.deepEqual(physics.cloth, { columns: 9, rows: 11, pinned_rows: 2 })
      assert.equal(rest.length, 198)
    }
    return { mesh, rest, edges: [...edges.values()], extension: 0, verticalExtension: 0, motion: 0 }
  })
  const mixer = new THREE.AnimationMixer(body)
  const clips = []
  for (const clip of await loadClips(body, load, modularAnimationClips)) {
    const name = clip.name
    mixer.stopAllAction()
    const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1)
    action.clampWhenFinished = true
    action.play()
    mixer.update(0)
    body.updateMatrixWorld(true)
    updatePeltPhysics(body, 0)
    resetPeltPhysics(body)
    const frames = Math.ceil(clip.duration * 60)
    let extension = 0
    for (let frame = 0; frame <= frames; frame++) {
      if (frame) {
        mixer.update(1 / 60)
        body.updateMatrixWorld(true)
        updatePeltPhysics(body, 1 / 60)
      }
      if (frame % Math.max(1, Math.floor(frames / 12)) && frame !== frames) continue
      for (const record of records) {
        const { mesh, rest, edges } = record
        const position = mesh.geometry.attributes.position
        const points = rest.map((_, i) => new THREE.Vector3().fromBufferAttribute(position, i))
        const cloth = mesh.userData.pelt_physics?.cloth
        if (cloth) {
          const count = cloth.columns * cloth.rows, pinned = cloth.columns * cloth.pinned_rows
          for (let i = 0; i < pinned; i++) for (const index of [i, i + count])
            assert.ok(points[index].distanceTo(rest[index]) < 1e-7, `${name}: ${mesh.name} pin moved`)
          for (let i = pinned; i < count; i++) {
            const a = i - cloth.columns
            const error = points[i].distanceTo(points[a]) / rest[i].distanceTo(rest[a]) - 1
            assert.ok(error <= .04002, `${name}: ${mesh.name} vertical extension ${error}`)
            record.verticalExtension = Math.max(record.verticalExtension, error)
            assert.ok(Math.abs(points[i].distanceTo(points[i + count]) - rest[i].distanceTo(rest[i + count])) < 1e-6, `${name}: cloth thickness changed`)
          }
        }
        for (let i = 0; i < points.length; i++) {
          assert.ok(points[i].toArray().every(Number.isFinite), `${name}: nonfinite point`)
          record.motion = Math.max(record.motion, points[i].distanceTo(rest[i]))
        }
        for (const { a, b, length } of edges) {
          const error = Math.max(0, points[a].distanceTo(points[b]) / length - 1)
          if (!cloth) assert.ok(Math.abs(points[a].distanceTo(points[b]) - length) < 1e-6, `${name}: ${mesh.name} rigid edge changed`)
          extension = Math.max(extension, error)
          record.extension = Math.max(record.extension, error)
        }
      }
    }
    clips.push({ clip: name, frames, maximum_edge_extension_relative: extension })
  }
  const panels = records.filter(r => r.mesh.userData.pelt_physics)
  assert.equal(panels.length, 4)
  assert.ok(panels.every(r => r.motion > .01))
  assert.equal(panels.filter(r => r.mesh.userData.pelt_physics.cloth).length, 2)
  disposePeltPhysics(body)
  for (const { mesh, rest } of records) for (let i = 0; i < rest.length; i++)
    assert.ok(new THREE.Vector3().fromBufferAttribute(mesh.geometry.attributes.position, i).equals(rest[i]))
  const report = { date: '2026-10-04', method: 'Actual barbarian runtime physics at 60 Hz, about 13 geometry samples per clip; rigid edges, side-grid pins, thickness and vertical length limits measured against original GLB positions', sources,
    mesh_count: meshes.length, panel_count: panels.length, all_skin_influences: 'Hips only', clips,
    limits: 'Rigid edge change under 1 micrometer; side-cloth vertical extension at most 4.002%, pinned rows fixed and original thickness preserved. Side-grid lateral/diagonal stretch is reported, not constrained to the retired bend solver limit.',
    cached_geometry_restored: true,
    meshes: records.map(r => ({ name: r.mesh.name, triangles: r.mesh.geometry.index.count / 3, maximum_edge_extension_relative: r.extension, maximum_vertical_extension_relative: r.mesh.userData.pelt_physics?.cloth ? r.verticalExtension : undefined, maximum_local_motion_m: r.motion })) }
  writeFileSync(new URL(`doc/assets/modular-caveman-tripo-pants-${runtime ? 'runtime-' : ''}animation-v2.json`, root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report.clips))
} finally {
  await server.close()
}

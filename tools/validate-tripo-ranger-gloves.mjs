import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { clipSampler, headlessThree, loadClips, root } from './lib/headless-three.mjs'

const directory = 'assets/modular_human_male_01/ranger/tripo_gloves_v1/'
const fittingPath = 'doc/assets/modular-ranger-tripo-gloves-fitting-v1.json'
const fitting = JSON.parse(readFileSync(new URL(fittingPath, root)))
const { server, sources, load } = await headlessThree()
try {
  const { bindModularPart, modularAnimationClips } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const { trimModularClothing } = await server.ssrLoadModule('/src/lib/utils/modularClothing.ts')
  const body = (await load('assets/modular_human_male_01/fitted/base.glb')).scene
  const meshes = bindModularPart(body, (await load(directory + 'gloves_ranger.glb')).scene)
  assert.equal(meshes.length, 2)
  body.updateMatrixWorld(true)
  const forearms = []
  body.traverse(mesh => {
    if (mesh.isSkinnedMesh && mesh.userData.region === 'forearms') {
      trimModularClothing(mesh, 'ranger_gloves')
      forearms.push(mesh)
    }
  })
  assert.ok(forearms.length)
  const records = meshes.map(mesh => {
    const side = mesh.name.endsWith('left') ? 'Left' : 'Right'
    const geometry = mesh.geometry
    const rest = Array.from({ length: geometry.attributes.position.count }, (_, i) => mesh.getVertexPosition(i, new THREE.Vector3()))
    const rigid = rest.map((_, i) => {
      let sum = 0
      let isRigid = true
      for (let k = 0; k < 4; k++) {
        const w = geometry.attributes.skinWeight.getComponent(i, k)
        const name = mesh.skeleton.bones[geometry.attributes.skinIndex.getComponent(i, k)].name
        sum += w
        if (w > 1e-6) {
          assert.ok(name.startsWith(side), `${mesh.name}: opposite-side bone`)
          if (name !== side + 'ForeArm') isRigid = false
        }
      }
      assert.ok(Math.abs(sum - 1) < 1e-6)
      return isRigid
    })
    for (const region of Object.values(fitting.finger_regions_by_side[side])) for (const index of region.vertices) {
      const allowed = region.allowed_bones.map(name => name.replace('Right', side))
      for (let k = 0; k < 4; k++) if (geometry.attributes.skinWeight.getComponent(index, k) > 1e-6)
        assert.ok(allowed.includes(mesh.skeleton.bones[geometry.attributes.skinIndex.getComponent(index, k)].name))
    }
    const edges = new Map(), groups = new Map()
    for (const [i, p] of rest.entries()) {
      const key = p.toArray().map(n => n.toFixed(6)).join(',')
      if (!groups.has(key)) groups.set(key, [])
      groups.get(key).push(i)
    }
    for (let i = 0; i < geometry.index.count; i += 3) for (let k = 0; k < 3; k++) {
      const a = geometry.index.getX(i + k), b = geometry.index.getX(i + (k + 1) % 3)
      const length = rest[a].distanceTo(rest[b])
      if (length > .001) edges.set([a, b].sort((a, b) => a - b).join(','), { a, b, length, rigid: rigid[a] && rigid[b] })
    }
    assert.ok(rigid.filter(Boolean).length > 100)
    const elbow = mesh.skeleton.bones.find(b => b.name === side + 'ForeArm')
    const wrist = mesh.skeleton.bones.find(b => b.name === side + 'Hand').getWorldPosition(new THREE.Vector3())
    const axis = wrist.clone().sub(elbow.getWorldPosition(new THREE.Vector3())).normalize()
    const radialX = new THREE.Vector3(0, 0, 1).cross(axis).normalize()
    const radialY = axis.clone().cross(radialX)
    const bins = Array.from({ length: 16 }, (_, bin) => {
      const center = bin * Math.PI / 8
      return Math.min(...rest.flatMap(point => {
        const relative = point.clone().sub(wrist)
        const angle = Math.atan2(relative.dot(radialY), relative.dot(radialX)) - center
        return Math.abs(Math.atan2(Math.sin(angle), Math.cos(angle))) < Math.PI / 8 ? [relative.dot(axis)] : []
      }))
    })
    assert.ok(bins.every(Number.isFinite))
    const cuffEnd = Math.max(...bins)
    const boundary = []
    for (const skin of forearms) {
      const candidates = []
      for (let i = 0; i < skin.geometry.attributes.position.count; i++) {
        const point = new THREE.Vector3().fromBufferAttribute(skin.geometry.attributes.position, i)
        if (side === 'Right' ? point.x < -.28 : point.x > .28)
          candidates.push({ skin, i, along: point.sub(wrist).dot(axis) })
      }
      const end = Math.max(...candidates.map(p => p.along))
      boundary.push(...candidates.filter(p => Math.abs(p.along - end) < 1e-4))
    }
    assert.ok(boundary.length > 10, side + ': missing skin cut boundary')
    return { mesh, rest, edges: [...edges.values()], seams: [...groups.values()].filter(g => g.length > 1),
      cuff: { elbow, wrist, axis, cuffEnd, boundary, inverseRest: elbow.matrixWorld.clone().invert(), minimumOverlap: Infinity },
      rigidVertices: rigid.filter(Boolean).length, stretch: 1, rigidError: 0, seamGap: 0, motion: 0 }
  })
  const skeleton = meshes[0].skeleton
  const sampler = clipSampler(body, { restore: skeleton.bones })
  const clips = [], snapshots = [], poses = []
  for (const clip of await loadClips(body, load, modularAnimationClips)) {
    const name = clip.name
    let stretch = 1, rigidError = 0, seamGap = 0
    for (const { sample, time } of sampler(clip)) {
      skeleton.update()
      for (const skin of forearms) skin.skeleton.update()
      for (const record of records) {
        const { cuff } = record
        const undoForearm = cuff.inverseRest.clone().invert().multiply(cuff.elbow.matrixWorld.clone().invert())
        for (const { skin, i } of cuff.boundary) {
          const point = skin.getVertexPosition(i, new THREE.Vector3()).applyMatrix4(skin.matrixWorld).applyMatrix4(undoForearm)
          const overlap = point.sub(cuff.wrist).dot(cuff.axis) - cuff.cuffEnd
          cuff.minimumOverlap = Math.min(cuff.minimumOverlap, overlap)
          assert.ok(overlap > .015, `${name}: ${record.mesh.name} skin ends before cuff overlap (${overlap})`)
        }
        const points = record.rest.map((_, i) => record.mesh.getVertexPosition(i, new THREE.Vector3()))
        for (let i = 0; i < points.length; i++) {
          assert.ok(points[i].toArray().every(Number.isFinite), `${name}: nonfinite vertex`)
          record.motion = Math.max(record.motion, points[i].distanceTo(record.rest[i]))
        }
        for (const { a, b, length, rigid } of record.edges) {
          const ratio = points[a].distanceTo(points[b]) / length
          stretch = Math.max(stretch, ratio)
          record.stretch = Math.max(record.stretch, ratio)
          if (rigid) {
            const error = Math.abs(ratio - 1)
            assert.ok(error < 1e-4, `${name}: rigid bracer changed length`)
            rigidError = Math.max(rigidError, error)
            record.rigidError = Math.max(record.rigidError, error)
          }
        }
        for (const group of record.seams) for (const i of group.slice(1)) {
          const gap = points[group[0]].distanceTo(points[i])
          seamGap = Math.max(seamGap, gap)
          record.seamGap = Math.max(record.seamGap, gap)
          assert.ok(gap < 1e-5, `${name}: split UV seam`)
        }
      }
      const matrices = skeleton.bones.map((bone, i) => new THREE.Matrix4().multiplyMatrices(bone.matrixWorld, skeleton.boneInverses[i]).toArray())
      poses.push({ clip: name, time, matrices })
      if (sample === 10) snapshots.push({ clip: name, time, bone_deformation_matrices: Object.fromEntries(skeleton.bones.map((bone, i) => [bone.name, matrices[i]])) })
    }
    clips.push({ clip: name, samples: 25, maximum_edge_stretch_ratio: stretch, maximum_rigid_bracer_length_error_relative: rigidError, maximum_seam_gap_m: seamGap })
  }
  assert.ok(records.every(r => r.motion > .1), 'Gloves did not animate')
  const report = { date: '2026-10-07', sources, exact_rig_binding: true, normalized_weights: true,
    opposite_side_influences: false, finger_bone_isolation: true,
    triangles: meshes.reduce((sum, mesh) => sum + mesh.geometry.index.count / 3, 0), clips,
    meshes: records.map(r => ({ name: r.mesh.name, rigid_bracer_vertices: r.rigidVertices,
      maximum_edge_stretch_ratio: r.stretch, maximum_rigid_bracer_length_error_relative: r.rigidError,
      maximum_seam_gap_m: r.seamGap, maximum_animated_motion_m: r.motion })),
    cuff_overlap: records.map(r => ({ name: r.mesh.name, boundary_vertices: r.cuff.boundary.length, angular_windows: 16,
      shortest_cuff_proximal_distance_from_wrist_m: -r.cuff.cuffEnd,
      minimum_skin_overlap_m: r.cuff.minimumOverlap })),
    scope: 'Actual modular binding, seven clips and 25 samples per clip. Cuff overlap uses 16 overlapping angular vertex windows and the clipped skin boundary, measured in the moving ForeArm frame. Forearm protection is rigid; wrist and hand deform. This does not certify all equipment combinations, every animation frame or all body intersections.' }
  writeFileSync(new URL('doc/assets/modular-ranger-tripo-gloves-animation-v1.json', root), JSON.stringify(report, null, 2) + '\n')
  writeFileSync(new URL(directory + 'animation-snapshots.json', root), JSON.stringify(snapshots) + '\n')
  writeFileSync(new URL(directory + 'validation-poses.json', root), JSON.stringify(poses) + '\n')
  console.log(JSON.stringify(clips))
} finally { await server.close() }

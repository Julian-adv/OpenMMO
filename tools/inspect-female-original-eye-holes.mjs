import { readFile, writeFile } from 'node:fs/promises'
import assert from 'node:assert/strict'
import sharp from 'sharp'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { headlessThree } from './lib/headless-three.mjs'

const directory = 'assets/modular_human_female_01/faces/tripo_custom_v1/'
const fitting = JSON.parse(await readFile(directory + 'fitting.json', 'utf8'))
const rgba = await sharp(directory + 'lash_skin_alpha_v1/original-lash-skin-alpha.png').ensureAlpha().raw().toBuffer({ resolveWithObject: true })
const key = (geometry, triangle, source = false) => {
  const { index, attributes: { position, uv } } = geometry
  return [0, 1, 2].map(corner => {
    const i = index.getX(triangle * 3 + corner)
    const p = [position.getX(i), position.getY(i), position.getZ(i)]
    if (source) for (let axis = 0; axis < 3; axis++) p[axis] = Math.fround(p[axis] * fitting.scale + fitting.offset_m[axis])
    return [...p, uv.getX(i), uv.getY(i)].join(',')
  }).sort().join('|')
}
function alpha(uv) {
  const { width, height } = rgba.info
  const x = uv.x * width - .5, y = uv.y * height - .5, i = Math.floor(x), j = Math.floor(y), a = x - i, b = y - j
  const value = (xx, yy) => rgba.data[(Math.max(0, Math.min(height - 1, yy)) * width + Math.max(0, Math.min(width - 1, xx))) * 4 + 3]
  return (value(i, j) * (1 - a) + value(i + 1, j) * a) * (1 - b) + (value(i, j + 1) * (1 - a) + value(i + 1, j + 1) * a) * b
}
function interp(x, xs, ys) {
  let i = 0
  while (i < xs.length - 2 && x > xs[i + 1]) i++
  const t = Math.max(0, Math.min(1, (x - xs[i]) / (xs[i + 1] - xs[i])))
  return ys[i] * (1 - t) + ys[i + 1] * t
}
function interior(p) {
  const x = Math.abs(p.x + .0008), xs = [.016, .020, .026, .033, .040, .047, .052]
  const top = interp(x, xs, [1.6819, 1.6860, 1.6891, 1.6894, 1.6878, 1.6854, 1.6835])
  let bottom = interp(x, xs, [1.6819, 1.6804, 1.6795, 1.6796, 1.6806, 1.6826, 1.6835])
  if (fitting.lower_eyelid_fit?.ridge_x_knots_m) {
    const t = value => { const q = Math.max(0, Math.min(1, value)); return q * q * (3 - 2 * q) }
    const ends = t((x - .016) / .005) * (1 - t((x - .042) / .007))
    bottom = interp(x, fitting.lower_eyelid_fit.ridge_x_knots_m, fitting.lower_eyelid_fit.ridge_y_knots_m) - fitting.lower_eyelid_fit.drop_mm / 1000 * ends
  }
  return x > .0165 && x < .0515 && p.y > bottom + .0002 && p.y < top - .0002 && p.z > .051
}
const { server, load } = await headlessThree()
try {
  const source = await load(directory + 'source.glb'), sourceKeys = new Map()
  source.scene.traverse(mesh => {
    if (!mesh.isMesh) return
    for (let i = 0; i < mesh.geometry.index.count / 3; i++) sourceKeys.set(key(mesh.geometry, i, true), i)
  })
  const { scene } = await load(directory + 'base_custom.glb'), meshes = []
  const mapping = fitting.lower_eyelid_fit ? (await load(directory + fitting.lower_eyelid_fit.baseline_path)).scene : scene
  scene.traverse(original => {
    if (!original.isSkinnedMesh || original.name === 'female_body') return
    const mappingGeometry = mapping.getObjectByName(original.name).geometry
    const geometry = original.geometry.clone(), p = geometry.attributes.position, ids = geometry.index.array, indices = [], sourceIds = []
    for (let j = 0; j < ids.length; j += 3) {
      const points = [0, 1, 2].map(k => new THREE.Vector3().fromBufferAttribute(p, ids[j + k]))
      if (Math.max(...points.map(q => q.y)) < 1.665 || Math.min(...points.map(q => q.y)) > 1.696 || Math.max(...points.map(q => q.z)) < .035) continue
      indices.push(...ids.slice(j, j + 3))
      sourceIds.push(sourceKeys.get(key(mappingGeometry, j / 3)))
    }
    geometry.setIndex(indices)
    geometry.computeBoundingSphere()
    const mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial({ side: THREE.DoubleSide }))
    mesh.name = original.name
    mesh.userData.sourceIds = sourceIds
    mesh.updateMatrixWorld(true)
    meshes.push(mesh)
  })
  const protectedFaces = fitting.protected_eye_source_faces ?? []
  const opaqueFaces = new Set(meshes.filter(mesh => mesh.name !== 'eyelashes_female_custom').flatMap(mesh => mesh.userData.sourceIds))
  const alphaFaces = new Set(meshes.filter(mesh => mesh.name === 'eyelashes_female_custom').flatMap(mesh => mesh.userData.sourceIds))
  for (const id of protectedFaces) {
    assert.ok(opaqueFaces.has(id), `Eye surface ${id} must be opaque even when another surface is behind it`)
    assert.ok(!alphaFaces.has(id), `Eye surface ${id} must not use eyelash alpha`)
  }
  const views = [
    ['front', [1184, 420], [.001, 1.6821, .813], [.001, 1.6821, 0], 3.6, [60, 110, 1120, 310]],
    ['angle', [900, 900], [.085, 1.695, .155], [.035, 1.685, .064], 35, [80, 240, 840, 680]],
    ['other-angle', [900, 900], [-.0866, 1.695, .155], [-.0366, 1.685, .064], 35, [80, 240, 840, 680]],
    ['profile', [900, 900], [.45, 1.682, .06], [.033, 1.682, .06], 12, [220, 330, 650, 600]],
    ['other-profile', [900, 900], [-.4516, 1.682, .06], [-.0346, 1.682, .06], 12, [220, 330, 650, 600]],
  ]
  const ray = new THREE.Raycaster(), counts = new Map(), records = []
  for (const [name, size, position, target, fov, box] of views) {
    const camera = new THREE.PerspectiveCamera(fov, size[0] / size[1], .01, 30)
    camera.position.fromArray(position)
    camera.lookAt(...target)
    camera.updateMatrixWorld(true)
    let holes = 0
    for (let y = box[1]; y < box[3]; y += 3) for (let x = box[0]; x < box[2]; x += 3) {
      ray.setFromCamera(new THREE.Vector2(x / size[0] * 2 - 1, 1 - y / size[1] * 2), camera)
      const hits = ray.intersectObjects(meshes, false), first = hits[0]
      if (!first || first.object.name !== 'eyelashes_female_custom' || !interior(first.point) || alpha(first.uv1) >= 127.5) continue
      const solid = hits.find(hit => hit.object.name !== 'eyelashes_female_custom' || alpha(hit.uv1) >= 127.5)
      if (solid && solid.distance - first.distance < .012) continue
      const id = first.object.userData.sourceIds[first.faceIndex]
      if (id === undefined) throw new Error('Missing source triangle mapping')
      counts.set(id, (counts.get(id) ?? 0) + 1)
      holes++
    }
    records.push({ view: name, sampled_hole_pixels: holes })
  }
  const report = { source_faces: [...counts.keys()].sort((a, b) => a - b), hits_by_source_face: [...counts.entries()], views: records,
    protected_eye_faces_opaque: protectedFaces,
    scope: 'Protected original eye and inner-corner faces checked for opaque material membership regardless of backing depth. Three-pixel ray grid within the native eye opening detects loss without opaque backing within 12mm; it does not detect every eye surface defect.' }
  await writeFile(directory + 'lash_skin_alpha_v1/eye-hole-diagnostic.json', JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(report))
} finally {
  await server.close()
}

import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { fileURLToPath } from 'node:url'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFLoader } from '../client/node_modules/three/examples/jsm/loaders/GLTFLoader.js'
import { createServer } from '../client/node_modules/vite/dist/node/index.js'

const root = new URL('../', import.meta.url)
const server = await createServer({ root: fileURLToPath(new URL('client/', root)), configFile: false,
  optimizeDeps: { noDiscovery: true, include: [] }, server: { middlewareMode: true, watch: null }, appType: 'custom' })
globalThis.self = globalThis
const loader = new GLTFLoader().register(() => ({ name: 'headless-materials', loadMaterial: async () => new THREE.MeshBasicMaterial() }))
const sources = []
async function load(path) {
  const data = readFileSync(new URL(path, root))
  sources.push({ path, sha256: createHash('sha256').update(data).digest('hex') })
  return (await loader.parseAsync(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), '')).scene
}
try {
  const { bindModularPart, skinnedParts, showModularOutfit, RANGER_MODULAR_OUTFIT } = await server.ssrLoadModule('/src/lib/utils/modularCharacter.ts')
  const body = await load('assets/modular_human_male_01/parts/fitted/base.glb')
  const bodyMeshes = skinnedParts(body)
  const gloves = bindModularPart(body, await load('assets/modular_human_male_01/parts/ranger_tripo_gloves_v1/gloves_ranger.glb'))
  const cuff = JSON.parse(readFileSync(new URL('client/src/lib/data/rangerGloveCuff.json', root)))
  assert.equal(sources.find(s => s.path === cuff.source)?.sha256, cuff.sha256)
  const parts = new Map([['gloves_ranger', gloves]])
  for (const top of ['plate', 'linen', 'leather']) parts.set('top_' + top, bindModularPart(body, await load('assets/modular_human_male_01/parts/fitted/top_' + top + '.glb')))
  body.updateMatrixWorld(true)
  const skeleton = gloves[0].skeleton
  const frames = ['Right', 'Left'].map(side => {
    const wrist = skeleton.bones.find(b => b.name === side + 'Hand').getWorldPosition(new THREE.Vector3())
    const forearm = skeleton.bones.findIndex(b => b.name === side + 'ForeArm')
    const axis = wrist.clone().sub(skeleton.bones[forearm].getWorldPosition(new THREE.Vector3())).normalize()
    const mesh = gloves.find(m => m.name.endsWith(side.toLowerCase()))
    const p = mesh.geometry.attributes.position, index = mesh.geometry.index
    const triangles = []
    for (let i = 0; i < index.count; i += 3) triangles.push([0, 1, 2].map(k => new THREE.Vector3().fromBufferAttribute(p, index.getX(i + k))))
    return { side, wrist, forearm, axis, triangles }
  })
  const posePath = 'assets/modular_human_male_01/parts/ranger_tripo_gloves_v1/validation-poses.json'
  const data = readFileSync(new URL(posePath, root))
  sources.push({ path: posePath, sha256: createHash('sha256').update(data).digest('hex') })
  const poses = JSON.parse(data).map(p => ({ ...p, matrices: p.matrices.map(v => new THREE.Matrix4().fromArray(v)) }))
  const records = []
  const ray = new THREE.Ray(), hit = new THREE.Vector3()
  for (const top of ['plate', 'linen', 'leather']) {
    showModularOutfit(bodyMeshes, parts, { ...RANGER_MODULAR_OUTFIT, top })
    assert.ok(bodyMeshes.filter(m => m.userData.region === 'forearms').every(m => !m.visible))
    const sleeves = parts.get(top === 'plate' ? 'top_plate' : 'top_linen').filter(m => m.visible && (top === 'plate' || m.userData.region === 'sleeves'))
    assert.ok(sleeves.length)
    for (const frame of frames) {
      const samples = []
      for (const mesh of sleeves) {
        const { position, skinIndex, skinWeight } = mesh.geometry.attributes
        const addSample = (indices, factors) => {
          const p = new THREE.Vector3(), contributors = []
          for (const [offset, i] of indices.entries()) {
            const vertex = new THREE.Vector3().fromBufferAttribute(position, i)
            p.addScaledVector(vertex, factors[offset])
            contributors.push({ p: vertex, factor: factors[offset],
              joints: Array.from({ length: 4 }, (_, k) => skinIndex.getComponent(i, k)),
              weights: Array.from({ length: 4 }, (_, k) => skinWeight.getComponent(i, k)) })
          }
          const along = p.clone().sub(frame.wrist).dot(frame.axis)
          if ((frame.side === 'Right' ? p.x < -.28 : p.x > .28) && along >= -.207 && along <= -.17)
            samples.push({ contributors })
        }
        for (let i = 0; i < position.count; i++) addSample([i], [1])
        const index = mesh.geometry.index
        for (let i = 0; i < index.count; i += 3) for (const factors of [[.6, .2, .2], [.2, .6, .2], [.2, .2, .6]])
          addSample([0, 1, 2].map(k => index.getX(i + k)), factors)
      }
      assert.ok(samples.length > 10, `${top} ${frame.side}: no overlap vertices`)
      let clearance = Infinity
      for (const pose of poses) for (const sample of samples) {
        const p = new THREE.Vector3()
        for (const vertex of sample.contributors) for (let k = 0; k < 4; k++) if (vertex.weights[k])
          p.addScaledVector(vertex.p.clone().applyMatrix4(pose.matrices[vertex.joints[k]]), vertex.weights[k] * vertex.factor)
        p.applyMatrix4(pose.matrices[frame.forearm].clone().invert())
        const along = p.clone().sub(frame.wrist).dot(frame.axis)
        ray.origin.copy(frame.wrist).addScaledVector(frame.axis, along)
        ray.direction.copy(p).sub(ray.origin).normalize()
        let outer = -Infinity
        for (const tri of frame.triangles) if (ray.intersectTriangle(...tri, false, hit)) outer = Math.max(outer, hit.distanceTo(ray.origin))
        assert.ok(Number.isFinite(outer), `${top} ${frame.side}: open glove section`)
        const margin = outer - p.distanceTo(ray.origin)
        clearance = Math.min(clearance, margin)
        assert.ok(margin > .0005, `${top} ${frame.side} ${pose.clip}: sleeve protrudes (${margin})`)
      }
      records.push({ top, side: frame.side, sampled_vertices_and_triangle_points: samples.length, poses: poses.length, minimum_outer_surface_clearance_m: clearance })
    }
  }
  const runtime_sources = ['client/src/lib/utils/modularCharacter.ts', 'client/src/lib/utils/modularClothing.ts',
    'client/src/lib/utils/rangerGloveCuff.ts', 'client/src/lib/data/rangerGloveCuff.json'].map(path =>
      ({ path, sha256: createHash('sha256').update(readFileSync(new URL(path, root))).digest('hex') }))
  const report = { date: '2026-10-07', sources, runtime_sources, records, scope: 'Actual clipped and tucked sleeve vertices and three barycentric points per triangle in the cuff overlap, checked radially against the fitted glove outer surface in the moving ForeArm frame over 175 stored poses. Does not certify every surface point, all sleeve regions or every animation frame.' }
  writeFileSync(new URL('doc/assets/modular-ranger-tripo-gloves-sleeve-coverage-v3.json', root), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify(records))
} finally { await server.close() }

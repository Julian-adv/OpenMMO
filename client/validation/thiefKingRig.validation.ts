import { test, expect } from 'vitest'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'

const root = new URL('../../', import.meta.url)
const read = (path: string) => readFileSync(new URL(path, root))
const load = async (path: string) => {
  const data = read(path)
  const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder)
    .register(() => ({ name: 'geometry-only', loadMaterial: async () => new THREE.MeshBasicMaterial() }))
  return loader.parseAsync(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength), '')
}

test('baked monster animation and ground sampling', async () => {
  const target = await load('client/public/models/monsters/thief_king.glb')
  const raw = await load('assets/monsters/thief_king/rig_v1/thief_king.glb')
  const meshes: THREE.SkinnedMesh[] = []
  target.scene.traverse(o => { if (o instanceof THREE.SkinnedMesh) meshes.push(o) })
  expect(meshes.length).toBe(1)
  const mesh = meshes[0], geo = mesh.geometry, pos = geo.getAttribute('position'), skinWeight = geo.getAttribute('skinWeight')
  const rawMesh = raw.scene.getObjectByProperty('type', 'SkinnedMesh') as THREE.SkinnedMesh
  expect(pos.count).toBe(15723)
  expect(mesh.skeleton.bones.length).toBe(65)
  expect(mesh.skeleton.bones.every(b => !b.name.includes(':'))).toBe(true)
  for (const name of ['Hips', 'Head', 'LeftHand', 'RightHand', 'RightHandThumb1', 'RightHandPinky4']) expect(mesh.skeleton.getBoneByName(name)).toBeDefined()
  target.scene.traverse(o => { expect(o.scale.distanceTo(new THREE.Vector3(1, 1, 1))).toBeLessThan(1e-6) })
  expect(geo.index!.count / 3).toBe(10823)
  let error = 0, difference = 0
  for (let i = 0; i < pos.count; i++) {
    const sum = skinWeight.getX(i) + skinWeight.getY(i) + skinWeight.getZ(i) + skinWeight.getW(i)
    error = Math.max(error, Math.abs(sum - 1))
    difference = Math.max(difference, new THREE.Vector3().fromBufferAttribute(pos, i).distanceTo(new THREE.Vector3().fromBufferAttribute(rawMesh.geometry.getAttribute('position'), i)))
  }
  expect(error).toBeLessThan(1e-6)
  target.scene.updateMatrixWorld(true)
  const bindBox = new THREE.Box3()
  let bindDrift = 0
  for (let i = 0; i < pos.count; i++) {
    const original = new THREE.Vector3().fromBufferAttribute(pos, i).applyMatrix4(mesh.matrixWorld)
    const skinned = mesh.getVertexPosition(i, new THREE.Vector3()).applyMatrix4(mesh.matrixWorld)
    bindDrift = Math.max(bindDrift, original.distanceTo(skinned))
    bindBox.expandByPoint(skinned)
  }
  expect(bindDrift).toBeLessThan(1e-5)
  expect(bindBox.min.y).toBeCloseTo(0, 5)
  expect(bindBox.max.y).toBeCloseTo(2.1, 5)
  expect(difference).toBeLessThan(1e-6)
  const names = ['idle1', 'walk', 'run', 'slash1', 'combat_idle', 'dying']
  const clips = target.animations
  expect(clips.map(c => c.name).sort()).toEqual([...names].sort())
  const fit = JSON.parse(readFileSync(new URL('assets/monsters/thief_king/grip_v1/fit.json', root), 'utf8'))
  for (const clip of clips) {
    for (const [finger, angles] of Object.entries(fit.fingers_degrees_x) as [string, number[]][]) {
      for (let i = 1; i <= 3; i++) {
        const name = `RightHand${finger}${i}`
        const bone = rawMesh.skeleton.getBoneByName(name)!
        const expected = bone.quaternion.clone().multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), THREE.MathUtils.degToRad(angles[i - 1])))
        if (finger === 'Thumb') {
          expected.multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), THREE.MathUtils.degToRad(fit.thumb_degrees_y[i - 1])))
          expected.multiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 0, 1), THREE.MathUtils.degToRad(fit.thumb_degrees_z[i - 1])))
        }
        const track = clip.tracks.find(t => t.name === `${name}.quaternion`)
        expect(track).toBeDefined()
        for (let k = 0; k < track!.values.length; k += 4) {
          const q = new THREE.Quaternion().fromArray(track!.values, k)
          expect(1 - Math.abs(q.dot(expected))).toBeLessThan(1e-6)
        }
      }
    }
  }
  const mixer = new THREE.AnimationMixer(target.scene)
  const dir = '/tmp/thief-king-validation'
  mkdirSync(dir, { recursive: true })
  type Frame = { fraction: number; time: number; minimum_y_m: number; bounds: number[][]; hand_matrix: number[] }
  const report = { clips: [] as Record<string, unknown>[], model: 'client/public/models/monsters/thief_king.glb', joints: mesh.skeleton.bones.length, triangles: geo.index!.count / 3, vertices: pos.count, max_weight_sum_error: error, max_bind_drift_m: bindDrift }
  const vertex = new THREE.Vector3()
  const skinIndex = geo.getAttribute('skinIndex')
  const footBones = new Set(mesh.skeleton.bones.map((b, i) => /foot|toe/i.test(b.name) ? i : -1))
  const sole: number[] = []
  for (let i = 0; i < pos.count; i++) {
    let influence = 0
    for (let k = 0; k < 4; k++) if (footBones.has(skinIndex.getComponent(i, k))) influence += skinWeight.getComponent(i, k)
    if (influence > .5) sole.push(i)
  }
  const fingerBones = new Set(mesh.skeleton.bones.map((b, i) => /^RightHand(?:$|Thumb|Index|Middle|Ring|Pinky)/.test(b.name) ? i : -1))
  const fingerWeights = Array.from({ length: pos.count }, (_, i) => {
    let sum = 0
    for (let k = 0; k < 4; k++) if (fingerBones.has(skinIndex.getComponent(i, k))) sum += skinWeight.getComponent(i, k)
    return sum
  })
  const sourceIndex = geo.index!
  const fingerFaces: number[] = [], thumbFaces: number[] = [], oppositeFaces: number[] = []
  const influence = (i: number, pattern: RegExp) => {
    let sum = 0
    for (let k = 0; k < 4; k++) if (pattern.test(mesh.skeleton.bones[skinIndex.getComponent(i, k)]?.name ?? '')) sum += skinWeight.getComponent(i, k)
    return sum
  }
  const thumbWeights = Array.from({ length: pos.count }, (_, i) => influence(i, /^RightHandThumb/))
  const oppositeWeights = Array.from({ length: pos.count }, (_, i) => influence(i, /^RightHand(Index|Middle)/))
  for (let i = 0; i < sourceIndex.count; i += 3) {
    const face = [sourceIndex.getX(i), sourceIndex.getX(i + 1), sourceIndex.getX(i + 2)]
    if (face.some(id => fingerWeights[id] > .05)) fingerFaces.push(...face)
    if (face.every(id => thumbWeights[id] > .6)) thumbFaces.push(...face)
    if (face.every(id => oppositeWeights[id] > .6)) oppositeFaces.push(...face)
  }
  const fingerIds = [...new Set(fingerFaces)]
  const map = new Map(fingerIds.map((id, i) => [id, i]))
  mkdirSync(`${dir}/grip`, { recursive: true })
  writeFileSync(`${dir}/grip/mesh.json`, JSON.stringify({ ids: fingerIds, faces: fingerFaces.map(id => map.get(id)), thumb_faces: thumbFaces.map(id => map.get(id)), opposing_faces: oppositeFaces.map(id => map.get(id)) }))
  for (const clip of clips) {
    expect(clip.validate()).toBe(true)
    const action = mixer.clipAction(clip).setLoop(THREE.LoopOnce, 1)
    action.clampWhenFinished = true
    action.play()
    const frames: Frame[] = [], gripFrames: { hand_matrix: number[]; vertices: number[] }[] = []
    for (let k = 0; k <= 48; k++) {
      const time = clip.duration * k / 48
      mixer.setTime(Math.min(time, clip.duration - 1e-5))
      target.scene.updateMatrixWorld(true); mesh.skeleton.update()
      const values: number[] = [], box = new THREE.Box3()
      for (let i = 0; i < pos.count; i++) {
        mesh.getVertexPosition(i, vertex).applyMatrix4(mesh.matrixWorld)
        expect(Number.isFinite(vertex.x + vertex.y + vertex.z)).toBe(true)
        box.expandByPoint(vertex)
        values.push(vertex.x, vertex.y, vertex.z)
      }
      const hand = target.scene.getObjectByName('RightHand')!
      const frame = { fraction: k / 48, time, minimum_y_m: box.min.y, bounds: [box.min.toArray(), box.max.toArray()], hand_matrix: hand.matrixWorld.toArray() }
      frames.push(frame)
      gripFrames.push({ hand_matrix: frame.hand_matrix, vertices: fingerIds.flatMap(id => values.slice(id * 3, id * 3 + 3)) })
      if (k % 6 === 0) writeFileSync(`${dir}/${clip.name}-${k / 6}.json`, JSON.stringify({ ...frame, vertices: values }))
    }
    writeFileSync(`${dir}/grip/${clip.name}.json`, JSON.stringify(gripFrames))
    const heights: number[] = []
    const steps = Math.ceil(clip.duration * 120)
    for (let i = 0; i <= steps; i++) {
      mixer.setTime(Math.min(clip.duration - 1e-5, clip.duration * i / steps))
      target.scene.updateMatrixWorld(true); mesh.skeleton.update()
      let lowest = Infinity
      for (const id of sole) { mesh.getVertexPosition(id, vertex).applyMatrix4(mesh.matrixWorld); lowest = Math.min(lowest, vertex.y) }
      heights.push(lowest)
    }
    if (['idle1', 'walk', 'combat_idle'].includes(clip.name)) {
      expect(Math.min(...heights)).toBeGreaterThan(-.02)
      expect(Math.max(...heights)).toBeLessThan(.02)
    }
    if (clip.name === 'run') expect(Math.min(...heights)).toBeGreaterThan(-.02)
    if (clip.name === 'dying') expect(Math.abs(frames.at(-1)!.minimum_y_m)).toBeLessThan(.02)
    report.clips.push({ name: clip.name, duration: clip.duration, frames, sole_height_range_m: [Math.min(...heights), Math.max(...heights)], sole_samples: heights.length })
    action.stop(); mixer.uncacheClip(clip)
  }
  writeFileSync(`${dir}/validation.json`, JSON.stringify(report, null, 2))
}, 180000)

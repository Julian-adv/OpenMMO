import assert from 'node:assert/strict'
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { MeshBVH } from '../client/node_modules/three-mesh-bvh/src/index.js'
import { headlessThree, root } from './lib/headless-three.mjs'
import { monsterRow, monsterPacks, packClipNames, gripRotations } from './lib/humanoid-grip.mjs'

const id = process.argv[2]; assert.match(id ?? '', /^[a-z0-9_]+$/)
const samples = 49
const row = monsterRow(id)
const fit = JSON.parse(readFileSync(new URL(`assets/monsters/${id}/grip_v1/fit.json`, root)))
const spec = JSON.parse(readFileSync(new URL(`assets/monsters/${id}/rig_spec.json`, root)))
const out = `/tmp/${id}-review`; mkdirSync(out, { recursive: true })
const { server, load } = await headlessThree()
try {
  const runtime = await server.ssrLoadModule('/src/lib/utils/characterAnimationUtils.ts')
  const { splitClipNames } = await server.ssrLoadModule('/src/lib/data/monsterDefs.ts')
  const { seatMonsterWeapon } = await server.ssrLoadModule('/src/lib/utils/monsterWeapon.ts')
  const { CHARACTER_ANIMATION_PACK_PATHS } = await server.ssrLoadModule('/src/lib/utils/modelPaths.ts')
  const isClip = (key, name) => splitClipNames(row[key]).includes(name)
  const target = await load(`client/public/models/monsters/${id}.glb`), raw = await load(`assets/monsters/${id}/rig_v1/${id}.glb`)
  const mesh = runtime.findPrimarySkinnedMesh(target.scene), rest = runtime.findPrimarySkinnedMesh(raw.scene)
  assert.ok(mesh && rest)
  const prop = new THREE.Object3D(); seatMonsterWeapon(mesh.skeleton.getBoneByName('RightHand'), prop, row); prop.updateMatrix()
  const attach = { matrix: prop.matrix.clone(), offset: prop.position.toArray(), rotation: splitClipNames(row.weaponRotation).map(Number) }
  const geo = mesh.geometry, position = geo.getAttribute('position'), weights = geo.getAttribute('skinWeight'), joints = geo.getAttribute('skinIndex')
  assert.equal(mesh.skeleton.bones.length, rest.skeleton.bones.length)
  assert.ok(mesh.skeleton.bones.every(b => !b.name.includes(':')))
  target.scene.traverse(o => assert.ok(o.scale.distanceTo(new THREE.Vector3(1, 1, 1)) < 1e-6))
  let maxError = 0, unweighted = 0, maxBindDifference = 0
  const rawPosition = rest.geometry.getAttribute('position')
  assert.equal(position.count, rawPosition.count)
  assert.equal(geo.index.count, rest.geometry.index.count)
  for (let i = 0; i < position.count; i++) {
    let sum = 0; for (let k = 0; k < 4; k++) sum += weights.getComponent(i, k)
    if (sum === 0) unweighted++; maxError = Math.max(maxError, Math.abs(1 - sum))
    maxBindDifference = Math.max(maxBindDifference, new THREE.Vector3().fromBufferAttribute(position, i).distanceTo(new THREE.Vector3().fromBufferAttribute(rawPosition, i)))
  }
  assert.equal(unweighted, 0); assert.ok(maxError < 1e-6); assert.ok(maxBindDifference < 1e-6)
  const box = new THREE.Box3().setFromObject(mesh); assert.ok(Math.abs(box.min.y) < 1e-5 && Math.abs(box.max.y - spec.height_m) < 1e-5)
  const grip = gripRotations(rest, fit, await load(fit.reference_model))
  const { paths, attack } = monsterPacks(row, CHARACTER_ANIMATION_PACK_PATHS)
  const packs = await Promise.all(paths.map(p => load(`client/public${p}`)))
  const names = packClipNames(row, packs, splitClipNames)
  assert.deepEqual(target.animations.map(c => c.name).sort(), names.sort())
  for (const clip of target.animations) {
    assert.ok(clip.validate())
    const original = packs.flatMap(pack => pack.animations).find(c => c.name === clip.name)
    assert.ok(original && Math.abs(clip.duration - (clip.name === attack?.clip ? attack.end - attack.impact + attack.landing : original.duration)) < 1e-4, `${clip.name} duration`)
    for (const [name, q] of grip) {
      const track = clip.tracks.find(t => t.name === `${name}.quaternion`); assert.ok(track, `${clip.name} ${name}`)
      for (let i = 0; i < track.values.length; i += 4) assert.ok(1 - Math.abs(new THREE.Quaternion().fromArray(track.values, i).dot(q)) < 1e-6)
    }
  }
  const influence = (i, re) => { let value = 0; for (let k = 0; k < 4; k++) if (re.test(mesh.skeleton.bones[joints.getComponent(i, k)]?.name ?? '')) value += weights.getComponent(i, k); return value }
  const sole = [], handWeights = [], thumbWeights = [], oppositeWeights = []
  for (let i = 0; i < position.count; i++) {
    if (influence(i, /foot|toe/i) > .5) sole.push(i)
    handWeights.push(influence(i, /^RightHand(?:$|Thumb|Index|Middle|Ring|Pinky)/))
    thumbWeights.push(influence(i, /^RightHandThumb/)); oppositeWeights.push(influence(i, /^RightHand(Index|Middle)/))
  }
  assert.ok(sole.length > 100)
  const faces = [], thumbFaces = [], oppositeFaces = [], index = geo.index
  for (let i = 0; i < index.count; i += 3) {
    const face = [index.getX(i), index.getX(i + 1), index.getX(i + 2)]
    if (face.some(j => handWeights[j] > .05)) faces.push(...face)
    if (face.every(j => thumbWeights[j] > .6)) thumbFaces.push(...face)
    if (face.every(j => oppositeWeights[j] > .6)) oppositeFaces.push(...face)
  }
  assert.ok(faces.length && thumbFaces.length && oppositeFaces.length)
  const ids = [...new Set(faces)], remap = new Map(ids.map((v, i) => [v, i]))
  const tipVertices = Object.fromEntries(['Thumb', 'Index', 'Middle'].map(finger => {
    const pattern = new RegExp(`^RightHand${finger}3$`)
    const entries = ids.map((id, i) => [i, influence(id, pattern)]).filter(([, w]) => w > .4)
    assert.ok(entries.length > 10, `${finger} tip weights`)
    return [finger, entries]
  }))
  function surface(faceList) {
    const g = new THREE.BufferGeometry()
    g.setAttribute('position', new THREE.BufferAttribute(new Float32Array(ids.length * 3), 3))
    g.setIndex(faceList.map(i => remap.get(i))); return g
  }
  const handGeo = surface(faces), thumbGeo = surface(thumbFaces), oppositeGeo = surface(oppositeFaces)
  const items = Object.values(JSON.parse(readFileSync(new URL('data/items.json', root))))
  const monsterWeapon = items.find(i => i.id === row.weapon)
  assert.ok(monsterWeapon?.weaponType, `Unknown weapon type for ${row.weapon}`)
  const weaponPaths = [...new Set(items.filter(i => i.weaponType === monsterWeapon.weaponType && i.worldModel).map(i => i.worldModel))]
  assert.ok(weaponPaths.length)
  const weapons = []
  for (const path of weaponPaths) {
    const loaded = await load(`client/public/models/${path}`); loaded.scene.updateMatrixWorld(true)
    const parts = []
    loaded.scene.traverse(o => { if (o.isMesh) { const geometry = o.geometry.clone(); geometry.applyMatrix4(o.matrixWorld); geometry.boundsTree = new MeshBVH(geometry); parts.push(geometry) } })
    assert.ok(parts.length, path); weapons.push({ path, parts, samples: 0, collisions: 0, minimum_clearance_m: Infinity })
  }
  const hand = target.scene.getObjectByName('RightHand'), inverse = new THREE.Matrix4(), vertex = new THREE.Vector3()
  const mixer = new THREE.AnimationMixer(target.scene), clips = [], opposition = []
  let thumbCollisions = 0, thumbGap = Infinity, thumbMaxGap = 0, minTipRise = Infinity, minSideSeparation = Infinity
  for (const clip of target.animations) {
    const action = mixer.clipAction(clip).setLoop(THREE.LoopOnce, 1); action.clampWhenFinished = true; action.play()
    const heights = [], steps = Math.ceil(clip.duration * 120); let restMin = 0
    for (let k = 0; k <= steps; k++) {
      mixer.setTime(Math.min(clip.duration - 1e-5, clip.duration * k / steps)); target.scene.updateMatrixWorld(true); mesh.skeleton.update()
      let low = Infinity
      for (const i of sole) { mesh.getVertexPosition(i, vertex).applyMatrix4(mesh.matrixWorld); low = Math.min(low, vertex.y) }
      heights.push(low)
      if (k === steps && isClip('animDie', clip.name)) { restMin = Infinity; for (let i = 0; i < position.count; i++) { mesh.getVertexPosition(i, vertex).applyMatrix4(mesh.matrixWorld); restMin = Math.min(restMin, vertex.y) } }
    }
    const min = Math.min(...heights), max = Math.max(...heights)
    if (['animIdle', 'animWalk', 'animAttackIdle', 'animAttack'].some(key => isClip(key, clip.name))) assert.ok(min >= -.02 && max < .02, `${clip.name} sole ${min}..${max}`)
    if (isClip('animRun', clip.name)) assert.ok(min >= -.02 && min < .02)
    if (isClip('animDie', clip.name)) assert.ok(Math.abs(restMin) < .02)
    for (let k = 0; k < samples; k++) {
      mixer.setTime(Math.min(clip.duration - 1e-5, clip.duration * k / Math.max(1, samples - 1))); target.scene.updateMatrixWorld(true); mesh.skeleton.update(); inverse.copy(hand.matrixWorld).invert()
      const values = handGeo.attributes.position
      ids.forEach((id, j) => { mesh.getVertexPosition(id, vertex).applyMatrix4(mesh.matrixWorld).applyMatrix4(inverse); values.setXYZ(j, vertex.x, vertex.y, vertex.z) })
      const tipCenters = Object.fromEntries(Object.entries(tipVertices).map(([name, entries]) => {
        const sum = new THREE.Vector3(); let total = 0
        for (const [i, w] of entries) { sum.addScaledVector(new THREE.Vector3().fromBufferAttribute(values, i), w); total += w }
        return [name, sum.multiplyScalar(1 / total).applyMatrix4(new THREE.Matrix4().copy(attach.matrix).invert())]
      }))
      const opposingTip = tipCenters.Index.clone().add(tipCenters.Middle).multiplyScalar(.5)
      const rise = tipCenters.Thumb.z - opposingTip.z, separation = Math.abs(tipCenters.Thumb.y - opposingTip.y)
      minTipRise = Math.min(minTipRise, rise); minSideSeparation = Math.min(minSideSeparation, separation)
      assert.ok(rise > .003 && separation > .005, `${clip.name} thumb surface does not cross the finger tips`)
      values.needsUpdate = true; handGeo.boundsTree = new MeshBVH(handGeo)
      for (const weapon of weapons) {
        weapon.samples++
        let hit = false, minimum = Infinity
        for (const part of weapon.parts) {
          if (handGeo.boundsTree.intersectsGeometry(part, attach.matrix)) hit = true
          else { const result = handGeo.boundsTree.closestPointToGeometry(part, attach.matrix); if (result) minimum = Math.min(minimum, result.distance) }
        }
        if (hit) weapon.collisions++; else weapon.minimum_clearance_m = Math.min(weapon.minimum_clearance_m, minimum)
      }
      thumbGeo.attributes.position.array.set(values.array); oppositeGeo.attributes.position.array.set(values.array)
      thumbGeo.boundsTree = new MeshBVH(thumbGeo); oppositeGeo.boundsTree = new MeshBVH(oppositeGeo)
      if (thumbGeo.boundsTree.intersectsGeometry(oppositeGeo, new THREE.Matrix4())) thumbCollisions++
      else { const result = thumbGeo.boundsTree.closestPointToGeometry(oppositeGeo, new THREE.Matrix4()); if (result) { thumbGap = Math.min(thumbGap, result.distance); thumbMaxGap = Math.max(thumbMaxGap, result.distance) } }
      if (k === 0) {
        const toWeapon = new THREE.Matrix4().copy(attach.matrix).invert().multiply(inverse)
        const points = [1, 2, 3, 4].map(i => mesh.skeleton.getBoneByName(`RightHandThumb${i}`).getWorldPosition(new THREE.Vector3()).applyMatrix4(toWeapon))
        const indexTip = mesh.skeleton.getBoneByName('RightHandIndex4').getWorldPosition(new THREE.Vector3()).applyMatrix4(toWeapon)
        const middleTip = mesh.skeleton.getBoneByName('RightHandMiddle4').getWorldPosition(new THREE.Vector3()).applyMatrix4(toWeapon)
        assert.ok(points[0].y < 0 && points[1].y < 0 && points[2].y < .002 && points[3].y > points[2].y && points[3].y > -.005, `Thumb opposition ${clip.name}`)
        assert.ok(indexTip.x > middleTip.x, `${clip.name} index/middle order`)
        opposition.push({ clip: clip.name, thumb: points.map(p => p.toArray()), index: indexTip.toArray(), middle: middleTip.toArray() })
      }
    }
    clips.push({ name: clip.name, duration_s: clip.duration, sole_height_range_m: [min, max], rest_lowest_m: isClip('animDie', clip.name) ? restMin : undefined })
    action.stop(); mixer.uncacheClip(clip)
  }
  for (const weapon of weapons) {
    delete weapon.parts
    assert.equal(weapon.collisions, 0, `${weapon.path} intersects in ${weapon.collisions} samples`)
    assert.ok(Number.isFinite(weapon.minimum_clearance_m) && weapon.minimum_clearance_m > 0)
  }
  assert.equal(thumbCollisions, 0); assert.ok(thumbGap > 0); assert.ok(thumbMaxGap < .01, `Thumb gap ${thumbGap}..${thumbMaxGap} m`)
  const report = { id, attachment_source: 'data/monsters.json', attachment: { offset_m: attach.offset, rotation_degrees_xyz: attach.rotation }, joints: mesh.skeleton.bones.length, triangles: index.count / 3, vertices: position.count, height_m: box.max.y - box.min.y, unweighted_vertices: unweighted, max_weight_sum_error: maxError, max_bind_position_difference_m: maxBindDifference, clips, weapons, thumb: { collision_samples: thumbCollisions, opposing_gap_range_m: [thumbGap, thumbMaxGap], min_surface_tip_rise_m: minTipRise, min_surface_side_separation_m: minSideSeparation, opposition }, samples_per_clip: samples }
  writeFileSync(`${out}/validation.json`, JSON.stringify(report, null, 2) + '\n'); console.log(JSON.stringify(report, null, 2))
} finally { await server.close() }

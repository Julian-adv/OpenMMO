import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import * as THREE from '../../client/node_modules/three/build/three.module.js'
import { root } from './headless-three.mjs'

export function monsterRow(id) {
  const row = JSON.parse(readFileSync(new URL('data/monsters.json', root)))[id]
  assert.ok(row, `Unknown monster ${id}`)
  return row
}

export function packClipNames(row, packs, splitClipNames) {
  const available = new Set(packs.flatMap(pack => pack.animations.map(clip => clip.name)))
  return [...new Set(Object.entries(row).filter(([key]) => key.startsWith('anim')).flatMap(([, value]) => splitClipNames(value)).filter(name => available.has(name)))]
}

export function gripRotations(mesh, fit, reference) {
  const clip = reference.animations.find(c => c.name === fit.reference_clip)
  assert.ok(clip, `Missing grip clip ${fit.reference_clip}`)
  const sourceHand = reference.scene.getObjectByName('RightHand')
  const targetHand = mesh.skeleton.getBoneByName('RightHand')
  reference.scene.updateMatrixWorld(true); mesh.updateWorldMatrix(true, true)
  const tracks = clip.tracks.filter(t => /^RightHand(Thumb|Index|Middle|Ring|Pinky)[123]\.quaternion$/.test(t.name))
  assert.equal(tracks.length, 15)
  for (const track of tracks) reference.scene.getObjectByName(track.name.split('.')[0]).quaternion.fromArray(track.values)
  reference.scene.updateMatrixWorld(true)
  const result = new Map(), saved = new Map()
  for (const finger of ['Thumb', 'Index', 'Middle', 'Ring', 'Pinky']) for (let i = 1; i <= 3; i++) {
    const name = `RightHand${finger}${i}`, childName = `RightHand${finger}${i + 1}`
    const source = reference.scene.getObjectByName(name), sourceChild = reference.scene.getObjectByName(childName)
    const target = mesh.skeleton.getBoneByName(name), targetChild = mesh.skeleton.getBoneByName(childName)
    assert.ok(source && sourceChild && target && targetChild, name)
    saved.set(name, target.quaternion.clone())
    const sourceHandQ = sourceHand.getWorldQuaternion(new THREE.Quaternion()), targetHandQ = targetHand.getWorldQuaternion(new THREE.Quaternion())
    const sourceDirection = sourceChild.getWorldPosition(new THREE.Vector3()).sub(source.getWorldPosition(new THREE.Vector3())).applyQuaternion(sourceHandQ.clone().invert()).normalize()
    const targetDirection = targetChild.getWorldPosition(new THREE.Vector3()).sub(target.getWorldPosition(new THREE.Vector3())).applyQuaternion(targetHandQ.clone().invert()).normalize()
    const correction = targetHandQ.clone().multiply(new THREE.Quaternion().setFromUnitVectors(targetDirection, sourceDirection)).multiply(targetHandQ.clone().invert())
    const rotation = target.parent.getWorldQuaternion(new THREE.Quaternion()).invert().multiply(correction).multiply(target.getWorldQuaternion(new THREE.Quaternion()))
    target.quaternion.copy(rotation); target.updateWorldMatrix(false, true)
    result.set(name, rotation.clone().normalize())
  }
  for (const [name, rotation] of saved) {
    const finger = name.match(/^RightHand(Thumb|Index|Middle|Ring|Pinky)/)[1]
    const adjusted = rotation.clone().slerp(result.get(name), fit.pose_blend[finger])
    if (finger === 'Thumb') {
      const angles = fit.thumb_correction_degrees_xyz[Number(name.at(-1)) - 1].map(THREE.MathUtils.degToRad)
      adjusted.multiply(new THREE.Quaternion().setFromEuler(new THREE.Euler(...angles, 'XYZ')))
    }
    result.set(name, adjusted.normalize())
    mesh.skeleton.getBoneByName(name).quaternion.copy(rotation)
  }
  targetHand.updateWorldMatrix(true, true)
  return result
}

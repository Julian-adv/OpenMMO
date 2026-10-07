import * as THREE from 'three'
import { blendSkinToJoint } from './skinWeights'
import cuff from '../data/rangerBootCuff.json'

function profileAt(x: number, z: number, column: 0 | 1) {
  const sample =
    ((Math.atan2(z, x) / (2 * Math.PI) + 1) % 1) * cuff.profile.length
  const index = Math.floor(sample)
  return THREE.MathUtils.lerp(
    cuff.profile[index][column],
    cuff.profile[(index + 1) % cuff.profile.length][column],
    sample - index
  )
}

export function rangerBootRim(point: THREE.Vector3) {
  const x = Math.abs(point.x) - cuff.origin[0]
  const z = point.z - cuff.origin[1]
  return {
    x,
    z,
    radius: Math.hypot(x, z),
    height: profileAt(x, z, 0),
    innerRadius: profileAt(x, z, 1) - cuff.inset,
  }
}

export const rangerPantsBootDistance = (point: THREE.Vector3) =>
  point.y -
  profileAt(Math.abs(point.x) - cuff.origin[0], point.z - cuff.origin[1], 0) +
  cuff.overlap

export function tuckPantsIntoRangerBoots(geometry: THREE.BufferGeometry) {
  const { position, skinIndex, skinWeight } = geometry.attributes
  const point = new THREE.Vector3()
  for (let i = 0; i < position.count; i++) {
    point.fromBufferAttribute(position, i)
    const rim = rangerBootRim(point)
    const hem = rim.height - cuff.overlap
    const blend =
      1 - THREE.MathUtils.smoothstep(point.y, hem, rim.height + cuff.taper)
    if (!blend) continue
    const radius = THREE.MathUtils.lerp(
      rim.radius,
      Math.min(rim.radius, rim.innerRadius),
      blend
    )
    const scale = radius / Math.max(rim.radius, 1e-9)
    const side = point.x < 0 ? -1 : 1
    position.setXYZ(
      i,
      side * (cuff.origin[0] + rim.x * scale),
      Math.max(point.y, hem),
      cuff.origin[1] + rim.z * scale
    )
    if (skinIndex && skinWeight)
      blendSkinToJoint(
        skinIndex,
        skinWeight,
        i,
        side === 1 ? cuff.leftLeg : cuff.rightLeg,
        blend
      )
  }
  geometry.computeVertexNormals()
}

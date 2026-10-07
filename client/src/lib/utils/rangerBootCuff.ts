import * as THREE from 'three'
import cuff from '../data/rangerBootCuff.json'

export function rangerBootRim(point: THREE.Vector3) {
  const x = Math.abs(point.x) - cuff.origin[0]
  const z = point.z - cuff.origin[1]
  const angle = Math.atan2(z, x)
  const sample = ((angle / (2 * Math.PI) + 1) % 1) * cuff.profile.length
  const index = Math.floor(sample)
  const first = cuff.profile[index]
  const next = cuff.profile[(index + 1) % cuff.profile.length]
  const fraction = sample - index
  return {
    x,
    z,
    radius: Math.hypot(x, z),
    height: THREE.MathUtils.lerp(first[0], next[0], fraction),
    innerRadius: THREE.MathUtils.lerp(first[1], next[1], fraction) - cuff.inset,
  }
}

export const rangerPantsBootDistance = (point: THREE.Vector3) =>
  point.y - (rangerBootRim(point).height - cuff.overlap)

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
    if (skinIndex && skinWeight) {
      const leg = side === 1 ? cuff.leftLeg : cuff.rightLeg
      const weights = new Map<number, number>([[leg, blend]])
      for (let j = 0; j < 4; j++) {
        const joint = skinIndex.getComponent(i, j)
        weights.set(
          joint,
          (weights.get(joint) ?? 0) +
            skinWeight.getComponent(i, j) * (1 - blend)
        )
      }
      const sorted = [...weights].sort((a, b) => b[1] - a[1]).slice(0, 4)
      const total = sorted.reduce((sum, [, weight]) => sum + weight, 0)
      for (let j = 0; j < 4; j++) {
        skinIndex.setComponent(i, j, sorted[j]?.[0] ?? 0)
        skinWeight.setComponent(i, j, (sorted[j]?.[1] ?? 0) / total)
      }
    }
  }
  geometry.computeVertexNormals()
}

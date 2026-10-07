import * as THREE from 'three'
import cuff from '../data/rangerGloveCuff.json'

const wrist = new THREE.Vector3().fromArray(cuff.wrist)
const axis = new THREE.Vector3().fromArray(cuff.axis)
const basis = cuff.basis.map((v) => new THREE.Vector3().fromArray(v))

export function tuckSleevesIntoRangerGloves(geometry: THREE.BufferGeometry) {
  const { position, skinIndex, skinWeight } = geometry.attributes
  const point = new THREE.Vector3()
  for (let i = 0; i < position.count; i++) {
    point.fromBufferAttribute(position, i)
    if (Math.abs(point.x) < 0.28) continue
    const left = point.x > 0
    if (left) point.x *= -1
    point.sub(wrist)
    const along = point.dot(axis)
    const blend = THREE.MathUtils.smoothstep(
      along,
      cuff.taperStart,
      cuff.taperEnd
    )
    if (!blend) continue
    let firstIndex = 0
    while (
      firstIndex + 1 < cuff.sections.length &&
      cuff.sections[firstIndex + 1].along <= along
    )
      firstIndex++
    const first = cuff.sections[firstIndex]
    const next =
      cuff.sections[Math.min(firstIndex + 1, cuff.sections.length - 1)]
    const t =
      first === next
        ? 0
        : THREE.MathUtils.clamp(
            (along - first.along) / (next.along - first.along),
            0,
            1
          )
    const center = first.center.map((v, j) =>
      THREE.MathUtils.lerp(v, next.center[j], t)
    )
    const x = point.dot(basis[0]) - center[0]
    const y = point.dot(basis[1]) - center[1]
    const angle =
      ((Math.atan2(y, x) / (2 * Math.PI) + 1) % 1) * first.radius.length
    const sample = Math.floor(angle)
    const radiusAt = (section: typeof first) =>
      THREE.MathUtils.lerp(
        section.radius[sample],
        section.radius[(sample + 1) % section.radius.length],
        angle - sample
      )
    const radius = Math.hypot(x, y)
    const limit = Math.max(
      0.001,
      THREE.MathUtils.lerp(radiusAt(first), radiusAt(next), t) - cuff.inset
    )
    const scale = THREE.MathUtils.lerp(
      1,
      Math.min(1, limit / Math.max(radius, 1e-9)),
      blend
    )
    point
      .copy(wrist)
      .addScaledVector(axis, along)
      .addScaledVector(basis[0], center[0] + x * scale)
      .addScaledVector(basis[1], center[1] + y * scale)
    position.setXYZ(i, left ? -point.x : point.x, point.y, point.z)
    if (skinIndex && skinWeight) {
      const joint = left ? cuff.leftForeArm : cuff.rightForeArm
      const weights = new Map<number, number>([[joint, blend]])
      for (let j = 0; j < 4; j++) {
        const index = skinIndex.getComponent(i, j)
        weights.set(
          index,
          (weights.get(index) ?? 0) +
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

import type * as THREE from 'three'

type Attribute = THREE.BufferAttribute | THREE.InterleavedBufferAttribute

export function blendSkinToJoint(
  skinIndex: Attribute,
  skinWeight: Attribute,
  i: number,
  joint: number,
  blend: number
) {
  const weights = new Map<number, number>([[joint, blend]])
  for (let j = 0; j < 4; j++) {
    const index = skinIndex.getComponent(i, j)
    weights.set(
      index,
      (weights.get(index) ?? 0) + skinWeight.getComponent(i, j) * (1 - blend)
    )
  }
  const sorted = [...weights].sort((a, b) => b[1] - a[1]).slice(0, 4)
  const total = sorted.reduce((sum, [, weight]) => sum + weight, 0)
  for (let j = 0; j < 4; j++) {
    skinIndex.setComponent(i, j, sorted[j]?.[0] ?? 0)
    skinWeight.setComponent(i, j, (sorted[j]?.[1] ?? 0) / total)
  }
}

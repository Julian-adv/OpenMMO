import * as THREE from 'three'

type Cut =
  | 'bracers'
  | 'gauntlets'
  | 'gloves'
  | 'greaves'
  | 'leather_boots'
  | 'caveman_boots'
  | 'collar'
  | 'tripo_collar'
  | 'tripo_waist'
  | 'tripo_covered_waist'
type Distance = (point: THREE.Vector3) => number

function sleeveCut(fraction: number): Distance {
  const planes = [1, -1].map((side) => {
    const elbow = new THREE.Vector3(side * 0.311339, 1.247962, -0.056036)
    const wrist = new THREE.Vector3(side * 0.445653, 0.993593, -0.045557)
    return new THREE.Plane().setFromNormalAndCoplanarPoint(
      elbow.clone().sub(wrist).normalize(),
      elbow.lerp(wrist, fraction)
    )
  })
  return (point) =>
    Math.max(
      0.28 - Math.abs(point.x),
      planes[point.x < 0 ? 1 : 0].distanceToPoint(point)
    )
}

const cuts: Record<Cut, Distance> = {
  bracers: sleeveCut(0.23),
  gauntlets: sleeveCut(0.85),
  gloves: sleeveCut(0.89),
  greaves: (point) => point.y - 0.46,
  leather_boots: (point) => point.y - 0.235,
  caveman_boots: (point) => point.y - 0.43,
  collar: (point) => Math.max(1.61 - point.y, Math.abs(point.x) - 0.075),
  tripo_collar: (point) => Math.max(1.54 - point.y, Math.abs(point.x) - 0.075),
  tripo_waist: (point) => point.y - 1.14,
  tripo_covered_waist: (point) => point.y - 1.105,
}

export function clipSkinnedGeometry(
  source: THREE.BufferGeometry,
  distance: Distance
): THREE.BufferGeometry {
  const attributes = Object.entries(source.attributes)
  const values = Object.fromEntries(
    attributes.map(([name]) => [name, [] as number[]])
  )
  const vertices = new Map<number, number>()
  const edges = new Map<string, number>()
  const indices: number[] = []
  const point = new THREE.Vector3()
  const position = source.getAttribute('position')
  const distances = Array.from({ length: position.count }, (_, i) =>
    distance(point.fromBufferAttribute(position, i))
  )
  const vertex = (index: number) => {
    const existing = vertices.get(index)
    if (existing !== undefined) return existing
    const next = values.position.length / 3
    for (const [name, attribute] of attributes)
      for (let i = 0; i < attribute.itemSize; i++)
        values[name].push(attribute.getComponent(index, i))
    vertices.set(index, next)
    return next
  }
  const intersect = (a: number, b: number) => {
    if (distances[a] === 0) return vertex(a)
    if (distances[b] === 0) return vertex(b)
    if (a > b) [a, b] = [b, a]
    const key = `${a}:${b}`
    const existing = edges.get(key)
    if (existing !== undefined) return existing
    const next = values.position.length / 3
    const t = distances[a] / (distances[a] - distances[b])
    for (const [name, attribute] of attributes) {
      if (name === 'skinIndex' || name === 'skinWeight') continue
      const interpolated = Array.from({ length: attribute.itemSize }, (_, i) =>
        THREE.MathUtils.lerp(
          attribute.getComponent(a, i),
          attribute.getComponent(b, i),
          t
        )
      )
      if (name === 'normal' || name === 'tangent') {
        point.fromArray(interpolated).normalize().toArray(interpolated)
        if (name === 'tangent')
          interpolated[3] = attribute.getComponent(t < 0.5 ? a : b, 3)
      }
      values[name].push(...interpolated)
    }
    if (source.hasAttribute('skinIndex') && source.hasAttribute('skinWeight')) {
      const weights = new Map<number, number>()
      for (const [index, factor] of [
        [a, 1 - t],
        [b, t],
      ])
        for (let i = 0; i < 4; i++) {
          const joint = source.attributes.skinIndex.getComponent(index, i)
          const weight =
            source.attributes.skinWeight.getComponent(index, i) * factor
          weights.set(joint, (weights.get(joint) ?? 0) + weight)
        }
      const blended = [...weights].sort((a, b) => b[1] - a[1]).slice(0, 4)
      const total = blended.reduce((sum, [, weight]) => sum + weight, 0)
      for (let i = 0; i < 4; i++) {
        values.skinIndex.push(blended[i]?.[0] ?? 0)
        values.skinWeight.push(total ? (blended[i]?.[1] ?? 0) / total : 0)
      }
    }
    edges.set(key, next)
    return next
  }
  const geometry = new THREE.BufferGeometry()
  const count = source.index?.count ?? position.count
  for (const group of source.groups.length
    ? source.groups
    : [{ start: 0, count }]) {
    const start = indices.length
    for (
      let i = group.start;
      i < Math.min(group.start + group.count, count);
      i += 3
    ) {
      const face = [0, 1, 2].map(
        (offset) => source.index?.getX(i + offset) ?? i + offset
      )
      const polygon: number[] = []
      for (let j = 0; j < 3; j++) {
        const a = face[j],
          b = face[(j + 1) % 3]
        if (distances[a] >= 0) polygon.push(vertex(a))
        if (distances[a] >= 0 !== distances[b] >= 0)
          polygon.push(intersect(a, b))
      }
      for (let j = 1; j + 1 < polygon.length; j++) {
        const a = polygon[0],
          b = polygon[j],
          c = polygon[j + 1]
        if (a !== b && a !== c && b !== c) indices.push(a, b, c)
      }
    }
    if (source.groups.length)
      geometry.addGroup(start, indices.length - start, group.materialIndex)
  }
  for (const [name, attribute] of attributes) {
    const clipped =
      name === 'skinIndex'
        ? new THREE.Uint16BufferAttribute(values[name], attribute.itemSize)
        : new THREE.Float32BufferAttribute(values[name], attribute.itemSize)
    geometry.setAttribute(name, clipped)
  }
  geometry.setIndex(indices)
  return geometry
}

interface Variants {
  source: THREE.BufferGeometry
  cuts: Map<string, THREE.BufferGeometry>
}

const variants = new WeakMap<THREE.BufferGeometry, Variants>()

function setGeometry(mesh: THREE.SkinnedMesh, geometry: THREE.BufferGeometry) {
  if (mesh.geometry === geometry) return
  mesh.geometry = geometry
  const bounds: {
    boundingBox: THREE.Box3 | null
    boundingSphere: THREE.Sphere | null
  } = mesh
  bounds.boundingBox = null
  bounds.boundingSphere = null
}

export function trimModularClothing(
  mesh: THREE.SkinnedMesh,
  cut?: Cut,
  skin = false
) {
  let cached = variants.get(mesh.geometry)
  if (!cut) {
    if (cached) setGeometry(mesh, cached.source)
    return
  }
  if (!mesh.geometry.hasAttribute('position')) return
  if (!cached) {
    cached = { source: mesh.geometry, cuts: new Map() }
    variants.set(mesh.geometry, cached)
    const owned = cached
    mesh.geometry.addEventListener('dispose', () => {
      for (const geometry of owned.cuts.values()) {
        variants.delete(geometry)
        geometry.dispose()
      }
      owned.cuts.clear()
      variants.delete(owned.source)
    })
  }
  const key = `${cut}:${skin}`
  let geometry = cached.cuts.get(key)
  if (!geometry) {
    geometry = clipSkinnedGeometry(
      cached.source,
      (point) => cuts[cut](point) * (skin ? -1 : 1)
    )
    cached.cuts.set(key, geometry)
    variants.set(geometry, cached)
  }
  setGeometry(mesh, geometry)
}

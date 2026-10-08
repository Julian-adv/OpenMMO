import { Box3, type BufferGeometry, Vector3 } from 'three'

export function meshIslands(geometry: BufferGeometry, precision: number) {
  const position = geometry.getAttribute('position')
  const index = geometry.index
  const parents = Array.from({ length: position.count }, (_, i) => i)
  const find = (i: number): number => {
    while (parents[i] !== i) {
      parents[i] = parents[parents[i]]
      i = parents[i]
    }
    return i
  }
  const join = (a: number, b: number) => {
    parents[find(a)] = find(b)
  }
  const key = (p: Vector3) =>
    `${Math.round(p.x * precision)},${Math.round(p.y * precision)},${Math.round(p.z * precision)}`
  const welded = new Map<string, number>()
  const point = new Vector3()
  for (let i = 0; i < position.count; i++) {
    const id = key(point.fromBufferAttribute(position, i))
    const previous = welded.get(id)
    if (previous !== undefined) join(i, previous)
    else welded.set(id, i)
  }
  const count = index?.count ?? position.count
  for (let i = 0; i < count; i += 3) {
    const first = index?.getX(i) ?? i
    join(index?.getX(i + 1) ?? i + 1, first)
    join(index?.getX(i + 2) ?? i + 2, first)
  }
  const islands = new Map<number, { bounds: Box3; count: number }>()
  for (let i = 0; i < position.count; i++) {
    const root = find(i)
    let island = islands.get(root)
    if (!island) {
      island = { bounds: new Box3(), count: 0 }
      islands.set(root, island)
    }
    island.bounds.expandByPoint(point.fromBufferAttribute(position, i))
    island.count++
  }
  return {
    find,
    islands,
    islandAt: (p: Vector3) => find(welded.get(key(p))!),
  }
}

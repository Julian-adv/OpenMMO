import {
  Box3,
  type BufferGeometry,
  type Material,
  type SkinnedMesh,
  Vector3,
} from 'three'

const separated = new WeakMap<BufferGeometry, BufferGeometry>()
const ornamentMaterials = new WeakMap<Material, Material>()

export function separateWavyHairMaterials(mesh: SkinnedMesh): void {
  const source = mesh.geometry
  let geometry = separated.get(source)
  if (!geometry) {
    const position = source.getAttribute('position')
    const indices = source.index
    if (!indices) throw new Error('Wavy hair has no triangle indices')
    const parents = Array.from({ length: position.count }, (_, i) => i)
    const find = (index: number): number =>
      parents[index] === index ? index : (parents[index] = find(parents[index]))
    const join = (a: number, b: number) => {
      parents[find(a)] = find(b)
    }
    const welded = new Map<string, number>()
    const point = new Vector3()
    for (let i = 0; i < position.count; i++) {
      point.fromBufferAttribute(position, i)
      const key = point
        .toArray()
        .map((value) => Math.round(value * 1e6))
        .join(',')
      const previous = welded.get(key)
      if (previous !== undefined) join(i, previous)
      else welded.set(key, i)
    }
    for (let i = 0; i < indices.count; i += 3) {
      join(indices.getX(i), indices.getX(i + 1))
      join(indices.getX(i), indices.getX(i + 2))
    }
    const components = new Map<number, { bounds: Box3; count: number }>()
    for (let i = 0; i < position.count; i++) {
      const id = find(i)
      let component = components.get(id)
      if (!component) {
        component = { bounds: new Box3(), count: 0 }
        components.set(id, component)
      }
      component.bounds.expandByPoint(point.fromBufferAttribute(position, i))
      component.count++
    }
    const ornaments = new Set(
      [...components]
        .filter(([, { bounds, count }]) => {
          const size = bounds.getSize(point)
          return count > 40 && Math.max(size.x, size.y, size.z) < 0.04
        })
        .map(([id]) => id)
    )
    const hairIndices: number[] = []
    const boneIndices: number[] = []
    for (let i = 0; i < indices.count; i += 3) {
      const target = ornaments.has(find(indices.getX(i)))
        ? boneIndices
        : hairIndices
      target.push(indices.getX(i), indices.getX(i + 1), indices.getX(i + 2))
    }
    geometry = source.clone()
    geometry.setIndex([...hairIndices, ...boneIndices])
    geometry.clearGroups()
    geometry.addGroup(0, hairIndices.length, 0)
    geometry.addGroup(hairIndices.length, boneIndices.length, 1)
    separated.set(source, geometry)
    source.addEventListener('dispose', () => geometry?.dispose())
  }
  const original: Material = Array.isArray(mesh.material)
    ? mesh.material[0]
    : mesh.material
  let ornament = ornamentMaterials.get(original)
  if (!ornament) {
    ornament = original.clone()
    ornament.name = 'wavy_hair_bone_ornaments'
    ornament.userData.appearance_color_fixed = true
    ornamentMaterials.set(original, ornament)
  }
  mesh.geometry = geometry
  mesh.material = [original, ornament]
}

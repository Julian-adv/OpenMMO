import {
  type BufferGeometry,
  type Material,
  type SkinnedMesh,
  Vector3,
} from 'three'
import { meshIslands } from './meshIslands'

const separated = new WeakMap<BufferGeometry, BufferGeometry>()
const ornamentMaterials = new WeakMap<Material, Material>()

export function separateWavyHairMaterials(mesh: SkinnedMesh): void {
  const source = mesh.geometry
  let geometry = separated.get(source)
  if (!geometry) {
    const indices = source.index
    if (!indices) throw new Error('Wavy hair has no triangle indices')
    const { find, islands } = meshIslands(source, 1e6)
    const size = new Vector3()
    const ornaments = new Set(
      [...islands]
        .filter(([, { bounds, count }]) => {
          bounds.getSize(size)
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

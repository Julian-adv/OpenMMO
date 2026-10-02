import { Material, Mesh, type Object3D } from 'three'
import { StandardNodeLibrary, type Node, type NodeMaterial } from 'three/webgpu'
import { convert, luminance, mix, output, uniform, vec3, vec4 } from 'three/tsl'

const library = new StandardNodeLibrary()

export class CharacterGrayscale {
  private readonly amount = uniform(0)
  private readonly materials = new Map<
    Material,
    { material: NodeMaterial; version: number }
  >()
  private readonly originals = new WeakMap<Material, Material>()
  private meshes = new Set<Mesh>()

  setEnabled(enabled: boolean): void {
    this.amount.value = enabled ? 1 : 0
  }

  private configure(material: NodeMaterial): void {
    const base = convert(
      material.fragmentNode ?? material.outputNode ?? output,
      'vec4'
    ) as Node<'vec4'>
    const grayscale = vec3(luminance(base.rgb).mul(0.85))
    const result = vec4(mix(base.rgb, grayscale, this.amount), base.a)
    if (material.fragmentNode) material.fragmentNode = result
    else material.outputNode = result
  }

  private convert(source: Material): NodeMaterial {
    const material = library.fromMaterial(source.clone())
    this.configure(material)
    this.originals.set(material, source)
    return material
  }

  restore(root: Object3D): void {
    root.traverse((object) => {
      if (!(object instanceof Mesh)) return
      const original = (material: Material) =>
        this.originals.get(material) ?? material
      object.material = Array.isArray(object.material)
        ? object.material.map(original)
        : original(object.material)
    })
  }

  sync(...roots: (Object3D | null)[]): void {
    if (this.amount.value === 0 && this.materials.size === 0) return
    const used = new Set<Material>()
    const meshes = new Set<Mesh>()
    const wrap = (current: Material) => {
      const source = this.originals.get(current) ?? current
      used.add(source)
      let entry = this.materials.get(source)
      if (!entry) {
        entry = { material: this.convert(source), version: source.version }
        this.materials.set(source, entry)
      } else if (entry.version !== source.version) {
        const updated = library.fromMaterial(source.clone())
        entry.material.copy(updated)
        this.configure(entry.material)
        entry.material.needsUpdate = true
        entry.version = source.version
      }
      return entry.material
    }
    for (const root of roots) {
      root?.traverse((object) => {
        if (!(object instanceof Mesh)) return
        meshes.add(object)
        object.material = Array.isArray(object.material)
          ? object.material.map(wrap)
          : wrap(object.material)
      })
    }
    for (const mesh of this.meshes) {
      if (!meshes.has(mesh)) this.restore(mesh)
    }
    this.meshes = meshes
    for (const [source, entry] of this.materials) {
      if (used.has(source)) continue
      entry.material.dispose()
      this.materials.delete(source)
    }
  }

  dispose(): void {
    for (const mesh of this.meshes) this.restore(mesh)
    this.meshes.clear()
    for (const entry of this.materials.values()) entry.material.dispose()
    this.materials.clear()
  }
}

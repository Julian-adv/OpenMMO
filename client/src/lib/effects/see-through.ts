import type * as THREE from 'three'

/** How solid the invisibility cloak wearer looks to themselves. */
const SEE_THROUGH_OPACITY = 0.3

type MaterialSlot = THREE.Material | THREE.Material[]

interface Faded {
  original: MaterialSlot
  faded: MaterialSlot
  castShadow: boolean
}

const sources = new WeakMap<THREE.Material, THREE.Material>()

/** The material a see-through clone was made from, so material swaps applied
 *  over a faded mesh tint the real material instead of the ghost. */
export function seeThroughSource(
  material: THREE.Material
): THREE.Material | undefined {
  return sources.get(material)
}

function fade(material: THREE.Material): THREE.Material {
  const clone = material.clone()
  clone.transparent = true
  clone.opacity = material.opacity * SEE_THROUGH_OPACITY
  return clone
}

/** Fades a character to a ghost while `on`. Runs per frame so gear loaded or
 *  reskinned later fades too; clones keep materials shared with others solid. */
export class SeeThrough {
  private meshes = new Map<THREE.Mesh, Faded>()
  // One clone per source: a fresh clone per swap compiles a new pipeline.
  private fades = new Map<THREE.Material, THREE.Material>()

  private fadeOf(material: THREE.Material): THREE.Material {
    if (sources.has(material)) return material
    let faded = this.fades.get(material)
    if (!faded) {
      faded = fade(material)
      sources.set(faded, material)
      this.fades.set(material, faded)
    }
    return faded
  }

  update(root: THREE.Object3D | undefined, on: boolean) {
    if (!on) return this.restore()
    root?.traverse((child) => {
      const mesh = child as THREE.Mesh
      if (!mesh.isMesh) return
      const known = this.meshes.get(mesh)
      if (known?.faded === mesh.material) return
      const original = mesh.material
      const faded = Array.isArray(original)
        ? original.map((material) => this.fadeOf(material))
        : this.fadeOf(original)
      this.meshes.set(mesh, {
        original,
        faded,
        castShadow: known?.castShadow ?? mesh.castShadow,
      })
      mesh.material = faded
      mesh.castShadow = false
    })
  }

  restore() {
    for (const [mesh, { original, faded, castShadow }] of this.meshes) {
      if (mesh.material === faded) mesh.material = original
      mesh.castShadow = castShadow
    }
    this.meshes.clear()
    for (const faded of this.fades.values()) faded.dispose()
    this.fades.clear()
  }
}

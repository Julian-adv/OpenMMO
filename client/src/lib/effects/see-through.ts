import * as THREE from 'three'

/** How solid the invisibility cloak wearer looks to themselves. */
const SEE_THROUGH_OPACITY = 0.3
const STONE_COLOR = new THREE.Color(0x8a8780)

type MaterialSlot = THREE.Material | THREE.Material[]

interface Swapped {
  original: MaterialSlot
  swapped: MaterialSlot
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
  sources.set(clone, material)
  return clone
}

/** Plain grey rock that keeps the surface relief. */
function stone(material: THREE.Material): THREE.Material {
  const source = material as THREE.MeshStandardMaterial
  return new THREE.MeshStandardMaterial({
    color: STONE_COLOR,
    roughness: 1,
    metalness: 0,
    normalMap: source.normalMap ?? null,
    alphaMap: source.alphaMap ?? null,
    alphaTest: source.alphaTest ?? 0,
    side: source.side,
  })
}

/** Swaps every mesh under a character to `swap(material)` while `on`. Runs
 *  per frame so gear loaded or reskinned later changes too; copies keep
 *  materials shared with others untouched. */
class MaterialSwap {
  private meshes = new Map<THREE.Mesh, Swapped>()
  // One copy per source: a fresh copy per swap compiles a new pipeline.
  private copies = new Map<THREE.Material, THREE.Material>()
  private made = new WeakSet<THREE.Material>()

  constructor(
    private swap: (material: THREE.Material) => THREE.Material,
    private castShadow: boolean
  ) {}

  private copyOf(material: THREE.Material): THREE.Material {
    if (this.made.has(material)) return material
    let copy = this.copies.get(material)
    if (!copy) {
      copy = this.swap(material)
      this.made.add(copy)
      this.copies.set(material, copy)
    }
    return copy
  }

  update(root: THREE.Object3D | undefined, on: boolean) {
    if (!on) return this.restore()
    root?.traverse((child) => {
      const mesh = child as THREE.Mesh
      if (!mesh.isMesh) return
      const known = this.meshes.get(mesh)
      if (known?.swapped === mesh.material) return
      const original = mesh.material
      const swapped = Array.isArray(original)
        ? original.map((material) => this.copyOf(material))
        : this.copyOf(original)
      this.meshes.set(mesh, {
        original,
        swapped,
        castShadow: known?.castShadow ?? mesh.castShadow,
      })
      mesh.material = swapped
      mesh.castShadow = this.castShadow && mesh.castShadow
    })
  }

  restore() {
    for (const [mesh, { original, swapped, castShadow }] of this.meshes) {
      if (mesh.material === swapped) mesh.material = original
      mesh.castShadow = castShadow
    }
    this.meshes.clear()
    for (const copy of this.copies.values()) copy.dispose()
    this.copies.clear()
  }
}

/** Fades a character to a ghost while `on`. */
export class SeeThrough extends MaterialSwap {
  constructor() {
    super(fade, false)
  }
}

/** Turns a character to stone while `on` (doc/DEBUFF.md `petrifies`). */
export class Petrify extends MaterialSwap {
  constructor() {
    super(stone, true)
  }
}

import * as THREE from 'three'

// Triangle centres (bind space, metres) the user painted gold or back to leather, and the sole colour (sRGB hex);
// the bake matches them the same way.
export type SoleEdits = {
  gold: number[][]
  leather: number[][]
  colour?: string
}
// A saved centre further than this from every triangle centre is ignored.
const MATCH = 0.004

const centre = (geometry: THREE.BufferGeometry, triangle: number) => {
  const position = geometry.getAttribute('position')
  const index = geometry.index!
  const point = new THREE.Vector3()
  const corner = new THREE.Vector3()
  for (let k = 0; k < 3; k++)
    point.add(
      corner.fromBufferAttribute(position, index.getX(triangle * 3 + k))
    )
  return point.divideScalar(3)
}

/**
 * Moves single triangles of a boots part between its sole material and the leather it was cut from, by hiding them in
 * their primitive and drawing them again in an overlay with the other material.
 */
export function femaleSolePaint(meshes: THREE.SkinnedMesh[]) {
  const sole = meshes.find((mesh) =>
    (mesh.material as THREE.Material).name.endsWith('_sole')
  )
  const leather = meshes.filter((mesh) => mesh !== sole)
  const originals = new Map(
    meshes.map((mesh) => [mesh, mesh.geometry.index!.array.slice()])
  )
  // Each boot has its own leather texture; a sole triangle returns to the leather on its side.
  const leatherFor = (x: number) =>
    leather.reduce((best, mesh) => {
      mesh.geometry.computeBoundingBox()
      const side = mesh.geometry.boundingBox!.getCenter(new THREE.Vector3()).x
      return Math.sign(side) === Math.sign(x) ? mesh : best
    }, leather[0])
  let overlays: THREE.SkinnedMesh[] = []

  const overlay = (
    source: THREE.SkinnedMesh,
    triangles: number[],
    material: THREE.Material
  ) => {
    const index = originals.get(source)!
    const geometry = new THREE.BufferGeometry()
    for (const [name, attribute] of Object.entries(
      source.geometry.attributes
    )) {
      const size = attribute.itemSize
      const values = new (attribute.array
        .constructor as Float32ArrayConstructor)(triangles.length * 3 * size)
      triangles.forEach((triangle, t) => {
        for (let k = 0; k < 3; k++) {
          const vertex = index[triangle * 3 + k]
          for (let c = 0; c < size; c++)
            values[(t * 3 + k) * size + c] = attribute.array[vertex * size + c]
        }
      })
      geometry.setAttribute(
        name,
        new THREE.BufferAttribute(values, size, attribute.normalized)
      )
    }
    const mesh = new THREE.SkinnedMesh(geometry, material)
    mesh.frustumCulled = false
    source.add(mesh)
    mesh.bind(source.skeleton, source.bindMatrix)
    return mesh
  }

  return {
    available: !!sole && leather.length > 0,
    /** Bind-space point under the cursor, on an original primitive or an overlay. */
    pointOf(hit: THREE.Intersection) {
      const mesh = hit.object as THREE.SkinnedMesh
      if (!hit.face || !hit.barycoord) return undefined
      const position = mesh.geometry.getAttribute('position')
      const corner = new THREE.Vector3()
      const point = new THREE.Vector3()
      for (const [vertex, weight] of [
        [hit.face.a, hit.barycoord.x],
        [hit.face.b, hit.barycoord.y],
        [hit.face.c, hit.barycoord.z],
      ])
        point.addScaledVector(
          corner.fromBufferAttribute(position, vertex),
          weight
        )
      return point
    },
    /** Centres of the part's triangles within `radius` of a bind-space point. */
    centresNear(point: THREE.Vector3, radius: number) {
      const found: THREE.Vector3[] = []
      for (const [mesh, index] of originals) {
        const geometry = mesh.geometry
        const saved = geometry.index!.array.slice()
        geometry.index!.array.set(index)
        for (let t = 0; t < index.length / 3; t++) {
          const c = centre(geometry, t)
          if (c.distanceTo(point) <= radius) found.push(c)
        }
        geometry.index!.array.set(saved)
      }
      return found
    },
    targets: () => [...meshes, ...overlays],
    colour: () =>
      sole &&
      `#${(sole.material as THREE.MeshStandardMaterial).color.getHexString()}`,
    apply(edits: SoleEdits) {
      if (sole && edits.colour)
        (sole.material as THREE.MeshStandardMaterial).color.set(edits.colour)
      for (const mesh of overlays) {
        mesh.removeFromParent()
        mesh.geometry.dispose()
      }
      overlays = []
      if (!sole) return
      for (const [mesh, index] of originals) {
        mesh.geometry.index!.array.set(index)
        mesh.geometry.index!.needsUpdate = true
      }
      const triangles = meshes.flatMap((mesh) =>
        Array.from({ length: originals.get(mesh)!.length / 3 }, (_, t) => ({
          mesh,
          triangle: t,
          centre: centre(mesh.geometry, t),
        }))
      )
      const moves = new Map<
        THREE.SkinnedMesh,
        Map<THREE.SkinnedMesh, number[]>
      >()
      for (const [wanted, list] of [
        ['gold', edits.gold],
        ['leather', edits.leather],
      ] as const)
        for (const values of list) {
          const point = new THREE.Vector3().fromArray(values)
          let best: (typeof triangles)[number] | undefined
          let distance = MATCH
          for (const candidate of triangles) {
            const d = candidate.centre.distanceTo(point)
            if (d < distance) [best, distance] = [candidate, d]
          }
          if (!best || (best.mesh === sole) === (wanted === 'gold')) continue
          const target = wanted === 'gold' ? sole : leatherFor(best.centre.x)
          const bySource = moves.get(best.mesh) ?? new Map()
          moves.set(best.mesh, bySource)
          bySource.set(target, [...(bySource.get(target) ?? []), best.triangle])
        }
      for (const [source, bySource] of moves) {
        const index = source.geometry.index!
        for (const [target, list] of bySource) {
          for (const triangle of list)
            for (let k = 1; k < 3; k++)
              index.setX(triangle * 3 + k, index.getX(triangle * 3))
          overlays.push(
            overlay(source, list, target.material as THREE.Material)
          )
        }
        index.needsUpdate = true
      }
    },
  }
}

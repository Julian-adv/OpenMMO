import * as THREE from 'three'
import { MeshStandardNodeMaterial } from 'three/webgpu'
import { mix, step, texture, uniform, uv } from 'three/tsl'
import { getItemDef } from '../data/itemDefs'
import { loadPrint } from './familyEmblem'

export const TABARD_PART = 'tabard_guard'
const TABARD_MODELS = new Set(['armor/guard_tabard.glb'])

/** A tabard's cloth colour and the URL of the picture on its chest. */
export interface TabardSkin {
  color: string
  texture?: string | null
}

/** A back item worn as a fitted tabard rather than a hanging cape. */
export function isTabard(itemDefId: string | null | undefined): boolean {
  return TABARD_MODELS.has(
    (itemDefId && getItemDef(itemDefId)?.worldModel) || ''
  )
}

/** The cloth colour a worn tabard shows and the colour its texture was made
 *  in, or null when the back item is not a tabard. */
export function wornTabard(
  itemDefId: string | null | undefined,
  dye?: string | null,
  texture?: string | null
): { skin: TabardSkin; undyed: string } | null {
  if (!isTabard(itemDefId)) return null
  const undyed = getItemDef(itemDefId ?? '')?.capeColor ?? '#ffffff'
  return { skin: { color: dye ?? undyed, texture }, undyed }
}

const tintUniform = () => uniform(new THREE.Color(1, 1, 1))

type Dressed = {
  material: MeshStandardNodeMaterial
  tint: ReturnType<typeof tintUniform>
  print?: string | null
}

// The part's second UV set: 0..1 is the chest picture, u -10 other cloth and
// -20 the belt, which takes neither dye nor picture.
function dress(mesh: THREE.SkinnedMesh, undyed: THREE.Color): Dressed {
  const source = mesh.material as THREE.MeshStandardMaterial
  const material = new MeshStandardNodeMaterial({
    map: source.map,
    normalMap: source.normalMap,
    roughnessMap: source.roughnessMap,
    metalnessMap: source.metalnessMap,
    roughness: source.roughness,
    metalness: source.metalness,
    side: source.side,
  })
  const tint = tintUniform()
  const dressed: Dressed = { material, tint }
  mesh.userData.tabard = dressed
  mesh.userData.tabard_undyed = undyed
  mesh.material = material
  return dressed
}

function colour(dressed: Dressed, map: THREE.Texture, print?: THREE.Texture) {
  const base = texture(map).rgb
  const spot = uv(1)
  const dyed = mix(base, base.mul(dressed.tint), step(-15, spot.x))
  if (!print) return dyed
  const picture = texture(print, spot)
  const inside = step(0, spot.x)
    .mul(step(spot.x, 1))
    .mul(step(0, spot.y))
    .mul(step(spot.y, 1))
  return mix(dyed, picture.rgb, picture.a.mul(inside))
}

/** Dye the tabard and put the picture on its chest. `undyed` is the colour
 *  its texture was made in. */
export function skinTabard(
  meshes: THREE.SkinnedMesh[],
  skin: TabardSkin,
  undyed: string
) {
  for (const mesh of meshes) {
    const dressed: Dressed =
      mesh.userData.tabard ?? dress(mesh, new THREE.Color(undyed))
    const base: THREE.Color = mesh.userData.tabard_undyed
    const dye = new THREE.Color(skin.color)
    dressed.tint.value.setRGB(dye.r / base.r, dye.g / base.g, dye.b / base.b)
    const map = dressed.material.map
    if (!map || (dressed.print === skin.texture && dressed.material.colorNode))
      continue
    dressed.print = skin.texture
    dressed.material.colorNode = colour(dressed, map)
    dressed.material.needsUpdate = true
    const wanted = skin.texture
    if (wanted)
      loadPrint(wanted)
        .then((print) => {
          if (dressed.print !== wanted) return
          dressed.material.colorNode = colour(dressed, map, print)
          dressed.material.needsUpdate = true
        })
        // A blocked or missing picture leaves the colour, as on capes.
        .catch(() => {})
  }
}

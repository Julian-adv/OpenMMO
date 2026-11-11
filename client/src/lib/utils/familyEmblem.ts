import * as THREE from 'three'
import { MeshStandardNodeMaterial } from 'three/webgpu'
import { color, float, frontFacing, mix, texture } from 'three/tsl'
import { capeTextureUrl } from './networkUtils'

/** Shared `family::Emblem`: a cape texture hash and a `#rrggbb` colour. */
export interface Emblem {
  texture: string | null
  color: string | null
}

/** Bare linen for a banner whose estate has left its family. */
const UNMARKED = 0xd8ccb0
const EMBLEM_DEFAULT = 0x6d1720

const loader = new THREE.TextureLoader()
const prints = new Map<string, Promise<THREE.Texture>>()

function loadPrint(url: string) {
  let pending = prints.get(url)
  if (!pending) {
    pending = loader.loadAsync(url).then((map) => {
      map.colorSpace = THREE.SRGBColorSpace
      // The cloth UVs come from glTF, whose v runs top-down.
      map.flipY = false
      map.needsUpdate = true
      return map
    })
    prints.set(url, pending)
    pending.catch(() => prints.delete(url))
  }
  return pending
}

/** Cloth for a family banner: the emblem picture over its colour on the
 *  front, the colour alone behind, as on a cape. The caller disposes it. */
export function emblemClothMaterial(emblem: Emblem | null | undefined) {
  const tint = new THREE.Color(
    emblem ? (emblem.color ?? EMBLEM_DEFAULT) : UNMARKED
  )
  const material = new MeshStandardNodeMaterial({
    color: tint,
    roughness: 0.8,
    metalness: 0,
    side: THREE.DoubleSide,
  })
  const url = capeTextureUrl(emblem?.texture)
  if (url) {
    loadPrint(url)
      .then((map) => {
        const print = texture(map)
        material.colorNode = mix(
          color(tint),
          print.rgb,
          print.a.mul(float(frontFacing))
        )
        material.needsUpdate = true
      })
      // A blocked or missing picture leaves the colour, as on capes.
      .catch(() => {})
  }
  return material
}

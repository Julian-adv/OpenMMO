import { loadGLB } from './gltfCache'
import {
  buildShopSignBoard,
  getShopSignStyle,
  type ShopSignStyleId,
} from './shop-sign'
import type { EstateFurniturePlacementDefinition } from '../terrain/estatePlacement'
import { houseFloorY } from '../terrain/estatePlacement'
import type { HouseData } from '../types/housing'
import type * as THREE from 'three'
import type { EstateChest } from '../network/networkTypes'
import { getEstateStorageDef } from '../data/estateFurnitureDefs'
import { getObjectDef } from '../data/objectCatalog'

export function estateFurnitureRenderY(
  chest: EstateChest,
  houses: HouseData[]
) {
  if (chest.item_def_id !== 'furniture_hearthbound_rug') return chest.position.y
  for (const house of houses) {
    const floorY = houseFloorY(
      house,
      chest.floor_level,
      chest.position.x,
      chest.position.z
    )
    if (floorY !== null) return Math.max(chest.position.y, floorY)
  }
  return chest.position.y
}

export function estateFurnitureInteractionData(chest: EstateChest) {
  const definition = getEstateStorageDef(chest.item_def_id)
  const model = getObjectDef(definition?.modelId)
  if (!model?.interaction) return {}
  return {
    objectId: chest.id,
    objectType: chest.item_def_id,
    objectInteraction: model.interaction,
    objectInteractOffset: model.interactOffset,
  }
}

const models = new Map<
  string,
  Promise<{ scene: THREE.Group; animations: THREE.AnimationClip[] }>
>()

export function loadEstateFurnitureModel(
  definition: EstateFurniturePlacementDefinition
) {
  const key = definition.modelId ?? definition.modelUrl
  let pending = models.get(key)
  if (!pending) {
    const model = getObjectDef(definition.modelId)
    pending =
      model?.procedural === 'shopSign'
        ? Promise.resolve({
            scene: buildShopSignBoard(
              getShopSignStyle(model.shopSignStyle as ShopSignStyleId).board
            ),
            animations: [],
          })
        : loadGLB(definition.modelUrl)
    models.set(key, pending)
    void pending.catch(() => models.delete(key))
  }
  return pending
}

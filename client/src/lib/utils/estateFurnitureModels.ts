import { loadGLB } from './gltfCache'
import {
  buildShopSignBoard,
  getShopSignStyle,
  type ShopSignStyleId,
} from './shop-sign'
import type { EstateFurniturePlacementDefinition } from '../terrain/estatePlacement'
import type * as THREE from 'three'
import type { EstateChest } from '../network/networkTypes'
import { getEstateStorageDef } from '../data/estateFurnitureDefs'
import { getObjectDef } from '../data/objectCatalog'

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

import catalog from '../../../public/models/objects/catalog.json'
import { loadGLB } from './gltfCache'
import {
  buildShopSignBoard,
  getShopSignStyle,
  type ShopSignStyleId,
} from './shop-sign'
import type { EstateFurniturePlacementDefinition } from '../terrain/estatePlacement'
import type { ObjectDef } from '../stores/editorStore'
import type * as THREE from 'three'
import type { EstateChest } from '../network/networkTypes'
import { getEstateStorageDef } from '../data/estateFurnitureDefs'

export function furnitureModelDefinition(modelId: string | undefined) {
  return (catalog as ObjectDef[]).find((def) => def.id === modelId)
}

export function estateFurnitureInteractionData(chest: EstateChest) {
  const definition = getEstateStorageDef(chest.item_def_id)
  const model = furnitureModelDefinition(definition?.modelId)
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
    const model = furnitureModelDefinition(definition.modelId)
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

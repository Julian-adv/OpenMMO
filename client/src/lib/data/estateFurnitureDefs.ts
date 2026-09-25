import estateStorageJson from '../../../../data/estate_storage.json'
import catalog from '../../../public/models/objects/catalog.json'
import footprints from '../../../../data/furniture_footprints.json'
import type { EstateFurniturePlacementDefinition } from '../terrain/estatePlacement'
import { getObjectModelPath } from '../utils/modelPaths'

interface EstateStorageData {
  id: string
  modelId: string
  capacityKg: number
  snapStep: number
  rotationStep: number
  footprintWidth: number
  footprintDepth: number
  minFloor: number
  maxFloor: number
  floorEdgeClearance: number
  indoorCollisionRadius: number
  outdoorCollisionRadius: number
  maxHeightOffset: number
  textLabel: boolean
}

export interface EstateStorageDefinition extends EstateFurniturePlacementDefinition {
  itemDefId: string
  modelId: string
  capacityKg: number
  textLabel: boolean
}

export const estateStorageDefs = new Map<string, EstateStorageDefinition>(
  Object.values(estateStorageJson as Record<string, EstateStorageData>).map(
    (data) => [
      data.id,
      {
        itemDefId: data.id,
        modelId: data.modelId,
        capacityKg: data.capacityKg,
        modelUrl: getObjectModelPath(
          catalog.find((def) => def.id === data.modelId)?.model ??
            `objects/${data.modelId}.glb`
        ),
        maxHeightOffset: data.maxHeightOffset,
        textLabel: data.textLabel,
        solid: data.modelId in footprints,
        snapStep: data.snapStep,
        rotationStep: data.rotationStep,
        footprint: {
          width: data.footprintWidth,
          depth: data.footprintDepth,
          ...((
            footprints as Record<
              string,
              { minX: number; maxX: number; minZ: number; maxZ: number }
            >
          )[data.modelId] ?? {}),
        },
        floorEdgeClearance: data.floorEdgeClearance,
        minFloor: data.minFloor,
        maxFloor: data.maxFloor,
      },
    ]
  )
)

export function getEstateStorageDef(itemDefId: string | null | undefined) {
  return itemDefId ? estateStorageDefs.get(itemDefId) : undefined
}

export function isEstateStorageItem(itemDefId: string) {
  return estateStorageDefs.has(itemDefId)
}

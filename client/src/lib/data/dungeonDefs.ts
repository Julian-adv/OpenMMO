/** Entrance metadata; shared WASM generates layouts from the source CSV. */
import dungeonsJson from '../../../../data/dungeons.json'

export interface DungeonEntranceDef {
  id: string
  name: string
  x: number
  y: number
  z: number
  /** Semicolon-separated guaranteed chest drops. */
  chestDrops?: string
  /** Layout settings are embedded separately in shared WASM. */
  floors?: number
  boss?: string
  chestTier?: number
  entranceDir?: string
  keyPrefix?: string
  spawnGroup?: string
}

export const DUNGEON_ENTRANCES: DungeonEntranceDef[] = Object.values(
  dungeonsJson as Record<string, DungeonEntranceDef>
)

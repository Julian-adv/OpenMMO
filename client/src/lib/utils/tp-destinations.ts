/** Named admin destinations, including generated dungeon and city locations. */
import { translate } from '../i18n'
import { dungeon_layout } from '../wasm/onlinerpg_shared'
import {
  dungeonCellCenter,
  type DungeonFloorLayout,
} from '../managers/dungeonManager'
import { DUNGEON_ENTRANCES } from '../data/dungeonDefs'
import { MAP_LABELS, type MapLabelKind } from '../data/mapLabels'
import worldJson from '../../../../data-src/world.json'

export interface TpDestination {
  name: string
  label: string
  x: number
  y: number
  z: number
}

/** Surface entries use y=0: the client snaps to terrain height on arrival. */
const STATIC_DESTINATIONS: TpDestination[] = [
  {
    name: 'spawn',
    get label() {
      return translate('command.tpSpawn')
    },
    x: worldJson.spawnPosition.x,
    y: 0,
    z: worldJson.spawnPosition.z,
  },
  {
    name: 'snowpeak',
    get label() {
      return translate('command.tpSnowpeak')
    },
    x: -1078,
    y: 0,
    z: 5067,
  },
]

const CITY_KINDS = new Set<MapLabelKind>(['capital', 'city', 'town'])

let cached: TpDestination[] | null = null

export function tpDestinations(): TpDestination[] {
  if (cached) return cached

  const out = [...STATIC_DESTINATIONS]

  for (const e of DUNGEON_ENTRANCES) {
    const short = e.id.split('_').pop() ?? e.id
    out.push({
      name: short,
      get label() {
        return translate('command.tpEntrance', { name: e.name })
      },
      x: e.x,
      y: 0,
      z: e.z,
    })

    const layouts = dungeon_layout(e.id) as DungeonFloorLayout[]
    const last = layouts[layouts.length - 1]
    const boss = last?.spawns.find((s) => s.isBoss)
    if (!last || !boss) continue
    const depth = last.depth
    out.push({
      name: `${short}-boss`,
      get label() {
        return translate('command.tpBoss', { name: e.name, depth })
      },
      ...dungeonCellCenter(e, depth, boss),
    })
  }

  for (const label of MAP_LABELS) {
    // Aldermark duplicates the spawn entry.
    if (!CITY_KINDS.has(label.kind) || label.id === 'aldermark') continue
    out.push({
      name: label.id,
      get label() {
        return translate('command.tpCity', {
          name: label.name,
          kind:
            label.kind === 'capital'
              ? translate('command.tpKind.capital')
              : label.kind === 'city'
                ? translate('command.tpKind.city')
                : translate('command.tpKind.town'),
        })
      },
      x: label.x,
      y: 0,
      z: label.z,
    })
  }

  cached = out
  return out
}

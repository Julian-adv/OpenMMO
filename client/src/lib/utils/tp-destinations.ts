/** Named admin destinations, including generated dungeon and city locations. */
import { translate } from '../i18n'
import { dungeonAlias, dungeonShortNames } from './tp-args'
import { dungeon_layout } from '../wasm/onlinerpg_shared'
import {
  dungeonCellCenter,
  dungeonFloorLanding,
  type DungeonFloorLayout,
} from '../managers/dungeonManager'
import { DUNGEON_ENTRANCES, type DungeonEntranceDef } from '../data/dungeonDefs'
import { MAP_LABELS, type MapLabelKind } from '../data/mapLabels'
import worldJson from '../../../../data-src/world.json'

export interface TpDestination {
  name: string
  label: string
  dungeonId?: string
  boss?: boolean
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

export function tpDestinations(): TpDestination[] {
  const out = [...STATIC_DESTINATIONS]

  const shortNames = dungeonShortNames(DUNGEON_ENTRANCES.map((e) => e.id))
  for (const [i, e] of DUNGEON_ENTRANCES.entries()) {
    const short = shortNames[i]
    const alias = dungeonAlias(i)
    out.push({
      name: `${short}/${alias}`,
      dungeonId: e.id,
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
      name: `${short}-boss/${alias}-boss`,
      dungeonId: e.id,
      boss: true,
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

  return out
}

/** The landing of `entrance`'s floor `depth`; null when it has no such floor. */
export function dungeonFloorDestination(
  entrance: DungeonEntranceDef,
  depth: number
) {
  const layouts = dungeon_layout(entrance.id) as DungeonFloorLayout[]
  const floor = layouts.find((l) => l.depth === depth)
  return floor ? dungeonFloorLanding(entrance, floor) : null
}

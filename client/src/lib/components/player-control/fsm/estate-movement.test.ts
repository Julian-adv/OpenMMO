import { readFileSync } from 'node:fs'
import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest'
import { initSync, passability_find_path } from '../../../wasm/onlinerpg_shared'
import {
  applyEstateChestVisibility,
  resetEstateStorage,
} from '../../../stores/estateStorageStore'
import {
  DEFAULT_MOVEMENT_CONFIG,
  type Position,
} from '../../../utils/movementUtils'
import { startClickMovement } from './move-request'
import { stepMovementSubstrate } from './movement-substrate'
import { TerrainHeightManager } from '../../../managers/terrainHeightManager'
import { createPlayerPhysics } from '../player-physics'

const origin = { x: -1373.8974609375, y: 0.6809865832328796, z: 4477.103515625 }
const goal = { x: -1382.1763916015625, y: origin.y, z: 4473.5732421875 }
const chests = [
  [-1374.5, 0.71250152587890625, 4477.5],
  [-1374.5, 0.649993896484375, 4476],
  [-1373, 0.625, 4475.5],
  [-1373, 0.70001220703125, 4477],
].map(([x, y, z], id) => ({
  id,
  estate_id: 1,
  owner_id: 1,
  item_def_id: 'storage_chest',
  position: { x, y, z },
  rotation_deg: 90,
  floor_level: 0,
  overdue: false,
  revision: 0,
}))

const pathing = {
  currentFloor: 0,
  getFloorAt: () => 0,
  findPath: passability_find_path,
  waypointHeight: () => origin.y,
}

const heightManager = new TerrainHeightManager()
const { isMovementBlocked } = createPlayerPhysics({
  getHeightManager: () => heightManager,
  getCurrentPlayerY: () => origin.y,
  getFloorOffset: () => 0,
  getPassabilityFloor: () => 0,
})

describe('September 14 estate storage movement', () => {
  beforeAll(() => {
    initSync({
      module: readFileSync(
        new URL('../../../wasm/onlinerpg_shared_bg.wasm', import.meta.url)
      ),
    })
  })
  beforeEach(() => applyEstateChestVisibility(chests, []))
  afterEach(resetEstateStorage)

  it('rejects the occupied chest goal and routes a new goal around all four chests', () => {
    const sendPlayerMove = vi.fn()
    const input = {
      ...pathing,
      currentPos: origin,
      startSpeed: 0,
      sendPlayerMove,
    }
    expect(
      startClickMovement({ ...input, clickPosition: chests[0].position })
    ).toBeNull()
    expect(sendPlayerMove).not.toHaveBeenCalled()
    expect(
      isMovementBlocked(origin.x, origin.z, goal.x, goal.z, origin.y)
    ).toBe(true)

    const started = startClickMovement({ ...input, clickPosition: goal })
    expect(started).not.toBeNull()
    if (!started) throw new Error('Missing estate detour')
    expect(started.pathWaypoints.length).toBeGreaterThan(1)
    let position: Position = { ...origin }
    let arrived = false
    for (let frame = 0; frame < 1000 && !arrived; frame++) {
      const outcome = stepMovementSubstrate({
        ...started,
        currentPos: position,
        config: DEFAULT_MOVEMENT_CONFIG,
        deltaTimeSeconds: 0.016,
        sampleHeight: () => origin.y,
        waypointHeight: pathing.waypointHeight,
        isMovementBlocked,
        isUphillTooSteep: () => false,
        setFloorLevel: () => {},
        writePlayerPosition: (next) => {
          position = next
        },
        sendPlayerMove,
      })
      expect(outcome.kind).not.toBe('blocked')
      if (outcome.kind === 'next_waypoint') Object.assign(started, outcome)
      arrived = outcome.kind === 'arrived'
    }
    expect(arrived).toBe(true)
    expect(position).toEqual(goal)
  })

  it('allows walking out when a chest overlaps the player on the surface', () => {
    const from = chests[0].position
    expect(isMovementBlocked(from.x, from.z, from.x, from.z + 1, from.y)).toBe(
      false
    )
  })

  it('removes collision when the visible chests are removed', () => {
    applyEstateChestVisibility(
      [],
      chests.map((chest) => chest.id)
    )
    expect(
      isMovementBlocked(origin.x, origin.z, goal.x, goal.z, origin.y)
    ).toBe(false)
    expect(
      passability_find_path(origin.x, origin.z, 0, goal.x, goal.z, 0).waypoints
    ).toEqual([{ x: goal.x, z: goal.z, floor: 0 }])
  })
})

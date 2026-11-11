import { beforeEach, describe, expect, it, vi } from 'vitest'
import { remotePlayerManager } from './remotePlayerManager'

vi.mock('../wasm/onlinerpg_shared', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  passability_storey_ground_y: () => NaN,
}))

const ID = 1

function positionOf() {
  return remotePlayerManager.players.get(ID)!.position
}

/** Steps the loop until the remote stops, recording where it went. */
function walk(maxFrames = 600) {
  const path = [{ ...positionOf() }]
  for (let i = 0; i < maxFrames; i++) {
    remotePlayerManager.update(16)
    path.push({ ...positionOf() })
    if (remotePlayerManager.players.get(ID)!.state === 'idle') break
  }
  return path
}

// Corridor turning a corner: east along z=0, then north along x=2.
const AROUND_CORNER = [
  { x: 1, y: 0, z: 0 },
  { x: 2, y: 0, z: 0 },
  { x: 2, y: 0, z: 1 },
  { x: 2, y: 0, z: 2 },
]

function sendCorner() {
  for (const p of AROUND_CORNER) remotePlayerManager.setTargetPosition(ID, p, 0)
}

/** The diagonal cut a straight line from (0,0) to (2,2) would make. */
function cutsCorner(path: { x: number; z: number }[]) {
  return path.some((p) => p.x < 1.9 && p.z > 0.1)
}

describe('remote movement received during a clip', () => {
  beforeEach(() => {
    remotePlayerManager.reset()
    remotePlayerManager.initPlayer(ID, { x: 0, y: 0, z: 0 }, 0)
  })

  it('replays the corner after an attack instead of cutting through it', () => {
    remotePlayerManager.handleAttack(ID)
    sendCorner()
    remotePlayerManager.attackAnimationDuration = 0

    const path = walk()

    expect(cutsCorner(path)).toBe(false)
    expect(positionOf().x).toBeCloseTo(2)
    expect(positionOf().z).toBeCloseTo(2)
  })

  it('replays the corner after a pickup crouch', () => {
    remotePlayerManager.handleInteraction(ID, 'pickup', 0)
    sendCorner()

    const path = walk()

    expect(cutsCorner(path)).toBe(false)
    expect(positionOf().z).toBeCloseTo(2)
  })

  it('drops the trail on a teleport', () => {
    remotePlayerManager.handleInteraction(ID, 'pickup', 0)
    sendCorner()
    remotePlayerManager.teleportPlayer(ID, { x: 50, y: 0, z: 50 }, 0)

    walk()

    expect(positionOf()).toMatchObject({ x: 50, z: 50 })
  })
})

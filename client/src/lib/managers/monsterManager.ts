import { SvelteMap } from 'svelte/reactivity'
import * as THREE from 'three'
import { networkManager } from '../network/socket'
import { get } from 'svelte/store'
import { gameStore } from '../stores/gameStore'
import { remotePlayerManager } from './remotePlayerManager'
import type { MonsterData } from '../types/Monster'
import { getMonsterDef } from '../data/monsterDefs'
import type { Position } from '../utils/movementUtils'
import type { TerrainHeightManager } from './terrainHeightManager'
import { findPath } from './pathfinding'
import {
  passability_get_floor_at,
  passability_get_floor_y_base,
} from '../wasm/onlinerpg_shared'

const MIN_MOVE_DIST = 2.0
const MAX_MOVE_DIST = 10.0
const FLEE_HEALTH_RATIO = 0.3
const DEFAULT_FLEE_CHANCE = 0.5
const DEFAULT_RETURN_CHANCE = 0.7
const FLEE_DURATION_MS = 3000
const RETURN_ARRIVE_DIST = 5.0
const LEASH_RANGE = 50.0

class MonsterManager {
  monsters = new SvelteMap<string, MonsterData>()
  heightManager: TerrainHeightManager | null = null

  private sampleHeight(x: number, z: number): number {
    return this.heightManager?.getHeightAtWorldPosition(x, z) ?? 0
  }

  findMeshPosition(
    monsterId: string,
    meshes: THREE.Group[]
  ): Position | undefined {
    for (const group of meshes) {
      if (group) {
        let found = false
        group.traverse((child) => {
          if (child.userData.monsterId === monsterId) {
            found = true
          }
        })
        if (found) {
          return {
            x: group.position.x,
            y: group.position.y,
            z: group.position.z,
          }
        }
      }
    }
    return undefined
  }

  private timeSinceLastSpawn = 0
  private readonly SPAWN_INTERVAL = 10000 // 10 seconds

  spawnWithId(
    id: string,
    type: MonsterData['type'],
    position: { x: number; y: number; z: number },
    ownerId?: string,
    health?: number,
    maxHealth?: number
  ) {
    if (this.monsters.has(id)) return

    const def = getMonsterDef(type)
    const hp = health ?? def?.health ?? 10
    const maxHp = maxHealth ?? def?.health ?? 10

    this.monsters.set(id, {
      id,
      type,
      position,
      rotation: 0,
      state: 'idle',
      ownerId,
      moveSpeed: def?.walkSpeed ?? 1,
      stateTimer: 0,
      health: hp,
      maxHealth: maxHp,
      spawnPosition: { ...position },
    })
  }

  remove(id: string) {
    this.monsters.delete(id)
  }

  handleMonsterDead(id: string) {
    const monster = this.monsters.get(id)
    if (monster) {
      monster.pathState = undefined
      // If we are waiting for an impact, delay the visual death
      if (monster.impactDelay && monster.impactDelay > 0) {
        monster.isDeadPending = true
      } else {
        // Otherwise die immediately
        monster.state = 'dead'
        monster.stateTimer = 0
      }
      this.monsters.set(id, { ...monster })
    }
  }

  handleMonsterAttacked(
    monsterId: string,
    playerId: string,
    hit: boolean,
    damage: number
  ) {
    const monster = this.monsters.get(monsterId)
    if (!monster || monster.state === 'dead') return

    // Set impact delay (e.g., 400ms for player's slash to land)
    monster.impactDelay = 540
    monster.targetPlayerId = playerId
    monster.isLastHitSuccess = hit
    // Temporarily store damage to show at impact
    monster.pendingDamage = damage

    const gameState = get(gameStore)
    const myPlayerId = gameState.currentPlayer?.id

    // Only respond with state changes if we own this monster
    if (monster.ownerId === myPlayerId) {
      // We will transition to 'hit' in the update loop after impactDelay
      // but we can set the targetPlayerId now to ensure retaliation
      const def = getMonsterDef(monster.type)
      monster.moveSpeed = def?.runSpeed ?? 8
    }

    // Trigger reactivity
    this.monsters.set(monsterId, { ...monster })
  }

  reset() {
    this.monsters.clear()
    this.timeSinceLastSpawn = 0
  }

  update(
    deltaTime: number,
    playerPosition: { x: number; y: number; z: number } | null
  ) {
    // 1. Spawning Logic
    if (playerPosition) {
      this.timeSinceLastSpawn += deltaTime
      if (this.timeSinceLastSpawn >= this.SPAWN_INTERVAL) {
        this.timeSinceLastSpawn = 0
        this.spawnRandomMonster(playerPosition)
      }
    }

    // 2. FSM & Movement Logic
    const gameState = get(gameStore)
    const myPlayerId = gameState.currentPlayer?.id

    for (const monster of this.monsters.values()) {
      // Keep monster Y aligned with terrain height
      const terrainY = this.sampleHeight(monster.position.x, monster.position.z)
      if (Math.abs(monster.position.y - terrainY) > 0.001) {
        monster.position = { ...monster.position, y: terrainY }
      }

      let impactJustExpired = false

      // Impact Delay Handling (Global for all clients to keep visuals synced)
      if (monster.impactDelay !== undefined && monster.impactDelay > 0) {
        monster.impactDelay -= deltaTime
        if (monster.impactDelay <= 0) {
          monster.impactDelay = 0
          impactJustExpired = true

          // Trigger damage display only for local player's attacks
          if (monster.targetPlayerId === myPlayerId) {
            monster.lastDamageInfo = {
              damage: monster.pendingDamage || 0,
              hit: !!monster.isLastHitSuccess,
              trigger: (monster.lastDamageInfo?.trigger || 0) + 1,
            }
          }

          if (monster.isDeadPending) {
            // Death impact!
            monster.state = 'dead'
            monster.stateTimer = 0
            monster.isDeadPending = false
          } else if (monster.isLastHitSuccess) {
            // Normal hit impact - stagger then retaliate (or flee)
            monster.state = 'hit'
            monster.stateTimer = 0
            monster.movementIntent = undefined
            // Force immediate update to network if owner
            if (monster.ownerId === myPlayerId) {
              networkManager.sendMonsterMove(
                monster.id,
                monster.position,
                monster.rotation,
                'hit',
                monster.position
              )
            }
          } else if (monster.targetPlayerId && monster.state !== 'attack') {
            // Miss - skip stagger; flee if low health (probabilistic), else retaliate
            const def = getMonsterDef(monster.type)
            const fleeRatio = def?.fleeHealthRatio ?? FLEE_HEALTH_RATIO
            const fleeChance = def?.fleeChance ?? DEFAULT_FLEE_CHANCE
            const shouldFlee =
              monster.health <= monster.maxHealth * fleeRatio &&
              Math.random() < fleeChance
            if (shouldFlee && monster.ownerId === myPlayerId) {
              this.transitionToFlee(monster)
            } else {
              monster.state = 'attack'
              monster.stateTimer = 0
              if (monster.ownerId === myPlayerId) {
                networkManager.sendMonsterMove(
                  monster.id,
                  monster.position,
                  monster.rotation,
                  'attack',
                  monster.position
                )
              }
            }
          }
        }
      }

      // Only control monsters that YOU own
      if (monster.ownerId === myPlayerId) {
        // Guard: If dead or about to die, stop AI immediately
        if (monster.state === 'dead' || monster.isDeadPending) {
          // Keep reactivity
          this.monsters.set(monster.id, { ...monster })
          continue
        }

        this.updateMonsterAI(monster, deltaTime)
        // Trigger reactivity with new reference
        this.monsters.set(monster.id, { ...monster })
      } else {
        // Interpolate remote monsters
        if (
          monster.state !== 'dead' &&
          !monster.isDeadPending &&
          (monster.state === 'walk' ||
            monster.state === 'run' ||
            monster.state === 'attack') &&
          monster.targetPosition
        ) {
          this.moveTowards(monster, monster.targetPosition, deltaTime)
          // Trigger reactivity with new reference
          this.monsters.set(monster.id, { ...monster })
        } else if (impactJustExpired) {
          // Impact delay expired and caused a state change (e.g., dead, hit).
          // Non-moving states don't call set() above, so trigger reactivity here
          // so the animation updates (e.g., attack → dead/hit).
          this.monsters.set(monster.id, { ...monster })
        }
      }
    }
  }

  private updateMonsterAI(monster: MonsterData, deltaTime: number) {
    monster.stateTimer += deltaTime

    switch (monster.state) {
      case 'dead':
        // No AI for dead monsters, just wait for removal
        break

      case 'hit': {
        // Wait for stagger animation to finish (approx 800ms)
        if (monster.stateTimer >= 800) {
          const def = getMonsterDef(monster.type)
          const fleeRatio = def?.fleeHealthRatio ?? FLEE_HEALTH_RATIO
          const fleeChance = def?.fleeChance ?? DEFAULT_FLEE_CHANCE
          const shouldFlee =
            monster.health <= monster.maxHealth * fleeRatio &&
            Math.random() < fleeChance
          if (shouldFlee) {
            this.transitionToFlee(monster)
          } else {
            monster.state = 'attack'
            monster.stateTimer = 0
            networkManager.sendMonsterMove(
              monster.id,
              monster.position,
              monster.rotation,
              'attack',
              monster.position
            )
          }
        }
        break
      }

      case 'idle':
        // 1 second interval check
        if (monster.stateTimer >= 1000) {
          monster.stateTimer = 0
          // 30% chance to move
          if (Math.random() < 0.3) {
            this.transitionToMove(monster)
          }
        }
        break

      case 'walk':
      case 'run': {
        if (monster.movementIntent === 'flee') {
          this.tickFlee(monster, deltaTime)
          break
        }
        if (monster.movementIntent === 'return') {
          this.tickReturn(monster, deltaTime)
          break
        }
        if (monster.targetPosition) {
          const reached = this.followPath(monster, deltaTime)

          if (reached) {
            // 50% Idle, 50% Move again
            if (Math.random() < 0.5) {
              this.transitionToIdle(monster)
            } else {
              this.transitionToMove(monster)
            }
          }
        } else {
          this.transitionToIdle(monster)
        }
        break
      }

      case 'attack':
        if (monster.targetPlayerId) {
          const gameState = get(gameStore)
          let targetPlayer:
            | { position: { x: number; y: number; z: number }; health?: number }
            | undefined

          if (gameState.currentPlayer?.id === monster.targetPlayerId) {
            targetPlayer = {
              position: {
                x: gameState.currentPlayer.position.x,
                y: gameState.currentPlayer.position.y,
                z: gameState.currentPlayer.position.z,
              },
              health: gameState.currentPlayer.health,
            }
          } else {
            const remotePlayerState = remotePlayerManager.players.get(
              monster.targetPlayerId
            )
            const remotePlayer = gameState.otherPlayers.get(
              monster.targetPlayerId
            )

            if (remotePlayerState) {
              targetPlayer = {
                position: remotePlayerState.position,
                health: remotePlayer?.health,
              }
            }
          }

          // Stop attacking if target is dead
          if (
            targetPlayer &&
            targetPlayer.health !== undefined &&
            targetPlayer.health <= 0
          ) {
            monster.targetPlayerId = undefined
            this.transitionToReturn(monster)
            return
          }

          if (targetPlayer) {
            const def = getMonsterDef(monster.type)
            const dx = targetPlayer.position.x - monster.position.x
            const dz = targetPlayer.position.z - monster.position.z
            const distSq = dx * dx + dz * dz
            const attackRange = def?.attackRange ?? 2
            const ATTACK_RANGE_SQ = attackRange * attackRange
            const chaseRange = def?.chaseRange ?? 25
            const CHASE_RANGE_SQ = chaseRange * chaseRange

            // Leash: return to spawn if too far from home
            if (monster.spawnPosition) {
              const spawnDx = monster.position.x - monster.spawnPosition.x
              const spawnDz = monster.position.z - monster.spawnPosition.z
              if (
                spawnDx * spawnDx + spawnDz * spawnDz >
                LEASH_RANGE * LEASH_RANGE
              ) {
                monster.targetPlayerId = undefined
                this.transitionToReturn(monster)
                return
              }
            }

            if (distSq > CHASE_RANGE_SQ) {
              // Target too far, return to spawn
              monster.targetPlayerId = undefined
              this.transitionToReturn(monster)
              return
            }

            // Look at player
            monster.rotation = Math.atan2(dx, dz)

            if (distSq <= ATTACK_RANGE_SQ) {
              // Within range - wait for attack animation/cooldown
              if (monster.stateTimer >= (def?.attackCooldown ?? 1500)) {
                monster.stateTimer = 0

                const myPlayerId = gameState.currentPlayer?.id
                if (!myPlayerId || monster.ownerId !== myPlayerId) {
                  return
                }

                networkManager.sendMonsterMove(
                  monster.id,
                  monster.position,
                  monster.rotation,
                  'attack',
                  monster.position
                )

                // Request server-side damage resolution
                networkManager.sendMonsterAttack(
                  monster.id,
                  monster.targetPlayerId
                )
              }
            } else {
              // Out of range - move towards player using A* pathfinding
              monster.moveSpeed = def?.runSpeed ?? 8

              // Recompute path if needed
              const now = performance.now()
              const ps = monster.pathState
              const needsRepath =
                !ps ||
                ps.currentWaypointIndex >= ps.waypoints.length ||
                now - ps.lastPathTime > 500 ||
                Math.abs(targetPlayer.position.x - (ps?.lastTargetX ?? 0)) +
                  Math.abs(targetPlayer.position.z - (ps?.lastTargetZ ?? 0)) >
                  3

              if (needsRepath) {
                const targetFloor = passability_get_floor_at(
                  targetPlayer.position.x,
                  targetPlayer.position.z,
                  targetPlayer.position.y
                )
                this.computePath(
                  monster,
                  targetPlayer.position.x,
                  targetPlayer.position.z,
                  targetFloor
                )
              }

              const reached = this.followPath(monster, deltaTime)

              if (reached && !monster.pathState) {
                // Path exhausted but still not in attack range — give up if stuck
                const recheckDx = targetPlayer.position.x - monster.position.x
                const recheckDz = targetPlayer.position.z - monster.position.z
                if (
                  recheckDx * recheckDx + recheckDz * recheckDz >
                  CHASE_RANGE_SQ
                ) {
                  monster.targetPlayerId = undefined
                  this.transitionToReturn(monster)
                  return
                }
              }

              // Update network to sync movement
              networkManager.sendMonsterMove(
                monster.id,
                monster.position,
                monster.rotation,
                'attack',
                targetPlayer.position
              )
            }
          } else {
            // Target lost
            monster.targetPlayerId = undefined
            this.transitionToReturn(monster)
          }
        } else {
          monster.targetPlayerId = undefined
          this.transitionToReturn(monster)
        }
        break
    }
  }

  private transitionToIdle(monster: MonsterData) {
    monster.state = 'idle'
    monster.stateTimer = 0
    monster.movementIntent = undefined
    monster.pathState = undefined
    monster.targetPosition = undefined
    networkManager.sendMonsterMove(
      monster.id,
      monster.position,
      monster.rotation,
      'idle',
      monster.position
    )
  }

  private transitionToFlee(monster: MonsterData) {
    monster.state = 'run'
    monster.movementIntent = 'flee'
    monster.fleeTimer = 0
    monster.stateTimer = 0

    const def = getMonsterDef(monster.type)
    monster.moveSpeed = def?.runSpeed ?? 8

    if (monster.spawnPosition) {
      this.computePath(
        monster,
        monster.spawnPosition.x,
        monster.spawnPosition.z
      )
    }

    if (!monster.pathState) {
      this.transitionToIdle(monster)
      return
    }

    const firstWp = monster.pathState.waypoints[0]
    monster.rotation = Math.atan2(
      firstWp.x - monster.position.x,
      firstWp.z - monster.position.z
    )

    networkManager.sendMonsterMove(
      monster.id,
      monster.position,
      monster.rotation,
      'run',
      monster.spawnPosition ?? monster.position
    )
  }

  private transitionToReturn(monster: MonsterData) {
    const def = getMonsterDef(monster.type)
    const returnChance = def?.returnChance ?? DEFAULT_RETURN_CHANCE
    if (Math.random() >= returnChance) {
      this.transitionToIdle(monster)
      return
    }

    if (monster.spawnPosition) {
      const dx = monster.position.x - monster.spawnPosition.x
      const dz = monster.position.z - monster.spawnPosition.z
      if (dx * dx + dz * dz <= RETURN_ARRIVE_DIST * RETURN_ARRIVE_DIST) {
        this.transitionToIdle(monster)
        return
      }
    }

    monster.state = 'walk'
    monster.movementIntent = 'return'
    monster.stateTimer = 0
    monster.moveSpeed = def?.walkSpeed ?? 1

    if (monster.spawnPosition) {
      monster.targetPosition = { ...monster.spawnPosition }
      this.computePath(
        monster,
        monster.spawnPosition.x,
        monster.spawnPosition.z
      )
    }

    if (!monster.pathState) {
      this.transitionToIdle(monster)
      return
    }

    const firstWp = monster.pathState.waypoints[0]
    monster.rotation = Math.atan2(
      firstWp.x - monster.position.x,
      firstWp.z - monster.position.z
    )

    networkManager.sendMonsterMove(
      monster.id,
      monster.position,
      monster.rotation,
      'walk',
      monster.spawnPosition ?? monster.position
    )
  }

  private tickFlee(monster: MonsterData, deltaTime: number) {
    monster.fleeTimer = (monster.fleeTimer ?? 0) + deltaTime

    if (monster.fleeTimer >= FLEE_DURATION_MS) {
      monster.targetPlayerId = undefined
      this.transitionToReturn(monster)
      return
    }

    const reached = this.followPath(monster, deltaTime)
    if (reached) {
      monster.targetPlayerId = undefined
      this.transitionToReturn(monster)
    }
  }

  private tickReturn(monster: MonsterData, deltaTime: number) {
    if (monster.spawnPosition) {
      const dx = monster.spawnPosition.x - monster.position.x
      const dz = monster.spawnPosition.z - monster.position.z
      if (dx * dx + dz * dz <= RETURN_ARRIVE_DIST * RETURN_ARRIVE_DIST) {
        this.transitionToIdle(monster)
        return
      }
    }

    if (
      !monster.pathState ||
      monster.pathState.currentWaypointIndex >=
        monster.pathState.waypoints.length
    ) {
      if (monster.spawnPosition) {
        this.computePath(
          monster,
          monster.spawnPosition.x,
          monster.spawnPosition.z
        )
      }
      if (!monster.pathState) {
        this.transitionToIdle(monster)
        return
      }
    }

    this.followPath(monster, deltaTime)
  }

  private transitionToMove(monster: MonsterData) {
    // 1. Determine distance first
    const angle = Math.random() * Math.PI * 2
    const distance =
      MIN_MOVE_DIST + Math.random() * (MAX_MOVE_DIST - MIN_MOVE_DIST)

    // 2. Probability Logic
    // d=2(MIN) -> walk chance 80% (P=0.8)
    // d=10(MAX) -> walk chance 20% (P=0.2) => run chance 80%
    // Linear equation: P(walk) = slope * distance + intercept
    // slope = (0.2 - 0.8) / (10 - 2) = -0.6 / 8 = -0.075
    // intercept: 0.8 = -0.075 * 2 + b => b = 0.8 + 0.15 = 0.95
    const walkProbability = -0.075 * distance + 0.95
    const isWalking = Math.random() < walkProbability

    const def = getMonsterDef(monster.type)
    monster.state = isWalking ? 'walk' : 'run'
    monster.moveSpeed = isWalking ? (def?.walkSpeed ?? 1) : (def?.runSpeed ?? 8)

    const targetX = monster.position.x + Math.cos(angle) * distance
    const targetZ = monster.position.z + Math.sin(angle) * distance
    const targetY = this.sampleHeight(targetX, targetZ)

    // Don't move into water — stay idle instead
    if (targetY < 0) {
      monster.state = 'idle'
      return
    }

    monster.targetPosition = {
      x: targetX,
      y: targetY,
      z: targetZ,
    }

    // Compute A* path to target
    this.computePath(monster, targetX, targetZ)

    // If pathfinding found no path, stay idle
    if (!monster.pathState) {
      monster.state = 'idle'
      return
    }

    // Look at first waypoint
    const firstWp = monster.pathState.waypoints[0]
    monster.rotation = Math.atan2(
      firstWp.x - monster.position.x,
      firstWp.z - monster.position.z
    )

    networkManager.sendMonsterMove(
      monster.id,
      monster.position,
      monster.rotation,
      monster.state,
      monster.targetPosition
    )
  }

  private computePath(
    monster: MonsterData,
    goalX: number,
    goalZ: number,
    goalFloor?: number
  ) {
    const startFloor = monster.currentFloor ?? 0
    const gFloor = goalFloor ?? 0
    const result = findPath(
      monster.position.x,
      monster.position.z,
      startFloor,
      goalX,
      goalZ,
      gFloor
    )
    if (result.waypoints.length > 0) {
      monster.pathState = {
        waypoints: result.waypoints,
        currentWaypointIndex: 0,
        lastPathTime: performance.now(),
        lastTargetX: goalX,
        lastTargetZ: goalZ,
      }
    } else {
      monster.pathState = undefined
    }
  }

  /**
   * Follow the stored waypoint path. Returns true when the final waypoint is reached.
   */
  private followPath(monster: MonsterData, deltaTime: number): boolean {
    const ps = monster.pathState
    if (!ps || ps.waypoints.length === 0) return true

    const wp = ps.waypoints[ps.currentWaypointIndex]

    // Determine Y: use floor yBase for upper floors, terrain height for ground
    const waypointY = this.getYForFloor(wp.x, wp.z, wp.floor)
    const target = { x: wp.x, y: waypointY, z: wp.z }

    // Look at current waypoint
    const dx = wp.x - monster.position.x
    const dz = wp.z - monster.position.z
    monster.rotation = Math.atan2(dx, dz)

    if (!this.moveTowards(monster, target, deltaTime)) return false

    // Waypoint reached — update floor and advance
    monster.currentFloor = wp.floor
    ps.currentWaypointIndex++
    if (ps.currentWaypointIndex >= ps.waypoints.length) {
      monster.pathState = undefined
      return true
    }
    return false
  }

  private getYForFloor(x: number, z: number, floor: number): number {
    if (floor > 0) {
      const yBase = passability_get_floor_y_base(x, z, floor)
      if (!isNaN(yBase)) return yBase
    }
    return this.sampleHeight(x, z)
  }

  private moveTowards(
    monster: MonsterData,
    target: { x: number; y: number; z: number },
    deltaTime: number // in ms
  ): boolean {
    const dx = target.x - monster.position.x
    const dz = target.z - monster.position.z
    const distance = Math.sqrt(dx * dx + dz * dz)

    const moveStep = (monster.moveSpeed * deltaTime) / 1000
    const onUpperFloor = (monster.currentFloor ?? 0) > 0

    if (distance <= moveStep) {
      if (!onUpperFloor) {
        const y = this.sampleHeight(target.x, target.z)
        if (y < 0) return true
        monster.position = { ...target, y }
      } else {
        monster.position = { ...target }
      }
      return true
    } else {
      const newX = monster.position.x + (dx / distance) * moveStep
      const newZ = monster.position.z + (dz / distance) * moveStep
      if (!onUpperFloor) {
        const newY = this.sampleHeight(newX, newZ)
        if (newY < 0) return true
        monster.position = { x: newX, y: newY, z: newZ }
      } else {
        monster.position = { x: newX, y: target.y, z: newZ }
      }
      return false
    }
  }

  updateMonsterFromNetwork(
    id: string,
    position: { x: number; y: number; z: number },
    rotation: number,
    state: MonsterData['state'],
    targetPosition: { x: number; y: number; z: number }
  ) {
    const monster = this.monsters.get(id)
    if (monster) {
      // Guard: If monster is dead, don't allow state changes back to alive states
      if (monster.state === 'dead' && state !== 'dead') {
        return
      }

      monster.position = position
      monster.rotation = rotation
      monster.state = state

      // Update moveSpeed based on state for remote monsters
      const def = getMonsterDef(monster.type)
      if (monster.state === 'run') {
        monster.moveSpeed = def?.runSpeed ?? 8
      } else if (monster.state === 'walk') {
        monster.moveSpeed = def?.walkSpeed ?? 1
      }

      monster.targetPosition = targetPosition
      this.monsters.set(id, { ...monster })
    }
  }

  private spawnRandomMonster(playerPos: { x: number; y: number; z: number }) {
    // Random position around the player (distance 5-15)
    const angle = Math.random() * Math.PI * 2
    const distance = 5 + Math.random() * 10
    const x = playerPos.x + Math.cos(angle) * distance
    const z = playerPos.z + Math.sin(angle) * distance

    const y = this.sampleHeight(x, z)

    // Don't spawn underwater
    if (y < 0) return

    // Request spawn from server
    networkManager.requestSpawnMonster(
      'scp939',
      { x, y, z },
      Math.random() * Math.PI * 2
    )
  }
  requestSpawnFromServer(
    monsterType: string,
    position: { x: number; y: number; z: number },
    rotation: number
  ) {
    networkManager.requestSpawnMonster(monsterType, position, rotation)
  }
}

export const monsterManager = new MonsterManager()

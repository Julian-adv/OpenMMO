import { SvelteMap } from 'svelte/reactivity'
import { networkManager } from '../network/socket'
import { get } from 'svelte/store'
import { gameStore } from '../stores/gameStore'
import type { MonsterData } from '../types/Monster'

const WALK_SPEED = 1.0
const RUN_SPEED = 8.0
const MIN_MOVE_DIST = 2.0
const MAX_MOVE_DIST = 10.0

class MonsterManager {
  monsters = new SvelteMap<string, MonsterData>()
  private timeSinceLastSpawn = 0
  private readonly SPAWN_INTERVAL = 10000 // 10 seconds

  spawnWithId(
    id: string,
    type: MonsterData['type'],
    position: { x: number; y: number; z: number },
    ownerId?: string,
    health: number = 10,
    maxHealth: number = 10
  ) {
    if (this.monsters.has(id)) return

    this.monsters.set(id, {
      id,
      type,
      position,
      rotation: 0,
      state: 'idle',
      ownerId,
      moveSpeed: 3.5, // default
      stateTimer: 0,
      health,
      maxHealth,
    })
    console.log(
      `Spawned monster ${id} (synced) at`,
      position,
      `Owner: ${ownerId}`
    )
  }

  remove(id: string) {
    this.monsters.delete(id)
  }

  handleMonsterDead(id: string) {
    const monster = this.monsters.get(id)
    if (monster) {
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

  handleMonsterAttacked(monsterId: string, playerId: string, hit: boolean) {
    const monster = this.monsters.get(monsterId)
    if (!monster || monster.state === 'dead') return

    // Set impact delay (e.g., 400ms for player's slash to land)
    monster.impactDelay = 540
    monster.targetPlayerId = playerId
    monster.isLastHitSuccess = hit

    const gameState = get(gameStore)
    const myPlayerId = gameState.currentPlayer?.id

    // Only respond with state changes if we own this monster
    if (monster.ownerId === myPlayerId) {
      // We will transition to 'hit' in the update loop after impactDelay
      // but we can set the targetPlayerId now to ensure retaliation
      monster.moveSpeed = RUN_SPEED
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
      // Impact Delay Handling (Global for all clients to keep visuals synced)
      if (monster.impactDelay !== undefined && monster.impactDelay > 0) {
        monster.impactDelay -= deltaTime
        if (monster.impactDelay <= 0) {
          monster.impactDelay = 0

          if (monster.isDeadPending) {
            // Death impact!
            monster.state = 'dead'
            monster.stateTimer = 0
            monster.isDeadPending = false
          } else if (monster.isLastHitSuccess) {
            // Normal hit impact
            monster.state = 'hit'
            monster.stateTimer = 0
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

      case 'hit':
        // Wait for stagger animation to finish (approx 800ms)
        if (monster.stateTimer >= 800) {
          monster.state = 'attack'
          monster.stateTimer = 0
          // Notify network of state change
          networkManager.sendMonsterMove(
            monster.id,
            monster.position,
            monster.rotation,
            'attack',
            monster.position
          )
        }
        break

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
      case 'run':
        if (monster.targetPosition) {
          const reached = this.moveTowards(
            monster,
            monster.targetPosition,
            deltaTime
          )

          if (reached) {
            // 50% Idle, 50% Move again
            if (Math.random() < 0.5) {
              monster.state = 'idle'
              monster.stateTimer = 0
              networkManager.sendMonsterMove(
                monster.id,
                monster.position,
                monster.rotation,
                'idle',
                monster.position
              )
            } else {
              this.transitionToMove(monster)
            }
          }
        } else {
          monster.state = 'idle'
        }
        break

      case 'attack':
        if (monster.targetPlayerId) {
          const gameState = get(gameStore)
          let targetPlayer:
            | { position: { x: number; y: number; z: number } }
            | undefined

          if (gameState.currentPlayer?.id === monster.targetPlayerId) {
            targetPlayer = gameState.currentPlayer
          } else {
            targetPlayer = gameState.otherPlayers.get(monster.targetPlayerId)
          }

          if (targetPlayer) {
            const dx = targetPlayer.position.x - monster.position.x
            const dz = targetPlayer.position.z - monster.position.z
            const distSq = dx * dx + dz * dz
            const ATTACK_RANGE = 2.0
            const ATTACK_RANGE_SQ = ATTACK_RANGE * ATTACK_RANGE
            const CHASE_RANGE = 25.0
            const CHASE_RANGE_SQ = CHASE_RANGE * CHASE_RANGE

            if (distSq > CHASE_RANGE_SQ) {
              // Target too far, stop chasing
              monster.state = 'idle'
              monster.targetPlayerId = undefined
              monster.stateTimer = 0
              networkManager.sendMonsterMove(
                monster.id,
                monster.position,
                monster.rotation,
                'idle',
                monster.position
              )
              return
            }

            // Look at player
            monster.rotation = Math.atan2(dx, dz)

            if (distSq <= ATTACK_RANGE_SQ) {
              // Within range - wait for attack animation/cooldown
              if (monster.stateTimer >= 1500) {
                // Attack every 1.5s
                monster.stateTimer = 0
                console.log(
                  `Monster ${monster.id} attacks player ${monster.targetPlayerId}`
                )

                networkManager.sendMonsterMove(
                  monster.id,
                  monster.position,
                  monster.rotation,
                  'attack',
                  monster.position
                )
              }
            } else {
              // Out of range - move towards player
              monster.moveSpeed = RUN_SPEED
              const dist = Math.sqrt(distSq)
              const moveStep = (monster.moveSpeed * deltaTime) / 1000

              monster.position = {
                x: monster.position.x + (dx / dist) * moveStep,
                y: monster.position.y,
                z: monster.position.z + (dz / dist) * moveStep,
              }

              // Update network to sync movement
              // Throttle network updates for performance if needed,
              // but for now let's send it to keep it responsive.
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
            monster.state = 'idle'
            monster.targetPlayerId = undefined
          }
        } else {
          monster.state = 'idle'
        }
        break
    }
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

    monster.state = isWalking ? 'walk' : 'run'
    // Speed constant usage
    monster.moveSpeed = isWalking ? WALK_SPEED : RUN_SPEED

    monster.targetPosition = {
      x: monster.position.x + Math.cos(angle) * distance,
      y: monster.position.y,
      z: monster.position.z + Math.sin(angle) * distance,
    }

    // Look at target
    monster.rotation = Math.atan2(
      monster.targetPosition.x - monster.position.x,
      monster.targetPosition.z - monster.position.z
    )

    networkManager.sendMonsterMove(
      monster.id,
      monster.position,
      monster.rotation,
      monster.state,
      monster.targetPosition
    )
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

    if (distance <= moveStep) {
      monster.position = { ...target }
      return true
    } else {
      monster.position = {
        x: monster.position.x + (dx / distance) * moveStep,
        y: monster.position.y,
        z: monster.position.z + (dz / distance) * moveStep,
      }
      return false
    }
  }

  updateMonsterFromNetwork(
    id: string,
    position: { x: number; y: number; z: number },
    rotation: number,
    state: string,
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
      monster.state = state as MonsterData['state']

      // Update moveSpeed based on state for remote monsters
      if (monster.state === 'run') {
        monster.moveSpeed = RUN_SPEED
      } else if (monster.state === 'walk') {
        monster.moveSpeed = WALK_SPEED
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

    // Request spawn from server
    networkManager.requestSpawnMonster(
      'scp939',
      { x, y: 0, z }, // Assuming flat ground for now
      Math.random() * Math.PI * 2
    )
  }
}

export const monsterManager = new MonsterManager()

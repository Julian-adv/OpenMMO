import { SvelteMap } from 'svelte/reactivity'
import { networkManager } from '../network/socket'
import { get } from 'svelte/store'
import { gameStore } from '../stores/gameStore'

export interface MonsterData {
  id: string
  type: 'scp939'
  position: { x: number; y: number; z: number }
  rotation: number
  state: 'idle' | 'moving' | 'attack'
  ownerId?: string
  targetPosition?: { x: number; y: number; z: number }
  moveSpeed: number
  stateTimer: number
}

class MonsterManager {
  monsters = new SvelteMap<string, MonsterData>()
  private timeSinceLastSpawn = 0
  private readonly SPAWN_INTERVAL = 10000 // 10 seconds

  spawnWithId(
    id: string,
    type: MonsterData['type'],
    position: { x: number; y: number; z: number },
    ownerId?: string
  ) {
    if (this.monsters.has(id)) return

    this.monsters.set(id, {
      id,
      type,
      position,
      rotation: 0,
      state: 'idle',
      ownerId,
      moveSpeed: 3.5, // slightly faster than player? or slower?
      stateTimer: 0,
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
      // Only control monsters that YOU own
      if (monster.ownerId === myPlayerId) {
        this.updateMonsterAI(monster, deltaTime)
        // Trigger reactivity with new reference
        this.monsters.set(monster.id, { ...monster })
      } else {
        // Interpolate remote monsters (Basic lerp for now)
        if (monster.state === 'moving' && monster.targetPosition) {
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

      case 'moving':
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
    }
  }

  private transitionToMove(monster: MonsterData) {
    monster.state = 'moving'
    const angle = Math.random() * Math.PI * 2
    const distance = Math.random() * 5 // Max 5 meters

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
      'moving',
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
      monster.position = position
      monster.rotation = rotation
      monster.state = state as MonsterData['state']
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

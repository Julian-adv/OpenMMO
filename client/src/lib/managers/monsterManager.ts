import { SvelteMap } from 'svelte/reactivity'
import { networkManager } from '../network/socket'

export interface MonsterData {
  id: string
  type: 'scp939'
  position: { x: number; y: number; z: number }
  rotation: number
  state: 'idle' | 'moving' | 'attack'
}

class MonsterManager {
  monsters = new SvelteMap<string, MonsterData>()
  private timeSinceLastSpawn = 0
  private readonly SPAWN_INTERVAL = 10000 // 10 seconds

  spawnWithId(
    id: string,
    type: MonsterData['type'],
    position: { x: number; y: number; z: number }
  ) {
    if (this.monsters.has(id)) return

    this.monsters.set(id, {
      id,
      type,
      position,
      rotation: 0,
      state: 'idle',
    })
    console.log(`Spawned monster ${id} (synced) at`, position)
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
    if (!playerPosition) return

    this.timeSinceLastSpawn += deltaTime

    if (this.timeSinceLastSpawn >= this.SPAWN_INTERVAL) {
      this.timeSinceLastSpawn = 0
      this.spawnRandomMonster(playerPosition)
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
import { SvelteMap } from 'svelte/reactivity'

export interface MonsterData {
  id: string
  type: 'scp939'
  position: { x: number; y: number; z: number }
  rotation: number
  state: 'idle' | 'moving' | 'attack'
}

class MonsterManager {
  monsters = new SvelteMap<string, MonsterData>()
  private nextId = 1

  spawn(
    type: MonsterData['type'],
    position: { x: number; y: number; z: number }
  ) {
    const id = `monster_${this.nextId++}`
    this.monsters.set(id, {
      id,
      type,
      position,
      rotation: 0,
      state: 'idle',
    })
    console.log(`Spawned monster ${id} at`, position)
    return id
  }

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
    this.nextId = 1
  }
}

export const monsterManager = new MonsterManager()

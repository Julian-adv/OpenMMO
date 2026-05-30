import { SvelteMap } from 'svelte/reactivity'
import { hmrSingleton } from '../utils/hmr'
import * as THREE from 'three'
import { networkManager } from '../network/socket'
import { get } from 'svelte/store'
import { gameStore, type GameState } from '../stores/gameStore'
import { remotePlayerManager } from './remotePlayerManager'
import type { MonsterData } from '../types/Monster'
import { getMonsterDef } from '../data/monsterDefs'
import type { Position } from '../utils/movementUtils'
import type { TerrainHeightManager } from './terrainHeightManager'
import type { TerrainSplatManager } from './terrainSplatManager'
import type { NoSpawnZone } from './zoneManager'
import { TILE_DIM, worldToTileCoord } from './terrain-height-types'
import { TERRAIN_TILE_SIZE } from '../components/game-scene/terrain-utils'
import { readCell, VEGETATION_BASE_SLOT } from '../terrain/splat-encoding'
import {
  ai_load_templates,
  ai_create_brain,
  ai_remove_brain,
  ai_tick_brain,
  ai_handle_hit,
  ai_handle_death,
} from '../wasm/onlinerpg_shared'
import aiTemplatesJson from '../../../../data/ai_templates.json'
import monstersJson from '../../../../data/monsters.json'

type MonsterState = MonsterData['state']

interface AiCommand {
  type: 'Move' | 'Attack'
  monster_id: string
  position?: { x: number; y: number; z: number }
  rotation?: number
  state?: MonsterState
  target_position?: { x: number; y: number; z: number }
  target_player_id?: string
}

interface TickResult {
  commands: AiCommand[]
  position: { x: number; y: number; z: number }
  rotation: number
  state: MonsterState
}

// Ambient spawn placement: distance band around the player and town buffer.
const AMBIENT_MIN_DIST = 20
const AMBIENT_MAX_DIST = 25
const TOWN_MARGIN = 30 // keep spawns this far outside no-spawn zones too
const WATER_MIN_HEIGHT = 0.3 // reject sea / submerged ground below this
const MAX_SPAWN_ATTEMPTS = 12

class MonsterManager {
  monsters = new SvelteMap<string, MonsterData>()
  heightManager: TerrainHeightManager | null = null
  splatManager: TerrainSplatManager | null = null
  private noSpawnZones: NoSpawnZone[] = []
  private templatesLoaded = false

  private sampleHeight(x: number, z: number): number {
    return this.heightManager?.getHeightAtWorldPosition(x, z) ?? 0
  }

  setNoSpawnZones(zones: NoSpawnZone[]) {
    this.noSpawnZones = zones
  }

  private ensureTemplatesLoaded() {
    if (!this.templatesLoaded) {
      ai_load_templates(JSON.stringify(aiTemplatesJson))
      this.templatesLoaded = true
    }
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

    // Create WASM brain for owned monsters
    const gameState = get(gameStore)
    const myPlayerId = gameState.currentPlayer?.id
    if (ownerId === myPlayerId) {
      this.ensureTemplatesLoaded()
      const monsterDef = (
        monstersJson as Record<string, { aiTemplate?: string }>
      )[type]
      const templateName = monsterDef?.aiTemplate ?? 'default'
      ai_create_brain({
        monsterId: id,
        monsterType: type,
        position,
        health: hp,
        maxHealth: maxHp,
        walkSpeed: def?.walkSpeed ?? 1,
        runSpeed: def?.runSpeed ?? 8,
        templateName,
      })
    }
  }

  remove(id: string) {
    const monster = this.monsters.get(id)
    const gameState = get(gameStore)
    if (monster?.ownerId === gameState.currentPlayer?.id) {
      ai_remove_brain(id)
    }
    this.monsters.delete(id)
  }

  handleMonsterDead(id: string, droppedWeaponItemDefId?: string | null) {
    const monster = this.monsters.get(id)
    if (monster) {
      ai_handle_death(id)
      monster.droppedWeaponItemDefId = droppedWeaponItemDefId ?? undefined
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

    // Trigger reactivity
    this.monsters.set(monsterId, { ...monster })
  }

  reset() {
    // Remove all brains
    for (const id of this.monsters.keys()) {
      ai_remove_brain(id)
    }
    this.monsters.clear()
  }

  update(deltaTime: number) {
    // FSM & Movement Logic
    const gameState = get(gameStore)
    const myPlayerId = gameState.currentPlayer?.id
    const nearbyPlayers = this.buildNearbyPlayers(gameState)

    for (const monster of this.monsters.values()) {
      // Keep non-owned monster Y aligned with terrain (owned monsters get Y from TickResult)
      if (monster.ownerId !== myPlayerId) {
        const terrainY = this.sampleHeight(
          monster.position.x,
          monster.position.z
        )
        if (Math.abs(monster.position.y - terrainY) > 0.001) {
          monster.position = { ...monster.position, y: terrainY }
        }
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
          } else if (monster.ownerId === myPlayerId) {
            const hitCommands: AiCommand[] =
              ai_handle_hit(
                monster.id,
                monster.targetPlayerId ?? '',
                !!monster.isLastHitSuccess,
                monster.pendingDamage ?? 0
              ) ?? []
            this.processAiCommands(monster, hitCommands)
          } else if (monster.isLastHitSuccess) {
            // Non-owner: show hit stagger visually
            monster.state = 'hit'
            monster.stateTimer = 0
          } else if (monster.targetPlayerId && monster.state !== 'attack') {
            // Non-owner miss: show attack state visually
            monster.state = 'attack'
            monster.stateTimer = 0
          }
        }
      }

      // Only control monsters that YOU own
      if (monster.ownerId === myPlayerId) {
        // Guard: If dead or about to die, stop AI immediately
        if (monster.state === 'dead' || monster.isDeadPending) {
          this.monsters.set(monster.id, { ...monster })
          continue
        }

        const raw = ai_tick_brain(monster.id, deltaTime, nearbyPlayers)
        // ai_tick_brain returns a TickResult object with commands, position, rotation, state
        const result = raw as TickResult

        // Always apply brain position/rotation (FSM moves internally via follow_path)
        if (result.position) {
          const terrainY = this.sampleHeight(
            result.position.x,
            result.position.z
          )
          monster.position = {
            x: result.position.x,
            y: terrainY,
            z: result.position.z,
          }
        }
        if (result.rotation !== undefined) {
          monster.rotation = result.rotation
        }
        if (result.state) {
          monster.state = result.state
          this.updateMoveSpeedFromState(monster)
        }

        // Process transition commands (network sync, attacks)
        if (result.commands) {
          this.processAiCommands(monster, result.commands)
        }

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
          this.monsters.set(monster.id, { ...monster })
        } else if (impactJustExpired) {
          this.monsters.set(monster.id, { ...monster })
        }
      }
    }
  }

  private buildNearbyPlayers(gameState: GameState): Array<{
    id: string
    position: { x: number; y: number; z: number }
    health: number
  }> {
    const players: Array<{
      id: string
      position: { x: number; y: number; z: number }
      health: number
    }> = []

    // Current player
    if (gameState.currentPlayer) {
      players.push({
        id: gameState.currentPlayer.id,
        position: {
          x: gameState.currentPlayer.position.x,
          y: gameState.currentPlayer.position.y,
          z: gameState.currentPlayer.position.z,
        },
        health: gameState.currentPlayer.health ?? 0,
      })
    }

    // Remote players
    for (const [playerId, remoteState] of remotePlayerManager.players) {
      const remotePlayer = gameState.otherPlayers.get(playerId)
      players.push({
        id: playerId,
        position: remoteState.position,
        health: remotePlayer?.health ?? 0,
      })
    }

    return players
  }

  private updateMoveSpeedFromState(monster: MonsterData) {
    const def = getMonsterDef(monster.type)
    if (monster.state === 'run') {
      monster.moveSpeed = def?.runSpeed ?? 8
    } else if (monster.state === 'walk') {
      monster.moveSpeed = def?.walkSpeed ?? 1
    }
  }

  private processAiCommands(monster: MonsterData, commands: AiCommand[]) {
    for (const cmd of commands) {
      if (cmd.type === 'Move') {
        if (cmd.target_position) {
          monster.targetPosition = cmd.target_position
        }
        networkManager.sendMonsterMove(
          cmd.monster_id,
          monster.position,
          monster.rotation,
          monster.state,
          cmd.target_position ?? monster.position
        )
      } else if (cmd.type === 'Attack' && cmd.target_player_id) {
        networkManager.sendMonsterAttack(cmd.monster_id, cmd.target_player_id)
      }
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
      this.updateMoveSpeedFromState(monster)

      monster.targetPosition = targetPosition
      this.monsters.set(id, { ...monster })
    }
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

  /**
   * Server asked us to spawn a monster near the local player. Pick a position
   * 20–25m away on grassland, avoiding water and towns, then request it. Picks
   * the first valid spot found; if none after a few tries, the server retries
   * next tick.
   */
  tryAmbientSpawn(monsterType: string) {
    const player = get(gameStore).currentPlayer
    if (!player) return
    const px = player.position.x
    const pz = player.position.z

    // Don't spawn anything around a player who is standing in (or near) a town.
    if (this.nearNoSpawnZone(px, pz)) return

    for (let i = 0; i < MAX_SPAWN_ATTEMPTS; i++) {
      const angle = Math.random() * Math.PI * 2
      const distance =
        AMBIENT_MIN_DIST + Math.random() * (AMBIENT_MAX_DIST - AMBIENT_MIN_DIST)
      const x = px + Math.cos(angle) * distance
      const z = pz + Math.sin(angle) * distance

      const y = this.sampleHeight(x, z)
      if (y < WATER_MIN_HEIGHT) continue // sea / submerged
      if (!this.isGrassAt(x, z)) continue // road / sand / cliff / riverbed / snow
      if (this.nearNoSpawnZone(x, z)) continue // town + margin

      networkManager.requestSpawnMonster(
        monsterType,
        { x, y, z },
        Math.random() * Math.PI * 2
      )
      return
    }
  }

  /** Is the dominant terrain type at (x,z) the grass-supporting base ground? */
  private isGrassAt(x: number, z: number): boolean {
    const sm = this.splatManager
    if (!sm) return false
    const tileX = worldToTileCoord(x)
    const tileZ = worldToTileCoord(z)
    const data = sm.getSplatData(tileX, tileZ)
    if (!data) return false

    const tileMinX = tileX * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const tileMinZ = tileZ * TERRAIN_TILE_SIZE - TERRAIN_TILE_SIZE / 2
    const cellX = Math.min(TILE_DIM - 1, Math.max(0, Math.floor(x - tileMinX)))
    const cellZ = Math.min(TILE_DIM - 1, Math.max(0, Math.floor(z - tileMinZ)))
    const cell = readCell(data, cellZ * TILE_DIM + cellX)
    const dominant = cell.blend >= 128 ? cell.secondaryIdx : cell.primaryIdx
    return dominant === VEGETATION_BASE_SLOT
  }

  /** Within TOWN_MARGIN of any no-spawn zone (towns / safe areas)? */
  private nearNoSpawnZone(x: number, z: number): boolean {
    const m = TOWN_MARGIN
    for (const zone of this.noSpawnZones) {
      if (
        x >= zone.minX - m &&
        x <= zone.maxX + m &&
        z >= zone.minZ - m &&
        z <= zone.maxZ + m
      ) {
        return true
      }
    }
    return false
  }
}

export const monsterManager = hmrSingleton(
  'monsterManager',
  () => new MonsterManager()
)

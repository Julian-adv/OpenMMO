import { SvelteMap } from 'svelte/reactivity'
import { hmrSingleton } from '../utils/hmr'
import * as THREE from 'three'
import { get } from 'svelte/store'
import { gameStore, type GameState } from '../stores/gameStore'
import { inventoryStore } from '../stores/inventoryStore'
import { remotePlayerManager } from './remotePlayerManager'
import type { MonsterData } from '../types/Monster'
import type { ServerMonster } from '../network/networkTypes'
import { getMonsterDef } from '../data/monsterDefs'
import { getItemDef, isRangedWeapon } from '../data/itemDefs'
import { flightMsFor, requestArrow } from '../stores/arrowStore'
import {
  getMaterialHitSoundUrl,
  getMaterialMissSoundUrl,
} from '../data/materialImpactSounds'
import { dungeonManager } from './dungeonManager'
import { SLEEP_SINK_MS } from '../effects/dormant-sleep'
import { housingManager } from './housingManager'
import { computeChaseAim } from './chase-aim'
import type { Position } from '../utils/movementUtils'
import type { TerrainHeightManager } from './terrainHeightManager'
import {
  playMonsterDeathSound,
  playBowSound,
  playSwordHitSound,
  playSwordMissSound,
} from './sfxManager'
import { clearXpArrival, releaseXpArrival } from './xpArrival'
import {
  monster_body_engage_point,
  passability_remove_monster_body,
  passability_set_monster_body,
} from '../wasm/onlinerpg_shared'
import { MONSTER_SKILL_DEFS } from '../data/monsterSkills'
import {
  shortestWrappedDeltaX,
  unwrapWorldXNear,
  wrapWorldX,
} from '../terrain/world-wrap'
import {
  DAMAGE_TEXT_AFTER_IMPACT_MS,
  PLAYER_RANGED_DRAW_DELAY_MS,
  PLAYER_RANGED_IMPACT_DELAY_MS,
  DEFAULT_MONSTER_ATTACK_IMPACT_DELAY_MS,
  PLAYER_ATTACK_IMPACT_DELAY_MS,
  SWORD_MISS_DELAY_MS,
} from '../data/combatTiming'
type MonsterState = MonsterData['state']

const MONSTER_POSITION_EPSILON = 0.001
// Server corrections under this size are absorbed by speed, not snapped.
const SYNC_BLEND_MAX_METERS = 2.5
const SYNC_CATCHUP_FRACTION = 0.5 // of move speed, while moving
const SYNC_IDLE_ABSORB_MPS = 2.0
// Engagement corrections (hit/attack) absorb fast: the model is already in a
// combat pose, so lingering meters from where it fights reads worse than a
// quick slide.
const SYNC_COMBAT_ABSORB_MPS = 5.0
// Backstop for a pending death whose hit clip never reports completion.
const DEAD_PENDING_TIMEOUT_MS = 2000

class MonsterManager {
  monsters = new SvelteMap<string, MonsterData>()
  heightManager: TerrainHeightManager | null = null
  /** Living solid bodies, sealed in the wasm passability cache like the
   *  server's. Their facing never changes. */
  private bodies = new Set<string>()

  private removeBody(id: string) {
    if (this.bodies.delete(id)) passability_remove_monster_body(id)
  }

  /** Where reach to `monsterId` is measured from `from`: the point just
   *  outside a solid body, else `meshPosition`. */
  engagePosition(
    monsterId: string,
    from: Position,
    meshPosition: Position
  ): Position {
    const point =
      this.bodies.has(monsterId) &&
      monster_body_engage_point(monsterId, from.x, from.z)
    return point
      ? { x: point[0], y: meshPosition.y, z: point[1] }
      : meshPosition
  }

  /** Keep the current height until terrain is available. */
  private monsterGroundYOrNull(
    monster: MonsterData,
    x: number,
    z: number,
    fallbackY: number
  ): number | null {
    const fl = monster.floorLevel ?? 0
    if (fl < 0) {
      return dungeonManager.floorHeightAt(-fl, x, z) ?? fallbackY
    }
    if (!this.heightManager) return fallbackY
    const y = this.heightManager.groundYOrNull(x, z)
    const footprint = getMonsterDef(monster.type)?.footprint
    if (y === null || fl !== 0 || !footprint) return y
    return this.lowestUnderBody(monster, footprint, x, z, y)
  }

  /** `halfWidth|front|back` → body-frame sample points (corners, edge
   *  midpoints), parsed once per footprint. */
  private bodySamples = new Map<string, [number, number][]>()
  /** Last settled height per bodied monster; they rarely move or turn. */
  private bodyGround = new Map<
    string,
    { x: number; z: number; rotation: number; y: number }
  >()

  /** A body on a slope settles to its lowest corner, so no edge floats over
   *  the downhill side. `footprint` is `halfWidth|front|back` (monsters.csv). */
  private lowestUnderBody(
    monster: MonsterData,
    footprint: string,
    x: number,
    z: number,
    centerY: number
  ): number {
    const { rotation } = monster
    const cached = this.bodyGround.get(monster.id)
    if (
      cached &&
      cached.x === x &&
      cached.z === z &&
      cached.rotation === rotation
    )
      return cached.y
    let samples = this.bodySamples.get(footprint)
    if (!samples) {
      const [halfWidth, front, back] = footprint.split('|').map(Number)
      samples = [-halfWidth, 0, halfWidth].flatMap((lx) =>
        [front, 0, -back].map((lz): [number, number] => [lx, lz])
      )
      this.bodySamples.set(footprint, samples)
    }
    const s = Math.sin(rotation)
    const c = Math.cos(rotation)
    let lowest = centerY
    let complete = true
    for (const [lx, lz] of samples) {
      const y = this.heightManager!.groundYOrNull(
        wrapWorldX(x + lx * c + lz * s),
        z - lx * s + lz * c
      )
      if (y === null) complete = false
      else if (y < lowest) lowest = y
    }
    // Unloaded tiles may hold a lower corner; ask again next frame.
    if (complete) this.bodyGround.set(monster.id, { x, z, rotation, y: lowest })
    return lowest
  }

  private monsterGroundY(monster: MonsterData, x: number, z: number): number {
    const y = monster.position.y
    return this.monsterGroundYOrNull(monster, x, z, y) ?? y
  }

  /** Ground-resolved position, or null while terrain is loading. */
  private snapToMonsterGround(
    monster: MonsterData,
    position: { x: number; y: number; z: number }
  ): Position | null {
    const y = this.monsterGroundYOrNull(
      monster,
      position.x,
      position.z,
      position.y
    )
    if (y === null) return null
    return { x: position.x, y, z: position.z }
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

  spawnWithId(monster: ServerMonster) {
    if (this.monsters.has(monster.id)) return

    const type = monster.monster_type as MonsterData['type']
    // Corpses stay in the server's AOI, so re-entering a floor respawns
    // them dead; anything else transient collapses to idle.
    const spawnDead = monster.state === 'dead'
    const def = getMonsterDef(type)
    // Server is authoritative for HP and always sends health/max_health on
    // spawn; the constant is only a defensive fallback.
    const record: MonsterData = {
      id: monster.id,
      type,
      position: monster.position,
      rotation: def?.footprint ? monster.rotation : 0,
      state: spawnDead ? 'dead' : 'idle',
      moveSpeed: def?.walkSpeed ?? 1,
      attackCounter: 0,
      hitCounter: 0,
      health: monster.health ?? 10,
      maxHealth: monster.max_health ?? 10,
      floorLevel: monster.floor_level ?? 0,
    }
    this.monsters.set(monster.id, record)
    const { x, y, z } = monster.position
    if (
      def?.footprint &&
      !spawnDead &&
      record.floorLevel === 0 &&
      passability_set_monster_body(
        monster.id,
        def.footprint,
        x,
        y,
        z,
        monster.rotation
      )
    )
      this.bodies.add(monster.id)
  }

  /** A sleeping boss outlives its removal until its sink ends. */
  remove(id: string) {
    if (this.monsters.get(id)?.sinkAt === undefined) this.drop(id)
  }

  private drop(id: string) {
    this.monsters.delete(id)
    this.removeBody(id)
    this.bodyGround.delete(id)
  }

  // Whether a killing blow should play the hit reaction before the death clip.
  // Defaults to true; monsters with an awkward hit clip opt out via the def.
  private deathPlaysHitFor(monster: MonsterData): boolean {
    return getMonsterDef(monster.type)?.deathPlaysHit ?? true
  }

  private playPendingSwordHitSound(monster: MonsterData) {
    if (!monster.pendingSwordHitSoundUrl) return

    playSwordHitSound(monster.pendingSwordHitSoundUrl)
    monster.pendingSwordHitSoundUrl = undefined
  }

  // Rides the killing blow's contact frame; the death clip lands much later.
  private playDeathSound(monster: MonsterData) {
    const url = getMonsterDef(monster.type)?.deathSound
    if (url) playMonsterDeathSound(url)
  }

  /** This kill is still on its way down (waiting out the blade's impact or
   *  the hit reaction), so its XP has a moment to wait for. */
  isDeathPending(id: string): boolean {
    return this.monsters.get(id)?.isDeadPending === true
  }

  handleMonsterDead(id: string, droppedWeaponItemDefId?: string | null) {
    this.removeBody(id)
    const monster = this.monsters.get(id)
    if (monster) {
      monster.droppedWeaponItemDefId = droppedWeaponItemDefId ?? undefined
      const deathPlaysHit = this.deathPlaysHitFor(monster)
      // If we are waiting for an impact, delay the visual death
      if (monster.impactDelay && monster.impactDelay > 0) {
        monster.isDeadPending = true
        monster.deadPendingTimer = 0
      } else if (
        monster.state === 'hit' &&
        monster.isLastHitSuccess &&
        deathPlaysHit
      ) {
        this.playDeathSound(monster)
        monster.isDeadPending = true
        // The hit clip may have already finished (clamped, no further
        // 'finished' event) — restart it so its completion re-arms the death.
        this.restartHitClip(monster)
      } else {
        // Otherwise die immediately
        this.playDeathSound(monster)
        this.applyMonsterPose(monster, { state: 'dead' })
      }
      this.monsters.set(id, { ...monster })
    }
  }

  handleMonsterHealth(id: string, health: number, maxHealth: number) {
    const monster = this.monsters.get(id)
    if (!monster) return
    this.monsters.set(id, { ...monster, health, maxHealth })
  }

  handleMonsterHitFinished(id: string) {
    const monster = this.monsters.get(id)
    if (!monster?.isDeadPending || monster.state !== 'hit') return

    this.finishPendingDeath(monster)
  }

  private finishPendingDeath(monster: MonsterData) {
    this.applyMonsterPose(monster, { state: 'dead' })
    monster.isDeadPending = false
    this.monsters.set(monster.id, { ...monster })
  }

  // Restarts the flinch clip (a bump forces the component to replay it even
  // when the state is already 'hit') and re-opens the pending-death window.
  private restartHitClip(monster: MonsterData) {
    monster.hitCounter = (monster.hitCounter ?? 0) + 1
    monster.deadPendingTimer = 0
  }

  handleMonsterAttacked(
    monsterId: string,
    playerId: number,
    hit: boolean,
    damage: number,
    /** The round the server spent, so the arrow drawn is the one that left
     *  the quiver — a remote shooter's bag is not visible from here. */
    ammoItemDefId?: string | null,
    daggerSkill = false,
    critical = false,
    /** Valhalla Charge: the blade lands as the dash arrives. */
    chargeImpactMs?: number
  ) {
    const monster = this.monsters.get(monsterId)
    if (!monster || monster.state === 'dead') return

    // Resolve the attacker's weapon from local inventory or remote state.
    const state = get(gameStore)
    const isLocalPlayerAttack = playerId === state.currentPlayer?.id
    const weaponItemDefId = isLocalPlayerAttack
      ? get(inventoryStore).equipped.main_hand?.item_def_id
      : (state.otherPlayers.get(playerId)?.mainHand ?? undefined)
    const ranged = isRangedWeapon(weaponItemDefId)

    if (daggerSkill) {
      monster.targetPlayerId = playerId
      monster.isLastHitSuccess = hit
      if (isLocalPlayerAttack) {
        this.emitDamageText(monster, damage, hit, critical)
        if (hit)
          playSwordHitSound(
            getMaterialHitSoundUrl(
              'metal',
              getMonsterDef(monster.type)?.material
            )
          )
      }
      if (!hit) playSwordMissSound(getMaterialMissSoundUrl('metal'), 0)
      if (hit) {
        this.applyMonsterPose(monster, { state: 'hit' })
        this.restartHitClip(monster)
      }
      this.monsters.set(monsterId, { ...monster })
      return
    }

    // A ranged shot resolves when its arrow lands, so everything the impact
    // drives waits out the flight on top of the release.
    const shooter = isLocalPlayerAttack
      ? state.currentPlayer?.position
      : remotePlayerManager.players.get(playerId)?.position
    const flightMs = ranged
      ? flightMsFor(
          Math.hypot(
            shortestWrappedDeltaX(
              shooter?.x ?? monster.position.x,
              monster.position.x
            ),
            monster.position.z - (shooter?.z ?? monster.position.z)
          )
        )
      : 0
    monster.impactDelay = ranged
      ? PLAYER_RANGED_IMPACT_DELAY_MS + flightMs
      : chargeImpactMs !== undefined
        ? Math.max(1, chargeImpactMs)
        : PLAYER_ATTACK_IMPACT_DELAY_MS
    monster.targetPlayerId = playerId
    monster.isLastHitSuccess = hit
    const weaponMaterial = weaponItemDefId
      ? getItemDef(weaponItemDefId)?.material
      : undefined
    // Bow material describes the stave, so skip melee impact sounds for arrows.
    if (hit && isLocalPlayerAttack && !ranged) {
      const monsterMaterial = getMonsterDef(monster.type)?.material
      monster.pendingSwordHitSoundUrl = getMaterialHitSoundUrl(
        weaponMaterial,
        monsterMaterial
      )
    } else {
      monster.pendingSwordHitSoundUrl = undefined
    }
    if (ranged) {
      playBowSound('draw', PLAYER_RANGED_DRAW_DELAY_MS)
      playBowSound('release', PLAYER_RANGED_IMPACT_DELAY_MS)
      // GameScene reads the bow's position at release time.
      window.setTimeout(
        () =>
          requestArrow({ playerId, monsterId, hit, flightMs, ammoItemDefId }),
        PLAYER_RANGED_IMPACT_DELAY_MS
      )
    } else if (!hit) {
      // Every swing that misses whooshes, another player's included.
      playSwordMissSound(
        getMaterialMissSoundUrl(weaponMaterial),
        chargeImpactMs ?? SWORD_MISS_DELAY_MS
      )
    }
    if (isLocalPlayerAttack) {
      monster.pendingDamageText = {
        delay: monster.impactDelay + DAMAGE_TEXT_AFTER_IMPACT_MS,
        damage,
        hit,
        critical,
      }
    }

    // Trigger reactivity
    this.monsters.set(monsterId, { ...monster })
  }

  /** Struck without a swing (Healing Prayer searing an undead, a rolling
   *  cart): it flinches `delayMs` from now. */
  handleStruck(
    monsterId: string,
    playerId: number,
    damage: number,
    delayMs = 0
  ) {
    const monster = this.monsters.get(monsterId)
    if (!monster || monster.state === 'dead') return
    monster.impactDelay = Math.max(1, delayMs)
    monster.targetPlayerId = playerId
    monster.isLastHitSuccess = true
    monster.pendingSwordHitSoundUrl = undefined
    if (playerId === get(gameStore).currentPlayer?.id)
      monster.pendingDamageText = {
        delay: monster.impactDelay + DAMAGE_TEXT_AFTER_IMPACT_MS,
        damage,
        hit: true,
        critical: false,
      }
    this.monsters.set(monsterId, { ...monster })
  }

  handleMonsterSkillStarted(
    monsterId: string,
    skill: string,
    rotation: number,
    range: number,
    halfAngle: number
  ) {
    const monster = this.monsters.get(monsterId)
    if (!monster || monster.state === 'dead' || !MONSTER_SKILL_DEFS[skill])
      return
    this.applyMonsterPose(monster, { rotation, state: 'attack' })
    monster.skill = {
      id: skill,
      counter: (monster.skill?.counter ?? 0) + 1,
      startedAt: globalThis.performance?.now() ?? Date.now(),
      rotation,
      range,
      halfAngle,
    }
    this.monsters.set(monsterId, { ...monster })
  }

  handleMonsterStunned(monsterIds: string[], durationMs: number) {
    const until = (globalThis.performance?.now() ?? Date.now()) + durationMs
    for (const id of monsterIds) {
      const monster = this.monsters.get(id)
      if (!monster || monster.state === 'dead') continue
      monster.stunnedUntil = until
      this.monsters.set(id, { ...monster })
    }
  }

  handleMonsterSlowed(monsterId: string, moveMult: number, durationMs: number) {
    const monster = this.monsters.get(monsterId)
    if (!monster || monster.state === 'dead') return
    monster.slowedUntil =
      (globalThis.performance?.now() ?? Date.now()) + durationMs
    monster.slowMult = moveMult
  }

  private stepSpeed(monster: MonsterData) {
    if (monster.slowedUntil === undefined) return monster.moveSpeed
    if (monster.slowedUntil > performance.now())
      return monster.moveSpeed * (monster.slowMult ?? 1)
    monster.slowedUntil = undefined
    return monster.moveSpeed
  }

  /** A killing blow's flinch is `deathPlaysHit`'s call. */
  private shrugsOffHit(monster: MonsterData) {
    return (
      !monster.isDeadPending &&
      getMonsterDef(monster.type)?.staggerImmune === true
    )
  }

  /** A cast owns the pose: its strikes start no swing and cause no flinch. */
  isCasting(monster: MonsterData) {
    const skill = monster.skill
    if (!skill || monster.state === 'dead') return false
    const now = globalThis.performance?.now() ?? Date.now()
    return now - skill.startedAt < MONSTER_SKILL_DEFS[skill.id].durationMs
  }

  // Facing is the client's call: the server's rotation lags the target.
  handleMonsterAttackStarted(
    monsterId: string,
    dedupeWindowMs = 0,
    target?: { x: number; z: number }
  ) {
    const monster = this.monsters.get(monsterId)
    if (!monster || monster.state === 'dead' || this.isCasting(monster)) return

    const now = globalThis.performance?.now() ?? Date.now()
    if (
      dedupeWindowMs > 0 &&
      monster.lastAttackStartedAt !== undefined &&
      now - monster.lastAttackStartedAt < dedupeWindowMs
    ) {
      return
    }

    let rotation: number | undefined
    let fromLeft = false
    if (target) {
      const dx = shortestWrappedDeltaX(monster.position.x, target.x)
      const dz = target.z - monster.position.z
      if (dx !== 0 || dz !== 0) rotation = Math.atan2(dx, dz)
      const r = monster.rotation
      fromLeft = dx * Math.cos(r) - dz * Math.sin(r) > 0
    }
    this.applyMonsterPose(monster, { rotation, state: 'attack' })
    monster.attackFromLeft = fromLeft
    monster.attackCounter = (monster.attackCounter ?? 0) + 1
    monster.lastAttackStartedAt = now
    this.monsters.set(monsterId, { ...monster })
  }

  getMonsterAttackDamageTextDelayMs(monsterId: string) {
    const monster = this.monsters.get(monsterId)
    if (!monster) return DEFAULT_MONSTER_ATTACK_IMPACT_DELAY_MS
    if (this.isCasting(monster)) return 0

    const def = getMonsterDef(monster.type)
    return (
      def?.attackDamageTextDelay ??
      def?.attackImpactDelay ??
      DEFAULT_MONSTER_ATTACK_IMPACT_DELAY_MS
    )
  }

  private emitDamageText(
    monster: MonsterData,
    damage: number,
    hit: boolean,
    critical = false
  ) {
    monster.lastDamageInfo = {
      damage,
      hit,
      critical,
      trigger: (monster.lastDamageInfo?.trigger || 0) + 1,
    }
  }

  reset() {
    for (const id of [...this.bodies]) this.removeBody(id)
    this.bodyGround.clear()
    this.monsters.clear()
    clearXpArrival()
  }

  /** A raid boss goes back to sleep: it drops its fight and stands idle,
   *  bodiless at once like the server's, then sinks from `sinkAt`. */
  beginSleep(id: string, sinkAt: number) {
    const monster = this.monsters.get(id)
    if (!monster) return
    this.removeBody(id)
    this.monsters.set(id, {
      ...monster,
      state: 'idle',
      skill: undefined,
      sinkAt,
    })
  }

  /** A corpse, or a boss going back to sleep: already gone on the server. */
  isGone(id: string) {
    const monster = this.monsters.get(id)
    return monster?.state === 'dead' || monster?.sinkAt !== undefined
  }

  update(deltaTime: number) {
    // FSM & Movement Logic
    const gameState = get(gameStore)
    const now = Date.now()
    for (const monster of this.monsters.values()) {
      if (monster.sinkAt !== undefined) {
        if (now - monster.sinkAt >= SLEEP_SINK_MS) this.drop(monster.id)
        continue
      }
      const terrainY = this.monsterGroundY(
        monster,
        monster.position.x,
        monster.position.z
      )
      if (Math.abs(monster.position.y - terrainY) > MONSTER_POSITION_EPSILON) {
        this.applyMonsterPose(monster, {
          position: { ...monster.position, y: terrainY },
        })
      }

      let impactJustExpired = false
      let damageTextFired = false

      // Impact Delay Handling (Global for all clients to keep visuals synced)
      if (monster.impactDelay !== undefined && monster.impactDelay > 0) {
        monster.impactDelay -= deltaTime
        if (monster.impactDelay <= 0) {
          monster.impactDelay = 0
          impactJustExpired = true

          if (monster.isDeadPending) {
            this.playDeathSound(monster)
            // Fatal impact: optionally play hit first, then transition to death
            // when the hit clip reports completion. Monsters with an awkward hit
            // clip (deathPlaysHit=false) go straight to the death clip.
            const leadWithHit =
              monster.isLastHitSuccess && this.deathPlaysHitFor(monster)
            this.applyMonsterPose(monster, {
              state: leadWithHit ? 'hit' : 'dead',
            })
            if (leadWithHit) {
              this.restartHitClip(monster)
            } else {
              monster.isDeadPending = false
            }
          } else if (monster.isLastHitSuccess) {
            // Restart the hit clip for repeated impacts.
            this.applyMonsterPose(monster, { state: 'hit' })
            this.restartHitClip(monster)
          } else if (monster.targetPlayerId && monster.state !== 'attack') {
            // Show retaliation after a missed swing.
            this.applyMonsterPose(monster, { state: 'attack' })
          }
        }
      }

      // Backstop: never leave a killed monster standing if the hit clip's
      // completion event is missed.
      if (monster.isDeadPending && !monster.impactDelay) {
        monster.deadPendingTimer = (monster.deadPendingTimer ?? 0) + deltaTime
        if (monster.deadPendingTimer > DEAD_PENDING_TIMEOUT_MS) {
          this.finishPendingDeath(monster)
        }
      }

      // Release the damage number once its attack-start delay has elapsed.
      if (monster.pendingDamageText) {
        monster.pendingDamageText.delay -= deltaTime
        if (monster.pendingDamageText.delay <= 0) {
          const { damage, hit, critical } = monster.pendingDamageText
          monster.pendingDamageText = undefined
          this.emitDamageText(monster, damage, hit, critical)
          damageTextFired = true
        }
      }

      const blended = this.absorbSyncCorrection(monster, deltaTime)
      if (
        monster.state !== 'dead' &&
        !monster.isDeadPending &&
        this.isMovementState(monster.state) &&
        monster.targetPosition
      ) {
        const aim =
          this.liveChaseAim(monster, gameState) ?? monster.targetPosition
        this.moveTowards(monster, aim, deltaTime)
        this.monsters.set(monster.id, { ...monster })
      } else if (blended || impactJustExpired || damageTextFired) {
        this.monsters.set(monster.id, { ...monster })
      }
    }
  }

  // Dungeon monsters path on their depth's passability floor; surface
  // monsters use the open overworld (0).
  private pathFloorFor(monster: MonsterData): number {
    const fl = monster.floorLevel ?? 0
    return fl < 0 && dungeonManager.active
      ? dungeonManager.passabilityFloor(-fl)
      : 0
  }

  private updateMoveSpeedFromState(monster: MonsterData) {
    const def = getMonsterDef(monster.type)
    if (monster.state === 'run') {
      monster.moveSpeed = def?.runSpeed ?? 8
    } else if (monster.state === 'walk') {
      monster.moveSpeed = def?.walkSpeed ?? 1
    }
  }

  private isMovementState(state: MonsterData['state']) {
    return state === 'walk' || state === 'run'
  }

  private applyMonsterPose(
    monster: MonsterData,
    update: {
      position?: Position
      rotation?: number
      state?: MonsterState
      targetPosition?: Position
    }
  ) {
    if (
      update.state === 'hit' &&
      (this.isCasting(monster) || this.shrugsOffHit(monster))
    ) {
      this.playPendingSwordHitSound(monster)
      update = { ...update, state: undefined }
    }
    if (update.state) {
      // The frame the kill starts falling: XP held for it can ride the clip.
      const falling = update.state === 'dead' && monster.state !== 'dead'
      monster.state = update.state
      this.updateMoveSpeedFromState(monster)
      if (update.state === 'hit' || update.state === 'dead') {
        this.playPendingSwordHitSound(monster)
      }
      if (falling) releaseXpArrival(monster.id)
    }

    if (update.rotation !== undefined && !this.bodies.has(monster.id)) {
      monster.rotation = update.rotation
    }

    if (update.targetPosition !== undefined) {
      monster.targetPosition = update.targetPosition
    }

    if (!update.position) return

    monster.position = update.position
  }

  updateMonsterFromNetwork(
    id: string,
    position: { x: number; y: number; z: number },
    rotation: number,
    state: MonsterData['state'],
    targetPosition: { x: number; y: number; z: number },
    chasing?: { player_id: number; stop_range: number } | null
  ) {
    const monster = this.monsters.get(id)
    if (monster) {
      // Guard: If monster is dead, don't allow state changes back to alive states
      if (monster.state === 'dead' && state !== 'dead') {
        return
      }
      monster.chaseAim = chasing
        ? { playerId: chasing.player_id, stopRange: chasing.stop_range }
        : undefined

      const hasPendingImpact =
        monster.impactDelay !== undefined && monster.impactDelay > 0
      const shouldDelayNetworkHit = hasPendingImpact && state === 'hit'

      const jumpDx = shortestWrappedDeltaX(monster.position.x, position.x)
      const jumpDz = position.z - monster.position.z
      const jump = Math.hypot(jumpDx, jumpDz)
      const soften =
        jump > MONSTER_POSITION_EPSILON &&
        jump < SYNC_BLEND_MAX_METERS &&
        state !== 'dead' &&
        monster.state !== 'dead'
      monster.syncCorrection = soften ? { x: jumpDx, z: jumpDz } : undefined
      const snappedPosition = soften
        ? monster.position
        : (this.snapToMonsterGround(monster, position) ?? position)
      const snappedTargetPosition =
        this.snapToMonsterGround(monster, targetPosition) ?? targetPosition
      // Authoritative update: apply position/target directly (no movement gate).
      // When the hit is delayed, omit `state` so the current state is kept until
      // the pending impact resolves.
      this.applyMonsterPose(monster, {
        position: snappedPosition,
        rotation,
        state: shouldDelayNetworkHit ? undefined : state,
        targetPosition: snappedTargetPosition,
      })
      this.monsters.set(id, { ...monster })
    }
  }

  // A chasing monster walks at its target's live local position — exact for
  // the current player, interpolated for remotes — instead of the sync-old
  // leg target, so a head-on engagement starts where both actually stand.
  private liveChaseAim(
    monster: MonsterData,
    gameState: GameState
  ): { x: number; y: number; z: number } | undefined {
    const chase = monster.chaseAim
    const legTarget = monster.targetPosition
    if (!chase || !legTarget) return undefined
    const live =
      gameState.currentPlayer?.id === chase.playerId
        ? gameState.currentPlayer.position
        : remotePlayerManager.players.get(chase.playerId)?.position
    if (!live) return undefined
    const aim = computeChaseAim(
      monster.position,
      legTarget,
      live,
      chase.stopRange
    )
    if (!aim || aim === monster.position) return aim
    return housingManager.isMovementBlocked(
      monster.position.x,
      monster.position.z,
      unwrapWorldXNear(monster.position.x, aim.x),
      aim.z,
      this.pathFloorFor(monster),
      monster.position.y
    )
      ? undefined
      : aim
  }

  private absorbSyncCorrection(monster: MonsterData, deltaTime: number) {
    const c = monster.syncCorrection
    if (!c) return false
    const rate = this.isMovementState(monster.state)
      ? this.stepSpeed(monster) * SYNC_CATCHUP_FRACTION
      : monster.state === 'hit' || monster.state === 'attack'
        ? SYNC_COMBAT_ABSORB_MPS
        : SYNC_IDLE_ABSORB_MPS
    const remaining = Math.hypot(c.x, c.z)
    const step = (rate * deltaTime) / 1000
    const p = monster.position
    const done = this.moveTowards(
      monster,
      { x: wrapWorldX(p.x + c.x), y: p.y, z: p.z + c.z },
      deltaTime,
      rate,
      false
    )
    if (done) {
      monster.syncCorrection = undefined
    } else {
      const keep = 1 - step / remaining
      c.x *= keep
      c.z *= keep
    }
    return true
  }

  // Server rotation arrives only per sync, so the step sets the heading;
  // a sync-correction nudge must not (`face`), it is not where it's going.
  private moveTowards(
    monster: MonsterData,
    target: { x: number; y: number; z: number },
    deltaTime: number, // in ms
    speed = this.stepSpeed(monster),
    face = true
  ): boolean {
    // Positions are canonical, so a step toward a target across the seam has
    // to take the periodic short path and stay canonical afterwards.
    const dx = shortestWrappedDeltaX(monster.position.x, target.x)
    const dz = target.z - monster.position.z
    const distance = Math.sqrt(dx * dx + dz * dz)
    if (
      face &&
      distance > MONSTER_POSITION_EPSILON &&
      !this.bodies.has(monster.id)
    ) {
      monster.rotation = Math.atan2(dx, dz)
    }

    const moveStep = (speed * deltaTime) / 1000
    // Dungeon floors live below Y=0, so the "stepped into water" guard
    // only applies to surface monsters.
    const inDungeon = (monster.floorLevel ?? 0) < 0

    if (distance <= moveStep) {
      const targetX = wrapWorldX(target.x)
      const y = this.monsterGroundY(monster, targetX, target.z)
      if (!inDungeon && y < 0) return true
      this.applyMonsterPose(monster, {
        position: { x: targetX, y, z: target.z },
      })
      return true
    } else {
      const newX = wrapWorldX(monster.position.x + (dx / distance) * moveStep)
      const newZ = monster.position.z + (dz / distance) * moveStep
      const y = this.monsterGroundY(monster, newX, newZ)
      if (!inDungeon && y < 0) return true
      this.applyMonsterPose(monster, {
        position: { x: newX, y, z: newZ },
      })
      return false
    }
  }
}

export const monsterManager = hmrSingleton(
  'monsterManager',
  () => new MonsterManager()
)

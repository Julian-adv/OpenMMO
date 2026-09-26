<script lang="ts">
  import type { PlayerInteraction } from '../network/networkTypes'
  import { ServerMovement } from './player-control/server-movement'
  import { syncOwnFloor, ownPlayerFloor } from '../network/ownFloor'
  import { translate } from '../i18n'
  import { onMount } from 'svelte'
  import { localTeleportActive } from '../stores/teleportEffectStore'
  import {
    inspectionTargeting,
    cancelInspection,
    takeInspectionTarget,
  } from '../stores/inspectionStore'
  import {
    fishingTargeting,
    cancelFishingTargeting,
  } from '../stores/fishingStore'
  import { landscapingMode } from '../stores/landscapingStore'
  import {
    estateFurnitureEditorActive,
    estateFurniturePlacementMode,
    estateFurnitureSelectionMode,
  } from '../stores/estateFurniturePlacementStore'
  import { useThrelte } from '@threlte/core'
  import * as THREE from 'three'
  import {
    gameStore,
    hoverTarget,
    addChatMessage,
    reportSkillFailure,
    type LocalPlayer,
  } from '../stores/gameStore'
  import { travelDestination } from '../stores/travelStore'
  import {
    planTravelLeg,
    travelDistance,
    type TravelDestination,
  } from '../utils/autoTravel'
  import { networkManager } from '../network/socket'
  import { monsterManager } from '../managers/monsterManager'
  import { remotePlayerManager } from '../managers/remotePlayerManager'
  import { groundItemManager } from '../managers/groundItemManager'
  import { combatController } from '../managers/combatController'
  import {
    consumeDaggerSkill,
    daggerSkillState,
    daggerSkillCasts,
    playDaggerSkill,
    clearDaggerCast,
  } from '../stores/daggerSkillStore'
  import { DAGGER_SKILL } from '../data/daggerSkill'
  import {
    abilityEquipmentAllowed,
    AUSCULTATION,
    FISHING,
    isAbilityAvailable,
  } from '../data/abilities'
  import {
    playPropSound,
    preloadFishingSounds,
    preloadBowSounds,
    preloadPropSounds,
    preloadMonsterDeathSounds,
    preloadPlayerDeathSounds,
    preloadPlayerHurtSounds,
    preloadSwordHitSound,
    preloadSwordMissSound,
  } from '../managers/sfxManager'
  import {
    inputHandler,
    hoverTargetKey,
    type ClickIntent,
    type HoverTarget,
  } from '../managers/inputHandler'
  import { getNpcCapabilities } from '../data/traderDefs'
  import { tipHatManager } from '../managers/tipHatManager'
  import { mealManager } from '../managers/mealManager'
  import { activeDebuffs } from '../stores/debuffStore'
  import { staggerRadius, staggerTarget } from './player-control/stagger'
  import { tipHatDialog } from '../stores/tipHatStore'
  import { npcContextMenu, requestChatFocus } from '../stores/npcMenuStore'
  import {
    mapEditorMode,
    housingEditorMode,
    torchLightEnabled,
    cameraRotationEnabled,
    teleportLoading,
  } from '../stores/debugStore'
  import { localTorchEquipped, inventoryStore } from '../stores/inventoryStore'
  import { hungerState, SPRINT_MIN_SATIATION } from '../stores/hungerStore'
  import { isRangedWeapon, weaponRangeMeters } from '../data/itemDefs'
  import { type Position, type PlayerState } from '../utils/movementUtils'
  import { isMounted } from '../utils/mounts'
  import type { TerrainHeightManager } from '../managers/terrainHeightManager'
  import {
    playerInsideHouseId,
    playerVisualFloorLevel,
  } from '../stores/housingStore'
  import { currentDungeonDepth } from '../stores/dungeonStore'
  import { dungeonManager } from '../managers/dungeonManager'
  import { housingManager } from '../managers/housingManager'
  import {
    shouldIgnoreImplicitHouseFloorChange,
    wallApproachPositions,
    type ClosedHouseDoor,
  } from '../managers/housing-queries'
  import { PROP_SWING_IMPACT_MS } from '../data/combatTiming'
  import {
    DUNGEON_DOOR_APPROACH,
    HOUSE_DOOR_APPROACH,
    NPC_TRADE_APPROACH,
    approachForInteraction,
    PICKUP_APPROACH,
    PROP_APPROACH,
    STALL_TRADE_APPROACH,
    TIP_HAT_APPROACH,
  } from '../data/approachRanges'
  import {
    fishing_is_stern_cast,
    max_cast_distance_m,
    passability_get_floor_at,
  } from '../wasm/onlinerpg_shared'
  import { derived, get } from 'svelte/store'
  import {
    sprintRequested,
    keyboardMovementMode,
  } from '../stores/movementSettings'
  import { createPlayerPhysics } from './player-control/player-physics'
  import { subscribePlayerNetworkEvents } from './player-control/player-network-events'
  import type {
    PlayerControlEvent,
    PlayerControlUpdateOptions,
  } from './player-control/events'
  import {
    projectPlayerState,
    projectStoppedPlayerState,
    shouldEmitProjectedPlayerState,
  } from './player-control/fsm/projection'
  import { prepareMoveRequest } from './player-control/fsm/move-request'
  import { KeyboardDirectionSender } from './player-control/keyboard-direction'
  import {
    dispatchPlayerControlEvent as dispatchQueuedPlayerControlEvent,
    createCanvasIntentEvent,
    type PlayerControlEventActions,
  } from './player-control/fsm/events'
  import { worldView } from '../network/worldView'
  import {
    transitionToDeadState,
    transitionToRespawnedState,
  } from './player-control/fsm/lifecycle'
  import {
    exitPickupInteraction as buildExitPickupInteraction,
    handlePickupGrab,
    beginPickupInteraction,
    beginObjectInteraction,
    exitObjectInteraction as buildExitObjectInteraction,
    handleInteractKey,
    getInteractionExitKind,
  } from './player-control/fsm/interaction'
  import {
    planApproach,
    resolveApproach,
    type ApproachSpec,
    type PendingApproach,
    type RouteQuality,
  } from './player-control/fsm/approach'

  import {
    beginAttack,
    ensureAttackState,
    transitionAttackToIdle,
  } from './player-control/fsm/combat'
  import {
    buildAttackState,
    buildInteractState,
  } from './player-control/player-state-builders'
  import type {
    MovingControlState,
    PickingUpControlState,
    PlayerControlStateName,
  } from './player-control/fsm/control-state'
  import { createLocalPlayerControlMachine } from './player-control/fsm/state-definitions'
  import { shortestWrappedDeltaX, wrapWorldX } from '../terrain/world-wrap'
  import {
    emoteRequest,
    emoteStopRequest,
    localEmoteAnim,
    HELD_EMOTE_ANIMS,
    isEmoteAnim,
    isSelfEndingEmote,
  } from '../stores/emoteStore'
  import { respawnPoseRequest } from '../stores/respawnPoseStore'
  import { objectManager } from '../managers/objectManager'
  import { SitAnimationName } from '../types/animations'

  interface Props {
    onStateChange: (state: PlayerState) => void
    camera: THREE.Camera
    heightManager: TerrainHeightManager
    groundMeshes: THREE.Object3D[]
    groundItemMeshes: THREE.Object3D[]
    tipHatMeshes: THREE.Object3D[]
    stallMeshes: THREE.Object3D[]
    mealMeshes: THREE.Object3D[]
    monsterMeshes: THREE.Group[]
    /** Invisible bind-pose boxes, one per monster — the 20 Hz hover raycast
     *  tests these instead of the skinned triangles. */
    monsterHoverMeshes: THREE.Group[]
    npcMeshes?: THREE.Object3D[]
    playerMeshes?: THREE.Object3D[]
    /** Invisible boxes, one per remote player, for the hover raycast. */
    playerHoverMeshes?: THREE.Object3D[]
    doorMeshes: THREE.Object3D[]
    objectMeshes: THREE.Object3D[]
    propMeshes: THREE.Object3D[]
    attackCooldown?: number
    /** Baked water surface height at a world XZ (for fishing cast detection). */
    waterSurfaceAt?: (x: number, z: number) => number
  }

  let {
    onStateChange,
    camera,
    heightManager,
    groundMeshes,
    groundItemMeshes,
    tipHatMeshes,
    stallMeshes,
    mealMeshes,
    monsterMeshes,
    monsterHoverMeshes,
    npcMeshes = [],
    playerMeshes = [],
    playerHoverMeshes = [],
    doorMeshes,
    objectMeshes,
    propMeshes,
    attackCooldown,
    waterSurfaceAt,
  }: Props = $props()

  let currentPlayer = $state<LocalPlayer | null>(null)
  let officialPosition: Position | null = null
  const serverMovement = new ServerMovement(
    () => networkManager.nextMoveRequestId(),
    (goal) => networkManager.sendMoveGoal(goal),
    (requestId) => networkManager.sendMoveStop(requestId),
    (input) => networkManager.sendMoveDirection(input)
  )

  const { renderer } = useThrelte()

  const { isMovementBlocked } = createPlayerPhysics({
    getPassabilityFloor: currentPassabilityFloor,
  })

  let clickSprinting = false
  let autoTravelTarget: TravelDestination | null = null
  let travelPlayerId: number | null = null
  let travelProgressPosition: TravelDestination | null = null
  let travelStalledMs = 0
  let travelPlanCooldownMs = 0

  function cancelAutoTravel(message?: string) {
    if (!autoTravelTarget) return
    travelDestination.set(null)
    if (message) addChatMessage({ text: message, sender: 'system' })
  }

  function updateAutoTravel(deltaTime: number) {
    if (!autoTravelTarget) return
    if (
      !currentPlayer ||
      currentPlayer.health <= 0 ||
      $currentDungeonDepth > 0 ||
      $playerVisualFloorLevel > 0 ||
      $playerInsideHouseId !== null ||
      inputHandler.hasKeysPressed
    ) {
      cancelAutoTravel()
      return
    }
    if (getInteractionExitKind(playerState) !== 'none') return
    travelPlanCooldownMs = Math.max(0, travelPlanCooldownMs - deltaTime)
    const position = currentPlayer.position
    if (
      !travelProgressPosition ||
      travelDistance(travelProgressPosition, position) > 0.5
    ) {
      travelProgressPosition = { x: position.x, z: position.z }
      travelStalledMs = 0
    } else {
      travelStalledMs += Math.min(deltaTime, 100)
    }
    if (travelStalledMs > 15_000) {
      cancelAutoTravel(translate('travel.unavailable'))
      return
    }
    if (serverMovement.active || travelPlanCooldownMs > 0) return
    travelPlanCooldownMs = 500
    const leg = planTravelLeg(position, autoTravelTarget, (x, z) =>
      heightManager.hasHeightData(x, z)
    )
    if (leg.kind === 'waiting') return
    if (leg.kind === 'arrived') {
      cancelAutoTravel(translate('travel.arrived'))
      return
    }
    if (leg.kind === 'blocked') {
      cancelAutoTravel(translate('travel.noRoute'))
      return
    }
    startServerMove({ ...leg.target, y: position.y }, null, true)
  }

  function sprintAvailable(): boolean {
    return ($hungerState?.satiation ?? 0) > SPRINT_MIN_SATIATION
  }

  function isSprintingNow(): boolean {
    if (!sprintAvailable()) return false
    const state = playerControlMachine?.stateName
    const moving = state === 'moving'
    // Combat chase runs (see getMovementMode) — at sprint speed, or a fleeing
    // monster outruns the player. Same satiation gate and cost as sprint.
    if (combatController.isInCombat && moving) return true
    if (clickSprinting && moving) return true
    const input = inputHandler.getMovementInput()
    return (
      inputHandler.isSprintRequested &&
      input !== null &&
      ($keyboardMovementMode === 'world' ||
        !isMounted(currentPlayer) ||
        input.forward === 1)
    )
  }

  // Character rotation and current speed
  let playerRotation = $state(0)
  let currentSpeed = $state(0)

  const STAND_UP_DURATION = 300 // ms, matches animation crossfade duration
  let standUpTimer: ReturnType<typeof setTimeout> | null = null

  function clearStandUpTimer() {
    if (!standUpTimer) return
    clearTimeout(standUpTimer)
    standUpTimer = null
  }

  let pendingExit: (() => void) | null = null
  let interactionRevision = 0

  // Prop-break swing: when the player reaches a clicked barrel/crate, swing the
  // sword once and break it at the contact frame, then drop back to idle after
  // the follow-through.
  const PROP_SWING_RETURN_MS = 1000
  let propSwingCounter = 0
  let propBreakTimer: ReturnType<typeof setTimeout> | null = null
  let propSwingIdleTimer: ReturnType<typeof setTimeout> | null = null
  let doorInteractionRetryTimer: ReturnType<typeof setTimeout> | null = null

  function clearDoorInteractionRetry() {
    if (!doorInteractionRetryTimer) return
    clearTimeout(doorInteractionRetryTimer)
    doorInteractionRetryTimer = null
  }

  function clearPropSwingTimers() {
    if (propBreakTimer) {
      clearTimeout(propBreakTimer)
      propBreakTimer = null
    }
    if (propSwingIdleTimer) {
      clearTimeout(propSwingIdleTimer)
      propSwingIdleTimer = null
    }
  }

  function enqueuePlayerControlEvent(event: PlayerControlEvent) {
    playerControlMachine.enqueueEvent(event)
  }

  // Finish the in-flight pickup (settle the ground item) using the id owned by
  // the picking_up state. Callers always transition away from picking_up right
  // after, which drops the id — so this finishes exactly once per pickup. This
  // replaces the old reactive $effect backstop (L5): every path that leaves the
  // pickup state (stand-up via click/keyboard, anim finish, dead, respawn)
  // calls finishPendingPickup() explicitly.
  function finishPendingPickup() {
    const p = pickingUpState()
    if (p) groundItemManager.finishPickup(p.pendingPickupInstanceId)
  }

  function exitPickupInteraction() {
    const transition = buildExitPickupInteraction(playerState)
    if (transition.kind === 'ignored') return

    finishPendingPickup()
    setPlayerState(transition.nextPlayerState)
    transitionTo('idle')
  }

  function onInteractionFinished() {
    // A one-shot emote ends itself; notify so the server drops the stored
    // pose and remotes clear it. Held poses (bench, forge) stay until the
    // player moves, and pickup has its own exit below.
    if (playerState.state !== 'interact') return exitPickupInteraction()
    const anim = playerState.interactionAnim ?? ''
    if (isSelfEndingEmote(anim)) {
      exitObjectInteraction()
    } else if (anim === SitAnimationName.SIT_TO_STAND) {
      const exit = pendingExit
      pendingExit = null
      completeObjectExit(true)
      exit?.()
    } else {
      exitPickupInteraction()
    }
  }

  function onPickupGrab() {
    const p = pickingUpState()
    if (!p) return
    handlePickupGrab(p.pendingPickupInstanceId, {
      setInHand: (id) => groundItemManager.setInHand(id),
      remove: (id) => groundItemManager.remove(id),
      sendPickupItem: (id) => networkManager.sendPickupItem(id),
    })
  }

  function exitObjectInteraction(notify = true, then?: () => void) {
    interactionRevision++
    const anim =
      playerState.state === 'interact' ? playerState.interactionAnim : undefined
    if (anim === SitAnimationName.SIT_TO_STAND) {
      pendingExit = then ?? null
      return
    }
    if (notify && anim === SitAnimationName.SIT) {
      setPlayerState(
        buildInteractState(
          playerState,
          playerState.position,
          playerState.rotation,
          SitAnimationName.SIT_TO_STAND,
          playerState.interactOffsetY ?? 0
        )
      )
      pendingExit = then ?? null
      return
    }
    completeObjectExit(notify)
    then?.()
  }

  function completeObjectExit(notify: boolean) {
    setPlayerState(buildExitObjectInteraction(playerState))
    transitionTo('idle')
    if (notify) networkManager.sendStopInteraction()
  }

  function stopMovement() {
    const approach = movingState()?.approach ?? null
    serverMovement.clear()
    clearStandUpTimer()
    currentSpeed = 0
    clickSprinting = false
    // Resolve the walk-up action after entering idle.
    transitionTo('idle')
    updatePlayerState()
    if (
      approach &&
      currentPlayer &&
      currentPlayer.health > 0 &&
      resolveApproach(
        approach,
        currentPlayer.position,
        get(currentDungeonDepth)
      )
    ) {
      approach.act()
    }
  }

  // Explicitly drive the machine's owned state to a data-less state. The machine
  // no longer derives its state name from flags — callers transition at the real
  // decision points. Stateful transitions (moving/picking_up) carry their data.
  function transitionTo(
    name: Exclude<PlayerControlStateName, 'moving' | 'picking_up'>
  ) {
    playerControlMachine.transition({ name })
  }

  // `isMoving` is no longer a stored flag: being in motion IS being in the
  // moving/keyboard_moving state. Derive it from the machine's owned state.
  function isMovingNow(): boolean {
    const name = playerControlMachine.stateName
    return (name === 'moving' || name === 'keyboard_moving') && currentSpeed > 0
  }

  // Narrowed views of the machine's owned state, for reading/mutating the data
  // the active state holds. Null when not in that state.
  function movingState(): MovingControlState | null {
    const s = playerControlMachine.state
    return s.name === 'moving' ? s : null
  }
  function pickingUpState(): PickingUpControlState | null {
    const s = playerControlMachine.state
    return s.name === 'picking_up' ? s : null
  }

  function stopAndFace(rotation: number) {
    if ($localTeleportActive) return
    serverMovement.clear()
    networkManager.sendPlayerFace(rotation)
  }

  const keyboardSender = new KeyboardDirectionSender(
    (input) => serverMovement.direction(input),
    () => serverMovement.clear()
  )

  function writePlayerPosition(position: Position, rotation: number) {
    const wrappedX = wrapWorldX(position.x)
    gameStore.update((state) => {
      if (state.currentPlayer) {
        state.currentPlayer.position.set(wrappedX, position.y, position.z)
        state.currentPlayer.rotation = rotation
      }
      return state
    })
  }

  // Current player state
  let playerState = $state<PlayerState>({
    state: 'idle',
    speed: 0,
    rotation: 0,
    position: { x: 0, y: 0, z: 0 },
  })

  /** Turn to face a world point (rotation only; nothing is emitted). */
  function faceTowards(x: number, z: number) {
    if (!currentPlayer) return
    const dx = shortestWrappedDeltaX(currentPlayer.position.x, x)
    const dz = z - currentPlayer.position.z
    if (dx !== 0 || dz !== 0) playerRotation = Math.atan2(dx, dz)
  }

  // The panel highlight is a projection of the real state, not a flag set on
  // enter/exit: death, attacks, fishing, and bench-sitting all leave an emote
  // without passing any single exit function.
  let lastEmoteSync: string | null = null
  function syncLocalEmote(next: PlayerState) {
    const anim =
      next.state === 'interact' && isEmoteAnim(next.interactionAnim ?? '')
        ? (next.interactionAnim ?? null)
        : null
    if (anim === lastEmoteSync) return
    lastEmoteSync = anim
    localEmoteAnim.set(anim)
  }

  function setPlayerState(next: PlayerState) {
    playerState = next
    onStateChange(next)
    syncLocalEmote(next)
  }

  function applyStoppedPlayerPosition(position: Position, rotation: number) {
    const nextState = projectStoppedPlayerState(playerState, position, rotation)
    playerRotation = nextState.rotation
    currentSpeed = 0
    writePlayerPosition(position, playerRotation)
    setPlayerState(nextState)
  }

  gameStore.subscribe((state) => {
    const previousPlayerId = currentPlayer?.id ?? null
    currentPlayer = state.currentPlayer
    if (!currentPlayer) return

    const position = {
      x: currentPlayer.position.x,
      y: currentPlayer.position.y,
      z: currentPlayer.position.z,
    }
    if (currentPlayer.id === previousPlayerId) {
      playerState.position = position
      return
    }

    if (previousPlayerId !== null) {
      serverMovement.clear(false)
      if (playerControlMachine.stateName === 'moving') transitionTo('idle')
    }
    officialPosition = { ...position }
    playerRotation = currentPlayer.rotation
    currentSpeed = 0
    setPlayerState({
      state: currentPlayer.health > 0 ? 'idle' : 'dead',
      speed: 0,
      rotation: currentPlayer.rotation,
      position,
    })
  })

  // Update player state and notify parent
  function updatePlayerState(totalDistance?: number) {
    const currentPosition = currentPlayer
      ? {
          x: currentPlayer.position.x,
          y: currentPlayer.position.y,
          z: currentPlayer.position.z,
        }
      : playerState.position

    const newState = projectPlayerState({
      currentPosition,
      isMoving: isMovingNow(),
      currentSpeed,
      playerRotation,
      totalDistance,
      hasTorch: $localTorchEquipped || $torchLightEnabled,
      isInCombat: combatController.isInCombat,
      attackCounter: combatController.attackCounter,
      isSprinting: serverMovement.stopping
        ? playerState.movementMode === 'run'
        : isSprintingNow(),
    })

    // Only update if state actually changed
    if (shouldEmitProjectedPlayerState(playerState, newState)) {
      playerState = newState
      onStateChange(newState)
      syncLocalEmote(newState)
    }
  }

  /** Reach of the wielded weapon. The server gates on the same items.json
   *  column, so click-to-attack, the chase break-off and the rejection all
   *  agree on one distance. */
  function equippedAttackRange(): number {
    return weaponRangeMeters($inventoryStore.equipped.main_hand?.item_def_id)
  }

  /** Shared by click attacks and the chase tick. */
  function attackLineBlocked(from: Position, to: Position, floor: number) {
    return housingManager.attackLineBlocked(
      from.x,
      from.z,
      to.x,
      to.z,
      floor,
      isRangedWeapon($inventoryStore.equipped.main_hand?.item_def_id)
    )
  }

  /** Take the monster as a target and walk at it, attacking on arrival. */
  function chaseAndAttack(monsterId: string, goal: Position) {
    combatController.beginCombat(monsterId, false)
    handleClickToMove(goal)
  }

  function sendCombatAttack(monsterId: string) {
    const canUseDaggerSkill =
      isAbilityAvailable(DAGGER_SKILL.clip, currentPlayer?.characterClass) &&
      abilityEquipmentAllowed(DAGGER_SKILL.clip, $inventoryStore.equipped)
    if (canUseDaggerSkill && currentPlayer && consumeDaggerSkill()) {
      playDaggerSkill(currentPlayer.id)
      networkManager.sendDaggerDoubleSlash(monsterId)
    } else {
      if (!canUseDaggerSkill)
        daggerSkillState.update((state) => ({ ...state, queued: false }))
      if (currentPlayer) clearDaggerCast(currentPlayer.id)
      networkManager.sendPlayerAttack(monsterId)
    }
  }

  // Initiate attack on a monster
  function initiateAttack(monsterId: string) {
    cancelAutoTravel()
    if (getInteractionExitKind(playerState) === 'pickup') {
      finishPendingPickup()
    }

    const monsterInfo = monsterManager.monsters.get(monsterId)

    // A wall between us refuses the blow server-side, so walk at the monster
    // instead of swinging into a rejection.
    if (
      monsterInfo &&
      currentPlayer &&
      attackLineBlocked(
        currentPlayer.position,
        monsterInfo.position,
        currentPassabilityFloor()
      )
    ) {
      chaseAndAttack(monsterId, monsterInfo.position)
      return
    }

    // Without this the first swing keeps the old facing until the next cycle.
    if (monsterInfo) {
      faceTowards(monsterInfo.position.x, monsterInfo.position.z)
    }

    const result = beginAttack({
      monsterId,
      monsterInfo,
      currentPosition: currentPlayer
        ? {
            x: currentPlayer.position.x,
            y: currentPlayer.position.y,
            z: currentPlayer.position.z,
          }
        : null,
      playerRotation,
      previousPlayerState: playerState,
      beginCombat: (id, inRange) => combatController.beginCombat(id, inRange),
      stopAndFace,
      sendPlayerAttack: sendCombatAttack,
    })

    if (result.kind === 'ignored_unattackable_target') return

    currentSpeed = 0
    setPlayerState(result.nextPlayerState)
    transitionTo('attacking')
  }

  // Transition from attack to idle state
  function transitionToIdle() {
    const transition = transitionAttackToIdle(playerState)
    if (transition.kind === 'ignored') return
    setPlayerState(transition.nextPlayerState)
    transitionTo('idle')
  }

  function transitionToDead() {
    cancelAutoTravel()
    const transition = transitionToDeadState(playerState)
    if (transition.kind === 'ignored_already_dead') return

    combatController.cancelCombat()
    inputHandler.clearTransientInput()
    currentSpeed = transition.runtime.currentSpeed
    // Finish any in-flight pickup while still in picking_up, before the dead
    // transition drops that state (L5: explicit finish on every pickup exit).
    finishPendingPickup()

    setPlayerState(transition.nextPlayerState)
    transitionTo('dead')
  }

  function transitionToRespawned() {
    cancelAutoTravel()
    if (!currentPlayer) return

    const transition = transitionToRespawnedState(playerState, {
      x: currentPlayer.position.x,
      y: currentPlayer.position.y,
      z: currentPlayer.position.z,
    })
    combatController.cancelCombat()
    inputHandler.clearTransientInput()
    clearStandUpTimer()
    pendingExit = null
    clearPropSwingTimers()
    currentSpeed = transition.runtime.currentSpeed
    playerRotation = transition.runtime.playerRotation
    finishPendingPickup()

    setPlayerState(transition.nextPlayerState)
    transitionTo('idle')
  }

  /** Check E key interaction (door toggle). Call from game loop. */
  function checkInteraction() {
    handleInteractKey({
      currentPlayer,
      consumeInteract: () => {
        const consumed = inputHandler.consumeInteract()
        if (consumed) cancelAutoTravel()
        return consumed
      },
      findNearestDoor: (x, z, y, range) =>
        housingManager.findNearestDoor(x, z, y, range),
      sendToggleDoor: (houseId, roomIndex, wallDir, segmentIndex) =>
        networkManager.sendToggleDoor(
          houseId,
          roomIndex,
          wallDir,
          segmentIndex
        ),
    })
  }

  let chaseGoal: Position | null = null

  function updatePlayerMovement(deltaTime: number) {
    if (!currentPlayer) return
    if (currentPlayer.health <= 0) {
      transitionToDead()
      return
    }
    if (playerState.state === 'dead') {
      transitionToRespawned()
      return
    }
    updateAutoTravel(deltaTime)
    const pose = serverMovement.sample((from, to) =>
      isMovementBlocked(from.x, from.z, to.x, to.z, from.y)
    )
    if (pose) {
      if (playerControlMachine.stateName === 'attacking') {
        applyStoppedPlayerPosition(pose.position, pose.rotation)
      } else {
        playerRotation = pose.rotation
        currentSpeed = pose.speed
        writePlayerPosition(pose.position, pose.rotation)
        if (
          !serverMovement.active &&
          !serverMovement.stopping &&
          playerControlMachine.stateName === 'keyboard_moving'
        )
          transitionTo('idle')
        updatePlayerState()
      }
    }
    const targetId = combatController.targetMonsterId
    if (!targetId || !officialPosition) return
    const monster = monsterManager.monsters.get(targetId)
    const target = monsterManager.findMeshPosition(targetId, monsterMeshes)
    const blocked =
      !!target &&
      attackLineBlocked(officialPosition, target, currentPassabilityFloor())
    const result = combatController.update(
      deltaTime,
      officialPosition,
      monster,
      target,
      serverMovement.active,
      (attackCooldown ? attackCooldown * 1000 : 1500) /
        ($hungerState?.attackMult ?? 1),
      playerState.state,
      blocked,
      equippedAttackRange()
    )
    switch (result.action) {
      case 'idle':
        chaseGoal = null
        stopMovement()
        transitionToIdle()
        break
      case 'reached_attack_range':
        initiateAttack(targetId)
        break
      case 'chasing':
        if (
          result.newTarget &&
          (!chaseGoal ||
            travelDistance(chaseGoal, result.newTarget) > 1.5 ||
            !serverMovement.active)
        ) {
          chaseGoal = { ...result.newTarget }
          startServerMove(chaseGoal, null, true)
        }
        break
      case 'attacking': {
        playerRotation = result.rotation
        const transition = ensureAttackState(
          playerState,
          result.rotation,
          combatController.attackCounter
        )
        if (transition.kind === 'attack')
          setPlayerState(transition.nextPlayerState)
        transitionTo('attacking')
        break
      }
      case 'attack_cycle':
        playerRotation = result.rotation
        networkManager.sendPlayerFace(playerRotation)
        sendCombatAttack(result.monsterId)
        setPlayerState(
          buildAttackState(
            playerState,
            playerRotation,
            combatController.attackCounter
          )
        )
        transitionTo('attacking')
        break
    }
  }

  function updateKeyboardMovement(_deltaTime: number) {
    const input = inputHandler.getMovementInput()
    if (
      !currentPlayer ||
      currentPlayer.health <= 0 ||
      !worldView.covers(currentPlayer.position.x, currentPlayer.position.z)
    ) {
      keyboardSender.clear()
      if (serverMovement.stopping) serverMovement.clear()
      return
    }
    if (input) {
      cancelAutoTravel()
      clearDoorInteractionRetry()
      combatController.cancelCombat()
      chaseGoal = null
      const interaction = getInteractionExitKind(playerState)
      if (interaction === 'pickup') exitPickupInteraction()
      if (interaction === 'object') {
        exitObjectInteraction()
        return
      }
      if (playerControlMachine.stateName !== 'keyboard_moving')
        transitionTo('keyboard_moving')
    }
    if (!input && playerControlMachine.stateName === 'keyboard_moving') {
      keyboardSender.reset()
      serverMovement.stopDirection()
      if (!serverMovement.stopping) {
        currentSpeed = 0
        transitionTo('idle')
        updatePlayerState()
      }
      return
    }
    keyboardSender.update(
      input,
      playerRotation,
      isMounted(currentPlayer),
      $keyboardMovementMode,
      isSprintingNow()
    )
  }

  function startServerMove(
    target: Position,
    approach: PendingApproach | null,
    sprinting: boolean,
    stopAtEntrance = false
  ) {
    interactionRevision++
    keyboardSender.reset()
    clickSprinting = sprinting
    playerControlMachine.transition({ name: 'moving', approach })
    serverMovement.request(target.x, target.z, sprinting, stopAtEntrance)
    updatePlayerState()
  }

  function createMoveRequestActions(
    clickPosition: Position,
    options: {
      approach?: PendingApproach | null
      sprinting?: boolean
      stopAtHouseEntrance?: boolean
    }
  ) {
    return {
      exitPickupAndRetry: () => {
        exitPickupInteraction()
        handleClickToMove(clickPosition, options)
      },
      exitObjectAndDelay: () => {
        exitObjectInteraction(true, () => {
          clearStandUpTimer()
          standUpTimer = setTimeout(() => {
            standUpTimer = null
            enqueuePlayerControlEvent({
              type: 'delayed_request_move',
              position: { ...clickPosition },
              approach: options.approach ?? null,
              sprinting: options.sprinting,
              stopAtHouseEntrance: options.stopAtHouseEntrance,
            })
          }, STAND_UP_DURATION)
        })
      },
    }
  }

  function currentPassabilityFloor(): number {
    const floor = get(ownPlayerFloor)
    return floor < 0 ? dungeonManager.passabilityFloor(-floor) : floor
  }

  function handleClickToMove(
    clickPosition: Position,
    options: {
      approach?: PendingApproach | null
      sprinting?: boolean
      stopAtHouseEntrance?: boolean
    } = {}
  ) {
    cancelAutoTravel()
    // Any fresh movement cancels a pending prop break/open (breakProp/openProp
    // re-arm it after their own walk-up call below).
    dungeonManager.clearPendingBreak()
    dungeonManager.clearPendingOpen()
    // Approach moves (chase, walk-up) carry no modifier: follow the preference.
    clickSprinting =
      (options.sprinting ?? sprintRequested(false)) && sprintAvailable()
    // A drunk walker weaves on free moves only; a walk-up still has to
    // arrive where its target is.
    if (!options.approach) {
      const radius = staggerRadius(get(activeDebuffs), Date.now())
      if (radius > 0) clickPosition = staggerTarget(clickPosition, radius)
    }
    if (
      !currentPlayer ||
      !Number.isFinite(clickPosition.x) ||
      !Number.isFinite(clickPosition.z)
    )
      return
    if (
      !(
        options.stopAtHouseEntrance &&
        housingManager.findHouseAtPoint(
          clickPosition.x,
          clickPosition.y,
          clickPosition.z
        )
      ) &&
      !options.approach &&
      routeQuality(clickPosition) === 'none'
    )
      return
    if (
      !prepareMoveRequest(
        {
          currentPlayerHealth: currentPlayer.health,
          interactionExit: getInteractionExitKind(playerState),
          hasCurrentPlayer: true,
          hasKeyboardInput: inputHandler.hasKeysPressed,
        },
        createMoveRequestActions(clickPosition, options)
      )
    )
      return
    startServerMove(
      clickPosition,
      options.approach ?? null,
      clickSprinting,
      options.stopAtHouseEntrance
    )
  }

  function enterInteraction(
    intent: Extract<ClickIntent, { type: 'interact_object' }>,
    claim = true
  ) {
    interactionRevision++
    cancelAutoTravel()
    if (getInteractionExitKind(playerState) === 'pickup') finishPendingPickup()
    currentSpeed = 0
    pendingExit = null
    clearStandUpTimer()
    if (claim) {
      combatController.cancelCombat()
      serverMovement.clear()
      keyboardSender.reset()
      transitionTo('idle')
      updatePlayerState()
      networkManager.sendInteractObject(intent.objectType, intent.objectId)
      return
    }
    const result = beginObjectInteraction({
      intent,
      previousPlayerState: playerState,
      cancelCombat: () => combatController.cancelCombat(),
    })
    playerRotation = result.playerRotation
    setPlayerState(result.nextPlayerState)
    transitionTo('object_interacting')
  }

  async function applyServerInteraction(interaction: PlayerInteraction) {
    const revision = ++interactionRevision
    if (!currentPlayer || currentPlayer.id !== interaction.player_id) return
    officialPosition = { ...interaction.position }
    syncOwnFloor(
      interaction.floor_level,
      interaction.position.x,
      interaction.position.z
    )
    playerRotation = interaction.rotation
    writePlayerPosition(interaction.position, interaction.rotation)
    if (!interaction.object_type) {
      if (
        playerState.state === 'interact' &&
        playerState.interactionAnim !== SitAnimationName.SIT_TO_STAND
      )
        completeObjectExit(false)
      return
    }
    if (
      interaction.object_id === null ||
      serverMovement.active ||
      inputHandler.hasKeysPressed
    )
      return
    const { anim, interactOffset } = await objectManager.resolvePose(
      interaction.object_type,
      interaction.position.x,
      interaction.position.z,
      interaction.object_id
    )
    if (
      revision !== interactionRevision ||
      serverMovement.active ||
      inputHandler.hasKeysPressed ||
      currentPlayer?.id !== interaction.player_id ||
      currentPlayer.health <= 0 ||
      $localTeleportActive
    )
      return
    enterInteraction(
      {
        type: 'interact_object',
        objectId: interaction.object_id,
        objectType: interaction.object_type,
        interaction: anim,
        position: interaction.position,
        rotation: interaction.rotation,
        interactOffset,
      },
      false
    )
  }

  /** Lie down on the bed the server respawned us on. */
  async function enterRespawnPose(objectType: string) {
    cancelAutoTravel()
    if (!currentPlayer) return
    const { x, z } = currentPlayer.position
    const { anim, interactOffset, placement, rotation } =
      await objectManager.resolvePose(objectType, x, z)
    if (!placement || rotation === undefined || !currentPlayer) return
    enterInteraction(
      {
        type: 'interact_object',
        objectId: placement.id,
        objectType,
        interaction: anim,
        position: { ...currentPlayer.position },
        rotation: currentPlayer.rotation,
        interactOffset,
      },
      false
    )
  }

  /** Enter an emote clip in place. Unlike enterInteraction there is no object
   *  to face, snap to, or claim, and the server already heard about it through
   *  the chat command — so no sendInteractObject here. */
  function startEmote(anim: string) {
    cancelAutoTravel()
    if (!currentPlayer) return
    if (getInteractionExitKind(playerState) === 'pickup') {
      finishPendingPickup()
    }

    // Leftover deceleration would let the movement tick's resetStoppedSpeed
    // project the fresh interact state back to idle one frame later.
    currentSpeed = 0
    pendingExit = null

    const result = beginObjectInteraction({
      intent: {
        type: 'interact_object',
        objectId: 0,
        objectType: anim,
        interaction: anim,
        position: {
          x: currentPlayer.position.x,
          y: currentPlayer.position.y,
          z: currentPlayer.position.z,
        },
        rotation: playerRotation,
      },
      previousPlayerState: playerState,
      cancelCombat: () => combatController.cancelCombat(),
    })

    setPlayerState(result.nextPlayerState)
    transitionTo('object_interacting')
  }

  $effect(() => {
    const anim = $emoteRequest
    if (!anim) return
    emoteRequest.set(null)
    startEmote(anim)
  })

  $effect(() => {
    const objectType = $respawnPoseRequest
    if (!objectType) return
    respawnPoseRequest.set(null)
    void enterRespawnPose(objectType)
  })

  $effect(() => {
    if (!$emoteStopRequest) return
    emoteStopRequest.set(false)
    // Only a performance ends here — the tune running out or Escape. By now
    // the player may have sat down on something, and that pose is not ours
    // to cancel.
    if (
      playerState.state === 'interact' &&
      HELD_EMOTE_ANIMS.has(playerState.interactionAnim ?? '')
    ) {
      exitObjectInteraction()
    }
  })

  function enterPickup(instanceId: number) {
    cancelAutoTravel()
    // Face the item: an in-reach click never walks, and a blocked walk-up
    // stops facing its travel direction.
    const item = groundItemManager.items.get(instanceId)
    if (item) faceTowards(item.position.x, item.position.z)

    const result = beginPickupInteraction({
      instanceId,
      previousPlayerState: { ...playerState, rotation: playerRotation },
      hasGroundItem: () => item !== undefined,
      beginPickup: (id) => groundItemManager.beginPickup(id),
      cancelCombat: () => combatController.cancelCombat(),
    })

    if (result.kind === 'ignored') return

    // The picking_up state OWNS the instance id being grabbed; entering it drops
    // any moving data (the far-pickup approach that led here).
    currentSpeed = 0
    if (currentPlayer) stopAndFace(playerRotation) // others see the facing
    networkManager.sendPickupStarted()
    setPlayerState(result.nextPlayerState)
    playerControlMachine.transition({
      name: 'picking_up',
      pendingPickupInstanceId: result.pendingPickupInstanceId,
    })
  }

  function routeQuality(target: Position): RouteQuality {
    if (!Number.isFinite(target.x) || !Number.isFinite(target.z)) return 'none'
    return housingManager.isCircleBlocked(
      target.x,
      target.z,
      0.05,
      currentPassabilityFloor(),
      currentPlayer?.position.y ?? target.y
    )
      ? 'none'
      : 'found'
  }

  /** `canActNow` is false while an interaction animation still has to be
   *  exited — the walk-up runs the exit, then fires the action on arrival. */
  function approachAndAct(
    spec: ApproachSpec,
    act: () => void,
    canActNow = true,
    canAct?: PendingApproach['canAct']
  ) {
    if (!currentPlayer || currentPlayer.health <= 0) return 'ignored'

    const plan = planApproach(
      currentPlayer.position,
      spec,
      routeQuality,
      canActNow,
      canAct
    )
    if (plan.kind === 'unreachable') return 'unreachable'
    if (plan.kind === 'act_now') {
      act()
      return 'act_now'
    }

    combatController.cancelCombat()
    handleClickToMove(plan.target, {
      approach: { spec, depth: get(currentDungeonDepth), canAct, act },
    })
    return 'walk'
  }

  function pickupItem(
    intent: Extract<ClickIntent, { type: 'pickup_ground_item' }>
  ) {
    if (playerState.state === 'dead') return
    const item = groundItemManager.items.get(intent.instanceId)
    // Never pick up straight from an interaction: re-entering picking_up would
    // overwrite the owned id and strand the grabbed item on the hand bone
    // (finishPickup never runs). The walk-up settles the interaction first.
    approachAndAct(
      { position: item?.position ?? intent.position, ...PICKUP_APPROACH },
      () => enterPickup(intent.instanceId),
      getInteractionExitKind(playerState) === 'none'
    )
  }

  function openDoorThenRetry(door: ClosedHouseDoor, retryAction: () => void) {
    clearDoorInteractionRetry()
    networkManager.sendToggleDoor(
      door.houseId,
      door.roomIndex,
      door.wallDir,
      door.segmentIndex
    )

    let attempts = 0
    const retry = () => {
      doorInteractionRetryTimer = null
      if (housingManager.isDoorOpen(door)) {
        retryAction()
        return
      }
      attempts++
      if (attempts < 20) doorInteractionRetryTimer = setTimeout(retry, 100)
    }
    doorInteractionRetryTimer = setTimeout(retry, 100)
  }

  function approachDoorThenRetry(
    door: ClosedHouseDoor,
    retryAction: () => void
  ) {
    if (!currentPlayer) return
    approachAndAct(
      {
        position: { ...door.position, y: currentPlayer.position.y },
        ...HOUSE_DOOR_APPROACH,
      },
      () => openDoorThenRetry(door, retryAction)
    )
  }

  function interactObject(
    intent: Extract<ClickIntent, { type: 'interact_object' }>,
    forceWalk = false
  ) {
    if (!currentPlayer) return
    const player = currentPlayer
    const floor = currentPassabilityFloor()
    const door = housingManager.findClosedDoorOnSegment(
      player.position.x,
      player.position.z,
      intent.position.x,
      intent.position.z,
      floor
    )
    if (door) {
      approachDoorThenRetry(door, () => interactObject(intent, true))
      return
    }

    const canAct = (position: Pick<Position, 'x' | 'z'>) =>
      !housingManager.isHouseWallBlockingSegment(
        position.x,
        position.z,
        intent.position.x,
        intent.position.z,
        floor
      )
    const spec = {
      position: intent.position,
      ...approachForInteraction(intent.interaction),
    }
    const approach = approachAndAct(
      spec,
      () => enterInteraction(intent),
      !forceWalk && canAct(player.position),
      canAct
    )
    return approach
  }

  function toggleDoor(intent: Extract<ClickIntent, { type: 'toggle_door' }>) {
    const toggle = () =>
      networkManager.sendToggleDoor(
        intent.houseId,
        intent.roomIndex,
        intent.wallDir,
        intent.segmentIndex
      )
    if (intent.isWindow && currentPlayer) {
      const player = currentPlayer
      const floor = currentPassabilityFloor()
      const dx = shortestWrappedDeltaX(player.position.x, intent.position.x)
      const dz = intent.position.z - player.position.z
      if (
        Math.hypot(dx, dz) <= HOUSE_DOOR_APPROACH.range &&
        !housingManager.isHouseWallBlockingSegment(
          player.position.x,
          player.position.z,
          intent.position.x,
          intent.position.z,
          floor
        )
      ) {
        toggle()
        return
      }
      const targets = wallApproachPositions(
        intent.position,
        player.position,
        intent.wallDir,
        HOUSE_DOOR_APPROACH.stopShort
      )
      for (const target of targets) {
        const position = { ...target, y: player.position.y }
        if (routeQuality(position) !== 'found') continue
        approachAndAct({ position, range: 0.35, stopShort: 0 }, toggle)
        return
      }

      return
    }
    approachAndAct(
      { position: intent.position, ...HOUSE_DOOR_APPROACH },
      toggle
    )
  }

  function toggleDungeonDoor(
    intent: Extract<ClickIntent, { type: 'toggle_dungeon_door' }>
  ) {
    approachAndAct(
      { position: intent.position, ...DUNGEON_DOOR_APPROACH },
      () => {
        const id = dungeonManager.dungeonId
        if (id) {
          networkManager.sendToggleDungeonDoor(id, intent.depth, intent.doorId)
        }
      }
    )
  }

  function tradeWithNpc(
    intent: Extract<ClickIntent, { type: 'interact_npc' }>
  ) {
    approachAndAct({ position: intent.position, ...NPC_TRADE_APPROACH }, () =>
      networkManager.sendOpenShop(intent.playerId)
    )
  }

  function tipHat(intent: Extract<ClickIntent, { type: 'tip_hat' }>) {
    approachAndAct({ position: intent.position, ...TIP_HAT_APPROACH }, () => {
      const hat = tipHatManager.hats.get(intent.hatId)
      if (hat)
        tipHatDialog.set({
          hatId: hat.id,
          ownerName: hat.owner_name,
          acceptsSongRequests: hat.accepts_song_requests,
        })
    })
  }

  /** No walk-up: the server checks that we sit at the plate's chair. */
  function eatMeal(intent: Extract<ClickIntent, { type: 'meal' }>) {
    if (mealManager.meals.get(intent.mealId)?.eaten === false) {
      networkManager.sendEatMeal(intent.mealId)
    }
  }

  /** Step up to the table; the server decides shop front or stall panel. */
  function tradeAtStall(intent: Extract<ClickIntent, { type: 'stall' }>) {
    approachAndAct({ position: intent.position, ...STALL_TRADE_APPROACH }, () =>
      networkManager.sendOpenStall(intent.stallId)
    )
  }

  /** Shared walk-up for a clicked interactive prop: move to within reach if
   *  needed (it's a solid pillar, so stop just short), then arm `setPending` so
   *  the dungeon layer fires the break/open once the player is in range. */
  function approachProp(
    intent: { depth: number; propId: number; position: Position },
    setPending: (p: {
      depth: number
      propId: number
      x: number
      z: number
    }) => void
  ) {
    if (!currentPlayer) return
    const plan = planApproach(
      currentPlayer.position,
      { position: intent.position, ...PROP_APPROACH },
      routeQuality
    )
    if (plan.kind === 'unreachable') return
    if (plan.kind === 'walk') {
      combatController.cancelCombat()
      handleClickToMove(plan.target)
    }
    setPending({
      depth: intent.depth,
      propId: intent.propId,
      x: intent.position.x,
      z: intent.position.z,
    })
  }

  /** Click a barrel/crate: walk up, then arm the break. The dungeon layer fires
   *  it via the server once the player is in range. */
  function breakProp(intent: Extract<ClickIntent, { type: 'break_prop' }>) {
    approachProp(intent, (p) => dungeonManager.setPendingBreak(p))
  }

  /** Click a chest: walk up, then arm the open. The dungeon layer sends the open
   *  via the server once in range; every client (the opener included) plays the
   *  lid animation on the broadcast. */
  function openProp(intent: Extract<ClickIntent, { type: 'open_prop' }>) {
    // Already open — nothing to do (avoid a pointless walk-up).
    if (dungeonManager.isPropOpened(intent.depth, intent.propId)) return
    approachProp(intent, (p) => dungeonManager.setPendingOpen(p))
  }

  /** The player has walked up to a clicked barrel/crate: swing the sword once
   *  and break it at the contact frame. Called from the dungeon layer the frame
   *  the player comes into range (see GameSceneDungeonLayer onPropReady). */
  export function swingAndBreakProp(
    entranceId: string,
    depth: number,
    propId: number,
    x: number,
    z: number
  ) {
    if (!currentPlayer) return
    // Don't interrupt an in-flight swing (the layer can fire across frames).
    if (playerState.state === 'attack' && propBreakTimer) return
    combatController.cancelCombat()
    clearPropSwingTimers()

    // Use a separate counter to replay each prop swing outside monster combat.
    faceTowards(x, z)
    currentSpeed = 0
    propSwingCounter += 1
    setPlayerState(
      buildAttackState(playerState, playerRotation, propSwingCounter)
    )
    transitionTo('attacking')
    stopAndFace(playerRotation) // others see the facing

    propBreakTimer = setTimeout(() => {
      propBreakTimer = null
      playPropSound('break')
      dungeonManager.noteSelfBreak(depth, propId)
      networkManager.sendBreakDungeonProp(entranceId, depth, propId)
    }, PROP_SWING_IMPACT_MS)
    propSwingIdleTimer = setTimeout(() => {
      propSwingIdleTimer = null
      if (playerState.state === 'attack') transitionToIdle()
    }, PROP_SWING_RETURN_MS)
  }

  // Sticky hover keeps the target ring up while the pointer sits in the
  // hovered monster's margin; a click there should attack, not walk, even
  // though the ray misses the actual silhouette.
  function hoveredMonsterAttackIntent(): ClickIntent | null {
    const hover = get(hoverTarget)
    if (hover?.kind !== 'monster' || isMonsterDead(hover.monsterId)) return null
    const monster = monsterManager.monsters.get(hover.monsterId)
    if (!monster || !currentPlayer) return null
    const p = currentPlayer.position
    const dx = shortestWrappedDeltaX(p.x, monster.position.x)
    const dz = monster.position.z - p.z
    return {
      type: 'attack_monster',
      monsterId: hover.monsterId,
      hitPoint: { x: p.x + dx, y: monster.position.y, z: monster.position.z },
      distance: Math.sqrt(dx * dx + dz * dz),
    }
  }

  // Same coherence for NPCs: a click in the sticky margin interacts instead
  // of walking. Non-NPC players stay left-click inert by design, so hovering
  // one never overrides a click.
  function hoveredNpcInteractIntent(): ClickIntent | null {
    const hover = get(hoverTarget)
    if (hover?.kind !== 'player' || !currentPlayer) return null
    if (!get(gameStore).otherPlayers.get(hover.playerId)?.isOfficialNpc)
      return null
    const npcPos = remotePlayerManager.players.get(hover.playerId)?.position
    if (!npcPos) return null
    const p = currentPlayer.position
    return {
      type: 'interact_npc',
      playerId: hover.playerId,
      position: {
        x: p.x + shortestWrappedDeltaX(p.x, npcPos.x),
        y: npcPos.y,
        z: npcPos.z,
      },
    }
  }

  function processClickIntent(
    event: MouseEvent,
    movementOnly = false
  ): ClickIntent {
    const groundOnly =
      get(landscapingMode) !== null || get(estateFurnitureEditorActive)
    const targetingWater = get(fishingTargeting)
    const intent = inputHandler.processCanvasClick(
      event,
      {
        groundOnly,
        movementOnly,
        fishingTargeting: targetingWater,
        camera,
        monsterMeshes,
        npcMeshes,
        doorMeshes,
        objectMeshes,
        propMeshes,
        groundItemMeshes,
        tipHatMeshes,
        stallMeshes,
        mealMeshes,
        groundMeshes,
        playerPosition: {
          x: currentPlayer!.position.x,
          y: currentPlayer!.position.y,
          z: currentPlayer!.position.z,
        },
        playerVisualFloorLevel: get(playerVisualFloorLevel),
        resolveHousingStairTarget: (floorLevel, x, y, z, stairFloor) =>
          housingManager.stairLandingTargetAt(floorLevel, x, y, z, stairFloor),
        isMonsterDead,
        canCastFishing:
          !movementOnly &&
          abilityEquipmentAllowed(FISHING.id, get(inventoryStore).equipped) &&
          currentPassabilityFloor() === 0,
        waterSurfaceAt,
      },
      renderer.domElement.getBoundingClientRect()
    )
    if (
      !movementOnly &&
      !groundOnly &&
      !targetingWater &&
      (intent.type === 'move_to_ground' || intent.type === 'none')
    ) {
      return (
        hoveredMonsterAttackIntent() ?? hoveredNpcInteractIntent() ?? intent
      )
    }
    return intent
  }

  /** Right-click on an NPC: open the context menu with the interactions the
   *  NPC's data supports (doc/ECONOMY.md "거래 진입 UI"). Right-click on a
   *  player offers to report the picture on their cape, which is the only
   *  brake on what people print (doc/CAPE_CUSTOMIZATION.md). */
  function handleNpcContextMenu(event: MouseEvent) {
    if (!currentPlayer || currentPlayer.health <= 0) return
    const intent = processClickIntent(event)
    if (intent.type === 'interact_npc') {
      const npc = get(gameStore).otherPlayers.get(intent.playerId)
      if (npc?.isOfficialNpc) {
        const caps = getNpcCapabilities(npc.name)
        const entries = [{ label: 'Talk', action: () => requestChatFocus() }]
        if (caps.trade) {
          entries.push({
            label: 'Trade',
            action: () => tradeWithNpc(intent),
          })
        }
        npcContextMenu.set({
          npcName: npc.name,
          screenX: event.clientX,
          screenY: event.clientY,
          entries,
        })
        return
      }
    }

    const playerId = inputHandler.pickPlayer(event, camera, playerMeshes)
    if (playerId === null) return
    const player = get(gameStore).otherPlayers.get(playerId)
    if (!player || player.isOfficialNpc || !player.backTexture) return
    npcContextMenu.set({
      npcName: player.name,
      screenX: event.clientX,
      screenY: event.clientY,
      entries: [
        {
          label: 'Report cape',
          action: () => networkManager.sendReportCapeTexture(playerId),
        },
      ],
    })
  }

  function handleCanvasClickIntent(event: MouseEvent) {
    if ($localTeleportActive) return
    if (get(inspectionTargeting)) {
      if (event.button !== 0) return
      const hover = inputHandler.processHover(event, {
        camera,
        objectMeshes: [],
        tipHatMeshes: [],
        stallMeshes: [],
        mealMeshes: [],
        propMeshes: [],
        groundItemMeshes: [],
        monsterMeshes: monsterHoverMeshes,
        playerMeshes: playerHoverMeshes,
        isHoverable,
        ownerName,
      })
      const target = takeInspectionTarget(
        hover?.kind === 'monster'
          ? { kind: 'monster', monster_id: hover.monsterId }
          : hover?.kind === 'player'
            ? { kind: 'player', player_id: hover.playerId }
            : null,
        get(inventoryStore).equipped
      )
      if (target)
        networkManager.sendUseAbility(
          AUSCULTATION.id,
          target.kind === 'monster' ? target.monster_id : null,
          target.kind === 'player' ? target.player_id : null
        )
      return
    }
    if (event.button === 0 && $cameraRotationEnabled && !get(fishingTargeting))
      return
    const editorMode =
      $mapEditorMode ||
      $housingEditorMode ||
      get(landscapingMode) !== null ||
      get(estateFurnitureEditorActive)
    if (event.button === 2 && !editorMode) {
      handleNpcContextMenu(event)
      return
    }
    const playerControlEvent = createCanvasIntentEvent({
      event,
      editorMode,
      currentPlayer,
      processIntent: () => processClickIntent(event),
    })
    if (!playerControlEvent) return
    if (
      get(fishingTargeting) &&
      playerControlEvent.type === 'canvas_intent' &&
      playerControlEvent.intent.type === 'none'
    ) {
      reportSkillFailure(
        translate('fishing.clickWater', { distance: max_cast_distance_m() })
      )
      return
    }

    enqueuePlayerControlEvent(playerControlEvent)
    return (
      !editorMode &&
      playerControlEvent.type === 'canvas_intent' &&
      playerControlEvent.intent.type === 'move_to_ground'
    )
  }

  function handleCanvasDragMove(event: MouseEvent) {
    if (
      $localTeleportActive ||
      !currentPlayer ||
      currentPlayer.health <= 0 ||
      $cameraRotationEnabled ||
      $mapEditorMode ||
      $housingEditorMode ||
      get(landscapingMode) !== null ||
      get(estateFurnitureEditorActive) ||
      get(inspectionTargeting) ||
      get(fishingTargeting) ||
      inputHandler.hasKeysPressed
    )
      return
    const intent = processClickIntent(event, true)
    if (intent.type === 'move_to_ground') {
      enqueuePlayerControlEvent({
        type: 'canvas_intent',
        intent,
        editorMode: false,
      })
    }
  }

  function createPlayerControlEventActions(): PlayerControlEventActions {
    return {
      attackInRange: (monsterId) => {
        // initiateAttack transitions to attacking, which drops any moving data
        // (no separate runtime reset needed).
        initiateAttack(monsterId)
      },
      chaseAndAttack,
      toggleDoor,
      toggleDungeonDoor,
      interactObject,
      pickupItem,
      interactNpc: (intent) => {
        const npc = get(gameStore).otherPlayers.get(intent.playerId)
        if (!npc?.isOfficialNpc) return
        // Click default per NPC kind: merchants open their shop, everyone
        // else starts a conversation. Right-click offers both explicitly.
        const caps = getNpcCapabilities(npc.name)
        if (caps.defaultAction === 'trade') {
          tradeWithNpc(intent)
        } else {
          requestChatFocus()
        }
      },
      tipHat,
      tradeAtStall,
      eatMeal,
      breakProp,
      openProp,
      moveToGround: (position, sprinting, viaHousingStair) => {
        combatController.cancelCombat()
        const snapped = dungeonManager.snapDescentWallClick(
          position.x,
          position.z,
          position.y
        )
        const target = snapped ?? position
        const currentFloor = currentPassabilityFloor()
        const targetFloor = passability_get_floor_at(
          target.x,
          target.z,
          target.y
        )
        if (
          shouldIgnoreImplicitHouseFloorChange(
            get(playerInsideHouseId),
            currentFloor,
            targetFloor,
            viaHousingStair
          )
        ) {
          return
        }
        handleClickToMove(target, {
          sprinting,
          stopAtHouseEntrance: true,
        })
      },
      castFishing: (intent) => {
        if (!currentPlayer || currentPlayer.health <= 0) return
        const boating = currentPlayer.mount === 'rowboat'
        if (
          boating &&
          !fishing_is_stern_cast(
            shortestWrappedDeltaX(currentPlayer.position.x, intent.position.x),
            intent.position.z - currentPlayer.position.z,
            playerRotation
          )
        ) {
          addChatMessage({
            text: translate('fishing.behindBoat'),
            sender: 'system',
          })
          return
        }
        cancelFishingTargeting()
        // Movement would cancel the cast on the next waypoint send.
        combatController.cancelCombat()
        stopMovement()
        if (!boating) faceTowards(intent.position.x, intent.position.z)
        setPlayerState({ ...playerState, rotation: playerRotation })
        // Sync heading before the server validates the cast direction.
        stopAndFace(playerRotation)
        networkManager.sendFishingCast(intent.position)
      },
      requestMove: handleClickToMove,
      onInteractionFinished,
      onPickupGrab,
      onInteractionRejected: () => {
        if (playerState.state === 'interact') exitObjectInteraction(false)
      },
    }
  }

  function dispatchPlayerControlEvent(event: PlayerControlEvent) {
    if ($localTeleportActive && event.type === 'canvas_intent') return
    // A fresh click supersedes the armed walk-up action, even one that starts
    // no movement of its own (a cast, an in-reach interaction). A click that
    // hit nothing at all shouldn't cancel the walk the player is already on.
    if (event.type === 'canvas_intent' && event.intent.type !== 'none') {
      cancelAutoTravel()
      clearDoorInteractionRetry()
      const m = movingState()
      if (m) m.approach = null
    }
    dispatchQueuedPlayerControlEvent(
      event,
      createPlayerControlEventActions(),
      equippedAttackRange()
    )
  }

  const playerControlMachine = createLocalPlayerControlMachine({
    dispatchEvent: dispatchPlayerControlEvent,
    stateActions: {
      onInteractionFinished,
      onPickupGrab,
      onInteractionRejected: () => {
        if (playerState.state === 'interact') exitObjectInteraction(false)
      },
      handleInteractKey: checkInteraction,
      handleKeyboard: updateKeyboardMovement,
      tick: updatePlayerMovement,
      onServerMoveExit: () => serverMovement.clear(),
    },
  })

  export function updatePlayerControl(
    deltaTime: number,
    options: PlayerControlUpdateOptions
  ) {
    if ($localTeleportActive) return
    if (options.editorMode) {
      if (serverMovement.active || serverMovement.stopping) stopMovement()
      cancelAutoTravel()
      keyboardSender.clear()
    }
    const skillState = get(daggerSkillState)
    const hasDagger = abilityEquipmentAllowed(
      DAGGER_SKILL.clip,
      $inventoryStore.equipped
    )
    if (
      options.editorMode ||
      !currentPlayer ||
      !isAbilityAvailable(DAGGER_SKILL.clip, currentPlayer.characterClass) ||
      currentPlayer.health <= 0 ||
      !hasDagger ||
      isMounted(currentPlayer)
    ) {
      if (skillState.queued)
        daggerSkillState.update((state) => ({ ...state, queued: false }))
      if (currentPlayer && get(daggerSkillCasts).has(currentPlayer.id))
        clearDaggerCast(currentPlayer.id)
    }
    if (skillState.pending && Date.now() - skillState.requestAt > 4000) {
      daggerSkillState.update((state) => ({ ...state, pending: false }))
      if (currentPlayer) clearDaggerCast(currentPlayer.id)
    }
    playerControlMachine.update(deltaTime, options)
  }

  // Hover overlays: signpost speech bubble, ground-item, prop and monster names.
  // Driven by pointermove (event-based, not per-frame) and raycast only against
  // those groups, throttled to ~20 Hz — negligible cost.
  let lastHoverRaycast = 0
  let lastHoverKey: string | null = null
  let hoverTrailing: ReturnType<typeof setTimeout> | null = null
  let pendingHoverEvent: MouseEvent | null = null

  const isMonsterDead = (id: string) =>
    monsterManager.monsters.get(id)?.state === 'dead'

  // An item being picked up is hidden through its parent's `visible`, which
  // raycasts ignore — so a hit on one that is gone or already in hand doesn't
  // count. A corpse likewise names nothing; both let the ray look past them.
  function isHoverable(target: HoverTarget): boolean {
    if (target.kind === 'groundItem') {
      const item = groundItemManager.items.get(target.instanceId)
      return !!item && !item.inHand
    }
    if (target.kind === 'monster') return !isMonsterDead(target.monsterId)
    if (target.kind === 'player') {
      const player = get(gameStore).otherPlayers.get(target.playerId)
      return !!player && player.health > 0
    }
    return true
  }

  /** Display name for a player id (stall owner labels), self included. */
  function ownerName(playerId: number): string | null {
    const state = get(gameStore)
    if (state.currentPlayer?.id === playerId) {
      return state.currentPlayer.name ?? null
    }
    return state.otherPlayers.get(playerId)?.name ?? null
  }

  function runHover(event: MouseEvent) {
    if (get(landscapingMode) || get(estateFurnitureEditorActive)) {
      clearHover()
      return
    }
    lastHoverRaycast = performance.now()
    const target = inputHandler.processHover(event, {
      camera,
      objectMeshes,
      tipHatMeshes,
      stallMeshes,
      mealMeshes,
      propMeshes,
      groundItemMeshes,
      monsterMeshes: monsterHoverMeshes,
      playerMeshes: playerHoverMeshes,
      isHoverable,
      ownerName,
    })
    const key = hoverTargetKey(target)
    if (key === lastHoverKey) return
    lastHoverKey = key
    hoverTarget.set(target)
  }

  function handlePointerHover(event: MouseEvent) {
    pendingHoverEvent = event
    const dt = performance.now() - lastHoverRaycast
    if (dt >= 50) {
      if (hoverTrailing) {
        clearTimeout(hoverTrailing)
        hoverTrailing = null
      }
      runHover(event)
    } else if (!hoverTrailing) {
      // Trailing edge: process the final position after the throttle window so a
      // quick flick off a signpost (then stop, without leaving the canvas)
      // doesn't strand the bubble over empty ground.
      hoverTrailing = setTimeout(() => {
        hoverTrailing = null
        if (pendingHoverEvent) runHover(pendingHoverEvent)
      }, 50 - dt)
    }
  }

  function clearHover() {
    if (hoverTrailing) {
      clearTimeout(hoverTrailing)
      hoverTrailing = null
    }
    if (lastHoverKey === null) return
    lastHoverKey = null
    hoverTarget.set(null)
  }

  // Crossing a dungeon boundary swaps the visible entity layers wholesale;
  // a resting cursor would otherwise keep a stale snapshot hover. Subscribed
  // below the hover state it clears: subscribe fires its callback right here,
  // and the `let`s above are TDZ until their declarations run.
  currentDungeonDepth.subscribe(() => clearHover())

  onMount(() => {
    const unsubscribeTeleportEffect = localTeleportActive.subscribe(
      (active) => {
        if (!active) return
        cancelAutoTravel()
        combatController.cancelCombat()
        clearStandUpTimer()
        clearPropSwingTimers()
        clearDoorInteractionRetry()
        keyboardSender.clear()
        inputHandler.clearTransientInput()
        currentSpeed = 0
        clickSprinting = false
        transitionTo('idle')
        updatePlayerState()
      }
    )
    const canvasCursor = renderer.domElement.style.cursor
    const unsubscribeTargeting = derived(
      [inspectionTargeting, fishingTargeting],
      (states) => states.some(Boolean)
    ).subscribe((active) => {
      renderer.domElement.style.cursor = active ? 'crosshair' : canvasCursor
      if (!active || !currentPlayer) return
      cancelAutoTravel()
      combatController.cancelCombat()
      clearPropSwingTimers()
      const movement = movingState()
      if (movement) movement.approach = null
      stopMovement()
      stopAndFace(playerRotation)
    })
    const unsubscribeTravel = travelDestination.subscribe((destination) => {
      const wasTravelling = autoTravelTarget !== null
      autoTravelTarget = destination
      travelPlayerId = destination ? (currentPlayer?.id ?? null) : null
      travelProgressPosition = null
      travelStalledMs = 0
      travelPlanCooldownMs = 0
      if (wasTravelling || (destination && isMovingNow())) {
        const m = movingState()
        if (m) m.approach = null
        stopMovement()
        if (currentPlayer && currentPlayer.health > 0 && !get(teleportLoading))
          stopAndFace(playerRotation)
      }
      if (!destination) return
      combatController.cancelCombat()
      clearStandUpTimer()
      clearPropSwingTimers()
      clearDoorInteractionRetry()
      dungeonManager.clearPendingBreak()
      dungeonManager.clearPendingOpen()
      const interaction = getInteractionExitKind(playerState)
      if (interaction === 'pickup') exitPickupInteraction()
      if (interaction === 'object') exitObjectInteraction()
    })
    const unsubscribeTeleport = teleportLoading.subscribe((loading) => {
      if (loading) {
        serverMovement.clear(false)
        if (playerControlMachine.stateName === 'moving') transitionTo('idle')
        cancelAutoTravel()
      }
    })
    const unsubscribeTravelPlayer = gameStore.subscribe((state) => {
      if (
        !state.isConnected ||
        !state.currentPlayer ||
        state.currentPlayer.id !== travelPlayerId ||
        state.currentPlayer.health <= 0
      )
        cancelAutoTravel()
    })
    const enterPlacementMode = (mode: unknown) => {
      if (!mode) return
      cancelAutoTravel()
      clearStandUpTimer()
      clearPropSwingTimers()
      currentSpeed = 0
      clickSprinting = false
      transitionTo('idle')
      updatePlayerState()
    }
    const unsubscribeLandscapingMode =
      landscapingMode.subscribe(enterPlacementMode)
    const unsubscribeFurnitureMode =
      estateFurniturePlacementMode.subscribe(enterPlacementMode)
    const unsubscribeFurnitureSelection =
      estateFurnitureSelectionMode.subscribe(enterPlacementMode)
    preloadSwordHitSound()
    preloadSwordMissSound()
    preloadMonsterDeathSounds()
    preloadPlayerHurtSounds()
    preloadPlayerDeathSounds()
    preloadPropSounds()
    preloadBowSounds()
    preloadFishingSounds()

    const removeInputListeners = inputHandler.setupEventListeners(
      renderer.domElement,
      handleCanvasClickIntent,
      handleCanvasDragMove,
      () => keyboardSender.clear()
    )

    const canvas = renderer.domElement
    // Ignore pointerleave caused by OrbitControls capturing on the wrapper.
    const handlePointerLeave = (e: PointerEvent) => {
      if (e.relatedTarget instanceof Node && e.relatedTarget.contains(canvas))
        return
      clearHover()
    }
    canvas.addEventListener('pointermove', handlePointerHover)
    canvas.addEventListener('pointerleave', handlePointerLeave)

    const unsubscribeMovePath = networkManager.movePath.on((path) => {
      if (!currentPlayer || currentPlayer.health <= 0 || $localTeleportActive) {
        serverMovement.clear(false)
        return
      }
      if (!serverMovement.acceptPath(path)) return
      officialPosition = { ...path.position }
      syncOwnFloor(path.floor_level, path.position.x, path.position.z)
    })
    const unsubscribeMoveProgress = networkManager.moveProgress.on(
      (progress) => {
        if (
          !currentPlayer ||
          currentPlayer.health <= 0 ||
          $localTeleportActive
        ) {
          serverMovement.clear(false)
          return
        }
        if (
          serverMovement.acceptStopped(
            progress,
            {
              position: currentPlayer.position,
              rotation: playerRotation,
              speed: currentSpeed,
            },
            playerControlMachine.stateName === 'attacking'
          )
        ) {
          officialPosition = { ...progress.position }
          if (
            !serverMovement.active &&
            playerControlMachine.stateName !== 'object_interacting'
          ) {
            syncOwnFloor(
              progress.floor_level,
              progress.position.x,
              progress.position.z
            )
            if (serverMovement.stopping) return
            applyStoppedPlayerPosition(progress.position, progress.rotation)
          }
          return
        }
        if (!serverMovement.acceptProgress(progress)) return
        officialPosition = { ...progress.position }
        syncOwnFloor(
          progress.floor_level,
          progress.position.x,
          progress.position.z
        )
        if (
          serverMovement.isCurrentRequest(progress.request_id) &&
          progress.status !== 'moving' &&
          progress.status !== 'searching'
        ) {
          if (!['arrived', 'partial', 'blocked'].includes(progress.status)) {
            const movement = movingState()
            if (movement) movement.approach = null
          }
          writePlayerPosition(progress.position, progress.rotation)
          playerRotation = progress.rotation
          if (progress.status !== 'arrived' && progress.status !== 'stopped')
            cancelAutoTravel(translate('travel.blocked'))
          if (progress.status === 'rejected') serverMovement.clear()
          else serverMovement.finish()
          stopMovement()
        }
      }
    )
    const unsubscribeMoveConnection = gameStore.subscribe((state) => {
      if (!state.isConnected || !state.currentPlayer) {
        serverMovement.clear(false)
        keyboardSender.reset()
        inputHandler.clearTransientInput()
      }
    })
    const unsubscribeInteraction = networkManager.interactionChanged.on(
      (interaction) => {
        void applyServerInteraction(interaction)
      }
    )
    const unsubscribeRelocated = networkManager.playerRelocated.on(
      (position, rotation) => {
        interactionRevision++
        serverMovement.clear(false)
        keyboardSender.reset()
        officialPosition = { ...position }
        playerRotation = rotation
        currentSpeed = 0
        writePlayerPosition(position, rotation)
        if (
          playerControlMachine.stateName === 'moving' ||
          playerControlMachine.stateName === 'keyboard_moving'
        )
          transitionTo('idle')
        cancelAutoTravel()
      }
    )
    const unsubscribeNetworkEvents = subscribePlayerNetworkEvents({
      isCurrentPlayerEligibleForRespawn: () =>
        !!currentPlayer && currentPlayer.health <= 0,
      isCurrentPlayer: (id) => !!currentPlayer && currentPlayer.id === id,
      isInteracting: () => playerState.state === 'interact',
      onRespawned: transitionToRespawned,
      onInteractionRejected: () =>
        enqueuePlayerControlEvent({ type: 'network_interaction_rejected' }),
    })

    return () => {
      unsubscribeTargeting()
      cancelInspection()
      cancelFishingTargeting()
      renderer.domElement.style.cursor = canvasCursor
      unsubscribeTravel()
      unsubscribeTeleport()
      unsubscribeTeleportEffect()
      unsubscribeTravelPlayer()
      travelDestination.set(null)
      removeInputListeners()
      unsubscribeLandscapingMode()
      unsubscribeFurnitureMode()
      unsubscribeFurnitureSelection()
      canvas.removeEventListener('pointermove', handlePointerHover)
      canvas.removeEventListener('pointerleave', handlePointerLeave)
      clearHover()
      serverMovement.clear()
      unsubscribeMovePath()
      unsubscribeMoveProgress()
      unsubscribeMoveConnection()
      interactionRevision++
      unsubscribeInteraction()
      unsubscribeRelocated()
      unsubscribeNetworkEvents()
      playerControlMachine.dispose()
      clearStandUpTimer()
      clearPropSwingTimers()
      clearDoorInteractionRetry()
      // The store outlives this component (character select, logout).
      lastEmoteSync = null
      localEmoteAnim.set(null)
    }
  })
</script>

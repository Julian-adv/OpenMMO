<script lang="ts">
  import { onMount } from 'svelte'
  import { useThrelte } from '@threlte/core'
  import * as THREE from 'three'
  import { gameStore, hoveredSignpost, type LocalPlayer } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import { monsterManager } from '../managers/monsterManager'
  import { groundItemManager } from '../managers/groundItemManager'
  import { combatController } from '../managers/combatController'
  import { preloadSwordHitSound, preloadSwordMissSound } from '../managers/sfxManager'
  import { inputHandler, type ClickIntent } from '../managers/inputHandler'
  import { getMerchantByNpcName } from '../data/merchantDefs'
  import { NPC_TRADE_RANGE_METERS } from '../data/tradeConstants'
  import { mapEditorMode, housingEditorMode, debugSpeedMode, torchLightEnabled } from '../stores/debugStore'
  import { localTorchEquipped } from '../stores/inventoryStore'
  import {
    DEFAULT_MOVEMENT_CONFIG,
    type Position,
    type MovementState,
    type MovementConfig,
    type PlayerState,
  } from '../utils/movementUtils'
  import type { TerrainHeightManager } from '../managers/terrainHeightManager'
  import { playerFloorOffset, playerFloorLevel } from '../stores/housingStore'
  import { housingManager } from '../managers/housingManager'
  import { findPath } from '../managers/pathfinding'
  import { passability_get_floor_at } from '../wasm/onlinerpg_shared'
  import { get } from 'svelte/store'
  import { createPlayerPhysics } from './player-control/player-physics'
  import { subscribePlayerNetworkEvents } from './player-control/player-network-events'
  import type {
    PlayerControlEvent,
    PlayerControlUpdateOptions,
  } from './player-control/events'
  import {
    projectPlayerState,
    shouldEmitProjectedPlayerState,
  } from './player-control/fsm/projection'
  import {
    runMoveRequest,
    type MoveRequestActions,
  } from './player-control/fsm/move-request'
  import { runKeyboardFrame } from './player-control/fsm/keyboard'
  import {
    dispatchPlayerControlEvent as dispatchQueuedPlayerControlEvent,
    createCanvasIntentEvent,
    type PlayerControlEventActions,
  } from './player-control/fsm/events'
  import { runPlayerMovementTick } from './player-control/fsm/movement-tick'
  import {
    beginJumpFeedback,
    shouldFinishJumpFeedback,
    transitionToDeadState,
    transitionToRespawnedState,
  } from './player-control/fsm/lifecycle'
  import {
    exitPickupInteraction as buildExitPickupInteraction,
    handlePickupGrab,
    decidePickupApproach,
    applyObjectInteractionPosition,
    getObjectInteractionExitPosition,
    beginPickupInteraction,
    beginObjectInteraction,
    exitObjectInteraction as buildExitObjectInteraction,
    handleInteractKey,
    getInteractionExitKind,
  } from './player-control/fsm/interaction'
  import {
    beginAttack,
    ensureAttackState,
    transitionAttackToIdle,
  } from './player-control/fsm/combat'
  import type {
    MovingControlState,
    PickingUpControlState,
    PlayerControlStateName,
  } from './player-control/fsm/control-state'
  import { createLocalPlayerControlMachine } from './player-control/fsm/state-definitions'

  interface Props {
    onStateChange: (state: PlayerState) => void
    camera: THREE.Camera
    heightManager: TerrainHeightManager
    groundMeshes: THREE.Object3D[]
    groundItemMeshes: THREE.Object3D[]
    monsterMeshes: THREE.Group[]
    npcMeshes?: THREE.Object3D[]
    doorMeshes: THREE.Object3D[]
    objectMeshes: THREE.Object3D[]
    attackCooldown?: number
  }

  let { onStateChange, camera, heightManager, groundMeshes, groundItemMeshes, monsterMeshes, npcMeshes = [], doorMeshes, objectMeshes, attackCooldown }: Props = $props()

  let floorOffset = 0
  playerFloorOffset.subscribe((v) => (floorOffset = v))

  let currentPlayer = $state<LocalPlayer | null>(null)

  const { renderer } = useThrelte()

  const physics = createPlayerPhysics({
    getHeightManager: () => heightManager,
    getCurrentPlayerY: () => currentPlayer?.position.y ?? null,
    getFloorOffset: () => floorOffset,
  })
  const { sampleHeight, isMovementBlocked, isUphillTooSteep } = physics

  // Movement data (target, integrator, A* waypoints, far-pickup target) lives
  // inside the machine's `moving` state — see movingState(). Leaving the moving
  // state drops that data, so there are no movement flags to reset here.
  // lastSentPosition is kinematic (send dedup), not state-membership data.
  let lastSentPosition = $state<Position | null>(null)

  // Use the same movement config as remote players, with debug speed multiplier
  let MOVEMENT_CONFIG = $derived<MovementConfig>({
    ...DEFAULT_MOVEMENT_CONFIG,
    maxSpeed: DEFAULT_MOVEMENT_CONFIG.maxSpeed * ($debugSpeedMode ? 10 : 1),
    acceleration: DEFAULT_MOVEMENT_CONFIG.acceleration * ($debugSpeedMode ? 10 : 1),
    deceleration: DEFAULT_MOVEMENT_CONFIG.deceleration * ($debugSpeedMode ? 10 : 1),
  })

  // Character rotation and current speed
  let playerRotation = $state(0)
  let currentSpeed = $state(0)

  const STAND_UP_DURATION = 300 // ms, matches animation crossfade duration
  let standUpTimer: ReturnType<typeof setTimeout> | null = null

  const JUMP_FEEDBACK_DURATION_MS = 1500
  const JUMP_FEEDBACK_COOLDOWN_MS = 1000
  let jumpFeedbackTimer: ReturnType<typeof setTimeout> | null = null
  let lastJumpFeedbackAt = 0

  function clearJumpFeedbackTimer() {
    if (!jumpFeedbackTimer) return
    clearTimeout(jumpFeedbackTimer)
    jumpFeedbackTimer = null
  }

  function enqueuePlayerControlEvent(event: PlayerControlEvent) {
    playerControlMachine.enqueueEvent(event)
  }

  /**
   * Briefly switch the player to the 'jump' state to play the jump animation
   * as a one-shot feedback that the terrain ahead is too steep. Cooldown
   * prevents the animation from restarting every frame while the user keeps
   * pushing into the slope.
   */
  function triggerJumpFeedback() {
    const transition = beginJumpFeedback({
      previousPlayerState: playerState,
      now: Date.now(),
      lastJumpFeedbackAt,
      cooldownMs: JUMP_FEEDBACK_COOLDOWN_MS,
    })
    lastJumpFeedbackAt = transition.runtime.lastJumpFeedbackAt
    if (transition.kind === 'cooldown') return

    setPlayerState(transition.nextPlayerState)
    transitionTo('jump_feedback')

    clearJumpFeedbackTimer()
    jumpFeedbackTimer = setTimeout(() => {
      jumpFeedbackTimer = null
      if (shouldFinishJumpFeedback(playerState)) {
        updatePlayerState()
        transitionTo('idle')
      }
    }, JUMP_FEEDBACK_DURATION_MS)
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
    exitPickupInteraction()
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

  function exitObjectInteraction(notify = true) {
    if (currentPlayer) {
      applyObjectInteractionPosition(
        currentPlayer,
        getObjectInteractionExitPosition(
          {
            x: currentPlayer.position.x,
            y: currentPlayer.position.y,
            z: currentPlayer.position.z,
          },
          playerRotation
        ),
        {
          hasHeightData: (x, z) => heightManager.hasHeightData(x, z),
          sampleHeight,
        }
      )
    }

    setPlayerState(buildExitObjectInteraction(playerState))
    transitionTo('idle')

    if (notify) {
      networkManager.sendStopInteraction()
    }
  }

  function stopMovement() {
    if (standUpTimer) {
      clearTimeout(standUpTimer)
      standUpTimer = null
    }
    currentSpeed = 0
    // Settle into idle BEFORE emitting: the projection derives 'moving' vs
    // 'idle' from the machine's owned state, so the transition must precede the
    // emit. Leaving the moving state also drops its target/movementState/path —
    // nothing to reset. arrive() overrides idle with pickup/attack right after.
    transitionTo('idle')
    updatePlayerState()
  }

  // Explicitly drive the machine's owned state to a data-less state. The machine
  // no longer derives its state name from flags — callers transition at the real
  // decision points. Stateful transitions (moving/picking_up) carry their data.
  function transitionTo(name: Exclude<PlayerControlStateName, 'moving' | 'picking_up'>) {
    playerControlMachine.transition({ name })
  }

  // `isMoving` is no longer a stored flag: being in motion IS being in the
  // moving/keyboard_moving state. Derive it from the machine's owned state.
  function isMovingNow(): boolean {
    const name = playerControlMachine.stateName
    return name === 'moving' || name === 'keyboard_moving'
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

  // Wrapper for sending move packets to track last sent position
  function sendPlayerMove(position: Position, rotation: number) {
    lastSentPosition = { ...position }
    networkManager.sendPlayerMove(position, rotation, Math.max(0, get(playerFloorLevel)))
  }

  function writePlayerPosition(position: Position, rotation: number) {
    gameStore.update((state) => {
      if (state.currentPlayer) {
        state.currentPlayer.position.set(position.x, position.y, position.z)
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

  function setPlayerState(next: PlayerState) {
    playerState = next
    onStateChange(next)
  }

  gameStore.subscribe((state) => {
    currentPlayer = state.currentPlayer
    if (currentPlayer) {
      playerState.position = {
        x: currentPlayer.position.x,
        y: currentPlayer.position.y,
        z: currentPlayer.position.z,
      }
    }
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
    })

    // Only update if state actually changed
    if (shouldEmitProjectedPlayerState(playerState, newState)) {
      playerState = newState
      onStateChange(newState)
    }
  }

  // Initiate attack on a monster
  function initiateAttack(monsterId: string) {
    if (getInteractionExitKind(playerState) === 'pickup') {
      finishPendingPickup()
    }

    const monsterInfo = monsterManager.monsters.get(monsterId)
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
      lastSentPosition,
      beginCombat: (id, inRange) => combatController.beginCombat(id, inRange),
      sendPlayerMove,
      sendPlayerAttack: (id) => networkManager.sendPlayerAttack(id),
    })

    if (result.kind === 'ignored_dead_target') return

    // Entering attacking drops any moving-state data (the chase that brought us
    // here), so there is nothing else to reset.
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
    const transition = transitionToDeadState(playerState)
    if (transition.kind === 'ignored_already_dead') return

    combatController.cancelCombat()
    // Finish any in-flight pickup while still in picking_up, before the dead
    // transition drops that state (L5: explicit finish on every pickup exit).
    finishPendingPickup()

    setPlayerState(transition.nextPlayerState)
    transitionTo('dead')
  }

  function transitionToRespawned() {
    if (!currentPlayer) return

    const transition = transitionToRespawnedState(playerState, {
      x: currentPlayer.position.x,
      y: currentPlayer.position.y,
      z: currentPlayer.position.z,
    })
    combatController.cancelCombat()
    playerRotation = transition.runtime.playerRotation
    finishPendingPickup()

    setPlayerState(transition.nextPlayerState)
    transitionTo('idle')
  }

  /** Check E key interaction (door toggle). Call from game loop. */
  function checkInteraction() {
    handleInteractKey({
      currentPlayer,
      consumeInteract: () => inputHandler.consumeInteract(),
      findNearestDoor: (x, z, y, range) =>
        housingManager.findNearestDoor(x, z, y, range),
      sendToggleDoor: (houseId, roomIndex, wallDir, segmentIndex) =>
        networkManager.sendToggleDoor(houseId, roomIndex, wallDir, segmentIndex),
    })
  }

  // Stable action bags reused every frame by the movement/keyboard ticks.
  // They only read live `$state` inside their closures, so building them once
  // avoids reallocating ~20 closures per frame on the render hot path.
  const combatTickActions = {
    stopMovingToIdle: () => {
      if (isMovingNow()) {
        // Leaving the moving state drops its target/movementState. Transition
        // before emit so the projection sees idle (chase -> idle).
        transitionTo('idle')
        updatePlayerState()
      }
      transitionToIdle()
    },
    prepareReachedAttackRange: () => {
      currentSpeed = 0
      // Reached range stops movement (leaving moving drops its data); settle to
      // idle before the emit. beginAttack (next, same outcome) transitions to
      // attacking; if the target just died and beginAttack is ignored, we
      // correctly remain idle.
      transitionTo('idle')
      updatePlayerState()
    },
    beginAttack: initiateAttack,
    setChasingMovement: (
      nextMovementTarget: Position,
      nextMovementState: MovementState,
      nextRotation: number
    ) => {
      playerRotation = nextRotation
      // Chase reports as 'moving' (playerState stays 'moving' while pathing to
      // the monster); 'attacking' is reserved for in-range swinging. Update the
      // live moving state in place (preserving its A* path), or — when chase
      // resumes from the attacking state — start a fresh pathless moving state.
      const m = movingState()
      if (m) {
        m.target = nextMovementTarget
        m.movementState = nextMovementState
      } else {
        playerControlMachine.transition({
          name: 'moving',
          target: nextMovementTarget,
          movementState: nextMovementState,
          waypoints: [],
          waypointIndex: 0,
          pendingPickupAfterMove: null,
        })
      }
    },
    showAttackState: (nextRotation: number) => {
      playerRotation = nextRotation
      const transition = ensureAttackState(playerState, nextRotation)
      if (transition.kind === 'ignored') return
      setPlayerState(transition.nextPlayerState)
      transitionTo('attacking')
    },
    sendAttackCycle: (monsterId: string, nextRotation: number) => {
      playerRotation = nextRotation
      networkManager.sendPlayerAttack(monsterId)
      updatePlayerState()
      transitionTo('attacking')
    },
  }

  const movementTickActions = {
    stopMovement,
    triggerJumpFeedback,
    setNextWaypoint: (
      nextCurrentSpeed: number,
      nextPlayerRotation: number,
      nextMovementTarget: Position,
      nextMovementState: MovementState,
      nextWaypointIndex: number
    ) => {
      currentSpeed = nextCurrentSpeed
      playerRotation = nextPlayerRotation
      const m = movingState()
      if (m) {
        m.target = nextMovementTarget
        m.movementState = nextMovementState
        m.waypointIndex = nextWaypointIndex
      }
    },
    arrive: (nextCurrentSpeed: number, nextPlayerRotation: number) => {
      currentSpeed = nextCurrentSpeed
      playerRotation = nextPlayerRotation
      const pickupAfterArrival = movingState()?.pendingPickupAfterMove ?? null
      // stopMovement() settles to idle (and emits); the pickup/attack branches
      // below override that state when arrival hands off to them.
      stopMovement()

      if (pickupAfterArrival !== null) {
        enterPickup(pickupAfterArrival)
        return
      }

      if (combatController.isInCombat) {
        initiateAttack(combatController.targetMonsterId!)
      }
    },
    continueMovement: (
      nextCurrentSpeed: number,
      nextPlayerRotation: number,
      totalDistance: number
    ) => {
      currentSpeed = nextCurrentSpeed
      playerRotation = nextPlayerRotation
      updatePlayerState(totalDistance)
    },
  }

  const keyboardFrameActions = {
    exitPickupInteraction,
    exitObjectInteraction,
    clearClickMovement: () => {
      // No-op: keyboard always transitions to keyboard_moving (markMoving),
      // idle (setKeyboardIdleRuntime), or via stopMovement this same frame, and
      // leaving the moving state drops its target/movementState/pendingPickup.
    },
    cancelCombat: () => combatController.cancelCombat(),
    markMoving: () => {
      transitionTo('keyboard_moving')
    },
    setKeyboardIdleRuntime: () => {
      currentSpeed = 0
      transitionTo('idle')
    },
    emitKeyboardPlayerState: () => {
      updatePlayerState(isMovingNow() ? 100 : undefined)
    },
    stopMovement,
    triggerJumpFeedback,
    setMoved: (nextCurrentSpeed: number, nextPlayerRotation: number) => {
      currentSpeed = nextCurrentSpeed
      playerRotation = nextPlayerRotation
    },
  }

  // Update player movement (click-to-move) with acceleration/deceleration
  function updatePlayerMovement(deltaTime: number) {
    const m = movingState()
    runPlayerMovementTick({
      deltaTime,
      currentPlayer,
      playerStateName: playerState.state,
      isMoving: isMovingNow(),
      currentSpeed,
      movementTarget: m?.target ?? null,
      movementState: m?.movementState ?? null,
      pathWaypoints: m?.waypoints ?? [],
      currentWaypointIndex: m?.waypointIndex ?? 0,
      config: MOVEMENT_CONFIG,
      isInCombat: combatController.isInCombat,
      combatController,
      cooldownMs: attackCooldown ? attackCooldown * 1000 : 1500,
      getMonsterInfo: (monsterId) => {
        const monsterData = monsterManager.monsters.get(monsterId)
        return monsterData
          ? {
              state: monsterData.state,
              isDeadPending: monsterData.isDeadPending,
            }
          : undefined
      },
      findMonsterPosition: (monsterId) =>
        monsterManager.findMeshPosition(monsterId, monsterMeshes),
      sampleHeight,
      hasHeightData: (x, z) => heightManager.hasHeightData(x, z),
      isMovementBlocked,
      isUphillTooSteep,
      getFloorLevel: () => get(playerFloorLevel),
      setFloorLevel: (floor) => playerFloorLevel.set(floor),
      writePlayerPosition,
      sendPlayerMove,
      actions: {
        transitionToDead,
        resetStoppedSpeed: () => {
          currentSpeed = 0
          updatePlayerState()
        },
        combat: combatTickActions,
        movement: movementTickActions,
      },
    })
  }

  function updateKeyboardMovement() {
    runKeyboardFrame({
      currentPlayer,
      hasKeysPressed: inputHandler.hasKeysPressed,
      interactionExit: getInteractionExitKind(playerState),
      hasMovementTarget: movingState() !== null,
      isInCombat: combatController.isInCombat,
      direction: inputHandler.getMovementDirection(),
      config: MOVEMENT_CONFIG,
      sampleHeight,
      isMovementBlocked,
      isUphillTooSteep,
      writePlayerPosition,
      sendPlayerMove,
      actions: keyboardFrameActions,
    })
  }

  function createMoveRequestActions(
    clickPosition: Position,
    pickupAfterArrival: number | null,
    options: { pickupAfterArrival?: number | null }
  ): MoveRequestActions {
    return {
      clearPendingPickupAfterMove: () => {
        const m = movingState()
        if (m) m.pendingPickupAfterMove = null
      },
      exitPickupAndRetry: () => {
        exitPickupInteraction()
        handleClickToMove(clickPosition, options)
      },
      exitObjectAndDelay: () => {
        exitObjectInteraction()

        if (standUpTimer) clearTimeout(standUpTimer)
        standUpTimer = setTimeout(() => {
          standUpTimer = null
          enqueuePlayerControlEvent({
            type: 'delayed_request_move',
            position: { ...clickPosition },
            pickupAfterArrival,
          })
        }, STAND_UP_DURATION)
      },
      applyStartedMovement: (started) => {
        playerRotation = started.playerRotation
        // The moving state OWNS the path data. Transition before emit: the
        // projection derives 'moving' from the machine's owned state.
        playerControlMachine.transition({
          name: 'moving',
          target: started.movementTarget,
          movementState: started.movementState,
          waypoints: started.pathWaypoints,
          waypointIndex: started.currentWaypointIndex,
          pendingPickupAfterMove: started.pendingPickupAfterMoveInstanceId,
        })
        updatePlayerState(started.movementState.totalDistance)
      },
    }
  }

  function handleClickToMove(
    clickPosition: Position,
    options: { pickupAfterArrival?: number | null } = {}
  ) {
    const pickupAfterArrival = options.pickupAfterArrival ?? null

    runMoveRequest({
      clickPosition,
      pickupAfterArrival,
      currentPlayer,
      interactionExit: getInteractionExitKind(playerState),
      isMoving: isMovingNow(),
      hasKeyboardInput: inputHandler.hasKeysPressed,
      currentFloor: Math.max(0, get(playerFloorLevel)),
      getFloorAt: passability_get_floor_at,
      findPath,
      sampleHeight,
      sendPlayerMove,
      actions: createMoveRequestActions(
        clickPosition,
        pickupAfterArrival,
        options
      ),
    })
  }

  function enterInteraction(intent: Extract<ClickIntent, { type: 'interact_object' }>) {
    if (getInteractionExitKind(playerState) === 'pickup') {
      finishPendingPickup()
    }

    const result = beginObjectInteraction({
      intent,
      previousPlayerState: playerState,
      cancelCombat: () => combatController.cancelCombat(),
    })

    // Entering object_interacting drops any moving data; just face the object.
    playerRotation = result.playerRotation
    setPlayerState(result.nextPlayerState)
    transitionTo('object_interacting')

    if (currentPlayer) {
      applyObjectInteractionPosition(currentPlayer, result.entryPosition, {
        hasHeightData: (x, z) => heightManager.hasHeightData(x, z),
        sampleHeight,
      })
    }

    networkManager.sendInteractObject(intent.objectType, intent.objectId)
  }

  function enterPickup(instanceId: number) {
    const result = beginPickupInteraction({
      instanceId,
      previousPlayerState: playerState,
      hasGroundItem: (id) => groundItemManager.items.has(id),
      beginPickup: (id) => groundItemManager.beginPickup(id),
      cancelCombat: () => combatController.cancelCombat(),
    })

    if (result.kind === 'ignored') return

    // The picking_up state OWNS the instance id being grabbed; entering it drops
    // any moving data (the far-pickup approach that led here).
    currentSpeed = 0
    setPlayerState(result.nextPlayerState)
    playerControlMachine.transition({
      name: 'picking_up',
      pendingPickupInstanceId: result.pendingPickupInstanceId,
    })
  }

  function approachAndPickup(intent: Extract<ClickIntent, { type: 'pickup_ground_item' }>) {
    const decision = decidePickupApproach({
      playerState,
      intent,
      getGroundItem: (instanceId) => groundItemManager.items.get(instanceId),
    })
    if (decision.kind === 'ignored_dead') return

    combatController.cancelCombat()
    handleClickToMove(decision.target, {
      pickupAfterArrival: decision.pickupAfterArrival,
    })
  }

  function handleCanvasClickIntent(event: MouseEvent) {
    const editorMode = $mapEditorMode || $housingEditorMode
    const playerControlEvent = createCanvasIntentEvent({
      event,
      editorMode,
      currentPlayer,
      processIntent: () =>
        inputHandler.processCanvasClick(event, {
          camera,
          monsterMeshes,
          npcMeshes,
          doorMeshes,
          objectMeshes,
          groundItemMeshes,
          groundMeshes,
          playerPosition: {
            x: currentPlayer!.position.x,
            y: currentPlayer!.position.y,
            z: currentPlayer!.position.z,
          },
          playerFloorLevel: get(playerFloorLevel),
          isMonsterDead: (id) => {
            const m = monsterManager.monsters.get(id)
            return m?.state === 'dead' || false
          },
        }),
    })
    if (!playerControlEvent) return

    enqueuePlayerControlEvent(playerControlEvent)
  }

  function createPlayerControlEventActions(): PlayerControlEventActions {
    return {
      attackInRange: (monsterId) => {
        // initiateAttack transitions to attacking, which drops any moving data
        // (no separate runtime reset needed).
        initiateAttack(monsterId)
      },
      chaseAndAttack: (monsterId, hitPoint) => {
        combatController.beginCombat(monsterId, false)
        handleClickToMove(hitPoint)
      },
      toggleDoor: (houseId, roomIndex, wallDir, segmentIndex) => {
        const m = movingState()
        if (m) m.pendingPickupAfterMove = null
        networkManager.sendToggleDoor(houseId, roomIndex, wallDir, segmentIndex)
      },
      enterInteraction,
      enterPickup,
      approachAndPickup,
      interactNpc: (intent) => {
        const npc = get(gameStore).otherPlayers.get(intent.playerId)
        if (!npc) return
        // Merchant NPCs open their shop; other NPCs have no click
        // interaction yet (planned: context menu with talk/trade).
        if (!getMerchantByNpcName(npc.name)) return

        if (intent.distance <= NPC_TRADE_RANGE_METERS) {
          networkManager.sendOpenShop(intent.playerId)
          return
        }

        // Too far: walk toward the merchant, stopping just short.
        if (!currentPlayer) return
        combatController.cancelCombat()
        const dx = currentPlayer.position.x - intent.position.x
        const dz = currentPlayer.position.z - intent.position.z
        const dist = Math.sqrt(dx * dx + dz * dz) || 1
        const stopShort = Math.min(NPC_TRADE_RANGE_METERS - 1, dist)
        handleClickToMove({
          x: intent.position.x + (dx / dist) * stopShort,
          y: intent.position.y,
          z: intent.position.z + (dz / dist) * stopShort,
        })
      },
      moveToGround: (position) => {
        combatController.cancelCombat()
        handleClickToMove(position)
      },
      requestMove: handleClickToMove,
      onInteractionFinished,
      onPickupGrab,
      onRespawned: transitionToRespawned,
      onInteractionRejected: () => {
        if (playerState.state === 'interact') exitObjectInteraction(false)
      },
    }
  }

  function dispatchPlayerControlEvent(event: PlayerControlEvent) {
    dispatchQueuedPlayerControlEvent(event, createPlayerControlEventActions())
  }

  const playerControlMachine = createLocalPlayerControlMachine({
    dispatchEvent: dispatchPlayerControlEvent,
    stateActions: {
      onInteractionFinished,
      onPickupGrab,
      clearJumpFeedbackTimer,
      onRespawned: transitionToRespawned,
      onInteractionRejected: () => {
        if (playerState.state === 'interact') exitObjectInteraction(false)
      },
      handleInteractKey: checkInteraction,
      handleKeyboard: updateKeyboardMovement,
      tick: updatePlayerMovement,
    },
  })

  export function updatePlayerControl(
    deltaTime: number,
    options: PlayerControlUpdateOptions
  ) {
    playerControlMachine.update(deltaTime, options)
  }

  // Hover speech bubble for placed objects that carry text (e.g. signposts).
  // Driven by pointermove (event-based, not per-frame) and raycast only against
  // the object overlay group, throttled to ~20 Hz — negligible cost.
  let lastHoverRaycast = 0
  let lastHoverKey: string | null = null
  let hoverTrailing: ReturnType<typeof setTimeout> | null = null
  let pendingHoverEvent: MouseEvent | null = null

  function runHover(event: MouseEvent) {
    lastHoverRaycast = performance.now()
    const hit = inputHandler.processHover(event, camera, objectMeshes)
    const key = hit
      ? `${hit.text}@${hit.position.x.toFixed(1)},${hit.position.z.toFixed(1)}`
      : null
    if (key === lastHoverKey) return
    lastHoverKey = key
    hoveredSignpost.set(
      hit
        ? { x: hit.position.x, y: hit.position.y, z: hit.position.z, text: hit.text }
        : null
    )
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
    hoveredSignpost.set(null)
  }

  onMount(() => {
    preloadSwordHitSound()
    preloadSwordMissSound()

    const removeInputListeners = inputHandler.setupEventListeners(
      renderer.domElement,
      handleCanvasClickIntent
    )

    const canvas = renderer.domElement
    canvas.addEventListener('pointermove', handlePointerHover)
    canvas.addEventListener('pointerleave', clearHover)

    const unsubscribeNetworkEvents = subscribePlayerNetworkEvents({
      isCurrentPlayerEligibleForRespawn: () =>
        !!currentPlayer && currentPlayer.health <= 0,
      isCurrentPlayer: (id) => !!currentPlayer && currentPlayer.id === id,
      isInteracting: () => playerState.state === 'interact',
      onRespawned: () => enqueuePlayerControlEvent({ type: 'network_respawned' }),
      onInteractionRejected: () =>
        enqueuePlayerControlEvent({ type: 'network_interaction_rejected' }),
    })

    return () => {
      removeInputListeners()
      canvas.removeEventListener('pointermove', handlePointerHover)
      canvas.removeEventListener('pointerleave', clearHover)
      clearHover()
      unsubscribeNetworkEvents()
      playerControlMachine.dispose()
      if (standUpTimer) clearTimeout(standUpTimer)
      clearJumpFeedbackTimer()
    }
  })
</script>

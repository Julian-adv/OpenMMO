<script lang="ts">
  import { onMount } from 'svelte'
  import { useThrelte } from '@threlte/core'
  import * as THREE from 'three'
  import { gameStore, hoveredSignpost, type LocalPlayer } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import { monsterManager } from '../managers/monsterManager'
  import { groundItemManager } from '../managers/groundItemManager'
  import { combatController } from '../managers/combatController'
  import { inputHandler, type ClickIntent } from '../managers/inputHandler'
  import { mapEditorMode, housingEditorMode, debugSpeedMode, torchLightEnabled } from '../stores/debugStore'
  import { localTorchEquipped } from '../stores/inventoryStore'
  import {
    calculateMovementStep,
    initMovementState,
    getMovementMode,
    DEFAULT_MOVEMENT_CONFIG,
    type Position,
    type MovementState,
    type MovementConfig,
    type PlayerState,
    type MovementMode,
  } from '../utils/movementUtils'
  import type { TerrainHeightManager } from '../managers/terrainHeightManager'
  import { playerFloorOffset, playerFloorLevel } from '../stores/housingStore'
  import { housingManager } from '../managers/housingManager'
  import { findPath } from '../managers/pathfinding'
  import { passability_get_floor_at } from '../wasm/onlinerpg_shared'
  import { get } from 'svelte/store'
  import { createPlayerPhysics } from './player-control/player-physics'
  import {
    buildJumpState,
    buildIdleAfterInteract,
    buildAttackState,
    buildIdleAfterAttack,
    buildDeadState,
    buildRespawnedState,
    buildInteractState,
    buildPickupState,
  } from './player-control/player-state-builders'
  import { subscribePlayerNetworkEvents } from './player-control/player-network-events'
  import { dispatchCanvasClickIntent } from './player-control/canvas-click-dispatcher'

  interface Props {
    onStateChange: (state: PlayerState) => void
    camera: THREE.Camera
    heightManager: TerrainHeightManager
    groundMeshes: THREE.Object3D[]
    groundItemMeshes: THREE.Object3D[]
    monsterMeshes: THREE.Group[]
    doorMeshes: THREE.Object3D[]
    objectMeshes: THREE.Object3D[]
    attackCooldown?: number
  }

  let { onStateChange, camera, heightManager, groundMeshes, groundItemMeshes, monsterMeshes, doorMeshes, objectMeshes, attackCooldown }: Props = $props()

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

  // Movement system
  let movementTarget = $state<Position | null>(null)
  let isMoving = $state(false)
  let movementState = $state<MovementState | null>(null)
  let lastSentPosition = $state<Position | null>(null)

  // A* pathfinding waypoints
  let pathWaypoints: Array<{ x: number; z: number; floor: number }> = []
  let currentWaypointIndex = 0

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

  /**
   * Briefly switch the player to the 'jump' state to play the jump animation
   * as a one-shot feedback that the terrain ahead is too steep. Cooldown
   * prevents the animation from restarting every frame while the user keeps
   * pushing into the slope.
   */
  function triggerJumpFeedback() {
    const now = Date.now()
    if (now - lastJumpFeedbackAt < JUMP_FEEDBACK_COOLDOWN_MS) return
    lastJumpFeedbackAt = now

    setPlayerState(buildJumpState(playerState))

    if (jumpFeedbackTimer) clearTimeout(jumpFeedbackTimer)
    jumpFeedbackTimer = setTimeout(() => {
      jumpFeedbackTimer = null
      // Only return to idle if we're still in the jump feedback state —
      // some other event (combat, interaction) may have already moved us on.
      if (playerState.state === 'jump') {
        updatePlayerState()
      }
    }, JUMP_FEEDBACK_DURATION_MS)
  }

  let pendingPickupInstanceId = $state<number | null>(null)

  function finishPendingPickup() {
    const id = pendingPickupInstanceId
    if (id === null) return
    groundItemManager.finishPickup(id)
    pendingPickupInstanceId = null
  }

  function exitPickupInteraction() {
    if (
      playerState.state !== 'interact' ||
      playerState.interactionAnim !== 'pickup'
    ) {
      return
    }
    finishPendingPickup()
    setPlayerState(buildIdleAfterInteract(playerState))
  }

  export function onInteractionFinished() {
    exitPickupInteraction()
  }

  export function onPickupGrab() {
    const id = pendingPickupInstanceId
    if (id === null) return
    groundItemManager.setInHand(id)
    if (id < 0) {
      groundItemManager.remove(id)
      return
    }
    networkManager.sendPickupItem(id)
  }

  $effect(() => {
    if (pendingPickupInstanceId === null) return
    const s = playerState
    if (s.state !== 'interact' || s.interactionAnim !== 'pickup') {
      finishPendingPickup()
    }
  })

  function exitObjectInteraction(notify = true) {
    if (currentPlayer) {
      const footDist = 0.7
      const fx = currentPlayer.position.x + Math.sin(playerRotation) * footDist
      const fz = currentPlayer.position.z + Math.cos(playerRotation) * footDist
      currentPlayer.position.x = fx
      currentPlayer.position.z = fz
      if (heightManager.hasHeightData(fx, fz)) {
        currentPlayer.position.y = sampleHeight(fx, fz)
      }
    }

    setPlayerState(buildIdleAfterInteract(playerState))

    if (notify) {
      networkManager.sendStopInteraction()
    }
  }

  function stopMovement() {
    isMoving = false
    movementTarget = null
    movementState = null
    currentSpeed = 0
    pathWaypoints = []
    currentWaypointIndex = 0
    if (standUpTimer) {
      clearTimeout(standUpTimer)
      standUpTimer = null
    }
    updatePlayerState()
  }

  // Wrapper for sending move packets to track last sent position
  function sendPlayerMove(position: Position, rotation: number) {
    lastSentPosition = { ...position }
    networkManager.sendPlayerMove(position, rotation, Math.max(0, get(playerFloorLevel)))
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

    // Determine movement mode based on distance or if chasing a monster.
    // Torch has no jog animation, so fall back to walk when no distance is known.
    const hasTorch = $localTorchEquipped || $torchLightEnabled
    let movementMode: MovementMode | undefined
    if (isMoving) {
      if (combatController.isInCombat) {
        movementMode = 'run'
      } else if (totalDistance !== undefined) {
        movementMode = getMovementMode(totalDistance, hasTorch)
      } else {
        movementMode = hasTorch ? 'walk' : 'jog'
      }
    }

    const newState: PlayerState = {
      state: isMoving ? 'moving' : 'idle',
      speed: currentSpeed,
      rotation: playerRotation,
      position: currentPosition,
      movementMode,
      attackCounter: combatController.isInCombat
        ? combatController.attackCounter
        : undefined,
    }

    // Only update if state actually changed
    if (
      newState.state !== playerState.state ||
      Math.abs(newState.speed - playerState.speed) > 0.01 ||
      newState.rotation !== playerState.rotation ||
      Math.abs(newState.position.x - playerState.position.x) > 0.01 ||
      Math.abs(newState.position.z - playerState.position.z) > 0.01 ||
      newState.movementMode !== playerState.movementMode ||
      newState.attackCounter !== playerState.attackCounter
    ) {
      playerState = newState
      onStateChange(newState)
    }
  }

  // Initiate attack on a monster
  function initiateAttack(monsterId: string) {
    const monsterData = monsterManager.monsters.get(monsterId)
    if (monsterData?.state === 'dead' || monsterData?.isDeadPending) return

    combatController.beginCombat(monsterId, true)

    // Ensure position sync
    if (currentPlayer) {
      const currentPos: Position = {
        x: currentPlayer.position.x,
        y: currentPlayer.position.y,
        z: currentPlayer.position.z,
      }

      const shouldSendMove =
        !lastSentPosition ||
        Math.abs(currentPos.x - lastSentPosition.x) > 0.01 ||
        Math.abs(currentPos.z - lastSentPosition.z) > 0.01

      if (shouldSendMove) {
        sendPlayerMove(currentPos, playerRotation)
      }
    }

    setPlayerState(buildAttackState(playerState))

    networkManager.sendPlayerAttack(monsterId)
  }

  // Transition from attack to idle state
  function transitionToIdle() {
    if (playerState.state === 'attack') {
      setPlayerState(buildIdleAfterAttack(playerState))
    }
  }

  function transitionToDead() {
    if (playerState.state === 'dead') return

    isMoving = false
    movementTarget = null
    movementState = null
    combatController.cancelCombat()
    currentSpeed = 0

    setPlayerState(buildDeadState(playerState))
  }

  function transitionToRespawned() {
    if (!currentPlayer) return

    isMoving = false
    movementTarget = null
    movementState = null
    combatController.cancelCombat()
    currentSpeed = 0
    playerRotation = 0

    setPlayerState(buildRespawnedState(
      playerState,
      {
        x: currentPlayer.position.x,
        y: currentPlayer.position.y,
        z: currentPlayer.position.z,
      },
      playerRotation,
    ))
  }

  /** Check E key interaction (door toggle). Call from game loop. */
  export function checkInteraction() {
    if (!currentPlayer || currentPlayer.health <= 0) return
    if (!inputHandler.consumeInteract()) return

    const door = housingManager.findNearestDoor(
      currentPlayer.position.x,
      currentPlayer.position.z,
      currentPlayer.position.y,
      2.0
    )
    if (!door) return

    networkManager.sendToggleDoor(door.houseId, door.roomIndex, door.wallDir, door.segmentIndex)
  }

  // Update player movement (click-to-move) with acceleration/deceleration
  export function updatePlayerMovement(deltaTime: number) {
    // Dead players cannot move
    if (currentPlayer && currentPlayer.health <= 0) {
      transitionToDead()
      return
    }

    // Keep player Y aligned with terrain height (handles spawn and terrain edits)
    // Skip during object interaction — character is positioned on the object
    if (playerState.state !== 'interact' && currentPlayer && heightManager.hasHeightData(currentPlayer.position.x, currentPlayer.position.z)) {
      const terrainY = sampleHeight(currentPlayer.position.x, currentPlayer.position.z)
      if (Math.abs(currentPlayer.position.y - terrainY) > 0.001) {
        currentPlayer.position.y = terrainY
      }
    }

    // Combat update
    if (combatController.isInCombat && currentPlayer) {
      const targetId = combatController.targetMonsterId!
      const monsterData = monsterManager.monsters.get(targetId)
      const monsterObjPos = monsterManager.findMeshPosition(targetId, monsterMeshes)
      const cooldownMs = attackCooldown ? attackCooldown * 1000 : 1500

      const result = combatController.update(
        deltaTime,
        { x: currentPlayer.position.x, y: currentPlayer.position.y, z: currentPlayer.position.z },
        monsterData
          ? {
              state: monsterData.state,
              isDeadPending: monsterData.isDeadPending,
            }
          : undefined,
        monsterObjPos,
        isMoving,
        cooldownMs,
        playerState.state
      )

      switch (result.action) {
        case 'idle': {
          if (isMoving) {
            isMoving = false
            movementTarget = null
            movementState = null
            updatePlayerState()
          }
          transitionToIdle()
          return
        }

        case 'reached_attack_range': {
          isMoving = false
          movementTarget = null
          movementState = null
          currentSpeed = 0
          updatePlayerState()
          initiateAttack(targetId)
          return
        }

        case 'chasing': {
          if (result.newTarget) {
            if (
              !movementTarget ||
              Math.abs(movementTarget.x - result.newTarget.x) > 0.1 ||
              Math.abs(movementTarget.z - result.newTarget.z) > 0.1
            ) {
              movementTarget = result.newTarget
              if (movementState) {
                movementState.targetPos = { ...result.newTarget }
                const dx = result.newTarget.x - currentPlayer.position.x
                const dz = result.newTarget.z - currentPlayer.position.z
                movementState.totalDistance = Math.sqrt(dx * dx + dz * dz)
                movementState.startPos = {
                  x: currentPlayer.position.x,
                  y: currentPlayer.position.y,
                  z: currentPlayer.position.z,
                }
              } else {
                movementState = initMovementState(
                  {
                    x: currentPlayer.position.x,
                    y: currentPlayer.position.y,
                    z: currentPlayer.position.z,
                  },
                  result.newTarget,
                  currentSpeed
                )
              }
              sendPlayerMove(result.newTarget, playerRotation)
            }
          }
          break // Fall through to movement processing
        }

        case 'attacking': {
          playerRotation = result.rotation
          if (playerState.state !== 'attack') {
            setPlayerState(buildAttackState(playerState, result.rotation))
          }
          return
        }

        case 'attack_cycle': {
          playerRotation = result.rotation
          networkManager.sendPlayerAttack(result.monsterId)
          updatePlayerState()
          return
        }

        case 'none':
          break
      }
    }

    // Movement processing
    if (!isMoving || !movementTarget || !currentPlayer || !movementState) {
      if (currentSpeed > 0) {
        currentSpeed = 0
        updatePlayerState()
      }
      return
    }

    const currentPos: Position = {
      x: currentPlayer.position.x,
      y: currentPlayer.position.y,
      z: currentPlayer.position.z,
    }

    const deltaTimeSeconds = deltaTime / 1000

    // Use the shared movement calculation
    const result = calculateMovementStep(
      currentPos,
      movementState,
      MOVEMENT_CONFIG,
      deltaTimeSeconds
    )

    // Update movement state speed
    movementState.currentSpeed = result.newSpeed
    currentSpeed = result.newSpeed
    playerRotation = result.rotation

    if (result.arrived) {
      // Check wall collision before finalizing arrival
      if (
        movementTarget &&
        isMovementBlocked(
          currentPos.x,
          currentPos.z,
          movementTarget.x,
          movementTarget.z,
          currentPos.y
        )
      ) {
        stopMovement()
        return
      }

      // Apply arrived waypoint's floor before updating position so that
      // GameSceneHousingLayer filters stairwells correctly on the next frame.
      // This is critical for stacked stairwells at the same XZ where the
      // player transitions floors without physical XZ movement.
      const arrivedWp = pathWaypoints[currentWaypointIndex]
      if (arrivedWp && arrivedWp.floor !== get(playerFloorLevel)) {
        playerFloorLevel.set(arrivedWp.floor)
      }

      gameStore.update((state) => {
        if (state.currentPlayer && movementTarget) {
          const y = sampleHeight(movementTarget.x, movementTarget.z)
          state.currentPlayer.position.set(movementTarget.x, y, movementTarget.z)
          state.currentPlayer.rotation = playerRotation
        }
        return state
      })

      currentWaypointIndex++
      if (currentWaypointIndex < pathWaypoints.length) {
        const nextWp = pathWaypoints[currentWaypointIndex]

        if (nextWp.floor !== get(playerFloorLevel)) {
          playerFloorLevel.set(nextWp.floor)
        }

        const wpPos: Position = {
          x: nextWp.x,
          y: sampleHeight(nextWp.x, nextWp.z),
          z: nextWp.z,
        }

        const ndx = wpPos.x - movementTarget!.x
        const ndz = wpPos.z - movementTarget!.z
        playerRotation = Math.atan2(ndx, ndz)

        const prevSpeed = movementState?.currentSpeed ?? 0
        movementState = initMovementState(movementTarget!, wpPos, prevSpeed)
        movementTarget = wpPos

        sendPlayerMove(wpPos, playerRotation)
        return
      }

      sendPlayerMove(movementTarget, playerRotation)
      stopMovement()

      if (combatController.isInCombat) {
        initiateAttack(combatController.targetMonsterId!)
      }
    } else {
      // Check wall collision before updating position
      if (
        isMovementBlocked(
          currentPos.x,
          currentPos.z,
          result.newPos.x,
          result.newPos.z,
          currentPos.y
        )
      ) {
        stopMovement()
        return
      }

      // Slope-too-steep check: look ahead in the movement direction and
      // refuse to climb if the terrain there is steeper than the limit.
      // dirX/dirZ derived from rotation (= atan2(dx, dz) → x = sin, z = cos).
      const dirX = Math.sin(result.rotation)
      const dirZ = Math.cos(result.rotation)
      if (isUphillTooSteep(currentPos.x, currentPos.z, currentPos.y, dirX, dirZ)) {
        stopMovement()
        triggerJumpFeedback()
        return
      }

      gameStore.update((state) => {
        if (state.currentPlayer) {
          const y = sampleHeight(result.newPos.x, result.newPos.z)
          state.currentPlayer.position.set(result.newPos.x, y, result.newPos.z)
          state.currentPlayer.rotation = playerRotation
        }
        return state
      })
      updatePlayerState(movementState.totalDistance)
    }
  }

  // Keyboard movement system
  export function updateKeyboardMovement() {
    if (!currentPlayer || !inputHandler.hasKeysPressed) {
      return
    }

    // Stand up first when leaving object interaction
    if (playerState.state === 'interact') {
      if (playerState.interactionAnim === 'pickup') {
        exitPickupInteraction()
      } else {
        exitObjectInteraction()
      }
    }

    // Cancel click-to-move if keyboard input detected
    if (inputHandler.hasKeysPressed && movementTarget) {
      movementTarget = null
      movementState = null
      combatController.cancelCombat()
    }

    if (inputHandler.hasKeysPressed && combatController.isInCombat) {
      combatController.cancelCombat()
    }

    const dir = inputHandler.getMovementDirection()

    // Apply keyboard movement if any keys are pressed
    if (dir) {
      // Use fixed speed for keyboard movement (instant response)
      currentSpeed = MOVEMENT_CONFIG.maxSpeed
      const speed = MOVEMENT_CONFIG.maxSpeed * (1000 / 120 / 1000) // Adjust for frame rate (120 FPS target)
      let newX = currentPlayer.position.x + dir.x * speed
      let newZ = currentPlayer.position.z + dir.z * speed

      // Wall collision check (use current Y for correct floor matching)
      if (
        isMovementBlocked(
          currentPlayer.position.x,
          currentPlayer.position.z,
          newX,
          newZ,
          currentPlayer.position.y
        )
      ) {
        stopMovement()
        return
      }

      // Slope-too-steep check (uphill only): refuse to climb terrain
      // steeper than MAX_TRAVERSABLE_SLOPE_DEG and play jump as feedback.
      if (
        isUphillTooSteep(
          currentPlayer.position.x,
          currentPlayer.position.z,
          currentPlayer.position.y,
          dir.x,
          dir.z
        )
      ) {
        stopMovement()
        triggerJumpFeedback()
        return
      }

      const groundY = sampleHeight(newX, newZ)

      // Calculate rotation based on movement direction
      playerRotation = Math.atan2(dir.x, dir.z)

      gameStore.update((state) => {
        if (state.currentPlayer) {
          state.currentPlayer.position.set(newX, groundY, newZ)
          state.currentPlayer.rotation = playerRotation
          isMoving = true
        }
        return state
      })

      // Send position to server periodically
      sendPlayerMove(
        {
          x: newX,
          y: groundY,
          z: newZ,
        },
        playerRotation
      )
    } else {
      isMoving = false
      currentSpeed = 0
    }

    // Keyboard movement uses large distance to always show RUN animation
    updatePlayerState(isMoving ? 100 : undefined)
  }

  export function handleClickToMove(clickPosition: Position) {
    if (currentPlayer && currentPlayer.health <= 0) return

    // Stand up first when leaving object interaction
    if (playerState.state === 'interact') {
      if (playerState.interactionAnim === 'pickup') {
        exitPickupInteraction()
        handleClickToMove(clickPosition)
        return
      }

      exitObjectInteraction()

      if (standUpTimer) clearTimeout(standUpTimer)
      standUpTimer = setTimeout(() => {
        standUpTimer = null
        handleClickToMove(clickPosition)
      }, STAND_UP_DURATION)
      return
    }

    if (!currentPlayer || isMoving || inputHandler.hasKeysPressed) {
      // Allow overriding current movement with new click
      if (currentPlayer && isMoving && !inputHandler.hasKeysPressed) {
        // Proceed
      } else {
        return
      }
    }

    if (!currentPlayer) return

    const currentPos: Position = {
      x: currentPlayer.position.x,
      y: currentPlayer.position.y,
      z: currentPlayer.position.z,
    }

    const startFloor = Math.max(0, get(playerFloorLevel))
    const goalFloor = passability_get_floor_at(
      clickPosition.x,
      clickPosition.z,
      clickPosition.y
    )
    const result = findPath(
      currentPos.x,
      currentPos.z,
      startFloor,
      clickPosition.x,
      clickPosition.z,
      goalFloor
    )
    if (result.waypoints.length > 0) {
      pathWaypoints = result.waypoints
    } else {
      // No path (open terrain or unreachable) — direct move fallback
      pathWaypoints = [{ x: clickPosition.x, z: clickPosition.z, floor: goalFloor }]
    }
    currentWaypointIndex = 0

    const firstWp = pathWaypoints[0]
    const wpPos: Position = {
      x: firstWp.x,
      y: sampleHeight(firstWp.x, firstWp.z),
      z: firstWp.z,
    }

    const dx = wpPos.x - currentPos.x
    const dz = wpPos.z - currentPos.z
    playerRotation = Math.atan2(dx, dz)

    movementState = initMovementState(currentPos, wpPos, 0)
    movementTarget = wpPos
    isMoving = true

    sendPlayerMove(wpPos, playerRotation)

    updatePlayerState(movementState.totalDistance)
  }

  function enterInteraction(intent: Extract<ClickIntent, { type: 'interact_object' }>) {
    combatController.cancelCombat()
    isMoving = false
    movementTarget = null

    playerRotation = intent.rotation
    const offset = intent.interactOffset

    setPlayerState(buildInteractState(
      playerState,
      intent.position,
      playerRotation,
      intent.interaction,
      offset?.y ?? 0,
    ))

    if (currentPlayer) {
      const fx = intent.position.x + (offset?.x ?? 0)
      const fz = intent.position.z + (offset?.z ?? 0)
      currentPlayer.position.x = fx
      currentPlayer.position.z = fz
      if (heightManager.hasHeightData(fx, fz)) {
        currentPlayer.position.y = sampleHeight(fx, fz)
      }
    }

    networkManager.sendInteractObject(intent.objectType, intent.objectId)
  }

  function enterPickup(instanceId: number) {
    if (playerState.state === 'dead') return

    groundItemManager.beginPickup(instanceId)
    pendingPickupInstanceId = instanceId

    combatController.cancelCombat()
    isMoving = false
    movementTarget = null
    movementState = null
    currentSpeed = 0

    setPlayerState(buildPickupState(playerState))
  }

  function handleCanvasClickIntent(event: MouseEvent) {
    const editorMode = $mapEditorMode || $housingEditorMode
    const expectedButton = editorMode ? 2 : 0
    if (event.button !== expectedButton) return
    if (!currentPlayer || currentPlayer.health <= 0) return

    const intent = inputHandler.processCanvasClick(event, {
      camera,
      monsterMeshes,
      doorMeshes,
      objectMeshes,
      groundItemMeshes,
      groundMeshes,
      playerPosition: {
        x: currentPlayer.position.x,
        y: currentPlayer.position.y,
        z: currentPlayer.position.z,
      },
      playerFloorLevel: get(playerFloorLevel),
      isMonsterDead: (id) => {
        const m = monsterManager.monsters.get(id)
        return m?.state === 'dead' || false
      },
    })

    dispatchCanvasClickIntent(intent, editorMode, {
      attackInRange: (monsterId) => {
        initiateAttack(monsterId)
        isMoving = false
        movementTarget = null
      },
      chaseAndAttack: (monsterId, hitPoint) => {
        combatController.beginCombat(monsterId, false)
        handleClickToMove(hitPoint)
      },
      toggleDoor: (houseId, roomIndex, wallDir, segmentIndex) => {
        networkManager.sendToggleDoor(houseId, roomIndex, wallDir, segmentIndex)
      },
      enterInteraction,
      enterPickup,
      moveToGround: (position) => {
        combatController.cancelCombat()
        handleClickToMove(position)
      },
    })
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
      onRespawned: transitionToRespawned,
      onInteractionRejected: () => exitObjectInteraction(false),
    })

    return () => {
      removeInputListeners()
      canvas.removeEventListener('pointermove', handlePointerHover)
      canvas.removeEventListener('pointerleave', clearHover)
      clearHover()
      unsubscribeNetworkEvents()
      if (standUpTimer) clearTimeout(standUpTimer)
      if (jumpFeedbackTimer) clearTimeout(jumpFeedbackTimer)
    }
  })
</script>

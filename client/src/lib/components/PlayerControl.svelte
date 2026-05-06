<script lang="ts">
  import { onMount } from 'svelte'
  import * as THREE from 'three'
  import { gameStore, type LocalPlayer } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import { monsterManager } from '../managers/monsterManager'
  import { groundItemManager } from '../managers/groundItemManager'
  import { combatController } from '../managers/combatController'
  import { inputHandler } from '../managers/inputHandler'
  import { mapEditorMode, housingEditorMode, debugSpeedMode, torchLightEnabled } from '../stores/debugStore'
  import { localTorchEquipped } from '../stores/inventoryStore'
  import {
    calculateMovementStep,
    initMovementState,
    getMovementMode,
    isSlopeTooSteepUphill,
    SLOPE_LOOKAHEAD_DISTANCE,
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
  import { bridgeManager } from '../managers/bridgeManager'
  import { findPath } from '../managers/pathfinding'
  import { passability_get_floor_at } from '../wasm/onlinerpg_shared'
  import { get } from 'svelte/store'

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

  function sampleHeight(x: number, z: number): number {
    const deckY = bridgeManager.findDeckYAt(x, z, currentPlayer?.position.y ?? null)
    if (deckY !== null) return deckY
    return heightManager.getHeightAtWorldPosition(x, z) + floorOffset
  }

  function isMovementBlocked(
    fromX: number,
    fromZ: number,
    toX: number,
    toZ: number,
    y: number
  ): boolean {
    if (housingManager.isMovementBlocked(fromX, fromZ, toX, toZ, y)) return true
    if (bridgeManager.isMovementBlocked(fromX, fromZ, toX, toZ, y)) return true
    return false
  }

  let currentPlayer = $state<LocalPlayer | null>(null)

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
   * Sample the terrain ahead of `from` along (dirX, dirZ) and report whether
   * the climb to that point would exceed MAX_TRAVERSABLE_SLOPE_DEG.
   *
   * Uses sampleHeight() so it respects bridges/housing floor offsets the
   * same way as the player's vertical position.
   */
  function isUphillTooSteep(
    fromX: number,
    fromZ: number,
    fromY: number,
    dirX: number,
    dirZ: number
  ): boolean {
    const aheadX = fromX + dirX * SLOPE_LOOKAHEAD_DISTANCE
    const aheadZ = fromZ + dirZ * SLOPE_LOOKAHEAD_DISTANCE
    const aheadY = sampleHeight(aheadX, aheadZ)
    return isSlopeTooSteepUphill(fromY, aheadY, SLOPE_LOOKAHEAD_DISTANCE)
  }

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

    const jumpState: PlayerState = {
      ...playerState,
      state: 'jump',
      speed: 0,
      movementMode: undefined,
      attackCounter: undefined,
    }
    playerState = jumpState
    onStateChange(jumpState)

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

  function exitPickupInteraction() {
    if (
      playerState.state !== 'interact' ||
      playerState.interactionAnim !== 'pickup'
    ) {
      return
    }
    const idleState: PlayerState = {
      ...playerState,
      state: 'idle',
      speed: 0,
      interactionAnim: undefined,
      interactOffsetY: undefined,
    }
    playerState = idleState
    onStateChange(idleState)
  }

  export function onInteractionFinished() {
    exitPickupInteraction()
  }

  export function onPickupGrab() {
    if (pendingPickupInstanceId !== null) {
      networkManager.sendPickupItem(pendingPickupInstanceId)
      groundItemManager.setInHand(pendingPickupInstanceId)
    }
  }

  $effect(() => {
    const id = pendingPickupInstanceId
    if (id === null) return
    const s = playerState
    if (s.state !== 'interact' || s.interactionAnim !== 'pickup') {
      groundItemManager.finishPickup(id)
      pendingPickupInstanceId = null
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

    const idleState: PlayerState = {
      ...playerState,
      state: 'idle',
      speed: 0,
      interactionAnim: undefined,
      interactOffsetY: undefined,
    }
    playerState = idleState
    onStateChange(idleState)

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

    const newPlayerState = {
      ...playerState,
      state: 'attack',
    } as PlayerState

    playerState = newPlayerState
    onStateChange(newPlayerState)

    networkManager.sendPlayerAttack(monsterId)
  }

  // Transition from attack to idle state
  function transitionToIdle() {
    if (playerState.state === 'attack') {
      const idleState = {
        ...playerState,
        state: 'idle',
        attackCounter: 0,
      } as PlayerState
      playerState = idleState
      onStateChange(idleState)
    }
  }

  function transitionToDead() {
    if (playerState.state === 'dead') return

    isMoving = false
    movementTarget = null
    movementState = null
    combatController.cancelCombat()
    currentSpeed = 0

    const deadState: PlayerState = {
      ...playerState,
      state: 'dead',
      speed: 0,
      movementMode: undefined,
    }
    playerState = deadState
    onStateChange(deadState)
  }

  function transitionToRespawned() {
    if (!currentPlayer) return

    isMoving = false
    movementTarget = null
    movementState = null
    combatController.cancelCombat()
    currentSpeed = 0
    playerRotation = 0

    const revivedState: PlayerState = {
      ...playerState,
      state: 'idle',
      speed: 0,
      rotation: playerRotation,
      movementMode: undefined,
      attackCounter: 0,
      position: {
        x: currentPlayer.position.x,
        y: currentPlayer.position.y,
        z: currentPlayer.position.z,
      },
    }
    playerState = revivedState
    onStateChange(revivedState)
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
            const attackState = {
              ...playerState,
              state: 'attack',
              rotation: result.rotation,
            } as PlayerState
            playerState = attackState
            onStateChange(attackState)
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

  // Handle canvas click intent from input handler
  function handleCanvasClickIntent(event: MouseEvent) {
    if ($housingEditorMode) return
    const expectedButton = $mapEditorMode ? 2 : 0
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

    if ($mapEditorMode && intent.type !== 'move_to_ground') return

    switch (intent.type) {
      case 'attack_monster': {
        if (intent.distance < 2.0) {
          initiateAttack(intent.monsterId)
          isMoving = false
          movementTarget = null
        } else {
          combatController.beginCombat(intent.monsterId, false)
          handleClickToMove(intent.hitPoint)
        }
        break
      }
      case 'toggle_door': {
        networkManager.sendToggleDoor(
          intent.houseId,
          intent.roomIndex,
          intent.wallDir,
          intent.segmentIndex
        )
        break
      }
      case 'interact_object': {
        combatController.cancelCombat()
        isMoving = false
        movementTarget = null

        // Align character with object rotation (face +Z / south)
        playerRotation = intent.rotation

        const interactState: PlayerState = {
          ...playerState,
          state: 'interact',
          speed: 0,
          rotation: playerRotation,
          position: intent.position,
          interactionAnim: intent.interaction,
          interactOffsetY: intent.interactOffset?.y ?? 0,
        }
        playerState = interactState
        onStateChange(interactState)

        if (currentPlayer) {
          const fx = intent.position.x + (intent.interactOffset?.x ?? 0)
          const fz = intent.position.z + (intent.interactOffset?.z ?? 0)
          currentPlayer.position.x = fx
          currentPlayer.position.z = fz
          if (heightManager.hasHeightData(fx, fz)) {
            currentPlayer.position.y = sampleHeight(fx, fz)
          }
        }

        networkManager.sendInteractObject(intent.objectType, intent.objectId)
        break
      }
      case 'pickup_ground_item': {
        if (playerState.state === 'dead') break

        groundItemManager.beginPickup(intent.instanceId)
        pendingPickupInstanceId = intent.instanceId

        combatController.cancelCombat()
        isMoving = false
        movementTarget = null
        movementState = null
        currentSpeed = 0

        const pickupState: PlayerState = {
          ...playerState,
          state: 'interact',
          speed: 0,
          interactionAnim: 'pickup',
        }
        playerState = pickupState
        onStateChange(pickupState)
        break
      }
      case 'move_to_ground': {
        combatController.cancelCombat()
        handleClickToMove(intent.position)
        break
      }
    }
  }

  let respawnRequested = $state(false)

  onMount(() => {
    const removeInputListeners = inputHandler.setupEventListeners(
      handleCanvasClickIntent
    )

    const unsubscribeRespawnRequested = networkManager.respawnRequested.on(() => {
      if (!currentPlayer || currentPlayer.health > 0 || respawnRequested) return
      respawnRequested = true
    })

    const unsubscribePlayerRespawned = networkManager.playerRespawned.on(
      (playerId) => {
        if (!currentPlayer || currentPlayer.id !== playerId) return
        respawnRequested = false
        transitionToRespawned()
      }
    )

    const unsubscribeInteractionRejected = networkManager.interactionRejected.on(
      () => {
        if (playerState.state === 'interact') {
          exitObjectInteraction(false)
        }
      }
    )

    return () => {
      removeInputListeners()
      unsubscribeRespawnRequested()
      unsubscribePlayerRespawned()
      unsubscribeInteractionRejected()
      if (standUpTimer) clearTimeout(standUpTimer)
      if (jumpFeedbackTimer) clearTimeout(jumpFeedbackTimer)
    }
  })
</script>

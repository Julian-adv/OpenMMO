<script lang="ts">
  import { onMount } from 'svelte'
  import * as THREE from 'three'
  import { gameStore, type LocalPlayer } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import { monsterManager } from '../managers/monsterManager'
  import { combatController } from '../managers/combatController'
  import { inputHandler } from '../managers/inputHandler'
  import { mapEditorMode, housingEditorMode, debugSpeedMode } from '../stores/debugStore'
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
  import { playerFloorOffset } from '../stores/housingStore'
  import { housingManager } from '../managers/housingManager'
  import { get } from 'svelte/store'

  interface Props {
    onStateChange: (state: PlayerState) => void
    camera: THREE.Camera
    heightManager: TerrainHeightManager
    groundMeshes: THREE.Object3D[]
    monsterMeshes: THREE.Group[]
    attackCooldown?: number
  }

  let { onStateChange, camera, heightManager, groundMeshes, monsterMeshes, attackCooldown }: Props = $props()

  function sampleHeight(x: number, z: number): number {
    return heightManager.getHeightAtWorldPosition(x, z) + get(playerFloorOffset)
  }

  let currentPlayer = $state<LocalPlayer | null>(null)

  // Movement system
  let movementTarget = $state<Position | null>(null)
  let isMoving = $state(false)
  let movementState = $state<MovementState | null>(null)
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

  function stopMovement() {
    isMoving = false
    movementTarget = null
    movementState = null
    currentSpeed = 0
    updatePlayerState()
  }

  // Wrapper for sending move packets to track last sent position
  function sendPlayerMove(position: Position, rotation: number) {
    lastSentPosition = { ...position }
    networkManager.sendPlayerMove(position, rotation)
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

    // Determine movement mode based on distance or if chasing a monster
    let movementMode: MovementMode | undefined
    if (isMoving) {
      if (combatController.isInCombat) {
        movementMode = 'run'
      } else if (totalDistance !== undefined) {
        movementMode = getMovementMode(totalDistance)
      } else {
        movementMode = 'jog'
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
    if (currentPlayer && heightManager.hasHeightData(currentPlayer.position.x, currentPlayer.position.z)) {
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
        housingManager.isMovementBlocked(
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

      // Movement complete
      gameStore.update((state) => {
        if (state.currentPlayer && movementTarget) {
          const y = sampleHeight(movementTarget.x, movementTarget.z)
          state.currentPlayer.position.set(movementTarget.x, y, movementTarget.z)
        }
        return state
      })

      // Send final position to server
      sendPlayerMove(movementTarget, playerRotation)

      stopMovement()

      // If we were chasing a target, attack it now
      if (combatController.isInCombat) {
        initiateAttack(combatController.targetMonsterId!)
      }
    } else {
      // Check wall collision before updating position
      if (
        housingManager.isMovementBlocked(
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

      gameStore.update((state) => {
        if (state.currentPlayer) {
          const y = sampleHeight(result.newPos.x, result.newPos.z)
          state.currentPlayer.position.set(result.newPos.x, y, result.newPos.z)
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
        housingManager.isMovementBlocked(
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

      const groundY = sampleHeight(newX, newZ)

      // Calculate rotation based on movement direction
      playerRotation = Math.atan2(dir.x, dir.z)

      gameStore.update((state) => {
        if (state.currentPlayer) {
          state.currentPlayer.position.set(newX, groundY, newZ)
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

  // Handle click-to-move
  export function handleClickToMove(clickPosition: Position) {
    if (currentPlayer && currentPlayer.health <= 0) return
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

    // Calculate rotation to face target direction
    const dx = clickPosition.x - currentPos.x
    const dz = clickPosition.z - currentPos.z
    playerRotation = Math.atan2(dx, dz)

    // Initialize movement state using shared utility
    movementState = initMovementState(currentPos, clickPosition, 0)
    movementTarget = clickPosition
    isMoving = true

    // Send target position to server when movement starts
    sendPlayerMove(clickPosition, playerRotation)

    updatePlayerState(movementState.totalDistance)
  }

  // Handle canvas click intent from input handler
  function handleCanvasClickIntent(event: MouseEvent) {
    if ($mapEditorMode || $housingEditorMode) return
    if (!currentPlayer || currentPlayer.health <= 0) return

    const intent = inputHandler.processCanvasClick(event, {
      camera,
      monsterMeshes,
      groundMeshes,
      playerPosition: {
        x: currentPlayer.position.x,
        y: currentPlayer.position.y,
        z: currentPlayer.position.z,
      },
      isMonsterDead: (id) => {
        const m = monsterManager.monsters.get(id)
        return m?.state === 'dead' || false
      },
    })

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

    return () => {
      removeInputListeners()
      unsubscribeRespawnRequested()
      unsubscribePlayerRespawned()
    }
  })
</script>

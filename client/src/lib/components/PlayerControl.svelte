<script lang="ts">
  import { onMount } from 'svelte'
  import { Vector2, Raycaster } from 'three'
  import * as THREE from 'three'
  import { gameStore, type Player } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import { monsterManager } from '../managers/monsterManager'
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

  interface Props {
    onStateChange: (state: PlayerState) => void
    camera: THREE.PerspectiveCamera
    groundMesh: THREE.Mesh
    monsterMeshes: THREE.Group[]
    attackCooldown?: number
  }

  let { onStateChange, camera, groundMesh, monsterMeshes, attackCooldown }: Props = $props()

  let currentPlayer = $state<Player | null>(null)
  let keysPressed = $state(new Set<string>())

  // Movement system
  let movementTarget = $state<Position | null>(null)
  let isMoving = $state(false)
  let movementState = $state<MovementState | null>(null)
  let targetMonsterId = $state<string | null>(null)
  let lastChaseUpdate = 0
  let lastSentPosition = $state<Position | null>(null)
  let attackTimer = 0
  let attackCounter = $state(0)

  // Use the same movement config as remote players
  const MOVEMENT_CONFIG: MovementConfig = {
    ...DEFAULT_MOVEMENT_CONFIG,
  }

  // Character rotation and current speed
  let playerRotation = $state(0)
  let currentSpeed = $state(0)

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
      if (targetMonsterId) {
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
      attackCounter: targetMonsterId ? attackCounter : undefined,
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

  // Update player movement (click-to-move) with acceleration/deceleration
  export function updatePlayerMovement(deltaTime: number) {
    // If we have a target monster
    if (targetMonsterId && currentPlayer) {
      // Check if monster exists and its state
      const monsterData = monsterManager.monsters.get(targetMonsterId)
      const cooldownMs = attackCooldown ? attackCooldown * 1000 : 1500
      const isFinishingAttack =
        playerState.state === 'attack' && attackTimer < cooldownMs

      // Only clear target if monster is gone/dead AND we aren't in the middle of a final swing
      if (!monsterData || (monsterData.state === 'dead' && !isFinishingAttack)) {
        targetMonsterId = null
        attackCounter = 0
        attackTimer = 0
        if (isMoving) {
          isMoving = false
          movementTarget = null
          movementState = null
          updatePlayerState()
        } else if (playerState.state === 'attack') {
          const idleState = {
            ...playerState,
            state: 'idle',
            attackCounter: 0,
          } as PlayerState
          playerState = idleState
          onStateChange(idleState)
        }
        return
      }

      // Find the monster object
      let monsterObj: THREE.Object3D | undefined
      // TODO: Optimize lookup
      for (const group of monsterMeshes) {
        if (group) {
          let found = false
          group.traverse((child) => {
            if (child.userData.monsterId === targetMonsterId) {
              found = true
            }
          })
          if (found) {
            monsterObj = group
            break
          }
        }
      }

      if (monsterObj) {
        const monsterPos = monsterObj.position
        const currentPos = new THREE.Vector3(
          currentPlayer.position.x,
          0,
          currentPlayer.position.z
        )
        const targetVector = new THREE.Vector3(monsterPos.x, 0, monsterPos.z)
        const dist = currentPos.distanceTo(targetVector)

        if (isMoving) {
          // PHASE 1: CHASING
          if (dist < 2.0) {
            // Reached attack range - Transition to Combat
            isMoving = false
            movementTarget = null
            movementState = null
            currentSpeed = 0
            updatePlayerState()
            handleAttack(targetMonsterId)
            return
          } else {
            // Update target position to tracking point (throttled)
            const now = Date.now()
            if (now - lastChaseUpdate >= 1000) {
              lastChaseUpdate = now
              const newTarget = { x: monsterPos.x, y: 0, z: monsterPos.z }
              if (
                !movementTarget ||
                Math.abs(movementTarget.x - newTarget.x) > 0.1 ||
                Math.abs(movementTarget.z - newTarget.z) > 0.1
              ) {
                movementTarget = newTarget
                if (movementState) {
                  movementState.targetPos = { ...movementTarget }
                  const dx = movementTarget.x - currentPlayer.position.x
                  const dy = movementTarget.y - currentPlayer.position.y
                  const dz = movementTarget.z - currentPlayer.position.z
                  movementState.totalDistance = Math.sqrt(
                    dx * dx + dy * dy + dz * dz
                  )
                  movementState.startPos = { ...currentPos }
                } else {
                  movementState = initMovementState(
                    currentPos,
                    movementTarget,
                    currentSpeed
                  )
                }
                sendPlayerMove(movementTarget, playerRotation)
              }
            }
          }
        } else {
          // PHASE 2: COMBAT (In Range)
          if (dist > 2.5 && !isFinishingAttack) {
            // Range too far, stop attacking
            targetMonsterId = null
            attackCounter = 0
            if (playerState.state === 'attack') {
              const idleState = {
                ...playerState,
                state: 'idle',
                attackCounter: 0,
              } as PlayerState
              playerState = idleState
              onStateChange(idleState)
            }
          } else {
            // Still in range, rotate to face monster
            const dx = monsterPos.x - currentPlayer.position.x
            const dz = monsterPos.z - currentPlayer.position.z
            playerRotation = Math.atan2(dx, dz)

            attackTimer += deltaTime

            // Only start new attack cycles if monster is not already dying
            if (monsterData && !monsterData.state && !monsterData.isDeadPending) {
              // This block is for monsterData that might still be alive
            }

            // Corrected logic: Always increment timer, but only restart if alive
            const isMonsterAlive =
              monsterData &&
              monsterData.state !== 'dead' &&
              !monsterData.isDeadPending

            if (attackTimer >= cooldownMs) {
              if (isMonsterAlive) {
                // Restart cycle
                attackTimer = 0
                attackCounter++
                networkManager.sendPlayerAttack(targetMonsterId)
                updatePlayerState()
              } else {
                // Monster is dead/dying and animation finished -> Idle
                targetMonsterId = null
                attackCounter = 0
                attackTimer = 0
                const idleState = {
                  ...playerState,
                  state: 'idle',
                  attackCounter: 0,
                } as PlayerState
                playerState = idleState
                onStateChange(idleState)
                return
              }
            }

            if (playerState.state !== 'attack') {
              const attackState = {
                ...playerState,
                state: 'attack',
                rotation: playerRotation,
              } as PlayerState
              playerState = attackState
              onStateChange(attackState)
            }
            return // No movement processing while in combat
          }
        }
      } else {
        // Target lost
        targetMonsterId = null
        attackCounter = 0
        if (isMoving) {
          isMoving = false
          movementTarget = null
          movementState = null
          updatePlayerState()
        } else if (playerState.state === 'attack') {
          const idleState = {
            ...playerState,
            state: 'idle',
            attackCounter: 0,
          } as PlayerState
          playerState = idleState
          onStateChange(idleState)
        }
        return
      }
    }

    if (!isMoving || !movementTarget || !currentPlayer || !movementState) {
      // Reset speed when not moving
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
      // Movement complete
      gameStore.update((state) => {
        if (state.currentPlayer && movementTarget) {
          state.currentPlayer.position.set(
            movementTarget.x,
            movementTarget.y,
            movementTarget.z
          )
        }
        return state
      })

      // Send final position to server
      sendPlayerMove(movementTarget, playerRotation)

      isMoving = false
      movementTarget = null
      movementState = null
      currentSpeed = 0
      updatePlayerState()

      // If we were chasing a target (and for some reason arrived without triggering distance check above), attack it now
      if (targetMonsterId) {
        handleAttack(targetMonsterId)
      }
    } else {
      // Continue movement
      gameStore.update((state) => {
        if (state.currentPlayer) {
          state.currentPlayer.position.set(
            result.newPos.x,
            result.newPos.y,
            result.newPos.z
          )
        }
        return state
      })
      updatePlayerState(movementState.totalDistance)
    }
  }

  // Keyboard movement system
  export function updateKeyboardMovement() {
    if (!currentPlayer || keysPressed.size === 0) {
      return
    }

    // Cancel click-to-move if keyboard input detected
    if (keysPressed.size > 0 && movementTarget) {
      movementTarget = null
      movementState = null
      targetMonsterId = null // Cancel chase/combat
    }

    if (keysPressed.size > 0 && targetMonsterId) {
      targetMonsterId = null
      attackCounter = 0
    }

    // Calculate movement direction based on pressed keys
    let moveX = 0
    let moveZ = 0

    if (keysPressed.has('KeyW') || keysPressed.has('ArrowUp')) moveZ -= 1
    if (keysPressed.has('KeyS') || keysPressed.has('ArrowDown')) moveZ += 1
    if (keysPressed.has('KeyA') || keysPressed.has('ArrowLeft')) moveX -= 1
    if (keysPressed.has('KeyD') || keysPressed.has('ArrowRight')) moveX += 1

    // Normalize diagonal movement
    if (moveX !== 0 && moveZ !== 0) {
      moveX *= 0.707 // 1/sqrt(2)
      moveZ *= 0.707
    }

    // Apply keyboard movement if any keys are pressed
    if (moveX !== 0 || moveZ !== 0) {
      // Use fixed speed for keyboard movement (instant response)
      currentSpeed = MOVEMENT_CONFIG.maxSpeed
      const speed = MOVEMENT_CONFIG.maxSpeed * (1000 / 120 / 1000) // Adjust for frame rate (120 FPS target)
      const newX = currentPlayer.position.x + moveX * speed
      const newZ = currentPlayer.position.z + moveZ * speed

      // Calculate rotation based on movement direction
      playerRotation = Math.atan2(moveX, moveZ)

      gameStore.update((state) => {
        if (state.currentPlayer) {
          state.currentPlayer.position.set(
            newX,
            0, // Keep player on ground level
            newZ
          )
          isMoving = true
        }
        return state
      })

      // Send position to server periodically
      sendPlayerMove(
        {
          x: newX,
          y: 0, // Keep player on ground level
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
    if (!currentPlayer || isMoving || keysPressed.size > 0) {
      // Allow overriding current movement with new click
      if (currentPlayer && isMoving && !keysPressed.size) {
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

  // Handle attack logic
  function handleAttack(monsterId: string) {
    const monsterData = monsterManager.monsters.get(monsterId)
    if (monsterData?.state === 'dead' || monsterData?.isDeadPending) return

    console.log('Attacking monster:', monsterId)

    // Ensure position sync: send final move packet if current position differs from last sent
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

    // 1. Set local player state to attack
    const newPlayerState = {
      ...playerState,
      state: 'attack',
    } as PlayerState

    // Force immediate update
    playerState = newPlayerState
    onStateChange(newPlayerState)

    // 2. Send attack packet
    networkManager.sendPlayerAttack(monsterId)

    // 3. Set attacking target for persistent attack
    targetMonsterId = monsterId
    attackCounter = 1 // Start at 1
    attackTimer = 0
  }

  // Handle canvas click events
  function handleCanvasClick(event: MouseEvent) {
    if (!currentPlayer) return

    // Calculate mouse position in normalized device coordinates (-1 to +1)
    const rect = (event.target as HTMLCanvasElement).getBoundingClientRect()
    const mouse = new Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    )

    // Create raycaster
    const raycaster = new Raycaster()
    raycaster.setFromCamera(mouse, camera)

    // 1. Check intersection with monsters first
    if (monsterMeshes.length > 0) {
      const monsterIntersects = raycaster.intersectObjects(monsterMeshes, true)
      if (monsterIntersects.length > 0) {
        // Find the root object that has the monsterId
        let object: THREE.Object3D | null = monsterIntersects[0].object
        let monsterId: string | undefined

        while (object) {
          if (object.userData && object.userData.monsterId) {
            monsterId = object.userData.monsterId
            break
          }
          object = object.parent
        }

        if (monsterId) {
          // Check if monster is dead
          const monsterData = monsterManager.monsters.get(monsterId)
          if (monsterData?.state === 'dead') {
            console.log('Cannot attack a dead monster')
            return
          }

          // Calculate distance to monster
          // If we traveled up the hierarchy, using parent position might be safer if we have ref to it
          // But raycast point is exact hit point. Let's use intersection point for distance?
          // The user requirement says "get close (<2m)".
          // We should probably use the monster's root position, but we only have meshes here.
          // Getting position from intersection point is easiest
          const hitPoint = monsterIntersects[0].point

          const dist = new THREE.Vector3(
            currentPlayer.position.x,
            0,
            currentPlayer.position.z
          ).distanceTo(new THREE.Vector3(hitPoint.x, 0, hitPoint.z))

          if (dist < 2.0) {
            handleAttack(monsterId)
            // Stop any movement
            isMoving = false
            movementTarget = null
            targetMonsterId = monsterId
          } else {
            // Chase logic
            targetMonsterId = monsterId
            lastChaseUpdate = Date.now()

            // Target the monster directly as per user request
            handleClickToMove({
              x: hitPoint.x,
              y: 0,
              z: hitPoint.z,
            })
          }
          return // Stop checks
        }
      }
    }

    // 2. Check intersection with ground
    const intersects = raycaster.intersectObject(groundMesh)

    if (intersects.length > 0) {
      const point = intersects[0].point
      const clickPosition: Position = {
        x: point.x,
        y: 0, // Position player on ground level
        z: point.z,
      }

      // Normal click-to-move, clear attack target
      targetMonsterId = null // Clear persistent attack/chase
      attackCounter = 0
      handleClickToMove(clickPosition)
    }
  }

  // Keyboard event handlers
  function handleKeyDown(event: KeyboardEvent) {
    // Ignore keyboard input when typing in input fields
    const target = event.target as HTMLElement
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
      return
    }

    // Ignore movement keys when Ctrl is pressed (e.g. for Ctrl+D toggle)
    if (event.ctrlKey) return

    keysPressed.add(event.code)
    event.preventDefault()
  }

  function handleKeyUp(event: KeyboardEvent) {
    // Always remove from tracked keys on keyup, to prevent stuck keys
    // especially when focus changes (e.g. Enter to open chat)
    if (keysPressed.has(event.code)) {
      keysPressed.delete(event.code)
    }

    // Ignore keyboard input when typing in input fields
    const target = event.target as HTMLElement
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
      return
    }
    event.preventDefault()
  }

  onMount(() => {
    // Add keyboard event listeners
    document.addEventListener('keydown', handleKeyDown)
    document.addEventListener('keyup', handleKeyUp)

    // Add click event listener to canvas - wait until canvas exists
    let canvas: HTMLCanvasElement | null = null
    const findCanvas = () => {
      canvas = document.querySelector('canvas')
      if (canvas) {
        canvas.addEventListener('mousedown', handleCanvasClick)
      } else {
        setTimeout(findCanvas, 100)
      }
    }
    findCanvas()

    return () => {
      document.removeEventListener('keydown', handleKeyDown)
      document.removeEventListener('keyup', handleKeyUp)
      if (canvas) {
        canvas.removeEventListener('mousedown', handleCanvasClick)
      }
    }
  })
</script>

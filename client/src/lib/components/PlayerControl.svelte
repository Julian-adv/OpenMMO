<script lang="ts">
  import { onMount } from 'svelte'
  import { Vector2, Raycaster } from 'three'
  import * as THREE from 'three'
  import { gameStore, type Player } from '../stores/gameStore'
  import { networkManager } from '../network/socket'
  import {
    calculateMovementStep,
    initMovementState,
    DEFAULT_MOVEMENT_CONFIG,
    type Position,
    type MovementState,
    type MovementConfig,
  } from '../utils/movementUtils'

  export interface PlayerState {
    state: 'idle' | 'moving'
    speed: number
    direction: number
    position: { x: number; y: number; z: number }
  }

  interface Props {
    onStateChange: (state: PlayerState) => void
    camera?: THREE.PerspectiveCamera
    groundMesh?: THREE.Mesh
  }

  let { onStateChange, camera, groundMesh }: Props = $props()

  let currentPlayer = $state<Player | null>(null)
  let keysPressed = $state(new Set<string>())

  // Movement system
  let movementTarget = $state<Position | null>(null)
  let isMoving = $state(false)
  let movementState = $state<MovementState | null>(null)

  // Use the same movement config as remote players
  const MOVEMENT_CONFIG: MovementConfig = {
    ...DEFAULT_MOVEMENT_CONFIG,
  }

  // Character rotation and current speed
  let playerRotation = $state(0)
  let currentSpeed = $state(0)

  // Current player state
  let playerState = $state<PlayerState>({
    state: 'idle',
    speed: 0,
    direction: 0,
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
  function updatePlayerState() {
    const currentPosition = currentPlayer
      ? {
          x: currentPlayer.position.x,
          y: currentPlayer.position.y,
          z: currentPlayer.position.z,
        }
      : playerState.position

    const newState: PlayerState = {
      state: isMoving ? 'moving' : 'idle',
      speed: currentSpeed,
      direction: playerRotation,
      position: currentPosition,
    }

    // Only update if state actually changed
    if (
      newState.state !== playerState.state ||
      Math.abs(newState.speed - playerState.speed) > 0.01 ||
      newState.direction !== playerState.direction ||
      Math.abs(newState.position.x - playerState.position.x) > 0.01 ||
      Math.abs(newState.position.z - playerState.position.z) > 0.01
    ) {
      playerState = newState
      onStateChange(newState)
    }
  }

  // Update player movement (click-to-move) with acceleration/deceleration
  export function updatePlayerMovement(deltaTime: number) {
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
      networkManager.sendPlayerMove(movementTarget, playerRotation)

      isMoving = false
      movementTarget = null
      movementState = null
      currentSpeed = 0
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
    }
    updatePlayerState()
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
      networkManager.sendPlayerMove(
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

    updatePlayerState()
  }

  // Handle click-to-move
  export function handleClickToMove(clickPosition: Position) {
    if (!currentPlayer || isMoving || keysPressed.size > 0) return

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
    networkManager.sendPlayerMove(clickPosition, playerRotation)

    updatePlayerState()
  }

  // Handle canvas click events
  function handleCanvasClick(event: MouseEvent) {
    if (!camera || !groundMesh || !currentPlayer) return

    // Calculate mouse position in normalized device coordinates (-1 to +1)
    const rect = (event.target as HTMLCanvasElement).getBoundingClientRect()
    const mouse = new Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    )

    // Create raycaster
    const raycaster = new Raycaster()
    raycaster.setFromCamera(mouse, camera)

    // Check intersection with ground
    const intersects = raycaster.intersectObject(groundMesh)

    if (intersects.length > 0) {
      const point = intersects[0].point
      const clickPosition: Position = {
        x: point.x,
        y: 0, // Position player on ground level
        z: point.z,
      }

      // Use existing click-to-move logic
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
    keysPressed.add(event.code)
    event.preventDefault()
  }

  function handleKeyUp(event: KeyboardEvent) {
    // Ignore keyboard input when typing in input fields
    const target = event.target as HTMLElement
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
      return
    }
    keysPressed.delete(event.code)
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

import { Vector2, Raycaster } from 'three'
import * as THREE from 'three'
import type { Position } from '../utils/movementUtils'

export type ClickIntent =
  | {
      type: 'attack_monster'
      monsterId: string
      hitPoint: Position
      distance: number
    }
  | { type: 'move_to_ground'; position: Position }
  | { type: 'none' }

export interface RaycastContext {
  camera: THREE.PerspectiveCamera
  monsterMeshes: THREE.Group[]
  groundMesh: THREE.Mesh
  playerPosition: Position
  isMonsterDead: (monsterId: string) => boolean
}

class InputHandler {
  private keysPressed = new Set<string>()

  get hasKeysPressed(): boolean {
    return this.keysPressed.size > 0
  }

  getMovementDirection(): { x: number; z: number } | null {
    let moveX = 0
    let moveZ = 0

    if (this.keysPressed.has('KeyW') || this.keysPressed.has('ArrowUp'))
      moveZ -= 1
    if (this.keysPressed.has('KeyS') || this.keysPressed.has('ArrowDown'))
      moveZ += 1
    if (this.keysPressed.has('KeyA') || this.keysPressed.has('ArrowLeft'))
      moveX -= 1
    if (this.keysPressed.has('KeyD') || this.keysPressed.has('ArrowRight'))
      moveX += 1

    if (moveX === 0 && moveZ === 0) return null

    // Normalize diagonal movement
    if (moveX !== 0 && moveZ !== 0) {
      moveX *= 0.707 // 1/sqrt(2)
      moveZ *= 0.707
    }

    return { x: moveX, z: moveZ }
  }

  processCanvasClick(event: MouseEvent, context: RaycastContext): ClickIntent {
    const rect = (event.target as HTMLCanvasElement).getBoundingClientRect()

    // Define 5 points to raycast: center, up, right, down, left (10px offsets)
    const offsets = [
      { dx: 0, dy: 0 }, // Center
      { dx: 0, dy: -10 }, // Up (Screen coordinates: -y is up)
      { dx: 10, dy: 0 }, // Right
      { dx: 0, dy: 10 }, // Down
      { dx: -10, dy: 0 }, // Left
    ]

    const raycaster = new Raycaster()

    // Check intersection with monsters using 5 rays
    if (context.monsterMeshes.length > 0) {
      for (const offset of offsets) {
        const mouseNDC = new Vector2(
          ((event.clientX - rect.left + offset.dx) / rect.width) * 2 - 1,
          -((event.clientY - rect.top + offset.dy) / rect.height) * 2 + 1
        )

        raycaster.setFromCamera(mouseNDC, context.camera)
        const monsterIntersects = raycaster.intersectObjects(
          context.monsterMeshes,
          true
        )

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
            if (context.isMonsterDead(monsterId)) {
              continue // Try other rays
            }

            const hitPoint = monsterIntersects[0].point
            const dist = new THREE.Vector3(
              context.playerPosition.x,
              0,
              context.playerPosition.z
            ).distanceTo(new THREE.Vector3(hitPoint.x, 0, hitPoint.z))

            return {
              type: 'attack_monster',
              monsterId,
              hitPoint: { x: hitPoint.x, y: 0, z: hitPoint.z },
              distance: dist,
            }
          }
        }
      }
    }

    // Check intersection with ground (only use the center ray)
    const centerNDC = new Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    )
    raycaster.setFromCamera(centerNDC, context.camera)
    const intersects = raycaster.intersectObject(context.groundMesh)

    if (intersects.length > 0) {
      const point = intersects[0].point
      return {
        type: 'move_to_ground',
        position: { x: point.x, y: 0, z: point.z },
      }
    }

    return { type: 'none' }
  }

  handleKeyDown(event: KeyboardEvent): boolean {
    const target = event.target as HTMLElement
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
      return false
    }
    if (event.ctrlKey) return false

    this.keysPressed.add(event.code)
    return true
  }

  handleKeyUp(event: KeyboardEvent): boolean {
    // Always remove from tracked keys on keyup, to prevent stuck keys
    // especially when focus changes (e.g. Enter to open chat)
    if (this.keysPressed.has(event.code)) {
      this.keysPressed.delete(event.code)
    }

    const target = event.target as HTMLElement
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
      return false
    }
    return true
  }

  setupEventListeners(onCanvasClick: (event: MouseEvent) => void): () => void {
    const onKeyDown = (event: KeyboardEvent) => {
      if (this.handleKeyDown(event)) {
        event.preventDefault()
      }
    }
    const onKeyUp = (event: KeyboardEvent) => {
      if (this.handleKeyUp(event)) {
        event.preventDefault()
      }
    }

    document.addEventListener('keydown', onKeyDown)
    document.addEventListener('keyup', onKeyUp)

    // Add click event listener to canvas - wait until canvas exists
    let canvas: HTMLCanvasElement | null = null
    const findCanvas = () => {
      canvas = document.querySelector('canvas')
      if (canvas) {
        canvas.addEventListener('mousedown', onCanvasClick)
      } else {
        setTimeout(findCanvas, 100)
      }
    }
    findCanvas()

    return () => {
      document.removeEventListener('keydown', onKeyDown)
      document.removeEventListener('keyup', onKeyUp)
      if (canvas) {
        canvas.removeEventListener('mousedown', onCanvasClick)
      }
    }
  }
}

export const inputHandler = new InputHandler()

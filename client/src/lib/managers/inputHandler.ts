import { Vector2, Raycaster } from 'three'
import * as THREE from 'three'
import type { Position } from '../utils/movementUtils'
import type { WallDirection } from '../utils/house-geometry'

const MAX_DOOR_INTERACT_DISTANCE = 2.0
const MAX_FURNITURE_INTERACT_DISTANCE = 3.0

export type ClickIntent =
  | {
      type: 'attack_monster'
      monsterId: string
      hitPoint: Position
      distance: number
    }
  | {
      type: 'toggle_door'
      houseId: string
      roomIndex: number
      wallDir: WallDirection
      segmentIndex: number
    }
  | {
      type: 'interact_furniture'
      furnitureId: number
      furnitureType: string
      interaction: string
      position: Position
      rotation: number
      interactOffset?: Position
    }
  | {
      type: 'pickup_ground_item'
      instanceId: number
      position: Position
      distance: number
    }
  | { type: 'move_to_ground'; position: Position }
  | { type: 'none' }

export interface RaycastContext {
  camera: THREE.Camera
  monsterMeshes: THREE.Group[]
  doorMeshes: THREE.Object3D[]
  furnitureMeshes: THREE.Object3D[]
  groundItemMeshes: THREE.Object3D[]
  groundMeshes: THREE.Object3D[]
  playerPosition: Position
  playerFloorLevel: number
  isMonsterDead: (monsterId: string) => boolean
}

class InputHandler {
  private keysPressed = new Set<string>()
  private _interactJustPressed = false

  get hasKeysPressed(): boolean {
    return this.keysPressed.size > 0
  }

  /** Returns true once per E key press, then resets. */
  consumeInteract(): boolean {
    if (this._interactJustPressed) {
      this._interactJustPressed = false
      return true
    }
    return false
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
              hitPoint: { x: hitPoint.x, y: hitPoint.y, z: hitPoint.z },
              distance: dist,
            }
          }
        }
      }
    }

    const centerNDC = new Vector2(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    )
    raycaster.setFromCamera(centerNDC, context.camera)

    // Check intersection with door meshes
    if (context.doorMeshes?.length > 0) {
      const doorHits = raycaster.intersectObjects(context.doorMeshes, true)
      if (doorHits.length > 0) {
        const hitPoint = doorHits[0].point
        const pp = context.playerPosition
        const dx = hitPoint.x - pp.x
        const dz = hitPoint.z - pp.z
        if (
          dx * dx + dz * dz <=
          MAX_DOOR_INTERACT_DISTANCE * MAX_DOOR_INTERACT_DISTANCE
        ) {
          let obj: THREE.Object3D | null = doorHits[0].object
          while (obj) {
            const d = obj.userData
            if (
              d &&
              d.doorHouseId &&
              (context.playerFloorLevel === -1 ||
                d.doorFloorLevel === context.playerFloorLevel)
            ) {
              return {
                type: 'toggle_door',
                houseId: d.doorHouseId,
                roomIndex: d.doorRoomIndex,
                wallDir: d.doorWallDir,
                segmentIndex: d.doorSegmentIndex,
              }
            }
            obj = obj.parent
          }
        }
      }
    }

    // Check intersection with furniture meshes
    if (context.furnitureMeshes.length > 0) {
      const furnitureHits = raycaster.intersectObjects(
        context.furnitureMeshes,
        true
      )
      if (furnitureHits.length > 0) {
        const hitPoint = furnitureHits[0].point
        const pp = context.playerPosition
        const dx = hitPoint.x - pp.x
        const dz = hitPoint.z - pp.z
        if (
          dx * dx + dz * dz <=
          MAX_FURNITURE_INTERACT_DISTANCE * MAX_FURNITURE_INTERACT_DISTANCE
        ) {
          let obj: THREE.Object3D | null = furnitureHits[0].object
          while (obj) {
            const d = obj.userData
            if (
              d &&
              d.furnitureId != null &&
              d.furnitureType &&
              d.furnitureInteraction
            ) {
              return {
                type: 'interact_furniture',
                furnitureId: d.furnitureId as number,
                furnitureType: d.furnitureType,
                interaction: d.furnitureInteraction,
                position: {
                  x: obj.position.x,
                  y: obj.position.y,
                  z: obj.position.z,
                },
                rotation: obj.rotation.y,
                interactOffset: d.furnitureInteractOffset,
              }
            }
            obj = obj.parent
          }
        }
      }
    }

    // Check intersection with ground items
    if (context.groundItemMeshes.length > 0) {
      const itemHits = raycaster.intersectObjects(
        context.groundItemMeshes,
        true
      )
      if (itemHits.length > 0) {
        const hitPoint = itemHits[0].point
        const pp = context.playerPosition
        const dx = hitPoint.x - pp.x
        const dz = hitPoint.z - pp.z
        const distSq = dx * dx + dz * dz
        let obj: THREE.Object3D | null = itemHits[0].object
        while (obj) {
          if (obj.userData && obj.userData.groundItemId != null) {
            return {
              type: 'pickup_ground_item',
              instanceId: obj.userData.groundItemId as number,
              position: { x: hitPoint.x, y: hitPoint.y, z: hitPoint.z },
              distance: Math.sqrt(distSq),
            }
          }
          obj = obj.parent
        }
      }
    }

    // Check intersection with ground meshes
    if (context.groundMeshes.length === 0) {
      return { type: 'none' }
    }
    const intersects = raycaster.intersectObjects(context.groundMeshes, true)

    // Pick the first hit with an upward-facing normal (floor/terrain, not walls)
    for (const hit of intersects) {
      if (hit.face && hit.face.normal.y > 0.5) {
        return {
          type: 'move_to_ground',
          position: { x: hit.point.x, y: hit.point.y, z: hit.point.z },
        }
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

    if (event.code === 'KeyE' && !event.repeat) {
      this._interactJustPressed = true
    }
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
    const onContextMenu = (event: MouseEvent) => event.preventDefault()
    const findCanvas = () => {
      canvas = document.querySelector('canvas')
      if (canvas) {
        canvas.addEventListener('mousedown', onCanvasClick)
        canvas.addEventListener('contextmenu', onContextMenu)
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
        canvas.removeEventListener('contextmenu', onContextMenu)
      }
    }
  }
}

export const inputHandler = new InputHandler()

import { Vector2, Raycaster } from 'three'
import * as THREE from 'three'
import type { Position } from '../utils/movementUtils'
import type { WallDirection } from '../utils/house-geometry'

const MAX_DOOR_INTERACT_DISTANCE = 2.0
const MAX_OBJECT_INTERACT_DISTANCE = 3.0

function hasAncestorBridge(obj: THREE.Object3D | null): boolean {
  for (let o = obj; o; o = o.parent) {
    if (o.userData?.objectKind === 'bridge') return true
  }
  return false
}

/** Walk up the parent chain to the first object carrying `key` in userData. */
function findAncestorWithUserData(
  obj: THREE.Object3D | null,
  key: string
): THREE.Object3D | null {
  for (let o = obj; o; o = o.parent) {
    if (o.userData?.[key] != null) return o
  }
  return null
}

/** Entity clicks get a little slack: the click point plus 4 nearby offsets
 *  (10px up/right/down/left) are each raycast until one resolves. */
const CLICK_RAY_OFFSETS = [
  { dx: 0, dy: 0 },
  { dx: 0, dy: -10 }, // Screen coordinates: -y is up
  { dx: 10, dy: 0 },
  { dx: 0, dy: 10 },
  { dx: -10, dy: 0 },
]

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
  | { type: 'toggle_dungeon_door' }
  | {
      type: 'interact_object'
      objectId: number
      objectType: string
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
  | {
      type: 'interact_npc'
      playerId: string
      position: Position
      distance: number
    }
  | { type: 'move_to_ground'; position: Position }
  | { type: 'none' }

export interface RaycastContext {
  camera: THREE.Camera
  monsterMeshes: THREE.Group[]
  npcMeshes: THREE.Object3D[]
  doorMeshes: THREE.Object3D[]
  objectMeshes: THREE.Object3D[]
  groundItemMeshes: THREE.Object3D[]
  groundMeshes: THREE.Object3D[]
  playerPosition: Position
  playerFloorLevel: number
  isMonsterDead: (monsterId: string) => boolean
}

/** Result of hovering a placed object that carries display text (e.g. signpost). */
export interface HoverText {
  position: Position
  text: string
}

class InputHandler {
  private keysPressed = new Set<string>()
  private _interactJustPressed = false
  /** Dedicated raycaster reused across pointermove hover queries. */
  private _hoverRaycaster = new Raycaster()
  private readonly _hoverNDC = new Vector2()
  private readonly _hoverWorldPos = new THREE.Vector3()

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

  /** Cast the click ray plus the 4 offset rays against `meshes`, returning the
   *  first non-null result of `resolve` over each ray's closest intersection. */
  private raycastWithOffsets<T>(
    event: MouseEvent,
    rect: DOMRect,
    camera: THREE.Camera,
    meshes: THREE.Object3D[],
    resolve: (hit: THREE.Intersection) => T | null
  ): T | null {
    const raycaster = new Raycaster()
    for (const offset of CLICK_RAY_OFFSETS) {
      const mouseNDC = new Vector2(
        ((event.clientX - rect.left + offset.dx) / rect.width) * 2 - 1,
        -((event.clientY - rect.top + offset.dy) / rect.height) * 2 + 1
      )

      raycaster.setFromCamera(mouseNDC, camera)
      const hits = raycaster.intersectObjects(meshes, true)
      if (hits.length === 0) continue

      const result = resolve(hits[0])
      if (result !== null) return result
    }
    return null
  }

  processCanvasClick(event: MouseEvent, context: RaycastContext): ClickIntent {
    const rect = (event.target as HTMLCanvasElement).getBoundingClientRect()

    // Check intersection with monsters
    if (context.monsterMeshes.length > 0) {
      const monsterIntent = this.raycastWithOffsets<ClickIntent>(
        event,
        rect,
        context.camera,
        context.monsterMeshes,
        (hit) => {
          const owner = findAncestorWithUserData(hit.object, 'monsterId')
          if (!owner) return null
          const monsterId = owner.userData.monsterId as string
          if (context.isMonsterDead(monsterId)) return null // Try other rays

          const hitPoint = hit.point
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
      )
      if (monsterIntent) return monsterIntent
    }

    // Check intersection with NPC models
    if (context.npcMeshes.length > 0) {
      const npcIntent = this.raycastWithOffsets<ClickIntent>(
        event,
        rect,
        context.camera,
        context.npcMeshes,
        (hit) => {
          const owner = findAncestorWithUserData(hit.object, 'npcPlayerId')
          if (!owner) return null

          const npcPosition = new THREE.Vector3()
          owner.getWorldPosition(npcPosition)
          const dx = npcPosition.x - context.playerPosition.x
          const dz = npcPosition.z - context.playerPosition.z
          return {
            type: 'interact_npc',
            playerId: owner.userData.npcPlayerId as string,
            position: {
              x: npcPosition.x,
              y: npcPosition.y,
              z: npcPosition.z,
            },
            distance: Math.sqrt(dx * dx + dz * dz),
          }
        }
      )
      if (npcIntent) return npcIntent
    }

    const raycaster = new Raycaster()
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
            if (d && d.dungeonDoor) {
              return { type: 'toggle_dungeon_door' }
            }
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

    // Check intersection with object meshes
    if (context.objectMeshes.length > 0) {
      const objectHits = raycaster.intersectObjects(context.objectMeshes, true)
      if (objectHits.length > 0) {
        const hitPoint = objectHits[0].point
        const pp = context.playerPosition
        const dx = hitPoint.x - pp.x
        const dz = hitPoint.z - pp.z
        if (
          dx * dx + dz * dz <=
          MAX_OBJECT_INTERACT_DISTANCE * MAX_OBJECT_INTERACT_DISTANCE
        ) {
          let obj: THREE.Object3D | null = objectHits[0].object
          while (obj) {
            const d = obj.userData
            if (
              d &&
              d.objectId != null &&
              d.objectType &&
              d.objectInteraction
            ) {
              return {
                type: 'interact_object',
                objectId: d.objectId as number,
                objectType: d.objectType,
                interaction: d.objectInteraction,
                position: {
                  x: obj.position.x,
                  y: obj.position.y,
                  z: obj.position.z,
                },
                rotation: obj.rotation.y,
                interactOffset: d.objectInteractOffset,
              }
            }
            obj = obj.parent
          }
        }
        const face = objectHits[0].face
        if (
          face &&
          face.normal.y > 0.5 &&
          hasAncestorBridge(objectHits[0].object)
        ) {
          return {
            type: 'move_to_ground',
            position: { x: hitPoint.x, y: hitPoint.y, z: hitPoint.z },
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
        const pp = context.playerPosition
        let obj: THREE.Object3D | null = itemHits[0].object
        while (obj) {
          if (obj.userData && obj.userData.groundItemId != null) {
            const itemPosition = new THREE.Vector3()
            obj.getWorldPosition(itemPosition)
            const dx = itemPosition.x - pp.x
            const dz = itemPosition.z - pp.z
            const distSq = dx * dx + dz * dz
            return {
              type: 'pickup_ground_item',
              instanceId: obj.userData.groundItemId as number,
              position: {
                x: itemPosition.x,
                y: itemPosition.y,
                z: itemPosition.z,
              },
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

    if (intersects.length > 0) {
      const firstHit = intersects[0]
      return {
        type: 'move_to_ground',
        position: {
          x: firstHit.point.x,
          y: firstHit.point.y,
          z: firstHit.point.z,
        },
      }
    }

    return { type: 'none' }
  }

  /**
   * Raycast the pointer against the placed-object meshes only and return the
   * display text of the first object that carries one (set on userData.objectText).
   * Cheap enough to run on pointermove: it intersects just the object overlay
   * group, not the whole scene. Returns null when nothing texted is under the cursor.
   */
  processHover(
    event: MouseEvent,
    camera: THREE.Camera,
    objectMeshes: THREE.Object3D[]
  ): HoverText | null {
    if (objectMeshes.length === 0) return null
    const rect = (event.target as HTMLCanvasElement).getBoundingClientRect()
    this._hoverNDC.set(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1
    )
    this._hoverRaycaster.setFromCamera(this._hoverNDC, camera)
    const hits = this._hoverRaycaster.intersectObjects(objectMeshes, true)
    if (hits.length === 0) return null

    let obj: THREE.Object3D | null = hits[0].object
    while (obj) {
      const text = obj.userData?.objectText
      if (typeof text === 'string' && text.length > 0) {
        // World position (robust if the overlay group is ever transformed);
        // equals obj.position today since the group sits at the scene root.
        obj.getWorldPosition(this._hoverWorldPos)
        return {
          position: {
            x: this._hoverWorldPos.x,
            y: this._hoverWorldPos.y,
            z: this._hoverWorldPos.z,
          },
          text,
        }
      }
      obj = obj.parent
    }
    return null
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

  setupEventListeners(
    canvas: HTMLCanvasElement,
    onCanvasClick: (event: MouseEvent) => void
  ): () => void {
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

    // OS shortcuts (e.g. Win+Shift+S) can swallow keyup of held modifiers,
    // leaving keys "stuck" and blocking click-to-move via hasKeysPressed.
    const onWindowBlur = () => this.keysPressed.clear()

    document.addEventListener('keydown', onKeyDown)
    document.addEventListener('keyup', onKeyUp)
    window.addEventListener('blur', onWindowBlur)

    const onContextMenu = (event: MouseEvent) => event.preventDefault()
    canvas.addEventListener('mousedown', onCanvasClick)
    canvas.addEventListener('contextmenu', onContextMenu)

    return () => {
      document.removeEventListener('keydown', onKeyDown)
      document.removeEventListener('keyup', onKeyUp)
      window.removeEventListener('blur', onWindowBlur)
      canvas.removeEventListener('mousedown', onCanvasClick)
      canvas.removeEventListener('contextmenu', onContextMenu)
    }
  }
}

export const inputHandler = new InputHandler()

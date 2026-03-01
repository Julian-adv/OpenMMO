import * as THREE from 'three'

export interface Vector3Like {
  x: number
  y: number
  z: number
}

export interface CameraOffset {
  x: number
  y: number
  z: number
}

export interface ViewportSize {
  width: number
  height: number
}

export const INITIAL_DISTANCE = 50
export const INITIAL_PITCH = Math.atan(1 / Math.sqrt(2))
export const INITIAL_YAW = -Math.PI / 4
export const ORTHOGRAPHIC_FRUSTUM_HEIGHT = 20
export const ORTHOGRAPHIC_FRUSTUM_VERTICAL_OFFSET = 0
export const ORTHOGRAPHIC_DEFAULT_ZOOM = 1

const ORTHOGRAPHIC_NEAR = 0.1
const ORTHOGRAPHIC_FAR = 500

const CAMERA_OFFSET_HORIZONTAL_DISTANCE =
  INITIAL_DISTANCE * Math.cos(INITIAL_PITCH)

export const DEFAULT_CAMERA_OFFSET: CameraOffset = {
  x: CAMERA_OFFSET_HORIZONTAL_DISTANCE * Math.sin(INITIAL_YAW),
  y: INITIAL_DISTANCE * Math.sin(INITIAL_PITCH),
  z: CAMERA_OFFSET_HORIZONTAL_DISTANCE * Math.cos(INITIAL_YAW),
}

export function resetCameraRotation(
  camera: THREE.OrthographicCamera,
  playerPos: Vector3Like
): [number, number, number] {
  const dx = camera.position.x - playerPos.x
  const dy = camera.position.y - playerPos.y
  const dz = camera.position.z - playerPos.z
  const currentDistance = Math.sqrt(dx * dx + dy * dy + dz * dz)

  const currentHorizontalDistance = currentDistance * Math.cos(INITIAL_PITCH)
  camera.position.set(
    playerPos.x + currentHorizontalDistance * Math.sin(INITIAL_YAW),
    playerPos.y + currentDistance * Math.sin(INITIAL_PITCH),
    playerPos.z + currentHorizontalDistance * Math.cos(INITIAL_YAW)
  )
  camera.lookAt(playerPos.x, playerPos.y, playerPos.z)

  return [playerPos.x, playerPos.y, playerPos.z]
}

export function updateOrthographicFrustum(
  camera: THREE.OrthographicCamera | undefined,
  viewportSize: ViewportSize
) {
  if (!camera) return

  const aspect =
    Math.max(1, viewportSize.width) / Math.max(1, viewportSize.height)
  const halfHeight = ORTHOGRAPHIC_FRUSTUM_HEIGHT / 2
  const halfWidth = halfHeight * aspect

  camera.left = -halfWidth
  camera.right = halfWidth
  camera.top = halfHeight - ORTHOGRAPHIC_FRUSTUM_VERTICAL_OFFSET
  camera.bottom = -halfHeight - ORTHOGRAPHIC_FRUSTUM_VERTICAL_OFFSET
  camera.near = ORTHOGRAPHIC_NEAR
  camera.far = ORTHOGRAPHIC_FAR
  camera.updateProjectionMatrix()
}

export function calculateCameraOffset(
  camera: THREE.OrthographicCamera | undefined,
  playerPos: Vector3Like | null,
  defaultOffset: CameraOffset
): CameraOffset {
  if (!camera || !playerPos) {
    return {
      x: defaultOffset.x,
      y: defaultOffset.y,
      z: defaultOffset.z,
    }
  }

  return {
    x: camera.position.x - playerPos.x,
    y: camera.position.y - playerPos.y,
    z: camera.position.z - playerPos.z,
  }
}

export function updateCameraWithOffset(
  camera: THREE.OrthographicCamera,
  playerPos: Vector3Like,
  offset: CameraOffset
): [number, number, number] {
  camera.position.set(
    playerPos.x + offset.x,
    playerPos.y + offset.y,
    playerPos.z + offset.z
  )
  camera.lookAt(playerPos.x, playerPos.y, playerPos.z)

  return [playerPos.x, playerPos.y, playerPos.z]
}

export function resetCameraToInitialState(
  camera: THREE.OrthographicCamera,
  playerPos: Vector3Like,
  cameraOffset: CameraOffset
): [number, number, number] {
  camera.position.set(
    playerPos.x + cameraOffset.x,
    playerPos.y + cameraOffset.y,
    playerPos.z + cameraOffset.z
  )
  camera.lookAt(playerPos.x, playerPos.y, playerPos.z)

  camera.zoom = ORTHOGRAPHIC_DEFAULT_ZOOM
  camera.updateProjectionMatrix()

  return [playerPos.x, playerPos.y, playerPos.z]
}

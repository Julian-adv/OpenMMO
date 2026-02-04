<script lang="ts">
  import { T } from '@threlte/core'
  import { Text } from '@threlte/extras'
  import type { Vector3 } from 'three'
  import * as THREE from 'three'

  interface Props {
    position: Vector3
    cameraPosition: Vector3
    message: string
  }

  let { position, cameraPosition, message }: Props = $props()

  const HEIGHT_OFFSET = 3.2

  // Calculate rotation to face camera in world space
  function calculateBillboardRotation(): [number, number, number] {
    if (!cameraPosition) {
      return [0, 0, 0]
    }

    const worldX = position.x
    const worldY = position.y + HEIGHT_OFFSET
    const worldZ = position.z

    const dx = cameraPosition.x - worldX
    const dy = cameraPosition.y - worldY
    const dz = cameraPosition.z - worldZ

    const yaw = Math.atan2(dx, dz)
    const horizontalDistance = Math.sqrt(dx * dx + dz * dz)
    const pitch = -Math.atan2(dy, horizontalDistance)

    return [pitch, yaw, 0]
  }

  // Create rounded rectangle shape for chat bubble with tail
  function createRoundedRectShape(
    width: number,
    height: number,
    radius: number
  ): THREE.Shape {
    const shape = new THREE.Shape()
    const x = -width / 2
    const y = -height / 2

    shape.moveTo(x + radius, y)
    // Bottom edge with curved tail in the center
    shape.lineTo(-radius, y)
    shape.quadraticCurveTo(0, y, 0, y - radius)
    shape.quadraticCurveTo(0, y, radius, y)
    shape.lineTo(x + width - radius, y)
    // Right edge
    shape.quadraticCurveTo(x + width, y, x + width, y + radius)
    shape.lineTo(x + width, y + height - radius)
    // Top edge
    shape.quadraticCurveTo(
      x + width,
      y + height,
      x + width - radius,
      y + height
    )
    shape.lineTo(x + radius, y + height)
    // Left edge
    shape.quadraticCurveTo(x, y + height, x, y + height - radius)
    shape.lineTo(x, y + radius)
    shape.quadraticCurveTo(x, y, x + radius, y)

    return shape
  }

  // Create line geometry from shape for border
  function createBorderGeometry(shape: THREE.Shape): THREE.BufferGeometry {
    const points = shape.getPoints(32)
    const geometry = new THREE.BufferGeometry().setFromPoints(points)
    return geometry
  }

  const bubbleWidth = $derived(Math.min(message.length * 0.15 + 0.4, 4))
  const bubbleHeight = 0.5
  const cornerRadius = 0.1
  const bubbleShape = $derived(
    createRoundedRectShape(bubbleWidth, bubbleHeight, cornerRadius)
  )
  const displayText = $derived(
    message.length > 25 ? message.slice(0, 25) + '...' : message
  )
</script>

<!-- Chat bubble background -->
<T.Mesh
  position={[position.x, position.y + HEIGHT_OFFSET, position.z]}
  rotation={calculateBillboardRotation()}
>
  <T.ShapeGeometry args={[bubbleShape]} />
  <T.MeshBasicMaterial color="#000000" opacity={0.7} transparent={true} />
</T.Mesh>

<!-- Chat bubble border (white line) -->
<T.LineLoop
  position={[position.x, position.y + HEIGHT_OFFSET, position.z + 0.001]}
  rotation={calculateBillboardRotation()}
>
  <T is={createBorderGeometry(bubbleShape)} />
  <T.LineBasicMaterial color="#ffffff" />
</T.LineLoop>

<!-- Chat bubble text -->
<Text
  text={displayText}
  position={[position.x, position.y + HEIGHT_OFFSET, position.z + 0.01]}
  rotation={calculateBillboardRotation()}
  fontSize={0.25}
  color="#ffffff"
  anchorX="center"
  anchorY="middle"
  maxWidth={3.5}
/>

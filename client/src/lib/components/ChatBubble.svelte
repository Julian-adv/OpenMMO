<script lang="ts">
  import { T, useTask } from '@threlte/core'
  import { Text } from '@threlte/extras'
  import type { Vector3 } from 'three'
  import * as THREE from 'three'

  interface Props {
    position: Vector3
    camera: THREE.PerspectiveCamera | undefined
    message: string
  }

  let { position, camera, message }: Props = $props()

  const HEIGHT_OFFSET = 3.2
  const PADDING_X = 0.4
  const PADDING_Y = 0.2

  let textBounds = $state({ width: 1, height: 0.3 })
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let textRef = $state<any>(null)
  let bubbleGroup = $state<THREE.Group | undefined>(undefined)

  function handleTextSync() {
    if (textRef?.textRenderInfo?.blockBounds) {
      const [minX, minY, maxX, maxY] = textRef.textRenderInfo.blockBounds
      textBounds = {
        width: maxX - minX,
        height: maxY - minY,
      }
    }
  }

  // Calculate rotation to face camera in world space
  function calculateBillboardRotation(): [number, number, number] {
    if (!camera) {
      return [0, 0, 0]
    }

    const worldX = position.x
    // Camera is targeting the player's feet, so compute rotation from the feet.
    const rotationOriginY = position.y
    const worldZ = position.z

    const dx = camera.position.x - worldX
    const dy = camera.position.y - rotationOriginY
    const dz = camera.position.z - worldZ

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

  const bubbleWidth = $derived(Math.min(textBounds.width + PADDING_X, 4))
  const bubbleHeight = $derived(textBounds.height + PADDING_Y)
  const cornerRadius = 0.1
  const bubbleShape = $derived(
    createRoundedRectShape(bubbleWidth, bubbleHeight, cornerRadius)
  )
  const displayText = $derived(
    message.length > 100 ? message.slice(0, 100) + '...' : message
  )
</script>

<!-- Chat bubble background -->
<T.Group
  bind:ref={bubbleGroup}
  position={[position.x, position.y + HEIGHT_OFFSET, position.z]}
  rotation={calculateBillboardRotation()}
>
  <T.Mesh position={[0, 0, 0]}>
    <T.ShapeGeometry args={[bubbleShape]} />
    <T.MeshBasicMaterial color="#000000" opacity={0.7} transparent={true} />
  </T.Mesh>

  <!-- Chat bubble border (white line) -->
  <T.LineLoop position={[0, 0, 0.001]}>
    <T is={createBorderGeometry(bubbleShape)} />
    <T.LineBasicMaterial color="#ffffff" />
  </T.LineLoop>

  <!-- Chat bubble text -->
  <Text
    bind:ref={textRef}
    text={displayText}
    position={[0, 0, 0.01]}
    fontSize={0.25}
    color="#ffffff"
    anchorX="center"
    anchorY="middle"
    maxWidth={3.5}
    onsync={handleTextSync}
  />
</T.Group>

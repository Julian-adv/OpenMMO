<script lang="ts">
  import { T, useTask } from '@threlte/core'
  import { Text } from '@threlte/extras'
  import * as THREE from 'three'
  import type { AccountCharacter } from '../network/socket'

  interface Props {
    character: AccountCharacter | undefined
    selected: boolean
    positionX: number
    positionZ: number
    camera: THREE.Camera | undefined
    onclick: () => void
  }

  let { character, selected, positionX, positionZ, camera, onclick }: Props =
    $props()

  const PANEL_Y = -0.9
  const CORNER_RADIUS = 0.08

  let labelGroup = $state<THREE.Group | undefined>(undefined)

  // Panel dimensions depend on content
  const CHAR_PANEL_WIDTH = 1.4
  const CHAR_PANEL_HEIGHT = 0.95
  const EMPTY_PANEL_WIDTH = 0.9
  const EMPTY_PANEL_HEIGHT = 0.4

  const panelWidth = $derived(character ? CHAR_PANEL_WIDTH : EMPTY_PANEL_WIDTH)
  const panelHeight = $derived(
    character ? CHAR_PANEL_HEIGHT : EMPTY_PANEL_HEIGHT
  )

  const borderColor = $derived(selected ? '#7cc9ff' : '#53657b')
  const bgColor = $derived(selected ? '#223552' : '#141e2c')
  const bgOpacity = $derived(selected ? 0.75 : 0.5)

  function createRoundedRectShape(
    width: number,
    height: number,
    radius: number
  ): THREE.Shape {
    const shape = new THREE.Shape()
    const x = -width / 2
    const y = -height / 2

    shape.moveTo(x + radius, y)
    shape.lineTo(x + width - radius, y)
    shape.quadraticCurveTo(x + width, y, x + width, y + radius)
    shape.lineTo(x + width, y + height - radius)
    shape.quadraticCurveTo(
      x + width,
      y + height,
      x + width - radius,
      y + height
    )
    shape.lineTo(x + radius, y + height)
    shape.quadraticCurveTo(x, y + height, x, y + height - radius)
    shape.lineTo(x, y + radius)
    shape.quadraticCurveTo(x, y, x + radius, y)

    return shape
  }

  function createBorderGeometry(shape: THREE.Shape): THREE.BufferGeometry {
    const points = shape.getPoints(32)
    return new THREE.BufferGeometry().setFromPoints(points)
  }

  const panelShape = $derived(
    createRoundedRectShape(panelWidth, panelHeight, CORNER_RADIUS)
  )

  // Stat layout: 2 columns, 3 rows
  const STAT_FONT_SIZE = 0.09
  const STAT_COL_GAP = 0.35
  const STAT_VALUE_OFFSET = 0.22 // offset from label start to value start
  const STAT_ROW_GAP = 0.13
  const STATS_START_Y = $derived(-panelHeight / 2 + 0.15)

  useTask(() => {
    if (!labelGroup || !camera) return
    labelGroup.position.set(positionX, PANEL_Y, positionZ)
    labelGroup.quaternion.copy(camera.quaternion)
  })
</script>

<T.Group
  bind:ref={labelGroup}
>
  <!-- Background (click target) -->
  <T.Mesh renderOrder={0} {onclick}>
    <T.ShapeGeometry args={[panelShape]} />
    <T.MeshBasicMaterial
      color={bgColor}
      opacity={bgOpacity}
      transparent={true}
      depthWrite={false}
    />
  </T.Mesh>

  <!-- Border -->
  <T.LineLoop position={[0, 0, 0.001]} renderOrder={1}>
    <T is={createBorderGeometry(panelShape)} />
    <T.LineBasicMaterial color={borderColor} />
  </T.LineLoop>

  {#if character}
    <!-- Character name -->
    <Text
      text={character.name}
      position={[0, panelHeight / 2 - 0.12, 0.02]}
      fontSize={0.13}
      color="#f7fafc"
      anchorX="center"
      anchorY="middle"
      depthOffset={-1}
    />

    <!-- Level -->
    <Text
      text={`Lv. ${character.level}  HP ${character.max_hp}`}
      position={[0, panelHeight / 2 - 0.27, 0.02]}
      fontSize={0.1}
      color="#f0c040"
      anchorX="center"
      anchorY="middle"
      depthOffset={-1}
    />

    <!-- Stats: 2 columns x 3 rows -->
    {#each [
      { label: 'STR', value: character.attributes.str, col: 0, row: 0 },
      { label: 'DEX', value: character.attributes.dex, col: 1, row: 0 },
      { label: 'CON', value: character.attributes.con, col: 0, row: 1 },
      { label: 'INT', value: character.attributes.int, col: 1, row: 1 },
      { label: 'WIS', value: character.attributes.wis, col: 0, row: 2 },
      { label: 'CHA', value: character.attributes.cha, col: 1, row: 2 },
    ] as stat (stat.label)}
      {@const colX = stat.col === 0 ? -STAT_COL_GAP : 0.02}
      {@const rowY = STATS_START_Y + (2 - stat.row) * STAT_ROW_GAP}
      <!-- Stat label -->
      <Text
        text={stat.label}
        position={[colX, rowY, 0.02]}
        fontSize={STAT_FONT_SIZE}
        color="#a7b7ca"
        anchorX="left"
        anchorY="middle"
        depthOffset={-1}
      />
      <!-- Stat value -->
      <Text
        text={String(stat.value)}
        position={[colX + STAT_VALUE_OFFSET, rowY, 0.02]}
        fontSize={STAT_FONT_SIZE}
        color="#a7b7ca"
        anchorX="left"
        anchorY="middle"
        depthOffset={-1}
      />
    {/each}
  {:else}
    <!-- Empty slot -->
    <Text
      text="+ Create"
      position={[0, 0, 0.02]}
      fontSize={0.12}
      color="#9fb0c6"
      anchorX="center"
      anchorY="middle"
      depthOffset={-1}
    />
  {/if}
</T.Group>

<script lang="ts">
  import * as THREE from 'three'
  import { T } from '@threlte/core'
  import { onDestroy } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import {
    editorTool,
    editorHeightManager,
    selectedNpcSchedule,
    selectedScheduleIndex,
    draggingWaypointIndex,
  } from '../../stores/editorStore'
  import type { TerrainHeightManager } from '../../managers/terrainHeightManager'
  import type { NpcScheduleData } from '../../managers/npcScheduleManager'
  import type { Unsubscriber } from 'svelte/store'

  const HOME_COLOR = new THREE.Color(0xe2b93b)
  const WAYPOINT_COLOR = new THREE.Color(0x44ccff)
  const DRAG_COLOR = new THREE.Color(0xffffff)
  const LINE_COLOR = new THREE.Color(0x44ccff)
  const Y_OFFSET = 0.3
  const CIRCLE_RADIUS = 1.5
  const LABEL_Y_OFFSET = 0.5

  interface WaypointMarker {
    index: number // -1 = home, 0..n = waypoint
    x: number
    y: number
    z: number
    material: THREE.MeshBasicMaterial
    label: string
  }

  let tool = $state('')
  let heightMgr = $state<TerrainHeightManager | null>(null)
  let schedule = $state<NpcScheduleData | null>(null)
  let schedIdx = $state(0)
  let dragIdx = $state<number | null>(null)

  const unsubs: Unsubscriber[] = [
    editorTool.subscribe((v) => (tool = v)),
    editorHeightManager.subscribe((v) => (heightMgr = v)),
    selectedNpcSchedule.subscribe((v) => (schedule = v)),
    selectedScheduleIndex.subscribe((v) => (schedIdx = v)),
    draggingWaypointIndex.subscribe((v) => (dragIdx = v)),
  ]

  // Shared circle geometry and materials
  const circleGeo = new THREE.CircleGeometry(CIRCLE_RADIUS, 32)
  const homeMat = new THREE.MeshBasicMaterial({ color: HOME_COLOR, transparent: true, opacity: 0.7, side: THREE.DoubleSide, depthWrite: false })
  const waypointMat = new THREE.MeshBasicMaterial({ color: WAYPOINT_COLOR, transparent: true, opacity: 0.7, side: THREE.DoubleSide, depthWrite: false })
  const dragMat = new THREE.MeshBasicMaterial({ color: DRAG_COLOR, transparent: true, opacity: 0.7, side: THREE.DoubleSide, depthWrite: false })
  const lineMat = new THREE.LineBasicMaterial({ color: LINE_COLOR, linewidth: 2, depthWrite: false })

  // Label sprite cache
  const labelTextures = new SvelteMap<string, THREE.SpriteMaterial>()

  function getLabelMaterial(text: string): THREE.SpriteMaterial {
    let mat = labelTextures.get(text)
    if (mat) return mat

    const canvas = document.createElement('canvas')
    canvas.width = 64
    canvas.height = 64
    const ctx = canvas.getContext('2d')!
    ctx.clearRect(0, 0, 64, 64)
    ctx.fillStyle = '#ffffff'
    ctx.font = 'bold 36px Courier New'
    ctx.textAlign = 'center'
    ctx.textBaseline = 'middle'
    ctx.fillText(text, 32, 32)

    const texture = new THREE.CanvasTexture(canvas)
    mat = new THREE.SpriteMaterial({ map: texture, depthWrite: false, depthTest: false })
    labelTextures.set(text, mat)
    return mat
  }

  // Build connecting line geometry
  let prevLineGeo: THREE.BufferGeometry | null = null

  let markers = $derived.by((): WaypointMarker[] => {
    if (tool !== 'npc' || !heightMgr || !schedule) return []
    const entry = schedule.schedule[schedIdx]
    if (!entry) return []

    const mgr = heightMgr
    const result: WaypointMarker[] = []

    // Home position
    const homeY = mgr.getHeightAtWorldPosition(entry.pos[0], entry.pos[2])
    result.push({
      index: -1,
      x: entry.pos[0],
      y: homeY + Y_OFFSET,
      z: entry.pos[2],
      material: dragIdx === -1 ? dragMat : homeMat,
      label: '#0',
    })

    // Waypoints
    const waypoints = entry.waypoints
    for (let i = 0; i < waypoints.length; i++) {
      const wp = waypoints[i]
      const wpY = mgr.getHeightAtWorldPosition(wp[0], wp[2])
      result.push({
        index: i,
        x: wp[0],
        y: wpY + Y_OFFSET,
        z: wp[2],
        material: dragIdx === i ? dragMat : waypointMat,
        label: `#${i + 1}`,
      })
    }

    return result
  })

  let lineGeo = $derived.by((): THREE.BufferGeometry | null => {
    if (markers.length < 2 || !heightMgr) {
      prevLineGeo?.dispose()
      prevLineGeo = null
      return null
    }

    const mgr = heightMgr
    const points: number[] = []
    const lineYOff = Y_OFFSET + 0.1

    for (const m of markers) {
      points.push(m.x, mgr.getHeightAtWorldPosition(m.x, m.z) + lineYOff, m.z)
    }
    // Close the loop back to home
    const home = markers[0]
    points.push(home.x, mgr.getHeightAtWorldPosition(home.x, home.z) + lineYOff, home.z)

    prevLineGeo?.dispose()
    const geo = new THREE.BufferGeometry()
    geo.setAttribute('position', new THREE.Float32BufferAttribute(new Float32Array(points), 3))
    prevLineGeo = geo
    return geo
  })

  onDestroy(() => {
    unsubs.forEach((u) => u())
    prevLineGeo?.dispose()
    circleGeo.dispose()
    homeMat.dispose()
    waypointMat.dispose()
    dragMat.dispose()
    lineMat.dispose()
    for (const mat of labelTextures.values()) {
      mat.map?.dispose()
      mat.dispose()
    }
    labelTextures.clear()
  })
</script>

{#each markers as m (`wp-${m.index}`)}
  <!-- Circle disc on terrain -->
  <T.Mesh
    geometry={circleGeo}
    material={m.material}
    position={[m.x, m.y, m.z]}
    rotation={[-Math.PI / 2, 0, 0]}
    renderOrder={999}
    frustumCulled={false}
  />

  <!-- Number label sprite -->
  <T.Sprite
    material={getLabelMaterial(m.label)}
    position={[m.x, m.y + LABEL_Y_OFFSET, m.z]}
    scale={[2, 2, 1]}
    renderOrder={1000}
    frustumCulled={false}
  />
{/each}

<!-- Connecting line -->
{#if lineGeo}
  <T.Line renderOrder={999} frustumCulled={false}>
    <T is={lineGeo} />
    <T is={lineMat} />
  </T.Line>
{/if}

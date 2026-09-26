<script lang="ts">
  import { untrack } from 'svelte'
  import { get } from 'svelte/store'
  import { weatherRadarVisible, playerDebugInfo } from '../stores/debugStore'
  import { mountOverlay } from '../stores/overlayStack'
  import { weather, weatherSectorsReady } from '../stores/weatherStore'
  import { gameTimeState } from './GameTimeWidget.svelte'
  import { gameMinutesAt } from '../utils/weatherSample'
  import {
    CONTINENT_VIEW,
    canvasHeightFor,
    canvasToWorld,
    cellInView,
    cellsAt,
    formatGameMinutes,
    formatRealMinutes,
    minutesUntilRain,
    RAIN_SEARCH_HORIZON_MIN,
    viewWrappedX,
    worldToCanvas,
    zoneName,
    type RadarCell,
  } from '../utils/weatherRadar'
  import { weather_rain_at } from '../wasm/onlinerpg_shared'
  import { RegionImageCache } from '../terrain/regionImageCache'
  import { pickMinimapSourceSize } from '../terrain/regionMinimapGenerator'
  import { REGION_CELLS, TILE_DIM } from '../terrain/terrain-constants'
  import { WORLD_MIN_REGION_Z, WORLD_MAX_REGION_Z } from '../terrain/world-wrap'
  import { minimapVersion } from '../stores/editorStore'

  const WIDTH = 760
  const HEIGHT = canvasHeightFor(CONTINENT_VIEW, WIDTH)
  const SCALE = WIDTH / (CONTINENT_VIEW.x1 - CONTINENT_VIEW.x0)
  const AHEAD_MAX = 720
  const FAST_FORWARD = 60
  const SAMPLE_MS = 250
  const OCEAN = '#01294e'
  const NEXT_RAIN_MS = 2000
  const SCRUB_SETTLE_MS = 250

  let ahead = $state(0)
  let fastForward = $state(false)
  let canvas: HTMLCanvasElement | null = $state(null)
  let pointer: {
    worldX: number
    worldZ: number
    tipX: number
    tipY: number
    flip: boolean
  } | null = $state(null)
  let cells: RadarCell[] = $state([])
  let nextRain: number | null = $state(null)
  let hereRain = $state(0)
  let hasForecast = $state(false)

  const regionImages = new RegionImageCache()
  regionImages.limit = 0
  let mapLayer: HTMLCanvasElement | null = null
  let mapVersionDrawn: number | null = null
  let nextRainDueAt = 0

  const ready = $derived(
    $weather !== null &&
      ($weatherSectorsReady || $weather.rainOverride !== null)
  )

  const hover = $derived.by(() => {
    const at = pointer
    if (!at) return null
    const cell = cells.find((cell) => {
      const dx = at.worldX - viewWrappedX(cell.x, CONTINENT_VIEW)
      const dz = at.worldZ - cell.z
      return dx * dx + dz * dz <= cell.radiusM * cell.radiusM
    })
    return cell ? { cell, tipX: at.tipX, tipY: at.tipY, flip: at.flip } : null
  })

  function viewMinutes() {
    return gameMinutesAt(gameTimeState.date, gameTimeState.serverHour) + ahead
  }

  // Keep tile pixels in the layer and retry state in the image cache.
  function buildMapLayer(version: number) {
    const layer = document.createElement('canvas')
    layer.width = WIDTH
    layer.height = HEIGHT
    const ctx = layer.getContext('2d')
    if (!ctx) return
    ctx.fillStyle = OCEAN
    ctx.fillRect(0, 0, WIDTH, HEIGHT)
    mapLayer = layer

    const size = Math.ceil(REGION_CELLS * SCALE)
    const sourceSize = pickMinimapSourceSize(REGION_CELLS * SCALE)
    const minRx = Math.floor((CONTINENT_VIEW.x0 + TILE_DIM / 2) / REGION_CELLS)
    const maxRx = Math.floor((CONTINENT_VIEW.x1 + TILE_DIM / 2) / REGION_CELLS)
    const minRz = Math.floor((CONTINENT_VIEW.z0 + TILE_DIM / 2) / REGION_CELLS)
    const maxRz = Math.floor((CONTINENT_VIEW.z1 + TILE_DIM / 2) / REGION_CELLS)
    for (let rz = minRz; rz <= maxRz; rz++) {
      if (rz < WORLD_MIN_REGION_Z || rz > WORLD_MAX_REGION_Z) continue
      for (let rx = minRx; rx <= maxRx; rx++) {
        const worldX = rx * REGION_CELLS - TILE_DIM / 2
        const worldZ = rz * REGION_CELLS - TILE_DIM / 2
        const at = worldToCanvas(worldX, worldZ, CONTINENT_VIEW, WIDTH)
        void regionImages.load(rx, rz, version, sourceSize).then((img) => {
          if (img && mapLayer === layer)
            ctx.drawImage(img, at.x, at.y, size, size)
        })
      }
    }
  }

  function drawCell(ctx: CanvasRenderingContext2D, cell: RadarCell) {
    const at = worldToCanvas(
      viewWrappedX(cell.x, CONTINENT_VIEW),
      cell.z,
      CONTINENT_VIEW,
      WIDTH
    )
    const r = cell.radiusM * SCALE
    if (r <= 0) return
    // Matches rain_falloff: flat to 70 % of the radius, then a short fade.
    const gradient = ctx.createRadialGradient(at.x, at.y, 0, at.x, at.y, r)
    const peak = 0.12 + 0.5 * cell.env
    gradient.addColorStop(0, `rgba(90, 169, 230, ${peak})`)
    gradient.addColorStop(0.7, `rgba(90, 169, 230, ${peak})`)
    gradient.addColorStop(1, 'rgba(90, 169, 230, 0)')
    ctx.fillStyle = gradient
    ctx.beginPath()
    ctx.arc(at.x, at.y, r, 0, Math.PI * 2)
    ctx.fill()

    ctx.strokeStyle =
      cell.stage === 'forming'
        ? 'rgba(224, 178, 90, 0.75)'
        : cell.stage === 'clearing'
          ? 'rgba(138, 164, 184, 0.6)'
          : 'rgba(90, 169, 230, 0.85)'
    ctx.lineWidth = 1
    ctx.beginPath()
    ctx.arc(at.x, at.y, r, 0, Math.PI * 2)
    // Where the centre will be when the cell dies.
    ctx.moveTo(at.x, at.y)
    ctx.lineTo(
      at.x + cell.vx * cell.remainMin * SCALE,
      at.y + cell.vz * cell.remainMin * SCALE
    )
    ctx.stroke()
  }

  function drawPlayer(
    ctx: CanvasRenderingContext2D,
    pos: { x: number; z: number } | undefined
  ) {
    if (!pos) return
    const wrapped = viewWrappedX(pos.x, CONTINENT_VIEW)
    const at = worldToCanvas(wrapped, pos.z, CONTINENT_VIEW, WIDTH)
    const outside =
      wrapped < CONTINENT_VIEW.x0 ||
      wrapped > CONTINENT_VIEW.x1 ||
      pos.z < CONTINENT_VIEW.z0 ||
      pos.z > CONTINENT_VIEW.z1
    // Mark offscreen players with a ring at the map edge.
    const x = Math.min(Math.max(at.x, 6), WIDTH - 6)
    const y = Math.min(Math.max(at.y, 6), HEIGHT - 6)
    ctx.strokeStyle = '#f0c36b'
    ctx.lineWidth = 2
    ctx.beginPath()
    if (outside) {
      ctx.arc(x, y, 4, 0, Math.PI * 2)
    } else {
      ctx.moveTo(x - 5, y)
      ctx.lineTo(x + 5, y)
      ctx.moveTo(x, y - 5)
      ctx.lineTo(x, y + 5)
    }
    ctx.stroke()
  }

  function render(
    list: RadarCell[],
    pos: { x: number; z: number } | undefined
  ) {
    const ctx = canvas?.getContext('2d')
    if (!ctx) return
    ctx.fillStyle = OCEAN
    ctx.fillRect(0, 0, WIDTH, HEIGHT)
    if (mapLayer) ctx.drawImage(mapLayer, 0, 0)
    for (const cell of list) drawCell(ctx, cell)
    drawPlayer(ctx, pos)
  }

  function tick() {
    const version = get(minimapVersion)
    if (version !== mapVersionDrawn) {
      mapVersionDrawn = version
      buildMapLayer(version)
    }

    const w = get(weather)
    const pos = get(playerDebugInfo)?.position
    if (!w || !ready || !pos) {
      cells = []
      hereRain = 0
      nextRain = null
      nextRainDueAt = 0
      hasForecast = false
      render([], pos)
      return
    }

    const t = viewMinutes()
    const list =
      w.rainOverride !== null
        ? []
        : cellsAt(w.seed, w.bias, t).filter((c) =>
            cellInView(c, CONTINENT_VIEW)
          )
    cells = list
    hereRain =
      w.rainOverride ?? weather_rain_at(w.seed, w.bias, t, pos.x, pos.z)
    const now = performance.now()
    if (w.rainOverride !== null) {
      nextRain = null
    } else if (now >= nextRainDueAt) {
      nextRainDueAt = now + NEXT_RAIN_MS
      nextRain = minutesUntilRain(w.seed, w.bias, t, pos.x, pos.z)
      hasForecast = true
    }
    render(list, pos)
  }

  $effect(() =>
    $weatherRadarVisible ? mountOverlay('weatherRadar') : undefined
  )

  $effect(() => {
    if (!$weatherRadarVisible) {
      stopHover()
      mapLayer = null
      mapVersionDrawn = null
      return
    }
    nextRainDueAt = 0
    // Per-frame updates must not restart the sampling timer.
    untrack(tick)
    const id = setInterval(tick, SAMPLE_MS)
    return () => clearInterval(id)
  })

  $effect(() => {
    if (!$weatherRadarVisible || !fastForward) return
    const id = setInterval(() => {
      ahead = ahead + FAST_FORWARD > AHEAD_MAX ? 0 : ahead + FAST_FORWARD
      retime()
    }, 500)
    return () => clearInterval(id)
  })

  // Delay the forecast scan until scrubbing settles.
  function retime() {
    hasForecast = false
    nextRainDueAt = performance.now() + SCRUB_SETTLE_MS
    untrack(tick)
  }

  function onPointerMove(event: PointerEvent) {
    if (!canvas) return
    const rect = canvas.getBoundingClientRect()
    // Convert display pixels to canvas coordinates for hit-testing.
    const tipX = event.clientX - rect.left
    const tipY = event.clientY - rect.top
    const world = canvasToWorld(
      (tipX / rect.width) * WIDTH,
      (tipY / rect.height) * HEIGHT,
      CONTINENT_VIEW,
      WIDTH
    )
    pointer = {
      worldX: world.x,
      worldZ: world.z,
      tipX,
      tipY,
      flip: tipX > rect.width / 2,
    }
  }

  function stopHover() {
    pointer = null
  }

  function toNow() {
    ahead = 0
    fastForward = false
    retime()
  }
</script>

{#if $weatherRadarVisible}
  <div class="dialog">
    <div class="dialog-title">
      <span>Weather Radar</span>
      <span class="sub">
        {#if !ready}
          waiting for sectors
        {:else if $weather && $weather.rainOverride !== null}
          admin override {$weather.rainOverride.toFixed(2)}
        {:else}
          seed {$weather?.seed} · bias {$weather?.bias}
        {/if}
      </span>
      <button
        class="close"
        onclick={() => weatherRadarVisible.set(false)}
        aria-label="Close the weather radar">x</button
      >
    </div>

    <div class="dialog-body">
      <div class="mapbox">
        <canvas
          bind:this={canvas}
          width={WIDTH}
          height={HEIGHT}
          onpointermove={onPointerMove}
          onpointerleave={stopHover}
        ></canvas>
        {#if hover}
          <div
            class="tip"
            style="left:{hover.tipX + 12}px; top:{hover.tipY +
              12}px; transform:{hover.flip
              ? 'translateX(calc(-100% - 24px))'
              : 'none'}"
          >
            <b>{zoneName(hover.cell.zone)}</b> · sector {hover.cell.sector}<br
            />
            {hover.cell.stage} · r {(hover.cell.radiusM / 1000).toFixed(1)} km<br
            />
            ends in {formatGameMinutes(hover.cell.remainMin)}
          </div>
        {/if}
      </div>

      <div class="controls">
        <input
          type="range"
          min="0"
          max={AHEAD_MAX}
          step="5"
          bind:value={ahead}
          oninput={retime}
          aria-label="Game minutes ahead"
        />
        <span class="readout">
          {ahead === 0 ? 'now' : `+${formatGameMinutes(ahead)}`}
        </span>
        <button onclick={toNow}>Now</button>
        <button
          class:active={fastForward}
          onclick={() => (fastForward = !fastForward)}
        >
          FF ×{FAST_FORWARD}
        </button>
      </div>

      <div class="rows">
        <div class="row">
          <span class="label">Here</span>
          <span class="value">{hereRain.toFixed(2)}</span>
          <span class="bar"><i style="width:{hereRain * 100}%"></i></span>
        </div>
        <div class="row">
          <span class="label">Next rain</span>
          <span class="value">
            {#if $weather && $weather.rainOverride !== null}
              held by /weather
            {:else if !hasForecast}
              …
            {:else if nextRain === null}
              none within {formatGameMinutes(RAIN_SEARCH_HORIZON_MIN)}
            {:else if nextRain === 0}
              {ahead === 0 ? 'raining now' : 'raining then'}
            {:else}
              {formatGameMinutes(nextRain)} ({formatRealMinutes(nextRain)})
            {/if}
          </span>
        </div>
        <div class="row">
          <span class="label">Cells in view</span>
          <span class="value">{cells.length}</span>
        </div>
      </div>

      <div class="tablewrap">
        <table>
          <thead>
            <tr>
              <th>Zone</th>
              <th>Stage</th>
              <th class="num">Radius</th>
              <th class="num">Ends in</th>
            </tr>
          </thead>
          <tbody>
            {#each cells as cell (cell.sector)}
              <tr>
                <td>{zoneName(cell.zone)}</td>
                <td><span class="chip {cell.stage}">{cell.stage}</span></td>
                <td class="num">{(cell.radiusM / 1000).toFixed(1)} km</td>
                <td class="num">{formatGameMinutes(cell.remainMin)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog {
    position: fixed;
    top: 56px;
    right: 270px;
    max-width: calc(100vw - 280px);
    z-index: 40;
    width: 460px;
    box-sizing: border-box;
    max-height: calc(100vh - 150px);
    max-height: calc(100dvh - 150px);
    display: flex;
    flex-direction: column;
    background: rgba(0, 0, 0, 0.9);
    border: 1px solid rgba(0, 255, 0, 0.25);
    border-radius: 8px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.8);
    font-family: 'Courier New', monospace;
    color: #c8e0c8;
    overflow: hidden;
  }

  .dialog-title {
    flex-shrink: 0;
    font-size: 11px;
    font-weight: bold;
    color: #00ff00;
    padding: 6px 12px 4px;
    border-bottom: 1px solid rgba(0, 255, 0, 0.15);
    display: flex;
    align-items: center;
    gap: 8px;
    justify-content: space-between;
  }

  .close {
    flex-shrink: 0;
    background: none;
    border: none;
    color: #7aa07a;
    cursor: pointer;
    font: inherit;
    line-height: 1;
    padding: 0 2px;
  }

  .close:hover {
    color: #00ff00;
  }

  .sub {
    color: #7aa07a;
    font-weight: normal;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .dialog-body {
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .mapbox {
    position: relative;
    margin: 8px;
  }

  canvas {
    box-sizing: border-box;
    display: block;
    width: 100%;
    height: auto;
    border: 1px solid rgba(0, 255, 0, 0.15);
  }

  .tip {
    position: absolute;
    pointer-events: none;
    transform: translateZ(0);
    max-width: 200px;
    background: rgba(0, 0, 0, 0.92);
    border: 1px solid rgba(0, 255, 0, 0.25);
    border-radius: 4px;
    padding: 5px 8px;
    font-size: 10px;
    line-height: 1.5;
    white-space: nowrap;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 8px;
    font-size: 11px;
  }

  input[type='range'] {
    flex: 1;
    min-width: 0;
    accent-color: #5aa9e6;
  }

  .readout {
    min-width: 7ch;
    text-align: right;
  }

  button {
    background: rgba(0, 255, 0, 0.08);
    border: 1px solid rgba(0, 255, 0, 0.25);
    border-radius: 3px;
    color: #c8e0c8;
    font: inherit;
    font-size: 10px;
    padding: 3px 8px;
    cursor: pointer;
  }

  button.active {
    background: rgba(90, 169, 230, 0.3);
    border-color: #5aa9e6;
  }

  .rows {
    padding: 0 12px 8px;
    font-size: 11px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }

  .label {
    color: #7aa07a;
    min-width: 9ch;
  }

  .bar {
    flex: 1;
    height: 5px;
    background: rgba(0, 255, 0, 0.1);
    border-radius: 3px;
    overflow: hidden;
  }

  .bar i {
    display: block;
    height: 100%;
    background: #5aa9e6;
  }

  .tablewrap {
    padding: 0 12px 10px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 10px;
  }

  th {
    text-align: left;
    color: #5f7f5f;
    font-weight: normal;
    padding-bottom: 3px;
    position: sticky;
    top: 0;
    background: rgba(0, 0, 0, 0.95);
  }

  td {
    padding: 3px 0;
    border-top: 1px solid rgba(0, 255, 0, 0.1);
  }

  .num {
    text-align: right;
  }

  .chip {
    padding: 0 5px;
    border-radius: 8px;
    font-size: 9px;
  }

  .chip.forming {
    background: rgba(224, 178, 90, 0.2);
    color: #e0b25a;
  }

  .chip.raining {
    background: rgba(90, 169, 230, 0.2);
    color: #5aa9e6;
  }

  .chip.clearing {
    background: rgba(138, 164, 184, 0.2);
    color: #8aa4b8;
  }

  @media (max-width: 740px) {
    .dialog {
      right: 10px;
      max-width: calc(100vw - 20px);
    }
  }
</style>

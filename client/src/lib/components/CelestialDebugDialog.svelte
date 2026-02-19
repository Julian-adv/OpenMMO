<script lang="ts">
  import { celestialDebugVisible } from '../stores/debugStore'
  import { gameTimeState } from './GameTimeWidget.svelte'
  import {
    ELDER_MOON_DEFINITION,
    SWIFT_MOON_DEFINITION,
    getMoonPhaseState,
    getMoonPhaseLabel,
    getGameCalendarDayIndex,
    getSunElevation,
    moonPhaseCanvasAction,
    getSunPeriodFromElevation,
    SUN_AXIAL_TILT_DEG,
    SUN_LATITUDE_DEG,
  } from '../utils/celestialSimulation'
  import { getDeclinationRadFromDayIndex } from '../utils/celestialDirection'

  // Diagram layout constants (2x original size)
  const D = 640
  const C = D / 2 // 320
  const EARTH_ORBIT_R = 170
  const ELDER_MOON_ORBIT_R = 82
  const SWIFT_MOON_ORBIT_R = 54
  const EARTH_DISPLAY_R = 18
  const INDICATOR_LENGTH = 10 // extent of indicator beyond Earth surface
  const SUN_SIZE = 60
  const ELDER_MOON_SIZE = 28
  const SWIFT_MOON_SIZE = 20
  const TAU = 2 * Math.PI

  // Night sky ring — outside Elder moon's maximum reach from center (170+82=252)
  const NIGHT_SKY_RING_INNER_R = 262 // 10px outside Elder moon max orbit
  const NIGHT_SKY_RING_OUTER_R = 315 // close to SVG edge (320)
  const NIGHT_SKY_RING_HEIGHT = NIGHT_SKY_RING_OUTER_R - NIGHT_SKY_RING_INNER_R // 53px

  // Seasonal markers on Earth's orbit ring
  // SPRING_EQUINOX_DAY_INDEX=90 (from celestialDirection.ts), so each event is 90 days apart
  const SEASON_MARKERS: { angle: number; label: string; anchor: string; dy: number }[] = [
    { angle: TAU / 4,    label: 'WS', anchor: 'middle', dy: 18  }, // bottom (dayOfYear=0,   Winter Solstice)
    { angle: 0,          label: 'SE', anchor: 'start',  dy: 0   }, // right  (dayOfYear=90,  Spring Equinox)
    { angle: -TAU / 4,   label: 'SS', anchor: 'middle', dy: -12 }, // top    (dayOfYear=180, Summer Solstice)
    { angle: -TAU / 2,   label: 'AE', anchor: 'end',    dy: 0   }, // left   (dayOfYear=270, Autumn Equinox)
  ]

  // Earth orbital position (one revolution per 360 game days, starting from top)
  // Counterclockwise in screen (y↓): decreasing angle as dayOfYear increases — matches real astronomy
  const absoluteDayIndex = $derived(getGameCalendarDayIndex(gameTimeState.date))
  const dayOfYear = $derived((gameTimeState.date.month - 1) * 30 + (gameTimeState.date.day - 1))

  // Canvas action: polar UV mapping of panorama onto the ring
  // For each of 512 angular segments, column i of the panorama maps to angle i/512 * TAU.
  // ctx.rotate(midAngle - TAU/4) makes +y point radially outward at midAngle,
  // so drawImage maps panorama rows → radial direction (innerR..outerR).
  function nightSkyRingAction(node: HTMLCanvasElement) {
    const ctx = node.getContext('2d')!
    const img = new Image()
    img.src = '/icons/night-sky-panorama-512.png'
    img.onload = () => {
      const N = 512
      ctx.globalAlpha = 0.8
      for (let i = 0; i < N; i++) {
        const angle1 = (i / N) * TAU - TAU / 4
        const angle2 = ((i + 1) / N) * TAU - TAU / 4
        const midAngle = (angle1 + angle2) / 2
        // arcWidth: tangential pixel width at outer radius, +2 to avoid seam gaps
        const arcWidth = Math.ceil((TAU * NIGHT_SKY_RING_OUTER_R) / N) + 2
        ctx.save()
        // Clip to this donut segment
        ctx.beginPath()
        ctx.arc(C, C, NIGHT_SKY_RING_OUTER_R, angle1, angle2)
        ctx.arc(C, C, NIGHT_SKY_RING_INNER_R, angle2, angle1, true)
        ctx.closePath()
        ctx.clip()
        // Rotate so that +y points radially outward at midAngle, then flip y
        ctx.translate(C, C)
        ctx.rotate(midAngle - TAU / 4)
        ctx.scale(1, -1)
        // After scale(1,-1): +y is now toward center, so outer edge is at -OUTER_R
        // Panorama top (y=0) → outer edge, bottom → inner edge
        ctx.drawImage(
          img,
          (i + N - 128) % N, 0, 1, img.naturalHeight || 512,
          -arcWidth / 2, -NIGHT_SKY_RING_OUTER_R,
          arcWidth, NIGHT_SKY_RING_HEIGHT,
        )
        ctx.restore()
      }
    }
    return { destroy() {} }
  }

  const earthAngle = $derived(TAU / 4 - (dayOfYear / 360) * TAU)
  const earthX = $derived(C + EARTH_ORBIT_R * Math.cos(earthAngle))
  const earthY = $derived(C + EARTH_ORBIT_R * Math.sin(earthAngle))

  // Sun direction vector from Earth (normalized)
  const earthDist = $derived(Math.max(1, Math.sqrt((C - earthX) ** 2 + (C - earthY) ** 2)))
  const sunDirX = $derived((C - earthX) / earthDist)
  const sunDirY = $derived((C - earthY) / earthDist)
  // Perpendicular to sun direction in screen space (rotate 90° CCW in screen → perpX=-sunDirY, perpY=sunDirX)
  // This gives sweep-flag=1 for the lit semicircle (verified geometrically)
  const perpX = $derived(-sunDirY)
  const perpY = $derived(sunDirX)

  const earthToSunAngle = $derived(Math.atan2(C - earthY, C - earthX))

  // Terminator chord offset: shifts the day/night boundary based on observer latitude + declination.
  // Summer (decl > 0): chord moves anti-sun → lit arc > 180° (longer day at this latitude)
  // Winter (decl < 0): chord moves toward sun → lit arc < 180° (shorter day)
  // Formula: d = -R * sin(lat) * tan(decl)
  const LAT_RAD = (SUN_LATITUDE_DEG * Math.PI) / 180
  const chordOffset = $derived.by(() => {
    const decl = getDeclinationRadFromDayIndex(dayOfYear, SUN_AXIAL_TILT_DEG)
    return -(EARTH_DISPLAY_R + 2.5) * Math.sin(LAT_RAD) * Math.tan(decl)
  })

  // Builds an SVG arc path for the lit portion of a circle of radius r, with chord offset d along sunDir.
  function litChordPath(
    cx: number, cy: number, r: number, d: number,
    sdx: number, sdy: number, px: number, py: number
  ) {
    const clampedD = Math.max(-(r - 0.5), Math.min(r - 0.5, d))
    const half = Math.sqrt(Math.max(0, r * r - clampedD * clampedD))
    const largeArc = clampedD < 0 ? 1 : 0
    return (
      `M ${cx + clampedD * sdx + half * px},${cy + clampedD * sdy + half * py}` +
      ` A ${r},${r} 0 ${largeArc},0` +
      ` ${cx + clampedD * sdx - half * px},${cy + clampedD * sdy - half * py} Z`
    )
  }

  // SVG path: lit arc on the sun-facing portion of Earth (chord offset shifts with season)
  const litHalfPath = $derived(
    litChordPath(earthX, earthY, EARTH_DISPLAY_R, chordOffset, sunDirX, sunDirY, perpX, perpY)
  )
  const cloudHighlightPath = $derived(
    litChordPath(earthX, earthY, EARTH_DISPLAY_R - 4, chordOffset, sunDirX, sunDirY, perpX, perpY)
  )

  // Observer (player) on Earth's surface
  // At hour 12 (noon): observer faces Sun → angle = earthToSunAngle
  // Earth rotates CCW in screen: angle decreases as hour increases past noon
  const observerAngle = $derived(earthToSunAngle - (gameTimeState.hour - 12) * (TAU / 24))
  const obsX = $derived(earthX + EARTH_DISPLAY_R * Math.cos(observerAngle))
  const obsY = $derived(earthY + EARTH_DISPLAY_R * Math.sin(observerAngle))
  const obsOutX = $derived(
    earthX + (EARTH_DISPLAY_R + INDICATOR_LENGTH) * Math.cos(observerAngle)
  )
  const obsOutY = $derived(
    earthY + (EARTH_DISPLAY_R + INDICATOR_LENGTH) * Math.sin(observerAngle)
  )

  // Actual sun elevation using declination (axial tilt + latitude), matching GameTimeWidget
  const sunElevation = $derived(
    getSunElevation({
      hour: gameTimeState.hour,
      month: gameTimeState.date.month,
      day: gameTimeState.date.day,
    })
  )
  const sunPeriod = $derived(getSunPeriodFromElevation(sunElevation))
  const indicatorColor = $derived(
    sunPeriod === 'day' ? '#ffdd44' : sunPeriod === 'twilight' ? '#ff9944' : '#8899dd'
  )
  const timeOfDayLabel = $derived(
    sunPeriod === 'day' ? 'Day' : sunPeriod === 'twilight' ? 'Twilight' : 'Night'
  )

  // Moon orbital positions
  const elderPhase = $derived(
    getMoonPhaseState(ELDER_MOON_DEFINITION, absoluteDayIndex, gameTimeState.hour)
  )
  const swiftPhase = $derived(
    getMoonPhaseState(SWIFT_MOON_DEFINITION, absoluteDayIndex, gameTimeState.hour)
  )
  // orbitalProgress=0 → new moon (toward Sun), 0.5 → full moon (away from Sun)
  // Counterclockwise in screen: subtract progress — consistent with Earth orbit and real astronomy
  const elderAngle = $derived(earthToSunAngle - elderPhase.orbitalProgress * TAU)
  const swiftAngle = $derived(earthToSunAngle - swiftPhase.orbitalProgress * TAU)
  const elderX = $derived(earthX + ELDER_MOON_ORBIT_R * Math.cos(elderAngle))
  const elderY = $derived(earthY + ELDER_MOON_ORBIT_R * Math.sin(elderAngle))
  const swiftX = $derived(earthX + SWIFT_MOON_ORBIT_R * Math.cos(swiftAngle))
  const swiftY = $derived(earthY + SWIFT_MOON_ORBIT_R * Math.sin(swiftAngle))

  const elderPhaseLabel = $derived(
    getMoonPhaseLabel(elderPhase.illumination, elderPhase.isWaxing)
  )
  const swiftPhaseLabel = $derived(
    getMoonPhaseLabel(swiftPhase.illumination, swiftPhase.isWaxing)
  )

  function formatHour(h: number) {
    const hh = Math.floor(h)
    const mm = Math.floor((h - hh) * 60)
    return `${hh.toString().padStart(2, '0')}:${mm.toString().padStart(2, '0')}`
  }
</script>

{#if $celestialDebugVisible}
  <div class="dialog">
    <div class="dialog-title">Celestial Orbits</div>

    <div class="diagram" style="width:{D}px; height:{D}px">
      <!-- Night sky ring: polar UV mapped panorama on canvas (static, behind SVG) -->
      <canvas class="night-sky-ring" width={D} height={D} use:nightSkyRingAction></canvas>

      <!-- All static geometry + Earth + Sun in SVG -->
      <svg width={D} height={D} class="diagram-svg">
        <!-- Earth orbit ring -->
        <circle
          cx={C}
          cy={C}
          r={EARTH_ORBIT_R}
          fill="none"
          stroke="rgba(100,160,255,0.18)"
          stroke-width="1"
          stroke-dasharray="4 5"
        />

        <!-- Seasonal markers on Earth's orbit (동지·춘분·하지·추분) -->
        {#each SEASON_MARKERS as m (m.label)}
          {@const mx = C + EARTH_ORBIT_R * Math.cos(m.angle)}
          {@const my = C + EARTH_ORBIT_R * Math.sin(m.angle)}
          {@const tx = C + (EARTH_ORBIT_R + 14) * Math.cos(m.angle)}
          {@const ty = C + (EARTH_ORBIT_R + 14) * Math.sin(m.angle) + m.dy}
          <!-- marker dot -->
          <circle cx={mx} cy={my} r="3" fill="rgba(200,210,255,0.55)" />
          <!-- tick line from orbit outward -->
          <line
            x1={mx}
            y1={my}
            x2={C + (EARTH_ORBIT_R + 8) * Math.cos(m.angle)}
            y2={C + (EARTH_ORBIT_R + 8) * Math.sin(m.angle)}
            stroke="rgba(180,195,255,0.4)"
            stroke-width="1"
          />
          <!-- label -->
          <text
            x={tx}
            y={ty}
            text-anchor={m.anchor}
            dominant-baseline="middle"
            font-size="10"
            font-family="Courier New, monospace"
            fill="rgba(170,185,230,0.75)"
          >{m.label}</text>
        {/each}

        <!-- Moon orbit rings (centered on Earth) -->
        <circle
          cx={earthX}
          cy={earthY}
          r={ELDER_MOON_ORBIT_R}
          fill="none"
          stroke="rgba(200,215,255,0.22)"
          stroke-width="1"
          stroke-dasharray="3 4"
        />
        <circle
          cx={earthX}
          cy={earthY}
          r={SWIFT_MOON_ORBIT_R}
          fill="none"
          stroke="rgba(200,215,255,0.16)"
          stroke-width="1"
          stroke-dasharray="3 4"
        />

        <!-- Sun corona glow (behind image) -->
        <circle cx={C} cy={C} r={SUN_SIZE * 0.85} fill="rgba(255,210,60,0.07)" />
        <circle cx={C} cy={C} r={SUN_SIZE * 0.60} fill="rgba(255,230,100,0.11)" />

        <!-- Earth: night side (dark navy circle) -->
        <circle cx={earthX} cy={earthY} r={EARTH_DISPLAY_R} fill="#081830" />

        <!-- Earth: day side (lit semicircle facing Sun, arc sweep=1 clockwise) -->
        <path d={litHalfPath} fill="#4a90d9" opacity="0.90" />

        <!-- Earth: cloud highlight on lit side (subtle lighter arc, smaller radius) -->
        <path d={cloudHighlightPath} fill="rgba(160,210,255,0.18)" />

        <!-- Earth: atmosphere ring -->
        <circle
          cx={earthX}
          cy={earthY}
          r={EARTH_DISPLAY_R}
          fill="none"
          stroke="rgba(100,170,255,0.55)"
          stroke-width="1.5"
        />
        <circle
          cx={earthX}
          cy={earthY}
          r={EARTH_DISPLAY_R + 2.5}
          fill="none"
          stroke="rgba(100,170,255,0.10)"
          stroke-width="2.5"
        />

        <!-- Observer indicator (stick person standing on Earth's surface) -->
        <!-- Base dot: standing point on Earth's surface -->
        <circle cx={obsX} cy={obsY} r="2.5" fill={indicatorColor} opacity="0.85" />
        <!-- Body: line extending radially outward -->
        <line
          x1={obsX}
          y1={obsY}
          x2={obsOutX}
          y2={obsOutY}
          stroke={indicatorColor}
          stroke-width="2"
          stroke-linecap="round"
        />
        <!-- Head: circle at outer tip -->
        <circle cx={obsOutX} cy={obsOutY} r="3.5" fill={indicatorColor} />

        <!-- Sun image (rendered on top of glow) -->
        <image
          href="/icons/sun.png"
          x={C - SUN_SIZE / 2}
          y={C - SUN_SIZE / 2}
          width={SUN_SIZE}
          height={SUN_SIZE}
        />
      </svg>

      <!-- Moon canvases (HTML, absolutely positioned over SVG) -->
      <canvas
        class="moon elder-moon"
        aria-label="Eldor (Elder Moon)"
        use:moonPhaseCanvasAction={{
          moonId: 'elder',
          illumination: elderPhase.illumination,
          isWaxing: elderPhase.isWaxing,
          sizePx: ELDER_MOON_SIZE,
          isDaylight: sunPeriod === 'day',
        }}
        style="left:{elderX}px; top:{elderY}px; width:{ELDER_MOON_SIZE}px; height:{ELDER_MOON_SIZE}px;"
      ></canvas>
      <canvas
        class="moon swift-moon"
        aria-label="Serin (Swift Moon)"
        use:moonPhaseCanvasAction={{
          moonId: 'swift',
          illumination: swiftPhase.illumination,
          isWaxing: swiftPhase.isWaxing,
          sizePx: SWIFT_MOON_SIZE,
          isDaylight: sunPeriod === 'day',
        }}
        style="left:{swiftX}px; top:{swiftY}px; width:{SWIFT_MOON_SIZE}px; height:{SWIFT_MOON_SIZE}px; filter: hue-rotate(12deg) drop-shadow(0 0 6px rgba(215,228,255,0.7));"
      ></canvas>
    </div>

    <!-- Info panel -->
    <div class="info">
      <div class="info-row">
        <span class="info-label">Time</span>
        <span class="info-value">{formatHour(gameTimeState.hour)}</span>
      </div>
      <div class="info-row">
        <span class="info-label">Day</span>
        <span class="info-value">{dayOfYear + 1}/360</span>
      </div>
      <div class="info-row">
        <span class="info-label">Status</span>
        <span class="info-value" style="color:{indicatorColor}">{timeOfDayLabel}</span>
      </div>
      <div class="info-divider"></div>
      <div class="info-row">
        <span class="info-label elder-label">Eldor</span>
        <span class="info-value elder-val"
          >{elderPhaseLabel} ({Math.round(elderPhase.illumination * 100)}%)</span
        >
      </div>
      <div class="info-row">
        <span class="info-label swift-label">Serin</span>
        <span class="info-value swift-val"
          >{swiftPhaseLabel} ({Math.round(swiftPhase.illumination * 100)}%)</span
        >
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog {
    position: fixed;
    top: 56px;
    right: 10px;
    z-index: 999;
    background: rgba(0, 0, 0, 0.9);
    border: 1px solid rgba(0, 255, 0, 0.25);
    border-radius: 8px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.8);
    font-family: 'Courier New', monospace;
    color: #c8e0c8;
    overflow: hidden;
    pointer-events: none;
  }

  .dialog-title {
    font-size: 11px;
    font-weight: bold;
    color: #00ff00;
    padding: 6px 12px 4px;
    border-bottom: 1px solid rgba(0, 255, 0, 0.15);
    letter-spacing: 1px;
    text-transform: uppercase;
  }

  .diagram {
    position: relative;
    line-height: 0;
    background: radial-gradient(circle, #0d1e38 0%, #030810 100%);
  }

  .night-sky-ring {
    position: absolute;
    top: 0;
    left: 0;
  }

  .diagram-svg {
    position: absolute;
    top: 0;
    left: 0;
  }

  /* Moon canvases are absolutely positioned over the SVG */
  .moon {
    position: absolute;
    transform: translate(-50%, -50%);
    image-rendering: pixelated;
  }

  .elder-moon {
    filter: drop-shadow(0 0 6px rgba(215, 228, 255, 0.7));
  }

  .info {
    padding: 6px 12px 8px;
    border-top: 1px solid rgba(0, 255, 0, 0.15);
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .info-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
    gap: 16px;
  }

  .info-label {
    color: rgba(150, 200, 150, 0.7);
    min-width: 46px;
  }

  .info-value {
    color: #d0ecd0;
    text-align: right;
  }

  .info-divider {
    height: 1px;
    background: rgba(0, 255, 0, 0.1);
    margin: 2px 0;
  }

  .elder-label,
  .elder-val {
    color: #c8d8f0;
  }

  .swift-label,
  .swift-val {
    color: #b8cce8;
  }
</style>

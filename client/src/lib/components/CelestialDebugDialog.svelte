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
  const D = 520
  const C = D / 2 // 260
  const EARTH_ORBIT_R = 170
  const ELDER_MOON_ORBIT_R = 82
  const SWIFT_MOON_ORBIT_R = 54
  const EARTH_DISPLAY_R = 18
  const INDICATOR_LENGTH = 10 // extent of indicator beyond Earth surface
  const SUN_SIZE = 60
  const ELDER_MOON_SIZE = 28
  const SWIFT_MOON_SIZE = 20
  const TAU = 2 * Math.PI

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
      <!-- All static geometry + Earth + Sun in SVG -->
      <svg width={D} height={D} class="diagram-svg">
        <defs>
          <radialGradient id="spaceGrad" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stop-color="#0d1e38" />
            <stop offset="100%" stop-color="#030810" />
          </radialGradient>
        </defs>

        <!-- Space background -->
        <rect width={D} height={D} fill="url(#spaceGrad)" />

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
  }

  .diagram-svg {
    display: block;
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

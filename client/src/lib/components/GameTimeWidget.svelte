<script lang="ts" module>
  export const gameTimeState = $state({
    hour: 12,
    date: { year: 217, month: 1, day: 1 },
  })

  export function setGameHour(hour: number) {
    gameTimeState.hour = ((hour % 24) + 24) % 24
  }

  export function setGameDate(year: number, month: number, day: number) {
    gameTimeState.date = {
      year: Math.max(1, Math.floor(year)),
      month: Math.min(12, Math.max(1, Math.floor(month))),
      day: Math.min(30, Math.max(1, Math.floor(day))),
    }
  }
</script>

<script lang="ts">
  import { calendarVisible } from '../stores/debugStore'
  import {
    getCelestialDirectionFromHourAndDeclination,
    getDeclinationRadFromDayIndex,
  } from '../utils/celestialDirection'
  import { getSolarDaylightWindow } from '../utils/celestialSimulation'
  import {
    type MoonDefinition,
    ELDER_MOON_DEFINITION,
    MOON_VISIBILITY_THRESHOLD,
    SWIFT_MOON_DEFINITION,
    SUN_AXIAL_TILT_DEG,
    SUN_LATITUDE_DEG,
    SUN_TWILIGHT_ELEVATION_THRESHOLD,
    getGameCalendarDayIndex,
    getMoonPhaseLabel,
    getMoonPhaseState,
    getMoonTrackState,
    getSunTrackState,
    moonPhaseCanvasAction,
  } from '../utils/celestialSimulation'

  const SUN_LEFT_MARGIN_PERCENT = -10
  const SUN_RIGHT_MARGIN_PERCENT = 110
  const HORIZON_Y_PERCENT = 85
  const SUN_ARC_HEIGHT_PERCENT = 68
  const MOON_ARC_HEIGHT_PERCENT = 54
  const MOON_DAYLIGHT_VISIBILITY_SCALE = 0.45

  const MONTH_NAMES = [
    'Dawnmere',
    'Reson',
    'Verdant',
    'Highsun',
    'Emberfall',
    'Redrain',
    'Harvestwind',
    'Gloam',
    'Riftwane',
    'Mistveil',
    'Frostrest',
    'Afterglow',
  ] as const

  interface MoonVisualDefinition extends MoonDefinition {
    sizePx: number
    hueRotateDeg: number
    saturation: number
  }

  interface MoonVisualState {
    id: MoonVisualDefinition['id']
    displayName: string
    cycleDay: number
    periodDays: number
    phaseLabel: string
    illumination: number
    isWaxing: boolean
    xPercent: number
    yPercent: number
    sizePx: number
    hueRotateDeg: number
    saturation: number
    isVisible: boolean
    opacity: number
  }

  const MOONS: readonly MoonVisualDefinition[] = [
    {
      ...ELDER_MOON_DEFINITION,
      sizePx: 20,
      hueRotateDeg: 0,
      saturation: 1,
    },
    {
      ...SWIFT_MOON_DEFINITION,
      sizePx: 14,
      hueRotateDeg: 12,
      saturation: 0.85,
    },
  ] as const

  function getMoonVisualState(
    moon: MoonVisualDefinition,
    hour: number,
    absoluteDayIndex: number,
    isDaylight: boolean
  ): MoonVisualState {
    const phaseState = getMoonPhaseState(moon, absoluteDayIndex, hour)
    const phaseLabel = getMoonPhaseLabel(
      phaseState.illumination,
      phaseState.isWaxing
    )
    const trackState = getMoonTrackState({
      phaseState,
      isDaylight,
      leftPercent: SUN_LEFT_MARGIN_PERCENT,
      rightPercent: SUN_RIGHT_MARGIN_PERCENT,
      horizonYPercent: HORIZON_Y_PERCENT,
      arcHeightPercent: MOON_ARC_HEIGHT_PERCENT,
      daylightVisibilityScale: MOON_DAYLIGHT_VISIBILITY_SCALE,
      visibilityThreshold: MOON_VISIBILITY_THRESHOLD,
    })

    return {
      id: moon.id,
      displayName: moon.displayName,
      cycleDay: phaseState.cycleDay,
      periodDays: moon.periodDays,
      phaseLabel,
      illumination: phaseState.illumination,
      isWaxing: phaseState.isWaxing,
      xPercent: trackState.xPercent,
      yPercent: trackState.yPercent,
      sizePx: moon.sizePx,
      hueRotateDeg: moon.hueRotateDeg,
      saturation: moon.saturation,
      isVisible: trackState.isVisible,
      opacity: trackState.opacity,
    }
  }

  function formatGameDate() {
    const monthName =
      MONTH_NAMES[gameTimeState.date.month - 1] ?? `Month ${gameTimeState.date.month}`
    const day = gameTimeState.date.day.toString().padStart(2, '0')
    return `${gameTimeState.date.year} ${monthName} ${day}`
  }

  function formatGameTime() {
    const h = Math.floor(gameTimeState.hour)
    const m = Math.floor((gameTimeState.hour - h) * 60)
    return `${h.toString().padStart(2, '0')}:${m.toString().padStart(2, '0')}`
  }

  function getCurrentDaylightWindow() {
    return getSolarDaylightWindow({
      latitudeDeg: SUN_LATITUDE_DEG,
      month: gameTimeState.date.month,
      day: gameTimeState.date.day,
      axialTiltDeg: SUN_AXIAL_TILT_DEG,
    })
  }

  function getSunVisualState(hour: number, sunriseHour: number, sunsetHour: number) {
    return getSunTrackState({
      hour,
      sunriseHour,
      sunsetHour,
      leftPercent: SUN_LEFT_MARGIN_PERCENT,
      rightPercent: SUN_RIGHT_MARGIN_PERCENT,
      horizonYPercent: HORIZON_Y_PERCENT,
      arcHeightPercent: SUN_ARC_HEIGHT_PERCENT,
    })
  }

  const daylightWindow = $derived(getCurrentDaylightWindow())
  const sunVisual = $derived(
    getSunVisualState(
      gameTimeState.hour,
      daylightWindow.sunriseHour,
      daylightWindow.sunsetHour
    )
  )
  const dayOfYear = $derived(
    (gameTimeState.date.month - 1) * 30 + gameTimeState.date.day
  )
  const sunElevation = $derived.by(() => {
    const declination = getDeclinationRadFromDayIndex(dayOfYear, SUN_AXIAL_TILT_DEG)
    const dir = getCelestialDirectionFromHourAndDeclination(
      gameTimeState.hour,
      12,
      SUN_LATITUDE_DEG,
      declination
    )
    return dir.y
  })
  const isTwilight = $derived(
    sunElevation > 0 && sunElevation < SUN_TWILIGHT_ELEVATION_THRESHOLD
  )
  const absoluteDayIndex = $derived(getGameCalendarDayIndex(gameTimeState.date))
  const moonVisuals = $derived(
    MOONS.map((moon) =>
      getMoonVisualState(moon, gameTimeState.hour, absoluteDayIndex, sunVisual.isDaylight)
    )
  )
</script>

<div class="time-widget" class:compact={!$calendarVisible}>
  {#if $calendarVisible}
    <div class="meta">
      <span class="date">{formatGameDate()}</span>
      <span class="time">{formatGameTime()}</span>
    </div>
  {/if}
  <div class="sky-track">
    <img
      class="horizon"
      src={
        isTwilight
          ? '/icons/horizon-sunset.png'
          : sunVisual.isDaylight
            ? '/icons/horizon.png'
            : '/icons/horizon-night.png'
      }
      alt=""
    />
    {#if sunVisual.isDaylight}
      <img
        class="sun"
        src="/icons/sun.png"
        alt="Sun"
        style={`--sun-x:${sunVisual.xPercent}%; --sun-y:${sunVisual.yPercent}%`}
      />
    {/if}
    {#each moonVisuals as moon (moon.id)}
      <canvas
        class="moon"
        aria-label={`${moon.displayName} Moon`}
        use:moonPhaseCanvasAction={{
          illumination: moon.illumination,
          isWaxing: moon.isWaxing,
          sizePx: moon.sizePx,
          isDaylight: sunVisual.isDaylight,
        }}
        style={`--moon-x:${moon.xPercent}%; --moon-y:${moon.yPercent}%; --moon-size:${moon.sizePx}px; --moon-opacity:${moon.opacity}; --moon-hue:${moon.hueRotateDeg}deg; --moon-saturation:${moon.saturation}; --moon-glow:${sunVisual.isDaylight ? 'drop-shadow(0 0 3px rgba(255,255,255,0.95)) drop-shadow(0 0 1px rgba(60,80,130,0.6))' : 'drop-shadow(0 0 4px rgba(215,228,255,0.65))'};`}
      ></canvas>
    {/each}
    <img
      class="horizon-front"
      src={
        isTwilight
          ? '/icons/horizon-sunset-front.png'
          : sunVisual.isDaylight
          ? '/icons/horizon-front.png'
          : '/icons/horizon-night-front.png'
      }
      alt=""
    />
  </div>
</div>

<style>
  .time-widget {
    position: fixed;
    top: 10px;
    right: 10px;
    z-index: 1000;
    pointer-events: none;
    background: rgba(0, 0, 0, 0.8);
    color: #f7f1d0;
    border-radius: 10px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.45);
    padding: 5px;
    font-family: 'Courier New', monospace;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .time-widget.compact {
    background: transparent;
    box-shadow: none;
    padding: 5px;
    border-radius: 0;
    width: auto;
  }

  .compact .sky-track {
    flex: none;
    width: 256px;
  }

  .meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 108px;
  }

  .sky-track {
    position: relative;
    flex: 1;
    width: 256px;
    height: 36px;
    border-radius: 8px;
    overflow: hidden;
    background:
      linear-gradient(
        180deg,
        rgba(130, 210, 255, 0.82) 0%,
        rgba(85, 170, 230, 0.72) 55%,
        rgba(22, 43, 74, 0.5) 100%
      );
  }

  .horizon {
    position: absolute;
    left: 0;
    bottom: 0;
    width: 100%;
    height: 100%;
    object-fit: fill;
    opacity: 0.95;
    z-index: 1;
  }

  .sun {
    position: absolute;
    width: 32px;
    height: 32px;
    left: var(--sun-x);
    top: var(--sun-y);
    transform: translate(-50%, -50%);
    filter: drop-shadow(0 0 6px rgba(255, 225, 100, 0.85));
    opacity: 1;
    z-index: 2;
  }

  .moon {
    position: absolute;
    width: var(--moon-size);
    height: var(--moon-size);
    left: var(--moon-x);
    top: var(--moon-y);
    transform: translate(-50%, -50%);
    opacity: var(--moon-opacity);
    filter:
      saturate(var(--moon-saturation))
      hue-rotate(var(--moon-hue))
      var(--moon-glow);
    z-index: 2;
  }

  .horizon-front {
    position: absolute;
    left: 0;
    bottom: 0;
    width: 100%;
    height: 100%;
    object-fit: fill;
    z-index: 3;
  }

  .date {
    font-size: 12px;
    opacity: 0.9;
    line-height: 1;
    white-space: nowrap;
    min-width: 108px;
    text-align: left;
  }

  .time {
    font-size: 14px;
    font-weight: bold;
    opacity: 0.95;
    line-height: 1;
    white-space: nowrap;
    letter-spacing: 1px;
  }
</style>

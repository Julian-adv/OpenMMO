<script lang="ts" module>
  let currentGameHour = $state(12)
  let currentGameDate = $state({ year: 217, month: 1, day: 1 })

  export function setGameHour(hour: number) {
    const normalizedHour = ((hour % 24) + 24) % 24
    currentGameHour = normalizedHour
  }

  export function setGameDate(year: number, month: number, day: number) {
    currentGameDate = {
      year: Math.max(1, Math.floor(year)),
      month: Math.min(12, Math.max(1, Math.floor(month))),
      day: Math.min(30, Math.max(1, Math.floor(day))),
    }
  }
</script>

<script lang="ts">
  const SUNRISE_HOUR = 6
  const SUNSET_HOUR = 18
  const DAYLIGHT_HOURS = SUNSET_HOUR - SUNRISE_HOUR
  const HOURS_PER_DAY = 24
  const DAYS_PER_MONTH = 30
  const MONTHS_PER_YEAR = 12
  const DAYS_PER_YEAR = DAYS_PER_MONTH * MONTHS_PER_YEAR
  const SUN_LEFT_MARGIN_PERCENT = 0
  const SUN_RIGHT_MARGIN_PERCENT = 100
  const HORIZON_Y_PERCENT = 70
  const SUN_ARC_HEIGHT_PERCENT = 68
  const MOON_ARC_HEIGHT_PERCENT = 54
  const SUNSET_WINDOW_HOURS = 0.5

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

  interface MoonDefinition {
    id: 'elder' | 'swift'
    displayName: string
    alias: string
    periodDays: number
    phaseOffsetDays: number
    sizePx: number
    hueRotateDeg: number
    saturation: number
  }

  interface MoonVisualState {
    id: MoonDefinition['id']
    displayName: string
    alias: string
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

  const MOONS: readonly MoonDefinition[] = [
    {
      id: 'elder',
      displayName: 'Eldor',
      alias: 'Elder',
      periodDays: 30,
      phaseOffsetDays: 0,
      sizePx: 18,
      hueRotateDeg: 0,
      saturation: 1,
    },
    {
      id: 'swift',
      displayName: 'Serin',
      alias: 'Swift',
      periodDays: 20,
      phaseOffsetDays: 5,
      sizePx: 14,
      hueRotateDeg: 12,
      saturation: 0.85,
    },
  ] as const

  function normalizeHour(hour: number) {
    return ((hour % HOURS_PER_DAY) + HOURS_PER_DAY) % HOURS_PER_DAY
  }

  function positiveModulo(value: number, mod: number) {
    return ((value % mod) + mod) % mod
  }

  function getAbsoluteDayIndex() {
    const normalizedYear = Math.max(1, Math.floor(currentGameDate.year))
    const normalizedMonth = Math.min(
      MONTHS_PER_YEAR,
      Math.max(1, Math.floor(currentGameDate.month))
    )
    const normalizedDay = Math.min(
      DAYS_PER_MONTH,
      Math.max(1, Math.floor(currentGameDate.day))
    )
    return (
      (normalizedYear - 1) * DAYS_PER_YEAR +
      (normalizedMonth - 1) * DAYS_PER_MONTH +
      (normalizedDay - 1)
    )
  }

  function getMoonIllumination(cycleDay: number, fullMoonDay: number, periodDays: number) {
    if (cycleDay <= fullMoonDay) {
      return (cycleDay - 1) / Math.max(1, fullMoonDay - 1)
    }

    return (
      1 - (cycleDay - fullMoonDay) / Math.max(1, periodDays - fullMoonDay)
    )
  }

  function getMoonPhaseLabel(illumination: number, isWaxing: boolean) {
    if (illumination <= 0.05) return 'New'
    if (illumination >= 0.95) return 'Full'
    if (illumination >= 0.45 && illumination <= 0.55) {
      return isWaxing ? 'First Quarter' : 'Last Quarter'
    }
    if (isWaxing) return illumination < 0.5 ? 'Waxing Crescent' : 'Waxing Gibbous'
    return illumination < 0.5 ? 'Waning Crescent' : 'Waning Gibbous'
  }

  interface MoonCanvasParams {
    illumination: number
    isWaxing: boolean
    sizePx: number
  }

  function toMoonPhaseAngleRad(illumination: number, isWaxing: boolean) {
    const clamped = Math.min(1, Math.max(0, illumination))
    const baseAngle = Math.acos(1 - 2 * clamped)
    return isWaxing ? baseAngle : 2 * Math.PI - baseAngle
  }

  function drawMoonToCanvas(node: HTMLCanvasElement, params: MoonCanvasParams) {
    const pixelRatio = globalThis.devicePixelRatio ?? 1
    const renderSize = Math.max(24, Math.round(params.sizePx * pixelRatio))
    if (node.width !== renderSize || node.height !== renderSize) {
      node.width = renderSize
      node.height = renderSize
    }

    const context = node.getContext('2d')
    if (!context) return

    const imageData = context.createImageData(renderSize, renderSize)
    const pixels = imageData.data
    const radius = renderSize * 0.5 - 0.5
    const center = renderSize * 0.5
    const phaseAngle = toMoonPhaseAngleRad(params.illumination, params.isWaxing)
    const sunX = Math.sin(phaseAngle)
    const sunZ = -Math.cos(phaseAngle)

    for (let py = 0; py < renderSize; py += 1) {
      for (let px = 0; px < renderSize; px += 1) {
        const nx = (px + 0.5 - center) / radius
        const ny = (py + 0.5 - center) / radius
        const radiusSquared = nx * nx + ny * ny
        const pixelIndex = (py * renderSize + px) * 4

        if (radiusSquared > 1) {
          pixels[pixelIndex + 3] = 0
          continue
        }

        const nz = Math.sqrt(1 - radiusSquared)
        const lightDot = nx * sunX + nz * sunZ
        const distanceFromEdge = Math.sqrt(radiusSquared)
        const edgeAlpha = Math.min(1, Math.max(0, (1 - distanceFromEdge) / 0.05))

        let red = 0
        let green = 0
        let blue = 0
        let alpha = 0

        if (lightDot > 0) {
          const shade = 0.75 + 0.25 * lightDot
          const base = Math.round(188 + shade * 62)
          red = base - 8
          green = base - 3
          blue = base + 6
          alpha = Math.round(255 * edgeAlpha)
        } else {
          const shade = 0.16 + 0.12 * nz
          const base = Math.round(12 + shade * 42)
          red = base
          green = base + 2
          blue = base + 8
          alpha = Math.round(228 * edgeAlpha)
        }

        pixels[pixelIndex] = red
        pixels[pixelIndex + 1] = green
        pixels[pixelIndex + 2] = blue
        pixels[pixelIndex + 3] = alpha
      }
    }

    context.clearRect(0, 0, renderSize, renderSize)
    context.putImageData(imageData, 0, 0)

    context.beginPath()
    context.arc(center, center, radius - 0.5, 0, 2 * Math.PI)
    context.strokeStyle = 'rgba(220, 230, 255, 0.24)'
    context.lineWidth = Math.max(1, renderSize * 0.04)
    context.stroke()
  }

  function moonPhaseCanvas(node: HTMLCanvasElement, params: MoonCanvasParams) {
    let lastSignature = ''

    const render = (next: MoonCanvasParams) => {
      const signature = `${next.sizePx}:${next.isWaxing ? 1 : 0}:${next.illumination.toFixed(4)}`
      if (signature === lastSignature) return
      lastSignature = signature
      drawMoonToCanvas(node, next)
    }

    render(params)

    return {
      update(next: MoonCanvasParams) {
        render(next)
      },
    }
  }

  function getMoonVisualState(
    moon: MoonDefinition,
    hour: number,
    absoluteDayIndex: number,
    isDaylight: boolean
  ): MoonVisualState {
    const cycleDay =
      positiveModulo(absoluteDayIndex + moon.phaseOffsetDays, moon.periodDays) + 1
    const fullMoonDay = moon.periodDays / 2
    const illumination = Math.max(
      0,
      Math.min(
        1,
        getMoonIllumination(cycleDay, fullMoonDay, moon.periodDays)
      )
    )
    const isWaxing = cycleDay <= fullMoonDay
    const phaseLabel = getMoonPhaseLabel(illumination, isWaxing)
    const orbitalProgress = isWaxing
      ? ((cycleDay - 1) / Math.max(1, fullMoonDay - 1)) * 0.5
      : 0.5 +
        ((cycleDay - fullMoonDay) / Math.max(1, moon.periodDays - fullMoonDay)) *
          0.5

    // New moon aligns with the sun (transit around noon), full moon transits at midnight.
    const transitHour = normalizeHour(12 + orbitalProgress * HOURS_PER_DAY)
    const riseHour = normalizeHour(transitHour - 6)
    const normalizedHour = normalizeHour(hour)
    const hoursSinceRise = normalizeHour(normalizedHour - riseHour)
    const isAboveHorizon = hoursSinceRise <= 12
    const nightArcProgress = Math.min(1, Math.max(0, hoursSinceRise / 12))
    const arc = 1 - Math.pow(nightArcProgress * 2 - 1, 2)

    const xPercent =
      SUN_LEFT_MARGIN_PERCENT +
      nightArcProgress * (SUN_RIGHT_MARGIN_PERCENT - SUN_LEFT_MARGIN_PERCENT)
    const yPercent = HORIZON_Y_PERCENT - arc * MOON_ARC_HEIGHT_PERCENT
    const daylightVisibilityScale = isDaylight ? 0.45 : 1
    const opacity = Math.min(1, Math.max(0, illumination * daylightVisibilityScale))
    const isVisible = isAboveHorizon && opacity > 0.02

    return {
      id: moon.id,
      displayName: moon.displayName,
      alias: moon.alias,
      cycleDay,
      periodDays: moon.periodDays,
      phaseLabel,
      illumination,
      isWaxing,
      xPercent,
      yPercent,
      sizePx: moon.sizePx,
      hueRotateDeg: moon.hueRotateDeg,
      saturation: moon.saturation,
      isVisible,
      opacity,
    }
  }

  function formatGameDate() {
    const monthName =
      MONTH_NAMES[currentGameDate.month - 1] ?? `Month ${currentGameDate.month}`
    const day = currentGameDate.day.toString().padStart(2, '0')
    return `${currentGameDate.year} ${monthName} ${day}`
  }

  function getSunVisualState(hour: number) {
    const normalizedHour = normalizeHour(hour)
    const clampedHour = Math.min(
      SUNSET_HOUR,
      Math.max(SUNRISE_HOUR, normalizedHour)
    )
    const progress = (clampedHour - SUNRISE_HOUR) / DAYLIGHT_HOURS
    const arc = 1 - Math.pow(progress * 2 - 1, 2)

    return {
      xPercent:
        SUN_LEFT_MARGIN_PERCENT +
        progress * (SUN_RIGHT_MARGIN_PERCENT - SUN_LEFT_MARGIN_PERCENT),
      yPercent: HORIZON_Y_PERCENT - arc * SUN_ARC_HEIGHT_PERCENT,
      isDaylight: normalizedHour >= SUNRISE_HOUR && normalizedHour <= SUNSET_HOUR,
      isSunsetWindow:
        Math.abs(normalizedHour - SUNRISE_HOUR) <= SUNSET_WINDOW_HOURS ||
        Math.abs(normalizedHour - SUNSET_HOUR) <= SUNSET_WINDOW_HOURS,
    }
  }

  const sunVisual = $derived(getSunVisualState(currentGameHour))
  const absoluteDayIndex = $derived(getAbsoluteDayIndex())
  const moonVisuals = $derived(
    MOONS.map((moon) =>
      getMoonVisualState(moon, currentGameHour, absoluteDayIndex, sunVisual.isDaylight)
    )
  )
</script>

<div class="time-widget">
  <div class="meta">
    <span class="date">{formatGameDate()}</span>
  </div>
  <div class="sky-track">
    <img
      class="horizon"
      src={
        sunVisual.isSunsetWindow
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
      {#if moon.isVisible}
        <canvas
          class="moon"
          aria-label={`${moon.displayName} Moon`}
          use:moonPhaseCanvas={{
            illumination: moon.illumination,
            isWaxing: moon.isWaxing,
            sizePx: moon.sizePx,
          }}
          style={`--moon-x:${moon.xPercent}%; --moon-y:${moon.yPercent}%; --moon-size:${moon.sizePx}px; --moon-opacity:${moon.opacity}; --moon-hue:${moon.hueRotateDeg}deg; --moon-saturation:${moon.saturation};`}
        ></canvas>
      {/if}
    {/each}
    <img
      class="horizon-front"
      src={
        sunVisual.isSunsetWindow
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
    padding: 10px;
    font-family: 'Courier New', monospace;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: min(360px, calc(100vw - 20px));
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
    object-fit: cover;
    object-position: center bottom;
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
    transition:
      left 220ms linear,
      top 220ms linear;
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
      drop-shadow(0 0 4px rgba(215, 228, 255, 0.65));
    transition:
      left 220ms linear,
      top 220ms linear,
      opacity 220ms linear;
    z-index: 2;
  }

  .horizon-front {
    position: absolute;
    left: 0;
    bottom: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center bottom;
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
</style>

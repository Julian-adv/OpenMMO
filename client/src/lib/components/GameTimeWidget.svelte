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
  const SUN_LEFT_MARGIN_PERCENT = 0
  const SUN_RIGHT_MARGIN_PERCENT = 100
  const HORIZON_Y_PERCENT = 70
  const SUN_ARC_HEIGHT_PERCENT = 68
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

  function formatGameDate() {
    const monthName =
      MONTH_NAMES[currentGameDate.month - 1] ?? `Month ${currentGameDate.month}`
    const day = currentGameDate.day.toString().padStart(2, '0')
    return `${currentGameDate.year} ${monthName} ${day}`
  }

  function getSunVisualState(hour: number) {
    const normalizedHour = ((hour % 24) + 24) % 24
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
</script>

<div class="time-widget">
  <span class="date">{formatGameDate()}</span>
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
    align-items: center;
    gap: 10px;
    width: 360px;
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

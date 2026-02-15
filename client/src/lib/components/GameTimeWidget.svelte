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

  function formatGameTime(hour: number) {
    const totalMinutes = Math.floor(hour * 60)
    const hh = Math.floor(totalMinutes / 60)
    const mm = totalMinutes % 60
    return `${hh.toString().padStart(2, '0')}:${mm.toString().padStart(2, '0')}`
  }

  function formatGameDate() {
    const monthName =
      MONTH_NAMES[currentGameDate.month - 1] ?? `Month ${currentGameDate.month}`
    return `${currentGameDate.year} ${monthName} ${currentGameDate.day.toString().padStart(2, '0')}`
  }
</script>

<div class="time-widget">
  <span class="date">{formatGameDate()}</span>
  <span class="time">{formatGameTime(currentGameHour)}</span>
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
    border: 1px solid rgba(247, 241, 208, 0.35);
    border-radius: 6px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.45);
    padding: 8px 12px;
    font-family: 'Courier New', monospace;
    display: flex;
    flex-direction: column;
    gap: 2px;
    align-items: flex-end;
    min-width: 100px;
  }

  .time {
    font-size: 18px;
    font-weight: 700;
    line-height: 1;
  }

  .date {
    font-size: 12px;
    opacity: 0.9;
    line-height: 1;
  }
</style>

export function createChartSelection(timeAtPointer: (event: MouseEvent) => number | null, defaultTime: () => number | null, period: () => string | number, clearDetail?: () => void) {
  let time = $state<number | null>(null)
  let pinned = $state(false)
  const activePeriod = $derived.by(period)

  function clear() {
    time = null
    pinned = false
    clearDetail?.()
  }

  function leave() {
    if (!pinned) clear()
  }

  function toggle(timestamp: number | null) {
    time = timestamp
    pinned = !pinned && time !== null
  }

  $effect(() => {
    void activePeriod
    clear()
  })

  return {
    get time() { return time },
    get pinned() { return pinned },
    get hint() { return pinned ? '고정됨 · 다시 클릭하거나 Esc로 해제' : '클릭하면 이 시점에 고정' },
    handlers: {
      onpointermove(event: PointerEvent) {
        if (!pinned) time = timeAtPointer(event)
      },
      onpointerleave: leave,
      onclick(event: MouseEvent) {
        toggle(timeAtPointer(event))
      },
      onfocus() {
        if (time === null) time = defaultTime()
      },
      onblur: leave,
      onkeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') clear()
        else if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault()
          toggle(time ?? defaultTime())
        }
      },
    },
  }
}

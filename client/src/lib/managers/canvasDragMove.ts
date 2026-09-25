const DRAG_DISTANCE = 4
const UPDATE_INTERVAL_MS = 100

export function setupCanvasDragMove(
  canvas: HTMLCanvasElement,
  onClick: (event: MouseEvent) => boolean | void,
  onDragMove: (event: MouseEvent) => void
): () => void {
  let start: MouseEvent | null = null
  let dragging = false
  let pending: MouseEvent | null = null
  let timer: ReturnType<typeof setTimeout> | null = null
  let lastUpdate = -Infinity

  const overCanvas = (event: MouseEvent) =>
    document.elementFromPoint(event.clientX, event.clientY) === canvas

  function clearPending() {
    if (timer !== null) clearTimeout(timer)
    timer = null
    pending = null
  }

  function cancel() {
    clearPending()
    start = null
    dragging = false
    lastUpdate = -Infinity
  }

  function flush() {
    const event = pending
    clearPending()
    if (!event || !overCanvas(event)) return
    lastUpdate = performance.now()
    onDragMove(event)
  }

  function onDown(event: MouseEvent) {
    cancel()
    const canDrag = onClick(event)
    if (event.button === 0 && canDrag) start = event
  }

  function onMove(event: MouseEvent) {
    if (!start) return
    if (event.buttons !== 1) {
      cancel()
      return
    }
    if (!overCanvas(event)) {
      clearPending()
      return
    }
    if (!dragging) {
      if (
        Math.hypot(
          event.clientX - start.clientX,
          event.clientY - start.clientY
        ) < DRAG_DISTANCE
      )
        return
      dragging = true
    }
    pending = event
    const delay = UPDATE_INTERVAL_MS - (performance.now() - lastUpdate)
    if (delay <= 0) flush()
    else if (timer === null) timer = setTimeout(flush, delay)
  }

  function onUp(event: MouseEvent) {
    if (event.button !== 0) return
    if (dragging) {
      pending = event
      flush()
    }
    cancel()
  }

  function onVisibilityChange() {
    if (document.hidden) cancel()
  }

  canvas.addEventListener('mousedown', onDown)
  // OrbitControls can capture the pointer on the canvas wrapper.
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp, true)
  window.addEventListener('pointercancel', cancel, true)
  window.addEventListener('blur', cancel)
  document.addEventListener('visibilitychange', onVisibilityChange)

  return () => {
    cancel()
    canvas.removeEventListener('mousedown', onDown)
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp, true)
    window.removeEventListener('pointercancel', cancel, true)
    window.removeEventListener('blur', cancel)
    document.removeEventListener('visibilitychange', onVisibilityChange)
  }
}

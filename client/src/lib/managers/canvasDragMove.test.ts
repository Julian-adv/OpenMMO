import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { setupCanvasDragMove } from './canvasDragMove'

describe('canvas drag movement', () => {
  let canvas: HTMLCanvasElement
  let onClick: ReturnType<typeof vi.fn<() => boolean>>
  let onDragMove: ReturnType<typeof vi.fn<(event: MouseEvent) => void>>
  let elementFromPoint: ReturnType<typeof vi.fn>
  let cleanup: () => void

  function mouse(
    type: string,
    x = 50,
    y = 50,
    options: { button?: number; buttons?: number; shiftKey?: boolean } = {}
  ) {
    const event = Object.assign(new Event(type), {
      clientX: x,
      clientY: y,
      button: 0,
      buttons: type === 'mouseup' ? 0 : 1,
      shiftKey: false,
      ...options,
    })
    ;(type === 'mousedown' ? canvas : window).dispatchEvent(event)
    return event
  }

  beforeEach(() => {
    vi.useFakeTimers()
    canvas = new EventTarget() as HTMLCanvasElement
    elementFromPoint = vi.fn(() => canvas)
    vi.stubGlobal('window', new EventTarget())
    vi.stubGlobal(
      'document',
      Object.assign(new EventTarget(), {
        elementFromPoint,
        hidden: false,
      })
    )
    onClick = vi.fn(() => true)
    onDragMove = vi.fn()
    cleanup = setupCanvasDragMove(canvas, onClick, onDragMove)
  })

  afterEach(() => {
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it('keeps ordinary clicks immediate without retargeting for small movements', () => {
    const click = mouse('mousedown')
    expect(onClick).toHaveBeenCalledExactlyOnceWith(click)
    mouse('mousemove', 51, 51)
    mouse('mouseup', 51, 51)
    vi.runAllTimers()
    expect(onDragMove).not.toHaveBeenCalled()
  })

  it('follows a drag even when pointer capture retargets events to the wrapper', () => {
    mouse('mousedown')
    const first = mouse('mousemove', 60)
    expect(first.target).toBe(window)
    expect(onDragMove).toHaveBeenCalledExactlyOnceWith(first)
    mouse('mousemove', 70)
    const latest = mouse('mousemove', 80, 60, { shiftKey: true })
    expect(onDragMove).toHaveBeenCalledOnce()
    vi.advanceTimersByTime(100)
    expect(onDragMove).toHaveBeenCalledTimes(2)
    expect(onDragMove).toHaveBeenLastCalledWith(latest)
    vi.advanceTimersByTime(1000)
    expect(onDragMove).toHaveBeenCalledTimes(2)
  })

  it('uses the release point immediately and stops tracking after release', () => {
    mouse('mousedown')
    mouse('mousemove', 60)
    mouse('mousemove', 70)
    const release = mouse('mouseup', 85)
    expect(onDragMove).toHaveBeenCalledTimes(2)
    expect(onDragMove).toHaveBeenLastCalledWith(release)
    mouse('mousemove', 100, 50, { buttons: 0 })
    vi.runAllTimers()
    expect(onDragMove).toHaveBeenCalledTimes(2)
  })

  it('does not drag when the initial click handled an interaction or editor action', () => {
    onClick.mockReturnValue(false)
    mouse('mousedown')
    mouse('mousemove', 80)
    mouse('mouseup', 80)
    expect(onClick).toHaveBeenCalledOnce()
    expect(onDragMove).not.toHaveBeenCalled()
  })

  it.each([1, 2])(
    'preserves button %i clicks without starting movement',
    (button) => {
      mouse('mousedown', 50, 50, { button })
      mouse('mousemove', 80)
      mouse('mouseup', 80)
      expect(onClick).toHaveBeenCalledOnce()
      expect(onDragMove).not.toHaveBeenCalled()
    }
  )

  it('does not start a drag from outside the canvas', () => {
    mouse('mousemove', 80)
    mouse('mouseup', 80)
    expect(onDragMove).not.toHaveBeenCalled()
  })

  it('pauses over UI and resumes when the held pointer returns to the canvas', () => {
    mouse('mousedown')
    mouse('mousemove', 60)
    mouse('mousemove', 70)
    elementFromPoint.mockReturnValue(null)
    mouse('mousemove', 80)
    vi.advanceTimersByTime(100)
    expect(onDragMove).toHaveBeenCalledOnce()
    elementFromPoint.mockReturnValue(canvas)
    const resumed = mouse('mousemove', 90)
    expect(onDragMove).toHaveBeenCalledTimes(2)
    expect(onDragMove).toHaveBeenLastCalledWith(resumed)
  })

  it('drops pending updates when UI covers the canvas before the timer fires', () => {
    mouse('mousedown')
    mouse('mousemove', 60)
    mouse('mousemove', 70)
    elementFromPoint.mockReturnValue(null)
    vi.advanceTimersByTime(100)
    mouse('mouseup', 80)
    elementFromPoint.mockReturnValue(canvas)
    mouse('mousemove', 90)
    expect(onDragMove).toHaveBeenCalledOnce()
  })

  it.each(['blur', 'pointercancel', 'hidden', 'lost button', 'cleanup'])(
    'discards pending movement on %s',
    (reason) => {
      mouse('mousedown')
      mouse('mousemove', 60)
      mouse('mousemove', 70)
      if (reason === 'hidden') {
        Object.assign(document, { hidden: true })
        document.dispatchEvent(new Event('visibilitychange'))
      } else if (reason === 'lost button') {
        mouse('mousemove', 80, 50, { buttons: 0 })
      } else if (reason === 'cleanup') {
        cleanup()
      } else {
        window.dispatchEvent(new Event(reason))
      }
      vi.runAllTimers()
      mouse('mousemove', 90)
      mouse('mouseup', 90)
      expect(onDragMove).toHaveBeenCalledOnce()
    }
  )
})

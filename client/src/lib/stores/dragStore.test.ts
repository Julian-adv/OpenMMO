import { get } from 'svelte/store'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { dragMeta, startDrag, type DragMeta } from './dragStore'
import { assignQuickslot } from './quickslotStore'

vi.mock('./quickslotStore', () => ({ assignQuickslot: vi.fn() }))

class Rect {
  constructor(
    public x: number,
    public y: number,
    public width: number,
    public height: number
  ) {}
  get left() {
    return this.x
  }
  get right() {
    return this.x + this.width
  }
  get top() {
    return this.y
  }
  get bottom() {
    return this.y + this.height
  }
}

class DragTarget extends EventTarget {
  captured = false
  setPointerCapture() {
    this.captured = true
  }
  hasPointerCapture() {
    return this.captured
  }
  releasePointerCapture() {
    this.captured = false
  }
}

const skill: DragMeta = {
  skill: 'guardian_ward',
  source: { type: 'skill' },
  icon: '/icons/skills/dagger-double-slash-v2.png',
}

const item: DragMeta = {
  instanceId: 9,
  defId: 'dagger',
  enchant: 9,
  equipSlot: 'main_hand',
  source: { type: 'bag' },
  icon: 'weapons/dagger.png',
}

let target: DragTarget
let cancel: (() => void) | undefined

function begin(meta: DragMeta, onDrop = vi.fn(), onClick = vi.fn()) {
  cancel = startDrag(
    {
      currentTarget: target,
      pointerId: 1,
      clientX: 20,
      clientY: 20,
    } as unknown as PointerEvent,
    meta,
    onDrop,
    onClick
  )
  return { onDrop, onClick }
}

function pointer(type: string, x: number, y: number, pointerId = 1) {
  target.dispatchEvent(
    Object.assign(new Event(type, { cancelable: true }), {
      pointerId,
      clientX: x,
      clientY: y,
    })
  )
}

beforeEach(() => {
  vi.clearAllMocks()
  dragMeta.set(null)
  target = new DragTarget()
  vi.stubGlobal('window', new EventTarget())
  vi.stubGlobal('DOMRect', Rect)
  const slots = [0, 1].map((index) => ({
    dataset: { quickslot: String(index) },
    getBoundingClientRect: () => new Rect(400 + index * 50, 500, 40, 40),
  }))
  const bar = {
    getBoundingClientRect: () => new Rect(400, 500, 90, 40),
    querySelectorAll: () => slots,
  }
  vi.stubGlobal('document', { querySelector: () => ({ parentElement: bar }) })
})

afterEach(() => {
  cancel?.()
  vi.unstubAllGlobals()
})

describe('shared item and skill drag', () => {
  it.each([item, skill])(
    'binds either kind to the same nearest quickslot',
    (meta) => {
      const { onDrop, onClick } = begin(meta)
      pointer('pointermove', 460, 515)
      expect(get(dragMeta)).toEqual(meta)
      pointer('pointerup', 460, 515)
      expect(assignQuickslot).toHaveBeenCalledWith(
        1,
        'skill' in meta
          ? { skill: meta.skill }
          : { defId: meta.defId, enchant: 9 }
      )
      expect(onDrop).not.toHaveBeenCalled()
      expect(onClick).not.toHaveBeenCalled()
      expect(get(dragMeta)).toBeNull()
      expect(target.captured).toBe(false)
    }
  )

  it('does not bind a click below the drag threshold', () => {
    const { onClick } = begin(skill)
    pointer('pointermove', 23, 22)
    pointer('pointerup', 23, 22)
    expect(assignQuickslot).not.toHaveBeenCalled()
    expect(onClick).toHaveBeenCalledOnce()
  })

  it('keeps item-specific drop handling outside the quickslots', () => {
    const { onDrop } = begin(item)
    pointer('pointermove', 100, 100)
    pointer('pointerup', 100, 100)
    expect(onDrop).toHaveBeenCalledWith(100, 100)
    expect(assignQuickslot).not.toHaveBeenCalled()
  })

  it('leaves a skill unbound when released outside the bar', () => {
    begin(skill)
    pointer('pointermove', 100, 100)
    pointer('pointerup', 100, 100)
    expect(assignQuickslot).not.toHaveBeenCalled()
    expect(get(dragMeta)).toBeNull()
  })

  it.each(['pointercancel', 'lostpointercapture', 'escape', 'blur'])(
    'cancels without assigning on %s',
    (reason) => {
      begin(skill)
      pointer('pointermove', 460, 515)
      if (reason === 'escape') {
        window.dispatchEvent(
          Object.assign(new Event('keydown'), { key: 'Escape' })
        )
      } else if (reason === 'blur') {
        window.dispatchEvent(new Event('blur'))
      } else {
        pointer(reason, 460, 515)
      }
      pointer('pointerup', 460, 515)
      expect(assignQuickslot).not.toHaveBeenCalled()
      expect(get(dragMeta)).toBeNull()
      expect(target.captured).toBe(false)
    }
  )

  it('ignores another pointer during the captured gesture', () => {
    begin(skill)
    pointer('pointermove', 460, 515, 2)
    pointer('pointerup', 460, 515, 2)
    expect(get(dragMeta)).toBeNull()
    expect(target.captured).toBe(true)
    expect(assignQuickslot).not.toHaveBeenCalled()
  })

  it('does not clear a later drag when a finished source is destroyed', () => {
    begin(skill)
    pointer('pointermove', 100, 100)
    pointer('pointerup', 100, 100)
    dragMeta.set(item)
    cancel?.()
    expect(get(dragMeta)).toEqual(item)
  })

  it('does not assign a multi-item selection to one quickslot', () => {
    begin({ ...item, groupItems: [{ icon: item.icon, quantity: 2 }] })
    pointer('pointermove', 460, 515)
    pointer('pointerup', 460, 515)
    expect(assignQuickslot).not.toHaveBeenCalled()
  })
})

import { writable } from 'svelte/store'
import type { EquipSlot } from '../network/networkTypes'

export const FALLBACK_ICON = 'icon_frame.png'

export type DragMeta = {
  instanceId: number
  equipSlot: EquipSlot | null
  source: { type: 'bag' } | { type: 'equipped'; slot: EquipSlot }
  icon: string
}

export const dragMeta = writable<DragMeta | null>(null)
export const dragPos = writable({ x: 0, y: 0 })

export function isSlotCompatible(
  itemSlot: EquipSlot | null,
  targetSlot: EquipSlot
): boolean {
  if (!itemSlot) return false
  if (itemSlot === targetSlot) return true
  if (itemSlot === 'ring' && targetSlot === 'ring_left') return true
  return false
}

export function pointInRect(x: number, y: number, r: DOMRect): boolean {
  return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom
}

/** Grow a rect outward by `m` px on every side (for forgiving hit-testing). */
export function inflateRect(r: DOMRect, m: number): DOMRect {
  return new DOMRect(r.x - m, r.y - m, r.width + 2 * m, r.height + 2 * m)
}

export function isOverAnyDialog(x: number, y: number): boolean {
  for (const dialog of document.querySelectorAll('[role="dialog"]')) {
    if (pointInRect(x, y, dialog.getBoundingClientRect())) return true
  }
  return false
}

const DRAG_THRESHOLD_SQ = 64

export function startDrag(
  e: PointerEvent,
  meta: DragMeta,
  onDrop: (x: number, y: number) => void
) {
  const target = e.currentTarget as HTMLElement
  target.setPointerCapture(e.pointerId)
  const startX = e.clientX
  const startY = e.clientY
  let started = false
  const pos = { x: 0, y: 0 }

  function onMove(me: PointerEvent) {
    me.preventDefault()
    const dx = me.clientX - startX
    const dy = me.clientY - startY
    if (!started && dx * dx + dy * dy < DRAG_THRESHOLD_SQ) return
    if (!started) {
      started = true
      dragMeta.set(meta)
    }
    pos.x = me.clientX
    pos.y = me.clientY
    dragPos.set(pos)
  }

  function removeListeners() {
    target.removeEventListener('pointermove', onMove)
    target.removeEventListener('pointerup', onEnd)
    target.removeEventListener('pointercancel', onEnd)
    target.removeEventListener('lostpointercapture', onLostCapture)
  }

  function onEnd(ue: PointerEvent) {
    removeListeners()
    if (target.hasPointerCapture(ue.pointerId)) {
      target.releasePointerCapture(ue.pointerId)
    }
    if (started && ue.type !== 'pointercancel') {
      onDrop(ue.clientX, ue.clientY)
    }
    dragMeta.set(null)
  }

  function onLostCapture() {
    removeListeners()
    dragMeta.set(null)
  }

  target.addEventListener('pointermove', onMove)
  target.addEventListener('pointerup', onEnd)
  target.addEventListener('pointercancel', onEnd)
  target.addEventListener('lostpointercapture', onLostCapture)
}

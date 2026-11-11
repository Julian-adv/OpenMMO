import { mount, unmount, type Component } from 'svelte'
import { get } from 'svelte/store'
import { dragMeta } from '../stores/dragStore'

let nextId = 0

/** A hover/focus tooltip action: mounts `component` on the body with the
 *  props `toProps` builds plus `id` and the node's `anchor` rect. Hidden by
 *  Escape, a press, a drag, and any scroll or resize while shown. */
export function createTooltipAction<T>(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  component: Component<any>,
  toProps: (params: T) => Record<string, unknown>
) {
  return (node: HTMLElement, initial: T | null | undefined) => {
    const id = `tooltip-${nextId++}`
    let params = initial
    let instance: object | null = null

    function show() {
      if (!params || instance || get(dragMeta)) return
      instance = mount(component, {
        target: document.body,
        props: { ...toProps(params), id, anchor: node.getBoundingClientRect() },
      })
      node.setAttribute('aria-describedby', id)
      window.addEventListener('scroll', hide, true)
      window.addEventListener('resize', hide)
    }

    function hide() {
      if (!instance) return
      node.removeAttribute('aria-describedby')
      unmount(instance)
      instance = null
      window.removeEventListener('scroll', hide, true)
      window.removeEventListener('resize', hide)
    }

    function keydown(event: KeyboardEvent) {
      if (event.key === 'Escape') hide()
    }

    const unsubDrag = dragMeta.subscribe((value) => {
      if (value) hide()
    })
    node.addEventListener('mouseenter', show)
    node.addEventListener('mouseleave', hide)
    node.addEventListener('focus', show)
    node.addEventListener('blur', hide)
    node.addEventListener('pointerdown', hide)
    node.addEventListener('keydown', keydown)

    return {
      update(next: T | null | undefined) {
        if (next === params) return
        const wasVisible = instance !== null
        hide()
        params = next
        if (wasVisible) show()
      },
      destroy() {
        hide()
        unsubDrag()
        node.removeEventListener('mouseenter', show)
        node.removeEventListener('mouseleave', hide)
        node.removeEventListener('focus', show)
        node.removeEventListener('blur', hide)
        node.removeEventListener('pointerdown', hide)
        node.removeEventListener('keydown', keydown)
      },
    }
  }
}

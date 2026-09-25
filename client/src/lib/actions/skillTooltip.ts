import { mount, unmount } from 'svelte'
import { get } from 'svelte/store'
import SkillTooltip from '../components/SkillTooltip.svelte'
import { dragMeta } from '../stores/dragStore'

export interface SkillTooltipParams {
  name: string
  description: string
  stats?: readonly { label: string; value: string }[]
  details?: readonly string[]
}

let nextId = 0

export function skillTooltip(
  node: HTMLElement,
  params: SkillTooltipParams | null
) {
  const id = `skill-tooltip-${nextId++}`
  let instance: object | null = null

  function show() {
    if (!params || instance || get(dragMeta)) return
    instance = mount(SkillTooltip, {
      target: document.body,
      props: { ...params, id, anchor: node.getBoundingClientRect() },
    })
    node.setAttribute('aria-describedby', id)
  }

  function hide() {
    node.removeAttribute('aria-describedby')
    if (instance) {
      unmount(instance)
      instance = null
    }
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
  window.addEventListener('scroll', hide, true)
  window.addEventListener('resize', hide)

  return {
    update(next: SkillTooltipParams | null) {
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
      window.removeEventListener('scroll', hide, true)
      window.removeEventListener('resize', hide)
    },
  }
}

<script lang="ts">
  import DamageTextItem from './DamageTextItem.svelte'
  import * as THREE from 'three'
  import type { PlayerDamageInfo, PlayerGoldInfo } from '../stores/gameStore'

  interface Props {
    lastDamageInfo?: PlayerDamageInfo
    lastRegenInfo?: PlayerDamageInfo
    lastGoldInfo?: PlayerGoldInfo
    startYOffset?: number
  }

  let {
    lastDamageInfo,
    lastRegenInfo,
    lastGoldInfo,
    startYOffset = 1.8,
  }: Props = $props()

  interface FloatingText {
    id: number
    text: string
    color: string
  }

  let floatingTexts = $state<FloatingText[]>([])
  let nextTextId = 0
  let lastDamageTrigger = $state(0)
  let lastRegenTrigger = $state(0)
  let lastGoldTrigger = $state(0)
  let itemRefs = $state<
    (ReturnType<typeof DamageTextItem> & {
      update: (
        deltaTime: number,
        baseX: number,
        baseY: number,
        baseZ: number,
        camera: THREE.Camera
      ) => void
      isAlive: () => boolean
    })[]
  >([])

  // Spawn a floating text when an info source's trigger advances past the last
  // one we rendered, and return the new trigger to record. Shared by every
  // floating-text source (damage, regen, gold) so each is a single call.
  function emitIfTriggered<T extends { trigger: number }>(
    info: T | undefined,
    lastTrigger: number,
    render: (info: T) => { text: string; color: string }
  ): number {
    if (info && info.trigger !== lastTrigger) {
      const { text, color } = render(info)
      floatingTexts = [...floatingTexts, { id: nextTextId++, text, color }]
      return info.trigger
    }
    return lastTrigger
  }

  export function update(
    deltaTime: number,
    baseX: number,
    baseY: number,
    baseZ: number,
    camera: THREE.Camera
  ) {
    // 1. Spawn a floating text for each source whose trigger advanced.
    lastDamageTrigger = emitIfTriggered(
      lastDamageInfo,
      lastDamageTrigger,
      (i) => ({
        text: i.hit ? `${i.damage}` : 'Miss',
        color: i.hit ? '#ff4d4d' : '#a0aec0',
      })
    )
    lastRegenTrigger = emitIfTriggered(
      lastRegenInfo,
      lastRegenTrigger,
      (i) => ({
        text: `+${i.damage}`,
        color: '#48bb78', // Green
      })
    )
    lastGoldTrigger = emitIfTriggered(lastGoldInfo, lastGoldTrigger, (i) => ({
      text: `+${i.amount} copper`,
      color: '#f6c453',
    }))

    // 2. Update existing items
    if (floatingTexts.length > 0) {
      for (const ref of itemRefs) {
        ref?.update(deltaTime, baseX, baseY, baseZ, camera)
      }

      // Filter out dead items
      if (itemRefs.some((ref) => ref && !ref.isAlive())) {
        const remainingTexts: FloatingText[] = []
        floatingTexts.forEach((text, index) => {
          if (itemRefs[index]?.isAlive() !== false) {
            remainingTexts.push(text)
          }
        })
        floatingTexts = remainingTexts
      }
    }
  }
</script>

{#each floatingTexts as text, index (text.id)}
  <DamageTextItem
    bind:this={itemRefs[index]}
    text={text.text}
    color={text.color}
    {startYOffset}
  />
{/each}

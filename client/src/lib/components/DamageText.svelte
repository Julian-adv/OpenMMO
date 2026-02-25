<script lang="ts">
  import DamageTextItem from './DamageTextItem.svelte'
  import * as THREE from 'three'
  import type { PlayerDamageInfo } from '../stores/gameStore'

  interface Props {
    lastDamageInfo?: PlayerDamageInfo
    lastRegenInfo?: PlayerDamageInfo
  }

  let { lastDamageInfo, lastRegenInfo }: Props = $props()

  interface FloatingText {
    id: number
    text: string
    color: string
  }

  let floatingTexts = $state<FloatingText[]>([])
  let nextTextId = 0
  let lastDamageTrigger = $state(0)
  let lastRegenTrigger = $state(0)
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

  export function update(
    deltaTime: number,
    baseX: number,
    baseY: number,
    baseZ: number,
    camera: THREE.Camera
  ) {
    // 1. Check for new damage
    if (lastDamageInfo && lastDamageInfo.trigger !== lastDamageTrigger) {
      lastDamageTrigger = lastDamageInfo.trigger

      const text = lastDamageInfo.hit ? `${lastDamageInfo.damage}` : 'Miss'
      const color = lastDamageInfo.hit ? '#ff4d4d' : '#a0aec0'

      floatingTexts = [...floatingTexts, { id: nextTextId++, text, color }]
    }

    // 2. Check for new regen
    if (lastRegenInfo && lastRegenInfo.trigger !== lastRegenTrigger) {
      lastRegenTrigger = lastRegenInfo.trigger

      const text = `+${lastRegenInfo.damage}`
      const color = '#48bb78' // Green

      floatingTexts = [...floatingTexts, { id: nextTextId++, text, color }]
    }

    // 3. Update existing items
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
  />
{/each}

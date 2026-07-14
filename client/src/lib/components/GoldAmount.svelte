<script lang="ts">
  import { splitGold } from '../utils/currency'

  const { copper }: { copper: number } = $props()

  const display = $derived.by(() => {
    const p = splitGold(copper)
    const segments: { cls: string; text: string }[] = []
    if (p.gold > 0) segments.push({ cls: 'gold', text: `${p.gold}g` })
    if (p.silver > 0) segments.push({ cls: 'silver', text: `${p.silver}s` })
    if (p.copper > 0 || segments.length === 0) {
      segments.push({ cls: 'copper', text: `${p.copper}c` })
    }
    return { negative: p.negative, segments }
  })
</script>

<span class="gold-amount"
  >{display.negative ? '-' : ''}{#each display.segments as seg, i (seg.cls)}{i >
    0
      ? ' '
      : ''}<span class={seg.cls}>{seg.text}</span>{/each}</span
>

<style>
  .gold-amount {
    white-space: nowrap;
  }

  .gold {
    color: #ffd700;
  }

  .silver {
    color: #c8d1dc;
  }

  .copper {
    color: #c9824f;
  }
</style>

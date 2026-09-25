<script lang="ts">
  import { T } from '@threlte/core'
  import { onDestroy } from 'svelte'
  import {
    fitLabelCanvas,
    measureCtx,
    releaseLabelCanvas,
    type LabelCanvas,
  } from '../../utils/text-label-pool'
  import {
    drawOutlinedText,
    STALL_SIGN_BADGE_STYLE,
  } from '../../utils/textBadge'

  let { text, topY }: { text: string; topY: number } = $props()

  const style = STALL_SIGN_BADGE_STYLE
  const NO_RAYCAST = () => {}
  let label: LabelCanvas | null = null
  let board = $state<{
    texture: LabelCanvas['texture']
    width: number
    height: number
  } | null>(null)

  $effect(() => {
    const font = `${style.bold ? 'bold ' : ''}${style.fontPx}px sans-serif`
    measureCtx.font = font
    const pad = Math.ceil(style.outlineWidth) + 4
    const width = Math.ceil(measureCtx.measureText(text).width) + pad * 2
    const height = Math.ceil(style.fontPx * 1.25) + pad * 2
    label = fitLabelCanvas(label, width, height)
    const { canvas, ctx, texture } = label
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    ctx.font = font
    drawOutlinedText(ctx, text, width / 2, height / 2, style)
    texture.needsUpdate = true
    const scale = Math.min(1 / style.pixelsPerUnit, 4 / width)
    board = { texture, width: width * scale, height: height * scale }
  })

  onDestroy(() => {
    if (label) releaseLabelCanvas(label)
  })
</script>

{#if board}
  <T.Sprite
    position.y={topY + 0.4}
    scale={[board.width, board.height, 1]}
    renderOrder={4}
    raycast={NO_RAYCAST}
  >
    <T.SpriteMaterial
      map={board.texture}
      transparent={true}
      depthWrite={false}
    />
  </T.Sprite>
{/if}

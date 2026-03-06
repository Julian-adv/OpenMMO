<script lang="ts">
  import { T } from '@threlte/core'
  import * as THREE from 'three'
  import { MeshBasicNodeMaterial } from 'three/webgpu'
  import { onDestroy } from 'svelte'

  interface Props {
    text: string
    fontSize?: number
    color?: string
    anchorX?: 'left' | 'center' | 'right'
    anchorY?: 'top' | 'middle' | 'bottom'
    fillOpacity?: number
    maxWidth?: number
    overflowWrap?: 'normal' | 'break-word'
    whiteSpace?: 'normal' | 'nowrap'
    depthOffset?: number
    onsync?: () => void
    position?: [number, number, number]
    'position.y'?: number
  }

  let {
    text,
    fontSize = 0.3,
    color = '#ffffff',
    anchorX = 'center',
    anchorY = 'middle',
    fillOpacity = 1.0,
    maxWidth,
    overflowWrap = 'normal',
    whiteSpace = 'normal',
    depthOffset,
    onsync,
    position = [0, 0, 0],
    'position.y': positionY,
  }: Props = $props()

  const PIXELS_PER_UNIT = 256

  // Exported for ChatBubble compatibility (bind:this → ref.textRenderInfo.blockBounds)
  export const textRenderInfo = { blockBounds: [0, 0, 0, 0] as number[] }

  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')!
  const texture = new THREE.CanvasTexture(canvas)
  texture.minFilter = THREE.LinearFilter
  texture.magFilter = THREE.LinearFilter

  let worldWidth = $state(0.01)
  let worldHeight = $state(0.01)
  let anchorOffsetX = $state(0)
  let anchorOffsetY = $state(0)

  const material = new MeshBasicNodeMaterial()
  material.map = texture
  material.transparent = true
  material.depthWrite = false
  material.side = THREE.DoubleSide

  function wrapText(
    inputText: string,
    maxWidthPx: number | undefined,
    breakWord: boolean,
  ): string[] {
    const paragraphs = inputText.split('\n')
    if (!maxWidthPx || whiteSpace === 'nowrap') {
      return paragraphs.length ? paragraphs : ['']
    }

    const allLines: string[] = []
    for (const para of paragraphs) {
      if (!para) {
        allLines.push('')
        continue
      }
      if (breakWord) {
        let cur = ''
        for (const ch of para) {
          const test = cur + ch
          if (ctx.measureText(test).width > maxWidthPx && cur.length > 0) {
            allLines.push(cur)
            cur = ch
          } else {
            cur = test
          }
        }
        if (cur) allLines.push(cur)
      } else {
        const words = para.split(/\s+/)
        let cur = ''
        for (const w of words) {
          if (!w) continue
          const test = cur ? cur + ' ' + w : w
          if (ctx.measureText(test).width > maxWidthPx && cur.length > 0) {
            allLines.push(cur)
            cur = w
          } else {
            cur = test
          }
        }
        if (cur) allLines.push(cur)
      }
    }
    return allLines.length ? allLines : ['']
  }

  function renderCanvas() {
    const pxFont = fontSize * PIXELS_PER_UNIT
    const font = `${pxFont}px sans-serif`
    ctx.font = font

    const maxWPx = maxWidth ? maxWidth * PIXELS_PER_UNIT : undefined
    const lines = wrapText(text, maxWPx, overflowWrap === 'break-word')
    const lineHeight = pxFont * 1.2

    let maxLineWidth = 0
    for (const line of lines) {
      maxLineWidth = Math.max(maxLineWidth, ctx.measureText(line).width)
    }

    const totalTextHeight = lines.length * lineHeight
    const pad = 4
    const cw = Math.max(1, Math.ceil(maxLineWidth + pad * 2))
    const ch = Math.max(1, Math.ceil(totalTextHeight + pad * 2))

    canvas.width = cw
    canvas.height = ch
    ctx.clearRect(0, 0, cw, ch)
    ctx.font = font
    ctx.fillStyle = color
    ctx.textBaseline = 'top'

    for (let i = 0; i < lines.length; i++) {
      let x = pad
      if (anchorX === 'center') {
        x = (cw - ctx.measureText(lines[i]).width) / 2
      } else if (anchorX === 'right') {
        x = cw - ctx.measureText(lines[i]).width - pad
      }
      ctx.fillText(lines[i], x, pad + i * lineHeight)
    }

    texture.needsUpdate = true

    worldWidth = cw / PIXELS_PER_UNIT
    worldHeight = ch / PIXELS_PER_UNIT

    if (anchorX === 'left') anchorOffsetX = worldWidth / 2
    else if (anchorX === 'right') anchorOffsetX = -worldWidth / 2
    else anchorOffsetX = 0

    if (anchorY === 'top') anchorOffsetY = -worldHeight / 2
    else if (anchorY === 'bottom') anchorOffsetY = worldHeight / 2
    else anchorOffsetY = 0

    // Troika-compatible blockBounds: [minX, minY, maxX, maxY] relative to anchor
    textRenderInfo.blockBounds = [
      -worldWidth / 2 + anchorOffsetX,
      -worldHeight / 2 + anchorOffsetY,
      worldWidth / 2 + anchorOffsetX,
      worldHeight / 2 + anchorOffsetY,
    ]

    onsync?.()
  }

  // Re-render canvas when visual properties change
  $effect(() => {
    renderCanvas()
  })

  // Update material opacity without re-rendering canvas
  $effect(() => {
    material.opacity = fillOpacity
  })

  // Update depth offset
  $effect(() => {
    if (depthOffset !== undefined) {
      material.polygonOffset = true
      material.polygonOffsetFactor = depthOffset
      material.polygonOffsetUnits = depthOffset
    }
  })

  let meshRef = $state<THREE.Mesh | undefined>(undefined)

  onDestroy(() => {
    // Hide mesh immediately so the renderer won't try to draw it
    if (meshRef) meshRef.visible = false
    // Defer disposal to next frame so the renderer finishes the current
    // frame without encountering null texture bindings
    requestAnimationFrame(() => {
      texture.dispose()
      material.dispose()
    })
  })
</script>

<T.Mesh
  bind:ref={meshRef}
  position={[
    (position[0] ?? 0) + anchorOffsetX,
    (positionY ?? position[1] ?? 0) + anchorOffsetY,
    position[2] ?? 0,
  ]}
>
  <T.PlaneGeometry args={[worldWidth, worldHeight]} />
  <T is={material} />
</T.Mesh>

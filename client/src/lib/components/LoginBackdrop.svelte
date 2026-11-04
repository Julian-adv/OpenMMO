<script lang="ts">
  import { onMount } from 'svelte'
  import { createRng } from '../utils/simplex-noise'
  import { createRenderCadence } from '../utils/renderCadence'

  let canvas = $state<HTMLCanvasElement | null>(null)

  const MAX_DPR = 1.5
  const TARGET_FPS = 30
  const PARALLAX = 24
  const LAYER_DEPTHS = [0.35, 0.65, 1]
  const FOG = 'rgba(160,110,170,'

  const MOONS = [
    {
      x: 0.74,
      y: 0.2,
      orbitX: 0.05,
      orbitY: 0.03,
      speed: 0.012,
      offset: 0,
      size: 0.055,
      phase: 0.55,
      phaseAmp: 0.25,
      phaseSpeed: 0.02,
      parallax: 0.3,
      lit: '#f1ead2',
      halo: 'rgba(240,230,200,0.12)',
    },
    {
      x: 0.2,
      y: 0.3,
      orbitX: 0.06,
      orbitY: 0.04,
      speed: 0.02,
      offset: 2,
      size: 0.026,
      phase: -0.6,
      phaseAmp: 0.2,
      phaseSpeed: 0.03,
      parallax: 0.35,
      lit: '#f0a982',
      halo: 'rgba(240,150,110,0.14)',
    },
  ]
  const CRATERS = [
    [-0.3, -0.2, 0.22],
    [0.25, 0.3, 0.16],
    [0.1, -0.45, 0.1],
  ]

  function valueNoise(rand: () => number, size: number) {
    const v = Array.from({ length: size + 1 }, rand)
    return (x: number) => {
      const i = Math.floor(x) % size
      const f = x - Math.floor(x)
      const s = f * f * (3 - 2 * f)
      return v[i] * (1 - s) + v[i + 1] * s
    }
  }

  interface Star {
    x: number
    y: number
    r: number
    a: number
    speed: number
    phase: number
  }

  interface Mote {
    x: number
    y: number
    vx: number
    vy: number
    size: number
    phase: number
  }

  interface Light {
    x: number
    y: number
    w: number
    h: number
    phase: number
  }

  interface Layer {
    image: HTMLCanvasElement
    top: number
  }

  onMount(() => {
    const ctx = canvas?.getContext('2d')
    if (!canvas || !ctx) return
    const el: HTMLCanvasElement = canvas
    const c: CanvasRenderingContext2D = ctx
    const reduced = window.matchMedia(
      '(prefers-reduced-motion: reduce)'
    ).matches

    let w = 0
    let h = 0
    let dpr = 1
    let sky: HTMLCanvasElement
    let fog: CanvasGradient
    let layers: Layer[] = []
    let moonSprites: HTMLCanvasElement[] = []
    let stars: Star[] = []
    let motes: Mote[] = []
    let windows: Light[] = []
    let shooting: {
      x: number
      y: number
      vx: number
      vy: number
      life: number
    } | null = null
    let nextShot = 4
    let px = 0
    let py = 0
    let tx = 0
    let ty = 0

    const glow = document.createElement('canvas')
    glow.width = glow.height = 32
    const gctx = glow.getContext('2d')!
    const gg = gctx.createRadialGradient(16, 16, 0, 16, 16, 16)
    gg.addColorStop(0, 'rgba(255,255,255,1)')
    gg.addColorStop(0.25, 'rgba(255,255,255,0.45)')
    gg.addColorStop(1, 'rgba(255,255,255,0)')
    gctx.fillStyle = gg
    gctx.fillRect(0, 0, 32, 32)

    function offscreen(cw: number, ch: number) {
      const image = document.createElement('canvas')
      image.width = Math.ceil(cw * dpr)
      image.height = Math.ceil(ch * dpr)
      const x = image.getContext('2d')!
      x.scale(dpr, dpr)
      return [image, x] as const
    }

    function buildSky() {
      const [image, x] = offscreen(w, h)
      const g = x.createLinearGradient(0, 0, 0, h)
      g.addColorStop(0, '#070b1f')
      g.addColorStop(0.45, '#1a1846')
      g.addColorStop(0.72, '#46285a')
      g.addColorStop(0.86, '#a4536a')
      g.addColorStop(1, '#2a1a3a')
      x.fillStyle = g
      x.fillRect(0, 0, w, h)
      const rand = createRng(7)
      x.globalCompositeOperation = 'lighter'
      for (let i = 0; i < 5; i++) {
        const cx = rand() * w
        const cy = rand() * h * 0.5
        const r = (0.25 + rand() * 0.3) * Math.max(w, h)
        const n = x.createRadialGradient(cx, cy, 0, cx, cy, r)
        const hue = 220 + rand() * 80
        n.addColorStop(0, `hsla(${hue},70%,45%,0.07)`)
        n.addColorStop(1, 'hsla(0,0%,0%,0)')
        x.fillStyle = n
        x.fillRect(0, 0, w, h)
      }
      return image
    }

    function buildMoon(r: number, lit: string, halo: string) {
      const [image, x] = offscreen(r * 8, r * 8)
      const g = x.createRadialGradient(
        r * 4,
        r * 4,
        r * 0.8,
        r * 4,
        r * 4,
        r * 4
      )
      g.addColorStop(0, halo)
      g.addColorStop(1, 'rgba(0,0,0,0)')
      x.fillStyle = g
      x.fillRect(0, 0, r * 8, r * 8)
      x.fillStyle = lit
      x.beginPath()
      x.arc(r * 4, r * 4, r, 0, Math.PI * 2)
      x.fill()
      x.fillStyle = 'rgba(255,255,255,0.08)'
      for (const [ox, oy, cr] of CRATERS) {
        x.beginPath()
        x.arc(r * (4 + ox), r * (4 + oy), cr * r, 0, Math.PI * 2)
        x.fill()
      }
      return image
    }

    function buildLayer(
      seed: number,
      base: number,
      amp: number,
      freq: number,
      color: string,
      haze: string,
      castle: boolean
    ): Layer {
      const lw = w + PARALLAX * 2
      const s = Math.max(0.6, Math.min(1.4, h / 800))
      const top = Math.max(0, h * (base - amp) - (castle ? 150 * s : 0))
      const [image, x] = offscreen(lw, h - top)
      x.translate(0, -top)
      const rand = createRng(seed)
      const n1 = valueNoise(rand, 64)
      const n2 = valueNoise(rand, 64)
      const n3 = valueNoise(rand, 64)
      const ridge = (lx: number) => {
        const u = (lx / lw) * freq
        return (
          h * base -
          amp * h * (n1(u) * 0.6 + n2(u * 2.3) * 0.3 + n3(u * 5.7) * 0.1)
        )
      }
      x.beginPath()
      x.moveTo(0, h)
      for (let p = 0; p <= lw; p += 3) x.lineTo(p, ridge(p))
      x.lineTo(lw, h)
      x.closePath()
      const g = x.createLinearGradient(0, h * (base - amp), 0, h)
      g.addColorStop(0, haze)
      g.addColorStop(0.35, color)
      g.addColorStop(1, color)
      x.fillStyle = g
      x.fill()

      if (castle) {
        const cx = lw * 0.7
        const gy = ridge(cx) + 6
        x.fillStyle = color
        const towers: [number, number, number][] = [
          [-70, 46, 18],
          [-40, 74, 22],
          [-12, 110, 26],
          [20, 82, 22],
          [52, 58, 18],
          [80, 38, 14],
        ]
        x.fillRect(cx - 80 * s, gy - 40 * s, 170 * s, 40 * s + h)
        for (const [dx, th, tw] of towers) {
          const tx0 = cx + dx * s
          const roof = gy - th * s
          x.fillRect(tx0, roof, tw * s, th * s + h)
          x.beginPath()
          x.moveTo(tx0 - 3 * s, roof)
          x.lineTo(tx0 + (tw * s) / 2, roof - tw * 1.3 * s)
          x.lineTo(tx0 + tw * s + 3 * s, roof)
          x.fill()
          for (let k = 0; k < 2; k++) {
            if (rand() < 0.3) continue
            windows.push({
              x: tx0 + (tw * s) / 2 - 2 * s,
              y: roof + (14 + k * 22) * s,
              w: 4 * s,
              h: 7 * s,
              phase: rand() * 10,
            })
          }
        }
        for (let i = 0; i < 9; i++) {
          const hx = cx + (-260 + rand() * 520) * s
          if (Math.abs(hx - cx) < 100 * s) continue
          const hy = ridge(hx) + 4
          const hw = (16 + rand() * 14) * s
          const hh = (10 + rand() * 8) * s
          x.fillRect(hx, hy - hh, hw, hh + h)
          x.beginPath()
          x.moveTo(hx - 3 * s, hy - hh)
          x.lineTo(hx + hw / 2, hy - hh - 10 * s)
          x.lineTo(hx + hw + 3 * s, hy - hh)
          x.fill()
          windows.push({
            x: hx + hw / 2 - 2 * s,
            y: hy - hh / 2 - 2 * s,
            w: 4 * s,
            h: 4 * s,
            phase: rand() * 10,
          })
        }
      }
      return { image, top }
    }

    function spawnMote(anywhere: boolean): Mote {
      return {
        x: Math.random() * w,
        y: anywhere ? h * (0.4 + Math.random() * 0.6) : h + 10,
        vx: (Math.random() - 0.5) * 8,
        vy: -(6 + Math.random() * 18),
        size: 6 + Math.random() * 12,
        phase: Math.random() * 6.28,
      }
    }

    function build() {
      const rect = el.getBoundingClientRect()
      w = Math.max(1, Math.round(rect.width))
      h = Math.max(1, Math.round(rect.height))
      dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR)
      el.width = Math.ceil(w * dpr)
      el.height = Math.ceil(h * dpr)
      c.setTransform(dpr, 0, 0, dpr, 0, 0)
      windows = []
      sky = buildSky()
      layers = [
        buildLayer(11, 0.78, 0.3, 3, '#2b2452', 'rgba(120,80,140,0.6)', false),
        buildLayer(23, 0.86, 0.22, 5, '#1a1636', 'rgba(70,45,95,0.8)', false),
        buildLayer(37, 0.95, 0.14, 4, '#0c0a1c', '#0c0a1c', true),
      ]
      const m = Math.min(w, h)
      moonSprites = MOONS.map((moon) =>
        buildMoon(m * moon.size, moon.lit, moon.halo)
      )
      fog = c.createLinearGradient(0, h * 0.7, 0, h)
      fog.addColorStop(0, `${FOG}0)`)
      fog.addColorStop(0.5, `${FOG}1)`)
      fog.addColorStop(1, `${FOG}0)`)
      const rand = createRng(3)
      const count = Math.round(Math.min(260, (w * h) / 5000))
      stars = Array.from({ length: count }, () => ({
        x: rand() * w,
        y: Math.pow(rand(), 1.4) * h * 0.8,
        r: rand() < 0.08 ? 1.6 : 0.6 + rand() * 0.7,
        a: 0.35 + rand() * 0.65,
        speed: 0.6 + rand() * 2.2,
        phase: rand() * 6.28,
      }))
      motes = Array.from({ length: Math.round(Math.min(60, w / 24)) }, () =>
        spawnMote(true)
      )
    }

    function drawMoons(t: number) {
      const m = Math.min(w, h)
      const skyH = Math.min(h, w * 0.75)
      MOONS.forEach((moon, i) => {
        const a = t * moon.speed + moon.offset
        const r = m * moon.size
        const cx = w * (moon.x + Math.cos(a) * moon.orbitX) - px * moon.parallax
        const cy =
          skyH * (moon.y + Math.sin(a) * moon.orbitY) - py * moon.parallax * 0.7
        const phase = moon.phase + Math.sin(t * moon.phaseSpeed) * moon.phaseAmp
        c.drawImage(moonSprites[i], cx - r * 4, cy - r * 4, r * 8, r * 8)
        c.save()
        c.beginPath()
        c.arc(cx, cy, r, 0, Math.PI * 2)
        c.clip()
        c.fillStyle = 'rgba(14,12,40,0.88)'
        c.beginPath()
        c.arc(
          cx + phase * r * 2.1,
          cy - phase * r * 0.3,
          r * 1.02,
          0,
          Math.PI * 2
        )
        c.fill()
        c.restore()
      })
    }

    function drawShootingStar(dt: number) {
      nextShot -= dt
      if (!shooting && nextShot <= 0) {
        shooting = {
          x: w * (0.2 + Math.random() * 0.6),
          y: h * (0.05 + Math.random() * 0.2),
          vx: 600 + Math.random() * 300,
          vy: 200 + Math.random() * 150,
          life: 0.9,
        }
        nextShot = 6 + Math.random() * 8
      }
      if (!shooting) return
      const s = shooting
      s.x += s.vx * dt
      s.y += s.vy * dt
      s.life -= dt
      const ex = s.x - s.vx * 0.15
      const ey = s.y - s.vy * 0.15
      const tail = c.createLinearGradient(s.x, s.y, ex, ey)
      tail.addColorStop(0, `rgba(255,255,255,${Math.max(0, s.life)})`)
      tail.addColorStop(1, 'rgba(255,255,255,0)')
      c.strokeStyle = tail
      c.lineWidth = 1.5
      c.beginPath()
      c.moveTo(s.x, s.y)
      c.lineTo(ex, ey)
      c.stroke()
      if (s.life <= 0) shooting = null
    }

    function draw(t: number, dt: number) {
      px += (tx - px) * Math.min(1, dt * 3)
      py += (ty - py) * Math.min(1, dt * 3)
      c.globalCompositeOperation = 'source-over'
      c.globalAlpha = 1
      c.drawImage(sky, 0, 0, w, h)

      c.fillStyle = '#ebebff'
      for (const s of stars) {
        c.globalAlpha = s.a * (0.55 + 0.45 * Math.sin(t * s.speed + s.phase))
        c.fillRect(s.x - px * 0.15, s.y - py * 0.1, s.r, s.r)
      }
      c.globalAlpha = 1

      drawMoons(t)
      drawShootingStar(dt)

      layers.forEach((layer, i) => {
        const d = LAYER_DEPTHS[i]
        c.drawImage(
          layer.image,
          -PARALLAX - px * d,
          layer.top - py * d * 0.4,
          w + PARALLAX * 2,
          h - layer.top
        )
        if (i === 1) {
          c.globalAlpha = 0.12 + 0.05 * Math.sin(t * 0.3)
          c.fillStyle = fog
          c.fillRect(0, h * 0.7, w, h * 0.3)
          c.globalAlpha = 1
        }
      })

      const ox = -PARALLAX - px
      const oy = -py * 0.4
      c.fillStyle = '#ffbe5a'
      for (const win of windows) {
        c.globalAlpha =
          0.75 +
          0.25 * Math.sin(t * 3 + win.phase) * Math.sin(t * 1.7 + win.phase * 2)
        c.fillRect(win.x + ox, win.y + oy, win.w, win.h)
      }

      c.globalCompositeOperation = 'lighter'
      c.globalAlpha = 0.35
      for (const win of windows) {
        c.drawImage(glow, win.x + ox - 10, win.y + oy - 10, 24, 24)
      }
      for (let i = 0; i < motes.length; i++) {
        const p = motes[i]
        p.x += (p.vx + Math.sin(t + p.phase) * 10) * dt
        p.y += p.vy * dt
        if (p.y < h * 0.25) motes[i] = spawnMote(false)
        const life =
          Math.min(1, (h - p.y) / (h * 0.2)) *
          Math.min(1, (p.y - h * 0.25) / (h * 0.2))
        c.globalAlpha =
          Math.max(0, life) * (0.5 + 0.5 * Math.sin(t * 2 + p.phase))
        c.drawImage(glow, p.x - p.size / 2, p.y - p.size / 2, p.size, p.size)
      }
      c.globalAlpha = 1
    }

    const cadence = createRenderCadence(TARGET_FPS)
    const start = performance.now()
    let raf = 0
    let lastTick = 0
    let lastDraw = 0
    function frame(now: number) {
      raf = requestAnimationFrame(frame)
      const delta = now - lastTick
      lastTick = now
      if (!cadence.shouldRender(delta)) return
      draw((now - start) / 1000, Math.min(0.1, (now - lastDraw) / 1000))
      lastDraw = now
    }

    function run() {
      cancelAnimationFrame(raf)
      if (reduced) {
        draw(0, 0)
        return
      }
      if (document.hidden) return
      lastTick = lastDraw = performance.now()
      raf = requestAnimationFrame(frame)
    }

    function onPointer(e: PointerEvent) {
      tx = (e.clientX / w - 0.5) * PARALLAX
      ty = (e.clientY / h - 0.5) * PARALLAX
    }

    let resizeRaf = 0
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect
      if (Math.round(width) === w && Math.round(height) === h) return
      cancelAnimationFrame(resizeRaf)
      resizeRaf = requestAnimationFrame(() => {
        build()
        if (reduced) draw(0, 0)
      })
    })
    build()
    ro.observe(el)
    run()
    document.addEventListener('visibilitychange', run)
    if (!reduced)
      window.addEventListener('pointermove', onPointer, { passive: true })

    return () => {
      cancelAnimationFrame(raf)
      cancelAnimationFrame(resizeRaf)
      ro.disconnect()
      document.removeEventListener('visibilitychange', run)
      window.removeEventListener('pointermove', onPointer)
    }
  })
</script>

<canvas class="backdrop" bind:this={canvas} aria-hidden="true"></canvas>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
    pointer-events: none;
    background: #1a1846;
  }
</style>

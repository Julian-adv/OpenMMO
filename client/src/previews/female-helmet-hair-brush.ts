import * as THREE from 'three'
import type { femaleOutfitMask } from '../lib/utils/female-outfit-mask'
import { femaleBrushCursor } from './female-brush-cursor'
import { skinBrush } from './female-skin-brush'

// Runtime helmet and hair part names, as tools/prepare-female-knight.mjs bakes them.
export type HelmetHair = { helmet: string; hair: string; mesh: THREE.Mesh }

const url = '/__female-preview/helmet-hair.json'

// Click-to-hide brush for hair that pokes through a helmet; strokes are hair triangle numbers in the rest pose.
export function helmetHairBrush(options: {
  canvas: HTMLCanvasElement
  camera: THREE.Camera
  mask: ReturnType<typeof femaleOutfitMask>
  shown: () => HelmetHair | undefined
  pause: (brushing: boolean) => void
  disposed: () => boolean
}) {
  const { canvas, camera, mask, shown } = options
  const panel = document.getElementById('helmet-hair-brush')!
  const toggle = document.getElementById(
    'helmet-hair-brush-on'
  ) as HTMLInputElement
  const controls = document.getElementById('helmet-hair-brush-controls')!
  const modeGroup = document.getElementById('helmet-hair-brush-mode')!
  const radius = document.getElementById(
    'helmet-hair-brush-radius'
  ) as HTMLInputElement
  const status = document.getElementById('helmet-hair-brush-status')!
  const hides = () =>
    modeGroup.querySelector<HTMLInputElement>('input:checked')!.value === 'hide'
  const edits = new Map<string, Map<string, Set<number>>>()
  const hiddenOf = (helmet: string, hair: string) => {
    let byHair = edits.get(helmet)
    if (!byHair) edits.set(helmet, (byHair = new Map()))
    let hidden = byHair.get(hair)
    if (!hidden) byHair.set(hair, (hidden = new Set()))
    return hidden
  }
  const restIndex = new WeakMap<THREE.Mesh, ArrayLike<number>>()
  const pickers = new WeakMap<THREE.Mesh, ReturnType<typeof skinBrush>>()
  const cursors = new Map<THREE.Mesh, ReturnType<typeof femaleBrushCursor>>()
  const registered = new Set<string>()
  const history: { helmet: string; hair: string; hidden: number[] }[] = []
  // Masks rebuild the index, so the first look at a mesh keeps its unmasked index.
  const current = () => {
    const target = shown()
    if (target && !restIndex.has(target.mesh))
      restIndex.set(target.mesh, target.mesh.geometry.index!.array.slice())
    return target
  }
  // Re-registers the shown pair's mask only when its strokes changed.
  const refresh = (edited = false) => {
    const target = current()
    panel.hidden = !target
    const active = target && `${target.helmet}:${target.hair}`
    if (target && active && (edited || !registered.has(active))) {
      mask.unregister(active)
      mask.register(
        active,
        {
          mesh_index: 0,
          primitive_index: 0,
          primitive_triangles: restIndex.get(target.mesh)!.length / 3,
          hide_triangle_indices: [...hiddenOf(target.helmet, target.hair)].sort(
            (a, b) => a - b
          ),
        },
        target.mesh
      )
      registered.add(active)
    }
    mask.setEquippedAll(
      Object.fromEntries([...registered].map((key) => [key, key === active]))
    )
  }
  void fetch(url)
    .then((response) => (response.ok ? response.json() : { helmets: {} }))
    .then((saved: { helmets: Record<string, Record<string, number[]>> }) => {
      for (const [helmet, byHair] of Object.entries(saved.helmets))
        for (const [hair, list] of Object.entries(byHair))
          if (!edits.get(helmet)?.has(hair))
            for (const t of list) hiddenOf(helmet, hair).add(t)
      if (options.disposed()) return
      for (const key of registered) mask.unregister(key)
      registered.clear()
      refresh()
    })
    .catch(() => undefined)

  const raycaster = new THREE.Raycaster()
  const hitAt = (event: PointerEvent) => {
    const target = current()
    if (!target || !toggle.checked) return undefined
    const rect = canvas.getBoundingClientRect()
    raycaster.setFromCamera(
      new THREE.Vector2(
        ((event.clientX - rect.left) / rect.width) * 2 - 1,
        -((event.clientY - rect.top) / rect.height) * 2 + 1
      ),
      camera
    )
    if (target.mesh instanceof THREE.SkinnedMesh)
      target.mesh.computeBoundingSphere()
    const hit = raycaster.intersectObject(target.mesh, false)[0]
    return hit?.face && hit.barycoord ? { target, hit } : undefined
  }
  const hideCursors = () => cursors.forEach((cursor) => cursor.hide())
  const point = new THREE.Vector3()
  const corner = new THREE.Vector3()
  let hover: PointerEvent | undefined
  let hoverFrame = 0
  const updateCursor = () => {
    hoverFrame = 0
    hideCursors()
    const found = hover && hitAt(hover)
    if (!found) return
    const { target, hit } = found
    const position = target.mesh.geometry.getAttribute(
      'position'
    ) as THREE.BufferAttribute
    point.set(0, 0, 0)
    for (const [vertex, weight] of [
      [hit.face!.a, hit.barycoord!.x],
      [hit.face!.b, hit.barycoord!.y],
      [hit.face!.c, hit.barycoord!.z],
    ])
      point.addScaledVector(
        corner.fromBufferAttribute(position, vertex),
        weight
      )
    let cursor = cursors.get(target.mesh)
    if (!cursor)
      cursors.set(target.mesh, (cursor = femaleBrushCursor(target.mesh)))
    cursor.show(
      point,
      Number(radius.value) / 100,
      hides() ? '#ff5050' : '#50e080'
    )
  }
  const onMove = (event: PointerEvent) => {
    if (!toggle.checked) return
    hover = event
    hoverFrame ||= requestAnimationFrame(updateCursor)
  }
  const onLeave = () => {
    hover = undefined
    hideCursors()
  }
  let down: { x: number; y: number } | undefined
  const onDown = (event: PointerEvent) => {
    down = { x: event.clientX, y: event.clientY }
  }
  const onUp = (event: PointerEvent) => {
    const start = down
    down = undefined
    if (
      !start ||
      Math.hypot(event.clientX - start.x, event.clientY - start.y) > 4
    )
      return
    const found = hitAt(event)
    if (!found) return
    const { target, hit } = found
    let pick = pickers.get(target.mesh)
    if (!pick)
      pickers.set(
        target.mesh,
        (pick = skinBrush(
          target.mesh.geometry.getAttribute(
            'position'
          ) as THREE.BufferAttribute,
          restIndex.get(target.mesh)!
        ))
      )
    const hidden = hiddenOf(target.helmet, target.hair)
    history.push({ ...target, hidden: [...hidden] })
    for (const t of pick(hit.face!, hit.barycoord!, Number(radius.value) / 100))
      if (hides()) hidden.add(t)
      else hidden.delete(t)
    refresh(true)
    status.textContent = `${hidden.size}개 삼각형을 가립니다. 저장하지 않았습니다.`
  }
  toggle.onchange = () => {
    controls.hidden = !toggle.checked
    options.pause(toggle.checked)
    if (!toggle.checked) hideCursors()
  }
  radius.oninput = () => {
    document.getElementById('helmet-hair-brush-radius-value')!.textContent =
      radius.value
  }
  document.getElementById('helmet-hair-brush-undo')!.onclick = () => {
    const last = history.pop()
    if (!last) return
    edits.get(last.helmet)!.set(last.hair, new Set(last.hidden))
    refresh(true)
  }
  document.getElementById('helmet-hair-brush-save')!.onclick = async () => {
    const target = current()
    if (!target) return
    status.textContent = '저장하는 중…'
    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          helmet: target.helmet,
          hair: Object.fromEntries(
            [...(edits.get(target.helmet) ?? [])].map(([hair, hidden]) => [
              hair,
              [...hidden],
            ])
          ),
        }),
      })
      const result = (await response.json()) as { error?: string }
      status.textContent = response.ok
        ? `${target.helmet} 헤어를 저장했습니다.`
        : (result.error ?? '저장하지 못했습니다.')
    } catch {
      status.textContent = '저장하지 못했습니다.'
    }
  }
  canvas.addEventListener('pointerdown', onDown)
  canvas.addEventListener('pointerup', onUp)
  canvas.addEventListener('pointermove', onMove)
  canvas.addEventListener('pointerleave', onLeave)
  const dispose = () => {
    cancelAnimationFrame(hoverFrame)
    canvas.removeEventListener('pointerdown', onDown)
    canvas.removeEventListener('pointerup', onUp)
    canvas.removeEventListener('pointermove', onMove)
    canvas.removeEventListener('pointerleave', onLeave)
  }
  return { refresh, dispose }
}

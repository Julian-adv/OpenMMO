import type { SkinnedMesh } from 'three'
import {
  HAIR_FIT_DEFAULT,
  femaleHairFitMatrix,
  femaleHairPivot,
  type HairFit,
  type HairFitFace,
  type HairFitStyle,
  type HairFits,
} from '../lib/utils/femaleHairFit'

const url = '/__female-preview/hair-fit.json'
const sliders: {
  key: keyof HairFit
  label: string
  min: number
  max: number
  step: number
  unit: string
}[] = [
  {
    key: 'move-x',
    label: '좌우 이동',
    min: -5,
    max: 5,
    step: 0.05,
    unit: 'cm',
  },
  {
    key: 'move-y',
    label: '위아래 이동',
    min: -5,
    max: 5,
    step: 0.05,
    unit: 'cm',
  },
  {
    key: 'move-z',
    label: '앞뒤 이동',
    min: -5,
    max: 5,
    step: 0.05,
    unit: 'cm',
  },
  { key: 'scale', label: '전체 크기', min: 70, max: 130, step: 0.5, unit: '%' },
  { key: 'scale-x', label: '너비', min: 70, max: 130, step: 0.5, unit: '%' },
  { key: 'scale-y', label: '높이', min: 70, max: 130, step: 0.5, unit: '%' },
  { key: 'scale-z', label: '두께', min: 70, max: 130, step: 0.5, unit: '%' },
  {
    key: 'rotate-x',
    label: '앞뒤 기울기',
    min: -30,
    max: 30,
    step: 0.5,
    unit: '°',
  },
  {
    key: 'rotate-y',
    label: '좌우 회전',
    min: -30,
    max: 30,
    step: 0.5,
    unit: '°',
  },
  {
    key: 'rotate-z',
    label: '좌우 기울기',
    min: -30,
    max: 30,
    step: 0.5,
    unit: '°',
  },
]

export const faceHairFitPanel = `
  <fieldset id="face-hair-fit" disabled>
    <legend>이 얼굴의 헤어 맞춤</legend>
    <p class="details">얼굴·헤어 조합별로 조절합니다. 저장한 값은 게임을 새로고침하면 적용됩니다.</p>
    ${sliders
      .map(
        ({ key, label, min, max, step }) => `
      <label for="face-hair-${key}">${label} <output id="face-hair-${key}-value"></output></label>
      <input id="face-hair-${key}" type="range" min="${min}" max="${max}" step="${step}" />`
      )
      .join('')}
    <button id="face-hair-save" type="button">이 얼굴·헤어 보정 저장</button>
    <button id="face-hair-revert" type="button">저장값으로 되돌리기</button>
    <button id="face-hair-default" type="button">원래 위치·크기</button>
  </fieldset>
  <p id="face-hair-fit-status" class="details" role="status"></p>`

export async function faceHairFitEditor(
  meshes: SkinnedMesh[],
  face: HairFitFace,
  hair: HairFitStyle | 'none',
  disposed: () => boolean
) {
  const panel = document.getElementById('face-hair-fit') as HTMLFieldSetElement
  const status = document.getElementById('face-hair-fit-status')!
  if (hair === 'none' || !meshes.length) {
    status.textContent = '헤어를 선택하면 조절할 수 있습니다.'
    return
  }
  let fits: HairFits
  try {
    const response = await fetch(url, { cache: 'no-store' })
    if (!response.ok) throw new Error()
    fits = (await response.json()) as HairFits
  } catch {
    if (!disposed())
      status.textContent =
        '저장된 헤어 보정을 불러오지 못했습니다. 새로고침해 주세요.'
    return
  }
  if (disposed()) return
  let saved: HairFit = { ...HAIR_FIT_DEFAULT, ...fits[face][hair] }
  let fit = { ...saved }
  const draftKey = `female-face-hair-fit:${face}:${hair}`
  const sources = meshes.map((mesh) => ({
    mesh,
    geometry: mesh.geometry,
    pivot: femaleHairPivot(mesh),
  }))
  const input = (key: string) =>
    document.getElementById(`face-hair-${key}`) as HTMLInputElement
  const dirty = () => sliders.some(({ key }) => fit[key] !== saved[key])
  const remember = () => {
    try {
      if (dirty()) sessionStorage.setItem(draftKey, JSON.stringify(fit))
      else sessionStorage.removeItem(draftKey)
    } catch {
      /* Browser storage is optional. */
    }
  }
  try {
    const draft = JSON.parse(sessionStorage.getItem(draftKey) ?? 'null')
    if (
      draft &&
      sliders.every(
        ({ key, min, max }) =>
          typeof draft[key] === 'number' &&
          Number.isFinite(draft[key]) &&
          draft[key] >= min &&
          draft[key] <= max
      )
    )
      fit = draft
  } catch {
    /* Ignore an expired draft. */
  }
  const show = () => {
    for (const { key, unit } of sliders) {
      input(key).value = String(fit[key])
      document.getElementById(`face-hair-${key}-value`)!.textContent =
        `${Number(fit[key].toFixed(2))}${unit}`
    }
    for (const { mesh, geometry, pivot } of sources) {
      const previous = mesh.geometry
      mesh.geometry = sliders.every(
        ({ key }) => fit[key] === HAIR_FIT_DEFAULT[key]
      )
        ? geometry
        : geometry.clone().applyMatrix4(femaleHairFitMatrix(fit, pivot))
      if (previous !== geometry) previous.dispose()
    }
    remember()
    status.textContent = dirty()
      ? '저장하지 않은 조절이 있습니다.'
      : '이 얼굴·헤어의 저장값을 표시합니다.'
  }
  for (const { key } of sliders) {
    const slider = input(key)
    slider.oninput = () => {
      fit[key] = Number(slider.value)
      show()
    }
    slider.addEventListener(
      'wheel',
      (event) => {
        event.preventDefault()
        if (!event.deltaY) return
        if (event.deltaY < 0) slider.stepUp()
        else slider.stepDown()
        slider.dispatchEvent(new Event('input'))
      },
      { passive: false }
    )
  }
  document.getElementById('face-hair-default')!.onclick = () => {
    fit = { ...HAIR_FIT_DEFAULT }
    show()
  }
  document.getElementById('face-hair-revert')!.onclick = () => {
    fit = { ...saved }
    show()
  }
  document.getElementById('face-hair-save')!.onclick = async () => {
    panel.disabled = true
    status.textContent = '저장하는 중…'
    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ face, hair, fit }),
      })
      const result = (await response.json()) as { error?: string }
      if (disposed()) return
      if (!response.ok) throw new Error(result.error ?? '저장하지 못했습니다.')
      saved = { ...fit }
      remember()
      status.textContent = '저장했습니다. 게임을 새로고침하면 적용됩니다.'
    } catch (error) {
      if (!disposed())
        status.textContent =
          error instanceof Error ? error.message : '저장하지 못했습니다.'
    } finally {
      if (!disposed()) panel.disabled = false
    }
  }
  show()
  panel.disabled = false
}

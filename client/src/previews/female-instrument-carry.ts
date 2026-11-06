import * as THREE from 'three'
import type { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import initial from '../lib/data/instrumentCarry.json'
import {
  poseInstrumentCarry,
  type InstrumentCarry,
} from '../lib/utils/handProps'

const url = '/__female-preview/instrument-carry.json'
// Keep in sync with ranges in tools/lib/instrument-carry.mjs; lengths show in cm.
const sliders: {
  key: keyof InstrumentCarry
  label: string
  min: number
  max: number
  step: number
  cm?: boolean
}[] = [
  {
    key: 'grip',
    label: '쥐는 위치 (소리통에서)',
    min: 0,
    max: 0.75,
    step: 0.005,
    cm: true,
  },
  { key: 'tilt', label: '앞으로 기울기', min: -30, max: 90, step: 1 },
  { key: 'splay', label: '옆으로 벌리기', min: -45, max: 45, step: 1 },
  { key: 'twist', label: '넥 축 회전', min: -180, max: 180, step: 1 },
  {
    key: 'front',
    label: '앞뒤 이동',
    min: -0.1,
    max: 0.1,
    step: 0.0025,
    cm: true,
  },
  {
    key: 'palm',
    label: '손바닥 쪽 이동',
    min: -0.1,
    max: 0.1,
    step: 0.0025,
    cm: true,
  },
]

export const instrumentCarryPanel = `
  <div id="instrument-carry" hidden>
    <p class="details">들고 다니는 만돌린의 위치와 각도입니다. 저장하면 게임에도 그대로 적용됩니다.</p>
    ${sliders
      .map(
        ({ key, label, min, max, step }) =>
          `<label for="instrument-carry-${key}">${label} <output id="instrument-carry-${key}-value"></output></label>
    <input id="instrument-carry-${key}" type="range" min="${min}" max="${max}" step="${step}" />`
      )
      .join('\n    ')}
    <button id="instrument-carry-reset" type="button">저장된 값으로</button>
    <button id="instrument-carry-save" type="button">만돌린 들기 저장</button>
    <p id="instrument-carry-status" class="details" aria-live="polite"></p>
  </div>`

// Mandolin in the right hand, posed live from the panel's sliders.
export function instrumentCarryEditor(
  hand: THREE.Object3D,
  loader: GLTFLoader,
  disposed: () => boolean
) {
  const panel = document.getElementById('instrument-carry')!
  const status = document.getElementById('instrument-carry-status')!
  let saved: InstrumentCarry = { ...initial }
  const carry: InstrumentCarry = { ...initial }
  let prop: THREE.Object3D | undefined
  let loading: Promise<void> | undefined
  let shown = false
  const input = (key: string) =>
    document.getElementById(`instrument-carry-${key}`) as HTMLInputElement
  const show = () => {
    for (const { key, cm } of sliders) {
      input(key).value = String(carry[key])
      document.getElementById(`instrument-carry-${key}-value`)!.textContent = cm
        ? `${(carry[key] * 100).toFixed(1)}cm`
        : `${carry[key]}°`
    }
    if (prop) poseInstrumentCarry(prop, carry)
  }
  for (const { key } of sliders) {
    const slider = input(key)
    slider.oninput = () => {
      carry[key] = Number(slider.value)
      show()
      status.textContent = '저장하지 않은 조절이 있습니다.'
    }
    // One wheel notch moves the slider one step.
    slider.addEventListener(
      'wheel',
      (event) => {
        event.preventDefault()
        if (event.deltaY === 0) return
        if (event.deltaY < 0) slider.stepUp()
        else slider.stepDown()
        slider.dispatchEvent(new Event('input'))
      },
      { passive: false }
    )
  }
  document.getElementById('instrument-carry-reset')!.onclick = () => {
    Object.assign(carry, saved)
    show()
    status.textContent = ''
  }
  document.getElementById('instrument-carry-save')!.onclick = async () => {
    status.textContent = '저장하는 중…'
    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(carry),
      })
      const result = (await response.json()) as { error?: string }
      if (response.ok) saved = { ...carry }
      status.textContent = response.ok
        ? '저장했습니다. 게임은 새로고침하면 반영됩니다.'
        : (result.error ?? '저장하지 못했습니다.')
    } catch {
      status.textContent = '저장하지 못했습니다.'
    }
  }
  show()
  return {
    setShown(value: boolean) {
      shown = value
      panel.hidden = !value
      if (prop) prop.visible = value
      if (!value || prop) return
      loading ??= loader
        .loadAsync('/models/weapons/mandolin.glb')
        .then(({ scene }) => {
          if (disposed()) return
          prop = scene
          prop.name = 'femaleMandolin'
          prop.visible = shown
          poseInstrumentCarry(prop, carry)
          hand.add(prop)
        })
    },
  }
}

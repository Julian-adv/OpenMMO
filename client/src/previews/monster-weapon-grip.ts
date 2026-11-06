import type * as THREE from 'three'
import type { MonsterDefinition } from '../lib/data/monsterDefs'
import { seatMonsterWeapon } from '../lib/utils/monsterWeapon'

const url = '/__monster-preview/weapon-grip'

interface Grip {
  weaponOffset: number
  weaponOffsetX: number
  weaponOffsetZ: number
  rotationX: number
  rotationY: number
  rotationZ: number
}

// Keep in sync with ranges in tools/lib/monster-weapon-grip.mjs; lengths show in cm.
const sliders: {
  key: keyof Grip
  label: string
  min: number
  max: number
  step: number
  cm?: boolean
}[] = [
  {
    key: 'weaponOffset',
    label: '손목→손바닥 (Y)',
    min: -0.3,
    max: 0.3,
    step: 0.0025,
    cm: true,
  },
  {
    key: 'weaponOffsetX',
    label: '좌우 (X)',
    min: -0.3,
    max: 0.3,
    step: 0.0025,
    cm: true,
  },
  {
    key: 'weaponOffsetZ',
    label: '앞뒤 (Z)',
    min: -0.3,
    max: 0.3,
    step: 0.0025,
    cm: true,
  },
  { key: 'rotationX', label: 'X축 회전', min: -180, max: 180, step: 1 },
  { key: 'rotationY', label: 'Y축 회전', min: -180, max: 180, step: 1 },
  { key: 'rotationZ', label: 'Z축 회전', min: -180, max: 180, step: 1 },
]

export const weaponGripPanel = `
  <fieldset id="weapon-grip" hidden>
    <legend>손 무기 위치</legend>
    <p class="details">무기를 쥐는 위치와 각도입니다. 저장하면 monsters.csv에 기록되고 게임은 새로고침하면 반영됩니다.</p>
    ${sliders
      .map(
        ({ key, label, min, max, step }) =>
          `<label for="weapon-grip-${key}">${label} <output id="weapon-grip-${key}-value"></output></label>
    <input id="weapon-grip-${key}" type="range" min="${min}" max="${max}" step="${step}" />`
      )
      .join('\n    ')}
    <button id="weapon-grip-reset" type="button">저장된 값으로</button>
    <button id="weapon-grip-save" type="button">무기 위치 저장</button>
    <p id="weapon-grip-status" class="details" aria-live="polite"></p>
  </fieldset>`

const gripOf = (def: MonsterDefinition): Grip => {
  const [x = 0, y = 0, z = 0] = String(def.weaponRotation ?? '')
    .split('|')
    .map((part) => Number(part) || 0)
  return {
    weaponOffset: def.weaponOffset ?? 0,
    weaponOffsetX: def.weaponOffsetX ?? 0,
    weaponOffsetZ: def.weaponOffsetZ ?? 0,
    rotationX: x,
    rotationY: y,
    rotationZ: z,
  }
}

/** Live sliders for a monster's hand weapon seat, saved into its csv row. */
export function weaponGripEditor(
  id: string,
  def: MonsterDefinition,
  hand: THREE.Object3D,
  weapon: THREE.Object3D
) {
  document.getElementById('weapon-grip')!.hidden = false
  const status = document.getElementById('weapon-grip-status')!
  let saved = gripOf(def)
  const grip = { ...saved }
  const input = (key: string) =>
    document.getElementById(`weapon-grip-${key}`) as HTMLInputElement
  const show = () => {
    for (const { key, cm } of sliders) {
      input(key).value = String(grip[key])
      document.getElementById(`weapon-grip-${key}-value`)!.textContent = cm
        ? `${(grip[key] * 100).toFixed(2)}cm`
        : `${grip[key]}°`
    }
    seatMonsterWeapon(hand, weapon, {
      ...grip,
      weaponRotation: `${grip.rotationX}|${grip.rotationY}|${grip.rotationZ}`,
    })
  }
  for (const { key } of sliders) {
    const slider = input(key)
    slider.oninput = () => {
      grip[key] = Number(slider.value)
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
  document.getElementById('weapon-grip-reset')!.onclick = () => {
    Object.assign(grip, saved)
    show()
    status.textContent = ''
  }
  document.getElementById('weapon-grip-save')!.onclick = async () => {
    status.textContent = '저장하는 중…'
    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id, ...grip }),
      })
      const result = (await response.json()) as { error?: string }
      if (response.ok) saved = { ...grip }
      status.textContent = response.ok
        ? '저장했습니다. 게임은 새로고침하면 반영됩니다.'
        : (result.error ?? '저장하지 못했습니다.')
    } catch {
      status.textContent = '저장하지 못했습니다.'
    }
  }
  show()
}

import type * as THREE from 'three'
import type { MonsterDefinition } from '../lib/data/monsterDefs'
import { seatMonsterWeapon } from '../lib/utils/monsterWeapon'
import { sliderEditor, sliderPanel } from './slider-editor'
import sliders from './weapon-grip-sliders.json'

export const weaponGripPanel = sliderPanel('weapon-grip', sliders, {
  legend: '손 무기 위치',
  note: '무기를 쥐는 위치와 각도입니다. 저장하면 monsters.csv에 기록되고 게임은 새로고침하면 반영됩니다. 플레이어 체형은 <a href="?handfit=modular_male">손 소지품 위치 편집</a>에서 맞춥니다.',
  saveLabel: '무기 위치 저장',
})

/** Live sliders for a monster's hand weapon seat, saved into its csv row. */
export function weaponGripEditor(
  id: string,
  def: MonsterDefinition,
  hand: THREE.Object3D,
  weapon: THREE.Object3D
) {
  const [rotationX = 0, rotationY = 0, rotationZ = 0] = String(
    def.weaponRotation ?? ''
  )
    .split('|')
    .map((part) => Number(part) || 0)
  const grip = {
    weaponOffset: def.weaponOffset ?? 0,
    weaponOffsetX: def.weaponOffsetX ?? 0,
    weaponOffsetZ: def.weaponOffsetZ ?? 0,
    rotationX,
    rotationY,
    rotationZ,
  }
  sliderEditor('weapon-grip', sliders, grip, {
    url: '/__monster-preview/weapon-grip',
    extra: () => ({ id }),
    apply: (grip) =>
      seatMonsterWeapon(hand, weapon, {
        ...grip,
        weaponRotation: `${grip.rotationX}|${grip.rotationY}|${grip.rotationZ}`,
      }),
  }).panel.hidden = false
}

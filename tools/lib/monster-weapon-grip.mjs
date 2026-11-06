import { readFile } from 'node:fs/promises'
import { sliderList, sliderSaveRoute, writeAtomic } from './slider-save-route.mjs'

// The monster preview saves a hand weapon's seat into its csv row; the csv watcher regenerates the json.
const target = new URL('../../data-src/monsters.csv', import.meta.url)
const round = (value, digits) => String(Number(value.toFixed(digits)))

export function monsterWeaponGrip() {
  return sliderSaveRoute('/__monster-preview/weapon-grip', sliderList('weapon-grip-sliders.json'), async input => {
    if (typeof input.id !== 'string' || !/^[a-z0-9_]+$/.test(input.id))
      return { status: 400, error: '몬스터 id가 올바르지 않습니다.' }
    const lines = (await readFile(target, 'utf8')).split('\n')
    const header = lines[0].split(',')
    const row = lines.findIndex(line => line.split(',')[0] === input.id)
    if (row < 1) return { status: 404, error: `monsters.csv에 ${input.id} 행이 없습니다.` }
    const cells = lines[row].split(',')
    const set = (name, value) => { cells[header.indexOf(name)] = value }
    for (const key of ['weaponOffset', 'weaponOffsetX', 'weaponOffsetZ']) set(key, round(input[key], 6))
    set('weaponRotation', ['rotationX', 'rotationY', 'rotationZ'].map(k => input[k].toFixed(3)).join('|'))
    lines[row] = cells.join(',')
    await writeAtomic(target, lines.join('\n'))
  })
}

import { readFile, writeFile, rename } from 'node:fs/promises'

// The monster preview saves a hand weapon's seat into its csv row; the csv watcher regenerates the json.
const target = new URL('../../data-src/monsters.csv', import.meta.url)
const route = '/__monster-preview/weapon-grip'
// Keep in sync with the sliders in client/src/previews/monster-weapon-grip.ts.
const ranges = {
  weaponOffset: [-0.3, 0.3], weaponOffsetX: [-0.3, 0.3], weaponOffsetZ: [-0.3, 0.3],
  rotationX: [-180, 180], rotationY: [-180, 180], rotationZ: [-180, 180],
}
const round = (value, digits) => String(Number(value.toFixed(digits)))

export function monsterWeaponGrip() {
  return (req, res, next) => {
    if (req.url?.split('?')[0] !== route) return next()
    const reply = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
      res.end(JSON.stringify(value))
    }
    void (async () => {
      if (req.method !== 'POST') return reply(405, { error: '지원하지 않는 요청입니다.' })
      if (req.headers['sec-fetch-site'] === 'cross-site' ||
          !req.headers['content-type']?.startsWith('application/json'))
        return reply(400, { error: '올바른 저장 요청이 아닙니다.' })
      let raw = ''
      for await (const chunk of req) {
        raw += chunk
        if (raw.length > 4096) return reply(413, { error: '저장값이 너무 큽니다.' })
      }
      let input
      try { input = JSON.parse(raw) } catch { return reply(400, { error: '저장값을 읽을 수 없습니다.' }) }
      if (typeof input?.id !== 'string' || !/^[a-z0-9_]+$/.test(input.id))
        return reply(400, { error: '몬스터 id가 올바르지 않습니다.' })
      for (const [key, [min, max]] of Object.entries(ranges)) {
        const value = input[key]
        if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max)
          return reply(400, { error: `값 범위를 확인하세요: ${key}` })
      }
      const text = await readFile(target, 'utf8')
      const lines = text.split('\n')
      const header = lines[0].split(',')
      const column = name => header.indexOf(name)
      const row = lines.findIndex(line => line.split(',')[0] === input.id)
      if (row < 1) return reply(404, { error: `monsters.csv에 ${input.id} 행이 없습니다.` })
      const cells = lines[row].split(',')
      cells[column('weaponOffset')] = round(input.weaponOffset, 6)
      cells[column('weaponOffsetX')] = round(input.weaponOffsetX, 6)
      cells[column('weaponOffsetZ')] = round(input.weaponOffsetZ, 6)
      cells[column('weaponRotation')] = ['rotationX', 'rotationY', 'rotationZ'].map(k => input[k].toFixed(3)).join('|')
      lines[row] = cells.join(',')
      await writeFile(new URL(target.href + '.tmp'), lines.join('\n'))
      await rename(new URL(target.href + '.tmp'), target)
      reply(200, { saved: true })
    })().catch(next)
  }
}

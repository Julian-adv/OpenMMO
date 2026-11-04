import { readFile, writeFile, rename } from 'node:fs/promises'

const root = new URL('../../', import.meta.url)
const target = new URL('doc/assets/female-priest-preview-fit.json', root)
const parts = ['top', 'bottom', 'gloves', 'boots', 'helmet']
// Paired parts save { left, right }; left is the character's left (+x).
const paired = ['gloves', 'boots']
// Keep in sync with PART_FIT_RANGES in client/src/previews/female-part-fit.ts.
const ranges = {
  'move-x': [-10, 10], 'move-y': [-10, 10], 'move-z': [-10, 10],
  scale: [70, 130], 'scale-x': [70, 130], 'scale-y': [70, 130], 'scale-z': [70, 130],
  'rotate-x': [-45, 45], 'rotate-y': [-45, 45], 'rotate-z': [-45, 45],
}

async function load() {
  try {
    return JSON.parse(await readFile(target, 'utf8'))
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
    return { version: 1, parts: {} }
  }
}

export function priestPreviewFit() {
  return (req, res, next) => {
    if (req.url?.split('?')[0] !== '/__female-preview/priest-fit.json') return next()
    const reply = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
      res.end(JSON.stringify(value))
    }
    void (async () => {
      if (req.method === 'GET') return reply(200, await load())
      if (req.method !== 'POST') return reply(405, { error: '지원하지 않는 요청입니다.' })
      if (req.headers['sec-fetch-site'] === 'cross-site' ||
          !req.headers['content-type']?.startsWith('application/json'))
        return reply(400, { error: '올바른 저장 요청이 아닙니다.' })
      let raw = ''
      for await (const chunk of req) {
        raw += chunk
        if (raw.length > 512 * 1024) return reply(413, { error: '저장값이 너무 큽니다.' })
      }
      let input
      try { input = JSON.parse(raw) } catch { return reply(400, { error: '저장값을 읽을 수 없습니다.' }) }
      if (!parts.includes(input?.part) || typeof input.settings !== 'object' || !input.settings)
        return reply(400, { error: '피팅값이 올바르지 않습니다.' })
      const check = (source) => {
        if (typeof source !== 'object' || !source) return `피팅값이 올바르지 않습니다.`
        const fit = {}
        for (const [key, [min, max]] of Object.entries(ranges)) {
          const value = source[key]
          if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max)
            return `피팅값 범위를 확인하세요: ${key}`
          fit[key] = value
        }
        return fit
      }
      const sides = paired.includes(input.part) ? ['left', 'right'] : [null]
      const checked = sides.map(side => check(side ? input.settings[side] : input.settings))
      const error = checked.find(value => typeof value === 'string')
      if (error) return reply(400, { error })
      const settings = sides[0] ? { left: checked[0], right: checked[1] } : checked[0]
      // Brush strokes stay across bakes: the bake reapplies them after its own skin hiding.
      const triangles = (list) => Array.isArray(list) && list.every(t => Number.isInteger(t) && t >= 0 && t < 1e6)
      const brush = input.brush ?? { hide: [], show: [] }
      if (!triangles(brush.hide) || !triangles(brush.show)) return reply(400, { error: '브러시 값이 올바르지 않습니다.' })
      const saved = await load()
      saved.parts[input.part] = { ...saved.parts[input.part], pending: settings, saved_at: new Date().toISOString(),
                                  brush: { hide: [...new Set(brush.hide)].sort((a, b) => a - b),
                                           show: [...new Set(brush.show)].sort((a, b) => a - b) } }
      await writeFile(new URL(target.href + '.tmp'), JSON.stringify(saved, null, 2) + '\n')
      await rename(new URL(target.href + '.tmp'), target)
      reply(200, { saved: true })
    })().catch(next)
  }
}

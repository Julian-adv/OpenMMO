import { readFile, writeFile, rename } from 'node:fs/promises'

const root = new URL('../../', import.meta.url)
// Parts each hand-fitted outfit has; keep in sync with OUTFITS in tools/blender-scripts/fit_female_outfit.py.
const outfits = {
  priest: ['top', 'bottom', 'gloves', 'boots', 'helmet'],
  ranger: ['top', 'bottom', 'gloves', 'boots'],
  bard: ['top', 'bottom', 'gloves', 'boots', 'helmet'],
  ceremony: ['top', 'bottom', 'gloves', 'boots', 'helmet'],
}
const route = /^\/__female-preview\/([a-z]+)-fit\.json$/
// Paired parts save { left, right }; left is the character's left (+x).
const paired = ['gloves', 'boots']
// Keep in sync with PART_FIT_RANGES in client/src/previews/female-part-fit.ts.
const ranges = {
  'move-x': [-10, 10], 'move-y': [-10, 10], 'move-z': [-10, 10],
  scale: [70, 130], 'scale-x': [70, 130], 'scale-y': [70, 130], 'scale-z': [70, 130],
  'rotate-x': [-45, 45], 'rotate-y': [-45, 45], 'rotate-z': [-45, 45],
}

async function load(target) {
  try {
    return JSON.parse(await readFile(target, 'utf8'))
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
    return { version: 1, parts: {} }
  }
}

export function outfitPreviewFit() {
  return (req, res, next) => {
    const outfit = req.url?.split('?')[0].match(route)?.[1]
    if (!outfit || !Object.hasOwn(outfits, outfit)) return next()
    const parts = outfits[outfit]
    const target = new URL(`doc/assets/female-${outfit}-preview-fit.json`, root)
    const reply = (status, value) => {
      res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' })
      res.end(JSON.stringify(value))
    }
    void (async () => {
      if (req.method === 'GET') return reply(200, await load(target))
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
        // Glove fits saved since 2026-11-05 work in the forearm frame.
        if (source.frame !== undefined) {
          if (source.frame !== 'forearm' || input.part !== 'gloves') return '피팅 기준이 올바르지 않습니다.'
          fit.frame = 'forearm'
        }
        return fit
      }
      const sides = paired.includes(input.part) ? ['left', 'right'] : [null]
      const checked = sides.map(side => check(side ? input.settings[side] : input.settings))
      const error = checked.find(value => typeof value === 'string')
      if (error) return reply(400, { error })
      const settings = sides[0] ? { left: checked[0], right: checked[1] } : checked[0]
      // Sleeve-end offsets (cm) of a top whose sleeves are aligned as one tube.
      const ends = input.settings['sleeve-end']
      if (ends !== undefined) {
        const valid = ['left', 'right'].every(side => ['forward', 'out'].every(key => {
          const value = ends?.[side]?.[key]
          return typeof value === 'number' && Number.isFinite(value) && Math.abs(value) <= 5
        }))
        if (sides[0] || !valid) return reply(400, { error: '소매 끝 값이 올바르지 않습니다.' })
        settings['sleeve-end'] = Object.fromEntries(['left', 'right'].map(side =>
          [side, { forward: ends[side].forward, out: ends[side].out }]))
      }
      // Brush strokes stay across bakes: the bake reapplies them after its own skin hiding.
      const triangles = (list) => Array.isArray(list) && list.every(t => Number.isInteger(t) && t >= 0 && t < 1e6)
      const brush = input.brush ?? { hide: [], show: [] }
      if (!triangles(brush.hide) || !triangles(brush.show)) return reply(400, { error: '브러시 값이 올바르지 않습니다.' })
      // Boots with a sole colour: clicked triangle centres (bind space, m) and the colour.
      const sole = input.sole
      if (sole !== undefined) {
        const centres = (list) => Array.isArray(list) && list.length <= 5000 &&
          list.every(c => Array.isArray(c) && c.length === 3 && c.every(v => typeof v === 'number' && Number.isFinite(v) && Math.abs(v) < 3))
        if (input.part !== 'boots' || !centres(sole?.gold) || !centres(sole?.leather) ||
            (sole.colour !== undefined && !/^#[0-9a-f]{6}$/i.test(sole.colour)))
          return reply(400, { error: '밑창 값이 올바르지 않습니다.' })
      }
      const saved = await load(target)
      saved.parts[input.part] = { ...saved.parts[input.part], pending: settings, saved_at: new Date().toISOString(),
                                  ...(sole && { sole: { gold: sole.gold, leather: sole.leather,
                                                        ...(sole.colour && { colour: sole.colour.toLowerCase() }) } }),
                                  brush: { hide: [...new Set(brush.hide)].sort((a, b) => a - b),
                                           show: [...new Set(brush.show)].sort((a, b) => a - b) } }
      await writeFile(new URL(target.href + '.tmp'), JSON.stringify(saved, null, 2) + '\n')
      await rename(new URL(target.href + '.tmp'), target)
      reply(200, { saved: true })
    })().catch(next)
  }
}

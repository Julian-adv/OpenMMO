import { readFile } from 'node:fs/promises'
import { sliderList, sliderSaveRoute, writeAtomic } from './slider-save-route.mjs'

// Where every rig holds every hand prop; the game reads it through handProps.ts.
const HAND_FITS_FILE = new URL('../../client/src/lib/data/handFits.json', import.meta.url)
const KEY = /^(hand:(main|off)|item:[a-z0-9_]+|model:[a-z0-9_./-]+|type:[a-z_]+|category:[a-z_]+)$/
const round = value => Math.round(value * 1e6) / 1e6

/** Dev-server plugin saving the hand-fit preview into handFits.json. The
 *  tilt/lean/spin sliders are edits the page folds into `rotation`. */
export function handFitPlugin() {
  return {
    name: 'hand-fit',
    apply: 'serve',
    // The preview saves while open; skip the page reload it would trigger.
    handleHotUpdate({ file }) {
      if (file === HAND_FITS_FILE.pathname) return []
    },
    configureServer(server) {
      server.middlewares.use(sliderSaveRoute('/__hand-fit', sliderList('hand-fit-sliders.json'), async input => {
        if (!/^[a-z_]+$/.test(input.rig) || typeof input.key !== 'string' || !KEY.test(input.key))
          return { status: 400, error: '체형이나 적용 대상이 올바르지 않습니다.' }
        const { rotation } = input
        if (!Array.isArray(rotation) || rotation.length !== 3 || !rotation.every(Number.isFinite))
          return { status: 400, error: '회전값이 올바르지 않습니다.' }
        const fits = JSON.parse(await readFile(HAND_FITS_FILE, 'utf8'))
        fits[input.rig] = {
          ...fits[input.rig],
          [input.key]: { position: [input.px, input.py, input.pz].map(round), rotation: rotation.map(round) },
        }
        // One number array per line, as prettier keeps it.
        await writeAtomic(HAND_FITS_FILE, JSON.stringify(fits, null, 2)
          .replace(/\[\s+([-\d.e,\s]+?)\s+\]/g, (_, list) => `[${list.split(/,\s*/).join(', ')}]`) + '\n')
      }))
    },
  }
}

import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import { convertCsvFile } from './convert.mjs'
import { monsterWeaponGrip } from './lib/monster-weapon-grip.mjs'

const toolsDir = path.dirname(fileURLToPath(import.meta.url))
const sourceDir = path.join(toolsDir, '..', 'data-src')

function convertAllCsv() {
  const csvFiles = fs.readdirSync(sourceDir).filter((f) => f.endsWith('.csv'))
  for (const csvFile of csvFiles) {
    const { count } = convertCsvFile(csvFile)
    console.log(
      `\x1b[36m[csv]\x1b[0m Converted ${count} entry/entries from ${csvFile}`
    )
  }
}

export function monsterCsvPlugin() {
  return {
    name: 'csv-data-watch',
    buildStart() {
      convertAllCsv()
    },
    configureServer(server) {
      server.middlewares.use(monsterWeaponGrip())
      // Chokidar 4 takes no globs; watch the folder.
      server.watcher.add(sourceDir)
      server.watcher.on('change', (changedPath) => {
        const resolved = path.resolve(changedPath)
        if (resolved.startsWith(sourceDir) && resolved.endsWith('.csv')) {
          const csvFile = path.basename(resolved)
          const { count } = convertCsvFile(csvFile)
          console.log(
            `\x1b[36m[csv]\x1b[0m ${csvFile} changed, regenerated ${count} entry/entries`
          )
        }
      })
    },
  }
}

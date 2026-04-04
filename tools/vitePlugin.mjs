import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import { convertCsvFile } from './convert.mjs'

const toolsDir = path.dirname(fileURLToPath(import.meta.url))
const dataDir = path.join(toolsDir, '..', 'data')

function convertAllCsv() {
  const csvFiles = fs.readdirSync(dataDir).filter((f) => f.endsWith('.csv'))
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
      server.watcher.add(path.join(dataDir, '*.csv'))
      server.watcher.on('change', (changedPath) => {
        const resolved = path.resolve(changedPath)
        if (resolved.startsWith(dataDir) && resolved.endsWith('.csv')) {
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

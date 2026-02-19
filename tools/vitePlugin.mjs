import path from 'path'
import { convertCsvToJson, csvPath } from './convert.mjs'

export function monsterCsvPlugin() {
  return {
    name: 'monster-csv-watch',
    buildStart() {
      convertCsvToJson()
    },
    configureServer(server) {
      server.watcher.add(csvPath)
      server.watcher.on('change', (changedPath) => {
        if (path.resolve(changedPath) === csvPath) {
          const count = convertCsvToJson()
          console.log(
            `\x1b[36m[monsters]\x1b[0m CSV changed, regenerated ${count} monster(s)`
          )
        }
      })
    },
  }
}

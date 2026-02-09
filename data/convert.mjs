import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'

const dataDir = path.dirname(fileURLToPath(import.meta.url))
const csvPath = path.join(dataDir, 'monsters.csv')
const jsonPath = path.join(dataDir, 'monsters.json')

const csv = fs.readFileSync(csvPath, 'utf-8').trim()
const lines = csv.split('\n')
const headers = lines[0].split(',')

const monsters = {}

for (let i = 1; i < lines.length; i++) {
  const line = lines[i].trim()
  if (!line) continue

  const values = line.split(',')
  const entry = {}

  for (let j = 0; j < headers.length; j++) {
    const key = headers[j].trim()
    const raw = values[j].trim()

    // Try to parse as number, keep as string otherwise
    const num = Number(raw)
    entry[key] = Number.isNaN(num) ? raw : num
  }

  monsters[entry.id] = entry
}

fs.writeFileSync(jsonPath, JSON.stringify(monsters, null, 2) + '\n')
console.log(`Converted ${Object.keys(monsters).length} monster(s) -> ${jsonPath}`)

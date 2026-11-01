import { createRequire } from 'node:module'
import { mkdir, readFile, writeFile } from 'node:fs/promises'

const require = createRequire(import.meta.url)
const validator = require('gltf-validator')
const paths = [
  'assets/monsters/thief_king/rig_v1/thief_king.glb',
  'client/public/models/monsters/thief_king.glb',
]
const results = {}
for (const path of paths) {
  const bytes = await readFile(path)
  const result = await validator.validateBytes(new Uint8Array(bytes), { uri: path, maxIssues: 100 })
  results[path] = result.issues
  if (result.issues.numErrors) throw new Error(`Invalid glTF: ${path}`)
}
await mkdir('/tmp/thief-king-review', { recursive: true })
await writeFile('/tmp/thief-king-review/gltf-validation.json', JSON.stringify(results, null, 2) + '\n')
console.log(JSON.stringify(results, null, 2))

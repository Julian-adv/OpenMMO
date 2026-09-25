import { createHash } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { gzipSync } from 'node:zlib'

const root = process.argv[2]
  ? path.resolve(process.argv[2])
  : path.resolve(import.meta.dirname, '../dist')
const compressedByHash = new Map()
let count = 0
let before = 0
let after = 0

function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      walk(file)
      continue
    }
    if (!entry.isFile() || !/\.(glb|js|css|wasm)$/i.test(entry.name)) continue

    const stat = fs.statSync(file)
    const output = `${file}.gz`
    if (stat.size < 1024) {
      fs.rmSync(output, { force: true })
      continue
    }

    const source = fs.readFileSync(file)
    const hash = createHash('sha256').update(source).digest('hex')
    const cached = compressedByHash.get(hash)
    const compressed =
      cached === undefined ? gzipSync(source, { level: 6 }) : null
    if (cached === null || (compressed && compressed.length >= source.length)) {
      compressedByHash.set(hash, null)
      fs.rmSync(output, { force: true })
      continue
    }

    const temporary = `${output}.${process.pid}.tmp`
    try {
      if (cached) fs.copyFileSync(cached, temporary)
      else fs.writeFileSync(temporary, compressed)
      fs.chmodSync(temporary, stat.mode & 0o777)
      fs.utimesSync(temporary, stat.atime, stat.mtime)
      fs.renameSync(temporary, output)
    } finally {
      fs.rmSync(temporary, { force: true })
    }
    compressedByHash.set(hash, output)
    count++
    before += source.length
    after += fs.statSync(output).size
  }
}

walk(root)
console.log(
  `compress-assets: ${count} files, ${(before / 1e6).toFixed(2)} MB -> ${(after / 1e6).toFixed(2)} MB`
)

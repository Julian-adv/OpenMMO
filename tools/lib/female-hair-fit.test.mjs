import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import test from 'node:test'
import { femaleHairFit } from './female-hair-fit.mjs'

test('saves independent face/hair fits and rejects malformed requests', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'female-hair-fit-'))
  const target = pathToFileURL(join(directory, 'fits.json'))
  await writeFile(target, JSON.stringify({ default: {}, custom: {} }))
  const middleware = femaleHairFit(target)
  const server = createServer((req, res) => middleware(req, res, error => {
    res.writeHead(error ? 500 : 404); res.end()
  }))
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
  const url = `http://127.0.0.1:${server.address().port}/__female-preview/hair-fit.json`
  const fit = { 'move-x': 0, 'move-y': .5, 'move-z': 0, scale: 100,
    'scale-x': 100, 'scale-y': 100, 'scale-z': 100,
    'rotate-x': 0, 'rotate-y': 0, 'rotate-z': 0 }
  const post = (body, headers = {}) => fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: JSON.stringify(body) })
  try {
    const responses = await Promise.all(['default', 'custom'].flatMap(face => ['bob', 'long', 'ponytail'].map(hair => post({ face, hair, fit }))))
    assert.ok(responses.every(response => response.status === 200))
    const saved = await (await fetch(url)).json()
    for (const face of ['default', 'custom'])
      for (const hair of ['bob', 'long', 'ponytail']) assert.deepEqual(saved[face][hair], fit)
    assert.equal((await post({ face: 'custom', hair: 'bob', fit: { ...fit, 'move-y': 6 } })).status, 400)
    assert.equal((await post({ face: '__proto__', hair: 'bob', fit })).status, 400)
    assert.equal((await post({ face: 'custom', hair: 'none', fit })).status, 400)
    assert.equal((await post({ face: 'custom', hair: 'bob', fit }, { 'Sec-Fetch-Site': 'cross-site' })).status, 400)
    assert.equal((await post({ face: 'custom', hair: 'bob', fit, padding: 'a'.repeat(5000) })).status, 413)
    assert.deepEqual(JSON.parse(await readFile(target, 'utf8')), saved)
  } finally {
    await new Promise(resolve => server.close(resolve))
    await rm(directory, { recursive: true, force: true })
  }
})

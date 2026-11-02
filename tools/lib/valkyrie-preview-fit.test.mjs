import assert from 'node:assert/strict'
import { once } from 'node:events'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { createServer } from 'node:http'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import test from 'node:test'
import { valkyriePreviewFit } from './valkyrie-preview-fit.mjs'

test('saves fitting values, protects snapshots on reload, and validates replacements', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'valkyrie-fit-'))
  const destination = pathToFileURL(join(directory, 'fit.json'))
  const middleware = valkyriePreviewFit(destination)
  const server = createServer((req, res) => middleware(req, res, () => { res.writeHead(404); res.end() }))
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const url = `http://127.0.0.1:${server.address().port}/__female-preview/valkyrie-fit.json`
  const accepted = JSON.parse(await readFile(new URL('../../doc/assets/valkyrie-bracer-preview-fit.json', import.meta.url), 'utf8'))
  const send = (settings, automatic = false) => fetch(url, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ settings, hide_skin: true, automatic }),
  })
  try {
    assert.equal((await fetch(url)).status, 404)
    assert.equal((await (await send(accepted.settings, true)).json()).saved, true)
    const adjusted = { ...accepted.settings, scale: 110 }
    assert.equal((await (await send(adjusted, true)).json()).saved, false)
    assert.deepEqual((await (await fetch(url)).json()).settings, accepted.settings)
    assert.equal((await (await send(adjusted)).json()).saved, true)
    assert.deepEqual((await (await fetch(url)).json()).settings, adjusted)
    assert.equal((await send({ ...adjusted, scale: 500 })).status, 400)
    assert.equal((await send({})).status, 400)
    assert.deepEqual(JSON.parse(await readFile(destination, 'utf8')).settings, adjusted)
  } finally {
    server.closeAllConnections()
    await new Promise(resolve => server.close(resolve))
    await rm(directory, { recursive: true, force: true })
  }
})

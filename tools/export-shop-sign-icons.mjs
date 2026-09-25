import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { join, resolve } from 'node:path'
import { createServer } from '../client/node_modules/vite/dist/node/index.js'

const root = fileURLToPath(new URL('../', import.meta.url))
const output = resolve(process.argv[2] ?? join(root, 'assets/shop-sign-icons'))
await mkdir(output, { recursive: true })
const server = await createServer({
  root: join(root, 'client'),
  configFile: false,
  cacheDir: join(output, '.vite'),
  optimizeDeps: { noDiscovery: true, include: [] },
  server: { middlewareMode: true, hmr: false, watch: null },
})

try {
  const { buildShopSignBoard, getShopSignStyle, SHOP_SIGN_DEFAULTS } =
    await server.ssrLoadModule('/src/lib/utils/shop-sign.ts')
  const catalog = JSON.parse(await readFile(
    join(root, 'client/public/models/objects/catalog.json'), 'utf8'
  ))
  for (const definition of catalog.filter(entry => entry.procedural === 'shopSign')) {
    const params = { ...SHOP_SIGN_DEFAULTS, ...getShopSignStyle(definition.shopSignStyle).board }
    const board = buildShopSignBoard(params)
    const geometry = board.children[0].geometry
    const path = join(output, `${definition.id}.json`)
    await writeFile(path, JSON.stringify({
      texture: params.texture,
      positions: Array.from(geometry.getAttribute('position').array),
      uvs: Array.from(geometry.getAttribute('uv').array),
      indices: geometry.index ? Array.from(geometry.index.array) : null,
    }))
    geometry.dispose()
    console.log(path)
  }
} finally {
  await server.close()
}

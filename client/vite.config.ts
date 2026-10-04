/// <reference types="vitest/config" />
import fs from 'node:fs'
import { Agent } from 'node:https'
import { execSync } from 'node:child_process'
import { defineConfig, loadEnv, type Plugin } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import wasm from 'vite-plugin-wasm'
// @ts-expect-error no type declarations for .mjs
import { monsterCsvPlugin } from '../tools/vitePlugin.mjs'

// https://vite.dev/config/
function gitShortHash(): string {
  try {
    return execSync('git rev-parse --short HEAD', {
      stdio: ['ignore', 'pipe', 'ignore'],
    })
      .toString()
      .trim()
  } catch {
    return 'unknown'
  }
}

function landOwnershipPreview(): Plugin {
  const snapshot = new URL(
    '../data/land-ownership-preview.json',
    import.meta.url
  )
  return {
    name: 'land-ownership-preview',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (
          req.method !== 'GET' ||
          req.url?.split('?')[0] !== '/api/terrain/land-ownership'
        ) {
          next()
          return
        }
        fs.readFile(snapshot, (error, data) => {
          if (error?.code === 'ENOENT') {
            next()
            return
          }
          if (error) {
            next(error)
            return
          }
          res.setHeader('Content-Type', 'application/json')
          res.setHeader('Cache-Control', 'no-store')
          res.setHeader('X-Land-Ownership-Source', 'local-preview')
          res.end(data)
        })
      })
    },
  }
}

function modularCharacterPreview(): Plugin {
  const directory = new URL(
    '../assets/modular_human_male_01/rigged_hand_tuned/',
    import.meta.url
  )
  const files = new Set(['hand-grips.json', 'animations.glb'])
  const fitted = new URL(
    '../assets/modular_human_male_01/parts/fitted/',
    import.meta.url
  )
  const caveman = new URL(
    '../assets/modular_human_male_01/parts/caveman_tripo_top_v1/top_caveman.glb',
    import.meta.url
  )
  const cavemanPants = new URL(
    '../assets/modular_human_male_01/parts/caveman_tripo_pants_v1/pants_caveman.glb',
    import.meta.url
  )
  const cavemanBoots = new URL(
    '../assets/modular_human_male_01/parts/caveman_tripo_boots_v1/boots_caveman.glb',
    import.meta.url
  )
  const cavemanGloves = new URL(
    '../assets/modular_human_male_01/parts/caveman_tripo_bracer_v1/gloves_caveman.glb',
    import.meta.url
  )
  const selection = JSON.parse(
    fs.readFileSync(
      new URL(
        '../doc/assets/modular-rogue-source-selection.json',
        import.meta.url
      ),
      'utf8'
    )
  )
  const rogue = new URL(
    `../${selection.fitting_candidate.directory}/`,
    import.meta.url
  )
  const rogueParts = new Set([
    'top_rogue',
    'pants_rogue',
    'gloves_rogue',
    'boots_rogue',
  ])
  const parts = new Set([
    'base',
    'hair_crop',
    'hair_sidepart',
    'top_linen',
    'top_leather',
    'pants_cloth',
    'gloves_leather',
    'boots_leather',
    'top_plate',
    'pants_plate',
    'gloves_plate',
    'boots_plate',
    'helmet_plate',
    'top_barbarian',
    'pants_barbarian',
    'gloves_barbarian',
    'boots_barbarian',
    'helmet_barbarian',
  ])
  return {
    name: 'modular-character-preview',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const request = new URL(req.url ?? '/', 'http://localhost')
        const path = request.pathname
        const prefix = '/__modular-character/'
        if (!path.startsWith(prefix)) return next()
        const name = path.slice(prefix.length)
        const part = /^parts\/(\w+)\.glb$/.exec(name)
        const url =
          part?.[1] === 'top_caveman'
            ? caveman
            : part?.[1] === 'pants_caveman'
              ? cavemanPants
              : part?.[1] === 'boots_caveman'
                ? cavemanBoots
                : part?.[1] === 'gloves_caveman'
                  ? cavemanGloves
                  : part && rogueParts.has(part[1])
                    ? selection.fitting_candidate.part_overrides?.[part[1]]
                      ? new URL(
                          `../${selection.fitting_candidate.part_overrides[part[1]]}`,
                          import.meta.url
                        )
                      : new URL(`${part[1]}.glb`, rogue)
                    : part && parts.has(part[1])
                      ? new URL(`${part[1]}.glb`, fitted)
                      : files.has(name)
                        ? new URL(name, directory)
                        : null
        if (req.method !== 'GET' || !url) {
          res.statusCode = 404
          res.end('Unknown preview asset')
          return
        }
        fs.readFile(url, (error, data) => {
          res.setHeader('Cache-Control', 'no-store')
          if (error) {
            res.statusCode = error.code === 'ENOENT' ? 404 : 500
            res.end('Preview asset unavailable: ' + name)
            return
          }
          res.setHeader(
            'Content-Type',
            name.endsWith('.glb') ? 'model/gltf-binary' : 'application/json'
          )
          res.setHeader('Content-Length', data.length)
          res.end(data)
        })
      })
    },
  }
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')

  // Default to IPv4 explicitly: Node 18+ resolves 'localhost' to ::1 first,
  // which fails because the Rust server only listens on 127.0.0.1, causing
  // the proxy to reset every /ws and /api request.
  const backendHost = env.VITE_BACKEND_HOST ?? '127.0.0.1'
  const apiTarget = env.VITE_API_TARGET ?? `http://${backendHost}:10007`
  const wsTarget = env.VITE_WS_TARGET ?? `ws://${backendHost}:10006`
  const proxyAgent = env.VITE_PROXY_CA
    ? new Agent({
        ca: fs.readFileSync(env.VITE_PROXY_CA),
        ...(env.VITE_PROXY_SERVERNAME
          ? { servername: env.VITE_PROXY_SERVERNAME }
          : {}),
      })
    : undefined

  const httpsKey = env.VITE_HTTPS_KEY
  const httpsCert = env.VITE_HTTPS_CERT
  const httpsCa = env.VITE_HTTPS_CA
  const https =
    httpsKey && httpsCert
      ? {
          key: fs.readFileSync(httpsKey),
          cert: fs.readFileSync(httpsCert),
          ...(httpsCa ? { ca: fs.readFileSync(httpsCa) } : {}),
        }
      : undefined

  const hmrHost = env.VITE_HMR_HOST
  const hmrProtocol = env.VITE_HMR_PROTOCOL
  const hmr =
    hmrHost || hmrProtocol
      ? {
          ...(hmrHost ? { host: hmrHost } : {}),
          ...(hmrProtocol ? { protocol: hmrProtocol as 'ws' | 'wss' } : {}),
        }
      : undefined

  // The build's commit rides on the handshake version so the server log can
  // tell which bundle a session runs.
  const appVersion = `${
    JSON.parse(
      fs.readFileSync(new URL('./package.json', import.meta.url), 'utf8')
    ).version
  }+${gitShortHash()}`

  return {
    plugins: [
      landOwnershipPreview(),
      modularCharacterPreview(),
      monsterCsvPlugin(),
      wasm(),
      svelte(),
    ],
    define: { __APP_VERSION__: JSON.stringify(appVersion) },
    server: {
      host: true,
      port: 10004,
      strictPort: true,
      https,
      hmr,
      // No global Cache-Control here: it only ever applied to transformed
      // source modules (proxied /api responses bypass server.headers), where
      // an hour of max-age serves stale modules after HMR/scp churn — wasm
      // export errors, split store singletons. Vite's default ETag/304
      // revalidation is the right policy for dev.
      proxy: {
        // All REST endpoints share one backend, so a single prefix covers them.
        '/api': { target: apiTarget, changeOrigin: true, agent: proxyAgent },
        '/ws': {
          target: wsTarget,
          ws: true,
          changeOrigin: true,
          agent: proxyAgent,
        },
      },
    },
    build: { target: 'esnext' },
    optimizeDeps: { esbuildOptions: { target: 'esnext' } },
    test: { setupFiles: ['./src/test-setup.ts'] },
  }
})

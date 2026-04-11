import fs from 'node:fs'
import { defineConfig, loadEnv } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import wasm from 'vite-plugin-wasm'
// @ts-expect-error no type declarations for .mjs
import { monsterCsvPlugin } from '../tools/vitePlugin.mjs'

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')

  const backendHost = env.VITE_BACKEND_HOST ?? 'localhost'
  const apiTarget = `http://${backendHost}:10007`
  const wsTarget = `ws://${backendHost}:10006`

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

  return {
    plugins: [monsterCsvPlugin(), wasm(), svelte()],
    server: {
      host: true,
      port: 10004,
      https,
      hmr,
      headers: {
        'Cache-Control': 'public, max-age=3600',
      },
      proxy: {
        '/api/terrain': { target: apiTarget, changeOrigin: true },
        '/api/housing': { target: apiTarget, changeOrigin: true },
        '/api/npcs': { target: apiTarget, changeOrigin: true },
        '/ws': { target: wsTarget, ws: true, changeOrigin: true },
      },
    },
    build: { target: 'esnext' },
    optimizeDeps: { esbuildOptions: { target: 'esnext' } },
  }
})

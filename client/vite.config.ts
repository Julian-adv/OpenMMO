import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import wasm from 'vite-plugin-wasm'
// @ts-expect-error no type declarations for .mjs
import { monsterCsvPlugin } from '../tools/vitePlugin.mjs'

// https://vite.dev/config/
export default defineConfig({
  plugins: [monsterCsvPlugin(), wasm(), svelte()],
  server: {
    host: true,
    port: 10004,
    https: {
      key: './node_modules/.vite-ssl/key.pem',
      cert: './node_modules/.vite-ssl/cert.pem',
    },
    headers: {
      'Cache-Control': 'public, max-age=3600',
    },
    proxy: {
      '/api/terrain': {
        target: 'http://localhost:10016',
        changeOrigin: true,
      },
      '/api/housing': {
        target: 'http://localhost:10016',
        changeOrigin: true,
      },
      '/api/npcs': {
        target: 'http://localhost:10016',
        changeOrigin: true,
      },
      '/ws': {
        target: 'ws://localhost:10015',
        ws: true,
        changeOrigin: true,
      },
    },
  },
  build: { target: 'esnext' },
  optimizeDeps: { esbuildOptions: { target: 'esnext' } },
})

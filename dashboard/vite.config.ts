import { defineConfig, loadEnv } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { loadDashboardEnv } from './env.mjs'

export default defineConfig(({ mode }) => {
  const env = loadDashboardEnv(loadEnv, mode, process.cwd())
  const proxy = {
    '/api/metrics': {
      target: env.DASHBOARD_API_TARGET || 'http://127.0.0.1:10007',
      changeOrigin: true,
    },
  }

  return {
    base: env.DASHBOARD_BASE || '/',
    define: { 'import.meta.env.VITE_GOOGLE_CLIENT_ID': JSON.stringify(env.VITE_GOOGLE_CLIENT_ID) },
    plugins: [svelte()],
    server: { host: '127.0.0.1', port: 10008, strictPort: true, proxy },
    preview: { host: '127.0.0.1', port: 10008, strictPort: true, proxy },
  }
})

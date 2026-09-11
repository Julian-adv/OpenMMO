import { defineConfig, loadEnv } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const proxy = {
    '/api/metrics': {
      target: env.DASHBOARD_API_TARGET || 'http://127.0.0.1:10007',
      changeOrigin: true,
    },
  }

  return {
    base: env.DASHBOARD_BASE || '/',
    plugins: [svelte()],
    server: { host: '127.0.0.1', port: 10008, strictPort: true, proxy },
    preview: { host: '127.0.0.1', port: 10008, strictPort: true, proxy },
  }
})

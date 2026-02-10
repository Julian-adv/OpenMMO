import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
// @ts-expect-error no type declarations for .mjs
import { monsterCsvPlugin } from '../data/vitePlugin.mjs'

// https://vite.dev/config/
export default defineConfig({
  plugins: [monsterCsvPlugin(), svelte()],
})

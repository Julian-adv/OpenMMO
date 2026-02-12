import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import wasm from 'vite-plugin-wasm'
// @ts-expect-error no type declarations for .mjs
import { monsterCsvPlugin } from '../data/vitePlugin.mjs'

// https://vite.dev/config/
export default defineConfig({
  plugins: [monsterCsvPlugin(), wasm(), svelte()],
})

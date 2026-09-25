import { mount } from 'svelte'
import './app.css'
import './lib/utils/assetUrl'
import App from './App.svelte'
import { registerAssetServiceWorker } from './lib/utils/assetServiceWorker'

if (import.meta.env.PROD) void registerAssetServiceWorker()

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app

import { getCachedAssetUrls } from './assetUrl'

const MANIFEST_MESSAGE = 'openmmo:asset-manifest'

export async function registerAssetServiceWorker(): Promise<void> {
  if (!('serviceWorker' in navigator)) return

  try {
    const message = {
      type: MANIFEST_MESSAGE,
      urls: getCachedAssetUrls(),
    }
    const notified = new WeakSet<ServiceWorker>()
    const sendManifest = (worker: ServiceWorker | null): void => {
      if (!worker || notified.has(worker)) return
      worker.postMessage(message)
      notified.add(worker)
    }
    const registration = await navigator.serviceWorker.register(
      '/model-service-worker.js',
      { scope: '/', updateViaCache: 'none' }
    )

    const workers = [
      registration.active,
      registration.waiting,
      registration.installing,
    ]
    workers.forEach(sendManifest)

    navigator.serviceWorker.addEventListener(
      'controllerchange',
      () => sendManifest(navigator.serviceWorker.controller),
      { once: true }
    )

    void navigator.serviceWorker.ready
      .then((readyRegistration) => sendManifest(readyRegistration.active))
      .catch(() => {})
  } catch (error) {
    console.warn('Asset service worker registration failed', error)
  }
}

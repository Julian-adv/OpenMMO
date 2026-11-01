/// <reference types="vitest/config" />
import fs from 'node:fs'
import { Agent } from 'node:https'
import { execSync } from 'node:child_process'
import { defineConfig, loadEnv, type Plugin } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import wasm from 'vite-plugin-wasm'
// @ts-expect-error no type declarations for .mjs
import { monsterCsvPlugin } from '../tools/vitePlugin.mjs'
// @ts-expect-error no type declarations for .mjs
import { femalePreviewPlugin } from '../tools/femalePreviewPlugin.mjs'

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
    '../assets/modular_human_male_01/animations/',
    import.meta.url
  )
  const files = new Set(['hand-grips.json', 'animations.glb'])
  const part = (path: string) =>
    new URL(`../assets/modular_human_male_01/${path}`, import.meta.url)
  const fitted = part('fitted/')
  const faces: Record<string, URL> = {
    rugged: part('faces/tripo_rugged_v1/base_rugged.glb'),
    ranger: part('faces/tripo_ranger_v1/base_ranger.glb'),
  }
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
  const fixed: Record<string, URL> = {
    female_valkyrie_wings: new URL(
      '../assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v1/wings-front-normals-preview.glb',
      import.meta.url
    ),
    ysolde_ground_mattress: new URL(
      '../assets/items/ysolde_camp_bed/ground-mattress.glb',
      import.meta.url
    ),
    ysolde: new URL('../assets/npcs/ysolde/rig_v1/ysolde.glb', import.meta.url),
    barkan: new URL(
      '../assets/monsters/barkan/rig_v1/barkan.glb',
      import.meta.url
    ),
    terraclash: new URL(
      '../assets/monsters/terraclash/rig_v1/terraclash.glb',
      import.meta.url
    ),
    helmet_brilliance: new URL(
      '../assets/items/helm_of_brilliance/tripo_v2/fitted_v3/male/helmet_brilliance.glb',
      import.meta.url
    ),
    'male-brilliance-helmet-occlusion.json': new URL(
      '../assets/items/helm_of_brilliance/tripo_v2/fitted_v3/male/body-occlusion.json',
      import.meta.url
    ),
    female_brilliance_helmet: new URL(
      '../assets/items/helm_of_brilliance/tripo_v2/fitted_v3/female/helmet_brilliance.glb',
      import.meta.url
    ),
    'female-brilliance-helmet-occlusion.json': new URL(
      '../assets/items/helm_of_brilliance/tripo_v2/fitted_v3/female/body-occlusion.json',
      import.meta.url
    ),
    female_knight_boots: new URL(
      '../assets/items/female_knight_boots/fitted_v5/boots_female_knight.glb',
      import.meta.url
    ),
    'female-knight-boots-occlusion.json': new URL(
      '../assets/items/female_knight_boots/fitted_v5/body-occlusion.json',
      import.meta.url
    ),
    female_knight_bottom: new URL(
      '../assets/items/female_knight_bottom/tripo_v1/fitted_v42/bottom_female_knight.glb',
      import.meta.url
    ),
    'female-knight-bottom-occlusion.json': new URL(
      '../assets/items/female_knight_bottom/tripo_v1/fitted_v42/body-occlusion.json',
      import.meta.url
    ),
    female_knight_top: new URL(
      '../assets/items/female_knight_top/tripo_v1/fitted_v3/top_female_knight.glb',
      import.meta.url
    ),
    female_barbarian_top: new URL(
      '../assets/items/female_barbarian_top/tripo_v1/fitted_v1/top_female_barbarian.glb',
      import.meta.url
    ),
    female_caveman_bottom: new URL(
      '../assets/items/female_caveman_bottom/tripo_v1/fitted_v9/bottom_female_caveman.glb',
      import.meta.url
    ),
    'female-caveman-bottom-occlusion.json': new URL(
      '../assets/items/female_caveman_bottom/tripo_v1/fitted_v9/body-occlusion.json',
      import.meta.url
    ),
    female_caveman_boots: new URL(
      '../assets/items/female_caveman_boots/tripo_v1/fitted_v4/boots_female_caveman.glb',
      import.meta.url
    ),
    'female-caveman-boots-occlusion.json': new URL(
      '../assets/items/female_caveman_boots/tripo_v1/fitted_v4/body-occlusion.json',
      import.meta.url
    ),
    female_caveman_gloves: new URL(
      '../assets/items/female_caveman_bracers/tripo_v1/fitted_v1/gloves_female_caveman.glb',
      import.meta.url
    ),
    'female-caveman-gloves-occlusion.json': new URL(
      '../assets/items/female_caveman_bracers/tripo_v1/fitted_v1/body-occlusion.json',
      import.meta.url
    ),
    female_caveman_top: new URL(
      '../assets/items/female_caveman_top/tripo_v1/fitted_v7/top_female_caveman.glb',
      import.meta.url
    ),
    'female-caveman-top-occlusion.json': new URL(
      '../assets/items/female_caveman_top/tripo_v1/fitted_v7/body-occlusion.json',
      import.meta.url
    ),
    female_rogue_top: new URL(
      '../assets/items/female_rogue_top/tripo_v1/fitted_v6/top_female_rogue.glb',
      import.meta.url
    ),
    female_rogue_boots: new URL(
      '../assets/items/female_rogue_boots/tripo_v1/fitted_v5/boots_female_rogue.glb',
      import.meta.url
    ),
    female_rogue_garters: new URL(
      '../assets/items/female_rogue_garters/fitted_v1/garters_female_rogue.glb',
      import.meta.url
    ),
    female_rogue_gloves: new URL(
      '../assets/items/female_rogue_gloves/tripo_v1/fitted_v1/gloves_female_rogue.glb',
      import.meta.url
    ),
    female_rogue_glove_source: new URL(
      '../assets/items/female_rogue_gloves/tripo_v1/source/model_url.glb',
      import.meta.url
    ),
    'female-rogue-gloves-occlusion.json': new URL(
      '../assets/items/female_rogue_gloves/tripo_v1/fitted_v1/body-occlusion.json',
      import.meta.url
    ),
    'female-rogue-boots-occlusion.json': new URL(
      '../assets/items/female_rogue_boots/tripo_v1/fitted_v5/body-occlusion.json',
      import.meta.url
    ),
    'female-rogue-top-occlusion.json': new URL(
      '../assets/items/female_rogue_top/tripo_v1/fitted_v6/body-occlusion.json',
      import.meta.url
    ),
    female_rogue_bottom: new URL(
      '../assets/items/female_rogue_skirt/tripo_v1/fitted_v6/bottom_female_rogue.glb',
      import.meta.url
    ),
    'female-rogue-bottom-occlusion.json': new URL(
      '../assets/items/female_rogue_skirt/tripo_v1/fitted_v6/body-occlusion.json',
      import.meta.url
    ),
    'female-barbarian-top-occlusion.json': new URL(
      '../assets/items/female_barbarian_top/tripo_v1/fitted_v1/body-occlusion.json',
      import.meta.url
    ),
    female_barbarian_boots: new URL(
      '../assets/items/female_barbarian_boots/tripo_v1/fitted_v3/boots_female_barbarian.glb',
      import.meta.url
    ),
    'female-barbarian-boots-occlusion.json': new URL(
      '../assets/items/female_barbarian_boots/tripo_v1/fitted_v3/body-occlusion.json',
      import.meta.url
    ),
    female_barbarian_bottom: new URL(
      '../assets/items/female_barbarian_bottom/tripo_v1/fitted_v1/bottom_female_barbarian.glb',
      import.meta.url
    ),
    'female-barbarian-bottom-occlusion.json': new URL(
      '../assets/items/female_barbarian_bottom/tripo_v1/fitted_v1/body-occlusion.json',
      import.meta.url
    ),
    female_barbarian_gloves: new URL(
      '../assets/items/female_barbarian_bracers/tripo_v1/fitted_v1/gloves_female_barbarian.glb',
      import.meta.url
    ),
    'female-barbarian-gloves-occlusion.json': new URL(
      '../assets/items/female_barbarian_bracers/tripo_v1/fitted_v1/body-occlusion.json',
      import.meta.url
    ),
    female_barbarian_helmet: new URL(
      '../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/helmet_female_barbarian.glb',
      import.meta.url
    ),
    female_barbarian_helmet_hair_bob: new URL(
      '../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/hair_bob_barbarian_helmet.glb',
      import.meta.url
    ),
    female_barbarian_helmet_hair_knight: new URL(
      '../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/hair_knight_barbarian_helmet.glb',
      import.meta.url
    ),
    'female-barbarian-helmet-occlusion.json': new URL(
      '../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/body-occlusion.json',
      import.meta.url
    ),
    female_knight_gloves: new URL(
      '../assets/items/female_knight_gloves/tripo_v1/fitted_v3/gloves_female_knight.glb',
      import.meta.url
    ),
    female_knight_helmet: new URL(
      '../assets/items/female_knight_helmet/tripo_v1/fitted_v1/helmet_female_knight.glb',
      import.meta.url
    ),
    'female-knight-helmet-occlusion.json': new URL(
      '../assets/items/female_knight_helmet/tripo_v1/fitted_v1/body-occlusion.json',
      import.meta.url
    ),
    'female-knight-gloves-occlusion.json': new URL(
      '../assets/items/female_knight_gloves/tripo_v1/fitted_v3/body-occlusion.json',
      import.meta.url
    ),
    'female-knight-gloves-top-occlusion.json': new URL(
      '../assets/items/female_knight_gloves/tripo_v1/fitted_v3/top-occlusion.json',
      import.meta.url
    ),
    'female-knight-top-occlusion.json': new URL(
      '../assets/items/female_knight_top/tripo_v1/fitted_v3/body-occlusion.json',
      import.meta.url
    ),
    female_base: new URL(
      '../assets/modular_human_female_01/rig_v1/female-base.glb',
      import.meta.url
    ),
    female_knight_hair: new URL(
      '../assets/modular_human_female_01/hair/tripo_knight_v1/fitted_v1/hair_female_knight.glb',
      import.meta.url
    ),
    female_knight_bob_source: new URL(
      '../assets/modular_human_female_01/hair/tripo_knight_bob_v1/model_url.glb',
      import.meta.url
    ),
    female_knight_bob: new URL(
      '../assets/modular_human_female_01/hair/tripo_knight_bob_v1/fitted_v1/hair_female_knight_bob.glb',
      import.meta.url
    ),
    helmet_bard: part('bard/tripo_hat_v1/helmet_bard.glb'),
    top_bard: part('bard/tripo_top_v1/top_bard.glb'),
    pants_bard: part('bard/tripo_pants_v1/pants_bard.glb'),
    boots_bard: part('bard/tripo_boots_v1/boots_bard.glb'),
    helmet_priest: part('priest/tripo_helmet_v1/helmet_priest.glb'),
    top_priest: part('priest/tripo_top_v1/top_priest.glb'),
    pants_priest: part('priest/tripo_pants_v1/pants_priest.glb'),
    boots_priest: part('priest/tripo_boots_v1/boots_priest.glb'),
    top_ranger: part('ranger/tripo_top_v4/top_ranger.glb'),
    pants_ranger: part('ranger/tripo_pants_v1/pants_ranger.glb'),
    boots_ranger: part('ranger/tripo_boots_v1/boots_ranger.glb'),
    gloves_ranger: part('ranger/tripo_gloves_v1/gloves_ranger.glb'),
    top_caveman: part('caveman/tripo_top_v1/top_caveman.glb'),
    pants_caveman: part('caveman/tripo_pants_v1/pants_caveman.glb'),
    boots_caveman: part('caveman/tripo_boots_v1/boots_caveman.glb'),
    gloves_caveman: part('caveman/tripo_bracer_v1/gloves_caveman.glb'),
    hair_wavy_bone: part('hair/tripo_wavy_v1/hair_wavy_bone.glb'),
    hair_ranger: part('hair/tripo_ranger_v1/hair_ranger.glb'),
  }
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
        const id = part?.[1]
        const override = id && selection.fitting_candidate.part_overrides?.[id]
        const face = request.searchParams.get('face') ?? ''
        const url =
          id === 'base' && Object.hasOwn(faces, face)
            ? faces[face]
            : id && Object.hasOwn(fixed, id)
              ? fixed[id]
              : Object.hasOwn(fixed, name)
                ? fixed[name]
                : id && rogueParts.has(id)
                  ? override
                    ? new URL(`../${override}`, import.meta.url)
                    : new URL(`${id}.glb`, rogue)
                  : id && parts.has(id)
                    ? new URL(`${id}.glb`, fitted)
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

  // Match the Rust server's IPv4 binding.
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
      femalePreviewPlugin(),
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
    build: {
      target: 'esnext',
      rollupOptions: {
        output: {
          onlyExplicitManualChunks: true,
          manualChunks(id) {
            if (id.includes('/node_modules/three/build/three.core.js'))
              return 'three-core'
            if (id.includes('/node_modules/three/build/three.webgpu.js'))
              return 'three-webgpu'
            if (id.includes('/node_modules/three/build/three.module.js'))
              return 'three-webgl'
            if (id.includes('/node_modules/@threlte/')) return 'threlte'
            if (
              id.includes('/node_modules/svelte/') ||
              id.includes('/node_modules/i18next/')
            )
              return 'vendor'
          },
        },
      },
    },
    optimizeDeps: { esbuildOptions: { target: 'esnext' } },
    test: { setupFiles: ['./src/test-setup.ts'] },
  }
})

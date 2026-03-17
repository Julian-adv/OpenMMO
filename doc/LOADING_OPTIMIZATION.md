# Loading & Shader Compilation Optimization

## Problem

When entering the game, the player experienced ~40 seconds of unresponsive waiting with no loading indicator. The root cause was **WebGPU pipeline compilation** — Three.js TSL (Three Shading Language) node materials generate WGSL shader code that the browser's GPU compiler must compile into render pipelines on first use. Each unique material × render pass (forward, CSM shadow × 2 cascades, point light shadow) requires a separate pipeline.

## Results

| Optimization | Loading Time | Reduction |
|---|---|---|
| Baseline (before any changes) | ~40s | — |
| + Loading dialog + data loading improvements | ~40s | UX fix (dialog visible) |
| + Merged ORM shader pass | ~37s | -3s |
| + 2×2 terrain tile grid (9→4 tiles) | ~34s | -3s |
| + Pre-baked water noise texture | ~18s | -16s |
| + Larger grass sub-chunks (25→9 per type) | ~15s | -3s |
| **Total** | **~15s** | **-25s (62%)** |

## Changes

### 1. Loading Dialog & Data Pipeline (UX)

**Problem**: `isSceneCompiling` was gated only on `loadSplatLayers()`, which resolved instantly (cached from character select). The loading dialog disappeared after ~1 frame while 35+ seconds of GPU compilation happened with no feedback.

**Fix**:
- Gate `isSceneCompiling` on **smooth frame detection** — monitor `rawDeltaTime` in the game loop and only hide the dialog after 3 consecutive frames under 100ms
- Show "Loading..." during `isCurrentPlayerLoading` (player model + animation loading)
- Pre-fetch all tile heightmaps in parallel during `onMount`
- Drain all terrain tiles and work queue items immediately (no 1-per-frame staggering)
- Defer refraction/reflection/wetness render passes for 5 frames after dialog closes

**Files**: `GameScene.svelte`, `App.svelte`

### 2. Pre-baked Caustics Texture

**Problem**: `generateCausticsTexture()` ran synchronously on the main thread during `onMount`, computing a 256×256 Voronoi distance field (128 cells × 3×3 tiled grid × 65K pixels).

**Fix**: Pre-generated the texture as a static PNG (`/textures/caustics.png`). The function is deterministic with a fixed seed, so the output never changes.

**Files**: `caustics-gen.ts`, `GameScene.svelte`

### 3. Reactive Camera Initialization

**Problem**: Camera init used a hardcoded 1.1s `setTimeout` to wait for the camera ref to be bound.

**Fix**: Replaced with a Svelte 5 `$effect` that fires as soon as both `camera` and `currentPlayer` are available.

**Files**: `GameScene.svelte`

### 4. Merged ORM Shader Pass (-3s)

**Problem**: The terrain splat material had 3 separate `Fn()` nodes for roughness, metalness, and AO. Each independently sampled the ORM atlas 4 times = 12 ORM texture samples total. Each also called `getWeights()` and computed UV derivatives independently.

**Fix**:
- Merged into a single `Fn()` that samples the ORM atlas 4 times and returns `vec3(ao, roughness, metalness)` — material properties read `.r`, `.g`, `.b` from the shared result
- Hoisted `getWeights()`, `uv()`, `dFdx()`, `dFdy()` as shared fragment-level nodes, reused across color/normal/ORM nodes

**Result**: 25 → 13 texture samples per fragment, 5 → 1 `getWeights()` evaluations.

**Files**: `makeSplatStandardMaterial.ts`

### 5. 2×2 Terrain Tile Grid (-3s)

**Problem**: 3×3 grid = 9 terrain tiles, each with its own material instance. Each material needed forward + shadow pass pipeline compilation.

**Fix**: Reduced to 2×2 grid (4 tiles) using `Math.floor`-based chunk positioning. The 4 nearest tiles always surround the player. With 64-unit tiles and a ~50×35 orthographic viewport, 4 tiles (128×128 coverage) is more than sufficient.

**Result**: 5 fewer terrain materials, 5 fewer water tiles, fewer shadow draw calls.

**Files**: `terrain-utils.ts`, `GameScene.svelte`, `terrainHeightManager.ts`

### 6. Pre-baked Water Noise Texture (-16s, largest win)

**Problem**: The water shader used procedural `valueNoise()` — a hash-based noise function called 8 times in the fragment shader. Each call inlined the `hash()` function (sin, dot, fract) 4 times + bilinear interpolation. This generated massive WGSL code that dominated pipeline compilation time.

**Fix**: Pre-baked a 512×512 tileable value noise texture (`/textures/value-noise.jpg`) with 64 noise periods and smoothstep interpolation baked in. Added `sampleNoise()` helper that maps noise-space coordinates to texture UV (divides by period count). This completely removes the `hash()` and `valueNoise()` Fn() definitions from the WGSL shader.

**Key detail**: UV scaling must account for the texture's period count. `sampleNoise(p)` divides `p` by 64 (the number of noise periods) so that the texture maps identically to the original procedural `valueNoise(p)`.

**Result**: ~120 ALU instructions removed from WGSL, replaced with 8 simple texture samples.

**Files**: `water-material.ts`, `value-noise.jpg`

### 7. Larger Grass Sub-chunks (-3s)

**Problem**: 25 grass sub-chunks per type (5×5 grid of 16-unit chunks) × 3 types = 75 InstancedMesh draw calls per frame, plus shadow passes.

**Fix**: Increased sub-chunk size from 16 to 32 units, reduced grid from 5×5 to 3×3 (radius 1). Mesh capacity scaled 4× to handle the larger area per chunk.

**Result**: 75 → 27 draw calls per frame. Coverage 96×96 world units (sufficient for viewport).

**Files**: `GameSceneGrassLayer.svelte`

## Architecture Notes

### Why WebGPU Pipeline Compilation Is Slow

- TSL node materials generate WGSL shader code at runtime
- Each unique material × render pass combination needs a separate GPU pipeline
- Render passes: forward, CSM shadow (2 cascades), point light shadow = 4 pipeline variants per material
- The browser's GPU compiler (Dawn/Tint for Chrome) compiles WGSL → native GPU instructions
- `renderer.compileAsync()` submits compilation but resolves before GPU finishes
- Actual compilation happens synchronously during Threlte's render loop via `createRenderPipeline`

### What Doesn't Help

- **Disabling shadows during loading then re-enabling**: Causes full recompilation of all materials (shader variants change when lighting config changes). Made loading 50% slower.
- **Setting `scene.visible = false` during `compileAsync`**: `compileAsync` skips invisible objects, so nothing gets compiled.
- **Reducing grass trail/gust loop iterations**: Only saved ~1s — not worth the visual trade-off.

### Browser Pipeline Caching

Per the W3C WebGPU spec, browsers cache compiled pipelines between sessions. The second visit should be faster than the first. This is implementation-specific (Chrome uses Dawn's pipeline cache).

### Three.js Known Issues

- [TSL: Slow compilation time (#31674)](https://github.com/mrdoob/three.js/issues/31674) — bind group layout cache bug, partially fixed in r180
- [WebGPU Node system startup (#26820)](https://github.com/mrdoob/three.js/issues/26820) — ~400ms per material for node traversal, fixed in earlier PRs
- [WebGPU Renderer slower than WebGL (#31055)](https://github.com/mrdoob/three.js/issues/31055) — known issue with unbatched meshes

### Remaining 15s Breakdown

The remaining loading time is dominated by GPU-side pipeline compilation for unique material types:
- Terrain splat material (forward + shadow variants)
- Water material (forward only, no shadow)
- Grass materials × 2 unique pipelines (grass + flower, forward + shadow variants)
- Player model material (forward + shadow variants)
- Wetness capture (reuses water material pipeline)

Further reduction would require simplifying these materials (fewer texture samples, simpler node graphs) or waiting for Three.js/browser improvements in pipeline compilation speed.

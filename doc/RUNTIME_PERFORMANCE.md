# Runtime Performance Optimization (60fps)

## Problem

Heavy scene (4-story buildings x2, 1-story houses x3, trees, grass, character) showing 55fps instead of target 60fps. Frame budget: 16.67ms, actual: ~18.2ms — needed to save ~1.5ms per frame.

## Results

| Optimization | FPS | Improvement |
|---|---|---|
| Baseline | 55 | -- |
| + Dynamic grass compute count | 57 | +2 |
| + Remove terrain castShadow | 58-60 | +2 |
| + Remove door/shutter castShadow | 60 (stable) | +1 |

## Render Pipeline (per frame)

The game renders multiple passes per frame:

1. **Update logic** (CPU): player, animations, grass compute dispatch, housing detection
2. **Wetness pass**: 256x256 RT per water tile (negligible)
3. **Refraction pass**: half-res render -- terrain + housing (hides water, entities, grass, trees)
4. **Reflection pass**: half-res render -- entities only (hides terrain, water, housing, grass, trees)
5. **Shadow pass**: CSM 2 cascades x 2048x2048 -- all castShadow objects
6. **Main render**: full scene at full resolution

## Changes

### 1. Dynamic Grass Compute Dispatch Count

**File**: `client/src/lib/components/game-scene/GameSceneGrassLayer.svelte`

**Problem**: Grass wind simulation uses GPU compute shaders dispatched per sub-chunk (3x3 grid = 9 sub-chunks x 3 types = up to 27 dispatches). Each dispatch was fixed at full buffer capacity (131,072 for short/tall grass, 2,048 for flowers) regardless of actual blade count. If a sub-chunk had 5,000 blades, 126,000 GPU threads were wasted.

**Fix**: Set `computeUpdate.count` to actual blade count before each dispatch.

```typescript
// Before: always dispatches capacity (131K) threads
renderer.compute(slot.ctx.computeUpdate)

// After: dispatch only actual blade count
;(slot.ctx.computeUpdate as { count: number }).count = slot.ctx.count
renderer.compute(slot.ctx.computeUpdate)
```

Three.js `ComputeNode.count` is dynamically writable (`ComputeNode.js:setCount()`). The buffer stays allocated at full capacity, but only active indices are processed. Safe because unused indices' output is never read (`mesh.count` limits rendering).

### 2. Remove Terrain Shadow Casting

**File**: `client/src/lib/components/SplatTerrain.svelte`

**Problem**: All 9 splat terrain tiles had `castShadow = true`. Terrain is mostly flat ground -- it doesn't need to cast shadows onto other objects. Each tile was rendered into the shadow map for both CSM cascades = 18 unnecessary shadow draw calls.

**Fix**: Remove `castShadow` from SplatTerrain, keep `receiveShadow` so terrain still receives shadows from trees, buildings, etc.

### 3. Remove Door/Shutter Shadow Casting

**File**: `client/src/lib/utils/house-geo-walls.ts`

**Problem**: Every door panel and window shutter was an individual mesh with `castShadow = true`. A 4-story building can have many doors and windows, each creating 1-2 shadow draw calls x 2 CSM cascades. These tiny objects produce shadows invisible in isometric view.

**Fix**: Remove `castShadow` from door panels and window shutters. The building walls (merged meshes) still cast shadows normally.

## Profiling Tools

- **Loop profiler**: `GameScene.svelte` has a built-in profiler tracking per-section CPU time (grassUpdate, refractionPass, reflectionPass, housingUpdate, etc.). Enable via `setLoopProfileEnabled(true)` on the game scene context. Output goes to browser console as `[LoopProfile]` grouped tables every 1 second.
- **Browser DevTools**: Chrome Performance tab shows GPU timing. Look for long "GPU" blocks in the flame chart.
- **renderer.info**: Three.js renderer exposes draw call and triangle counts per render call (auto-resets each `render()`).

## Key Findings

- **GPU-bound, not CPU-bound**: CPU-side optimizations (skipping render pass submissions, reducing JS work) had minimal impact. GPU workload reduction (fewer compute threads, fewer shadow draw calls) had direct impact.
- **Shadow maps are expensive**: CSM with 2 cascades doubles shadow rendering. Every `castShadow = true` mesh is rendered once per cascade. Small objects (doors, shutters) and flat geometry (terrain) should not cast shadows unless visually necessary.
- **Compute dispatch count matters**: WebGPU compute dispatches process all threads up to the specified count. If actual data is a fraction of buffer capacity, most GPU threads run on empty data. Always set dispatch count to actual work size.

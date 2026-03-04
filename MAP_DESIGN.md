# Procedural Terrain Generation Spec

## Overview

맵 에디터에서 현재 리전(16x16 타일 = 1024x1024 셀)을 절차적으로 생성하는 기능.
산, 평지, 강, 바다를 포함하는 자연스러운 지형을 노이즈 기반으로 생성한다.

## Terrain System Reference

| Item | Value |
|------|-------|
| Region size | 16 x 16 tiles |
| Tile size | 64 x 64 cells (65x65 vertices) |
| Region cells | 1024 x 1024 |
| Height encoding | `Uint16`: `encode(m) = round((m + 500) / 0.05)` |
| Height range | -500.0 m ~ +3276.0 m (0.05 m precision) |
| Sea level | 0.0 m (encoded = 10000) |
| Splatmap | `Uint8Array` 64x64x4 (RGBA), values sum to 255 |
| Save format | Binary PUT per tile (height: 8192 B, splat: 16384 B) |
| Region meta | JSON, 4 layers `{ texture, tileScale }` |

## Generation Parameters

| Parameter | Type | Range | Default | Description |
|-----------|------|-------|---------|-------------|
| `seed` | number | any integer | random | Noise seed for reproducibility |
| `minHeight` | number | -500.0 ~ 0.0 m | -20 | Lowest generated height (sea floor) |
| `maxHeight` | number | 0.0 ~ 3276.0 m | 80 | Highest generated height (mountain peak) |
| `seaProportion` | number | 0 ~ 0.6 | 0.25 | Fraction of cells classified as sea |
| `plainProportion` | number | 0 ~ 0.8 | 0.45 | Fraction classified as plains |
| `mountainProportion` | number | 0 ~ 0.6 | 0.25 | Fraction classified as mountains |
| `riverCount` | number | 0 ~ 5 | 2 | Number of rivers to carve |

Proportions are auto-normalized to sum = 1.0.

## Algorithm

### 1. Base Elevation (fBm Simplex Noise)

- 6-octave fractional Brownian motion
- Lacunarity: 2.0, Persistence: 0.5
- Base frequency: 1/512 (large-scale features spanning multiple tiles)
- Noise coordinates use world position: `(regionX * 1024 + cx, regionZ * 1024 + cz)`
  - Same seed produces continuous terrain across adjacent regions

### 2. Quantile-Based Classification

Sort all 1,048,576 height values to find percentile thresholds matching user proportions:
- Sea cells (bottom `seaProportion` %): remap to `[minHeight, -0.5 m]`
- Plain cells (middle `plainProportion` %): remap to `[0.5 m, 10 m]`
- Mountain cells (top `mountainProportion` %): remap to `[10 m, maxHeight]`

This guarantees exact proportion distribution regardless of noise characteristics.

### 3. River Carving

1. Find candidate starting points in mountain areas (height > sea level + 15 m)
2. For each river, follow gradient descent to sea:
   - At each step, move to the lowest neighbor (3x3 kernel)
   - 20% random lateral drift for natural meandering
   - Terminate at sea level or if path loops
3. Carve channel along path:
   - Width: 2-3 cells
   - Depth: slightly below sea level (smooth Gaussian cross-section)
   - Channel widens toward sea

### 4. Coastline Smoothing

1. BFS flood fill from all sea cells → coast distance field (per-cell distance to nearest water)
2. Identify coastline band: cells within 6 cells of water boundary
3. Apply Gaussian blur (radius=4, sigma=2.0) only to coastline-band cells
4. Result: smooth, gradual transition from land to sea

### 5. Region Boundary Blending

인접 리전과의 경계를 매끄럽게 연결:

1. **Noise continuity**: 월드 좌표 기반 노이즈 → 동일 seed면 인접 리전 생성 시 경계 자동 일치
2. **Existing neighbor blending**: 생성 전 인접 4방향 리전의 경계 타일 높이를 서버에서 로드
   - 존재하면 경계 16셀에 걸쳐 lerp 블렌딩:
     - Edge (distance=0): 기존 높이 100%
     - 16 cells inward: 생성 높이 100%
     - Between: linear interpolation
   - 존재하지 않으면 스킵 (나중에 인접 리전 생성 시 동일 seed 노이즈로 자연스럽게 연결)

```
interface NeighborEdgeData {
  north?: Uint16Array[]   // 16 tiles × 64 cell bottom row
  south?: Uint16Array[]   // 16 tiles × 64 cell top row
  east?:  Uint16Array[]   // 16 tiles × 64 cell left column
  west?:  Uint16Array[]   // 16 tiles × 64 cell right column
}
```

### 6. Splat Map Auto-Painting

Region meta is auto-configured for generation:

| Channel | Texture | Purpose | tileScale |
|---------|---------|---------|-----------|
| R | `rocky_terrain_02_1k` | Default land (grass/soil) | 8.0 |
| G | `gravel_floor_1k` | Rock (steep slopes) | 6.0 |
| B | `sandy_gravel_02_1k` | Sandy coastline | 8.0 |
| A | `snow_02_1k` | Snow (high altitude) | 4.0 |

Classification rules (per cell):

| Condition | Weights (R, G, B, A) |
|-----------|---------------------|
| Underwater (h < 0) | (0, 0, 255, 0) — sand |
| Coast (0-6 cells from water) | blend B↔R by distance |
| Steep slope (> 1.5 m/cell) | blend G↔R by steepness |
| High altitude (> 70% maxHeight) | blend A↔R by altitude |
| Default land | (255, 0, 0, 0) — rocky terrain |

Slope is computed from central differences of the height field.
Coast distance is precomputed via BFS flood fill from sea cells.

## UI Design

### Generate Button
- Located in `MapEditorPanel`, below Height/Splat tool tabs
- Same visual style as existing tab buttons (dark bg, gold accent `#e2b93b`)

### Generate Dialog
Modal overlay (matches `RespawnDialog` pattern):
- Dark semi-transparent backdrop
- Rounded panel with controls
- Slider controls for all parameters
- Seed input with "Randomize" button
- Progress bar during generation/save
- "Generate" (primary, gold) and "Cancel" (secondary) buttons

## Data Flow

```
1. User clicks "Generate" button
   → showGenerateDialog store = true

2. Dialog opens, user adjusts settings, clicks "Generate"

3. Load neighbor edge data (if adjacent regions exist)
   → Parallel fetch of boundary tile heights from API

4. generateRegionTerrain(rx, rz, config, neighborEdges)
   → Returns 256 GeneratedTile[] (heightmap + splatmap per tile)

5. Save region meta via metaManager.saveMeta()
   → R=rocky_terrain, G=gravel_floor, B=sandy_gravel, A=snow

6. Apply tiles (batched, 8 concurrent):
   - heightManager.setHeightmap() + markDirty()
   - splatManager.setSplatmap() + markDirty()
   - If geometry loaded: applyHeightToGeometry()

7. Save all tiles via parallel HTTP batches
   → 512 requests (256 height + 256 splat), 8 concurrent

8. Increment regionMetaVersion → triggers texture re-resolution
9. Dialog closes
```

## File Structure

### New Files
- `client/src/lib/utils/simplex-noise.ts` — Self-contained 2D simplex noise + fBm
- `client/src/lib/terrain/terrainGenerator.ts` — Core generation algorithm
- `client/src/lib/components/map-editor/GenerateTerrainDialog.svelte` — UI dialog

### Modified Files
- `client/src/lib/stores/editorStore.ts` — Add `showGenerateDialog` store
- `client/src/lib/components/map-editor/MapEditorPanel.svelte` — Add Generate button
- `client/src/lib/managers/terrainHeightManager.ts` — Add `setHeightmap`, `markDirty`, `saveAllDirty`
- `client/src/lib/managers/terrainSplatManager.ts` — Add `setSplatmap`, `markDirty`, `saveAllDirty`
- `client/src/App.svelte` — Render GenerateTerrainDialog

## Performance

| Operation | Estimated Time |
|-----------|---------------|
| Noise generation (1M cells x 6 octaves) | ~300 ms |
| Quantile sort (1M values) | ~50 ms |
| Coast BFS (1M cells) | ~20 ms |
| River carving | ~5 ms |
| Coastline blur | ~30 ms |
| Boundary blending | ~10 ms |
| Splat map generation | ~30 ms |
| Tile saving (512 reqs, 8 concurrent) | ~2-3 s |
| **Total** | **~3-4 s** |

Progress bar covers the wait time during save phase.

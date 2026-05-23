# 강 시스템 (River System)

내륙 강 렌더링 시스템. 바다 수면([WATER_SYSTEM.md](WATER_SYSTEM.md))과 별도의
파이프라인으로 동작하며, Phase 4 절차적 생성이 만든 강 polyline을 **타일별
RFD1 (River Field Data v1) 바이너리 — surfaceY + flow direction 텍스처** 로
굽고, 런타임은 그 위에 플랫 quad 하나만 깔아서 셰이더로 모든 시각 효과를
유도한다.

> 과거 디자인은 polyline → 리본 메시 + 정점별 attribute 였지만, 메시 봉합·
> miter join·flow direction 보간이 복잡해서 현재의 "타일당 65×65 텍스처 +
> 플랫 quad" 구조로 완전히 교체됨. 옛 ribbon geometry 코드 경로는 모두 제거.

## 1. 개요

- Phase 4 [`rivers.rs`](../shared/src/worldgen/rivers.rs)가 추출한 강 polyline을
  타일 bake 시점에 **per-pixel surfaceY + flowDir** 로 굽는다 (65×65 grid,
  지형 vertex 와 정확히 정렬).
- 런타임은 타일당 플랫 quad 하나(Y는 vertex shader 가 surfaceY 로 변위)와
  공유 강 머티리얼로 렌더. drawcall 은 강 있는 타일에만 1회.
- 가시 범위 / 가장자리 페이드는 모두 셰이더에서 `depth = surfaceY − heightmap`
  로 유도 — 별도 폭(width) 메시 같은 건 없다. 강의 보이는 폭은 곧 hightmap
  carve가 만든 단면.

### 1.1 바다 셰이더를 재사용할 수 없는 이유

- 바다는 Y=0 평면 quad 가 전제. 강은 매 픽셀의 surfaceY 가 다르다 (해발
  2m 해안부터 산지 100m+).
- 바다의 Gerstner wave, shore drawback, wet sand, hole alpha 는 전부 "넓은
  해수면" 전제. 수 m 폭의 강에 그대로 적용하면 어색.
- 바다는 한 방향 wind drift. 강은 픽셀마다 flow direction 이 다르고 그
  방향으로 ripple 노멀맵이 스크롤되어야 한다.

## 2. 데이터 파이프라인

### 2.1 Phase 4 polyline 추출

[`shared/src/worldgen/rivers.rs`](../shared/src/worldgen/rivers.rs)

1. `compute_flow(map) → RiverMap` — Barnes 2014 priority-queue pit-fill +
   D8 flow direction.
2. `extract_rivers(map, river_map, min_peak_elev, min_polyline_length)` —
   peak 셀에서 mouth 까지 trace 해서 `Vec<Polyline>` 채움.
3. 후처리 (`extract_rivers` 내부, 이 순서로 수행):
   - `naturalize_river_meanders` — 비-anchor 정점을 windowed 접선의 수직
     방향으로 2-octave sine noise 변위. 진폭은 flow 에 비례, 합류부에서
     점차 0 으로 taper.
   - `remove_polyline_self_overlaps` — Bresenham 래스터화 후 hairpin 으로
     같은 셀이 `SELF_OVERLAP_MIN_LOOP_VERTICES = 8` 정점 이상 거리를 둔 채
     재방문되는 구간을 절단. 합류부에 평행으로 흐르는 두 갈래가 만들어
     내는 flow field smearing 을 방지.
   - `merge_overlapping_polylines` — 평행하게 흐르는 인접 polyline 을
     main stem 으로 흡수.

각 polyline 정점은 (cell_x, cell_z) `u32` 격자 좌표 + `flow` 누적값.
World-meter 좌표는 `vector_features.rs`의 `river_polyline_to_world` 변환에서
X-wrap 이음새 분리와 함께 처리된다.

**Distributary 분기**는 Phase 4 가 아니라 타일 베이크 진입 시
`BakeContext::new` 가 호출하는 [`tile_bake/context.rs::apply_mouth_distributaries`](../shared/src/worldgen/tile_bake/context.rs)
에서 만들어진다. trunk 의 sea-bound tail (`RIVER_MOUTH_FAN_ARC_CELLS = 8.5`
arc-cell) 을 잘라내고, 4–6 갈래의 좁은 meandering branch 로 교체. 각 branch
는 sub-sea `bed_floor` 값을 운반해 carve 단계에서 채널을 바다에 살짝 잠기게
한다. (이전에는 polyline 자체를 fan factor 로 부풀렸지만, 지금은 분기로
대체했고 옛 wide-fan 경로는 제거됨 — `river_geom.rs` 헤더 참조.)

### 2.2 Polyline → 세그먼트 변환

[`shared/src/worldgen/vector_features.rs`](../shared/src/worldgen/vector_features.rs)

타일 bake 시점에 `BakeContext`가 polyline 을 Chaikin smooth + 정점별
`flow_norm` / `width` / `bed_floor` 보간된 **세그먼트** 로 분해.

```rust
struct RiverSegment {
    ax, az, bx, bz: f32,            // world-space endpoints
    flow_norm_a, flow_norm_b: f32,  // 정규화된 flow [0,1]
    width_a, width_b: f32,          // 폭 (m)
    bed_floor_a, bed_floor_b: f32,  // per-vertex bed floor (m).
                                    // 자연 segment 는 0, distributary
                                    // branch 는 RIVER_MOUTH_BRANCH_BED_Y_M
                                    // (-0.5 m) 까지 sub-sea 로 내려감.
}
```

`width` 매핑: `min_w + (max_w − min_w) · log2(max(flow, 1)) / log2(max_flow)`
(상수는 `tile_bake/constants.rs`: `RIVER_MIN_WIDTH_M = 1.0`,
`RIVER_MAX_WIDTH_M = 10.0`). 로그 압축으로 flow accumulation 의 ~10⁴
dynamic range 를 perceptually 균등한 [0,1] 로 매핑 — 선형이면 99 % 셀이
min-width 빈에 쌓인다.

타일별 베이크는 `river_segments_near_tile(rivers_world, tile_min, tile_max, margin)`
로 bbox + margin 안에 걸친 세그먼트만 끄집어내서 사용 — 한 타일에 보통
수~수십 개.

### 2.3 Heightmap carve

[`tile_bake/heightmap.rs`](../shared/src/worldgen/tile_bake/heightmap.rs)

각 heightmap 정점에서 가장 가까운 강 세그먼트로의 projection 거리를 계산,
flow-aware carve 깊이를 차감.

세그먼트별 carve 파라미터 (`tile_bake/constants.rs`, 모두 `flow_norm ∈ [0,1]`
로 선형 보간):
- 폭 `width = lerp(width_a, width_b, t)` (≈ 1–10 m, 위 §2.2 의 log2 매핑)
- 측면 taper `RIVER_CARVE_TAPER_MIN_M = 3.0` + `RIVER_CARVE_TAPER_EXTRA_M
  = 7.0 · flow_norm` (3–10 m smoothstep 페이드)
- 깊이 `RIVER_CARVE_DEPTH_MIN_M = 1.5` + `RIVER_CARVE_DEPTH_EXTRA_M
  = 2.5 · flow_norm` (1.5–4.0 m)
- bed floor `RIVER_CARVE_MIN_BED_Y_M = 0.0` (해수면) — 자연 segment 의 bed
  하한. Distributary branch 는 segment 의 `bed_floor` (0..−0.5 m) 를
  적용해 바다 잠김 표현을 허용 (sea 셰이더의 `edgeCutoff` 가 이걸 보고
  ocean alpha 를 적절히 컷).
- 굴곡 보상 (bend asymmetry): 인접 segment 와 이루는 각도가
  `RIVER_BEND_TURN_FULL_STRENGTH_RAD = π/4` 이상이면, 바깥쪽 둑을 `+30 %`
  더 깊게, 안쪽 둑을 `−35 %` 얕게 — 실제 강의 cut-bank/point-bar 비대칭
  모사.

폭은 세그먼트 정점 `width` 보간으로, 가장자리는 smoothstep taper.
이 carve 가 만든 단면이 곧 보이는 강의 폭이다. 셰이더는 별도의 width
mesh 를 갖지 않고 `depth = surfaceY − bedY` 로 가시 영역만 그린다.
폭/테이퍼/델타 fan 공식의 단일 소스는
[`tile_bake/river_geom.rs`](../shared/src/worldgen/tile_bake/river_geom.rs)
— heightmap·splatmap·bridges·river_field 가 모두 같은 값을 본다.

### 2.4 RFD1 baking

[`tile_bake/river_field.rs`](../shared/src/worldgen/tile_bake/river_field.rs)

`bake_river_field(map, ctx, heights, tile_origin_x, tile_origin_z, river_segs) → Option<Vec<u8>>`.
세그먼트가 0 이면 `None` 을 반환해 파일을 만들지 않음 (강 없는 타일).

처리:
1. **세그먼트 unit tangent 사전계산** — 타일당 S 개 segment 에 대해
   `(dx, dz)/len` 을 한 번만 계산해 픽셀 루프에서 재사용.
2. **픽셀 65×65** 마다 `weighted_flow_and_nearest(wx, wz, segs, tangents)`
   호출:
   - 모든 세그먼트의 `1/(d² + 1)` 가중치 평균으로 flow direction 계산.
     인접 픽셀들이 Voronoi 경계에서 다른 세그먼트로 할당될 때 1-픽셀
     direction step 이 생기는 것을 막는다 (가까울수록 dominant).
   - cancellation (방향 상쇄) 시 nearest 세그먼트의 tangent 로 폴백 —
     픽셀이 zero flow 로 stall 되지 않게.
3. **surfaceY** — 한 픽셀이 nearest segment 의 centerline projection 점
   `(proj_x, proj_z)` 으로 떨어지면, `bed_at_proj = sample_carved_bed(map,
   ctx, proj_x, proj_z, segs)` 로 carve 된 bed 를 다시 평가
   (`heightmap.rs` 의 carve 와 같은 공식 → 베이크 중인 타일 밖에서도 동일
   값이 나옴). 그 위에 `RIVER_DEPTH_OFFSET_M = 0.5 m` 를 얹어 `surface_full
   = bed_at_proj + 0.5` 가 후보 surface.
4. **carve envelope 밖에선 collapse** — pixel 의 projection 거리
   `dist > half_width + taper` 면 surfaceY 를 그 픽셀의 local bed 로
   collapse. 안 그러면 절벽/델타/하구처럼 지형이 *내려가는* 곳에서
   centerline 의 surfaceY 가 픽셀 전체에 퍼져 강물이 범람으로 렌더된다.
   envelope 안은 `surface_full`, envelope 가장자리 (`half_width <
   dist < half_width + taper`) 는 smoothstep 으로 둘 사이 보간.
5. 결과를 16-byte header + 65×65×4 byte pixel 로 직렬화.

## 3. RFD1 바이너리 포맷

매직 `b"RFD1"` = **R**iver **F**ield **D**ata version **1**. 디코더가 첫
4 바이트로 포맷을 식별하는 표식 (PNG·ZIP 등이 첫 바이트에 매직을 두는
관습과 동일).

```
header (16 bytes):
  bytes  0..4   magic    b"RFD1"
  bytes  4..6   u16      version (현재 1)
  bytes  6..8   u16      grid_x  (== 65)
  bytes  8..10  u16      grid_z  (== 65)
  bytes 10..16  u8[6]    reserved (0)

per-pixel (4 bytes, row-major over 65×65, X then Z):
  bytes  0..2   u16      surfaceY (heightmap 와 동일: (h + 500) / 0.05)
  byte   2      i8       flowX (unit vector × 127, [-127..+127])
  byte   3      i8       flowZ (unit vector × 127)
```

총 `16 + 65*65*4 = 16916` bytes per file.

**타일 경계 일관성**: 한 정점이 두 타일에 동시에 속할 때, 양쪽이 같은
세그먼트 목록(전역 `river_margin` 필터)을 보므로 같은 world-XZ 픽셀에서
surfaceY/flowDir 가 비트 단위로 일치 → 타일 이음새 보이지 않음.

디코더: [`client/src/lib/utils/river-field-data.ts`](../client/src/lib/utils/river-field-data.ts).

## 4. 클라이언트 로딩

| 컴포넌트 | 역할 |
|---|---|
| [`riverFieldManager.ts`](../client/src/lib/managers/riverFieldManager.ts) | 타일별 RFD1 fetch + 디코드 + 캐시. 404 → null. |
| [`river-field-data.ts`](../client/src/lib/utils/river-field-data.ts) | 바이너리 → `{surfaceY, flowX, flowZ}: Float32Array` (각 65×65). |
| [`river-quad-geometry.ts`](../client/src/lib/utils/river-quad-geometry.ts) | 65×65 PlaneGeometry 생성 + vertex Y 를 surfaceY 로 채움. 보조 함수 `buildRiverFieldTexture` 는 RGBA32F DataTexture (R=surfaceY, GB=flowDir) 도 만들어 셰이더에서 bilinear 보간으로 다시 샘플. |
| [`GameSceneRiverLayer.svelte`](../client/src/lib/components/game-scene/GameSceneRiverLayer.svelte) | 활성 terrain 타일 목록과 동기화해 타일별 메시 lifecycle 관리. |

**왜 geometry vertex Y 와 텍스처 둘 다?** Vertex Y 는 65×65 격자에 정확히
정렬돼서 alpha=0 픽셀의 폴리곤이 바닥에 깔린다 (z-fighting 없음).
텍스처 surfaceY 는 픽셀별 bilinear 로 더 부드러운 depth fade.

## 5. 강 머티리얼 (River Field Material)

[`client/src/lib/shaders/river-field-material.ts`](../client/src/lib/shaders/river-field-material.ts)

TSL `NodeMaterial` (WebGPU). 입력: `heightmapTexture`, `riverField`,
`normalMap`, `reflectionMap`, `refractionMap` + 시간 / 태양 / 횃불 uniforms.

### 5.1 Vertex

플랫 quad 의 `positionLocal` 을 그대로 world 로 변환. Y 는 이미 geometry
에 구워져 있으므로 변위 없음.

### 5.2 Fragment

샘플 UV 는 `clamp(toHeightmapUV(uv()), 0, 1)` — half-texel inset 로 텍셀
중심에 정확히 정렬.

1. **Depth fade**
   - `bedHeight = heightmapTex.sample(uv).r`
   - `depth = max(0, surfaceY − bedHeight)` (surfaceY = `vWorldPos.y`,
     즉 geometry 의 baked vertex Y)
   - `depthFactor = clamp(depth / uMaxDepth (=0.5 m), 0, 1)`
   - **Hard edge**: `depthEdgeCut = smoothstep(0, 0.05, depth)` —
     5 cm 안쪽은 hard cut 으로 carve 경계를 정확히 띄움.
   - **Body alpha**: `mix(0.005, 0.95, smoothstep(0.05, uMaxDepth, depth))`
     로 마무리 페이드.
   - **Sea fade**: `seaFade = smoothstep(uSeaFadeBottom, uSeaFadeTop,
     bedHeight)` — bedHeight 가 `SEA_LEVEL − 1.5` 보다 낮으면 알파가 0
     으로 떨어진다 (delta 끝, 바다 잠긴 segment 의 강 quad 를 가린다).
   - 합산: `alpha = 0.95 · depthEdgeCut · bodyAlpha · seaFade`.

2. **색상 그라디언트** — 3-stop 깊이 (sea-style), smoothstep `(0, 0.4)`
   → `(0.4, 0.85)`:
   - `uShallowColor` → `uMidColor` → `uDeepColor`. 야간 감쇠 적용.

3. **Refraction** (얕은 물에서 바닥이 비침)
   - `refractionTex` 를 ripple 노멀로 distort 해 sample.
   - `refrShallow = 1 − smoothstep(0.05, 0.5, depthFactor)` 로 얕은 곳만
     refraction 비중 ↑, 깊은 중앙은 body 색이 dominant.

4. **Ripple normal** — flow 방향 스크롤 + dual-phase flowmap
   - 픽셀별 `flow = riverFieldTex.sample(uv).gb` (bilinear 보간 →
     합류부에서 두 흐름이 자연스럽게 섞임).
   - `flowSpeed = mix(0.3, 1.0, smoothstep(SEA_LEVEL, SEA_LEVEL + 1.5,
     bedHeight))` — bed 가 sea level 가까울수록 flow magnitude 를 줄여
     하구에서 강이 *감속*하게 한다 (시간 위상이 아니라 벡터를 스케일링 →
     인접 픽셀이 phase-coherent 유지).
   - `flow × uTime` 처럼 단순히 스크롤하면 인접 픽셀의 약간 다른 flow
     가 시간이 지나며 텍스처 공간에서 decorrelate → **소용돌이 아티팩트**
     누적. 해결: Valve 식 dual-phase flowmap.

   ```ts
   buildWrappedDrift(rate, flow) {
     phase = uTime × rate
     pA    = fract(phase)
     pB    = fract(phase + 0.5)
     mixW  = abs(pA − 0.5) × 2          // triangle wave
     return { driftA: flow × pA, driftB: flow × pB, mixW }
   }
   ```

   두 phase 로 normalMap 을 각각 샘플해 `mix(sA, sB, mixW)` 로 crossfade —
   각 phase wrap 시점에 반대 phase 가 dominant 라 점프가 안 보인다.
   적용 위치: main ripple (`rate=0.4`) + sky reflection drift
   (`rate=WOBBLE_DRIFT_RATE=0.05`).

5. **Sky reflection** — fresnel + reflection map + cloud photo + sun glare.
   바다 셰이더와 거의 동일. `reflRippleN` 으로 view-aligned 약간의 noise.

6. **Specular** — sun half-vector pow + sparkle layer (uTime × 0.05 로
   스크롤, sparkle 자체는 dual-phase 미적용 — 고주파라 vortex 가
   눈에 띄지 않음).

7. **횃불 라이팅** — torch position 기반 diffuse + specular + 거리 페이드.

8. **야간**: 태양 고도 기반 multiplier + moon ambient/specular.

### 5.3 Uniforms

| 이름 | 타입 | 용도 |
|---|---|---|
| `uTime` | f32 | scroll/sparkle 위상 |
| `uSunDirection`, `uSunColor`, `uMoonBrightness` | vec3/color/f32 | 라이팅 |
| `uCameraDirection` | vec3 | view 방향 (specular) |
| `uTorchPos`, `uTorchColor`, `uTorchIntensity`, `uTorchDistance` | — | 횃불 |
| `uShallowColor`, `uMidColor`, `uDeepColor` | color×3 | depth gradient |
| `uMaxDepth` | f32 | depth fade 상한 (0.5 m, RFD1 의 OFFSET 과 일치) |
| `uSeaFadeBottom`, `uSeaFadeTop` | f32 ×2 | `SEA_LEVEL − 1.5` / `SEA_LEVEL − 0.6` — bed 가 sea level 아래로 잠기는 곳에서 강 알파 감쇠 |
| `uRefractionStrength` | f32 | refraction UV 왜곡량 (0.04) |
| `uReflectionMap`, `uRefractionMap`, `uNormalMap` | tex | 멀티패스 + ripple |
| `uHeightmapTexture` | tex | bedHeight 샘플 |
| `uRiverField` | tex | surfaceY + flowDir (RFD1 → DataTexture) |

## 6. 하구 (Estuary) 처리

별도 mouth-detection 메타데이터 없음 — 네 단계가 합쳐서 자연스러운 델타가
나온다:

1. **Distributary 분기** — `tile_bake/context.rs::apply_mouth_distributaries`
   가 trunk 의 sea-bound tail (`RIVER_MOUTH_FAN_ARC_CELLS = 8.5` arc-cell)
   을 잘라내고 4–6 갈래의 좁은 S-curve branch 로 교체. 각 branch 는 sub-sea
   `bed_floor` (`RIVER_MOUTH_BRANCH_BED_Y_M = −0.5 m`) 와 더 좁은 bank taper
   (`RIVER_MOUTH_BRANCH_TAPER_M = 5 m`) 를 운반.

2. **Bed floor 변화** — 자연 segment 는 `RIVER_CARVE_MIN_BED_Y_M = 0.0 m`
   (해수면) 에서 멈추지만, distributary branch 는 자신의 sub-sea
   `bed_floor` 까지 깎인다 → 채널이 sea level 아래로 살짝 잠겨 바다와
   연결되는 시각.

3. **Sea 셰이더 협조** — bed 가 sea level 아래로 내려간 cell 에서는 splat
   에 인코딩된 `RIVER_FOAM_SUPPRESS_RADIUS_M = 30 m` 의 foam 억제 ramp 가
   적용돼 강 outlet 주변에서 해변 거품이 사라진다.

4. **River 셰이더 알파 fade** — `seaFade = smoothstep(SEA_LEVEL − 1.5,
   SEA_LEVEL − 0.6, bedHeight)` 가 bed 가 sea level 깊이 잠긴 곳에서 강
   quad 알파를 0 으로 떨어뜨려, 바다 위에 떠 있는 강 ribbon 이 보이지
   않게 한다. depthEdgeCut + bodyAlpha 의 자연 페이드도 함께 작동.

색 팔레트는 강·바다 양쪽 모두 그대로 유지 (mouth fade 가 알파 페이드일
뿐 색을 섞지 않음). 진정한 sediment plume / 탁한 mouth 색은 별도 worldgen
단계 필요 — 현재 미구현. 강바닥 splat 자체는 mouth 근처에서 `PAL_RIVER_BED`
→ `PAL_SAND` 로 페이드 (`RIVER_FAN_SAND_BASE_WIDTH_M` 트리거).

## 7. 다리 (Bridges)

도로가 강을 가로지르는 곳마다 베이크 시점에 다리 한 개를 떨어뜨린다.
[`tile_bake/bridges.rs`](../shared/src/worldgen/tile_bake/bridges.rs) 가
모든 로직을 담당하며 출력물은 region 단위 오브젝트 JSON
(`data/terrain/objects/r±NN_±NN.json`) + per-tile heightmap flatten 두 가지.

### 7.1 배치 조건 (`detect_bridges`)

Phase 6 의 `roads::snap_crossings_to_grid` 가 모든 road↔river crossing 을
순수한 90° 축 정렬로 만들어 두기 때문에, 같은 셀에 떨어지는 road cell + river
cell 한 쌍이 곧 다리 후보다. 다음 필터를 차례로 통과해야 다리가 놓인다:

1. **중복 제거** — 같은 셀에 떨어지는 두 도로 (분기·합류) 는 하나로.
2. **델타 버퍼 컷** — sea-bound 강의 mouth 로부터 arc-length
   `RIVER_DELTA_BUFFER_ARC_CELLS = 14 cell` 안쪽이면 거부. 그 영역은 이미
   distributary 로 분기돼 한 다리가 잇기엔 너무 갈래가 많다 ([§2.1](#21-phase-4-polyline-추출)
   의 `apply_mouth_distributaries`). 단, river-mouth 마을의 settlement pad
   가 강을 덮고 있어 시각 공백은 안 남는다.
3. **폭 측정** — 후보 셀의 nearest segment 에서 `baked_width +
   segment_carve_taper_at(s, t)` 로 *visible* 수면 폭을 추정. taper 가 한
   번만 더해지는 이유는 `river_field::compute_pixel` 의 surface→bed
   smoothstep 이 보이는 수면 가장자리를 `half_width + 0.5·taper` 근처에
   두기 때문 (alpha 0 외곽선 `half_width + taper` 이 아님).
4. **하드 캡** — visible width 가 `BRIDGE_MAX_VISIBLE_WIDTH_M = 28 m` 를
   넘으면 어떤 다리 모델도 deck 가 둑까지 닿지 않으므로 placement 거부.
   같은 임계값을 road A* 가 `MASK_WIDE` 로 강에 부여해 start/goal 외에는
   가로지를 수 없게 한다 (`shared/src/worldgen/roads/astar.rs`) — 그래서
   넓은 강은 처음부터 다리가 안 놓일 자리에 도로가 오지도 않는다.

### 7.2 모델 선택 (`BridgeCatalog`)

`catalog.json` 의 `kind: "bridge"` 엔트리 중 두 모델을 베이크가 사용:

| 슬롯 | 모델 ID | 임계값 |
|---|---|---|
| narrow | `stone_bridge` | visible width `< BRIDGE_WIDE_RIBBON_M (12 m)` — flow_norm ≲ 0.5, 헤드워터·분기 stub |
| wide   | `bridge_wood_long` | visible width `≥ 12 m` — flow_norm 0.5+ 의 본류 |

`flatten_foot_length_z` (deck 양 끝 발 부분의 길이) 와
`flatten_foot_half_width_x` 는 카탈로그가 명시한 값 또는 deck Y profile 의
`FOOT_DECK_Y_THRESHOLD_M = 0.5` 임계로 자동 도출. 모델 ID 가 바뀌면
[`LEGACY_BRIDGE_MODEL_IDS`](../shared/src/worldgen/tile_bake/bridges.rs)
에 등록해서 다음 베이크 때 region object JSON 에서 옛 모델의 placement 가
정리되도록 한다.

### 7.3 Y / 회전 결정

- **Y** — 강 tangent 에 수직인 방향으로 `model.deck_min_z` /
  `model.deck_max_z` 위치의 지형 표고를 각각 샘플해 평균. 샘플은
  `settlement_flatten::flatten_height_at` 을 통과시켜 마을 패드 안이면
  평탄화된 패드 표면을, 밖이면 자연 표고 + detail noise 를 본다. **강 carve
  는 일부러 제외** — carve 는 per-tile 베이크에서 패드/평탄화보다 늦게
  적용되므로, deck 끝이 만나는 가시 표면은 carve 가 들어가기 전 높이라
  carve 를 빼면 deck 가 강 바닥으로 박힌다.
- **회전** — road tangent (강 tangent 의 수직) 에서 `θ = atan2(road_dx,
  road_dz)` 로 three.js Y rotation 도출. `canonical_deck_angle` 으로
  `[0, π)` 범위로 접어서 deck 의 양면 대칭에서 오는 ±180° 차이를 한
  값으로 모아 베이크 결정성 보장.

### 7.4 Per-tile heightmap flatten

각 다리는 deck 양 끝의 **foot rect 두 개** (full deck width × foot_length_z)
만 평탄화하고 가운데 아치 구간은 건드리지 않는다. 가운데를 평탄화하면
강 carve 가 사라지면서 `surfaceY = bed + 0.5` 가 deck 아치 아래로 떠올라
foot 까지 삼키기 때문. 평탄화는 `client/src/lib/managers/terrain-height-brushes.ts`
의 `flattenRotatedRect` 와 동일한 distance-to-rotated-rect 공식:

- foot rect 안: `targetY = placement.y + minLocalY + flattenBuryDepth`.
- foot rect 바깥 `BRIDGE_FLATTEN_BLEND_M = 2 m` 까지: smoothstep blend.
- 그 너머: 자연 표고 그대로.

`group_flattens_by_tile` 가 회전된 AABB + blend 만큼 펼친 영역의 모든 타일에
같은 directive 를 복사해 두므로, rayon 으로 타일을 병렬 베이크해도 둑 양쪽
타일이 일관된 flatten 을 본다. heightmap 단계에서는
`apply_bridge_flatten` 이 이 directive 를 적용.

### 7.5 출력

다리 placement 는 region 단위로 묶여 `data/terrain/objects/r±NN_±NN.json`
의 `placements[]` 배열에 기록 (모델 ID, world `(x, y, z)`, three.js
Y-rotation degrees). 런타임은 같은 카탈로그를 읽어 GLB 인스턴스를 배치
하므로 다리 외형은 베이크와 무관하게 모델 파일 한 곳에서만 바뀐다.

## 8. 핵심 파일

### Shared (Rust)
| 파일 | 역할 |
|---|---|
| [`shared/src/worldgen/rivers.rs`](../shared/src/worldgen/rivers.rs) | Phase 4: polyline 추출 + meander/self-overlap/merge 후처리 |
| [`shared/src/worldgen/vector_features.rs`](../shared/src/worldgen/vector_features.rs) | `RiverSegment`, `RiverWorldPolyline`, `river_polyline_to_world`, `river_chaikin_smooth`, `nearest_river_segment`, `project_point_to_segment`, `river_segments_near_tile` |
| [`shared/src/worldgen/tile_bake/context.rs`](../shared/src/worldgen/tile_bake/context.rs) | `BakeContext` 구성, `apply_mouth_distributaries` (델타 분기) |
| [`shared/src/worldgen/tile_bake/river_geom.rs`](../shared/src/worldgen/tile_bake/river_geom.rs) | width/taper/fan-arc 공식 단일 소스 — heightmap·splatmap·bridges·river_field 가 모두 본다 |
| [`shared/src/worldgen/tile_bake/heightmap.rs`](../shared/src/worldgen/tile_bake/heightmap.rs) | 강 carve (flow-aware depth + width + bend asymmetry) |
| [`shared/src/worldgen/tile_bake/river_field.rs`](../shared/src/worldgen/tile_bake/river_field.rs) | RFD1 바이너리 베이크 (weighted flow + surfaceY + envelope collapse) |
| [`shared/src/worldgen/tile_bake/bridges.rs`](../shared/src/worldgen/tile_bake/bridges.rs) | road↔river crossing 다리 배치 (delta buffer 밖에서만 허용) |
| [`shared/src/worldgen/tile_bake/constants.rs`](../shared/src/worldgen/tile_bake/constants.rs) | `RIVER_*` 상수 (폭/깊이/오프셋/min bed/분기 fan/foam 억제) |

### Client (TS / Svelte)
| 파일 | 역할 |
|---|---|
| [`client/src/lib/utils/river-field-data.ts`](../client/src/lib/utils/river-field-data.ts) | RFD1 디코더 |
| [`client/src/lib/managers/riverFieldManager.ts`](../client/src/lib/managers/riverFieldManager.ts) | 타일별 fetch + 캐시 |
| [`client/src/lib/utils/river-quad-geometry.ts`](../client/src/lib/utils/river-quad-geometry.ts) | 65×65 quad + DataTexture 빌드 |
| [`client/src/lib/shaders/river-field-material.ts`](../client/src/lib/shaders/river-field-material.ts) | TSL `NodeMaterial` (depth fade + dual-phase flowmap + 멀티패스) |
| [`client/src/lib/components/game-scene/GameSceneRiverLayer.svelte`](../client/src/lib/components/game-scene/GameSceneRiverLayer.svelte) | 타일 lifecycle / 메시 풀 |

### Tools
| 파일 | 역할 |
|---|---|
| [`tools/terrain-gen/src/main.rs`](../tools/terrain-gen/src/main.rs) | `bake` 시 RFD1 도 함께 출력 |
| [`tools/terrain-gen/src/inspect.rs`](../tools/terrain-gen/src/inspect.rs) | `inspect-tile` (타일에 영향 주는 segment 덤프) + `probe-point` (world `(x, z)` 에서 자연 표고 / nearest segment params / carve 깊이 분해) 서브커맨드 |

## 9. 미해결 / 추후

- **폭포**: 세그먼트 slope 가 큰 구간을 별도 처리 안 함 — 그냥 surfaceY 가
  급격히 떨어지는 모양. 별도 셰이더/효과는 후순위.
- **플레이어 수영**: 서버가 surfaceY 를 알아야 수영 판정이 가능. RFD1 을
  서버측에서도 로드하는 패스 필요. 현재는 시각화 전용.
- **Sediment plume / delta 지형**: 하구에 가까울수록 바다 색이 탁해지는
  거리장 효과 + 부채꼴 carve. worldgen 개편이 함께 필요.

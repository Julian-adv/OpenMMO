# 강 시스템 (River System)

내륙 강 수면 렌더링 시스템의 설계. 바다 수면([WATER_SYSTEM.md](WATER_SYSTEM.md))과 별개로
존재하며, Phase 4 절차적 생성이 만든 강 polyline을 따라 지형 표면에 리본 형태의
수면을 놓는다.

## 1. 개요

- Phase 4 `rivers.rs`가 추출한 강 polyline을 클라이언트에서 시각화한다.
- 바다 수면(Y=0 평면)과 달리 강은 **지형 고도에 따라 Y가 변하는 곡선 리본**이다.
  해발 2m 해안부터 산지 100m+까지 연속적으로 이어진다.
- 타일 단위로 리본 메시를 생성해 공유 강 머티리얼로 렌더한다.
- 바다 쉐이더의 핵심 컴포넌트(굴절, 노멀 리플, 스페큘러, 깊이 기반 색상)를
  공유 헬퍼로 뽑아 재사용하되, 강 고유 요소(흐름 방향 스크롤, 여울 foam)는
  별도 구현한다.

### 1.1 바다 쉐이더를 재사용할 수 없는 이유

- 바다는 64×64m 평면 quad를 Y=0.01에 놓고 깊이를 `worldPos.y - heightmap` 으로
  계산한다. 강은 매 정점마다 지형 Y가 달라 이 전제가 깨진다.
- 바다의 Gerstner 파도, shore drawback, wet sand, hole alpha는 모두 "넓은
  해수면" 전제. 수 m 폭의 강 리본에 적용하면 어색하다.
- foam 밴드가 "depth 1.5→0.15m로 밀려오는 파도"를 가정 — 강에는 조석/쇄파가 없다.

## 2. 데이터 파이프라인

### 2.1 Phase 4 출력 (현재)

[rivers.rs](../shared/src/worldgen/rivers.rs)가 `Vec<Polyline>` 생성.
`Polyline.points: Vec<(u32, u32)>` — 전역 4K 격자 셀 좌표. Peak에서 바다/싱크까지
trace된 경로. Chaikin smoothing은 `tile_bake.rs`에서 bake 시점에 수행 (8m 셀 vertex를
곡선화).

### 2.2 Bake 출력 확장 (신규)

**`rivers/r±xx_±zz/r_±xxxxx_±zzzzz.bin`** — 타일 단위 바이너리 (나무/풀과 동일 패턴).
타일 bbox + margin에 걸친 Chaikin-smoothed polyline 세그먼트만 포함.

`worldgen.json`에는 **넣지 않는다**. 현재 rivers는 시각화 전용이고, 타일 로딩 시점에
필요한 세그먼트만 읽으면 되므로 전역 목록은 중복이다. 나중에 월드맵 오버뷰나 서버
하천변 스폰 같은 전역 용도가 생기면 roads와 같은 패턴으로 추가. (roads가 지금
`worldgen.json`에 있는 이유는 NPC 경로탐색/맵 렌더링이라는 전역 소비자가 있기
때문.)

```
header (16 bytes):
  bytes 0..4   ASCII "RIV1" (magic)
  bytes 4..6   u16  version (currently 1)
  bytes 6..8   u16  segment_count
  bytes 8..12  f32  reserved (0.0)
  bytes 12..16 f32  reserved (0.0)

per-segment (32 bytes, 8 × f32):
  x0, z0             // world-space 시작점
  x1, z1             // world-space 끝점
  width_a, width_b   // 정점 A/B별 폭 (§2.4 flow → width 매핑)
  flow_norm_a,       // 정규화된 유량 (0~1, 색상/스크롤 속도 변조용)
  flow_norm_b
```

정점별로 폭/flow_norm을 기록하는 이유: Chaikin smoothing과 polyline 상 보간을
거쳐 인접 세그먼트 경계에서도 값이 연속적이므로, 렌더러가 두 세그먼트 사이에서
리본 폭과 유속을 매끄럽게 잇는다. 세그먼트 할당은 midpoint 기준 — 한 세그먼트의
중점이 속한 타일이 해당 세그먼트를 소유하므로 중복 저장 없음. 인접 세그먼트가
공유 정점에서 만나므로 타일 경계에서도 리본이 이어진다.

리본 메시 생성은 **클라이언트가 bake 시점에 WASM**으로 또는 **bake 단계에서 세그먼트를 그대로 기록하고 클라이언트에서 메시 빌드**. 후자가 단순하므로 후자를 택한다.

### 2.4 상류→하구 폭 변화

Phase 4 `RiverMap.flow[cell]`이 이미 셀별 flow accumulation(상류 집수 셀 개수)을
가지고 있다. 상류 peak에서는 ≈1, 하구에서는 수천~수만까지 자란다. 이를 그대로 폭에
매핑한다.

상수 제안 (튜닝 대상):
- `RIVER_MIN_WIDTH_M` = 1.5 — 상류 최소 폭.
- `RIVER_MAX_WIDTH_M` = 10.0 — 하구 최대 폭.
- `RIVER_WIDTH_GAMMA` = log 또는 sqrt — flow는 지수적으로 자라므로 선형 매핑은
  대부분의 구간이 거의 `MIN_WIDTH`로 뭉친다. `sqrt` 또는 `log1p`로 압축.

매핑 (bake 시점에 세그먼트 중앙 셀의 flow로):

```rust
let t = (flow.log(2.0) / max_flow.log(2.0)).clamp(0.0, 1.0);
let width_m = RIVER_MIN_WIDTH_M + (RIVER_MAX_WIDTH_M - RIVER_MIN_WIDTH_M) * t;
let flow_norm = t;  // 셰이더용 정규값
```

`max_flow`는 월드 전체에서 가장 큰 river의 하구 flow — 타일마다 달라지면 안
되므로 Phase 4 직후에 전역 최댓값을 한 번 계산해 bake 전체에 공유.

세그먼트 경계에서 폭이 갑자기 바뀌지 않도록, 리본 지오메트리 생성 시 polyline
vertex의 폭은 **인접 두 세그먼트 평균**으로 보간한다 (§4.2의 miter 법선 길이는
이 보간된 폭을 쓴다).

### 2.5 Flow-aware heightmap carve (bake 동시 변경)

현재 [tile_bake.rs:91-106](../shared/src/worldgen/tile_bake.rs#L91-L106)의 carve는 상수:

```rust
const RIVER_CARVE_HALF_WIDTH_M: f32 = 2.5;
const RIVER_CARVE_TAPER_M: f32 = 10.0;
const RIVER_CARVE_DEPTH_M: f32 = 2.0;
```

어차피 river 세그먼트 출력을 위해 re-bake가 필요하므로, carve도 이번에 flow-aware로
바꾼다. 안 하면 상류 1.5m 폭 강이 5m 폭 V자 계곡 한가운데를 흐르는 모양이 된다.

세그먼트별 carve 파라미터를 flow로 스케일:

```rust
fn segment_carve_params(flow_norm: f32, width_m: f32) -> (f32, f32, f32) {
    // half_width: 리본 폭과 일치 → 수면이 채널 평평한 바닥을 완전히 덮음
    let half_width = width_m * 0.5;
    // taper: 큰 강일수록 더 완만한 둑
    let taper = 3.0 + 7.0 * flow_norm;  // 3m ~ 10m
    // depth: 상류는 얕고 하구는 깊게
    let depth = 0.6 + 1.4 * flow_norm;  // 0.6m ~ 2.0m
    (half_width, taper, depth)
}
```

`river_carve_m(d_m, half_width, taper, depth)`를 세그먼트별로 호출. 현재
`min_distance_to_segments`는 거리만 리턴하지만, **거리 + 최근접 세그먼트 인덱스**를
같이 리턴하도록 바꿔야 한다 (`Segment` 에 `half_width/taper/depth` 를 추가).

세그먼트 경계에서 carve 파라미터가 계단처럼 바뀌지 않도록, polyline vertex별로
저장하고 세그먼트 내부는 `t ∈ [0,1]`로 선형 보간. 세그먼트 struct 제안:

```rust
struct Segment {
    a: (f32, f32),       // world xz
    b: (f32, f32),
    half_width_a: f32,   // polyline vertex A에서의 폭/테이퍼/깊이
    taper_a: f32,
    depth_a: f32,
    half_width_b: f32,   // vertex B에서의 값
    taper_b: f32,
    depth_b: f32,
}
```

최근접 세그먼트의 `t`(사영 파라미터)로 `half_width/taper/depth`를 보간해 carve 계산.

바이너리 포맷은 §2.2 참조 — 32 bytes/segment (8 × f32). carve 파라미터 자체는
binary에 쓰지 않고, 클라이언트는 width/flow_norm만 소비 (carve는 bake 내부에서만
`segment_carve_params`로 계산).

### 2.6 `PAL_RIVER_BED` 제거 검토

현재 splatmap은 river 중앙에 `PAL_RIVER_BED`(pebbles) 를 칠한다
([tile_bake.rs:740-741](../shared/src/worldgen/tile_bake.rs#L740-L741)). 강 수면 리본이
추가되면 그 아래 바닥 텍스처는 수면 투명도로 비쳐 보이는데, pebbles 그대로 두는 게
자연스러운지, 흙바닥(PAL_PLAIN)이 더 맞는지는 실측 후 결정. 우선 그대로 유지.

### 2.3 bake.rs 변경

- Phase 4 이후 `RiverMap.rivers`를 Chaikin-smoothed world-space polyline으로 변환
  (tile_bake에서 이미 하고 있음 — [tile_bake.rs:222-228](../shared/src/worldgen/tile_bake.rs#L222-L228) 의 `rivers_world` 재사용).
- 각 polyline에 flow accumulation을 붙여 세그먼트별 폭을 결정. `flow_accumulation` 샘플링
  또는 polyline trace 시 기록.
- `segments_near_tile`을 타일별로 실행 → 세그먼트 배열 → 바이너리 직렬화.
- 0 세그먼트 타일은 파일을 만들지 않는다 (파일 없음 = 강 없음).

## 3. 클라이언트 로딩

### 3.1 리소스 매니저

나무/풀 로더 패턴을 그대로 따름. `client/src/lib/managers/riverDataManager.ts`:

- `loadRivers(tileX, tileZ): Promise<RiverSegment[] | null>` — 파일 존재 안 하면 null.
- 타일 언로드 시 메모리 해제.

### 3.2 레이어 컴포넌트

`client/src/lib/components/game-scene/GameSceneRiverLayer.svelte`:

- 활성 terrain 타일 목록을 받아 타일별 리본 메시를 관리.
- `RiverTile.svelte` 개별 타일 컴포넌트 (WaterTile.svelte와 동일한 수명주기).
- 공유 강 머티리얼 풀 (바다 풀과 분리).

## 4. 리본 지오메트리

### 4.1 기본 구조

세그먼트 하나당 **quad 2개** (양쪽으로 폭만큼 펼쳐서 triangle strip). 모든 세그먼트를
하나의 `BufferGeometry`로 합친다 (타일당 drawcall 1회).

세그먼트 `(A, B, width)`에서:
- 탄젠트 `t = normalize(B - A)`, 법선(수평) `n = (-t.z, 0, t.x)`.
- 정점 4개: `A ± n·(width/2)`, `B ± n·(width/2)`.
- Y는 각 정점에서 heightmap 샘플 (강바닥 기준) + `RIVER_DEPTH_OFFSET_M` (0.6m 정도
  제안 — carve 2m 중 1.4m 정도가 수중이 되도록).

### 4.2 인접 세그먼트 정점 공유

단순히 세그먼트마다 독립 quad를 쌓으면 polyline 꺾임에서 gap/overlap이 발생. 해결:

- polyline을 그대로 strip으로: 각 polyline vertex에서 인접 두 세그먼트의
  평균 탄젠트로 법선을 계산 → 좌/우 정점 1쌍 생성.
- 급한 꺾임에서 법선 길이를 `1 / cos(θ/2)`로 늘려 외곽선 교차 방지 (miter join).
- 꺾임이 너무 급하면 (cos < 0.3 정도) miter 포기하고 bevel로 폴백.

### 4.3 정점 attribute

정점별로 다음 속성을 올린다:

| attribute | type | 용도 |
|-----------|------|------|
| `position` | vec3 | 월드 좌표 (Y는 heightmap sample + offset) |
| `uv` | vec2 | U = 리본 가로(0~1), V = polyline 누적 길이 / scale |
| `flow_dir` | vec2 | 세그먼트 방향 벡터 (XZ) — 스크롤 방향 |
| `flow_speed` | float | flow accumulation 기반 스크롤 속도 배수 |
| `edge_dist` | float | 중앙=0, 양쪽 기슭=1 — foam fringe 마스크 |

### 4.4 타일 경계 처리

- 세그먼트가 타일 경계를 넘어가면 두 타일 각각에 잘린 조각이 들어간다. 경계에서 같은 Y를
  쓰도록 heightmap 샘플 규칙은 기존과 동일 (65×65 중심 정렬).
- 경계에서 이음새(얇은 interpenetration)는 바다 물 타일의 +0.01 offset과 동일하게
  `RIVER_DEPTH_OFFSET_M` 균일 적용으로 회피.

## 5. 강 쉐이더 (River Material)

파일: `client/src/lib/shaders/river-material.ts`.

### 5.1 버텍스 단계

- 버텍스 변위 없음 (Gerstner 없음). 지형 곡면에 딱 붙은 리본 그대로.
- `flow_dir`, `flow_speed`, `edge_dist`, `uv` 를 varying으로 통과.
- worldPos와 clipPos 계산은 표준.

### 5.2 프래그먼트 단계

1. **깊이 근사**: heightmap에서 지형 높이 `H_bed`를 샘플. 수면 Y는 정점 `position.y`
   (= `H_bed + offset`). `depth = position.y - H_bed` 고정에 가까움 — 강은 수심이
   대체로 얕으므로 **`edge_dist` 기반 단조 감소**로 대체한다:
   - 중앙 `edge_dist=0` → `depth ≈ 1.2m`
   - 기슭 `edge_dist=1` → `depth = 0`
   - `depthFactor = smoothstep(0, 1.2, (1 - edge_dist) * maxDepth)`

2. **색상 그라디언트**: 바다보다 짧은 램프. 강은 바닥이 비치는 투명한 맑은 물.
   - 기슭(0): 거의 투명, 굴절 지배
   - 중간(0.4): 연한 올리브-블루 (0.35, 0.55, 0.45)
   - 중앙 깊은(1.0): 흐린 블루-그린 (0.1, 0.25, 0.3)

3. **표면 노멀 — 흐름 방향 스크롤**:
   - 바다의 `sampleNormalNoise`와 같은 노멀맵을 **`flow_dir` 방향으로만 스크롤**.
   - 레이어 2개: 큰 스케일(0.05) 느린 속도, 작은 스케일(0.15) 빠른 속도.
   - `u`(가로) 축으로는 스크롤하지 않음 → 강이 흘러가는 느낌.
   - `flow_speed` attribute로 속도 배수.

4. **굴절**: 바다와 공유 헬퍼 `buildRefraction` 사용. 다만 depthFactor 램프가
   짧으므로 `mixFactor`의 smoothstep 범위를 조정 (0.02~0.25).

5. **스페큘러**: 바다와 공유 헬퍼 `buildSpecular` 사용. Gerstner 없이 노멀 리플만
   있으므로 sparkle 강도는 약간 낮춘다.

6. **하늘 반사**: 바다의 `buildSkyReflection` 공유. Fresnel 계수는 동일.

7. **여울 foam (rapids)**:
   - 폴리라인 세그먼트 `slope = (H_downstream - H_upstream) / segment_length` 를
     bake 시 계산해 `flow_speed`에 반영 (또는 별도 attribute `rapid`).
   - `rapid > threshold`인 세그먼트에서 foam 텍스처(바다와 공유 `foamMap`)를
     흐름 방향 스크롤로 가산.
   - 추가로 `edge_dist` 근처에 얇은 edge foam (0.85~1.0 구간).

8. **기슭 페이드 (edge fade)**:
   - 폭의 바깥 10%(edge_dist 0.9~1.0)에서 alpha를 선형 페이드 → 스플랫맵 sand 밴드와
     자연스럽게 합성.
   - hole alpha 같은 노이즈는 **쓰지 않음** — 강에는 조석이 없으므로 단조로운 경계가 맞음.

9. **야간 감쇠**: 바다와 동일 (태양 고도 기반 승수).

10. **알파**: 기본 0.85 (바다보다 투명하게, 강바닥이 보이도록). edge fade로 기슭에서 0.

### 5.3 유니폼

바다와 공유 가능한 것은 머티리얼 생성 시 인자로 받는다: `normalMap`, `foamMap`,
`refractionMap`, `reflectionMap`, `sunDirection`, `sunColor`, `cameraDirection`,
`moonBrightness`, `time`.

강 고유:
- `uFlowSpeedScale: float` — 전역 스크롤 속도.
- `uMaxDepth: float` — 1.2m.
- `uShallowColor`, `uMidColor`, `uDeepColor`.
- `uHeightmapTexture` — 타일 heightmap (깊이 확인용; depth 근사를 edge_dist로 하면 생략 가능).

## 6. 공유 헬퍼 리팩토링

바다 쉐이더의 일부 함수를 별도 모듈로 뽑는다.

파일: `client/src/lib/shaders/water-shared.ts`.

이전:
- [water-material.ts](../client/src/lib/shaders/water-material.ts) 안에 `buildRefraction`, `buildSurfaceNormal`, `buildSpecular`, `buildSkyReflection`이 클로저로 존재.

이후:
- `water-shared.ts`가 이 함수들을 export. 파라미터로 필요한 노드/유니폼을 받는다.
- `water-material.ts`와 `river-material.ts`가 import해서 호출.

리팩토링 시 동작 변화 없음 (바다 출력 바이트 동치) 검증 필요 — 눈으로 비교 + 스크린샷.

## 7. 구현 순서

1. **데이터 파이프라인 (단일 re-bake)**
   - [ ] Phase 4에서 전역 `max_flow` 계산, polyline vertex별 `flow_norm` + `width_m` 저장.
   - [ ] `Segment` struct 확장 (§2.5) — vertex별 `half_width/taper/depth`.
   - [ ] `min_distance_to_segments`가 `(distance, seg_idx, t)` 리턴하도록 변경.
   - [ ] `river_carve_m`을 세그먼트별 파라미터로 호출 (보간 포함).
   - [ ] 타일별 `rivers/*.bin` 출력 (32-byte/segment).
   - [ ] `terrain-gen bake --seed 42` 재실행, heightmap/splat 결과 시각 검증.
   - [ ] 바이너리 포맷 검증: hex dump + Python으로 파싱.
2. **클라이언트 로딩**
   - [ ] `riverDataManager.ts` — 타일별 세그먼트 로드.
3. **리본 지오메트리**
   - [ ] `river-geometry.ts` — 세그먼트 배열 → `BufferGeometry`. miter join 포함.
   - [ ] 디버그: `MeshBasicMaterial` 단색으로 시각 확인.
4. **공유 헬퍼 리팩토링**
   - [ ] `water-shared.ts`로 헬퍼 이동. 바다 렌더 regression 확인.
5. **강 머티리얼**
   - [ ] `river-material.ts` — 단계 5.2 순서대로 구현. 매 단계마다 스크린샷.
6. **레이어 통합**
   - [ ] `GameSceneRiverLayer.svelte` + `RiverTile.svelte`.
   - [ ] `GameScene.svelte`에서 water layer 다음에 mount.
7. **튜닝**
   - [ ] 깊이, 색상, 스크롤 속도, foam 임계값을 seed 42 월드에서 대표 장소 5군데 기준으로 조정.

각 단계 끝에 `terrain-gen bake` 불필요 (데이터 포맷은 1에서 확정). 1 이후에는 쉐이더만
반복.

## 8. 미결정 / 추후 결정

- **폭포 표현**: 세그먼트 slope가 매우 큰 경우(예: 1.0 이상). 현재는 그냥 리본이
  급하게 Y가 떨어지는 것으로 표현. 별도 폭포 쉐이더/지오메트리는 후속 과제.
- **깊이 계산 방식**: §5.2-1은 `edge_dist` 기반 근사. 엄밀히는 heightmap에서 실제
  carve 단면을 샘플링해야 하지만 bake가 carve를 이미 했으므로 `position.y - H_bed`가
  그대로 나온다. 성능 대비 정확도 측정 후 택일.
- **강 메시 컬링**: 타일 단위로 이미 나뉘어 frustum culling은 자동. 원거리 LOD는
  아직 불필요 (월드 전체 river 세그먼트 수가 수천 수준으로 추정).
- **플레이어 수영 판정**: 수면 Y를 서버가 알아야 플레이어 수영 상태를 판정할 수
  있다. 서버 측 river 샘플링은 별도 작업. 우선 시각화만.
- **강↔바다 경계**: 강 하구에서 수면 Y가 0(바다)로 수렴. 리본 끝 세그먼트를
  바다 quad와 살짝 오버랩시켜 seam을 숨길지, 명시적으로 zero-length 종단 처리할지
  구현 시 결정.

## 9. 핵심 파일 (예상)

| 파일 | 역할 |
|------|------|
| `shared/src/worldgen/rivers.rs` | (기존) Phase 4 polyline 추출 |
| `tools/terrain-gen/src/bake.rs` | (변경) `worldgen.json` rivers + `rivers/*.bin` 출력 |
| `client/src/lib/managers/riverDataManager.ts` | (신규) 타일별 세그먼트 로드 |
| `client/src/lib/utils/river-geometry.ts` | (신규) 세그먼트 → BufferGeometry |
| `client/src/lib/shaders/water-shared.ts` | (신규) 바다/강 공유 헬퍼 |
| `client/src/lib/shaders/water-material.ts` | (변경) 공유 헬퍼 사용 |
| `client/src/lib/shaders/river-material.ts` | (신규) 강 TSL 쉐이더 |
| `client/src/lib/components/RiverTile.svelte` | (신규) 강 타일 메시 + 유니폼 |
| `client/src/lib/components/game-scene/GameSceneRiverLayer.svelte` | (신규) 타일 관리 레이어 |

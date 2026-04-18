# 절차적 지형 생성 시스템 (Procedural Terrain Generation)

이 문서는 OnlineRPG의 32km × 32km 오픈월드를 절차적으로 생성하기 위한
시스템의 설계를 정의한다. 기존 [MAP_DESIGN.md](MAP_DESIGN.md)의 런타임
지형 표현(타일, 하이트맵, 스플랫맵 포맷)은 그대로 유지하고, 이 문서는
**그 아래 파일들을 어떻게 만드는가**를 다룬다.

## 1. 목표

- 32,768 m × 32,768 m 월드 전체를 단일 시드로부터 결정론적으로 생성.
- 시드를 바꿔 가며 오프라인에서 반복 생성 → 만족스러운 결과를 선택
  → 최종 에셋(하이트맵, 스플랫맵, 초목/나무 데이터)을 디스크에
  베이크하는 **반복형 워크플로우**.
- 대륙/바다, 산지/평야, 하천, 정착지, 도로 등 **전역 피처**가 존재하는
  맵. 단순 fBm noise만으로는 "어디든 비슷한" 지형이 되기 쉬움.
- 기존 런타임 지형 로더(`terrain` crate의 `TerrainIO`, `HeightSampler`,
  클라이언트 `terrainHeightManager`)와 **포맷 호환**.

## 2. 핵심 설계: 2단 해상도

전체 맵을 풀 해상도(32,768² ≈ 10억 셀)로 한꺼번에 생성하면 메모리와
시간 비용이 막대하다 (hydraulic erosion 같은 단계는 GPU compute로도
분 단위). 대신 두 단계로 나눈다:

### 2.1 저해상도 전역 맵 (4,096 × 4,096)

- 셀당 8m × 8m. 32 MB uint16 heightmap + 보조 레이어.
- **전역 구조**를 여기서 결정한다: 대륙 모양, 산지 분포, 하천 네트워크,
  정착지 위치, 도로. 즉 "여러 타일에 걸친" 피처는 전부 이 단계에서.
- 이 해상도에서는 hydraulic erosion, flow accumulation, A* pathfinding
  같은 무거운 연산이 모두 저렴하게 돌아간다.

### 2.2 고해상도 타일 베이킹 (1m/px)

- 타일별 65 × 65 하이트맵 vertex + 64 × 64 스플랫맵 cell
  (기존 [MAP_DESIGN.md](MAP_DESIGN.md) 포맷 그대로).
- 전역 맵을 bilinear 샘플링해서 base height을 얻고, 여기에 **국소 detail
  noise**(octave 추가)를 합성.
- 전역 맵이 가리키는 하천 polyline이 타일을 통과하면 거기서 채널을
  carve. 도로도 마찬가지.
- 스플랫맵은 지형 경사, biome, 하천/해안 거리, 도로 유무 등을 기반으로
  각 셀의 primary/secondary 텍스처와 blend를 결정.

## 3. 파이프라인 단계

전역 맵은 아래 순서로 필드가 쌓인다. 각 단계는 이전 단계의 출력을 읽고
자기 필드를 채운다. 단계 결과는 모두 `GlobalMap` 구조에 누적.

| # | 단계 | 출력 필드 | 기법 |
|---|------|-----------|------|
| 1 | 대륙/바다 마스크 | `continent_potential`, `land_mask`, `sea_level_potential` | fBm + 반경 edge falloff + quantile threshold |
| 2 | 고도 레이어링 | `elevation` (f32 meters) | Phase 1 potential + 산지 마스크(secondary fBm) modulated amplitude |
| 3 | Hydraulic erosion | `elevation` 갱신, `sediment_map` | Particle-based droplet 시뮬레이션 (수만~수십만 개 drop) |
| 4 | 하천 추출 | `flow_accumulation`, `rivers: Vec<Polyline>` | Flow field 계산 → 임계값 이상 셀을 trace해 polyline으로 |
| 5 | 정착지 배치 | `settlements: Vec<Settlement>` | Poisson-disk + 지형 적합도 스코어 (해안/강변/평야 가산점) |
| 6 | 도로 망 | `roads: Vec<Polyline>` | 정착지 쌍 간 A*, 비용 = 경사 + 강/늪 페널티 |
| 7 | (런타임) 타일 베이크 | 타일별 hmap/splat 파일 | 전역 맵 샘플 + detail noise + 강/도로 carve + biome → splat |
| 8 | 초목/나무 배치 | 타일별 vegetation, tree data | biome + slope + 수원 거리 기반 density |

**주의:** 4번 하천은 3번 erosion의 flow accumulation 결과물에서 파생된다.
별도 "강 파기" 단계가 아니라 erosion이 자연스럽게 만든 흐름을 이름 붙이는
것. 유저 원안의 1→2→3→4는 이렇게 3+4가 하나의 erosion pass로 통합된다.

## 4. 공유 Rust 코드 배치

생성 로직은 전부 `shared/src/worldgen/` 에 둔다. 이유:

- 서버는 MMO 런타임에서 타일을 lazy-baking 하거나 지형 검증을 해야
  할 수 있음.
- 클라이언트 에디터에서 "현재 시드의 전역 맵 미리보기" 같은 기능을
  붙이려면 WASM으로 같은 코드를 쓰는 것이 편함 (기존 XP/pathfinding
  WASM 바인딩 패턴과 동일).
- 오프라인 도구는 이 crate를 단순히 의존하는 얇은 래퍼가 된다.

현재 구조:

```
shared/src/worldgen/
  mod.rs
  config.rs          # WorldGenConfig
  noise.rs           # 결정론적 Perlin + fBm (외부 crate 없음)
  global_map.rs      # GlobalMap 누적 구조
  grid.rs            # BFS/min-heap helpers (crate-internal)
  growth.rs          # Eden 성장 기반 대륙 시드 (Phase 1 sub-pass)
  continent.rs       # Phase 1
  elevation.rs       # Phase 2
  erosion.rs         # Phase 3
  rivers.rs          # Phase 4
  settlements.rs     # Phase 5
  roads.rs           # Phase 6
  tile_bake.rs       # Phase 7 (고해상도 타일 샘플링 + V2 splatmap)
  (vegetation.rs)    # Phase 8 예정
```

## 5. 오프라인 도구: `tools/terrain-gen`

Rust 바이너리 crate (워크스페이스 새 멤버). `shared::worldgen`에
의존하며, 자체적으로 이미지 출력과 최종 에셋 베이킹을 담당.

### 5.1 명령 구조

```
terrain-gen preview  --seed <N> [--config <toml>]
terrain-gen bake     --seed <N> [--config <toml>] --out <dir>
```

- `preview`: 전역 맵만 생성하고 PNG 여러 장 출력 (elevation, biome,
  rivers overlay, settlements + roads). 수초 내 완료. 반복 튜닝용.
- `bake`: 전역 맵 + 모든 262,144개 타일 파일을 `data/terrain/` 포맷에
  맞춰 디스크에 쓴다. 수 분~ 수십 분. 타일 수가 많으므로 rayon 병렬화.

### 5.2 config 파일

`WorldGenConfig`를 TOML로 수정 가능하게 한다. 시드 튜닝 시 코드 리빌드
없이 파라미터 조정. 기본값은 코드 상수.

### 5.3 출력물

**Preview 모드** (`preview_out/<seed>/`):
- `elevation.png` — grayscale, 4096² (해수면 0 기준 명도 매핑)
- `biome.png` — RGB (바다/해안/평야/숲/산 색상)
- `rivers.png` — elevation에 하천 polyline overlay
- `settlements.png` — biome에 정착지 점 + 도로 overlay
- `meta.json` — 사용된 config, 관측 통계 (실제 sea_ratio, 마을 수 등)

**Bake 모드** (`data/terrain/` 또는 지정 경로):
- `height/r±xx_±zz/h_±xxxxx_±zzzzz.bin` — uint16 65×65
- `splat/r±xx_±zz/s_±xxxxx_±zzzzz.bin` — 64×64×4 bytes (V2)
- `meta/r±xx_±zz.json` — 팔레트 (리전의 biome 구성에 따라 자동 결정)
- `worldgen.json` — 시드, config, 정착지/도로 목록 (게임 서버가
  로드할 수 있도록)

## 6. 반복 워크플로우

1. `terrain-gen preview --seed 12345` 실행.
2. `preview_out/12345/*.png` 열어서 확인.
3. 마음에 안 들면 시드 바꾸거나 config 조정 → goto 1.
4. 마음에 들면 `terrain-gen bake --seed 12345 --out data/terrain`.
5. 게임 실행. 기존 `TerrainIO`가 파일을 그대로 로드.

preview는 수 초 안에 끝나야 반복 튜닝이 실용적임. 그래서 Phase 1-6은
저해상도 전역 맵에서만 동작하도록 최적화한다.

## 7. 기존 시스템과의 통합

- **포맷**: 기존 `terrain` crate의 uint16 인코딩 (`height = value*0.05
  - 500`), 65×65 vertex layout, V2 splatmap 포맷을 그대로 따른다.
  변경 없음.
- **서버**: 기존 `HeightSampler`, `TerrainIO`가 그대로 파일을 읽는다.
  바뀌는 것 없음.
- **클라이언트 `GenerateTerrainDialog.svelte`**: 장기적으로 WASM
  바인딩을 통해 같은 코드를 미리보기에 쓸 수 있지만, Phase 7까지는
  기존 TS 생성기를 건드리지 않는다. bake 완료 후 기존 TS 생성기는
  제거/deprecate 판단.
- **No-spawn zones, 정착지**: `worldgen.json`에 저장된 정착지 위치
  데이터는 기존 `NoSpawnZone` 메커니즘과 연계 가능 (자동으로 마을
  중심 반경을 no-spawn으로 등록).

## 8. 월드 경계 처리

- **동서(X축) 연결**: 월드는 X축으로 **원통 위상**이다. 오른쪽 가장자리를
  벗어나면 왼쪽 가장자리로 들어온다. 이를 위해 Phase 1의 대륙 noise는
  **X-periodic**으로 샘플링한다 (3D Perlin에서 X축을 원에 매핑:
  `(R·cosθ, y, R·sinθ)`, `θ = 2π·x/W`). 좌우 경계에서 대륙이 자연스럽게
  이어진다.
- **남북(Y축) 벽**: 위아래는 wrap이 geometry상 어려우므로 **통과 불가능한
  높은 산**으로 막는다. Phase 1에서는 이 경계에 대한 특별 처리 없이
  land가 자연스럽게 형성되게 두고, Phase 2에서 `y < margin` 또는
  `y > res - margin` 영역의 land 고도를 강하게 boost해 산맥 벽으로 만든다.
- **가장자리 바다 깊이**: 플레이어는 바다 깊이를 체감하지 않으므로 인위적
  bias 없음 (과거 시도에서 margin cliff 문제 발생 → 제거).

## 9. 결정론과 시드 파생

하나의 마스터 시드 `u64`에서 출발하여 각 단계가 독립 시드를 쓴다.
단계 시드 = `master_seed ^ PHASE_TAG` (`PHASE_TAG`는 단계별 상수).
이렇게 하면 한 단계의 알고리즘을 바꿔도 다른 단계가 재현 불가능해지지
않는다.

## 10. 미결정 / 추후 결정

- **Erosion 기법**: particle-based droplet(간단, 튜닝 직관적) vs
  grid-based stream power(수학적). Phase 3 구현 때 결정.
- **하천 수원**: 고산 피크에서 시작 vs flow accumulation 임계값으로
  자동. 후자가 더 자연스러움.
- **도로 곡선화**: A* 출력은 각진 경로. Chaikin smoothing 또는 Catmull-Rom
  으로 부드럽게 할지.
- **지하 동굴/지하 구조**: 현 시스템 범위 밖. 하이트맵 위에 별도
  오브젝트로 배치 (기존 정책 그대로).
- **바이옴 구분**: 온도/습도 노이즈로 더 세분(타이가/사막/열대 등)할지.
  현재는 고도/경사/수원 거리만 쓴다.

## 11. 현재 진행 상황

- [x] Phase 1: 대륙/바다 마스크 + 프레임워크 (`shared/src/worldgen/`).
      seed 기반 Eden 성장(`growth.rs`)으로 연속 대륙 생성, 작은 섬 필터,
      isthmus cut, 연속 대륙 간 최소 간격 유지까지 포함.
- [x] Phase 2: 고도 레이어링 (`elevation.rs`). 해안 거리 기반 base
      gradient + mountain/plain selector noise로 진폭 조절, 남북 경계
      산맥 벽.
- [x] Phase 3: Hydraulic erosion (`erosion.rs`). Particle-based droplet
      시뮬레이션, 브러시 반경으로 gully 스무딩.
- [x] Phase 4: Flow accumulation + 하천 추출 (`rivers.rs`).
      Barnes 2014 priority-queue pit-fill → D8 flow → peak에서 mouth까지
      trace.
- [x] Phase 5: 정착지 배치 (`settlements.rs`). 해안/강변/평야 fitness
      스코어 + greedy min-spacing으로 habitable 셀에서 top-N 배치.
      habitability는 elevation + slope 하드 컷오프.
- [x] Phase 6: 도로 망 (`roads.rs`). Prim MST + K-NN 추가 엣지 (평행 제거)
      + 경사 페널티 A*. Phase 5b "도로변 마을" 후처리와 결합.
- [x] Phase 7: 타일 베이크 (`tile_bake.rs`). 전역 맵 bilinear 샘플 +
      high-freq detail noise + 해저 shallow bathymetry. V2 splatmap 은
      road > river > sea > alpine > cliff > coast > plain 우선순위로
      primary/secondary slot + blend 를 결정하고, 평야에는 vegMeta 에
      short-grass 밀도 bake. 고정 6-슬롯 팔레트 (`rocky_terrain`,
      `sandy_gravel`, `red_laterite`, `snow_02`, `gravel_road`,
      `gravel_floor`).
- [x] `tools/terrain-gen` 스캐폴딩 및 `preview` / `bake` 명령.
      `preview` 출력: `01_potential.png`, `01_land_sea.png`,
      `01_land_sea_shifted.png` (wrap 검증용), `02_elevation.png`,
      `02_elevation_hypso.png`, `03_rivers.png`, `04_settlements.png`,
      `05_roads.png`, `meta.json` (정착지 목록 포함).
      `bake` 출력: `height/r{rx}_{rz}/h_{tx}_{tz}.bin`,
      `splat/r{rx}_{rz}/s_{tx}_{tz}.bin`, `meta/r{rx}_{rz}.json`,
      `worldgen.json` (seed/config/settlements/roads). rayon 병렬.
- [ ] Phase 8: 초목/나무 배치.

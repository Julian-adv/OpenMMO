# Vegetation System

terrain의 splat map R 채널을 기반으로 풀(grass), 나무(tree), 꽃(flower)을 절차적으로 배치하는 시스템.

## Splat Map R 채널 인코딩

R 채널 값이 vegetation 타입과 밀도를 동시에 인코딩한다.

| R 값 | 타입 | 밀도 |
|-------|------|------|
| 0~229 | vegetation 없음 (바위/모래/눈 등) | - |
| 230~239 | Short grass | R값이 높을수록 밀도 높음 |
| 240~249 | Tall grass | R값이 높을수록 밀도 높음 |
| 250~255 | 미사용 | - |

상수 정의: `client/src/lib/shaders/grass-material.ts`

## Grass 배치

핵심 파일: `client/src/lib/utils/grass-data.ts`

### 배치 로직 (`computeInstances`)

각 terrain 타일(64×64 셀)을 순회하며:

1. **R값 필터** — 해당 타입 범위(short: 230~239, tall: 240~249)인 셀만 처리
2. **밀도 계산** — `density = (rVal - rMin) / (rMax - rMin)` (0~1)
3. **셀 내 그리드 분포** — 균일 그리드에서 각 위치마다 `rand() < density` 체크로 솎아냄
   - Short grass: 12×12 = 최대 144개/셀
   - Tall grass: 10×10 = 최대 100개/셀
4. **높이 필터** — `worldY < 0.05`이면 제외 (수면 아래)

### 스케일

| 타입 | 최소 | 최대 |
|------|------|------|
| Short grass | 0.4 | 0.7 |
| Tall grass | 0.5 | 1.5 |

### 경계 블렌딩

인접 셀이 다른 풀 타입인 경우, 30%(`BOUNDARY_BLEND_RATIO`) 확률로 상대 타입의 스케일을 적용한다. 변환된 인스턴스는 상대 타입 배열에 합쳐진다:

```
shortInstances = short.own + tall.converted
tallInstances  = tall.own  + short.converted
```

이를 통해 short/tall grass 영역 간 자연스러운 전환이 이루어진다.

### 전역 밀도 조절

`GRASS_DENSITY_SCALE` (0~1) 상수로 로드 시점에 인스턴스를 확률적으로 솎아낸다(thinning). 꽃에는 적용되지 않는다.

## Flower 배치

핵심 함수: `computeFlowerInstances` (grass-data.ts)

- **Short grass 셀에서만** 생성
- 셀당 최대 1개
- **풀이 성긴 곳에 꽃이 더 많이** 핀다:

| R값 | 풀 밀도 | 꽃 확률 |
|-----|---------|---------|
| 230 (최저) | 0% | ~40% |
| 239 (최고) | 100% | ~5% |

계산식: `flowerProb = 0.4 * Math.pow(0.125, t)` (t = 정규화된 밀도 0~1)

스케일 범위: 0.42 ~ 0.60

## Tree 배치

핵심 파일: `client/src/lib/utils/tree-data.ts`

### 배치 로직 (`computeTreePlacement`)

64×64 셀 순회:

1. **R값 필터** — R값이 `SHORT_GRASS_R_MIN`(230) ~ `TALL_GRASS_R_MAX`(249) 범위인 셀 (= 풀이 있는 곳)
2. **확률 체크** — `TREE_PROBABILITY = 0.004` (0.4%)
3. **경사 필터** — slope > 1.5이면 제외
4. **높이 필터** — `worldY < 0.5`이면 제외 (수면 아래)
5. **셀 내 오프셋** — 0.1~0.9 범위 랜덤으로 자연스러운 위치
6. **모델 배정** — 50:50 확률로 `tree1` 또는 `tree2`

스케일 범위: 0.6 ~ 1.4, 회전: 0 ~ 2π 랜덤

### 밀도 조절

| 레이어 | 역할 |
|--------|------|
| Splat map R 채널 | **어디에** 나무가 생기는지 결정 (풀 영역에만) |
| `TREE_PROBABILITY` | **얼마나** 나무가 생기는지 결정 (전역 확률) |

## 렌더링

### Grass 렌더링

파일: `client/src/lib/components/game-scene/GameSceneGrassLayer.svelte`

- Sub-chunk(32×32) 단위로 분할하여 관리
- 3종류 InstancedMesh: short grass, tall grass, flower
- 플레이어 주변 3×3 sub-chunk만 활성화
- Compute shader로 매 프레임 바람/플레이어 인터랙션 계산

### Tree 렌더링

파일: `client/src/lib/components/game-scene/GameSceneTreeLayer.svelte`

- `tree.glb`, `tree2.glb` 두 모델을 lazy 로드
- 인스턴스마다 `scene.clone()`으로 복제 → `treeGroup`에 추가
- 타일이 뷰에서 벗어나면 geometry dispose 후 정리

## 바이너리 포맷

### Grass (v3 — "GR03")

```
[u32 magic=0x47523033] [u32 shortCount] [u32 tallCount] [u32 flowerCount]
[N × { u16 localX, u16 localZ, u8 rotation, u8 scale }]
```

16바이트 헤더 + 인스턴스당 6바이트

### Tree (v1 — "TR01")

```
[u32 magic=0x54523031] [u32 tree1Count] [u32 tree2Count]
[N × { u16 localX, u16 localZ, u8 rotation, u8 scale }]
```

12바이트 헤더 + 인스턴스당 6바이트

### 인메모리 레이아웃

디코딩 후 작업용 포맷 (공통):

```
[u32 counts...] [N × { f32 x, f32 y, f32 z, f32 rotation, f32 scale }]
```

인스턴스당 20바이트. Y좌표는 디코딩 시 heightmap에서 샘플링하여 복원.

## 데이터 흐름

```
splat map 생성 (terrain-splat-gen.ts)
    ↓
computeGrassPlacement() / computeTreePlacement()
    ↓
바이너리 인코딩 (GR03 / TR01)
    ↓
서버 저장 (API: /api/terrain/grass/{x}/{z}, /api/terrain/trees/{x}/{z})
    ↓
클라이언트 로드 → 디코딩 → 렌더링
```

생성은 WorldMapDialog의 resplat 워크플로우에서 트리거된다.

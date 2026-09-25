# Vegetation System

terrain의 splatmap vegMeta(바이트 3)를 기반으로 풀(grass), 나무(tree), 꽃(flower)을 절차적으로 배치하는 시스템.

## Splatmap vegMeta 인코딩

vegMeta 값이 vegetation 타입과 밀도를 동시에 인코딩한다.

| R 값 | 타입 | 밀도 |
|-------|------|------|
| 0~229 | vegetation 없음 (바위/모래/눈 등) | - |
| 230~239 | Short grass | R값이 높을수록 밀도 높음 |
| 240~249 | Tall grass | R값이 높을수록 밀도 높음 |
| 250~255 | 미사용 | - |

상수 정의: `client/src/lib/shaders/grass-material.ts`

## Grass 배치

서버는 1m × 1m 셀마다 짧은 풀·긴 풀·꽃의 개수를 저장한다. 각각 0~255 범위의 1바이트이며 개별 위치·회전·크기는 저장하지 않는다.

베이크(`shared/src/worldgen/vegetation.rs`)는 vegMeta 밀도에 따라 짧은 풀 최대 64개/셀, 긴 풀 최대 36개/셀을 계산한다. 셀 중심이 수면 아래면 비운다. 클라이언트 에디터의 재생성도 같은 최대 밀도를 사용한다.

클라이언트의 `decodeGrassData`는 개수만큼 셀 안에 지터 그리드로 배치하고 높이맵을 샘플링한다. 타일·셀·종류별 시드로 위치·회전·크기를 만들므로 재로드와 인접 셀 변경에도 배치가 안정적이며 월드 X 경계에서도 같은 패턴을 사용한다. 수면 아래의 개체는 표시하지 않는다.

조경으로 지운 셀은 세 개수를 모두 0으로 만든다. 건물 제거 영역과 일부라도 겹치는 셀도 비워서 재생성 후 건물 안에 풀이 생기지 않게 한다. 건물 철거용 `grass-original`도 같은 밀도 포맷을 사용한다.

### 스케일

| 타입 | 최소 | 최대 |
|------|------|------|
| Short grass | 0.4 | 0.7 |
| Tall grass | 0.5 | 1.5 |

### 경계 블렌딩

인접 셀이 다른 풀 타입이면 약 30%를 상대 종류의 개수로 옮긴다.

### 전역 밀도 조절

그래픽 프리셋의 `grassDensity`로 표시할 풀을 솎아낸다. 저장된 개수는 바뀌지 않으며 꽃에는 적용하지 않는다.

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

텍스처: `/textures/flowerx4.png` — 2×2 아틀라스 (4종 꽃). instance hash 기반 랜덤 quadrant 선택으로 variety 확보

Material: `FLOWER_CONFIG` — baseColor 진녹, windStrength 0.04 (뻣뻣한 줄기), atlasGrid 2

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

### Grass (v4 — GR04)

```
[u32 little-endian magic=0x47523034]
[64 × 64 × { u8 shortCount, u8 tallCount, u8 flowerCount }]
```

셀 순서는 `z * 64 + x`. 헤더 4바이트 + 12,288바이트 = 타일당 **12,292바이트**다. 기존 V3 목록은 셀별로 집계하고 종류별 개수를 255로 제한한다.

### 기존 파일 변환

서버를 중지한 상태에서 실행한다. 재실행할 수 있으며 이미 V4인 파일은 건너뛴다.

```bash
cargo run --release -p onlinerpg-terrain --bin terrain-grass-migrate -- data/terrain
```

`grass/`와 `grass-original/` 파일을 각각 임시 파일 + rename으로 교체한다. 서버·클라이언트는 전환 중 남아 있는 V3 파일도 셀별 개수로 읽으며 새로 저장하는 파일은 V4다. 변환 후 `terrain-manifests`로 원본 파일 해시 목록을 준비하고 서버를 재시작해 메모리의 타일 버전을 갱신한다. 해시 목록 준비는 원본 파일을 수정하거나 전송용 본문을 복제하지 않는다.

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
셀별 개수 인코딩 (GR04) / 나무 배치 인코딩 (TR01)
    ↓
서버 저장 (API: /api/terrain/grass/{x}/{z}, /api/terrain/trees/{x}/{z})
    ↓
클라이언트 로드 → 디코딩 → 렌더링
```

생성은 WorldMapDialog의 resplat 워크플로우에서 트리거된다.

## Wind Particles

파일: `client/src/lib/components/game-scene/GameSceneWindParticles.svelte`

- 민들레 홀씨 (dandelion seed) + 잔디 잎 (grass leaf) 2종
- InstancedMesh × 2, MeshBasicNodeMaterial (unlit), CPU 파티클 시뮬레이션
- 텍스처: Canvas 프로시저럴 생성 (`wind-particle-material.ts`). 민들레: 64×64 흰색 pappus, 잔디: 32×64 녹색 blade
- 바람 동기화: `GrassLayer.getWindState()` → WindState를 매 프레임 읽어 파티클에 적용
- 트리거: `windStrength > 0.45` 일 때 spawn, 바람 세기에 비례하는 spawn rate
- 수량: 타입당 최대 25개 (총 50개), 플레이어 주변 20m 반경에서 생성
- 수명: 민들레 5~9초, 잔디 2~5초. Fade in/out (10%/30%)
- Billboard: 카메라 quaternion 복사로 CPU-side billboard
- draw calls: 2회 (타입당 1 InstancedMesh)

## 향후 계획

### Wheat Field (미구현)
- Cross-billboard geometry (PlaneGeometry 2장 X자 교차) → 어느 각도에서든 볼륨감
- Alpha cutout 텍스처 (밀 이삭 실루엣) + alphaTest
- 황금색~갈색 color palette
- 바람 phase를 군집 단위로 coherent하게

### 유채꽃 / 갈대 (Rapeseed / Reeds)
- 유채꽃: tall grass 변형. 높이 크고 상단에 노란 색상. cross-billboard로 볼륨감
- 갈대: 수변/습지 영역. 상단 밝은 베이지, 가늘고 긴 실루엣
- 배치: splat map 기반 또는 biome/height 조건 (갈대 → 수변 height 0~0.3 근처)

### 풀 색상 Variation
- grass shader의 baseColor/tipColor를 biome 또는 height 기반으로 보간
  - 초원: 연두색 / 숲 근처: 진녹색 / 해변: 황록색

### 추가 아이디어

| 아이디어 | 설명 | 난이도 |
|---------|------|--------|
| 클로버 패치 (ground cover) | 지면에 깔리는 flat billboard, splat만으로 표현 불가한 디테일 추가 | 낮음 |
| 나비 / 잠자리 | 꽃 근처 소수 spawn, Lissajous curve 비행 패턴 | 중간 |
| 이슬 / 반짝임 | 아침 시간대 grass tip에 specular highlight 강화 | 낮음 |

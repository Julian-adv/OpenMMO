# Vegetation System

## Splatmap 정보 밀도 문제

4채널 blend weight 방식은 대부분의 셀에서 1개 채널만 dominant하고 나머지 3채널은 0.
자유도는 실질적으로 3 (합=255 제약). 추가 데이터를 넣을 여유가 없음.

대안 검토:
- weight 3채널 + data 1채널 → terrain shader 전면 수정 필요, ROI 낮음
- 별도 vegetationMap 레이어 추가 → 가능하지만 현 시점에서 과도
- **R채널 범위 세분화** → 변경 최소, 가장 현실적 ✓

## Vegetation Subtype via R Channel Ranges

R채널 230~255 범위를 subtype으로 세분화하여 vegetation variety 확보.
terrain blend shader는 230 이상을 전부 "grass"로 취급하므로 변경 없음.

| R값 범위    | Vegetation Type | 설명 |
|------------|-----------------|------|
| 0~229      | (terrain blend weight) | grass blend weight, 풀 인스턴스 없음 |
| 230~239    | Short grass     | 기본 풀 (낮고 가는 blade) |
| 240~249    | Tall grass      | 높이 2x, 폭 넓고, 진한 색, 더 큰 wind sway |
| 250~255    | (미할당)          | 향후 Wheat 등 추가 vegetation용 예비 |

## 구현 상태

### Short Grass (구현 완료)
- Geometry: `createGrassBladeGeometry(0.03, 0.4, 0.4, 0.5)` — 5-vertex tapered blade
- Material: `createGrassMaterial()` — 기본 파라미터
- Density: 3×3 = 9 blades/cell (R값 기반 확률적 솎아내기)
- Scale: 0.7 ~ 1.3

### Tall Grass (구현 완료)
- Geometry: `createGrassBladeGeometry(0.05, 0.8, 0.35, 0.4)` — 더 넓고 2x 높은 blade
- Material: `createGrassMaterial(TALL_GRASS_CONFIG)` — 진한 녹색, windStrength 0.12
- Density: 6×6 = 36 blades/cell (적지만 큰 블레이드)
- Scale: 0.8 ~ 1.3
- R채널 범위: 240~249

### Wildflowers (구현 완료)
- Geometry: grass blade geometry 공유
- Material: `createGrassMaterial(FLOWER_CONFIG)` — windStrength 0.04, `flowerColors` palette
- Texture: `/textures/flowerx4.png` 2×2 atlas (4 flower varieties), per-instance random quadrant via hash
- 배치: short grass 셀 (R=230~239)에서 확률적 생성, grass density 반비례 (sparse → 80%, dense → 10%)
- Density: 셀당 최대 1개 (grass blade에 추가, 대체 아님)
- Scale: 0.42 ~ 0.60
- 색상: 아틀라스 텍스처에서 직접 읽음 (4종 꽃 디자인)

### Architecture
- `computeGrassPlacement()`: 타일별 short + tall + flower 인스턴스 생성, binary format v2로 직렬화
- 타일당 3개의 InstancedMesh (short + tall + flower), sub-chunk 키 기반 슬롯 관리
- Trail uniform은 3개 material에 동일하게 업데이트

### Wheat Field (미구현)
- Cross-billboard geometry (PlaneGeometry 2장 X자 교차) → 어느 각도에서든 볼륨감
- Alpha cutout 텍스처 (밀 이삭 실루엣) + alphaTest
- 황금색~갈색 color palette
- 바람 phase를 군집 단위로 coherent하게

---

## Vegetation Beautification Plan

씬 비주얼 향상을 위한 vegetation 확장 계획. 기존 billboard instanced grass 인프라 활용.

### Phase 1: 야생화 (Wildflowers) — 구현 완료

**목표**: 초원에 저밀도 꽃 추가로 색상 다양성 확보

- **R채널 범위**: 별도 범위 없음. Short grass 셀 (R=230~239) 내에서 확률적으로 생성
- **배치 로직**: grass density와 반비례하는 확률. R=230 (성긴 풀) → ~80%, R=239 (빽빽한 풀) → ~10%. 셀당 최대 1개, grass blade와 독립적으로 추가 (대체하지 않음)
- **구현 방식**: 기존 grass pipeline 활용, 별도 material + 텍스처. 타일당 별도 InstancedMesh (capacity 2048)
- **텍스처**: `/textures/flowerx4.png` — 2×2 아틀라스 (4종 꽃). instance hash 기반 랜덤 quadrant 선택으로 variety 확보
- **Material**: `FLOWER_CONFIG` — baseColor 진녹, windStrength 0.04 (뻣뻣한 줄기), scale 0.42~0.60, atlasGrid 2
- **바람 반응**: grass와 동일한 Gerstner wave uniform 공유, windStrength 낮게 설정
- **데이터**: grass binary format v2 — 12-byte header에 flowerCount 포함, short/tall 뒤에 flower 인스턴스 연속 배치

### Phase 2: 유채꽃 / 갈대 (Rapeseed / Reeds)

**목표**: 높이 variation 추가, 특정 지역에 군집 형성

- **유채꽃**: tall grass 변형. 높이 크고 상단에 노란 색상 (tipColor 황색). cross-billboard로 볼륨감
- **갈대**: 수변/습지 영역. 상단 밝은 베이지 (씨앗 부분), 가늘고 긴 실루엣
- **배치**: splat map 기반 또는 biome/height 조건 (갈대 → 수변 height 0~0.3 근처)
- **난이도**: 중간 — tall grass config 변형 + 전용 텍스처 필요

### Phase 3: 바람 파티클 (Wind-Blown Particles) — 구현 완료

**목표**: 바람의 존재감을 시각적으로 극대화

- **종류**: 민들레 홀씨 (dandelion seed) + 잔디 잎 (grass leaf) 2종
- **구현**: `GameSceneWindParticles.svelte` — InstancedMesh × 2 (타입당 1개), MeshBasicNodeMaterial (unlit), CPU 파티클 시뮬레이션
- **텍스처**: Canvas 프로시저럴 생성 (`wind-particle-material.ts`). 민들레: 64×64 흰색 pappus, 잔디: 32×64 녹색 blade
- **바람 동기화**: `GrassLayer.getWindState()` → WindState (windDir, windStrength, time)를 매 프레임 읽어 파티클에 적용
- **트리거**: `windStrength > 0.45` 일 때 spawn, 바람 세기에 비례하는 spawn rate
- **수량**: 타입당 최대 25개 (총 50개), 플레이어 주변 20m 반경에서 생성
- **물리**: 민들레 — 위로 뜨는 drift + bobbing, 잔디 — 중력 낙하 + tumble flutter. 공통 drag + wind acceleration
- **수명**: 민들레 5~9초, 잔디 2~5초. Fade in/out (10%/30%)
- **Billboard**: 카메라 quaternion 복사로 CPU-side billboard, 매 프레임 instance matrix 갱신
- **WebGPU**: `parent.remove + add` 패턴으로 buffer re-upload 강제. 첫 렌더 시 count=max 유지
- **렌더 패스**: refraction/reflection 시 그룹 숨김 (grass와 동일 패턴)
- **draw calls**: 2회 (타입당 1 InstancedMesh)

### Phase 4: 풀 색상 Variation (Grass Color by Biome/Height)

**목표**: 같은 밀도의 풀이라도 위치에 따라 색감 변화

- **구현**: grass shader의 baseColor/tipColor를 biome 또는 height 기반으로 보간
  - 초원: 연두색
  - 숲 근처: 진녹색
  - 해변: 황록색
- **난이도**: 낮음 — shader uniform 또는 per-instance attribute 추가만으로 가능, 에셋 불필요

### 추가 아이디어 (미확정)

| 아이디어 | 설명 | 난이도 |
|---------|------|--------|
| 클로버 패치 (ground cover) | 지면에 깔리는 flat billboard, splat만으로 표현 불가한 디테일 추가 | 낮음 |
| 나비 / 잠자리 | 꽃 근처 소수 spawn, Lissajous curve 비행 패턴. 적은 수로 생동감 | 중간 |
| 이슬 / 반짝임 | 아침 시간대 grass tip에 specular highlight 강화 (roughness 조절) | 낮음 |

### 우선순위

임팩트 대비 구현 난이도 기준:

1. **야생화 (Phase 1)** — 기존 파이프라인 거의 그대로, 즉시 시각적 효과
2. **유채꽃/갈대 (Phase 2)** — tall grass 변형이라 빠르게 가능
3. **바람 파티클 (Phase 3)** — 씬 전체 분위기 업그레이드
4. **풀 색상 variation (Phase 4)** — shader만 수정, 에셋 불필요

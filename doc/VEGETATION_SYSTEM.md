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
| 250~255    | Wheat / 곡물     | (미구현) Cross-billboard + alpha texture |

## 구현 상태

### Short Grass (구현 완료)
- Geometry: `createGrassBladeGeometry(0.03, 0.4, 0.4, 0.5)` — 5-vertex tapered blade
- Material: `createGrassMaterial()` — 기본 파라미터
- Density: 10×10 = 100 blades/cell
- Scale: 0.7 ~ 1.3

### Tall Grass (구현 완료)
- Geometry: `createGrassBladeGeometry(0.05, 0.8, 0.35, 0.4)` — 더 넓고 2x 높은 blade
- Material: `createGrassMaterial(TALL_GRASS_CONFIG)` — 진한 녹색, windStrength 0.12
- Density: 6×6 = 36 blades/cell (적지만 큰 블레이드)
- Scale: 0.8 ~ 1.3
- 생성 확률: scatter circle의 30%가 tall grass (`TALL_GRASS_PROB = 0.3`)

### Architecture
- `generateVegetationForTile()`: generic 생성 함수, `VegetationConfig`으로 파라미터화
- 타일당 2개의 InstancedMesh (short + tall), 별도 SvelteMap으로 관리
- Trail uniform은 양쪽 material에 동일하게 업데이트

### Wheat Field (미구현)
- Cross-billboard geometry (PlaneGeometry 2장 X자 교차) → 어느 각도에서든 볼륨감
- Alpha cutout 텍스처 (밀 이삭 실루엣) + alphaTest
- 황금색~갈색 color palette
- 바람 phase를 군집 단위로 coherent하게

---

## Vegetation Beautification Plan

씬 비주얼 향상을 위한 vegetation 확장 계획. 기존 billboard instanced grass 인프라 활용.

### Phase 1: 야생화 (Wildflowers)

**목표**: 초원에 저밀도 꽃 추가로 색상 다양성 확보

- **R채널 범위**: 250~254 할당 (현재 미사용)
- **구현 방식**: 기존 grass pipeline 그대로 활용, 별도 material + 텍스처
- **텍스처**: 꽃 billboard alpha map 1장. 노란색(민들레), 분홍(코스모스) 등 색상 variation은 instance hash 기반 hue shift로 처리 → 텍스처 1장으로 다양성 확보
- **밀도**: grass 대비 낮은 밀도 (BLADES_PER_AXIS = 1~2)
- **바람 반응**: grass와 동일한 Gerstner wave uniform 공유, windStrength 낮게 설정 (꽃은 줄기가 뻣뻣)
- **난이도**: 낮음 — 기존 파이프라인 거의 그대로, 텍스처 + material config만 추가

### Phase 2: 유채꽃 / 갈대 (Rapeseed / Reeds)

**목표**: 높이 variation 추가, 특정 지역에 군집 형성

- **유채꽃**: tall grass 변형. 높이 크고 상단에 노란 색상 (tipColor 황색). cross-billboard로 볼륨감
- **갈대**: 수변/습지 영역. 상단 밝은 베이지 (씨앗 부분), 가늘고 긴 실루엣
- **배치**: splat map 기반 또는 biome/height 조건 (갈대 → 수변 height 0~0.3 근처)
- **난이도**: 중간 — tall grass config 변형 + 전용 텍스처 필요

### Phase 3: 바람 파티클 (Wind-Blown Particles)

**목표**: 바람의 존재감을 시각적으로 극대화

- **종류**: 꽃잎, 풀씨, 민들레 홀씨
- **구현**: 기존 Gerstner wave wind uniform 공유하는 경량 파티클 시스템
- **트리거**: windStrength가 임계값 이상일 때 꽃잎 spawn → 바람 세기 체감
- **수량**: 카메라 주변 수십 개 수준 (저비용)
- **난이도**: 중간 — 별도 파티클 시스템이지만 wind uniform 재활용으로 동기화 용이

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

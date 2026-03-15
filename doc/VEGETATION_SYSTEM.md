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

# Environment Assets

## Grass

- grass: https://github.com/thebenezer/FluffyGrass — **[미사용]** 자체 grass 시스템(`grass-material.ts` 등)으로 대체됨

## Tree

- tree.glb & tree2.glb
  - https://sketchfab.com/3d-models/low-poly-tree-scene-free-89daa5e21f0d4f08a59dba0d566e88bd

### tree normal-map optimization (2026-10-25)

`tree.glb`도 `tree2.glb`와 동일한 512×512 나무껍질 노멀맵을 사용하므로,
같은 도구로 16비트 RGB PNG를 8비트 RGB PNG로 변환했다.
변환된 노멀맵은 `tree2.glb`의 최적화된 노멀맵과 바이트까지 같다.
메시·재질·다른 텍스처를 담은 나머지 bufferView 18개는 바이트까지 유지했다.

- GLB: 2,344,276 → 1,584,904 bytes.
- gzip level 6: 2,241,036 → 1,481,662 bytes (33.9% 감소).
- 원본: `assets/vegetation/tree-normal-8bit-v1/original.glb`.
- 재생성: `node tools/optimize-tree-normal.mjs assets/vegetation/tree-normal-8bit-v1/original.glb client/public/models/vegetation/tree.glb`.
- 검증: `assets/vegetation/tree-normal-8bit-v1/validation.json`.

동일한 카메라·조명의 Chrome/Three.js 렌더에서 평균 RGB 채널 차이는
전체 약 0.0027/255, 줄기 확대 약 0.0114/255였다. 최대 차이는 각각
2/255, 13/255였으며 브라우저 예외는 없었다.

### tree2 normal-map optimization (2026-10-25)

`tree2.glb`의 나무껍질 노멀맵 `bark_willow_nor_gl_2k`를 512×512
16비트 RGB PNG에서 같은 해상도의 8비트 RGB PNG로 변환했다.
메시·재질·다른 텍스처를 담은 나머지 bufferView 18개는 바이트까지 유지했다.

- GLB: 2,495,728 → 1,736,352 bytes.
- gzip level 6: 2,318,166 → 1,558,732 bytes (32.8% 감소).
- 원본: `assets/vegetation/tree2-normal-8bit-v1/original.glb`.
- 재생성: `node tools/optimize-tree-normal.mjs assets/vegetation/tree2-normal-8bit-v1/original.glb client/public/models/vegetation/tree2.glb`.
- 검증: `assets/vegetation/tree2-normal-8bit-v1/validation.json`.

Chrome/Three.js에서 동일한 카메라·조명으로 원본과 변환본을 렌더링했다.
전체 모습의 RGB 채널 최대 차이는 1/255, 줄기 확대는 3/255였다.
확대 이미지의 평균 채널 차이는 약 0.0103/255이며 브라우저 예외는 없었다.
[전체·줄기 확대 비교](../images/tree2-normal-8bit-before-after.png).

## Terrain Textures

`client/public/textures/**/*.glb`는 배포본이다: 원본(Poly Haven 1k GLB)은 `assets/textures-src/`(HF 동기화)에 두고
`python3 tools/repack-material-glbs.py`로 재생성한다 — 샘플 메시·specular 제거, 모든 맵 WebP q90(노멀맵 포함; 렌더 A/B에서 무손실 대비 0.3/255 이내),
palette 전용 레이어는 아틀라스 크기인 512로 축소. 270 MB → 31 MB (2026-09-05).
2026-11-08부터 하우징·던전 텍스처도 모두 512로 줄인다(게임 카메라에서 작게 보이므로). GLB 합계 31 MB → 8.6 MB.
white-cloud.jpg도 같은 폴더의 원본(5964px)을 2048px q85로 줄인 것.

- 자갈: https://polyhaven.com/a/gravel_floor — **[미사용]** 도로는 gravel_road 사용 (palette.json), 참조 없음
- 잔디: https://polyhaven.com/a/rocky_terrain_02
- 눈: https://polyhaven.com/a/snow_02
- 흙: https://polyhaven.com/a/red_laterite_soil_stones
- 모래: https://polyhaven.com/a/sandy_gravel_02
- 도로: https://polyhaven.com/a/gravel_road
- 절벽: https://polyhaven.com/a/rocky_trail
- 강바닥: https://polyhaven.com/a/ganges_river_pebbles
- 해안 모래: https://sketchfab.com/3d-models/fine-sand-material-6e54464d405a4c1e8bdb0f81e8d74db2 — **[미사용]** 참조 없음
- https://polyhaven.com/a/cobblestone_color — Radiance / Bow 조준 표식 프리뷰의 바닥에 사용
- https://polyhaven.com/a/grey_stone_path
- https://polyhaven.com/a/stone_pathway
- https://polyhaven.com/a/pavement_02 — **[미사용]** 참조 없음 (patterned_paving_02 사용)
- https://polyhaven.com/a/patterned_paving_02
- https://polyhaven.com/a/japanese_stone_wall — **[미사용]** 참조 없음
- https://polyhaven.com/a/rock_embedded_concrete — **[미사용]** 참조 없음
- marble_cliff_01 (Poly Haven) — **[미사용]** 초기 절벽 텍스처, rocky_trail로 교체됨
- cliff_side (Poly Haven) — **[미사용]** 참조 없음
- gray_rocks (Poly Haven) — **[미사용]** 참조 없음

## Cloud

- <a href="https://kr.freepik.com/free-photo/white-cloud_3816314.htm#fromView=keyword&page=1&position=8&uuid=159badd4-6cbe-4767-aec8-8a40d2789230&query=Sky+cloud">작가 lifeforstock 출처 Freepik</a>

## Rain Puddle Reflections

- `client/public/textures/puddle-light-streak.png` — **[미사용]** 물웅덩이 반사광용 흑백 마스크 (1254×1254 PNG). 지그재그 무늬가 부자연스러워 셰이더에서 제거하고 텍스처·프롬프트 파일도 삭제함.
  - 출처: OpenAI 내장 `image_gen`으로 생성, 2026-09-14 (KST).
  - 생성 도구/티어: Codex 내장 이미지 생성; 계정 요금제와 서비스 티어는 도구에서 미노출.
  - 라이선스: OpenAI 생성 출력물에 적용되는 서비스 약관. 별도 제3자 stock/CC 에셋을 사용하지 않음.
  - 원본: `exec-dc34119b-bd15-42fc-a47f-08d30de51617.png`.

## Sea

- sea https://www.filterforge.com/filters/4141.html — **[미사용]** 물 표면은 절차적 렌더 + waternormals(three.js examples); 4141 사용 흔적 없음
- sea foam https://www.filterforge.com/filters/13843.html — 수면 및 나룻배의 항적·노 물보라에 사용 (`textures/13843.png`, water-foam-gen.ts, effects/boat-water.ts)
- waternormals (Three.js examples) https://github.com/mrdoob/three.js/tree/dev/examples/textures — 물 표면 노멀맵으로 사용 중 (`textures/waternormals.jpg`, scene-init.ts)

## Union Hall / 가문 혼약 예식장 (2026-11-11)

- 원화: [union-hall-concept-v2.png](../images/props/union-hall-concept-v2.png), 1536×1024 PNG. 가문 합병 의식용 건물이며 사용자 확정 2026-11-11. OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 생성 2026-11-11. 외부 참조 이미지 없이 생성했으며 OpenAI 생성 출력물 이용 조건 적용. 밝은 석회암 벽, 슬레이트 박공지붕, 버트레스 5구간, 4단 석조 종탑. 장미창과 첨탑 끝의 맞물린 두 고리는 두 가문의 결합을 뜻한다. 목표 크기는 폭 12m·깊이 22m·처마 9m·용마루 15m·종탑 26m. [프롬프트·해시](../../assets/props/union-hall/concept-source-v2.json).
- 분리 구성: 텐트와 같은 `perimeter`·`cutaway` 방식. 실내에서 남는 후면부는 바닥·먼 쪽 긴 벽 안쪽 면·제단부 끝벽, 숨는 전면부는 지붕·정면 박공·가까운 쪽 긴 벽이다. 좌석·제단·깃발은 별도 오브젝트로 둔다.
- 후면부 원화: [union-hall-back-concept-v1.png](../images/props/union-hall-back-concept-v1.png), 1536×1024 PNG. v2 원화를 참조로 붙여 같은 내장 ImageGen·**ChatGPT Pro 20x**로 생성, 같은 이용 조건. 전면부는 v2 외관을 디자인으로 쓴다. [프롬프트·해시·파트 구성](../../assets/props/union-hall/split-concepts-v1.json).
- 직접 모델링(2026-11-11, 사용자 결정; v2 2026-11-12): Tripo 대신 Blender 5.2 Python 형상에 타일링 재질을 입혔다. 생성형 3D 도구는 쓰지 않았다. 본당 12×17m·처마 9m·용마루 15m, 제단부 8×5m, 종탑 4.5m 각·첨탑 끝 26m, 정문 폭 3.8m·높이 5.7m. 삼각형 14,146개(`hall_back` 5,040·`hall_front` 6,456·`hall_tower` 2,650), GLB 1,253,848바이트. v2에서 창틀·창대, 3겹 정문 아치와 기둥, 처마 받침, 박공 덮개돌, 제단부 옆창을 더하고 깃발 걸이와 첨탑을 가늘게 했다. 재질은 기존 하우징의 Poly Haven CC0 텍스처(marble_01·medieval_blocks_03·grey_roof_tiles_02·dark_wooden_planks)와 직접 만든 납틀 유리 텍스처다. marble_01·grey_roof_tiles_02는 ORM의 금속 채널이 0.5라 고정 거칠기·금속 0으로 바꿨다. 렌더: [외관](../images/props/union-hall-model/exterior.png)·[기본 확대 실내](../images/props/union-hall-model/game-zoom1-indoors.png)·[기본 확대 정문](../images/props/union-hall-model/game-zoom1-portal.png). [소스·해시·남은 문제](../../assets/props/union-hall/model/sources.json).
- 배치 제약: 입구는 서향(회전 270) 전용이며 남향으로 놓으려면 좌우를 뒤집은 모델이 필요하다. `perimeter`가 평평한 직사각형 하나라 본당 12×17m만 걸을 수 있고 제단부는 배경이다. 노드는 `hall_back`·`hall_tower`(실내 유지)와 `hall_front`(숨김)이다.
- 설치 상태(2026-11-11): `client/public/models/objects/union_hall.glb`(로컬, HF·`assets.lock` 미등록), `object_catalog.json`의 `union_hall`(foundation 23×30m, perimeter 12×17×9m·입구 3.8m, cutaway `hall_front`). 개발 월드 가구점 옆에 배치해 재질과 실내 숨김을 확인했다. 벽 충돌은 서버 재시작 뒤 정상이고 프레임 저하는 없었다. 종탑·제단부·버트레스에는 충돌이 없다.

## Ysolde's Excavation Tent / 이졸데의 발굴 야영 텐트 (2026-10-24)

- 원화 초안: [ysolde-tent-concept.png](../images/props/ysolde-tent-concept.png), 1536×1024 PNG. 이졸데의 현재 위치 근처에 놓을 소형 거주 텐트의 초기 디자인 검토용이다.
- OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 생성 2026-10-24. 외부 참조 이미지 없이 생성했으며 OpenAI 생성 출력물 이용 조건 적용.
- 모래색·올리브색 캔버스, 목재 지지대, 삼줄과 말뚝으로 만든 박공지붕 텐트. 내부에 1인 침대와 통로, 입구에 유물 상자·붓·모종삽·말린 도면과 랜턴을 두어 발굴 야영지 분위기를 표현했다.
- 제작 시 목표 크기는 본체 약 2.4×3.2m, 용마루 높이 2.2m. 원화의 시각적 비례를 바탕으로 3D 단계에서 침대 출입 공간을 검증한다.
- 전체 생성 프롬프트·해시: [concept-source.json](../../assets/props/ysolde-tent/concept-source.json).
- 분리 디자인 참고 v2(2026-10-24): [후면부](../images/props/ysolde-tent-back-concept-v2.png)·[전면부](../images/props/ysolde-tent-front-concept-v2.png), 각각 1536×1024 PNG. 같은 내장 ImageGen·**ChatGPT Pro 20x**로 기존 원화와 사용자가 표시한 측벽 가이드를 참조해 수정했으며 같은 OpenAI 출력물 이용 조건 적용. [전체 프롬프트·해시·파트 구성](../../assets/props/ysolde-tent/split-concepts-v2.json).
- 사용자가 실내 느낌을 위해 표시한 왼쪽 경사벽 전체를 후면부에 추가했다. 후면부는 삼각형 뒤벽·왼쪽 경사벽·용마루와 뒤 기둥·빈 바닥천이며 실내에서도 남긴다. 전면부는 오른쪽 경사천·앞 기둥·묶어 연 입구 천이며 실내에서 숨긴다. 왼쪽 경사벽은 전면부에서 제거해 파트가 중복되지 않게 했다.
- **[미사용]** 분리 원화 v1: [후면부](../images/props/ysolde-tent-back-concept-v1.png)·[전면부](../images/props/ysolde-tent-front-concept-v1.png). 실내에서 뒤벽만 남던 구성이며 v2로 대체했다. [이전 프롬프트·해시](../../assets/props/ysolde-tent/split-concepts-v1.json).
- 잠자리와 모든 소품은 텐트 메시에서 제외하고 독립 오브젝트로 다룬다. 처음에는 여관 2층 부활 침대(id 52–55)와 같은 `rustic_bed`로 공간을 검토했으며, 이후 그 침구를 재사용한 바닥 매트리스와 낮은 목제 야전침대를 미리보기 후보로 추가했다. [잠자리 제작 기록](props.md).
- 직교 제작 원화 v3(2026-10-24): Blender에서 공통 폭 2.4m·깊이 3.2m·용마루 2.2m·기둥 끝 2.3m 기준형을 만든 뒤, 내장 ImageGen(**ChatGPT Pro 20x**)으로 재질을 입힌 별도 PNG 6장이다. 생성 이미지의 색상 외곽 크기를 기준 렌더와 비교했을 때 폭 차이는 최대 약 1.5%, 높이는 약 0.5%였다. 이 수치는 투영 외곽 비교이며 3D 형상의 일치를 보장하지 않는다. [전체 프롬프트·해시·투영 검사](../../assets/props/ysolde-tent/orthographic-v3/sources.json), [기준형 생성 소스](../../assets/props/ysolde-tent/render_orthographic_guides.py).

| 파트 | 정면 | 측면 | 후면 |
| --- | --- | --- | --- |
| 후면부 | [정면](../images/props/ysolde-tent-ortho-v3/back/front.png) | [왼쪽](../images/props/ysolde-tent-ortho-v3/back/side.png) | [후면](../images/props/ysolde-tent-ortho-v3/back/back.png) |
| 전면부 | [정면](../images/props/ysolde-tent-ortho-v3/front/front.png) | [오른쪽](../images/props/ysolde-tent-ortho-v3/front/side.png) | [후면](../images/props/ysolde-tent-ortho-v3/front/back.png) |

- Tripo API 유료 크레딧으로 파트별 3시점 생성, 모델 `P2-20260801`, detailed PBR `v3.5-20260815`, 목표 4,000 faces. 후면 task `86d03297-5057-411d-838a-9659210d3341`, 전면 task `5ede9e77-1cb2-494c-946e-adfda9697004`. 각각 120크레딧, 합계 **240크레딧**(760 → 520). Tripo 이용 약관 적용. 요청·입력 해시·응답·원본은 `assets/props/ysolde-tent/tripo_ortho_v3/{back,front}/`에 보존했다.
- 원본은 반대쪽 경사천과 불필요한 끝면을 추정해 추가했다. 그대로 배치하지 않고, [로컬 조립 소스](../../assets/props/ysolde-tent/fit_tripo_parts.py)로 불필요한 면을 잘라 파트를 분리하고 공통 크기·원점을 맞췄다. 바닥천은 직접 만든 평면에 원본 Tripo 캔버스 재질의 UV 영역을 사용한다. 외부 에셋과 추가 유료 생성은 사용하지 않았다.
- 바닥 UV 보정(2026-10-24): 밝은 테두리·고리가 섞이던 후면 텍스처 샘플을 전면부 하단 올리브색 천 영역으로 교체했다. 좁은 천 영역을 폭 방향으로 8회 배치해 무늬가 늘어나지 않도록 맞췄다. 원본 Tripo 재질을 재사용하며 추가 생성 비용은 없다.
- 입구 연결 보정(2026-10-24): 접힌 천·매듭·앞 기둥을 연결된 원본 구성별로 구분해 왼쪽 천이 절단 경계에서 잘리지 않도록 복원했다. 양쪽 접힌 천의 깊이와 옆면 입구 끝단을 함께 맞춰 파묻힘과 틈을 줄이고, 상단은 원래 기둥 연결로 부드럽게 전환했다. 전면부에 남던 왼쪽 옆면 중복 조각을 제거했으며 기존 UV·PBR 재질을 유지했다.
- 뒤벽 중앙 보정(2026-10-24): 바깥으로 돌출되던 별도 덮개 평면 2개를 제거했다. 뒤벽 양쪽 천의 중앙 가장자리를 같은 높이에서 분할·연장·용접하고 기존 뒤벽 UV를 연장해 하단 틈을 닫았다. [수정된 뒤쪽 렌더](../images/props/ysolde-tent-fitted-v3/rear.png). 내보낸 GLB의 기둥을 제외한 뒤벽 천에 정면·사선 레이 2,100개를 쏘아 중앙 틈이 없음을 확인했다. 기존 Tripo 재질을 재사용했으며 추가 생성 비용은 없다.
- 조립본: [Blender](../../assets/props/ysolde-tent/fitted_v3/ysolde-tent.blend), [후면 GLB](../../assets/props/ysolde-tent/fitted_v3/tent-back.glb)·[전면 GLB](../../assets/props/ysolde-tent/fitted_v3/tent-front.glb). [외부 렌더](../images/props/ysolde-tent-fitted-v3/outside.png)·[전면을 숨긴 내부 렌더](../images/props/ysolde-tent-fitted-v3/inside.png). 10개 방향별 레이 검사로 후면부의 왼쪽·뒤벽·바닥은 남고, 전면부의 오른쪽 경사천·입구만 분리되는 것을 확인했다. 아직 게임 배치와 침대 설치는 하지 않았다.
- 개발 미리보기: `/modular-character-preview.html?asset=ysolde-tent`. 전체·실내(전면 숨김)·전면부만 표시, 직교 카메라 회전·확대와 5개 시점, 1m 격자·와이어프레임을 지원한다. ‘잠자리’에서 놓지 않음·바닥 매트리스·낮은 야전침대·비교용 여관 침대를 고를 수 있으며 표시 모드·시점·선택은 URL에 저장한다. 각각 `bed=mattress`·`bed=cot`·`bed=inn`이고, 이전 `bed=1` 링크는 바닥 매트리스로 연다. ‘회전’ 각도와 R 키로 텐트·잠자리를 함께 돌리고 `rotation`에 각도를 저장한다. 이전 `facing` 링크도 같은 각도로 복원한다. 텐트는 아래 런타임 GLB를 읽으며 미리보기는 게임 월드의 배치 데이터를 수정하지 않는다.
- 맵 에디터 배치(2026-10-24): Object → **이졸데의 발굴 텐트**를 선택해 기존 회전 각도와 R 키로 방향을 바꾼다. 기본 모델의 입구는 로컬 +Z이므로 남쪽은 yaw 0°, 동쪽 90°, 북쪽 180°, 서쪽 270°다. R 키는 90°씩 돌리고, Select로 기존 텐트를 선택해 Rot 슬라이더로 변경·저장한다. 별도 입구 방향 선택 없이 기존 오브젝트 `rotation` 값으로 보존한다. 회전한 로컬 영역 안에 플레이어가 있으면 해당 텐트 인스턴스의 `tent_front`만 숨기며, 나가면 복원한다.
- 가장자리 통행 제한(2026-10-24): 카탈로그 `perimeter`의 본체 크기 2.4×3.2m·높이 2.3m와 로컬 +Z 중앙 입구 1.8m를 공유 Rust에서 1m 격자의 양방향 가장자리 플래그로 변환한다. 좌우·뒤와 입구 양옆은 막고 내부·입구는 비워 둔다. 회전·소수 좌표를 반영하며 WASM·서버·agent-client가 같은 생성·삭제 처리를 사용한다. 가구와 별도 캐시 항목을 써서 침대에 갇힌 플레이어의 탈출 예외가 텐트 벽을 통과시키지 않는다. 디버그 passability 표시에도 실제 가장자리 비트를 그린다. 네 방향·45°의 이동·반경·A* 검사, 실제 배치 좌표의 브라우저 검사, 텐트 안 침대의 눕기·일어나기를 확인했다.
- 텐트 지반 자동 평탄화(2026-10-24): 지상 배치·복제 시 회전한 4.4×4.8m 지지 영역의 평균 지형 높이로 본체를 맞춘다. 로프·말뚝을 포함한 지지 영역과 지형 보간용 1m 가장자리를 같은 높이로 만들고, 외곽 2m는 주변 지형과 완만하게 연결하며 풀을 정리한다. 말뚝의 묻힌 끝 대신 본체 바닥(로컬 Y=0)을 기준으로 쓰며 기존 집의 1층 바닥 영역은 보호한다. 오브젝트 저장이 성공한 뒤 지형을 저장하고, 2층 배치에는 자동 평탄화를 적용하지 않는다. 기존 텐트는 Select → **Flatten Terrain**으로 현재 배치 높이에 맞출 수 있다.
- 런타임 에셋: [ysolde_tent.glb](../../client/public/models/objects/ysolde_tent.glb), 13,426 triangles·4,230,908바이트. 최종 편집 원본에서 파트 이름과 UV를 유지해 조립한 뒤 베이스 컬러는 최대 2048², 노멀·ORM은 기존 재패킹 도구로 최대 1024²까지 줄였다. 기존 ImageGen **ChatGPT Pro 20x**·Tripo 유료 API 출처와 이용 조건을 승계하며 추가 생성은 없다. [내보내기 소스](../../assets/props/ysolde-tent/export_runtime.py), [파일 해시·출처](../../assets/props/ysolde-tent/runtime.json).
- 전송량 최적화(2026-10-28): 기존 런타임 GLB의 베이스 컬러 2장을 2048² JPEG에서 1024² WebP q94로 줄였다. GLB는 4,230,908 → 2,076,344바이트, gzip level 6은 3,778,052 → 1,654,454바이트(56.2% 감소)다. 5개 메시의 정점·인덱스, 노드 계층과 나머지 텍스처는 동일하다. 외부와 전면을 숨긴 단독 모델을 동일한 조명에서 비교했다. 원본 내보내기 후 `node tools/optimize-ysolde-tent.mjs client/public/models/objects/ysolde_tent.glb client/public/models/objects/ysolde_tent.glb`로 재현한다. 게임용 바이너리는 Hugging Face에 게시하고 `assets.lock`에 고정했다.
- 기존 텐트 미리보기에서 로딩과 실내 모드 전환을 확인했으며 브라우저 오류·실패 응답은 없었다. 텐트·맵 에디터 관련 테스트 20개가 통과했다. 게임 월드에서의 화면 비교는 아직 하지 않았다.
- 야간 명암 보정(2026-10-24): 좁은 가장자리 면의 노멀이 넓은 천 면에 섞여 생기던 삼각형 명암을 줄이기 위해 면적 가중 코너 노멀을 다시 계산했다. 같은 위치의 UV 경계 정점은 함께 계산하고 30°를 넘는 꺾임은 보존한다. 삼각형 수·위치·UV·재질은 유지했으며 그림자 설정은 변경하지 않았다. 미리보기의 ‘조명 → 야간 · 광휘의 투구’(`lighting=helm`)로 야간 명암과 그림자를 비교할 수 있다. 광원의 색·강도·범위·감쇠는 게임의 광휘 조명과 같으며, 높이 1.9m에서 시점을 따라 주변을 이동한다. 미리보기는 WebGL 그림자, 검증용 렌더는 게임의 포인트 그림자 필터를 사용했다.
- 고정 로프·말뚝 추가(2026-10-24): 앞뒤 기둥의 감긴 매듭에서 양옆으로 지름 21mm의 로프 4줄을 연결하고, 34cm 목제 말뚝에 두 바퀴 감아 고정했다. 로프에는 약간의 처짐을 주고 말뚝 아래쪽은 지면에 묻히게 했다. 본체 바닥은 2.4×3.2m 그대로이며 로프·말뚝을 포함한 전체 폭·깊이는 약 4.18×4.55m다. Blender에서 직접 만든 4,092 triangles이며 기존 Tripo 로프·기둥 목재의 UV 영역과 재질을 재사용한다. 신규 텍스처·외부 에셋·유료 생성은 없다. 앞쪽 줄·말뚝은 `tent_front`의 자식이라 전면과 함께 숨겨지고 뒤쪽은 남는다. 전체 줄의 연결부를 제외한 구간을 BVH로 검사해 본체와 최소 약 5.4cm 간격을 확인했으며, 기존 10개 방향별 개구부 검사도 통과했다.
- 커밋 전 정리(2026-10-24): 원본 검사용 Blender 2개·Tripo 결과 비교 이미지·배포와 문서에서 쓰지 않는 전면부 렌더·일회성 검사 스크립트 등 중간 파일 16개(약 35MiB)를 삭제했다. 원본 검사 수치는 각 `generation.json`에 통합하고, 최종 GLB·편집 Blender·재현 소스·생성 입력과 문서 이미지는 보존했다.

## References

- https://github.com/tigerabrodi/webgpu-vfx for reference — **[미사용]** 기법 참고용
- https://freestylized.com/material/moss_ground_01/ — **[미사용]** 참고용, 팔레트에 없음

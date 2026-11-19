# 여성 기사 하의

2026-11-20. 사용자가 제공한 원화를 기준으로 하의 메시와 재질을 새로 제작했다. 채택본은 **reference_v1/fitted_v1**이다.

허벅지 바깥을 감싸는 판을 좌우 다섯 장씩 겹쳤다. 위쪽 두 단의 사선 이음매, 아래에서 꺾이는 모서리, 판 끝의 리벳을 원화에 맞췄다. 허리 앞에서 교차하는 가죽띠와 부츠 고리까지 내려오는 끈, 둥근 덮개 아래로 가늘어지는 중앙 보호판을 각각 모델링했다. 앞뒤 끈의 끝과 황동 리벳은 부츠 윗단 곡면을 따르며 같은 쪽 허벅지 뼈에 고정했다. 뒤쪽 황동 고리는 윗단 가장자리에 걸치고 둥근 가죽 끝은 그 아래 16mm까지 내려온다. 뒤쪽 가죽끈은 겹판의 곡면을 따라 밀착하고, 골반과 허벅지가 따로 움직이는 부분은 황동 고리 아래에서 겹쳐 연결한다.

허리띠와 옆판의 바깥선을 [사용자가 표시한 곡선](../../assets/items/female_knight_bottom/reference_v1/waist-curve-guide.png)에 맞췄다. 앞 교차띠와 등 뒤 허리띠의 깊이를 줄이고, 앞뒤 세로끈의 윗끝과 리벳을 새 허리띠 곡면에 맞췄다. 위쪽 두 판은 골반에 고정하고, 아래 세 판은 대기 자세에서 윗판 안쪽으로 포개지도록 허벅지 관절을 기준으로 회전시켰다.

앞 가림판의 둥근 덮개를 아랫단 기준으로 4.5도 앞으로 돌리고, 교차띠 중앙과 리벳도 함께 띄워 피부 위로 금속 윗선이 보이게 했다. 앞뒤 중앙 가림판은 윗단을 기준으로 세로로 16% 늘렸다. 앞판 끝은 24.5mm, 뒷판 끝은 22.0mm 내려왔다. 안감은 통째로 18mm 위에 달았다. 판 사이로 보이는 속옷은 기준 몸체의 속옷 1,110삼각형과 관절 가중치를 그대로 사용한다. 허리띠와 같은 갈색 가죽 텍스처와 거칠기·금속성 맵을 공유한다.

- [편집용 Blender](../../assets/items/female_knight_bottom/reference_v1/fitted_v1/female-knight-bottom-fitted.blend) · [착용 GLB](../../assets/items/female_knight_bottom/reference_v1/fitted_v1/bottom_female_knight.glb) · [몸체 가림](../../assets/items/female_knight_bottom/reference_v1/fitted_v1/body-occlusion.json)
- 금속판은 장마다 골반 또는 같은 쪽 허벅지 뼈 하나를 따른다. 걷기·달리기·점프·앉고 일어서기·베기 등 11개 동작의 671개 시점에서 판 가장자리 길이 변화를 검사했다. [게임용 모델 검사 결과](../../assets/items/female_knight_bottom/reference_v1/fitted_v1/runtime-validation.json)
- 게임용 하의는 10,653삼각형, 재질 1개, 468,496바이트다. 기본 여성 몸체·단발·판금 5부위·시작 철검·횃불 조합은 숨김 포함 71,182삼각형, 착용 시 표시 54,281삼각형이다. 얼굴에 4,604삼각형을 사용한다.
- [사용자가 제공한 원화](../../assets/items/female_knight_bottom/reference_v1/user-reference.png)의 원저작자와 라이선스는 미확인이다. 메시와 UV는 로컬에서 제작했다. 은색 판금은 흉갑 앞면에서 채취한 색·거칠기·금속성 텍스처를 사용한다. 추가 생성 비용은 0이다. 몸체와 골격, 가림 인덱스는 기존 여성 기사의 자산을 사용한다.

## 재현

```bash
.venv/bin/python tools/build-female-knight-tassets.py
blender -b --python-exit-code 1 -P tools/blender-scripts/pack_female_knight_plate.py -- bottom
node tools/prepare-female-knight.mjs --part bottom
node tools/validate-female-knight-bottom.mjs
```

## 이전 피팅 v42 [미사용]

- [착용 GLB](../../assets/items/female_knight_bottom/tripo_v1/fitted_v42/bottom_female_knight.glb):
  12,118삼각형·4 primitives·65본, 리그 `human_female_01_local_v1`.
  2026-10-17 미터 단위로 전환했다.
- [Blender 제작본](../../assets/items/female_knight_bottom/tripo_v1/fitted_v42/female-knight-bottom-fitted.blend),
  [몸체 가림](../../assets/items/female_knight_bottom/tripo_v1/fitted_v42/body-occlusion.json),
  [피팅 명세](../../assets/items/female_knight_bottom/tripo_v1/fitted_v42/fitting.json).
- v42 검사 기록: [런타임](../../assets/items/female_knight_bottom/tripo_v1/fitted_v42/runtime-validation.json),
  [클립 연결](../../assets/items/female_knight_bottom/tripo_v1/fitted_v42/clip-attachment-validation.json),
  [직선 구간](../../assets/items/female_knight_bottom/tripo_v1/fitted_v42/straightness-validation.json).
  검사 도구는 삭제됐다.
- 가림은 몸체 하의 속옷 1,110삼각형만 숨기고 피부는 남긴다. 상의 가림과 합집합으로 적용한다.

v42의 재생성 도구는 남아 있지 않다. 위 v42 `.blend`·GLB가 당시 편집 원본이다.

## 런타임 메시 압축 — 2026-10-25

`client/public/models/characters/modular_female/bottom.glb`에 양자화 없이
Meshopt 압축을 적용했다. 텍스처 3장의 원본 WebP 바이트, 정점 속성,
65본 리깅·바인드 행렬·가림 명세를 보존한다. 삼각형 순서와 와인딩도
유지하며 각 삼각형의 시작 정점만 순환할 수 있다.

- GLB: 2,140,888 → 1,521,892 bytes (28.9% 감소).
- gzip level 6: 1,536,286 → 1,410,525 bytes (8.2% 감소).
- 압축 전 런타임 원본: `assets/items/female_knight_bottom/runtime-meshopt-v1/original.glb`.
- 검증 기록: `assets/items/female_knight_bottom/runtime-meshopt-v1/validation.json`.
- 재현: `node tools/compress-glb-meshes.mjs assets/items/female_knight_bottom/runtime-meshopt-v1/original.glb client/public/models/characters/modular_female/bottom.glb --lossless`.

`tools/prepare-female-knight.mjs --part bottom`으로 다시 만들 때도 같은
무손실 Meshopt 설정을 적용한다. 별도 출력 경로에서 재생성을 확인했다.
대기·걷기·달리기·베기·앉기 5개 동작의 55개 시점에서 스킨 변형을
비교했으며, 비교한 정점 132,495개의 위치 차이는 0이었다.
Chrome/Three.js의 정면·뒷면·달리기·앉기 렌더 4쌍도 픽셀 차이 0이며,
관련 회귀 테스트 12개를 통과했다.

## 색상 텍스처 1024 — 2026-10-25

메시 압축본의 색상 WebP만 2048×2048에서 1024×1024로 줄였다
(Lanczos3, WebP q94·effort 5). 압축된 메시 bufferView 19개와
노멀·ORM 이미지 바이트, 리깅·가림 명세는 그대로다.

- GLB: 1,521,892 → 1,074,144 bytes.
- gzip level 6: 1,410,525 → 962,577 bytes (추가 31.8% 감소).
- 색상 WebP: 686,190 → 238,454 bytes.
- 원본: `assets/items/female_knight_bottom/runtime-meshopt-v1/original.glb`에 위의 무손실 메시 압축을 적용한 결과를 입력으로 사용한다.
- 검증 기록: `assets/items/female_knight_bottom/runtime-color-1024-v1/validation.json`.
- 재현: 메시 압축 결과를 임시 GLB로 만든 뒤 `node tools/resize-glb-base-color.mjs INPUT.glb client/public/models/characters/modular_female/bottom.glb 1024`를 실행한다. 중간 GLB는 보관하지 않는다.

게임의 골격 연결·의상 가림 함수를 사용해 동일한 자세·카메라·조명으로
[착용 전체·하의 확대 비교](../images/female-knight-bottom-color-1024-before-after.png)를 렌더링했다.
전체 모습의 평균 RGB 채널 차이는 약 0.0056/255, 확대는 약 0.1677/255이며,
확대에서는 작은 문양이 조금 부드러워진다. 브라우저 예외는 없었다.
생성 도구도 이 하의의 색상 맵을 최대 1024로 만들며, 별도 출력으로 확인했다.

## 출처·라이선스

- 원화: OpenAI Codex ImageGen, **ChatGPT Pro 20x**, 2026-10-12. OpenAI 생성 출력물 이용 조건을 적용한다.
  - [concept_v2 시트](../../assets/items/female_knight_bottom/concept_v2/sheet.png)(채택),
    [프롬프트](../../assets/items/female_knight_bottom/concept_v2/prompt.txt),
    [출처·해시 기록](../../assets/items/female_knight_bottom/concept_v2/sources.json).
    concept_v1 시트에서 뒤 중앙 가리개만 앞 가리개 형태로 바꾼 편집본이다.
  - [concept_v1 시트](../../assets/items/female_knight_bottom/concept_v1/sheet.png)(concept_v2 편집 원본),
    [프롬프트](../../assets/items/female_knight_bottom/concept_v1/prompt.txt),
    [출처 기록](../../assets/items/female_knight_bottom/concept_v1/sources.json).
  - [사용자 제공 참고 이미지](../../assets/items/female_knight_bottom/concept_v1/user-reference.png):
    원저작자·라이선스 미확인.
- 3D: Tripo API, 유료 크레딧, 2026-10-12. multiview-to-model, 모델 `P2-20260801`,
  텍스처 `v3.5-20260815`, detailed PBR, 조명 제거, 목표 4,000 triangles, 자동 크기·자동 리깅 미사용.
  작업 ID `6722d281-43da-4a7e-944f-69a49d6117bd`, 비용 120크레딧.
  [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
  - [요청 명세](../../assets/items/female_knight_bottom/tripo_v1/build.json),
    [생성·비용·해시 기록](../../assets/items/female_knight_bottom/tripo_v1/generation.json).
  - 입력: concept_v2 시트를 FFmpeg로 자른 887×887
    [앞면](../../assets/items/female_knight_bottom/tripo_v1/inputs/front.png)·[뒷면](../../assets/items/female_knight_bottom/tripo_v1/inputs/back.png).
  - [Tripo 원본 GLB](../../assets/items/female_knight_bottom/tripo_v1/source/model_url.glb):
    3,578 triangles, 리깅 전, 4096×4096 색상·노멀·ORM 텍스처.
    [원본 Blender](../../assets/items/female_knight_bottom/tripo_v1/source.blend),
    [원본 검사](../../assets/items/female_knight_bottom/tripo_v1/review.json),
    [추출 텍스처 기록](../../assets/items/female_knight_bottom/tripo_v1/texture-extraction.json).
- 피팅·리깅·끈 정렬은 로컬 Python/Blender 편집이며 추가 생성·유료 API 호출은 없다.

## 제한

- 끈은 가중치 변형이며 천 시뮬레이션이 아니다. 앉기와 큰 관절 회전에서 끈이 몸에 묻히거나 접힐 수 있다.
- 모든 동작의 무관통은 검증하지 않았다.
- v42 `.blend`는 미터 전환 전 제작 단위(몸체 높이 1)로 남아 있다. 단위 전환은 [여성 기사 런타임](female-knight-runtime.md#원본-미터-단위-전환--2026-10-17)을 참고한다.

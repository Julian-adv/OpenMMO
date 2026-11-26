# 여성 바바리안 하의

[여성 바바리안 원화](../../client/public/character_concepts/female_barbarian.webp)의 하의(리벳 허리띠·원형 버클·짧은 모피 치마)를
Tripo로 만들어 여성 공통 몸체(`human_female_01_local_v1`, 65본)에 피팅한 파츠다.
게임에서는 여성 캐릭터의 하의 슬롯에 `barbarian_bottom.glb`로 표시된다(적용 범위는 [여성 기사 게임 적용](female-knight-runtime.md#여성-야만용사-적용--2026-10-16)).
채택본은 `tripo_v1/fitted_v1`이며 원본 좌표는 미터 단위다.

![여성 바바리안 하의 원화](../../assets/items/female_barbarian_bottom/concept_v1/sheet.png)

## 최종 파일

| 용도 | 파일 |
| --- | --- |
| 하의 GLB | [bottom_female_barbarian.glb](../../assets/items/female_barbarian_bottom/tripo_v1/fitted_v1/bottom_female_barbarian.glb) |
| 몸체 가림 | [body-occlusion.json](../../assets/items/female_barbarian_bottom/tripo_v1/fitted_v1/body-occlusion.json) |
| 형상·깊이 데이터 | [fit-data.npz](../../assets/items/female_barbarian_bottom/tripo_v1/fitted_v1/fit-data.npz) |
| 피팅 기록 | [modular-female-barbarian-bottom-fitting-v1.json](modular-female-barbarian-bottom-fitting-v1.json) |
| 동작 검사 | [modular-female-barbarian-bottom-animation-v1.json](modular-female-barbarian-bottom-animation-v1.json) |
| 성능 기록 | [modular-female-barbarian-performance.json](modular-female-barbarian-performance.json) |
| 게임용 | `client/public/models/characters/modular_female/barbarian_bottom.glb` |

- 메시 5개(허리띠·버클·고리, 앞 판·땋은 끈, 뒤 판, 왼쪽 판, 오른쪽 판), **3,340 triangles**, 정점 4,999개.
  텍스처는 Tripo 원본을 WebP q90으로 내보냈다.
- 2026-11-26 용량 최적화: 1.74MB → 0.73MB. 게임용 GLB의 base color만 2048² → 1024² WebP q90으로 줄이고
  무손실 meshopt(정점 순서 유지)를 걸었다. `robe_physics` 정점 인덱스와 검증 수치는 그대로다.
  `prepare-female-knight.mjs`를 다시 돌리면 2048²·무압축으로 돌아가므로 이 후처리를 다시 해야 한다.
- 모피 4판은 원본에 `pelt_physics`(미터 값, `unit: 1.8`)를 담는다. 게임은 패키징 때 여기서 만든 `robe_physics`
  (판 4장이 허리 회전축 하나를 공유, 캐릭터당 확장 스켈레톤 1개·제어 본 6개)로 움직인다.
- 치마 착용 중에는 허리띠 안쪽 피부·팬티 윗단과 골반 일부를 가린다(피부 224면·팬티 408면). 벗으면 원래 인덱스로 복원한다.

## 재현

`.blend`는 없고, 피팅·수리 스크립트와 단계별 폴더는 삭제했다. 채택 GLB 자체가 편집 원본이다.

```bash
node tools/prepare-female-knight.mjs
node tools/validate-female-barbarian-bottom.mjs
blender -b --python-exit-code 1 -P tools/blender-scripts/review_female_barbarian_bottom.py
node tools/benchmark-female-barbarian.mjs --count=10 --output=/tmp/female-crowd-after.json
```

`prepare-female-knight.mjs`가 게임용 GLB와 `robe_physics`를 만든다.
미리보기: `/modular-character-preview.html?character=female&camera=lower&bottom=barbarian`.

## 출처·라이선스

- 원화: OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.162.1), **ChatGPT Pro 20x**, 2026-10-15.
  OpenAI 출력물 이용 조건을 따른다. 참조 원화의 출처는 [캐릭터 문서](characters.md)에 있다.
  - [앞뒤 원화](../../assets/items/female_barbarian_bottom/concept_v1/sheet.png),
    [프롬프트](../../assets/items/female_barbarian_bottom/concept_v1/prompt.txt),
    [출처·해시](../../assets/items/female_barbarian_bottom/concept_v1/sources.json).
  - 참조 입력: 원본 원화를 흰 바탕에 합성해 `[290,560,440,380]`을 자른
    [reference-crop.png](../../assets/items/female_barbarian_bottom/concept_v1/reference-crop.png).
- 3D: Tripo API 유료 크레딧, 2026-10-15. 모델 `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR,
  입력 조명 제거, 삼각형 목표 4,000, seed 101501.
  [API 문서](https://developers.tripo3d.ai/en/docs/generation-multiview-to-model/p),
  [이용 조건](https://developers.tripo3d.ai/en/terms).
  - 작업 ID `b323b163-bc48-4cf7-9e66-917a8ff30854`, **120크레딧 사용**, 잔액 800 → 680.
    재제출하지 않고 `tools/outfits/tripo.py`로 기존 작업을 조회·회수한다.
  - 입력: 원화를 열 780에서 나눈 870×870 [앞면](../../assets/items/female_barbarian_bottom/tripo_v1/inputs/front.png)·
    [뒷면](../../assets/items/female_barbarian_bottom/tripo_v1/inputs/back.png).
  - [제작 명세](../../assets/items/female_barbarian_bottom/tripo_v1/build.json),
    [요청·비용·작업 기록](../../assets/items/female_barbarian_bottom/tripo_v1/generation.json).
  - 원본: [model_url.glb](../../assets/items/female_barbarian_bottom/tripo_v1/source/model_url.glb)
    (메시 1개, 3,241 triangles, 4K 컬러·노멀·ORM),
    [rendered_image_url.png](../../assets/items/female_barbarian_bottom/tripo_v1/source/rendered_image_url.png),
    [source.blend](../../assets/items/female_barbarian_bottom/tripo_v1/source.blend),
    [원본 진단](../../assets/items/female_barbarian_bottom/tripo_v1/review.json)
    ([재현 스크립트](../../assets/items/female_barbarian_bottom/tripo_v1/review_source.py)).
- 피팅에서 추가한 면(구멍 메움 15, 허리띠·모피 연결 84)은 기존 Tripo 텍스처의 갈색 안감·모피 영역을 쓴다. 추가 생성 비용은 없다.

## 제한

- 기본 몸체 피부에 속옷이 구워져 있어 모피 틈으로 흰색이 보일 수 있다. 큰 틈은 면을 메워 막았다.
- 큰 동작에서 허벅지가 모피 판 앞으로 조금 지나가고, 달리기·앉기에서 옆 판이 비스듬히 벌어진다(쓰러짐에서 늘어남 최대 5.2배).
- 앞 가운데 판 끝은 무릎 위 약 5cm로 원화보다 조금 길다.

# 여성 바바리안 뿔 투구

[여성 바바리안 원화](../../client/public/character_concepts/female_barbarian.webp)의 뿔 투구를 앞·옆·뒤 원화로 옮기고
Tripo로 생성해 여성 공통 몸체(`human_female_01_local_v1` 65본)의 머리에 피팅했다. 채택본은 Tripo v2의 `fitted_v1`이다.
투구 전체를 **Head 100%**로 연결하고 피부는 가리지 않는다. 투구 아래로 보이는 헤어는 전용으로 잘라낸 단발·긴 헤어를 쓴다.
게임 적용은 [여성 야만용사 런타임](female-knight-runtime.md#여성-야만용사-적용--2026-10-16)을 따른다.

![앞·옆·뒤 원화](../../assets/items/female_barbarian_helmet/concept_v1/sheet.png)

## 최종 파일

`assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/`:

| 용도 | 파일 |
| --- | --- |
| 투구 | [helmet_female_barbarian.glb](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/helmet_female_barbarian.glb) (2,179 triangles, 4K 컬러·노멀·ORM) |
| 투구 Blender | [female-barbarian-helmet-fitted.blend](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/female-barbarian-helmet-fitted.blend) |
| 몸체 가림 | [body-occlusion.json](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/body-occlusion.json) (가리는 면 없음) |
| 단발 | [hair_bob_barbarian_helmet.glb](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/hair_bob_barbarian_helmet.glb) (1,805 triangles) |
| 긴 헤어 **[미사용]** (2026-11-05 미리보기에서도 제외) | [hair_knight_barbarian_helmet.glb](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/hair_knight_barbarian_helmet.glb) (3,707 triangles) |
| 헤어 Blender | [female-barbarian-helmet-hair-fitted.blend](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/female-barbarian-helmet-hair-fitted.blend) (긴 헤어는 숨김) |
| 기록 | [피팅](modular-female-barbarian-helmet-fitting-v1.json), [동작 검사](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/animation-validation.json), [헤어 절단·해시](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/hair-trim.json), [헤어 동작 검사](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/hair-animation-validation.json), [볼가리개 노출 기준점](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/hair-cheek-coverage-anchors.json), [노출 검사](../../assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/hair-cheek-coverage-validation.json) |

투구를 벗으면 원래 헤어로 돌아가고, 헤어 없음·단발·긴 헤어 선택을 유지한다.

## 재현

```bash
.venv/bin/python tools/fit-female-barbarian-helmet.py
node tools/validate-female-barbarian-helmet.mjs
blender -b --python-exit-code 1 -P tools/blender-scripts/review_female_barbarian_helmet.py
.venv/bin/python tools/trim-female-barbarian-helmet-hair.py
OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 node tools/validate-female-barbarian-helmet-hair.mjs
node tools/validate-female-barbarian-helmet-hair-coverage.mjs
blender -b --python-exit-code 1 --python tools/blender-scripts/save_female_barbarian_helmet_hair.py
```

헤어 절단에는 NumPy·SciPy·Shapely 2.1·trimesh가 필요하다.

## 출처·라이선스

- 원화: OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.162.1), **ChatGPT Pro 20x**, 2026-10-15.
  OpenAI 출력물 이용 조건을 따른다. 참조 원화의 출처는 [캐릭터 문서](characters.md)에 있다.
  [프롬프트](../../assets/items/female_barbarian_helmet/concept_v1/prompt.txt),
  [참조·분리 영역·해시](../../assets/items/female_barbarian_helmet/concept_v1/sources.json),
  [참조 입력](../../assets/items/female_barbarian_helmet/concept_v1/reference-crop.png)(원화 머리 부분을 흰 배경에 잘라 3배 확대).
  뒤쪽 목가리개는 원래 원화에 없는 부분이라 새로 정했다.
- 3D: Tripo API 유료 크레딧, 모델 `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR, 입력 조명 제거,
  목표 2,500 triangles, 2026-10-15. [API 문서](https://developers.tripo3d.ai/en/docs/generation-multiview-to-model/p),
  [이용 조건](https://developers.tripo3d.ai/en/terms). 두 번 생성해 **240크레딧**을 썼다(잔액 440 → 200).
- **v2(채택)**: 앞·뒤 2장, seed 101505, 작업 ID `58d0c136-f714-4e60-bc2f-aed24abf3901`, 120크레딧.
  [제작 명세](../../assets/items/female_barbarian_helmet/tripo_v2/build.json),
  [요청·비용·작업 기록](../../assets/items/female_barbarian_helmet/tripo_v2/generation.json),
  [입력](../../assets/items/female_barbarian_helmet/tripo_v2/inputs/),
  [Tripo GLB](../../assets/items/female_barbarian_helmet/tripo_v2/source/model_url.glb),
  [생성 미리보기](../../assets/items/female_barbarian_helmet/tripo_v2/source/rendered_image_url.png),
  [원본 Blender](../../assets/items/female_barbarian_helmet/tripo_v2/source.blend),
  [원본 진단](../../assets/items/female_barbarian_helmet/tripo_v2/review.json)([스크립트](../../assets/items/female_barbarian_helmet/tripo_v2/review_source.py)).
- **[미사용] v1**: 앞·옆·뒤 3장, seed 101504, 작업 ID `9c2d2ed5-c194-4d5b-905f-c25dde1a51d9`, 120크레딧.
  뒤통수에 앞면(이마 끝·볼가리개·얼굴 구멍)이 복제되어 쓰지 않는다. 생성 원본으로 보존한다.
  [제작 명세](../../assets/items/female_barbarian_helmet/tripo_v1/build.json),
  [작업 기록](../../assets/items/female_barbarian_helmet/tripo_v1/generation.json),
  [입력](../../assets/items/female_barbarian_helmet/tripo_v1/inputs/),
  [Tripo GLB](../../assets/items/female_barbarian_helmet/tripo_v1/source/model_url.glb),
  [생성 미리보기](../../assets/items/female_barbarian_helmet/tripo_v1/source/rendered_image_url.png),
  [원본 Blender](../../assets/items/female_barbarian_helmet/tripo_v1/source.blend),
  [원본 진단](../../assets/items/female_barbarian_helmet/tripo_v1/review.json)([스크립트](../../assets/items/female_barbarian_helmet/tripo_v1/review_source.py)).
- 헤어: [금발 헤어](modular-female-knight-hair.md)의 파생본이며 그 출처·이용 조건을 따른다.
  원본 헤어 GLB와 투구 GLB는 수정하지 않았다.

## 제한

- 동작 검사는 정지 자세와 미리보기 클립 14개 표본에서 관통 0이다. 낚시·승마 등 런타임 전용 클립은 검사하지 않았다.
- 헤어 검사는 표본 239시점에서 교차 0건, 볼가리개 노출 기준점 121/121 통과다. 모든 각도·동작을 보장하지 않는다.
- `.blend` 편집본은 미터 전환 전 제작 단위(리그 높이 1)다.

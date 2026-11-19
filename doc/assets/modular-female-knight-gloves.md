# 여성 기사 장갑

팔꿈치 보호대·팔뚝 판금·손가락 장갑과 안쪽 가죽을 묶은 장갑 슬롯이다. 상의 소매를 바깥에서 덮는다. 은색 판은 흉갑의 색과 거칠기를 기준으로 보정하고 표면 요철을 줄였다.
채택본은 `tripo_v1/fitted_v3`이며 여성 기준 몸체 65본 리그에 연결했다.
양손 **4,693삼각형**(장갑 4,140 + 팔꿈치 안감 553)이고, 보호대는 같은 쪽 Arm 55%·ForeArm 45%로 움직인다.

![장갑 원화](../../assets/items/female_knight_gloves/concept_v2/sheet.png)

## 최종 파일

- [GLB](../../assets/items/female_knight_gloves/tripo_v1/fitted_v3/gloves_female_knight.glb),
  [Blender 편집본](../../assets/items/female_knight_gloves/tripo_v1/fitted_v3/female-knight-gloves-fitted.blend)
- [피팅 기록](../../assets/items/female_knight_gloves/tripo_v1/fitted_v3/fitting.json),
  [피팅 데이터](../../assets/items/female_knight_gloves/tripo_v1/fitted_v3/fit-data.npz),
  [몸체 가림](../../assets/items/female_knight_gloves/tripo_v1/fitted_v3/body-occlusion.json),
  [상의 소매 가림](../../assets/items/female_knight_gloves/tripo_v1/fitted_v3/top-occlusion.json)
- [동작 검사](../../assets/items/female_knight_gloves/tripo_v1/fitted_v3/animation-validation.json),
  [조립 삼각형](../../assets/items/female_knight_gloves/tripo_v1/fitted_v3/assembly-counts.json),
  [해시](../../assets/items/female_knight_gloves/tripo_v1/fitted_v3/local-artifacts.json)

## 재현

`fit-female-knight-gloves.py`가 Tripo 원본에서 `fitted_v3`를 직접 만든다. `--pair` 없이 실행하면 오른손만 만든다.
접촉 측정은 동작 검사가 쓰는 `validation-poses.json`이 필요하다.

```bash
.venv/bin/python tools/fit-female-knight-gloves.py --pair
node tools/validate-female-knight-gloves.mjs --pair
.venv/bin/python tools/validate-female-knight-gloves-clearance.py
blender -b --python-exit-code 1 --python tools/blender-scripts/review_female_knight_gloves.py -- --pair
```

## 출처·라이선스

- 원화: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-13. OpenAI 출력물 이용 조건 적용.
  v2 [프롬프트](../../assets/items/female_knight_gloves/concept_v2/prompt.txt),
  [출처·해시](../../assets/items/female_knight_gloves/concept_v2/sources.json).
- **[미사용]** [v1 시트](../../assets/items/female_knight_gloves/concept_v1/sheet.png)
  ([프롬프트](../../assets/items/female_knight_gloves/concept_v1/prompt.txt),
  [출처](../../assets/items/female_knight_gloves/concept_v1/sources.json)): v2의 편집 원본. v2는 보호대 위 밴드·천 소매를 지운 수정본이다.
- 원화 참고: [사용자 이미지](../../assets/items/female_knight_top/concept_v1/user-reference.png)(상의와 공용). 원저작자·라이선스 미확인.
- 메시: Tripo API 유료 크레딧, 2026-10-13. `P2-20260801`, 한쪽 2,000삼각형 목표(실제 2,144), detailed PBR,
  `v3.5-20260815`, 입력 조명 제거. 작업 ID `bd7d5e10-8255-4ac9-ad65-21646f90d1c5`, **120크레딧**.
  [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
  [제작 명세](../../assets/items/female_knight_gloves/tripo_v1/build.json),
  [작업·비용 기록](../../assets/items/female_knight_gloves/tripo_v1/generation.json),
  [해시](../../assets/items/female_knight_gloves/tripo_v1/local-artifacts.json).
- Tripo 입력: v2 시트에서 FFmpeg로 손등 `[0,160,596,1070]`, 손바닥 `[596,160,597,1070]`을 잘라
  [front](../../assets/items/female_knight_gloves/tripo_v1/inputs/front.png)·[back](../../assets/items/female_knight_gloves/tripo_v1/inputs/back.png)으로 제출했다.
- 생성 원본: [GLB](../../assets/items/female_knight_gloves/tripo_v1/source/model_url.glb),
  [렌더](../../assets/items/female_knight_gloves/tripo_v1/source/rendered_image_url.png),
  [텍스처·재질 대응](../../assets/items/female_knight_gloves/tripo_v1/source/textures.json)(4K 컬러·노멀·ORM),
  [텍스처 내장 Blender](../../assets/items/female_knight_gloves/tripo_v1/source.blend),
  [메시 진단](../../assets/items/female_knight_gloves/tripo_v1/review.json)·[스크립트](../../assets/items/female_knight_gloves/tripo_v1/review_source.py).
- 팔꿈치 안감은 여성 기준 몸체 표면에서 만든 절차적 단색 재질로, 몸체의 출처·이용 조건을 따른다. 피팅·대칭·리깅은 로컬 작업이며 추가 비용은 없다.

## 제한

- 보호대 안쪽에 열린 경계가 남아 밀폐 솔리드가 아니다.
- 가림 전 피부와의 내부 접촉이 남으며, 모든 시점의 무관통은 검사하지 않았다. 다른 상의·혼합 장갑 조합은 미검증이다.

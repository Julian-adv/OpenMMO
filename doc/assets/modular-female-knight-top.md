# 여성 기사 판금 상의

2026-11-20. 기존 Tripo 원본에서 짧은 흉갑과 양쪽 어깨·상완 판금을 다시 피팅했다.

## 채택한 피팅

원본의 금속성 텍스처와 메시 연결 관계로 흉갑·등판·목깃, 왼쪽 어깨·상완, 오른쪽 어깨·상완을 구분했다. 각 부위에 균일 배율 하나와 회전·이동 하나를 적용했다. 흉갑은 Spine1, 양쪽 견갑은 같은 쪽 Arm 본을 따른다. 가죽·천 이음새에서만 변환을 섞었다. 금속 거칠기를 높이고 노멀 맵 강도를 0.5로 줄여 은색 판금에 부드러운 반사가 맺히게 했다. 원본 UV와 삼각형 순서를 유지해 몸체·장갑 가림을 그대로 적용한다.

소매 끝까지 원본에서 통째로 맞춘 첫 시안은 팔 피부가 입구를 최대 21.3mm 통과했다. 기존 v3 천 소맷단과 가중치를 다시 사용했다. 맨팔과 만나는 양쪽 입구 정점 20개를 9동작에서 각각 17번 측정했으며, 최소 여유는 14.3mm였다. 이 측정은 소매 입구에 한정한다.

- [착용 GLB](../../assets/items/female_knight_top/tripo_v1/fitted_v4/top_female_knight.glb) · [Blender 편집본](../../assets/items/female_knight_top/tripo_v1/fitted_v4/female-knight-top-fitted.blend) · [몸체 가림](../../assets/items/female_knight_top/tripo_v1/fitted_v4/body-occlusion.json)
- 금속 2,130면의 모서리 길이를 균일 배율 적용 후 원본과 비교했다. 저장된 메시의 최대 상대 오차는 0.003%다. 65본을 결합한 뒤 대기·걷기·달리기·점프·공격·앉기 등을 포함한 9동작 1,089자세에서 단단한 판의 길이 변화를 검사했다. [피팅 측정](modular-female-knight-top-fitting-v4.json) · [동작 측정](modular-female-knight-top-animation-v4.json)
- 게임용 상의는 4,279삼각형, 메시 1개·재질 1개다. 파일 크기는 1,059,808에서 997,792바이트로 줄었다. 기본 여성 몸체·단발·판금 5부위·시작 철검·횃불을 합하면 숨김 포함 71,182삼각형, 착용 시 표시 54,281삼각형이다. 얼굴 영역은 기존 4,604삼각형이고 이 조합은 망토를 착용하지 않는다.
- 기존 Tripo 생성물의 로컬 편집이다. 2026-10-12 제작 출처와 이용 조건을 따르며 추가 생성 비용은 없다.

## 재현

```bash
.venv/bin/python tools/refit-female-knight-top.py
blender -b --python-exit-code 1 -P tools/blender-scripts/pack_female_knight_plate.py -- top
node tools/prepare-female-knight.mjs --part top
node tools/validate-female-knight-top-v2.mjs v4
```

## 출처·라이선스

- 원화: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-10-12. OpenAI 출력물 이용 조건 적용.
  [앞뒤 원화](../../assets/items/female_knight_top/concept_v1/sheet.png),
  [프롬프트](../../assets/items/female_knight_top/concept_v1/prompt.txt),
  [출처·해시](../../assets/items/female_knight_top/concept_v1/sources.json).
- 원화 참고: [사용자 사진](../../assets/items/female_knight_top/concept_v1/user-reference.png). 원저작자·라이선스 미확인.
- 메시: Tripo API 유료 크레딧, 2026-10-12. `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR, 입력 조명 제거,
  4,000삼각형 목표. 작업 ID `93a31e72-6ad5-4e28-84c6-293ddf9062cc`, **120크레딧**.
  [API 문서](https://developers.tripo3d.ai/en/docs/generation-multiview-to-model/p), [이용 조건](https://developers.tripo3d.ai/en/terms).
  [제작 명세](../../assets/items/female_knight_top/tripo_v1/build.json),
  [요청·비용·작업 기록](../../assets/items/female_knight_top/tripo_v1/generation.json).
  `tools/outfits/tripo.py`로 기존 작업을 조회·회수한다.
- Tripo 입력: 시트의 `[0,128,768,704]`, `[768,128,768,704]`를 FFmpeg로 잘라
  [앞](../../assets/items/female_knight_top/tripo_v1/inputs/front.png)·[뒤](../../assets/items/female_knight_top/tripo_v1/inputs/back.png)로 제출했다.
- 생성 원본: [GLB](../../assets/items/female_knight_top/tripo_v1/source/model_url.glb),
  [Blender](../../assets/items/female_knight_top/tripo_v1/source.blend),
  [메시 진단](../../assets/items/female_knight_top/tripo_v1/review.json)·[스크립트](../../assets/items/female_knight_top/tripo_v1/review_source.py).
- 피팅·리깅은 로컬 작업이며 추가 비용은 없다.


## 이전 피팅 [미사용]

2026-10-12의 v3는 몸통과 팔을 국소 변형한 버전이다. v4는 여기서 여성 리그, 검증된 천 소맷단과 착용 위치를 참조한다. [v3 편집본](../../assets/items/female_knight_top/tripo_v1/fitted_v3/female-knight-top-fitted.blend) · [착용 화면](../images/characters/modular_human_female_01/female-knight-top-v3.png) · [소매 보정](modular-female-knight-top-fitting-v3.json) · [동작 검사](modular-female-knight-top-animation-v3.json) · [브라우저 검사](modular-female-knight-top-preview-v3.json)

# 여성 합병식 예복

가문 합병식에 입는 여성 예복 기록이다. 여성 공통 몸체는 [여성 몸체 기록](modular-human-female-01.md), 제작 절차는 [공통 워크플로우](modular-outfit-workflow.md)와 [연결 기준](modular-outfit-connections.md)을 따른다.
이름은 "합병식 예복"으로 쓴다.

## 구성 (사용자 결정 2026-11-14)

- 흰색 새틴에 금 장식. 어깨를 드러낸 보디스, 초승달 두 개 걸쇠의 금 벨트, 속치마로 부풀린 무릎 아래 길이 종 모양 치마.
- 팔꿈치 아래 길이 흰 장갑, 흰 스타킹과 흰 하이힐, 초승달 금 머리띠에 단 베일.
- 슬롯 계획: 상의, 치마, 장갑, 신발(스타킹+구두), 머리(베일).
- 치마가 넓어 내린 팔이 묻히므로 이 옷에는 팔을 조금 벌린 대기 자세를 쓴다. 치마에는 전용 본을 추가한다. 둘 다 미제작.

## 원화 — 2026-11-14

![세 시점 원화](../../assets/items/female_ceremony_outfit/concepts/sheet-v3.png)

마네킹: `tools/blender-scripts/render_female_concept_base.py --mannequin --heel 30 --view front|side|back`로 여성 몸체를 파란 무광으로 렌더했다(Blender 5.2 Cycles, 생성 아님). 생성 모델에서 몸과 옷을 색으로 가르고 마네킹끼리 맞춰 올리기 위해서다([긴 생머리 기록](modular-female-long-straight-hair.md)과 같은 방식). 옷에는 파란색 계열을 쓰지 않았다.

원화: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-11-14. OpenAI 생성 출력물 이용 조건을 따른다. 파일은 `assets/items/female_ceremony_outfit/concepts/`, 프롬프트는 같은 폴더의 `*-prompt.txt`.

| 파일 | SHA-256 | 내용 |
| --- | --- | --- |
| `mannequin-front.png` | `27ec968a…da06f` | 마네킹 정면 |
| `mannequin-side.png` | `792f5a08…7e86` | 마네킹 옆 (얼굴이 화면 왼쪽, Tripo `left` 칸) |
| `mannequin-back.png` | `2047d83c…3e69` | 마네킹 뒤 |
| `front-v3.png` | `6dda5e10…8cf2fd` | 정면 채택본 |
| `side-v1.png` | `7e91a00d…427b` | 옆 채택본 |
| `back-v1.png` | `f76cb550…bc06` | 뒤 채택본 |

생성 뒤 손본 곳(원본은 `*-raw.png`):

- 정면: 화면 오른쪽 다리가 마네킹보다 35px 안쪽이었다. 맞게 그려진 왼쪽 다리를 y≥1140에서 좌우 대칭으로 옮겨 붙였다. 다리 바깥선 차이 0–7px.
- 옆: 다리가 26px 앞으로 나와 y≥1136 구간을 26px 뒤로 옮겼다.
- 뒤: 손이 파란 맨손으로 그려져 손 영역의 파란 화소를 흰색으로 바꿨다.

정면과 뒤의 벨트·밑단 높이는 1px 안에서 같다.

실패·폐기:

- `front-v1.png`: 아이보리, 발목 길이 A라인, 베일 없음. 사용자가 흰색·넓고 짧은 치마·베일로 바꿨다.
- `front-v2.png`: 스타킹 없음, 다리 위치 어긋남. v3로 대체.

## Tripo 생성 — 2026-11-14

- Tripo API, 유료 API 크레딧, 작업 ID `a5cdc033-9e17-4404-b617-4a1dae3627d4`, 120크레딧(3520 → 3400). [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- 다중 시점 입력: front `front-v3.png`, left `side-v1.png`, back `back-v1.png`. 설정은 `tripo_multiview_v1/build.json`(P2-20260801, face_limit 20000, detailed 텍스처, seed 111401), 응답은 `generation.json`.
- 출력 `tripo_multiview_v1/model_url.glb`, SHA-256 `d08ac592fa01a44808177992537293a72fc6c11e437929a72b0ff0fba09bb8dc`. 메시 1개, 19,850 triangles, 4096² 텍스처, 높이 1.0, +X 정면.

## 마네킹 분리

`tools/split-female-ceremony-outfit.py`가 `tripo_multiview_v1/segmented.glb`와 `split.json`을 만든다.

- 파란 면(텍스처 B−R > 15가 절반 이상) 5,807개를 버린다. 파란 면 중 가장 큰 덩어리를 30° 하이힐 자세 몸체에 ICP로 맞춘다(배율 1.837, 평균 6.3mm, 95% 17.9mm).
- 남은 면은 연결 성분으로 나눈다: 드레스, 다리, 베일, 장갑 둘, 머리띠, 걸쇠. 드레스는 금 벨트 윗선(1.264m)에서 수평으로 잘라 상의와 치마로, 다리는 0.829m에서 잘라 좌우 신발로 나눈다(높이는 굽 들림 4.9cm 포함).
- 부위별로 몸체와의 차이: 스타킹 정강이가 1.5cm 뒤, 장갑이 1.6cm 앞. 좌우 차이는 3mm 안이다.
- B−R > 40인 파란 화소만 가까운 옷 색으로 메운다. 기준을 15로 낮추면 밑단 흰 띠가 금 자수 색으로 번졌다.

## 피팅 v1

[피팅 스크립트](../../tools/blender-scripts/fit_female_outfit.py)의 `ceremony` 항목. 배율 1, `on_heels`.

| 부위 | 출력 | Triangles | 가린 몸체 삼각형 |
| --- | --- | ---: | ---: |
| 상의 | `top/fitted_v1/top_female_ceremony.glb` | 2,301 | 3,152 |
| 치마 | `bottom/fitted_v1/bottom_female_ceremony.glb` | 4,240 | 1,542 |
| 장갑 | `gloves/fitted_v1/gloves_female_ceremony.glb` | 2,645 | 4,212 |
| 스타킹과 구두 | `boots/fitted_v1/boots_female_ceremony.glb` | 2,514 | 3,359 |
| 베일 | `helmet/fitted_v1/helmet_female_ceremony.glb` | 1,877 | 162 |

합계 13,577 triangles, 몸체는 따로다.

- 상의는 `hide_underwear=(1.15, 1.7)`로 속옷 상의를 끈까지 가린다.
- 치마는 다리 가중치만으로는 걸을 때 찢어졌다. 치마 노드에 `robe_physics`를 넣어 게임의 밑단 본([robe-rig](../../client/src/lib/effects/robe-rig.ts))이 골반에 매달고, 허벅지·정강이 충돌체를 피할 만큼만 밑단을 든다.

![대기·걷기·달리기](../../assets/items/female_ceremony_outfit/preview-idle-walk-run.png)

미리보기 `/modular-character-preview.html?character=female&top=ceremony&bottom=ceremony&gloves=ceremony&boots=ceremony&helmet=ceremony`. 미리보기는 얼굴 파츠의 목 띠에 의상 가림을 적용하지 않아 속옷 끈 윗부분이 남아 보인다. 게임은 `registerBodyMask`가 목 띠도 가린다.

남은 일:

- 팔: 대기에서 손과 아래팔이 치마에 묻히고, 걷기·달리기에서 팔이 치마를 스친다. 팔 벌린 자세 미제작.
- 베일: 단발 위에서 머리띠가 머리카락에 묻힌다. 헤어별 마스크 미제작.
- 상의만 입으면 아랫단에 톱니가 보인다.
- 아이템·아이콘·상점 등록, 게임 내 확인, 남성 캐릭터 착용 시 표시.

## 상의 재생성 (v2) — 2026-11-14

v1은 베일이 등을 덮은 채 생성돼 보디스 등판이 톱니 모양으로 비었다. 베일과 금관을 지운 세 시점(`front-v4.png`, `side-v2.png`, `back-v2.png`; v1 원화의 국소 편집, 옷·다리 위치 차이 2px 안)으로 다시 생성했다(사용자 결정).

- Tripo API, 작업 ID `5fe7ff40-304d-4ca6-bd01-06c0ab9b5e9f`, 120크레딧(3400 → 3280), 설정은 v1과 같고 seed 111402. 출력 `tripo_multiview_v2/model_url.glb`, SHA-256 `b38f26bcbba3cab2a6065b99dcb61d449103f3f0b6c8a2a0a37d4d1299880449`.
- 분리: `tools/split-female-ceremony-outfit.py --version v2`. 맞춤 평균 4.5mm, 95% 10.1mm.
- v2에서는 상의만 쓴다. v2 치마는 스타킹과 붙어 생성돼 밑단 아래로 조각이 남았다. 치마·장갑·신발·베일은 v1이다.
- v2 상의 아랫단(1.255m)은 v1 치마 윗단(1.264m)보다 9mm 낮아 벨트 안으로 들어간다.

## 피팅 보정 — 2026-11-14

- 구두: 판금 부츠 밑창보다 1.55cm 높게 생성돼 구두를 통째로 내렸다(`lower_shoe`). 굽은 양쪽 0.85cm씩 늘렸다(`extend_heel.gap`, 사용자 결정: 끝까지 늘리면 굽이 앞창보다 먼저 닿는다). 굽 기둥은 윗부분 가중치를 그대로 받아 한 덩어리로 움직인다(`rigid_post`).
- 장갑: 사용자가 앞뒤 6.5°, 옆 1.5° 기울여 맞췄다. 분리 때 금색 띠 끝 평면에서 자른다.
- 피부 자르기: 장갑 입구(`clip_cuff`), 상의 아랫단과 스타킹 윗단(`clip_hem`)은 걸친 몸체 삼각형을 그 평면에서 잘라 쓴다(가림 정보의 `clip_planes`, `female-outfit-mask.ts`). 삼각형 단위 가림은 톱니와 구멍을 남겼다.
- 베일: 헤어별 조절값은 `client/src/lib/data/femaleHelmetFits.json`([기록](female-helmet-hair-brush.md)). 단발 기준 107%는 피팅에 구웠다.

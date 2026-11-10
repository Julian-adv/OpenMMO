# 여성 시크 얼굴 — 두 번째 여성 얼굴

2026-11-11. [순찰자 얼굴](modular-female-ranger-face.md)과 같은 절차로 만든 두 번째 여성 얼굴이다.
얼굴만 따로 생성한 민머리 두상을 여성 공통 리그에 붙인다. 게임에 쓰는 것은 2차 시도(`tripo_chic_v2`)다.

## 1차 시도 (`assets/modular_human_female_01/faces/tripo_chic_v1/`) — 미사용

사용자가 고른 인상(도도하고 시크한, 청회색 눈)으로 그린 뒤 "더 동안으로" 고친 원화로 생성했다.
리그에 붙여 보니 순찰자 얼굴과 얼굴형·나이대가 비슷해 눈색 말고는 구별이 어려웠다(사용자 판단, 2026-11-11).
원화 `face-closeup-v1.png`·`face-closeup-v2.png`·`face-bald-v1.png`(`face-sources.json`), Tripo seed 102033,
task `25e44ff5-1d9a-45fa-b8da-f0622e7a9c25`, 120크레딧(4240 → 4120). 피팅 결과와 검토 렌더는 폴더에 남겨 두었다.
배운 점: 3D와 작은 화면에서 남는 차이는 얼굴 길이·광대·턱선·눈 모양·눈썹 각도·입술 같은 골격과 화장이다.

## 원화 (`assets/modular_human_female_01/faces/tripo_chic_v2/`)

OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-11, OpenAI 생성 출력물 이용 조건 적용.
프롬프트·해시·참고 이미지는 `face-sources.json`.

- `face-closeup-v3.png`: 성숙하고 날카로운 인상(긴 얼굴, 높은 광대, 각진 턱, 가늘게 올라간 눈, 눈 밑 점). 미사용 — 덜 세게 해 달라는 요청.
- `face-closeup-v4.png`: v3의 미간을 펴고 볼을 채우고 화장을 옅게 한 것. 사용자 승인.
- `face-bald-v1.png`: v4에서 머리카락과 옷을 지운 Tripo 입력. 순찰자 민머리 원화를 피부 톤 기준으로 같이 넣었다.

## 3D

Tripo API 유료 크레딧, `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR, face_limit 10000, 정면 한 장, seed 102034.
task `8a0f1d71-088d-4dae-ad4f-88c10a5ecc1e`, 120크레딧(4120 → 4000). Tripo 이용 약관 적용. `build.json`·`generation.json`.
원본 `model_url.glb`는 9,935 triangles, 4096² 색·노멀·ORM.

## 피팅 (`tools/fit-female-ranger-face.py --face chic`)

도구가 얼굴별 설정(`FACES`)을 받도록 바꿨다. 순찰자 얼굴의 출력 파일은 바이트 단위로 그대로다.

- 회전·균일 스케일 0.293·이동 `[0, 1.6506, 0.008]`m. 턱과 정수리 높이는 순찰자 얼굴과 같다(턱이 예전 기본 얼굴보다 4mm 위).
  이 두상은 목이 얼굴 기준으로 더 뒤에 있어서, 목 경계(1.552m)의 반지름이 몸체 목과 가장 잘 맞는 앞뒤 위치를 골랐다.
  순찰자 얼굴보다 얼굴 앞면이 10mm 앞이다. 몸체 목의 최대 이동은 9.6mm다(순찰자 14.6mm).
- 이음매 가중치(`ease_seam_weights`): 1차 시도에서 찾은 문제다. 머리 목이 원래 몸체 표면에서 떨어져 있으면
  가장 가까운 몸체 표면에서 옮긴 가중치가 이음매 바로 위에서 어깨 쪽으로 튀어(1mm 사이 8%p), 대기 자세에서 목 옆·뒤가
  쐐기 모양으로 꺾였다. 이음매 위 2.5cm의 머리 가중치를 같은 각도의 경계 고리 가중치에서 서서히 이어 가게 했다(118 정점).
- 속눈썹(`lashes_on_lids`): 생성물은 위·아래 판(눈마다 238 triangles)이었다. 떼어 내고 1차 순찰자 머리의 가닥 속눈썹을 옮겼다.
  눈매가 가늘고 꼬리가 올라가 있어 안구 기준 일괄 이동으로는 가닥이 눈꺼풀 위로 뜬다.
  정면에서 본 눈꺼풀 가장자리를 두 머리에서 구해, 가닥마다 눈 너비의 같은 비율 위치로 옮긴다
  (가닥별 얼굴 평면 회전·균일 스케일 0.92/0.89·이동). 왼쪽 43·오른쪽 42가닥.
- 홍채: 다시 그리지 않았다. `tools/project-female-face-eyes.py --face chic`가 두 눈을 정면 무조명으로 편 이미지
  (`iris_cleanup_v1/front-*.png`, `visible-*.png`)를 만들고, 피팅 도구가 흰자와 맞닿은 홍채 양옆 가장자리에서 중심과
  반지름(5.2mm)을 잰다. 이 투영 도구는 순찰자 얼굴의 기록된 정면 이미지와 같은 틀을 낸다(안구 마스크 IoU 0.998).
  오른쪽 눈 동공이 조금 일그러져 있다. 눈이 가늘어 크게 드러나지 않아 두었다.
- 결과 `base_chic.glb`(몸체 15,729 + 머리 8,452 + 속눈썹 230 triangles), 기록 `fitting.json`,
  검토 렌더 `rig-review.png`(위 시크·아래 순찰자), `neck-review.png`(아랫줄은 대기 자세).

## 피부 톤

머리 목(1.555–1.60m)의 선형 평균은 0.769·0.339·0.193, roughness 0.61이다. 몸 목은 0.740·0.337·0.177, 0.634로
머리가 4% 밝다(1차 시도는 10%). 목 아래 5cm의 몸체 색·roughness를 머리 쪽으로 섞는 기존 단계가 경계를 가린다.
머리 텍스처는 고치지 않았다. 아이라인·눈두덩 음영·입술색·눈 밑 점은 텍스처에 구워져 있다.

## 게임 연결

사용자 결정(2026-11-11): 저장된 `face` 값 `custom`을 이 얼굴로, 나머지를 순찰자 얼굴로 표시한다(`femaleFace`).
서버는 여성 얼굴을 `default`·`custom` 둘만 저장하므로 서버·프로토콜·저장 데이터는 바꾸지 않았다.
기존 `custom` 여성 캐릭터는 순찰자 얼굴에서 이 얼굴로 바뀐다.

- 파츠 `face_chic.glb`(1024², 1.18MB)·`face_chic_512.glb`(512², 1.04MB): `node tools/prepare-female-custom-face.mjs --face chic [--size 512]`.
  [1024² 기록](modular-female-chic-face-runtime.json), [512² 기록](modular-female-chic-face-runtime-512.json).
- 얼굴별로 캐시·1024² 파츠 해제·헤어 보정(`femaleHairFits.json`의 `chic`)·눈색 원
  (`CHIC_FEMALE_EYE`: 중심 x 0.0295/0.0297m, y 1.6848m, 반지름 5.4mm)을 따로 쓴다.
- 헤어 보정: 순찰자 두피를 이 두상의 두피에 맞추는 변환(균일 96.6%, 앞으로 1.25cm, 위로 0.16cm; 평균 오차 0.9mm)을 계산해 넣고,
  사용자가 미리보기에서 조정했다(2026-11-11). 단발은 99.5%·가로 98.5%·앞으로 0.65cm·위로 0.05cm, 묶은 머리는 계산값에 가로 97%,
  긴 머리는 계산값 그대로다.
- 생성 화면의 여성 얼굴 선택을 다시 살렸다. 썸네일 `female-face-default.png`(순찰자)·`female-face-custom.png`(시크)는
  `tools/blender-scripts/render_female_hair_thumbnail.py`에 피팅된 얼굴(`--body`)과 단발 헤어 보정(`--hair-move`·`--hair-scale`)을 주어
  단발 헤어로 512² 렌더한 뒤 128²로 줄였다. 로컬 렌더이며 새 생성이나 유료 호출은 없다.
- 미리보기: `/modular-character-preview.html?character=female-face&face=chic`.

## 남은 일

- 눈꺼풀 텍스처에 그려진 속눈썹과, 오른쪽 눈꼬리의 작은 얼룩이 남아 있다.
- 브라우저 확인은 사용자가 한다. Blender 렌더와 수치로만 확인했다.

```bash
.venv/bin/python tools/outfits/tripo.py submit assets/modular_human_female_01/faces/tripo_chic_v2/build.json
cd tools && ../.venv/bin/python project-female-face-eyes.py --face chic && ../.venv/bin/python fit-female-ranger-face.py --face chic
node tools/prepare-female-custom-face.mjs --face chic && node tools/prepare-female-custom-face.mjs --face chic --size 512
```

# 여성 순찰자 얼굴 (실험) — 얼굴만 따로 생성해 붙이기

2026-11-10. 전신 원화로 만든 Tripo 모델은 얼굴이 작아 뭉개진다. 얼굴 클로즈업 원화 한 장으로 머리만 따로 생성해
몸에 붙이는 방식을 시험했다. 게임에는 아직 연결하지 않았고, 제작실 미리보기에서만 볼 수 있다.

## 1차: 헤어 포함 머리 접합 (`assets/items/female_ranger_outfit/tripo_head_v1/`)

- 원화 `face-closeup-v2.png`: OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-10.
  전신 원화 `concepts/front-v1.png`의 얼굴을 클로즈업으로 다시 그린 v1을 사용자 요청으로 더 예쁘고 동안으로 고친 것이다.
  OpenAI 생성 출력물 이용 조건 적용. 프롬프트·해시는 같은 폴더의 `face-closeup-sources.json`.
- 3D: Tripo API 유료 크레딧, `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR, face_limit 10000, seed 102031.
  task `ceab0685-b5b4-475e-b638-59360d0afdf8`, 120크레딧(4480 → 4360). Tripo 이용 약관 적용. `build.json`·`generation.json`.
- `graft_head.py`로 기존 전신 모델(`tripo_segmented_v1/original/model.glb`)의 머리·헤어를 떼고 새 머리를 붙였다.
  Z축 -90° 회전, 균일 스케일 0.1755, 이동만 적용했고, 깃 높이의 기울어진 평면 하나로 양쪽 목을 잘랐다.
  머리(헤어 포함) 6,777 triangles, 전신 19,945 triangles(기존 19,415). 뼈대는 없다.
- [기존 얼굴과 비교](../../assets/items/female_ranger_outfit/tripo_head_v1/comparison.png)에서 눈·코·입 형상과 텍스처가 뚜렷하게 나아졌다.
  새 피부는 roughness가 낮아 이마·콧등이 번들거리고, 목 이음매에 색 차이와 단차가 남는다.
- 머리 텍스처 크기 비교(`render_texture_sizes.py`): 게임 카메라(세로 20m 정사영, 1080p에서 머리 약 12픽셀)에서는
  256²까지 차이가 없고, 얼굴 700픽셀 클로즈업에서는 1024²까지 4096²과 거의 같다.
  사용자 결정(2026-11-10): 캐릭터 생성·선택 화면 **1024²**, 게임 화면 **512²**.
- 미리보기: `/modular-character-preview.html?asset=ranger-head`.

## 2차: 민머리 두상을 여성 공통 리그에 연결 (`assets/modular_human_female_01/faces/tripo_ranger_v1/`)

게임은 얼굴 파츠에 헤어를 따로 씌우므로 같은 얼굴을 민머리로 다시 생성했다.

- 원화 `face-bald-v1.png`: 위 `face-closeup-v2.png`에서 머리카락과 옷을 지운 것. 같은 도구·등급, 2026-11-10. `face-bald-sources.json`.
- 3D: 같은 Tripo 설정, seed 102032. task `2b9a5b12-1d50-466e-9bbd-9287291d2137`, 120크레딧(4360 → 4240).
  원본 `model_url.glb`는 9,955 triangles, 4096² 색·노멀·ORM.
- 피팅 `tools/fit-female-ranger-face.py`: 기존 [여성 새 얼굴](modular-female-custom-face.md) 도구의 절차를 재사용한다.
  머리는 회전·균일 스케일 0.293·이동 `[-0.002, 1.6386, -0.008]`m만 적용했다. 턱 높이를 기존 새 얼굴(약 1.57m)에 맞춰
  정수리는 공통 두상보다 15mm 낮다. 높이 1.552m의 한 경계에서 몸체와 직접 연결한다.
  몸체와 머리의 경계 점은 목 축 둘레의 각도로 짝짓는다. 기존 도구처럼 둘레 길이로 짝지으면 경계 정점이 옆으로 밀리고,
  몸체 정점이 가장 가까운 경계 선분의 변위를 받아 목에 주름이 생겼다. 각도로 짝지으면 몸체 목은 반지름 방향으로만
  움직이며 최대 6.7mm다(경계 아래 5cm에 걸쳐 줄어든다).
  사용자 요청(2026-11-10)으로 이음매 위 2cm·아래 4cm의 목 반지름을 높이 방향으로 고르게 해 윤곽의 턱을 없앴다
  (σ 8mm, 몸체 최대 1.9mm·머리 최대 0.8mm, 반지름 방향만). 이 구간 밖의 머리는 회전·균일 스케일·이동 그대로다.
  목 아래 5cm의 몸체 색과 roughness를 머리 쪽으로 섞었다(`body-neck-albedo.png`·`body-neck-orm.png`).
  몸체 노멀맵은 건드리지 않는다. 이음매 법선은 머리의 원래 법선을 그대로 쓴다: 면에서 다시 계산하면 경계의 곧은 변 위에
  추가된 정점들이 그 변의 평평한 법선을 받아 이음매에 톱니 무늬가 생겼다. 검토 렌더 `neck-review.png`.
  가중치는 몸체 표면에서 옮기고 1.59–1.61m 위는 `Head`에 고정했다. 65본·bind 행렬·9개 동작은 그대로다.
- 속눈썹: 민머리 생성물은 가닥 위에 큰 판이 얹혀 나왔다(파라미터는 1차와 같고 원화와 시드만 다르다).
  자기 속눈썹 조각 581 triangles를 떼고 1차 머리의 가닥 속눈썹 230 triangles를 옮겼다.
  눈마다 안구 위치·크기를 기준으로 균일 스케일 1.084와 이동만 적용했다. 1차 머리의 색상 텍스처를 쓰는 별도 재질이며 `Head`에 고정한다.
- 결과 `base_ranger.glb`(몸체 15,730 + 머리 8,598 + 속눈썹 230 triangles, 작업본 32MB), 기록 `fitting.json`.
  검토 렌더 `rig-review.png`(기본·달리기·베기 자세, 기존 새 얼굴과 비교), `eyelash-review.png`.
- 미리보기: `/modular-character-preview.html?character=female-face&face=ranger`. 이 얼굴의 헤어 보정은
  `client/src/lib/data/femaleHairFits.json`의 `ranger`에 따로 저장하며 초기값은 세 헤어 모두 아래로 1.5cm다.

## 게임 연결 (2026-11-10)

사용자 결정: 기존 여성 얼굴 두 종류(기존 얼굴·새 얼굴)를 이 얼굴로 바꾼다. 저장된 `face` 값이 `default`든 `custom`이든
여성 캐릭터는 모두 이 얼굴로 표시한다. 서버·프로토콜·저장 데이터는 바꾸지 않았다.

- 게임용 파츠 `client/public/models/characters/modular_female/face_ranger.glb`(1.22MB):
  `node tools/prepare-female-custom-face.mjs --face ranger`. 머리 8,599·속눈썹 230·목 띠 662 triangles,
  머리 색·노멀·ORM과 목 띠 색·ORM은 1024² WebP다. 공통 `base.glb`에 붙이고 몸체의 머리·목과 기본 속눈썹은 숨긴다.
  [게임용 파일 기록](modular-female-ranger-face-runtime.json).
- 속눈썹은 1차 머리 텍스처에서 어두운 절반의 평균색 하나로 칠한다(텍스처 없음).
- 눈색: 홍채 마스크가 아직 없어 얼굴·속눈썹 재질을 착색 대상에서 뺐다. 생성 화면에서 여성은 얼굴 선택과 눈색 항목을 숨긴다.
  저장된 눈색 값은 그대로 두며 지금은 표시에 쓰이지 않는다.
- 헤어 보정은 `femaleHairFits.json`의 `ranger` 값을 쓴다(세 헤어 모두 아래로 1.5cm에서 시작).
- **[미사용]** `face_custom.glb`와 여성 얼굴 썸네일 `female-face-custom.png`는 게임에서 더 이상 불러오지 않는다. 파일은 남겨 두었다.
- 2단 텍스처(생성·선택 화면 1024², 게임 512²)는 아직 없다. 지금은 어디서나 1024²다.

## 남은 일

- 안구 텍스처의 검은 반사 얼룩, 눈꺼풀에 그려진 속눈썹 그림자 띠, 눈색 착색 마스크.
- 몸(번들거림)과 머리(무광)의 광택 차이, 후면 목의 옅은 색 경계.
- 게임 화면용 512² 텍스처 단계, 여성 얼굴·헤어 썸네일 다시 렌더.
- `face_ranger.glb`를 Hugging Face에 올리고 `assets.lock`에 넣어야 배포할 수 있다.
- 브라우저에서는 정면(헤어 없음·묶은 머리)만 확인했다. 나머지 시점·동작은 Blender 렌더로 확인했다.

```bash
.venv/bin/python tools/outfits/tripo.py submit assets/modular_human_female_01/faces/tripo_ranger_v1/build.json
cd tools && ../.venv/bin/python fit-female-ranger-face.py
```

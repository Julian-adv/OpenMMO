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
  머리는 회전·균일 스케일 0.293·이동 `[-0.002, 1.6506, -0.008]`m만 적용했다. 턱은 예전 기본 얼굴(약 1.578m)보다 4mm 높고
  정수리는 공통 두상보다 3mm 낮다. 처음엔 예전 새 얼굴(약 1.57m)에 맞췄으나 옷깃이 올라오는 의상에서 목이 짧아 보여
  사용자 확인을 거쳐 8mm, 다시 4mm 올렸다(2026-11-10). 기존 의상의 옷깃은 기본 얼굴 턱 높이에 맞춰져 있다. 높이 1.552m의 한 경계에서 몸체와 직접 연결한다.
  몸체와 머리의 경계 점은 목 축 둘레의 각도로 짝짓는다. 기존 도구처럼 둘레 길이로 짝지으면 경계 정점이 옆으로 밀리고,
  몸체 정점이 가장 가까운 경계 선분의 변위를 받아 목에 주름이 생겼다. 각도로 짝지으면 몸체 목은 반지름 방향으로만
  움직이며 최대 14.6mm다(경계 아래 5cm에 걸쳐 줄어든다).
  사용자 요청(2026-11-10)으로 이음매 위 2cm·아래 4cm의 목 반지름을 높이 방향으로 고르게 해 윤곽의 턱을 없앴다
  (σ 8mm, 최대 1.9mm, 반지름 방향만). 이 구간 밖의 머리는 회전·균일 스케일·이동 그대로다.
  목 아래 5cm의 몸체 색과 roughness를 머리 쪽으로 섞었다(`body-neck-albedo.png`·`body-neck-orm.png`).
  몸체 노멀맵은 건드리지 않는다. 고른 구간의 법선은 고른 반지름 값의 기울기에서 직접 계산해 구간 가중치만큼 섞는다.
  면에서 다시 계산하면 경계의 곧은 변 위에 추가된 정점들이 그 변의 평평한 법선을 받아 톱니 무늬가 생겼고,
  원래 법선을 그대로 두면 이음매 바로 위아래가 실제 면보다 6–9° 위를 향해 옅은 띠로 보였다(지금은 2° 이내).
  검토 렌더 `neck-review.png`.
  가중치는 몸체 표면에서 옮기고 1.59–1.61m 위는 `Head`에 고정했다. 65본·bind 행렬·9개 동작은 그대로다.
- 속눈썹: 민머리 생성물은 가닥 위에 큰 판이 얹혀 나왔다(파라미터는 1차와 같고 원화와 시드만 다르다).
  자기 속눈썹 조각 581 triangles를 떼고 1차 머리의 가닥 속눈썹 230 triangles를 옮겼다.
  눈마다 안구 위치·크기를 기준으로 균일 스케일 1.084와 이동만 적용했다. 1차 머리의 색상 텍스처를 쓰는 별도 재질이며 `Head`에 고정한다.
- 결과 `base_ranger.glb`(몸체 15,736 + 머리 8,654 + 속눈썹 230 triangles), 기록 `fitting.json`.
  검토 렌더 `rig-review.png`(기본·달리기·베기 자세, 기존 새 얼굴과 비교), `eyelash-review.png`.
- 미리보기: `/modular-character-preview.html?character=female-face&face=ranger`. 이 얼굴의 헤어 보정은
  `client/src/lib/data/femaleHairFits.json`의 `ranger`에 따로 저장하며 초기값은 세 헤어 모두 아래로 0.3cm다.

## 게임 연결 (2026-11-10)

사용자 결정: 기존 여성 얼굴 두 종류(기존 얼굴·새 얼굴)를 이 얼굴로 바꾼다. 저장된 `face` 값이 `default`든 `custom`이든
여성 캐릭터는 모두 이 얼굴로 표시한다. 서버·프로토콜·저장 데이터는 바꾸지 않았다.

- 게임용 파츠 `client/public/models/characters/modular_female/face_ranger.glb`(1.22MB):
  `node tools/prepare-female-custom-face.mjs --face ranger`. 머리 8,654·속눈썹 230·목 띠 667 triangles,
  머리 색·노멀·ORM과 목 띠 색·ORM은 1024² WebP다. 공통 `base.glb`에 붙이고 몸체의 머리·목과 기본 속눈썹은 숨긴다.
  [게임용 파일 기록](modular-female-ranger-face-runtime.json).
- 속눈썹은 1차 머리 텍스처에서 어두운 절반의 평균색 하나로 칠한다(텍스처 없음).
- 홍채 보정(2026-11-10): 생성된 안구 텍스처는 동공이 일그러지고 흰 반사 무늬가 구워져 있었다. 두 눈을 정면에서 조명 없이 펴서
  (`iris_cleanup_v1/front-*.png`) OpenAI Codex built-in ImageGen(**ChatGPT Pro 20x**, 2026-11-10)으로 홍채와 동공만 다시 그렸다.
  둥근 동공, 반사 없음, 기존 황갈색 유지. 프롬프트·해시는 `iris_cleanup_v1/sources.json`.
  피팅 도구가 그 홍채를 (반지름, 각도) 이미지로 편 뒤 안구 메시 앞쪽 텍셀에만 칠한다(왼쪽 14,117·오른쪽 15,866 texels).
  눈꺼풀에 가려 보이지 않던 각도는 같은 반지름의 가장 가까운 보이는 각도로 채운다. 안구 밖 텍셀은 그대로다(`face-eye-albedo.png`).
- 눈색: 안구는 본체와 떨어진 메시라서 머리 메시의 UV1.x에 안구 정점만 1로 표시했다. 착색 셰이더는 홍채 원(중심 x 0.0319/0.0310m,
  y 1.6842m, 반지름 6.8mm) 안에서 이 표시가 있는 면만 칠하므로 그 앞의 눈꺼풀(원 안의 정점 126개)에는 번지지 않는다.
  기본 눈색을 고르면 보정한 원래 홍채를 그대로 쓴다. 생성 화면의 여성 눈색 항목을 다시 살렸고, 얼굴 선택만 숨긴다.
  속눈썹 재질만 착색 대상에서 뺀다.
- 헤어 보정은 `femaleHairFits.json`의 `ranger` 값을 쓴다(세 헤어 모두 아래로 0.3cm에서 시작).
- **[미사용]** `face_custom.glb`와 여성 얼굴 썸네일 `female-face-custom.png`는 게임에서 더 이상 불러오지 않는다. 파일은 남겨 두었다.
- 2단 텍스처: `face_ranger.glb`(1024², 1.22MB)는 생성·선택 화면과 감정표현 미리보기처럼 캐릭터를 크게 보여 주는 화면에서,
  `face_ranger_512.glb`(512², 1.06MB)는 게임 화면의 플레이어(`PlayerModel`)에서 불러온다. 형상은 같고 머리·목 띠 텍스처 크기만 다르다.
  `loadCharacterModel`의 세 번째 인자 `'closeup' | 'game'`으로 고르며 단계별로 따로 캐시한다.
  선택·생성 화면과 게임은 렌더러 하나를 같이 쓰므로, 게임 화면으로 넘어간 다음 프레임에 `releaseFemaleCloseup`이
  1024² 파츠의 텍스처·지오메트리를 GPU에서 내리고 캐시에서 뺀다. 다음에 크게 보여 줄 때 다시 불러온다.
  `node tools/prepare-female-custom-face.mjs --face ranger --size 512`. [512² 파일 기록](modular-female-ranger-face-runtime-512.json).

## 몸 피부 톤 맞춤 (2026-11-10)

목 이음매의 가로선은 색과 광택 차이였다. 몸의 목(1.50–1.545m)은 선형 평균 0.523·0.307·0.203에 roughness 0.383,
머리의 목(1.555–1.60m)은 0.739·0.332·0.177에 0.636이다. 사용자 요청으로 몸 전체의 피부를 얼굴 톤에 맞췄다.

- `tools/match-female-body-skin.py`: 몸 색상맵의 피부 텍셀에 선형 RGB 배율 1.414·1.082·0.872를 곱하고(밝은 곳은 부드럽게 눌러 포화를 막는다),
  ORM의 roughness에 0.25를 더한다. 채도 0.10–0.22를 경계로 피부만 고르며 흰 속옷과 쓰지 않는 텍셀(전체의 18%)은 그대로다.
  결과는 `assets/modular_human_female_01/rig_v1/skin_tone_v1/`의 `body-albedo.png`·`body-orm.png`(4096²), 기록 `record.json`, 검토 렌더 `review.png`.
  원본 `female-base.glb`는 바꾸지 않았다. 로컬 가공이며 새 생성이나 유료 호출은 없다.
- 게임 몸체: `node tools/prepare-female-knight.mjs --base-textures`가 이미 빌드된 `base.glb`에서 색(2048² WebP q94)과 ORM(1024² WebP q90)만
  바꿔 넣는다. 전체 빌드와 같은 크기·인코딩이며, 전체 빌드도 이제 같은 파일을 읽는다. 바꾼 뒤 지오메트리·노드·bind 행렬·
  16개 동작의 528개 채널이 이전 파일과 값이 같은지 확인했다.
- 얼굴 피팅과 목 띠도 같은 파일에서 몸 색·ORM을 읽는다.
- 도적 속옷을 피부색으로 덮는 셰이더의 피부색 상수를 194·151·125에서 226·157·117로 바꿨다(`femaleUnderwearColor.ts`).

## 남은 일

- 눈꺼풀에 그려진 속눈썹 그림자 띠.
- 이음매 자리의 가는 선(형상·노멀맵 경계)이 아주 옅게 남는다.
- 의상 텍스처에 피부가 그려져 들어간 곳이 있으면 예전 톤으로 남는다. 아직 확인하지 않았다.
- 여성 헤어 썸네일 다시 렌더.
- 브라우저에서는 정면(헤어 없음·묶은 머리)만 확인했다. 나머지 시점·동작은 Blender 렌더로 확인했다.

```bash
.venv/bin/python tools/outfits/tripo.py submit assets/modular_human_female_01/faces/tripo_ranger_v1/build.json
cd tools && ../.venv/bin/python fit-female-ranger-face.py
```

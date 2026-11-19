# 여성 상체(머리·목·어깨) 생성물 — 세 번째 얼굴 시안

2026-11-19. 몸체의 승모근이 커 보인다는 지적에 따라, 어깨까지의 상체를 따로 생성해 몸체에 붙이려던 시도다(사용자 결정). 상체로는 붙이지 않았고, 목 위를 세 번째 여성 얼굴 시안으로 피팅했다(아래 "얼굴 시안"). 게임에는 아직 넣지 않았다.

- 참고 이미지: 사용자 제공 이미지(SHA-256 `3a488a95…1e6f`). 출처와 이용 조건을 확인하지 못해 공개 저장소에 올리지 않았다. 원화의 인상 참고로만 썼다. 게임에 넣기 전에 사용자 확인이 필요하다.
- 원화: OpenAI Codex built-in ImageGen(`codex exec`), **ChatGPT Pro 20x**, OpenAI 생성 출력물 이용 조건. `bust-front-v1.png` `165fa387…2a2d`, 프롬프트 `bust-front-v1-prompt.txt`. 피부 톤 기준으로 시크 얼굴의 민머리 원화를 같이 넣었다. 사용자 승인.
- Tripo API 유료 크레딧, `P2-20260801`, detailed PBR, face_limit 10000, 정면 한 장, seed 111903. 작업 ID `ca0a829f-20fd-46e5-be40-018a91208d28`, 120크레딧(1480 → 1360). 출력 `model_url.glb` `01952ce8…7348`, 9,575 triangles. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- 파일은 `assets/modular_human_female_01/faces/tripo_bust_v1/`.

## 붙이지 않은 이유

어깨와 가슴을 기준으로 몸체에 정렬하면(배율 0.398, 기준 오차 7mm) 어깨선 높이·어깨 폭·턱 높이가 몸체와 같고, 목 밑동은 생성물이 오히려 넓다.

| 높이 | 몸체 반폭 | 생성물 반폭 |
| --- | ---: | ---: |
| 1.50m | 11.8cm | 11.7cm |
| 1.53m | 8.5cm | 10.9cm |
| 1.54m | 6.8cm | 8.4cm |
| 1.56m | 4.7cm | 5.9cm |
| 1.60m | 4.4cm | 5.1cm |

생성물이 날씬해 보인 것은 머리가 12% 커서다(반폭 9.6cm 대 8.6cm). 문제는 몸체가 아니라 [메이드 상의](modular-female-maid-tripo.md)의 깃이었고, 상의를 다시 생성해 풀었다.

몸체 정점을 수식으로 내려 승모근을 줄인 시안도 만들었으나 표면이 울퉁불퉁해져 버렸다(사용자 판단).

## 얼굴 시안 — 2026-11-19

사용자 요청으로 목 위를 [시크 얼굴](modular-female-chic-face.md)과 같은 절차로 붙였다. 임시 이름 `elegant`.
`tools/fit-female-ranger-face.py --face elegant`, 결과 `base_elegant.glb`, 기록 `fitting.json`.

- 회전·균일 스케일 0.339·이동 `[0, 1.6271, -0.008]`m. 턱과 정수리 높이를 순찰자·시크 얼굴에 맞춘 배율이다. 위 표의 0.398은 어깨에 맞춘 배율이라 머리가 컸다.
  앞뒤 위치는 목 경계(1.552m)에서 몸체 목의 이동이 가장 작은 곳이다. 최대 4.7mm(시크 9.6mm, 순찰자 14.6mm).
- 머리 7,163 + 속눈썹 230 triangles, 몸체 15,747. 이음매 가중치 181 정점(`ease_seam_weights`).
- 눈알이 180 triangles씩이라 눈알과 속눈썹을 가르는 기준 200에 걸렸다. 얼굴별 기준(`eye_piece_limit` 150)을 두었다.
- 금속성: 생성물의 피부 metalness가 0.52 안팎이라 옻칠한 듯 번쩍였다(사용자 지적). 다른 얼굴은 0.04, 몸체는 0.02다.
  머리 ORM의 metalness를 0.04 이하로 누른다(`metallic`). 몸체 목에 섞는 재질도 같은 값을 쓴다. 거칠기는 그대로다(얼굴 평균 0.48, 순찰자 0.48).
- 속눈썹: 생성물의 판(눈마다 190 triangles)을 떼고 1차 순찰자 머리의 가닥을 눈꺼풀 가장자리로 옮겼다(`lashes_on_lids`).
- 홍채: 다시 그리지 않았다. 중심 x ∓0.0335/0.0331m, y 1.6865m, 반지름 7.2/7.1mm.
- 헤어 보정(`femaleHairFits.json`의 `elegant`): 순찰자 두피를 이 두피에 맞춘 변환(균일 98.9%, 위로 0.22cm, 뒤로 0.08cm, 평균 오차 1.8mm)을 넣고
  사용자가 미리보기에서 조정했다. 단발과 긴 머리는 계산값보다 0.9–1.6cm 위, 배율 97%가 되었다. 묶은 머리와 긴 웨이브는 계산값 그대로다.
- 미리보기: `/modular-character-preview.html?character=female-face&face=elegant`. 피팅 원본을 바로 읽는다.
- 피팅 도구가 다시 만드는 `face-eye-albedo.png`·`face-orm.png`·`body-neck-*.png`·`iris_cleanup_v1/input-*.png`와 검토 렌더는 사용자 요청으로 지웠다.
- 도구를 고친 뒤에도 시크 얼굴의 `base_chic.glb`·`fitting.json`은 바이트 단위로 그대로다.

### 남은 일

- 동공이 일그러져 있다. 옮긴 속눈썹 가닥이 위로 곧게 서고 사선에서 눈꼬리 밖으로 나온다. 눈썹이 옅다. 귀 뒤 텍스처에 얼룩이 있다. 이마와 콧등의 하이라이트가 색 텍스처에 구워져 있다.
- 게임 연결(이름, 저장할 얼굴 값, 파츠 `face_*.glb`, 눈색 원, 썸네일)은 하지 않았다.
- Blender 렌더와 수치로 확인했고, 브라우저는 사용자가 미리보기에서 확인했다.

```bash
cd tools && ../.venv/bin/python project-female-face-eyes.py --face elegant && ../.venv/bin/python fit-female-ranger-face.py --face elegant
```

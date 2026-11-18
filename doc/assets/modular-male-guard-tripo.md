# 경비대 복장 (Karl)

경비대장 Karl을 통짜 모델 `guard.glb`에서 남성 공통 몸체로 옮기며 만든 복장이다. 몸체는 [남성 몸체 안내](modular-human-male-01.md), 마네킹 방식은 [남성 합병식 턱시도](modular-male-ceremony-tuxedo-tripo.md), 같은 전환의 첫 사례는 [잡화점 주인 의상](modular-female-shopkeeper-tripo.md)에 있다.

## 구성 (사용자 결정 2026-11-18)

- 기존 Karl 원화·초상화의 장비를 그대로 옮긴다. 검과 단검은 뺐다.
- 투구(철모와 목 밑까지 오는 사슬 두건), 상의(사슬 어깨 덮개·긴소매 사슬 갑옷·허리띠), 바지(모직 바지·무릎 보호대), 장갑, 장화(리넨 감개 포함), 타바드를 따로 만든다.
- 투구와 타바드는 몸과 따로 그려 따로 생성한다. 한 장에 그리면 떼어 내기 어렵다.
- 타바드는 망토 칸(back) 아이템이다. 망토처럼 염색과 문장 그림을 받는다. 원화의 화살 문장은 그리지 않는다.
- 판매가와 상의 방어는 Chain Mail에 맞춘다(한 벌 25,000, 상의 guard 5). 얼굴은 기본 얼굴.

## 원화 — 2026-11-18

OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**. OpenAI 생성 출력물 이용 조건을 따른다. 파일과 프롬프트(`*-prompt.txt`)는 `assets/items/male_guard_outfit/concepts/`.

마네킹은 `render_concept_base.py --character male --mannequin`(Blender 5.2 Cycles, 생성 아님)이다. 이번에 `--centre`, `--span`(흉상 렌더)과 `--wear`(피팅된 부위를 입힌 채 렌더)를 추가했다. 옵션 없이 렌더한 정면은 이전 파일과 화소 단위로 같다.

| 파일 | SHA-256 | 내용 |
| --- | --- | --- |
| `front-v1.png`, `back-v1.png` | `f012d7ef…4a4a`, `7177f85a…62d8` | **[미사용]** 투구·타바드까지 한 장에 그린 첫 시안. v2의 밑그림. 그림은 지우고 프롬프트만 남겼다 |
| `body-front-v2.png`, `body-back-v2.png` | `ae5b4230…7453`, `7f7516f0…3250` | 투구·타바드를 뺀 몸. 머리와 목이 파랗다 |
| `helmet-front-v1.png`, `helmet-side-v1.png`, `helmet-back-v1.png` | `92240289…5562`, `3be66581…8247`, `723e3ea0…c7ca` | 민머리 흉상(`bust-*.png`, 높이 1.1–2.1m) 위의 투구. 옆면에만 리벳 띠가 한 줄 더 있다 |
| `mailed-front.png`, `mailed-back.png` | `7f961319…a381`, `013f5b64…9e31` | 피팅된 사슬 갑옷을 입힌 파란 마네킹 |
| `tabard-front-v2.png`, `tabard-back-v2.png` | `b3107e39…7fdb`, `cbb093fa…00d7` | 위 마네킹 위에 그린 무지 타바드와 허리띠 |

철과 사슬은 따뜻한 회색으로 그렸다. 마네킹을 파란색으로 잘라내므로 푸른 기가 도는 쇠는 함께 잘린다.

## Tripo 생성 — 2026-11-18

Tripo API, 유료 API 크레딧. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms). P2-20260801, detailed 텍스처 `v3.5-20260815`. 설정과 응답은 각 폴더의 `build.json`, `generation.json`. 세 번 모두 한 번에 쓸 수 있는 결과가 나왔다(2,800 → 2,440 크레딧).

| 폴더 | 입력 | 작업 ID | 출력 SHA-256 | Triangles |
| --- | --- | --- | --- | ---: |
| `tripo_body_v1` | front, back (seed 111801, face_limit 20000) | `dac4e61d-153a-44aa-bd6e-f30dc1369dd3` | `a1c2ca3d…4b2c` | 19,552 |
| `tripo_helmet_v1` | front, left, back (seed 111802, face_limit 10000) | `89c5bcb1-b740-44b2-bf2c-d7307aaf644b` | `21e90f84…cf93` | 9,579 |
| `tripo_tabard_v1` | front, back (seed 111803, face_limit 20000) | `034138ef-4784-4d2c-97e0-4dfc9a281347` | `6b55ca8d…d728` | 20,251 |

## 마네킹 분리

- `tools/split-male-guard.py`: 머리와 목의 파란 면만으로 세운다(평균 5.3mm). 장갑은 몸의 손에서 3cm 떨어져 있어 함께 쓰면 평균 17mm로 나빠지고 밑창이 4.7cm 떴다. 장화·감개·바지가 한 덩어리로 나와 감개 윗단(0.505m)에서 자른다.
- `tools/split-male-guard-helmet.py`: 흉상 전체로 크기를 정하고(0.868), 얼굴과 목 가운데 폭 18cm로 위치를 맞춘다(평균 6.2mm). 얼굴만으로 크기까지 풀면 0.53으로 무너졌다. 이를 위해 `mannequin.py`의 `align`에 `rescale`을 추가했다.
- `tools/split-male-guard-tabard.py`: 몸체와 피팅된 사슬 갑옷을 함께 기준으로 삼는다(평균 4.6mm).

## 피팅 v1

`tools/fit-male-guard.py`가 `assets/modular_human_male_01/guard/tripo_v1/{helmet,top,pants,gloves,boots,tabard}_guard.glb`와 [피팅 기록](modular-male-guard-fitting-v1.json)을 만든다. 부위를 통째로만 옮겼다. 형태를 바꾼 것은 타바드의 둘레뿐이다.

| 부위 | Triangles |
| --- | ---: |
| 투구 | 2,754 |
| 상의 | 4,177 |
| 바지 | 3,085 |
| 장갑 | 2,302 |
| 장화 | 6,204 |
| 타바드 | 5,675 |

합계 24,197 triangles, 몸체는 따로다. 조립 캐릭터 목표(15,000–20,000)를 넘는다. 보이는 삼각형 수와 얼굴 배분은 재지 않았다. 줄이지 않았다.

- 상의: 목이 놓은 자리에 그대로 둔다. 몸통에 얹으려고 맞추면 가슴에서 8cm 뒤로 밀렸다. 소매는 턱시도처럼 어깨와 팔꿈치에서 돌린다. 사슬 치마는 허벅지를 따른다.
- 바지·장화: 생성된 다리가 바닥에서 1.5cm 떠서 골반을 중심으로 1.0146배 키우고, 각각 다리와 발에 맞춰 수평으로 옮긴다.
- 장갑: 손에 맞춰 옮기고 돌린다(13–14°, 생성된 팔이 16cm 앞에 있었다).
- 타바드: 사슬 갑옷의 가장 가까운 정점에서 가중치를 그대로 가져온다. 따로 구하면 자세마다 사슬이 천을 뚫었다. 기본 자세에서도 가슴과 허리띠 주변은 천이 사슬 안쪽에 있어, 타바드를 입으면 천에 가려지는 사슬 면을 그리지 않는다(`modularClothing.ts`의 `guard_tabard`).
- 타바드 둘레: 다른 상의 위에도 입도록 높이는 그대로 두고 좌우·앞뒤로 1.16배 키웠다(사용자 요청 2026-11-18). 1.10배에서는 턱시도 깃과 허리띠 밑 옷자락이 뚫고 나왔다. 미리보기에서 사슬 갑옷, 판금, 리넨, 순찰자, 바드, 사제, 야만용사 상의 위를 확인했고 턱시도 가슴에 작은 점이 남는다.
- 타바드의 둘째 UV: 가슴 앞판에 22×35.2cm 그림 자리(윗변 1.49m, 아랫변은 허리띠 밑), 나머지 천은 u −10(염색만), 허리띠는 u −20(그대로).

미리보기 `/modular-character-preview.html?outfit=guard&tabard`. 이 페이지는 WebGL이라 타바드의 염색과 그림은 보이지 않는다.

## 게임 연결

- 아이템 `guard_helmet`(guard 2), `guard_top`(5), `guard_pants`(1), `guard_gloves`(1), `guard_boots`(1), `guard_tabard`(망토 칸, `capeColor #c7a682`): 가격·판매처 없음, 거래 가능(사용자 결정). 아이콘과 바닥 모델은 `export_outfit_items.py --outfit guard --texture-size 512`.
- 타바드는 `capeColor`가 있어 서버에서는 망토다. 염색약, 그림 전사, 가문 문장, NPC 문장이 그대로 적용된다. 클라이언트는 바닥 모델 이름으로 타바드를 알아보고(`utils/tabard.ts`) 망토 천 대신 피팅된 부위를 보인다.
- `data-src/npcs.csv`의 Karl: 창과 위 여섯 아이템 착용, 문장 `aldermark`, 성별 male, 한 벌 25,000에 판매·선물.
- `modelPaths.ts`에서 Karl의 `guard.glb` 지정을 지웠다. `guard.glb`는 **[미사용]**이다.
- 개발 서버에서 Karl이 이 복장으로 초소 앞에 서서 대답하는 것, 플레이어가 사슬 갑옷과 타바드를 입은 모습을 확인했다(2026-11-18). Karl의 타바드는 올더마크 색으로 물들어 보였다. 플레이어 타바드에 같은 색과 문장 그림을 미리보기 값으로 넣어 가슴에 문장이 바로 서서 나오는 것도 확인했다.

## 남은 일

- Karl 자신의 가슴 문장은 보지 못했다. 초소에서 카메라를 등지고 선다.
- 여성판은 [여성 경비대 복장](modular-female-guard-tripo.md)에 있다. 다른 상의 위의 모습은 미리보기의 대기·달리기 자세로만 확인했다.
- 바지는 무릎 밑에서 끝나 다른 신발과 신으면 종아리가 드러난다. 장화와 다른 바지, 투구와 다른 얼굴의 조합은 확인하지 않았다.
- 녹색으로 물든 Karl의 타바드 등판에 옅은 얼룩이 보였다. 원인은 확인하지 않았다.
- 수면·앉기 동작, 옷 판매·선물, 캐릭터 선택 화면은 확인하지 않았다. 판매·선물은 prod의 agent-client 설정과 배포가 따라야 한다.
- 개발 서버의 jake1에게 `guard_top`, `guard_tabard`를 지급해 시험했다. 타바드는 입은 채이고 상의는 시험 전과 다른 것을 입고 있다.
- `assets.lock`과 HF는 갱신하지 않았다.

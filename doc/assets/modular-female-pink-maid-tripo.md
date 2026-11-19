# 분홍 메이드복 (Cocoly)

Cocoly를 통짜 모델 `pink_maid.glb`에서 여성 공통 몸체로 옮기며 만든 의상이다. 플레이어도 입는다. 절차와 피팅 방식은 [검은 메이드복](modular-female-maid-tripo.md)을 따른다. 2026-11-19.

## 구성 (사용자 승인 2026-11-19)

- 머리: 검은 메이드복의 흰 머리띠 `maid_helmet`을 그대로 쓴다.
- 상의: 흰 퍼프소매 블라우스, 펼친 깃과 분홍 리본, 주름을 두른 열린 가슴선, 어깨 주름이 달린 분홍 코르셋 조끼.
- 하의: 흰 허리띠, 짧은 분홍 치마, 흰 레이스 속치마, 주름 앞치마, 뒤의 큰 흰 리본.
- 장갑: 검은 메이드 장갑을 흰색으로 바꾼 것.
- 신발: 흰 허벅지 스타킹과 검은 에나멜 스틸레토, 붉은 밑창. 윗단 레이스는 없다.
- 가터 끈: 흰 끈 네 줄, 금색 걸쇠.

## 원화

OpenAI Codex built-in ImageGen(`codex exec`), **ChatGPT Pro 20x**, 2026-11-19, OpenAI 생성 출력물 이용 조건. 파일과 프롬프트(`*-prompt.txt`)는 `assets/items/female_pink_maid_outfit/concepts/`. 디자인은 Cocoly의 기존 원화 `doc/images/characters/pink-maid-concept.png`(`reference.png`)에서 옷만 가져왔다.

| 파일 | SHA-256 | 바탕 |
| --- | --- | --- |
| `top-front-v1.png`, `top-back-v1.png` | `33f5bf9e…`, `01851bcd…` | 엉덩이 위 마네킹 `bust-*.png` |
| `skirt-front-v1.png`, `skirt-back-v1.png` | `16d8a721…`, `0d8dc8fd…` | 전신 마네킹 `mannequin-*.png` |
| `boots-front-v1.png`, `boots-side-v1.png`, `boots-back-v1.png` | `89f84946…`, `e3f1122c…`, `be44c1e2…` | 전신 마네킹, 구두 모양은 `maid-front-v2.png` 참고 |

`sheet-v1.png`, `boots-sheet-v1.png`은 사용자에게 보인 모아 보기다.

## Tripo 생성

Tripo API 유료 크레딧, 건당 120크레딧(1240 → 880). 설정은 각 폴더의 `build.json`(P2-20260801, face_limit 20000, detailed 텍스처). [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).

| 폴더 | 시점 | 작업 ID | seed | `model_url.glb` | Triangles |
| --- | --- | --- | ---: | --- | ---: |
| `tripo_top_v1` | front, back | `c9f4e8d9-28b1-4391-b83b-386d8c56a9cf` | 111905 | `ca736baf…` | 18,183 |
| `tripo_skirt_v1` | front, back | `26c1ce47-c319-41af-b93e-a2bad9f5fb51` | 111906 | `79a11356…` | 19,825 |
| `tripo_boots_v1` | front, left, back | `c2672bc3-0adc-44d2-9d49-286cfd0e9d31` | 111907 | `adeab71f…` | 19,709 |

## 분리

```bash
cd tools
../.venv/bin/python split-female-maid-top.py assets/items/female_pink_maid_outfit/tripo_top_v1
../.venv/bin/python split-female-guard-skirt.py assets/items/female_pink_maid_outfit/tripo_skirt_v1 1.25
../.venv/bin/python split-female-guard-skirt.py assets/items/female_pink_maid_outfit/tripo_boots_v1 0.9 boots 0.83
```

두 분리 스크립트가 폴더를 인자로 받게 했다. 치마 스크립트의 둘째 인자는 그 높이 위의 떨어진 조각을 버리고, `boots`는 조각을 다리별로 나누며, 넷째 인자는 그 높이에서 윗단을 평평하게 자른다.

- 상의: 배율 0.798, 평균 5.5mm, 95% 12.3mm.
- 치마: 배율 1.838, 평균 9.9mm. 마네킹 수영복의 등 끈 조각 두 개가 치마에 딸려 와 허리띠보다 15cm 솟았고, 허리 절단 평면이 거기에 맞춰져 수직으로 섰다. 팔 피부까지 328개를 잘못 잘라 위팔에 구멍이 보였다(사용자 지적). 1.25m 위의 조각을 버려 평면이 1.21m에 수평으로 놓였다.
- 스타킹: 배율 1.822, 평균 6.1mm. 시안의 윗단 레이스 띠가 3D에서는 마네킹 파란색으로 나와 윗단이 들쭉날쭉했다. 0.83m에서 평평하게 잘라 민무늬로 쓴다(사용자 결정).

## 피팅

[피팅 스크립트](../../tools/blender-scripts/fit_female_outfit.py)의 `pink_maid` 항목. 의상 설정은 `maid`를 물려받는다.

| 부위 | 출력 | Triangles | 가린 몸체 삼각형 |
| --- | --- | ---: | ---: |
| 상의 | `top/fitted_v1/top_female_pink_maid.glb` | 11,886 | 4,469 |
| 치마 | `bottom/fitted_v1/bottom_female_pink_maid.glb` | 4,243 | 1,748 |
| 장갑 | `gloves/fitted_v1/gloves_female_pink_maid.glb` | 4,078 | 4,708 |
| 스타킹과 구두 | `boots/fitted_v1/boots_female_pink_maid.glb` | 4,110 | 3,349 |

- 상의: 소매 정렬과 소매 끝 슬라이더는 검은 메이드와 같다. 소매 주름이 소매와 같은 흰색이라 색으로 고를 수 없어, 소매 끝 3.5cm를 주름으로 본다(`clip_sleeve`의 `ruffle`).
- 상의 허리: 코르셋 아랫단이 몸체 허리보다 2cm 넓었다(19.6×16.1cm, 몸체 17.8×14.1cm). 가슴 밑 1.29m에서 아랫단까지 선형으로 좁혀 아랫단을 0.93배로 맞춘다(`taper_waist`, 사용자 제안).
- 치마: 설정은 검은 메이드 치마와 같고, 분홍 치마 판이 260–1,250면 조각으로 나와 속 껍질 판정의 겹 기준만 250면으로 낮췄다. 허리띠가 몸체보다 작아 코르셋이 띠를 뚫었다. 사용자가 미리보기에서 전체 102.5%, 좌우 0.3cm, 앞뒤 −0.2cm로 맞췄다([저장값](female-pink_maid-preview-fit.json)). 띠만 1.16–1.18배 넓히는 보정은 너무 커서 버렸다(사용자 지적).
- 장갑: 검은 장갑의 텍스처에서 이 파츠의 텍셀을 흰색으로 칠한다(`repaint`의 `white_cloth`). 게임용 1024px 텍스처에서 조각 가장자리에 검정이 섞여 회색 솔기가 보여, 칠하는 범위를 가장자리 밖 8px로 넓혔다.
- 스타킹: 굵기 보정 없이 통째로 9mm 뒤로 옮겼다(`offset_y`). 생성물이 다리 전체에서 앞으로 1cm쯤 치우쳐 있었다. 구두는 2.3cm 내려 밑창을 판금 부츠 높이에 맞췄다(`lower_shoe`).
  - 무릎 근처가 몸체보다 폭 6–9mm, 앞뒤 2cm 두껍다. Tripo 원본에 무릎과 종아리 굴곡이 새겨져 있다. 입구와 발목 두 단면으로 맞추는 `taper`는 이 굴곡을 줄이지 못해 뺐다(사용자 결정).
  - 구두의 검정이 윗선을 넘어 스타킹에 번져 있어, 무광인데 어두운 텍셀을 스타킹의 흰색으로 칠한다(`repaint`의 `clean_knit`). 붉은 텍셀과 아래를 향한 면은 뺀다.
  - 생성된 붉은색이 굽 안쪽 면 중간에서 끝나, 밑창과 굽 안쪽 면을 붉은 재질로 바꾼다(`sole_colour`).
- 검은 스타킹을 흰색으로 칠하는 방식은 버렸다. 구두의 광택 경계가 들쭉날쭉해 흰색이 구두로 번졌고, 윗선을 평면으로 다시 그려도 깔끔하지 않았다(사용자 지적). 그 코드는 남기지 않았다.

미리보기 `/modular-character-preview.html?character=female&top=pink_maid&bottom=pink_maid&boots=pink_maid&gloves=pink_maid&helmet=maid&hair=twintail`.

## 가터 끈

`tools/blender-scripts/build_female_maid_garters.py -- --outfit pink_maid`가 `garters/fitted_v1/garters_female_pink_maid.glb`를 만든다. 검은 가터와 같은 형태이고 끈은 흰색, 걸쇠는 금색 금속 재질이다(사용자 요청). 448 triangles.

- 메이드 치마(검정·분홍)와 메이드 스타킹(검정·흰색)을 함께 입으면 보이고, 색은 스타킹을 따른다(`femaleCharacter.ts`).
- 흰 스타킹은 붉은 밑창 때문에 재질이 둘이라 three.js가 메쉬 이름에 `_1`, `_2`를 붙인다. 미리보기의 착용 조건은 `boots_female_pink_maid_1`을 본다.
- 스타킹을 고치면 걸쇠 위치가 달라지므로 다시 만든다.

## 게임 연결

- 아이템 `pink_maid_top`, `pink_maid_pants`, `pink_maid_gloves`, `pink_maid_boots`: 가격·판매처 없음. 아이콘과 바닥 모델은 `export_outfit_items.py --outfit pink_maid --texture-size 512`.
- 남성 몸체에는 여성 전용 옷으로 처리돼 기본 옷이 보인다(`FEMALE_ONLY_MODEL`).
- `data-src/npcs.csv`의 Cocoly: `maid_helmet`과 위 네 아이템 착용, 헤어 `ponytail`, 헤어색 `#e3c27a`, 여성. `modelPaths.ts`의 Cocoly 모델 지정을 지웠다.

## 남은 일

- Cocoly의 헤어(포니테일, 금발)와 얼굴(기본)은 사용자에게 묻지 않고 원화에 가까운 것으로 골랐다.
- 게임 화면에서 Cocoly를 보지 않았다. 확인은 미리보기의 대기 자세와 `femaleCharacter.test.ts`까지다. 걷기·앉기는 보지 않았다.
- `client/public/models/characters/pink_maid.glb`와 `assets/npcs/pink_maid`는 지우지 않았다.

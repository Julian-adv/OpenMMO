# 여성 긴 웨이브 · 땋은 머리

2026-11-16. Rica의 원화·초상화에 있는 헤어다. Rica를 모듈형으로 옮기며 만들었고([잡화점 의상](modular-female-shopkeeper-tripo.md)), 사용자 결정으로 플레이어도 고를 수 있다. 절차는 [긴 생머리 두 종](modular-female-long-straight-hair.md)과 같다.

| | |
|---|---|
| 저장값 `HairStyle` | `long_wavy` (신규, 프로토콜 161) |
| 게임 파츠 | `long_wavy_hair.glb` (고른 캐릭터만 지연 로드) |
| 작업 폴더 | `assets/modular_human_female_01/hair/tripo_rica_wavy_v1` |
| 헤어 삼각형 (생성 전체) | 3,925 (9,928) |
| 기본 색(텍스처 평균) | `#b9aca3` |
| 생성 화면 라벨 | 긴 웨이브 · 땋은 머리 |

## 원화

OpenAI Codex built-in ImageGen(`codex exec`), **ChatGPT Pro 20x**, 2026-11-16, OpenAI 생성 출력물 이용 조건. 프롬프트·해시·참고 이미지는 `hair-sources.json`.

- `tripo_long_side_v2/mannequin-*.png`(민머리 파란 마네킹 정면·옆·뒤) 위에 그렸다. 정면은 `rica-portrait.png`의 헤어를 참고했고, 옆·뒤는 정면 결과를 참고했다.
- 가운데 가르마, 앞머리 없음, 양 관자놀이에서 뒤통수로 도는 가는 땋은 머리, 굵은 물결, 가슴 위·견갑골 길이. 초상화보다 짧다.
- `worn-front.png` `ef0d8d42…`, `worn-side.png` `7f8947f2…`, `worn-back.png` `8eed0a82…`. 세 장 모두 사용자 승인.

## 3D

- Tripo API 유료 크레딧, `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR, face_limit 10000, 다중 시점 front·**left**·back, seed 111603. 작업 ID `b2fb9dc4-364d-482f-ae04-5c2fabf127b6`, 120크레딧(2920 → 2800). [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms). 한 번에 사용 가능한 결과가 나왔다.
- 출력 `model_url.glb`, SHA-256 `459f8c03…`.

## 분리와 피팅

```bash
cd tools && ../.venv/bin/python split-female-long-hair.py --style wavy
blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_long_hair.py -- --style wavy
cd tools && ../.venv/bin/python rig-female-long-hair.py --style wavy
node tools/prepare-female-knight.mjs --part long_wavy_hair
```

- 분리: 은회색 머리는 빨강−파랑 차이가 10–30이라 기존 기준(30 미만은 마네킹)으로는 헤어가 29 triangles만 남았다. 이 스타일만 기준을 −20으로 둔다. 두상 평균 오차 3.2mm, 95% 8.3mm.
- 피팅: 옆 가르마 머리처럼 이마선을 내리고 이마 가장자리를 피부에 붙인다. 내리는 양은 `doc/assets/female-hairline.json`에서 읽는다. 처음 1.5cm로 구웠고, 사용자가 미리보기의 이마선 슬라이더(휠로도 조절)로 2cm를 저장해 다시 구웠다(2026-11-16). 슬라이더가 저장한 `pending_cm`은 다음 피팅 때 `baked_cm`이 된다.
- 얼굴별 보정(`femaleHairFits.json`): 기본(순찰자) 얼굴은 보정 없음. chic 얼굴은 보정 없이는 가르마 앞 두피가 드러나 `long`의 값을 그대로 넣었고 미리보기에서 가려지는 것을 확인했다. custom 얼굴도 `long` 값을 넣었으나 확인하지 않았다.

## 게임 연결

- `shared/src/character.rs`: `HairStyle::LongWavy`, 여성 헤어 판정, 텍스처 평균색. 프로토콜 **161**.
- 클라이언트: `FEMALE_HAIR_PARTS`, `femaleHairFit.ts`, `appearanceColors.ts`, 생성 화면 선택지와 5개 언어 라벨, 미리보기 두 곳.
- 썸네일 `female-hair-long-wavy.png`: `render_female_hair_thumbnail.py --body .../faces/tripo_ranger_v1/base_ranger.glb`로 512² 렌더 후 128². 기본 몸체로 렌더하면 얼굴 파츠가 없어 가르마 앞이 V자로 비어 보인다.
- `data-src/npcs.csv`의 Rica가 `long_wavy`, 머리색 `#d9d5cd`를 쓴다.

## 남은 점

- 투구 헤어 마스크는 칠하지 않았다. 은빛 기사 투구는 미리보기 정면에서 큰 문제가 없었고 다른 투구는 보지 않았다.
- 동작 검토(`review_female_long_hair.py`)는 돌리지 않았다. 미리보기의 걷기·달리기에서만 봤다.
- 중간 산출물 `hair-split.glb`·`hair-fitted-unskinned.glb`는 사용자 요청(2026-11-16)으로 지웠다. 위 명령을 차례로 실행하면 다시 생긴다.

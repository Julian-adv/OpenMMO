# 여성 긴 생머리 두 종 — 옆 가르마, 일자 앞머리

2026-11-11. 사용자 결정으로 기존 긴 머리([전사 긴 헤어](modular-female-warrior-hair.md))를 대신하는 옆 가르마 긴 생머리와,
새 헤어인 일자 앞머리 긴 생머리를 만들었다. 길이는 둘 다 가슴 위·견갑골 높이다.

| | 옆 가르마 | 일자 앞머리 |
|---|---|---|
| 저장값 `HairStyle` | `long` (기존 값, 모습만 교체) | `long_bangs` (신규, 프로토콜 145) |
| 게임 파츠 | `valkyrie_hair.glb` | `long_bangs_hair.glb` (고른 캐릭터만 지연 로드) |
| 작업 폴더 `assets/modular_human_female_01/hair/` | `tripo_long_side_v3` | `tripo_long_bangs_v2` |
| 헤어 삼각형 (생성 전체) | 1,928 (9,832) | 1,130 (9,743) |
| 기본 색(텍스처 평균) · 선형 밝기 | `#cbac91` · 0.442 | `#dbb99d` · 0.522 |
| 생성 화면 라벨 | 긴 생머리 | 긴 생머리 · 앞머리 |

발키리의 기본 헤어는 그대로 `long`이라 새 옆 가르마 머리가 된다. 기존 `long` 캐릭터의 저장된 머리색 `#d2bda2`는 그대로 그 색으로 착색된다.
투구를 써도 두 헤어 모두 고른 그대로 보인다.

## 원화

OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-11, OpenAI 생성 출력물 이용 조건.
프롬프트·해시·참고 이미지는 각 폴더의 `hair-sources.json`.

1. 머리만 있는 정면 시안(`tripo_long_side_v1/hair-front-v1.png`, `tripo_long_bangs_v1/hair-front-v1.png`)으로 모양을 승인받았다.
2. 우리 여성 몸체(순찰자 얼굴, `base_ranger.glb`)를 민머리 파란 무광 마네킹으로 정면·옆·뒤 평행 투영 렌더했다
   (`tripo_long_side_v2/mannequin-*.png`, Blender 5.2 Cycles, 생성 아님).
3. 그 렌더 위에 시안의 헤어를 그려 넣었다(`worn-front.png`, `worn-side*.png`, `worn-back.png`).
   옆모습은 어깨와 팔이 머리카락 앞에 오게 그린다. 머리카락이 어깨를 덮으면 정면·뒷면과 어긋난다.

## 3D

Tripo API 유료 크레딧, `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR, 다중 시점, Tripo 이용 약관.
각 폴더의 `build.json`·`generation.json`.

| 시도 | 폴더 | 입력 | seed · task | 크레딧 (잔액) | 결과 |
|---|---|---|---|---|---|
| 옆 가르마 1차 | `tripo_long_side_v1` | 머리만, front·right·back, face_limit 3000 | 111101 · `bb47d1db-5fbe-4a2e-8928-a7cf9941d3fa` | 120 (4000 → 3880) | **[미사용]** 뒤통수가 납작한 판, 정수리 능선, 폭 과다 |
| 옆 가르마 2차 | `tripo_long_side_v2` | 마네킹 착용, front·right·back, face_limit 10000 | 111103 · `09243dbc-c160-447b-a655-a8c9948e9546` | 120 (3880 → 3760) | **[미사용]** 뒤통수에 얼굴이 하나 더 생김 |
| 옆 가르마 3차 | `tripo_long_side_v3` | 2차와 같은 그림, front·**left**·back | 111103 · `4abe1865-8e41-44c0-a26e-cf21103de880` | 120 (3760 → 3640) | 사용 |
| 일자 앞머리 | `tripo_long_bangs_v2` | 마네킹 착용, front·left·back, face_limit 10000 | 111104 · `1c44c61e-2df2-4944-8b78-97a28207ccc4` | 120 (3640 → 3520) | 사용 |

배운 점:

- **옆모습 칸**: 얼굴이 화면 왼쪽을 보는 옆모습은 착용자의 왼쪽에서 본 것이고 `left` 칸에 넣는다. `right`에 넣으면 정면 그림과
  충돌해 앞뒤에 얼굴이 생긴다. [단발 기록](modular-female-knight-hair.md)의 "화면 왼쪽을 보는 측면 = right"는 틀렸다.
  머리만 있는 좌우 대칭 입력에서는 드러나지 않았다. Tripo 문서에는 left·right의 카메라 정의가 없어 결과에서 확인한 것이다.
- **두상 없이 그린 헤어**는 뒤통수 볼륨이 없고 폭이 넓게 나온다. 우리 몸체 마네킹 위에 그리면 두상끼리 맞춰 올릴 수 있고
  파란색으로 헤어만 떼어낼 수 있다.

## 분리와 피팅

```bash
cd tools && ../.venv/bin/python split-female-long-hair.py --style side   # 또는 bangs
blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_long_hair.py -- --style side
cd tools && ../.venv/bin/python rig-female-long-hair.py --style side
node tools/prepare-female-knight.mjs --part valkyrie_hair                # 또는 long_bangs_hair
blender -b --python-exit-code 1 --python tools/blender-scripts/review_female_long_hair.py -- --style side --out <폴더>
```

- `split-female-long-hair.py`: 생성물의 파란 정점을 우리 몸체에 ICP(회전·균일 스케일·이동)로 맞춘다. 두상 평균 오차는
  옆 가르마 2.9mm, 앞머리 4.3mm다. 색 텍스처에서 파란색이 20% 이상 섞인 삼각형을 버리고, 잘린 가장자리의 홈과
  뾰족한 조각을 정리하고, 파란 텍셀을 가장 가까운 헤어 색으로 채운다. 헤어가 한 겹 면이 되므로 재질을 양면으로 둔다.
  결과 `fitted_v1/hair-split.glb`, 기록 `split.json`.
- `fit_female_long_hair.py`: 순찰자 얼굴에 저장된 보정값(`femaleHairFits.json`)을 적용한 위치에서 아래를 계산하고 되돌린다.
  보정값을 크게 바꾸면 다시 실행한다. 기록 `fitted_v1/fitting.json`.
  - 공통: 얼굴 둘레의 잘린 가장자리를 선을 따라 매끄럽게 펴고, 몸 표면 3mm 안쪽 정점을 표면 밖으로 올린다.
  - 옆 가르마(사용자 요청): 이마 위 머리카락을 이마 경사를 따라 1.5cm 내리고, 이마 가장자리를 피부 1.5mm 위에 붙인다.
  - 앞머리(사용자 요청): 끝 선을 좌우 같은 높이의 일자로 맞추고, 옆머리 안쪽 선과 직각으로 만나게 모서리의 머리카락을 옮긴다.
    눈 옆(끝 선 아래 4cm)에서 앞머리 끝과 옆머리를 잇던 벽면 삼각형 26개를 지웠다. 캐릭터 왼쪽은 벽면 전체를,
    오른쪽은 옆머리 앞 모서리보다 앞에 있는 조각만 지운다(오른쪽은 다 지우면 머리 안쪽이 들여다보였다).
- 중간 산출물 `hair-split.glb`·`hair-fitted-unskinned.glb`는 사용자 요청(2026-11-11)으로 지웠다. 위 명령을 차례로 실행하면 다시 생긴다.
- `rig-female-long-hair.py`: 여성 리그 65본. 목 위는 `Head`, 가슴에 닿는 부분은 가까운 가슴 표면의 가중치, 나머지는 몸통 평균.
  물리는 없다.

## 얼굴별 보정

긴 머리 두 종은 어깨 아래가 몸에 얹혀 있고 몸은 얼굴과 상관없이 같다. 그래서 얼굴별 보정(`femaleHairFit.ts`의
`applyFemaleHairFit`)을 높이 1.60m 위에서는 전부, 1.46m 아래에서는 0으로, 그 사이는 부드럽게 줄여 적용한다
(사용자 확인 2026-11-11: 시크 얼굴에서 헤어 전체를 앞으로 당기면 뒷머리가 등에 묻혔다). 단발·포니테일은 전체에 적용한다.
보정값은 사용자가 미리보기(`/modular-character-preview.html?character=female-face&face=ranger|chic&hair=long|long_bangs`)에서 맞춘다.

## 게임 연결

- `shared/src/character.rs`: `HairStyle::LongBangs`, 여성 헤어 판정, 텍스처 평균색. `long`의 평균색을 `#cbac91`로 바꿨다.
  프로토콜 **145**.
- 클라이언트: `FEMALE_HAIR_PARTS`, `femaleHairFit.ts`, `appearanceColors.ts`, 생성 화면 선택지와 5개 언어 라벨.
- 썸네일 `female-hair-long.png`·`female-hair-long-bangs.png`: `render_female_hair_thumbnail.py`로 512² 렌더 후 128²,
  순찰자 얼굴 보정값을 `--hair-move`·`--hair-scale`로 넘겼다. 로컬 렌더이며 새 생성은 없다.
- 투구 헤어 마스크: 기존 긴 머리용 마스크 두 건은 메시가 달라져 지웠다. [투구 헤어 브러시](female-helmet-hair-brush.md) 참조.

## 검사와 남은 점

- 동작 검토(`fitted_v1/motion-review.json`, 몸체의 9개 동작 각 3시점, 5mm 넘게 몸 안으로 새로 들어간 정점 수):
  대기 0–5, 걷기·달리기 3–6, 앉기 약 50, 베기 32–90, 점프 109–197. 점프와 베기는 고개를 크게 숙일 때 뒷머리가 등에서 들리고
  앞가닥이 팔과 겹친다.
- 브라우저 확인은 사용자가 미리보기에서 했다. 발키리 날개관, 의상별 깃·어깨 갑옷과의 겹침은 확인하지 않았다.
- 옆 가르마: 뒷머리 아래 끝이 등에서 조금 떠 있다. 가르마 끝과 관자놀이에 작은 틈이 남아 있다.
- 앞머리: 사선에서 보면 앞머리 끝과 옆머리 사이에 단차가 보인다. 캐릭터 왼쪽 눈 옆은 벽면이 없어 각도에 따라 머리 안쪽이 보인다.
- 조립 삼각형은 몸체 22,175에 헤어 1,928 또는 1,130을 더한 값이며 조립 목표 15,000–20,000을 넘는다. 감축하지 않았다.

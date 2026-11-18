# 여성 경비대 복장

[남성 경비대 복장](modular-male-guard-tripo.md)의 여성판이다. 같은 아이템(`guard_*`)을 여성이 입으면 이 모양으로 보인다. 몸체는 [여성 몸체 기록](modular-human-female-01.md), 절차는 [잡화점 주인 의상](modular-female-shopkeeper-tripo.md)과 같다.

## 구성 (사용자 결정 2026-11-18)

- 투구(철모·사슬 두건), 상의(사슬 깃·긴소매 사슬 갑옷·허리띠), 장갑, 타바드는 남성판과 같은 디자인이다.
- 바지 대신 치마: 짙은 갈색 가죽 띠를 겹쳐 늘어뜨린 치마, 띠 끝마다 쇠못, 무릎 위 길이.
- 장화는 허벅지 중간까지 올라오고 윗단에 천이 없다. 무릎 보호대를 장화 위에 맨다. 굽은 스틸레토.
- 이 투구는 머리 모양과 관계없이 헤어를 모두 가린다.

무릎 보호대는 남성판에서 바지에, 여성판에서 장화에 속한다.

## 원화 — 2026-11-18

OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**. OpenAI 생성 출력물 이용 조건을 따른다. 파일과 프롬프트(`*-prompt.txt`)는 `assets/items/female_guard_outfit/concepts/`. 마네킹은 잡화점 의상 때의 30° 하이힐 마네킹과, 같은 스크립트로 렌더한 흉상(`bust-*.png`, 높이 1.05–2.05m)·사슬 갑옷을 입힌 마네킹(`mailed-*.png`)이다.

| 파일 | SHA-256 | 내용 |
| --- | --- | --- |
| `body-front-v5.png`, `body-back-v5.png` | `632e72a6…c22b`, `e5eff42a…b4f0` | 채택본. 남성 `body-front-v2.png`을 참조 |
| `body-*-v1`…`v4` | | **[미사용]** v1 바지, v2 모직 주름 치마, v3 가죽 치마·허벅지 장화·장화 윗단 리넨, v4 리넨 제거(굵은 굽). 그림은 지우고 프롬프트만 남겼다 |
| `helmet-front-v1.png`, `helmet-side-v1.png`, `helmet-back-v1.png` | `7d0cf5d2…6c1e`, `8f1d3cc8…7333`, `16642978…e5c3` | 흉상 위의 투구 |
| `skirt-front-v1.png`, `skirt-back-v1.png` | `2ce358cc…60b0`, `d4177265…6282` | 맨 마네킹 위에 허리부터 그린 치마 |
| `tabard-front-v1.png`, `tabard-back-v1.png` | `1a218005…0b41`, `7a8b4c2e…029f` | 사슬 갑옷을 입힌 마네킹 위의 무지 타바드 |

## Tripo 생성 — 2026-11-18

Tripo API, 유료 API 크레딧. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms). P2-20260801, detailed 텍스처 `v3.5-20260815`, 설정과 응답은 각 폴더의 `build.json`, `generation.json`. 네 건 480크레딧(2,440 → 1,960).

| 폴더 | 입력 | 작업 ID | 출력 SHA-256 |
| --- | --- | --- | --- |
| `tripo_body_v1` | front, back (seed 111804) | `f05f9ac7-274f-4e9a-a01c-1e36f4f498b1` | `c18bef02…7351` |
| `tripo_helmet_v1` | front, left, back (seed 111805) | `d5daea7e-59a4-42d1-b800-d433e0d7d1b3` | `7f5803a5…0d54` |
| `tripo_skirt_v1` | front, back (seed 111806) | `936ce494-3954-43b7-9cf8-dc4c67c4cca8` | `7d940fd2…03b4` |
| `tripo_tabard_v1` | front, back (seed 111807) | `daea28a4-9935-4298-9d82-8c6dc503ca86` | `b1a3c4b1…ca04` |

몸 생성에서 가죽 띠가 사슬 밑단과 한 덩어리로 나왔다. 밑단에서 자르면 치마는 높이 9cm 띠만 남아, 치마를 따로 생성했다(사용자 결정). 스틸레토 굽은 끊기지 않고 나왔다.

## 마네킹 분리

- `tools/split-female-guard.py`: 축척은 바닥에서 정수리까지의 키로 고정한다(1.85). 파란 부분(머리·목·허벅지 띠)에 맞춰 축척까지 풀면 8% 작아지고 밑창이 13cm 떴다. 사슬 밑단(0.81m) 아래의 띠는 버려지지 않고 `skirt`로 남지만 쓰지 않는다. 무릎 보호대와 끈은 장화에 넣는다.
- `tools/split-male-guard-helmet.py --female`, `tools/split-male-guard-tabard.py --female`: 남성판 스크립트에 여성 몸체(하이힐 자세)를 기준으로 쓰는 옵션을 넣었다. 남성 출력은 해시가 그대로다.
- `tools/split-female-guard-skirt.py`: 몸 전체가 파랗게 드러나 축척까지 맞춘다(1.840, 평균 8.7mm).

## 피팅 v1

투구·상의·치마·장갑·장화는 [피팅 스크립트](../../tools/blender-scripts/fit_female_outfit.py)의 `guard` 항목, 타바드는 `tools/fit-female-guard-tabard.py`.

| 부위 | Triangles |
| --- | ---: |
| 투구 | 2,093 |
| 상의 | 4,720 |
| 치마 | 5,616 |
| 장갑 | 2,906 |
| 장화 | 5,218 |
| 타바드 | 7,432 |

합계 27,985 triangles, 몸체는 따로다. 조립 캐릭터 목표(15,000–20,000)를 넘는다. 줄이지 않았다. 보이는 삼각형 수와 얼굴 배분은 재지 않았다.

- 사슬 갑옷의 엉덩이가 팔만큼 옆으로 나와 있어, 소매 정렬과 장갑 안쪽 소매 자르기가 엉덩이를 소매로 보고 밑단을 찢었다. `arm_reach=.12`로 아래팔 축 가까이의 점만 소매로 본다(`female_outfit_pose.py`, `cut_sleeves`). 값이 없는 다른 복장은 이전과 같다.
- 치마: 사슬 갑옷과 같이 입으면 밑단(0.775m) 아래의 띠 끝만 그린다(`modularClothing.ts`의 `female_guard_skirt`). 통째로 그리면 사슬 위를 덮는다.
- 타바드: 남성판처럼 사슬 갑옷의 가중치를 가져오고 둘레를 1.16배 키웠다. 문장 자리는 18×28.8cm, 윗변 1.42m.

## 게임 연결

- `femaleCharacter.ts`: `guard` 스타일(다섯 부위 지연 로드), 두건 투구는 모든 헤어를 숨김(`HOODED_HELMETS`). 타바드는 `femaleTabardMeshes`로 불러와 남성과 같은 재질 코드(`tabard.ts`)를 쓴다.
- `prepare-female-knight.mjs --part guard_<slot>|guard_tabard`. 미리보기 `/modular-character-preview.html?character=female&top=guard&bottom=guard&boots=guard&gloves=guard&helmet=guard`.
- `data-src/npcs.csv`의 Brenna, Halda: 낡은 발키리 갑옷과 양모 망토 대신 룬 검과 이 여섯 아이템 착용(사용자 결정 2026-11-18), 성별 female, 한 벌 25,000에 판매·선물. 문장은 그대로 `aldermark`. 개발 서버에서 Halda가 녹색 타바드와 허벅지 장화 차림으로 숙소 침대에 누워 있는 것을 봤고 Brenna는 보지 못했다.
- 개발 서버에서 여성 캐릭터가 여섯 부위를 입은 모습, 타바드 염색, 치마만 입은 모습을 확인했다(2026-11-18).

## 남은 일

- 타바드 가슴의 문장 그림은 팔과 지팡이에 가려 또렷이 보지 못했다.
- 굽 끝·앞코를 판금 부츠 기준에 맞추는 검사(`compare-female-boot-lift.mjs`)와 미리보기 손 피팅을 하지 않았다.
- 타바드는 사슬 갑옷 위에서만 확인했다. 다른 여성 상의 위, 수면·앉기 동작, 캐릭터 선택 화면은 확인하지 않았다.
- Brenna와 Halda의 옷 판매·선물은 시험하지 않았다. prod에서는 agent-client 배포가 따라야 한다.
- 개발 서버의 jake5에게 여섯 아이템을 지급해 시험했다. 투구·장갑·치마·장화는 입은 채이고 상의와 타바드는 벗겨 두었다.
- `assets.lock`과 HF는 갱신하지 않았다.

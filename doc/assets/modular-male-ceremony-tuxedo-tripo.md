# 남성 합병식 턱시도

가문 합병식에 입는 남성 예복 기록이다. 남성 공통 몸체는 [남성 몸체 안내](modular-human-male-01.md), 여성 예복은 [여성 합병식 예복](modular-female-ceremony-tripo.md)에 있다.

## 구성 (사용자 결정 2026-11-15)

- 새틴 깃의 검은 재킷(금 단추 하나, 허리 아래 길이), 흰 셔츠와 검은 나비넥타이, 금색 조끼, 깃에 초승달 두 개 핀.
- 옆선에 새틴 줄을 넣은 검은 바지, 손목 길이 흰 장갑, 검은 구두. 머리 장식은 없다.
- 슬롯: 상의(재킷·셔츠·조끼·넥타이·커프스), 바지, 장갑, 신발.
- 여성 예복 아이템과 따로 새 아이템 네 종으로 넣는다. 여성이 입으면 천 기본 외형으로 보인다.

## 원화 — 2026-11-15

![정면 원화](../../assets/items/male_ceremony_tuxedo/concepts/front-v1.png)

마네킹: `tools/blender-scripts/render_concept_base.py --character male --mannequin --view front|side`로 남성 몸체를 파란 무광으로 렌더했다(Blender 5.2 Cycles, 생성 아님). 스크립트는 여성용이던 것을 남녀 공용으로 바꿨고, 여성 정면 렌더는 이전 파일과 화소 단위로 같다. 남성은 `body_boot_ankles`를 빼고 바닥 높이를 모든 메시에서 잰다.

원화: OpenAI Codex built-in ImageGen(`codex exec`, codex-cli 0.164.0), **ChatGPT Pro 20x**, 2026-11-15. OpenAI 생성 출력물 이용 조건을 따른다. 정면·옆 마네킹을 참조로 주고 정면 한 장을 받았다. 파일은 `assets/items/male_ceremony_tuxedo/concepts/`, 프롬프트는 `front-v1-prompt.txt`.

| 파일 | SHA-256 | 내용 |
| --- | --- | --- |
| `mannequin-front.png` | `11e59acf…f870` | 마네킹 정면 |
| `mannequin-side.png` | `3b196d4e…f20e` | 마네킹 옆 |
| `front-v1.png` | `c6d05f46…dd0f` | 정면 채택본, 손보지 않음 |

원화의 손은 마네킹보다 조금 안쪽이다. 다리와 발 위치는 맞는다.

## Tripo 생성 — 2026-11-15

- Tripo API, 유료 API 크레딧, 작업 ID `b5d5604c-b33d-4e2f-9c42-4221c7c10b9a`, 120크레딧(3280 → 3160). [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- 정면 한 장 입력(사용자 결정). 설정은 `tripo_front_v1/build.json`(P2-20260801, face_limit 20000, detailed 텍스처, seed 111501), 응답은 `generation.json`.
- 출력 `tripo_front_v1/model_url.glb`, SHA-256 `99f4c18f4ebbef1cacc3b5fbab423bf7671ac202fdbbe35cfd05667b330a0314`. 메시 1개, 19,823 triangles, 4096² 텍스처, 높이 1.0, +X 정면.
- 재킷, 바지, 장갑 둘, 구두 둘, 나비넥타이, 깃, 커프스, 소매 단추가 서로 떨어진 덩어리로 생성됐다. 바지는 재킷 밑단 안쪽으로 허리(1.11m)까지 이어진다.

## 마네킹 분리

`tools/split-male-tuxedo.py`가 `tripo_front_v1/segmented.glb`(SHA-256 `969191f3…0e2b`)와 `split.json`을 만든다. 둘은 보관하지 않으므로(사용자 결정 2026-11-15) 피팅을 다시 돌리기 전에 이 스크립트를 먼저 돌린다. 같은 해시가 나온다. Tripo가 함께 준 미리보기 그림과 쓰지 않은 뒤쪽 마네킹 렌더도 지웠다. 색 판정·연결 성분·정렬·자르기·기록은 `tools/outfits/mannequin.py`에 모았고 여성 예복 분리 스크립트도 이 모듈을 쓴다. 옮긴 뒤 여성 v2 `segmented.glb`는 기록된 해시 그대로이고, v1은 옮기기 전 스크립트의 출력과 같다(v1 보관본은 이전 판 스크립트로 만든 것이라 다시 만들지 않았다).

- 파란 면 4,487개를 버린다. 옷이 몸을 다 덮어 파란 부분은 머리와 목뿐이다.
- 머리만으로 맞추면 2.3° 기울고 밑창이 바닥에서 3.7cm 떴다. 머리와 장갑을 함께 쓰고 수직축 회전만 허용한다(배율 1.907, 밑창 −0.3cm, 좌우 대칭).
- 남은 면은 연결 성분으로 나눈다. 흰색이고 1,000 triangles가 넘는 팔 끝 덩어리가 장갑, 0.3m 아래가 구두, 0.4m 아래로 내려오는 것이 바지, 나머지가 상의다. 머리 높이(1.66m 위)의 덩어리는 마네킹의 입이라 버린다.

| 부위 | Triangles |
| --- | ---: |
| 상의 | 6,941 |
| 바지 | 1,314 |
| 장갑 | 3,512 |
| 구두 | 3,424 |

합계 15,191 triangles, 몸체는 따로다. 줄이지 않았다.

한 벌을 입은 캐릭터(크롭 헤어, 무기 없음)는 미리보기 집계로 조합 29,680 / 표시 18,946 triangles, 얼굴 1,505다. 표시 기준으로는 15,000–20,000 목표 안이고 숨긴 영역까지 합치면 넘는다.

## 피팅 v1

`tools/fit-male-tuxedo.py`가 `assets/modular_human_male_01/tuxedo/tripo_v1/{top,pants,gloves,boots}_tuxedo.glb`와 [피팅 기록](modular-male-tuxedo-fitting-v1.json)을 만든다. 기준은 `fitted/base.glb`, 리그 `human_male_01_mixamo_candidate_v2`.

생성된 인물은 우리 몸체보다 다리가 3cm 뒤, 팔이 5–10° 다른 각도였다. 부위를 통째로만 옮겼다.

- 구두: 좌우 각각 발에 맞춰 수평 이동(앞으로 3.3cm). 높이는 그대로 둔다.
- 바지: 골반 관절을 축으로 1.96° 기울여 밑단이 구두를 따라간다. 통째로 앞으로 옮기면 허리가 재킷 앞으로 나왔다.
- 장갑: 좌우 각각 손에 강체로 맞춘다(회전 10°, 중앙값 3mm).
- 소매: 어깨와 팔꿈치 관절을 축으로 몸체 팔 방향으로 돌린다(어깨 왼쪽 9.4°·오른쪽 5.2°, 팔꿈치 1.4°·0.8°). 관절에서 3–9cm 구간에 걸쳐 섞는다. 여성 의상의 소매 정렬과 같은 처리다.
- 소매 끝: 소매가 굵은 아래팔에 헐겁게 맞아 끝이 손목 축에서 1.5cm 비켜 있었다(사용자 제보 2026-11-15, 게임에서 장갑과 어긋나 보임). 팔꿈치에서 한 번 더 돌려 소매 끝 3cm의 중심을 손목 축에 놓는다(왼쪽 3.1°, 오른쪽 2.3°; 남은 차이 1mm·4mm). 아래 소매를 팔에 강체로 맞추면 팔을 타고 8cm 올라가 쓰지 않았다.
- 가중치: 가장 가까운 몸체 표면에서 옮기고 메시 위에서 고르게 편다. 바지는 기존 바지 피팅의 `skin_weights`를 쓴다.
- 재킷과 바지는 근육질 몸체보다 가늘다. 덮인 몸체 부위는 게임에서 숨기므로 밀어내지 않았다.

## 런타임

- 스타일 `tuxedo`: `modularCharacter.ts`, `modularEquipment.ts`, `tools/prepare-modular-character.mjs --tuxedo-only`(`toktx` 필요).
- 상의는 몸통·위팔·아래팔을, 장갑은 손을, 바지는 다리·발목을, 구두는 발을 숨긴다. 목은 `tripo_collar`로 자른다.
- 장갑 입구는 소매보다 굵어(반지름 3.8cm 대 3.6cm) 소매 밖으로 나왔다. 재킷과 함께 끼면 장갑을 손목 바로 위(팔꿈치→손목 0.97)에서 자른다(`tuxedo_glove_cuff`).
- 장갑만 끼면 손 메시를 통째로 숨기는 대신 장갑 입구 안쪽(0.92)에서 피부를 자른다(`tuxedo_gloves`). 통째로 숨기면 아래팔 메시의 톱니 모양 경계가 드러났다(사용자 제보 2026-11-15). 장갑 입구의 어둡고 찢긴 테두리는 0.9에서 잘라낸다(`tuxedo_glove_rim`).
- 마네킹 오브젝트는 배치 데이터의 `body: "male"`로 남성 몸체를 세운다(없으면 여성). 개발 데이터에 옷가게 마네킹 145(-1484.4, 4789.8)를 넣었다. 배치 API가 401을 돌려줘 파일에 직접 넣었고, 서버가 다시 뜨기 전에는 통과된다. 게임에서 보지 않았다.
- 미리보기 `/modular-character-preview.html?outfit=tuxedo`.

![대기와 걷기](../../assets/items/male_ceremony_tuxedo/preview-idle-walk.png)
![걷기·달리기·앉기·공격](../../assets/items/male_ceremony_tuxedo/preview-walk-run-sit-slash.png)

확인(미리보기, 2026-11-15): 대기·걷기·달리기·공격·앉기에서 찢어짐이나 피부 노출 없음. 재킷+천 바지+가죽 부츠, 리넨 셔츠+턱시도 바지, 장갑만 낀 조합도 확인했다.

## 아이템 등록 — 2026-11-15

| 아이템 | 칸 | 무게 | 가격 |
| --- | --- | ---: | --- |
| `tuxedo_top` 예복 턱시도 재킷 | chest | 2 | 2금 40은 |
| `tuxedo_pants` 예복 정장 바지 | pants | 2 | 2금 |
| `tuxedo_gloves` 예복 흰 장갑 | hands | 0.5 | 60은 |
| `tuxedo_boots` 예복 구두 | boots | 2 | 1금 |

- 모두 천, guard 1, 거래 가능. 재봉사 Linnea가 여성 예복과 함께 판다.
- 바닥 모델과 128px 아이콘은 피팅된 파츠에서 만들었다(새 생성 요청 없음).

```sh
blender -b --python-exit-code 1 --python tools/blender-scripts/export_outfit_items.py -- --outfit tuxedo --texture-size 512
```

## 남은 일

- 게임 안 착용·구매 확인.
- `assets.lock` 갱신과 HF 올리기.
- 턱시도 바지와 목이 긴 부츠, 재킷 소매와 판금 장갑·손목 보호대 조합은 확인하지 않았다.
- prod 옷가게에는 남성 마네킹이 없다. 배포 때 배치해야 한다.
- 앉은 자세에서 무릎에 얹은 손가락 끝이 바지 안으로 들어간다.
- 턱시도 파츠는 게임에서 입은 캐릭터만 불러온다(`characterModel.ts`의 지연 목록). 이 목록에 없으면 아이템을 입어도 기본 외형으로 보인다.

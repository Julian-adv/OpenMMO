# 잡화점 주인 의상 (Rica)

Rica를 통짜 모델 `npc_woman.glb`에서 여성 공통 몸체로 옮기며 만든 NPC 전용 의상이다. 몸체는 [여성 몸체 기록](modular-human-female-01.md), 절차는 [공통 워크플로우](modular-outfit-workflow.md), 마네킹 방식은 [합병식 예복](modular-female-ceremony-tripo.md)을 따른다.

## 구성 (사용자 승인 2026-11-16)

- 상의: 레이스 단 둥근 목선의 아이보리 리넨 블라우스, 팔꿈치 아래로 걷은 소매, 밤색 가죽 코르셋.
- 하의: 놋쇠 버클 벨트, 동전 주머니, 열쇠 꾸러미, 주머니 달린 허리 앞치마, 발목 길이 꼭두서니색 치마.
- 신발: 검은 가죽 하이힐 앵클부츠. 장갑·투구 없음.
- 기존 원화의 남색 치마는 붉은색으로 바꿨다. 마네킹을 파란색으로 잘라내므로 옷에 파란 계열을 쓸 수 없다. 초커와 어깨 가방은 뺐다.

## 원화 — 2026-11-16

OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**. OpenAI 생성 출력물 이용 조건을 따른다. 파일은 `assets/items/female_shopkeeper_outfit/concepts/`, 프롬프트는 같은 폴더의 `*-prompt.txt`.

| 파일 | SHA-256 | 내용 |
| --- | --- | --- |
| `mannequin-front.png` | `27ec968a…da06f` | 예복 때 렌더한 30° 하이힐 마네킹 정면 |
| `front-v1.png` | `3bfed88a…c292` | 정면 채택본. 머리·팔·발 윤곽이 마네킹과 1px 안에서 같다 |
| `back-v1.png` | `f2beffb4…1d70` | 뒷면. 윤곽이 마네킹 뒷면과 2px 안에서 같고 밑단 높이는 정면과 4px 차이 |

## Tripo 생성 — 2026-11-16

게임에 쓰는 것은 v2다.

- **v2** (사용자 결정: 뒷면을 새로 생성): 다중 시점 front `front-v1.png`, back `back-v1.png`. 작업 ID `10a7b8bd-f40d-41da-8a73-2abc04660b38`, 120크레딧(3040 → 2920). 설정은 `tripo_multiview_v2/build.json`(v1과 같고 seed 111602). 출력 `tripo_multiview_v2/model_url.glb`, SHA-256 `a440c7c2…7695`, 19,101 triangles. 치마 뒤는 붉은 천과 작은 리본 매듭만 있고, 블라우스·코르셋·벨트·치마·앞치마가 서로 떨어진 조각으로 나왔다.
- **v1** **[미사용]**: 정면 한 장. 작업 ID `8cb0754d-fe84-4010-a20e-a8a102a355c5`, 120크레딧(3160 → 3040), `tripo_front_v1/`, seed 111601. 정면만 보고 치마 뒤에도 앞치마 판을 만들었고, 판 가장자리가 찢어져 있었다. 판 밑에 치마 면이 없어 뗄 수 없었고, 붉게 칠해도 윤곽과 틈이 남아 버렸다.
- Tripo API, 유료 API 크레딧. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms). P2-20260801, face_limit 20000, detailed 텍스처.

## 마네킹 분리

`tools/split-female-shopkeeper-outfit.py`가 `tripo_multiview_v2/segmented.glb`(SHA-256 `a5c222e0…9e78`)와 `split.json`을 만든다.

- 파란 면 5,552개를 버린다. 200면 미만 파란 조각은 옷의 그늘이라 남긴다.
- 머리와 양 아래팔의 파란 면으로 하이힐 자세 몸체에 맞춘다(배율 1.848, 평균 4.8mm, 95% 10.9mm).
- 바닥에서 1.2m 위까지 닿는 조각은 상의(블라우스, 코르셋, 끈), 나머지는 치마(벨트, 앞치마, 주머니, 열쇠, 매듭)다. 처음에는 조각 중심 높이로 갈라 코르셋 끈 한 조각이 치마에 붙었고, 상의를 벗으면 배 위에 떠 보였다.
- 치마와 앞치마의 윗단은 코르셋 밑에서 톱니 모양으로 끝나 상의를 벗으면 보였다. 벨트 뒤(1.16m)에서 수평으로 자른다.
- 소매 끝: Tripo가 마네킹 팔이 나오던 자리를 푸른 회색 원판으로 막아 놓았다. 원판을 떼면 소맷부리가 같이 찢어져서(기준을 네 번 바꿔도 남았다) 떼지 않고, 원판과 테두리의 푸른 텍셀을 블라우스 색(중앙값의 85%)으로 칠한다. 팔이 통과하므로 테두리만 보인다.
- 부츠 통은 치마 안에서 종아리 위까지 올라와 있어 무릎 아래(0.43m)에서 자른다.

## 피팅 v1

[피팅 스크립트](../../tools/blender-scripts/fit_female_outfit.py)의 `shopkeeper` 항목. 배율 1, `on_heels`. 형태는 바꾸지 않았다.

| 부위 | 출력 | Triangles | 가린 몸체 삼각형 |
| --- | --- | ---: | ---: |
| 상의 | `top/fitted_v1/top_female_shopkeeper.glb` | 6,174 | 5,092 |
| 치마 | `bottom/fitted_v1/bottom_female_shopkeeper.glb` | 4,819 | 3,704 |
| 부츠 | `boots/fitted_v1/boots_female_shopkeeper.glb` | 2,300 | 2,432 |

합계 13,293 triangles, 몸체는 따로다. 보이는 삼각형 수와 얼굴 배분은 재지 않았다.

- 치마는 `robe_physics`로 골반에 매달고, 밑단이 같은 쪽 허벅지를 따라간다. 예복의 다리 충돌체를 쓰면 걸을 때 부츠가 치마 앞을 뚫었다. 이 치마는 기본 자세에서 이미 충돌체에 닿아 있고, 리그가 그 접촉을 그대로 두기 때문이다.
- 허벅지가 앞치마를 뚫지 않게 치마 안 다리 피부(`UpLeg`, `Leg`)를 숨긴다.
- 부츠: 종아리가 치마에 가려 생성돼 통이 다리보다 앞에 있었다. 사용자가 미리보기에서 양쪽을 옆 -1.6cm, 앞뒤 2cm 옮겨 맞췄다([저장값](female-shopkeeper-preview-fit.json), 2026-11-16). 그 뒤 밑창이 판금 부츠보다 1.8mm 낮아 통째로 올렸고(`lower_shoe=-.0018`), 7mm 짧은 굽 끝은 예복 구두처럼 절반(3.6mm)만 늘렸다. 대기 자세 들림 0.0595m로 판금 부츠(0.0594m)와 같다.
- 상의: 코르셋 앞 아랫단이 피부와 1~3mm로 붙어 피부가 비쳤다. 사용자가 미리보기에서 앞으로 0.2cm, 앞뒤 기울임 -2°로 맞췄다(2026-11-16). 뒤 아랫단은 치마 주름 위를 덮는 모양이라 맨 허리에서 5cm쯤 뜨고, 치마를 입으면 벨트에 가려진다. 그대로 둔다.
- 소매 끝에서 팔 피부가 삼각형 단위로 가려져 톱니로 보였다. 장갑 입구처럼 소매 끝 1cm 안쪽 평면에서 피부를 자르고 그 위쪽 팔 피부를 숨긴다(`clip_sleeve`, 피팅 스크립트에 추가).
- 부츠만 신으면 윗단에 걸친 종아리 삼각형이 톱니로 보였다. `clip_hem`으로 피부를 윗단 평면에서 자른다(`preview-boots.png`).
- 부츠 통 뒤쪽 위에 치마 색이 묻은 붉은 줄이 있다. 치마를 입으면 가려진다.

![걷기·달리기](../../assets/items/female_shopkeeper_outfit/preview-walk-run.png)

![앞·뒤·옆](../../assets/items/female_shopkeeper_outfit/preview-back.png)

미리보기 `/modular-character-preview.html?character=female&top=shopkeeper&bottom=shopkeeper&boots=shopkeeper&hair=warrior`.

## 게임 연결

- 아이템 `shopkeeper_top`, `shopkeeper_pants`, `shopkeeper_boots`: 거래 불가, 가격·판매처 없음. 아이콘과 바닥 모델은 `export_outfit_items.py --outfit shopkeeper --texture-size 512`.
- `data-src/npcs.csv`의 Rica: 위 세 아이템 착용, 헤어 `long_wavy`([제작 기록](modular-female-long-wavy-hair.md)), 머리색 `#d9d5cd`, 기본 얼굴.
- `modelPaths.ts`에서 Rica의 `npc_woman.glb` 지정을 지웠다.
- v1 의상으로는 개발 서버에서 Rica가 가게에 서서 인사하는 것을 확인했다. v2로 바꾼 뒤에는 미리보기만 확인했다.

## 남은 일

- v2 의상의 게임 내 확인.
- 달릴 때 뒤로 찬 다리의 부츠가 치마 뒤를 뚫는다. 걸을 때 치마가 넓게 벌어진다.
- 수면·앉기 동작(밤에는 2층으로 올라가 확인하지 못했다), 거래 창 열기, 거래 창 초상화와의 차이(치마 색, 초커)는 확인하지 않았다.
- `assets.lock`·HF 미반영.

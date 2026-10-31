# 여성 원시전사 시작 장비

상태: 상의 v7과 하의 v9 피팅·리깅 후보를 미리보기에 연결했다. 일부 동작에서 관통이 남아 있으며 게임 적용은 완료되지 않았다.

여성 원시전사도 남성 원시전사와 같은 네 슬롯의 시작 장비를 사용한다.
외형은 [여성 원시전사 통합 원화](../../client/public/character_concepts/cavewoman.webp)를 기준으로 한다.
남성 복장의 부위 구분과 부츠는 [남성 복장 기록](modular-caveman-concept.md)을 참고한다.

| 슬롯 | 아이템 ID | 원화 기준 및 제작 범위 |
| --- | --- | --- |
| chest | `worn_caveman_top` | 가죽 상의와 뼈 장식. 목걸이의 소속과 뒷면 연결은 미확정. |
| pants | `worn_caveman_pants` | 벨트, 주머니, 앞뒤와 옆의 모피 하의. |
| hands | `worn_caveman_bracers` | 손가락이 노출되는 좌우 손목 보호대. |
| boots | `worn_caveman_boots` | 원화의 한쪽 다리 모피를 참고한 좌우 부츠. 원화는 맨발이므로 발등, 뒤꿈치, 밑창 디자인은 미확정. |

투구는 시작 장비에 포함하지 않는다. 실제 형상과 장식의 안팎 순서는 부위 원화에서 결정한다.
여성 공통 몸체와 리그는 [여성 몸체 기록](modular-human-female-01.md)을 따른다.
[공통 제작 절차](modular-outfit-workflow.md)와 [연결 기준](modular-outfit-connections.md)을 적용하되,
남성 전용 기준 단면이나 피팅 수치를 여성 몸체에 그대로 적용하지 않는다.

## 현재 상태와 남은 작업

- 기존 여성 원시전사는 통합 모델을 사용하며 신규 캐릭터에 위 네 장비를 지급하지 않는다.
- 상의와 하의의 동작 중 관통 보정, 나머지 파츠 원화·3D 생성 및 연결부 검증이 필요하다.
- 같은 세트와 다른 장비 혼합, 장착·해제, 동작 중 관통, 게임 성능을 검증해야 한다.
- 검증된 파츠가 준비된 후 여성 캐릭터의 모듈형 모델 선택, 시작 장비 지급과 게임 연결을 구현한다.
- 기존 여성 캐릭터에 대한 소급 지급은 범위와 데이터 처리 방법이 아직 정해지지 않았다.

## 참조 및 생성 시도

- 여성 참조: `client/public/character_concepts/cavewoman.webp`, SHA-256 `0d20c8774aa138edcb8892a93d128471bdac2b3ac2ef23e08f39a24a19a1168d`.
  출처와 이용 조건은 [캐릭터 기록](characters.md#other-classes)을 따른다.
- 남성 참조: `doc/images/characters/modular_human_male_01/concepts/caveman-outfit-v2.png`, SHA-256 `480c7421f68e79706eea86bc614cfe4631de0b66c5cc0c0f3a37f30b1042fead`.
  출처와 이용 조건은 [남성 원화 기록](modular-caveman-concept.md)을 따른다.
- 2026-11-01 OpenAI Codex 내장 ImageGen, ChatGPT Pro 20x로 여성 참조를 바탕으로 앞뒤 복장 원화를 요청했으나 안전 필터가 출력 생성을 거절했다. 생성된 이미지는 없다.
- 같은 날 사용자 요청에 따라 기존 여성 원화에서 상의 부분만 잘라 상의 단품의 앞·뒤 원화 초안을 생성했다. 뒷면 끈 여밈은 원화에 없는 제안이다. 목걸이는 이번 초안에 포함하지 않았고 장비 소속은 미확정이다.
  - 입력: FFmpeg `crop=390:260:320:325`, [참조 크롭](../../assets/items/female_caveman_top/concept_v1/reference-crop.png), SHA-256 `be24a165c086a1a7a0aed1fb849f4036db3d821c924977ca668dadbfa71a3917`.
  - 출력: [상의 원화 초안](../../assets/items/female_caveman_top/concept_v1/sheet.png), SHA-256 `19f6a79a29d03ab6d59eb77d27cbdfc6cfe8112c81e6f76f7f3a7c5e2c3899d1`.
  - 생성: OpenAI Codex 내장 ImageGen, **ChatGPT Pro 20x**, 2026-11-01. OpenAI 출력물 이용 조건 적용. [프롬프트](../../assets/items/female_caveman_top/concept_v1/prompt.txt).
  - 정면과 후면을 각각 `crop=887:887:0:0`, `crop=887:887:887:0`으로 분리했다.

## 하의 원화 초안 — 2026-11-01

- 기존 여성 원화에서 허리와 모피 하의 부분만 잘라 참조했다. 어두운 가죽 벨트와 장식, 주머니, 갈색 모피와 밝은 테두리를 갖춘 하의 단품을 앞·뒤에서 같은 크기로 표현했다. 원화에 보이지 않는 뒷면은 제안이며 확정 디자인이 아니다.
- 입력: FFmpeg `crop=400:390:315:590`, [하의 참조 크롭](../../assets/items/female_caveman_bottom/concept_v1/reference-crop.png), SHA-256 `b6241b5e54d4bd1b4bfdbedde8cc4e02ba76c0e0035a3d7f0769117646f7a418`.
- 출력: [하의 앞·뒤 원화 초안](../../assets/items/female_caveman_bottom/concept_v1/sheet.png), SHA-256 `a3870c9025b992e30193ecc0b14bddf29bf438f4698516dec752ca23ef446451`.
- 생성: OpenAI Codex 내장 ImageGen, **ChatGPT Pro 20x**, 2026-11-01. OpenAI 출력물 이용 조건 적용. [프롬프트](../../assets/items/female_caveman_bottom/concept_v1/prompt.txt).
- 3D 생성과 몸체 피팅·리깅은 아직 진행하지 않았다.

## 하의 3D 생성 — 2026-11-01

- 앞·뒤 원화 초안을 각각 `crop=768:1024:0:0`, `crop=768:1024:768:0`으로 분리했다. [앞면 입력](../../assets/items/female_caveman_bottom/tripo_v1/inputs/front.png)·[뒷면 입력](../../assets/items/female_caveman_bottom/tripo_v1/inputs/back.png), [제작 명세](../../assets/items/female_caveman_bottom/tripo_v1/build.json).
- Tripo API 유료 크레딧, 모델 `P2-20260801`, 상세 PBR 텍스처, 목표 3,000 triangles. 앞·뒤 이미지를 다중 시점 입력으로 한 번 제출했다. 작업 ID `760b6594-ef01-4753-ba8d-52667c043e1f`. [작업 기록](../../assets/items/female_caveman_bottom/tripo_v1/generation.json), [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- 생성에 120크레딧을 사용했고 잔액은 560 → 440크레딧이다. [원본 GLB](../../assets/items/female_caveman_bottom/tripo_v1/source/model_url.glb), [Tripo 미리보기](../../assets/items/female_caveman_bottom/tripo_v1/source/rendered_image_url.png), [Blender 원본](../../assets/items/female_caveman_bottom/tripo_v1/source.blend).
- 원본은 2,564 triangles, 재질 1개, 본과 애니메이션이 없는 메시다. 앞면의 벨트와 주머니, 모피 패널이 생성됐으며 뒷면의 밝은 모피 테두리는 원화보다 흐리다. 생성 원본의 여성 몸체 피팅·리깅과 동작 검증 결과는 아래에 기록한다. 게임 연결은 아직 진행하지 않았다.

## 하의 피팅·리깅 후보 v5 — 2026-11-01

- 생성 메시를 여성 공통 몸체의 허리와 골반 크기에 맞추고 가죽 벨트와 모피 패널, 장식을 함께 배치했다. 허리에는 골반·척추, 아래쪽 모피에는 골반과 양쪽 허벅지 본의 가중치를 적용했다. 단위는 미터다.
- 중간 피팅본과 검토용 렌더는 정리하고 생성 원본과 최종 피팅본을 보존한다.
- 2,564 triangles와 기존 65본을 사용하며, 몸체와 동일한 본 계층·기준 변환·역바인드 및 정규화된 가중치를 확인했다. 9개 동작의 각 13개 자세에서 유한 변형과 피부 표면 간 거리를 표본 검사했다. 허벅지가 크게 움직이는 점프·달리기·앉기에서 모피와 다리의 겹침 후보가 남는다. 움직이는 의복에 대한 후속 보정이 필요하며, 최근접 표면 근사값만으로 실제 충돌 범위 전체를 확정할 수 없다.
- 미리보기의 하의 메뉴에서 `원시전사 모피 하의 (피팅 검수 중)`를 고를 수 있다. [가림 정보](../../assets/items/female_caveman_bottom/tripo_v1/fitted_v9/body-occlusion.json)는 하체 속옷 1,110삼각형만 숨기고 피부를 보존한다. 게임 장비에는 아직 연결하지 않았다.

```sh
blender -b -t 4 --python-exit-code 1 --python tools/blender-scripts/fit_female_caveman_bottom.py
blender -b -t 4 --python-exit-code 1 --python tools/blender-scripts/validate_female_caveman_bottom.py
.venv/bin/python tools/outfits/female_caveman_bottom_occlusion.py
```

## 하의 허리 피팅 조정 v9 — 2026-11-01

- 사용자 피드백에 따라 허리 둘레를 좁히고, 피부에 겹치는 바깥쪽 허리띠 면을 바깥으로 보정했다. 벨트 위쪽의 리그 가중치를 인접한 몸체에서 전사해 대기와 걷기에서도 몸체를 따르게 했다.
- [피팅 GLB](../../assets/items/female_caveman_bottom/tripo_v1/fitted_v9/bottom_female_caveman.glb), [Blender 파일](../../assets/items/female_caveman_bottom/tripo_v1/fitted_v9/female-caveman-bottom.blend), [동작 검사](../../assets/items/female_caveman_bottom/tripo_v1/fitted_v9/animation-validation.json). 기준 자세에서 뒤쪽 허리의 동일 정점 107개를 몸통 표면에 광선 투사해 비교한 결과 방사 방향 간격 중앙값은 약 31.7mm에서 15.9mm로 줄었다. 표본 간격이며 옷 전체의 최솟값이 아니다.
- 몸체에 구워진 밝은 속옷 무늬는 원시전사 하의를 착용한 미리보기에서만 피부색에 맞춰 표시한다. 하의 장착을 해제하거나 다른 하의를 선택하면 원래 표시로 복원한다. 분리된 하체 속옷 메시 가림은 유지하며 피부 면은 숨기지 않는다.
- 9개 동작 각 13개 표본에서 본 계층·역바인드·가중치와 유한 변형을 확인했다. 대기·걷기에서 3mm 이상 피부 겹침 표본의 최대 개수는 각각 116 → 80개, 183 → 144개였다. 근사 표본 검사이며 점프·앉기 등 큰 동작의 모피·다리 관통과 벨트 변형은 후속 보정이 필요하다.

## 상의 3D 생성 — 2026-11-01

- Tripo API 유료 크레딧, 모델 `P2-20260801`, 텍스처 `v3.5-20260815`, detailed PBR, 목표 3,000 triangles. 앞·뒤 이미지를 다중 시점 입력으로 제출했다.
- 작업 ID `9a8e90e5-faa9-4b26-a0eb-b84a88c65e8a`, 비용 120크레딧(잔액 680 → 560).
- [제작 명세](../../assets/items/female_caveman_top/tripo_v1/build.json), [요청·작업 기록](../../assets/items/female_caveman_top/tripo_v1/generation.json), [앞](../../assets/items/female_caveman_top/tripo_v1/inputs/front.png)·[뒤](../../assets/items/female_caveman_top/tripo_v1/inputs/back.png) 입력. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- [생성 GLB](../../assets/items/female_caveman_top/tripo_v1/source/model_url.glb), [Tripo 미리보기](../../assets/items/female_caveman_top/tripo_v1/source/rendered_image_url.png).
- 원본은 2,996 triangles, 리그 없음, 4K color·normal·ORM 텍스처다. 진단상 열린 경계가 218개이며 퇴화 면과 붕괴 UV는 0이다. 이 수치만으로 착용에 적합한 두께·개구부를 보장하지 않는다.
- 앞면 장식은 생성됐지만 뒷면의 끈 여밈은 원화보다 흐리고 평평하게 표현됐다. 생성 결과는 피팅·리깅·게임 검증 전 원본이다.

## 상의 피팅·리깅 후보 v7 — 2026-11-01

Tripo 원본을 여성 공통 몸체의 실제 미터 크기에 맞추고 65본 가중치를 전사했다.
착용 검토에 따라 몸과 가죽 사이의 간격과 뒤쪽 높이를 줄이고, 앞뒤 뼈 장식을 가죽 표면에 맞췄다.
중앙 장식은 윗끝을 가죽 안쪽에 넣고 아래쪽이 몸통을 따르도록 했다.
피부가 앞면을 뚫고 나오던 부위는 가죽을 피부 법선 방향으로 최대 9mm 띄웠다.
중간 피팅본과 검토용 렌더는 삭제하고 생성 원본과 최종 피팅본을 보존한다.

- [피팅 GLB](../../assets/items/female_caveman_top/tripo_v1/fitted_v7/top_female_caveman.glb), [편집용 Blender](../../assets/items/female_caveman_top/tripo_v1/fitted_v7/female-caveman-top.blend), [피팅 기록](../../assets/items/female_caveman_top/tripo_v1/fitted_v7/fitting.json), [동작 검사](../../assets/items/female_caveman_top/tripo_v1/fitted_v7/animation-validation.json).
- 기준 몸체 SHA-256 `3eb7c0a5f3c6b30aec0fbae0b70c8fd925e4cb91772647420007eee795a1e0a5`. 상의는 2,996 triangles, 65본이며 본 계층·역바인드와 정규화된 가중치를 검사했다.
- 대기·무장 대기·걷기·달리기·점프·공격·앉기·전투 대기·쓰러짐을 각 13개 자세로 검사했다. 앞면 가죽 72개 면에서 면당 15개 지점을 표본 검사한 결과, 대기에서 1mm 이상 겹치는 표본은 없었고 공격에서 최대 1개, 약 1.01mm였다. 중앙 뼈 장식의 윗끝 중심은 검사한 모든 자세에서 가죽 안쪽에 유지됐다. 표본 기반 근사 검사이므로 모든 관통이 없다는 보장은 아니다. 다른 부위와 팔을 포함한 동작 중 겹침 가능성, 혼합 장비 검증과 실제 게임 적용은 남아 있다.

피팅과 미리보기 가림 정보 재생성:

```sh
blender -b -t 4 --python-exit-code 1 --python tools/blender-scripts/fit_female_caveman_top.py -- --animation-clearance
blender -b -t 4 --python-exit-code 1 --python tools/blender-scripts/validate_female_caveman_top.py
.venv/bin/python tools/outfits/female_caveman_top_occlusion.py
```

### 여성 캐릭터 미리보기

`/modular-character-preview.html?character=female&top=caveman&camera=upper`에서 상의 후보를 착용한다.
상의 선택 메뉴에서도 `원시전사 상의 (피팅 검수 중)`를 고를 수 있다.
[미리보기 전용 가림 정보](../../assets/items/female_caveman_top/tripo_v1/fitted_v7/body-occlusion.json)는
기준 몸체의 상체 속옷 2,488삼각형을 숨기고 피부는 유지한다. 장비를 벗거나 바꾸면 기존 속옷을 복원한다.
가림 정보는 정확한 몸체와 상의 해시를 기록한다. 게임 모델이나 시작 장비에는 아직 연결하지 않았다.

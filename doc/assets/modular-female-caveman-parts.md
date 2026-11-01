# 여성 원시전사 시작 장비

상태: 상의 v7, 하의 v9, 부츠 v4, 손목 보호대 v1 피팅·리깅 후보를 미리보기에 연결했다. 일부 동작에서 관통이 남아 있으며 게임 적용은 완료되지 않았다.

여성 원시전사도 남성 원시전사와 같은 네 슬롯의 시작 장비를 사용한다.
외형은 [여성 원시전사 통합 원화](../../client/public/character_concepts/cavewoman.webp)를 기준으로 한다.
남성 복장의 부위 구분과 부츠는 [남성 복장 기록](modular-caveman-concept.md)을 참고한다.

| 슬롯 | 아이템 ID | 원화 기준 및 제작 범위 |
| --- | --- | --- |
| chest | `worn_caveman_top` | 가죽 상의와 뼈 장식. 목걸이의 소속과 뒷면 연결은 미확정. |
| pants | `worn_caveman_pants` | 벨트, 주머니, 앞뒤와 옆의 모피 하의. |
| hands | `worn_caveman_bracers` | 손가락이 노출되는 좌우 가죽 손목 보호대. 미리보기 피팅 검수 중. |
| boots | `worn_caveman_boots` | 종아리 모피를 참고한 두 후보 중 하이힐 모피 부츠 원화를 선택했다. |

투구는 시작 장비에 포함하지 않는다. 실제 형상과 장식의 안팎 순서는 부위 원화에서 결정한다.
여성 공통 몸체와 리그는 [여성 몸체 기록](modular-human-female-01.md)을 따른다.
[공통 제작 절차](modular-outfit-workflow.md)와 [연결 기준](modular-outfit-connections.md)을 적용하되,
남성 전용 기준 단면이나 피팅 수치를 여성 몸체에 그대로 적용하지 않는다.

## 현재 상태와 남은 작업

- 기존 여성 원시전사는 통합 모델을 사용하며 신규 캐릭터에 위 네 장비를 지급하지 않는다.
- 상의와 하의의 동작 중 관통 보정, 손목 보호대 동작·다른 장비와의 연결부 검증이 필요하다.
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

## 손목 보호대 원화 초안 — 2026-11-01

- 여성 원시전사 통합 원화의 양쪽 손목 보호대만 잘라 참고해 가죽과 감은 끈, 늘어진 끈을 살린 보호대 단품의 바깥쪽·안쪽 초안을 그렸다. 뒤쪽 끈의 세부 배치는 원화에서 확인되지 않아 제안한 형태다.
- 참조: FFmpeg `crop=160:160:70:330,scale=960:960:flags=lanczos`, [이미지 왼쪽 손목](../../assets/items/female_caveman_bracers/concept_v1/reference-wrist-left.png), SHA-256 `745fadbeb06f8844240276deeaaf8b0635380671e9bd2acd202943decd7dadfe`; `crop=160:160:805:330,scale=960:960:flags=lanczos`, [이미지 오른쪽 손목](../../assets/items/female_caveman_bracers/concept_v1/reference-wrist-right.png), SHA-256 `547ebbf78694f0f3edeecbfb2071ac02879e321d74943b3da27bb16fb13b2f53`. 원본 출처와 이용 조건은 [캐릭터 기록](characters.md#other-classes)을 따른다.
- 출력: [보호대 원화](../../assets/items/female_caveman_bracers/concept_v1/sheet.png), SHA-256 `239703ba889325500a2ed9615cf7f6133b01513ef598b96b9a85d354a6f55f28`. OpenAI Codex 내장 ImageGen, **ChatGPT Pro 20x**, 2026-11-01 생성. OpenAI 출력물 이용 조건 적용. [프롬프트](../../assets/items/female_caveman_bracers/concept_v1/prompt.txt).
- 사용자 요청으로 합본과 손목 크롭을 참조해 [앞면](../../assets/items/female_caveman_bracers/concept_v1/front.png)을 따로 그렸고, 앞면과 합본을 참조해 [뒷면](../../assets/items/female_caveman_bracers/concept_v1/back.png)을 그렸다. 각각 SHA-256 `1c664690cc9ad6162dfb8b7667117778123b4bfe515a0275373fc5491e0ce7d8`, `dafb382553c6b7428eafedca1ddb3a4ab38bf4774f7ce11237c5081d2efc9a73`. OpenAI Codex 내장 ImageGen, **ChatGPT Pro 20x**, 2026-11-01 생성. OpenAI 출력물 이용 조건 적용. [앞면 프롬프트](../../assets/items/female_caveman_bracers/concept_v1/prompt-front.txt), [뒷면 프롬프트](../../assets/items/female_caveman_bracers/concept_v1/prompt-back.txt). 두 시점은 각각 생성한 2D 원화로, 끈 연결과 형상의 정확한 일치는 3D 제작 시 확인해야 한다.

## 손목 보호대 3D와 피팅 후보 — 2026-11-01

- 사용자가 Tripo에서 만든 [한쪽 원본 GLB](../../assets/items/female_caveman_bracers/tripo_v1/source/model_url.glb)를 전달했다. SHA-256 `4272c6f0eedf3057f69de43e5dd80fca8718b105333f9b2181958adf0a4a6351`, 1,600 triangles. 정확한 생성 날짜, 요금제, 입력 이미지, 설정과 비용은 확인되지 않았다. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- 원본의 막힌 안쪽 면 25개를 각 짝에서 제거하고, 늘어진 끈을 포함한 나머지 메시를 양팔에 맞췄다. 한쪽 1,575개, 양쪽 합계 3,150 triangles다. UV와 원본 텍스처를 보존하고 각 팔의 `ForeArm` 본에 연결했다. [피팅 GLB](../../assets/items/female_caveman_bracers/tripo_v1/fitted_v1/gloves_female_caveman.glb), [Blender 편집 파일](../../assets/items/female_caveman_bracers/tripo_v1/fitted_v1/female-caveman-bracers-fitted.blend), [피팅 기록](modular-female-caveman-bracers-fitting-v1.json).
- 손은 유지하고 보호대 안쪽 팔뚝 피부 204면만 가린다. [가림 정보](../../assets/items/female_caveman_bracers/tripo_v1/fitted_v1/body-occlusion.json)는 해제 시 원래 피부를 복원한다. 기본 자세에서 각 보호대 면당 7개 표본을 최근접 피부 면과 비교했을 때 보이는 피부 쪽 1mm 초과 겹침 표본은 없었다.
- [동작 검사](../../assets/items/female_caveman_bracers/tripo_v1/fitted_v1/animation-validation.json)는 14개 동작 각 33시점에서 본 연결과 피부 가림 복원을 확인했다. 광선 패리티 검사에서는 팔과의 겹침 후보가 모든 동작에서, 허벅지와의 후보가 걷기 등에서 검출됐다. 열린 메시의 광선 패리티와 최근접 면 표본은 근사 검사이며 실제 관통의 유무나 모든 장비 조합을 보장하지 않는다.
- 미리보기 신발과 별도로 장갑 메뉴의 `원시전사 가죽 손목 보호대 (피팅 검수 중)`에서 볼 수 있다. 게임 장비 연결은 아직 하지 않았다.

```sh
.venv/bin/python tools/fit-female-caveman-bracers.py
node tools/validate-female-caveman-bracers.mjs
.venv/bin/python tools/validate-female-caveman-bracer-clearance.py
blender -b -t 4 --python-exit-code 1 --python tools/blender-scripts/review_female_caveman_bracers.py
```

## 신발 원화 후보 두 가지 — 2026-11-01

기존 여성 원시전사 원화의 한쪽 종아리 모피와 맨발 부위를 잘라 참조했다. 양발에 사용할 신발로 두 가지를 제안했고, 사용자가 후보 2를 선택했다. 3D 제작과 피팅은 아래에 기록한다.

- 공통 참조: FFmpeg `crop=220:440:300:1030`, [종아리 모피 크롭](../../assets/items/female_caveman_boots/concept_v1/reference-crop.png), SHA-256 `a40769bf8c863930556ecedc728dc81e7c1b564fa2b75716d63c1e6f83bae25f`. 기존 여성 원화의 출처와 이용 조건은 [캐릭터 기록](characters.md#other-classes)을 따른다.
- 후보 1: 기존 야만전사 하이힐 샌들에 종아리 모피를 덧댄 [앞·옆·뒤 원화](../../assets/items/female_caveman_boots/concept_v1/sandal-fur.png), SHA-256 `b452cd6573fc7c0d29b90ee291b0068bc13341ff3e960aa4fed6cad87d464f40`. 기존 [야만전사 샌들 원화](../../assets/items/female_barbarian_boots/concept_v1/sheet.png)를 추가 참조했다. [프롬프트](../../assets/items/female_caveman_boots/concept_v1/prompt-sandal.txt).
- 후보 2: 발끝부터 종아리까지 덮는 하이힐 모피 부츠의 [앞·옆·뒤 원화](../../assets/items/female_caveman_boots/concept_v1/fur-boot.png), SHA-256 `3419417175e6df416fbcf53c6b8685703b1e902c1aee432aeafac393f4026b93`. [프롬프트](../../assets/items/female_caveman_boots/concept_v1/prompt-boot.txt).
- 두 원화는 OpenAI Codex 내장 ImageGen, **ChatGPT Pro 20x**, 2026-11-01 생성이며 OpenAI 출력물 이용 조건을 따른다.
- 선택한 후보 2의 원본 시트를 FFmpeg로 분리했다: [앞](../../assets/items/female_caveman_boots/concept_v1/fur-boot-views/front.png) `crop=464:1024:0:0`, [옆](../../assets/items/female_caveman_boots/concept_v1/fur-boot-views/side.png) `crop=640:1024:464:0`, [뒤](../../assets/items/female_caveman_boots/concept_v1/fur-boot-views/back.png) `crop=432:1024:1104:0`. 원본 시트는 보존한다.
- 사용자 피드백에 따라 옆면의 버클과 끈을 앞쪽 가장자리로 옮기고 옆·뒷면 굽을 야만전사 샌들처럼 가는 스틸레토로 수정했다. 현재 원화는 기존 [앞면](../../assets/items/female_caveman_boots/concept_v1/fur-boot-views/front.png), [수정 옆면](../../assets/items/female_caveman_boots/concept_v1/fur-boot-views/side-v2.png), [수정 뒷면](../../assets/items/female_caveman_boots/concept_v1/fur-boot-views/back-v2.png)이다. 각 이미지는 독립적인 2D 원화이므로 3D 제작 시 시점 간 형상을 확인해야 한다.
- 수정 생성: OpenAI Codex 내장 ImageGen, **ChatGPT Pro 20x**, 2026-11-01. OpenAI 출력물 이용 조건 적용. [옆면 프롬프트](../../assets/items/female_caveman_boots/concept_v1/prompt-side-v2.txt), [뒷면 프롬프트](../../assets/items/female_caveman_boots/concept_v1/prompt-back-v2.txt).
- 수정 입력 SHA-256: 기존 앞면 `ab452d0be226cf3407ce2a73f5622804014fc98aa24fe2a0dd74bb89d0ce8937`, 기존 옆면 `cd600ac7e55ccc84cf3e10d542ddfadd0ee86b8151f1fb1b3f152e64d2ea3bf8`, 기존 뒷면 `eea5af720ec789d27bf323a9bbe31657a8aeb553d49c1a8774d845148624e03b`, [야만전사 샌들 원화](../../assets/items/female_barbarian_boots/concept_v1/sheet.png) `c7ce930968e26c4f983e19ad0772dbb03d2b3b4cd1eddcc8f7a10c0363863ae7`. 수정 옆면은 뒷면 수정에도 참조했다.
- 수정 출력 SHA-256: 옆면 `3db6eeebfec57749a2503a6ecb44466ca9e6158a67f24ad11618726a412474b4`, 뒷면 `e2002c4ec897273ebd31aba59ea3fd7b0839e25375b385b809cec07c16dd615a`.

## 신발 3D 및 초기 피팅 — 2026-11-01

- 사용자가 Tripo에서 생성한 한 짝 GLB를 2026-11-01에 전달했다. 생성 날짜, 이용 요금제, 정확한 생성 모델, 입력 이미지, 설정, 작업 ID와 비용은 확인되지 않았다. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms). [원본 GLB](../../assets/items/female_caveman_boots/tripo_v1/source/model_url.glb), SHA-256 `8f88cd3c636b86d7da24c8efced835ace5182c57bcc89682e10d62a2ba24614f`. 원본은 1,520 triangles다.
- 원본을 대칭하고 양쪽 종아리에 각각 맞춘 뒤 여성 공통 몸체의 65본에 연결했다. 사용자 피드백에 따라 종아리 둘레를 좁혔다. 높이 0.43m 단면에서 피부와 부츠 표면의 거리 중앙값은 오른쪽 약 13.6 → 8.4mm, 왼쪽 약 14.4 → 7.9mm였다. 모피 두께가 포함된 방사형 표본이며 전체 간격을 뜻하지 않는다.
- 중간 피팅본과 검수 렌더는 정리하고 사용자 수정의 기준인 v3, 사용자가 수정한 원본, 최종 v4는 보존한다. 미리보기의 신발 메뉴에서 확인할 수 있으며 게임 장비에는 아직 연결하지 않았다.

## 신발 발 부분 피팅 조정 v3 — 2026-11-01

- 사용자 피드백에 따라 종아리 윗단의 v2 피팅은 유지하고 발 부분의 폭을 12%, 길이를 10% 줄인 뒤 앞뒤 위치를 조정했다. [피팅 GLB](../../assets/items/female_caveman_boots/tripo_v1/fitted_v3/boots_female_caveman.glb), [편집용 Blender](../../assets/items/female_caveman_boots/tripo_v1/fitted_v3/female-caveman-boots-fitted.blend), [가림 정보](../../assets/items/female_caveman_boots/tripo_v1/fitted_v3/body-occlusion.json).
- 굽 기준 자세에서 오른쪽 부츠의 발 부분과 굽을 함께 측정한 투영 바운딩 폭·길이는 v2 약 134.3×265.1mm, v3 약 118.2×238.6mm다. 같은 방법으로 측정한 기존 야만전사 샌들은 약 115.8×241.3mm다. 단면이 아닌 전체 외곽 범위이므로 피부와의 간격은 나타내지 않는다.
- 14개 동작 각 61시점에서 본 연결, 가중치, 발바닥 변형과 접지를 확인했다. 윗단 표본 검사에서 보이는 피부가 1mm 넘게 겹친 표본은 없었다. 발 부분은 착용 시 몸체를 가린다. 발 표면의 최근접 면 부호 거리 진단에서 피부와 부츠의 겹침 후보가 있으며, 이 근사값으로 전체 실제 관통을 확정할 수 없다.

```sh
.venv/bin/python tools/fit-female-caveman-boots-v3.py
node tools/validate-female-caveman-boots.mjs assets/items/female_caveman_boots/tripo_v1/fitted_v3
blender -b -t 4 --python-exit-code 1 --python tools/blender-scripts/validate_female_caveman_boots_v3.py
blender -b -t 4 --python-exit-code 1 --python tools/blender-scripts/review_female_caveman_boots_v3.py
```

## 사용자 메시 수정 반영 v4 — 2026-11-01

- 사용자가 PC4090의 Blender에서 오른쪽 부츠를 직접 수정했다. [저장한 원본 Blender](../../assets/items/female_caveman_boots/manual_edit_v1/user-edited.blend), SHA-256 `2f2914add8ef3117466745670b85d1c1ec8515e13f1c4cb5d41941fee0ca28cc`. 편집 모드로 저장된 파일을 별도로 읽어 변경 사항을 추출했다.
- 변경된 오른쪽 정점 18개의 위치를 반영하고, 하이힐 기준 자세에서 변경량을 대칭한 뒤 왼쪽 기존 리그에 역변환해 적용했다. 겹쳐 있던 정점의 수정량도 일치한다. 원본 UV·텍스처·토폴로지·가중치·모프는 유지하고 변경 부근의 노멀을 갱신했다. [피팅 GLB](../../assets/items/female_caveman_boots/tripo_v1/fitted_v4/boots_female_caveman.glb), [편집용 Blender](../../assets/items/female_caveman_boots/tripo_v1/fitted_v4/female-caveman-boots-fitted.blend), [제작 기록](../../assets/items/female_caveman_boots/tripo_v1/fitted_v4/fitting.json).
- [동작 검사](../../assets/items/female_caveman_boots/tripo_v1/fitted_v4/runtime-validation.json)는 14개 동작 각 61시점의 본 연결, 가중치, 밑창 변형과 접지를 확인했다. [윗단 표본 검사](../../assets/items/female_caveman_boots/tripo_v1/fitted_v4/clearance-validation.json)에서 보이는 피부와 1mm 넘게 겹친 표본은 없었다. v3의 가려진 발 피부와 부츠 간 겹침 가능성은 그대로 남는다.

```sh
blender -b -t 2 --python-exit-code 1 --python tools/blender-scripts/extract_female_caveman_boot_edit.py
.venv/bin/python tools/apply-female-caveman-boot-edit.py
node tools/validate-female-caveman-boots.mjs assets/items/female_caveman_boots/tripo_v1/fitted_v4
blender -b -t 4 --python-exit-code 1 --python tools/blender-scripts/validate_female_caveman_boots_v4.py
blender -b -t 4 --python-exit-code 1 --python tools/blender-scripts/review_female_caveman_boots_v4.py
```

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

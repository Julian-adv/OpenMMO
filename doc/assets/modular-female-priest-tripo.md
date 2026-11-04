# 여성 사제 Tripo 복장

외형 기준은 [여성 사제 원화](../../client/public/character_concepts/female_priest.webp)다.
여성 공통 몸체는 [여성 몸체 기록](modular-human-female-01.md),
제작 절차는 [공통 워크플로우](modular-outfit-workflow.md)와 [연결 기준](modular-outfit-connections.md)을 따른다.

## 원본 — 2026-11-04

- 사용자가 Tripo에서 만들고 직접 세그먼트한 `Y:\public\web_downloads\fantasy armor character 3d model.glb`(2026-11-04 01:16).
  [보관본](../../assets/items/female_priest_outfit/tripo_segmented_v1/original/model.glb),
  SHA-256 `fa0efcf56621c6baa7de70e4317d5bb14bf97a48e3c3e46b8413a033b5c5f13a`.
  생성일 외 요금제·작업 ID·설정은 전달되지 않았다. [Tripo 이용 조건](https://developers.tripo3d.ai/en/terms).
- 높이 약 1.0m, 메시 노드 53개, 20,096 triangles, 리그·애니메이션 없음.
- 사용자 세그먼트: `helmet` 700, `top` 5,624, `skirt` 4,008, `left_glove` 1,417, `left_boots`·`right_boots` 각 1,343.
  나머지 `tripo_part_*` 46개는 얼굴·눈·속눈썹·노출 피부로 참고용이다.

## 오른쪽 장갑 분리

원본의 `tripo_part_25`에 오른쪽 장갑, 눈 하나, 견갑 조각이 함께 있었다.
삼각형 중심 높이로 나눴다: 0.85m 초과는 눈, 0.66m 초과는 상의 조각, 나머지는 장갑.

| 노드 | Triangles |
| --- | ---: |
| `right_glove` | 1,387 |
| `tripo_part_25_eye` | 168 |
| `tripo_part_25_top_fragment` | 2 |

정점·UV·텍스처는 원본 접근자를 그대로 공유하고 인덱스만 나눴다. 오른쪽 장갑의 경계 상자는 왼쪽 장갑과 좌우 대칭으로 같다.
[결과](../../assets/items/female_priest_outfit/tripo_segmented_v1/segmented.glb),
SHA-256 `2baf723d6fad2117159e6e369032711808f7a9a17b95b7819ffefbca7ae21bfe`.

```sh
.venv/bin/python tools/split-female-priest-right-glove.py \
  assets/items/female_priest_outfit/tripo_segmented_v1/original/model.glb \
  assets/items/female_priest_outfit/tripo_segmented_v1/segmented.glb
```

## 피팅 v1 — 2026-11-04

생성 몸체가 1.8배에서 여성 공통 몸체와 거의 겹친다. 위팔·몸통·다리 차이는 10mm 안팎이고, 팔꿈치 아래만 앞으로 15–21° 굽어 장갑이 20–33mm 어긋났다.

- 자세: 1.8배 균일 확대 후 팔꿈치 기준 아래팔 회전과 손 ICP 정렬([female_priest_pose.py](../../tools/outfits/female_priest_pose.py)). 장갑 중앙 거리는 3.4mm가 됐다.
- 피팅: 사용자 결정(2026-11-04)으로 어느 부위도 피부 밖으로 밀어내지 않는다. 생성 형상을 그대로 두고 크기 차이는 미리보기 조절로 맞춘다. 가장 가까운 몸체 피부의 가중치를 이웃 정점과 평균해 옮겼다. 몸체는 미리보기가 쓰는 glTF primitive 0(피부)이다.
- 장갑은 손 ICP 변환 하나로 커프까지 통째로 옮긴다. 손목에서 아래팔·손 변환을 섞으면 커프 위아래가 다르게 움직여 꼬였다.
- 팔 정점은 가장 가까운 피부의 팔·손 가중치로 고른다. x 좌표로 고르면 장화·치마 가장자리까지 팔로 돌아갔다.
- 가림: 법선 방향 6cm 안에 장비가 있거나 4cm 안쪽에 장비가 있는(장비 밖으로 튀어나온) 피부를, 세 꼭짓점과 중심이 모두 해당할 때 가린다. 침식은 튀어나온 피부 조각을 남겨서 쓰지 않는다. 장갑은 손 전체, 상의는 위팔·아래팔 피부와 상체 속옷을 가린다. 상의는 가린 피부 중 바깥에서 보이는 것(법선과 72° 안의 광선 하나라도 장비에 안 막히고, 장비 앞면보다 앞에 있지 않은 것)을 되살린다. 열린 깃 아래 목 밑이 구멍처럼 비어 보였다. 장비 바로 아래(6mm 안) 피부는 되살리지 않는다(738개 복원).
장화 외의 부위는 idle1·walk·run·combat_idle·slash1 각 6프레임에서 몸과 장비를 함께 움직여, 정지 상태에서 장비 아래이거나 장비 면 1cm 앞에 있던 피부가 장비 앞으로 나오면 가린다(상의 153개). 대기 동작에서 가슴 옆 피부가 비치던 것을 막는다. 정지 상태에서 드러난 피부까지 넣으면 다른 프레임에 구멍이 났고, 주변 한 겹까지 넓히면 가슴골 가장자리에 틈이 생겼다.
- 손목·등 안쪽의 작은 조각(`tripo_part_10/15/18/23/33/36`)은 장비에 가려 보이지 않아 제외했다.

| 부위 | 출력 | Triangles | 가린 몸체 삼각형 |
| --- | --- | ---: | ---: |
| 사제관 | `helmet/fitted_v1/helmet_female_priest.glb` | 700 | 537 |
| 상의 | `top/fitted_v1/top_female_priest.glb` | 5,626 | 5,715 |
| 치마 | `bottom/fitted_v1/bottom_female_priest.glb` | 4,008 | 1,282 |
| 장갑 | `gloves/fitted_v1/gloves_female_priest.glb` | 2,804 | 3,909 |
| 장화 | `boots/fitted_v1/boots_female_priest.glb` | 2,686 | 3,345 |

다섯 부위 합계 15,824 triangles이고, 몸체는 따로다.

장화는 발키리 부츠처럼 발 자세를 GLB에 담는다. 공통 30° `heel_pose`(원시전사·로그·야만전사·기사 부츠와 같은 굽는 동작 변형)로 자세를 잡은 몸체에 맞추고, 가중치를 옮긴 뒤 자세 행렬의 역으로 바인드 공간에 되돌린다.
생성 장화는 발목 높이가 평발 몸체와 같다. 0.18m 아래 구두 부분은 형태 그대로 4.9cm 내려 자세 잡은 발바닥에 맞추고, 0.18–0.32m 정강이 관만 늘였다. 구두까지 늘이고 밀어내면 발 아치가 찌그러졌다.
상의는 사용자 값(크기 105%, 높이 109%, 너비 101.5%, 두께 105.5%, 앞뒤 기울기 4.5°, 앞뒤 −0.5cm)을 구운 뒤 소매를 팔에 맞춘다(`PARTS` 상의의 `align_sleeves`, 2026-11-04).
어깨–팔꿈치–손목 본을 축으로 어깨 아래 9cm·팔꿈치·손목 2cm 앞 세 곳에서 ±3cm 안의 소매와 몸체 팔 피부 단면을 비교하고, 그 사이는 직선 보간해 소매 정점을 옮긴다. 단면 중심은 팔에 수직한 두 축에서 범위의 가운데를 쓴다(정점 평균은 촘촘한 쪽으로 쏠렸다).
어깨는 앞뒤로만 옮기고(옆으로 옮기면 견갑 아래가 벌어졌다), 팔꿈치·손목은 팔 길이 방향을 뺀 전후좌우 모두 맞춘다. 어깨에서 3–9cm 사이에서 점차 적용해 견갑은 그대로 둔다.
2cm마다 맞추면 소매가 구불구불해졌다. 이동(mm, x·y·z, y +가 뒤): 왼쪽 어깨 (0, 10.6, 0)·팔꿈치 (−2.2, −5.2, −1.6)·손목 (8.3, −43.5, 10.5), 오른쪽 (0, −0.9, 0)·(1.0, −21.7, 1.6)·(−13.5, −44.3, 22.7).
소매 끝은 장갑 커프 안 1cm에서 평면으로 잘라 새 정점을 넣는다. 커프 위 가장자리가 팔 축에 26° 기울어 있어 평면을 커프 테두리 점들에 맞춘다. 팔에 수직으로 자르면 커프가 낮은 쪽에서 피부가 보이거나 높은 쪽에 원래의 들쭉날쭉한 끝이 남았다. 잘린 끝 너머 피부는 장갑을 벗으면 맨살이라 가리지 않고 소매 안쪽 6mm까지 보이게 둔다(1cm면 커프 위로 비쳤다). 피부색 끝단도 함께 잘려 나갔다.
상의 아랫단은 텍스처에 그려진 갈색 벨트 띠의 윗선 1cm 아래로 자른다(사용자 요청; 5mm는 피부 띠, 1.5cm는 갈색 선이 보였다). 띠 위치는 아랫단 삼각형마다 45점씩 텍스처를 읽어 찾고(정점에서만 읽으면 큰 삼각형 위의 띠를 놓쳤다), 허리 둘레 18방향의 높이(앞 1.11m 안팎, 뒤 1.05m 안팎)를 이은 곡선을 평면으로 펴서 자른 뒤 되돌린다. 잘린 선 아래 피부는 치마를 벗으면 맨살이라 가리지 않는다. 치마의 벨트가 잘린 선을 덮는다.
장화는 사용자가 미리보기에서 맞춘 값(앞뒤 왼쪽 −1.8cm·오른쪽 −1.1cm, 너비 103%, 두께 104%)을 구워 완성했다(2026-11-04).
장화 안의 정강이·발·발가락 피부는 모두 가린다. 무릎은 허벅지·정강이 가중치가 섞여 광선 검사가 조각을 남겼으므로, 각 다리에서 장화 입구의 가장 낮은 높이(0.727m)보다 1cm 아래의 다리 피부를 모두 가린다.

밀어내기 없이 남은 차이(2026-11-04 미리보기): 공통 몸체가 생성 몸체보다 큰 곳에서 가슴 일부가 상의 밖으로 비치고, 깃이 목 안에 묻히며, 정수리가 사제관 위로 나온다.
수치는 각 `fitting.json`에 있다.

```sh
blender -b --python-exit-code 1 --python tools/blender-scripts/fit_female_priest.py -- --part top
blender -b --python tools/blender-scripts/review_female_priest.py -- --parts top bottom gloves helmet --out /tmp/review --views torso --clip walk:8
```

### 미리보기 수동 조절

여성 미리보기(`?character=female&top=priest&bottom=priest&gloves=priest&helmet=priest`)에서 사제 부위를 고르면 `사제 복장 피팅 조절`이 나온다.
이동·크기·너비/높이/두께·회전을 조절하고 `이 부위 피팅 저장`을 누르면 `doc/assets/female-priest-preview-fit.json`의 `pending`에 저장된다.
장갑·부츠는 `조절할 쪽`에서 양쪽 함께(대칭)·캐릭터 왼쪽만·오른쪽만을 고른다. 저장값은 `{left, right}`이고, 예전의 한 값은 양쪽에 같게 적용한다. 좌우 이동은 바깥쪽이 +다. 마우스 휠 한 칸은 이동 1mm, 크기·회전 0.5다.
`피부 브러시`를 켜면 화면을 짧게 클릭해(드래그는 회전) 반경 안의 몸체 삼각형을 가리거나 되살린다. 정지 자세 기준 위치로 고르므로 옷 뒤 피부도 칠해진다. 클릭한 삼각형은 항상 포함하고, 중심이나 꼭짓점 하나라도 반경 안이면 고른다(긴 삼각형 끝만 비칠 때 중심 기준으로는 안 잡혔다). 클릭할 때 맨 앞 물체 이름을 상태 줄에 보여 준다. 저장하면 `brush.hide`/`brush.show`로 남고, 굽기는 자동 가림 계산 뒤에 매번 다시 적용한다(굽기 뒤에도 지워지지 않음).
`이 부위가 가린 피부 보이기`를 켜면 고른 부위의 몸체 가림을 잠시 끄고 원래 피부·속옷을 보여 준다. 굽기에서 가림은 새 형상 기준으로 다시 계산한다.
`--keep-pending`을 붙이면 저장값을 굽지 않고 남겨 미리보기가 계속 그 위에 적용한다. 붙이지 않고 피팅 명령을 다시 실행하면 저장값을 가중치 계산 뒤 바인드 공간에서 같은 변환으로 적용하고 `baked` 목록으로 옮긴다. 이후 실행도 `baked`를 순서대로 다시 적용한다.
변환은 [female-part-fit.ts](../../client/src/previews/female-part-fit.ts)와 [female_priest_adjust.py](../../tools/outfits/female_priest_adjust.py)가 같고, 테스트가 두 결과를 맞춘다.

## 게임 시작 복장 — 2026-11-04

여성 사제는 공통 여성 몸체(`modular_female/base.glb`)를 쓰고, 신규 생성 시 다섯 장비를 지급·장착한다. NPC는 기존 통합 모델을 유지한다.

| 아이템 | 슬롯 | 런타임 파츠 |
| --- | --- | --- |
| `worn_priest_helmet` | head | `priest_helmet` (단발 헤어 유지) |
| `worn_priest_top` | chest | `priest_top` |
| `worn_priest_pants` | pants | `priest_bottom` (치마) |
| `worn_priest_gloves` | hands | `priest_gloves` (새 아이템) |
| `worn_priest_boots` | boots | `priest_boots` |

- 남성 사제의 기존 네 아이템을 같이 쓰고, 외형은 아이템 `worldModel`의 `armor/priest_` 접두어로 고른다. 남성은 장갑을 지급받지 않으며, 착용하면 맨손으로 보인다(남성 사제 장갑 모델 없음).
- `worn_priest_gloves`: 무게 0.5, 천, 거래·판매 불가. 바닥 모델 `client/public/models/armor/priest_gloves.glb`와 128px 아이콘 `client/public/items/armor/priest_gloves.png`는 피팅된 장갑에서 만들었다(새 생성 요청 없음, 원본 출처는 위 Tripo 기록).
- 사제 하이힐 장화를 새 부츠 변형으로 추가해 여성 기본 동작과 10개 애니메이션 팩의 부츠 보정을 다시 구웠다.
- 기존 여성 사제는 자동으로 바뀌지 않는다. 배포 때 서비스를 멈추고 아래 도구로 빠진 장비만 가방에 넣는다(장착 상태는 그대로).

```sh
blender -b --python-exit-code 1 --python tools/blender-scripts/export_outfit_items.py -- --outfit priest
for p in top bottom boots gloves helmet; do node tools/prepare-female-knight.mjs --part priest_$p; done
node tools/prepare-female-knight.mjs --bake-existing
.venv/bin/python tools/grant-female-priest-starter-outfit-20261104.py data/game_data.db            # 미리보기
.venv/bin/python tools/grant-female-priest-starter-outfit-20261104.py data/game_data.db --apply --expected-preview preview.json
```

5,000명 동시 접속 부하 시험과 실제 게임 접속 확인은 하지 않았다. 장비 조합은 `femaleCharacter.test.ts`가 모든 방어구 아이템으로 확인한다.

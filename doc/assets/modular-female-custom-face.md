# 여성 사용자 제공 얼굴: 목 연결·속눈썹 알파·아래 눈꺼풀 단차

2026-11-07. 사용자의 요청으로 **눈·속눈썹을 수정하기 전, 원본 머리에 목만 맞춘 버전**으로 복원했다.
현재 브랜치는 `codex/female-new-face`이며 여성 캐릭터 생성의 **새 얼굴** 선택지로 연결했다.
그 복원본에서 사용자의 추가 요청에 따라 **원본 속눈썹 판의 살색 부분만 알파 처리**했다.

## 현재 상태

2026-11-08, 홍채 보정 상태를 `5dfad9e2`에 저장한 뒤 게임에 연결했다.
여성 플레이어 8개 직업에서 기본 얼굴과 새 얼굴을 선택하며 선택값 `custom`을 서버에 저장한다.
기존 캐릭터는 기본 얼굴을 유지하며 남성 얼굴 목록은 기존 세 종류다.
런타임은 공통 `base.glb`에 얼굴 파츠 `client/public/models/characters/modular_female/face_custom.glb`(2.88MB)를 붙인다(2026-11-08, 이전 `base_custom.glb` 5.13MB 대체).
파츠는 머리·속눈썹과 목 띠 632삼각형이다. 목 띠는 몸체에서 y ≥ 1.502m에 닿는 삼각형으로, 목 피팅 위치와 목 색 보정을 그대로 가진다.
그 아래 몸체 15,069삼각형은 `base.glb` 몸체와 위치·UV·가중치가 같아서 `base.glb` 몸체를 그대로 쓰고, 머리·목 5,730삼각형과 기본 속눈썹은 숨긴다.
의상 가림은 `base.glb` 기준 한 벌만 쓴다. 목 띠의 각 삼각형이 대신하는 몸체 삼각형 번호를 파츠에 저장해, 의상이 그 몸체 삼각형을 가리면 목 띠도 함께 가린다. 새 의상을 추가해도 얼굴 파츠는 다시 만들 필요가 없다.
목 띠에는 정점 보정·경계 자르기가 적용되지 않는다. 2026-11-08 현재 이를 쓰는 의상 가림은 목 띠 영역에 닿지 않는다.
목 띠 텍스처는 몸체 아틀라스 배치를 유지하고, 목 띠가 쓰지 않는 텍셀은 평균색으로 채웠다(색 2048² 61KB, `base.glb`의 노멀·ORM 1024² 각 7KB·5KB).
몸 노멀·ORM은 이제 `base.glb`의 1024²를 쓴다. 기본 얼굴과 같은 몸 질감이다.
[조립 상태 이음새 검사](modular-female-face-part-seams.json): 8개 동작 × 25자세에서 목 띠-몸체 112정점, 목 띠-머리 117정점의 오차 0m (`node tools/validate-female-face-part.mjs`).
기본 눈 색에서는 보정한 홍채 원본을 유지하며 다른 눈 색을 선택하면 홍채 부분만 착색한다.
생성 메뉴의 여성 얼굴 썸네일 두 장은 기존 Blender 헤어 썸네일 렌더러로 생성했다. 새 AI 생성은 없다.
출처·라이선스는 아래 사용자 제공 Tripo 원본과 기존 여성 몸체·헤어 기록을 따른다.
재현: `node tools/prepare-female-custom-face.mjs`.
[게임용 파일·의상 가림 출처 기록](modular-female-custom-face-runtime.json).
[실제 생성 화면](../images/characters/modular_human_female_01/customization/face-custom-character-create.png).

검증은 실제 `CharacterCreateScreen`·`CharacterCreateScene`를 임시 브라우저 하네스에 올려 진행했다.
새 얼굴 선택, 도적→순찰자 변경 시 얼굴 유지·헤어 기본값 전환, 남녀 전환 시 얼굴 목록 정리,
눈색 변경과 생성 요청의 `appearance.face=custom`을 확인했다. 브라우저 오류는 0건이다.
WASM 프로토콜 137에서 해당 생성 요청의 직렬화를 확인했으며,
임시 DB를 사용하는 서버 테스트에서 저장·재시작 후 재조회·캐릭터 목록의 외형 보존을 검사했다.
생성 미리보기는 실제 게임과 같은 모델 로더·의상 처리·애니메이션 팩을 사용한다.

### 얼굴별 헤어 보정

여성 얼굴 프리뷰의 **이 얼굴의 헤어 맞춤**에서 단발·긴 머리·묶은 머리를 조절한다.
좌우·상하·앞뒤 이동(cm), 전체·축별 크기(%), 세 축 회전(°)을 머리 본의 바인드 위치를 중심으로 적용한다.
**이 얼굴·헤어 보정 저장**은 선택한 조합만 `client/src/lib/data/femaleHairFits.json`에 저장한다.
프리뷰는 이 파일의 최신 값을 읽으며, 저장하지 않은 조절은 같은 탭에서 조합을 오가도 유지한다.
**저장값으로 되돌리기**는 미저장 조절을 취소하고, **원래 위치·크기**는 보정 전 값으로 돌린다.
게임은 새로고침 후 저장값을 적용한다. 기본 얼굴과 새 얼굴의 조절값은 별개다.

프리뷰와 게임은 같은 최종 헤어 GLB와 변환 함수를 사용한다. 원본 형상·UV·스킨 가중치·본·동작 파일은 수정하지 않는다.
보정된 헤어 지오메트리는 조합별로 캐시해 캐릭터끼리 공유한다. 투구 아래의 잘린 단발도 같은 단발 보정을 사용한다.
기존 여성 헤어의 출처·라이선스를 유지하며 새 생성 에셋은 없다.
실시간 이동·저장·새로고침, 얼굴·헤어별 독립성, 미저장 조절 복원과 게임 로더의 헤어 좌표 일치를 확인했다.

### 캐릭터 생성 조명

생성 화면의 천장 스포트라이트를 얼굴 앞 좌우의 넓은 면광원으로 바꿨다.
따뜻한 주광과 약한 보조광, 낮은 강도의 환경광을 함께 사용해 눈가의 강한 머리카락 그림자를 줄인다.
금속 의상도 넓은 반사광을 받으며, 얼굴 확대와 전신 보기는 같은 조명을 사용한다.
광원과 환경맵은 생성 화면을 열 때 준비하고 닫을 때 정리한다. 헤어·피부 재질과 텍스처는 유지한다.
[조명 변경 후 생성 화면](../images/characters/modular_human_female_01/customization/face-custom-character-create-lighting.png).

### 머리색·눈색 원본 복원

캐릭터 생성의 머리색과 눈색에 각각 **원래 색** 버튼을 둔다.
누르면 현재 헤어 또는 얼굴 텍스처의 원래 컬러를 사용하며, 색상 견본·직접 선택을 누르면 해당 부위만 다시 착색한다.
원본 모드는 `hair_color_original`·`eye_color_original`로 저장하고 직업·헤어·얼굴을 바꿔도 유지한다.
기존 캐릭터 데이터에서 두 값이 없으면 기존 착색 상태를 유지한다. `master` 통합 후 프로토콜 버전은 139다.
기존 재질과 텍스처는 유지하며, 원본과 착색 전환은 같은 재질의 uniform으로 처리한다.

### 홍채 가장자리 틴트

`a8ea2706` 기준에서 새 얼굴의 홍채 착색 범위를 바깥쪽과 아래쪽까지 넓혔다.
밝은 금색 테두리도 착색하고 원래 색이 섞이는 비율을 없앴다.
원본 컬러의 청색/녹색 성분 비율로 피부·흰자를 제외하고, 위 눈꺼풀의 경계로 착색 범위를 제한한다.
원래 색 모드와 기존 얼굴의 설정은 유지한다. 메시·텍스처·가중치 파일은 바꾸지 않는다.
실제 생성 화면에서 두 여성 얼굴의 원래 색·파랑·초록·보라를 확대 확인했으며 브라우저 오류는 없었다.
[파랑](../images/characters/modular_human_female_01/customization/face-custom-iris-tint-edge-blue.png),
[초록](../images/characters/modular_human_female_01/customization/face-custom-iris-tint-edge-green.png).

목 피팅 기준본은 사용자 제공 머리 전체에 균일 배율 `1.8008793356130923`과 이동 `[-0.0008, 0, 0.014]m`만 적용했다.
눈·눈꺼풀·눈썹 주변·두피·목과 원래 속눈썹 판의 모든 정점 위치·노멀·가중치를 포함한다.
이전 눈꺼풀 시안은 복원한 뒤, 아래의 v2 작업에서 실제 단차선을 찾아 곡선을 유지하며 내렸다.
원본 PBR 재질 값과 노멀맵을 유지한다. 홍채 반사광 116픽셀 보정본을 기준으로, 아래 단차의 컬러·ORM 매핑을 위로 보정했다.
속눈썹에만 원본 컬러 픽셀을 보존한 RGBA 아틀라스와 알파 재질을 사용한다.
단차선 주변 밖의 머리 좌표는 원본 스케일·이동 결과를 유지한다.

몸체와 머리는 높이 **1.552m**의 한 경계에서 직접 연결한다. 별도 목 연결면은 없다.
기존 몸체 목 아래 5cm의 피팅과 목 아래 3.5cm의 피부색 보정은 유지한다.
몸체의 모든 정점 속성·인덱스와 목 컬러맵이 초기 피팅 기준 파일과 같은지 검사했다.
65본의 계층·변환·bind 행렬과 기존 9개 동작을 유지했다.

현재 미리보기:
`/modular-character-preview.html?character=female-face&face=custom&hair=none&camera=face`.
게임과 같은 `base.glb`와 `face_custom.glb`를 불러온다(2026-11-08). 피팅 원본 `base_custom_preview.glb`를 보려면 `&source=preview`를 붙인다.
현재 개발 미리보기 조명으로 표시하며, 아래 눈꺼풀 단차선 주변을 제외한 얼굴 형상과 PBR 재질 값은 원본이다. 속눈썹 살색 알파와 아래의 홍채 반사광 완화를 적용했다.
[복원된 정면](../images/characters/modular_human_female_01/customization/face-custom-neck-only-restored-front.png),
[눈 확대](../images/characters/modular_human_female_01/customization/face-custom-neck-only-restored-eyes.png).

## 동공·홍채 텍스처 정리 v2

2026-11-08, 사용자가 만족한 아래 눈꺼풀 상태를 `9f58038c`에 먼저 커밋했다.
그 기준에서 오른눈의 흩어진 검은 동공 조각과 왼눈에 남은 흰 반사 무늬를 정리했다.
기존 황갈색 홍채를 정면 투영해 ImageGen으로 보정하고, 원래 시선 위치에 맞춰 배치한 뒤
홍채 영역 마스크에만 적용했다. 아틀라스의 마스크 밖 컬러는 동일하다. 아주 작은 원본 UV 조각에 흰 무늬가 남아,
해당 홍채 면도 기존 512×512 UV1 패치에 연결했다. 정점은 UV 경계에서 복제하지만 삼각형 위치·노멀·UV0·가중치·리그·동작은 커밋 기준본과 일치한다.
노멀·ORM·속눈썹 RGBA 이미지 바이트도 그대로다. ORM은 기존 UV1 좌표를 복사한 UV2로 샘플링해 표면 반사 특성을 유지한다.
실제 조명 반사는 남으며 고정 반사 무늬가 제거된 것은 조명 없는 컬러 투영으로도 확인했다.

원본·보정 투영 이미지와 적용 마스크, 보정 전 GLB·Blender·피팅 기록은 `iris_texture_cleanup_v2/`에 있다.
`before/`는 **[미사용: 이번 홍채 보정 전 비교·재현용]**이다. ImageGen **ChatGPT Pro 20x**, 2026-11-08 생성.
사용자 제공 원본 텍스처의 출처·라이선스 조건을 따른다. 새 안구·눈꺼풀 형상은 만들지 않았다.
[실제 미리보기 전후](../images/characters/modular_human_female_01/customization/face-custom-iris-retouch-v2-comparison.png),
[출처·보정·검증 기록](modular-female-custom-face-iris-retouch-v2.json).

## 아래 눈꺼풀 단차선 v2

2026-11-08, 사용자 빨간색·파란색 표시를 기준으로 실제 메시 단차의 앞·뒤 모서리를 찾았다.
이전 시안은 이 정점들이 대부분 고정된 채 아래쪽 면만 움직였기 때문에 폐기했다.
이번에는 중앙 단차선 정점 9개를 모두 2.8mm 하향 평행 이동했다. 바깥쪽 정점은 2.458mm 내리고,
눈꼬리와 주변 피부에 부드럽게 연결한다. 중앙 정점 사이의 상대 좌표를 비교해 기존 곡선 보존을 검사했다.
원래 윗속눈썹 위치는 고정하며 X·Z 좌표, 몸체·목·리그·가중치·9개 동작을 유지한다.

따라 내려간 텍스처는 기존 색과 ORM을 정면 투영해 반대 방향으로 보정했다. 원래 눈동자 무늬를 유지하며,
작은 UV 조각이 늘어나는 문제를 피하도록 원본 아틀라스의 사용하지 않는 공간에 512×512 패치 두 개를 배치했다.
머리의 컬러·ORM에 UV1을 사용하고 원본 UV0·노멀맵은 유지한다. UV 경계의 정점은 복제하지만
삼각형은 추가하지 않는다. 전체 23,444삼각형, 메시·재질 3개와 텍스처 최대 변 2048을 유지한다.
속눈썹 RGBA는 그대로다. 새 AI 이미지나 안구 형상은 생성하지 않았다.

텍스처 없는 회색 렌더와 실제 재질 렌더를 정면·사선·측면에서 비교했다.
33개 안구 불투명 면, 새로 열린 영역의 5시점 표본 검사, 225자세의 목 경계·머리 본 추종,
압축 전후 정점 속성·삼각형 와인딩·동작과 속눈썹 RGBA 검사를 통과했다.
표시선에 대한 수동 근사이며 원본의 거친 표면 디테일은 남아 있다.

기준 GLB·압축본·편집본은 `lower_lid_ridge_v2/before/`에 **[미사용: v2 수정 전 비교·재현용]**으로 보존한다.
사용자 표시와 제어점, 두 UV 패치, 원본 색을 이용한 보정 맵은 `lower_lid_ridge_v2/`에 있다.
로컬 Python·Three.js·Blender 작업이며 원본 모델과 기존 홍채 보정의 출처·라이선스를 따른다.
[메시와 재질 전후 비교](../images/characters/modular_human_female_01/customization/face-custom-ridge-v2-comparison.png),
[검증·출처 기록](modular-female-custom-face-ridge-v2.json).

## 아래 눈꺼풀 시안 [미사용]

2026-11-08 아래 눈꺼풀을 내린 시안은 측면에서 돌출된 경계가 충분히 이동하지 않고
텍스처만 내려간 것처럼 보여 사용자가 원상복구를 요청했다.
`lower_lid_fit_v1/before/`의 GLB·미리보기·Blender·피팅 기록을 그대로 복원했다.
이 복원본을 위 v2 작업의 기준으로 사용한다.
`tools/fit-female-lower-eyelids.py`와 해당 비교 이미지는 **[미사용]**이다.
거절한 모델은 `lower_lid_fit_v1/rejected-fitting/`에 보존한다.
[실험 기록](modular-female-custom-face-lower-lid-fit-v1.json).

## 홍채의 고정 반사광 완화

2026-11-08, 커밋 `f1775cb9`의 눈동자를 기준으로 컬러맵에 그려진 강한 흰 반사 무늬만 줄였다.
원본 색을 정면 투영해 ImageGen으로 수정한 뒤, 홍채 내부의 밝은 반사광 마스크에만 최대 65% 혼합했다.
2048×2048 컬러맵에서 바뀐 픽셀은 116개, 채널 최대 변화는 65/255다. 마스크 밖의 픽셀은 동일하다.
황갈색 무늬·동공·흰자·눈꺼풀·형상·UV·리그·동작과 속눈썹 RGBA를 유지한다.
노멀·ORM·재질 값은 바꾸지 않아 실제 조명에 의한 반사는 남는다.

보정 입력·출력·마스크·커밋 기준본은 `iris_highlight_cleanup_v1/`에 보존했다.
기준본은 **[미사용: 보정 전 비교 및 재현용]**이며, 새 원본은 사용자 제공 Tripo 파일이다.
ImageGen은 **ChatGPT Pro 20x**, 2026-11-08 생성이다. 기존 원본의 출처·라이선스 조건을 따른다.
[실제 미리보기 전후](../images/characters/modular_human_female_01/customization/face-custom-iris-highlight-cleanup-v1.png),
[보정·검증 기록](modular-female-custom-face-iris-highlights-v1.json).

## 원본 속눈썹의 살색 알파

원본 속눈썹 판 475삼각형만 별도 `MASK` 재질로 분리했다. 원본의 어두운 무늬는 남기고,
밝기와 따뜻한 갈색 차이로 살색·피부 그림자에 투명도를 적용했다. cutoff는 0.5다.
새 가닥이나 형상·위치·굵기·방향 보정은 추가하지 않았다. 원본 UV0는 유지한다.
원래 판의 노멀·ORM과 기타 PBR 값은 그대로 사용한다.

원본 컬러 조각만 256×256 RGBA에 패킹하고 컬러용 UV1을 추가했다. 원본 픽셀 해상도를 유지하며,
삼각형 표본의 RGB 보간 오차는 8비트 채널 값 기준 0.001 미만이다. 투명 픽셀의 RGB까지 유지하도록 이 작은 RGBA는 PNG로 보존하며, 압축본의 RGBA 전체가 일치하는지 검사했다.
정점 위치·노멀·UV0·가중치·삼각형을 알파 적용 전과 비교했다. 목 연결과 9개 동작도 유지한다.
이 알파는 원본의 어두운 무늬만 남기는 처리이며, 검은 뿌리나 원본 판 형태를 새 가닥으로 재작성하지 않는다.

[살색 알파 전후](../images/characters/modular_human_female_01/customization/face-custom-original-lash-alpha-comparison.png),
[현재 눈 확대](../images/characters/modular_human_female_01/customization/face-custom-original-lash-alpha-eyes.png),
[출처·보존·검증 기록](modular-female-custom-face-original-lash-alpha-v1.json).
작업 데이터는 `lash_skin_alpha_v1/`, 알파 이전 목 연결 원본은 `lash_skin_alpha_v1/before/`에 있다.
로컬 코드로 알파 채널을 계산했으며 새 AI 생성이나 외부 유료 호출은 없다. 원본 모델의 출처·라이선스를 따른다.

안구 경계면 24개, 눈 안쪽·아래 테두리 8개, 홍채 가장자리 1개 면의 알파를 해제하고 원래 불투명 재질로 복원했다.
속눈썹 RGBA와 정점·삼각형은 그대로 유지한다. 정면·양쪽 사선·양쪽 측면의 3픽셀 간격
표본 검사에서 표면 누락 표본은 201개에서 0개로 줄었다. 뒤에 다른 면이 있는 표면 누락은 이 검사로 찾지 못했다. 추가 복원한 눈 안쪽·아래 테두리를 포함해 33개 원본 면의 불투명 분류를 별도로 검사하고 확대 스크린샷으로 확인했다.
복원할 원본 면 목록은 `lash_skin_alpha_v1/eye-surface-protection.json`에 기록했다.
이전 알파 결과는 `lash_skin_alpha_v1/before-eye-protection/`에 **[미사용]**으로 보존한다.

직전 복원본은 `lash_skin_alpha_v1/before-inner-corner-repair/`에 **[미사용]**으로 보존한다.

홍채 옆 삼각형 면에 남아 있던 알파를 추가 해제했다. 직전 결과는 `lash_skin_alpha_v1/before-iris-edge-repair/`에 **[미사용]**으로 보존한다.

## 출처

- 사용자 전달 파일: `Y:\public\web_downloads\tripo_convert_2362307c-3152-41f0-8201-e540004153fa.glb`.
- 보존 원본: `assets/modular_human_female_01/faces/tripo_custom_v1/source.glb`.
- 원본 SHA-256: `8dbcbcb21d06cd2ed33020a9eef617e1b4396fa4e43292c95748ddbbe3152dce`.
- 사용자 제공 Tripo 변환 출력이다. 생성일·구독 등급·별도 라이선스는 전달되지 않았다.
- 복원에는 로컬 Python·Three.js·Blender만 사용했다. 새 AI 생성 호출은 없다.
- 몸체·헤어 출처와 이용 조건은 [여성 베이스](modular-human-female-01.md)와
  [기사 헤어](modular-female-knight-hair.md)를 따른다.

## 산출물과 검증

파일은 `assets/modular_human_female_01/faces/tripo_custom_v1/`에 있다.

| 파일 | 현재 용도 |
| --- | --- |
| `base_custom.glb` | 원본 머리와 목 피팅 몸체, 동작 9종 |
| `base_custom_preview.glb` | 실제 미리보기에서 제공하는 2K 압축본. 게임용 `face_custom.glb`의 입력이다 |
| `face_custom.glb` | 공통 여성 리그에 붙이는 원본 머리·목 파츠 |
| `custom-face-fitting.blend` | 복원 상태의 편집본 |
| `body-neck-albedo.png` | 초기 목 아래 피부색 보정 맵 |
| `lash_skin_alpha_v1/original-lash-skin-alpha.png` | 원본 색을 유지한 속눈썹 RGBA |
| `fitting.json` | 원본 보존·아핀 오차·파일 해시 |

머리 7,743 + 몸체 15,701 = **23,444삼각형**이다. 단발 2,436개를 포함하면 **25,880삼각형**이다.
불투명 영역 22,969삼각형과 속눈썹 알파 판 475삼각형을 포함한 제출 수다.
속눈썹 판의 가시 픽셀은 살색 알파로 줄어든다. 시점별 뒷면·자체 가림과 부분 삼각형은 별도 합산하지 않았다.
얼굴 앞면만의 별도 분류는 하지 않았다.
15,000–20,000의 전체 목표를 초과하며 원본 얼굴 보존을 위해 유지한다.
메시 3개·재질 3개이며 무기·의상·망토 없는 조합이다.

Meshopt 정점 압축, WebP 컬러 품질 94와 JPEG 노멀·ORM을 사용한다. 텍스처 최대 변은 2048이다.
압축 전후 정점 속성·삼각형 순서·와인딩·동작 트랙을 검사했다.
9개 동작에서 각 25자세, 총 225자세의 유한한 변형·머리 본 추종·목 경계 일치를 확인했다.
분리한 머리 파츠도 기준 몸체 리그에 바인딩했다.
[피팅 기록](modular-female-custom-face-fitting-v1.json),
[동작 검사](modular-female-custom-face-animation-v1.json),
[압축 검사](modular-female-custom-face-compressed-v1.json).

재생성:

```bash
.venv/bin/python tools/restore-female-neck-fit.py
.venv/bin/python tools/apply-female-original-lash-alpha.py
.venv/bin/python tools/clean-female-iris-highlights.py
.venv/bin/python tools/fit-female-eyelid-ridge.py
node tools/optimize-modular-part.mjs \
  assets/modular_human_female_01/faces/tripo_custom_v1/lower_lid_ridge_v2/base_custom.glb \
  assets/modular_human_female_01/faces/tripo_custom_v1/lower_lid_ridge_v2/base_custom_preview.glb \
  --body --size=2048 --quality=94 --keep-alpha-png
.venv/bin/python tools/fit-female-eyelid-ridge.py --activate
.venv/bin/python tools/retouch-female-irises.py
node tools/optimize-modular-part.mjs \
  assets/modular_human_female_01/faces/tripo_custom_v1/base_custom.glb \
  assets/modular_human_female_01/faces/tripo_custom_v1/base_custom_preview.glb \
  --body --size=2048 --quality=94 --keep-alpha-png
node tools/inspect-female-original-eye-holes.mjs
node tools/validate-female-custom-face.mjs
node tools/validate-female-reference-compression.mjs
```

## 이전 실험 [미사용]

눈·속눈썹 수정 시안은 [실험 기록](modular-female-custom-face-experiments-v2.md)에 남겼다.
마지막 실험의 GLB·압축본·Blender·피팅 기록은 `reference_v2/rejected-eye-edits-v2/`에 보관한다.
`reference_v2/`의 생성 홍채·눈 아틀라스·속눈썹 RGBA·변형 파라미터와 반복 스크린샷은 현재 사용하지 않는다.
ImageGen 홍채는 당시 **ChatGPT Pro 20x**, 2026-11-07 생성 기록을 유지한다.

`tools/fit-female-custom-face.py`는 이전 v1 눈 보정을, `tools/refine-female-reference-eyes.py`는
이전 v2 눈 실험을 재생성하는 도구다. 현재 복원에는 위의 `restore-female-neck-fit.py`를 사용한다.
원래 두 경계 연결 시안도 `two-seam-unused-v1/`에 **[미사용]** 상태로 보존한다.

2026-11-08 커밋 준비: 재생성 가능한 검토용 PNG는 정리했다. 문서에 사용하는 이미지는
`doc/images/characters/modular_human_female_01/customization/`에 보존하며, 촬영 카메라·모델 해시는 각 `capture.json`에 남긴다.

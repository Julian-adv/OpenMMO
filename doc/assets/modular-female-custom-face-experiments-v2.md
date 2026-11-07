**[미사용] 눈·속눈썹 실험 기록. 현재 버전은 원본 머리에 목만 맞춘 상태다.**

# 여성 사용자 제공 얼굴 피팅

2026-11-07. 사용자 제공 GLB의 머리를 기존 여성 몸체에 연결하고, 참고 이미지에 맞춰
속눈썹·홍채·안구를 수정한 검토본이다. 브랜치는 `codex/female-new-face`다.
게임 외형 목록에는 아직 등록하지 않았다.

## 현재 결과: 참고 이미지 v2

황갈색 홍채와 검은 동공, 부드러운 흰자, 바깥으로 펼쳐지는 윗속눈썹과 짧은 아랫속눈썹을 적용했다.
작은 UV 조각에 나뉘어 있던 눈 앞면을 매끈한 안구 표면으로 덮었다. 홍채의 윤기는 실제 조명으로 계산한다.
원본 피부 컬러는 보존했다. 눈의 위치와 윤곽을 맞추기 위해 앞쪽 눈·눈꺼풀·눈썹 주변만 국소 변형했다.
두피·목과 눈 영역 밖의 원본 위치·노멀은 유지했다. 눈썹이나 입술 텍스처는 새로 그리지 않았다.

윗속눈썹은 한 눈당 84가닥, 아래는 48가닥이다. 가닥마다 끝으로 갈수록 가늘어지는 리본 형상이며,
눈꺼풀 표면에 맞춘 뿌리 띠와 함께 기존 속눈썹 메시 하나에 합쳤다. 원본 판 508삼각형을 교체했다.
눈꼬리는 바깥으로 펼치며 위·아래 뿌리를 눈꺼풀에 붙였다. 재질은 양면 `BLEND`, metallic 0,
roughness 0.9다. 64×64 RGBA는 색·끝 투명도만 제공하고 가닥 사이 빈 공간은 실제 형상으로 나뉜다.

메시 3개·재질 3개를 유지한다. 눈과 속눈썹을 위해 별도 런타임 업데이트나 추가 드로콜을 만들지 않았다.
얼굴 아틀라스는 2048×2576이며 원본 2048×2048 컬러 픽셀을 위쪽에 그대로 보존하고 아래에 눈 타일을 배치했다.
기존 UV의 V값만 `2048/2576`으로 바꿔 원래 픽셀 위치를 유지했다. 새 눈 앞면과 아래 눈꺼풀 테두리는
1,728삼각형이다. 피부 거칠기를 올리고 얼굴 노멀맵 강도를 0으로 설정해 번진 반사를 줄였다.

초기 12차 수정은 재질과 속눈썹에 치우쳐 눈 윤곽 차이가 컸다. 현재는 머리·코 구도를 맞추고
참고 이미지의 눈 앞머리·눈꼬리·위아래 곡선을 기준으로 다시 피팅했다. 양쪽 눈의 앞머리 폭을
별도로 조정했고, 홍채 반지름은 7.4mm에서 8.55mm로 키웠다. 눈 주변을 최대 4.5mm 아래로
옮기며 주변 피부에 부드럽게 연결했다. 기존 안구 면은 새 표면 뒤로 정리하고 안쪽 끝도 맞췄다.
원본 눈 영역의 총 최대 변위는 10.88mm, Y 최대 변위는 8.06mm다. 본체·목·동작 트랙은 바꾸지 않았다.

측면 돌출은 안구와 주변 피부를 함께 변형해 줄였다. 안구 앞면만 깊게 넣은 21–23차는
눈꺼풀 주변 접힘과 반사 감소가 생겨 교체했다. 현재는 타원구 곡면(반축 X/Y/Z = 23/16/9mm)에 작은 각막 곡률을 더하고,
주변 피부에 부드러운 깊이 변형을 적용하며, 가장 앞선 안구 지점이 이전보다 7.88mm 뒤로 들어간다.
표본 정점의 최대 후퇴는 8.20mm, 중앙값은 5.24mm다.
바깥 눈꼬리는 타원구를 감싸도록 일부 앞으로 맞춘다.
안구·아래 눈꺼풀의 맞닿는 98개 표본 좌표는 동일하다. 피부 쪽 끝은 실제 표면에 맞췄다.
속눈썹 가닥은 뿌리 기준으로 이동해 곡률·굵기를 유지한다. 눈 앞면의 반사 노멀은 분리해 윤기를 보존한다.
정면·확대·양쪽 사선·양쪽 측면·머리 측면을 같은 카메라로 전후 촬영했다.
[측면 전후 비교](../images/characters/modular_human_female_01/customization/face-custom-reference-v2-depth-comparison.png),
[머리 측면 전후](../images/characters/modular_human_female_01/customization/face-custom-reference-v2-head-depth-comparison.png).


최종 검토는 `26-inset-ellipsoid`다. 13차는 같은 구도 비교용 촬영이고, 14–19차는 눈 윤곽과
겹침 수정본이다. 20차는 측면 기준 촬영, 21–26차는 깊이와 눈꺼풀 연결 보정이다. 정면·확대·양쪽 사선을 실제 브라우저에서 확인했다. 확대 화면도 정면과 같은
카메라 위치를 사용해 가까운 광각 촬영의 왜곡을 피한다. 수치화한 이미지 유사도 점수는 사용하지 않았다.
참고 이미지와 조명·표면 디테일은 차이가 있다. `reference_v2/shape-guide.json`에 수동 기준점과
카메라를 기록했다.

- [눈 윤곽 원본·수정 비교](../images/characters/modular_human_female_01/customization/face-custom-reference-v2-eye-shape-comparison.png)

- [참고 이미지와 최종 정면 비교](../images/characters/modular_human_female_01/customization/face-custom-reference-v2-comparison.png)
- [눈 확대](../images/characters/modular_human_female_01/customization/face-custom-reference-v2-eyes.png)
- [양쪽 사선](../images/characters/modular_human_female_01/customization/face-custom-reference-v2-angles.png)
- [같은 미리보기 조명의 수정 전·후](../images/characters/modular_human_female_01/customization/face-custom-reference-v2-before-after.png)
- [출처·반복 검토·검증 기록](modular-female-custom-face-reference-v2.json)

## 출처와 라이선스

- 사용자 전달 경로: `Y:\public\web_downloads\tripo_convert_2362307c-3152-41f0-8201-e540004153fa.glb`.
- 보존 원본: `assets/modular_human_female_01/faces/tripo_custom_v1/source.glb`.
- SHA-256: `8dbcbcb21d06cd2ed33020a9eef617e1b4396fa4e43292c95748ddbbe3152dce`.
- 사용자 제공 Tripo 변환 출력이다. 원화 출처·생성일·구독 등급·라이선스는 전달되지 않았다.
- 참고 얼굴은 사용자가 이 대화에 첨부한 이미지이며 `reference_v2/target.png`에 보존했다. 별도 라이선스 정보는 없다.
- 새 홍채는 기본 ImageGen으로 2026-11-07에 생성했다. 등급은 **ChatGPT Pro 20x**다.
  이전 홍채와 사용자 참고 이미지를 입력으로 사용했다. 생성 출력은 `reference_v2/iris-hazel.png`에 그대로 보존했다.
  `reference_v2/generation.json`에 입력·출력 해시와 프롬프트 요약이 있다.
- 안구·속눈썹 형상과 텍스처 통합은 프로젝트 Python 코드로 제작했다. 추가 유료 3D 생성 호출은 없다.
- 몸체·헤어는 [여성 베이스](modular-human-female-01.md), [기사 헤어](modular-female-knight-hair.md),
  [여전사 헤어](modular-female-warrior-hair.md)의 출처와 이용 조건을 따른다.

## 머리 연결과 리그

원본은 39,865삼각형의 단일 메시이며 리그가 없다. 균일 배율 `1.8008793356130923`과
이동 `[-0.0008, 0, 0.014]m`으로 기존 여성 몸체에 맞췄다. 몸체와 새 머리를 높이 **1.552m**에서
한 번 잘라 직접 연결했다. 목 연결면은 0개다. 서로 다른 경계 정점 수는 모서리 분할로 맞췄다.
눈 주변 이외의 원본 얼굴·두피·목은 같은 배율과 이동을 따른다. 국소 변형한 눈 주변,
새 안구와 속눈썹은 이 아핀 보존 검사에서 제외한다.

목 아래 몸체의 위쪽 5cm만 새 머리에 맞췄고, 목 아래 3.5cm 구간의 몸체 컬러를 보정했다.
이번 눈 윤곽 작업에서도 몸체의 모든 정점 속성·인덱스와 목 연결을 그대로 유지했다.
65본의 계층·변환·bind 행렬과 9개 동작의 트랙을 보존했다. 눈과 속눈썹은 Head 본을 따른다.
표정·눈 깜빡임 리그는 추가하지 않았다.

## 삼각형과 압축

| 파트 | 숨겨진 부분을 포함한 삼각형 |
| --- | ---: |
| 기존 몸체 | 15,701 |
| 얼굴·두피·목과 새 안구·테두리 | 8,963 |
| 속눈썹·뿌리 띠 | 3,408 |
| 헤어 없는 조합 | **28,072** |
| 단발 포함 조합 | **30,508** |

머리 배분은 12,371삼각형이다. 새 안구 뒤에 남긴 원본 눈도 전체 수에 포함한다.
정면에서 원본 눈 735삼각형의 모든 꼭짓점·중심이 새 안구 뒤에 가려지는 것을 검사했다.
이를 제외한 정면 표시 형상의 보수적 상한은 27,337개, 단발 포함 29,773개다.
부분 가림·다른 뒷면·헤어 가림은 빼지 않았으므로 실제 보이는 삼각형 수의 정확한 측정치는 아니다.
무기·의상·망토 없는 조합이다. 15,000–20,000의 전체 목표를 초과하며 얼굴 확대 품질을 위해 유지했다.
숫자를 맞추기 위한 데시메이션은 하지 않았다.

검토 GLB는 약 4.9MB다. Meshopt로 정점 속성을 손실 없이 압축하고, 컬러는 WebP 품질 94,
속눈썹 RGBA는 무손실 WebP, 노멀·ORM은 JPEG로 저장했다. 최대 텍스처 변 길이는 2576이다.
몸체 컬러는 2576×2576, 얼굴은 2048×2576, 속눈썹은 64×64다. 이 설정은 개발 미리보기용이다.
[압축 검사](modular-female-custom-face-compressed-v1.json)에 실제 크기·해시를 기록한다.

## 산출물과 재검토

파일은 `assets/modular_human_female_01/faces/tripo_custom_v1/`에 있다.

| 파일 | 용도 |
| --- | --- |
| `source.glb` | 사용자 제공 전신 원본 |
| `face_custom.glb` | 공통 리그에 붙일 머리·목·눈·속눈썹 파츠 |
| `base_custom.glb` | 기존 몸체에 연결한 모델과 동작 9종 |
| `base_custom_preview.glb` | 브라우저 압축 검토본 |
| `custom-face-fitting.blend` | 현재 모델·리그·단발과 동작 표본을 담은 편집본 |
| `body-neck-albedo.png` | 목 아래 피부색을 맞춘 몸체 컬러 |
| `reference_v2/style.json` | 현재 눈·속눈썹 파라미터 |
| `reference_v2/original-face.npz` | 눈 UV 수정 이전의 원본 스키닝 얼굴 속성 |
| `reference_v2/before-reference.glb` | v2 빌더의 몸체·리그 기준 입력 |
| `reference_v2/face-reference-*.png` | 현재 얼굴 컬러·노멀·ORM |
| `reference_v2/eyelashes-natural.png` | 현재 속눈썹 색·투명도 |
| `reference_v2/review/` | 반복별 정면·확대·양쪽 사선과 촬영 대상 해시 |

미리보기: `/modular-character-preview.html?character=female-face&face=custom&hair=none&camera=face`.
기존 얼굴 비교, 단발·전사 장발·헤어 없음, 정면·사선·측면·전신, 동작 9종을 제공한다.
`/__female-preview/face-custom.glb`는 ETag로 압축본을 재검증한다.

[피팅 기록](modular-female-custom-face-fitting-v1.json),
[애니메이션 검사](modular-female-custom-face-animation-v1.json),
`reference_v2/geometry-validation.json`에서 몸체·눈 영역 밖의 원본 얼굴·스킨 픽셀 보존을 확인한다.
동작마다 25자세, 총 225자세에서 유한한 변형·머리 본 추종·목 경계를 검사했다.
검토 이미지의 실제 대상 파일 해시는 각 `capture.json`에 있다. 모든 의상·투구 관통과 게임 외형 메뉴 연결은 범위 밖이다.

현재 버전 재생성:

```bash
.venv/bin/python tools/refine-female-reference-eyes.py
.venv/bin/python tools/validate-female-reference-eyes.py
node tools/optimize-modular-part.mjs \
  assets/modular_human_female_01/faces/tripo_custom_v1/base_custom.glb \
  assets/modular_human_female_01/faces/tripo_custom_v1/base_custom_preview.glb \
  --body --size=2576 --quality=94
node tools/validate-female-reference-compression.mjs
node tools/validate-female-custom-face.mjs
.venv/bin/python tools/review-female-reference-eyes.py --iteration review-current
```

## 이전 시안 [미사용 런타임]

- `reference_v2/depth_refinement/before.glb`, `before-preview.glb`, `before-style.json`, `before-builder.py`는
  이번 측면 보정 전 19차 입력이다. 사용자 제공 원본과 기존 생성 홍채의 출처·라이선스를 동일하게 따른다.

- `reference_v2/rejected-12-fitting.blend`는 눈 윤곽 지적 이전 12차 편집본이다. 현재는 사용하지 않는다.

- `tools/fit-female-custom-face.py`는 원본 머리 연결과 v1 텍스처·속눈썹을 재생성한다.
  현재 결과를 만들 때는 위의 v2 빌더를 사용한다. v1 빌더만 실행하면 현재 결과를 덮어쓴다.
- `eyelash-alpha.png`, `eyelash-fit-alpha.png`, `eye-iris-clean-v1.png`, `face-eye-*.png`,
  `eye-cleanup-mask.png`는 이전 판 기반 속눈썹·눈 텍스처다. v2는 직접 사용하지 않는다.
  이전 홍채는 새 홍채의 ImageGen 입력으로만 사용했다.
- `face-corner-normal.png`, `face-corner-orm.png`는 v2 아틀라스의 기반 입력으로 계속 사용한다.
- v1의 속눈썹 알파·뿌리 밀착·방향·굵기·눈 UV 검토 JSON과 이미지는 당시 결과의 기록이다.
  이전의 머리 7,743·전체 23,444·단발 포함 25,880삼각형 수는 현재 값이 아니다.
- **[미사용] 두 경계 시안**은 `two-seam-unused-v1/`에 보존했다. 중간 목 연결면 782개와
  머리 변형을 사용했지만, 사용자가 한 경계와 스케일·이동만 요청해 교체했다.

커밋 시 반복 촬영 PNG는 정리했다. 문서 이미지는 위 링크에, 반복별 카메라·해시는 `reference_v2/review/*/capture.json`에 보존한다.

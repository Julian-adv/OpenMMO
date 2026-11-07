# 여성 사용자 제공 얼굴: 목 연결과 원본 속눈썹 살색 알파

2026-11-07. 사용자의 요청으로 **눈·속눈썹을 수정하기 전, 원본 머리에 목만 맞춘 버전**으로 복원했다.
현재 브랜치는 `codex/female-new-face`이며 게임 외형 목록에는 등록하지 않았다.
그 복원본에서 사용자의 추가 요청에 따라 **원본 속눈썹 판의 살색 부분만 알파 처리**했다.

## 현재 상태

사용자 제공 머리 전체에 균일 배율 `1.8008793356130923`과 이동 `[-0.0008, 0, 0.014]m`만 적용했다.
눈·눈꺼풀·눈썹 주변·두피·목과 원래 속눈썹 판의 모든 정점 위치·노멀·가중치를 포함한다.
눈 형상 변형, 새 안구, 생성 속눈썹·뿌리 피팅, 생성 홍채와 눈 UV·재질 보정은 사용하지 않는다.
피부·안구의 원본 PBR 재질 값과 컬러·노멀·ORM 이미지 바이트를 복원했다.
속눈썹에만 원본 컬러 픽셀을 보존한 RGBA 아틀라스와 알파 재질을 사용한다.
압축 전 GLB의 머리 좌표는 원본 스케일·이동 결과와 float32 정밀도 안에서 일치한다.

몸체와 머리는 높이 **1.552m**의 한 경계에서 직접 연결한다. 별도 목 연결면은 없다.
기존 몸체 목 아래 5cm의 피팅과 목 아래 3.5cm의 피부색 보정은 유지한다.
몸체의 모든 정점 속성·인덱스와 목 컬러맵이 초기 피팅 기준 파일과 같은지 검사했다.
65본의 계층·변환·bind 행렬과 기존 9개 동작을 유지했다.

현재 미리보기:
`/modular-character-preview.html?character=female-face&face=custom&hair=none&camera=face`.
현재 개발 미리보기 조명으로 표시하며, 얼굴 형상과 피부·안구 재질은 원본이다. 속눈썹 살색 알파만 추가했다.
[복원된 정면](../images/characters/modular_human_female_01/customization/face-custom-neck-only-restored-front.png),
[눈 확대](../images/characters/modular_human_female_01/customization/face-custom-neck-only-restored-eyes.png).

## 원본 속눈썹의 살색 알파

원본 속눈썹 판 475삼각형만 별도 `MASK` 재질로 분리했다. 원본의 어두운 무늬는 남기고,
밝기와 따뜻한 갈색 차이로 살색·피부 그림자에 투명도를 적용했다. cutoff는 0.5다.
새 가닥을 그리거나 형상·위치·굵기·방향을 바꾸지 않았다. 원본 UV0도 유지한다.
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
| `base_custom_preview.glb` | 실제 미리보기에서 제공하는 2K 압축본 |
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

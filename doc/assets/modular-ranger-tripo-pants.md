# Modular Ranger — Tripo 바지 피팅

2026-10-05. 사용자가 전달한 `Y:\public\web_downloads\leather+pants+3d+model.glb`를
현재 남성 몸체에 맞추고 순찰자 상의 v4와 함께 제작실에 연결했다.
원본은 **3,714 triangles**, 1개 메시·재질, 내장 2K JPEG이며 리깅과 애니메이션이 없다.
권장했던 1,500쿼드 설정의 실제 적용 여부는 미확인이다. GLB의 삼각형 수를 실제 측정값으로 기록한다.

## 보관 파일과 재현

- 원본: `assets/modular_human_male_01/ranger/tripo_pants_v1/source.glb`.
- 피팅·리깅: 같은 폴더의 `pants_ranger.glb`.
- 편집본: 같은 폴더의 `ranger-pants-fitting.blend`.
- 게임 동작 표본: 같은 폴더의 `animation-snapshots.json`, `validation-poses.json`.
- [출처·파일 해시](modular-ranger-tripo-pants-sources.json).
- [피팅 수치·리그 검사](modular-ranger-tripo-pants-fitting-v1.json).
- [시도와 보정 기록](modular-ranger-tripo-pants-fitting-history.json).

```bash
.venv/bin/python tools/fit-tripo-ranger-pants.py
node tools/validate-tripo-rogue.mjs \
  --directory assets/modular_human_male_01/ranger/tripo_pants_v1 \
  --part pants_ranger --report doc/assets/modular-ranger-tripo-pants-animation-v1.json
.venv/bin/python tools/review-ranger-waist.py \
  --pants assets/modular_human_male_01/ranger/tripo_pants_v1/pants_ranger.glb \
  --poses assets/modular_human_male_01/ranger/tripo_pants_v1/validation-poses.json \
  --report doc/assets/modular-ranger-tripo-pants-waist-review-v1.json \
  --heights 1.07 1.09 1.11 1.13 1.14
blender -b --python-exit-code 1 --python tools/blender-scripts/review_tripo_ranger_pants.py
```

원본 면·UV·텍스처를 보존한다. 원본의 허리와 발목은 열려 있으므로 막힌 면을 제거하지 않았다.
현재 몸체 `fitted/base.glb`, `interfaces/v1`, `human_male_01_mixamo_candidate_v2`의
65본·rest 계층·inverse bind를 그대로 사용했다. 기존 바지의 종아리 축소를 중복 적용하지 않는다.
무릎과 가랑이 높이를 별도로 정렬하고 피부 단면에 여유를 준다. 발목은 현행 공통 단면과 가중치를 적용한다.
벨트·버클·작은 주머니는 바지 파츠다. 분리된 장식은 골반에 고정하고,
조끼와 겹치는 부분은 면의 중심·모서리 중점까지 검사해 안으로 넣는다. 분리 장식은 형태가 찌그러지지 않도록 통째로 이동한다.

## 착용·동작 검수

- [원본 형상 검사](modular-ranger-tripo-pants-source-review-v1.json).
- [피팅 앞·뒤·옆](../images/characters/modular_human_male_01/parts/ranger/tripo-pants-fitted-v1-rest.png).
- [허리 연결 확대](../images/characters/modular_human_male_01/parts/ranger/tripo-pants-fitted-v1-connections.png).
- [6종 대표 동작](../images/characters/modular_human_male_01/parts/ranger/tripo-pants-fitted-v1-motion.png).
- [달리기·앉기 앞뒤](../images/characters/modular_human_male_01/parts/ranger/tripo-pants-fitted-v1-rear-motion.png).
- [게임 동작 수치 검사](modular-ranger-tripo-pants-animation-v1.json).
- [허리 단면 검사](modular-ranger-tripo-pants-waist-review-v1.json).
- [Blender 편집본·검수 이미지 기록](modular-ranger-tripo-pants-review-v1.json).
- [브라우저 검수](modular-ranger-tripo-pants-workshop-v1.json).

대기·걷기·달리기·점프·베기·전투 대기·앉기 7종을 각 13시점에서 검사한다.
유한 좌표·UV 이음 유지·리그 바인딩과 허리 겹침을 확인하고, 앞뒤 착용 화면을 별도로 검수한다.
허리 검사는 골반 좌표계의 5개 높이·120방향에서 양쪽 표면을 만나는 광선만 비교한다.
올라온 다리가 허리 측정을 오염시키지 않도록 원래 높이가 1.0m 이상인 바지 면을 대상으로 한다.
조끼의 트임에 해당하는 광선은 비교하지 않는다. 이 검사는 모든 삼각형의 교차나 다른 장비 호환을 보장하지 않는다.

## 제작실 연결과 가림

`modular-character-preview.html?outfit=ranger&fit=pants-v1`에서 상의 v4와 이 바지를 함께 선택한다.
첫 카메라는 전신이며 무기는 끄고, 기존 crop 헤어와 맨발을 표시한다. 다른 상의·부츠는 별도 검수 대상이다.
바지가 표시되면 다리·발목과 기존 천 바지를 숨긴다. 발과 팔꿈치 아래 피부는 유지한다.
벗기·다른 바지로 교체·파일 로딩 실패 시 해당 장비 아래 피부를 복원한다. 기준 몸체 파일은 수정하지 않는다.
게임 출시 장비 등록과 다른 세트의 혼합 호환 합격은 아직 선언하지 않았다.

## 폴리곤 예산과 출처

바지는 원본과 같은 **3,714 triangles**를 유지한다. 숫자만 맞추기 위한 데시메이션은 하지 않았다.
현재 몸체·crop 헤어·상의·바지 합계는 숨김 포함 **23,434 triangles**, 제작실 표시 **16,584**,
얼굴 할당 **1,505**다. [실측 기록](modular-ranger-tripo-pants-budget-v1.json)을 참고한다.
제작실의 조합 수치는 목 가림으로 잘린 형상을 합산하므로 보관 GLB 합계와 다를 수 있다.
15,000–20,000 목표보다 숨김 포함 합계가 높으며 장갑·부츠·무기·망토는 포함하지 않았다.
최종 예산은 세트 완성 후 얼굴 디테일을 보존하며 함께 조정한다.

Tripo Studio 출력물 이용 조건과 기존 계정 출처 기록을 따른다. 사용자가 과거 약 USD 20/월 구독을
보고했으나 이 파일의 정확한 등급·생성일·모델 버전·작업 ID·제출 원화는 별도 확인되지 않았다.
2026-10-05는 전달·피팅일이다. 새 AI 생성이나 유료 API 호출은 하지 않았다.
원화는 [기존 생성 기록](modular-ranger-parts-sources.json)의 OpenAI Codex built-in ImageGen,
ChatGPT Pro 20x 출처를 따른다.

## 작업 파일 정리 — 2026-10-06

원본만 전시한 이미지와 최종 동작 시트에 중복되는 브라우저 동작 캡처 3장을 삭제했다.
임시 렌더 로그·브라우저 검사 스크립트·브라우저 프로필도 정리했다.
원본 GLB·최종 피팅 GLB·Blender 편집본·동작 표본·앞뒤 원화·최종 검수 이미지와 재현 도구는 보관한다.
삭제한 이미지의 해시와 크기는 [정리 기록](modular-ranger-tripo-pants-fitting-history.json)에 남겼다.

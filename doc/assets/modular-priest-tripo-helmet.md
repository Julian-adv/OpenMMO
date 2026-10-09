# 사제관 — 사용자 제공 Tripo 메시 피팅

2026-10-09. 사용자가 전달한 `Y:\public\web_downloads\bishop+mitre+3d+model.glb`를
`/mnt/y/web_downloads/bishop+mitre+3d+model.glb`에서 읽어 원본을 보관했다.
Tripo 구독 등급·생성일·작업 ID·실제 제출한 시점 조합은 확인되지 않았다.
원화는 [사제관 앞·뒤·옆면](modular-priest-helmet.md)이며, 원본 모델은 기존 Tripo 출력물
이용 조건을 따른다. 새 AI 생성이나 재생성은 하지 않았다.

## 원본과 피팅

- 원본·출력: `assets/modular_human_male_01/priest/tripo_helmet_v1/{source,helmet_priest}.glb`.
- 편집본: 같은 폴더의 `priest-helmet-fitting.blend`. 원본은 숨김 컬렉션에 보관하고,
  현재 기본 남성 몸체·사제 상의와 게임 동작 표본을 함께 넣었다.
- 원본과 피팅 모델 모두 **1,924 triangles**. 예산을 맞추기 위한 감면은 하지 않았다.
- 기본 머리에 이마 띠를 맞추고, 실제 머리 표면 기준으로 가까운 정점은 3mm, 삼각형 중심은 2mm 여유를 목표로 보정했다.
  뒤쪽 천은 목·옷깃 뒤로 이동하고 Head→Neck→Spine2 가중치를 사용한다. 천 물리 시뮬레이션은 없다.
- 앞판 안쪽에 바깥 보석 무늬가 잘못 복제되어 있어 안쪽 면 211개의 UV를 기존 흰 천 영역에
  다시 연결했다. 외부 UV와 원본 텍스처 바이트, 삼각형 개수는 보존했다.
- 현재 공통 몸체의 65개 본 계층·역바인드 행렬이 일치하고 가중치 합계 검사를 통과했다.

## 검수와 범위

[제작 미리보기](https://localhost:10004/modular-character-preview.html?outfit=priest)에서
사제 복장을 열면 사제관도 착용한다. 헬멧 선택에서 착탈할 수 있다.
현재 연결 범위는 제작 미리보기이며, 사제관 아이템 등록·시작 지급은 포함하지 않는다.

기본 얼굴로 브라우저 로딩, 걷기·달리기·점프·베기·앉기 전환, 사제관 해제·재장착을
확인했고 pageerror는 없었다. `npm run check`, `npm run lint`를 통과했다.
7개 동작을 각 13개 시점으로 검사해 유한 좌표·UV 경계 벌어짐·리그 연결을 확인했다.
별도 Blender 표본 렌더로 머리·옷깃과 뒤쪽 천을 검토했다.
숫자 검사는 모든 자세의 옷 관통을 보장하지 않으며 다른 얼굴과 혼합 복장은 추가 검수가 필요하다.
점프에서 뒤쪽 천의 최대 모서리 늘어남은 약 1.52배여서 동적 천 표현의 개선 여지가 있다.

브라우저의 현재 전체 사제 조합은 숨김 포함 **23,011 triangles**, 표시 **14,680 triangles**,
기본 얼굴 **1,505 triangles**였다. 숨김 포함 합계는 15,000–20,000 목표보다 높다.
사제관 자체는 목표 약 2,000 triangles 이내다.

![원본 3면 검수](../images/characters/modular_human_male_01/parts/priest/tripo-helmet-source-v1.png)
![기본 머리 피팅](../images/characters/modular_human_male_01/parts/priest/tripo-helmet-fitted-v2.png)
![동작 표본](../images/characters/modular_human_male_01/parts/priest/tripo-helmet-motion-v2.png)
![브라우저 미리보기](../images/characters/modular_human_male_01/parts/priest/tripo-helmet-workshop-v2.png)

검수 이미지는 제공 GLB를 Blender·브라우저로 렌더한 것으로 같은 출처·이용 조건을 따른다.

- [원본 메시 수치](modular-priest-tripo-helmet-source-review-v1.json)
- [출처·해시·피팅 기록](modular-priest-tripo-helmet-fitting-v2.json)
- [게임 동작 수치](modular-priest-tripo-helmet-animation-v2.json)

```bash
.venv/bin/python tools/fit-tripo-priest-helmet.py
node tools/validate-tripo-rogue.mjs --directory assets/modular_human_male_01/priest/tripo_helmet_v1 --part helmet_priest --report doc/assets/modular-priest-tripo-helmet-animation-v2.json
blender -b -t 6 --python-exit-code 1 --python tools/blender-scripts/review_tripo_priest_helmet.py -- --raw
blender -b -t 6 --python-exit-code 1 --python tools/blender-scripts/review_tripo_priest_helmet.py
```

## 머리 크기 재피팅 — v2

사용자가 이마 앞쪽이 뜨고 모자가 크다고 지적해 가로와 세로 스케일을 약 14%,
앞뒤 스케일을 약 15% 줄였다. 이마 띠의 아래 높이는 유지하고, 머리 표면과 삼각형 중심의
간격을 함께 검사해 축소로 생긴 뒤통수 관통을 보정했다.
원본·외부 UV·1,924 triangles·리그를 유지한다. 안쪽 띠 표본 간격은 v2 피팅 기록에 남겼다.

**[미사용]** v1 피팅은 머리에 비해 커서 v2로 교체했다. 이전 모델·검수 이미지·수치 기록은
정리하고 원본과 최종 v2 자료를 보관했다. 재생성 가능한 `validation-poses.json`도 삭제했다.
동작 표본 렌더에 사용하는 `animation-snapshots.json`은 유지한다.

## 사제관 아래 헤어 표시

2026-10-09. 사제관은 선택된 헤어를 남기고 머리 윗부분만 가린다.
띠 안쪽으로 헤어 뿌리를 압축하고, 뒤쪽 머리의 깊이를 줄여 사제관과 겹치지 않게 한다.
순찰자·웨이브 장발을 선택하면 뒤로 내려오는 천도 머리 바깥으로 이동한다.
착탈·헤어 교체 시 원본 형상을 복원하며 공유 원본 GLB는 변경하지 않는다.
사제관 로딩 실패 시에는 원래 헤어를 표시하고, 판금·바바리안 투구의 기존 가림은 유지한다.

기본 얼굴에서 순찰자·웨이브 장발의 앞뒤 모습과 사제관 착탈을 브라우저로 검수했다.
클리핑·복원·공유 메시 독립성과 장발 해제 시 뒤쪽 천 복원은 자동 테스트로 확인한다.
아래 이미지는 기존 헤어·사제관을 브라우저에서 렌더한 것으로 원본의 출처·이용 조건을 따른다.

![순찰자 장발 앞면](../images/characters/modular_human_male_01/parts/priest/tripo-helmet-ranger-hair-v1.png)
![순찰자 장발 뒷면](../images/characters/modular_human_male_01/parts/priest/tripo-helmet-ranger-hair-back-v1.png)

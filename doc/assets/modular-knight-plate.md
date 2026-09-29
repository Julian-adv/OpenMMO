# Modular Knight Plate — 기사 시작 복장

2026-09-29 제작. 남성 모듈러 기사의 생성·선택·게임·감정표현 화면에 적용한다.
은색 판금 갑옷·바지·신발·장갑·열린 헬멧의 5종 세트다. 헬멧을 쓰면 헤어를 숨기고
얼굴은 유지한다. 다른 남성 직업은 기존 천 복장을 사용한다.
현재는 직업별 기본 외형이며 인벤토리 아이템 지급·능력치·방어구 슬롯 연동은 포함하지 않는다.

## 파일

아래 파일명은 제작용 `assets/modular_human_male_01/parts/fitted/`와
게임용 `client/public/models/characters/modular_male/`에서 같다.

| 파일 | 구성 | Triangles |
| --- | --- | ---: |
| `top_plate.glb` | 흉갑·등판·목깃·어깨·팔·허리 판금 | 3,101 |
| `pants_plate.glb` | 허벅지·무릎·정강이 판금과 관절 안감 | 1,354 |
| `boots_plate.glb` | 좌우 발목 높이 판금 신발 | 709 |
| `gloves_plate.glb` | 손가락 관절을 따르는 좌우 판금 장갑과 손목 테두리·안감 | 1,404 |
| `helmet_plate.glb` | 얼굴이 보이는 열린 헬멧 | 837 |

- 편집 원본: `assets/modular_human_male_01/parts/fitted/plate_parts.blend`.
  몸체와 새 파츠를 한 리그에 연결하고 텍스처를 내장했다. 기존 `character_parts.blend`는 별도로 유지한다.
- 재가공 입력: `assets/modular_human_male_01/parts/plate_sources/`의 같은 이름 GLB 5개.
- 원화: `doc/images/characters/modular_human_male_01/parts/*_plate.png` 5개.
- 실제 생성 프롬프트·설정·작업 ID·크레딧·SHA-256: [출처 기록](modular-knight-plate-sources.json).
- 배포 압축본의 입력·출력 해시: [게임 manifest](../../client/public/models/characters/modular_male/manifest.json).

몸체·5종 판금·철검의 숨김 포함 합계는 **21,227**, 표시 합계는 **10,282 triangles**다.
얼굴 지정 영역은 기존 **1,505 triangles**와 4096px 텍스처를 그대로 유지한다.
시작 복장에는 망토가 없다. 기본 절차형 망토를 추가하면 120이 늘어
합계 **21,347 / 표시 10,402 triangles**가 된다.
목표 15,000–20,000보다 조금 많지만 재생성한 상의의 표면 품질을 유지하고 수치만 맞추기 위한 감축은 하지 않았다.

## 재생성

저장소 루트에서 실행한다. 프로젝트 `.venv`의 NumPy·Pillow와 클라이언트 의존성이 필요하다.

```sh
.venv/bin/python tools/fit-modular-plate.py
blender -b --python-exit-code 1 -P tools/blender-scripts/pack_modular_plate.py
node tools/prepare-modular-character.mjs
```

Windows 환경에서는 `.venv/Scripts/python.exe`를 사용한다.
파츠는 기존 `human_male_01_mixamo_candidate_v2`의 65본·기준 행렬을 그대로 공유한다.
상의를 A 자세로 맞추고 목깃·손목을 조정했으며, 하의의 무릎과 발목을 맞췄다.
장갑은 손가락 기준점으로 변형하고 반대 손을 대칭 제작했다. 몸체에서 가중치를 옮기고,
헬멧은 Head 본에 고정했다. 표면 법선·접선과 미사용 GLB 데이터를 정리했다.
게임 압축은 텍스처만 처리한다.

2026-09-29 발목 이음 보정: 신발 입구를 좌우 정강이 본의 중심선에 맞추고,
바지 끝단을 신발 안으로 들어가도록 좁혔다. 겹치는 구간은 같은 높이별
Leg/Foot 가중치를 사용해 발목 회전 시 두 파츠가 따로 움직이지 않게 했다.
발끝·밑창 형상과 폴리곤 수는 유지하며, 추가 AI 생성·크레딧 사용은 없다.

2026-09-29 손목 이음 보정: 팔 갑옷 끝단의 중심을 좌우 ForeArm–Hand 축에 맞추고,
장갑 입구를 같은 축으로 정렬했다. 겹치는 구간은 축 방향 거리에 따라 같은
ForeArm/Hand 가중치를 사용한다. 손가락 기준점·리그는 유지했다.

후속 손목 마감 보정: 벌어진 기존 커프를 잘라내고 연결된 금속 테두리와 둥근 모서리,
안쪽 틈을 메우는 어두운 패딩 면을 만들었다. 손목 주변 원본의 불필요한 돌출 면도 제거했다.
장갑은 1,038에서 **1,404 triangles**로 늘었으며 양손 모두 닫힌 메시다.
재생성 시 열린 모서리·비정상 연결·뒤집힌 면·퇴화 삼각형이 있으면 실패하도록 검사한다.
추가 AI 생성·크레딧 사용은 없다.

개발 서버의 `/modular-character-preview.html`에서 판금 세트로 시작한다.
각 파츠를 개별 교체할 수 있고, `기사 판금 세트 입기`로 전체를 다시 적용한다.

## 출처와 라이선스

- 원화 5종: OpenAI Codex built-in ImageGen, **ChatGPT Pro 20x**, 2026-09-29.
  OpenAI 생성 출력물 이용 조건 적용. 새 원화는 모두 작업 공간에 보관한다.
- 3D·PBR: Meshy Image to 3D API, **Meshy Premium**, `meshy-7.1`, 2026-09-29.
  상의는 4K 형상, 나머지는 2K 형상, PBR 텍스처는 모두 2K다.
  목표 triangles는 상의 3,000·바지 1,300·신발 700·한쪽 장갑 500·헬멧 800으로 나눴다.
  채택 5종 175크레딧, 미채택 상의 35크레딧을 포함해 총 **210크레딧** 사용.
  [Meshy 유료 생성물 이용 조건](https://help.meshy.ai/en/articles/10137554-what-is-the-ownership-of-the-generated-models) 적용.
  Meshy Community에는 게시하지 않았다.
- 몸체·리그·동작은 기존 [모듈러 남성 출처](modular-human-male-01.md)를 따른다.
  새 리깅 서비스·AI 애니메이션은 사용하지 않았다.
- **[미사용]** 첫 상의 생성물은 가슴·어깨 표면이 거칠어 재생성본으로 교체했다.
  원본 해시·작업 기록만 출처 JSON에 남겼다.

## 검증

- 게임 압축 GLB 5종: Khronos glTF Validator 오류·경고·정보·힌트 0.
- 게임 로더에서 51클립 × 5시점 = 255포즈: 유한 정점·체형 범위·공유 65본 검사 통과, 브라우저 오류 0.
- 대기·걷기·달리기·점프·전투 대기·공격·쓰러지기와 얼굴·검 그립 확대를 렌더 확인했다.
- 발목 보정 후 전투 대기·대기·걷기·달리기·점프의 발목 확대를 비교 확인했다.
- 손목 보정 후 양손의 기본 자세·대기·걷기·달리기·전투 대기·공격 확대와
  추가 손목 회전을 확인했다. 압축 GLB·255포즈 검사와 로더 테스트 5개도 다시 통과했다.
- 손목 마감 보정 후 입구 안쪽을 내려다보는 각도에서도 양손을 확인했다.
  장갑의 열린 모서리·비정상 연결·면 방향 불일치·퇴화 삼각형은 모두 0개다.
- 관련 Vitest 36개, `npm run check`, `npm run lint` 통과.

관절 판금은 현재 스킨 가중치로 움직인다. 확대 시 손가락과 관절의 낮은 폴리곤 윤곽이 보이며,
검증은 모든 장비 조합과 모든 프레임의 관통이 없다는 뜻은 아니다.

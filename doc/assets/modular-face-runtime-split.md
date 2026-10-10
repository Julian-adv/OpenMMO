# 공통 몸체와 얼굴 파츠 분리 — 2026-10-10

남성 조립식 캐릭터는 공통 몸체 `base.glb`와 선택한 `face_*.glb`를 조립한다.
직업과 얼굴에 관계없이 몸통·팔다리·골격·피부 텍스처를 공유하며,
생성·선택 화면, 게임 플레이어와 감정표현 미리보기가 같은 `loadCharacterModel` 경로를 사용한다.

## 구성

| 게임용 파일 | 내용 | bytes |
| --- | --- | ---: |
| `base.glb` | 공통 몸통·팔다리 8개 메시와 골격, 피부 텍스처 3장 | 1,097,744 |
| `face_default.glb` | 기본 머리·목 2개 메시, 텍스처는 몸체 참조 | 127,932 |
| `face_rugged.glb` | 각진 얼굴·목·연결부 4개 메시, 고유 텍스처 2장 | 465,328 |
| `face_ranger.glb` | 순찰자 얼굴·목·연결부 4개 메시, 고유 텍스처 2장 | 1,351,552 |

얼굴별 목 법선·재질과 연결 메시를 유지하기 위해 `head`, `neck`, `face_neck_bridge`
영역을 얼굴 파츠에 함께 넣는다. 몸체에는 이 영역을 포함하지 않는다.
공통 피부 이미지는 얼굴 GLB에 중복 저장하지 않는다. 재질의 `modular_body_textures`에
공유할 슬롯을 기록하고 `bindModularFace`에서 공통 몸체의 실제 Texture 객체를 연결한다.
얼굴 고유 이미지와 목의 roughness·metalness 등 재질 설정은 유지한다.

`loadGLB`의 기존 캐시를 사용하므로 여러 얼굴을 선택해도 공통 몸체는 한 번만 내려받는다.
조립 결과는 얼굴별로 캐시하고 각 캐릭터는 독립된 골격을 갖는다. 눈색·머리색은 기존처럼
캐릭터별 재질로 적용한다. 공통 몸체 지오메트리와 텍스처는 얼굴 간에도 공유한다.

기존 게임용 `base_rugged.glb`, `base_ranger.glb`는 **[미사용]**이며 출력 폴더와 manifest에서
제거했다. 피팅·제작실 비교용 `assets/.../base_rugged.glb`, `assets/.../base_ranger.glb`는
목 보정과 전체 조립 상태를 보존하는 제작 원본으로 유지한다.

## 생성과 품질

```bash
node tools/prepare-modular-character.mjs --body-only
node tools/prepare-modular-character.mjs --part face_ranger
```

`tools/split-modular-body.mjs`가 기존 피팅 원본의 부위를 분리하고 공통 피부 이미지 참조를
기록한다. 이어서 기존 최적화 도구로 Meshopt와 WebP/JPEG 압축을 적용한다.
기본·각진 얼굴과 공통 몸체는 기존 1024px 정책을 사용한다. 순찰자 얼굴은 기존
2048px, WebP quality 95를 유지한다. 얼굴 3종의 최종 색상 이미지 바이트는 전환 전과 같다.
순찰자 몸체·아래 목에 중복되었던 4096px 피부 이미지는 공통 1024px 피부 이미지로 통일한다.

이 작업은 로딩 구조 변경이다. 몸체·얼굴의 KTX2 전환은 포함하지 않는다.
복장의 KTX2 설정은 유지한다.

출처·라이선스와 생성 구독 조건은 [기본 몸체](modular-human-male-01.md),
[각진 얼굴](modular-rugged-face-wavy-hair.md), [순찰자 얼굴](modular-ranger-tripo-face.md)의
기존 기록을 따른다. 새 모델·이미지 생성이나 유료 API 사용은 없다.

## 검증

- 이전 완성 몸체와 분리 후 재조립한 3종의 정점 속성, UV, 스킨 가중치, 방향을 유지한
  삼각형, 노드 메타데이터와 변환이 일치한다. Meshopt는 삼각형의 시작 인덱스를 회전할 수 있다.
- 목 보정은 얼굴 파츠에 보존했다. 얼굴별 모든 신발 높이 검사를 통과했고 sole offset은 동일하다.
- 얼굴·몸체 조립 및 피부 텍스처 공유 회귀 테스트를 추가했다. 관련 테스트 122개 통과.
- `npm run check` 오류·경고 0건, `npm run lint`, `vite build` 통과.
- Chromium SwiftShader WebGPUBackend와 WebGLBackend에서 얼굴 3종, 헤어 4종,
  기사·사제 투구 착탈과 대기·걷기·달리기·점프를 렌더했다. 페이지·에셋·GPU 오류 0건.
  공통 몸체 요청 1회, 얼굴별 파츠 요청 1회와 공통 지오메트리·Texture 객체 공유를 확인했다.
  실제 GPU 하드웨어 성능은 측정하지 않았다.
- 변경 전후 얼굴·목 확대 렌더와 눈색 적용을 확인했다.

3종 몸체를 모두 받던 경우의 합계는 9,960,856 → 3,042,556 bytes로 약 69.5% 줄었다.
공통 몸체를 받은 뒤 추가 얼굴 선택 시에는 각 얼굴 파츠만 필요하다.
파츠별 해시와 검증 결과는 [기록](modular-face-runtime-split-v1.json)에 보관한다.

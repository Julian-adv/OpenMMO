---
name: deploy-prod
description: Deploy OpenMMO to production or prepare a production village-data transfer. Use when the user asks to deploy, ship, or apply public village changes to prod; preserve production player data and verify the complete facility, terrain, and vegetation changes.
---

# Deploy OpenMMO

Use the shared [deployment workflow](../../../.claude/commands/deploy.md).
Read `~/work/notes/DEPLOY_NOTES.md` before preparing a deployment and retain its
production-data preservation rules. Follow the user's requested scope; preparing
or editing deployment instructions does not authorize a production deployment.

## Public village transfers

공용 시설이나 구역 하나를 이관 단위로 삼는다. 배포마다 요청된 마을 편집, 배포 메모,
마지막 이관 기록과 해당 구역의 개발·운영 데이터를 비교해 신규·수정·이동·삭제를 찾는다.
기록이 없으면 현재 양쪽 데이터를 비교해 범위를 정한다. 대부분 Git에서 제외된 데이터이므로
깨끗한 작업 트리나 같은 수정 시각만으로 변경 없음을 판단하지 않는다.

### 변경 목록과 보존

각 시설의 이름, 건물·오브젝트 ID, 이동 전후 월드 범위, 대상 경로·운영 해시·셀·ID를
기록한다. 아래 계층마다 `추가/수정/이동/삭제/변경 없음/제외`와 근거, 기대 상태를 정한다.
관련 변경을 설명 없이 제외하지 않는다. 이관이 없을 때도 비교 근거를 남긴다.

| 계층 | 함께 확인할 데이터 |
| --- | --- |
| 건물 | `data/housing/`의 origin·방·층·소유자·출입구. 운영 문 상태와 개인 주택을 보존한다. |
| 가구·전시물·간판 | `data/terrain/objects/`의 ID·종류·위치·층·문구. 플레이어 보관함·내용물은 DB에도 있으므로 건물 JSON만으로 판단하지 않는다. |
| 높이 | `data/terrain/height/`의 바닥 높이·외곽 연결·공유 경계와 복원용 `height-original/`. |
| 바닥 재질 | `data/terrain/splat/`과 `landscaping/`. 조경이 있으면 조경의 재질이 우선한다. |
| 풀·나무 | `data/terrain/grass/`, `trees/`, 조경 제거 마스크를 함께 적용한 결과와 복원용 `grass-original/`. |
| 시설 설정 | 상점 전시 ID·가격, NPC 설정·스케줄, 지도 표시, 필요한 예약 토지와 실제 배치의 일치. |

- 공용 대상은 요청·편집 기록·시설 문서로 식별한다. 개발의 `ownerId`나 같은 지역 파일에
  들어 있다는 이유로 개인 건물·가구를 공용으로 취급하지 않는다.
- 현재 운영 데이터에 선택한 셀·ID만 병합한다. 개발 DB·영지·집·가구·목책·지형을 일괄
  복사하지 않는다. 파일 하나에도 공용 구역과 운영 개인 편집이 섞일 수 있다.
- `tools/sync-terrain.sh`의 수정 시각 비교는 후보 탐색일 뿐이다. 선택 이관에 전체
  `--apply`·`--delete`를 사용하지 않는다. 해시는 대상 파일만 비교한다.
- 개발에 없다는 이유로 운영 소유물을 삭제하지 않는다. 의도된 이동·철거는 옛 자리 복원도
  포함하되 이웃 시설과 이후 운영 편집을 보존한다.
- 지난 배포 뒤 운영에서 보정한 내용을 기준에 포함한다. 오래된 개발 파일이나 양쪽 편집의
  충돌을 발견하면 자동 덮어쓰기를 중단하고 해당 범위를 다시 병합한다.

### 좌표와 적용 전 검증

지형은 `tile = floor((world + 32) / 64)`, `tileOrigin = tile * 64 - 32`,
`cellIndex = localZ * 64 + localX`를 사용한다. 높이맵은 65×65 정점, 스플랫·풀·제거
마스크는 64×64 셀이다. 음수 좌표와 경계 타일은 [좌표 구현](../../../terrain/src/coords.rs)을
따르고, 기록한 월드 범위를 셀 인덱스에서 역산해 대조한다. 타일 중심과 원점을 혼동하면
32m 어긋난 패치가 된다.

운영 원본에 패치를 적용한 임시 결과에서 다음 조건을 수치로 검사한다. 건물·가구만
바뀌었거나 개발·운영 파일이 같아도 생략하지 않는다. 개발 원본에도 문제가 남아 있을 수 있다.

- **바닥:** 지상층 지형이 실제 바닥 윗면보다 낮은지 높이 저장 단위의 반올림까지 적용해
  검사한다. 현재 [건축 처리](../../../server/src/game_state/house_building.rs)는 건물
  `origin.y`에 맞추고 외곽 4m를 연결한다. [평탄화](../../../terrain/src/height.rs) 시
  이웃 기초를 보호하고 타일 공유 경계 정점을 일치시킨다.
- **식생:** 현재 [건축 범위](../../../server/src/housing/routes.rs)의 방 주변 풀 1m·나무
  2m에 표시될 식생이 없는지 검사한다. GR04는 셀별 개수이므로
  [셀 교차 판정](../../../terrain/src/grass.rs)과 조경 제거 마스크를 함께 적용한다.
  건물 JSON 직접 복사는 평탄화·식생 제거를 실행하지 않는다.
- **포장:** 돌길·포장은 재질과 제거 마스크를 함께 확인한다. `vegMeta=0`만으로 기존 풀이
  사라지지 않는다. `LND1` 마스크와 풀 원본을 합쳐 표시 대상 0개를 확인한다. 마스크는
  나무도 숨기므로 보존할 나무가 있으면 풀 데이터만 수정하는 등 범위를 조정한다.
- **배치:** 가구·간판·NPC의 건물·층·높이와 설정의 참조 ID를 확인한다. 건물 높이를
  바꾸면 배치물 높이도 확인한다.
- **보존:** 대상 밖의 셀·배치·운영 소유 데이터를 비교한다. 변경하지 않는 파일은 해시로,
  공유 파일은 선택한 셀·ID를 제외한 내용으로 검증한다.

조건이 실패하면 해당 이관을 고친 뒤 다시 검사한다. 의도된 예외는 범위와 이유를 기록한다.

### 적용과 완료 확인

1. 수정할 운영 파일·행과 버전 메타데이터를 별도 배포 기록 디렉터리에 백업한다. 쓰기 직전
   원본 해시·행 내용이 준비 시점과 다르면 최신 운영 상태로 패치를 다시 만든다.
2. 지원되는 편집 API를 우선 사용하고 복원용 높이·풀을 보장한다. 파일 교체는 원자적으로
   수행한다. 건물·설정 파일 변경은 서버·NPC 캐시 갱신 또는 계획된 재시작까지 처리한다.
3. 지형 변경 후 해시 목록과 WebSocket 버전을 게시한다. 높이 편집은 서버 높이 캐시도
   갱신한다. 외부 파일 변경만으로 메모리 manifest가 갱신되지는 않는다.
   [지형 서빙](../../../doc/TERRAIN_STATIC_SERVING.md)의 처리 경로를 따른다.
4. 공개 manifest·원본 바이트·운영 디스크를 대조하고, 공개 데이터로 바닥·식생·포장 조건과
   대상 밖 보존을 다시 검사한다. 복사 성공이나 해시 일치만으로 완료하지 않는다.
5. 게임에서 바닥 노출, 남은 풀·나무, 포장 경계, 출입구·가구와 변경한 시설 기능을 확인한다.
   접근할 수 없으면 데이터 검증과 화면 미확인을 분리해 기록하고 마을 검증 완료로 표시하지 않는다.
6. 배포 메모에 결과·복구 경로·미확인 항목을 갱신한다. 운영 보정의 개발 반영 여부도 기록한다.
   개발 반영 시 개인 데이터를 보존하고, 미반영이면 다음 배포의 운영 보존 대상으로 명시한다.

복구도 변경한 셀·ID를 기준으로 한다. 이후 운영 편집을 확인하지 않고 백업 전체로 덮어쓰지 않는다.

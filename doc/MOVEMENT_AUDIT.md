# 이동 충돌 진단

최초 벽 밀기의 원인을 조사하기 위해 서버가 수락한 이동 명령과 이동 틱을 연결한다. 이동·충돌·강제 종료 정책은 바꾸지 않는다. 프로토콜 변경이나 클라이언트 업데이트 없이 서버 배포부터 수집된다.

## 로그

기본 `info` 로그 수준에서 다음을 수집한다.

- `Movement session joined` (`movement_audit`, info): `player_id`, 영속 `character_id`, `account_session_id`, 클라이언트 종류·신고 버전, 진입 위치·방향·층. 신고 버전은 최대 128자로 보관하며 진위 검증값이 아니다.
- `Movement collision trace` (`movement_audit`, warn): `detail=` 뒤의 단일 JSON 객체. 최초 차단 보정 때 출력하고, 이후 플레이어별 30초 간격으로 제한한다. 기존 위치 보정의 2초 제한도 적용된다.
- 기존 `Blocked move for player ...`: `player_id`와 `request_id`를 추가한다. 상세 출력 사이의 차단도 같은 명령 ID로 연결할 수 있다.

`player_id`는 접속별로 달라지고 `character_id`는 재접속 뒤에도 같다. `request_id`는 서버 프로세스 안에서 발급하는 이동 명령 ID이며 서버 재시작 뒤에는 재사용될 수 있다. 로그의 서비스 실행 구간도 함께 구분한다.

## 상세 필드 (`schema: 1`)

| 필드 | 의미 |
| --- | --- |
| `server_layout`, `block_key`, `stairwell`, `sealed` | 서버 레이아웃과 차단 대상·계단·갇힘 판정 |
| `step.pose`, `step.attempted`, `step.step_floor` | 마지막 안전 위치·방향·층·탑승 여부, 시도 위치, 충돌 캐시 층 |
| `step.dt`, `step.speed`, `step.at_ms` | 해당 틱 시간 간격, 적용 이동 속도, 처리 시각 |
| `step.intent` | 차단된 실제 큐 항목: 목표·층·방향·달리기·충돌 검사·말 회전/후진 여부와 원래 요청 |
| `step.intent.request.raw` | 클라이언트 `PlayerMove`의 위치·방향·층·append·sprinting 원본 |
| `request.received_ms`, `request.queued_ms` | 서버 이동 핸들러 시작·큐 삽입 시각(Unix ms). 클라이언트 전송 시각은 아님 |
| `request.received_pose` | 명령 처리 초기에 읽은 서버 위치·방향·층·탑승 상태 |
| `request.leg_start`, `request.leg_floor`, `request.target` | 거리·층 검증 기준점과 서버가 높이 등을 보정한 목표 |
| `request.queue_before`, `request.replaced`, `request.dropped` | 삽입 전 큐 길이, 기존 큐 교체 여부, 큐 포화로 버린 명령 ID |
| `request.corrections_issued_before_queue` | 이 명령을 큐에 넣기 전까지 서버가 발행한 위치 보정 수. 클라이언트 수신/처리 확인은 아님 |
| `step.queue` | 차단 순간 아직 남아 있던 경유지의 ID·목표·층. 이 큐는 차단 후 폐기됨 |
| `step.geometry` | 차단 객체 원점, 출발·시도 위치의 로컬 셀, 주변 3×3 충돌 마스크, 그리드 높이 |
| `history.requests` | 최근 수락된 명령 최대 16개 |
| `history.ticks` | 최근 이동 틱 최대 16개. 시작/종료 위치, 속도, clear/slid/blocked, 틱 시작 큐 머리/마지막 조회 명령 ID, 처리 후 남은 큐 길이 |
| `history.corrections`, `history.last_correction` | 이번 차단의 보정을 발행하기 직전까지의 보정 수와 마지막 보정 시각·위치·층 |
| `history.requests_evicted`, `history.ticks_evicted` | 보관 한도로 밀려난 레코드 수. 0이 아니면 전체 이력이 아님 |

`request` 표의 필드는 `step.intent.request`와 `history.requests`에 공통이다. 말 회전·후진 등 서버 생성 큐 항목은 `request=null`이며 `turn_only`·`recovery`로 구분한다. 원래 요청은 실제 큐 항목에도 보관하므로 최근 16개 이력에서 밀려나도 차단된 명령의 원본은 남는다.

충돌 마스크는 공유 passability grid의 원래 바이트다. 해당 캐시 층의 그리드가 없으면 `neighbors=null`, 경계 밖 셀은 `mask=null`이다. 계단 중간 층의 별도 가상 인코딩을 일반 층의 셀로 오인하지 않는다. 좌표는 로그 표시용 소수 첫째 자리 반올림 없이 직렬화한다.

## 조사 순서

1. 같은 `character_id`의 세션 로그와 상세 충돌 로그를 찾는다.
2. `step.intent.request`로 실제 차단된 원래 명령을 찾고, 원본 목표와 서버 보정 목표를 비교한다.
3. 수신 당시 위치와 `step.pose`를 비교한다. 큐 대기 중 이동한 거리, 교체/포화, 한 틱에서 여러 경유지 소비 여부를 본다.
4. `history.ticks`의 `slid`·`blocked`와 탑승 상태를 확인한다. 직선 목표와 실제 시도 궤적이 달랐는지 구분한다.
5. 이전 보정 시각·수와 후속 명령 ID·목표를 대조한다. 같은 큐가 남은 것인지, 보정 이후 새 요청이 들어온 것인지 확인한다.
6. 정적 던전은 `server_layout`과 좌표로 재현하고, 기록된 충돌 셀과 대조한다.

```bash
rg 'Movement session joined|Movement collision trace|Blocked move for player' server.log
```

이력은 모든 접속 플레이어에 대해 메모리에만 제한적으로 보관하고 퇴장 시 제거한다. 정상 이동을 매번 디스크에 쓰지 않으며, 상세 출력 제한 중에는 충돌 JSON·주변 그리드 덤프도 만들지 않는다. 한도는 레코드 수 기준이므로 명령이 몰리면 보관 시간은 짧아진다.

이 로그는 서버가 받은 명령과 처리 결과를 설명한다. 수락 전 거부된 명령 전체, 클라이언트의 최종 행동 목표·A* 경로·당시 캐시, 네트워크 단방향 지연은 복원하지 않는다. 이 정보만으로 클라이언트 변조나 사용자의 고의성을 확정하지 않는다.

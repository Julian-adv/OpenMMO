# 대상 캐릭터 전투 감사

의심 대상을 캐릭터 ID로 지정해 전투·회복을 관측한다. 게임 판정이나 보상은 바꾸지 않는다. 추적은 **수동 해제할 때까지** 유지하며, 서버 재시작 때 같은 설정 파일을 다시 읽는다.

## 설정

서버의 `--state-dir` 아래 `combat-audit.txt`에 캐릭터 ID를 한 줄에 하나씩 적는다. 기본 state-dir은 `./data`이다. 최초에 파일이 없으면 추적하지 않는다.

가상의 캐릭터 ID를 사용한 설정 예:

```text
1234
```

수동 해제하려면 해당 ID 줄을 삭제한다. 전부 해제하려면 파일 내용을 비운다.

- ID는 양의 정수이며 최대 128명을 지정한다. 빈 줄과 `#`로 시작하는 주석 줄은 무시하고 중복 ID는 합친다.
- 영구 캐릭터 ID를 사용하므로 이름 변경·재접속·서버 재시작 후에도 같은 캐릭터를 추적한다. 접속마다 달라지는 `player_id`와 구분한다.
- 로그 보관 기간은 서버 실행 옵션 `--combat-audit-retention-days` 또는 환경 변수 `COMBAT_AUDIT_RETENTION_DAYS`로 지정한다. 기본 30일, 최소 1일이며 **추적 만료 기간이 아니다.** 변경 시 서버를 재시작한다.
- 대상 파일은 서버 시작 시 읽고 이후 10분마다 다시 읽는다. 추가·해제는 다음 조회 때 반영된다. 임시 파일에 목록을 쓴 뒤 rename으로 교체하면 된다.
- 잘못된 ID 형식·파일 읽기 실패는 기존 설정을 유지하고 운영 로그에 경고한다. 시작 시 읽기가 실패하면 추적을 시작하지 않는다. 실행 중 파일 삭제도 해제로 취급하지 않는다.
- 대상 추가·삭제는 운영 로그의 `Combat audit targets updated`에서 확인한다. 배포만으로 특정 캐릭터를 자동 지정하지 않는다.

## 대시보드

운영자 대시보드의 **전투 기록 추적 대상**에서 현재 적용된 대상의 캐릭터 이름과 ID를
확인한다. 오프라인 대상도 포함하고, 이름은 조회 시점의 DB 값으로 표시한다. 삭제되었거나
존재하지 않는 ID도 목록에 남기며 이름 확인 불가로 표시한다.

화면은 매분 또는 수동 새로고침 시 갱신한다. API는 서버 메모리에 적용된 대상을 읽으므로
파일 수정 직후에는 기존 대상이 표시될 수 있다. 대상 변경은 기존 10분 재조회 주기에
반영되며, 파일 오류나 삭제로 기존 추적이 유지되는 경우 화면에도 그 대상을 계속 표시한다.
상세 API는 [운영 지표](METRICS.md#전투-기록-추적-대상)를 참고한다.

## 출력

`<state-dir>/combat-audit/combat-audit-YYYY-MM-DD.jsonl`에 대상의 1분 집계를 기록한다. 파일 날짜는 구간 시작 시각의 UTC 날짜이다. 접속 중에는 활동이 없는 구간도 기록한다. 로그아웃·수동 해제·정상 종료 때 남은 구간을 마감한다. 로그아웃한 구간은 다음 기록 주기(통상 1초 이내)에 저장한다.

| 필드 | 의미 |
|---|---|
| `schema`, `character_id`, `player_id`, `name` | 스키마 버전, 영구 캐릭터 ID, 접속 ID, 이름 |
| `start_ms`, `end_ms`, `reason` | Unix 밀리초, 마감 사유(`interval`, `logout`, `disabled`, `shutdown`) |
| `start_hp`, `end_hp`, `max_hp`, `level` | 시작·종료 HP, 종료 시점 최대 HP·레벨 |
| `health_gained` | 실제 증가한 HP를 `potion`, `food`, `natural`, `level_up`, `respawn`, `revive` 등으로 분리 |
| `health_lost` | 실제 감소한 HP를 `monster`, `debuff`, `death_penalty` 등으로 분리 |
| `deaths`, `level_ups` | 사망·레벨업 처리 횟수 |
| `monsters` | 몬스터 종류별 집계 |
| `player_attacks` | 플레이어가 보낸 공격 요청의 판정 건수와 요청별 상세 기록 (`schema: 2`부터) |
| `history_overflow` | 공격 이력 추적 상한에 도달했는지 여부 |

`monsters`의 각 종류에는 `server_attempts`, `client_requests`, `rejected`(사유별), `hits`, `misses`, `damage`, `kills`, `kills_without_observed_attempt`가 들어간다. 처치 관련 필드 외에는 **몬스터가 추적 캐릭터를 공격한 내역**이며, 플레이어의 공격 횟수·명중률과 구분한다.

- 공격 시도는 AI가 공격 명령을 실행하거나 클라이언트 요청이 들어온 횟수이다. AI의 탐색·접근·대기 자체는 시도로 세지 않는다. 클라이언트 요청은 서버 시도와 별도 집계한다.
- 거부 사유는 `missing_monster`, `not_controllable`, `cooldown`, `target_not_damageable`, `unreachable_floor`, `out_of_range`, `wall`, `target_disappeared_or_dead`, `client_disabled`이다. 없는 몬스터와 무시된 클라이언트 요청의 종류는 `unknown`이다.
- `damage`는 실제 HP 감소량이다. 남은 HP를 넘는 피해와 최대 HP를 넘는 회복을 제외한다. 일반적인 구간에서 `end_hp - start_hp = sum(health_gained) - sum(health_lost)`로 대조할 수 있다.
- `kills`는 본인이 마지막 타격을 가한 처치 수이다. 파티 공유 경험치 횟수가 아니다.
- `kills_without_observed_attempt`는 **추적 중 해당 캐릭터를 향한 공격 시도를 관측하지 못한 처치**다. 다른 사람에게 공격했거나, 추적 전에 공격했거나, 한 방에 죽은 경우도 포함한다. 이것만으로 악용을 판정하지 않는다. 시도 이력은 1분 경계를 넘어 유지한다.
- 몬스터 이력은 대상당 최대 4,096개이며 사라진 몬스터를 주기적으로 정리한다. 상한을 넘으면 해당 추적 세션의 `history_overflow`를 표시하고 이력이 없는 처치를 무반격 수치에 더하지 않는다.

### 플레이어 공격 요청

`player_attacks.requests`는 해당 구간에 판정이 기록된 요청 수이며, `outcomes`는 `accepted`, `cooldown`, `invalid_target`, `out_of_range`, `out_of_ammo`, `attacker_dead`, `not_in_game`, `interrupted`별 건수다. `accepted`는 대상 검사와 공격 간격 검사를 통과했다는 뜻으로, 명중·실제 피해 발생을 뜻하지 않는다. 판정 전 작업이 취소되면 추적 세션이 유지되는 경우 `interrupted`로 남는다.

`events`에는 캐릭터당 구간별 최초 256건을 기록한다. 초과한 요청도 전체·결과별 건수에는 포함하며, 상세 기록 생략 수를 `dropped_events`로 남긴다.

| 필드 | 의미 |
| --- | --- |
| `requested_at_ms` | 서버의 플레이어 공격 처리 진입 시각(Unix ms). 클라이언트 전송 시각이나 소켓 수신 시각이 아님 |
| `request_interval_ms` | 같은 추적 세션의 직전 공격 요청과의 간격. 대상 변경·1분 경계를 넘어 유지하며, 최초 요청은 `null` |
| `monster_id` | 요청 대상 ID. 최대 128자까지 기록 |
| `outcome`, `detail` | 허용·거절 결과와 대상 검증 거절 상세 사유(시체·레지스트리 부재 등) |
| `cooldown.checked_at_ms` | 서버가 공격 간격 판정에 사용한 시각 |
| `cooldown.since_accepted_ms` | 서버가 기억하는 직전 허용 공격 이후 경과 시간. 이전 허용 공격이 없으면 `null` |
| `cooldown.checked_interval_ms` | 이번 검사에서 적용한 최소 간격. 기본 1,380ms이며, 기본 검사 통과 후에는 배고픔 배율을 반영 |
| `cooldown.accepted` | 간격 검사 통과 여부 |

시체·거리 등의 검사에서 먼저 거절되면 `cooldown`은 `null`이다. 기본 간격보다 빠른 요청은 배고픔 조회 없이 먼저 거절하므로 이때 `checked_interval_ms`는 기본값이다. 허용된 공격은 배고픔까지 반영한 간격을 기록한다. 따라서 짧은 요청 간격과 짧은 **허용 공격 간격**을 구분할 수 있다. 공격 판정 규칙 자체는 바꾸지 않는다.

집계는 판정이 끝난 시점의 구간에 포함되므로 경계에서 시작된 요청은 다음 구간에 기록될 수 있다. 추적 해제·로그아웃 후 종료되는 요청은 해당 세션의 기록에 포함되지 않을 수 있다. 기존 `schema: 1` 로그에는 `player_attacks`가 없으며, 새 버전과 같은 날짜 파일에 섞일 수 있다.

파일 쓰기는 게임 상태 잠금 밖의 작업에서 수행한다. 쓰기 실패 시 오류를 남기고 마감 구간을 재시도한다. 미저장 구간은 최대 16,384개이며 초과하면 가장 오래된 구간을 버리고 오류를 남긴다. 강제 종료 시 진행 중이거나 아직 저장하지 못한 구간은 유실될 수 있다. 정상 종료에서는 남은 구간을 저장한다.

## 조사할 때

먼저 날짜 범위와 `character_id`를 선택하고, 구간별 HP 수지와 거부 사유를 확인한다. 명중률은 `hits / (hits + misses)`로 계산하고, 거부된 공격은 분모에서 제외한다. 몬스터별 피해와 회복원을 대조한 뒤 반격 시도가 없는 처치 비율을 함께 본다. 관측 시작·재접속·기록 실패·이력 초과 여부도 함께 확인한다.

구현: [combat_audit.rs](../server/src/game_state/combat_audit.rs).

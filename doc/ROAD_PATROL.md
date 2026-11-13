# 도로 순찰대

구현: 2026-11-13, 프로토콜 156. 출처: [Julian-adv/OpenMMO#208](https://github.com/Julian-adv/OpenMMO/issues/208).

공용 도로를 막은 목책을 경비병이 철거한다. 목책 설치 자체는 막지 않는다.

## 규칙

- 대상은 월드젠 도로 중심선에서 3m 이내에 중심이 놓인 영지 목책이다(`ROAD_CLEARANCE_M`). 운영자가 영지 밖에 놓은 목책은 제외한다.
- 서버 첫 기동 후 게임 시간 3일(실시간 9시간) 동안은 철거하지 않고 경고만 한다. 시행 시각은 `road_patrol_state.enforce_from`에 저장한다.
- 위반은 계정 단위로 센다. 순찰 한 바퀴가 한 계정에서 몇 개를 철거하든 1회다.
- 1회차는 압수한다. 경비대장 Karl에게서 물건 값의 절반을 내고 전부 돌려받는다.
- 2회차부터는 몰수한다.
- 철거할 때마다 Karl 명의의 편지가 주인에게 간다.

## 순찰대

| 조 | 대원 | 순찰 | 식사(여관) | 취침(경비대 사무소) |
|---|---|---|---|---|
| 주간 | Garrick, Brenna | 06:00–17:00 | 05:30–06:00, 17:00–18:15 | 18:15–05:30 |
| 야간 | Osric, Halda | 18:30–05:00 | 18:00–18:30, 05:00–05:45 | 05:45–18:00 |

- 경로는 앨더마크 북쪽 도로를 따라 해골 지하묘지 갈림목을 지나 도적 소굴 갈림목까지 1,145m이며 왕복한다.
- 한 바퀴는 게임 시간 약 2시간이다. 남은 순찰 시간이 `lap_minutes`보다 짧으면 새 바퀴를 시작하지 않고 도로 초입에 선다.
- 두 조가 사무소 침대 4·125번을 번갈아 쓴다. 여관 의자는 126–129번이다.
- 50레벨, 최대 HP 700, 가드 24, 룬 블레이드와 판금 한 벌. 죽으면 사무소에서 부활해 다음 바퀴를 시작한다.
- 조마다 `sweeps: true`인 한 명만 철거를 요청한다.

## 공지

- 서버가 Karl 이름으로 앨더마크 게시판(113번)에 한국어·영어 공지 두 장을 올린다(`server/src/game_state/guard_notices.rs`).
- 만료 5게임일 전에 새 글로 갈아 붙인다. 서버 시작 때와 순찰 중 한 시간에 한 번 확인한다.
- 문구를 바꾸면 다음 확인 때 옛 글을 내리고 새 글을 올린다.
- 순찰병 네 명은 위압적이고 딱딱한 말투를 쓴다(`agent-client/data/npcs/*/instance.txt`의 Bearing).

## 구현

- `server/src/road_index.rs`: `data/terrain/worldgen.json`의 도로 선분을 16m 격자에 담아 거리 판정.
- `server/src/game_state/road_patrol.rs`: `PatrolSweep`은 `guard` 클래스 공식 NPC만 보낼 수 있고, 경비병 주변 24m의 대상 목책을 철거한다. `ImpoundQuery`·`ImpoundReclaim`은 Karl 곁에서만 받는다.
- `server/src/auth_road_patrol.rs`: `guard_offences`(계정별 횟수·마지막 순찰), `guard_impounds`(계정·아이템별 수량), `road_patrol_state`.
- `shared/src/schedule.rs`: 일정 항목의 `patrol { partner, sweeps, lap_minutes }`. `waypoints`는 편도 경로다.
- `agent-client/src/driver/movement.rs::run_patrol_lap`: 경유지마다 철거 요청, 짝이 6m 안에 올 때까지 최대 20초 대기, 걷기. 백그라운드 작업이라 걷는 중에도 LLM이 말한다.
- `PatrolSweepResult`는 `[Patrol]` 이벤트로 LLM에 전달된다. 판정은 서버가 하고 LLM은 대사만 맡는다.
- 웹 클라이언트: Karl 우클릭 → 압수품 찾기(`ImpoundDialog.svelte`).

## 남은 일

- 플레이어 에이전트(봇)용 압수품 찾기 동작이 없다.
- 목책 외 물건(집·가구)의 철거와 압수.

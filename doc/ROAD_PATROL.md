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

| 시각 | 주간조(Garrick, Brenna)·Karl | 야간조(Osric, Halda) |
|---|---|---|
| 05:00 | 취침 | 순찰 종료, 여관 아침 |
| 05:15 | 기상, 여관 아침(Karl 포함) | |
| 05:45 | Karl은 사무소 앞, 주간조는 Karl에게 출발 보고 | 취침 |
| 06:05–17:00 | 순찰. Karl은 사무소 앞에 서고 매시 정각 마을을 한 바퀴 돈다 | |
| 17:00 | 여관 저녁(Karl 포함) | |
| 17:45 | Karl은 사무소 앞, 주간조는 야간조 침대 곁에서 깨움 | |
| 17:50 | | 기상 |
| 18:05 | 주간조 취침 | |
| 18:10 | | 여관 저녁 |
| 18:40 | | Karl에게 출발 보고 |
| 19:00–05:00 | Karl 19:15 취침 | 순찰 |

- 일정은 전부 시각으로 정해진다. 주간조가 숙소에 닿는 17:45가 야간조 기상 17:50보다 앞서도록 맞췄다. 주간조의 마지막 바퀴가 늦어지면 야간조는 혼자 일어난다.
- 깨우기·기상·보고 대사는 일정 항목의 `cue`가 낸다. 도착하면 `[Duty]` 이벤트로 LLM에 전달되고, 근처에 사람이 있을 때만 말한다.
- 경비병끼리는 군대식으로 짧게 말하고 Karl에게는 격식을 갖춰 보고한다(`instance.txt`의 Among the Guard).
- Karl의 `"*:00"` 순회는 일정 맨 앞에 두고 식사·보고·취침 구간을 뒤에 둔다. 같은 우선순위에서는 뒤 항목이 이긴다.
- 경로는 앨더마크 북쪽 도로를 따라 해골 지하묘지 갈림목을 지나 도적 소굴 갈림목까지 1,145m이며 왕복한다.
- 한 바퀴는 게임 시간 약 2시간이다. 남은 순찰 시간이 `lap_minutes`보다 짧으면 새 바퀴를 시작하지 않고 도로 초입에 선다.
- 사무소(`r-23_+73_2`, 6×8)에 침대가 다섯 개 있다. 남쪽 방 4(Osric)·125(Halda), 북쪽 방 130(Garrick)·131(Karl)·132(Brenna). 여관 의자는 126–129번, Karl은 133번이다.
- 50레벨, 최대 HP 700, 가드 24, 룬 블레이드. Garrick·Osric은 판금 한 벌, Brenna·Halda는 발키리 한 벌을 입고 모듈라 캐릭터로 그린다. Karl은 기존 `guard.glb` 그대로다(`NPC_MODEL_OVERRIDES`).
- 네 명 모두 알데마크 문장을 찍은 양모 망토를 두른다(`npcs.csv`의 `capeCrest`). 서버가 접속 때마다 loadout대로 입히고(`dress_npc`), 자리를 내준 옛 장비는 가방으로 간다.
- 사무소 정문 위에 간판 137번("Aldermark Watch House"), 그 위 박공에 휘장 134번, 문 양옆에 깃발 135·136번이 있다. 지역 오브젝트 파일의 `"emblem": {"texture": "aldermark", "color": "#1f4a34"}`로 문장을 지정한다. 죽으면 사무소에서 부활해 다음 바퀴를 시작한다.
- 조마다 `sweeps: true`인 한 명만 철거를 요청한다.
- 네 명은 `npcs.csv`의 `protects`가 켜져 있다. 20m 안에서 동료나 플레이어가 몬스터와 치고받으면 그 몬스터를 자기 전투 상대로 삼고, 40m 안이면 달려가서 싸운다(`record_nearby_combat`, `assist_target`). 끊긴 순찰 바퀴는 멈춘 경유지부터 잇는다.
- 평소에는 무기를 가방에 넣고 걷고, 전투가 시작되면 꺼냈다가 마지막 타격 10초 뒤 다시 넣는다(`weapon_command`).
- 이름 뒤에는 가문 이름 자리에 계급이 나온다(`npcs.csv`의 `rank` → `Player.rank`, 프로토콜 157, 문구는 `rank.<id>`). Karl은 경비대장(captain), Garrick·Osric은 조장(sergeant), Brenna·Halda는 경비병(guard).

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
- 일정 항목의 `cue`: 도착 직후 `check_schedule_transition`이 `[Duty] {cue}`를 넣는다.
- `agent-client/src/driver/movement.rs::run_patrol_lap`: 경유지마다 철거 요청, 짝이 6m 안에 올 때까지 최대 20초 대기, 걷기. 백그라운드 작업이라 걷는 중에도 LLM이 말한다.
- `PatrolSweepResult`는 `[Patrol]` 이벤트로 LLM에 전달된다. 판정은 서버가 하고 LLM은 대사만 맡는다.
- 웹 클라이언트: Karl 우클릭 → 압수품 찾기(`ImpoundDialog.svelte`).
- 말로 요청해도 열린다. Karl의 LLM이 `open_impound` 행동을 내면 `OfferImpound` → 서버가 Karl 곁에 선 플레이어에게 `ImpoundOffered`를 보내 같은 창을 띄운다. 멀리 있으면 Karl에게 `[Impound]` 안내가 간다.

## 남은 일

- 플레이어 에이전트(봇)용 압수품 찾기 동작이 없다.
- 목책 외 물건(집·가구)의 철거와 압수.

# NPC & Monster AI Architecture

## 대원칙

**서버는 client와 agent-client를 구분하지 않는다.** WebSocket으로 오가는 프로토콜은 완전히 동일하다. 서버 입장에서 NPC든 PC든 모두 같은 `Player`이고, 같은 `ClientMessage`/`ServerMessage`를 주고받는다.

## WS 스케일링

- 서버는 연결 상한 없음. 연결당 비용: tokio task 1개 (~1-2KB) + broadcast receiver
- Broadcast는 `tokio::broadcast` 사용, 메시지 직렬화 1회 + zero-copy 전달
- **100+ 연결 문제없음**, 10K도 OS 튜닝으로 가능
- 현재 프로토콜은 1 WS = 1 캐릭터 강제 → **NPC당 1 WS 유지**

## 아키텍처: Hybrid Orchestrator

```
Orchestrator Process
  ├── WS Connection 1 (NPC "경비병")  → SharedState_1 → 소유 몬스터
  ├── WS Connection 2 (NPC "상인")    → SharedState_2 → 소유 몬스터
  ├── WS Connection 3 (NPC "의뢰인")  → SharedState_3 → 소유 몬스터
  │
  ├── Monster AI Engine (60Hz tick, 각 연결의 소유 몬스터를 독립 구동)
  │     └── 상태머신: Idle → Patrol → Chase → Attack → Hit → Flee → Dead
  │     └── 몬스터 소유는 WS 연결(=player) 단위, 서버 전체 상한 공유
  │
  └── LLM Scheduler
        ├── Priority queue (urgent 이벤트 우선)
        ├── max_concurrent: 1~3 동시 LLM 호출
        └── NPC별 개별 system prompt + 대화 기억
```

### 몬스터 소유 모델

서버가 몬스터 스폰을 결정하고 소유자를 지정한다 (치팅 방지). 클라이언트는 할당받은 몬스터의 AI(이동/공격)만 담당한다.

**서버→클라이언트 흐름**:
1. 서버가 스폰 규칙에 따라 몬스터를 생성하고 owner_id를 특정 연결에 배정
2. `ServerMessage::MonsterAssigned { monster }` → 해당 클라이언트에 전달
3. 클라이언트는 할당된 몬스터에 대해 `MonsterMove`/`MonsterAttack` 전송
4. `RequestSpawnMonster`는 제거하거나 서버 승인 방식으로 변경

이 규칙은 웹 클라이언트와 agent-client 모두 동일하게 적용.

## 2계층 AI 시스템

| 계층 | 대상 | 방식 | 비용 |
|------|------|------|------|
| Deterministic | 몬스터 (patrol, chase, attack) | 상태머신 | 0원 |
| Deterministic | NPC 전투 (chase, attack loop) | 기존 tick_combat | 0원 |
| LLM | NPC 대화, 고수준 판단 | per-NPC LLM call | $$$ |

## 3계층 프롬프트 시스템

| 계층 | 파일 | 내용 | 갱신 |
|------|------|------|------|
| Template | `templates/guard.txt` | 역할 공통 프롬프트 (경비병의 일반 행동 규칙) | 개발자가 수동 |
| Instance | `instances/karen.txt` | 개체 고유 정보 (이름, 나이, 성격, 말투, 배경) | 개발자가 수동 |
| Memory | `memory/karen.txt` | 게임 내 경험 기억 (만난 사람, 사건, 감정) | LLM이 자동 갱신 |

LLM 호출 시 system prompt = `template + instance + memory` 순서로 결합.
Memory는 LLM 응답에 `memory_update` 필드를 추가하여 자동 갱신 (append 또는 요약 교체).

## LLM Scheduler 규칙

1. Urgent 이벤트 (NPC에게 말 걸기, 공격받음) → 즉시 호출
2. 동시 최대 N개 LLM 호출 (설정 가능)
3. Non-urgent NPC는 라운드로빈
4. 이벤트 없는 NPC는 스킵 (idle 시 1시간 간격)
5. 전투 액션은 LLM 대기 없이 deterministic 실행

## Config 구조

```toml
server = "ws://localhost:10015"

[[npcs]]
account = "npc_guard"
password = "..."
character_name = "경비병 카렌"
template_prompt = "data/prompts/templates/guard.txt"
instance_prompt = "data/prompts/instances/karen.txt"
memory_file = "data/prompts/memory/karen.txt"
llm = "openrouter"

[[npcs]]
account = "npc_merchant"
password = "..."
character_name = "상인 리코"
template_prompt = "data/prompts/templates/merchant.txt"
instance_prompt = "data/prompts/instances/rico.txt"
memory_file = "data/prompts/memory/rico.txt"
llm = "openrouter"

# 몬스터 스폰은 서버가 결정 — 클라이언트 config에서 제거
# 서버 config에서 스폰 규칙 정의 (어떤 몬스터, 몇 마리, 어디에)

[llm_scheduler]
max_concurrent = 2
min_interval_secs = 5
```

## 구현 Phase

### Phase 1: Monster AI Module

**서버 측:**
- 스폰 규칙 시스템: 어떤 몬스터를 몇 마리, 어디에 스폰할지 서버가 결정
- `ServerMessage::MonsterAssigned { monster }` 추가 — 소유자에게 직접 전송
- 서버가 연결된 클라이언트에 몬스터 소유권 배분
- `RequestSpawnMonster` 제거 또는 서버 승인 방식으로 변경

**클라이언트 측 (agent-client + web client 동일):**
- `agent-client/src/monster_ai.rs` 생성
- `monsterManager.ts`의 `updateMonsterAI` FSM을 Rust로 포팅
- `SharedState`에 `MonsterBrain` 저장소 추가
- 60Hz tick task에서 `MonsterMove`/`MonsterAttack` 메시지 전송
- `MonsterAssigned` 메시지 수신 시 해당 몬스터 AI 시작
- `data/monsters.json`에서 몬스터 정의 로드

### Phase 2: Orchestrator Refactor

- `agent-client/src/orchestrator.rs` 생성
- "connect → auth → enter game" 시퀀스를 `NpcConnection`으로 추출
- `[[npcs]]` 배열 config 지원
- HeightSampler, PassabilityCache 공유
- 단일 NPC config도 하위호환 유지

### Phase 3: LLM Scheduler

- `agent-client/src/llm_scheduler.rs` 생성
- NPC별 3계층 프롬프트 (template + instance + memory)
- Priority queue + concurrency limiter
- 기존 `classify_event()` 활용한 우선순위
- idle polling 시차 분산

### Phase 4: MCP Orchestrator Endpoint

- MCP 서버 확장: `list_npcs`, `say_as`, `move_npc`, `get_npc_events`
- 외부에서 모든 NPC 제어 가능

## 핵심 파일

- `agent-client/src/driver.rs` — LLM driver loop, combat tick (스케줄러가 대체)
- `agent-client/src/state.rs` — SharedState 확장 (MonsterBrain 추가)
- `client/src/lib/managers/monsterManager.ts` — 몬스터 AI 참조 구현
- `agent-client/src/main.rs` — 세션 라이프사이클 (orchestrator 확장)
- `shared/src/lib.rs` — 프로토콜 타입 정의
- `server/src/game_state/monster.rs` — 서버 몬스터 스폰/소유 로직

## Option 비교 (참고)

### Option 1: NPC 1명당 1 LLM (현재 방식 스케일링)
- N개 agent-client 프로세스 각각 독립 실행
- **장점**: 구현 변경 없음, 완벽한 컨텍스트 격리, 장애 격리
- **단점**: 비용 N배, 중복 world state

### Option 2: 1 LLM이 여러 NPC 동시 조종
- 하나의 프롬프트에 모든 NPC 상태를 넣고 한번에 응답
- **장점**: 비용 효율, NPC 간 협동 가능
- **단점**: 성격 오염, 레이턴시 (전원 블로킹), 컨텍스트 폭발, 단일 장애점

### Option 3: Hybrid Orchestrator (채택)
- 위 아키텍처 참조

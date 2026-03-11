# Agent Client 기획

AI agent가 인간 플레이어와 함께 게임 월드에서 플레이할 수 있도록 하는 agent 전용 클라이언트 시스템.

## 개요

인간 클라이언트는 3D 그래픽과 마우스/키보드 입력을 사용하지만, agent 클라이언트는 텍스트 기반 인터페이스로 게임에 참여한다. Agent가 캐릭터를 조종하며 인간 플레이어와 섞여서 플레이한다.

## 아키텍처

```
LLM Agent        <-->   Agent Client (상주 프로세스)   <-->   Game Server
(의도/전략)              (실행/네비게이션)                      (월드 시뮬레이션)
"대장간으로 가"           A* pathfinding                       좌표 기반 이동
"몬스터 공격"             타겟팅 + 스킬 로직                    전투 처리
"플레이어에게 인사"        채팅 메시지 전송                      메시지 브로드캐스트
```

### 레이어 구분

- **LLM**: 고수준 의사결정 (전략, 대화, 판단)
- **Agent Client**: 저수준 실행 (pathfinding, 상태 관리, 이벤트 수집)
- **Game Server**: 월드 시뮬레이션, 권한 검증

## 프로젝트 구조

모노레포 내 별도 패키지로 구성한다. 서버와 공유하는 타입/프로토콜 정의를 직접 import할 수 있고, 서버 프로토콜 변경 시 동기화가 용이하다.

```
OnlineRPG/
├── client/               # 기존 인간 클라이언트 (Svelte + Threlte)
├── agent-client/         # agent 전용 클라이언트 (신규)
│   ├── src/
│   │   ├── connection/        # WebSocket 통신
│   │   ├── navigation/        # A* pathfinding
│   │   ├── state/             # 월드 상태 관리
│   │   └── mcp/               # MCP 서버 인터페이스
│   └── package.json
├── server/               # 게임 서버
├── shared/               # 서버-클라이언트 공유 타입
├── tools/                # 개발 도구
├── data/                 # 게임 데이터
└── doc/                  # 문서
```

## 통신 프로토콜

### WebSocket + JSON

Agent 클라이언트는 WebSocket으로 서버와 통신하되, binary가 아닌 JSON 텍스트를 사용한다.

- LLM이 직접 읽고 생성할 수 있는 형태
- 디버깅 용이
- Agent 수가 수천 단위가 아닌 이상 성능 문제 없음

기존 인간 클라이언트가 binary 프로토콜을 사용한다면, 서버에 JSON endpoint를 별도로 추가한다.

### 대안: Binary 프로토콜 + 클라이언트 측 텍스트 변환

서버와의 통신은 기존 인간 클라이언트와 동일한 binary 프로토콜을 그대로 사용하고, Agent 클라이언트 내부에서 binary ↔ 텍스트 변환 레이어를 두어 LLM과는 텍스트로 통신하는 방식도 가능하다.

- **서버 수정 불필요**: 기존 binary 프로토콜을 그대로 사용하므로 서버에 JSON endpoint를 별도로 추가할 필요가 없음
- **프로토콜 일관성**: 인간 클라이언트와 Agent 클라이언트가 서버 입장에서 동일하게 취급됨. 서버가 클라이언트 종류를 구분할 필요 없음
- **기존 인프라 재사용**: 로드밸런싱, 인증, rate limiting 등 기존 인프라를 그대로 활용 가능

이 경우 Agent 클라이언트의 구조는 다음과 같다:

```
서버 ←──binary──→ [Agent Client: 변환 레이어] ←──text──→ LLM
```

변환 레이어가 binary 메시지를 LLM이 이해할 수 있는 텍스트(자연어 또는 JSON)로 직렬화하고, LLM의 텍스트 응답을 다시 binary 명령으로 변환하는 역할을 한다. 단, 변환 레이어의 구현 및 유지보수 비용이 추가되며, 프로토콜 변경 시 변환 로직도 함께 업데이트해야 하는 점은 고려해야 한다.

### 대안 2: 기존 인간 클라이언트에 LLM 연결 기능 내장

별도의 Agent 전용 클라이언트를 만들지 않고, 기존 인간용 클라이언트 자체에 LLM 연결 기능을 추가하는 방식이다. 클라이언트가 게임 상태를 텍스트로 요약하여 LLM에 전달하고, LLM의 응답을 클라이언트 내부에서 조작 명령으로 변환하여 실행한다.

```
서버 ←──기존 프로토콜──→ [인간 클라이언트 + LLM 연결 모듈] ←──text──→ LLM
                              ↑ 기존 UI/렌더링 그대로 동작
```

- **기존 클라이언트 코드베이스 재사용**: 게임 상태 파싱, 렌더링, 입력 처리 등 이미 구현된 로직을 그대로 활용. 별도 클라이언트를 밑바닥부터 만들 필요 없음
- **LLM 행동 실시간 관찰 가능**: 기존 UI가 그대로 동작하므로, LLM이 무엇을 보고 어떤 행동을 하는지 화면으로 직접 엿볼 수 있음. 디버깅과 행동 튜닝에 유리
- **인간 ↔ LLM 전환 용이**: 같은 클라이언트에서 인간이 직접 조작하다가 LLM에게 제어를 넘기거나, LLM이 플레이하는 것을 인간이 중간에 개입하는 하이브리드 운용이 가능

단, 기존 클라이언트가 UI/렌더링 등 무거운 의존성을 갖고 있다면 headless 환경에서의 대량 배포에는 적합하지 않을 수 있다. 관찰·디버깅 목적이나 소수의 Agent 운용에 적합한 방식이다.

### 대안 3: MCP 서버를 통한 LLM 브릿지

별도의 "LLM용 중간 서버"를 두는 방식이다. 이 중간 서버가 게임 서버와는 기존 binary 프로토콜로 통신하면서, LLM 측에는 MCP(Model Context Protocol) 서버로서 동작한다. 하나의 중간 서버가 여러 LLM Agent를 동시에 서빙할 수 있다.

```
게임 서버 ←──binary──→ [MCP 브릿지 서버] ←──MCP──→ LLM Agent 1
                            ↕                      LLM Agent 2
                       여러 Agent 동시 관리          LLM Agent N
```

- **LLM 사용자 측 프로세스 불필요**: Agent를 운용하려는 LLM 사용자가 별도 클라이언트 프로세스를 띄울 필요 없이, MCP 프로토콜로 중간 서버에 접속하면 바로 게임에 참여 가능
- **중앙 집중 관리**: 하나의 브릿지 서버가 다수의 LLM Agent 세션을 관리하므로, 모니터링·로깅·rate limiting 등을 한 곳에서 처리 가능
- **표준 프로토콜 활용**: MCP를 사용하므로 다양한 LLM 클라이언트(Claude, GPT 등)가 별도 어댑터 없이 연결 가능

단, 중간 서버가 단일 장애 지점(SPOF)이 될 수 있으며, 게임 서버와 LLM 사이에 홉이 하나 추가되므로 지연이 늘어날 수 있다. 또한 중간 서버 자체의 개발·운영 비용이 발생한다.

### 서버 → Agent Client 메시지 예시

```json
{
  "type": "world_update",
  "description": "당신은 마을 광장에 서 있습니다. 북쪽에 대장간, 동쪽에 여관이 보입니다.",
  "nearby_entities": [
    { "id": "player_42", "name": "용사김", "type": "player", "distance": 5.2, "direction": "북동" },
    { "id": "npc_blacksmith", "name": "대장장이 볼칸", "type": "npc", "distance": 12.0, "direction": "북" }
  ],
  "available_actions": ["move", "talk", "inspect", "use_skill"],
  "position": { "x": 128.5, "y": 0, "z": 64.3 }
}
```

### Agent Client → 서버 메시지 예시

```json
{ "type": "move", "x": 140.2, "y": 0, "z": 58.1 }
{ "type": "chat", "target": "player_42", "message": "안녕하세요!" }
{ "type": "use_skill", "skill": "attack", "target": "goblin_7" }
```

## 텍스트 월드 디스크립션

서버에 "텍스트 MUD 레이어"를 추가한다. 서버가 월드 상태를 알고 있으므로, 서버가 직접 텍스트 디스크립션을 생성하는 것이 일관성 있다.

### 디스크립션 내용

- 주변 환경 묘사 (지형, 건물, 날씨)
- 시야 내 엔티티 (플레이어, NPC, 몬스터)
- 가능한 행동 목록
- 최근 이벤트 로그 (누가 나타남, 누가 말함, 전투 결과 등)

### 예시

```
[환경] 당신은 해변에 서 있습니다. 파도 소리가 들립니다. 서쪽으로 마을이 보입니다.
[발견] 플레이어 '용사김'이 시야에 나타났습니다. (북동쪽, 약 15m)
[전투] 고블린이 당신을 공격했습니다. HP: 85/100
[채팅] 용사김: "파티 하실래요?"
```

## 네비게이션 시스템

LLM이 매 프레임 좌표를 결정하는 것은 비현실적이고 비용이 크다. Agent는 텍스트로 고수준 명령을 내리고, agent client가 pathfinding으로 실행한다.

### 흐름

1. LLM: `{ "action": "move_to", "destination": "대장간" }` 명령
2. Agent Client: "대장간"을 알려진 POI(Point of Interest) 좌표로 변환
3. Agent Client: A* 알고리즘으로 현재 위치 → 대장간 경로 계산
4. Agent Client: 경로를 따라 이동 명령을 서버에 순차 전송
5. Agent Client: 도착 시 LLM에 결과 보고

### Agent Client 담당

- A* pathfinding (서버에서 맵 데이터 수신)
- POI(관심 지점) 이름 → 좌표 변환
- 텍스트 명령 → 게임 액션 변환
- 상태 머신 관리 (이동 중, 전투 중, 대기, 대화 중 등)

## MCP 인터페이스

Agent client 위에 MCP(Model Context Protocol) 서버를 얹어 LLM 연동을 표준화한다.

### 왜 MCP인가

- MCP 단독으로는 부족: request-response 패턴이라 실시간 게임 이벤트 수신이 어려움
- Agent client가 실시간 WebSocket 연결 + pathfinding + 이벤트 버퍼링을 담당
- MCP는 LLM이 agent client를 제어하는 인터페이스 역할

### MCP Tool 예시

```
look_around()          → 주변 환경 텍스트 디스크립션 반환
move_to("대장간")       → pathfinding 실행, 도착 시 결과 반환
attack("goblin_7")     → 전투 실행, 결과 반환
talk("player_42", "안녕") → 채팅 메시지 전송
get_inventory()        → 인벤토리 목록 반환
get_status()           → HP, MP, 위치, 상태 등 반환
get_quest_log()        → 퀘스트 목록 반환
```

### MCP Resource 예시

```
game://world/map       → 월드 맵 정보
game://player/status   → 플레이어 상태
game://player/inventory → 인벤토리
game://nearby/entities → 주변 엔티티 목록
```

## 구현 우선순위

1. **Agent Client 기본 구조** - 기존 binary 프로토콜로 서버에 접속하여 한 명의 PC를 조종하는 클라이언트. 서버 수정 없이 시작
2. **클라이언트 측 텍스트 변환 레이어** - 수신한 binary 게임 상태를 LLM이 읽을 수 있는 텍스트로 변환, LLM 응답을 binary 명령으로 변환
3. **네비게이션 시스템** - A* pathfinding, POI 매핑
4. **MCP 서버 인터페이스** - LLM 연동용 tool/resource 정의
5. **LLM 통합 테스트** - 실제 LLM으로 게임 플레이 테스트

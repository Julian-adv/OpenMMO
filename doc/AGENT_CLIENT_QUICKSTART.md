# agent-client 실행 가이드

LLM이 당신의 캐릭터를 조종해 게임에 참여한다. 구글 계정으로 로그인하고, LLM은
당신 머신에서 당신 계정으로 돈다 — 게임 서버는 LLM 비용을 부담하지 않는다.

에이전트는 **평범한 플레이어**다. 서버는 사람이 조종하든 LLM이 조종하든 똑같이
대한다. 특별한 권한도, 특별한 제약도 없다.

## 0. 받기

배포되는 tarball은 **Ubuntu 24.04 이상**(glibc 2.39+)에서 돕니다. 그보다 오래된
배포판이거나 다른 아키텍처면 소스에서 빌드하십시오 — Rust 툴체인 말고는 준비물이
없고, 게임 데이터는 빌드가 알아서 생성합니다.

```bash
git clone https://github.com/Julian-adv/OpenMMO.git
cd OpenMMO && cargo build --release -p agent-client
```

빌드한 바이너리는 `target/release/agent-client`입니다. `agent-client/` 디렉터리에서
실행하고, `data/config.toml`은 `data/config.toml.example`을 참고해 만듭니다.

이때 **구글 로그인용 `client_secret`이 저장소에는 없습니다.** tarball을 하나 받아
그 안의 `data/config.toml`에서 값을 복사해 오십시오.

```bash
tar -xzf agent-client-*.tar.gz
grep client_secret agent-client-*/data/config.toml
```

기밀이 아닙니다 — 설치형 앱은 비밀을 지킬 수 없고(RFC 8252 §8.5) 모든 배포본이
같은 값을 씁니다. 저장소에 두지 않는 이유는 보안이 아니라 시크릿 스캐너 마찰입니다.

## 1. LLM 준비

기본 설정은 [Codex CLI](https://github.com/openai/codex)를 쓴다. 터미널에서
`codex` 명령이 정상 동작하는 상태여야 한다 (로그인·요금·레이트리밋 모두 당신
계정 기준). 다른 백엔드를 쓰려면 `data/config.toml`의 `llm`을 바꾼다:

| 값 | 필요한 것 |
|---|---|
| `codex` | `codex` CLI (기본값) |
| `claude` | `claude` CLI |
| `openrouter` | `OPENROUTER_API_KEY` 환경변수 |

## 2. 캐릭터 설정

`data/config.toml`에서 이름과 클래스를 정한다.

```toml
[[npcs]]
character_name = "당신의 캐릭터 이름"   # 서버 전체에서 유일해야 한다
character_class = "ranger"
llm = "codex"
```

고를 수 있는 클래스: `knight` `barbarian` `caveman` `valkyrie` `ranger`
`rogue` `priest`. (`merchant`, `guard`는 운영자 NPC 전용이라 거절된다.)

## 3. 실행

```bash
./agent-client
```

처음 실행하면 구글 로그인 안내가 뜬다:

```
  Sign in to continue:
    1. open https://www.google.com/device
    2. enter code XXX-XXX-XXX
```

브라우저(어느 기기든)에서 코드를 입력하면 접속이 이어진다. 자격증명은
`~/.config/onlinerpg/google.json`에 저장되므로 다음부터는 묻지 않는다.

로그아웃하려면 그 파일을 지운다.

## 문제가 생기면

| 증상 | 원인과 대처 |
|---|---|
| `Connection failed: HTTP error: 200 OK` | `server` 주소에 `/ws` 경로가 빠졌다. 게임 페이지(HTML)를 받아온 것이다 — `wss://<호스트>/ws` 로 고친다 |
| `Protocol vN required, you sent vM` | 서버가 업데이트됐다. 새 배포물을 받는다 |
| `Auth failed: ...` 후 종료 | 로그인 거절. 위 자격증명 파일을 지우고 다시 로그인 |
| 캐릭터 생성 실패 (이름 중복) | `character_name`은 서버 전체에서 유일해야 한다 |
| `The merchant class is not available` | 플레이어가 고를 수 없는 클래스다 |
| 지형 위를 걷지 못하거나 높이가 이상함 | 서버의 터레인 API에 접근이 안 되는 상태. `terrain` 값을 확인한다 |

로그를 자세히 보려면 `RUST_LOG=debug ./agent-client`.

## 동작 방식

- 게임 서버와는 웹 클라이언트와 **완전히 같은 WebSocket 프로토콜**로 통신한다
- 지형 높이는 필요한 타일만 서버에서 받아 `data/cache/height/`에 캐시한다
- LLM에는 주변 상황을 텍스트로 요약해 넘기고, 응답(이동·대화·전투)을 게임
  명령으로 바꿔 보낸다. 경로탐색·충돌·전투 판정은 전부 클라이언트/서버가 한다

설계 배경은 [REMOTE_AGENT_CLIENT.md](REMOTE_AGENT_CLIENT.md) 참고.

# IP 공지

특정 IP에서 접속한 캐릭터에게만 공지를 보낸다. 게임에 들어올 때 보내고, 접속해 있는 동안 1시간마다 다시 보낸다. 클라이언트는 로딩 화면이 끝난 뒤 확인 창(`OperatorNotice`)으로 띄우고, 확인을 눌러야 닫힌다. 해당 클라이언트의 운영자에게 고쳐야 할 점을 알릴 때 쓴다.

## 파일

`<state_dir>/ip_notices.txt`(기본 `data/ip_notices.txt`). IP가 들어 있어서 gitignore 대상이다. prod에만 두며, 배포가 덮어쓰지 않는다.

```
# 주석
220.89.217.201 BGM 파일을 캐시 없이 반복해서 받고 있습니다. 클라이언트를 고쳐 주세요.
```

- 한 줄에 `IP 메시지` 하나. IP 뒤 첫 공백 이후가 메시지다.
- 같은 IP 줄이 여러 개면 모두 보낸다.
- 입장할 때와 반복할 때마다 파일을 다시 읽는다. 그래서 수정하거나 지우면 재시작 없이 바로 반영된다. 파일이 없으면 아무것도 보내지 않는다.
- 보낼 때마다 서버 로그에 `Sent IP notices`가 남는다.

코드: `server/src/ip_notices.rs`, 입장 처리는 `server/src/connection/entry.rs`, 반복은 `server/src/connection.rs`.

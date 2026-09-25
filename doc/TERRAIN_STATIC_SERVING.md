# 지형 원본 파일의 nginx 직접 제공

프로토콜 83부터 `TerrainTileVersion`은 타일 좌표와 `files`를 보낸다. `files`의 `height`, `splat`, `trees`, `grass`, `landscape`는 각각 `{ path, hash }` 또는 `null`이다. 해시는 **디스크 원본 바이트의 SHA-256**이다. MessagePack에는 이 작은 목록만 넣으며 지형 본문은 직렬화하지 않는다.

클라이언트는 `/api/terrain/files/{path}?hash={hash}`로 원본을 받는다. nginx는 같은 terrain 디렉터리를 읽으며 쿼리의 해시로 파일을 선택하거나 검증하지 않는다. 클라이언트가 실제 응답의 해시를 확인한 뒤 사용한다. 다운로드 중 원본이 교체되면 이전 해시 요청에도 새 바이트가 올 수 있으므로, 불일치한 본문은 저장·적용하지 않고 최신 목록을 요청한다.

## 파일 목록과 준비

```bash
cargo run --release -p onlinerpg-terrain --bin terrain-manifests -- data/terrain
```

`TERRAIN_DIR/manifests/{x}/{z}.json`에 파일 목록·해시·원본 크기·수정 시각을 저장한다. 지형 본문을 복제하거나 합친 파일은 만들지 않는다. 기존 `snapshots/`는 새 코드에서 사용하지 않으며 자동 삭제하지 않는다. 구버전으로 롤백할 필요가 없을 때 별도로 정리할 수 있다.

준비 명령은 원본과 기존 메타데이터가 있는 타일을 확인하고 변경된 목록만 다시 계산한다. 런타임은 타일의 첫 조회에서 메타데이터를 확인하고 이후에는 메모리 목록을 재사용한다. 건축·조경·편집·삭제는 원본 저장 후 해당 타일의 해시를 다시 계산하고 WebSocket 변경을 게시한다. 계산 또는 메타데이터 저장에 실패하면 새 버전을 게시하지 않고 재시도한다.

서버 시작 시 전체 월드를 순회하지 않는다. WebSocket 구독 전에 필요한 초기 화면·편집기 타일은 `GET /api/terrain/manifest/{x}/{z}`로 같은 `files` 목록을 받는다. HTTP 목록은 `Cache-Control: no-store`다. 웹의 동시 초기 조회는 공유하며, 구독 중인 타일은 WebSocket으로 받은 목록을 사용한다.

프로세스 밖에서 원본을 바꾸면 서버를 재시작해 메모리 목록을 갱신한다. 원본의 크기·수정 시각까지 보존한 교체는 해당 메타데이터도 제거해 재계산한다. 외부 베이크와 목록 준비를 동시에 실행하지 않는다.

## 클라이언트 적용과 캐시

- 높이·스플랫 파일이 없거나 기존 서버의 크기 검증을 통과하지 못하면 목록에 `null`을 넣는다. 클라이언트는 해수면 높이 10000과 0으로 채운 스플랫맵을 생성한다. 나무·풀이 `null`이면 배치가 없는 타일이다.
- 조경 `LND1` 파일이 있으면 그 안의 스플랫맵을 사용하고 원본 스플랫은 다운로드하지 않는다. 나무·풀에는 조경 제거 마스크를 적용한다. 기존 GR03 풀 파일도 원본 그대로 전송하고 웹에서 셀별 개수로 읽는다.
- 웹은 같은 타일 목록에 필요한 파일을 병렬로 받고, 전체가 검증되면 현재 구독인지 확인한 뒤 적용한다. 늦게 도착한 이전 구독 응답은 적용하지 않는다.
- 메모리 캐시는 원본 출처와 내용 해시 기준으로 8MiB를 보관한다. 같은 내용의 동시 요청도 공유한다. 렌더러 초기 로딩과 WebSocket 갱신이 같은 로더를 사용한다.
- 프로덕션 서비스 워커는 `openmmo-terrain-files-v1` Cache Storage에 검증된 원본을 최대 128MB 보관한다. 모델·BGM의 500MB 캐시와 저장·조회·LRU 코드를 공유하며 용량과 배포 목록 정리는 분리한다. 모델 배포 목록을 정리해도 지형은 삭제하지 않는다. 워커가 없거나 저장소를 쓸 수 없으면 다운로드와 메모리 캐시는 계속 동작한다.
- Cache Storage에서 찾은 파일은 네트워크 요청 없이 사용한다. 캐시에 없을 때만 HTTP 캐시를 우회하여 다운로드한다. 원본 경로는 변경 가능하므로 HTTP 응답은 `no-store`이고 nginx ETag에 의존하지 않는다. Cache Storage는 HTTP 캐시 정책과 별개로 검증된 응답을 명시적으로 저장한다.
- 원격 에이전트는 높이와 현재 지면 재질에 필요한 파일만 받는다. 조경 파일이 있으면 스플랫을 추출한다. 나무·풀 파일은 받지 않는다. `terrain_cache/files/{hash}.bin`에 검증한 원본을 원자적으로 저장하며 로컬 terrain 경로도 같은 원본 해시를 검증한다.

## Docker Compose

서버 이미지에는 `terrain-manifests`를 포함한다. `terrain-init`은 베이크 후 해시 목록을 준비한다. client는 terrain 볼륨을 `/terrain:ro`로 마운트하며 `docker/nginx.conf.template`이 원본 파일을 직접 제공한다.

## systemd 운영 서버

`tools/deploy-prod.sh`는 `terrain-manifests`를 빌드하고 `${TERRAIN_DIR:-$REPO/data/terrain}`의 목록을 준비한다. 실패하면 웹 게시와 서비스 재시작을 진행하지 않는다. 서버의 실제 `--terrain-dir`와 같은 경로를 지정한다.

운영 nginx는 수동 관리다. `/etc/nginx/sites-available/openmmo`의 `server`에 아래 경로를 반영한다. 이전 snapshot location은 제거한다. 경로는 서버가 사용하는 terrain 디렉터리의 절대 경로다.

```nginx
location ~ "^/api/terrain/files/((?:height|splat|trees|grass|landscaping)/r[+-][0-9]+_[+-][0-9]+/[hstgl]_[+-][0-9]+_[+-][0-9]+\.bin)$" {
    alias /ABSOLUTE/TERRAIN_DIR/$1;
    default_type application/octet-stream;
    sendfile on;
    etag off;
    if_modified_since off;
    open_file_cache off;
    add_header Cache-Control "no-store" always;
}

location /api/terrain/files/ {
    add_header Cache-Control "no-store" always;
    return 404;
}
```

nginx worker가 디렉터리를 탐색하고 파일을 읽을 수 있어야 한다. 공개 경로는 현재 지형의 다섯 파일 종류만 허용한다. 복원용 원본이나 해시 메타데이터를 이 location으로 노출하지 않는다.

배포 시 서버·웹 WASM·에이전트를 **프로토콜 83으로 함께 갱신**한다. 배포 전 nginx 설정 검증과 운영 데이터 이관 지침을 따른다. nginx가 없는 개발 환경에서는 Rust HTTP가 같은 원본 바이트를 제공한다.

## 검증

```bash
bash tools/check-nginx-conf.sh
bash tools/test-terrain-static.sh
bash tools/test-deploy-prod.sh
```

정적 서빙 테스트는 임시 원본과 실제 nginx Unix 소켓을 사용한다. 게임 서버 없이 원본과 응답이 일치하는지, 파일 교체가 즉시 반영되는지, ETag 없이 본문을 주는지, 허용하지 않은 경로와 없는 파일에 캐시되지 않는 404를 주는지 검증한다. 운영 서비스와 운영 nginx는 변경하지 않는다.

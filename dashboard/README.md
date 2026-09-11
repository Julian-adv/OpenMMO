# OpenMMO Pulse

게임 운영 지표를 보여주는 독립 Svelte 앱입니다. 1단계에서는 동시 접속 계정 수, 기간 최고·평균, 최근 1·6·24시간 그래프를 제공합니다.

## 개발

저장소 루트에서 최신 게임 서버를 실행한 뒤 별도 터미널에서 시작합니다. 새 서버 코드가 실행되어야 지표 API와 수집이 활성화됩니다.

```bash
cd dashboard
npm ci
npm run dev
```

<http://localhost:10008>에서 확인합니다. 기본 API 대상은 `http://127.0.0.1:10007`이며 Vite가 `/api/metrics`를 프록시합니다. 다른 로컬 서버를 대상으로 실행하려면:

```bash
DASHBOARD_API_TARGET=http://127.0.0.1:10107 npm run dev
```

`.env.example`을 `.env.local`로 복사해서 설정해도 됩니다. 게임 클라이언트, WASM, 3D 에셋 빌드는 필요 없습니다. 화면에는 실제 API 응답만 표시하며, 수집 전의 과거 기록은 만들지 않습니다.

## 확인 및 빌드

```bash
npm run check
npm run lint
npm test
npm run build
npm run preview
```

`dist/`는 별도 도메인이나 게임 사이트의 하위 경로에 제공할 수 있는 정적 파일입니다. 하위 경로에 게시할 때는 `DASHBOARD_BASE=/dashboard/ npm run build`로 빌드합니다. API 경로는 두 경우 모두 같은 출처의 `/api/metrics/concurrent`입니다.

게임 서버의 기존 REST 포트로 `/api/metrics/`만 프록시하고, 나머지는 대시보드 정적 파일을 제공합니다. 별도 사이트의 nginx 설정 예:

```nginx
root /var/www/openmmo-dashboard;
index index.html;

location / {
    try_files $uri $uri/ /index.html;
    add_header Cache-Control "no-cache";
}

location /api/metrics/ {
    proxy_pass http://127.0.0.1:10007;
    proxy_cache off;
}
```

대시보드 정적 파일 교체에는 게임 서버 재시작이 필요하지 않습니다. 기존 `tools/deploy-prod.sh`에는 대시보드 게시가 포함되지 않습니다. 위 설정은 구성 예시이며 실제 운영 배포는 별도로 진행합니다.

지표의 정확한 범위와 API는 [METRICS.md](../doc/METRICS.md)를 참고하세요.

# OpenMMO Pulse

허용된 운영자만 게임 운영 지표를 볼 수 있는 독립 Svelte 앱입니다. 동시 접속 계정 수, 기간 최고·평균, 최근 1일, 1주일, 1개월, 6개월, 1년 그래프를 제공합니다. 동시 접속 추이의 기본 조회 기간은 1일(최근 24시간)입니다.

## 접근 권한

Google 로그인 후 서버의 `ADMIN_EMAILS` / `--admin-emails` 목록에 등록된, 이메일 인증을 마친 계정만 입장합니다. 허용 계정은 서버에서 관리하며 프런트엔드에 이메일 목록을 넣지 않습니다. 기존 게임 어드민 목록을 공유하므로 계정을 추가하면 게임 관리 권한도 함께 부여됩니다.

모든 `/api/metrics/` 조회에는 `Authorization: Bearer <Google ID token>`이 필요합니다. `/api/metrics/session`은 권한 확인에 성공하면 204를 반환합니다. 누락·위조·만료된 토큰은 401, 일반 계정·미인증 이메일은 403이며 NPC 토큰과 게임 세션 토큰은 사용할 수 없습니다. Google 인증이나 허용 목록이 설정되지 않으면 접근을 허용하지 않습니다. 성공·실패 응답 모두 `Cache-Control: no-store`입니다.

로그인 전에는 지표를 요청하지 않습니다. 토큰은 브라우저 메모리에만 유지하며 페이지를 새로고침하면 다시 로그인합니다. 로그아웃·토큰 만료·API의 401/403 응답 시 대시보드를 닫고 표시 중인 통계와 자동 갱신을 정리합니다.

## 표시 지표

합계 접속 수를 유지하면서 웹 접속과 외부 에이전트의 구성을 색상으로 구분합니다. 현재 접속 카드와 그래프 툴팁에서 유형별 계정 수·비율을 확인할 수 있으며, 구분 정보가 없는 과거 기록은 기타·미분류로 표시합니다.

아래의 유니크 접속 계정 그래프는 매일 한국 시간 자정 기준으로 집계해 DB에 저장한 값을 표시합니다. 1일·1주일·1개월·6개월·1년의 계정을 각각 중복 제거하며, 화면 조회 시 접속 이력을 다시 계산하지 않습니다. 각 그래프는 기간을 따로 선택하고 마우스를 올려 상세 내용을 확인합니다. 유니크 계정은 마지막 집계 기준 시각과 이력 부족 안내를 표시하며, 첫 집계는 수집 시작 후 다음 자정에 생성됩니다. 화면은 1시간마다 저장된 결과를 다시 읽습니다.

서버 총 골드 그래프는 기존 `gold_snapshots`의 시간별 합계를 사용합니다. 오프라인 캐릭터와 NPC를 포함한 모든 캐릭터의 보유 골드를 집계하며, 최근 총량과 마지막 집계 기준 시각을 표시합니다. 1시간·24시간·1주일·1개월은 시간별 기록, 6개월은 6시간 평균, 1년은 일평균입니다. 새로고침은 저장된 집계를 조회하며 추가 기록을 생성하지 않습니다.

그 아래의 활성 유저 1인당 골드 그래프는 총 골드와 조회 기간을 공유합니다. 별도의 활성 유저 선택자로 직전 1일·1주일·1개월·6개월·1년 유니크 계정을 선택합니다(기본 1일). 각 시간별 총 골드를 해당 날짜의 한국 시간 자정 집계로 나눈 뒤 구간 평균을 계산합니다. 분모는 당시의 계정 수이며, 일별 집계가 없거나 0계정인 시간은 제외합니다. 최근 수치에는 총 골드·계정 수와 두 집계 기준 시각을 함께 표시합니다.

서버를 다시 시작하면 접속을 받기 전에 수집 시작 이후 누락된 자정별 집계를 모두 채웁니다. 이미 저장된 날짜는 다시 계산하지 않으며, 수집을 시작하기 전의 기록은 만들지 않습니다.

활성 유저 1인당 골드 아래에는 골드 생산 순위 표가 있습니다. 상인 판매는 아이템별로 표시하고, 던전 보상 상자·동전 더미 획득(몬스터·일반 상자·파괴물 등)·동전 주머니 개봉·주민 NPC 급여도 생산원별로 합산합니다. 실제 지급액 내림차순으로 수량 또는 횟수·생성 골드·전체 생산액 대비 비중을 표시합니다. 흥정과 지갑 상한 적용 후 실제 지급액을 기록하고, 동전 더미는 주웠을 때 집계합니다. 주민 NPC·플레이어 간 거래와 팁은 제외하며 소비한 골드는 차감하지 않습니다. 기본 1일이며 1일·1주일·1개월·6개월·1년을 독립적으로 선택합니다. 메모리에서 시간·생산원별로 합산해 다음 정각에 저장하며 완료된 시간대만 표시합니다. 로그아웃 후에도 집계를 유지하고 정상 종료 시 진행 중인 시간대까지 저장하지만, 비정상 종료 시 미저장 통계는 유실될 수 있습니다. 기존 판매 기록은 보존하며 판매 외 골드는 적용 이후부터 수집하고 별도 시작 시각을 안내합니다.

레벨 상위 10명 표는 오프라인을 포함한 전체 캐릭터 중 공식 NPC 계정을 제외한 순위입니다. 레벨 내림차순, 동점이면 경험치 내림차순과 캐릭터 ID 오름차순으로 정렬합니다. 계정당 여러 캐릭터가 각각 순위에 들어갈 수 있습니다. 같은 계정의 두 번째 캐릭터부터 `Agent 1 (Jake 1)`처럼 표에서 가장 먼저 나온 캐릭터명을 괄호 안에 표시합니다. 순위는 저장된 현재값을 매시간 조회하며, 이력은 별도의 시간별 작업에서 기록합니다. 게임 진행 상태는 기존 32초 저장을 유지합니다.

순위 표 오른쪽에는 현재 상위 10명의 레벨 변화 그래프를 표시합니다. 좁은 화면에서는 표 아래로 배치하며, 1주일·1개월·6개월·1년을 선택할 수 있습니다. 표와 그래프는 캐릭터별 색상을 공유하고, 캐릭터명이나 범례를 선택하면 해당 선을 강조합니다. 그래프에 마우스를 올려 시점별 레벨을 확인합니다. 주·월은 시간별, 6개월은 6시간별, 1년은 일별 마지막 변경값을 계단선으로 연결합니다.

새 서버를 처음 실행하면 기존 캐릭터의 현재 레벨부터 기록을 시작하며, 이후 캐릭터 생성 시 기준값을 기록하고, 매시간 관측한 레벨이 직전 기록과 달라졌을 때 이력을 추가합니다. 상위 10명에 새로 진입해도 그 전에 수집한 이력을 표시합니다. 기록 시작 이전의 레벨은 만들지 않으며, 이름 변경에도 이력은 유지됩니다. 조회만으로 기록을 추가하지 않습니다.

레벨 순위 아래에는 같은 형식의 골드 상위 10명 표와 골드 변화 그래프를 표시합니다. 공식 NPC를 제외한 전체 캐릭터를 보유 골드 내림차순, 동점이면 캐릭터 ID 오름차순으로 정렬합니다. 같은 계정의 캐릭터 표시, 색상과 선택 강조, 1주일·1개월·6개월·1년 조회 및 1시간 갱신 방식은 레벨 순위와 같습니다. 금액은 `8g61s34c`처럼 표시하며 레벨 그래프와 조회 기간을 독립적으로 선택합니다.

개인별 골드 기록은 새 서버 적용 시점의 현재 보유량부터 시작합니다. 이후 캐릭터 생성과 시간별 골드 관측값의 변화를 기록하므로 현재 상위 10명이 되기 전의 이력도 조회할 수 있습니다. 기존 서버 총 골드 기록으로 개인별 과거 보유량을 추정하지 않습니다.

골드 순위 아래에는 무기 인챈트 상위 10명 표와 변화 그래프가 있습니다. 가방·장비 슬롯에 보유한 무기 중 최고 인챈트를 `+7`처럼 표시하며 장착 여부와 무관합니다. 방어구·창고 아이템은 제외하고 무기가 없으면 `+0`입니다. 공식 NPC 제외, 같은 계정 표시, 기간 선택·선 강조·1시간 갱신은 다른 순위와 같습니다. 최초 적용 시점의 현재값부터 기록하며 최고 무기 상실로 값이 내려가는 경우도 표시합니다.

무기 순위 아래에는 같은 형식의 방어구 인챈트 상위 10명 표와 변화 그래프가 있습니다. 가방·장비의 방어구를 아이템 정의의 슬롯별로 묶어 최고 인챈트를 하나씩 고른 뒤 합산합니다. 투구·갑옷·방패·장갑·바지·신발을 포함하며 같은 슬롯의 중복 방어구는 더하지 않습니다. 무기·액세서리·창고 아이템은 제외하고 방어구가 없으면 `+0`입니다. 예를 들어 투구 최고 `+4`, 갑옷 최고 `+3`, 방패 최고 `+2`이면 합계 `+9`입니다. 장착 여부는 순위에 영향을 주지 않으며 기록·조회·갱신 방식은 무기 순위와 같습니다.

DB와 API의 금액은 코퍼 단위이며 `1골드 = 100실버 = 10,000코퍼`입니다. 순위 표, 총 골드·1인당 골드 요약, 그래프 축과 툴팁은 `8g61s34c` 형식으로 표시하고 각 단위에 금색·은색·구리색을 적용합니다. 0인 단위는 생략하고 전체 금액이 0이면 `0c`를 표시합니다. 평균값은 가장 가까운 1코퍼로 반올림합니다.

지표 수집은 서버 시작 시와 매 정각에 실행하고, 화면은 최초 접속·기간 변경·수동 새로고침 외에는 매 정각 5초 후 갱신합니다. 유니크 계정 집계는 기존 하루 한 번이며 접속 중 마지막 확인 시각은 시간마다 저장합니다. 캐릭터 이력은 매시간 변경된 값만 추가하고 32초 게임 저장에서는 추가하지 않습니다. 기존 캐릭터 이력과 분 단위 동시 접속 기록은 보존합니다.

영지 보유 순위는 공식 NPC를 제외하고 캐릭터가 소유한 개척지·왕령 필지 수를 합산한 상위 10명입니다. 순위 옆에서 현재 상위 캐릭터들의 보유량 변화를 1주일·1개월·6개월·1년으로 조회할 수 있습니다. 최초 적용 시 현재 보유량을 기록하고, 이후 매시간 달라진 값만 저장합니다. 필지가 없는 캐릭터는 순위에서 제외하며, 과거 기록은 소급 생성하지 않습니다. 자세한 기준은 [운영 지표](../doc/METRICS.md#영지-보유-상위-10명)를 참고하세요.

## 개발

저장소 루트에서 최신 게임 서버를 실행한 뒤 별도 터미널에서 시작합니다. 새 서버 코드가 실행되어야 지표 API와 수집이 활성화됩니다.

Google 로그인 설정은 기존 `client/.env.local`의 `VITE_GOOGLE_CLIENT_ID`를 공유하므로 `dashboard/.env.local`을 따로 만들 필요가 없습니다. 개발 실행과 수동 빌드 모두 같은 설정을 사용합니다. 서버에는 `GOOGLE_CLIENT_ID`와 `ADMIN_EMAILS`를 설정하고, 해당 OAuth 클라이언트의 허용된 JavaScript 출처에 `http://localhost:10008`을 등록합니다.

대시보드에서 다른 클라이언트 ID나 API 주소를 사용할 때만 `.env.example`을 `.env.local`로 복사해 필요한 값을 지정합니다. Google 클라이언트 ID는 셸 환경변수, 대시보드 설정, 게임 클라이언트 설정 순서로 비어 있지 않은 값을 사용합니다. 각 폴더에서는 Vite의 모드별 환경 파일 우선순위를 따르며, 공통 설정에서 가져오는 값은 `VITE_GOOGLE_CLIENT_ID`뿐입니다. `client/`의 환경 파일을 수정한 뒤에는 대시보드 개발 서버를 재시작합니다.

```bash
cd dashboard
npm ci
npm run dev
```

<http://localhost:10008>에서 확인합니다. 기본 API 대상은 `http://127.0.0.1:10007`이며 Vite가 `/api/metrics`를 프록시합니다. 다른 로컬 서버를 대상으로 실행하려면:

```bash
DASHBOARD_API_TARGET=http://127.0.0.1:10107 npm run dev
```

게임 클라이언트, WASM, 3D 에셋 빌드는 필요 없습니다. 화면에는 실제 API 응답만 표시하며, 수집 전의 과거 기록은 만들지 않습니다.

## 확인 및 빌드

```bash
npm run check
npm run lint
npm test
npm run build
npm run preview
```

`dist/`는 별도 도메인이나 게임 사이트의 하위 경로에 제공할 수 있는 정적 파일입니다. 하위 경로에 게시할 때는 `DASHBOARD_BASE=/dashboard/ npm run build`로 빌드합니다. API 경로는 두 경우 모두 같은 출처의 `/api/metrics/concurrent`, `/api/metrics/unique`, `/api/metrics/gold`, `/api/metrics/gold-per-account`, `/api/metrics/level-leaderboard`, `/api/metrics/gold-leaderboard`, `/api/metrics/weapon-enchant-leaderboard`, `/api/metrics/armor-enchant-leaderboard`입니다.

운영 빌드도 기존 `client/`의 Google 로그인 설정을 공유합니다. 실제 대시보드 출처가 Google OAuth의 허용된 JavaScript 출처에 등록되어 있어야 합니다. `/api/metrics/` 전체를 인증이 적용된 최신 게임 서버로 프록시하고 `Authorization` 헤더를 전달해야 합니다. 인증 전 정적 로그인 화면은 열 수 있지만 지표 API와 데이터는 운영자만 볼 수 있습니다. 배포 후 토큰 없는 요청과 일반 계정이 거부되는지 확인합니다.

## 운영 Nginx 설정

접속 URL은 `https://openmmo.to.nexus/dashboard/`입니다. 기존 게임 사이트의 HTTPS `server` 블록에 대시보드 경로를 추가하므로 도메인과 인증서를 그대로 사용합니다. 2026-09-12 운영에 아래 경로 설정을 반영했으며, 이후 변경이나 재구성도 같은 절차로 진행합니다.

| 항목 | 경로 |
| --- | --- |
| Nginx 설정 | `/etc/nginx/sites-available/openmmo` |
| 활성 설정 링크 | `/etc/nginx/sites-enabled/openmmo` |
| 대시보드 정적 파일 | `/var/www/openmmo-dashboard/` |
| 지표 API 대상 | `http://127.0.0.1:10007/api/metrics/` |

게임 서버에는 `GOOGLE_CLIENT_ID`와 `ADMIN_EMAILS`를 설정해야 합니다. Nginx는 경로 연결과 캐시 제어를 담당하며 어드민 권한은 게임 서버가 확인합니다.

### 배포 스크립트로 자동 게시

`tools/deploy-prod.sh`는 게임 서버와 함께 대시보드도 필요한 경우 빌드하고 게시합니다. 기본 공개 경로는 `/dashboard/`, 게시 폴더는 `/var/www/openmmo-dashboard/`입니다. 각각 배포 환경변수 `DASHBOARD_BASE`, `DASHBOARD_WEBROOT`로 변경할 수 있으며 Nginx 경로도 맞춰야 합니다. 게임의 `WEBROOT`와 같거나 상하위 관계인 폴더는 거부합니다.

마지막으로 게시한 빌드의 식별값을 게시 폴더의 `.deploy-fingerprint`에 기록하고, 다음 배포에서 아래 입력과 비교합니다.

- `git pull` 이후 `dashboard/`의 Git 트리: 소스·의존성·Vite 설정 등 추적 파일 전체
- 실제 빌드에 사용하는 `VITE_*` 환경변수와 `DASHBOARD_BASE`
- 빌드에 사용하는 Node.js 버전

첫 배포, 입력 변경, 게시 폴더의 `index.html` 또는 식별 파일 누락 시 `npm ci`와 빌드를 실행하고 새 파일을 게시합니다. 변경이 없으면 대시보드의 의존성 설치·빌드·게시를 모두 건너뜁니다. API 등 서버 코드만 변경되어도 기존 게임 서버 빌드·재시작은 실행됩니다.

Google 클라이언트 ID는 배포 셸과 대시보드의 Vite production 환경 설정을 우선 사용합니다. 비어 있으면 기존 `client/`의 production 환경 설정에서 `VITE_GOOGLE_CLIENT_ID`를 가져옵니다. Vite의 `.env`, `.env.local`, `.env.production`, `.env.production.local` 우선순위를 따릅니다. 양쪽 모두 비어 있으면 게시 전에 중단합니다.

필요한 빌드가 모두 성공해야 파일 게시와 서비스 재시작을 시작합니다. 대시보드 게시 완료 후에만 식별값을 기록하므로, 빌드·게시 실패 후 재실행하면 다시 시도합니다. Nginx 설정은 스크립트가 변경하지 않습니다.

스크립트는 시작할 때 자신의 사본을 실행하므로 `git pull`로 스크립트 자체가 바뀌어도 해당 실행은 시작 시점의 로직을 사용합니다. 이 기능의 최초 적용 시에는 운영 저장소를 새 스크립트가 포함된 커밋으로 먼저 갱신한 뒤 실행해야 합니다.

아래는 최초 Nginx 구성이나 대시보드를 수동으로 게시할 때의 절차입니다. 자동 배포를 사용하는 경우 빌드·파일 배치 단계는 스크립트가 처리합니다.

### 1. 대시보드 빌드

아래 명령은 운영 서버의 같은 SSH 셸에서 순서대로 실행합니다.

```bash
ssh prod
cd ~/work/OnlineRPG/dashboard
```

수동 빌드도 기존 `client/.env.local`의 `VITE_GOOGLE_CLIENT_ID`를 사용하므로 대시보드 폴더에 별도 환경 파일을 만들 필요가 없습니다. Google OAuth의 허용된 JavaScript 출처는 `https://openmmo.to.nexus`이며 `/dashboard/` 경로는 포함하지 않습니다. 같은 클라이언트 ID로 게임에 로그인하고 있다면 기존 출처 설정을 공유합니다.

```bash
npm ci
DASHBOARD_BASE=/dashboard/ npm run build
```

`DASHBOARD_BASE`를 생략하면 기본 설정에서 JS·CSS 주소가 `/assets/`를 가리키므로, 운영 하위 경로용 빌드에는 반드시 `/dashboard/`를 지정합니다. `DASHBOARD_API_TARGET`은 Vite 개발·미리보기용이며 운영 API 연결은 Nginx의 `proxy_pass`가 결정합니다.

### 2. 빌드 파일 배치

위의 `dashboard/` 디렉터리에서 실행합니다.

```bash
sudo install -d -o www-data -g www-data -m 755 /var/www/openmmo-dashboard
sudo rsync -a --delete dist/ /var/www/openmmo-dashboard/
sudo chown -R www-data:www-data /var/www/openmmo-dashboard
```

게임 배포가 `/var/www/openmmo/`를 `rsync --delete`로 동기화하므로 대시보드 파일은 별도 폴더에 둡니다. 최종 파일 위치는 `/var/www/openmmo-dashboard/index.html`과 `/var/www/openmmo-dashboard/assets/`입니다.

### 3. Nginx 경로 추가

기존 설정을 백업한 뒤 편집합니다. 아래 `nginx_backup` 변수는 복구할 때 같은 셸에서 사용합니다.

```bash
nginx_backup="/etc/nginx/sites-available/openmmo.bak-$(date +%Y%m%d-%H%M%S)"
sudo cp -a /etc/nginx/sites-available/openmmo "$nginx_backup"
sudoedit /etc/nginx/sites-available/openmmo
```

`server_name openmmo.to.nexus;`와 `listen 443 ssl;`이 있는 기존 HTTPS `server { ... }` 안에 다음 블록을 추가합니다. 이미 같은 경로가 있으면 해당 블록을 수정합니다. 기존 게임의 `/`, `/assets/`, `/ws`, `/api/` 설정은 유지합니다.

```nginx
location = /dashboard {
    return 301 /dashboard/;
}

location ^~ /dashboard/ {
    alias /var/www/openmmo-dashboard/;
    index index.html;
    add_header Cache-Control "no-cache";
}

location ^~ /api/metrics/ {
    proxy_pass http://127.0.0.1:10007;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header Authorization $http_authorization;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_cache off;
    proxy_buffering off;
    add_header Cache-Control "no-store" always;
}
```

`alias` 경로 끝의 `/`를 유지합니다. `/dashboard`는 `/dashboard/`로 이동하고, `/dashboard/assets/...`는 대시보드 전용 폴더에서 읽습니다. `^~`는 기존 게임의 정적 파일 정규식보다 이 경로를 우선 적용합니다. `/api/metrics/`는 기존 `/api/`보다 구체적인 경로이므로 지표 전용 프록시 설정이 적용됩니다.

### 4. 검사 후 적용

```bash
sudo nginx -t && sudo systemctl reload nginx
systemctl is-active nginx
```

문법 검사에 성공한 경우에만 reload합니다. 이 작업은 게임 서버나 NPC 서비스를 재시작하지 않습니다. 이후 대시보드 정적 파일만 교체할 때는 Nginx reload도 필요하지 않습니다.

설정에 문제가 있으면 백업을 복구합니다.

```bash
sudo cp -a "$nginx_backup" /etc/nginx/sites-available/openmmo
sudo nginx -t && sudo systemctl reload nginx
```

### 5. 접속 확인

```bash
curl -I https://openmmo.to.nexus/
curl -I https://openmmo.to.nexus/dashboard
curl -I https://openmmo.to.nexus/dashboard/
curl -i https://openmmo.to.nexus/api/metrics/session
```

| 요청 | 서버·대시보드 배포 완료 후 기대 결과 |
| --- | --- |
| `/` | 기존 게임 화면, HTTP 200 |
| `/dashboard` | `/dashboard/`로 HTTP 301 |
| `/dashboard/` | 로그인 화면, HTTP 200 |
| 토큰 없는 `/api/metrics/session` | HTTP 401, `Cache-Control: no-store` |

브라우저에서는 일반 Google 계정이 거부되고 어드민 계정만 통계를 볼 수 있는지 확인합니다. 로그아웃하면 로그인 화면으로 돌아가야 합니다.

`/dashboard/`가 404이면 빌드 파일과 `alias` 경로를, 403이면 `index.html` 존재 여부와 파일·디렉터리 읽기 권한을 확인합니다. 로그인 화면은 열리지만 JS·CSS가 404이면 `/dashboard/`를 기준으로 다시 빌드합니다. 지표 API의 404는 서버 코드·프록시 경로를, 502는 REST 서버의 `127.0.0.1:10007` 리스닝 상태를 확인합니다.

지표의 정확한 범위와 API는 [METRICS.md](../doc/METRICS.md)를 참고하세요.

골드 생산 순위 API는 기존 경로를 유지하며 같은 출처의 `/api/metrics/item-gold-sources?hours=24`입니다.

골드 생산 순위 바로 아래에 골드 소모 순위를 표시합니다. 상인 구매와 재매입은 아이템별로 구분하고 가판 판매 수수료·토지세·토지 체납 복구 비용을 함께 정렬합니다. 수량 또는 횟수·실제 소모액·비중을 표시하며 기간 선택과 새로고침은 독립적입니다. 주민 NPC·플레이어 간 거래 대금과 세금 계좌 입출금은 제외합니다. API는 `/api/metrics/gold-sinks?hours=24`이며, 적용 후 수집한 기록만 다음 정각부터 표시합니다.

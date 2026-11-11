# 가문 시스템

> 상태: 1차(가문 기본·영지 공유) 구현됨, 프로토콜 144 (2026-11-10). 2차(채팅·창고·게시판) 구현됨, 프로토콜 146, 3차(문장·깃발·합병) 구현됨, 프로토콜 147 (2026-11-11). 2·3차 미배포.

영지를 가진 플레이어가 가문을 세우고, 다른 플레이어를 받아들여 이름·영지·문장·채팅·게시판·창고를 함께 쓴다.

코드에서는 `family`로 부른다. `house`는 집짓기(`shared::housing`, `PlaceHouse` 등)가 쓰고 있고, `guild`는 `shadow_guild` 던전이 쓰고 있다.

## 목표

1. 영지 소유자가 가문을 창설한다.
2. 가문 이름은 서버 전체에서 유일하다.
3. 가문 이름은 캐릭터 이름 뒤에 표시된다 (`jake adventurer`).
4. 가문이 없는 플레이어를 받아들인다.
5. 두 가문이 합병한다. 이름은 둘 중 하나를 고른다.
6. 가문 영지를 가문원이 함께 쓴다.
7. 가문 안에 계급을 둔다.
8. 계급별로 권한을 준다.
9. 가문원의 망토 문장을 가문 문장으로 통일한다.
10. 가문 문장이 들어간 깃발·휘장을 영지에 놓는다.
11. 가문 채팅 채널.
12. 영지에 놓는 가문 게시판 (가문원만 읽고 쓴다).
13. 가문 공유 창고.

## 범위 밖

- 가문 간 전쟁·동맹·순위
- 가문 레벨·경험치·가문 전용 버프
- 가문 금고(골드 공동 계좌). 영지 세금 금고(`land_estates.treasury`) 입금만 다룬다.
- 왕령지(crown land)와 영지 양도
- 가문 분리
- 문장 편집기. 기존 망토 그림 업로드 경로를 쓴다.

## 1. 소속 단위

결정: 캐릭터 단위.

- 가문원은 캐릭터다. 한 캐릭터는 한 가문에만 속한다.
- 같은 계정의 다른 캐릭터는 자동으로 가입되지 않는다. 따로 초대받아 같은 가문에 들어가거나, 다른 가문에 들어가거나, 가문 없이 지낼 수 있다.
- 영지는 계정 단위(`land_estates.account_name`)로 남는다. 같은 계정 캐릭터가 자기 영지를 쓰는 기존 규칙은 가문과 무관하게 유지된다.
- 영지는 한 번에 한 가문에만 연결된다. 그 계정의 캐릭터 중 해당 가문에 속한 캐릭터가 연결한다.
- 그 계정에서 해당 가문에 속한 캐릭터가 하나도 남지 않으면 영지는 가문에서 떨어진다.
- 가문 밖에 있는 같은 계정 캐릭터가 얻는 것은 자기 영지의 건축·배치 권한뿐이다. 이는 영지 소유자 본인의 권한이다. 가문 창고·게시판·채팅은 영지 접근이 아니라 가문 소속으로 검사하므로 쓸 수 없다.

## 2. 창설

- 조건: 체납 없는(`missed = 0`) 영지를 가진 계정의 캐릭터, 가문 미소속.
- 창설자가 가주가 되고, 창설자 계정의 영지가 가문 영지로 연결된다. 그 영지가 이미 다른 가문에 연결되어 있으면(같은 계정의 다른 캐릭터가 연결) 창설할 수 없다.
- 창설 장소: Aldwin(토지 등기관)과의 대화. 영지 문서를 파는 NPC와 같다. Aldwin 창의 "가문 업무"에 창설·개명·해산 메뉴가 있다. 가문이 없으면 창설, 가주면 개명과 해산이 보인다. Aldwin이 자는 동안은 할 수 없다.
- 창설 비용: 인지 10장. 인지(`revenue_stamp`)는 Aldwin이 한 장 1,000c에 판다. 합계 10,000c로 영지 문서 50,000c의 1/5이다.
- 인지는 가방에서 소모된다. 서류가 거절되면 돌려받는다. 게시 용지·편지지와 같은 흐름이다.

## 3. 가문 이름

- 규칙은 캐릭터 이름과 같다 (`auth.rs`의 이름 검증: ASCII 영숫자, `_`, 한글, 가나, 한자. 숫자·`_`로 시작 불가. 길이도 같다). `banned_names.txt`를 함께 적용한다.
- 대소문자 무시 유일.
- 캐릭터 이름과 겹칠 수 없다. 가문 창설 때 캐릭터 이름을 확인하고, 캐릭터 생성·개명 때 가문 이름과 버려진 가문 이름을 확인한다.
- 개명: 가주만, 현실 시간 30일에 한 번. 인지 10장. Aldwin에게서 한다.
- 합병으로 버려진 이름은 다른 가문이 쓸 수 없다. 나중에 가문 분리가 생기면 원래 가문이 되찾는다.
- 해산된 가문의 이름과 개명 전 이름은 30일 뒤 누구나 다시 쓸 수 있다.

## 4. 가입·탈퇴·추방

- 초대: 초대 권한이 있는 가문원이 접속 중인 가문 미소속 캐릭터에게 보낸다. 대기 중 초대는 메모리에만 두고 120초 뒤 만료된다 (친구 요청과 같은 방식). 공식 NPC는 초대할 수 없다.
- 정원: 30명.
- 탈퇴: 가주 외에는 언제든 가능.
- 추방: 추방 권한이 있고 대상보다 계급이 높은 가문원.
- 재가입 대기: 탈퇴·추방 후 24시간 동안 다른 가문에 가입 불가 (공유 창고 털고 옮기기 억제).
- 가주는 승계 후에만 탈퇴한다.
- 해산: 가문원이 가주 혼자일 때, Aldwin에게 가서 인지 1장을 내고 한다. 가문 창에서는 해산할 수 없다. 해산하면 이름이 30일 묶이고 가주에게도 24시간 재가입 대기가 걸린다.
- 캐릭터 삭제 시 탈퇴로 처리한다. 가주였다면 6절의 승계 순서로 자동 승계된다.

## 5. 영지 공유

가문은 영지를 소유하지 않는다. 영지 소유자는 그대로 계정이고, 가문은 접근 권한을 덧씌운다.

- `land_estates.family_id`가 있는 영지가 가문 영지다.
- 창설 시 창설자 영지가 연결된다. 영지를 가진 가문원은 자기 영지를 가문에 연결하거나 뗄 수 있다.
- 가문원이 탈퇴·추방되어 그 계정에 해당 가문 캐릭터가 남지 않으면 영지는 가문에서 떨어져 원래 소유자만 쓴다. 영지를 빼앗기는 일은 없다.
- 합병 시 두 가문의 영지가 모두 새 가문에 연결된다. 영지 수 상한은 따로 두지 않는다. 계정당 1개이므로 가문 정원이 상한이다.
- 세금은 영지 소유 계정의 금고에서 빠진다. 권한이 있는 가문원은 금고에 입금할 수 있다. 출금은 소유 계정만.
- 체납(`missed > 0`) 영지는 기존 규칙대로 소유자 포함 모두 못 쓴다.
- 압류되면 영지는 가문에서 떨어진다. 가문 영지가 하나도 없어도 가문은 유지되고, 영지에 묶인 기능(건축·깃발·게시판·창고)만 멈춘다.

가문원이 지은 집과 놓은 가구는 그 영지에 남는다. 탈퇴한 사람은 권한을 잃는다. 영지가 가문에서 떨어질 때 그 위의 가문 창고 처리는 9절.

### 바뀌는 권한 검사

| 검사 | 위치 | 지금 | 변경 |
|---|---|---|---|
| 집 짓기 | `auth.fence_plots` (`auth_fence.rs`) | 같은 계정의 체납 없는 영지 | + 가문 영지, 권한 `BUILD_HOUSE` |
| 집 철거 | `characters_share_account` (`house_building.rs`) | 집 주인과 같은 계정 | + 집 전체가 자기 계정 영지 안, 또는 권한 `DEMOLISH_HOUSE`가 있는 가문 영지 안 |
| 장식 가구 배치·이동·회수·팻말 글 | `placement_estate` / `chest_access` (`auth_estate_storage.rs`) | 같은 계정 | + 권한 `PLACE_FURNITURE` |
| 보관 궤짝 배치·열기·회수 | 같은 곳 | 같은 계정 | 변경 없음 |
| 울타리·조경 | `fence_plots`, `save_fence_edit`, `landscaping.rs` | 같은 계정 | + 권한 `LANDSCAPE` |

- 모든 검사는 SQL 조건 하나(`estate_access_sql`, `auth_family.rs`)를 쓴다: 계정 소유이거나, 영지의 `family_id`가 내 가문이고 내 단계가 해당 비트를 가진다. 두 조건 모두 상수와 비교하므로 계정 인덱스와 `idx_land_estates_family`를 쓴다. 가문 권한이 없는 대상(보관 궤짝)은 비트 대신 `None`을 넘겨 계정 조건만 쓴다.
- 보관 궤짝은 영지 소유 계정만 놓고 연다. 궤짝 접근이 영지 계정 기준이라, 가문원이 남의 영지에 놓으면 내용물이 영지 주인 것이 되기 때문이다. 공유 보관은 2차의 가문 창고로 한다.
- 영지 주인은 자기 영지 위에 가문원이 지은 집을 철거할 수 있다.
- 도구 상자 모드는 `LANDSCAPE`·`BUILD_HOUSE`·`DEMOLISH_HOUSE` 중 하나라도 있으면 그 영지에서 열린다. 칠하기는 `LANDSCAPE` 구획으로만 제한된다.
- `LandscapingMode.owned_character_ids`에는 같은 계정 캐릭터, 가문원, 가문 영지 소유 캐릭터가 들어간다. 클라이언트가 울타리 회수·집 철거를 제안할 때 쓰는 힌트이고, 실제 판정은 서버가 한다.

## 6. 계급

결정: 권한 단계는 4개로 고정하고 호칭은 가문이 정한다.

| 단계 | 기본 호칭 | 비고 |
|---|---|---|
| 0 | 가주 | 1명. 모든 권한. 권한 변경 불가 |
| 1 | 원로 | |
| 2 | 가족 | 신규 가입자의 기본 단계 |
| 3 | 식객 | |

- 아빠·엄마·장남 같은 호칭은 단계 호칭으로 가문이 직접 붙인다. 호칭 최대 12자.
- 개인별 호칭(같은 단계 안에서 "삼촌", "차남")이 필요하면 `family_members.label`을 추가한다. 1차에서는 넣지 않는다.
- 승계: 가주가 다른 가문원을 지명해 넘긴다.
- 승계 순서: 단계가 높은 순, 같은 단계에서는 가입이 빠른 순.
- 가주 캐릭터가 삭제되면 승계 순서의 첫 가문원이 자동으로 가주가 된다.
- 가주가 30일 미접속이면 승계 순서의 첫 가문원이 가주 자리를 넘겨받을 수 있다. 그 사람도 30일 미접속이면 다음 사람에게 자격이 간다.

### 권한 비트

단계 1~3에 대해 가주가 켜고 끈다.

| 비트 | 내용 | 원로 기본 | 가족 기본 | 식객 기본 |
|---|---|---|---|---|
| `INVITE` | 초대 | O | | |
| `KICK` | 추방 | O | | |
| `BUILD_HOUSE` | 집 짓기 | O | | |
| `DEMOLISH_HOUSE` | 집 철거 | O | | |
| `PLACE_FURNITURE` | 가구·깃발 배치와 회수 | O | O | |
| `LANDSCAPE` | 울타리·조경 | O | | |
| `STORAGE_DEPOSIT` | 가문 창고 넣기 | O | O | O |
| `STORAGE_WITHDRAW` | 가문 창고 꺼내기 | O | O | |
| `BOARD_WRITE` | 게시판 쓰기 | O | O | |
| `BOARD_MODERATE` | 남의 글 내리기 | O | | |
| `TREASURY_DEPOSIT` | 영지 금고 입금 | O | O | O |
| `EDIT_EMBLEM` | 문장 변경 | | | |
| `EDIT_RANKS` | 호칭·권한·단계 변경 | | | |

게시판 읽기와 가문 채팅은 모든 가문원에게 열려 있다.

## 7. 이름 표시

- `shared::entity::Player` 끝에 `family: Option<String>`을 `#[serde(default)]`로 추가한다 (`skip_serializing_if` 금지, 위치 배열 인코딩).
- 실시간 변경은 `PlayerFamilyChanged { player_id, family }`. `PlayerTitleChanged`와 같은 전파 경로 (WORLD_EVENT_DELIVERY.md §6).
- 클라이언트 이름표(`PlayerModel.svelte`)는 `이름 가문이름`을 한 줄로 그린다. 가문 이름 부분은 색을 달리한다.
- 채팅 줄, 파티 창, 친구 목록에도 같은 형식으로 표시한다.

## 8. 문장, 망토, 깃발

### 문장

- 가문은 `emblem_texture`(sha256 해시)와 `emblem_color`를 가진다.
- 등록은 기존 망토 그림 경로를 쓴다: `POST /api/cape-texture` 업로드 후 `SetFamilyEmblem { texture, color }`. 320×512, 업로드 제한, 신고·해시 차단이 그대로 적용된다 (CAPE_CUSTOMIZATION.md). 가문 창의 "문장"에서 색과 그림을 고른다. 비용은 없다.
- 차단된 해시가 문장이면 그림 없이 색만 보인다. 서버는 가문 명부를 읽을 때 차단된 그림을 뺀다.

### 망토

- 가문 문장이 있으면 가문원이 두른 망토는 가문 문장과 색으로 보인다.
- 아이템 인스턴스의 `cape_texture`/`cape_color`는 바꾸지 않는다. 서버가 `Player.back_texture`/`back_color`와 `PlayerBackChanged`를 만들 때 가문 문장으로 덮어쓴다 (`family::shown_cape`). 문장 색이 없으면 망토 자기 색을 쓰고, 그림은 문장 그림으로 바꾼다(없으면 지운다). 탈퇴하면 원래 그림이 다시 보인다. 캐릭터 선택 화면도 같은 규칙이다.
- 문장이 없는 가문은 덮어쓰지 않는다.
- 개인별 끄기 옵션은 없다 (요구사항이 "통일").

### 깃발·휘장

- `data-src/estate_storage.csv`에 가구로 추가한다: `family_banner`(세우는 깃발, 3,000c), `family_pennant`(벽걸이 휘장, 1,500c). Aldwin이 판다. 휘장은 벽 장식 칼처럼 높이를 올려 벽에 건다.
- 가문 영지에만 놓을 수 있다. 권한은 `PLACE_FURNITURE`.
- 그림은 `EstateChest.emblem`으로 서버가 채워 보낸다: 가구의 `family_id`가 영지의 `family_id`와 같을 때 그 가문 문장. 가구 행에는 저장하지 않는다.
- 문장 변경, 영지 연결·분리, 탈퇴, 해산, 합병 때 그 가문 가구를 다시 읽어 바뀐 것만 알린다 (`refresh_family_furniture`).
- 영지가 가문에서 떨어지면 깃발은 문장 없는 무지(리넨색) 깃발로 보인다.
- 모델: `tools/blender-scripts/build_family_banner.py`. 천은 `emblem_cloth` 메시이고, 클라이언트(`utils/familyEmblem.ts`)가 망토처럼 앞면에만 그림을 색 위에 얹는다.

## 9. 가문 창고

마을 은행이 없으므로 영지 궤짝 체계에 얹는다.

- `estate_storage.csv`에 `family_chest`를 추가한다. 가문 영지에만 놓을 수 있다. 가구 상점에는 진열대가 필요해서 Aldwin이 판다 (6,000c).
- 놓을 때 영지의 `family_id`를 `estate_chests.family_id`에 적고, 소유자(`owner_id`)는 놓은 가문원이 아니라 영지 소유 캐릭터로 둔다. 놓은 사람이 탈퇴·삭제돼도 창고가 남는다. 가문 가구는 같은 가문의 영지 안에서만 옮긴다.
- 용량: 200kg (`storage_chest`는 50kg). 개수는 가문당 4개.
- 열기: 3m 이내, 해당 가문원. 넣기 `STORAGE_DEPOSIT`, 꺼내기 `STORAGE_WITHDRAW`.
- 입출고 기록을 남긴다 (`family_storage_log`). 가문원이 창에서 최근 200건을 본다. 서버 로그에도 한 줄 남긴다.
- 회수는 비어 있을 때만.
- 가문 창고의 내용물은 가문 것이다. 영지 소유자라도 가문 권한 없이는 꺼낼 수 없다.
- 영지 소유자는 그 영지의 가문 창고가 모두 비어 있어야 영지를 가문에서 뗄 수 있다.
- 추방·압류로 영지가 떨어지면 그 영지의 가문 창고는 꺼내기 전용이 된다. `STORAGE_WITHDRAW` 권한이 있는 가문원이 찾아가 꺼낸다. 비면 영지 소유자가 회수한다.
- 가문이 해산되면 남은 내용물은 마지막 가주 캐릭터만 꺼낼 수 있다 (`family_id`를 비우고 `owner_id`를 가주로). 캐릭터 삭제로 가문원이 모두 사라져 해산되면 영지 소유 캐릭터가 꺼낸다.
- 영지가 사라진 궤짝을 누구나 여는 기존 규칙은 가문 창고에 적용하지 않는다.

## 10. 가문 채팅

- 명령 `/g`. (`/f`는 친구, `/p`는 파티.)
- `ClientMessage::FamilyChat { message }` → `ServerMessage::FamilyChatMessage { from, message }`.
- 접속 중인 가문원에게 `send_direct_message_to_players`로 보낸다 (파티 채팅과 같은 경로). 차단·채팅 금지 필터 적용.
- 가문원 접속·퇴장, 가입·탈퇴·승계는 가문 채널에 시스템 줄로 알린다.

## 11. 가문 게시판

- 기존 게시판은 맵 에디터가 놓는 지역 오브젝트(`BoardKey { rx, rz, object_id }`)다. 가문 게시판은 플레이어가 놓는 가구(`family_board`, Aldwin 2,000c)로 추가한다.
- 게시판 식별자는 `NoticeBoardRef { Village | Estate }`다. 저장은 `notice_posts`/`notice_comments`를 그대로 쓰고, 가문 게시판은 `BoardKey::estate(furniture_id)`(어느 지역 파일도 쓰지 않는 `rx = i32::MIN`) 아래 둔다.
- 글쓰기에 게시 용지가 들지 않는다.
- 읽기·쓰기·댓글은 그 영지의 가문원만. 서버가 목록 요청 단계에서 거른다.
- 글 내리기: 글쓴이, `BOARD_MODERATE`.
- 한도는 `shared/src/notice_board.rs` 값을 쓴다. 가문 게시판은 30일 만료가 없고, 게시판당 50건에서 오래된 글부터 밀린다.
- 가문에서 떨어진 영지의 게시판은 읽기·쓰기가 막힌다. 글은 남는다.

## 12. 합병

- A 가주가 B 가주에게 제안한다. 제안에 남길 이름(A 또는 B), 남길 문장(A 또는 B), 남을 가주를 담는다. B 가주가 수락하면 성립한다. 두 가주 모두 접속 중이어야 한다. 제안은 120초 뒤 만료되고, 거절된 제안은 만료 전까지 다시 보낼 수 없다.
- 이름이 남는 쪽이 남는 가문이다. 다른 가문의 가문원·영지·가구·창고 기록을 옮기고 그 가문 행을 지운다.
- 합친 인원이 정원을 넘으면 거절된다.
- 흡수되는 쪽 가주는 단계 1(원로)이 된다. 나머지는 단계를 유지한다.
- 단계 호칭과 권한은 남는 가문 것을 쓴다.
- 영지, 가문 창고, 게시판은 모두 새 가문에 연결된다. 창고 개수가 상한을 넘으면 넘는 채로 두고 새로 놓는 것만 막는다.
- 버려진 이름은 3절대로 다시 쓸 수 없다.
- 합병은 되돌릴 수 없다. 수락 창에 그 사실과 바뀌는 내용을 모두 보여준다.

## 13. 데이터

SQLite. 마이그레이션 파일이 없으므로 `ensure_family_schema`를 `AuthService::new`에서 호출한다.

```sql
CREATE TABLE IF NOT EXISTS families (
    id              INTEGER PRIMARY KEY,
    name            TEXT NOT NULL,
    emblem_texture  TEXT,
    emblem_color    TEXT,
    renamed_at      INTEGER,
    created_at      INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_families_name_nocase ON families(name COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS family_members (
    character_id  INTEGER PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    family_id     INTEGER NOT NULL REFERENCES families(id) ON DELETE CASCADE,
    rank          INTEGER NOT NULL,
    joined_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_family_members_family ON family_members(family_id);

CREATE TABLE IF NOT EXISTS family_ranks (
    family_id    INTEGER NOT NULL REFERENCES families(id) ON DELETE CASCADE,
    rank         INTEGER NOT NULL,
    label        TEXT NOT NULL,
    permissions  INTEGER NOT NULL,
    PRIMARY KEY (family_id, rank)
);

CREATE TABLE IF NOT EXISTS family_retired_names (
    name          TEXT PRIMARY KEY COLLATE NOCASE,
    successor_id  INTEGER REFERENCES families(id) ON DELETE SET NULL,
    retired_at    INTEGER NOT NULL,
    reusable_at   INTEGER
);

CREATE TABLE IF NOT EXISTS family_leave_cooldowns (
    character_id  INTEGER PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    left_at       INTEGER NOT NULL
);

ALTER TABLE land_estates ADD COLUMN family_id INTEGER REFERENCES families(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_land_estates_family ON land_estates(family_id) WHERE family_id IS NOT NULL;
```

2차에서 추가:

```sql
CREATE TABLE IF NOT EXISTS family_storage_log (
    id              INTEGER PRIMARY KEY,
    family_id       INTEGER NOT NULL,
    chest_id        INTEGER NOT NULL,
    character_id    INTEGER NOT NULL,
    character_name  TEXT NOT NULL,
    item_def_id     TEXT NOT NULL,
    quantity        INTEGER NOT NULL,
    at              INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_family_storage_log_family ON family_storage_log(family_id, id);

ALTER TABLE estate_chests ADD COLUMN family_id INTEGER;
CREATE INDEX IF NOT EXISTS idx_estate_chests_family ON estate_chests(family_id) WHERE family_id IS NOT NULL;
```

가주는 단계 0인 가문원이다. 따로 저장하지 않는다. 캐릭터·계정 삭제로 가문원 행이 연쇄 삭제되면 `settle_families`가 가주 없는 가문에 다음 순서를 올리고, 가문원 없는 가문을 해산하고, 계정 가문원이 없는 영지를 뗀다.

창설·개명·해산 수수료는 인지 구매로 낸다. 골드 소모 기록에는 기존 `item_purchase`(`revenue_stamp`)로 잡힌다.

`family_retired_names.successor_id`는 합병으로 그 이름을 흡수한 가문이다. 해산·개명이면 NULL. `reusable_at`은 다시 쓸 수 있는 시각이고, 합병이면 NULL(재사용 불가)이다. 지난 행은 창설·개명 때 지운다.

`family_storage_log.quantity`는 넣기 양수, 꺼내기 음수. 가문당 2,000건을 넘으면 오래된 것부터 지운다.

## 14. 프로토콜

1차는 프로토콜 144다. 서버·웹 wasm·agent-client를 함께 릴리스한다 (REMOTE_AGENT_CLIENT.md).

### 1차 (v144, 구현됨)

클라이언트 → 서버

- `CreateFamily { merchant_player_id, name }`: 토지 등기관 앞에서
- `InviteToFamily { name }`, `RespondFamilyInvite { inviter_id, accept }`
- `LeaveFamily`, `KickFamilyMember { character_id }`
- `DissolveFamily { merchant_player_id }`: 토지 등기관 앞에서
- `SetFamilyMemberRank { character_id, rank }`, `TransferFamilyHead { character_id }`, `ClaimFamilyHead`
- `SetFamilyRank { rank, label, permissions }`
- `RenameFamily { merchant_player_id, name }`: 토지 등기관 앞에서
- `AttachEstateToFamily { attach }`
- `DepositFamilyEstate { estate_id, amount }`

서버 → 클라이언트

- `FamilyState { family: Option<FamilyInfo> }`: 입장 때와 변경 때마다 접속 중인 가문원 각자에게. 이름, 단계 정의, 가문원(이름·레벨·직업·단계·접속 여부), 연결된 영지, 내 단계, 영지 연결 가능 여부, 승계 가능 여부, 개명 가능 시각.
- `FamilyInviteReceived { inviter_id, inviter_name, family_name }`
- `PlayerFamilyChanged { player_id, family }`: 주변 플레이어에게
- 거절 사유는 `Family: `로 시작하는 `SystemMessage`로 보낸다. 전용 결과 메시지는 없다.

기존 메시지 변경

- `Player` 끝에 `family` 필드 추가.

### 2차 (v146, 구현됨)

- `FamilyChat { message }` → `FamilyChatMessage { from, message }`
- `RequestFamilyStorageLog` → `FamilyStorageLog { entries }` (최근 200건, 페이지 없음)
- `EstateChestState`에 `can_withdraw`, `family`.
- 게시판 메시지의 `object_id`를 `board: NoticeBoardRef`로 바꿈 (`TakeDownNotices` 제외). 거절 사유 `not_family`, `no_family_rank`.

### 3차 (v147, 구현됨)

- `SetFamilyEmblem { texture, color }`
- `ProposeFamilyMerge { head_name, terms: { keep_name, keep_emblem, keep_head } }`, `RespondFamilyMerge { proposer_id, accept }` → `FamilyMergeProposal { proposal }`
- `FamilyInfo`에 `emblem_texture`, `emblem_color`. `EstateChest`에 `family_id`, `emblem`.

`shared/src/dungeon/`, `data-src/dungeons.csv`, `dungeon_spawns.csv`, `shared/src/housing.rs`는 건드리지 않는다 (레이아웃 지문).

## 15. 성능

동시 접속 5,000명 기준.

- 소속·단계·영지 연결은 DB에만 둔다. `GameState.families`는 가문별 접속 중 가문원 목록과 대기 중 초대만 가진다. 명부는 변경·입장·퇴장 때 DB에서 한 번 읽어 접속 중 가문원에게 보낸다.
- 채팅 팬아웃은 접속 중 가문원 수(최대 30)에 비례한다.
- 문장 변경, 개명, 합병에 따른 이름 변경은 접속 중 가문원 각자의 주변 구역에 한 번씩 알린다. 가문원 수만큼 발행.
- 영지 권한 검사는 지금도 요청마다 DB를 읽는다 (`fence_plots`). 가문 조인이 하나 더 붙는다. 건축·배치는 빈도가 낮아 그대로 둔다.
- 락 순서: `families`는 말단 락이다. 잡은 채로 다른 `GameState` 락을 잡지 않는다 (`game_state/mod.rs` 순서 문서). 가문 창고는 기존 궤짝 경로를 타므로 `player_gold`/`inventories` 순서를 그대로 따른다.

## 16. 구현 단계

세 번에 나눠 배포한다. 배포마다 프로토콜을 올린다.

### 1차 (**구현됨**, 미배포)

1. **가문 기본**: 테이블, 창설, 초대·탈퇴·추방, 단계와 권한, 승계, 개명, 이름 표시, 가문 창.
2. **영지 공유**: `land_estates.family_id`, 5절의 권한 검사 변경, 금고 입금.

코드: `shared/src/family.rs`, `server/src/auth_family.rs`, `server/src/game_state/family.rs`, 클라이언트 `familyStore.ts`·`FamilyPanel.svelte`·`FamilyInviteToast.svelte`·`FamilyFoundSection.svelte`. 테스트: `server/src/game_state/tests/family_tests.rs`.

가문 창은 H 키나 소셜 메뉴의 친구 아래 버튼으로 연다. 줍기 키를 H로 바꾼 경우 단축키는 꺼진다.

1차에 없는 것:

- agent-client는 주변 플레이어의 가문 이름을 인지하지만 초대에 답하지 못한다.
- 귓속말·파티 채팅 줄과 친구 목록에는 가문 이름이 붙지 않는다. 주변 채팅 줄과 파티 창에는 붙는다.

### 2차 (**구현됨**, 미배포)

3. **가문 채팅**: `/g`, 채팅 채널 메뉴, 접속·퇴장 알림.
4. **가문 창고**: `family_chest`, 입출고 기록 (가문 창의 "가문 창고 기록").
5. **가문 게시판**: `family_board`.

코드: `game_state/family.rs`(채팅·기록), `auth_estate_storage.rs`(`family_chest_access`, `family_board_permissions`), `game_state/notice_board.rs`(`Board`). 테스트: `family_tests.rs`.

2차에 없는 것:

- agent-client는 가문 채팅을 읽지만 말하지 못하고, 가문 게시판·창고를 쓰지 않는다.
- 가문 창고·게시판 아이콘은 기존 보관함·게시 용지 아이콘을 쓴다.

### 3차 (**구현됨**, 미배포)

6. **문장**: 문장 등록, 망토 덮어쓰기, 깃발·휘장 애셋과 가구.
7. **합병**: 가문 창의 "다른 가문과 합병"(가주), 상대 가주에게 수락 창.

테스트: `family_tests.rs`의 문장·깃발·합병 테스트.

3차에 없는 것:

- agent-client는 문장·합병을 다루지 않는다.

## 구현 시 함께 고칠 문서

- LAND_SYSTEM.md "범위 밖"의 "공동 영지·길드·권한 역할" 항목
- ESTATE_STORAGE.md, NOTICE_BOARD.md, CAPE_CUSTOMIZATION.md, CHARACTER_NAMES.md
- TODO.md 108행 "party, guild 기능"

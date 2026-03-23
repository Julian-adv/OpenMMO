# Housing System — Modular Room-Based Architecture

## Overview

유저가 방(Room)을 자유롭게 조합하여 집을 짓는 모듈러 하우징 시스템.
벽/바닥/지붕 텍스쳐 커스터마이즈, 문/창문 배치, 2층 지원.
집 안에 들어가면 앞벽+지붕이 숨겨져 내부가 보인다.

## Data Model

### HouseData

```rust
pub struct HouseData {
    pub id: String,
    pub owner_id: String,
    pub origin: Position,          // 월드 좌표 (1m 그리드 스냅)
    pub rooms: Vec<RoomData>,
    pub passability: Vec<PassabilityGrid>,  // 셀 기반 통행 가능 여부
}
```

### RoomData

```rust
pub struct RoomData {
    pub room_type: RoomType,        // Normal | Stairwell
    pub roof_type: RoofType,        // Flat | Gabled | Steep
    pub roof_ridge_dir: RoofRidgeDir, // Auto | X | Z
    pub local_x: i32,              // house origin 기준 오프셋 (미터)
    pub local_z: i32,
    pub size_x: u8,                // 3~6m
    pub size_z: u8,                // 3~6m
    pub floor_level: u8,           // 0 = 1층, 1 = 2층
    pub floor_texture: u8,         // 텍스쳐 카탈로그 인덱스
    pub roof_texture: u8,
    pub wall_height: f32,          // 기본 3m
    /// 벽은 1m 세그먼트 배열 (예: 5m 북벽 → 5개 WallConfig)
    pub wall_north: Vec<WallConfig>,  // length = size_x
    pub wall_south: Vec<WallConfig>,  // length = size_x
    pub wall_east: Vec<WallConfig>,   // length = size_z
    pub wall_west: Vec<WallConfig>,   // length = size_z
}
```

### WallConfig

```rust
pub struct WallConfig {
    pub variant: WallVariant,
    pub texture: u8,
    pub is_open: bool,          // 문 열림 상태 (WithDoor 전용, 기본 false)
}

pub enum WallVariant {
    Solid,
    WithDoor,
    WithWindow,
    Open,           // 인접 방 연결 또는 계단 공간
}
```

### PassabilityGrid

```rust
pub struct PassabilityGrid {
    pub floor_level: u8,
    pub origin_x: i32,         // house local 좌표 기준 그리드 원점
    pub origin_z: i32,
    pub width: u8,             // X 셀 수
    pub depth: u8,             // Z 셀 수
    pub cells: Vec<u8>,        // N=1, E=2, S=4, W=8 비트마스크
}
```

- 방 크기: 3~6m (정해진 세트), 배치 그리드: 1m 단위 스냅
- 벽은 1m 세그먼트 단위: 5m 북벽 → `wall_north` 길이 5
- 인접 방 공유 면: 양쪽 모두 `Open`이어야 함 (서버 검증)
- 2층 방의 `floor_level: 1`, y 오프셋 = wall_height + FLOOR_THICKNESS

## Wall Collision (Cell-Based Passability)

### 개요

1m 셀 기반 통행 가능 여부 시스템. 각 셀에 동서남북(N/E/S/W) 4비트로 해당 방향 edge가 막혀있는지 저장.

### Passability Build

집 건축/편집 시 클라이언트에서 `buildPassability(house)` 호출 → HouseData에 포함하여 서버 저장.

- 층별(floor level) 별도 그리드
- 벽 세그먼트 순회: `variant !== 'open'`이면 해당 셀 edge 비트 set
- 양쪽 셀 모두 비트 set (안쪽 셀 + 바깥쪽 인접 셀)
- 저장 시 정적 구조 기준 (모든 문은 닫힌 상태로 취급)

### Stairwell 처리

1F stairwell을 1F/2F 두 grid에 모두 등록:

- **1F grid**: entry(low) 랜딩만 side wall skip, exit(high) 랜딩 포함하여 측면+끝 blocked
- **2F grid**: exit(high) 랜딩만 side wall skip, entry(low) 랜딩 포함하여 측면+끝 blocked
- 각 층에서 열리는 랜딩만 side wall 없음, 반대쪽 랜딩은 측면까지 완전 차단

계단 방향: `sizeZ >= sizeX`이면 Z축(entry=north, exit=south), 아니면 X축(entry=west, exit=east)

### Runtime

- 집 로드 시 저장된 passability 그리드를 직접 사용 (없으면 fallback으로 벽 데이터에서 계산)
- 열린 문 상태 overlay: passability cells 배열에서 door segment의 edge 비트 직접 clear
- `toggleDoor`/`handleDoorToggled` 시 해당 edge 비트만 O(1) flip

### Movement Check

`isMovementBlocked(fromX, fromZ, toX, toZ, y)`:

1. house AABB fast rejection
2. Y로 해당 floor grid 매칭
3. world → house local → grid cell 좌표 변환
4. X축/Z축 각각 셀 edge 교차 검사
5. `WALL_HALF_THICKNESS(0.3m)` proximity buffer — 벽에서 0.3m 거리에서 정지

## Rendering

### Front Wall / Roof Hiding

오쏘그래픽 카메라 (pitch 45°, yaw -45°) → 카메라 방향 = (-X, -Y, -Z).

- **앞벽** = 남쪽벽(normal -Z) + 서쪽벽(normal -X) — 카메라 각도가 고정이므로 항상 동일
- **숨길 대상** = 앞벽 + 지붕

집 단위로 두 개의 THREE.Group 분리:

| Group | 포함 메쉬 | 플레이어 inside 시 |
|-------|----------|-------------------|
| `frontGroup` | 남쪽벽, 서쪽벽, 지붕 | Y를 OFFSCREEN_Y로 이동 |
| `backGroup` | 북쪽벽, 동쪽벽, 바닥 | `visible = true` (항상) |

멀티패스 렌더링(refraction/reflection) 시에는 모든 벽 visible 유지.

### Mesh Construction

- 벽/바닥/지붕/계단: `house-geometry.ts`에서 프로시저럴 생성
- 방별 geometry를 집 단위로 merged geometry 생성 (draw call 최소화)
- 문짝만 별도 Mesh → `THREE.Group` 힌지 피벗으로 Y축 회전 애니메이션

### Materials

기존 material pool 패턴 재활용:

- 텍스쳐 카탈로그: stone, brick, wood, marble 등 → 인덱스로 참조
- WebGPU 제약: 텍스쳐별 개별 material 인스턴스 필요 (파이프라인은 공유)
- TSL `MeshStandardNodeMaterial` + `texture()` uniform 노드

### 2층 처리

- `floor_level: 0` = 지상, `floor_level: 1` = 2층 (y = wall_height + FLOOR_THICKNESS)
- 2층 방 아래 1층 방 존재 시 → 1층 지붕 메쉬 생략 (2층 바닥이 대체)
- 계단: `room_type: Stairwell` — 별도 geometry, 랜딩+계단 스텝 메쉬
- 2층 inside 시: 1층+2층 앞벽 모두 숨김

## Door Interaction

1. E키 → 플레이어 근처(2m) 문 탐색 (`findNearestDoor`)
2. 서버에 `ToggleDoor` 전송 (낙관적 토글 없음)
3. 서버가 토글 후 **모든 플레이어(요청자 포함)**에게 `DoorToggled` 브로드캐스트 — 서버 권위적 상태
4. 클라이언트: `handleDoorToggled`로 서버 상태 적용 + passability edge 비트 업데이트
5. 문짝 애니메이션: 게임 루프에서 힌지 피벗 Y축 회전 lerp (0=닫힘, -π/2=열림)

## Network Protocol

### ClientMessage

```rust
ToggleDoor { house_id, room_index, wall_dir, segment_index }
```

### ServerMessage

```rust
HouseSpawned { house: HouseData },
HouseUpdated { house: HouseData },
HouseRemoved { house_id: String },
HousesInArea { houses: Vec<HouseData> },  // 청크 진입 시 전송
DoorToggled { house_id, room_index, wall_dir, segment_index, is_open }
```

## Server Storage

- 파일 기반: `data/housing/r{cx}_{cz}/{house_id}.json`
- REST 엔드포인트:
  - `GET /api/housing/area/{cx}/{cz}` — 청크 내 모든 집
  - `POST /api/housing` — 생성 (ID 서버 할당)
  - `PUT /api/housing/{id}` — 수정
  - `DELETE /api/housing/{id}` — 삭제
- 서버 검증: 인접 벽 유효성, 겹침 검사, 소유자 권한, 2층 floor support

## File Structure

### Key Files

| Path | Description |
|------|-------------|
| `shared/src/housing.rs` | HouseData, RoomData, WallConfig, PassabilityGrid 등 공유 타입 |
| `client/src/lib/types/housing.ts` | 클라이언트 타입 미러 |
| `client/src/lib/managers/housingManager.ts` | 집 로딩/캐싱, passability, 문 토글, 벽 충돌 |
| `client/src/lib/utils/house-geometry.ts` | 프로시저럴 geometry 생성, merged geometry 조립 |
| `client/src/lib/components/game-scene/GameSceneHousingLayer.svelte` | 하우징 렌더 레이어 |
| `client/src/lib/components/map-editor/HousingEditorCursor.svelte` | 건축 에디터 |
| `client/src/lib/components/map-editor/HousingEditorPanel.svelte` | 건축 UI 패널 |
| `server/src/housing/mod.rs` | 하우징 게임 로직 + 검증 |
| `server/src/housing/routes.rs` | REST 엔드포인트 |

## Implementation Phases

### Phase 1: Static House Rendering (MVP) ✅

### Phase 2: Server Integration ✅

### Phase 3: Building UI ✅

### Phase 4: Second Floor + Stairs ✅

### Phase 5: Optimization ✅

Merged geometry per house, draw call 최소화.

### Phase 6: Wall Collision ✅

셀 기반 passability grid로 구현. 이전의 line-segment intersection 방식에서 전환.
- 각 셀 N/E/S/W 4비트 edge 마스크
- 집 건축/편집 시 계산 → 서버 저장
- 런타임에 door overlay + O(1) bit flip
- 계단 전용 처리 (측면 벽 + 랜딩 open)

### Phase 7: Doors & Windows Interaction ✅

문짝 힌지 애니메이션, E키 상호작용, 네트워크 동기화, passability 연동.

### Phase 8: Third Floor+ (Optional)

1. `floor_level` 최대 4층 (`floor_level` 0~3)
2. visibility 로직 N층 일반화: 플레이어 층 이상의 앞벽+지붕 숨기기
3. `hasFloorSupport` 검증 N층 확장
4. 에디터 층 선택 UI 확장

### Phase 9: Roof Connection

1. 인접 방의 지붕 교차선(valley line) 계산
2. 작은 방 지붕 끝단을 큰 방 경사면 높이에 맞춰 조정
3. Valley 부분에 이음새 삼각형 메쉬 추가
4. ridge direction이 다른 경우(직각 배치)의 교차선 처리

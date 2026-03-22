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
}
```

### RoomData

```rust
pub struct RoomData {
    pub local_x: i32,              // house origin 기준 오프셋 (미터)
    pub local_z: i32,
    pub size_x: u8,                // 3~6m
    pub size_z: u8,                // 3~6m
    pub floor_level: u8,           // 0 = 1층, 1 = 2층
    pub floor_texture: u8,         // 텍스쳐 카탈로그 인덱스
    pub roof_texture: u8,
    pub wall_height: f32,          // 기본 3m
    pub wall_north: WallConfig,
    pub wall_south: WallConfig,
    pub wall_east: WallConfig,
    pub wall_west: WallConfig,
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

- 방 크기: 3~6m (정해진 세트), 배치 그리드: 1m 단위 스냅
- 인접 방 공유 면: 양쪽 모두 `Open`이어야 함 (서버 검증)
- 2층 방의 `floor_level: 1`, y 오프셋 = wall_height

## Rendering

### Front Wall / Roof Hiding

오쏘그래픽 카메라 (pitch 45°, yaw -45°) → 카메라 방향 = (-X, -Y, -Z).

- **앞벽** = 남쪽벽(normal -Z) + 서쪽벽(normal -X) — 카메라 각도가 고정이므로 항상 동일
- **숨길 대상** = 앞벽 + 지붕

집 단위로 두 개의 THREE.Group 분리:

| Group | 포함 메쉬 | 플레이어 inside 시 |
|-------|----------|-------------------|
| `frontGroup` | 남쪽벽, 서쪽벽, 지붕 | `visible = false` |
| `backGroup` | 북쪽벽, 동쪽벽, 바닥 | `visible = true` (항상) |

멀티패스 렌더링(refraction/reflection) 시에는 모든 벽 visible 유지.

### Mesh Construction

- **벽**: Blender GLB (solid/door/window 변형 × 사이즈별)
  - `gltfCache.ts`로 로드, geometry clone
  - 사이즈: 3m, 4m, 5m, 6m × variant 3종 = 12개 GLB
- **바닥/지붕**: `PlaneGeometry` 프로시저럴 생성
- **방 하나** = 최대 6 메쉬 (벽 4 + 바닥 + 지붕)
- **집 하나 (4방)** ≈ 16~24 메쉬

### Materials

기존 material pool 패턴 재활용:

- 텍스쳐 카탈로그: stone, brick, wood, marble 등 → 인덱스로 참조
- WebGPU 제약: 텍스쳐별 개별 material 인스턴스 필요 (파이프라인은 공유)
- TSL `MeshStandardNodeMaterial` + `texture()` uniform 노드

### 2층 처리

- `floor_level: 0` = 지상, `floor_level: 1` = 2층 (y = wall_height)
- 2층 방 아래 1층 방 존재 시 → 1층 지붕 메쉬 생략 (2층 바닥이 대체)
- 계단: Phase 4에서 별도 variant 또는 오브젝트로 구현
- 2층 inside 시: 1층+2층 앞벽 모두 숨김

## Network Protocol

### ClientMessage 추가

```rust
PlaceHouse { house: HouseData },
ModifyRoom { house_id: String, room_index: u32, room: RoomData },
RemoveHouse { house_id: String },
```

### ServerMessage 추가

```rust
HouseSpawned { house: HouseData },
HouseUpdated { house: HouseData },
HouseRemoved { house_id: String },
HousesInArea { houses: Vec<HouseData> },  // 청크 진입 시 전송
```

## Server Storage

- 파일 기반: `data/housing/{chunk_x}_{chunk_z}/{house_id}.json`
- REST 엔드포인트:
  - `GET /api/housing/area/{cx}/{cz}` — 청크 내 모든 집
  - `PUT /api/housing/{id}` — 생성/수정
  - `DELETE /api/housing/{id}` — 삭제
- 서버 검증: 인접 벽 유효성, 겹침 검사, 소유자 권한, 건축 가능 영역

## File Structure

### New Files

| Path | Description |
|------|-------------|
| `shared/src/housing.rs` | HouseData, RoomData, WallConfig, WallVariant |
| `client/src/lib/types/housing.ts` | 클라이언트 타입 미러 |
| `client/src/lib/managers/housingManager.ts` | 집 로딩/캐싱, 플레이어-내부 감지 |
| `client/src/lib/utils/house-geometry.ts` | GLB 로드, geometry cache, 집 Group 조립 |
| `client/src/lib/components/game-scene/GameSceneHousingLayer.svelte` | 하우징 렌더 레이어 |
| `server/src/housing/mod.rs` | 하우징 게임 로직 |
| `server/src/housing/routes.rs` | REST 엔드포인트 |

### Files to Modify

| Path | Change |
|------|--------|
| `shared/src/lib.rs` | `pub mod housing`, ClientMessage/ServerMessage variants |
| `client/src/lib/components/GameScene.svelte` | HousingLayer 추가, update 루프 연결 |
| `server/src/main.rs` | housing routes 등록 |
| `server/src/connection.rs` | housing 메시지 라우팅 |
| `server/src/game_state/mod.rs` | houses HashMap 추가 |

### Reference Patterns

| Pattern | Source File |
|---------|-------------|
| Material pooling | `GameSceneTerrainLayer.svelte` |
| InstancedMesh + Group visibility | `GameSceneGrassLayer.svelte` |
| GLB 로딩/캐싱 | `gltfCache.ts` |
| REST + 파일 저장 | `terrain/routes.rs` |
| 멀티패스 visibility 토글 | `reflectionRenderManager.ts` |

## Implementation Phases

### Phase 1: Static House Rendering (MVP)

1. `shared/src/housing.rs` — 데이터 타입 정의
2. `shared/src/lib.rs` — housing 모듈 연결, 메시지 타입 추가
3. 벽 GLB 에셋 제작 (1개 사이즈, solid/door/window)
4. `house-geometry.ts` — HouseData → THREE.Group 조립
5. `GameSceneHousingLayer.svelte` — 하드코딩 테스트 집 렌더링
6. 앞벽/지붕 숨기기 (AABB 플레이어 감지)

### Phase 2: Server Integration

1. `server/src/housing/` — REST + 파일 저장
2. `housingManager.ts` — 청크 기반 로딩/캐싱
3. ClientMessage/ServerMessage 하우징 핸들링
4. 멀티플레이어 동기화

### Phase 3: Building UI

1. ✅ 건축 모드 진입/종료
2. ✅ 방 배치 프리뷰 + 그리드 스냅
3. ✅ 벽/바닥/지붕 텍스쳐 선택 (placeholder 색상)
4. ✅ 삭제 모드
5. ✅ 지형 평탄화 + 잔디 제거
6. ✅ 벽 variant 개별 선택 UI
7. ✅ 배치 유효성 검증 + 피드백
8. ✅ 서버 검증
9. ✅ 다중 방 편집
10. ✅ 텍스쳐 에셋 적용

### Phase 4: Second Floor + Stairs

1. ✅ 2층 방 배치 로직
2. ✅ 계단 메쉬/variant
3. ✅ 카메라 전환 (1층↔2층 뷰) — 높이 오프셋 + 벽 숨기기로 충분

### Phase 5: Optimization

1. ✅ InstancedMesh 배칭 → merged geometry per house
2. ✅ 프로파일링 + 드로우콜 최적화

### Phase 6: Wall Collision

1. 벽을 축 정렬 선분(plane segment)으로 표현 — 남/북벽은 Z 고정 X 범위, 동/서벽은 X 고정 Z 범위
2. 1m 세그먼트 단위로 Open 구간 제외, Door는 `is_open` 상태에 따라 제외/포함
3. `housingManager`에 `checkWallCollision(from, to)` 추가 — 이동 벡터와 선분 교차 검사
4. Window/Solid → 충돌, Open → 통과, Door → 문 상태에 따라
5. 플레이어 이동 로직에서 충돌 검사 호출

### Phase 7: Doors & Windows Interaction

1. `WallConfig`에 `is_open: bool` 추가 (문 전용, 기본 false)
2. 문 세그먼트의 상단 벽(문 위 0.8m)은 merged geometry에 유지, 문짝(`DOOR_WIDTH × DOOR_HEIGHT`)만 별도 Mesh로 분리 → 힌지 기준 회전 애니메이션
3. 상호작용 키(E) 또는 마우스 클릭으로 열기/닫기, 서버 동기화
4. 닫힌 문 = 충돌, 열린 문 = 통과 (Phase 6 충돌 시스템 연동)

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

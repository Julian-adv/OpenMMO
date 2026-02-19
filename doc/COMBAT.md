# Combat System

NetHack/D&D 스타일의 스탯 기반 전투 시스템. 모든 전투 계산은 서버에서 처리한다.

## 캐릭터 스탯 (Attributes)

6개의 기본 능력치. 범위는 3~18.

| 스탯 | 약자 | 설명 |
|------|------|------|
| Strength     | STR | 근접 공격력, 장비 제한 |
| Dexterity    | DEX | 명중, 회피, 원거리 공격 |
| Constitution | CON | HP 보너스, 체력 |
| Intelligence | INT | 마법 효과, 스킬 |
| Wisdom       | WIS | 회복력, 저항력 |
| Charisma     | CHA | NPC 반응, 거래 |

### 스탯 생성: 4d6 drop lowest

캐릭터 생성 시 각 능력치마다 주사위 4개(d6)를 굴려 가장 낮은 값을 제외한 3개를 합산한다.

```
예) 3, 5, 2, 4 → 2 제외 → 3+5+4 = 12
```

6개 스탯의 합계는 72로 리밸런싱된다. 합계가 72 미만이면 낮은 스탯을 올리고, 초과하면 높은 스탯을 낮춘다. 각 스탯은 3~18 범위를 벗어날 수 없다.

- 구현: [server/src/game/character_attributes.rs](../server/src/game/character_attributes.rs)

---

## HP 계산

레벨 1 기준: `max_hp = 클래스 기본 HP + 종족 보너스`

### 클래스 기본 HP

| 클래스 | HP |
|--------|----|
| Knight, Barbarian, Caveman, Valkyrie | 14 |
| Ranger, Samurai | 13 |
| Monk, Priest | 12 |
| Archaeologist, Healer | 11 |
| Rogue, Wizard | 10 |
| Tourist | 8 |

### 종족 보너스

| 종족 | 보너스 |
|------|--------|
| Dwarf | +4 |
| Human | +2 |
| Elf, Gnome, Orc | +1 |

**예시:** Human Knight = 14 + 2 = **16 HP**

- 구현: [server/src/game/character_hp.rs](../server/src/game/character_hp.rs)

---

## 전투 공식

### 히트 롤 (Hit Roll)

```
d20 굴림 > hitThreshold  →  명중
d20 굴림 ≤ hitThreshold  →  빗나감
```

- d20 범위: 1~20
- `hitThreshold`는 몬스터(또는 공격자)별로 정의
- 예: `hitThreshold = 10` → 11 이상이면 명중 (명중 확률 50%)

### 대미지 롤 (Damage Roll)

명중 시에만 굴린다.

```
대미지 = dice notation 파싱 후 합산
예) "2d6" → d6 두 번 굴려 합산 (2~12)
```

주사위 표기법: `{count}d{sides}` (예: `1d6`, `2d8`, `3d4`)

- 구현: [server/src/game/combat.rs](../server/src/game/combat.rs)

---

## Guard (GUARD)

NetHack의 AC를 반전시킨 방어 수치. **높을수록 방어력이 좋다.**

- 범위: 0 이상 (상한 없음)
- 기본값(무방비): 0
- 10이 기준점 (이 이상부터 XP 보너스가 가속)

| GUARD | 의미 |
|-------|------|
| 0~3 | 무방비 |
| 4~7 | 약한 방어 |
| 8~9 | 단단한 방어 |
| 10+ | 중장갑 이상 |

> NetHack AC와의 대응: `GUARD = 10 − AC`
> (NetHack AC 0 → GUARD 10, AC -5 → GUARD 15)

---

## 몬스터 스탯 정의

몬스터는 [data/monsters.json](../data/monsters.json)에 정의한다.

| 필드 | 타입 | 설명 |
|------|------|------|
| `health` | u32 | 최대 HP |
| `level` | u8 | 몬스터 레벨 (XP 계산에 사용) |
| `guard` | u8 | 방어 수치 (높을수록 강함, XP 보너스에 영향) |
| `hitThreshold` | u8 | 명중 판정 임계값 (d20 비교) |
| `damageRoll` | string | 대미지 주사위 (예: `"1d6"`) |
| `attackRange` | f32 | 근접 공격 가능 거리 |
| `chaseRange` | f32 | 플레이어 추적 시작 거리 |
| `attackCooldown` | u32 | 공격 간격 (밀리초) |

**현재 몬스터 예시 (SCP-939):**

```json
{
  "health": 10,
  "level": 3,
  "guard": 5,
  "hitThreshold": 10,
  "damageRoll": "1d6",
  "attackRange": 2,
  "chaseRange": 25,
  "attackCooldown": 1500
}
```

---

## 전투 흐름

### 플레이어 → 몬스터 공격

1. 클라이언트가 `PlayerAttack { monster_id }` 전송
2. 서버에서 히트 롤: `roll_attack(hitThreshold, damageRoll)`
3. 결과를 전체 클라이언트에 브로드캐스트 (`PlayerAttacked`)
4. 명중 시 몬스터 HP 차감
5. HP가 0이 되면 `MonsterDead` 브로드캐스트, 30초 후 제거

### 몬스터 → 플레이어 공격

1. 클라이언트(몬스터 owner)가 `MonsterAttack { monster_id, target_player_id }` 전송
2. 서버에서 히트 롤: `roll_attack(hitThreshold, damageRoll)`
3. 결과를 전체 클라이언트에 브로드캐스트 (`MonsterAttackedPlayer`)
4. 명중 시 플레이어 HP 차감
5. HP가 0이 되면 `PlayerDead` 브로드캐스트

### 리스폰

- 클라이언트가 `RequestRespawn` 전송
- 서버에서 HP 0 확인 후 최대 HP로 회복, 원점(0,0,0)으로 이동
- `PlayerRespawned { player }` 브로드캐스트

---

## 경험치 (XP) 시스템

### 몬스터 처치 XP 공식

```
xp = 1 + level²  +  guard_bonus
```

**guard_bonus:**

| GUARD | 보너스 |
|-------|--------|
| 0 ~ 7 | 없음 |
| 8 | +5 |
| 9 | +6 |
| 10 | +7 |
| 11 | +9 |
| 12 | +11 |
| 10 + i | 7 + 2i |

일반 공식 (GUARD ≥ 10): `guard_bonus = 7 + 2 × (guard − 10)`

**예시:**

| 몬스터 | level | GUARD | xp |
|--------|-------|-------|----|
| 약한 적 | 1 | 3 | 1 + 1 = **2** |
| 보통 적 | 3 | 5 | 1 + 9 = **10** |
| 강한 적 | 5 | 10 | 1 + 25 + 7 = **33** |
| 보스 | 8 | 13 | 1 + 64 + 13 = **78** |

### 레벨업 필요 XP

모든 레벨에 동일한 공식 적용: `XP(n) = 20 × 2^(n−2)` (n ≥ 2)

| 레벨 | 필요 누적 XP |
|------|-------------|
| 1 | 0 |
| 2 | 20 |
| 3 | 40 |
| 4 | 80 |
| 5 | 160 |
| 6 | 320 |
| 7 | 640 |
| 8 | 1,280 |
| 9 | 2,560 |
| 10 | 5,120 |
| 11 | 10,240 |
| 12 | 20,480 |
| 13 | 40,960 |
| 14 | 81,920 |
| 15 | 163,840 |
| 16 | 327,680 |
| 17 | 655,360 |
| 18 | 1,310,720 |
| 19 | 2,621,440 |
| 20 | 5,242,880 |
| 21 | 10,485,760 |
| 22 | 20,971,520 |
| 23 | 41,943,040 |
| 24 | 83,886,080 |
| 25 | 167,772,160 |
| 26 | 335,544,320 |
| 27 | 671,088,640 |
| 28 | 1,342,177,280 |
| 29 | 2,684,354,560 |
| 30 | 5,368,709,120 |

---

## 네트워크 메시지

```
Client → Server:
  PlayerAttack { monster_id }
  MonsterAttack { monster_id, target_player_id }
  RequestRespawn

Server → Client (broadcast):
  PlayerAttacked   { player_id, monster_id, hit, roll, damage }
  MonsterAttackedPlayer { monster_id, player_id, hit, roll, damage }
  MonsterDead      { monster_id }
  PlayerDead       { player_id }
  PlayerRespawned  { player }
```

- 구현: [shared/src/lib.rs](../shared/src/lib.rs)

---

## 몬스터 AI 상태

클라이언트가 몬스터 AI를 처리하고, 공격 판정은 서버에 요청한다.

| 상태 | 설명 |
|------|------|
| `idle` | 대기 (30% 확률로 랜덤 이동) |
| `walk` | 이동 중 |
| `run` | 플레이어 추적 중 (chaseRange 이내) |
| `attack` | 공격 중 (attackRange 이내) |
| `hit` | 피격 경직 (~800ms) |
| `dead` | 사망 |

- 구현: [client/src/lib/managers/monsterManager.ts](../client/src/lib/managers/monsterManager.ts)

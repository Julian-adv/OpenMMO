# Combat System

NetHack/D&D 스타일의 스탯 기반 전투 시스템. 모든 전투 계산은 서버에서 처리한다.

## 기본 공격 주기

웹 클라이언트는 `StartPlayerAttack`으로 타깃과 요청 ID를 보낸다. 이동 요청은 서버가
공격도 중단하므로 별도 중단 패킷을 보내지 않는다. 이동 없이 공격만 취소할 때는
`StopPlayerAttack`을 보낸다. 서버가 공격을 반복하며 매번 거리·벽·생존·탄약을 검증한다.
간격은 무기의 `attackIntervalMs`(단검 1200ms)이고, 없으면 `player_attack_interval`의 1533ms다. 여기에 배고픔·디버프의 공격 배율을 적용한다.
반복 클릭, 타깃 변경, 중단 후 재시작은 마지막 공격의 쿨다운을 초기화하지 않는다.
서버는 플레이어별 다음 공격 시각을 예약하고, 가장 이른 예약까지 대기한다.
20ms 전체 순회는 사용하지 않으며, 이동·행동 전환과 공격 속도 변화는 해당 예약만 갱신한다.

클라이언트는 `PlayerAttacked`와 `DaggerDoubleSlashStarted`를 받아 공격 애니메이션을
재생한다. 추적 이동은 클라이언트가 유지하되, 이동을 시작하면 서버 공격을 중단하고
사거리 도착 시 다시 시작한다. 서버에서도 이동·사망·접속 종료·위치 이동 시 중단한다.
공격 검증 실패는 한 번만 보고하고 `PlayerAttackStopped`로 해당 요청을 종료한다.
단검 스킬은 다음 공격 주기에 실행되며, 서버 지연 후 밀린 공격을 몰아서 처리하지 않는다.

감정 표현·오브젝트 상호작용·청진·낚시·줍기 요청은 서버에서 자동 공격과 예약된 단검
타격을 중단한다. 반대로 공격을 실행하면 기존 자세와 낚시·연주를 종료한다.
청진·낚시의 타깃 선택 화면은 서버 요청 전의 로컬 UI이므로, 이 화면에서 공격을
멈추려면 `StopPlayerAttack`을 보낸다.

에이전트 클라이언트의 단발 `PlayerAttack`도 같은 서버 쿨다운을 적용한다.

## 단검 스킬: Double Slash

다음 기본 공격을 두 번의 독립적인 타격으로 대체한다. 장착 조건, 피해·애니메이션 타이밍과
타격 취소 규칙은 [Double Slash](abilities/DOUBLE_SLASH.md)에 정리한다. 다른 어빌리티는
[어빌리티 문서 목록](abilities/README.md)을 참고한다.

마나 도입과 전투 기술·마법의 구분은 [마나·스킬·마법 설계 초안](MANA_SKILLS_MAGIC.md)에 정리한다. WIS에 따른 최대 MP·레벨당 증가량·자연 회복과 Guardian Ward의 MP 2 비용은 구현되어 있다. INT 기반 공격 마법·마법 Guard는 후속 설계다.

## 캐릭터 스탯 (Attributes)

6개의 기본 능력치. 범위는 3~18.

| 스탯 | 약자 | 쓰임 |
|------|------|------|
| Strength     | STR | 단검을 뺀 근접 공격의 명중·피해 보정 |
| Dexterity    | DEX | 생성 시 GUARD, 활(`rangedAbility dex`)과 단검의 명중·피해 보정 |
| Constitution | CON | 최대 HP(레벨 1·레벨업), HP 재생량 |
| Intelligence | INT | 아직 쓰지 않는다 (INT 마법은 후속 설계) |
| Wisdom       | WIS | 최대 MP, MP 회복량 |
| Charisma     | CHA | 상인·주민과의 흥정 폭 |

### 스탯 생성

1. 클래스를 먼저 선택한다.
2. 각 능력치마다 4d6을 굴려 가장 낮은 주사위를 뺀 3개를 합산한다 (예: 3, 5, 2, 4 → 12).
3. 클래스 보정을 더하고 3~18로 자른다.
4. 합계를 72로 맞춘다. 모자라면 가장 낮은 스탯을 1씩 올리고, 넘치면 가장 높은 스탯을 1씩 내린다. 3~18 범위는 유지한다.
5. 최종 DEX로 GUARD를 계산한다.

- 구현: [server/src/game/character_attributes.rs](../server/src/game/character_attributes.rs)

### 클래스별 스탯 보정

보정 합은 클래스마다 0이다. 구현: `CharacterClass::stat_adjustments` ([shared/src/character.rs](../shared/src/character.rs)).

| 클래스 | STR | DEX | CON | INT | WIS | CHA |
|--------|-----|-----|-----|-----|-----|-----|
| Barbarian (M) | +3 | 0 | +2 | -2 | -2 | -1 |
| Barbarian (F) | +2 | +1 | +1 | -2 | -1 | -1 |
| Caveman (M) | +2 | 0 | +2 | -2 | 0 | -2 |
| Caveman (F) | +1 | +1 | +1 | -2 | +1 | -2 |
| Knight (M) | +1 | -1 | +1 | -1 | 0 | 0 |
| Knight (F) | 0 | 0 | 0 | -1 | +1 | 0 |
| Valkyrie | +2 | +1 | +1 | -1 | -2 | -1 |
| Ranger | +1 | +2 | 0 | -1 | 0 | -2 |
| Samurai | +1 | 0 | +2 | -1 | 0 | -2 |
| Monk | -1 | +2 | 0 | -1 | +2 | -2 |
| Priest | -1 | -1 | +1 | -1 | +3 | -1 |
| Archaeologist | -1 | +1 | 0 | +2 | +1 | -3 |
| Healer | -2 | -1 | +1 | +1 | +2 | -1 |
| Rogue | -1 | +3 | 0 | +1 | -1 | -2 |
| Wizard | -2 | 0 | -1 | +3 | +2 | -2 |
| Tourist | -1 | 0 | -1 | +1 | -1 | +2 |
| Bard | -2 | +2 | -1 | 0 | -1 | +2 |

**NPC 전용 클래스 (플레이어 선택 불가)**

| 클래스 | STR | DEX | CON | INT | WIS | CHA |
|--------|-----|-----|-----|-----|-----|-----|
| Merchant | -2 | 0 | -1 | +1 | -1 | +3 |
| Guard | +2 | 0 | +2 | -2 | -1 | -1 |
| Maid | -2 | +1 | -1 | -1 | +1 | +2 |

### 캐릭터 Guard 계산 (생성 시)

```
GUARD = clamp(10 + (DEX - 10) / 2, 1, 20)
```

- 나눗셈은 0 쪽으로 버린다. DEX 3~18에서 GUARD는 7~14다.

예시:

| DEX | dex_mod | GUARD |
|-----|---------|-------|
| 8   | -1      | 9     |
| 10  | 0       | 10    |
| 14  | +2      | 12    |
| 18  | +4      | 14    |

---

## HP 계산

레벨 1 기준: `max_hp = HD_max + con_mod + 종족 보너스`

```
con_mod = (CON - 10) / 2   // 0 쪽으로 버림. 이 문서의 con_mod는 모두 이 값이다
```

### 클래스 Hit Die (HD)

| 클래스 | HD |
|--------|----|
| Knight, Barbarian, Caveman, Valkyrie, Guard | d10 |
| Ranger, Samurai, Monk, Priest | d8 |
| Archaeologist, Healer, Rogue, Wizard, Bard | d6 |
| Tourist, Merchant, Maid | d4 |

### 종족 보너스

| 종족 | 보너스 |
|------|--------|
| Dwarf | +4 |
| Human | +2 |
| Elf, Gnome, Orc | +1 |

**레벨 1 예시:** Human Knight, CON 14 → `10 + 2 + 2 = 14 HP`

### 레벨업 시 Max HP 증가

레벨 2부터 HD를 굴리되 HD의 절반을 보장하고 `con_mod`를 더한다.

```
hp_delta = max(dHD, HD / 2) + con_mod
```

예: d10에서 3이 나오면 `5 + con_mod`.

- 구현: [server/src/game/character_hp.rs](../server/src/game/character_hp.rs) (`roll_level_hp_delta`)

---

## HP 재생 (Regeneration)

서버 시간 틱(8초) 두 번마다, 즉 16초마다 회복한다.

```
regeneration_amount = max(1, 1 + floor(Level / 5) + con_mod)
```

예: 레벨 6, CON 12 → `1 + 1 + 1 = 3 HP`.

회복 조건:

- 살아 있고 `health < max_health`여야 한다.
- 마지막 공격·피격으로부터 10초(`OUT_OF_COMBAT_MS`)가 지나야 한다.
- 쇠약(Weak)이거나 `blocksRegen` 디버프(식중독, 출혈)가 있으면 멈춘다. 배고픔(Hungry)이면 두 번에 한 번만 회복한다 ([HUNGER.md](HUNGER.md), [DEBUFF.md](DEBUFF.md)).
- 유효한 침대에서 16초 이상 쉬면 HP·MP 자연 회복량이 각각 2배가 된다.
  일어나기·이동·공격·시전·아이템 사용·피격·사망·침대 제거·접속 종료 시 수면은 끝난다.
  음식·물약의 회복량에는 적용하지 않는다.

- 구현: [server/src/game_state/combat.rs](../server/src/game_state/combat.rs) (`tick_regeneration`)

---

## 전투 공식

### 치명타

플레이어는 명중한 공격에 별도로 **5%** 확률의 치명타를 판정한다. 치명타는 능력치·무기 강화·탄약을 포함한 피해를 **2배**로 만든다. 근접·원거리 기본 공격과 Double Slash의 각 타격에 적용하며, 폭발 d20의 자연 20과는 독립적이다. 균열의 반지는 확률을 **+5%p** 높여 총 10%로 만들고, 두 개를 착용해도 효과는 중복되지 않는다. 서버가 현재 확률을 상태창에 보낸다. [규칙·보상 가치·강화 비교](CRITICAL_HITS.md).

### 히트 롤 (Hit Roll)

```
굴림 합계 + attack_bonus > target_guard  →  명중
굴림 합계 + attack_bonus ≤ target_guard  →  빗나감
```

- 플레이어 `attack_bonus = level / 2`(내림) + STR modifier + 무기 인챈트 + 디버프 `hitMod` 합(취기, [DEBUFF.md](DEBUFF.md))
  — 무기가 `rangedAbility`를 선언하면 STR 대신 그 능력치를 쓴다(아래 "원거리 전투").
  단검(`weaponType dagger`)은 STR 대신 DEX를 쓴다.
  능력치 modifier는 `floor((score − 10) / 2)`로, `con_mod`와 달리 음수 쪽으로 내린다(STR 9 → −1).
- 몬스터 `attack_bonus = level` + 장소 명중 보너스. 플레이어보다 가파르다. 플레이어 guard는
  레벨이 아니라 장비로 오르기 때문이다. monsters.csv의 `attackBonus`가 있으면 그 값이
  우선하고, 장소 레벨 보정은 그 값에 올라간 레벨만큼을 더한다.

#### 굴림 합계: 폭발 주사위 (Exploding d20)

d20을 굴린다. **20이 나오면 한 번 더 굴려서 더한다.** 그 굴림도 20이면 또
더한다 (최대 5회까지 — 합계 120이면 어떤 guard보다 높으므로 실질 무제한).

```
굴림이 13     → 합계 13
굴림이 20, 10 → 합계 30
굴림이 20, 20, 4 → 합계 44
```

합계 30이 나왔고 공격보너스가 +2라면 32가 되고, 목표 guard가 31 이하면
명중이다.

**왜 이렇게 하나.** "자연 20이면 무조건 명중"으로 두면 아무리 두꺼운 갑옷을
입어도 명중률이 정확히 5%에서 멈춘다. 그 5%는 밸런스 판단이 아니라 주사위
면이 20개라서 생긴 숫자일 뿐이다 — 코볼트가 판금 갑옷 플레이어를 스무 번에
한 번 때리게 된다. 폭발 주사위는 **모자란 만큼 확률이 줄어들되 0은 되지
않게** 한다. 자연 1은 특별 취급하지 않는다.

(폭발이 아예 없으면 guard 22 이상은 공격보너스 +2 이하 몬스터에게 수학적으로
무적이 된다.)

**확률.** 합계가 목표치 `t` 이상 나올 확률은 20점 구간마다 나눠서 본다.

- `t ≤ 20`: 그냥 d20 한 번이므로 `P(t) = (21 − t) / 20`
  (예: `P(15) = 6/20 = 30%` — 15,16,…,20 여섯 눈)
- `t > 20`: 첫 굴림이 반드시 20이어야 하고(1/20) 나머지를 같은 방식으로 다시
  본다 → `P(t) = P(t − 20) / 20`

즉 20점을 넘길 때마다 확률이 1/20로 꺾인다. 예를 들어 합계 30 이상은

```
P(30) = P(10) / 20 = (11/20) / 20 = 11/400 = 2.75%
```

`(21 − 30)/20`처럼 t가 20을 넘은 채로 첫 번째 식에 넣으면 안 된다 — 20을
넘는 순간 두 번째 식으로 넘어간다.

명중률로 정리하면(장소 보너스 없음):

| 몬스터 (보너스) | G15 | G21 | G24 | G30 | G40 | G50 |
|---|---:|---:|---:|---:|---:|---:|
| Kobold (+1) | 30% | 5% | 4.25% | 2.75% | 0.25% | 0.14% |
| Orc (+4) | 45% | 15% | 5% | 3.5% | 1% | 0.18% |
| Ogre (+8) | 65% | 35% | 20% | 4.5% | 2% | 0.23% |
| Stone Golem (+10) | 75% | 45% | 30% | 5% | 2.5% | 0.25% |

guard 40짜리 플레이어를 코볼트가 한 번 맞히려면 평균 400회, 공격 쿨다운
1.9초 기준 13분을 때려야 한다. 무적은 아니되 사실상 무의미한 수준이다.

### 장소 명중 보너스 (Place Attack Bonus)

플레이어 guard는 레벨이 아니라 획득한 방어구와 인챈트로 오르고, 상한이
없다. 프로드 실측(2026-08-20)에서 Lv13~16 활동 사냥꾼의 guard 중앙값은
40, 최고는 67이었고 그중 인챈트가 평균 14를 차지했다. 일반 몬스터의 명중
보너스는 레벨이므로 25에서 멈춘다 — 폭 28짜리 분포를 20칸짜리 d20
창으로 덮을 수 없다. 그래서 **난이도는 무엇을 만나느냐가 아니라 어디서
사냥하느냐**가 정한다.

```
던전:  보너스 = max(0, 깊이 − DUNGEON_SAFE_DEPTH(5)) × DUNGEON_DEPTH_ATTACK_BONUS(3)
표면:  보너스 = min(60, max(0, 마을 거리 − SURFACE_SAFE_METERS(300m)) / SURFACE_METERS_PER_ATTACK_BONUS(20m))
```

- 표면 상한 60은 가장 깊은 층(`MAX_FLOORS` 25)의 보너스다. 300m까지 0, 560m에서 +13,
  800m에서 +25, 1500m부터 +60.
- 구현: [`place_attack_bonus`](../server/src/game/combat.rs), 적용은
  [game_state/combat.rs](../server/src/game_state/combat.rs)의 몬스터 공격
- 이 보너스는 명중만 올린다. 레벨·HP·피해·XP는 아래 장소 레벨 보정이 올린다.
- **안전 구간이 초보 보호다.** 티어1 던전(Old Crypt, 5층)과 마을 반경
  300m는 보너스 0이라 기존 밸런스 그대로다. 설계 앵커: 상점 장비를 갖춘
  guard 15가 Old Crypt 5층에서 orc(lvl 4)에게 45% — 대상 guard를 보는
  하한·클램프 없이, 안전 구간 상수 두 개로 끝난다.

도입 당시 실이용자 킬 가중 명중률(프로드 실측 기준):

| 장소 | 보너스 | 도입 전 | 도입 후 |
|---|---:|---:|---:|
| Old Crypt 1~5층 | 0 | 4~19% | 변화 없음 |
| Orc Warrens 8층 | 9 | 4% | 14% |
| Orc Warrens 10층 | 15 | 8% | 45% |
| Ogre Stronghold 10층 | 15 | 4% | 23% |
| Ogre Stronghold 15층 | 30 | 2% | 49% |

층당 3은 진행 사다리에 맞춘 값이다(hobgoblin 깊이 보정 기준): 상자
졸업(guard 21)은 6~7층에서 40~55%, 인챈트를 약간 얹으면(26) 8~9층에서
50~70%, 인챈트 확정 구간을 채우면(39) 10층도 20~30%로 버틴다. 8층부터는
인챈트가 입장료가 되도록 — 연마유 골드 싱크가 진행 화폐다.

인챈트를 두른 guard 61은 근거리 표면에서 여전히 잘 안 맞는데, 그쪽은
명중이 아니라 보상으로 다룬다(아래 거리 게이트).

### 앰비언트 스폰 거리 게이트

표면 몬스터 종류도 플레이어 레벨이 아니라 **마을로부터의 거리**가 정한다.
레벨 1칸당 `AMBIENT_SPAWN_METERS_PER_LEVEL`(70m)씩 밀린다.

| 몬스터 | 레벨 | 해금 거리 |
|---|---:|---:|
| Kobold | 1 | 0m |
| Goblin | 2 | 70m |
| SCP-939 | 3 | 140m |
| Orc, Orc (F) | 4 | 210m |
| Hobgoblin | 5 | 280m |
| Gnoll | 6 | 350m |
| Bugbear | 7 | 420m |
| Ogre | 8 | 490m |
| Troll | 9 | 560m |
| Stone Golem | 10 | 630m |
| Lizardfolk | 13 | 840m |
| Cyclops | 15 | 980m |

- 구현: [`min_ambient_town_distance`](../server/src/game_state/monster.rs). 거리는
  플레이어가 아니라 **몬스터가 놓이는 지점**에서 잰다
  ([`ambient_spawn.rs`](../server/src/game_state/ambient_spawn.rs))
- 해금된 종류 중에서 균등하게 뽑으므로, 멀리 가도 저레벨 종류가 섞인다.
- 마을 근처는 저레벨 몬스터만 나오므로 XP도 드랍 등급도 낮다. 고레벨
  플레이어를 명중률로 쫓아내는 게 아니라 **보상 구배로 밀어낸다** — 실측상
  표면 킬의 94%가 마을 반경 100~400m 안에서 일어나고 있었다.
- 980m 밖에서는 종류가 더 늘지 않는다. 그 너머의 보상은 아래 레벨 보정이 올린다.

### 장소 레벨 보정

던전은 5층부터 2층마다 몬스터 레벨이 +1 오르고 25에서 멈춘다
(`monster_level_for_depth`). 표면은 마을 거리를 **장소 명중 보너스가 같은
던전 깊이**로 바꿔 같은 함수에 넣는다. 명중 보너스가 60에서 멈추므로
1500m 밖은 명중과 레벨 모두 25층과 같다.

```
환산 깊이 = DUNGEON_SAFE_DEPTH(5) + 표면 보너스 / DUNGEON_DEPTH_ATTACK_BONUS(3)
레벨     = monster_level_for_depth(정의 레벨, 환산 깊이)
```

- 구현: [`surface_equivalent_depth`](../server/src/game/combat.rs),
  [`level_at_depth`](../server/src/game_state/monster.rs)(던전 슬롯과 공용). 레벨은 스폰 지점에서
  정해져 `level_override`로 실린다.
- 360m 전까지는 그대로, 그 뒤 120m마다 +1. 980m의 Cyclops는 21(XP 462),
  1440m부터 25(XP 646). 코볼트는 25층과 같이 11에서 멈춘다.
- 오른 레벨은 던전처럼 HP·명중·기본 피해 주사위·XP·드랍 레벨 판정에 모두 반영된다.

### 대미지 롤 (Damage Roll)

명중 시에만 굴린다. 주사위 표기법은 `{count}d{sides}`다.

```
플레이어:  대미지 = max(1, 무기 주사위 + 탄약 주사위 + 능력치 modifier + 무기 인챈트)
몬스터:    대미지 = max(1, 몬스터 주사위 + 들고 있는 무기 주사위)
```

- 몬스터 주사위는 `damageRoll`이 없으면 레벨로 정한다(1d4 → … → 3d6, `monster_damage_roll_for_level`).
- 플레이어의 치명타는 이 결과를 2배로 만든다.
- 구현: [server/src/game/combat.rs](../server/src/game/combat.rs) (`roll_attack_with_accuracy`)

---

## 원거리 전투 (Ranged Combat)

무기가 `range`를 선언하면 그 거리까지 **히트스캔**으로 때린다. 화살 같은
투사체는 날아가지 않고, 공격 시점에 즉시 판정한다.

### 무기 데이터 (data-src/items.csv)

| 필드 | 타입 | 설명 |
|------|------|------|
| `range` | f32? | 리치(m). 비우면 근접 리치 2m. 단검은 1.5m |
| `attackIntervalMs` | u64? | 공격 간격(ms). 비우면 `player_attack_interval`(1533ms). 단검은 1200ms |
| `rangedAbility` | string? | 명중·대미지 보너스를 읽을 능력치 (`dex`, `int`, ...). 비우면 STR, 단검은 DEX |
| `hands` | u8? | 무기가 차지하는 손. 비우면 1, `2`면 오프핸드를 봉인한다 |

활(`bow`): `main_hand`, `hands 2`, `range 10`, `rangedAbility dex`, `1d1`(피해는 화살이 정한다, 아래 "탄약").

### 사거리 게이트

`validate_player_attack`이 `무기 range(없으면 근접 리치) + 0.5m`로 판정한다. 상수는
`shared/src/reach.rs`에 있다. 벽 검사
(`ranged_attack_line_blocked`)는 울타리를 통과시키지만, 벽·닫힌 문·가구로 막힌
칸 너머로는 쏠 수 없다. 울타리는 이동과 근접 공격을 계속 막는다. 사거리 밖 공격은 종전대로
`PLAYER_ATTACK_PROVOKE_RANGE_METERS`(10m) 안에서 어그로만 끈다.

클라이언트와 agent-client도 같은 `data/items.json` 열을 읽는다(`weaponRangeMeters`,
`attack_approach_range`). 리치에 들어오면 멈춰 공격하고, 벗어나면 다시 추격한다.
서버의 0.5m 여유는 클라이언트가 보는 몬스터 위치의 지연만 흡수한다.
근접은 2m(서버 2.5m), 단검은 1.5m(서버 2.0m)다.
추격은 사거리 끝에서 멈춰 서서 쏜다(카이팅 없음).

### 능력치 기반 보너스

`rangedAbility`가 있으면 히트 롤의 attack bonus와 대미지 보너스를 STR 대신
그 능력치의 modifier로 계산한다. 그 외 판정 경로는 근접과 완전히 같다.

### 타이밍

`data-src/player_anim_timing.csv`의 값은 전부 slash1 기준이라 활에 그대로 쓸 수
없다. 활은 클립 후반에야 화살이 떠나기 때문이다(`bow_shoot` 1.033초 중 활을 든
팔의 반동이 0.767–0.800초에 시작).

| 항목 | 근접 | 원거리 |
|------|------|--------|
| 화살이 떠나는 순간 | — | `player_ranged_impact` 780 |
| 명중 순간 (피해 텍스트·몬스터 반응·전리품 낙하) | `player_attack_impact` 540 | 780 + 비행 시간 |
| 피해 숫자 | `player_attack_damage_text` 750 | 명중 + 210 (근접의 540→750 간격을 그대로) |

원거리는 화살이 **보이므로** 떠나는 순간과 닿는 순간이 갈라진다. 비행은 고정
속도 30 m/s라 거리에 비례한다(2 m면 약 67 ms, 10 m면 333 ms) — 고정 시간으로
하면 근거리에서 화살이 기어간다. 서버의 전리품 지연만은 거리를 받지 않고
`player_ranged_flight`(333, 10 m 기준)를 더한 고정값을 쓴다. 가까운 사격에서
전리품이 화살보다 최대 그만큼 늦게 떨어지지만 보이지 않는 차이이고, 시각 효과
하나 때문에 `spawn_kill_loot_after_impact`에 거리를 흘려보내지 않는다.

공격 간격(`player_attack_interval` 1533)은 원거리도 그대로 쓴다 — 활의 DPS를
근접과 같은 타이밍에 묶어 둔다. 클립이 간격보다 짧아 끝 자세로 잠깐 머무는 것은
slash4(1.3초)도 마찬가지다.

다른 플레이어가 쏜 화살도 같은 타이밍으로 처리한다 — `PlayerMainHandChanged`가
남의 주손 장비를 브로드캐스트하고 클라이언트가 그것으로 남의 무기를 그리므로,
원거리인지 아닌지도 그 값으로 안다.

소리는 draw(`player_ranged_draw` 460 ms에 시작해 루스 직후까지 이어진다) + release(화살이 떠나는 순간)뿐이고, **원거리 명중은 재질
타격음을 내지 않는다.** 재질 표는 "이 무기가 저 대상을 때리면 어떤 소리인가"를
답하는데 활의 `wood`는 활 몸이지 화살이 아니라, 규칙이 없어 기본값인
칼-가죽 소리로 떨어져 활시위 소리를 덮어 버렸다. 화살 명중음이 생기면
`material-impact-sounds.json`에 규칙을 넣고 되살린다.

### 탄약

활은 쏠 때마다 화살을 한 발 쓴다. `items.csv`의 `ammoKind`가 무기와 탄약
양쪽에 같은 종류 이름을 적어 짝을 짓는다 — 활과 두 화살 모두 `arrow`. 무기에
`ammoKind`가 없으면 쏘는 데 아무것도 들지 않으므로 근접 무기는 손댈 필요가
없다.

| | 주사위 | 활과 합쳐 | 대응 | 가격 |
|---|---|---|---|---|
| bow | 1d1 | — | — | 4000 |
| iron_arrow | 1d6 | 4.5 | iron_sword 1d8 | 3 |
| steel_arrow | 1d8 | 5.5 | steel_longsword 1d10 | 10 |

피해는 **활 주사위 + 화살 주사위**다(`roll_attack_with_accuracy`의 `extra_damage_roll`,
몬스터가 자기 주사위에 무기 주사위를 더하는 것과 같은 경로). 활의 1d1은 일부러
토큰이다 — 아프게 하는 것은 화살이고, 활은 사거리와 능력치를 정한다. 능력치
보정과 활의 인챈트는 종전대로 더해진다. **화살은 인챈트할 수 없다** — 장비
슬롯에 들어가지 않으므로 두루마리가 닿지 않는다.

화살은 겹쳐 쌓이고, 겹치는 물건은 장비 슬롯에 들어갈 수 없다(슬롯은 한 개만
저장하므로 나머지가 사라진다 — `item_defs.rs`의 부팅 단언). 그래서 화살은
가방에 남고 `PlayerInventory.active_ammo`가 어느 더미에서 뽑을지를 가리킨다.

- 활을 착용할 때 **아직 유효한 선택이 없으면** 가장 센 화살이 자동으로 걸린다.
- 더 좋은 화살을 주워도 **직접 고른 선택을 덮지 않는다** — 싼 화살을 쓰겠다는
  결정이 그 자리에서 무효가 되면 고를 수 있다는 것이 의미가 없다.
- 고른 더미가 떨어지면 다음으로 센 것으로 **자동으로 내려간다**. 가방에 화살이
  남았는데 "화살 없음"이 뜨면 그건 버그로 보인다.
- 활을 벗어도 선택은 남는다. 순위는 주사위의 기댓값으로 매긴다 — 따로 등급
  칸을 두면 실제 피해와 어긋날 수 있다.

가방이 비면 `OutOfAmmo`로 거절하고, 다른 검증 실패와 같이 서버가 자동 공격을
끝낸다(`PlayerAttackStopped`). 거절된 공격은 화살을 쓰지 않는다 — 소비는 모든
게이트를 지나 쿨다운을 확보한 뒤에 일어난다.

### 화살 (client/src/lib/stores/arrowStore.ts)

화살은 **순수 시각 효과**다. 명중 판정은 활을 놓기도 전에 서버에서 끝나 있고
(`validate_player_attack`), 화살은 그 결과를 납득시키는 역할만 한다 — 지금까지
근거 없이 기다리던 지연에 눈에 보이는 이유를 주는 것이 목적이다.

- 명중이면 매 프레임 몬스터의 현재 위치로 조준을 고쳐 반드시 닿는다. 몬스터는
  비행 중 최대 2.7 m 움직인다(scp939 8 m/s × 333 ms). 발사 시점 위치로 직선
  비행시키면 화살은 허공에 꽂히는데 피해 숫자는 뜨는 모순이 생긴다.
- 빗나가면 조준을 고치지 않고 발사 시점 방향 그대로 목표를 3 m 지나쳐 사라진다.
- 활의 실제 월드 좌표에서 출발한다(`PlayerModel.getBowWorld`). 발사 요청은
  `monsterManager`가 놓는 순간에 큐에 넣고, 활 위치를 아는 `GameScene`이 다음
  프레임에 꺼내 쏜다.
- 사수당 한 발이면 충분하다 — 공격 간격 1533 ms가 최장 비행 333 ms보다 길다.
- 실제로 쓴 화살의 모델을 그린다. `PlayerAttacked.ammo_item_def_id`가 그것을
  실어 오므로, 남의 가방을 볼 수 없는 클라이언트도 남이 쏜 화살을 맞게 그린다.

땅에 꽂히기·꼬리 궤적·시위에 걸린 화살·명중음은 모두 범위 밖이다.

### 명중 시 어그로

서버는 명중·빗나감과 도발 범위 안의 사거리 거절을 `brain_hit`로 처리하여 몬스터의 표적을 갱신한다. 클라이언트에 별도의 몬스터 제어 메시지를 보내지 않는다.

### 양손 무기

`hands: 2` 무기를 착용하면 오프핸드 아이템이 가방으로 내려간다. 양손 무기를 든
상태에서 토치(`torch`·`worn_torch`)를 장착하면 양손 무기를 가방으로 옮기고 왼손에
토치를 든다. 인벤토리·퀵슬롯 사용과 왼손 슬롯으로 끌어놓기에 적용된다.
방패 등 다른 오프핸드 아이템은 양손 무기를 든 상태에서 장착할 수 없다.

- 구현: [server/src/game_state/combat.rs](../server/src/game_state/combat.rs),
  [server/src/game_state/inventory.rs](../server/src/game_state/inventory.rs),
  [client/src/lib/data/itemDefs.ts](../client/src/lib/data/itemDefs.ts)

---

## Guard (GUARD)

NetHack의 AC를 뒤집은 방어 수치이자 명중 목표값이다. 높을수록 맞히기 어렵다.
NetHack AC와의 대응은 `GUARD = 10 − AC`다.

- 캐릭터: 생성 시 DEX로 계산한 값(위 섹션)에 착용 아이템의 `guard`와 방어구 인챈트 +N을 더한다 ([ENCHANT.md](ENCHANT.md)).
- 몬스터: `data-src/monsters.csv`의 `guard`. 장소 보정을 받지 않는다.

---

## 몬스터 스탯 정의

몬스터는 [data-src/monsters.csv](../data-src/monsters.csv)에 정의하고, 빌드/개발 도구가 [data/monsters.json](../data/monsters.json)을 생성한다.

| 필드 | 타입 | 설명 |
|------|------|------|
| `health` | u32? | 최대 HP override. 비우면 레벨 기반 기본값 (`level d8` 평균 올림). 장소 레벨 보정은 보정 레벨의 기본값과 이 값 중 큰 쪽을 쓴다 |
| `level` | u8 | 몬스터 레벨 (기본 HP/명중/피해/XP 계산에 사용) |
| `guard` | u8 | 명중 목표값. 높을수록 맞히기 어렵고, 10 초과분은 XP 보너스에 영향 |
| `attackBonus` | i32? | 몬스터 명중 보너스 override. 비우면 `level` |
| `damageRoll` | string? | 대미지 주사위 override. 장소 레벨 보정과 무관하게 이 값을 쓴다. 비우면 레벨 기반 기본값(보정된 레벨 기준) |
| `behavior` | string | 몬스터 행동 트리 이름 (`data-src/behavior_trees.json`, 없으면 `brave` 사용) |
| `attackRange` | f32 | 근접 공격 가능 거리 |
| `chaseRange` | f32 | 플레이어 추적 시작 거리 |
| `attackCooldown` | u32 | 공격 간격 (밀리초) |

예: SCP-939는 `level 3`, `guard 10`, `behavior timid`, `attackRange 3`, `chaseRange 25`, `attackCooldown 4100`.

---

## 전투 흐름

### 플레이어 → 몬스터 공격

1. 웹은 `StartPlayerAttack`, agent-client는 단발 `PlayerAttack`을 보낸다 (위 "기본 공격 주기")
2. 서버에서 히트 롤: `roll_attack_with_accuracy(player_attack_bonus, monster_guard, ...)`
3. 결과를 주변 클라이언트에 보낸다 (`PlayerAttacked`)
4. 명중 시 몬스터 HP 차감
5. HP가 0이 되면 `MonsterDead`를 보내고, 시체는 60초 뒤 제거한다

### 몬스터 → 플레이어 공격

1. 서버 몬스터 AI가 표적을 선택하고 `monster_attack` 호출
2. 서버에서 히트 롤: `roll_attack_with_accuracy(monster_attack_bonus + place_attack_bonus, player_guard, ...)`
3. 결과를 주변 클라이언트에 보낸다 (`MonsterAttackedPlayer`)
4. 명중 시 플레이어 HP 차감
5. HP가 0이 되면 `PlayerDead`를 보낸다

### 리스폰

- 클라이언트가 `RequestRespawn` 전송
- 서버에서 HP 0 확인 후 최대 HP로 회복, 여관 병실(`data-src/world.json`의 `respawn`)로 이동
- `respawn.bedIds`의 침대 중 다른 플레이어가 차지하지 않은 첫 침대에 누운 상태로 (`object_type`/`object_id` 설정). 침대 위치·회전은 해당 지역 오브젝트 파일에서 읽으므로 에디터로 옮겨도 따라간다
- 침대가 모두 차 있으면 `respawn` 좌표에 서서 부활
- `PlayerRespawned { player }` 브로드캐스트. `player.object_type`이 있으면 클라이언트는 그 침대에 눕는다 (본인은 `InteractObject`를 다시 보내지 않음)

### 제자리 부활 (불사조의 부적)

- 가방에 `phoenix_talisman`이 있으면 사망 대화상자에 "Use Phoenix Talisman" 버튼이 뜬다
- 클라이언트는 일반 소모품처럼 `UseItem { instance_id }`를 보낸다 (`UseEffect::ReviveInPlace`)
- 서버는 HP 0일 때만 제자리·같은 층에서 최대 HP의 70%(items.csv `reviveHpPercent`)로 되살리고 부적 1개를 소모. 살아 있을 때 쓰면 거절하고 부적은 남긴다
- 사망 시 이미 적용된 XP 페널티는 그대로. 무적 시간·쿨타임은 없다
- 같은 `PlayerRespawned { player }`를 브로드캐스트하므로 클라이언트는 받은 위치·층을 그대로 따른다
- 획득: Rica 판매(basePrice 5,000c), 월드 드랍 0.2% (`data-src/world_drop.csv`)

---

## 경험치 (XP) 시스템

### 몬스터 처치 XP 공식

```
xp = 1 + level²  +  guard_bonus
```

**guard_bonus:**

| GUARD | 보너스 |
|-------|--------|
| 0 ~ 10 | 없음 |
| 11 | +2 |
| 12 | +4 |
| 13 | +6 |
| 10 + i | 2i |

일반 공식: `guard_bonus = max(guard - 10, 0) × 2`. `level`은 장소 레벨 보정이 반영된 레벨이다.

**예시:**

| 몬스터 | level | GUARD | xp |
|--------|-------|-------|----|
| 약한 적 | 1 | 8 | 1 + 1 = **2** |
| 보통 적 | 3 | 10 | 1 + 9 = **10** |
| 강한 적 | 5 | 12 | 1 + 25 + 4 = **30** |
| 보스 | 8 | 13 | 1 + 64 + 6 = **71** |

처치 XP는 막타가 아니라 각 플레이어가 준 피해 비율로 나눈다(`damage_xp_shares`). 피해를 준 플레이어는 최소 1을 받고, 1에 못 미친 몫을 1로 올린 만큼은 나머지가 나눌 양에서 뺀다. 몫을 내림해 남는 XP는 소수점 아래가 큰 순서로(같으면 피해가 큰 순서로) 1씩 더 준다. 그래서 몫의 합은 항상 처치 XP와 같다. 단, 피해를 준 사람 수가 처치 XP보다 많으면 모두 1을 받아 합이 처치 XP보다 커진다.

같은 파티원의 피해는 먼저 합쳐 한 몫으로 계산하므로, 파티 안에서 누가 쳤는지는 결과에 영향이 없다. 그 몫은 피해를 준 파티원 전원과, 피해를 주지 않았더라도 살아서 같은 층 150m 안에 있는 파티원이 나눈다. 한 명 늘 때마다 총량이 25%씩 늘고 균등 분배한다(`party_xp_share`, [shared/src/xp.rs](../shared/src/xp.rs)).

처치 전리품(무기, 드롭, 던전 열쇠)은 피해를 가장 많이 준 플레이어의 몫이다(`top_damage_dealer`, 같으면 먼저 때린 쪽). 파티 피해는 합치지 않고 개인 기준으로 본다. 떨어진 뒤 30초 동안은 그 플레이어만 주울 수 있고, 그 뒤에는 누구나 줍는다. 열쇠 판정도 이 플레이어 기준이다([DUNGEON_REWARD.md](DUNGEON_REWARD.md)). 처치 로그(`Player NAME killed …`)의 이름도 이 플레이어다. 우선권은 바닥 아이템의 `claimed_by`로 클라이언트에 전해지고, 30초가 지나면 서버가 `GroundItemClaimEnded`를 보내 푼다. 클라이언트는 남의 몫을 집으려 하지 않고, 무게 초과 때처럼 고개를 저으며 누구에게 우선권이 있는지 알린다. 막타로 전리품을 가져가던 문제(이슈 #199) 때문이다.

### 레벨업 필요 XP

플레이 시간에서 역산한 2~50레벨 표(`LEVEL_XP`, `shared/src/xp.rs`)를 쓴다. 표와 근거,
생성 스크립트는 [LEVEL_CURVE.md](LEVEL_CURVE.md). 50 이후는 `XP(n) = XP(50) × 2^(n−50)`.

| 레벨 | 필요 누적 XP |
|------|-------------|
| 1 | 0 |
| 2 | 17 |
| 5 | 586 |
| 10 | 9,956 |
| 20 | 511,156 |
| 30 | 3,451,156 |
| 40 | 11,371,156 |
| 50 | 26,971,156 |
| 51 | 53,942,312 |

### 죽음 페널티 (Death Penalty)

사망 시, 현재 레벨 구간 XP의 15%를 차감한다.

```
level_start_xp = XP(L)
next_level_xp = XP(L + 1)
level_band = next_level_xp - level_start_xp
penalty = max(1, floor(level_band * 0.15))
new_xp = max(0, current_xp - penalty)
```

#### 레벨 하락 조건

사망 후 XP가 현재 레벨 시작 XP보다 작아지면 레벨을 1 내린다.

```
if new_xp < XP(L):
  L = max(1, L - 1)   // 1회 사망당 최대 1레벨 하락
```

#### 레벨 하락 시 XP 보정

레벨 하락이 발생하면, 하위 레벨 구간의 최소 30% 진행도는 보장한다.

```
lower_start_xp = XP(L)
lower_next_xp = XP(L + 1)
lower_band = lower_next_xp - lower_start_xp
recovery_floor = lower_start_xp + floor(lower_band * 0.30)
new_xp = max(new_xp, recovery_floor)
```

#### 레벨 하락 시 Max HP 보정

레벨 업/다운 반복에서 이득이 없도록, 레벨업과 같은 `hp_delta`(위 "레벨업 시 Max HP 증가")를 굴려 뺀다.

```
hp_loss = hp_delta(HD(class), CON)
new_max_hp = max(level1_max_hp, current_max_hp - hp_loss)
current_hp = min(current_hp, new_max_hp)
```

- 레벨이 내려가지 않은 경우에는 `max_hp`를 깎지 않는다.
- `E(hp_gain) = E(hp_loss)`이므로 반복의 기대 순이득은 0이다.
- 클래스별 `E(max(roll, HD/2))`는 다음과 같다: d10=6.5, d8=5.25, d6=4.0, d4=2.75.

레벨 1에서는 레벨이 내려가지 않는다.

- 구현: `apply_death_penalty` ([shared/src/xp.rs](../shared/src/xp.rs))

---

## 네트워크 메시지

```
Client → Server:
  StartPlayerAttack { monster_id, dagger_skill, request_id }
  StopPlayerAttack  { request_id }
  PlayerAttack      { monster_id }                 (agent-client 단발 공격)
  RequestRespawn
  UseItem           { instance_id }                (phoenix_talisman → 제자리 부활)

Server → Client:
  PlayerAttacked        { player_id, monster_id, hit, roll, damage, critical, ammo_item_def_id, dagger_strike }
  PlayerAttackStopped   { monster_id, request_id }
  MonsterAttackedPlayer { monster_id, player_id, hit, roll, damage, current_health, ranged }
  MonsterDead           { monster_id, dropped_weapon_item_def_id }
  PlayerDead            { player_id }
  PlayerRespawned       { player }
```

- 구현: [shared/src/messages.rs](../shared/src/messages.rs)

---

## 몬스터 AI 상태

몬스터 AI는 서버에서만 돈다. 행동은 `behavior`가 고르는 행동 트리(`data-src/behavior_trees.json`)가 정하고,
웹은 서버가 보낸 이동과 상태를 보간해 보여 준다. 자세한 내용은 [SERVER_SIDE_MONSTER_AI.md](SERVER_SIDE_MONSTER_AI.md).

| 상태 | 설명 |
|------|------|
| `idle` | 대기 |
| `walk` | 이동 중 |
| `run` | 플레이어 추적 중 |
| `attack` | 공격 중 |
| `hit` | 피격 경직. `staggerImmune` 몬스터는 들어가지 않는다 |
| `dead` | 사망 |

- 구현: [server/src/game_state/monster_ai.rs](../server/src/game_state/monster_ai.rs), [shared/src/monster_ai/brain.rs](../shared/src/monster_ai/brain.rs)

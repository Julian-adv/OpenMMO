# Fishing

**Learned fishing (2026-09-21):** Watching one complete catch beside Tobin permanently unlocks Fishing. There is no fishing XP or skill level. Existing anglers keep access; all current fish and trophies are available immediately. Advanced Fishing remains planned. See also [the skills design](MANA_SKILLS_MAGIC.md).

Cast a rod at water, wait for the bite, hook in time, land the fish. The first
gathering profession, and the first consumer of the learned-skill system
(`shared/src/skills.rs`). Server-authoritative end to end: every timer, roll
and outcome lives in `server/src/game_state/fishing.rs`; clients render
broadcasts and answer with `FishingRespond`.

## 일반·고급 낚시 전환 설계

2026-09-21 합의. 낚시는 **일반 낚시 → 고급 낚시의 두 단계**로 구성한다.
각 스킬은 NPC에게 한 번 배우면 영구 습득하며, 낚싯대를 주 손에 장착해 사용한다.
낚시 경험치·레벨과 반복 사용에 따른 자동 승급을 없애고, MP는 소모하지 않는다.
라이선스 구입이나 허가증 대신 세계 속 낚시꾼에게 방법을 배우는 흐름으로 만든다.

### 습득과 역할

| 구분 | 일반 낚시 (Basic Fishing) | 고급 낚시 (Advanced Fishing) |
| --- | --- | --- |
| 스승 | 강가의 낚시꾼 토빈 (Tobin) | 항구 등에서 만나는 숙련된 낚시꾼 |
| 습득 | 토빈 곁에서 캐스팅부터 포획까지 한 차례 관찰하면 영구 습득 | 일반 낚시 습득 후 숙련된 낚시꾼에게 새로운 기법을 배워 습득 |
| 활동 | 강·호수·해안 및 현재 노 젓는 보트에서 낚시 | 향후 큰 배에서 하는 선상 낚시, 특수 미끼를 사용하는 낚시 |
| 어종 | 현재 잡을 수 있는 모든 어종과 모든 대물 | 참치처럼 현재 없는 신규 어종을 향후 추가 |

토빈은 낚싯대를 판매하며 채비, 찌 연결, 미끼 달기, 입질 대응을
간단히 설명한다. 스킬은 대화가 아니라 실제 낚시 관찰로 습득하며, 대화는 기본 조작을 안내한다.
일반 낚시에서 별도 미끼 아이템을 소비해야 하는지는 아직 정하지 않는다.
외형·A포즈 원화·리깅된 게임 모델은 [토빈 에셋 기록](assets/characters.md#tobin--토빈--강가의-낚시꾼-2026-09-21)에 정리한다.

고급 낚시는 동네 낚시꾼이 다른 스승을 소개해 주는 흐름으로 연결한다.
예를 들어 향후 큰 배에서 낚시하려는 플레이어에게 항구의 낚시꾼을 찾아가도록 안내한다.
새로운 지역과 인물을 찾아 배우는 과정이 성장의 계기가 되며,
반복 포획 횟수나 낚시 경험치를 채워 승급하는 조건은 두지 않는다.

### 현재 잡을 수 있는 어종

2026-09-21 [아이템 원본 데이터](../data-src/items.csv)와 [게임용 데이터](../data/items.json) 기준,
현재 낚이는 어종은 5종이며 각 어종에 대물 아이템이 하나씩 있다.
아래 일반 물고기 5종과 대물 5종 모두 일반 낚시의 대상이다.

| 어종 (게임 내 이름) | 일반 물고기 아이템 ID | 대물 아이템 ID |
| --- | --- | --- |
| Raw Minnow | `raw_minnow` | `trophy_raw_minnow` |
| Raw Perch | `raw_perch` | `trophy_raw_perch` |
| Raw Trout | `raw_trout` | `trophy_raw_trout` |
| River Salmon | `river_salmon` | `trophy_river_salmon` |
| Golden Sturgeon | `golden_sturgeon` | `trophy_golden_sturgeon` |

### 단계 구분과 낚시의 재미

- 해안과 현재 노 젓는 보트(Rowboat)에서의 낚시는 일반 낚시로 가능하다.
  바다 전체를 고급 낚시로 제한하지 않는다. 고급 단계의 선상 낚시는 향후 큰 배가
  도입될 때의 활동으로 남겨 두며, 특수한 채비로 물고기를 노리는 기법도 포함한다.
- 현재 잡을 수 있는 모든 어종은 희귀 어종과 모든 대물 변형까지 일반 낚시에 포함한다.
  기존 `minFishingLevel` 제한은 제거하며, 일반 낚시를 배우면 기존 어종 전부에 도전할 수 있다.
  기존 어종이나 대물을 고급 단계로 옮기지 않는다.
- 고급 낚시는 참치처럼 현재 없는 신규 어종과 낚시 기법을 향후 추가하는 확장으로 둔다.
  참치는 예시이며 구체적인 신규 어종 목록과 조건은 추후 정한다.
- 입질 대응, 장력 조절, 대물 도전과 칭호가 지속적인 재미와 목표를 제공한다.
  고급 습득만으로 대기 시간·성공률·보상을 일괄 상향하는 구조는 두지 않는다.
- 스킬 표시는 습득 여부, 사용 조건, 설명을 중심으로 구성하며 경험치·레벨 표시는 없앤다.

### 구현 전 남은 결정

- 고급 낚시 스승의 이름·위치, 교육 비용 유무와 구체적인 대화 흐름.
- 고급 낚시에 추가할 신규 어종, 특수 미끼의 종류·획득·소비 규칙, 필요한 수역·채비 조건.
- 향후 큰 배 도입 시 고급 선상 낚시의 구체적인 방식과 조건.
- 고급 스킬 부여 기준.

### 토빈 배치 (2026-09-21 구현)

- 위치 **world(-1499.9, 0.6, 4728.4)**, tile(-23, 74), cell(4, 24), 방향 **-89.0°**. 게임 시간 **08:30–19:00**, **19:30–다음 날 02:00**에 강가에서 낚시한다. **02:00–08:00**에는 근처 **111번 침대**(`rustic_bed`, world(-1496.4515, 0.6, 4722.6916), 방향 270°)에서 잔다.
- **08:00–08:30** 아침, **19:00–19:30** 저녁에는 강가 옆 **world(-1497.7, 0.6, 4728.4)**에서 모닥불 식사를 한다. 기존 NPC 모닥불 명령으로 육지 쪽에 불을 피우고, 매 끼니마다 소지한 생선 중 값이 낮은 것 한 마리를 실제로 굽고 먹은 뒤 쉰다. 잠근 생선은 사용하지 않고, 생선이 없으면 불가에서 쉰다. 식사 시간에는 낚시를 멈추고, 08:30과 19:30에 낚시를 재개한다.
- NPC 레지스트리 `tobin` / `Tobin`, 한국어 별칭 `토빈`. 작업 장비로 `fishing_rod`를 지급·장착한다. [일정](../agent-client/data/npcs/tobin/schedule.json)과 [역할 설정](../agent-client/data/npcs/tobin/instance.txt).
- 일정의 `action: "fishing"`은 지정 방향 4m 앞에 캐스팅한다. 서버가 수심을 검증하고 찌를 실제 수면 높이에 맞춘다. 낚싯대 장착만으로는 찌·바깥 낚싯줄을 표시하지 않으며, 실제 세션의 브로드캐스트로 표시하고 종료 시 없앤다. 성공 시에는 기존 물고기 들어 올리기 연출을 보여준다. 뒤늦게 접근한 플레이어도 진행 중인 낚시를 볼 수 있다.
- 공식 NPC도 플레이어와 같은 캐스팅·입질·힘겨루기·보상 흐름을 사용한다. 에이전트가 입질에 챔질하고 장력에 따라 감거나 풀며, 잡은 물고기는 실제 인벤토리로 지급된다. 일정 종료·이동·장비 해제·사망 시 중단한다.
- 에이전트는 5초마다 낚시 일정을 확인하며, 한 번 끝나면 최소 5초 쉬어 성공 연출이 끝난 뒤 재개한다. 캐스팅 응답은 최대 10초 기다리고, 서버가 거부하면 30초 뒤 재시도한다. 거래·전투·병문안 중이거나 낚싯대를 장착하지 않았을 때는 자동 캐스팅하지 않는다.
- 가방 무게가 한도의 80%에 이르거나 잡은 것이 무게 초과로 발밑에 떨어지면(2026-09-26), 낚시 일정 중 에이전트가 낚시를 멈추고 코인 주머니를 열고, 값 없는 잡동사니(`old_boot`·`clump_of_kelp`)를 버리고, 생선·유리병 편지를 리카에게 판다. 식사용으로 굽는 생선 중 싼 것 2마리는 남긴다. 리카가 같은 층 근처에 없거나 판매가 끝나면 10분 뒤에 다시 판단하고, 그동안 일정이 강가로 돌려보내 낚시를 재개한다. 무게 초과로 떨어진 catch는 프롬프트에 가방에 있다고 쓰지 않는다.
- 토빈의 상점에서 낚싯대를 기본 가격 3실버에 판매한다. 리카의 판매 목록에서는 제외했으며, 토빈이 장착한 작업용 낚싯대와 상점 재고는 별개다.
- 공식 NPC 토빈이 캐스팅할 때부터 포획을 마칠 때까지 **6m 이내**, 같은 층, 높이 차 3m 이내에서 살아 있는 상태로 관찰하면 **Fishing을 영구 습득**한다. 관찰에는 낚싯대나 비용이 필요 없다. 중간에 접근했거나 범위를 벗어나면 다음 캐스팅부터 다시 관찰한다. 취소·놓침에는 습득하지 않으며, 다른 NPC와 토빈을 사칭한 일반 플레이어는 가르칠 수 없다.
- 습득 즉시 알림과 캐릭터 스킬 목록을 갱신하고 기존 스킬 저장 경로로 영구 보존한다. 반복 관찰로 보상을 주지 않는다. 신규 플레이어는 습득 후 캐스팅할 수 있고, 기존 Fishing 기록이 있는 캐릭터와 공식 NPC는 계속 낚시할 수 있다. 낚시 XP·레벨·어종별 레벨 제한을 제거했다. 서버 시작 시 기존 DB의 스킬 XP·레벨 열도 제거하고 캐릭터 ID·스킬 ID만 보존한다. 캐릭터 자체의 XP·레벨은 유지한다. 고급 낚시는 후속 작업이다.

## The loop

Implementation reviewed on 2026-09-21 against the server, web client,
agent-client, and item data. Observation-based acquisition and removal of XP/levels are implemented; Advanced Fishing is pending.

```
FishingCast ─► Casting (1 s) ─► Waiting (3.2–9.6 s)
                                    │ bite rolls the fish (species/size/trophy)
                                    ▼
                              Bite (2.5 s + 0.5 s latency grace)
                              │ Hook in time      │ too late / never
                              ▼                   ▼
                            Fight              Escaped
              (continuous reel/give-line tension sim)
                              │ exhaust + reel in  │ line snaps / hook thrown
                              ▼                    ▼
                           Caught               Escaped
```

**Getting a rod:** Tobin stocks the Fishing Rod at a base price of 3 silver;
a haggled deal can change the purchase price. Equip it in the main hand.
New anglers must first watch one complete catch beside Tobin to learn Fishing.
Existing Fishing skill records retain access. Rods are excluded
from dungeon treasure: `ItemDefs::load` rejects a rod with `chestTier` set
(`server/src/item_defs.rs`).

- **Cast** (`FishingCast { position }`): needs a fishing rod in the main hand
  (`category == "fishing_rod"`), a living player on floor 0, and a water
  target within 8 m in XZ. Water is `waterSurfaceY − terrainBed > 0.1 m`,
  sampled server-side from the baked **unified water field** (WFD1, sea +
  rivers) via `terrain::WaterSampler` alongside the terrain `HeightSampler`.
  This covers oceans and inland rivers. Depth at or below 0.1 m, or a
  sampling error, produces a direct `FishingError`. Missing water-field
  tiles sample as sea level (0 m); terrain depth still determines whether
  the target is fishable.
  On a rowboat, the target must also lie within 45° of the stern; casting
  preserves the boat's heading and the seated angler's stern-facing pose.
- **Wait**: uniform 3.2–9.6 s for every angler.
  The fish — species, size, trophy — is rolled *at the bite*,
  not at resolution. Trophy status is revealed at the hook; species and
  exact size are revealed on landing.
- **Bite** (`FishingBite` broadcast): the bobber dips. `Hook` must arrive
  within 2.5 s plus 0.5 s latency grace, measured by server arrival time.
  An unanswered bite is reaped at 3.5 s or the next tick; the extra delay
  does not extend the 3 s response deadline. Hooking *early* (before the
  bite) scares the fish off. Reeling or giving line before setting the hook
  also loses the catch. `Hold` outside a fight and duplicate `Hook` inputs
  during a fight are ignored.
- **End** (`FishingEnded { outcome }` broadcast): `Caught { item_def_id,
  size_cm, trophy }`, `Escaped`, or `Aborted`. A caught fish arrives through
  the normal `InventoryUpdated` (fish stack by species and trophy variant — exact size is
  announced, not stored), or spills as a ground item when the
  bag can't take the weight — never silently lost. Moving, attacking,
  disconnecting, dying, stowing the rod (unequipping it, or swapping a
  weapon into the main hand), or `FishingStop` aborts the session; gear
  changes that leave the rod in hand — a hat, an off-hand torch — don't
  break concentration. Position or floor changes, including teleports,
  cancel fishing; turning in place does not.

Timers advance on a 250 ms server tick (`run_ticks` in `server/src/main.rs`)
using `tokio::time::Instant`. Paused-time session tests live in
`server/src/game_state/tests/fishing_tests/`; pure fight and weighting tests
live in `server/src/game_state/fishing.rs`.

## The catch table

Anything with a `catchWeight` in `data-src/items.csv` can end up on the
hook — fish (`category: "fish"`), junk flotsam, and coin catches alike.
The catch columns:

| column | meaning |
|---|---|
| `rarityTier` | fish: 1 (common) … 5 (legendary); junk/coins: 0 — determines fight difficulty |
| `catchWeight` | relative species weight within its fish/flotsam pool |
| `sizeDice` | rolled length in cm (e.g. `6d8`) |
| `trophyCm` | fish only — length at or above this is a trophy |

Species pick uses fixed `catchWeight` values. All five species are available
as soon as Fishing is learned; the `minFishingLevel` column was removed.
Fish weights are minnow **130**, perch **96**, trout **53**, salmon **22**,
and golden sturgeon **5**. These approximate the former level-10 rarity
balance with every species unlocked. Flotsam holds a fixed **20%** of draws.
Size uses `sizeDice`. Each fish has a 20% trophy roll, which doubles its
size and guarantees trophy status. A failed roll can still produce a trophy
if the ordinary size meets `trophyCm`. Junk never becomes a trophy.
With the fixed 20% flotsam share, trophies occur on about 16–17% of all
bites (about 84% chance of at least one in ten bites). Landing one still
requires winning its high-tension fight.

Fish are sellable (`basePrice`, ordinary merchant flow) and edible —
`category "fish"` uses the food eating effect. Fish stack by species and
trophy variant. Exact size is deliberately **not stored on the item**;
it lives only in the catch announcement.

Ordinary fish `basePrice` values are minnow 10c, perch 25c, trout 60c,
salmon 2s, and golden sturgeon 15s. These are catalog prices: Rica pays
40% before haggling, so ordinary catches sell for 4c, 10c, 24c, 80c, and
600c respectively. Trophy variants have three times the base price and
unmodified payout. Other merchants and haggled deals can pay differently
(`server/src/game_state/trading.rs`, `deals.rs`, `data/merchants.json`).
Golden sturgeon accounts for about 1.3% of all draws.

The economy test
(`item_defs::tests::expected_catch_value_stays_in_the_coin_pile_economy`)
checks ordinary item values plus the expected coin-pouch payout at Rica's
unmodified 40% rate against the existing **5–25c** band. It excludes trophies,
fight failures, and time spent fishing, so it is not an income-per-hour estimate.

## Flotsam (junk & coin catches)

Not everything that bites is a fish. Four flotsam rows share the catch
table (a flat 20% of draws): an **Old Boot** and a **Clump of Kelp**
(worthless bag junk — the classic fishing gag), a **Message in a Bottle**
(base price 15c; Rica pays 6c before haggling), and a **Sunken Coin Pouch**
(`category: "coin_catch"` — it lands in the bag sealed like any other
catch; opening it via `use_item` (double-click in the bag) rolls its
`dice` column, `3d8`, pays the copper to the wallet through the same
path as ground coin piles, and the combat log reports the amount). All have
`rarityTier 0`, never become trophies, and fight like common fish. The species
is never revealed on an escape. The economy test includes flotsam.

## Skill

Stay within 6 m of official NPC Tobin, alive and on the same floor with at
most 3 m height difference, from casting until a successful catch. A late
arrival or interrupted observation must wait for the next cast. Watching
requires no rod, payment, or conversation. A successful lesson sends a
system message and `SkillsUpdate { skills: { learned: ["fishing"] } }`.
The skill persists through logout, reconnect, and server restarts.

Fishing has no skill XP, levels, or character XP rewards. The character
panel shows the same icon, description, tooltip, and draggable skill row as
other skills, using the existing fishing rod icon. Catch chance and fight
mechanics depend on species and player input, never on past fishing XP.
Wait and fight tuning use the former level-10 baseline: 3.2–9.6 s waits,
10% less pull than the former novice, and 10% faster reeling. Species weights
use the same baseline with all five species and their trophies unlocked.

The `character_skills` table stores only `character_id` and `skill_id`;
a row means the skill is learned. Startup atomically removes the legacy
`level` and `xp` columns while preserving every learned skill, including
zero-XP records and unknown skill IDs. Character level and XP are unchanged.
Repeated startup leaves the migrated schema intact. Runtime and protocol
state carry only the learned skill IDs. Protocol v93 requires updated clients.

## Client

- Learned Fishing appears in the character panel's **Skills** tab and can be
  dragged into a quickslot. Double-click the skill row, click its quickslot,
  or press the slot's number key to select water. A fishing rod must be in
  the main hand; no MP is consumed. Rowboat fishing remains available.
- Selecting Fishing shows a crosshair and a water-target hint. Left-click
  water within 8 m to cast through the existing fishing action. Invalid
  targets keep selection active without moving, attacking, or opening NPC
  interactions. Escape, Cancel, or activating Fishing again cancels selection.
  Losing the rod, dying, changing floors, teleporting, or switching skills
  also cancels selection. Quickslot bindings remain saved.
- Click water within 8 m with a rod equipped on floor 0 → `cast_fishing` intent
  (`managers/inputHandler.ts`; water = the baked `WaterFieldManager.surfaceAt`
  sits >0.1 m above the clicked terrain, so both ocean and rivers cast while
  dry ground or distant water still requests movement) → stop, face the
  water, send (`PlayerControl.svelte`).
  The server re-validates, so the client check only decides cast-vs-walk.
  Rowboat casts keep the heading and reject clicks outside the stern cone
  before changing the player's state. The direction check is shared via WASM.
- `components/FishingBobber.svelte`: every nearby angler's bobber, gentle
  idle bob, hard dip on bite. It first renders 2.6 s after `FishingCasted`
  is received: the 1.6 s swing delay from `data/player_anim_timing.json`
  plus the 1 s `CAST_MS`, matching the local splash sound. This visual delay
  is separate from the server's 1 s casting phase. A sagging white line
  connects the angler's rod tip to the float. During the fight it chases the fish's broadcast
  position (client-side smoothing — the 4 Hz beats are never snapped to) and
  shows droplets while the fish is Running, with intensity following its
  remaining stamina. Resting and Exhausted fish show no droplets.
- `components/FishingPrompt.svelte`: SPACE, a canvas click, the HOOK button,
  or a wheel flick sets the hook during a bite. Canvas clicks are captured
  before movement handling; unrelated UI
  clicks pass through. The fight HUD shows a fish-state line,
  tension gauge, and two hold-to-act stance buttons. REEL: hold the button,
  hold SPACE, or wheel down (winding toward you); GIVE LINE: hold the
  button, hold S, or wheel up (350 ms bursts). Releasing the controlling
  input returns to `Hold`. ESC sends `FishingStop` and aborts without a catch.
  Combat-log lines narrate cast/bite/outcome; catches and escapes also appear
  in chat. Keyboard fishing controls are ignored while typing.
- State in `stores/fishingStore.ts`; server messages handled in
  `network/messageHandlers.ts`.
- While fighting, the left hand supports the raised rod and the right hand
  follows the reel crank. `reel` winds forward, `giveline` reverses, and
  `hold` stops the spool. Local input animates immediately; nearby players
  and NPCs use the stance in the server's fight beats (protocol v92).
  `utils/fishingReel.ts` blends the hand targets over the fishing idle pose,
  including its seated variant, and restores the pose before each mixer
  update. Casting, movement and equipment changes release the correction.
  The crank hand and forearm solve as one straight segment, with the elbow
  biased backward, so following the handle does not bend the wrist.
- A successful fish catch keeps the fishing pose for a 3.6-second presentation:
  raise the rod, draw the fish out of the water, then take the line in the right
  hand and lift it for display. The fish hangs from its mouth and sways with a
  moving tail; its appearance and display size follow the caught item and size.
  The existing `FishingEnded` broadcast starts the same presentation for nearby
  anglers. Movement, combat, recasting, departure and disconnect cancel it.
  Rewards remain immediate; escapes and junk catches use their existing ending.

Fishing events use the common world-event subscription system: delivery is
within 32 m of the **angler**, in the same space, rather than centered on the
bobber (`server/src/game_state/interest.rs::publish_fishing`). A joining
subscriber receives the cast snapshot plus the latest bite/fight state;
leaving the subscription clears the fishing effect with `FishingEnded`.

## Agent parity

Agents use the same protocol and server deadlines. `FishingBite` identifies
the angler; the reaction window comes from the shared constants.

The agent-client implements this as a reflex layer (`agent-client/src/state/events.rs`):
it auto-hooks its own bites and plays each `FishingFight` beat through the
shared `auto_stance` policy (answering only on change) — mechanically, like
its A* movement layer, while the LLM makes the decisions via two actions:
`{"type": "fish", "x": …, "z": …}` and `{"type": "stop_fishing"}`.
When both coordinates are supplied, they are used as the cast target.
Otherwise the target is 4 m south (`x = player.x`, `z = player.z + 4`),
independent of facing. This fallback does not search for water or choose a
rowboat stern target; the server still validates both. Outcomes come back
to the model as `[Fishing]` events; in-flight messages are classified as
noise so they cost no LLM calls.

NPC schedules can also start this same loop through `action: "fishing"`.
The routine casts in the scheduled direction, lets the reflex layer land
real catches, and pauses between attempts. Equipping a rod alone starts
neither a session nor its bobber/line effects.

Answers wait 300–800 ms for a hook (`HOOK_REACTION_MS`) and 250–350 ms
for a stance (`STANCE_REACTION_MS`), with one response pending at a time.
Beats received during that delay are skipped; ending the session cancels
the pending response. The simulation tests
`the_stance_policy_survives_a_human_reaction_delay` and
`trophy_policy_survives_human_reaction_delay` exercise these delays with
additional network latency. Changes to timing should recheck both policies.

## The fight

Hooking is only the start: the fight is a continuous tug-of-war simulated on
the server's 250 ms tick (constants in `shared/src/fishing.rs`, pure step in
`server/src/game_state/fishing.rs::step_fight`). The fish alternates
**Running** bursts (2 s to `3.5 + 0.15 × rarity` s) and shorter **Resting**
breathers (0.8–2 s). Junk uses rarity 1 for these fight calculations.
The angler holds one of three stances, changed any time via
`FishingRespond`: **reel**, **give line**, or **hold**.

- **Tension** (the gauge; snaps at 100, `Escaped`): a Running fish pulls
  `(18 + 1.8·rarity)/s`, scaled up to 1.3× by how much line is out. Reeling adds 14/s while the fish is active; Resting and Exhausted
  fish shed 8/s naturally. Giving line subtracts another 44/s, exceeding
  the strongest fish pull. The hook-set opens the fight at 30 tension.
  Trophy rates are scaled as described below.
- **Distance** (shown, not numbered: the bobber *is* the fish): runs take
  ~1.1–1.5 m/s of line, reeling takes it back (1.76 m/s vs a Resting fish,
  0.66 against a run, 2.75 when Exhausted). The fish wanders but stays within
  6 m of the cast point. The distance integration uses a **line floor**:
  the cast handler samples the player→cast ray in 0.5 m steps to find the
  first fishable point, adds 0.4 m, caps it at the cast distance, then floors
  it at 2 m. Exhausted fish steer back toward the cast ray. The fight keeps
  the cast point's water-surface Y and does not resample terrain or water at
  the wandered positions; the radius limit alone does not check shorelines.
  Giving line to a Running fish also adds 0.6 m/s to its outward speed.
- **Stamina** (shown on the HUD and in the splash): only drag
  burns it — Running under ≥20 tension costs `2 + 12·(tension/100)²` per
  second; the square means timid mid-band play barely tires the fish and
  real progress comes from riding the gauge near the top, while a slack
  line lets a Resting fish *recover*. Pools are `38 + 12·rarity`.
- **Endgame**: at 0 stamina the fish goes **Exhausted** — reel it down to
  the line floor (within 0.3 m) and it lands (`Caught`). A lively fish
  dragged within 1 m of the floor panics into a fresh run instead, so only
  a spent fish can ever be landed. A fight that reaches 60 s (40 s for
  trophies) throws the hook (`Escaped`). These
  deadlines use accumulated simulation time: each tick integrates elapsed
  time capped at 1 s, so a long server stall is not fully counted.

Trophy status is rolled at the bite, using `TROPHY_ROLL_CHANCE_PCT` or
the species-size threshold. It is announced on the first fight beat and stays
fixed through landing. Trophy fish drain stamina only while Running at
**80 or higher tension**. After first reaching 80 during a run, each full
second spent continuously below 80 while Running rolls a 1/3 chance to
lose the hook (three seconds on average, not a fixed deadline). Reaching
80 or entering Resting resets that timer. The initial pressure buildup and
exhausted reel-in are exempt. Below 80 they do not tire; resting on slack line
still restores stamina. Their tension changes at 40% of the ordinary
rate so the narrow high-tension band is playable with human reaction delay.
There is no final score gate: an exhausted trophy lands normally, while a
snapped line or timeout awards no fish. The shared agent reflex uses the
same trophy flag and reacts with its usual delay.

Successful trophies award exactly one `trophy_*` fish: a separate stack
with the same species icon, twice its ordinary weight, and three times its
base price. It remains edible and grills into the ordinary cooked fish.
The title check uses the base species. There is no second-fish roll or
accumulated bonus chance.

Every beat carries `FishingFight { player_id, bobber, fish_state,
tension_pct, stamina_pct, trophy, stance }`. The web HUD and agent reflex read
these same fields; bystanders receive them through world-event subscriptions.
Trophy catches are celebrated to everyone in delivery radius via the
`FishingEnded` broadcast they already receive.

## Deliberate limits

- The current implementation has no bait, rod tiers, or designated fishing
  spots (any water — ocean or river — works after learning Fishing).
  Special bait belongs to the planned Advanced Fishing expansion.
- Animations are in: a Mixamo cast plays once on `FishingCasted`, then a
  fishing idle loops until the line comes in (`fishing.glb` pack, local
  and remote players). Local animation is driven by
  `game-scene/GameScenePlayersLayer.svelte`; remote casts and the transition
  to idle are handled by `messageHandlers.ts` and `remotePlayerManager.ts`.
  Rowboat anglers use the seated fishing pose.
- Fishing SFX are local-player-only: cast whir at 1.6 s, splash at 2.6 s
  after `FishingCasted`, plop on bite, reel click when `Reel` is selected,
  snap on any `Escaped` result, and a jingle on catch. Pending delayed
  sounds are cancelled when fishing ends. Sources are recorded in
  [the sound asset documentation](assets/sfx.md).

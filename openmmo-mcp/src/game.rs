use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_client::driver::{
    adventurer_actions, format_event, run_actions, tick_combat, walk, WalkTo, Walked,
};
use agent_client::google_auth::{home_dir, DeviceSignIn, GoogleAuth, GoogleAuthConfig};
use agent_client::session::{self, AuthSource, Session, World};
use agent_client::state::SharedState;
use agent_client::ws::ClientIdentity;
use onlinerpg_shared::entity::ClientKind;
use onlinerpg_shared::messages::strip_command;
use onlinerpg_shared::{ClientMessage, ServerMessage};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

const LABEL: &str = "mcp";
const CLIENT: ClientIdentity = ClientIdentity {
    kind: ClientKind::Mcp,
    version: env!("CARGO_PKG_VERSION"),
};
const ENTRY_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const ATTACK_TURN: Duration = Duration::from_secs(20);
const DEFAULT_RETREAT_HP_PERCENT: u32 = 30;

/// Lines `look` always repeats: what can change under the character.
const LIVE_PREFIXES: [&str; 10] = [
    "You:",
    "You are DEFEATED",
    "You are indoors",
    "MP:",
    "Hunger:",
    "Player:",
    "Monster:",
    "Item on ground:",
    "Campfire nearby:",
    "Your party:",
];
/// Only a full look lists these: their distances shift with every step.
const DISTANT_PREFIX: &str = "Dungeon:";

/// Not confidential (RFC 8252 §8.5), but kept out of the repo.
const BAKED_CLIENT_SECRET: Option<&str> = option_env!("GOOGLE_CLI_CLIENT_SECRET");

fn google_config() -> GoogleAuthConfig {
    GoogleAuthConfig {
        client_secret: std::env::var("GOOGLE_CLI_CLIENT_SECRET")
            .ok()
            .or_else(|| BAKED_CLIENT_SECRET.map(str::to_string)),
        token_cache: std::env::var("OPENMMO_GOOGLE_CACHE").ok(),
        ..Default::default()
    }
}

fn cache_dir() -> PathBuf {
    let set = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    set("XDG_CACHE_HOME")
        .or_else(|| set("LOCALAPPDATA"))
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".cache")))
        .unwrap_or_else(|| PathBuf::from("data/cache"))
        .join("openmmo-mcp")
}

/// The chat commands any player may type. The rest are operator tools the
/// account may hold and the model must not.
const PLAYER_COMMANDS: [&str; 14] = [
    "/w",
    "/whisper",
    "/title",
    "/who",
    "/escape",
    "/emote",
    "/join_dance",
    "/play_instrument",
    "/play_music",
    "/recite",
    "/recite_quiet",
    "/light_campfire",
    "/lay_stall",
    "/pack_stall",
];

fn may_say(message: &str) -> bool {
    !message.trim_start().starts_with('/')
        || PLAYER_COMMANDS
            .iter()
            .any(|command| strip_command(message, command).is_some())
}

fn lines_of(view: &str) -> HashSet<String> {
    view.lines().map(str::to_string).collect()
}

fn position_line(s: &SharedState) -> String {
    s.self_player
        .as_ref()
        .map(|p| {
            format!(
                "You: HP {}/{} at ({:.1}, {:.1}, {:.1})",
                p.health, p.max_health, p.position.x, p.position.y, p.position.z
            )
        })
        .unwrap_or_default()
}

/// What a line reports on, for the kinds that are re-reported as distances change.
fn subject(line: &str) -> Option<&str> {
    if line.starts_with("[PlayerNearby] ") {
        return line.split(" Lv.").next();
    }
    if line.starts_with("[Sighted] ") {
        return line.split(" — ").next();
    }
    None
}

/// Drop repeats and the lines a fuller line in the same batch already covers.
fn quiet(lines: Vec<String>, own_name: Option<&str>) -> Vec<String> {
    let covered = |index: usize, line: &str| {
        let rest = |prefix: &str| line.strip_prefix(prefix).map(str::trim);
        if let Some(monster) = rest("[MonsterSpawned] ").and_then(|r| r.split(' ').next()) {
            let tag = format!("[{monster}]");
            return lines
                .iter()
                .any(|l| l.starts_with("[Sighted]") && l.contains(&tag));
        }
        if let Some(name) = rest("[PlayerAppeared] ") {
            let nearby = format!("[PlayerNearby] {name} ");
            return own_name == Some(name) || lines.iter().any(|l| l.starts_with(&nearby));
        }
        if let Some(subject) = subject(line) {
            return lines[index + 1..]
                .iter()
                .any(|l| self::subject(l) == Some(subject));
        }
        line.starts_with("[PlayerDisappeared] ")
            || own_name.is_some_and(|name| {
                line.strip_prefix("[Move] ")
                    .is_some_and(|r| r.starts_with(name) && r[name.len()..].starts_with(" ->"))
            })
    };
    lines
        .iter()
        .enumerate()
        .filter(|(index, line)| !covered(*index, line))
        .map(|(_, line)| line.clone())
        .collect()
}

struct Account {
    auth: AuthSource,
    name: Option<String>,
}

enum SignIn {
    Out,
    Pending {
        prompt: String,
        task: JoinHandle<anyhow::Result<GoogleAuth>>,
    },
    In(Arc<Account>),
}

pub struct Game {
    server: String,
    world: World,
    sign_in: Mutex<SignIn>,
    session: Mutex<Option<Session>>,
    shown: Mutex<HashSet<String>>,
}

impl Game {
    pub fn new(server: String, terrain: &str) -> Self {
        let cache = cache_dir().join("height");
        Self {
            server,
            world: World::open(terrain, &cache.to_string_lossy()),
            sign_in: Mutex::new(SignIn::Out),
            session: Mutex::new(None),
            shown: Mutex::new(HashSet::new()),
        }
    }

    async fn account(&self) -> anyhow::Result<Arc<Account>> {
        let mut sign_in = self.sign_in.lock().await;
        let account = match &*sign_in {
            SignIn::In(account) => return Ok(Arc::clone(account)),
            SignIn::Pending { prompt, task } if !task.is_finished() => {
                anyhow::bail!("Still waiting for Google sign-in. {prompt}")
            }
            SignIn::Pending { .. } => {
                let SignIn::Pending { task, .. } = std::mem::replace(&mut *sign_in, SignIn::Out)
                else {
                    unreachable!()
                };
                Account {
                    auth: AuthSource::Google(task.await??),
                    name: None,
                }
            }
            SignIn::Out => {
                let config = google_config();
                if let (Ok(name), Ok(token)) = (
                    std::env::var("OPENMMO_ACCOUNT"),
                    std::env::var("OPENMMO_NPC_TOKEN"),
                ) {
                    Account {
                        auth: AuthSource::NpcToken(token),
                        name: Some(name),
                    }
                } else if let Some(google) = GoogleAuth::cached(config.clone()).await {
                    Account {
                        auth: AuthSource::Google(google),
                        name: None,
                    }
                } else {
                    let pending = DeviceSignIn::begin(config).await?;
                    let prompt = format!(
                        "Ask the user to open {} and enter the code {} (valid for {} minutes), \
                         then call this tool again.",
                        pending.verification_url,
                        pending.user_code,
                        pending.expires_in_secs / 60
                    );
                    let message = format!("Google sign-in needed. {prompt}");
                    *sign_in = SignIn::Pending {
                        prompt,
                        task: tokio::spawn(pending.finish()),
                    };
                    anyhow::bail!(message)
                }
            }
        };
        let account = Arc::new(account);
        *sign_in = SignIn::In(Arc::clone(&account));
        Ok(account)
    }

    async fn lobby(&self) -> anyhow::Result<session::Lobby> {
        let login = async {
            let account = self.account().await?;
            session::login(
                &self.server,
                LABEL,
                &CLIENT,
                &account.auth,
                account.name.as_deref(),
            )
            .await
        };
        tokio::time::timeout(CONNECT_TIMEOUT, login)
            .await
            .map_err(|_| anyhow::anyhow!("could not reach the game server at {}", self.server))?
    }

    pub async fn characters(&self) -> anyhow::Result<String> {
        let lobby = self.lobby().await?;
        if lobby.characters.is_empty() {
            return Ok(
                "Signed in. This account has no characters; create one in the web client first."
                    .to_string(),
            );
        }
        let mut out = String::from("Signed in. Characters:\n");
        for c in &lobby.characters {
            let _ = writeln!(out, "- {} (Lv.{} {:?})", c.name, c.level, c.class);
        }
        Ok(out)
    }

    pub async fn enter(&self, wanted: Option<&str>) -> anyhow::Result<String> {
        let started = Instant::now();
        let mut slot = self.session.lock().await;
        *slot = None;
        let mut lobby = self.lobby().await?;
        let character = lobby
            .characters
            .iter()
            .find(|c| wanted.is_none_or(|name| c.name == name))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no matching character on this account"))?;
        let entering = session::enter_game(
            &mut lobby.tx,
            &mut lobby.rx,
            LABEL,
            character.id,
            &character.name,
            None,
        );
        let buffered = tokio::time::timeout(ENTRY_TIMEOUT, entering)
            .await
            .map_err(|_| anyhow::anyhow!("the server did not answer the entry request"))??;
        let session = Session::start(lobby, buffered, &self.world, LABEL, None, |_| {}).await;
        let state = Arc::clone(&session.state);
        *slot = Some(session);
        drop(slot);

        let deadline = Instant::now() + ENTRY_TIMEOUT;
        loop {
            {
                let s = state.lock().await;
                if s.self_player.is_some() {
                    let view = s.format_world_state();
                    *self.shown.lock().await = lines_of(&view);
                    return Ok(format!(
                        "Entered in {} ms.\n{view}",
                        started.elapsed().as_millis()
                    ));
                }
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "entered, but no player state arrived"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn state(&self) -> anyhow::Result<Arc<Mutex<SharedState>>> {
        match &*self.session.lock().await {
            Some(session) if !session.is_closed() => Ok(Arc::clone(&session.state)),
            Some(_) => anyhow::bail!("the connection was lost; call enter again"),
            None => anyhow::bail!("not in the game; call enter first"),
        }
    }

    fn take_events(s: &mut SharedState) -> String {
        let events = s.drain_events();
        let mut lines: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                ServerMessage::OperatorNotice { message } => {
                    Some(format!("[OperatorNotice] {message}"))
                }
                _ => format_event(s, e),
            })
            .collect();
        lines.extend(s.drain_agent_events());
        let own_name = s.self_player.as_ref().map(|p| p.name.as_str());
        quiet(lines, own_name).join("\n")
    }

    pub async fn look(&self, full: bool) -> anyhow::Result<String> {
        let state = self.state().await?;
        let mut s = state.lock().await;
        let view = s.format_world_state();
        let events = Self::take_events(&mut s);
        drop(s);

        let mut shown = self.shown.lock().await;
        let mut out = String::new();
        let mut omitted = 0;
        for line in view.lines() {
            let live = LIVE_PREFIXES.iter().any(|p| line.starts_with(p));
            let fresh = !line.starts_with(DISTANT_PREFIX) && !shown.contains(line);
            if full || live || fresh {
                out.push_str(line);
                out.push('\n');
            } else {
                omitted += 1;
            }
        }
        *shown = lines_of(&view);
        if omitted > 0 {
            let _ = writeln!(
                out,
                "({omitted} unchanged lines omitted; look with full=true lists them)"
            );
        }
        if !events.is_empty() {
            let _ = write!(out, "\nSince your last look:\n{events}\n");
        }
        Ok(out)
    }

    /// Returns when the monster is gone, health runs low, or the turn runs out.
    async fn fight(
        state: &Arc<Mutex<SharedState>>,
        monster_id: &str,
        sprint: Option<bool>,
        retreat_hp_percent: Option<u32>,
    ) -> &'static str {
        let retreat = retreat_hp_percent
            .unwrap_or(DEFAULT_RETREAT_HP_PERCENT)
            .min(100);
        let started = Instant::now();
        loop {
            let wait = {
                let s = state.lock().await;
                let Some(me) = &s.self_player else {
                    return "You are no longer in the world.";
                };
                if me.health == 0 {
                    return "You are defeated.";
                }
                if me.health * 100 < me.max_health * retreat {
                    return "Your health is below the retreat threshold; the attack stopped.";
                }
                s.player_attack_wait()
            };
            if started.elapsed() >= ATTACK_TURN {
                return "Still fighting; call attack again to continue.";
            }
            if !wait.is_zero() {
                tokio::time::sleep(wait.min(Duration::from_millis(200))).await;
                continue;
            }
            if !tick_combat(state, monster_id, sprint).await {
                return "The fight is over.";
            }
        }
    }

    async fn report(state: &Arc<Mutex<SharedState>>, outcome: &str) -> String {
        let mut s = state.lock().await;
        let events = Self::take_events(&mut s);
        format!("{outcome}\n{}\n{events}", position_line(&s))
            .trim()
            .to_string()
    }

    pub async fn attack(
        &self,
        monster_id: &str,
        retreat_hp_percent: Option<u32>,
    ) -> anyhow::Result<String> {
        let state = self.state().await?;
        let outcome = Self::fight(&state, monster_id, None, retreat_hp_percent).await;
        Ok(Self::report(&state, outcome).await)
    }

    /// A final attack is fought like `attack`.
    pub async fn act(&self, actions: &[serde_json::Value]) -> anyhow::Result<String> {
        let state = self.state().await?;
        let outcome = match run_actions(&state, actions, may_say).await {
            Some((monster_id, sprint)) => Self::fight(&state, &monster_id, sprint, None).await,
            None => "",
        };
        Ok(Self::report(&state, outcome).await)
    }

    /// One line per action, or the full text of those mentioning `about`.
    pub fn actions(about: Option<&str>) -> String {
        let docs = adventurer_actions();
        let Some(about) = about.map(str::to_lowercase) else {
            return docs
                .iter()
                .map(|d| {
                    let summary = d.doc.lines().next().unwrap_or_default();
                    format!("{summary} [{}]", d.names.join(", "))
                })
                .collect::<Vec<_>>()
                .join("\n");
        };
        let found: Vec<&str> = docs
            .iter()
            .filter(|d| {
                d.names.iter().any(|name| name.contains(&about))
                    || d.doc.to_lowercase().contains(&about)
            })
            .map(|d| d.doc)
            .collect();
        if found.is_empty() {
            format!("No action mentions '{about}'.")
        } else {
            found.join("\n\n")
        }
    }

    pub async fn walk_to(&self, x: f32, z: f32, floor: u8) -> anyhow::Result<String> {
        let state = self.state().await?;
        let outcome = walk(&state, &WalkTo::Place { x, z, floor }, false, None).await;
        let at = position_line(&*state.lock().await);
        Ok(match outcome {
            Walked::Arrived => format!("Arrived.\n{at}"),
            Walked::Lost(reason) => format!("Stopped: {}.\n{at}", reason.clause()),
            Walked::Error => format!("Stopped: the move could not be sent.\n{at}"),
        })
    }

    pub async fn say(&self, message: String) -> anyhow::Result<String> {
        anyhow::ensure!(
            may_say(&message),
            "that chat command is not available to you"
        );
        let state = self.state().await?;
        state
            .lock()
            .await
            .send_command(ClientMessage::ChatMessage { message })
            .await?;
        Ok("Sent.".to_string())
    }

    pub async fn leave(&self) -> String {
        match self.session.lock().await.take() {
            Some(_) => "Left the game.".to_string(),
            None => "Was not in the game.".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{may_say, quiet};

    #[test]
    fn only_player_chat_commands_pass() {
        for ok in ["hello", "!over here", "/w Ann hi", "/title 2", "/who"] {
            assert!(may_say(ok), "{ok}");
        }
        for refused in ["/spawnmob slime", "/titlefoo", " /god", "/tp 0 0"] {
            assert!(!may_say(refused), "{refused}");
        }
    }

    #[test]
    fn repeats_and_covered_lines_are_dropped() {
        let lines = [
            "[MonsterSpawned] m1 (Kobold)",
            "[MonsterSpawned] m2 (Kobold)",
            "[PlayerAppeared] Wick",
            "[PlayerAppeared] jake3",
            "[PlayerDisappeared] 5",
            "[Move] jake3 -> (1.0, 2.0, 3.0)",
            "[Sighted] Kobold [m1] HP 5/5 — at (0, 0), 9m east.",
            "[PlayerNearby] Wick Lv.1 appeared 9.0m away at (0.0, 0.0, 0.0)",
            "[Sighted] Kobold [m1] HP 5/5 — at (0, 0), 3m east.",
            "[PlayerNearby] Wick Lv.1 appeared 2.0m away at (0.0, 0.0, 0.0)",
            "[Attack] jake3 -> m1: hit=false dmg=0 critical=false",
            "[Attack] jake3 -> m1: hit=false dmg=0 critical=false",
        ]
        .map(str::to_string)
        .to_vec();
        assert_eq!(
            quiet(lines, Some("jake3")),
            [
                "[MonsterSpawned] m2 (Kobold)",
                "[Sighted] Kobold [m1] HP 5/5 — at (0, 0), 3m east.",
                "[PlayerNearby] Wick Lv.1 appeared 2.0m away at (0.0, 0.0, 0.0)",
                "[Attack] jake3 -> m1: hit=false dmg=0 critical=false",
                "[Attack] jake3 -> m1: hit=false dmg=0 critical=false",
            ]
        );
    }
}

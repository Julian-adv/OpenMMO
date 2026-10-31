use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex, Notify};
use tokio::time::Instant;
use tracing::{debug, info, warn};

use crate::driver::{LlmBackend, RunDir};
use crate::process_group::GroupKill;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CodexConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_system_prompt_file")]
    pub system_prompt_file: String,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
    /// Raced against `model` once it reports overload or stays silent for
    /// `fallback_after_secs`; "" turns the fallback off.
    #[serde(default = "default_fallback_model")]
    pub fallback_model: String,
    #[serde(default = "default_fallback_after_secs")]
    pub fallback_after_secs: u64,
    /// After the fallback answers, every turn goes to it for this long.
    #[serde(default = "default_fallback_cooldown_secs")]
    pub fallback_cooldown_secs: u64,
}

fn default_model() -> String {
    "gpt-5.6-luna".to_string()
}

fn default_reasoning_effort() -> String {
    "medium".to_string()
}

fn default_fallback_model() -> String {
    "gpt-6-luna".to_string()
}

fn default_fallback_after_secs() -> u64 {
    30
}

fn default_fallback_cooldown_secs() -> u64 {
    600
}

fn default_system_prompt_file() -> String {
    "data/system_prompt.txt".to_string()
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            system_prompt_file: default_system_prompt_file(),
            reasoning_effort: default_reasoning_effort(),
            fallback_model: default_fallback_model(),
            fallback_after_secs: default_fallback_after_secs(),
            fallback_cooldown_secs: default_fallback_cooldown_secs(),
        }
    }
}

impl CodexConfig {
    /// The fallback model, unless it is off or the same as `model`.
    fn fallback(&self) -> Option<&str> {
        Some(self.fallback_model.trim()).filter(|model| !model.is_empty() && *model != self.model)
    }
}

#[derive(Clone, Default)]
pub struct CodexAppServer {
    session: Arc<AsyncMutex<Option<Arc<AppServerSession>>>>,
    fallback: FallbackWindow,
}

/// Shared by every NPC: one slow turn moves them all to the fallback.
#[derive(Clone, Default)]
struct FallbackWindow(Arc<Mutex<Option<Instant>>>);

#[derive(Debug, PartialEq)]
enum Window {
    Closed,
    Open,
    Expired,
}

fn window_at(until: Option<Instant>) -> Window {
    match until {
        None => Window::Closed,
        Some(until) if Instant::now() < until => Window::Open,
        Some(_) => Window::Expired,
    }
}

impl FallbackWindow {
    #[cfg(test)]
    fn state(&self) -> Window {
        window_at(*lock(&self.0))
    }

    /// Where the next turn goes. Once expired, one caller probes the primary
    /// while the window holds the others on the fallback for `probe`.
    fn route(&self, probe: Duration) -> Window {
        let mut until = lock(&self.0);
        let window = window_at(*until);
        if window == Window::Expired {
            *until = Some(Instant::now() + probe);
        }
        window
    }

    fn open(&self, cooldown: Duration) {
        *lock(&self.0) = Some(Instant::now() + cooldown);
    }

    fn close(&self) {
        lock(&self.0).take();
    }
}

#[derive(Clone, Copy)]
struct TurnSpec<'a> {
    config: &'a CodexConfig,
    model: &'a str,
    cwd: &'a std::path::Path,
    label: &'a str,
}

impl<'a> TurnSpec<'a> {
    fn on(self, model: &'a str) -> Self {
        Self { model, ..self }
    }
}

/// A turn that finished unsuccessfully, with the app-server's error class.
#[derive(Debug)]
struct TurnFailed {
    status: String,
    kind: Option<String>,
    message: String,
}

impl std::fmt::Display for TurnFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Codex turn {}: {}", self.status, self.message)
    }
}

impl std::error::Error for TurnFailed {}

/// `codexErrorInfo` is a bare string or a single-key object.
fn error_kind(info: Option<&Value>) -> Option<String> {
    match info? {
        Value::String(kind) => Some(kind.clone()),
        Value::Object(map) => map.keys().next().cloned(),
        _ => None,
    }
}

fn is_overload(kind: Option<&str>, message: &str) -> bool {
    matches!(
        kind,
        Some("serverOverloaded" | "responseTooManyFailedAttempts")
    ) || message.contains("at capacity")
}

fn failed_from_overload(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<TurnFailed>()
        .is_some_and(|failed| is_overload(failed.kind.as_deref(), &failed.message))
}

/// Run on `model`, racing `fallback_model` once the primary reports overload
/// or passes `fallback_after_secs`; the first answer wins and the other turn
/// is interrupted.
async fn send_with_fallback(
    session: &Arc<AppServerSession>,
    window: &FallbackWindow,
    spec: TurnSpec<'_>,
    prompt: &str,
) -> anyhow::Result<String> {
    let config = spec.config;
    let primary_model = config.model.as_str();
    let Some(fallback_model) = config.fallback() else {
        return session.run_turn(spec, prompt, None).await;
    };
    let after = Duration::from_secs(config.fallback_after_secs);
    let route = window.route(after);
    if route == Window::Open {
        return match session
            .run_turn(spec.on(fallback_model), prompt, None)
            .await
        {
            Ok(text) => {
                info!("[{}] Codex turn answered by {fallback_model}", spec.label);
                Ok(text)
            }
            Err(error) => {
                warn!(
                    "[{}] Codex fallback {fallback_model} failed; back to {primary_model}: {error:#}",
                    spec.label
                );
                window.close();
                session.run_turn(spec, prompt, None).await
            }
        };
    }
    let primary_won = |text: String| {
        if route == Window::Expired {
            window.close();
            info!(
                "[{}] Codex {primary_model} answered again; fallback closed",
                spec.label
            );
        }
        text
    };

    let (overload_tx, mut overloads) = mpsc::unbounded_channel();
    let primary = session.run_turn(spec, prompt, Some(overload_tx));
    tokio::pin!(primary);
    let (primary_error, reason) = tokio::select! {
        result = &mut primary => match result {
            Ok(text) => return Ok(primary_won(text)),
            Err(error) if failed_from_overload(&error) => {
                (Some(error), "failed with overload".to_string())
            }
            Err(error) => return Err(error),
        },
        Some(reason) = overloads.recv() => (None, reason),
        _ = tokio::time::sleep(after) => (None, format!("gave no answer in {}s", after.as_secs())),
    };
    info!(
        "[{}] Codex {primary_model} {reason}; racing {fallback_model}",
        spec.label
    );

    let fallback_done = |result: anyhow::Result<String>| {
        match &result {
            Ok(_) => {
                window.open(Duration::from_secs(config.fallback_cooldown_secs));
                warn!(
                    "[{}] Codex {primary_model} {reason}; {fallback_model} answered, so every turn uses it for {}s",
                    spec.label, config.fallback_cooldown_secs
                );
            }
            Err(error) => warn!(
                "[{}] Codex fallback {fallback_model} failed: {error:#}",
                spec.label
            ),
        }
        result
    };
    let backup = session.run_turn(spec.on(fallback_model), prompt, None);
    tokio::pin!(backup);
    if let Some(error) = primary_error {
        return fallback_done(backup.await).map_err(|_| error);
    }
    tokio::select! {
        result = &mut primary => match result {
            Ok(text) => Ok(primary_won(text)),
            Err(error) => fallback_done(backup.await).map_err(|_| error),
        },
        result = &mut backup => match fallback_done(result) {
            Ok(text) => Ok(text),
            Err(_) => primary.await.map(primary_won),
        },
    }
}

impl CodexAppServer {
    pub fn new() -> Self {
        Self::default()
    }

    async fn session(&self) -> anyhow::Result<Arc<AppServerSession>> {
        let mut slot = self.session.lock().await;
        if let Some(session) = slot.as_ref().filter(|session| !session.is_dead()) {
            return Ok(Arc::clone(session));
        }

        let session = AppServerSession::spawn().await?;
        *slot = Some(Arc::clone(&session));
        Ok(session)
    }

    async fn send_turn(
        &self,
        config: &CodexConfig,
        cwd: &std::path::Path,
        label: &str,
        prompt: &str,
    ) -> anyhow::Result<String> {
        let session = self.session().await?;
        let spec = TurnSpec {
            config,
            model: &config.model,
            cwd,
            label,
        };
        send_with_fallback(&session, &self.fallback, spec, prompt).await
    }
}

type RpcReply = std::result::Result<Value, String>;
type AppServerInput = Box<dyn AsyncWrite + Unpin + Send>;

struct AppServerSession {
    input: AsyncMutex<AppServerInput>,
    pending: Mutex<HashMap<u64, oneshot::Sender<RpcReply>>>,
    turns: Mutex<HashMap<String, Arc<TurnRoute>>>,
    /// Last logged usage per limit id, so only changes are logged.
    rate_limits: Mutex<HashMap<String, String>>,
    next_id: AtomicU64,
    dead: AtomicBool,
    process: Mutex<Option<(Child, GroupKill)>>,
    _run_dir: Option<RunDir>,
}

impl AppServerSession {
    async fn spawn() -> anyhow::Result<Arc<Self>> {
        let run_dir = RunDir::create()?;
        let mut command = codex_command();
        command
            .arg("app-server")
            .current_dir(run_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let (mut child, group) =
            crate::process_group::spawn_in_group(&mut command, "codex app-server")?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture codex app-server stdin"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture codex app-server stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture codex app-server stderr"))?;

        let session = Arc::new(Self {
            input: AsyncMutex::new(Box::new(input)),
            pending: Mutex::new(HashMap::new()),
            turns: Mutex::new(HashMap::new()),
            rate_limits: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            dead: AtomicBool::new(false),
            process: Mutex::new(Some((child, group))),
            _run_dir: Some(run_dir),
        });

        tokio::spawn(read_stdout(Arc::downgrade(&session), output));
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!(target: "codex_stderr", "{line}");
            }
        });

        session
            .request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "openmmo-agent-client",
                        "title": "OpenMMO Agent Client",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            )
            .await
            .map_err(|error| anyhow::anyhow!("Failed to initialize codex app-server: {error}"))?;
        session.notify("initialized").await?;
        info!("Shared Codex app-server ready");
        Ok(session)
    }

    fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Acquire)
    }

    async fn request(self: &Arc<Self>, method: &str, params: Value) -> anyhow::Result<Value> {
        if self.is_dead() {
            return Err(anyhow::anyhow!("Codex app-server is not running"));
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        lock(&self.pending).insert(id, sender);
        let _guard = PendingRequest {
            session: Arc::downgrade(self),
            id,
        };

        self.write_json(&json!({
            "method": method,
            "id": id,
            "params": params
        }))
        .await?;

        receiver
            .await
            .map_err(|_| anyhow::anyhow!("Codex app-server closed the {method} request"))?
            .map_err(|error| anyhow::anyhow!("Codex app-server {method} failed: {error}"))
    }

    async fn notify(&self, method: &str) -> anyhow::Result<()> {
        self.write_json(&json!({ "method": method })).await
    }

    async fn write_json(&self, value: &Value) -> anyhow::Result<()> {
        let mut line = serde_json::to_vec(value)?;
        line.push(b'\n');
        let result = {
            let mut input = self.input.lock().await;
            input.write_all(&line).await
        };
        if let Err(error) = result {
            self.fail(format!("Codex app-server stdin failed: {error}"));
            return Err(error.into());
        }
        Ok(())
    }

    fn fail(&self, reason: String) {
        if self.dead.swap(true, Ordering::AcqRel) {
            return;
        }

        warn!("{reason}");
        for (_, sender) in lock(&self.pending).drain() {
            let _ = sender.send(Err(reason.clone()));
        }
        for (_, route) in lock(&self.turns).drain() {
            let _ = route.events.send(TurnEvent::Disconnected(reason.clone()));
        }

        if let Some((mut child, mut group)) = lock(&self.process).take() {
            if let Ok(Some(status)) = child.try_wait() {
                group.disarm();
                warn!("Codex app-server exited with status {status}");
            }
        }
    }

    fn route(&self, thread_id: &str) -> Option<Arc<TurnRoute>> {
        lock(&self.turns).get(thread_id).cloned()
    }

    async fn delete_thread(self: &Arc<Self>, thread_id: &str) {
        if self.is_dead() {
            return;
        }
        if let Err(error) = self
            .request("thread/delete", json!({ "threadId": thread_id }))
            .await
        {
            warn!("Codex app-server kept thread {thread_id}: {error}");
        }
    }

    fn remove_route(&self, thread_id: &str) {
        lock(&self.turns).remove(thread_id);
    }

    fn handle_message(self: &Arc<Self>, mut message: Value) {
        let Some(message) = message.as_object_mut() else {
            return;
        };
        if let Some(Value::String(method)) = message.remove("method") {
            if let Some(id) = message.remove("id") {
                self.reject_server_request(id, method);
            } else {
                self.handle_notification(&method, message.remove("params"));
            }
            return;
        }

        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            return;
        };
        let Some(sender) = lock(&self.pending).remove(&id) else {
            return;
        };
        let reply = match message.remove("error") {
            Some(error) => Err(rpc_error(&error)),
            None => Ok(message.remove("result").unwrap_or(Value::Null)),
        };
        let _ = sender.send(reply);
    }

    fn reject_server_request(self: &Arc<Self>, id: Value, method: String) {
        let session = Arc::clone(self);
        tokio::spawn(async move {
            let _ = session
                .write_json(&json!({
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Unsupported server request: {method}")
                    }
                }))
                .await;
        });
    }

    fn handle_notification(self: &Arc<Self>, method: &str, params: Option<Value>) {
        if method == "account/rateLimits/updated" {
            if let Some(params) = params {
                self.log_rate_limits(&params);
            }
            return;
        }
        if !matches!(
            method,
            "turn/started" | "item/completed" | "turn/completed" | "error"
        ) {
            return;
        }
        let Some(params) = params else {
            return;
        };
        let Some(thread_id) = params
            .get("threadId")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let Some(route) = self.route(&thread_id) else {
            return;
        };

        if method == "turn/started" {
            if let Some(turn_id) = params.pointer("/turn/id").and_then(Value::as_str) {
                route.set_turn_id(turn_id);
            }
            return;
        }

        let event = match method {
            "item/completed" => TurnEvent::Item(params),
            "turn/completed" => TurnEvent::Completed(params),
            "error" => TurnEvent::Error(params),
            _ => return,
        };
        if route.events.send(event).is_err() {
            self.remove_route(&thread_id);
        }
    }

    fn log_rate_limits(&self, params: &Value) {
        let Some(snapshot) = params.get("rateLimits") else {
            return;
        };
        let limit = snapshot
            .get("limitId")
            .and_then(Value::as_str)
            .unwrap_or("default");
        let mut summary: Vec<String> = ["primary", "secondary"]
            .into_iter()
            .filter_map(|window| {
                let window_value = snapshot.get(window)?;
                let used = window_value.get("usedPercent")?.as_i64()?;
                let mins = window_value
                    .get("windowDurationMins")
                    .and_then(Value::as_i64)
                    .map(|mins| format!("/{mins}m"))
                    .unwrap_or_default();
                Some(format!("{window}={used}%{mins}"))
            })
            .collect();
        if let Some(reached) = snapshot.get("rateLimitReachedType").and_then(Value::as_str) {
            summary.push(format!("reached={reached}"));
        }
        let summary = summary.join(" ");
        let mut logged = lock(&self.rate_limits);
        if logged.get(limit) != Some(&summary) {
            info!("Codex rate limits limit={limit} {summary}");
            logged.insert(limit.to_string(), summary);
        }
    }

    async fn run_turn(
        self: &Arc<Self>,
        spec: TurnSpec<'_>,
        prompt: &str,
        overloads: Option<mpsc::UnboundedSender<String>>,
    ) -> anyhow::Result<String> {
        let thread_result = self
            .request("thread/start", thread_start_params(spec))
            .await?;
        let thread_id = required_string(&thread_result, "/thread/id", "thread/start")?;

        let (sender, mut receiver) = mpsc::unbounded_channel();
        let route = Arc::new(TurnRoute {
            events: sender,
            turn_id: Mutex::new(None),
            turn_ready: Notify::new(),
        });
        lock(&self.turns).insert(thread_id.clone(), Arc::clone(&route));
        let mut guard = TurnGuard {
            session: Arc::clone(self),
            thread_id: thread_id.clone(),
            route,
            completed: false,
        };

        let started = Instant::now();
        let turn_result = match self
            .request("turn/start", turn_start_params(spec, &thread_id, prompt))
            .await
        {
            Ok(result) => result,
            Err(error) => {
                guard.finish();
                return Err(error);
            }
        };
        if let Some(turn_id) = turn_result.pointer("/turn/id").and_then(Value::as_str) {
            guard.route.set_turn_id(turn_id);
        }

        let mut last_text = String::new();
        capture_turn_messages(&turn_result, &mut last_text);
        while let Some(event) = receiver.recv().await {
            match event {
                TurnEvent::Item(params) => {
                    if let Some(item) = params.get("item") {
                        capture_agent_message(item, &mut last_text);
                    }
                }
                TurnEvent::Error(params) => {
                    let kind = error_kind(params.pointer("/error/codexErrorInfo"));
                    let message = params
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("no message");
                    let will_retry = params
                        .get("willRetry")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    warn!(
                        "[{}] Codex {} error after {:.1}s kind={} will_retry={will_retry}: {message}",
                        spec.label,
                        spec.model,
                        started.elapsed().as_secs_f64(),
                        kind.as_deref().unwrap_or("-")
                    );
                    if let Some(overloads) = &overloads {
                        if is_overload(kind.as_deref(), message) {
                            let _ = overloads.send(format!(
                                "reported {}",
                                kind.as_deref().unwrap_or("overload")
                            ));
                        }
                    }
                }
                TurnEvent::Completed(params) => {
                    capture_turn_messages(&params, &mut last_text);
                    let status = params
                        .pointer("/turn/status")
                        .and_then(Value::as_str)
                        .unwrap_or("failed");
                    let error = params
                        .pointer("/turn/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let kind = error_kind(params.pointer("/turn/error/codexErrorInfo"));
                    guard.finish();
                    return completed_turn(status, last_text, error, kind);
                }
                TurnEvent::Disconnected(reason) => return Err(anyhow::anyhow!(reason)),
            }
        }

        Err(anyhow::anyhow!(
            "Codex app-server stopped reporting thread {thread_id}"
        ))
    }
}

impl Drop for AppServerSession {
    fn drop(&mut self) {
        self.process
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }
}

struct PendingRequest {
    session: Weak<AppServerSession>,
    id: u64,
}

impl Drop for PendingRequest {
    fn drop(&mut self) {
        if let Some(session) = self.session.upgrade() {
            lock(&session.pending).remove(&self.id);
        }
    }
}

struct TurnRoute {
    events: mpsc::UnboundedSender<TurnEvent>,
    turn_id: Mutex<Option<String>>,
    turn_ready: Notify,
}

impl TurnRoute {
    fn set_turn_id(&self, turn_id: &str) {
        *lock(&self.turn_id) = Some(turn_id.to_string());
        self.turn_ready.notify_one();
    }

    fn turn_id(&self) -> Option<String> {
        lock(&self.turn_id).clone()
    }
}

enum TurnEvent {
    Item(Value),
    Error(Value),
    Completed(Value),
    Disconnected(String),
}

struct TurnGuard {
    session: Arc<AppServerSession>,
    thread_id: String,
    route: Arc<TurnRoute>,
    completed: bool,
}

impl TurnGuard {
    fn finish(&mut self) {
        self.completed = true;
    }
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        let thread_id = self.thread_id.clone();
        let route = Arc::clone(&self.route);
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.session.remove_route(&thread_id);
            return;
        };
        let session = Arc::downgrade(&self.session);
        let completed = self.completed;
        runtime.spawn(async move {
            let turn_id = if completed {
                None
            } else {
                match route.turn_id() {
                    Some(turn_id) => Some(turn_id),
                    None => {
                        tokio::select! {
                            _ = route.turn_ready.notified() => route.turn_id(),
                            _ = tokio::time::sleep(Duration::from_secs(30)) => None,
                        }
                    }
                }
            };
            if let Some(session) = session.upgrade() {
                if let Some(turn_id) = turn_id {
                    let _ = session
                        .request(
                            "turn/interrupt",
                            json!({ "threadId": thread_id, "turnId": turn_id }),
                        )
                        .await;
                }
                session.remove_route(&thread_id);
                session.delete_thread(&thread_id).await;
            }
        });
    }
}

async fn read_stdout<R>(session: Weak<AppServerSession>, output: R)
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(output).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                debug!(target: "codex_stdout", "{line}");
                let Ok(message) = serde_json::from_str(&line) else {
                    warn!("Ignoring malformed Codex app-server JSONL");
                    continue;
                };
                let Some(session) = session.upgrade() else {
                    break;
                };
                session.handle_message(message);
            }
            Ok(None) => {
                if let Some(session) = session.upgrade() {
                    session.fail("Codex app-server stdout closed".to_string());
                }
                break;
            }
            Err(error) => {
                if let Some(session) = session.upgrade() {
                    session.fail(format!("Codex app-server stdout failed: {error}"));
                }
                break;
            }
        }
    }
}

fn thread_start_params(spec: TurnSpec<'_>) -> Value {
    json!({
        "model": spec.model,
        "cwd": spec.cwd.to_string_lossy(),
        "approvalPolicy": "never",
        "sandbox": "read-only",
        // Ephemeral threads can't be deleted and the app-server keeps ~12 MB each
        // (OOM every 4h); persisted ones are deleted after the turn instead.
        "ephemeral": false,
        "serviceName": "openmmo-agent-client"
    })
}

fn turn_start_params(spec: TurnSpec<'_>, thread_id: &str, prompt: &str) -> Value {
    json!({
        "threadId": thread_id,
        "input": [{ "type": "text", "text": prompt }],
        "model": spec.model,
        "effort": spec.config.reasoning_effort,
        "approvalPolicy": "never"
    })
}

fn required_string(value: &Value, pointer: &str, method: &str) -> anyhow::Result<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("Codex app-server {method} returned no {pointer}"))
}

fn capture_turn_messages(value: &Value, last_text: &mut String) {
    let Some(items) = value.pointer("/turn/items").and_then(Value::as_array) else {
        return;
    };
    for item in items {
        capture_agent_message(item, last_text);
    }
}

fn capture_agent_message(item: &Value, last_text: &mut String) {
    if item.get("type").and_then(Value::as_str) == Some("agentMessage") {
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            *last_text = text.to_string();
        }
    }
}

fn completed_turn(
    status: &str,
    last_text: String,
    error: Option<String>,
    kind: Option<String>,
) -> anyhow::Result<String> {
    if status != "completed" {
        return Err(TurnFailed {
            status: status.to_string(),
            kind,
            message: error.unwrap_or_else(|| "no error detail".to_string()),
        }
        .into());
    }
    let response = last_text.trim().to_string();
    if response.is_empty() {
        return Err(anyhow::anyhow!("Codex produced no agentMessage"));
    }
    Ok(response)
}

fn rpc_error(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| error.to_string())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct CodexInvoker {
    config: CodexConfig,
    system_prompt: String,
    run_dir: RunDir,
    app_server: CodexAppServer,
    label: String,
}

impl CodexInvoker {
    pub fn new(
        config: &CodexConfig,
        system_prompt: String,
        app_server: CodexAppServer,
        label: &str,
    ) -> anyhow::Result<Self> {
        info!(
            "Codex invoker ready (model={}, effort={}, fallback={})",
            config.model,
            config.reasoning_effort,
            config.fallback().unwrap_or("off")
        );
        Ok(Self {
            config: config.clone(),
            system_prompt,
            run_dir: RunDir::create()?,
            app_server,
            label: label.to_string(),
        })
    }
}

#[cfg(windows)]
fn codex_command() -> Command {
    use std::path::PathBuf;
    use std::sync::OnceLock;

    static PROGRAM: OnceLock<(PathBuf, Option<PathBuf>)> = OnceLock::new();

    let (program, entrypoint) = PROGRAM.get_or_init(|| {
        let path = std::env::var_os("PATH").unwrap_or_default();

        if let Some(executable) = std::env::split_paths(&path)
            .map(|directory| directory.join("codex.exe"))
            .find(|candidate| candidate.is_file())
        {
            return (executable, None);
        }

        for directory in std::env::split_paths(&path) {
            let entrypoint = directory.join("node_modules/@openai/codex/bin/codex.js");
            if !entrypoint.is_file() {
                continue;
            }
            let bundled_node = directory.join("node.exe");
            let node = if bundled_node.is_file() {
                bundled_node
            } else {
                PathBuf::from("node.exe")
            };
            return (node, Some(entrypoint));
        }

        (PathBuf::from("codex.exe"), None)
    });

    let mut command = Command::new(program);
    if let Some(entrypoint) = entrypoint {
        command.arg(entrypoint);
    }
    command
}

#[cfg(not(windows))]
fn codex_command() -> Command {
    Command::new("codex")
}

#[async_trait]
impl LlmBackend for CodexInvoker {
    async fn send_message(&self, content: &str) -> anyhow::Result<String> {
        debug!(">>> TO CODEX ({} bytes):\n{}", content.len(), content);
        let prompt = format!("{}\n\n{}", self.system_prompt, content);
        let response = self
            .app_server
            .send_turn(&self.config, self.run_dir.path(), &self.label, &prompt)
            .await?;
        debug!("<<< FROM CODEX ({} bytes):\n{}", response.len(), response);
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, split};

    fn test_session(stream: tokio::io::DuplexStream) -> Arc<AppServerSession> {
        let (output, input) = split(stream);
        let session = Arc::new(AppServerSession {
            input: AsyncMutex::new(Box::new(input)),
            pending: Mutex::new(HashMap::new()),
            turns: Mutex::new(HashMap::new()),
            rate_limits: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            dead: AtomicBool::new(false),
            process: Mutex::new(None),
            _run_dir: None,
        });
        tokio::spawn(read_stdout(Arc::downgrade(&session), output));
        session
    }

    fn spec(config: &CodexConfig) -> TurnSpec<'_> {
        TurnSpec {
            config,
            model: &config.model,
            cwd: std::path::Path::new("/tmp"),
            label: "npc_test",
        }
    }

    type Requests = tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>;
    type Output = tokio::io::WriteHalf<tokio::io::DuplexStream>;

    async fn read_request(lines: &mut Requests) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(1), lines.next_line())
            .await
            .unwrap();
        serde_json::from_str(&line.unwrap().unwrap()).unwrap()
    }

    async fn expect_delete(requests: &mut Requests, output: &mut Output, thread_id: &str) {
        let delete = read_request(requests).await;
        assert_eq!(delete["method"], "thread/delete");
        assert_eq!(delete["params"]["threadId"], thread_id);
        write_message(output, json!({ "id": delete["id"].clone(), "result": {} })).await;
    }

    async fn write_message(output: &mut Output, message: Value) {
        output
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn runs_each_turn_in_a_fresh_thread_then_deletes_it() {
        let (client, server) = duplex(64 * 1024);
        let session = test_session(client);
        let (server_input, mut server_output) = split(server);
        let mut requests = BufReader::new(server_input).lines();

        let fake_server = tokio::spawn(async move {
            let thread = read_request(&mut requests).await;
            assert_eq!(thread["method"], "thread/start");
            assert_eq!(thread["params"]["ephemeral"], false);
            assert_eq!(thread["params"]["sandbox"], "read-only");
            assert_eq!(thread["params"]["approvalPolicy"], "never");
            write_message(
                &mut server_output,
                json!({
                    "id": thread["id"].clone(),
                    "result": { "thread": { "id": "thread-1" } }
                }),
            )
            .await;

            let turn = read_request(&mut requests).await;
            assert_eq!(turn["method"], "turn/start");
            assert_eq!(turn["params"]["threadId"], "thread-1");
            assert_eq!(turn["params"]["effort"], "medium");
            assert_eq!(turn["params"]["input"][0]["text"], "system\n\nevent");
            write_message(
                &mut server_output,
                json!({
                    "id": turn["id"].clone(),
                    "result": {
                        "turn": { "id": "turn-1", "status": "inProgress", "items": [] }
                    }
                }),
            )
            .await;
            write_message(
                &mut server_output,
                json!({
                    "method": "item/completed",
                    "params": {
                        "threadId": "thread-1",
                        "turnId": "turn-1",
                        "item": { "id": "item-1", "type": "agentMessage", "text": " answer " }
                    }
                }),
            )
            .await;
            write_message(
                &mut server_output,
                json!({
                    "method": "turn/completed",
                    "params": {
                        "threadId": "thread-1",
                        "turn": { "id": "turn-1", "status": "completed", "items": [] }
                    }
                }),
            )
            .await;

            expect_delete(&mut requests, &mut server_output, "thread-1").await;
        });

        let config = CodexConfig::default();
        let response = session
            .run_turn(spec(&config), "system\n\nevent", None)
            .await
            .unwrap();
        assert_eq!(response, "answer");
        fake_server.await.unwrap();
    }

    #[tokio::test]
    async fn failed_turn_start_releases_its_thread() {
        let (client, server) = duplex(16 * 1024);
        let session = test_session(client);
        let (server_input, mut server_output) = split(server);
        let mut requests = BufReader::new(server_input).lines();

        let fake_server = tokio::spawn(async move {
            let thread = read_request(&mut requests).await;
            write_message(
                &mut server_output,
                json!({
                    "id": thread["id"].clone(),
                    "result": { "thread": { "id": "thread-failed" } }
                }),
            )
            .await;

            let turn = read_request(&mut requests).await;
            write_message(
                &mut server_output,
                json!({
                    "id": turn["id"].clone(),
                    "error": { "message": "turn rejected" }
                }),
            )
            .await;

            expect_delete(&mut requests, &mut server_output, "thread-failed").await;
        });

        let config = CodexConfig::default();
        let error = session
            .run_turn(spec(&config), "event", None)
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Codex app-server turn/start failed: turn rejected"
        );
        fake_server.await.unwrap();
        assert!(lock(&session.turns).is_empty());
    }

    #[tokio::test]
    async fn dropping_a_turn_interrupts_only_that_turn() {
        let (client, server) = duplex(16 * 1024);
        let session = test_session(client);
        let (server_input, mut server_output) = split(server);
        let mut requests = BufReader::new(server_input).lines();
        let (events, _) = mpsc::unbounded_channel();
        let route = Arc::new(TurnRoute {
            events,
            turn_id: Mutex::new(Some("turn-9".to_string())),
            turn_ready: Notify::new(),
        });
        lock(&session.turns).insert("thread-9".to_string(), Arc::clone(&route));

        drop(TurnGuard {
            session: Arc::clone(&session),
            thread_id: "thread-9".to_string(),
            route,
            completed: false,
        });

        let interrupt = read_request(&mut requests).await;
        assert_eq!(interrupt["method"], "turn/interrupt");
        assert_eq!(interrupt["params"]["threadId"], "thread-9");
        assert_eq!(interrupt["params"]["turnId"], "turn-9");
        write_message(
            &mut server_output,
            json!({ "id": interrupt["id"].clone(), "result": {} }),
        )
        .await;

        expect_delete(&mut requests, &mut server_output, "thread-9").await;
    }

    #[test]
    fn reports_failed_turn_details() {
        let error = completed_turn(
            "failed",
            String::new(),
            Some("rate limited".to_string()),
            None,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Codex turn failed: rate limited");
    }

    enum Act {
        Answer(&'static str),
        AnswerAfter(Duration, &'static str),
        Overloaded,
        Fail(&'static str),
        Hang,
    }

    async fn turn_completed(
        output: &mut Output,
        thread: &str,
        turn: &str,
        status: &str,
        error: Value,
    ) {
        write_message(
            output,
            json!({
                "method": "turn/completed",
                "params": {
                    "threadId": thread,
                    "turn": { "id": turn, "status": status, "items": [], "error": error }
                }
            }),
        )
        .await;
    }

    async fn answer(output: &mut Output, thread: &str, turn: &str, text: &str) {
        write_message(
            output,
            json!({
                "method": "item/completed",
                "params": {
                    "threadId": thread,
                    "turnId": turn,
                    "item": { "id": "item", "type": "agentMessage", "text": text }
                }
            }),
        )
        .await;
        turn_completed(output, thread, turn, "completed", Value::Null).await;
    }

    async fn play(act: Act, output: Arc<AsyncMutex<Output>>, thread: String, turn: String) {
        let mut out = output.lock().await;
        match act {
            Act::Answer(text) => answer(&mut out, &thread, &turn, text).await,
            Act::AnswerAfter(delay, text) => {
                drop(out);
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    answer(&mut *output.lock().await, &thread, &turn, text).await;
                });
            }
            Act::Overloaded => {
                let error = json!({
                    "message": "Selected model is at capacity. Please try a different model.",
                    "codexErrorInfo": "serverOverloaded"
                });
                let notice = json!({
                    "method": "error",
                    "params": { "threadId": thread, "turnId": turn, "willRetry": true, "error": error }
                });
                write_message(&mut out, notice).await;
            }
            Act::Fail(kind) => {
                let error =
                    json!({ "message": format!("{kind} from the fake"), "codexErrorInfo": kind });
                turn_completed(&mut out, &thread, &turn, "failed", error).await;
            }
            Act::Hang => {}
        }
    }

    /// A session whose app-server plays each model's acts in turn order (then
    /// hangs), reporting every request it reads.
    fn scripted(
        script: Vec<(&'static str, Vec<Act>)>,
    ) -> (Arc<AppServerSession>, mpsc::UnboundedReceiver<Value>) {
        let (client, server) = duplex(64 * 1024);
        let (seen_tx, seen) = mpsc::unbounded_channel();
        let mut script: HashMap<String, std::collections::VecDeque<Act>> = script
            .into_iter()
            .map(|(model, acts)| (model.to_string(), acts.into()))
            .collect();
        let (input, output) = split(server);
        let output = Arc::new(AsyncMutex::new(output));
        tokio::spawn(async move {
            let mut requests = BufReader::new(input).lines();
            let mut models = HashMap::new();
            let mut threads = 0;
            while let Ok(Some(line)) = requests.next_line().await {
                let request: Value = serde_json::from_str(&line).unwrap();
                let _ = seen_tx.send(request.clone());
                let id = request["id"].clone();
                let params = &request["params"];
                let reply = match request["method"].as_str().unwrap_or_default() {
                    "thread/start" => {
                        threads += 1;
                        let thread = format!("thread-{threads}");
                        models.insert(
                            thread.clone(),
                            params["model"].as_str().unwrap().to_string(),
                        );
                        json!({ "thread": { "id": thread } })
                    }
                    "turn/start" => {
                        let thread = params["threadId"].as_str().unwrap().to_string();
                        let turn = format!("turn-{thread}");
                        let act = script
                            .get_mut(&models[&thread])
                            .and_then(|acts| acts.pop_front())
                            .unwrap_or(Act::Hang);
                        let reply =
                            json!({ "turn": { "id": turn, "status": "inProgress", "items": [] } });
                        write_message(
                            &mut *output.lock().await,
                            json!({ "id": id, "result": reply }),
                        )
                        .await;
                        play(act, Arc::clone(&output), thread, turn).await;
                        continue;
                    }
                    _ => json!({}),
                };
                write_message(
                    &mut *output.lock().await,
                    json!({ "id": id, "result": reply }),
                )
                .await;
            }
        });
        (test_session(client), seen)
    }

    /// Models of every thread started so far, in order.
    fn started_models(seen: &mut mpsc::UnboundedReceiver<Value>) -> Vec<String> {
        std::iter::from_fn(|| seen.try_recv().ok())
            .filter(|request| request["method"] == "thread/start")
            .map(|request| request["params"]["model"].as_str().unwrap().to_string())
            .collect()
    }

    const PRIMARY: &str = "gpt-5.6-luna";
    const FALLBACK: &str = "gpt-6-luna";

    async fn send(
        session: &Arc<AppServerSession>,
        window: &FallbackWindow,
        config: &CodexConfig,
    ) -> anyhow::Result<String> {
        send_with_fallback(session, window, spec(config), "event").await
    }

    #[tokio::test(start_paused = true)]
    async fn an_overload_notice_hands_the_turn_to_the_fallback() {
        let (session, mut seen) = scripted(vec![
            (PRIMARY, vec![Act::Overloaded]),
            (FALLBACK, vec![Act::Answer("from six")]),
        ]);
        let window = FallbackWindow::default();
        let config = CodexConfig::default();
        let started = Instant::now();

        assert_eq!(send(&session, &window, &config).await.unwrap(), "from six");
        assert!(started.elapsed() < Duration::from_secs(config.fallback_after_secs));
        assert_eq!(window.state(), Window::Open);

        let mut fallback_effort = None;
        let interrupted = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let request = seen.recv().await.unwrap();
                if request["method"] == "turn/start" && request["params"]["model"] == FALLBACK {
                    fallback_effort = Some(request["params"]["effort"].clone());
                }
                if request["method"] == "turn/interrupt" {
                    return request["params"]["threadId"].clone();
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(interrupted, "thread-1");
        assert_eq!(fallback_effort, Some(json!("medium")));
    }

    #[tokio::test(start_paused = true)]
    async fn a_silent_primary_races_the_fallback_after_the_deadline() {
        let (session, _seen) = scripted(vec![
            (PRIMARY, vec![Act::Hang]),
            (FALLBACK, vec![Act::Answer("rescued")]),
        ]);
        let started = Instant::now();

        let config = CodexConfig::default();
        let answer = send(&session, &FallbackWindow::default(), &config).await;

        assert_eq!(answer.unwrap(), "rescued");
        assert!(started.elapsed() >= Duration::from_secs(config.fallback_after_secs));
    }

    #[tokio::test(start_paused = true)]
    async fn the_window_keeps_turns_on_the_fallback_until_one_probe_succeeds() {
        let (session, mut seen) = scripted(vec![
            (
                PRIMARY,
                vec![
                    Act::Hang,
                    Act::AnswerAfter(Duration::from_secs(1), "primary again"),
                ],
            ),
            (
                FALLBACK,
                vec![Act::Answer("one"), Act::Answer("two"), Act::Answer("held")],
            ),
        ]);
        let window = FallbackWindow::default();
        let config = CodexConfig::default();

        assert_eq!(send(&session, &window, &config).await.unwrap(), "one");
        assert_eq!(send(&session, &window, &config).await.unwrap(), "two");
        tokio::time::advance(Duration::from_secs(config.fallback_cooldown_secs)).await;
        assert_eq!(window.state(), Window::Expired);
        let (probe, held) = tokio::join!(
            send(&session, &window, &config),
            send(&session, &window, &config)
        );

        assert_eq!(probe.unwrap(), "primary again");
        assert_eq!(held.unwrap(), "held");
        assert_eq!(
            started_models(&mut seen),
            [PRIMARY, FALLBACK, FALLBACK, PRIMARY, FALLBACK]
        );
        assert_eq!(window.state(), Window::Closed);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_fallback_still_waits_for_the_primary() {
        let (session, _seen) = scripted(vec![
            (
                PRIMARY,
                vec![Act::AnswerAfter(Duration::from_secs(45), "slow but here")],
            ),
            (FALLBACK, vec![Act::Fail("badRequest")]),
        ]);
        let window = FallbackWindow::default();

        let answer = send(&session, &window, &CodexConfig::default()).await;

        assert_eq!(answer.unwrap(), "slow but here");
        assert_eq!(window.state(), Window::Closed);
    }

    #[tokio::test]
    async fn an_overloaded_failure_falls_back_but_a_usage_limit_does_not() {
        let (session, mut seen) = scripted(vec![
            (
                PRIMARY,
                vec![
                    Act::Fail("serverOverloaded"),
                    Act::Fail("usageLimitExceeded"),
                ],
            ),
            (FALLBACK, vec![Act::Answer("covered")]),
        ]);
        let config = CodexConfig::default();

        let window = FallbackWindow::default();
        assert_eq!(send(&session, &window, &config).await.unwrap(), "covered");
        let error = send(&session, &FallbackWindow::default(), &config)
            .await
            .unwrap_err();

        let failed = error.downcast_ref::<TurnFailed>().unwrap();
        assert_eq!(failed.kind.as_deref(), Some("usageLimitExceeded"));
        assert_eq!(started_models(&mut seen), [PRIMARY, FALLBACK, PRIMARY]);
    }

    #[tokio::test]
    async fn no_fallback_model_runs_only_the_primary() {
        let (session, mut seen) = scripted(vec![(PRIMARY, vec![Act::Fail("serverOverloaded")])]);
        let config = CodexConfig {
            fallback_model: String::new(),
            ..CodexConfig::default()
        };

        assert!(send(&session, &FallbackWindow::default(), &config)
            .await
            .is_err());
        assert_eq!(started_models(&mut seen), [PRIMARY]);
    }
}

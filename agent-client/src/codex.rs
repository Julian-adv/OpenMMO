use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::driver::LlmBackend;

/// Configuration for the Codex CLI integration.
#[derive(Debug, Clone, Deserialize)]
pub struct CodexConfig {
    /// Model to use (default: "o4-mini")
    #[serde(default = "default_model")]
    pub model: String,
    /// Path to system prompt file (default: "data/system_prompt.txt")
    #[serde(default = "default_system_prompt_file")]
    pub system_prompt_file: String,
}

fn default_model() -> String {
    "o4-mini".to_string()
}
fn default_system_prompt_file() -> String {
    "data/system_prompt.txt".to_string()
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            system_prompt_file: default_system_prompt_file(),
        }
    }
}

/// JSONL event from `codex exec --json`.
/// Events have varying shapes; we extract text from `item.completed` events.
#[derive(Debug, Deserialize)]
struct CodexEvent {
    #[serde(rename = "type")]
    event_type: String,
    /// Present on "item.completed" events
    item: Option<CodexItem>,
}

#[derive(Debug, Deserialize)]
struct CodexItem {
    #[serde(rename = "type")]
    item_type: Option<String>,
    /// Text content for agent_message items
    text: Option<String>,
}

/// Invokes `codex exec` per prompt in full-auto JSONL mode.
/// Prompt is piped via stdin (using `-` argument).
pub struct CodexInvoker {
    config: CodexConfig,
    system_prompt: String,
}

impl CodexInvoker {
    pub fn new(config: &CodexConfig, system_prompt: String) -> anyhow::Result<Self> {
        info!("Codex invoker ready (model={})", config.model);
        Ok(Self {
            config: config.clone(),
            system_prompt,
        })
    }
}

#[async_trait]
impl LlmBackend for CodexInvoker {
    async fn send_message(&self, content: &str) -> anyhow::Result<String> {
        info!(">>> TO CODEX ({} bytes):\n{}", content.len(), content);

        // Build the full prompt: system prompt + user content
        let full_prompt = format!("{}\n\n{}", self.system_prompt, content);

        let mut cmd = Command::new("codex");
        cmd.arg("exec")
            .arg("--full-auto")
            .arg("--sandbox")
            .arg("read-only")
            .arg("--json")
            .arg("--ephemeral")
            .arg("--skip-git-repo-check")
            .arg("-m")
            .arg(&self.config.model)
            .arg("-") // read prompt from stdin
            .current_dir(std::env::temp_dir()) // avoid picking up AGENTS.md
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn codex CLI: {e}"))?;

        // Write prompt to stdin then close it
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(full_prompt.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture codex stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture codex stderr"))?;

        // Log stderr in background
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!(target: "codex_stderr", "{}", line);
            }
        });

        // Read JSONL lines and extract agent_message text from item.completed events
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut last_text = String::new();

        while let Ok(Some(line)) = lines.next_line().await {
            debug!(target: "codex_stdout", "{}", line);
            if let Ok(event) = serde_json::from_str::<CodexEvent>(&line) {
                if event.event_type == "item.completed" {
                    if let Some(item) = &event.item {
                        if item.item_type.as_deref() == Some("agent_message") {
                            if let Some(text) = &item.text {
                                last_text = text.clone();
                            }
                        }
                    }
                }
            }
        }

        // Wait for process to finish
        let status = child.wait().await?;
        if !status.success() {
            warn!("Codex process exited with status: {status}");
        }

        let response = last_text.trim().to_string();
        info!("<<< FROM CODEX ({} bytes):\n{}", response.len(), response);
        Ok(response)
    }
}

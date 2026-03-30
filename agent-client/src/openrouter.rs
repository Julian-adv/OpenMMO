use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::driver::LlmBackend;

/// Configuration for OpenRouter API integration.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenRouterConfig {
    /// OpenRouter API key (can also be set via OPENROUTER_API_KEY env var)
    #[serde(default)]
    pub api_key: String,
    /// Model identifier (e.g. "google/gemini-2.0-flash-001", "meta-llama/llama-3-70b-instruct")
    #[serde(default = "default_openrouter_model")]
    pub model: String,
    /// Path to system prompt file (default: "data/system_prompt.txt")
    #[serde(default = "default_system_prompt_file")]
    pub system_prompt_file: String,
    /// Max tokens for the response (default: 1024)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Temperature (default: 0.7)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_openrouter_model() -> String {
    "openrouter/hunter-alpha".to_string()
}
fn default_system_prompt_file() -> String {
    "data/system_prompt.txt".to_string()
}
fn default_max_tokens() -> u32 {
    1024
}
fn default_temperature() -> f32 {
    0.7
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_openrouter_model(),
            system_prompt_file: default_system_prompt_file(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
        }
    }
}

// --- OpenAI-compatible API types ---

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// Invokes LLMs via the OpenRouter API (OpenAI-compatible chat completions).
/// Maintains conversation history for multi-turn context.
pub struct OpenRouterInvoker {
    client: Client,
    config: OpenRouterConfig,
    system_prompt: String,
    api_key: String,
    messages: Mutex<Vec<ChatMessage>>,
}

impl OpenRouterInvoker {
    pub fn new(config: &OpenRouterConfig, system_prompt: String) -> anyhow::Result<Self> {
        // Resolve API key: config value takes precedence, then env var
        let api_key = if !config.api_key.is_empty() {
            config.api_key.clone()
        } else {
            std::env::var("OPENROUTER_API_KEY")
                .map_err(|_| anyhow::anyhow!(
                    "OpenRouter API key not set. Set openrouter.api_key in config or OPENROUTER_API_KEY env var"
                ))?
        };

        info!("OpenRouter invoker ready (model={})", config.model);

        Ok(Self {
            client: Client::new(),
            config: config.clone(),
            system_prompt,
            api_key,
            messages: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl LlmBackend for OpenRouterInvoker {
    async fn send_message(&self, content: &str) -> anyhow::Result<String> {
        info!(">>> TO OPENROUTER ({} bytes):\n{}", content.len(), content);

        let mut messages = self.messages.lock().await;

        // Add system prompt on first message
        if messages.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: self.system_prompt.clone(),
            });
        }

        // Add user message
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: content.to_string(),
        });

        // Trim conversation history if it gets too long (keep system + last 20 turns)
        const MAX_MESSAGES: usize = 41; // system + 20 user/assistant pairs
        if messages.len() > MAX_MESSAGES {
            let system = messages[0].clone();
            let keep_from = messages.len() - (MAX_MESSAGES - 1);
            *messages = std::iter::once(system)
                .chain(messages[keep_from..].iter().cloned())
                .collect();
            warn!(
                "OpenRouter: trimmed conversation history to {} messages",
                messages.len()
            );
        }

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: messages.clone(),
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
        };

        let response = self
            .client
            .post(OPENROUTER_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("OpenRouter API request failed: {e}"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read OpenRouter response body: {e}"))?;

        if !status.is_success() {
            anyhow::bail!("OpenRouter API error (HTTP {status}): {body}");
        }

        let chat_response: ChatResponse = serde_json::from_str(&body).map_err(|e| {
            anyhow::anyhow!("Failed to parse OpenRouter response: {e}\nRaw: {body}")
        })?;

        let result = chat_response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        // Store assistant response in conversation history
        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: result.clone(),
        });

        info!("<<< FROM OPENROUTER ({} bytes):\n{}", result.len(), result);
        Ok(result)
    }
}

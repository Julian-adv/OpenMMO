//! Google sign-in for a headless client, via the OAuth 2.0 device flow
//! ("TV and Limited-Input Devices"). The browser flow the web client uses
//! needs a browser on the same machine; this one prints a URL and a code the
//! runner can enter from any device.
//!
//! The refresh token is kept on disk so restarts do not re-prompt, and a
//! fresh ID token is minted per connection — they last an hour, the agent
//! runs for days. See `doc/REMOTE_AGENT_CLIENT.md`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};

const DEVICE_CODE_URL: &str = "https://oauth2.googleapis.com/device/code";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// `openid` gives the `sub` the account is keyed by; `email` feeds the
/// server's admin allowlist check.
const SCOPE: &str = "openid email";

/// OAuth client for headless sign-in. Registered in the same Google Cloud
/// project as the web client, as a separate "TV and Limited Input" client —
/// hence a different `aud`, which the server has to accept explicitly.
pub const DEFAULT_CLIENT_ID: &str =
    "73507098079-cssj1h0eir5aj11d5hs81o9k7e466i55.apps.googleusercontent.com";

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleAuthConfig {
    #[serde(default = "default_client_id")]
    pub client_id: String,
    /// Installed-app secret. Not confidential (RFC 8252 §8.5) but Google
    /// still wants it in the token exchange. Falls back to
    /// `GOOGLE_CLI_CLIENT_SECRET`.
    pub client_secret: Option<String>,
    /// Where the refresh token is cached; defaults to a per-user config dir.
    pub token_cache: Option<String>,
}

fn default_client_id() -> String {
    DEFAULT_CLIENT_ID.to_string()
}

impl Default for GoogleAuthConfig {
    fn default() -> Self {
        Self {
            client_id: default_client_id(),
            client_secret: None,
            token_cache: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    expires_in: u64,
    #[serde(default = "default_poll_interval")]
    interval: u64,
}

fn default_poll_interval() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Cached credential. `client_id` is stored alongside so pointing the config
/// at a different OAuth client re-prompts instead of failing on refresh.
#[derive(Debug, Serialize, Deserialize)]
struct CachedToken {
    client_id: String,
    refresh_token: String,
}

pub struct GoogleAuth {
    config: GoogleAuthConfig,
    client_secret: Option<String>,
    cache_path: PathBuf,
    refresh_token: Mutex<String>,
    http: reqwest::Client,
}

impl GoogleAuth {
    /// Reuse the cached refresh token, or run the device flow — which blocks
    /// on the person completing sign-in in a browser.
    pub async fn sign_in(config: GoogleAuthConfig) -> anyhow::Result<Self> {
        let client_secret = config
            .client_secret
            .clone()
            .or_else(|| std::env::var("GOOGLE_CLI_CLIENT_SECRET").ok());
        let cache_path = resolve_cache_path(config.token_cache.as_deref());
        let http = reqwest::Client::new();

        let cached = read_cache(&cache_path)
            .await
            .filter(|c| c.client_id == config.client_id);

        let refresh_token = match cached {
            Some(cached) => {
                info!(
                    "Google sign-in: reusing cached credential ({:?})",
                    cache_path
                );
                cached.refresh_token
            }
            None => {
                let token =
                    run_device_flow(&http, &config.client_id, client_secret.as_deref()).await?;
                write_cache(
                    &cache_path,
                    &CachedToken {
                        client_id: config.client_id.clone(),
                        refresh_token: token.clone(),
                    },
                )
                .await?;
                info!("Google sign-in complete; credential cached at {cache_path:?}");
                token
            }
        };

        Ok(Self {
            config,
            client_secret,
            cache_path,
            refresh_token: Mutex::new(refresh_token),
            http,
        })
    }

    /// A freshly minted ID token for `ClientMessage::Authenticate`.
    pub async fn id_token(&self) -> anyhow::Result<String> {
        let refresh_token = self.refresh_token.lock().await.clone();
        let mut form = vec![
            ("client_id", self.config.client_id.clone()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token".to_string()),
        ];
        if let Some(secret) = &self.client_secret {
            form.push(("client_secret", secret.clone()));
        }

        let response: TokenResponse = self
            .http
            .post(TOKEN_URL)
            .form(&form)
            .send()
            .await?
            .json()
            .await?;

        if let Some(error) = response.error {
            // A revoked or expired grant cannot be refreshed; drop the cache
            // so the next start prompts instead of looping on a dead token.
            if error == "invalid_grant" {
                warn!(
                    "Cached Google credential rejected; removing {:?}",
                    self.cache_path
                );
                let _ = tokio::fs::remove_file(&self.cache_path).await;
            }
            anyhow::bail!(
                "Google token refresh failed: {error}{}",
                describe(response.error_description)
            );
        }
        response
            .id_token
            .ok_or_else(|| anyhow::anyhow!("Google token refresh returned no id_token"))
    }
}

/// Prompt-and-poll half of the device flow. Returns the refresh token.
async fn run_device_flow(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: Option<&str>,
) -> anyhow::Result<String> {
    let device: DeviceCodeResponse = http
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await?
        .error_for_status()
        .map_err(|e| {
            anyhow::anyhow!(
                "Google refused the device-code request ({e}). Is {client_id} an OAuth client \
                 of type \"TV and Limited Input devices\"?"
            )
        })?
        .json()
        .await?;

    // Printed rather than logged: this is the one thing the runner must act on.
    println!();
    println!("  Sign in to continue:");
    println!("    1. open {}", device.verification_url);
    println!("    2. enter code {}", device.user_code);
    println!("  (expires in {} minutes)", device.expires_in / 60);
    println!();

    let mut interval = Duration::from_secs(device.interval);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(device.expires_in);

    loop {
        tokio::time::sleep(interval).await;
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("Google sign-in timed out — run again to get a fresh code");
        }

        let mut form = vec![
            ("client_id", client_id.to_string()),
            ("device_code", device.device_code.clone()),
            ("grant_type", DEVICE_GRANT.to_string()),
        ];
        if let Some(secret) = client_secret {
            form.push(("client_secret", secret.to_string()));
        }

        let response: TokenResponse = http
            .post(TOKEN_URL)
            .form(&form)
            .send()
            .await?
            .json()
            .await?;

        match response.error.as_deref() {
            None => {
                return response
                    .refresh_token
                    .ok_or_else(|| anyhow::anyhow!("Google sign-in returned no refresh_token"))
            }
            Some("authorization_pending") => continue,
            // Google asks for backoff by bumping the interval, not by failing.
            Some("slow_down") => interval += Duration::from_secs(5),
            Some(error) => anyhow::bail!(
                "Google sign-in failed: {error}{}{}",
                describe(response.error_description),
                secret_hint(client_secret)
            ),
        }
    }
}

/// Google's device flow wants the installed-app secret in the exchange even
/// though it is not confidential, and v0.2.0 packages shipped a truncated one.
fn secret_hint(client_secret: Option<&str>) -> &'static str {
    match client_secret {
        None => "\n  Set [auth] client_secret in data/config.toml, or GOOGLE_CLI_CLIENT_SECRET",
        Some(secret) if !secret.starts_with("GOCSPX-") => {
            "\n  client_secret in data/config.toml looks truncated — Google's start with GOCSPX-"
        }
        Some(_) => "",
    }
}

fn describe(error_description: Option<String>) -> String {
    error_description
        .map(|d| format!(" ({d})"))
        .unwrap_or_default()
}

fn resolve_cache_path(configured: Option<&str>) -> PathBuf {
    if let Some(path) = configured {
        return expand_home(path);
    }

    #[cfg(windows)]
    if let Some(app_data) = std::env::var_os("APPDATA") {
        return PathBuf::from(app_data).join("onlinerpg/google.json");
    }

    home_dir()
        .map(|home| home.join(".config/onlinerpg/google.json"))
        .unwrap_or_else(|| PathBuf::from("data/google.json"))
}

fn expand_home(path: &str) -> PathBuf {
    let rest = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"));
    match (rest, home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

fn home_dir() -> Option<PathBuf> {
    fn non_empty(name: &str) -> Option<std::ffi::OsString> {
        std::env::var_os(name).filter(|value| !value.is_empty())
    }

    let home = non_empty("HOME");
    #[cfg(windows)]
    let home = home.or_else(|| non_empty("USERPROFILE"));
    home.map(PathBuf::from)
}

async fn read_cache(path: &Path) -> Option<CachedToken> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    match serde_json::from_str(&text) {
        Ok(token) => Some(token),
        Err(e) => {
            warn!("Ignoring unreadable Google credential cache {path:?}: {e}");
            None
        }
    }
}

async fn write_cache(path: &Path, token: &CachedToken) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, serde_json::to_vec_pretty(token)?).await?;
    // Long-lived credential: owner-only, like the server's NPC token file.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_prefers_config_over_home() {
        assert_eq!(
            resolve_cache_path(Some("/tmp/creds.json")),
            PathBuf::from("/tmp/creds.json")
        );
    }

    #[test]
    fn cache_path_expands_leading_tilde() {
        let home = home_dir().expect("home directory set in test env");
        assert_eq!(
            resolve_cache_path(Some("~/creds.json")),
            home.join("creds.json")
        );
    }

    #[test]
    fn default_cache_path_is_absolute_and_per_user() {
        let path = resolve_cache_path(None);
        assert!(path.is_absolute(), "{path:?}");
        assert!(path.ends_with("onlinerpg/google.json"), "{path:?}");
    }
}

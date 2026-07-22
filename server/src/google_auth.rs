use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const JWKS_TTL: Duration = Duration::from_secs(3600);
/// Floor between JWKS refetches so bad tokens can't hammer Google.
const JWKS_MIN_REFRESH: Duration = Duration::from_secs(300);

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

/// `sub` identifies the account. Email is parsed only to match against the
/// REST-write admin allowlist; neither is stored.
#[derive(Debug, Deserialize)]
pub struct GoogleClaims {
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
}

pub struct GoogleAuthVerifier {
    /// Accepted `aud` values. The browser and the CLI sign in through
    /// different OAuth clients (a headless client cannot use the browser
    /// one), so tokens from either are valid here.
    client_ids: Vec<String>,
    http: reqwest::Client,
    jwks: RwLock<Option<(Instant, Jwks)>>,
}

impl GoogleAuthVerifier {
    pub fn new(client_ids: Vec<String>) -> Self {
        Self {
            client_ids,
            http: reqwest::Client::new(),
            jwks: RwLock::new(None),
        }
    }

    pub async fn verify(&self, id_token: &str) -> Result<GoogleClaims, String> {
        let header = decode_header(id_token).map_err(|e| format!("bad token header: {e}"))?;
        let kid = header.kid.ok_or("token has no key id")?;
        let key = self.decoding_key(&kid).await?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&self.client_ids);
        validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);

        decode::<GoogleClaims>(id_token, &key, &validation)
            .map(|data| data.claims)
            .map_err(|e| format!("token verification failed: {e}"))
    }

    async fn decoding_key(&self, kid: &str) -> Result<DecodingKey, String> {
        if let Some(key) = self.key_from_cache(kid, JWKS_TTL).await {
            return Ok(key);
        }

        {
            let guard = self.jwks.read().await;
            if let Some((fetched_at, _)) = guard.as_ref() {
                if fetched_at.elapsed() < JWKS_MIN_REFRESH {
                    return Err("unknown signing key".to_string());
                }
            }
        }

        let jwks: Jwks = self
            .http
            .get(JWKS_URL)
            .send()
            .await
            .map_err(|e| format!("JWKS fetch failed: {e}"))?
            .json()
            .await
            .map_err(|e| format!("JWKS parse failed: {e}"))?;
        *self.jwks.write().await = Some((Instant::now(), jwks));

        self.key_from_cache(kid, JWKS_TTL)
            .await
            .ok_or_else(|| "unknown signing key".to_string())
    }

    async fn key_from_cache(&self, kid: &str, max_age: Duration) -> Option<DecodingKey> {
        let guard = self.jwks.read().await;
        let (fetched_at, jwks) = guard.as_ref()?;
        if fetched_at.elapsed() > max_age {
            return None;
        }
        let jwk = jwks.keys.iter().find(|k| k.kid == kid)?;
        DecodingKey::from_rsa_components(&jwk.n, &jwk.e).ok()
    }
}

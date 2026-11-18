use crate::google_auth::GoogleAuthVerifier;

/// Credential checkers shared by every connection and the REST API.
pub struct AuthContext {
    /// None when the server was started without a Google client id; browser
    /// logins are rejected until it is configured.
    pub google: Option<GoogleAuthVerifier>,
    pub npc_token: String,
    /// Google account emails allowed to administer the game and view metrics.
    pub admin_emails: Vec<String>,
}

impl AuthContext {
    /// Verified-email allowlist for admin APIs and game commands.
    pub fn is_admin(&self, claims: &crate::google_auth::GoogleClaims) -> bool {
        claims.email_verified == Some(true)
            && claims
                .email
                .as_deref()
                .is_some_and(|email| self.is_admin_email(email))
    }

    pub fn is_admin_email(&self, email: &str) -> bool {
        self.admin_emails
            .iter()
            .any(|a| a.eq_ignore_ascii_case(email))
    }
}

/// Debug builds only: lets local testing skip Google.
#[cfg(debug_assertions)]
pub(super) fn parse_dev_login<'a>(token: &'a str, npc_token: &str) -> Option<&'a str> {
    let (account, secret) = token.strip_prefix("dev:")?.split_once(':')?;
    token_matches(secret, &dev_login_secret(npc_token)).then_some(account)
}

/// A hash of the NPC token, so the dev client bundle never carries the token
/// itself (it also authorizes REST writes).
#[cfg(debug_assertions)]
pub(super) fn dev_login_secret(npc_token: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(format!("dev-login:{npc_token}")))
}

/// Constant-time equality so the NPC token can't be probed byte by byte.
pub fn token_matches(provided: &str, expected: &str) -> bool {
    provided.len() == expected.len()
        && provided
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

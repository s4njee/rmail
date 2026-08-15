//! OAuth2 PKCE and XOAUTH2 engine (Roadmap 3.1 / RFC 7636 / RFC 6749 / RFC 6750).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use url::Url;

/// The port the loopback OAuth capture prefers (registerable in provider
/// consoles). Falls back to an ephemeral port when it's already in use.
const PREFERRED_LOOPBACK_PORT: u16 = 8080;
/// How long to wait for the browser to complete the sign-in.
const OAUTH_WAIT_TIMEOUT: Duration = Duration::from_secs(90);

/// Live loopback listeners, keyed by their redirect URI, awaiting the browser
/// redirect. `get_oauth_init` binds one; `wait_for_code` consumes it.
static LOOPBACKS: Mutex<Option<HashMap<String, LoopbackServer>>> = Mutex::new(None);

struct LoopbackServer {
    server: tiny_http::Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthProvider {
    Google,
    Microsoft365,
}

impl OAuthProvider {
    /// Map an account's protocol string ("Google (OAuth2)", "Microsoft 365
    /// (OAuth2)") back to its provider for token refresh.
    pub fn from_protocol(protocol: &str) -> Option<OAuthProvider> {
        if protocol.contains("Google") {
            Some(OAuthProvider::Google)
        } else if protocol.contains("Microsoft") || protocol.contains("365") {
            Some(OAuthProvider::Microsoft365)
        } else {
            None
        }
    }

    pub fn auth_url(&self) -> &'static str {
        match self {
            Self::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Self::Microsoft365 => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        }
    }

    pub fn token_url(&self) -> &'static str {
        match self {
            Self::Google => "https://oauth2.googleapis.com/token",
            Self::Microsoft365 => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        }
    }

    pub fn default_scopes(&self) -> &'static str {
        match self {
            Self::Google => "https://mail.google.com/ https://www.googleapis.com/auth/calendar email profile",
            Self::Microsoft365 => "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access email openid profile",
        }
    }

    pub fn default_imap_host(&self) -> &'static str {
        match self {
            Self::Google => "imap.gmail.com",
            Self::Microsoft365 => "outlook.office365.com",
        }
    }

    pub fn default_smtp_host(&self) -> &'static str {
        match self {
            Self::Google => "smtp.gmail.com",
            Self::Microsoft365 => "smtp.office365.com",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub token_type: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    token_type: Option<String>,
    id_token: Option<String>,
}

/// Generates a high-entropy PKCE code verifier and SHA-256 code challenge.
pub fn generate_pkce_challenge() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    (verifier, challenge)
}

/// Bind a loopback HTTP listener on 127.0.0.1 and return its redirect URI
/// (`http://127.0.0.1:<port>`), so the browser sign-in redirect is captured
/// automatically instead of being pasted by hand. Tries the preferred
/// (registerable) port first, then an ephemeral one.
pub fn bind_loopback() -> Result<String, String> {
    let server = tiny_http::Server::http(("127.0.0.1", PREFERRED_LOOPBACK_PORT))
        .or_else(|_| tiny_http::Server::http(("127.0.0.1", 0)))
        .map_err(|e| format!("couldn't start the local sign-in listener: {e}"))?;
    let addr = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| "loopback listener has no IP address".to_string())?;
    let redirect = format!("http://{}", addr);
    let mut lock = LOOPBACKS.lock().map_err(|e| e.to_string())?;
    lock.get_or_insert_with(HashMap::new)
        .insert(redirect.clone(), LoopbackServer { server });
    Ok(redirect)
}

/// Block until the browser redirects back to `redirect_uri` with a `code` (and
/// the expected `state`), returning the authorization code. Consumes the
/// listener. Falls back to the paste-manually flow via a clear timeout error.
pub async fn wait_for_code(redirect_uri: &str, state: &str) -> Result<String, String> {
    let server = {
        let mut lock = LOOPBACKS.lock().map_err(|e| e.to_string())?;
        let map = lock
            .as_mut()
            .ok_or_else(|| "no OAuth sign-in listener running".to_string())?;
        map.remove(redirect_uri)
            .ok_or_else(|| format!("no OAuth sign-in listener for {redirect_uri}"))?
    };

    // tiny_http blocks, so the capture runs on a thread and the result comes
    // back over a oneshot — the async command stays non-blocking.
    let state = state.to_string();
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    std::thread::spawn(move || {
        let _ = tx.send(capture_code(server, &state));
    });

    match tokio::time::timeout(OAUTH_WAIT_TIMEOUT, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("the local sign-in listener stopped unexpectedly".into()),
        Err(_) => Err(
            "Timed out waiting for the browser sign-in. You can still paste the code \
             (or the redirect URL) from the browser's address bar."
                .into(),
        ),
    }
}

/// Serve the loopback until the redirect arrives. Each request gets a tiny
/// "you can close this window" page.
fn capture_code(server: LoopbackServer, state: &str) -> Result<String, String> {
    let server = server.server;
    let content_type = tiny_http::Header::from_bytes(
        &b"Content-Type"[..],
        &b"text/html; charset=utf-8"[..],
    )
    .expect("static header");
    let deadline = std::time::Instant::now() + OAUTH_WAIT_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for the browser sign-in".into());
        }
        match server.recv_timeout(remaining) {
            Ok(Some(request)) => {
                let params: HashMap<String, String> = Url::parse(&format!(
                    "http://127.0.0.1{}",
                    request.url()
                ))
                .ok()
                .map(|u| {
                    u.query_pairs()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect()
                })
                .unwrap_or_default();
                let _ = request.respond(
                    tiny_http::Response::from_string(CLOSE_WINDOW_HTML)
                        .with_header(content_type.clone()),
                );
                match (params.get("code"), params.get("state")) {
                    (Some(code), Some(returned)) if returned == state => return Ok(code.clone()),
                    (Some(_), Some(_)) => {
                        return Err("sign-in state mismatch — please start the sign-in again".into())
                    }
                    _ => continue, // a request without the code (e.g. a favicon)
                }
            }
            Ok(None) => continue,
            Err(e) => return Err(format!("loopback listener error: {e}")),
        }
    }
}

const CLOSE_WINDOW_HTML: &str = r#"<!doctype html><html><body style="font-family: system-ui; display:flex; align-items:center; justify-content:center; height:100vh; margin:0;">
<div style="text-align:center"><h2>Signed in — you can close this window</h2><p>Return to Quill to finish connecting your account.</p></div>
</body></html>"#;

/// Constructs the authorization URL for user browser sign-in.
pub fn build_auth_url(
    provider: OAuthProvider,
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
) -> Result<String, String> {
    let mut url = Url::parse(provider.auth_url()).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", provider.default_scopes())
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");

    Ok(url.to_string())
}

/// Exchanges an authorization code and PKCE verifier for access and refresh tokens.
pub async fn exchange_code_for_tokens(
    provider: OAuthProvider,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<OAuthTokens, String> {
    let client = reqwest::Client::new();
    // Google issues a client secret for OAuth clients and expects it at the
    // token endpoint (even for desktop-app clients, which PKCE alone doesn't
    // exempt); it's optional for public clients that genuinely have none.
    let mut params: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
    ];
    if let Some(secret) = client_secret {
        params.push(("client_secret", secret));
    }

    let resp = client
        .post(provider.token_url())
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("token exchange request failed: {e}"))?;

    if !resp.status().is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        return Err(format!("token exchange error: {err_text}"));
    }

    let token_data: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse token response: {e}"))?;

    // Attempt to parse email from id_token JWT if present
    let email = token_data
        .id_token
        .as_deref()
        .and_then(extract_email_from_jwt);

    Ok(OAuthTokens {
        access_token: token_data.access_token,
        refresh_token: token_data.refresh_token,
        expires_in: token_data.expires_in,
        token_type: token_data.token_type,
        email,
    })
}

/// Refreshes an expired access token using the stored refresh token.
pub async fn refresh_access_token(
    provider: OAuthProvider,
    refresh_token: &str,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<OAuthTokens, String> {
    let client = reqwest::Client::new();
    let mut params: Vec<(&str, &str)> = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];
    if let Some(secret) = client_secret {
        params.push(("client_secret", secret));
    }

    let resp = client
        .post(provider.token_url())
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("token refresh request failed: {e}"))?;

    if !resp.status().is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        return Err(format!("token refresh error: {err_text}"));
    }

    let token_data: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse token refresh response: {e}"))?;

    let email = token_data
        .id_token
        .as_deref()
        .and_then(extract_email_from_jwt);

    Ok(OAuthTokens {
        access_token: token_data.access_token,
        refresh_token: token_data
            .refresh_token
            .or_else(|| Some(refresh_token.to_string())),
        expires_in: token_data.expires_in,
        token_type: token_data.token_type,
        email,
    })
}

/// Builds the SASL XOAUTH2 byte string.
/// Format: `user={email}\x01auth=Bearer {access_token}\x01\x01`
pub fn build_xoauth2_string(user: &str, access_token: &str) -> String {
    let raw = format!("user={user}\x01auth=Bearer {access_token}\x01\x01");
    base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
}

/// Extracts email claim from an unverified OpenID Connect JWT payload (base64 JSON).
fn extract_email_from_jwt(jwt: &str) -> Option<String> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload_b64 = parts[1];
    let decoded = URL_SAFE_NO_PAD.decode(payload_b64.as_bytes()).ok()?;
    let val: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    val.get("email")
        .and_then(|e| e.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let (verifier, challenge) = generate_pkce_challenge();
        assert!(!verifier.is_empty());
        assert!(!challenge.is_empty());
        assert_ne!(verifier, challenge);

        // Verify SHA256 of verifier matches challenge
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(challenge, expected);
    }

    #[test]
    fn test_build_auth_url() {
        let url = build_auth_url(
            OAuthProvider::Google,
            "google-client-id-123",
            "http://127.0.0.1:8080/callback",
            "test-challenge",
            "state-xyz",
        )
        .unwrap();

        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("client_id=google-client-id-123"));
        assert!(url.contains("code_challenge=test-challenge"));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn test_provider_from_protocol() {
        assert_eq!(
            OAuthProvider::from_protocol("Google (OAuth2)"),
            Some(OAuthProvider::Google)
        );
        assert_eq!(
            OAuthProvider::from_protocol("Microsoft 365 (OAuth2)"),
            Some(OAuthProvider::Microsoft365)
        );
        assert_eq!(OAuthProvider::from_protocol("IMAP"), None);
        assert_eq!(OAuthProvider::from_protocol("CalDAV"), None);
    }

    #[test]
    fn test_xoauth2_formatting() {
        let user = "alice@example.com";
        let token = "ya29.a0AfH6SM...";
        let encoded = build_xoauth2_string(user, token);

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let s = String::from_utf8(decoded).unwrap();
        assert_eq!(s, format!("user={user}\x01auth=Bearer {token}\x01\x01"));
    }

    /// The loopback capture binds a listener, the browser redirect (simulated
    /// here as a raw HTTP request) carries the code back, and `wait_for_code`
    /// validates the state and returns it.
    #[tokio::test]
    async fn loopback_captures_redirect_code() {
        use tokio::io::AsyncWriteExt;

        let redirect = bind_loopback().unwrap();
        let port = redirect.rsplit(':').next().unwrap().parse::<u16>().unwrap();

        // Fire the redirect slightly after `wait_for_code` starts consuming.
        let client = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            s.write_all(
                format!(
                    "GET /?code=test-code-123&state=test-state HTTP/1.1\r\n\
                     Host: 127.0.0.1\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        });

        let code = wait_for_code(&redirect, "test-state").await.unwrap();
        assert_eq!(code, "test-code-123");
        client.await.unwrap();

        // A second wait for the same URI fails — the listener is consumed.
        assert!(wait_for_code(&redirect, "test-state").await.is_err());
    }
}

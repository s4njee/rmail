//! Secure credential storage and token lifecycle management for OAuth2
//! accounts (OS keychain in release, dev-only file store in debug — see
//! `quill_store::credentials`).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::oauth::{refresh_access_token, OAuthProvider, OAuthTokens};
use quill_store::credentials as store;

const SERVICE: &str = "quill-oauth";
/// Client ID/secret live here so token refresh can read them back without the
/// caller having to carry them around (Roadmap 3.1).
const CONFIG_SERVICE: &str = "quill-oauth-config";

/// The OAuth client the account was created with — needed to refresh tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OAuthClientConfig {
    client_id: String,
    client_secret: Option<String>,
}

/// Persist the client ID/secret for an account (only used at the token
/// endpoint; never crosses IPC).
pub fn save_oauth_client_config(
    address: &str,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<(), String> {
    let json = serde_json::to_string(&OAuthClientConfig {
        client_id: client_id.to_string(),
        client_secret: client_secret.map(str::to_string),
    })
    .map_err(|e| e.to_string())?;
    store::set(CONFIG_SERVICE, address, &json)
}

fn get_oauth_client_config(address: &str) -> Result<OAuthClientConfig, String> {
    let raw = store::get(CONFIG_SERVICE, address)?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

pub fn delete_oauth_client_config(address: &str) -> Result<(), String> {
    store::delete(CONFIG_SERVICE, address)
}

static TOKEN_CACHE: Mutex<Option<HashMap<String, CachedToken>>> = Mutex::new(None);

#[derive(Clone)]
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

pub fn save_oauth_tokens(
    address: &str,
    provider: OAuthProvider,
    tokens: &OAuthTokens,
) -> Result<(), String> {
    if let Some(ref refresh) = tokens.refresh_token {
        store::set(SERVICE, address, refresh)?;
    }

    let ttl = tokens.expires_in.unwrap_or(3600);
    let expires_at = Instant::now() + Duration::from_secs(ttl.saturating_sub(60));

    let mut lock = TOKEN_CACHE.lock().map_err(|e| e.to_string())?;
    let map = lock.get_or_insert_with(HashMap::new);
    map.insert(
        format!("{address}:{provider:?}"),
        CachedToken {
            access_token: tokens.access_token.clone(),
            expires_at,
        },
    );

    Ok(())
}

pub fn get_refresh_token(address: &str) -> Result<String, String> {
    store::get(SERVICE, address)
}

pub fn delete_oauth_tokens(address: &str) -> Result<(), String> {
    store::delete(SERVICE, address)?;

    let mut lock = TOKEN_CACHE.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut map) = *lock {
        map.retain(|k, _| !k.starts_with(address));
    }

    Ok(())
}

/// Retrieves a valid access token for the account, automatically refreshing
/// via the stored refresh token if expired. The client ID/secret come from the
/// keychain (saved when the account was added), so callers only need the
/// address and provider.
pub async fn get_valid_access_token(
    address: &str,
    provider: OAuthProvider,
) -> Result<String, String> {
    let key = format!("{address}:{provider:?}");

    // Check memory cache
    {
        let lock = TOKEN_CACHE.lock().map_err(|e| e.to_string())?;
        if let Some(ref map) = *lock {
            if let Some(cached) = map.get(&key) {
                if Instant::now() < cached.expires_at {
                    return Ok(cached.access_token.clone());
                }
            }
        }
    }

    // Refresh from refresh_token in keychain using the saved client config.
    let refresh_tok = get_refresh_token(address)?;
    let config = get_oauth_client_config(address)?;
    let new_tokens = refresh_access_token(
        provider,
        &refresh_tok,
        &config.client_id,
        config.client_secret.as_deref(),
    )
    .await?;
    let access = new_tokens.access_token.clone();

    save_oauth_tokens(address, provider, &new_tokens)?;

    Ok(access)
}

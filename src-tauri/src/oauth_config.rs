//! Dev/test OAuth credentials loaded from a gitignored `oauth-config.json`.
//!
//! The client ID and secret for testing are read from a file instead of being
//! re-typed into the Add Account form on every `tauri dev` reload. The file is
//! gitignored — this is a local convenience for development, not a place for
//! production secrets (those belong in the OS keychain).
//!
//! Lookup order:
//! 1. `$QUILL_OAUTH_CONFIG` — explicit path.
//! 2. `./oauth-config.json`, then parent dirs (the app's working directory
//!    differs between `tauri dev` and a packaged build).
//!
//! Shape:
//! ```json
//! {
//!   "providers": {
//!     "google":    { "client_id": "…", "client_secret": "…" },
//!     "microsoft": { "client_id": "…", "client_secret": "…" }
//!   }
//! }
//! ```

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OAuthClientConfig {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OAuthConfigFile {
    #[serde(default)]
    providers: HashMap<String, OAuthClientConfig>,
}

/// Normalize the IPC provider string to a config-file key.
fn provider_key(provider: &str) -> Option<&'static str> {
    if provider.contains("google") {
        Some("google")
    } else if provider.contains("microsoft")
        || provider.contains("365")
        || provider.contains("outlook")
    {
        Some("microsoft")
    } else {
        None
    }
}

/// Locate `oauth-config.json` under the env override or the working tree.
fn find_config_path() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("QUILL_OAUTH_CONFIG") {
        candidates.push(PathBuf::from(p));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("oauth-config.json"));
        candidates.push(cwd.join("..").join("oauth-config.json"));
        candidates.push(cwd.join("..").join("..").join("oauth-config.json"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Load the config for a provider, or `None` when the file is absent, the
/// provider has no entry, or the entry has no client ID.
pub fn load(provider: &str) -> Option<OAuthClientConfig> {
    let path = find_config_path()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    parse_config(&raw, provider)
}

fn parse_config(raw: &str, provider: &str) -> Option<OAuthClientConfig> {
    let key = provider_key(provider)?;
    let file: OAuthConfigFile = serde_json::from_str(raw).ok()?;
    let config = file.providers.get(key)?.clone();
    if config
        .client_id
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        return None;
    }
    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = r#"{
        "providers": {
            "google":    { "client_id": "g-id", "client_secret": "g-secret" },
            "microsoft": { "client_id": "m-id", "client_secret": "m-secret" }
        }
    }"#;

    #[test]
    fn parses_provider_entries() {
        let google = parse_config(RAW, "google").unwrap();
        assert_eq!(google.client_id.as_deref(), Some("g-id"));
        assert_eq!(google.client_secret.as_deref(), Some("g-secret"));

        let ms = parse_config(RAW, "microsoft365").unwrap();
        assert_eq!(ms.client_id.as_deref(), Some("m-id"));
    }

    #[test]
    fn empty_client_id_is_absent() {
        let raw = r#"{ "providers": { "google": { "client_id": "", "client_secret": "x" } } }"#;
        assert!(parse_config(raw, "google").is_none());
    }

    #[test]
    fn unknown_provider_or_missing_entry_is_none() {
        assert!(parse_config(RAW, "yahoo").is_none());
        assert!(parse_config(RAW, "").is_none());
        assert!(parse_config("{}", "google").is_none());
        assert!(parse_config("not json", "google").is_none());
    }
}

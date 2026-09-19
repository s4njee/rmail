//! OAuth client configuration.
//!
//! Release builds take their provider client IDs from build-time environment
//! variables. A gitignored `oauth-config.json` remains available for debug
//! builds, so developer credentials cannot accidentally become a release
//! fallback.
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

/// Load debug-only credentials from the local config file. Production builds
/// must use the build-time configuration returned by [`configured`].
#[cfg(debug_assertions)]
fn local_config(provider: &str) -> Option<OAuthClientConfig> {
    let path = find_config_path()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    parse_config(&raw, provider)
}

/// OAuth config embedded at compile time for signed builds. Desktop OAuth
/// clients are public clients; the optional Google secret is provider metadata,
/// not a user credential.
fn build_config(provider: &str) -> Option<OAuthClientConfig> {
    let (client_id, client_secret) = match provider_key(provider)? {
        "google" => (
            option_env!("QUILL_GOOGLE_OAUTH_CLIENT_ID"),
            option_env!("QUILL_GOOGLE_OAUTH_CLIENT_SECRET"),
        ),
        "microsoft" => (option_env!("QUILL_MICROSOFT_OAUTH_CLIENT_ID"), None),
        _ => return None,
    };
    let client_id = client_id?.trim();
    if client_id.is_empty() {
        return None;
    }
    Some(OAuthClientConfig {
        client_id: Some(client_id.to_string()),
        client_secret: client_secret
            .map(str::trim)
            .filter(|secret| !secret.is_empty())
            .map(str::to_string),
    })
}

/// Return the configured OAuth client for a provider. Debug builds prefer the
/// local, gitignored file; release builds only accept build-time values.
pub fn configured(provider: &str) -> Option<OAuthClientConfig> {
    #[cfg(debug_assertions)]
    if let Some(config) = local_config(provider) {
        return Some(config);
    }
    build_config(provider)
}

/// Providers whose browser sign-in works in this build, as the IPC provider
/// strings the frontend passes to `get_oauth_init`. The UI uses this to offer
/// "Sign in with …" only when it will work, and to route Gmail to the
/// app-password path otherwise.
pub fn available_providers() -> Vec<&'static str> {
    ["google", "microsoft365"]
        .into_iter()
        .filter(|provider| configured(provider).is_some())
        .collect()
}

/// The error shown when sign-in is attempted without a configured client.
/// Written for the person signing in, not for the developer building Quill.
pub fn not_configured_message(provider: &str) -> String {
    match provider_key(provider) {
        Some("google") => "Signing in with Google isn't available in this build of Quill. \
                           Connect Gmail with an app password instead."
            .to_string(),
        Some("microsoft") => {
            "Signing in with Microsoft isn't available in this build of Quill.".to_string()
        }
        _ => format!("Browser sign-in isn't available for {provider}."),
    }
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

//! Credential resolution for account connections (Roadmap 3.1).
//!
//! Plain IMAP/SMTP accounts authenticate with a stored password; OAuth2
//! accounts (Google / Microsoft 365, protocol "… (OAuth2)") authenticate with
//! an XOAUTH2 bearer token fetched (and auto-refreshed) at connect time. Both
//! collapse into [`Credential`] so the sync and SMTP engines never have to know
//! which auth the account uses.

use quill_store::types::Account;

use crate::oauth::OAuthProvider;

/// Authentication material for one account's IMAP/SMTP connections.
#[derive(Debug, Clone)]
pub enum Credential {
    /// Plain password from the keychain (IMAP / Bridge accounts).
    Password(String),
    /// OAuth2 account: resolve an access token via the token store when needed.
    OAuth {
        address: String,
        provider: OAuthProvider,
    },
}

/// Resolve an account's credential from the OS keychain.
///
/// - OAuth accounts → [`Credential::OAuth`]; the access token is fetched and
///   refreshed lazily at connect time.
/// - Everything else → the stored password.
pub fn resolve_credential(account: &Account) -> Result<Credential, String> {
    if account.is_oauth() {
        let provider = OAuthProvider::from_protocol(&account.protocol)
            .ok_or_else(|| format!("no OAuth provider for protocol '{}'", account.protocol))?;
        Ok(Credential::OAuth {
            address: account.address.clone(),
            provider,
        })
    } else {
        let password = crate::credentials::get_credential(&account.address)?;
        Ok(Credential::Password(password))
    }
}

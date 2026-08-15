//! Well-known mail provider presets (backlog.md P0.2).
//!
//! Each preset carries the IMAP/SMTP/CalDAV endpoints, the auth kind the
//! provider expects (OAuth, app password, or plain password), and a short
//! provider-specific help string shown at the point of failure. The account
//! setup flow matches an email's domain to a preset to prefill the manual
//! form and to route OAuth/app-password guidance; `autodiscover` falls back
//! to DNS SRV / autoconfig / guesses when no preset matches.

use quill_store::types::{AuthKind, Endpoint, ProviderPreset};

fn ep(host: &str, port: u16, tls: bool) -> Endpoint {
    Endpoint {
        host: host.to_string(),
        port,
        tls,
    }
}

/// All known presets. Returned as owned values so the frontend's provider
/// chooser can render them (the static table itself is const data).
pub fn all_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "gmail".into(),
            name: "Gmail".into(),
            domains: vec!["gmail.com".into(), "googlemail.com".into()],
            imap: ep("imap.gmail.com", 993, true),
            smtp: ep("smtp.gmail.com", 465, true),
            caldav: None, // Google calendars sync via the Calendar API (OAuth).
            auth: AuthKind::Oauth,
            oauth_provider: Some("google".into()),
            help: "Google is moving to OAuth-only for mail. Sign in with Google below. \
                   If you need an app password instead, enable 2-Step Verification and \
                   create one at myaccount.google.com → Security → App passwords."
                .into(),
        },
        ProviderPreset {
            id: "microsoft365".into(),
            name: "Microsoft 365 / Outlook".into(),
            domains: vec![
                "outlook.com".into(),
                "hotmail.com".into(),
                "live.com".into(),
                "msn.com".into(),
            ],
            imap: ep("outlook.office365.com", 993, true),
            smtp: ep("smtp.office365.com", 587, true),
            caldav: None, // Microsoft calendars sync via the Graph API (OAuth).
            auth: AuthKind::Oauth,
            oauth_provider: Some("microsoft365".into()),
            help: "Microsoft 365 requires OAuth for Outlook.com accounts. Work/school \
                   accounts (e.g. name@contoso.com) may need your organisation's admin \
                   to allow the sign-in; try the browser sign-in below first."
                .into(),
        },
        ProviderPreset {
            id: "icloud".into(),
            name: "iCloud Mail".into(),
            domains: vec!["icloud.com".into(), "me.com".into(), "mac.com".into()],
            imap: ep("imap.mail.me.com", 993, true),
            smtp: ep("smtp.mail.me.com", 587, true),
            caldav: Some(ep("p05-caldav.icloud.com", 443, true)),
            auth: AuthKind::AppPassword,
            oauth_provider: None,
            help: "iCloud no longer accepts your regular password for mail. Create an \
                   app-specific password at appleid.apple.com → Sign-in & Security → \
                   App-Specific Passwords, then use it as the password here."
                .into(),
        },
        ProviderPreset {
            id: "fastmail".into(),
            name: "Fastmail".into(),
            domains: vec![
                "fastmail.com".into(),
                "fastmail.fm".into(),
                "fastmail.co.uk".into(),
                "fmailbox.net".into(),
                "fastmail.net".into(),
                "fastmail.email".into(),
            ],
            imap: ep("imap.fastmail.com", 993, true),
            smtp: ep("smtp.fastmail.com", 465, true),
            caldav: Some(ep("caldav.fastmail.com", 443, true)),
            auth: AuthKind::AppPassword,
            oauth_provider: None,
            help: "Fastmail requires an app password for third-party clients: \
                   Settings → Password & Authentication → App passwords."
                .into(),
        },
        ProviderPreset {
            id: "yahoo".into(),
            name: "Yahoo Mail".into(),
            domains: vec!["yahoo.com".into(), "ymail.com".into()],
            imap: ep("imap.mail.yahoo.com", 993, true),
            smtp: ep("smtp.mail.yahoo.com", 465, true),
            caldav: None,
            auth: AuthKind::AppPassword,
            oauth_provider: None,
            help: "Yahoo requires an app-generated password for mail apps: \
                   Account Security → Generate app password."
                .into(),
        },
        ProviderPreset {
            id: "aol".into(),
            name: "AOL Mail".into(),
            domains: vec!["aol.com".into()],
            imap: ep("imap.aol.com", 993, true),
            smtp: ep("smtp.aol.com", 465, true),
            caldav: None,
            auth: AuthKind::AppPassword,
            oauth_provider: None,
            help: "AOL requires an app password for third-party clients: \
                   Account Security → Generate app password."
                .into(),
        },
        ProviderPreset {
            id: "zoho".into(),
            name: "Zoho Mail".into(),
            domains: vec!["zoho.com".into(), "zohomail.com".into()],
            imap: ep("imap.zoho.com", 993, true),
            smtp: ep("smtp.zoho.com", 465, true),
            caldav: Some(ep("caldav.zoho.com", 443, true)),
            auth: AuthKind::Password,
            oauth_provider: None,
            help: "Zoho accepts your regular password, or an app password created at \
                   accounts.zoho.com → Security → App passwords."
                .into(),
        },
        ProviderPreset {
            id: "proton".into(),
            name: "Proton Mail (Bridge)".into(),
            domains: vec!["protonmail.com".into(), "proton.me".into()],
            imap: ep("127.0.0.1", 1143, false),
            smtp: ep("127.0.0.1", 1025, false),
            caldav: None,
            auth: AuthKind::Password,
            oauth_provider: None,
            help: "Proton Mail uses the Proton Bridge app for IMAP/SMTP. Install and sign \
                   into Bridge first; Quill then connects to Bridge running on your \
                   computer (127.0.0.1:1143 / 1025) with your Proton password."
                .into(),
        },
    ]
}

/// The preset whose domain suffixes include `domain` (case-insensitive).
pub fn preset_for_domain(domain: &str) -> Option<ProviderPreset> {
    let d = domain.to_lowercase();
    all_presets()
        .into_iter()
        .find(|p| p.domains.iter().any(|suffix| d == suffix.to_lowercase()))
}

/// The preset for a known provider id ("gmail", "icloud", …).
pub fn preset_by_id(id: &str) -> Option<ProviderPreset> {
    all_presets().into_iter().find(|p| p.id == id)
}

/// Map an IMAP host to the SMTP submission host for the well-known providers
/// that use a distinct SMTP hostname. Falls back to the IMAP host itself (the
/// common convention for self-hosted and smaller providers, where
/// `imap.<domain>` ⇒ `smtp.<domain>`). Kept in sync with the preset table.
pub fn smtp_host_for(imap_host: &str) -> String {
    let h = imap_host.to_lowercase();
    if h.contains("gmail") {
        "smtp.gmail.com".into()
    } else if h.contains("outlook") || h.contains("hotmail") || h.contains("office365") || h.contains("live") {
        "smtp.office365.com".into()
    } else if h.contains("yahoo") {
        "smtp.mail.yahoo.com".into()
    } else if h.contains("icloud") || h.contains("me.com") {
        "smtp.mail.me.com".into()
    } else if h.contains("aol") {
        "smtp.aol.com".into()
    } else {
        imap_host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_domains() {
        assert_eq!(preset_for_domain("gmail.com").unwrap().id, "gmail");
        assert_eq!(preset_for_domain("GMail.com").unwrap().id, "gmail");
        assert_eq!(preset_for_domain("icloud.com").unwrap().id, "icloud");
        assert_eq!(preset_for_domain("me.com").unwrap().id, "icloud");
        assert_eq!(preset_for_domain("fastmail.com").unwrap().id, "fastmail");
        assert_eq!(preset_for_domain("outlook.com").unwrap().id, "microsoft365");
        assert_eq!(preset_for_domain("proton.me").unwrap().id, "proton");
    }

    #[test]
    fn unknown_domain_has_no_preset() {
        assert!(preset_for_domain("example.org").is_none());
        assert!(preset_for_domain("").is_none());
    }

    #[test]
    fn oauth_providers_carry_their_provider() {
        let gmail = preset_by_id("gmail").unwrap();
        assert_eq!(gmail.auth, AuthKind::Oauth);
        assert_eq!(gmail.oauth_provider.as_deref(), Some("google"));
        let ms = preset_by_id("microsoft365").unwrap();
        assert_eq!(ms.oauth_provider.as_deref(), Some("microsoft365"));
    }

    #[test]
    fn app_password_providers_have_help() {
        for id in ["icloud", "fastmail", "yahoo", "aol"] {
            let p = preset_by_id(id).unwrap();
            assert_eq!(p.auth, AuthKind::AppPassword, "{id} should be app-password");
            assert!(!p.help.is_empty(), "{id} needs help text");
        }
    }

    #[test]
    fn smtp_host_mapping_matches_presets() {
        assert_eq!(smtp_host_for("imap.gmail.com"), "smtp.gmail.com");
        assert_eq!(smtp_host_for("outlook.office365.com"), "smtp.office365.com");
        assert_eq!(smtp_host_for("imap.mail.me.com"), "smtp.mail.me.com");
        assert_eq!(smtp_host_for("imap.custom.org"), "imap.custom.org");
    }
}

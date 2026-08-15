//! Connection error taxonomy (backlog.md P0.2).
//!
//! Every failure a user hits while connecting to a mail/calendar server is
//! reduced to a [`ConnectionIssue`]: which service+server, what *kind* of
//! failure (DNS vs TLS vs auth vs rate limit …), and optionally
//! provider-specific help. The add-account test and first-run flow render
//! these with a Retry / Edit Settings choice; the rest of the app keeps its
//! `Result<_, String>` surface but routes messages through
//! [`fmt_issue`] so they read as actionable text.

use quill_store::types::{ConnectionIssue, ErrorKind, Service};

/// Classify a connection error string into a kind, for the UI to offer the
/// right remedy. Substring heuristics over the lowercase message.
pub fn classify(detail: &str) -> ErrorKind {
    let d = detail.to_lowercase();
    if contains_any(&d, &[
        "dns", "couldn't resolve", "nodename nor servname", "getaddrinfo",
        "name or service not known", "no address associated",
    ]) {
        ErrorKind::Dns
    } else if contains_any(&d, &[
        "rate limit", "429", "too many", "throttl", "try again later",
    ]) {
        ErrorKind::RateLimit
    } else if contains_any(&d, &["timed out", "timeout", "timedout", "deadline"]) {
        ErrorKind::Timeout
    } else if contains_any(&d, &[
        "authentication", "login", "xoauth", "credential", "invalid credentials",
        "unauthorized", "password", "denied", "no such account", "bad username",
        "auth failed", "could not authenticate", "invalid_grant", "access denied",
    ]) {
        ErrorKind::Auth
    } else if contains_any(&d, &[
        "tls", "certificate", "cert", "handshake", "ssl", "identity", "security",
        "webpki", "unknown issuer",
    ]) {
        ErrorKind::Tls
    } else if contains_any(&d, &["offline", "network is unreachable", "not connected"]) {
        ErrorKind::Offline
    } else if contains_any(&d, &[
        "refused", "unreachable", "no route", "connection reset", "broken pipe",
        "connection aborted", "connect ",
    ]) {
        ErrorKind::Connect
    } else {
        // Greetings, protocol violations, and anything unclassified land here.
        ErrorKind::Protocol
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

/// The user-facing label for a service.
pub fn service_label(service: Service) -> &'static str {
    match service {
        Service::Imap => "IMAP",
        Service::Smtp => "SMTP",
        Service::CalDav => "CalDAV",
    }
}

/// Build a classified issue for a service/server pair.
pub fn issue(service: Service, server: &str, detail: &str) -> ConnectionIssue {
    ConnectionIssue {
        service,
        server: server.to_string(),
        kind: classify(detail),
        detail: detail.to_string(),
        help: None,
    }
}

/// Attach provider-specific help (from a preset) to an issue.
pub fn with_help(mut issue: ConnectionIssue, help: &str) -> ConnectionIssue {
    issue.help = Some(help.to_string());
    issue
}

/// Render an issue as one actionable sentence, e.g.
/// `IMAP (imap.gmail.com): authentication failed — Login denied`.
pub fn fmt_issue(issue: &ConnectionIssue) -> String {
    format!(
        "{} ({}): {}",
        service_label(issue.service),
        issue.server,
        issue.detail
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_error_kinds() {
        assert_eq!(classify("couldn't resolve imap.example.com"), ErrorKind::Dns);
        assert_eq!(classify("getaddrinfo: nodename nor servname"), ErrorKind::Dns);
        assert_eq!(
            classify("connection timed out after 5s"),
            ErrorKind::Timeout
        );
        assert_eq!(classify("login for a@b.com: Login denied"), ErrorKind::Auth);
        assert_eq!(classify("xoauth2 auth failed: invalid_grant"), ErrorKind::Auth);
        assert_eq!(classify("invalid credentials"), ErrorKind::Auth);
        assert_eq!(
            classify("TLS error: certificate verify failed"),
            ErrorKind::Tls
        );
        assert_eq!(
            classify("handshake failure: received fatal alert"),
            ErrorKind::Tls
        );
        assert_eq!(classify("too many requests — rate limited"), ErrorKind::RateLimit);
        assert_eq!(
            classify("connection refused: tcp connect"),
            ErrorKind::Connect
        );
        assert_eq!(classify("no route to host"), ErrorKind::Connect);
        assert_eq!(classify("network is unreachable"), ErrorKind::Offline);
        assert_eq!(classify("greeting: unexpected response"), ErrorKind::Protocol);
        assert_eq!(classify("something else entirely"), ErrorKind::Protocol);
    }

    #[test]
    fn issues_format_actionably() {
        let i = issue(Service::Imap, "imap.gmail.com", "Login denied");
        assert_eq!(i.kind, ErrorKind::Auth);
        assert_eq!(
            fmt_issue(&i),
            "IMAP (imap.gmail.com): Login denied"
        );
        let with_help = with_help(i, "Create an app password first.");
        assert_eq!(with_help.help.as_deref(), Some("Create an app password first."));
    }
}

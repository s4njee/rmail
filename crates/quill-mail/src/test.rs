//! Full connection test (backlog.md P0.2).
//!
//! The add-account form and first-run flow call this instead of the old raw
//! TCP probe: it walks resolve → TCP → TLS → protocol greeting → (optional)
//! auth per stage and reports each failure as a classified
//! [`ConnectionIssue`], so the UI can say *"IMAP (imap.example.com):
//! authentication failed"* and offer Retry / Edit Settings.

use std::time::Duration;

use base64::Engine;
use quill_store::types::{ConnectionIssue, ConnectionTestReport, Service, TestConnectionSettings};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use crate::error;
use crate::sync::Stream;

type BoxStream = Stream;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Run the connection test for one service. Never panics; always returns a
/// report with the issues encountered up to the failing stage. `password` is
/// supplied by the caller (straight from the form) — it never crosses IPC as
/// part of `settings`.
pub async fn test_connection(
    settings: &TestConnectionSettings,
    password: Option<&str>,
) -> ConnectionTestReport {
    let service = match settings.protocol.as_str() {
        "smtp" => Service::Smtp,
        "caldav" => Service::CalDav,
        _ => Service::Imap,
    };
    let mut issues: Vec<ConnectionIssue> = Vec::new();
    let server = settings.server.clone();

    // CalDAV takes the simple HTTP path — reqwest already resolves, connects,
    // and negotiates TLS, so one reachability check with a clear error is all
    // the user needs (the CalDAV client does its own PROPFIND auth later).
    if service == Service::CalDav {
        return test_caldav_reachability(&settings).await;
    }

    // 1. DNS resolve — done explicitly so a resolution failure is reported as
    //    such, not as a generic connect error.
    match resolve(&server).await {
        Ok(_) => {}
        Err(e) => {
            issues.push(error::issue(service, &server, &e));
            return report(false, false, issues, String::new());
        }
    }

    // 2. TCP connect with a timeout.
    let addr = format!("{}:{}", server, settings.port);
    let tcp = match tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => {
            issues.push(error::issue(
                service,
                &server,
                &format!("couldn't connect to {addr}: {e}"),
            ));
            return report(false, false, issues, String::new());
        }
        Err(_) => {
            issues.push(error::issue(
                service,
                &server,
                &format!("connection to {addr} timed out"),
            ));
            return report(false, false, issues, String::new());
        }
    };

    // 3. TLS handshake when enabled.
    let stream: BoxStream = if settings.tls {
        let tls = async_native_tls::TlsConnector::new();
        match tls.connect(&server, tcp.compat()).await {
            Ok(s) => Box::new(s),
            Err(e) => {
                issues.push(error::issue(
                    service,
                    &server,
                    &format!("TLS to {}: {e}", server),
                ));
                return report(false, false, issues, String::new());
            }
        }
    } else {
        Box::new(tcp.compat())
    };

    // 4. Protocol greeting, then auth when a password was supplied.
    match service {
        Service::Imap => test_imap(&settings, &server, stream, password, &mut issues).await,
        Service::Smtp => test_smtp(&settings, &server, stream, password, &mut issues).await,
        Service::CalDav => unreachable!(), // handled above
    }
}

async fn resolve(host: &str) -> Result<Vec<std::net::IpAddr>, String> {
    let resolver = hickory_resolver::TokioResolver::builder_tokio()
        .map_err(|e| format!("couldn't initialise DNS resolver: {e}"))?
        .build()
        .map_err(|e| format!("couldn't initialise DNS resolver: {e}"))?;
    let lookup = resolver
        .lookup_ip(host)
        .await
        .map_err(|e| format!("couldn't resolve {host}: {e}"))?;
    Ok(lookup.iter().collect())
}

/// IMAP: read the greeting, then attempt LOGIN when a password is provided.
async fn test_imap(
    settings: &TestConnectionSettings,
    server: &str,
    stream: BoxStream,
    password: Option<&str>,
    issues: &mut Vec<ConnectionIssue>,
) -> ConnectionTestReport {
    let mut client = async_imap::Client::new(stream);
    if let Err(e) = client.read_response().await {
        issues.push(error::issue(
            Service::Imap,
            server,
            &format!("no valid greeting from server: {e}"),
        ));
        return report(true, false, issues.clone(), String::new());
    }
    match password {
        Some(password) if !password.is_empty() => match client.login(&settings.email, password).await {
            Ok(_) => report(true, true, issues.clone(), "Connected and authenticated".into()),
            Err((e, _)) => {
                issues.push(error::issue(
                    Service::Imap,
                    server,
                    &format!("login for {}: {e}", settings.email),
                ));
                report(true, false, issues.clone(), "reachable, but authentication failed".into())
            }
        },
        _ => report(true, false, issues.clone(), "Connected (no password supplied)".into()),
    }
}

/// SMTP: read the greeting, EHLO, then AUTH PLAIN when a password is provided.
async fn test_smtp(
    settings: &TestConnectionSettings,
    server: &str,
    stream: BoxStream,
    password: Option<&str>,
    issues: &mut Vec<ConnectionIssue>,
) -> ConnectionTestReport {
    let mut reader = BufReader::new(stream.compat());
    let mut line = String::new();
    if reader.read_line(&mut line).await.is_err() || !line.starts_with("220") {
        issues.push(error::issue(
            Service::Smtp,
            server,
            &format!("no valid SMTP greeting (got {:?})", line.trim()),
        ));
        return report(true, false, issues.clone(), String::new());
    }
    let mut ehlo = String::new();
    let _ = reader
        .read_line(&mut ehlo)
        .await
        .map_err(|e| e.to_string());
    // Consume the rest of a multiline 250 response.
    while ehlo.trim_end().ends_with('-') {
        ehlo.clear();
        if reader.read_line(&mut ehlo).await.is_err() {
            break;
        }
    }

    match password {
        Some(password) if !password.is_empty() => {
            let auth = format!(
                "AUTH PLAIN {}\r\n",
                base64::engine::general_purpose::STANDARD
                    .encode(format!("\0{}\0{}", settings.email, password))
            );
            if let Err(e) = reader.write_all(auth.as_bytes()).await {
                issues.push(error::issue(
                    Service::Smtp,
                    server,
                    &format!("couldn't send AUTH: {e}"),
                ));
                return report(true, false, issues.clone(), String::new());
            }
            let _ = reader.flush().await;
            line.clear();
            // 235 = authentication succeeded.
            match reader.read_line(&mut line).await {
                Ok(_) if line.starts_with("235") => {
                    report(true, true, issues.clone(), "Connected and authenticated".into())
                }
                _ => {
                    issues.push(error::issue(
                        Service::Smtp,
                        server,
                        &format!("SMTP authentication rejected ({})", line.trim()),
                    ));
                    report(true, false, issues.clone(), "reachable, but authentication failed".into())
                }
            }
        }
        _ => report(true, false, issues.clone(), "Connected (no password supplied)".into()),
    }
}

/// CalDAV: HTTPS GET the well-known path; any HTTP status (including 401 —
/// auth is expected at the PROPFIND stage) means the server is reachable.
async fn test_caldav_reachability(settings: &TestConnectionSettings) -> ConnectionTestReport {
    let scheme = if settings.tls { "https" } else { "http" };
    let url = format!(
        "{}://{}:{}/.well-known/caldav",
        scheme, settings.server, settings.port
    );
    let client = reqwest::Client::builder()
        .timeout(CONNECT_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() || status.is_redirection() || status.as_u16() == 401 || status.as_u16() == 403 {
                report(true, false, Vec::new(), format!("CalDAV server reachable (HTTP {status})"))
            } else {
                let issues = vec![error::issue(
                    Service::CalDav,
                    &settings.server,
                    &format!("unexpected HTTP status {status} on {url}"),
                )];
                report(false, false, issues, String::new())
            }
        }
        Err(e) => {
            let issues = vec![error::issue(
                Service::CalDav,
                &settings.server,
                &format!("couldn't reach {url}: {e}"),
            )];
            report(false, false, issues, String::new())
        }
    }
}

fn report(
    ok: bool,
    authed: bool,
    issues: Vec<ConnectionIssue>,
    detail: String,
) -> ConnectionTestReport {
    ConnectionTestReport {
        ok,
        authed,
        issues,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The per-stage test paths need a live server, which unit tests avoid; the
    // resolve classifier and CalDAV reachability are covered by error.rs tests
    // and a TCP reachability check below using a local listener.

    #[tokio::test]
    async fn tcp_refused_reports_connect_issue() {
        // Bind then drop — the port is now closed, so connect is refused.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let report = test_connection(
            &TestConnectionSettings {
                email: "a@example.com".into(),
                protocol: "imap".into(),
                server: "127.0.0.1".into(),
                port,
                tls: false,
            },
            None,
        )
        .await;
        assert!(!report.ok);
        assert!(!report.issues.is_empty());
        assert_eq!(report.issues[0].kind, quill_store::types::ErrorKind::Connect);
    }
}

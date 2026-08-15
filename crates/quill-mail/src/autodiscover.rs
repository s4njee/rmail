//! Autodiscovery pipeline (backlog.md P0.2).
//!
//! Given an email domain, discover IMAP/SMTP/CalDAV settings in order of
//! trust: provider preset → DNS SRV (`_imaps`/`_imap`/`_submission`/
//! `_carddavs`) → Thunderbird/Mozilla autoconfig XML → standard guesses.
//! Every probe is recorded as a [`DiscoveryStep`] so the UI can show *what*
//! was tried and *why* a fallback happened, and the user can always drop to
//! the manual form.

use hickory_resolver::proto::rr::RData;
use hickory_resolver::TokioResolver;
use quill_store::types::{DiscoveryStep, DiscoveredSettings, Endpoint};

use crate::provider::preset_for_domain;

const AUTOCONFIG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

fn step(source: &str, status: &str, detail: impl Into<String>) -> DiscoveryStep {
    DiscoveryStep {
        source: source.into(),
        status: status.into(),
        detail: detail.into(),
    }
}

fn endpoint(host: impl Into<String>, port: u16, tls: bool) -> Endpoint {
    Endpoint {
        host: host.into(),
        port,
        tls,
    }
}

/// Full autodiscovery for an email domain. Network-bound; the pure pieces
/// (`parse_autoconfig`, guesses) are unit-tested offline.
pub async fn discover(domain: &str) -> DiscoveredSettings {
    let domain = domain.trim().trim_end_matches('.').to_lowercase();
    let mut steps: Vec<DiscoveryStep> = Vec::new();

    // 1. Provider preset — authoritative for well-known providers, so a match
    //    short-circuits (no point probing DNS for Gmail).
    if let Some(preset) = preset_for_domain(&domain) {
        steps.push(step(
            "preset",
            "ok",
            format!("Matched {} — using its known mail settings", preset.name),
        ));
        return DiscoveredSettings {
            imap: Some(preset.imap.clone()),
            smtp: Some(preset.smtp.clone()),
            caldav: preset.caldav.clone(),
            provider: Some(preset),
            steps,
        };
    }
    steps.push(step("preset", "skip", "No known provider matches this domain"));

    // 2. DNS SRV (RFC 6186 / 6764).
    let mut imap = None;
    if imap.is_none() {
        imap = lookup_srv(&domain, "_imaps", true).await;
        if imap.is_none() {
            imap = lookup_srv(&domain, "_imap", false).await;
        }
    }
    let smtp = lookup_srv(&domain, "_submission", true).await;
    let caldav = lookup_srv(&domain, "_carddavs", true).await;
    steps.extend(match (&imap, &smtp, &caldav) {
        (Some(_), _, _) | (_, Some(_), _) | (_, _, Some(_)) => vec![step(
            "dns_srv",
            "ok",
            "Found mail servers via DNS SRV records",
        )],
        _ => vec![step("dns_srv", "skip", "No DNS SRV records for this domain")],
    });

    // 3. Thunderbird / Mozilla autoconfig — fills any service SRV didn't.
    let (ac_imap, ac_smtp) = fetch_autoconfig(&domain, &mut steps).await;
    if imap.is_none() {
        imap = ac_imap;
    }
    let mut smtp = smtp.or(ac_smtp);

    // 4. Standard guesses as a last resort (imap.<domain>:993 etc.).
    if imap.is_none() {
        imap = Some(endpoint(format!("imap.{domain}"), 993, true));
        steps.push(step(
            "guess",
            "ok",
            format!("Guessed IMAP server imap.{domain}:993 — verify below"),
        ));
    }
    if smtp.is_none() {
        smtp = Some(endpoint(format!("smtp.{domain}"), 587, true));
        steps.push(step(
            "guess",
            "ok",
            format!("Guessed SMTP server smtp.{domain}:587 — verify below"),
        ));
    }
    if caldav.is_none() {
        steps.push(step(
            "dns_srv",
            "skip",
            "No CalDAV server discovered — add calendars manually if the provider offers them",
        ));
    }

    DiscoveredSettings {
        imap,
        smtp,
        caldav,
        provider: None,
        steps,
    }
}

/// Look up `<service>._tcp.<domain>` SRV and map the lowest-priority record to
/// an [`Endpoint`]. `tls` says whether this service's port speaks TLS directly.
async fn lookup_srv(domain: &str, service: &str, tls: bool) -> Option<Endpoint> {
    let name = format!("{service}._tcp.{domain}");
    let resolver = TokioResolver::builder_tokio().ok()?.build().ok()?;
    let lookup = resolver.srv_lookup(&name).await.ok()?;
    lookup
        .answers()
        .iter()
        .filter_map(|r| match &r.data {
            RData::SRV(srv) => Some(srv),
            _ => None,
        })
        .min_by_key(|srv| srv.priority)
        .map(|srv| {
            // SRV targets are fully-qualified names with a trailing dot.
            let host = srv.target.to_string().trim_end_matches('.').to_string();
            endpoint(host, srv.port, tls)
        })
}

/// Probe the autoconfig locations concurrently until one yields usable
/// settings. Appends a [`DiscoveryStep`] per attempt. Concurrent so a slow or
/// missing autoconfig server doesn't stall the whole discovery.
async fn fetch_autoconfig(
    domain: &str,
    steps: &mut Vec<DiscoveryStep>,
) -> (Option<Endpoint>, Option<Endpoint>) {
    let urls = autoconfig_urls(domain);
    let client = reqwest::Client::builder()
        .timeout(AUTOCONFIG_TIMEOUT)
        .user_agent("Quill-Mail/0.1 (mail autoconfiguration)")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let attempts: Vec<(String, Result<String, String>)> = futures::future::join_all(
        urls.into_iter().map(|url| {
            let client = client.clone();
            async move {
                let resp = match client.get(&url).send().await {
                    Ok(r) => r,
                    Err(e) => {
                        let msg = format!("{url}: {e}");
                        return (url, Err(msg));
                    }
                };
                let outcome = if resp.status().is_success() {
                    resp.text().await.map_err(|e| format!("{url}: {e}"))
                } else {
                    Err(format!("{url}: HTTP {}", resp.status()))
                };
                (url, outcome)
            }
        }),
    )
    .await;

    for (url, outcome) in attempts {
        match outcome {
            Ok(body) => {
                let (imap, smtp) = parse_autoconfig(&body);
                if imap.is_some() || smtp.is_some() {
                    steps.push(step(
                        "autoconfig",
                        "ok",
                        format!("Used mail config from {url}"),
                    ));
                    return (imap, smtp);
                }
                steps.push(step(
                    "autoconfig",
                    "error",
                    format!("{url}: no usable mail servers in config"),
                ));
            }
            Err(e) => steps.push(step("autoconfig", "error", e)),
        }
    }
    (None, None)
}

/// The Thunderbird/Mozilla autoconfig URLs probed in order.
fn autoconfig_urls(domain: &str) -> Vec<String> {
    vec![
        format!("https://autoconfig.{domain}/mail/config-v1.1.xml"),
        format!("https://autoconfig.thunderbird.net/v1.1/{domain}"),
        format!("https://{domain}/.well-known/autoconfig/mail/config-v1.1.xml"),
    ]
}

/// Parse a Thunderbird `config-v1.1.xml` (or the Mozilla ISPDB response) into
/// the first IMAP and SMTP server it describes. Returns `(None, None)` for
/// anything that isn't a usable config. Pure and unit-tested.
pub fn parse_autoconfig(xml: &str) -> (Option<Endpoint>, Option<Endpoint>) {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    let mut imap: Option<Endpoint> = None;
    let mut smtp: Option<Endpoint> = None;

    // In-progress server block state.
    let mut server_kind: Option<&'static str> = None; // "imap" | "smtp"
    let mut field: Option<&'static str> = None; // current child element
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut socket: Option<&'static str> = None;

    macro_rules! finish_server {
        () => {
            if let (Some(kind), Some(h), Some(p), Some(s)) =
                (server_kind, host.as_deref(), port, socket)
            {
                let tls = match s {
                    "SSL" => true,
                    "STARTTLS" => true,
                    _ => false, // plain / unknown
                };
                let ep = endpoint(h, p, tls);
                if kind == "imap" && imap.is_none() {
                    imap = Some(ep);
                } else if kind == "smtp" && smtp.is_none() {
                    smtp = Some(ep);
                }
            }
            server_kind = None;
            field = None;
            host = None;
            port = None;
            socket = None;
        };
    }

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"incomingServer" => {
                    // Only IMAP servers are usable — skip POP3.
                    let is_imap = e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .find(|a| a.key.as_ref() == b"type")
                        .and_then(|a| {
                            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .ok()
                        })
                        .map(|v| v == "imap")
                        .unwrap_or(false);
                    server_kind = if is_imap { Some("imap") } else { None };
                    field = None;
                    host = None;
                    port = None;
                    socket = None;
                }
                b"outgoingServer" => {
                    server_kind = Some("smtp");
                    field = None;
                    host = None;
                    port = None;
                    socket = None;
                }
                b"hostname" => field = Some("host"),
                b"port" => field = Some("port"),
                b"socketType" => field = Some("socket"),
                b"authentication" => field = Some("auth"),
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if server_kind.is_none() {
                    continue;
                }
                let text = t.decode().unwrap_or_default().trim().to_string();
                if text.is_empty() {
                    continue;
                }
                match field {
                    Some("host") => host = Some(text),
                    Some("port") => port = text.parse().ok(),
                    Some("socket") => {
                        socket = Some(match text.as_str() {
                            "SSL" => "SSL",
                            "STARTTLS" => "STARTTLS",
                            _ => "plain",
                        })
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"incomingServer" | b"outgoingServer" => {
                    finish_server!();
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    (imap, smtp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_thunderbird_config() {
        let xml = r#"<?xml version="1.0"?>
<clientConfig version="1.1">
  <emailProvider id="example.com">
    <incomingServer type="imap">
      <hostname>imap.example.com</hostname>
      <port>993</port>
      <socketType>SSL</socketType>
      <authentication>password-cleartext</authentication>
    </incomingServer>
    <outgoingServer type="smtp">
      <hostname>smtp.example.com</hostname>
      <port>465</port>
      <socketType>SSL</socketType>
      <authentication>password-cleartext</authentication>
    </outgoingServer>
  </emailProvider>
</clientConfig>"#;
        let (imap, smtp) = parse_autoconfig(xml);
        assert_eq!(imap, Some(endpoint("imap.example.com", 993, true)));
        assert_eq!(smtp, Some(endpoint("smtp.example.com", 465, true)));
    }

    #[test]
    fn rejects_non_config_xml() {
        let (imap, smtp) = parse_autoconfig("<html><body>hi</body></html>");
        assert_eq!(imap, None);
        assert_eq!(smtp, None);
        assert_eq!(parse_autoconfig("not xml"), (None, None));
    }
}

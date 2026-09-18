//! SMTP sending (Epic 12.3 & 13).
//!
//! Sends via the account's server (STARTTLS on 587, implicit TLS on 465 when
//! the account uses TLS) with the keychain password, supporting To, Cc, Bcc,
//! In-Reply-To, References, and multipart MIME attachments.

use base64::prelude::*;
use lettre::message::header::{ContentType, InReplyTo, References};
use lettre::message::{MultiPart, SinglePart};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use quill_store::types::{Account, OutgoingMessage};

use crate::auth::Credential;
use crate::oauth_store::get_valid_access_token;

fn sending_address<'a>(account: &'a Account, outgoing: &'a OutgoingMessage) -> &'a str {
    outgoing
        .from_address
        .as_deref()
        .filter(|a| !a.trim().is_empty())
        .unwrap_or(&account.address)
}

/// Assign a stable RFC 5322 ID before the message crosses a durable boundary
/// (scheduled-send or the offline queue). The sender's domain is deliberate:
/// it keeps the ID useful for threading without exposing a machine hostname.
pub fn ensure_message_id(account: &Account, outgoing: &mut OutgoingMessage) -> Result<(), String> {
    if outgoing.message_id.is_some() {
        return Ok(());
    }
    let from = sending_address(account, outgoing);
    let _: lettre::Address = from
        .parse()
        .map_err(|e| format!("invalid from address: {e}"))?;
    let domain = from
        .rsplit_once('@')
        .map(|(_, domain)| domain.trim())
        .filter(|domain| !domain.is_empty() && !domain.contains(['\r', '\n', ' ', '<', '>']))
        .ok_or("from address has no usable Message-ID domain")?;
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let nonce: u64 = rand::random();
    outgoing.message_id = Some(format!("<quill-{timestamp_ms}-{nonce:016x}@{domain}>"));
    Ok(())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn linkify_line(line: &str) -> String {
    let mut html = String::new();
    let mut cursor = 0;
    while cursor < line.len() {
        let Some(found) = line[cursor..].find("http") else {
            html.push_str(&escape_html(&line[cursor..]));
            break;
        };
        let start = cursor + found;
        let prefix = &line[cursor..start];
        let valid_start = start == 0
            || line[..start]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        if !valid_start {
            let end = start + 4;
            html.push_str(&escape_html(&line[cursor..end]));
            cursor = end;
            continue;
        }
        let end = line[start..]
            .find(char::is_whitespace)
            .map(|offset| start + offset)
            .unwrap_or(line.len());
        let candidate = &line[start..end];
        if (candidate.starts_with("https://") || candidate.starts_with("http://"))
            && url::Url::parse(candidate).is_ok()
        {
            html.push_str(&escape_html(prefix));
            let escaped = escape_html(candidate);
            html.push_str(&format!("<a href=\"{escaped}\">{escaped}</a>"));
        } else {
            html.push_str(&escape_html(&line[cursor..end]));
        }
        cursor = end;
    }
    html
}

fn text_to_html(text: &str) -> String {
    text.lines()
        .map(linkify_line)
        .collect::<Vec<_>>()
        .join("<br>\n")
}

fn without_plain_signature(outgoing: &OutgoingMessage) -> String {
    let Some(signature) = outgoing
        .plain_signature
        .as_deref()
        .filter(|signature| !signature.is_empty())
    else {
        return outgoing.body.clone();
    };
    let top = format!("\n\n{signature}\n\n");
    let bottom = format!("\n\n{signature}");
    match outgoing.signature_placement.as_deref() {
        Some("above_quote") => outgoing
            .body
            .strip_prefix(&top)
            .unwrap_or(&outgoing.body)
            .to_string(),
        Some("bottom") => outgoing
            .body
            .strip_suffix(&bottom)
            .unwrap_or(&outgoing.body)
            .to_string(),
        _ => outgoing.body.clone(),
    }
}

/// Render the current plain-text body immediately before SMTP submission.
/// The composer only supplies a trusted account signature, never a stale
/// prebuilt HTML body, so edits made after compose-open are always reflected.
fn rendered_html_body(outgoing: &OutgoingMessage) -> Option<String> {
    let signature = outgoing
        .html_signature
        .as_deref()
        .filter(|signature| !signature.trim().is_empty())?;
    let body = text_to_html(&without_plain_signature(outgoing));
    Some(match outgoing.signature_placement.as_deref() {
        Some("above_quote") => format!("{signature}<br>\n{body}"),
        _ => format!("{body}<br>\n{signature}"),
    })
}

/// Construct an RFC 5322 MIME message from [`OutgoingMessage`].
pub fn build_message(account: &Account, outgoing: &OutgoingMessage) -> Result<Message, String> {
    let from_addr = sending_address(account, outgoing);

    let addr: lettre::Address = from_addr
        .parse()
        .map_err(|e| format!("invalid from address: {e}"))?;
    let from = lettre::message::Mailbox::new(
        outgoing.from_name.clone().filter(|n| !n.trim().is_empty()),
        addr,
    );

    let mut builder = Message::builder()
        .from(from)
        .subject(&outgoing.subject)
        .message_id(outgoing.message_id.clone());

    if let Some(reply_to_str) = outgoing
        .reply_to
        .as_deref()
        .filter(|r| !r.trim().is_empty())
    {
        let reply_to: lettre::message::Mailbox = reply_to_str
            .parse()
            .map_err(|e| format!("invalid reply-to address {reply_to_str}: {e}"))?;
        builder = builder.reply_to(reply_to);
    }

    for to_str in &outgoing.to {
        let recipient: lettre::message::Mailbox = to_str
            .parse()
            .map_err(|e| format!("invalid recipient {to_str}: {e}"))?;
        builder = builder.to(recipient);
    }

    for cc_str in &outgoing.cc {
        let recipient: lettre::message::Mailbox = cc_str
            .parse()
            .map_err(|e| format!("invalid cc recipient {cc_str}: {e}"))?;
        builder = builder.cc(recipient);
    }

    for bcc_str in &outgoing.bcc {
        let recipient: lettre::message::Mailbox = bcc_str
            .parse()
            .map_err(|e| format!("invalid bcc recipient {bcc_str}: {e}"))?;
        builder = builder.bcc(recipient);
    }

    if let Some(ref in_reply_to) = outgoing.in_reply_to {
        builder = builder.header(InReplyTo::from(in_reply_to.clone()));
    }

    if let Some(ref references) = outgoing.references {
        builder = builder.header(References::from(references.clone()));
    }

    let html_body = rendered_html_body(outgoing);
    let has_html = html_body.is_some();

    if outgoing.attachments.is_empty() {
        if has_html {
            let html_body = html_body.as_deref().expect("checked above");
            let alt = MultiPart::alternative()
                .singlepart(SinglePart::plain(outgoing.body.clone()))
                .singlepart(SinglePart::html(html_body.to_string()));
            builder
                .multipart(alt)
                .map_err(|e| format!("build alternative body: {e}"))
        } else {
            builder
                .body(outgoing.body.clone())
                .map_err(|e| format!("build body: {e}"))
        }
    } else {
        let mut mixed = if has_html {
            let html_body = html_body.as_deref().expect("checked above");
            let alt = MultiPart::alternative()
                .singlepart(SinglePart::plain(outgoing.body.clone()))
                .singlepart(SinglePart::html(html_body.to_string()));
            MultiPart::mixed().multipart(alt)
        } else {
            MultiPart::mixed().singlepart(SinglePart::plain(outgoing.body.clone()))
        };

        for att in &outgoing.attachments {
            let data = BASE64_STANDARD
                .decode(&att.data_base64)
                .map_err(|e| format!("attachment {} invalid base64: {e}", att.filename))?;

            let content_type = att
                .content_type
                .parse::<ContentType>()
                .unwrap_or_else(|_| ContentType::parse("application/octet-stream").unwrap());

            let attachment_part =
                lettre::message::Attachment::new(att.filename.clone()).body(data, content_type);

            mixed = mixed.singlepart(attachment_part);
        }

        builder
            .multipart(mixed)
            .map_err(|e| format!("build multipart: {e}"))
    }
}

fn is_localhost(host: &str) -> bool {
    matches!(
        host.trim().to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

/// Send an outgoing message via SMTP. The credential is resolved by the caller
/// and never leaves this process.
pub async fn send_email(
    account: &Account,
    outgoing: &OutgoingMessage,
    credential: &Credential,
) -> Result<(), String> {
    let message = build_message(account, outgoing)?;

    match credential {
        Credential::Password(password) => {
            if account.smtp_security == "plain" && !is_localhost(&account.smtp_server) {
                return Err("refusing plaintext SMTP AUTH except for a localhost bridge".into());
            }
            let username = if account.smtp_username.trim().is_empty() {
                &account.address
            } else {
                &account.smtp_username
            };
            let creds = Credentials::new(username.clone(), password.clone());
            let smtp_host = &account.smtp_server;
            let mailer = match account.smtp_security.as_str() {
                "ssl" => AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_host)
                    .map_err(|e| format!("smtp relay {smtp_host}: {e}"))?
                    .port(account.smtp_port),
                "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_host)
                    .map_err(|e| format!("smtp relay {smtp_host}: {e}"))?
                    .port(account.smtp_port),
                "plain" => {
                    AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(smtp_host.clone())
                        .port(account.smtp_port)
                }
                mode => return Err(format!("unknown SMTP security mode {mode}")),
            }
            .credentials(creds)
            .build();

            mailer.send(message).await.map_err(|e| e.to_string())?;
            Ok(())
        }
        Credential::OAuth { address, provider } => {
            let access_token = get_valid_access_token(address, *provider).await?;
            let smtp_host = &account.smtp_server;
            if account.smtp_security != "starttls" {
                return Err("OAuth SMTP requires STARTTLS submission".into());
            }
            let mailer = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(smtp_host.clone())
                .port(account.smtp_port)
                .tls(Tls::Required(
                    TlsParameters::new(smtp_host.to_string())
                        .map_err(|e| format!("smtp tls params: {e}"))?,
                ))
                .authentication(vec![Mechanism::Xoauth2])
                .credentials(Credentials::new(address.clone(), access_token))
                .build();

            mailer.send(message).await.map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

/// SMTP responses in the 5xx class are permanent until the user edits the
/// message/account. Lettre's transport error is intentionally stringified at
/// this boundary, so recognize an RFC response code without treating a random
/// digit sequence as permanent.
pub fn is_permanent_smtp_failure(error: &str) -> bool {
    let bytes = error.as_bytes();
    bytes.windows(3).enumerate().any(|(index, code)| {
        code[0] == b'5'
            && code[1].is_ascii_digit()
            && code[2].is_ascii_digit()
            && (index == 0 || !bytes[index - 1].is_ascii_digit())
            && (index + 3 == bytes.len() || !bytes[index + 3].is_ascii_digit())
    })
}

/// Gmail and Microsoft 365 create their own Sent copy. Other providers need
/// an explicit IMAP APPEND after SMTP accepts the message.
pub fn provider_auto_saves_sent_copy(account: &Account) -> bool {
    let address = account.address.to_ascii_lowercase();
    let server = account.server.to_ascii_lowercase();
    address.ends_with("@gmail.com")
        || address.ends_with("@googlemail.com")
        || address.ends_with("@outlook.com")
        || address.ends_with("@hotmail.com")
        || address.ends_with("@live.com")
        || server.contains("gmail")
        || server.contains("office365")
        || server.contains("outlook")
}

/// Append the exact submitted RFC 5322 message with `\\Seen` when the
/// provider does not maintain Sent automatically. SMTP success remains final
/// if APPEND fails: retrying SMTP after an unknown append result could deliver
/// a duplicate.
pub async fn append_sent_copy(
    account: &Account,
    credential: &Credential,
    outgoing: &OutgoingMessage,
    sent_folder: &str,
) -> Result<(), String> {
    if provider_auto_saves_sent_copy(account) {
        return Ok(());
    }
    let raw = build_message(account, outgoing)
        .map_err(|e| format!("build sent copy: {e}"))?
        .formatted();
    let mut session = crate::sync::connect(account, credential).await?;
    session
        .append(sent_folder, Some("(\\Seen)"), None, raw)
        .await
        .map_err(|e| format!("append sent copy: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill_store::types::OutgoingAttachment;

    #[test]
    fn classifies_only_smtp_5xx_as_permanent() {
        assert!(is_permanent_smtp_failure(
            "smtp error: 550 mailbox unavailable"
        ));
        assert!(is_permanent_smtp_failure("554 invalid recipient"));
        assert!(!is_permanent_smtp_failure(
            "smtp error: 421 service unavailable"
        ));
        assert!(!is_permanent_smtp_failure("retry 5000 milliseconds"));
    }

    #[test]
    fn test_build_message_with_threading_and_attachments() {
        let account = Account {
            id: 1,
            address: "sender@example.com".into(),
            protocol: "IMAP".into(),
            sync_mode: "every 2 min".into(),
            color: "#3b5bdb".into(),
            local_bytes: 0,
            connected: true,
            server: "smtp.example.com".into(),
            port: 993,
            tls: true,
            imap_security: "ssl".into(),
            allow_plaintext_login: false,
            smtp_server: "smtp.example.com".into(),
            smtp_port: 587,
            smtp_security: "starttls".into(),
            smtp_username: "sender@example.com".into(),
            folder_count: 1,
            last_error: None,
        };

        let outgoing = OutgoingMessage {
            account_id: 1,
            from_name: None,
            from_address: None,
            reply_to: None,
            to: vec!["to@example.com".into()],
            cc: vec!["cc@example.com".into()],
            bcc: vec!["bcc@example.com".into()],
            subject: "Re: Discussion".into(),
            body: "Hello threaded world".into(),
            body_html: None,
            html_signature: None,
            plain_signature: None,
            signature_placement: None,
            in_reply_to: Some("<parent-id@example.com>".into()),
            references: Some("<root-id@example.com> <parent-id@example.com>".into()),
            attachments: vec![OutgoingAttachment {
                filename: "notes.txt".into(),
                content_type: "text/plain".into(),
                data_base64: BASE64_STANDARD.encode(b"Notes content"),
            }],
            original_message_id: None,
            is_forward: None,
            message_id: Some("<quill-test@example.com>".into()),
        };

        let msg = build_message(&account, &outgoing).unwrap();
        let formatted = String::from_utf8(msg.formatted()).unwrap();

        assert!(formatted.contains("From: sender@example.com"));
        assert!(formatted.contains("To: to@example.com"));
        assert!(formatted.contains("Cc: cc@example.com"));
        assert!(formatted.contains("Subject: Re: Discussion"));
        assert!(formatted.contains("In-Reply-To: <parent-id@example.com>"));
        assert!(formatted.contains("References: <root-id@example.com> <parent-id@example.com>"));
        assert!(formatted.contains("Message-ID: <quill-test@example.com>"));
        assert!(formatted.contains("notes.txt"));
    }

    #[test]
    fn test_build_message_with_alias_reply_to_and_html_body() {
        let account = Account {
            id: 1,
            address: "primary@example.com".into(),
            protocol: "IMAP".into(),
            sync_mode: "every 2 min".into(),
            color: "#3b5bdb".into(),
            local_bytes: 0,
            connected: true,
            server: "smtp.example.com".into(),
            port: 993,
            tls: true,
            imap_security: "ssl".into(),
            allow_plaintext_login: false,
            smtp_server: "smtp.example.com".into(),
            smtp_port: 587,
            smtp_security: "starttls".into(),
            smtp_username: "primary@example.com".into(),
            folder_count: 1,
            last_error: None,
        };

        let outgoing = OutgoingMessage {
            account_id: 1,
            from_name: Some("Jane Support".into()),
            from_address: Some("support@customdomain.com".into()),
            reply_to: Some("replies@customdomain.com".into()),
            to: vec!["client@example.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Support Request Resolved".into(),
            body: "Your issue is resolved.\n\nJane Support Team".into(),
            // A caller may carry an obsolete prebuilt body; sender rendering
            // must never use it after the user edits the composer.
            body_html: Some("<p>stale compose-open HTML</p>".into()),
            html_signature: Some("<b>Jane Support Team</b>".into()),
            plain_signature: Some("Jane Support Team".into()),
            signature_placement: Some("bottom".into()),
            in_reply_to: None,
            references: None,
            attachments: vec![],
            original_message_id: Some(42),
            is_forward: None,
            message_id: Some("<quill-test@customdomain.com>".into()),
        };

        let msg = build_message(&account, &outgoing).unwrap();
        let formatted = String::from_utf8(msg.formatted()).unwrap();

        assert!(formatted.contains("support@customdomain.com"));
        assert!(formatted.contains("Jane Support"));
        assert!(
            formatted.contains("Reply-To: replies@customdomain.com")
                || formatted.contains("replies@customdomain.com")
        );
        assert!(formatted.contains("To: client@example.com"));
        assert!(formatted.contains("Subject: Support Request Resolved"));
        assert!(formatted.contains("Your issue is resolved."));
        assert!(formatted.contains("<b>Jane Support Team</b>"));
        assert!(!formatted.contains("stale compose-open HTML"));
        assert!(formatted.contains("multipart/alternative"));
    }

    #[test]
    fn renders_current_body_escaped_and_linkified_at_send_time() {
        let account = Account {
            id: 1,
            address: "sender@example.com".into(),
            protocol: "IMAP".into(),
            sync_mode: "manual".into(),
            color: "#000".into(),
            local_bytes: 0,
            connected: true,
            server: "imap.example.com".into(),
            port: 993,
            tls: true,
            imap_security: "ssl".into(),
            allow_plaintext_login: false,
            smtp_server: "smtp.example.com".into(),
            smtp_port: 587,
            smtp_security: "starttls".into(),
            smtp_username: "sender@example.com".into(),
            folder_count: 0,
            last_error: None,
        };
        let outgoing = OutgoingMessage {
            account_id: 1,
            from_name: None,
            from_address: None,
            reply_to: None,
            to: vec!["to@example.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Current body".into(),
            body: "Changed <body> & visit https://example.com/docs?x=1&y=2\n\nPlain sig".into(),
            body_html: Some("<i>outdated</i>".into()),
            html_signature: Some("<strong>HTML sig</strong>".into()),
            plain_signature: Some("Plain sig".into()),
            signature_placement: Some("bottom".into()),
            in_reply_to: None,
            references: None,
            attachments: vec![],
            original_message_id: None,
            is_forward: None,
            message_id: Some("<current@example.com>".into()),
        };
        let html = rendered_html_body(&outgoing).unwrap();
        assert!(html.contains("Changed &lt;body&gt; &amp; visit"));
        assert!(html.contains("href=\"https://example.com/docs?x=1&amp;y=2\""));
        assert!(html.contains("<strong>HTML sig</strong>"));
        assert!(!html.contains("outdated"));
        assert!(!html.contains("Plain sig"));
        let formatted =
            String::from_utf8(build_message(&account, &outgoing).unwrap().formatted()).unwrap();
        assert!(formatted.contains("multipart/alternative"));
    }

    #[test]
    fn generated_message_id_uses_the_sending_domain() {
        let account = Account {
            id: 1,
            address: "sender@example.com".into(),
            protocol: "IMAP".into(),
            sync_mode: "manual".into(),
            color: "#000".into(),
            local_bytes: 0,
            connected: true,
            server: "imap.example.com".into(),
            port: 993,
            tls: true,
            imap_security: "ssl".into(),
            allow_plaintext_login: false,
            smtp_server: "smtp.example.com".into(),
            smtp_port: 587,
            smtp_security: "starttls".into(),
            smtp_username: "sender@example.com".into(),
            folder_count: 0,
            last_error: None,
        };
        let mut outgoing = OutgoingMessage {
            from_address: Some("alias@custom.example".into()),
            ..OutgoingMessage::default()
        };
        ensure_message_id(&account, &mut outgoing).unwrap();
        assert!(outgoing
            .message_id
            .as_deref()
            .is_some_and(|id| id.ends_with("@custom.example>")));
    }
}

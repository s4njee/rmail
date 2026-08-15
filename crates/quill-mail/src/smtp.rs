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
use lettre::{Message, SmtpTransport, Transport};
use quill_store::types::{Account, OutgoingMessage};

use crate::auth::Credential;
use crate::oauth_store::get_valid_access_token;

/// Construct an RFC 5322 MIME message from [`OutgoingMessage`].
pub fn build_message(account: &Account, outgoing: &OutgoingMessage) -> Result<Message, String> {
    let from_addr = outgoing
        .from_address
        .as_deref()
        .filter(|a| !a.trim().is_empty())
        .unwrap_or(&account.address);

    let addr: lettre::Address = from_addr
        .parse()
        .map_err(|e| format!("invalid from address: {e}"))?;
    let from = lettre::message::Mailbox::new(
        outgoing
            .from_name
            .clone()
            .filter(|n| !n.trim().is_empty()),
        addr,
    );

    let mut builder = Message::builder().from(from).subject(&outgoing.subject);

    if let Some(reply_to_str) = outgoing.reply_to.as_deref().filter(|r| !r.trim().is_empty()) {
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

    let has_html = outgoing.body_html.as_deref().map(|h| !h.trim().is_empty()).unwrap_or(false);

    if outgoing.attachments.is_empty() {
        if has_html {
            let html_body = outgoing.body_html.as_deref().unwrap();
            let alt = MultiPart::alternative()
                .singlepart(SinglePart::plain(outgoing.body.clone()))
                .singlepart(SinglePart::html(html_body.to_string()));
            builder.multipart(alt).map_err(|e| format!("build alternative body: {e}"))
        } else {
            builder
                .body(outgoing.body.clone())
                .map_err(|e| format!("build body: {e}"))
        }
    } else {
        let mut mixed = if has_html {
            let html_body = outgoing.body_html.as_deref().unwrap();
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

/// Map a common IMAP hostname to the provider's SMTP submission host. The
/// account only records the IMAP host, so password accounts used it for SMTP
/// too — which fails for every provider whose SMTP host differs (Gmail,
/// Outlook, Yahoo, iCloud, AOL). Self-hosted setups serve both protocols on
/// one host and are passed through unchanged. Single source of truth lives
/// with the provider presets (`crate::provider::smtp_host_for`).
fn smtp_host_for(imap_host: &str) -> String {
    crate::provider::smtp_host_for(imap_host)
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
            let creds = Credentials::new(account.address.clone(), password.clone());
            let smtp_host = smtp_host_for(&account.server);
            let mailer = if account.tls {
                SmtpTransport::relay(&smtp_host)
                    .map_err(|e| format!("smtp relay {smtp_host}: {e}"))?
                    .port(465)
            } else {
                SmtpTransport::starttls_relay(&smtp_host)
                    .map_err(|e| format!("smtp relay {smtp_host}: {e}"))?
                    .port(587)
            }
            .credentials(creds)
            .build();

            mailer.send(&message).map_err(|e| e.to_string())?;
            Ok(())
        }
        Credential::OAuth { address, provider } => {
            let access_token = get_valid_access_token(address, *provider).await?;
            // OAuth accounts store the IMAP host in `account.server`; submission
            // goes to the provider's dedicated SMTP host on 587 (STARTTLS).
            let smtp_host = provider.default_smtp_host();
            let mailer = SmtpTransport::builder_dangerous(smtp_host)
                .port(587)
                .tls(Tls::Required(
                    TlsParameters::new(smtp_host.to_string())
                        .map_err(|e| format!("smtp tls params: {e}"))?,
                ))
                .authentication(vec![Mechanism::Xoauth2])
                .credentials(Credentials::new(address.clone(), access_token))
                .build();

            mailer.send(&message).map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill_store::types::OutgoingAttachment;

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
            in_reply_to: Some("<parent-id@example.com>".into()),
            references: Some("<root-id@example.com> <parent-id@example.com>".into()),
            attachments: vec![OutgoingAttachment {
                filename: "notes.txt".into(),
                content_type: "text/plain".into(),
                data_base64: BASE64_STANDARD.encode(b"Notes content"),
            }],
            original_message_id: None,
            is_forward: None,
        };

        let msg = build_message(&account, &outgoing).unwrap();
        let formatted = String::from_utf8(msg.formatted()).unwrap();

        assert!(formatted.contains("From: sender@example.com"));
        assert!(formatted.contains("To: to@example.com"));
        assert!(formatted.contains("Cc: cc@example.com"));
        assert!(formatted.contains("Subject: Re: Discussion"));
        assert!(formatted.contains("In-Reply-To: <parent-id@example.com>"));
        assert!(formatted.contains("References: <root-id@example.com> <parent-id@example.com>"));
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
            body: "Your issue is resolved.\n-- \nJane Support Team".into(),
            body_html: Some("<p>Your issue is resolved.</p><br>-- <br><b>Jane Support Team</b>".into()),
            in_reply_to: None,
            references: None,
            attachments: vec![],
            original_message_id: Some(42),
            is_forward: None,
        };

        let msg = build_message(&account, &outgoing).unwrap();
        let formatted = String::from_utf8(msg.formatted()).unwrap();

        assert!(formatted.contains("support@customdomain.com"));
        assert!(formatted.contains("Jane Support"));
        assert!(formatted.contains("Reply-To: replies@customdomain.com") || formatted.contains("replies@customdomain.com"));
        assert!(formatted.contains("To: client@example.com"));
        assert!(formatted.contains("Subject: Support Request Resolved"));
        assert!(formatted.contains("Your issue is resolved."));
        assert!(formatted.contains("<b>Jane Support Team</b>"));
        assert!(formatted.contains("multipart/alternative"));
    }
}

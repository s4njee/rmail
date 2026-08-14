//! SMTP sending (Epic 12.3).
//!
//! Sends via the account's server (STARTTLS on 587, implicit TLS on 465 when
//! the account uses TLS) with the keychain password, then the sync engine
//! appends to the IMAP Sent folder on the next sync. Offline sends queue in a
//! visible outbox — that surface lands with compose (Epic 13).

use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use quill_store::types::Account;

/// Send an outgoing message. The password is read from the keychain by the
/// caller and never leaves this process.
pub fn send_email(
    account: &Account,
    to: &[String],
    subject: &str,
    body: &str,
    password: &str,
) -> Result<(), String> {
    let from = account
        .address
        .parse()
        .map_err(|e| format!("invalid from address: {e}"))?;
    let recipients: Vec<lettre::message::Mailbox> = to
        .iter()
        .map(|t| t.parse().map_err(|e| format!("invalid recipient {t}: {e}")))
        .collect::<Result<_, String>>()?;

    let mut builder = Message::builder().from(from).subject(subject);
    for recipient in &recipients {
        builder = builder.to(recipient.clone());
    }
    let message = builder.body(body.to_string()).map_err(|e| e.to_string())?;

    let creds = Credentials::new(account.address.clone(), password.to_string());
    let mailer = if account.tls {
        SmtpTransport::relay(&account.server)
            .map_err(|e| format!("smtp relay {}: {e}", account.server))?
            .port(465)
    } else {
        SmtpTransport::starttls_relay(&account.server)
            .map_err(|e| format!("smtp relay {}: {e}", account.server))?
            .port(587)
    }
    .credentials(creds)
    .build();

    mailer.send(&message).map_err(|e| e.to_string())?;
    Ok(())
}

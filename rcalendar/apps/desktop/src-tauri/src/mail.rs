//! iMIP handoff for T1.10.
//!
//! Almanac does not speak SMTP. Invitation payloads are generated in
//! `calendar-core` and handed to the parent rmail mailer (or the system
//! compose window) via an on-disk outbox. Set `ALMANAC_MAIL_COMMAND` to a
//! binary that receives the `.ics` path as its only argument to reuse an
//! existing mail transport.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use calendar_core::ImipEmailEnvelope;
use serde::{Deserialize, Serialize};

/// Result of preparing / handing off an iMIP message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImipDispatch {
    pub envelope: ImipEmailEnvelope,
    pub recipients: Vec<String>,
    pub outbox_path: Option<String>,
    pub mailed: bool,
}

/// Writes the `.ics` into `{dir}/imip-outbox/` and optionally invokes
/// `ALMANAC_MAIL_COMMAND` or a `mailto:` URL so a parent mailer can send it.
pub fn dispatch_imip(
    app_data_dir: &Path,
    envelope: ImipEmailEnvelope,
    recipients: Vec<String>,
) -> Result<ImipDispatch, String> {
    let outbox = app_data_dir.join("imip-outbox");
    fs::create_dir_all(&outbox).map_err(|e| format!("create imip outbox: {e}"))?;

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let safe_uid: String = envelope
        .ics_content
        .lines()
        .find(|l| l.to_ascii_uppercase().starts_with("UID:"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().replace(['/', '\\', ':'], "_"))
        .unwrap_or_else(|| "invite".into());
    let ics_path: PathBuf = outbox.join(format!("{stamp}-{safe_uid}.ics"));
    fs::write(&ics_path, &envelope.ics_content).map_err(|e| format!("write imip ics: {e}"))?;

    let meta = serde_json::json!({
        "subject": envelope.subject,
        "text_body": envelope.text_body,
        "mime_type": envelope.mime_type,
        "recipients": recipients,
        "ics_path": ics_path.to_string_lossy(),
    });
    let meta_path = ics_path.with_extension("json");
    fs::write(
        &meta_path,
        serde_json::to_vec_pretty(&meta).unwrap_or_default(),
    )
    .map_err(|e| format!("write imip meta: {e}"))?;

    let mut mailed = false;
    if let Ok(cmd) = std::env::var("ALMANAC_MAIL_COMMAND") {
        if !cmd.trim().is_empty() {
            let status = Command::new(&cmd)
                .arg(&ics_path)
                .status()
                .map_err(|e| format!("ALMANAC_MAIL_COMMAND `{cmd}`: {e}"))?;
            mailed = status.success();
        }
    }

    if !mailed && !recipients.is_empty() {
        // Last-resort compose window. The `.ics` lives in the outbox for a
        // parent rmail install to attach; mailto: itself cannot attach files.
        let to = recipients.join(",");
        let subject = urlencoding_lite(&envelope.subject);
        let body = urlencoding_lite(&envelope.text_body);
        let mailto = format!("mailto:{to}?subject={subject}&body={body}");
        let _ = open_url(&mailto);
    }

    Ok(ImipDispatch {
        envelope,
        recipients,
        outbox_path: Some(ics_path.to_string_lossy().into_owned()),
        mailed,
    })
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .status()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(url)
            .status()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

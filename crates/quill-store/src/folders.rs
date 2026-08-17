//! Folder-tree helpers (T0.1): hierarchy parsing, namespace inference, and
//! kind classification shared by the store and the IMAP sync engine.

use crate::types::FolderKind;

/// Last path component of an IMAP mailbox name.
pub fn display_name_of(server_name: &str, delimiter: &str) -> String {
    if delimiter.is_empty() {
        return server_name.to_string();
    }
    server_name
        .rsplit_once(delimiter)
        .map(|(_, leaf)| leaf.to_string())
        .filter(|leaf| !leaf.is_empty())
        .unwrap_or_else(|| server_name.to_string())
}

/// Parent mailbox name, if the path has more than one component.
pub fn parent_server_name(server_name: &str, delimiter: &str) -> Option<String> {
    if delimiter.is_empty() {
        return None;
    }
    server_name
        .rsplit_once(delimiter)
        .map(|(parent, _)| parent.to_string())
        .filter(|parent| !parent.is_empty())
}

/// Build a child mailbox name under `parent` (or a top-level name).
pub fn child_server_name(parent: Option<&str>, name: &str, delimiter: &str) -> String {
    match parent {
        Some(p) if !p.is_empty() => {
            if delimiter.is_empty() {
                format!("{p}{name}")
            } else {
                format!("{p}{delimiter}{name}")
            }
        }
        _ => name.to_string(),
    }
}

/// Rewrite `server_name` so its prefix `from` becomes `to` (rename/move of a
/// folder and every descendant).
pub fn rewrite_prefix(server_name: &str, from: &str, to: &str, delimiter: &str) -> String {
    if server_name == from {
        return to.to_string();
    }
    if delimiter.is_empty() {
        return server_name.to_string();
    }
    let prefix = format!("{from}{delimiter}");
    if let Some(rest) = server_name.strip_prefix(&prefix) {
        if to.is_empty() {
            rest.to_string()
        } else {
            format!("{to}{delimiter}{rest}")
        }
    } else {
        server_name.to_string()
    }
}

/// Infer the IMAP namespace prefix from a mailbox name.
///
/// Gmail's `[Gmail]` / `[Google Mail]`, Courier-style `INBOX.`, and the common
/// `Other Users` / `Shared Folders` prefixes are recognised. Everything else
/// is personal (`""`).
pub fn infer_namespace(server_name: &str, delimiter: &str) -> String {
    let lower = server_name.to_ascii_lowercase();
    for known in ["[gmail]", "[google mail]", "other users", "shared folders", "shared"] {
        if lower == known || lower.starts_with(&format!("{known}{delimiter}")) {
            let end = known.len().min(server_name.len());
            return server_name[..end].to_string();
        }
    }
    String::new()
}

/// Local storage key for `messages.folder`. Special kinds keep their
/// canonical display name so unified views and existing rows keep working;
/// custom mailboxes use the server name so two labels never collide.
pub fn local_name_for(server_name: &str, kind: FolderKind) -> String {
    if kind == FolderKind::Custom {
        server_name.to_string()
    } else if kind.is_virtual() {
        kind.display_name().to_string()
    } else if server_name.eq_ignore_ascii_case("inbox") {
        "Inbox".to_string()
    } else {
        kind.display_name().to_string()
    }
}

/// True when `name` (the last path component, lowercased) is a well-known
/// special mailbox. Used so `Work/All Hands` is Custom, not Archive.
fn leaf_is(name: &str, needles: &[&str]) -> bool {
    let leaf = name
        .rsplit(['/', '.', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    needles.iter().any(|n| leaf == *n || leaf.starts_with(&format!("{n} ")))
}

/// Classify a mailbox from RFC 6154 SPECIAL-USE attributes and conservative
/// name heuristics. Unknown names are [`FolderKind::Custom`] — never Inbox.
pub fn classify_folder_kind(name: &str, attribute_debug: impl Iterator<Item = String>) -> FolderKind {
    for attr in attribute_debug {
        let debug_str = attr.to_ascii_lowercase();
        if debug_str.contains("inbox") {
            return FolderKind::Inbox;
        }
        if debug_str.contains("draft") {
            return FolderKind::Drafts;
        }
        if debug_str.contains("sent") {
            return FolderKind::Sent;
        }
        if debug_str.contains("junk") || debug_str.contains("spam") {
            return FolderKind::Junk;
        }
        if debug_str.contains("trash") || debug_str.contains("bin") || debug_str.contains("deleted")
        {
            return FolderKind::Trash;
        }
        if debug_str.contains("archive") || debug_str.contains("allmail") {
            return FolderKind::Archive;
        }
        if debug_str.contains("flagged") || debug_str.contains("starred") {
            return FolderKind::Starred;
        }
    }

    let lower = name.to_ascii_lowercase();
    if lower == "inbox" || lower.ends_with("/inbox") || lower.ends_with(".inbox") {
        FolderKind::Inbox
    } else if leaf_is(name, &["drafts", "draft"]) {
        FolderKind::Drafts
    } else if leaf_is(name, &["sent", "sent mail", "sent messages", "sent items"]) {
        FolderKind::Sent
    } else if leaf_is(name, &["junk", "spam", "bulk", "junk mail", "bulk mail"]) {
        FolderKind::Junk
    } else if leaf_is(name, &["trash", "bin", "deleted", "deleted items", "deleted messages"]) {
        FolderKind::Trash
    } else if leaf_is(name, &["archive", "all mail"]) {
        FolderKind::Archive
    } else if leaf_is(name, &["starred", "flagged"]) {
        FolderKind::Starred
    } else {
        FolderKind::Custom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parent() {
        assert_eq!(display_name_of("Work/Projects/Q1", "/"), "Q1");
        assert_eq!(
            parent_server_name("Work/Projects/Q1", "/").as_deref(),
            Some("Work/Projects")
        );
        assert_eq!(parent_server_name("INBOX", "/"), None);
        assert_eq!(display_name_of("INBOX", "/"), "INBOX");
        assert_eq!(
            child_server_name(Some("Work"), "Projects", "/"),
            "Work/Projects"
        );
        assert_eq!(child_server_name(None, "Work", "/"), "Work");
    }

    #[test]
    fn rewrite_renames_descendants() {
        assert_eq!(
            rewrite_prefix("Work/Projects/Q1", "Work/Projects", "Work/Old", "/"),
            "Work/Old/Q1"
        );
        assert_eq!(
            rewrite_prefix("Work/Projects", "Work/Projects", "Archive/Work", "/"),
            "Archive/Work"
        );
        assert_eq!(rewrite_prefix("Inbox", "Work", "X", "/"), "Inbox");
    }

    #[test]
    fn namespace_gmail_and_shared() {
        assert_eq!(infer_namespace("[Gmail]/Sent Mail", "/"), "[Gmail]");
        assert_eq!(infer_namespace("INBOX", "/"), "");
        assert_eq!(infer_namespace("Other Users/ada/INBOX", "/"), "Other Users");
        assert_eq!(infer_namespace("Work/Projects", "/"), "");
    }

    #[test]
    fn local_name_keeps_specials_canonical() {
        assert_eq!(local_name_for("INBOX", FolderKind::Inbox), "Inbox");
        assert_eq!(
            local_name_for("[Gmail]/Sent Mail", FolderKind::Sent),
            "Sent"
        );
        assert_eq!(
            local_name_for("Work/Projects", FolderKind::Custom),
            "Work/Projects"
        );
    }

    #[test]
    fn classify_does_not_collapse_custom_to_inbox() {
        assert_eq!(
            classify_folder_kind("Receipts", std::iter::empty()),
            FolderKind::Custom
        );
        assert_eq!(
            classify_folder_kind("Work/All Hands", std::iter::empty()),
            FolderKind::Custom
        );
        assert_eq!(
            classify_folder_kind("INBOX", std::iter::empty()),
            FolderKind::Inbox
        );
        assert_eq!(
            classify_folder_kind("Sent Messages", std::iter::empty()),
            FolderKind::Sent
        );
        assert_eq!(
            classify_folder_kind("All Mail", std::iter::empty()),
            FolderKind::Archive
        );
        assert_eq!(
            classify_folder_kind("[Gmail]/Spam", std::iter::empty()),
            FolderKind::Junk
        );
    }

    #[test]
    fn unknown_kind_string_is_custom() {
        assert_eq!(FolderKind::from_str("custom"), FolderKind::Custom);
        assert_eq!(FolderKind::from_str("projects"), FolderKind::Custom);
        assert_eq!(FolderKind::from_str("inbox"), FolderKind::Inbox);
        assert_eq!(FolderKind::Custom.as_str(), "custom");
    }
}

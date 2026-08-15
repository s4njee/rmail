//! JWZ-style message threading and subject normalization (Roadmap 3.2).

/// Normalizes a subject line by stripping prefix markers (`Re:`, `Fwd:`, `Fw:`, `[tag]`, etc.)
/// and collapsing internal whitespace.
pub fn normalize_subject(subject: &str) -> String {
    let mut s = subject.trim();
    let mut changed = true;

    while changed {
        changed = false;

        // Strip bracketed prefixes like [quill-dev] or [mailing-list]
        if s.starts_with('[') {
            if let Some(close_idx) = s.find(']') {
                s = s[close_idx + 1..].trim_start();
                changed = true;
            }
        }

        // Strip Re:, Fwd:, Fw:, etc.
        let lower = s.to_ascii_lowercase();
        for prefix in ["re:", "fwd:", "fw:", "re-", "aw:", "sv:"] {
            if lower.starts_with(prefix) {
                s = s[prefix.len()..].trim_start();
                changed = true;
                break;
            }
        }
    }

    // Collapse multiple whitespace
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Computes or resolves a stable thread ID for a message using JWZ-style references
/// and subject fallback.
pub fn compute_thread_id(
    in_reply_to: Option<&str>,
    references: Option<&str>,
    subject: &str,
) -> String {
    // If References is present, use the very first Message-ID in References (thread root)
    if let Some(refs) = references {
        if let Some(first_ref) = refs.split_whitespace().next() {
            let clean = first_ref.trim_matches(|c| c == '<' || c == '>');
            if !clean.is_empty() {
                return format!("th_ref_{clean}");
            }
        }
    }

    // If In-Reply-To is present, use it as thread id
    if let Some(reply_to) = in_reply_to {
        let clean = reply_to.trim().trim_matches(|c| c == '<' || c == '>');
        if !clean.is_empty() {
            return format!("th_ref_{clean}");
        }
    }

    // Fallback: Normalized subject hash
    let norm = normalize_subject(subject);
    if norm.is_empty() {
        format!("th_subj_empty")
    } else {
        format!("th_subj_{}", norm.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_subject() {
        assert_eq!(
            normalize_subject("Re: [team] Project Roadmap"),
            "Project Roadmap"
        );
        assert_eq!(
            normalize_subject("Fwd: Re: Fw: Meeting Notes"),
            "Meeting Notes"
        );
        assert_eq!(normalize_subject("  [quill]   Weekly 1:1  "), "Weekly 1:1");
        assert_eq!(normalize_subject("Hello World"), "Hello World");
    }

    #[test]
    fn test_compute_thread_id_references() {
        let refs = "<msg-root-123@example.com> <msg-reply-456@example.com>";
        let tid = compute_thread_id(
            Some("<msg-reply-456@example.com>"),
            Some(refs),
            "Re: Update",
        );
        assert_eq!(tid, "th_ref_msg-root-123@example.com");
    }

    #[test]
    fn test_compute_thread_id_subject_fallback() {
        let tid1 = compute_thread_id(None, None, "Design System v2");
        let tid2 = compute_thread_id(None, None, "Re: [quill] Design System v2");
        assert_eq!(tid1, tid2);
    }
}

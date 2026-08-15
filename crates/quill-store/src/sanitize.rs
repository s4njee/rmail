//! HTML mail sanitization (Epic 7.3).
//!
//! Runs server-side of the IPC boundary: raw mail HTML is sanitized here, and
//! only the sanitized output ever reaches the webview, which additionally
//! renders it in a sandboxed iframe — two independent layers.
//!
//! Defense posture:
//! - Ammonia removes scripts, inline event handlers, `javascript:` URLs,
//!   `<style>` blocks, forms, iframes, and any unknown/active element.
//! - Remote images (`http(s)` `src`) are rewritten to a transparent
//!   placeholder with the original URL parked in `data-src`; nothing loads
//!   until the user's per-sender "Load images" choice swaps them back — and
//!   the iframe's CSP forbids remote loads regardless, so CSS `url()` phone
//!   homes are blocked too.
//! - Links keep their `href` (`http`/`https`/`mailto` only) but the sandbox
//!   makes them inert; the app's injected iframe script surfaces a click
//!   target to the parent, which opens it in the OS browser after showing
//!   the real destination.

use ammonia::Builder;

/// Transparent 1×1 GIF used in place of a blocked remote image.
const IMAGE_PLACEHOLDER: &str =
    "data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw==";

/// The sanitized HTML plus how many remote images were parked.
pub struct SanitizedHtml {
    pub html: String,
    pub remote_images: usize,
}

/// Sanitize raw mail HTML. Never panics on malformed input.
pub fn sanitize_html(raw: &str) -> SanitizedHtml {
    // Allow inline `style` so mail renders like its author intended — CSS is
    // inert here, and any remote resource a style tries to load is blocked by
    // the iframe's CSP, not by this sanitizer.
    let cleaned = Builder::default()
        .add_generic_attributes(&["style"])
        .link_rel(Some("noopener noreferrer"))
        .clean(raw)
        .to_string();
    let (html, remote_images) = rewrite_remote_images(&cleaned);
    SanitizedHtml {
        html,
        remote_images,
    }
}

/// Maximum length of a stored list-row snippet.
pub const SNIPPET_MAX: usize = 200;

/// A list-row preview from a message's plain-text body, falling back to the
/// HTML body with markup stripped for HTML-only mail. Whitespace is collapsed
/// and the result is capped at [`SNIPPET_MAX`] chars.
pub fn snippet_from_bodies(plain: &str, html: Option<&str>) -> String {
    let source = if plain.trim().is_empty() {
        html.map(html_to_text).unwrap_or_default()
    } else {
        plain.to_string()
    };
    let collapsed = source.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(SNIPPET_MAX).collect()
}

/// Reduce HTML markup to readable text for a snippet. HTML-only messages
/// expose their markup as the body; a tag-strip recovers the visible text,
/// with block-level tags acting as word separators.
pub fn html_to_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            _ => out.push(c),
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

/// Decode RFC 2047 encoded-words (`=?charset?Q?…?=` / `=?charset?B?…?=`)
/// interleaved with plain text, as found in Subject and From headers. Text
/// that isn't an encoded-word passes through unchanged; unknown charsets fall
/// back to a lossy UTF-8 decode.
pub fn decode_rfc2047(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(eq) = rest.find("=?") {
        out.push_str(&rest[..eq]);
        let word = &rest[eq..];
        match decode_one_encoded_word(word) {
            Some((decoded, consumed)) => {
                out.push_str(&decoded);
                rest = &word[consumed..];
                // RFC 2047 §6.2: linear whitespace between two adjacent
                // encoded-words is ignored, so "…Confidently?= =?with…" joins
                // as one space (from the trailing `_`) rather than two.
                let after_ws = rest.trim_start_matches([' ', '\t', '\r', '\n']);
                if after_ws.len() < rest.len() && after_ws.starts_with("=?") {
                    rest = after_ws;
                }
            }
            None => {
                // Not a well-formed encoded-word; keep the literal text.
                out.push_str("=?");
                rest = &word[2..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Decode one `=?charset?X?data?=` encoded-word starting at `input[0]`.
/// Returns the decoded text and the number of chars consumed (including the
/// trailing `?=`).
fn decode_one_encoded_word(input: &str) -> Option<(String, usize)> {
    let charset_end = input[2..].find('?')? + 2;
    let charset = &input[2..charset_end];
    let encoding = *input.as_bytes().get(charset_end + 1)?;
    // `=?charset?X?data?=` — data begins after the `?` that closes the
    // encoding letter.
    let data_start = charset_end + 3;
    let end_rel = input[data_start..].find("?=")?;
    let data = &input[data_start..data_start + end_rel];

    let bytes = match encoding {
        b'Q' | b'q' => decode_quoted_printable_word(data),
        b'B' | b'b' => {
            use base64::engine::general_purpose::STANDARD;
            use base64::Engine as _;
            STANDARD.decode(data.as_bytes()).ok()?
        }
        _ => return None,
    };
    let text = decode_charset(&bytes, charset);
    Some((text, data_start + end_rel + 2))
}

/// RFC 2047 Q-encoding: `_` is a space, `=XX` is a byte in hex.
fn decode_quoted_printable_word(data: &str) -> Vec<u8> {
    let bytes = data.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'_' => out.push(b' '),
            b'=' if i + 2 < bytes.len() => match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 2;
                }
                _ => out.push(b'='),
            },
            c => out.push(c),
        }
        i += 1;
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn decode_charset(bytes: &[u8], charset: &str) -> String {
    match charset.to_ascii_lowercase().as_str() {
        "iso-8859-1" | "latin1" | "latin-1" | "iso_8859-1" | "windows-1252" => {
            bytes.iter().map(|&b| b as char).collect()
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Replace remote `src` on `<img>` elements with the placeholder, parking the
/// original URL in `data-src`. Returns the rewritten HTML and the count.
fn rewrite_remote_images(html: &str) -> (String, usize) {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut remote = 0;
    loop {
        let Some(img_rel) = rest.find("<img") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..img_rel]);
        let after = &rest[img_rel..];
        let Some(gt) = after.find('>') else {
            out.push_str(after);
            break;
        };
        let tag = &after[..=gt];
        let (rewritten, was_remote) = rewrite_img_src(tag);
        if was_remote {
            remote += 1;
        }
        out.push_str(&rewritten);
        rest = &after[gt + 1..];
    }
    (out, remote)
}

/// Rewrite one `<img ...>` tag: park a remote `src` in `data-src`.
fn rewrite_img_src(tag: &str) -> (String, bool) {
    let Some(src_pos) = tag.find("src=\"") else {
        return (tag.to_string(), false);
    };
    let src_start = src_pos + 5;
    let Some(quote_rel) = tag[src_start..].find('"') else {
        return (tag.to_string(), false);
    };
    let src_end = src_start + quote_rel;
    let url = &tag[src_start..src_end];
    if url.starts_with("http://") || url.starts_with("https://") {
        let rewritten = format!(
            "{}src=\"{}\" data-src=\"{}\"{}",
            &tag[..src_pos],
            IMAGE_PLACEHOLDER,
            url,
            &tag[src_end + 1..]
        );
        (rewritten, true)
    } else {
        (tag.to_string(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(raw: &str) -> String {
        sanitize_html(raw).html
    }

    #[test]
    fn strips_script_tags_and_content() {
        let out = clean(r#"<p>hi</p><script>alert(1)</script>"#);
        assert!(!out.contains("<script"));
        assert!(!out.contains("alert"));
        assert!(out.contains("<p>hi</p>"));
    }

    #[test]
    fn strips_inline_event_handlers() {
        let out = clean(r#"<p onclick="alert(1)" onmouseover="steal()">hi</p>"#);
        assert!(!out.contains("onclick"));
        assert!(!out.contains("onmouseover"));
        assert!(out.contains("hi"));
    }

    #[test]
    fn strips_javascript_urls() {
        let out = clean(r#"<a href="javascript:alert(1)">x</a>"#);
        assert!(!out.contains("javascript:"));
    }

    #[test]
    fn strips_css_exfiltration() {
        let out =
            clean(r#"<style>body { background: url(https://evil.example/x) }</style><p>hi</p>"#);
        assert!(!out.contains("<style"));
        assert!(!out.contains("evil.example"));
    }

    #[test]
    fn strips_meta_refresh() {
        let out =
            clean(r#"<meta http-equiv="refresh" content="0; url=https://evil.example"><p>hi</p>"#);
        assert!(!out.contains("refresh"));
        assert!(!out.contains("evil.example"));
    }

    #[test]
    fn strips_svg_embedded_script() {
        let out = clean(r#"<svg onload="alert(1)"><script>alert(2)</script></svg><p>hi</p>"#);
        assert!(!out.contains("<script"));
        assert!(!out.contains("alert"));
        assert!(!out.contains("onload"));
    }

    #[test]
    fn strips_srcdoc_nesting() {
        let out = clean(r#"<iframe srcdoc="<script>alert(1)</script>"></iframe><p>hi</p>"#);
        assert!(!out.contains("iframe"));
        assert!(!out.contains("alert"));
    }

    #[test]
    fn strips_forms() {
        let out = clean(r#"<form action="https://evil.example"><input name="x"></form><p>hi</p>"#);
        assert!(!out.contains("<form"));
        assert!(!out.contains("<input"));
    }

    #[test]
    fn keeps_benign_markup() {
        let out = clean(
            r#"<p>Hello <b>world</b>, see <a href="https://example.com">link</a>.</p><ul><li>one</li></ul>"#,
        );
        assert!(out.contains("<p>Hello"));
        assert!(out.contains("<b>world</b>"));
        assert!(out.contains("href=\"https://example.com\""));
        assert!(out.contains("<ul>"));
        assert!(out.contains("<li>"));
    }

    #[test]
    fn rewrites_remote_images_and_counts_them() {
        let result = sanitize_html(r#"<p><img src="https://example.com/a.png" alt="a"></p>"#);
        assert_eq!(result.remote_images, 1);
        // The img's src becomes the placeholder; the URL is parked in data-src.
        assert!(result
            .html
            .contains(&format!("src=\"{IMAGE_PLACEHOLDER}\"")));
        assert!(result
            .html
            .contains("data-src=\"https://example.com/a.png\""));
        assert!(!result.html.contains(" src=\"https://"));
    }

    #[test]
    fn leaves_local_images_alone() {
        let result = sanitize_html(r#"<img src="cid:logo" alt="logo">"#);
        assert_eq!(result.remote_images, 0);
        assert!(!result.html.contains("data-src="));
    }

    #[test]
    fn allows_inline_style_but_drops_style_elements() {
        assert!(clean(r#"<p style="color: red">hi</p>"#).contains("style=\"color: red\""));
        assert!(!clean(r#"<style>p { color: red }</style>"#).contains("<style"));
    }

    #[test]
    fn snippet_prefers_plain_text() {
        assert_eq!(
            snippet_from_bodies("plain body", Some("<p>html</p>")),
            "plain body"
        );
    }

    #[test]
    fn snippet_strips_html_markup() {
        let s = snippet_from_bodies("", Some("<!DOCTYPE html><p>Hi there</p>"));
        assert_eq!(s, "Hi there");
    }

    #[test]
    fn html_to_text_keeps_block_text_separated() {
        assert_eq!(html_to_text("<p>a</p><p>b</p>"), " a  b ");
    }

    #[test]
    fn snippet_caps_length() {
        let long = "x".repeat(SNIPPET_MAX * 4);
        assert_eq!(snippet_from_bodies(&long, None).len(), SNIPPET_MAX);
    }

    #[test]
    fn decodes_rfc2047_q_encoded_utf8() {
        // The exact shape that was showing up as a subject.
        let s = decode_rfc2047(
            "=?UTF-8?Q?=F0=9F=9A=98_Sanjee,_Drive_Confidently_?= =?UTF-8?Q?with_FREE_Duralast_Brake_Pads?=",
        );
        assert_eq!(s, "🚘 Sanjee, Drive Confidently with FREE Duralast Brake Pads");
    }

    #[test]
    fn decodes_rfc2047_base64() {
        assert_eq!(
            decode_rfc2047("=?ISO-8859-1?B?SWYgeW91IGNhbiByZWFkIHRoaXMgeW8=?="),
            "If you can read this yo"
        );
    }

    #[test]
    fn decodes_rfc2047_latin1() {
        assert_eq!(
            decode_rfc2047("=?ISO-8859-1?Q?Patrik_F=E4ltstr=F6m?="),
            "Patrik Fältström"
        );
    }

    #[test]
    fn rfc2047_leaves_plain_and_malformed_text_alone() {
        assert_eq!(decode_rfc2047("Just a normal subject"), "Just a normal subject");
        // A bare "=?" that isn't a well-formed encoded-word is left intact.
        assert_eq!(decode_rfc2047("a =?not-well-formed"), "a =?not-well-formed");
        // Encoded-word followed by plain text.
        assert_eq!(decode_rfc2047("=?UTF-8?Q?Hi?= there"), "Hi there");
    }
}

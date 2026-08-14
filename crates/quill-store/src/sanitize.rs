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
}

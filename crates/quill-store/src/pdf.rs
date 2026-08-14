//! Minimal valid PDF generator for demo attachments.
//!
//! The mock's attachment opens via the OS viewer (Epic 7.4), so the demo file
//! must actually be a readable PDF — not zero padding. This builds a one-page
//! PDF with a single line of text and pads it to the size the store advertises
//! (248 KB) with trailing whitespace, which the PDF spec says readers ignore
//! after the `%%EOF` marker.

/// A valid one-page PDF whose byte length is `target`.
pub fn placeholder(target: usize) -> Vec<u8> {
    let content = b"BT /F1 24 Tf 72 720 Td (Quill demo attachment) Tj ET\n";
    let mut stream_obj = format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream_obj.extend_from_slice(content);
    stream_obj.extend_from_slice(b"\nendstream\nendobj\n");

    let objs: Vec<Vec<u8>> = vec![
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_vec(),
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_vec(),
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n".to_vec(),
        stream_obj,
        b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_vec(),
    ];

    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objs.len());
    for obj in &objs {
        offsets.push(out.len());
        out.extend_from_slice(obj);
    }

    let xref_offset = out.len();
    let mut tail = format!("xref\n0 {}\n", objs.len() + 1);
    tail.push_str("0000000000 65535 f \n");
    for off in &offsets {
        tail.push_str(&format!("{off:010} 00000 n \n"));
    }
    tail.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        objs.len() + 1
    ));
    out.extend_from_slice(tail.as_bytes());

    // Pad with whitespace after %%EOF (ignored by readers) so the file's byte
    // length matches the size the store advertises.
    let padding = target.saturating_sub(out.len());
    if padding > 0 {
        out.resize(out.len() + padding, b'\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_a_valid_sized_pdf() {
        let pdf = placeholder(253_952); // 248 KB, the mock's attachment size
        assert_eq!(pdf.len(), 253_952);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        let end = pdf
            .windows(6)
            .position(|w| w == b"%%EOF\n")
            .expect("has EOF marker");
        assert!(pdf[..=end].windows(4).any(|w| w == b"xref"));
        assert!(pdf[..=end].windows(9).any(|w| w == b"startxref"));
    }
}

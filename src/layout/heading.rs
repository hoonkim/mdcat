/// Heading scale for kitty OSC 66 text-sizing.
///
/// `s` is the integer cell-scale (1–7); a glyph occupies `s`×`s` cells.
/// `n`/`d` are an optional fractional scale the protocol supports, but this
/// kitty build only reliably renders the integer `s` (fractional scales were
/// observed to collapse to `s`), so headings use distinct integer scales.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub s: u8,
    pub n: u8,
    pub d: u8,
    pub bold: bool,
}

/// Map a Markdown heading level (1–6) to a kitty text-size Scale.
///
/// Integer scales only, so the levels are visibly distinct on terminals that
/// ignore fractional text sizing:
///
/// | level | s | bold  | size      |
/// |-------|---|-------|-----------|
/// | 1     | 3 | false | 3×        |
/// | 2     | 2 | false | 2×        |
/// | 3–6   | 1 | true  | bold (1×) |
pub fn scale_for(level: u8) -> Scale {
    match level {
        1 => Scale { s: 3, n: 0, d: 0, bold: false },
        2 => Scale { s: 2, n: 0, d: 0, bold: false },
        // H3–H6: body size, bold (no integer scale between 1 and 2 exists).
        _ => Scale { s: 1, n: 0, d: 0, bold: true },
    }
}

/// Return the effective linear scale factor for wrap-width / line-height math.
///
/// When the scale has a fractional component (`n > 0 && d > 0`), the factor is
/// `n/d`; otherwise it is `s`. Headings currently use integer `s` only, so this
/// returns `s` for them, but the fractional path is kept for completeness.
pub fn effective_factor(sc: &Scale) -> f32 {
    if sc.n > 0 && sc.d > 0 {
        sc.n as f32 / sc.d as f32
    } else {
        sc.s as f32
    }
}

/// Encode a single text fragment as a kitty OSC 66 text-sizing escape.
///
/// Format: `ESC ] 66 ; <meta> ; <text> ESC \`
///
/// The meta string uses colon-separated key=value pairs. When the scale is
/// fractional the keys `n` and `d` are appended after `s`.
///
/// The caller is responsible for splitting `text` so that each call stays
/// under 4 096 bytes.
pub fn osc66(text: &str, sc: &Scale) -> String {
    let mut meta = format!("s={}", sc.s);
    if sc.n > 0 && sc.d > 0 {
        meta.push_str(&format!(":n={}:d={}", sc.n, sc.d));
    }
    format!("\x1b]66;{};{}\x1b\\", meta, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h1_is_scale_3() {
        let sc = scale_for(1);
        assert_eq!((sc.s, sc.n, sc.d), (3, 0, 0));
        assert!(!sc.bold);
        assert!((effective_factor(&sc) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn h2_is_scale_2() {
        let sc = scale_for(2);
        assert_eq!((sc.s, sc.n, sc.d), (2, 0, 0));
        assert!(!sc.bold);
        assert!((effective_factor(&sc) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn h3_is_bold_body_size() {
        let sc = scale_for(3);
        assert_eq!(sc.s, 1);
        assert!(sc.bold);
        assert!((effective_factor(&sc) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn h4_is_plain_bold() {
        let sc = scale_for(4);
        assert_eq!(sc.s, 1);
        assert!(sc.bold);
        assert!((effective_factor(&sc) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn osc66_encodes_integer_scale() {
        let sc = scale_for(1);
        let out = osc66("Hi", &sc);
        assert_eq!(out, "\x1b]66;s=3;Hi\x1b\\");
    }

    #[test]
    fn osc66_encodes_fractional_when_present() {
        // osc66 still supports fractional encoding even though headings don't use it.
        let sc = Scale { s: 1, n: 3, d: 2, bold: false };
        let out = osc66("Hi", &sc);
        assert_eq!(out, "\x1b]66;s=1:n=3:d=2;Hi\x1b\\");
    }
}

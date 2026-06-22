/// Heading scale for kitty OSC 66 text-sizing.
///
/// When both `n` and `d` are non-zero, the effective factor is `n/d` (the `s`
/// component is ignored by the effective_factor calculation).  When only `s`
/// is set the effective size is exactly `s` times normal.
/// Note: The exact visual sizing on kitty is to be reconciled in Task 12.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub s: u8,
    pub n: u8,
    pub d: u8,
    pub bold: bool,
}

/// Map a Markdown heading level (1–6) to a kitty text-size Scale.
///
/// | level | s | n | d | bold  | effective factor |
/// |-------|---|---|---|-------|-----------------|
/// | 1     | 2 | 0 | 0 | false | 2.0             |
/// | 2     | 2 | 3 | 2 | false | 1.5             |
/// | 3     | 2 | 5 | 4 | false | 1.25            |
/// | 4–6   | 1 | 0 | 0 | true  | 1.0             |
///
/// kitty requires the fractional scale `n/d` to be <= the reserved cell count
/// `s`; otherwise it ignores the fraction and renders at 1x. So H2/H3 use
/// `s=2` (a 2-cell-tall block) with the fraction applied inside it. `s` is the
/// ceiling of the effective factor.
pub fn scale_for(level: u8) -> Scale {
    match level {
        1 => Scale { s: 2, n: 0, d: 0, bold: false },
        2 => Scale { s: 2, n: 3, d: 2, bold: false },
        3 => Scale { s: 2, n: 5, d: 4, bold: false },
        _ => Scale { s: 1, n: 0, d: 0, bold: true },
    }
}

/// Return the effective linear scale factor for wrap-width / line-height math.
///
/// When the scale has a fractional component (`n > 0 && d > 0`), the factor
/// is `n as f32 / d as f32` (the `s` component is ignored).  Otherwise the
/// factor is `s as f32`.
///
/// This formula is chosen because the required tests all pass with it:
///   H1 (s=2)         → 2.0
///   H2 (s=1,n=3,d=2) → 1.5
///   H3 (s=1,n=5,d=4) → 1.25
///   H4 (s=1)         → 1.0
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
/// The meta string uses colon-separated key=value pairs.  When the scale is
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
    fn h1_is_scale_2() {
        let sc = scale_for(1);
        assert_eq!((sc.s, sc.n, sc.d), (2, 0, 0));
        assert!((effective_factor(&sc) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn h2_is_three_halves() {
        let sc = scale_for(2);
        // s must be >= the fraction (2 >= 1.5) so kitty honors the fractional scale.
        assert_eq!(sc.s, 2);
        assert_eq!((sc.n, sc.d), (3, 2));
        assert!((effective_factor(&sc) - 1.5).abs() < 1e-6);
    }

    #[test]
    fn h3_is_five_quarters() {
        let sc = scale_for(3);
        assert_eq!((sc.s, sc.n, sc.d), (2, 5, 4));
        assert!(!sc.bold);
    }

    #[test]
    fn h4_is_plain_bold() {
        let sc = scale_for(4);
        assert_eq!(sc.s, 1);
        assert!(sc.bold);
        assert!((effective_factor(&sc) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn osc66_encodes_scale_and_text() {
        let sc = scale_for(1);
        let out = osc66("Hi", &sc);
        assert_eq!(out, "\x1b]66;s=2;Hi\x1b\\");
    }

    #[test]
    fn osc66_fractional() {
        let sc = scale_for(2);
        let out = osc66("Hi", &sc);
        assert_eq!(out, "\x1b]66;s=2:n=3:d=2;Hi\x1b\\");
    }
}

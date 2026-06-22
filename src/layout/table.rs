use crate::ir::{Align, Inline};
use crate::style::{Span, Style, StyledLine};
use crate::layout::inline::wrap_inlines;

pub fn render_table(
    aligns: &[Align],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    max_width: usize,
) -> Vec<StyledLine> {
    let ncol = header.len().max(aligns.len()).max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if ncol == 0 { return vec![]; }

    // 1) Measure the natural width of each cell (single-line assumed) → max per column
    let natural = |inl: &[Inline]| -> usize {
        wrap_inlines(inl, usize::MAX, Style::default()).iter().map(|l| l.width()).max().unwrap_or(0)
    };
    let mut col_w = vec![0usize; ncol];
    for (i, c) in header.iter().enumerate() { col_w[i] = col_w[i].max(natural(c)); }
    for row in rows {
        for (i, c) in row.iter().enumerate() { col_w[i] = col_w[i].max(natural(c)); }
    }

    // 2) Border cost: ncol+1 vertical bars + ncol*2 padding spaces (one on each side per cell)
    let chrome = (ncol + 1) + ncol * 2;
    let avail = max_width.saturating_sub(chrome);
    let total: usize = col_w.iter().sum();
    if total > avail && total > 0 {
        // Proportionally shrink (minimum 1)
        for w in col_w.iter_mut() {
            *w = ((*w * avail) / total).max(1);
        }
    }

    let mut out = Vec::new();
    out.push(border_line('┌', '┬', '┐', &col_w));
    out.extend(render_row(header, aligns, &col_w, true));
    out.push(border_line('├', '┼', '┤', &col_w));
    for row in rows {
        out.extend(render_row(row, aligns, &col_w, false));
    }
    out.push(border_line('└', '┴', '┘', &col_w));
    out
}

fn border_line(l: char, m: char, r: char, col_w: &[usize]) -> StyledLine {
    let mut s = String::new();
    s.push(l);
    for (i, w) in col_w.iter().enumerate() {
        s.push_str(&"─".repeat(w + 2));
        s.push(if i + 1 == col_w.len() { r } else { m });
    }
    StyledLine(vec![Span { text: s, style: Style::default() }])
}

// Wrap cell contents to column width → multi-line rows are padded vertically
fn render_row(cells: &[Vec<Inline>], aligns: &[Align], col_w: &[usize], head: bool) -> Vec<StyledLine> {
    let base = Style { bold: head, ..Style::default() };
    let wrapped: Vec<Vec<StyledLine>> = (0..col_w.len()).map(|i| {
        let empty = Vec::new();
        let inl = cells.get(i).unwrap_or(&empty);
        let mut w = wrap_inlines(inl, col_w[i].max(1), base);
        if w.is_empty() { w.push(StyledLine::default()); }
        w
    }).collect();
    let height = wrapped.iter().map(|c| c.len()).max().unwrap_or(1);

    let mut lines = Vec::new();
    for r in 0..height {
        let mut spans = vec![Span { text: "│ ".into(), style: Style::default() }];
        for (i, w) in col_w.iter().enumerate() {
            let empty = StyledLine::default();
            let line = wrapped[i].get(r).unwrap_or(&empty);
            let used = line.width();
            let pad = w.saturating_sub(used);
            let align = aligns.get(i).cloned().unwrap_or(Align::None);
            let (lp, rp) = match align {
                Align::Right => (pad, 0),
                Align::Center => (pad / 2, pad - pad / 2),
                _ => (0, pad),
            };
            if lp > 0 { spans.push(Span { text: " ".repeat(lp), style: base }); }
            spans.extend(line.0.iter().cloned());
            if rp > 0 { spans.push(Span { text: " ".repeat(rp), style: base }); }
            if i + 1 == col_w.len() {
                // Last cell: close with " │"
                spans.push(Span { text: " │".into(), style: Style::default() });
            } else {
                // Between cells: " │ " serves as separator (right-pad of this cell + border + left-pad of next)
                spans.push(Span { text: " │ ".into(), style: Style::default() });
            }
        }
        lines.push(StyledLine(spans));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Align, Inline};

    fn cell(s: &str) -> Vec<Inline> { vec![Inline::Text(s.into())] }
    fn text_of(l: &StyledLine) -> String { l.0.iter().map(|s| s.text.as_str()).collect() }

    #[test]
    fn renders_bordered_table() {
        let aligns = vec![Align::Left, Align::Right];
        let header = vec![cell("a"), cell("bb")];
        let rows = vec![vec![cell("1"), cell("2")]];
        let lines = render_table(&aligns, &header, &rows, 80);
        // top border + header + separator + 1 data row + bottom border = 5 rows
        assert_eq!(lines.len(), 5);
        assert!(text_of(&lines[0]).starts_with('┌'));
        assert!(text_of(&lines[0]).contains('┬'));
        assert!(text_of(&lines[0]).ends_with('┐'));
        assert!(text_of(&lines[4]).starts_with('└'));
    }

    #[test]
    fn header_cells_are_bold() {
        let lines = render_table(&[Align::Left], &[cell("h")], &[vec![cell("d")]], 80);
        // Header row (index 1) text span containing 'h' should be bold
        let header_line = &lines[1];
        assert!(header_line.0.iter().any(|s| s.text.contains('h') && s.style.bold));
    }
}

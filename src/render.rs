use std::io::{self, Write};
use crate::style::{Color, Style, StyledLine};
use crate::term::TermSize;
use crate::layout::document::{Doc, Element};
use crate::layout::heading::osc66;
use crate::image_kitty::transmit_and_place;

pub const RESET: &str = "\x1b[0m";

/// Left (and equal right) margin in columns. Content is drawn starting at
/// column `LEFT_MARGIN + 1`; the layout width must be `cols - 2*LEFT_MARGIN`
/// so the right edge keeps the same margin.
pub const LEFT_MARGIN: u16 = 2;

/// 1-based terminal column where content starts.
const CONTENT_COL: u16 = LEFT_MARGIN + 1;

pub fn sgr(style: &Style) -> String {
    let mut codes: Vec<String> = Vec::new();
    if style.bold { codes.push("1".into()); }
    if style.dim { codes.push("2".into()); }
    if style.italic { codes.push("3".into()); }
    if style.underline { codes.push("4".into()); }
    if style.reverse { codes.push("7".into()); }
    match style.fg {
        Some(Color::Indexed(i)) => codes.push(format!("38;5;{}", i)),
        Some(Color::Rgb(r, g, b)) => codes.push(format!("38;2;{};{};{}", r, g, b)),
        None => {}
    }
    if codes.is_empty() { String::new() } else { format!("\x1b[{}m", codes.join(";")) }
}

pub fn line_to_ansi(line: &StyledLine) -> String {
    let mut out = String::new();
    for span in &line.0 {
        let pre = sgr(&span.style);
        if pre.is_empty() {
            out.push_str(&span.text);
        } else {
            out.push_str(&pre);
            out.push_str(&span.text);
            out.push_str(RESET);
        }
    }
    if !out.ends_with(RESET) { out.push_str(RESET); }
    out
}

/// Draw the slice of the Doc visible at the given scroll offset.
///
/// The visible area is `term.rows - 1` rows; the last row is a reversed
/// status line showing scroll %.
pub fn draw_viewport(out: &mut impl Write, doc: &Doc, scroll: usize, term: &TermSize) -> io::Result<()> {
    // Clear screen + cursor home, then delete all kitty graphics placements
    // so stale images don't ghost/smear when scrolling.
    write!(out, "\x1b[2J\x1b[H")?;
    write!(out, "\x1b_Ga=d\x1b\\")?;
    let view_rows = term.rows.saturating_sub(1) as usize; // last row is status line
    let mut global_row = 0usize; // accumulated global row index

    for el in &doc.elements {
        let h = el.height();
        let el_start = global_row;
        let el_end = global_row + h; // [start, end)
        global_row = el_end;

        // Does this element overlap the viewport [scroll, scroll+view_rows)?
        let vstart = scroll;
        let vend = scroll + view_rows;
        if el_end <= vstart || el_start >= vend { continue; }

        match el {
            Element::TextRows(rows) => {
                for (ri, line) in rows.iter().enumerate() {
                    let grow = el_start + ri;
                    if grow < vstart || grow >= vend { continue; }
                    let srow = grow - vstart;
                    write!(out, "\x1b[{};{}H", srow + 1, CONTENT_COL)?; // 1-based row;col
                    write!(out, "{}", line_to_ansi(line))?;
                }
            }
            Element::Heading { sc, lines, height } => {
                // Atomic: only draw when the whole element fits within the viewport
                if el_start >= vstart && el_end <= vend {
                    let per = height / lines.len().max(1);
                    for (li, text) in lines.iter().enumerate() {
                        let srow = (el_start - vstart) + li * per;
                        write!(out, "\x1b[{};{}H", srow + 1, CONTENT_COL)?;
                        // Split into <=4096-byte chunks for osc66
                        for chunk in split_4096(text) {
                            write!(out, "{}", osc66(&chunk, sc))?;
                        }
                    }
                }
                // When partially scrolled off, leave blank (screen was already cleared)
            }
            Element::Image { data, cols, rows } => {
                // Compute visible vertical sub-range
                let visible_top = el_start.max(vstart);
                let visible_bot = el_end.min(vend);
                let skip = (visible_top - el_start) as u16;
                let show = (visible_bot - visible_top) as u16;
                let srow = visible_top - vstart;
                write!(out, "\x1b[{};{}H", srow + 1, CONTENT_COL)?;
                let crop = if skip == 0 && show == *rows { None } else { Some((skip, show)) };
                let esc = transmit_and_place(data, *cols, *rows, crop, term.cell_px_h);
                write!(out, "{}", esc)?;
            }
        }
    }

    // Status line (reversed): show scroll %
    let pct = if doc.total_rows > view_rows {
        (scroll * 100) / (doc.total_rows - view_rows).max(1)
    } else {
        100
    };
    write!(out, "\x1b[{};1H", term.rows)?;
    write!(out, "\x1b[7m {}% (q: quit) \x1b[0m", pct.min(100))?;
    out.flush()
}

fn split_4096(text: &str) -> Vec<String> {
    if text.len() <= 4096 { return vec![text.to_string()]; }
    let mut v = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if cur.len() + ch.len_utf8() > 4096 { v.push(std::mem::take(&mut cur)); }
        cur.push(ch);
    }
    if !cur.is_empty() { v.push(cur); }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, Span, Style, StyledLine};
    use crate::layout::document::{Doc, Element};

    #[test]
    fn sgr_bold_underline() {
        let s = Style { bold: true, underline: true, ..Style::default() };
        let out = sgr(&s);
        assert!(out.contains("1")); // bold
        assert!(out.contains("4")); // underline
    }

    #[test]
    fn line_to_ansi_wraps_with_reset() {
        let line = StyledLine(vec![Span { text: "hi".into(), style: Style { bold: true, ..Style::default() } }]);
        let out = line_to_ansi(&line);
        assert!(out.starts_with("\x1b["));
        assert!(out.ends_with(RESET));
        assert!(out.contains("hi"));
    }

    #[test]
    fn rgb_fg_encoded() {
        let s = Style { fg: Some(Color::Rgb(10, 20, 30)), ..Style::default() };
        let out = sgr(&s);
        assert!(out.contains("38;2;10;20;30"));
    }

    #[test]
    fn draw_viewport_emits_kitty_graphics_delete() {
        let line = StyledLine(vec![Span { text: "hello".into(), style: Style::default() }]);
        let doc = Doc {
            elements: vec![Element::TextRows(vec![line])],
            total_rows: 1,
        };
        let term = TermSize { cols: 80, rows: 24, cell_px_w: 8, cell_px_h: 16 };
        let mut buf = Vec::new();
        draw_viewport(&mut buf, &doc, 0, &term).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(
            output.contains("\x1b_Ga=d\x1b\\"),
            "draw_viewport must emit kitty graphics-delete sequence at frame start"
        );
    }
}

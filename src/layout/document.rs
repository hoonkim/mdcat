use std::path::Path;
use crate::ir::Block;
use crate::style::{Span, Style, StyledLine, display_width};
use crate::term::TermSize;
use crate::layout::{inline::wrap_inlines, table::render_table, heading, code::Highlighter};
use crate::image_kitty::{self, ImageData};

pub enum Element {
    TextRows(Vec<StyledLine>),
    Heading { sc: heading::Scale, lines: Vec<String>, height: usize },
    Image { data: ImageData, cols: u16, rows: u16 },
}

impl Element {
    pub fn height(&self) -> usize {
        match self {
            Element::TextRows(v) => v.len(),
            Element::Heading { height, .. } => *height,
            Element::Image { rows, .. } => *rows as usize,
        }
    }
}

pub struct Doc {
    pub elements: Vec<Element>,
    pub total_rows: usize,
}

pub fn build(blocks: &[Block], width: usize, term: &TermSize, hl: &Highlighter, base_dir: &Path) -> Doc {
    let mut elements = Vec::new();
    emit_blocks(blocks, width, 0, term, hl, base_dir, &mut elements);
    let total_rows = elements.iter().map(|e| e.height()).sum();
    Doc { elements, total_rows }
}

/// Blank rows between two consecutive top-level paragraphs.
const PARAGRAPH_SPACING_ROWS: usize = 2;
/// Blank rows between any other pair of top-level blocks (e.g. heading↔body).
const BLOCK_SPACING_ROWS: usize = 1;

fn blank() -> Element {
    Element::TextRows(vec![StyledLine::default()])
}

fn emit_blocks(
    blocks: &[Block],
    width: usize,
    indent: usize,
    term: &TermSize,
    hl: &Highlighter,
    base_dir: &Path,
    out: &mut Vec<Element>,
) {
    let cw = width.saturating_sub(indent);
    for (i, block) in blocks.iter().enumerate() {
        match block {
            Block::Heading { level, inlines } => {
                let sc = heading::scale_for(*level);
                let factor = heading::effective_factor(&sc).max(1.0);
                let avail = ((cw as f32) / factor).floor() as usize;
                let wrapped = wrap_inlines(inlines, avail.max(1), Style { bold: sc.bold, ..Style::default() });
                let lines: Vec<String> = wrapped.iter()
                    .map(|l| l.0.iter().map(|s| s.text.as_str()).collect())
                    .collect();
                let per = factor.ceil() as usize;
                let height = lines.len().max(1) * per;
                out.push(Element::Heading { sc, lines, height });
            }
            Block::Paragraph(inl) => {
                out.push(Element::TextRows(indent_lines(wrap_inlines(inl, cw, Style::default()), indent)));
            }
            Block::BlockQuote(inner) => {
                let mut sub = Vec::new();
                emit_blocks(inner, width, indent + 2, term, hl, base_dir, &mut sub);
                for el in sub {
                    if let Element::TextRows(rows) = el {
                        let marked = rows.into_iter().map(|mut l| {
                            let mut spans = vec![Span { text: "│ ".into(), style: Style { dim: true, ..Style::default() } }];
                            spans.append(&mut l.0);
                            StyledLine(spans)
                        }).collect();
                        out.push(Element::TextRows(marked));
                    } else {
                        out.push(el);
                    }
                }
            }
            Block::List { ordered, start, items } => {
                for (idx, item) in items.iter().enumerate() {
                    let marker = if *ordered {
                        format!("{}. ", *start as usize + idx)
                    } else {
                        "• ".into()
                    };
                    let mut sub = Vec::new();
                    emit_blocks(item, width, indent + display_width(&marker), term, hl, base_dir, &mut sub);
                    prepend_marker(&mut sub, &marker, indent);
                    out.append(&mut sub);
                }
            }
            Block::CodeBlock { lang, code } => {
                let rows = hl.render(lang.as_deref(), code, cw);
                out.push(Element::TextRows(indent_lines(rows, indent)));
            }
            Block::Table { aligns, header, rows } => {
                out.push(Element::TextRows(indent_lines(render_table(aligns, header, rows, cw), indent)));
            }
            Block::Image { url, alt } => {
                match image_kitty::load(url, base_dir) {
                    Some(data) => {
                        let cols = (cw as u16).min(term.cols);
                        let rows = term.image_rows(data.px_w, data.px_h, cols);
                        out.push(Element::Image { data, cols, rows });
                    }
                    None => {
                        let txt = format!("[image: {}]", if alt.is_empty() { url.as_str() } else { alt.as_str() });
                        out.push(Element::TextRows(vec![StyledLine(vec![Span {
                            text: txt,
                            style: Style { dim: true, ..Style::default() },
                        }])]));
                    }
                }
            }
            Block::Rule => {
                out.push(Element::TextRows(vec![StyledLine(vec![Span {
                    text: "─".repeat(cw),
                    style: Style { dim: true, ..Style::default() },
                }])]));
            }
        }
        // Blank rows between top-level blocks: wider only between two paragraphs.
        if indent == 0 && i + 1 < blocks.len() {
            let gap = if matches!(block, Block::Paragraph(_))
                && matches!(blocks[i + 1], Block::Paragraph(_))
            {
                PARAGRAPH_SPACING_ROWS
            } else {
                BLOCK_SPACING_ROWS
            };
            for _ in 0..gap {
                out.push(blank());
            }
        }
    }
}

fn indent_lines(lines: Vec<StyledLine>, indent: usize) -> Vec<StyledLine> {
    if indent == 0 {
        return lines;
    }
    let pad = " ".repeat(indent);
    lines.into_iter().map(|mut l| {
        let mut spans = vec![Span { text: pad.clone(), style: Style::default() }];
        spans.append(&mut l.0);
        StyledLine(spans)
    }).collect()
}

fn prepend_marker(els: &mut [Element], marker: &str, indent: usize) {
    for el in els.iter_mut() {
        if let Element::TextRows(rows) = el {
            if let Some(first) = rows.first_mut() {
                let pad = " ".repeat(indent);
                let lead = Span { text: format!("{}{}", pad, marker), style: Style::default() };
                // Remove existing indent padding span if present
                if let Some(s0) = first.0.first() {
                    if s0.text.trim().is_empty() {
                        first.0.remove(0);
                    }
                }
                first.0.insert(0, lead);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;
    use crate::term::TermSize;
    use crate::layout::code::Highlighter;
    use std::path::Path;

    fn term() -> TermSize { TermSize { cols: 80, rows: 24, cell_px_w: 8, cell_px_h: 16 } }

    #[test]
    fn paragraph_becomes_textrows_and_counts_rows() {
        let blocks = vec![Block::Paragraph(vec![Inline::Text("hello world".into())])];
        let hl = Highlighter::new();
        let doc = build(&blocks, 76, &term(), &hl, Path::new("."));
        assert!(doc.total_rows >= 1);
        assert!(matches!(doc.elements[0], Element::TextRows(_)));
    }

    #[test]
    fn heading_height_matches_scale() {
        let blocks = vec![Block::Heading { level: 1, inlines: vec![Inline::Text("Hi".into())] }];
        let hl = Highlighter::new();
        let doc = build(&blocks, 76, &term(), &hl, Path::new("."));
        // H1 = scale 2 → 한 줄 제목이 2행
        if let Element::Heading { height, .. } = &doc.elements[0] {
            assert_eq!(*height, 2);
        } else { panic!("expected heading"); }
    }

    #[test]
    fn total_rows_is_sum_of_heights() {
        let blocks = vec![
            Block::Heading { level: 1, inlines: vec![Inline::Text("A".into())] },
            Block::Paragraph(vec![Inline::Text("b".into())]),
        ];
        let hl = Highlighter::new();
        let doc = build(&blocks, 76, &term(), &hl, Path::new("."));
        let sum: usize = doc.elements.iter().map(|e| e.height()).sum();
        assert_eq!(sum, doc.total_rows);
    }
}

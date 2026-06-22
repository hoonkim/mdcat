use crate::ir::*;
use pulldown_cmark::{Event, Tag, TagEnd, Options, Parser as CmParser, HeadingLevel, Alignment};

pub fn parse(md: &str) -> Vec<Block> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = CmParser::new_ext(md, opts);
    let mut b = Builder::default();
    for ev in parser {
        b.event(ev);
    }
    b.finish()
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    stack: Vec<Frame>,
}

enum Frame {
    Paragraph(Vec<Inline>),
    Heading { level: u8, inl: Vec<Inline> },
    BlockQuote(Vec<Block>),
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<Block>>,
        cur: Vec<Block>,
        // Inline content that arrives directly under an Item (tight lists, where
        // pulldown-cmark omits the Paragraph wrapper). Flushed into `cur` as a
        // Paragraph when a block is added or the item ends.
        pending: Vec<Inline>,
    },
    Item,
    Code { lang: Option<String>, text: String },
    Table {
        aligns: Vec<Align>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        cur_row: Vec<Vec<Inline>>,
        in_head: bool,
    },
    Cell(Vec<Inline>),
    Inline { kind: InlineKind, inl: Vec<Inline> },
    Image { url: String, alt: String },
}

enum InlineKind { Strong, Emph, Strike, Link(String) }

impl Builder {
    fn event(&mut self, ev: Event) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(end) => self.end(end),
            Event::Text(t) => self.push_inline(Inline::Text(t.into_string())),
            Event::Code(t) => self.push_inline(Inline::Code(t.into_string())),
            Event::SoftBreak => self.push_inline(Inline::SoftBreak),
            Event::HardBreak => self.push_inline(Inline::HardBreak),
            Event::Rule => self.push_block(Block::Rule),
            Event::TaskListMarker(done) => {
                let m = if done { "[x] " } else { "[ ] " };
                self.push_inline(Inline::Text(m.into()));
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => self.stack.push(Frame::Paragraph(vec![])),
            Tag::Heading { level, .. } => self.stack.push(Frame::Heading { level: hl(level), inl: vec![] }),
            Tag::BlockQuote(_) => self.stack.push(Frame::BlockQuote(vec![])),
            Tag::List(start) => self.stack.push(Frame::List {
                ordered: start.is_some(),
                start: start.unwrap_or(1),
                items: vec![],
                cur: vec![],
                pending: vec![],
            }),
            Tag::Item => self.stack.push(Frame::Item),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) => {
                        let s = s.into_string();
                        if s.is_empty() { None } else { Some(s) }
                    }
                    _ => None,
                };
                self.stack.push(Frame::Code { lang, text: String::new() });
            }
            Tag::Table(aligns) => self.stack.push(Frame::Table {
                aligns: aligns.into_iter().map(conv_align).collect(),
                header: vec![],
                rows: vec![],
                cur_row: vec![],
                in_head: false,
            }),
            Tag::TableHead => self.set_in_head(true),
            Tag::TableRow => {} // cur_row is already empty
            Tag::TableCell => self.stack.push(Frame::Cell(vec![])),
            Tag::Strong => self.stack.push(Frame::Inline { kind: InlineKind::Strong, inl: vec![] }),
            Tag::Emphasis => self.stack.push(Frame::Inline { kind: InlineKind::Emph, inl: vec![] }),
            Tag::Strikethrough => self.stack.push(Frame::Inline { kind: InlineKind::Strike, inl: vec![] }),
            Tag::Link { dest_url, .. } => self.stack.push(Frame::Inline {
                kind: InlineKind::Link(dest_url.into_string()),
                inl: vec![],
            }),
            Tag::Image { dest_url, .. } => self.stack.push(Frame::Image {
                url: dest_url.into_string(),
                alt: String::new(),
            }),
            _ => self.stack.push(Frame::Paragraph(vec![])),
        }
    }

    fn end(&mut self, end: TagEnd) {
        // TableHead and TableRow do not push frames — handle before pop
        if matches!(end, TagEnd::TableRow) {
            if let Some(Frame::Table { rows, cur_row, .. }) = self.stack.last_mut() {
                rows.push(std::mem::take(cur_row));
            }
            return;
        }
        if matches!(end, TagEnd::TableHead) {
            self.set_in_head(false);
            return;
        }

        let frame = self.stack.pop().expect("balanced");
        match frame {
            Frame::Paragraph(inl) => self.push_block(Block::Paragraph(inl)),
            Frame::Heading { level, inl } => self.push_block(Block::Heading { level, inlines: inl }),
            Frame::BlockQuote(blocks) => self.push_block(Block::BlockQuote(blocks)),
            Frame::List { ordered, start, mut items, mut cur, mut pending } => {
                // pending is normally empty here (flushed at End(Item)); guard anyway.
                if !pending.is_empty() {
                    cur.push(Block::Paragraph(std::mem::take(&mut pending)));
                }
                if !cur.is_empty() { items.push(cur); }
                self.push_block(Block::List { ordered, start, items });
            }
            Frame::Item => {
                if let Some(Frame::List { items, cur, pending, .. }) = self.stack.last_mut() {
                    // tight-list item text: flush the pending inline buffer as a paragraph
                    if !pending.is_empty() {
                        cur.push(Block::Paragraph(std::mem::take(pending)));
                    }
                    items.push(std::mem::take(cur));
                }
            }
            Frame::Code { lang, text } => self.push_block(Block::CodeBlock { lang, code: text }),
            Frame::Table { aligns, header, rows, .. } => {
                self.push_block(Block::Table { aligns, header, rows });
            }
            Frame::Cell(inl) => {
                if let Some(Frame::Table { header, cur_row, in_head, .. }) = self.stack.last_mut() {
                    if *in_head { header.push(inl); } else { cur_row.push(inl); }
                }
            }
            Frame::Inline { kind, inl } => {
                let node = match kind {
                    InlineKind::Strong => Inline::Strong(inl),
                    InlineKind::Emph => Inline::Emph(inl),
                    InlineKind::Strike => Inline::Strike(inl),
                    InlineKind::Link(url) => Inline::Link { text: inl, url },
                };
                self.push_inline(node);
            }
            Frame::Image { url, alt } => self.push_block(Block::Image { url, alt }),
        }
    }

    fn set_in_head(&mut self, v: bool) {
        if let Some(Frame::Table { in_head, .. }) = self.stack.last_mut() {
            *in_head = v;
        }
    }

    fn push_inline(&mut self, node: Inline) {
        // Tight-list item content arrives directly under an Item frame with no
        // Paragraph wrapper; route it to the enclosing List's pending buffer.
        let n = self.stack.len();
        if n >= 1 && matches!(self.stack[n - 1], Frame::Item) {
            if n >= 2 {
                if let Frame::List { pending, .. } = &mut self.stack[n - 2] {
                    pending.push(node);
                }
            }
            return;
        }
        match self.stack.last_mut() {
            Some(Frame::Paragraph(inl))
            | Some(Frame::Heading { inl, .. })
            | Some(Frame::Inline { inl, .. })
            | Some(Frame::Cell(inl)) => inl.push(node),
            Some(Frame::Code { text, .. }) => {
                if let Inline::Text(s) = node { text.push_str(&s); }
            }
            Some(Frame::Image { alt, .. }) => {
                if let Inline::Text(s) = node { alt.push_str(&s); }
            }
            _ => {}
        }
    }

    fn push_block(&mut self, block: Block) {
        match self.stack.last_mut() {
            Some(Frame::BlockQuote(blocks)) => blocks.push(block),
            Some(Frame::List { cur, pending, .. }) => {
                if !pending.is_empty() {
                    cur.push(Block::Paragraph(std::mem::take(pending)));
                }
                cur.push(block);
            }
            Some(Frame::Item) => {
                let n = self.stack.len();
                if n >= 2 {
                    if let Frame::List { cur, pending, .. } = &mut self.stack[n - 2] {
                        // flush any pending inline text before the block so order
                        // is preserved (e.g. item text, then a nested sub-list)
                        if !pending.is_empty() {
                            cur.push(Block::Paragraph(std::mem::take(pending)));
                        }
                        cur.push(block);
                        return;
                    }
                }
            }
            None => self.blocks.push(block),
            _ => self.blocks.push(block),
        }
    }

    fn finish(self) -> Vec<Block> { self.blocks }
}

fn hl(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn conv_align(a: Alignment) -> Align {
    match a {
        Alignment::None => Align::None,
        Alignment::Left => Align::Left,
        Alignment::Center => Align::Center,
        Alignment::Right => Align::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_heading_and_paragraph() {
        let blocks = parse("# Title\n\nHello **world**");
        assert_eq!(blocks[0], Block::Heading { level: 1, inlines: vec![Inline::Text("Title".into())] });
        assert!(matches!(&blocks[1], Block::Paragraph(_)));
        if let Block::Paragraph(inl) = &blocks[1] {
            assert_eq!(inl[0], Inline::Text("Hello ".into()));
            assert_eq!(inl[1], Inline::Strong(vec![Inline::Text("world".into())]));
        }
    }

    #[test]
    fn parses_table_with_alignments() {
        let md = "| a | b |\n|:--|--:|\n| 1 | 2 |";
        let t = parse(md).into_iter().next().unwrap();
        match t {
            Block::Table { aligns, header, rows } => {
                assert_eq!(aligns, vec![Align::Left, Align::Right]);
                assert_eq!(header.len(), 2);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].len(), 2);
            }
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn parses_fenced_code_with_lang() {
        let blocks = parse("```rust\nfn main() {}\n```");
        assert_eq!(blocks[0], Block::CodeBlock { lang: Some("rust".into()), code: "fn main() {}\n".into() });
    }

    #[test]
    fn parses_tight_bullet_list() {
        // tight list: no blank lines between items -> pulldown omits Paragraph wrappers
        let blocks = parse("- first\n- second");
        match &blocks[0] {
            Block::List { ordered, items, .. } => {
                assert!(!ordered);
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], vec![Block::Paragraph(vec![Inline::Text("first".into())])]);
                assert_eq!(items[1], vec![Block::Paragraph(vec![Inline::Text("second".into())])]);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_tight_ordered_list_with_start() {
        let blocks = parse("3. a\n4. b");
        match &blocks[0] {
            Block::List { ordered, start, items } => {
                assert!(ordered);
                assert_eq!(*start, 3);
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], vec![Block::Paragraph(vec![Inline::Text("a".into())])]);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_tight_list_keeps_order() {
        let blocks = parse("- outer\n  - inner");
        match &blocks[0] {
            Block::List { items, .. } => {
                assert_eq!(items.len(), 1);
                let item = &items[0];
                // item text comes first, then the nested sub-list
                assert_eq!(item[0], Block::Paragraph(vec![Inline::Text("outer".into())]));
                assert!(matches!(&item[1], Block::List { .. }));
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_task_list_markers() {
        let blocks = parse("- [x] done\n- [ ] todo");
        match &blocks[0] {
            Block::List { items, .. } => {
                assert_eq!(items.len(), 2);
                // marker is rendered into the leading text
                if let Block::Paragraph(inl) = &items[0][0] {
                    let text: String = inl.iter().map(|i| match i {
                        Inline::Text(t) => t.clone(),
                        _ => String::new(),
                    }).collect();
                    assert!(text.contains("[x]"), "got {text:?}");
                    assert!(text.contains("done"), "got {text:?}");
                } else {
                    panic!("expected paragraph in task item");
                }
            }
            other => panic!("expected list, got {other:?}"),
        }
    }
}

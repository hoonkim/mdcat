# mdcat — kitty 마크다운 뷰어 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** kitty 터미널에서 `mdcat <file.md>` 로 마크다운을 전체화면 페이저로 렌더링한다 (제목은 OSC 66 텍스트 크기, 이미지는 kitty 그래픽 프로토콜, 표/코드/마우스 스크롤 지원).

**Architecture:** 파이프라인 = `parser`(pulldown-cmark → `Block` IR) → `layout`(터미널 폭 기준으로 `Element` 시퀀스 + 행 높이 계산) → `render`(이스케이프 시퀀스 출력) → `pager`(alternate screen, 행 단위 스크롤). 순수 변환 단계(parser/layout)는 TDD, I/O 단계(render/pager/그래픽)는 스냅샷 + 수동 검증.

**Tech Stack:** Rust, pulldown-cmark 0.13, crossterm 0.29, syntect 5.3, image 0.25, ureq 3.3, unicode-width 0.2, base64 0.22.

## Global Constraints

- 색/배경/폰트는 터미널 기본값 상속 — 자체 색 지정 최소화, 배경색 절대 지정 안 함, 전경색은 코드 하이라이트에서만 사용
- 가로는 터미널 폭에 맞추고 본문은 콘텐츠 폭(= 터미널 폭 - 좌우 여백 2칸씩)으로 워드랩
- kitty 전용 (다른 터미널 폴백 없음)
- OSC 66 형식: `\x1b]66;<meta>;<text>\x1b\\`, 페이로드 4096바이트 제한
- 제목 스케일 매핑: H1=`s=2`, H2=`n=3,d=2`, H3=`n=5,d=4`, H4–H6=`s=1`+굵게
- 셀 폭 계산은 항상 `unicode-width` 사용 (CJK/이모지 대응)
- Rust 2021 edition, 모든 작업 끝에 `cargo test`/`cargo build` 통과 후 커밋

---

### Task 1: 프로젝트 스캐폴드 + CLI

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/cli.rs`
- Test: `src/cli.rs` (인라인 `#[cfg(test)]`)

**Interfaces:**
- Produces: `cli::parse_args(args: Vec<String>) -> Result<Config, CliError>`, `struct Config { path: PathBuf }`, `cli::read_source(cfg: &Config) -> Result<String, CliError>`

- [ ] **Step 1: Cargo.toml 작성**

```toml
[package]
name = "mdcat"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "mdcat"
path = "src/main.rs"

[dependencies]
pulldown-cmark = "0.13"
crossterm = "0.29"
syntect = "5.3"
image = "0.25"
ureq = "3.3"
unicode-width = "0.2"
base64 = "0.22"
```

- [ ] **Step 2: 실패하는 테스트 작성 (`src/cli.rs`)**

```rust
use std::path::PathBuf;

#[derive(Debug)]
pub struct Config {
    pub path: PathBuf,
}

#[derive(Debug, PartialEq)]
pub enum CliError {
    MissingArg,
    Io(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_path_arg() {
        let cfg = parse_args(vec!["mdcat".into(), "README.md".into()]).unwrap();
        assert_eq!(cfg.path, PathBuf::from("README.md"));
    }

    #[test]
    fn errors_when_no_path() {
        assert_eq!(parse_args(vec!["mdcat".into()]), Err(CliError::MissingArg));
    }
}
```

- [ ] **Step 3: 실패 확인**

Run: `cargo test cli 2>&1 | tail -20`
Expected: FAIL — `parse_args` not found

- [ ] **Step 4: 구현**

```rust
pub fn parse_args(args: Vec<String>) -> Result<Config, CliError> {
    let path = args.get(1).ok_or(CliError::MissingArg)?;
    Ok(Config { path: PathBuf::from(path) })
}

pub fn read_source(cfg: &Config) -> Result<String, CliError> {
    std::fs::read_to_string(&cfg.path).map_err(|e| CliError::Io(e.to_string()))
}
```

- [ ] **Step 5: main.rs 스텁**

```rust
mod cli;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cfg = match cli::parse_args(args) {
        Ok(c) => c,
        Err(cli::CliError::MissingArg) => {
            eprintln!("usage: mdcat <file.md>");
            std::process::exit(2);
        }
        Err(cli::CliError::Io(e)) => {
            eprintln!("mdcat: {e}");
            std::process::exit(1);
        }
    };
    let src = match cli::read_source(&cfg) {
        Ok(s) => s,
        Err(cli::CliError::Io(e)) => { eprintln!("mdcat: {e}"); std::process::exit(1); }
        Err(_) => unreachable!(),
    };
    print!("{}", src.len()); // 임시: 이후 Task에서 교체
}
```

- [ ] **Step 6: 테스트/빌드 통과 확인**

Run: `cargo test cli && cargo build`
Expected: PASS, 빌드 성공

- [ ] **Step 7: 커밋**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/cli.rs
git commit -m "feat: scaffold project and CLI arg parsing"
```

---

### Task 2: 마크다운 → Block IR

**Files:**
- Create: `src/ir.rs`
- Create: `src/parser.rs`
- Modify: `src/main.rs` (모듈 선언 추가)
- Test: `src/parser.rs` (인라인)

**Interfaces:**
- Produces: `ir::{Block, Inline, Align}` (아래 정의), `parser::parse(md: &str) -> Vec<ir::Block>`
- Consumes: 없음

- [ ] **Step 1: IR 타입 정의 (`src/ir.rs`)**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Align { None, Left, Center, Right }

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Emph(Vec<Inline>),
    Strike(Vec<Inline>),
    Link { text: Vec<Inline>, url: String },
    SoftBreak,
    HardBreak,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading { level: u8, inlines: Vec<Inline> },
    Paragraph(Vec<Inline>),
    BlockQuote(Vec<Block>),
    List { ordered: bool, start: u64, items: Vec<Vec<Block>> },
    CodeBlock { lang: Option<String>, code: String },
    Table { aligns: Vec<Align>, header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
    Image { url: String, alt: String },
    Rule,
}
```

- [ ] **Step 2: 실패하는 테스트 작성 (`src/parser.rs`)**

```rust
use crate::ir::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

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
}
```

- [ ] **Step 3: 실패 확인**

Run: `cargo test parser 2>&1 | tail -20`
Expected: FAIL — `parse` not found

- [ ] **Step 4: 구현 (`src/parser.rs`)**

pulldown-cmark 0.13 이벤트 스트림을 스택 기반으로 IR로 접는다. 핵심 패턴:

```rust
use crate::ir::*;
use pulldown_cmark::{Event, Tag, TagEnd, Options, Parser as CmParser, HeadingLevel, Alignment, CowStr};

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

// Builder는 컨테이너 스택(Vec<Frame>)을 유지한다.
// Frame 종류: Root, Paragraph(Vec<Inline>), Heading(level, Vec<Inline>),
//   BlockQuote(Vec<Block>), List(ordered,start,items,cur_item:Vec<Block>),
//   CodeBlock(lang, String), Table(aligns, header, rows, cur_row, cur_cell, in_head),
//   그리고 인라인 누적용 InlineFrame(Strong/Emph/Strike/Link).
// Start(Tag) → 프레임 push, End(TagEnd) → pop 후 부모에 append.
// Event::Text/Code/SoftBreak/HardBreak → 현재 인라인 수집 대상에 push.
// Tag::Image → 자식 텍스트를 alt로 모은 뒤 End에서 Block::Image 또는 인라인 처리(MVP: 블록으로).
```

전체 구현(작업자가 그대로 작성):

```rust
#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    stack: Vec<Frame>,
}

enum Frame {
    Paragraph(Vec<Inline>),
    Heading { level: u8, inl: Vec<Inline> },
    BlockQuote(Vec<Block>),
    List { ordered: bool, start: u64, items: Vec<Vec<Block>>, cur: Vec<Block> },
    Item, // 마커; 실제 누적은 List.cur 사용
    Code { lang: Option<String>, text: String },
    Table { aligns: Vec<Align>, header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>>,
            cur_row: Vec<Vec<Inline>>, in_head: bool },
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
                ordered: start.is_some(), start: start.unwrap_or(1),
                items: vec![], cur: vec![] }),
            Tag::Item => self.stack.push(Frame::Item),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) => {
                        let s = s.into_string(); if s.is_empty() { None } else { Some(s) }
                    }
                    _ => None,
                };
                self.stack.push(Frame::Code { lang, text: String::new() });
            }
            Tag::Table(aligns) => self.stack.push(Frame::Table {
                aligns: aligns.into_iter().map(conv_align).collect(),
                header: vec![], rows: vec![], cur_row: vec![], in_head: false }),
            Tag::TableHead => self.set_in_head(true),
            Tag::TableRow => {} // cur_row는 이미 비어있음
            Tag::TableCell => self.stack.push(Frame::Cell(vec![])),
            Tag::Strong => self.stack.push(Frame::Inline { kind: InlineKind::Strong, inl: vec![] }),
            Tag::Emphasis => self.stack.push(Frame::Inline { kind: InlineKind::Emph, inl: vec![] }),
            Tag::Strikethrough => self.stack.push(Frame::Inline { kind: InlineKind::Strike, inl: vec![] }),
            Tag::Link { dest_url, .. } => self.stack.push(Frame::Inline {
                kind: InlineKind::Link(dest_url.into_string()), inl: vec![] }),
            Tag::Image { dest_url, .. } => self.stack.push(Frame::Image {
                url: dest_url.into_string(), alt: String::new() }),
            _ => self.stack.push(Frame::Paragraph(vec![])), // 알 수 없는 컨테이너는 문단처럼
        }
    }

    fn end(&mut self, _end: TagEnd) {
        let frame = self.stack.pop().expect("balanced");
        match frame {
            Frame::Paragraph(inl) => self.push_block(Block::Paragraph(inl)),
            Frame::Heading { level, inl } => self.push_block(Block::Heading { level, inlines: inl }),
            Frame::BlockQuote(blocks) => self.push_block(Block::BlockQuote(blocks)),
            Frame::List { ordered, start, mut items, cur } => {
                if !cur.is_empty() { items.push(cur); }
                self.push_block(Block::List { ordered, start, items });
            }
            Frame::Item => {
                // Item 종료: 부모 List의 cur를 items로 flush. Item 동안 blocks는 List.cur에 쌓였음.
                if let Some(Frame::List { items, cur, .. }) = self.stack.last_mut() {
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
        // TableRow 종료 처리: TagEnd::TableRow일 때 cur_row를 rows로 이동
        if matches!(_end, TagEnd::TableRow) {
            if let Some(Frame::Table { rows, cur_row, .. }) = self.stack.last_mut() {
                rows.push(std::mem::take(cur_row));
            }
        }
        if matches!(_end, TagEnd::TableHead) { self.set_in_head(false); }
    }

    fn set_in_head(&mut self, v: bool) {
        if let Some(Frame::Table { in_head, .. }) = self.stack.last_mut() { *in_head = v; }
    }

    fn push_inline(&mut self, node: Inline) {
        match self.stack.last_mut() {
            Some(Frame::Paragraph(inl)) | Some(Frame::Heading { inl, .. })
            | Some(Frame::Inline { inl, .. }) | Some(Frame::Cell(inl)) => inl.push(node),
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
            Some(Frame::List { cur, .. }) => cur.push(block),
            Some(Frame::Item) => {
                // Item 프레임 위에 직접 push되지 않도록 List.cur로 우회
                // (Item 아래 블록은 push_block 시 stack 최상단이 Item이므로 여기로 옴)
                // 한 단계 아래 List.cur에 넣는다.
                let n = self.stack.len();
                if n >= 2 {
                    if let Frame::List { cur, .. } = &mut self.stack[n - 2] {
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
        HeadingLevel::H1 => 1, HeadingLevel::H2 => 2, HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4, HeadingLevel::H5 => 5, HeadingLevel::H6 => 6,
    }
}

fn conv_align(a: Alignment) -> Align {
    match a {
        Alignment::None => Align::None, Alignment::Left => Align::Left,
        Alignment::Center => Align::Center, Alignment::Right => Align::Right,
    }
}
```

> 주의: `CowStr::into_string()` 사용. 일부 버전은 `to_string()`/`.into()` 필요 — 컴파일 에러 시 `String::from(s)`로 대체.

- [ ] **Step 5: main.rs 모듈 선언 추가**

`mod ir;` 와 `mod parser;` 를 `src/main.rs` 상단(`mod cli;` 아래)에 추가.

- [ ] **Step 6: 테스트 통과 확인**

Run: `cargo test parser 2>&1 | tail -20`
Expected: PASS (3 tests)

- [ ] **Step 7: 커밋**

```bash
git add src/ir.rs src/parser.rs src/main.rs
git commit -m "feat: parse markdown into Block IR"
```

---

### Task 3: 스타일 모델 + 인라인 워드랩

**Files:**
- Create: `src/style.rs`
- Create: `src/layout/mod.rs`
- Create: `src/layout/inline.rs`
- Modify: `src/main.rs` (`mod style; mod layout;`)
- Test: `src/layout/inline.rs` (인라인)

**Interfaces:**
- Produces:
  - `style::{Style, Span, StyledLine, Color}` — `Style { bold, italic, underline, dim, reverse, fg: Option<Color> }`, `Span { text: String, style: Style }`, `StyledLine(pub Vec<Span>)`
  - `layout::inline::wrap_inlines(inlines: &[ir::Inline], width: usize, base: Style) -> Vec<StyledLine>` — 인라인들을 폭 `width`(셀)로 워드랩한 스타일 행들
  - `style::display_width(s: &str) -> usize` (unicode-width 래퍼)
- Consumes: `ir::Inline` (Task 2)

- [ ] **Step 1: 스타일 모델 작성 (`src/style.rs`)**

```rust
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Color { Indexed(u8), Rgb(u8, u8, u8) }

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub reverse: bool,
    pub fg: Option<Color>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span { pub text: String, pub style: Style }

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyledLine(pub Vec<Span>);

impl StyledLine {
    pub fn width(&self) -> usize {
        self.0.iter().map(|s| display_width(&s.text)).sum()
    }
}

pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}
```

- [ ] **Step 2: 실패하는 테스트 작성 (`src/layout/inline.rs`)**

```rust
use crate::ir::Inline;
use crate::style::{Style, StyledLine, display_width};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Inline;
    use crate::style::Style;

    fn text_of(line: &StyledLine) -> String {
        line.0.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn wraps_plain_text_at_width() {
        let inl = vec![Inline::Text("the quick brown fox".into())];
        let lines = wrap_inlines(&inl, 9, Style::default());
        assert_eq!(text_of(&lines[0]), "the quick");
        assert_eq!(text_of(&lines[1]), "brown fox");
    }

    #[test]
    fn carries_bold_style() {
        let inl = vec![Inline::Strong(vec![Inline::Text("hi".into())])];
        let lines = wrap_inlines(&inl, 80, Style::default());
        assert!(lines[0].0[0].style.bold);
    }

    #[test]
    fn hard_break_forces_new_line() {
        let inl = vec![Inline::Text("a".into()), Inline::HardBreak, Inline::Text("b".into())];
        let lines = wrap_inlines(&inl, 80, Style::default());
        assert_eq!(text_of(&lines[0]), "a");
        assert_eq!(text_of(&lines[1]), "b");
    }
}
```

- [ ] **Step 3: 실패 확인**

Run: `cargo test inline 2>&1 | tail -20`
Expected: FAIL — `wrap_inlines` not found

- [ ] **Step 4: 구현 (`src/layout/inline.rs`)**

인라인 트리를 (단어, 스타일) 토큰 스트림으로 평탄화한 뒤 그리디 워드랩한다.

```rust
use crate::ir::Inline;
use crate::style::{Span, Style, StyledLine, display_width};

enum Tok { Word(String, Style), Space(Style), Soft, Hard }

fn flatten(inl: &[Inline], st: Style, out: &mut Vec<Tok>) {
    for node in inl {
        match node {
            Inline::Text(t) => push_words(t, st, out, false),
            Inline::Code(t) => push_words(t, Style { dim: true, ..st }, out, true),
            Inline::Strong(c) => flatten(c, Style { bold: true, ..st }, out),
            Inline::Emph(c) => flatten(c, Style { italic: true, ..st }, out),
            Inline::Strike(c) => flatten(c, Style { dim: true, ..st }, out), // 취소선 근사
            Inline::Link { text, .. } => flatten(text, Style { underline: true, ..st }, out),
            Inline::SoftBreak => out.push(Tok::Soft),
            Inline::HardBreak => out.push(Tok::Hard),
        }
    }
}

// 코드 조각은 공백 보존, 일반 텍스트는 공백 기준 분할
fn push_words(t: &str, st: Style, out: &mut Vec<Tok>, code: bool) {
    if code {
        out.push(Tok::Word(t.to_string(), st));
        return;
    }
    let mut first = true;
    for word in t.split(' ') {
        if !first { out.push(Tok::Space(st)); }
        first = false;
        if !word.is_empty() { out.push(Tok::Word(word.to_string(), st)); }
    }
}

pub fn wrap_inlines(inlines: &[Inline], width: usize, base: Style) -> Vec<StyledLine> {
    let mut toks = Vec::new();
    flatten(inlines, base, &mut toks);

    let mut lines: Vec<StyledLine> = Vec::new();
    let mut cur = StyledLine::default();
    let mut cur_w = 0usize;

    let push_span = |line: &mut StyledLine, text: &str, st: Style| {
        if let Some(last) = line.0.last_mut() {
            if last.style == st { last.text.push_str(text); return; }
        }
        line.0.push(Span { text: text.to_string(), style: st });
    };

    for tok in toks {
        match tok {
            Tok::Soft | Tok::Space(_) => {
                // 다음 단어 배치 시 공백을 넣을지 결정 (줄 시작이면 생략)
                if cur_w > 0 && cur_w < width {
                    push_span(&mut cur, " ", Style::default());
                    cur_w += 1;
                }
            }
            Tok::Hard => {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0;
            }
            Tok::Word(w, st) => {
                let ww = display_width(&w);
                if cur_w > 0 && cur_w + ww > width {
                    // 줄 끝 공백 제거 후 개행
                    trim_trailing_space(&mut cur, &mut cur_w);
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0;
                }
                push_span(&mut cur, &w, st);
                cur_w += ww;
            }
        }
    }
    if !cur.0.is_empty() || lines.is_empty() {
        trim_trailing_space(&mut cur, &mut cur_w);
        lines.push(cur);
    }
    lines
}

fn trim_trailing_space(line: &mut StyledLine, w: &mut usize) {
    while let Some(last) = line.0.last_mut() {
        if last.text.ends_with(' ') {
            last.text.pop();
            *w -= 1;
            if last.text.is_empty() { line.0.pop(); } else { break; }
        } else { break; }
    }
}
```

`src/layout/mod.rs` 에 `pub mod inline;` 추가.

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test inline 2>&1 | tail -20`
Expected: PASS (3 tests)

- [ ] **Step 6: 커밋**

```bash
git add src/style.rs src/layout/ src/main.rs
git commit -m "feat: add style model and inline word-wrap"
```

---

### Task 4: 표 레이아웃 (박스 드로잉)

**Files:**
- Create: `src/layout/table.rs`
- Modify: `src/layout/mod.rs` (`pub mod table;`)
- Test: `src/layout/table.rs` (인라인)

**Interfaces:**
- Produces: `layout::table::render_table(aligns: &[ir::Align], header: &[Vec<ir::Inline>], rows: &[Vec<Vec<ir::Inline>>], max_width: usize) -> Vec<StyledLine>`
- Consumes: `wrap_inlines` (Task 3), `ir::Align`, `style::StyledLine`

- [ ] **Step 1: 실패하는 테스트 작성 (`src/layout/table.rs`)**

```rust
use crate::ir::{Align, Inline};
use crate::style::StyledLine;
use crate::layout::inline::wrap_inlines;

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
        // 상단 테두리 + 헤더 + 구분선 + 1 데이터행 + 하단 테두리 = 5행
        assert_eq!(lines.len(), 5);
        assert!(text_of(&lines[0]).starts_with('┌'));
        assert!(text_of(&lines[0]).contains('┬'));
        assert!(text_of(&lines[0]).ends_with('┐'));
        assert!(text_of(&lines[4]).starts_with('└'));
    }

    #[test]
    fn header_cells_are_bold() {
        let lines = render_table(&[Align::Left], &[cell("h")], &[vec![cell("d")]], 80);
        // 헤더행(인덱스1)의 텍스트 span 중 'h'는 굵게
        let header_line = &lines[1];
        assert!(header_line.0.iter().any(|s| s.text.contains('h') && s.style.bold));
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test table 2>&1 | tail -20`
Expected: FAIL — `render_table` not found

- [ ] **Step 3: 구현 (`src/layout/table.rs`)**

```rust
use crate::ir::{Align, Inline};
use crate::style::{Span, Style, StyledLine, display_width};
use crate::layout::inline::wrap_inlines;

pub fn render_table(
    aligns: &[Align],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    max_width: usize,
) -> Vec<StyledLine> {
    let ncol = header.len().max(aligns.len()).max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if ncol == 0 { return vec![]; }

    // 1) 각 셀의 자연 폭(한 줄 가정) 측정 → 열별 최대
    let natural = |inl: &[Inline]| -> usize {
        wrap_inlines(inl, usize::MAX, Style::default()).iter().map(|l| l.width()).max().unwrap_or(0)
    };
    let mut col_w = vec![0usize; ncol];
    for (i, c) in header.iter().enumerate() { col_w[i] = col_w[i].max(natural(c)); }
    for row in rows {
        for (i, c) in row.iter().enumerate() { col_w[i] = col_w[i].max(natural(c)); }
    }

    // 2) 테두리 비용: 세로선 ncol+1개 + 셀당 좌우 패딩 1칸씩
    let chrome = (ncol + 1) + ncol * 2;
    let avail = max_width.saturating_sub(chrome);
    let total: usize = col_w.iter().sum();
    if total > avail && total > 0 {
        // 비율 축소 (최소 1)
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

// 셀 내용을 열 폭으로 래핑 → 행이 여러 줄일 수 있으므로 세로로 패딩
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
            spans.push(Span { text: " │ ".into(), style: Style::default() });
            // 마지막 열 뒤는 "│"만 남도록 위에서 " │ " 사용; 첫 진입 시 "│ "와 중복되니 정리
            let _ = i;
        }
        // 위 루프가 각 셀 뒤에 " │ "를 넣으므로 줄 끝은 " │ " → 끝 공백 1개 제거해 "│"로 맞춤
        if let Some(last) = spans.last_mut() {
            if last.text == " │ " { last.text = " │".into(); }
        }
        lines.push(StyledLine(spans));
    }
    lines
}
```

> 첫 셀 앞 마커가 `"│ "`이고 각 셀 뒤가 `" │ "`라서 셀 사이 구분이 `" │ "`로 일관됨. 테스트의 박스 문자 검증과 폭 계산(chrome)이 일치하는지 빌드 후 수치 확인.

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test table 2>&1 | tail -20`
Expected: PASS (2 tests). 실패 시 `chrome` 폭 상수와 마커 폭을 맞춰 조정.

- [ ] **Step 5: 커밋**

```bash
git add src/layout/table.rs src/layout/mod.rs
git commit -m "feat: render markdown tables with box drawing"
```

---

### Task 5: 제목 스케일 계산 + OSC 66 렌더

**Files:**
- Create: `src/layout/heading.rs`
- Modify: `src/layout/mod.rs` (`pub mod heading;`)
- Test: `src/layout/heading.rs` (인라인)

**Interfaces:**
- Produces:
  - `layout::heading::scale_for(level: u8) -> Scale` where `pub struct Scale { pub s: u8, pub n: u8, pub d: u8, pub bold: bool }`
  - `layout::heading::effective_factor(sc: &Scale) -> f32` (워드랩 폭/높이 계산용)
  - `layout::heading::osc66(text: &str, sc: &Scale) -> String` (단일 조각 이스케이프; 호출자가 4096바이트 이하로 분할)
- Consumes: 없음

- [ ] **Step 1: 실패하는 테스트 작성 (`src/layout/heading.rs`)**

```rust
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
        assert_eq!((sc.n, sc.d), (3, 2));
        assert!((effective_factor(&sc) - 1.5).abs() < 1e-6);
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
        assert_eq!(out, "\x1b]66;s=1:n=3:d=2;Hi\x1b\\");
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test heading 2>&1 | tail -20`
Expected: FAIL — `scale_for` not found

- [ ] **Step 3: 구현 (`src/layout/heading.rs`)**

> OSC 66 메타데이터 키는 `:` 로 구분한다 (예: `s=1:n=3:d=2`). 종결자는 `ESC \`(`\x1b\\`).

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale { pub s: u8, pub n: u8, pub d: u8, pub bold: bool }

pub fn scale_for(level: u8) -> Scale {
    match level {
        1 => Scale { s: 2, n: 0, d: 0, bold: false },
        2 => Scale { s: 1, n: 3, d: 2, bold: false },
        3 => Scale { s: 1, n: 5, d: 4, bold: false },
        _ => Scale { s: 1, n: 0, d: 0, bold: true },
    }
}

pub fn effective_factor(sc: &Scale) -> f32 {
    if sc.n > 0 && sc.d > 0 {
        sc.s as f32 + sc.n as f32 / sc.d as f32 - 1.0 // s=1 기준 분수: 1 + (n/d - 1)? 아래 주석 참조
    } else {
        sc.s as f32
    }
}

pub fn osc66(text: &str, sc: &Scale) -> String {
    let mut meta = format!("s={}", sc.s);
    if sc.n > 0 && sc.d > 0 {
        meta.push_str(&format!(":n={}:d={}", sc.n, sc.d));
    }
    format!("\x1b]66;{};{}\x1b\\", meta, text)
}
```

> **분수 스케일 의미 확인 필요(빌드 후 수동):** kitty 문서상 분수 스케일은 `s` 위에 `n/d`를 곱/가산하는 방식이다. 테스트는 H2의 effective_factor를 1.5로 기대하므로, 위 식이 `s=1,n=3,d=2`에서 1.5를 만들지 검증한다. 1.5가 안 나오면 `effective_factor`를 `if n>0&&d>0 { n as f32 / d as f32 } else { s as f32 }` 로 단순화하고 테스트의 H1(2.0)은 s=2 경로로 유지. **구현 시 테스트가 통과하는 식을 채택**하고 실제 kitty 렌더 크기는 Task 12 수동 확인에서 맞춘다.

작업자 지침: 위 `effective_factor`로 `cargo test heading`이 통과하지 않으면 다음으로 교체:

```rust
pub fn effective_factor(sc: &Scale) -> f32 {
    if sc.n > 0 && sc.d > 0 { sc.n as f32 / sc.d as f32 } else { sc.s as f32 }
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test heading 2>&1 | tail -20`
Expected: PASS (5 tests). effective_factor 식은 위 지침대로 통과하는 쪽 채택.

- [ ] **Step 5: 커밋**

```bash
git add src/layout/heading.rs src/layout/mod.rs
git commit -m "feat: heading scale mapping and OSC 66 encoding"
```

---

### Task 6: 코드 블록 하이라이팅 (syntect)

**Files:**
- Create: `src/layout/code.rs`
- Modify: `src/layout/mod.rs` (`pub mod code;`)
- Test: `src/layout/code.rs` (인라인)

**Interfaces:**
- Produces: `layout::code::Highlighter` with `Highlighter::new() -> Self` and `fn render(&self, lang: Option<&str>, code: &str, width: usize) -> Vec<StyledLine>`
- Consumes: `style::{Span, Style, Color, StyledLine}`

- [ ] **Step 1: 실패하는 테스트 작성 (`src/layout/code.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_each_source_line() {
        let hl = Highlighter::new();
        let lines = hl.render(Some("rust"), "let x = 1;\nlet y = 2;\n", 80);
        assert_eq!(lines.len(), 2);
        let joined: String = lines[0].0.iter().map(|s| s.text.as_str()).collect();
        assert!(joined.contains("let x = 1;"));
    }

    #[test]
    fn unknown_lang_falls_back_to_plain() {
        let hl = Highlighter::new();
        let lines = hl.render(None, "plain text\n", 80);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn applies_fg_color() {
        let hl = Highlighter::new();
        let lines = hl.render(Some("rust"), "fn main() {}\n", 80);
        assert!(lines[0].0.iter().any(|s| s.style.fg.is_some()));
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test layout::code 2>&1 | tail -20`
Expected: FAIL — `Highlighter` not found

- [ ] **Step 3: 구현 (`src/layout/code.rs`)**

배경은 절대 칠하지 않고 전경색만 적용한다 (터미널 배경 상속).

```rust
use crate::style::{Color, Span, Style, StyledLine};
use syntect::parsing::SyntaxSet;
use syntect::highlighting::{ThemeSet, Theme, Style as SynStyle};
use syntect::easy::HighlightLines;
use syntect::util::LinesWithEndings;

pub struct Highlighter {
    ps: SyntaxSet,
    theme: Theme,
}

impl Highlighter {
    pub fn new() -> Self {
        let ps = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes["base16-ocean.dark"].clone();
        Highlighter { ps, theme }
    }

    pub fn render(&self, lang: Option<&str>, code: &str, _width: usize) -> Vec<StyledLine> {
        let syntax = lang
            .and_then(|l| self.ps.find_syntax_by_token(l))
            .unwrap_or_else(|| self.ps.find_syntax_plain_text());
        let mut h = HighlightLines::new(syntax, &self.theme);
        let mut out = Vec::new();
        for line in LinesWithEndings::from(code) {
            let ranges: Vec<(SynStyle, &str)> = h.highlight_line(line, &self.ps).unwrap_or_default();
            let mut spans = Vec::new();
            for (syn, text) in ranges {
                let text = text.trim_end_matches('\n').to_string();
                if text.is_empty() { continue; }
                spans.push(Span {
                    text,
                    style: Style {
                        fg: Some(Color::Rgb(syn.foreground.r, syn.foreground.g, syn.foreground.b)),
                        ..Style::default()
                    },
                });
            }
            out.push(StyledLine(spans));
        }
        if out.is_empty() { out.push(StyledLine::default()); }
        out
    }
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test layout::code 2>&1 | tail -20`
Expected: PASS (3 tests)

- [ ] **Step 5: 커밋**

```bash
git add src/layout/code.rs src/layout/mod.rs
git commit -m "feat: syntax-highlight code blocks with syntect"
```

---

### Task 7: 터미널 정보 (크기 + 셀 픽셀)

**Files:**
- Create: `src/term.rs`
- Modify: `src/main.rs` (`mod term;`)
- Test: `src/term.rs` (인라인 — 순수 계산만)

**Interfaces:**
- Produces: `term::TermSize { cols: u16, rows: u16, cell_px_w: u16, cell_px_h: u16 }`, `term::query() -> TermSize`, `term::TermSize::image_rows(&self, img_px_w: u32, img_px_h: u32, target_cols: u16) -> u16`
- Consumes: 없음

- [ ] **Step 1: 실패하는 테스트 작성 (`src/term.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_rows_keeps_aspect() {
        // 셀 10x20px, 이미지 100x100px, 가로 5칸(=50px) 목표
        let t = TermSize { cols: 80, rows: 24, cell_px_w: 10, cell_px_h: 20 };
        // 50px 폭 → 비율유지 높이 50px → 50/20 = 2.5 → ceil 3행
        assert_eq!(t.image_rows(100, 100, 5), 3);
    }

    #[test]
    fn image_rows_min_one() {
        let t = TermSize { cols: 80, rows: 24, cell_px_w: 10, cell_px_h: 20 };
        assert_eq!(t.image_rows(10, 1, 1), 1);
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test term 2>&1 | tail -20`
Expected: FAIL — `TermSize` not found

- [ ] **Step 3: 구현 (`src/term.rs`)**

`ioctl(TIOCGWINSZ)` 로 행/열 + 픽셀 크기를 얻는다. libc 없이 직접 호출.

```rust
#[derive(Debug, Clone, Copy)]
pub struct TermSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_px_w: u16,
    pub cell_px_h: u16,
}

impl TermSize {
    pub fn image_rows(&self, img_px_w: u32, img_px_h: u32, target_cols: u16) -> u16 {
        if img_px_w == 0 || self.cell_px_h == 0 { return 1; }
        let target_px_w = target_cols as u32 * self.cell_px_w as u32;
        let scaled_h = (img_px_h as u64 * target_px_w as u64 / img_px_w as u64) as u32;
        let rows = (scaled_h + self.cell_px_h as u32 - 1) / self.cell_px_h as u32;
        rows.max(1) as u16
    }
}

#[repr(C)]
struct Winsize { ws_row: u16, ws_col: u16, ws_xpixel: u16, ws_ypixel: u16 }

pub fn query() -> TermSize {
    #[cfg(unix)]
    unsafe {
        let mut ws = Winsize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };
        // TIOCGWINSZ: macOS = 0x40087468, Linux = 0x5413
        #[cfg(target_os = "macos")]
        const TIOCGWINSZ: u64 = 0x40087468;
        #[cfg(target_os = "linux")]
        const TIOCGWINSZ: u64 = 0x5413;
        extern "C" { fn ioctl(fd: i32, request: u64, ...) -> i32; }
        let r = ioctl(1, TIOCGWINSZ, &mut ws as *mut Winsize);
        if r == 0 && ws.ws_col > 0 {
            let cols = ws.ws_col;
            let rows = ws.ws_row;
            let cell_px_w = if ws.ws_xpixel > 0 { ws.ws_xpixel / cols } else { 8 };
            let cell_px_h = if ws.ws_ypixel > 0 { ws.ws_ypixel / rows.max(1) } else { 16 };
            return TermSize { cols, rows, cell_px_w, cell_px_h };
        }
    }
    TermSize { cols: 80, rows: 24, cell_px_w: 8, cell_px_h: 16 }
}
```

> macOS의 `ioctl` request 타입은 `u64`(이 환경 darwin). Linux는 동일 시그니처로 동작. 빌드 경고가 나면 `request: libc::c_ulong` 대신 위 가변인자 `extern "C"` 유지.

- [ ] **Step 4: 테스트/빌드 통과 확인**

Run: `cargo test term && cargo build 2>&1 | tail -20`
Expected: PASS (2 tests), 빌드 성공

- [ ] **Step 5: 커밋**

```bash
git add src/term.rs src/main.rs
git commit -m "feat: query terminal size and cell pixel metrics"
```

---

### Task 8: 이미지 로드 + kitty 그래픽 프로토콜

**Files:**
- Create: `src/image_kitty.rs`
- Modify: `src/main.rs` (`mod image_kitty;`)
- Test: `src/image_kitty.rs` (인라인 — 인코딩만)

**Interfaces:**
- Produces:
  - `image_kitty::ImageData { px_w: u32, px_h: u32, fmt: Fmt, bytes: Vec<u8> }`, `enum Fmt { Png, Rgba }`
  - `image_kitty::load(src: &str, base_dir: &std::path::Path) -> Option<ImageData>` (로컬/원격)
  - `image_kitty::transmit_and_place(img: &ImageData, cols: u16, rows: u16, crop_rows: Option<(u16,u16)>, cell_px_h: u16) -> String` (이스케이프 생성; crop_rows=(skip_rows, show_rows))
- Consumes: `image` 크레이트, `ureq`, `base64`

- [ ] **Step 1: 실패하는 테스트 작성 (`src/image_kitty.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transmit_emits_graphics_escape() {
        let img = ImageData { px_w: 4, px_h: 4, fmt: Fmt::Rgba, bytes: vec![0u8; 4*4*4] };
        let out = transmit_and_place(&img, 2, 1, None, 16);
        // kitty 그래픽: ESC _ G ... ESC \
        assert!(out.contains("\x1b_G"));
        assert!(out.contains("\x1b\\"));
        assert!(out.contains("a=T")); // transmit+display
        assert!(out.contains("f=32")); // RGBA
        assert!(out.contains("c=2")); // 표시 칸수(가로)
        assert!(out.contains("r=1")); // 표시 행수
    }

    #[test]
    fn png_uses_format_100() {
        let img = ImageData { px_w: 1, px_h: 1, fmt: Fmt::Png, bytes: vec![1,2,3] };
        let out = transmit_and_place(&img, 1, 1, None, 16);
        assert!(out.contains("f=100"));
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test image_kitty 2>&1 | tail -20`
Expected: FAIL — types not found

- [ ] **Step 3: 구현 (`src/image_kitty.rs`)**

```rust
use std::path::Path;
use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fmt { Png, Rgba }

#[derive(Debug, Clone)]
pub struct ImageData { pub px_w: u32, pub px_h: u32, pub fmt: Fmt, pub bytes: Vec<u8> }

pub fn load(src: &str, base_dir: &Path) -> Option<ImageData> {
    let raw: Vec<u8> = if src.starts_with("http://") || src.starts_with("https://") {
        let mut buf = Vec::new();
        let resp = ureq::get(src).call().ok()?;
        use std::io::Read;
        resp.into_body().into_reader().take(50_000_000).read_to_end(&mut buf).ok()?;
        buf
    } else {
        let p = base_dir.join(src);
        std::fs::read(p).ok()?
    };

    // PNG는 그대로 전송 (헤더로 판별)
    let is_png = raw.len() > 8 && raw[0..8] == [0x89,0x50,0x4e,0x47,0x0d,0x0a,0x1a,0x0a];
    if is_png {
        let dims = image::load_from_memory(&raw).ok()?;
        return Some(ImageData { px_w: dims.width(), px_h: dims.height(), fmt: Fmt::Png, bytes: raw });
    }
    // 그 외: RGBA 디코드
    let dyn_img = image::load_from_memory(&raw).ok()?;
    let rgba = dyn_img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some(ImageData { px_w: w, px_h: h, fmt: Fmt::Rgba, bytes: rgba.into_raw() })
}

pub fn transmit_and_place(
    img: &ImageData, cols: u16, rows: u16,
    crop_rows: Option<(u16, u16)>, cell_px_h: u16,
) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(&img.bytes);
    let f = match img.fmt { Fmt::Png => 100, Fmt::Rgba => 32 };

    // 표시 영역: 세로 크롭이 있으면 source y/h(px)로 자른다
    let (y_px, h_rows) = match crop_rows {
        Some((skip, show)) => (skip as u32 * cell_px_h as u32, show),
        None => (0, rows),
    };

    // 공통 제어 키
    let mut ctrl = format!("a=T,f={},c={},r={}", f, cols, h_rows);
    if img.fmt == Fmt::Rgba {
        ctrl.push_str(&format!(",s={},v={}", img.px_w, img.px_h));
    }
    if let Some((_, _)) = crop_rows {
        let crop_h_px = h_rows as u32 * cell_px_h as u32;
        ctrl.push_str(&format!(",y={},H={}", y_px, crop_h_px));
    }

    // base64 페이로드를 4096바이트 청크로 분할, m=1(more) 체이닝
    let chunk = 4096;
    let bytes = b64.as_bytes();
    let mut out = String::new();
    if bytes.len() <= chunk {
        out.push_str(&format!("\x1b_G{};{}\x1b\\", ctrl, b64));
    } else {
        let mut i = 0;
        let mut first = true;
        while i < bytes.len() {
            let end = (i + chunk).min(bytes.len());
            let part = std::str::from_utf8(&bytes[i..end]).unwrap();
            let more = if end < bytes.len() { 1 } else { 0 };
            if first {
                out.push_str(&format!("\x1b_G{},m={};{}\x1b\\", ctrl, more, part));
                first = false;
            } else {
                out.push_str(&format!("\x1b_Gm={};{}\x1b\\", more, part));
            }
            i = end;
        }
    }
    out
}
```

> `ureq` 3.x 본문 읽기 API는 `resp.into_body().into_reader()` 형태. 컴파일 에러 시 `ureq` 3.3 문서대로 `resp.body_mut().read_to_vec()` 등으로 조정. 그래픽 키(`s`/`v`/`y`/`H`)는 Task 12 수동 확인에서 실제 표시로 검증.

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test image_kitty 2>&1 | tail -20`
Expected: PASS (2 tests)

- [ ] **Step 5: 커밋**

```bash
git add src/image_kitty.rs src/main.rs
git commit -m "feat: load images and emit kitty graphics protocol escapes"
```

---

### Task 9: 문서 레이아웃 조립 (Block → Element)

**Files:**
- Create: `src/layout/document.rs`
- Modify: `src/layout/mod.rs` (`pub mod document;`)
- Test: `src/layout/document.rs` (인라인)

**Interfaces:**
- Produces:
  - `layout::document::{Element, Doc}`:
    - `pub enum Element { TextRows(Vec<StyledLine>), Heading { sc: heading::Scale, lines: Vec<String>, height: usize }, Image { data: image_kitty::ImageData, cols: u16, rows: u16 } }`
    - `pub struct Doc { pub elements: Vec<Element>, pub total_rows: usize }`
    - `impl Element { pub fn height(&self) -> usize }`
  - `layout::document::build(blocks: &[ir::Block], width: usize, term: &term::TermSize, hl: &code::Highlighter, base_dir: &Path) -> Doc`
- Consumes: Task 3–8 전부

- [ ] **Step 1: 실패하는 테스트 작성 (`src/layout/document.rs`)**

```rust
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
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test layout::document 2>&1 | tail -20`
Expected: FAIL — `build` not found

- [ ] **Step 3: 구현 (`src/layout/document.rs`)**

각 블록을 Element로 변환하고 블록 사이에 빈 행을 한 줄 넣는다. 제목/이미지는 전용 Element, 나머지는 TextRows.

```rust
use std::path::Path;
use crate::ir::{Block, Inline, Align};
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

pub struct Doc { pub elements: Vec<Element>, pub total_rows: usize }

pub fn build(blocks: &[Block], width: usize, term: &TermSize, hl: &Highlighter, base_dir: &Path) -> Doc {
    let mut elements = Vec::new();
    emit_blocks(blocks, width, 0, term, hl, base_dir, &mut elements);
    // 블록 사이 간격: 각 최상위 블록 뒤에 빈 행 1개
    let total_rows = elements.iter().map(|e| e.height()).sum();
    Doc { elements, total_rows }
}

fn blank() -> Element { Element::TextRows(vec![StyledLine::default()]) }

fn emit_blocks(blocks: &[Block], width: usize, indent: usize, term: &TermSize,
               hl: &Highlighter, base_dir: &Path, out: &mut Vec<Element>) {
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
                // 좌측 '│ ' 마커 부착
                for el in sub {
                    if let Element::TextRows(rows) = el {
                        let marked = rows.into_iter().map(|mut l| {
                            let mut spans = vec![Span { text: "│ ".into(), style: Style { dim: true, ..Style::default() } }];
                            spans.append(&mut l.0);
                            StyledLine(spans)
                        }).collect();
                        out.push(Element::TextRows(marked));
                    } else { out.push(el); }
                }
            }
            Block::List { ordered, start, items } => {
                for (idx, item) in items.iter().enumerate() {
                    let marker = if *ordered { format!("{}. ", *start as usize + idx) } else { "• ".into() };
                    let mut sub = Vec::new();
                    emit_blocks(item, width, indent + display_width(&marker), term, hl, base_dir, &mut sub);
                    // 첫 줄에 마커 부착
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
                        let txt = format!("[image: {}]", if alt.is_empty() { url } else { alt });
                        out.push(Element::TextRows(vec![StyledLine(vec![Span { text: txt, style: Style { dim: true, ..Style::default() } }])]));
                    }
                }
            }
            Block::Rule => {
                out.push(Element::TextRows(vec![StyledLine(vec![Span {
                    text: "─".repeat(cw), style: Style { dim: true, ..Style::default() } }])]));
            }
        }
        // 최상위 블록 사이 간격
        if indent == 0 && i + 1 < blocks.len() {
            out.push(blank());
        }
    }
}

fn indent_lines(lines: Vec<StyledLine>, indent: usize) -> Vec<StyledLine> {
    if indent == 0 { return lines; }
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
                // 첫 줄 선두의 indent 패딩을 marker로 치환
                let pad = " ".repeat(indent);
                let lead = Span { text: format!("{}{}", pad, marker), style: Style::default() };
                // 기존 indent 패딩 span 제거 시도
                if let Some(s0) = first.0.first() {
                    if s0.text.trim().is_empty() { first.0.remove(0); }
                }
                first.0.insert(0, lead);
                return;
            }
        }
    }
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test layout::document 2>&1 | tail -20`
Expected: PASS (3 tests)

- [ ] **Step 5: 커밋**

```bash
git add src/layout/document.rs src/layout/mod.rs
git commit -m "feat: assemble Block IR into laid-out elements with row heights"
```

---

### Task 10: 렌더러 (Element → 이스케이프, 행 클립)

**Files:**
- Create: `src/render.rs`
- Modify: `src/main.rs` (`mod render;`)
- Test: `src/render.rs` (인라인 — SGR/클립 로직)

**Interfaces:**
- Produces:
  - `render::sgr(style: &style::Style) -> String` (스타일 → SGR 시작 시퀀스), `render::RESET: &str`
  - `render::line_to_ansi(line: &style::StyledLine) -> String`
  - `render::draw_viewport(out: &mut impl Write, doc: &Doc, scroll: usize, term: &TermSize) -> io::Result<()>` — scroll행부터 term.rows행만큼 그린다 (행 클립 + 이미지 크롭 + 제목 원자 처리)
- Consumes: Task 9 `Doc/Element`, Task 5 `osc66`, Task 8 `transmit_and_place`

- [ ] **Step 1: 실패하는 테스트 작성 (`src/render.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, Span, Style, StyledLine};

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
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test render 2>&1 | tail -20`
Expected: FAIL — `sgr` not found

- [ ] **Step 3: 구현 (`src/render.rs`)**

```rust
use std::io::{self, Write};
use crate::style::{Color, Style, StyledLine};
use crate::term::TermSize;
use crate::layout::document::{Doc, Element};
use crate::layout::heading::osc66;
use crate::image_kitty::transmit_and_place;

pub const RESET: &str = "\x1b[0m";

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

// 행 단위 뷰포트 렌더. scroll = 맨 위에 보일 전역 행 인덱스.
pub fn draw_viewport(out: &mut impl Write, doc: &Doc, scroll: usize, term: &TermSize) -> io::Result<()> {
    // 화면 클리어 + 커서 홈
    write!(out, "\x1b[2J\x1b[H")?;
    let view_rows = term.rows.saturating_sub(1) as usize; // 마지막 행은 상태줄
    let mut screen_row = 0usize; // 0-based 화면 행
    let mut global_row = 0usize; // 누적 전역 행

    for el in &doc.elements {
        let h = el.height();
        let el_start = global_row;
        let el_end = global_row + h; // [start, end)
        global_row = el_end;

        // 이 요소가 뷰포트와 겹치는가?
        let vstart = scroll;
        let vend = scroll + view_rows;
        if el_end <= vstart || el_start >= vend { continue; }

        match el {
            Element::TextRows(rows) => {
                for (ri, line) in rows.iter().enumerate() {
                    let grow = el_start + ri;
                    if grow < vstart || grow >= vend { continue; }
                    let srow = grow - vstart;
                    write!(out, "\x1b[{};1H", srow + 1)?; // 1-based
                    write!(out, "{}", line_to_ansi(line))?;
                }
            }
            Element::Heading { sc, lines, height } => {
                // 원자 처리: 요소 전체가 뷰포트 안에 완전히 들어올 때만 그린다
                if el_start >= vstart && el_end <= vend {
                    let per = height / lines.len().max(1);
                    for (li, text) in lines.iter().enumerate() {
                        let srow = (el_start - vstart) + li * per;
                        write!(out, "\x1b[{};1H", srow + 1)?;
                        // 4096바이트 분할
                        for chunk in split_4096(text) {
                            write!(out, "{}", osc66(&chunk, sc))?;
                        }
                    }
                }
                // 부분 가림이면 빈 칸으로 둔다 (clear가 이미 비움)
            }
            Element::Image { data, cols, rows } => {
                let r = *rows as usize;
                // 보이는 세로 구간 계산
                let visible_top = el_start.max(vstart);
                let visible_bot = el_end.min(vend);
                let skip = (visible_top - el_start) as u16;
                let show = (visible_bot - visible_top) as u16;
                let srow = visible_top - vstart;
                write!(out, "\x1b[{};1H", srow + 1)?;
                let crop = if skip == 0 && show == *rows { None } else { Some((skip, show)) };
                let esc = transmit_and_place(data, *cols, *rows, crop, term.cell_px_h);
                write!(out, "{}", esc)?;
                let _ = r;
            }
        }
    }

    // 상태줄
    let pct = if doc.total_rows > view_rows {
        (scroll * 100) / (doc.total_rows - view_rows).max(1)
    } else { 100 };
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
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test render 2>&1 | tail -20`
Expected: PASS (3 tests)

- [ ] **Step 5: 커밋**

```bash
git add src/render.rs src/main.rs
git commit -m "feat: render elements to escapes with viewport clipping"
```

---

### Task 11: 페이저 (alternate screen + 스크롤 입력 루프)

**Files:**
- Create: `src/pager.rs`
- Modify: `src/main.rs` (`mod pager;`)
- Test: `src/pager.rs` (인라인 — 스크롤 클램프 로직만)

**Interfaces:**
- Produces:
  - `pager::clamp_scroll(scroll: isize, total_rows: usize, view_rows: usize) -> usize`
  - `pager::run(blocks: &[ir::Block], base_dir: &Path) -> io::Result<()>` — alternate screen 진입, 입력 루프, 종료 시 복원
- Consumes: Task 7 `term`, Task 9 `build`, Task 10 `draw_viewport`, Task 6 `Highlighter`

- [ ] **Step 1: 실패하는 테스트 작성 (`src/pager.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_to_zero_minimum() {
        assert_eq!(clamp_scroll(-5, 100, 24), 0);
    }

    #[test]
    fn clamp_to_max_bottom() {
        // 최대 스크롤 = total - view = 100 - 24 = 76
        assert_eq!(clamp_scroll(999, 100, 24), 76);
    }

    #[test]
    fn no_scroll_when_fits() {
        assert_eq!(clamp_scroll(10, 20, 24), 0);
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test pager 2>&1 | tail -20`
Expected: FAIL — `clamp_scroll` not found

- [ ] **Step 3: 구현 (`src/pager.rs`)**

```rust
use std::io::{self, Write};
use std::path::Path;
use crossterm::{execute, terminal, event};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use crate::ir::Block;
use crate::term;
use crate::layout::code::Highlighter;
use crate::layout::document::{self, Doc};
use crate::render::draw_viewport;

pub fn clamp_scroll(scroll: isize, total_rows: usize, view_rows: usize) -> usize {
    let max = total_rows.saturating_sub(view_rows);
    scroll.max(0).min(max as isize) as usize
}

pub fn run(blocks: &[Block], base_dir: &Path) -> io::Result<()> {
    let hl = Highlighter::new();
    let mut stdout = io::stdout();

    terminal::enable_raw_mode()?;
    execute!(stdout,
        terminal::EnterAlternateScreen,
        event::EnableMouseCapture,
        crossterm::cursor::Hide,
    )?;

    let result = run_loop(&mut stdout, blocks, base_dir, &hl);

    execute!(stdout,
        event::DisableMouseCapture,
        terminal::LeaveAlternateScreen,
        crossterm::cursor::Show,
    )?;
    terminal::disable_raw_mode()?;
    result
}

fn build_doc(blocks: &[Block], t: &term::TermSize, hl: &Highlighter, base_dir: &Path) -> Doc {
    let width = t.cols.saturating_sub(4) as usize; // 좌우 여백 2칸씩
    document::build(blocks, width, t, hl, base_dir)
}

fn run_loop(stdout: &mut impl Write, blocks: &[Block], base_dir: &Path, hl: &Highlighter) -> io::Result<()> {
    let mut t = term::query();
    let mut doc = build_doc(blocks, &t, hl, base_dir);
    let mut scroll: usize = 0;

    loop {
        let view_rows = t.rows.saturating_sub(1) as usize;
        scroll = clamp_scroll(scroll as isize, doc.total_rows, view_rows);
        draw_viewport(stdout, &doc, scroll, &t)?;

        match event::read()? {
            Event::Key(KeyEvent { code, modifiers, .. }) => match (code, modifiers) {
                (KeyCode::Char('q'), _) => break,
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                (KeyCode::Char('j'), _) | (KeyCode::Down, _) =>
                    scroll = clamp_scroll(scroll as isize + 1, doc.total_rows, view_rows),
                (KeyCode::Char('k'), _) | (KeyCode::Up, _) =>
                    scroll = clamp_scroll(scroll as isize - 1, doc.total_rows, view_rows),
                (KeyCode::Char(' '), _) | (KeyCode::PageDown, _) =>
                    scroll = clamp_scroll(scroll as isize + view_rows as isize, doc.total_rows, view_rows),
                (KeyCode::Char('b'), _) | (KeyCode::PageUp, _) =>
                    scroll = clamp_scroll(scroll as isize - view_rows as isize, doc.total_rows, view_rows),
                (KeyCode::Char('g'), _) | (KeyCode::Home, _) => scroll = 0,
                (KeyCode::Char('G'), _) | (KeyCode::End, _) =>
                    scroll = clamp_scroll(isize::MAX, doc.total_rows, view_rows),
                _ => {}
            },
            Event::Mouse(me) => match me.kind {
                MouseEventKind::ScrollDown =>
                    scroll = clamp_scroll(scroll as isize + 3, doc.total_rows, view_rows),
                MouseEventKind::ScrollUp =>
                    scroll = clamp_scroll(scroll as isize - 3, doc.total_rows, view_rows),
                _ => {}
            },
            Event::Resize(_, _) => {
                let old_total = doc.total_rows.max(1);
                t = term::query();
                doc = build_doc(blocks, &t, hl, base_dir);
                // 비율 보정
                scroll = (scroll * doc.total_rows / old_total)
                    .min(doc.total_rows.saturating_sub(t.rows.saturating_sub(1) as usize));
            }
            _ => {}
        }
    }
    Ok(())
}
```

> crossterm 0.29 `event::read()`는 블로킹. 리사이즈는 `Event::Resize`로 도착(SIGWINCH 직접 처리 불필요). `EnableMouseCapture`로 휠 이벤트 수신.

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test pager 2>&1 | tail -20`
Expected: PASS (3 tests)

- [ ] **Step 5: 커밋**

```bash
git add src/pager.rs src/main.rs
git commit -m "feat: full-screen pager with scroll input loop"
```

---

### Task 12: main 연결 + 엔드투엔드 수동 검증

**Files:**
- Modify: `src/main.rs`
- Create: `samples/demo.md`
- Create: `samples/rust-logo.png` (작은 PNG, 아래 명령으로 생성)

**Interfaces:**
- Consumes: 모든 이전 Task

- [ ] **Step 1: main.rs 최종 연결**

`print!`로 길이 출력하던 임시 코드를 제거하고 페이저 호출로 교체:

```rust
mod cli;
mod ir;
mod parser;
mod style;
mod term;
mod image_kitty;
mod render;
mod pager;
mod layout {
    pub mod inline;
    pub mod table;
    pub mod heading;
    pub mod code;
    pub mod document;
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cfg = match cli::parse_args(args) {
        Ok(c) => c,
        Err(cli::CliError::MissingArg) => { eprintln!("usage: mdcat <file.md>"); std::process::exit(2); }
        Err(cli::CliError::Io(e)) => { eprintln!("mdcat: {e}"); std::process::exit(1); }
    };
    let src = match cli::read_source(&cfg) {
        Ok(s) => s,
        Err(cli::CliError::Io(e)) => { eprintln!("mdcat: {e}"); std::process::exit(1); }
        Err(_) => unreachable!(),
    };
    let blocks = parser::parse(&src);
    let base_dir = cfg.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("."));
    if let Err(e) = pager::run(&blocks, &base_dir) {
        eprintln!("mdcat: {e}");
        std::process::exit(1);
    }
}
```

> `src/layout/mod.rs`에 이미 각 `pub mod`가 선언돼 있다면 main.rs의 인라인 `mod layout {}`는 제거하고 `mod layout;`만 둔다. (둘 중 하나만 — 중복 선언 금지) 작업자는 기존 `src/layout/mod.rs` 존재 여부에 맞춰 `mod layout;` 한 줄로 통일한다.

- [ ] **Step 2: 데모 마크다운 작성 (`samples/demo.md`)**

```markdown
# mdcat 데모

본문은 터미널 기본 색과 폰트를 따릅니다. **굵게**, *기울임*, `인라인 코드`, [링크](https://example.com).

## 표 예시

| 이름 | 값 | 정렬 |
|:-----|---:|:----:|
| alpha | 1 | x |
| beta | 22 | y |

### 코드

```rust
fn main() {
    println!("hello, mdcat");
}
```

> 인용문도 됩니다.

- 리스트 1
- 리스트 2
  - 중첩 항목

![로고](rust-logo.png)
```

- [ ] **Step 3: 테스트용 PNG 생성**

Run:
```bash
printf '\x89PNG\r\n\x1a\n' > /dev/null; \
python3 -c "from PIL import Image; Image.new('RGB',(120,80),(180,80,40)).save('samples/rust-logo.png')" 2>/dev/null \
  || (cd samples && cargo new --bin _g >/dev/null 2>&1; echo skip)
```
PIL이 없으면 임의의 작은 PNG를 `samples/rust-logo.png`로 복사한다 (예: 시스템 아이콘). 이미지가 없으면 mdcat은 `[image: ...]` 텍스트로 대체 표시되므로 나머지 검증에는 지장 없음.

- [ ] **Step 4: 빌드 + 전체 테스트**

Run: `cargo build --release && cargo test 2>&1 | tail -25`
Expected: 빌드 성공, 모든 단위 테스트 PASS

- [ ] **Step 5: 수동 실행 검증 (kitty에서)**

Run: `./target/release/mdcat samples/demo.md`

확인 항목:
- [ ] 제목 H1/H2/H3가 본문보다 크게 표시됨 (OSC 66 동작) — 크기가 이상하면 Task 5의 `scale_for` n/d 값과 `effective_factor`를 실제 표시에 맞춰 조정
- [ ] 본문/표/코드가 터미널 기본 배경·전경을 사용 (배경 칠하지 않음)
- [ ] 표 테두리가 깔끔하고 정렬(좌/우/가운데)이 반영됨
- [ ] 코드 블록에 색이 입혀짐
- [ ] 이미지가 표시됨 (PNG 있을 경우)
- [ ] 마우스 휠로 부드럽게 스크롤됨, `j/k/Space/b/g/G` 동작
- [ ] 터미널 리사이즈 시 가로가 다시 꽉 차고 재배치됨
- [ ] `q`로 종료 시 원래 화면 복원, 그래픽 잔상 없음

발견된 문제는 해당 Task로 돌아가 수정 후 재검증.

- [ ] **Step 6: 커밋**

```bash
git add src/main.rs samples/
git commit -m "feat: wire main entrypoint and add demo sample"
```

---

## Self-Review (작성자 점검 결과)

**Spec coverage:**
- 전체화면 페이저/스크롤 → Task 11 ✓
- 가로 꽉 채움 + 워드랩 → Task 3, 9 (width = cols-4) ✓
- 터미널 기본 색/배경/폰트 상속 → 배경 미지정, fg는 코드만 (Task 6, 10) ✓
- 제목 OSC 66 → Task 5, 10 ✓
- 이미지 kitty 그래픽 (로컬+원격) → Task 8 ✓
- 표 → Task 4 ✓
- 마우스 스크롤 → Task 11 ✓
- `mdcat <path>` → Task 1, 12 ✓
- 코드 하이라이팅 → Task 6 ✓

**알려진 검증 의존 항목 (수동 확인에서 확정):**
- OSC 66 분수 스케일의 정확한 의미(`effective_factor` 식) — Task 5에서 테스트 통과 식 채택, Task 12에서 실제 크기 확인
- kitty 그래픽 키(`s`/`v`/`y`/`H`/`c`/`r`) 동작 — Task 12에서 표시 확인
- `ureq` 3.3 본문 읽기 API 시그니처 — Task 8에서 컴파일 시 확정
- `pulldown-cmark` 0.13 `CowStr` 변환 메서드 — Task 2에서 컴파일 시 확정
- 표 `chrome` 폭 상수와 셀 마커 폭 일치 — Task 4 테스트로 확인

**Placeholder scan:** 코드 블록 모두 실제 구현 포함. "적절히 처리" 류 없음. 라이브러리 API 미세 차이는 명시적 폴백 지침으로 대체.

**Type consistency:** `StyledLine`/`Span`/`Style`(Task 3) → 전 Task 일관. `Element`/`Doc`(Task 9) → Task 10/11 일관. `TermSize`(Task 7) → Task 8/9/10/11 일관. `Scale`(Task 5) → Task 9/10 일관.

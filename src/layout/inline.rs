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
            Inline::Strike(c) => flatten(c, Style { dim: true, ..st }, out),
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

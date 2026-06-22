use crate::style::{Color, Span, Style, StyledLine};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SynStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
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
            let ranges: Vec<(SynStyle, &str)> =
                h.highlight_line(line, &self.ps).unwrap_or_default();
            let mut spans = Vec::new();
            for (syn, text) in ranges {
                let text = text.trim_end_matches('\n').to_string();
                if text.is_empty() {
                    continue;
                }
                spans.push(Span {
                    text,
                    style: Style {
                        fg: Some(Color::Rgb(
                            syn.foreground.r,
                            syn.foreground.g,
                            syn.foreground.b,
                        )),
                        ..Style::default()
                    },
                });
            }
            out.push(StyledLine(spans));
        }
        if out.is_empty() {
            out.push(StyledLine::default());
        }
        out
    }
}

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

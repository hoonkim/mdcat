use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Color {
    #[allow(dead_code)]
    Indexed(u8),
    Rgb(u8, u8, u8),
}

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

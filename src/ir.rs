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

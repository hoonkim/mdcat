mod cli;
mod image_kitty;
mod ir;
mod layout;
mod pager;
mod parser;
mod render;
mod style;
mod term;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cfg = match cli::parse_args(args) {
        Ok(c) => c,
        Err(cli::CliError::MissingArg) => {
            eprintln!("usage: mdcat <file.md>");
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("mdcat: {e:?}");
            std::process::exit(1);
        }
    };
    let src = match cli::read_source(&cfg) {
        Ok(s) => s,
        Err(cli::CliError::Io(e)) => {
            eprintln!("mdcat: {e}");
            std::process::exit(1);
        }
        Err(_) => unreachable!(),
    };
    let blocks = parser::parse(&src);
    let base_dir = cfg.path.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    if let Err(e) = pager::run(&blocks, &base_dir) {
        eprintln!("mdcat: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod e2e_tests {
    use crate::parser;
    use crate::layout::code::Highlighter;
    use crate::layout::document;
    use crate::render::draw_viewport;
    use crate::term::TermSize;
    use std::path::Path;

    const DEMO_MD: &str = r#"# mdcat Demo

A paragraph with **bold**, *italic*, `inline code`, and [a link](https://example.com).

## Table Example

| Name  | Value | Align  |
|:------|------:|:------:|
| alpha |     1 | x      |
| beta  |    22 | y      |

### Code

```rust
fn main() {
    println!("hello, mdcat");
}
```

> A blockquote.

- List item 1
- List item 2
  - Nested item

![logo](rust-logo.png)
"#;

    #[test]
    fn pipeline_produces_nonempty_output_with_heading_and_table() {
        let term = TermSize { cols: 80, rows: 24, cell_px_w: 8, cell_px_h: 16 };
        let blocks = parser::parse(DEMO_MD);
        let hl = Highlighter::new();
        let doc = document::build(&blocks, 76, &term, &hl, Path::new("."));

        let mut buf: Vec<u8> = Vec::new();
        draw_viewport(&mut buf, &doc, 0, &term).expect("draw_viewport must not fail");

        let output = String::from_utf8_lossy(&buf);

        // Non-empty output
        assert!(!output.is_empty(), "draw_viewport produced no output");

        // OSC 66 heading sequence is present
        assert!(
            output.contains("\x1b]66;"),
            "expected OSC 66 heading sequence in output"
        );

        // Box-drawing characters from the table
        assert!(
            output.contains('┌') || output.contains('│'),
            "expected box-drawing character from table in output"
        );
    }
}

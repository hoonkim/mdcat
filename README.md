# mdcat

A terminal Markdown viewer for [kitty](https://sw.kovidgoyal.net/kitty/), written in Rust.

It renders Markdown as real terminal text — inheriting your terminal's default
colors, background, and font — and uses two kitty protocols for the parts plain
text can't do:

- **Text Sizing Protocol (OSC 66)** to draw headings larger than body text.
- **Graphics Protocol** to display images inline.

## Features

- Full-screen pager with smooth mouse-wheel and keyboard scrolling
- Headings scaled by level (H1 largest)
- Tables with box-drawing borders and column alignment
- Syntax-highlighted code blocks (via [syntect](https://github.com/trishume/syntect))
- Bullet / ordered / task lists, block quotes, horizontal rules
- Inline **bold**, *italic*, `code`, ~~strikethrough~~, and links
- Inline images from local paths or `http(s)` URLs, downscaled to display size
- Fills the terminal width with symmetric margins; reflows on resize
- Flicker-free redraws via synchronized output

## Requirements

- The [kitty](https://sw.kovidgoyal.net/kitty/) terminal (uses kitty-specific
  protocols; other terminals are not supported)
- Rust toolchain (to build)

## Install

### Homebrew (macOS)

```bash
brew install hoonkim/tap/mdcat
```

If the Rust toolchain isn't present, Homebrew installs it automatically as a
build dependency.

### From source

```bash
cargo build --release
# binary at target/release/mdcat
```

Or install it onto your `PATH` with Cargo:

```bash
cargo install --path .
```

## Usage

```bash
mdcat <file.md>
```

For example:

```bash
mdcat samples/showcase.md
```

### Controls

| Key                 | Action            |
|---------------------|-------------------|
| Mouse wheel / `j` `k` | Scroll down / up |
| `Space` / `b`       | Page down / up    |
| `g` / `G`           | Top / bottom      |
| `q` / `Ctrl-C`      | Quit              |

## Notes

- Body text uses your terminal's colors and font, so emphasis (italic) only
  renders slanted if your configured font has an italic face. CJK fonts
  generally have none, so CJK emphasis stays upright.
- Heading sizes use integer kitty scales (H1 = 3×, H2 = 2×, H3–H6 bold) because
  fractional text scaling is not honored on all kitty builds.

## License

MIT

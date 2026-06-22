use std::io::{self, Write};
use std::path::Path;
use crossterm::{execute, terminal, event};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use crate::ir::Block;
use crate::term;
use crate::layout::code::Highlighter;
use crate::layout::document;
use crate::render::draw_viewport;

pub fn clamp_scroll(scroll: isize, total_rows: usize, view_rows: usize) -> usize {
    let max = total_rows.saturating_sub(view_rows);
    scroll.max(0).min(max as isize) as usize
}

/// Drop guard that restores the terminal on any exit path (normal, error, or panic).
struct TermGuard;

impl Drop for TermGuard {
    fn drop(&mut self) {
        let mut out = std::io::stdout();
        let _ = execute!(
            out,
            event::DisableMouseCapture,
            terminal::LeaveAlternateScreen,
            crossterm::cursor::Show,
        );
        let _ = terminal::disable_raw_mode();
    }
}

pub fn run(blocks: &[Block], base_dir: &Path) -> io::Result<()> {
    let hl = Highlighter::new();
    let mut stdout = io::stdout();

    terminal::enable_raw_mode()?;
    if let Err(e) = execute!(
        stdout,
        terminal::EnterAlternateScreen,
        event::EnableMouseCapture,
        crossterm::cursor::Hide,
    ) {
        let _ = terminal::disable_raw_mode();
        return Err(e);
    }

    // The guard's Drop handles ALL teardown — normal return, Err, and panic.
    let _guard = TermGuard;
    run_loop(&mut stdout, blocks, base_dir, &hl)
}

fn build_doc<'a>(
    blocks: &[Block],
    t: &term::TermSize,
    hl: &Highlighter,
    base_dir: &Path,
) -> document::Doc {
    // Symmetric left/right margins; render offsets content by LEFT_MARGIN.
    let width = (t.cols as usize).saturating_sub(2 * crate::render::LEFT_MARGIN as usize);
    document::build(blocks, width, t, hl, base_dir)
}

fn run_loop(
    stdout: &mut impl Write,
    blocks: &[Block],
    base_dir: &Path,
    hl: &Highlighter,
) -> io::Result<()> {
    let mut t = term::query();
    let mut doc = build_doc(blocks, &t, hl, base_dir);
    let mut scroll: usize = 0;

    loop {
        let view_rows = (t.rows as usize).saturating_sub(1);
        scroll = clamp_scroll(scroll as isize, doc.total_rows, view_rows);
        draw_viewport(stdout, &doc, scroll, &t)?;

        match event::read()? {
            Event::Key(KeyEvent { code, modifiers, .. }) => match (code, modifiers) {
                (KeyCode::Char('q'), _) => break,
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
                    scroll = clamp_scroll(scroll as isize + 1, doc.total_rows, view_rows);
                }
                (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
                    scroll = clamp_scroll(scroll as isize - 1, doc.total_rows, view_rows);
                }
                (KeyCode::Char(' '), _) | (KeyCode::PageDown, _) => {
                    scroll = clamp_scroll(
                        scroll as isize + view_rows as isize,
                        doc.total_rows,
                        view_rows,
                    );
                }
                (KeyCode::Char('b'), _) | (KeyCode::PageUp, _) => {
                    scroll = clamp_scroll(
                        scroll as isize - view_rows as isize,
                        doc.total_rows,
                        view_rows,
                    );
                }
                (KeyCode::Char('g'), _) | (KeyCode::Home, _) => {
                    scroll = 0;
                }
                (KeyCode::Char('G'), _) | (KeyCode::End, _) => {
                    scroll = clamp_scroll(isize::MAX, doc.total_rows, view_rows);
                }
                _ => {}
            },
            Event::Mouse(me) => match me.kind {
                MouseEventKind::ScrollDown => {
                    scroll = clamp_scroll(scroll as isize + 3, doc.total_rows, view_rows);
                }
                MouseEventKind::ScrollUp => {
                    scroll = clamp_scroll(scroll as isize - 3, doc.total_rows, view_rows);
                }
                _ => {}
            },
            Event::Resize(_, _) => {
                let old_total = doc.total_rows.max(1);
                t = term::query();
                doc = build_doc(blocks, &t, hl, base_dir);
                let new_view_rows = (t.rows as usize).saturating_sub(1);
                // Scale scroll proportionally to new total, then clamp
                scroll = (scroll.saturating_mul(doc.total_rows) / old_total)
                    .min(doc.total_rows.saturating_sub(new_view_rows));
            }
            _ => {}
        }
    }
    Ok(())
}

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

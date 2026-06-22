#[derive(Debug, Clone, Copy)]
pub struct TermSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_px_w: u16,
    pub cell_px_h: u16,
}

impl TermSize {
    pub fn image_rows(&self, img_px_w: u32, img_px_h: u32, target_cols: u16) -> u16 {
        if img_px_w == 0 || self.cell_px_h == 0 {
            return 1;
        }
        let target_px_w = target_cols as u32 * self.cell_px_w as u32;
        let scaled_h = (img_px_h as u64 * target_px_w as u64 / img_px_w as u64) as u32;
        let rows = (scaled_h + self.cell_px_h as u32 - 1) / self.cell_px_h as u32;
        rows.max(1) as u16
    }
}

/// Query the terminal geometry via crossterm.
///
/// Uses `window_size()` which reports both cell counts and pixel dimensions
/// (needed for image sizing). Falls back to `size()` (cells only) and finally
/// to a sane 80x24 default. We deliberately avoid a raw `ioctl` here: declaring
/// the variadic C `ioctl` with a fixed signature segfaults on AArch64 (the
/// variadic ABI passes the arg on the stack, not in a register).
pub fn query() -> TermSize {
    // Preferred: window_size gives pixel dimensions, so we can derive cell px.
    if let Ok(ws) = crossterm::terminal::window_size() {
        if ws.columns > 0 {
            let cols = ws.columns;
            let rows = ws.rows;
            let cell_px_w = if ws.width > 0 { ws.width / cols } else { 8 };
            let cell_px_h = if ws.height > 0 {
                ws.height / rows.max(1)
            } else {
                16
            };
            return TermSize {
                cols,
                rows,
                cell_px_w,
                cell_px_h,
            };
        }
    }
    // Fallback: cell counts only (no pixel info), assume typical cell size.
    if let Ok((cols, rows)) = crossterm::terminal::size() {
        if cols > 0 {
            return TermSize {
                cols,
                rows,
                cell_px_w: 8,
                cell_px_h: 16,
            };
        }
    }
    TermSize {
        cols: 80,
        rows: 24,
        cell_px_w: 8,
        cell_px_h: 16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_rows_keeps_aspect() {
        // 셀 10x20px, 이미지 100x100px, 가로 5칸(=50px) 목표
        let t = TermSize {
            cols: 80,
            rows: 24,
            cell_px_w: 10,
            cell_px_h: 20,
        };
        // 50px 폭 → 비율유지 높이 50px → 50/20 = 2.5 → ceil 3행
        assert_eq!(t.image_rows(100, 100, 5), 3);
    }

    #[test]
    fn image_rows_min_one() {
        let t = TermSize {
            cols: 80,
            rows: 24,
            cell_px_w: 10,
            cell_px_h: 20,
        };
        assert_eq!(t.image_rows(10, 1, 1), 1);
    }
}

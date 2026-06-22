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

#[repr(C)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

pub fn query() -> TermSize {
    #[cfg(unix)]
    unsafe {
        let mut ws = Winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        #[cfg(target_os = "macos")]
        const TIOCGWINSZ: u64 = 0x40087468;
        #[cfg(target_os = "linux")]
        const TIOCGWINSZ: u64 = 0x5413;

        extern "C" {
            fn ioctl(fd: i32, request: u64, arg: *mut Winsize) -> i32;
        }

        let r = ioctl(1, TIOCGWINSZ, &mut ws as *mut Winsize);
        if r == 0 && ws.ws_col > 0 {
            let cols = ws.ws_col;
            let rows = ws.ws_row;
            let cell_px_w = if ws.ws_xpixel > 0 { ws.ws_xpixel / cols } else { 8 };
            let cell_px_h = if ws.ws_ypixel > 0 { ws.ws_ypixel / rows.max(1) } else { 16 };
            return TermSize {
                cols,
                rows,
                cell_px_w,
                cell_px_h,
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

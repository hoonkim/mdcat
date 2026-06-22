use std::path::Path;
use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fmt {
    Png,
    Rgba,
}

#[derive(Debug, Clone)]
pub struct ImageData {
    pub px_w: u32,
    pub px_h: u32,
    pub fmt: Fmt,
    pub bytes: Vec<u8>,
}

pub fn load(src: &str, base_dir: &Path) -> Option<ImageData> {
    let raw: Vec<u8> = if src.starts_with("http://") || src.starts_with("https://") {
        let mut resp = ureq::get(src).call().ok()?;
        resp.body_mut()
            .with_config()
            .limit(50_000_000)
            .read_to_vec()
            .ok()?
    } else {
        let p = base_dir.join(src);
        std::fs::read(p).ok()?
    };

    // PNG: detect by magic bytes and pass through as-is (f=100)
    let is_png = raw.len() > 8 && raw[0..8] == [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    if is_png {
        let dims = image::load_from_memory(&raw).ok()?;
        return Some(ImageData {
            px_w: dims.width(),
            px_h: dims.height(),
            fmt: Fmt::Png,
            bytes: raw,
        });
    }

    // Non-PNG: decode to RGBA (f=32)
    let dyn_img = image::load_from_memory(&raw).ok()?;
    let rgba = dyn_img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Some(ImageData {
        px_w: w,
        px_h: h,
        fmt: Fmt::Rgba,
        bytes: rgba.into_raw(),
    })
}

/// `rows` is the FULL display height of the (uncropped) image in cells.
/// `crop_rows = (skip_rows, show_rows)` selects a vertical slice of those rows.
/// The kitty source rectangle (`y`,`h`) is in SOURCE-IMAGE pixels, so the slice
/// is mapped from display rows back to source pixels via `img.px_h / rows`
/// (using cell pixels here would be wrong whenever the image is scaled).
pub fn transmit_and_place(
    img: &ImageData,
    cols: u16,
    rows: u16,
    crop_rows: Option<(u16, u16)>,
) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(&img.bytes);
    let f = match img.fmt {
        Fmt::Png => 100,
        Fmt::Rgba => 32,
    };

    let h_rows = match crop_rows {
        Some((_, show)) => show,
        None => rows,
    };

    // Build control string
    let mut ctrl = format!("a=T,f={},c={},r={}", f, cols, h_rows);
    if img.fmt == Fmt::Rgba {
        ctrl.push_str(&format!(",s={},v={}", img.px_w, img.px_h));
    }
    if let Some((skip, show)) = crop_rows {
        let rows = rows.max(1) as u32;
        let y_src = skip as u32 * img.px_h / rows;
        let h_src = show as u32 * img.px_h / rows;
        ctrl.push_str(&format!(",y={},h={}", y_src, h_src));
    }

    // Split base64 payload into 4096-byte chunks with m=1 continuation, m=0 on last
    const CHUNK: usize = 4096;
    let bytes = b64.as_bytes();
    let mut out = String::new();

    if bytes.len() <= CHUNK {
        // Single chunk: no m= needed (or m=0)
        out.push_str(&format!("\x1b_G{};{}\x1b\\", ctrl, b64));
    } else {
        let mut i = 0;
        let mut first = true;
        while i < bytes.len() {
            let end = (i + CHUNK).min(bytes.len());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transmit_emits_graphics_escape() {
        let img = ImageData { px_w: 4, px_h: 4, fmt: Fmt::Rgba, bytes: vec![0u8; 4*4*4] };
        let out = transmit_and_place(&img, 2, 1, None);
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
        let out = transmit_and_place(&img, 1, 1, None);
        assert!(out.contains("f=100"));
    }

    #[test]
    fn transmit_with_crop_maps_rows_to_source_pixels() {
        // Image is 80px tall displayed in 10 full rows -> 8 source px per row.
        let img = ImageData { px_w: 40, px_h: 80, fmt: Fmt::Rgba, bytes: vec![0u8; 40*80*4] };
        // Show rows [2, 5): skip 2, show 3 -> source y=16, h=24.
        let out = transmit_and_place(&img, 4, 10, Some((2, 3)));
        assert!(out.contains(",y=16"), "got {out}");
        assert!(out.contains(",h=24"), "got {out}");
        assert!(out.contains("r=3")); // display only the 3 visible rows
        // lowercase crop keys only
        assert!(!out.contains(",H="));
        assert!(!out.contains(",Y="));
    }
}

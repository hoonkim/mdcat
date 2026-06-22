// Generates a real (non-solid) demo image for the sample docs.
// Run with: cargo run --example gen_demo_image
use image::{Rgb, RgbImage};

fn main() {
    let (w, h) = (360u32, 200u32);
    let mut img = RgbImage::new(w, h);

    // Diagonal gradient background.
    for (x, y, p) in img.enumerate_pixels_mut() {
        let r = 40 + 180 * x / w;
        let g = 60 + 120 * y / h;
        let b = 160u32.saturating_sub(100 * x / w);
        *p = Rgb([r as u8, g as u8, b as u8]);
    }

    let put = |img: &mut RgbImage, x: i32, y: i32, c: [u8; 3]| {
        if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
            img.put_pixel(x as u32, y as u32, Rgb(c));
        }
    };

    // Filled yellow circle (left).
    let (cx, cy, rad) = (90i32, 100i32, 60i32);
    for y in (cy - rad)..=(cy + rad) {
        for x in (cx - rad)..=(cx + rad) {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= rad * rad {
                put(&mut img, x, y, [250, 210, 80]);
            }
        }
    }

    // White rectangle outline + red X (right).
    let (x0, y0, x1, y1) = (200i32, 50i32, 330i32, 150i32);
    for x in x0..=x1 {
        put(&mut img, x, y0, [255, 255, 255]);
        put(&mut img, x, y1, [255, 255, 255]);
    }
    for y in y0..=y1 {
        put(&mut img, x0, y, [255, 255, 255]);
        put(&mut img, x1, y, [255, 255, 255]);
    }
    let n = (x1 - x0).max(1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let xa = x0 + i;
        let ya = y0 + ((y1 - y0) as f32 * t) as i32;
        let yb = y1 - ((y1 - y0) as f32 * t) as i32;
        for d in -2..=2 {
            put(&mut img, xa, ya + d, [255, 80, 80]);
            put(&mut img, xa, yb + d, [255, 80, 80]);
        }
    }

    img.save("samples/demo-image.png").expect("save png");
    println!("wrote samples/demo-image.png ({}x{})", w, h);
}

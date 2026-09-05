//! The window and taskbar icon, rasterized at startup so no image file has to
//! ship alongside the binary. It draws the same mark the UI uses.

const SIZE: usize = 64;

/// Distance from point `p` to the segment `a`-`b`, in pixels.
fn dist_to_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq <= f32::EPSILON {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + dx * t, ay + dy * t);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

pub fn icon_data() -> egui::IconData {
    let n = SIZE as f32;
    let c = n * 0.5;
    let s = n * 0.5;
    let half_stroke = n * 0.048;

    // The mark as line segments: shaft, arrow head, tray.
    let strokes: [(f32, f32, f32, f32); 6] = [
        (c, c - s * 0.80, c, c + s * 0.14),
        (c - s * 0.40, c - s * 0.26, c, c + s * 0.15),
        (c + s * 0.40, c - s * 0.26, c, c + s * 0.15),
        (c - s * 0.68, c + s * 0.40, c - s * 0.68, c + s * 0.72),
        (c + s * 0.68, c + s * 0.40, c + s * 0.68, c + s * 0.72),
        (c - s * 0.68, c + s * 0.72, c + s * 0.68, c + s * 0.72),
    ];

    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = strokes
                .iter()
                .map(|(ax, ay, bx, by)| dist_to_segment(px, py, *ax, *ay, *bx, *by))
                .fold(f32::INFINITY, f32::min);

            // One pixel of feathering keeps the diagonals from looking chewed.
            let coverage = (1.0 - (d - half_stroke)).clamp(0.0, 1.0);
            let i = (y * SIZE + x) * 4;
            rgba[i] = 0xf2;
            rgba[i + 1] = 0xf2;
            rgba[i + 2] = 0xf2;
            rgba[i + 3] = (coverage * 255.0) as u8;
        }
    }

    egui::IconData {
        rgba,
        width: SIZE as u32,
        height: SIZE as u32,
    }
}
